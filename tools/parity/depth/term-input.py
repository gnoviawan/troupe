"""The R7b input probe: `mouse OUT` turns on SGR mouse reporting and records the first report;
`paste OUT` turns on bracketed paste and records one paste. OUT gets the bytes' repr."""

import os
import select
import sys
import termios
import tty

mode, out = sys.argv[1], sys.argv[2]
on, off, end = {
    "mouse": (b"\x1b[?1000h\x1b[?1006h", b"\x1b[?1000l\x1b[?1006l", (b"M", b"m")),
    "paste": (b"\x1b[?2004h", b"\x1b[?2004l", (b"\x1b[201~",)),
}[mode]
saved = termios.tcgetattr(0)
tty.setraw(0)
os.write(1, on + f"waiting for a {mode}\r\n".encode())
got = b""
try:
    while not any(got.endswith(e) for e in end):
        ready, _, _ = select.select([0], [], [], 20)
        if not ready:
            break
        got += os.read(0, 1024)
finally:
    os.write(1, off)
    termios.tcsetattr(0, termios.TCSADRAIN, saved)
with open(out, "w") as f:
    f.write(repr(got))
print(f"got {got!r}")
