#!/usr/bin/env python3
"""bench-030-04.py - guided bench session for the framed console on hardware.

usage:
  scripts/bench-030-04.py            flash podtest over the probe, capture, coach each action live, decode
  scripts/bench-030-04.py panic      flash panictest over the probe, capture the countdown and PANIC record
  scripts/bench-030-04.py decode DIR re-run the decode and report on an existing session directory

It tells you what to do, counts what the board reports back as you do it, and writes everything the
ticket asks for (record count, rate, last STATUS counters, BOOT count, integrity failures, performed
vs decoded control events) to <session>/report.md. The coaching reads the live byte stream with a
regex and is NOT the evidence: the evidence is console_decode over the raw file afterwards, which
checks every CRC. Standard library only.

Open the console BEFORE the board boots or BOOT is lost (a late reader never sees it), so the script
flashes first with the port closed, then opens it the instant the board re-enumerates.
"""
import glob
import os
import re
import select
import shutil
import subprocess
import sys
import time
from datetime import datetime

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MIN_SECONDS = 60
REC = re.compile(rb"~([A-Z]) ([0-9a-f]{8}) ([0-9a-f]{8}) (.*?)\*([0-9a-f]{4})")

# (label, what to do, event regex on the record text, how many events end the step)
STEPS = [
    ("encoder clockwise", "Turn the ENCODER CLOCKWISE, 5 slow detents (clicks).", r"ENC \+(\d+)", 5),
    ("encoder counter-clockwise", "Turn the ENCODER COUNTER-CLOCKWISE, 5 slow detents.", r"ENC -(\d+)", 5),
    ("encoder click", "PRESS and RELEASE the ENCODER BUTTON (push the knob), 5 times.", r"CLICK press", 5),
    ("button 1", "PRESS and RELEASE BUTTON 1, 5 times.", r"BTN1 press", 5),
    ("button 2", "PRESS and RELEASE BUTTON 2, 5 times.", r"BTN2 press", 5),
]


def say(msg=""):
    print(msg, flush=True)


def ask(prompt):
    try:
        return input(prompt).strip().lower()
    except EOFError:
        return ""


def reexec_in_nix():
    if shutil.which("probe-rs") and shutil.which("cargo"):
        return
    if os.environ.get("BENCH_IN_NIX"):
        sys.exit("probe-rs / cargo still missing inside nix develop")
    os.environ["BENCH_IN_NIX"] = "1"
    os.chdir(ROOT)
    os.execvp("nix", ["nix", "develop", "-c", sys.executable] + sys.argv)


def set_raw(path):
    """Raw 115200 8N1. BSD stty takes -f, GNU stty (nix) takes -F; try both."""
    for flag in ("-f", "-F"):
        r = subprocess.run(["stty", flag, path, "115200", "raw", "-echo"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if r.returncode == 0:
            return


def find_console(timeout):
    """Return (path, fd, bytes_already_read) for the port that emits console records.

    Several /dev/cu.usbmodem* can exist at once (the ST-Link's serial port is there before the
    board re-enumerates and says nothing), so the port is chosen by what it sends, not by name.
    The bytes read while sniffing are handed back so the BOOT record is not thrown away.
    """
    end = time.time() + timeout
    seen = {}
    while time.time() < end:
        for path in sorted(glob.glob("/dev/cu.usbmodem*")):
            if path not in seen:
                set_raw(path)
                try:
                    seen[path] = [os.open(path, os.O_RDONLY | os.O_NONBLOCK), b""]
                except OSError:
                    continue
            fd, got = seen[path]
            try:
                ready, _, _ = select.select([fd], [], [], 0.05)
                if ready:
                    got += os.read(fd, 65536)
                    seen[path][1] = got
            except OSError:
                os.close(fd)
                del seen[path]
                continue
            if REC.search(got):
                for other, (ofd, _) in seen.items():
                    if other != path:
                        os.close(ofd)
                return path, fd, got
    for fd, _ in seen.values():
        os.close(fd)
    return None


def flash(binary):
    say(f"\n[1/4] Flashing {binary} over the ST-Link (probe-flash, chip ASPERITAS_H750IB)...")
    env = dict(os.environ, DEFMT_LOG="info")
    r = subprocess.run(
        ["make", "-C", os.path.join(ROOT, "firmware"), "probe-flash", f"BINARY={binary}",
         "FEATURES=seed3 log-defmt"], env=env)
    if r.returncode != 0:
        say("\nFLASH FAILED. Check: probe plugged in and SWD cable seated, board powered.")
        say("Retry knobs: PROBE_EXTRA='--speed 1000' or UNDER_RESET=0 (see firmware/Makefile).")
        sys.exit(1)


class Console:
    """Raw tee of the CDC port to a file, plus a live line view for coaching."""

    def __init__(self, found, rawpath):
        self.path, self.fd, pre = found
        self.raw = open(rawpath, "wb")
        self.buf = b""
        self.total = 0
        self.last_seq = None
        self.live_gaps = 0
        self.events = []  # (monotonic, text)
        self.ingest(pre)

    def ingest(self, data):
        if not data:
            return []
        self.raw.write(data)
        self.raw.flush()
        self.total += len(data)
        self.buf += data
        new = []
        while True:
            m = REC.search(self.buf)
            if not m:
                self.buf = self.buf[-512:]
                break
            seq = int(m.group(2), 16)
            if self.last_seq is not None and seq != self.last_seq + 1 and seq > self.last_seq:
                self.live_gaps += 1
            self.last_seq = seq
            text = m.group(4).decode("ascii", "replace")
            new.append(text)
            self.events.append((time.monotonic(), text))
            self.buf = self.buf[m.end():]
        return new

    def pump(self, timeout=0.1):
        """Read what is available; return new record texts."""
        try:
            ready, _, _ = select.select([self.fd], [], [], timeout)
            if not ready:
                return []
            data = os.read(self.fd, 65536)
        except OSError:
            say("  !! console port dropped; reopening (a gap in the capture is expected)")
            try:
                os.close(self.fd)
            except OSError:
                pass
            time.sleep(1)
            found = find_console(15)
            if found:
                self.path, self.fd, pre = found
                return self.ingest(pre)
            return []
        return self.ingest(data)

    def close(self):
        self.raw.close()
        try:
            os.close(self.fd)
        except OSError:
            pass


def run_decode(session):
    raw = os.path.join(session, "capture.raw")
    out = os.path.join(session, "decoded.txt")
    err = os.path.join(session, "decode-summary.txt")
    with open(out, "wb") as o, open(err, "wb") as e:
        subprocess.run(["cargo", "run", "-q", "-p", "asperitas-wire", "--example",
                        "console_decode", "--", raw], stdout=o, stderr=e, cwd=ROOT)
    return out, err


def analyse(decoded_path):
    recs = []
    for line in open(decoded_path, "rb"):
        m = REC.search(line)
        if m:
            recs.append((m.group(1).decode(), int(m.group(2), 16), int(m.group(3)),
                         m.group(4).decode("ascii", "replace")))
    a = {"records": len(recs), "boots": sum(1 for r in recs if r[3].startswith("BOOT ")),
         "last_status": None, "enc_cw": 0, "enc_ccw": 0, "click": 0, "btn1": 0, "btn2": 0,
         "release": 0, "span_s": 0.0, "seq_gaps": []}
    for lvl, seq, ms, text in recs:
        if text.startswith("STATUS") or " STATUS " in text:
            a["last_status"] = text
        m = re.search(r"ENC ([+-]\d+)", text)
        if m:
            v = int(m.group(1))
            a["enc_cw" if v > 0 else "enc_ccw"] += abs(v)
        a["click"] += "CLICK press" in text
        a["btn1"] += "BTN1 press" in text
        a["btn2"] += "BTN2 press" in text
        a["release"] += " release" in text
    for prev, cur in zip(recs, recs[1:]):
        if cur[1] != prev[1] + 1 and cur[1] > prev[1]:
            a["seq_gaps"].append((prev[1], cur[1]))
    if len(recs) > 1:
        a["span_s"] = (recs[-1][2] - recs[0][2]) / 1000.0
    return a


def podtest_session():
    reexec_in_nix()
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    session = os.path.join(ROOT, "target", "bench", "030.04", stamp)
    os.makedirs(session, exist_ok=True)
    say(f"Framed console bench session  (files: {os.path.relpath(session, ROOT)})")
    say("Before we start: close any screen / cat / serial reader. Only this script may hold the port.")
    say("Have the board powered, ST-Link attached, USB-C to this Mac, hands free for the Pod controls.")
    ask("Press Enter when ready... ")

    # Unplugging nothing: the flash resets the board, which drops the CDC port. Wait it out, then open.
    flash("podtest")
    say("\n[2/4] Waiting for the console to appear...")
    found = find_console(30)
    if not found:
        sys.exit("No port sent console records in 30 s (ports: %s). Replug USB-C and rerun."
                 % (glob.glob("/dev/cu.usbmodem*") or "none"))
    con = Console(found, os.path.join(session, "capture.raw"))
    say(f"Console open on {con.path}. Capture is recording to capture.raw.")
    t0 = time.monotonic()
    # Let the boot records land and confirm the stream is alive before asking for hands.
    live_end = time.time() + 6
    got = 0
    while time.time() < live_end:
        got += len(con.pump(0.2))
    if got == 0:
        say("  !! no records in 6 s. Is podtest running? Check the board LED and USB-C, then Ctrl-C and rerun.")
    else:
        say(f"  stream alive: {got} records in 6 s (about {got / 6:.0f}/s; idle podtest is about 100/s).")

    say("\n[3/4] Hands on. Do each action when told. Slowly and deliberately; count along with me.")
    performed = {}
    try:
        for label, instr, rx, target in STEPS:
            say(f"\n>>> {instr}")
            rxc = re.compile(rx)
            seen = 0
            last_print = None
            step_start = time.monotonic()
            done_at = None
            while True:
                for text in con.pump(0.1):
                    m = rxc.search(text)
                    if m:
                        seen += int(m.group(1)) if m.groups() else 1
                        if seen != last_print:
                            say(f"    board reports: {seen} / {target}")
                            last_print = seen
                        if seen >= target and done_at is None:
                            done_at = time.monotonic()
                if done_at and time.monotonic() - done_at > 2.0:
                    break
                if time.monotonic() - step_start > 90 and seen == 0:
                    say("    nothing heard for 90 s. Skipping this step; the report will show 0.")
                    break
            extra = seen - target
            if extra > 0:
                say(f"    note: board counted {seen}, {extra} more than the {target} asked for.")
                say("    (fine if you overshot; say how many you actually did at the end)")
            performed[label] = seen
            say(f"    done: {label}.")
        elapsed = time.monotonic() - t0
        if elapsed < MIN_SECONDS:
            say(f"\n>>> Keep going for {MIN_SECONDS - elapsed:.0f} s more: mix encoder turns, "
                "knob presses and both buttons, any order.")
            while time.monotonic() - t0 < MIN_SECONDS:
                con.pump(0.2)
        say("\n>>> Hands off. Capturing 3 s of idle so the file ends on a complete record.")
        end = time.time() + 3
        while time.time() < end:
            con.pump(0.2)
    except KeyboardInterrupt:
        say("\nCtrl-C: stopping the capture early.")
    secs = time.monotonic() - t0
    con.close()
    say(f"\nCapture closed: {con.total} bytes in {secs:.0f} s. Live view saw {con.live_gaps} seq gaps.")

    say("\n[4/4] Questions only you can answer:")
    say("  How many of each did you really do? Enter a number, or just Enter if exactly as asked (5 each).")
    actual = {}
    for label, _, _, target in STEPS:
        v = ask(f"    {label} [{target}]: ")
        actual[label] = int(v) if v.isdigit() else target
    audio = ask("  Audio: was main.rs output rigged and listened to while logging? "
                "(n = not rigged / not listened; clean = listened, no clicks; bad = heard artefacts): ")
    # podtest does not produce main's audio; keep the answer verbatim so the ticket says what happened.
    say("\nDecoding with console_decode (checks every CRC)...")
    dec, summ = run_decode(session)
    a = analyse(dec)
    write_report(session, a, actual, audio, secs, summ, con)
    say(open(os.path.join(session, "report.md")).read())
    say(f"\nEverything is in {os.path.relpath(session, ROOT)}/. Paste report.md into the ticket notes "
        "and tell me; I can tick the criteria with you.")


def write_report(session, a, actual, audio, secs, summ, con):
    rate = a["records"] / a["span_s"] if a["span_s"] else 0.0
    exp = {"encoder clockwise": a["enc_cw"], "encoder counter-clockwise": a["enc_ccw"],
           "encoder click": a["click"], "button 1": a["btn1"], "button 2": a["btn2"]}
    lines = ["# Framed console bench report", "",
             f"- capture: {con.total} bytes, {secs:.0f} s wall clock",
             f"- decoded records: {a['records']} over {a['span_s']:.1f} s of device time = {rate:.1f} records/s",
             f"- BOOT records seen: {a['boots']} (0 = reader attached late; 2+ = board restarted mid-capture)",
             f"- seq gaps in decoded stream: {len(a['seq_gaps'])} {a['seq_gaps'][:5]}",
             f"- last STATUS: {a['last_status']}", "",
             "## console_decode summary (stderr)", "```", open(summ).read().strip(), "```", "",
             "## You did vs the log decoded (AC 2)", "",
             "| action | you did | decoded | match |", "|---|---|---|---|"]
    for label, _, _, _ in STEPS:
        d, g = actual[label], exp[label]
        lines.append(f"| {label} | {d} | {g} | {'yes' if d == g else 'NO - investigate'} |")
    lines += ["", "Decoded counts include any extra mixed play in the free-play minute, so a decoded "
              "count above yours there is expected; below yours is a lost event.", "",
              f"- audio (AC 3), verbatim: {audio or '(no answer)'}"]
    open(os.path.join(session, "report.md"), "w").write("\n".join(lines) + "\n")


def panic_session():
    reexec_in_nix()
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    session = os.path.join(ROOT, "target", "bench", "030.04", stamp + "-panic")
    os.makedirs(session, exist_ok=True)
    say("panictest spot check. Close any other serial reader. Keep USB-C attached to this Mac.")
    ask("Press Enter when ready... ")
    flash("panictest")
    found = find_console(30)
    if not found:
        sys.exit("No port sent console records in 30 s after flash.")
    con = Console(found, os.path.join(session, "capture.raw"))
    say("\nCapturing 20 s. Watch the board LED: note whether it goes red and roughly when "
        "(about 3 s after the PANIC line = DWT counts; milliseconds = DWT locked).")
    t0 = time.monotonic()
    while time.monotonic() - t0 < 20:
        for text in con.pump(0.2):
            if "PANIC" in text or "countdown" in text.lower():
                say(f"    [{time.monotonic() - t0:5.1f}s] {text}")
    con.close()
    dec, summ = run_decode(session)
    panics = [l for l in open(dec, "rb").read().splitlines() if b"PANIC" in l]
    say(f"\nDecoded PANIC records with valid CRC: {len(panics)}")
    for p in panics:
        say("   " + p.decode("ascii", "replace"))
    say(open(summ).read())
    led = ask("Red LED seen? Seconds after the PANIC line (number), 'none', or Enter to skip: ")
    open(os.path.join(session, "report.md"), "w").write(
        f"# panictest\n\n- PANIC records decoded with valid CRC: {len(panics)}\n- red LED observation: {led}\n"
        f"\n```\n{open(summ).read().strip()}\n```\n")
    say(f"Saved to {os.path.relpath(session, ROOT)}/report.md")


def decode_only(session):
    reexec_in_nix()
    dec, summ = run_decode(session)
    a = analyse(dec)
    say(str(a))
    say(open(summ).read())


if __name__ == "__main__":
    os.chdir(ROOT)
    if len(sys.argv) == 1:
        podtest_session()
    elif sys.argv[1] == "panic":
        panic_session()
    elif sys.argv[1] == "decode" and len(sys.argv) == 3:
        decode_only(sys.argv[2])
    else:
        sys.exit(__doc__)
