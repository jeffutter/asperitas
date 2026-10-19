//! Excerpt install: the one consumer of the console's inbound records, writing them into a QSPI slot.
//!
//! The host streams `EXCSTART` / `EXCDATA` / `EXCEND` records (`asperitas_wire::excerpt`) over the
//! CDC OUT endpoint; `usb::inbound()` hands them over whole and CRC-checked; `excerpt::Installer` turns
//! them into flash actions; the task below performs each action and nothing else. Every protocol
//! decision - ordering, short and overrun chunks, the stream CRC, when a sector is full, whether the
//! readback earns an `EXCOK` - is the installer's, and is covered by its host suite
//! (`crates/asperitas-host/tests/excerpt_install.rs`, including `dropping_any_frame_never_verifies`
//! for truncation and the CRC-mismatch verdict tests for corruption). This side adds only flash I/O,
//! the capture gate and the reporting, so a truncated or corrupted upload reaches `EXCOK` here exactly
//! when it does there: never.
//!
//! ### Only the async flash API, and what that costs
//!
//! daisy-embassy's blocking `read` / `write` / `erase` wait on the chip's WIP bit in an unbounded busy
//! loop (`wait_for_write`): a stuck chip would hang thread mode, and with it the console that could
//! say so. The async calls auto-poll the status register in QUADSPI hardware and wake on its match
//! interrupt, so an erase pause is an await that costs thread mode nothing. Their price is that each
//! wait is bounded by `with_timeout(..).expect(..)` - 600 ms per sector erase, 1.6 ms per 256-byte page
//! program, twice the datasheet maxima - so a chip that misses one panics, and the panic handler turns
//! that into a `PANIC` record and a red LED. That is the chosen failure: loud and attributable, rather
//! than a silent hang. The `flash` module's `AsyncFlash` offers no blocking method, so
//! none can be called. The one blocking wait left in the image is the driver's own
//! `reset()` inside `build_async`, run once at boot before audio starts.
//!
//! Wall clock is dominated by erases: a full 516,096-byte slot is 127 PCM sectors plus the header
//! sector, and at the IS25LP064A's typical sector erase (tens of milliseconds) that is seconds; a
//! 288,000-byte corpus clip is 71 + 1. Predicted against observed is the bench table's job, not a
//! figure this comment can know.
//!
//! ### Why an install cannot disturb the capture
//!
//! 1. It never overlaps one. `CAPTURE_ACTIVE` spans arm to `DUMPEND`; an `EXCSTART` inside it is
//!    refused with `EXCFAIL ... why=busy`, and an install already open when it begins is abandoned the
//!    same way at its next record, so at most the one flash operation in flight runs into the window.
//! 2. It runs on the thread executor only. The SAI1 callback never names `FLASH`, the installer or
//!    anything below; its world is the ring and the atomics above.
//! 3. Its interrupts sit below the audio executor. `hal::init` gives MDMA `Priority::P0` (embassy-stm32
//!    0.6.0 `Config::default()`) and `Qspi::new_bank1` enables QUADSPI at the reset value, also P0, so
//!    both would preempt the P6 callback. `main` drops both to P7 right after `build_async`. Neither
//!    handler wakes anything at audio priority - each wakes a thread-mode task - so there is no reason
//!    for them to outrank SAI1, unlike the SAI DMA streams, which pend it. At P7 they preempt the
//!    callback zero times, and the most the callback can do to them is delay a wake by one callback
//!    (~76 us against the 1.6 ms page timeout). Priced at P0 instead, for whoever moves them: one page
//!    program is one MDMA transfer-complete plus one QUADSPI status-match interrupt, a few hundred
//!    cycles each, and at the datasheet's typical ~0.2 ms per page at most four pages fit a 666 us
//!    period - about eight ISRs, a few microseconds, against the ~590 us of headroom the callback
//!    leaves. Affordable, but not free, and not needed. The `nvic priority` line at boot reads both
//!    back.
//!
//! Bus traffic is the remaining coupling: MDMA writes the readback into AXI SRAM (the staging's own region, either layout), which the SAI DMA
//! also reaches. Arbitration there is per beat, nanoseconds, and SDRAM (the ring) is not on the path.

#[cfg(feature = "log-usb")]
use crate::emit_console;
#[cfg(feature = "log-usb")]
use crate::fx_ctl;
#[cfg(feature = "log-usb")]
use crate::ring::CAPTURE_ACTIVE;
#[cfg(feature = "log-usb")]
use asperitas_logging::{error, info};
#[cfg(feature = "log-usb")]
use core::sync::atomic::Ordering;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;

/// The board's QSPI flash, async-only (see above).
pub(crate) use crate::flash::AsyncFlash as RigFlash;

/// The one flash handle, built once in `main` before audio starts.
///
/// A mutex because two thread-mode tasks will want it: the excerpt installer here, and the replay
/// refill. `ThreadModeRawMutex` because thread mode is the only place flash is
/// ever touched - the raw mutex panics if an interrupt tries - and the `Option` because a `static`
/// cannot run `build_async`. The rule for every user: hold the guard across your own flash awaits
/// only, never across a channel receive or a timer, or the other user stalls behind a host that is
/// thinking.
pub(crate) static FLASH: Mutex<ThreadModeRawMutex, Option<RigFlash>> = Mutex::new(None);

/// Sector size as a `usize`, for array lengths. The installer lends sectors as `[u8; SECTOR]`.
#[cfg(feature = "log-usb")]
const SECTOR_BYTES: usize = asperitas_wire::excerpt::SECTOR as usize;

/// Erase or rewrite one whole sector of `slot`: erase when `data` is `None`, erase then program
/// `data` otherwise. The only place this binary erases or writes QSPI.
///
/// The asserts run before the driver is called, because the driver erases the whole 4 KiB sector
/// containing any address it is handed and asserts nothing but the chip's top address: an
/// unaligned or out-of-slot address would silently take a neighbouring slot's sector with it.
/// Length needs no runtime check - `[u8; SECTOR_BYTES]` is the only shape `data` can have - so
/// every write is exactly one sector at a sector boundary inside the slot it names.
#[cfg(feature = "log-usb")]
async fn rewrite_sector(
    flash: &mut RigFlash,
    slot: u8,
    address: u32,
    data: Option<&[u8; SECTOR_BYTES]>,
) {
    use asperitas_wire::excerpt::{slot_base, SECTOR, SLOT_SPAN};
    let base = slot_base(slot);
    assert!(
        base.is_some_and(|base| address.is_multiple_of(SECTOR)
            && address >= base
            && address + SECTOR <= base + SLOT_SPAN),
        "rig: flash sector write outside its slot"
    );
    match data {
        Some(data) => flash.write_async(address, data).await,
        None => flash.erase_async(address, SECTOR).await,
    }
}

/// Read `bytes` from `address` in sector-sized chunks and return their CRC-16 and how many bytes
/// were read. `scratch` is the chunk buffer; the range must lie inside `slot`.
#[cfg(feature = "log-usb")]
async fn readback_crc(
    flash: &mut RigFlash,
    slot: u8,
    address: u32,
    bytes: u32,
    scratch: &mut [u8; SECTOR_BYTES],
) -> (u16, u32) {
    use asperitas_wire::excerpt::{slot_base, SLOT_SPAN};
    use asperitas_wire::frame::{crc16_ccitt_update, CRC16_INITIAL};
    assert!(
        slot_base(slot).is_some_and(|base| address >= base && address + bytes <= base + SLOT_SPAN),
        "rig: flash readback outside its slot"
    );
    let mut crc = CRC16_INITIAL;
    let mut read = 0u32;
    while read < bytes {
        let n = (bytes - read).min(SECTOR_BYTES as u32) as usize;
        flash.read_async(address + read, &mut scratch[..n]).await;
        crc = crc16_ccitt_update(crc, &scratch[..n]);
        read += n as u32;
    }
    (crc, read)
}

/// The flash handle behind a held `FLASH` guard.
///
/// `main` stores the handle before it starts polling anything that calls this, so `None` here is a
/// boot-order bug, not a runtime condition worth a quieter answer.
#[cfg(any(feature = "log-usb", feature = "stim-excerpt"))]
pub(crate) fn flash_of<'g>(
    guard: &'g mut embassy_sync::mutex::MutexGuard<'_, ThreadModeRawMutex, Option<RigFlash>>,
) -> &'g mut RigFlash {
    guard
        .as_mut()
        .expect("rig: FLASH is stored before the install task runs")
}

/// Report a failed install as `EXCFAIL`.
#[cfg(feature = "log-usb")]
fn emit_exc_fail(failure: &asperitas_wire::excerpt::Failure) {
    let mut body = [0u8; asperitas_wire::frame::MAX_BODY];
    let n = asperitas_wire::excerpt::exc_fail_body(failure, &mut body);
    emit_console(&body[..n]);
}

/// Answer one `CTL` record through the shared dispatcher (`VER`, `RSN`, `RST`, `SET`, `REL`; see
/// `host_control`). The rig adds `FX`, refused with `busy` while a capture is armed because changing
/// the effect mid-capture would make the recording unrepeatable. `SET` and `REL` are allowed
/// mid-capture: the effect kind and base value stay fixed while the override moves the parameter,
/// which is what a sweep is.
#[cfg(feature = "log-usb")]
async fn handle_control(record: &asperitas_logging::inbound::InboundRecord) {
    use asperitas_wire::control::{ErrorCode, Reply, Verb};
    asperitas_firmware::host_control::handle_control(record, |fx, value| {
        if CAPTURE_ACTIVE.load(Ordering::Acquire) {
            Reply::Error(ErrorCode::Busy)
        } else if fx_ctl::select_fx(fx, value) {
            Reply::Ok(Verb::Fx)
        } else {
            Reply::Error(ErrorCode::BadValue)
        }
    })
    .await;
}

/// Consume install records forever: the one consumer of `usb::inbound()`.
///
/// Emits exactly one verdict per install - `EXCOK` after the header is written, or one `EXCFAIL` -
/// and no progress records: the outcome is what the host waits for, and per-sector chatter would
/// share the pipe with everything else for no reader. Holds `FLASH` for one action at a time.
///
/// The installer's 4 KiB sector buffer and the 4 KiB `scratch` live in this future, which is part of
/// `main`'s statically allocated task, not on the stack.
#[cfg(feature = "log-usb")]
pub(crate) async fn run_install(inbound: asperitas_logging::usb::InboundReceiver) {
    use asperitas_wire::excerpt::{
        exc_ok_body, parse_record, Action, ExcRecord, Installer, RecordError, Verdict, Why,
        HEADER_LEN,
    };

    let mut installer = Installer::new();
    let mut scratch = [0u8; SECTOR_BYTES];
    // The slot the open install named in its `EXCSTART`, so every sector write is bounds-checked
    // against the slot the host asked for, not against one re-derived from the address being checked.
    let mut target: Option<u8> = None;
    loop {
        let record = inbound.receive().await;
        let body = record.body();
        if body.starts_with(asperitas_wire::control::CMD_PREFIX) {
            handle_control(&record).await;
            continue;
        }
        let parsed = parse_record(body);
        if matches!(parsed, Err(RecordError::NotExcerpt)) {
            continue;
        }

        // The capture gate, checked per record. Checking and acting happen with no await between
        // them, and the timeline sets `CAPTURE_ACTIVE` on this same thread executor, so the two
        // cannot interleave inside that span.
        let starting = matches!(parsed, Ok(ExcRecord::Start { .. }));
        // A replay pass counts too: from boot until the callback has handed over the last sample, the
        // refill task is reading the very slot an install would be erasing.
        let busy = CAPTURE_ACTIVE.load(Ordering::Acquire) || replay_pass_open();
        if busy && (starting || installer.is_open()) {
            info!("rig: excerpt install refused: a capture is armed or dumping, or a replay pass is open");
            emit_exc_fail(&installer.abort(Why::Busy));
            continue;
        }

        let action = match parsed {
            Ok(parsed) => installer.feed(parsed),
            // Re-parsed inside, which is what turns the grammar error into `why=malformed`.
            Err(_) => installer.feed_body(body),
        };
        match action {
            Action::InvalidateHeader {
                slot,
                address,
                aborted,
            } => {
                if let Some(old) = aborted {
                    info!("rig: excerpt install for slot {old} abandoned by a new EXCSTART");
                }
                info!("rig: excerpt install into slot {slot} started");
                target = Some(slot);
                let mut guard = FLASH.lock().await;
                rewrite_sector(flash_of(&mut guard), slot, address, None).await;
            }
            Action::WriteSector { address, data, .. } => {
                let slot = target.expect("rig: the installer wrote a sector with no EXCSTART seen");
                let mut guard = FLASH.lock().await;
                rewrite_sector(flash_of(&mut guard), slot, address, Some(data)).await;
            }
            Action::Verify {
                slot,
                address,
                bytes,
            } => {
                let mut guard = FLASH.lock().await;
                let flash = flash_of(&mut guard);
                let (crc, len) = readback_crc(flash, slot, address, bytes, &mut scratch).await;
                match installer.verdict(crc, len) {
                    Verdict::Ok {
                        slot,
                        address,
                        header,
                        encoded,
                    } => {
                        // The header goes last and as a whole sector: its presence is the proof
                        // that everything before it read back clean.
                        scratch.fill(0xFF);
                        scratch[..HEADER_LEN].copy_from_slice(&encoded);
                        rewrite_sector(flash, slot, address, Some(&scratch)).await;
                        drop(guard);
                        info!("rig: excerpt slot {slot} installed and verified");
                        let mut out = [0u8; asperitas_wire::frame::MAX_BODY];
                        let n = exc_ok_body(&header, &mut out);
                        emit_console(&out[..n]);
                    }
                    Verdict::Fail(failure) => {
                        drop(guard);
                        error!("rig: excerpt readback did not match; slot {slot} left blank");
                        emit_exc_fail(&failure);
                    }
                }
            }
            Action::Reject(failure) => emit_exc_fail(&failure),
            Action::Accepted | Action::Ignored => {}
        }
    }
}

/// Whether a replay pass is open, for the install gate. Always false without `stim-excerpt`.
#[cfg(feature = "log-usb")]
fn replay_pass_open() -> bool {
    #[cfg(feature = "stim-excerpt")]
    {
        crate::excerpt_play::pass_open()
    }
    #[cfg(not(feature = "stim-excerpt"))]
    {
        false
    }
}
