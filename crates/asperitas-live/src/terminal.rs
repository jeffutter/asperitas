//! The terminal shell of the knob view: raw mode, key polling, redraw, and the fixed-knob
//! fallback when stdout is not a terminal.

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use asperitas_dsp::asperitas::Controls;
use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::event::{self, Event};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{queue, ExecutableCommand};

use asperitas_live::view::{handle_key, render_view, Action};
use asperitas_live::ControlSlot;

/// How long one wait for a key lasts; it bounds how late `--seconds` can end the session.
const POLL: Duration = Duration::from_millis(50);

/// Raw mode with a hidden cursor for as long as the guard lives. Dropping it restores both,
/// which covers normal return, `?` early return and unwinding.
struct RawGuard;

impl RawGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = Self;
        io::stdout().execute(Hide)?;
        Ok(guard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Leave raw mode and show the cursor. Idempotent; errors are ignored because it runs while
/// already failing or unwinding.
fn restore() {
    let _ = terminal::disable_raw_mode();
    let mut out = io::stdout();
    let _ = out.execute(Show);
    let _ = out.flush();
}

/// Make a panic message readable: restore the terminal before the previous hook prints it.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}

fn draw(out: &mut impl Write, lines: &[String], redraw: bool) -> io::Result<()> {
    if redraw {
        queue!(out, MoveUp(lines.len() as u16))?;
    }
    for line in lines {
        queue!(out, MoveToColumn(0), Clear(ClearType::UntilNewLine))?;
        write!(out, "{line}\r\n")?;
    }
    out.flush()
}

/// Block until the session ends. On a terminal that is esc, ctrl-c or `seconds`, while key
/// presses write knob positions into `slot`; otherwise the knobs stay fixed and the session
/// ends after `seconds` or on Enter; `error` reports the latest stream error for the view; with stdin closed and no `seconds` it runs until killed.
pub fn wait(
    slot: &ControlSlot,
    error: &dyn Fn() -> Option<String>,
    initial: &Controls,
    source: &str,
    seconds: Option<f64>,
) -> Result<(), String> {
    let deadline = match seconds {
        Some(s) => Some(
            Duration::try_from_secs_f64(s)
                .ok()
                .and_then(|d| Instant::now().checked_add(d))
                .ok_or_else(|| format!("invalid duration: {s} seconds"))?,
        ),
        None => None,
    };
    if !(io::stdout().is_terminal() && io::stdin().is_terminal()) {
        println!("stdout is not a terminal: knob view disabled, knobs fixed");
        match deadline {
            Some(d) => std::thread::sleep(d.saturating_duration_since(Instant::now())),
            None => {
                println!("press Enter to stop");
                let mut line = String::new();
                if matches!(io::stdin().read_line(&mut line), Ok(0)) {
                    println!("stdin closed: running until killed");
                    loop {
                        std::thread::park();
                    }
                }
            }
        }
        return Ok(());
    }

    install_panic_hook();
    let _guard = RawGuard::enter().map_err(|e| format!("cannot enter raw mode: {e}"))?;
    let mut out = io::stdout();
    let mut controls = initial.clone();
    let mut shown = error();
    draw(
        &mut out,
        &render_view(&controls, source, shown.as_deref()),
        false,
    )
    .map_err(|e| e.to_string())?;
    loop {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(());
        }
        let mut redraw = false;
        if event::poll(POLL).map_err(|e| e.to_string())? {
            if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                match handle_key(&controls, key) {
                    Action::Quit => return Ok(()),
                    Action::None => {}
                    Action::Set(next) => {
                        slot.write(&next);
                        controls = next;
                        redraw = true;
                    }
                }
            }
        }
        let latest = error();
        if latest != shown {
            shown = latest;
            redraw = true;
        }
        if redraw {
            draw(
                &mut out,
                &render_view(&controls, source, shown.as_deref()),
                true,
            )
            .map_err(|e| e.to_string())?;
        }
    }
}
