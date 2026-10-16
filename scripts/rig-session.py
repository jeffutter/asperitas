"""One rig bench session (TASK-019.03.04): reset the board over the probe, hold the console as the only
reader, send CTL FX on attach, wait for the capture to arm, run a timed CTL SET schedule (like
`asperitas-rig override`, 20 steps/s), send REL, and record raw bytes until DUMPEND + 2 s.

  rig-session.py sweep OUT FX_F FX_V FROM TO DUR_S START_AFTER_ARM_S MAX_S
  rig-session.py soak  OUT FX_F FX_V MAX_S        (env SNAP=<knob> adds two snaps; SNAPS=none removes them)
  rig-session.py hold  OUT FX_F FX_V SET_V MAX_S  (FX then one SET before the capture arms)

Frames come from the `ctl_frame` example (build it first: `cargo build -p asperitas-logging --example
ctl_frame`), so the bytes are the ones the device parses. The soak ramp is a triangle that starts and
ends at the base value 0x8000, because a REL (or a first SET away from the base) snaps the parameter
and a snap trips the discontinuity detector whatever the smoothing. Env: ASPERITAS_CONSOLE (default
/dev/cu.usbmodem101). Raw output goes to OUT; compress it with zstd and
judge it with `asperitas-measure analyze`. Needs the probe attached and the bootloaded rig flashed.
"""
import os, re, select, subprocess, sys, termios, time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEV = os.environ.get('ASPERITAS_CONSOLE', '/dev/cu.usbmodem101')
CTL = os.path.join(REPO, 'target/debug/examples/ctl_frame')
_cache = {}


def frame(*args):
    key = tuple(str(a) for a in args)
    if key not in _cache:
        _cache[key] = subprocess.run([CTL, *key], capture_output=True, check=True).stdout
    return _cache[key]


def wire(level):
    return max(0, min(0xFFFF, int(round(level * 0xFFFF))))


def sweep_schedule(a, b, dur, start):
    """[(t_s, frame)] : SETs at 20/s linear from a to b over dur, then REL."""
    n = int(round(dur * 20))
    sched = [(start + i / 20.0, frame('set', '00', '%04x' % wire(a + (b - a) * i / n))) for i in range(n + 1)]
    sched.append((start + dur + 0.1, frame('rel', '00')))
    return sched


def soak_schedule():
    """Ramp 0.50 -> 0.95 over 250 s from t=10 s, with two big snaps (+0.25 for 0.5 s) at 80 s and 160 s."""
    a, b, start, dur = 0.50, 0.95, 10.0, 250.0
    snaps = [] if os.environ.get('SNAPS') == 'none' else [80.0, 160.0]
    sched = []
    n = int(dur * 20)
    for i in range(n + 1):
        t = i / 20.0
        x = t / dur
        v = a + (b - a) * (2 * x if x < 0.5 else 2 * (1 - x))  # triangle: starts and ends at the base value
        if any(s <= t < s + 0.5 for s in snaps):
            v = min(1.0, v + float(os.environ.get('SNAP', '0.25')))
        sched.append((start + t, frame('set', '00', '%04x' % wire(v))))
    sched.append((start + dur + 0.1, frame('rel', '00')))
    return sched


def probe_reset():
    r = subprocess.run(['probe-rs', 'reset', '--chip', 'ASPERITAS_H750IB', '--chip-description-path',
                        os.path.join(REPO, 'firmware/asperitas-h750.yaml'), '--non-interactive',
                        '--connect-under-reset'], capture_output=True, text=True, timeout=40)
    return r.returncode


def run(out, fx_f, fx_v, sched, max_s, pre=()):
    rc = probe_reset()
    t0 = time.time()
    print('probe reset rc=%d' % rc, flush=True)
    if rc != 0:
        sys.exit('probe reset failed')
    while os.path.exists(DEV) and time.time() - t0 < 6:
        time.sleep(0.01)
    print('node gone: %s after %.2fs' % (not os.path.exists(DEV), time.time() - t0), flush=True)
    fd = None
    rx = bytearray()
    fx_sent = None
    boot_seen = None
    arm_wall = None
    idx = 0
    dump_end = None
    fxframe = frame(fx_f, fx_v)
    while time.time() - t0 < max_s:
        if fd is None:
            try:
                fd = os.open(DEV, os.O_RDWR | os.O_NONBLOCK | os.O_NOCTTY)
                a = termios.tcgetattr(fd)
                a[0] = a[1] = a[3] = 0
                a[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
                termios.tcsetattr(fd, termios.TCSANOW, a)
                print('attached at %.2fs' % (time.time() - t0), flush=True)
            except OSError:
                fd = None
                time.sleep(0.01)
                continue
        if fx_sent is None and boot_seen is None:
            try:
                r0, _, _ = select.select([fd], [], [], 0.05)
                if r0:
                    rx += os.read(fd, 65536)
            except (BlockingIOError, OSError):
                pass
            if b'BOOT proto' in rx:
                boot_seen = time.time()
                print('BOOT record seen at %.2fs' % (boot_seen - t0), flush=True)
            elif time.time() - t0 > 12:
                sys.exit('no BOOT record: this is not a fresh boot, refusing to send FX')
            else:
                continue
        if fx_sent is None:
            try:
                os.write(fd, fxframe)
                fx_sent = time.time()
                for extra in pre:
                    os.write(fd, extra)
                print('FX sent at %.2fs (+%d pre frames)' % (fx_sent - t0, len(pre)), flush=True)
            except (BlockingIOError, OSError):
                pass
        wait = 0.05
        if arm_wall is not None and idx < len(sched):
            due = arm_wall + sched[idx][0]
            wait = max(0.0, min(0.05, due - time.time()))
            while idx < len(sched) and time.time() >= arm_wall + sched[idx][0]:
                try:
                    os.write(fd, sched[idx][1])
                except BlockingIOError:
                    break
                except OSError:
                    break
                idx += 1
        try:
            r, _, _ = select.select([fd], [], [], wait)
        except OSError:
            os.close(fd); fd = None; continue
        if r:
            try:
                b = os.read(fd, 65536)
            except BlockingIOError:
                b = b''
            except OSError:
                os.close(fd); fd = None; continue
            rx += b
        if arm_wall is None:
            m = re.findall(rb'delivered=(\d+)', bytes(rx[-4000:]))
            if m and int(m[-1]) > 0:
                arm_wall = time.time()
                print('armed (delivered>0) seen at %.2fs' % (arm_wall - t0), flush=True)
        if dump_end is None and b'DUMPEND' in rx[-4000:]:
            dump_end = time.time()
        if dump_end and time.time() - dump_end > 2.0:
            break
    open(out, 'wb').write(bytes(rx))
    txt = bytes(rx)
    print('SET/REL acks: %d OK SET, %d OK REL, %d ERR' % (txt.count(b'CTLR OK SET'), txt.count(b'CTLR OK REL'), txt.count(b'CTLR ERR')))
    print('FX reply:', b'CTLR OK FX' in txt, 'schedule sent %d/%d' % (idx, len(sched)))
    print('bytes', len(rx), 'dumpend', dump_end is not None, 'total %.1fs' % (time.time() - t0))


if __name__ == '__main__':
    mode = sys.argv[1]
    if mode == 'sweep':
        out, f, v, a, b, dur, start, mx = sys.argv[2], sys.argv[3], sys.argv[4], float(sys.argv[5]), float(sys.argv[6]), float(sys.argv[7]), float(sys.argv[8]), float(sys.argv[9])
        run(out, f, v, sweep_schedule(a, b, dur, start), mx)
    elif mode == 'hold':
        run(sys.argv[2], sys.argv[3], sys.argv[4], [], float(sys.argv[6]), pre=[frame('set', '00', sys.argv[5])])
    elif mode == 'soak':
        run(sys.argv[2], sys.argv[3], sys.argv[4], soak_schedule(), float(sys.argv[5]))
