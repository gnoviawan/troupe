#!/usr/bin/env python3
"""Records the PTY byte streams that `apps/rune/crates/term/tests/cells.rs` replays.

Each session runs a program in a real PTY of a fixed size, types its steps, and
writes every byte the PTY printed to `<out>/<name>.bin` and the byte offsets of
its checkpoints to `<out>/<name>.checkpoints` (JSON; the extension keeps the
formatter off it). A checkpoint is taken once the output has been quiet for a
moment after a step, so the screen it marks is settled. `term-cells.ts` then
feeds the same bytes to `@xterm/headless` and dumps the cells at each
checkpoint to `<out>/<name>.cells`.

    python3 tools/parity/depth/term-record.py apps/rune/crates/term/tests/fixtures
"""

import fcntl
import json
import os
import pty
import select
import shutil
import struct
import sys
import tempfile
import termios
import time

HERE = os.path.dirname(os.path.abspath(__file__))
QUIET = 0.6


def sample_file(tmp):
    path = os.path.join(tmp, "sample.rs")
    lines = []
    for n in range(1, 121):
        lines.append(f"fn step_{n}(value: u32) -> u32 {{ value.wrapping_mul({n}) + {n * 3} }} // line {n}")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    return path


def sessions(tmp):
    sample_file(tmp)
    listing = os.path.join(tmp, "listing")
    os.makedirs(os.path.join(listing, "src"))
    os.makedirs(os.path.join(listing, "docs"))
    for name, mode in [("run.sh", 0o755), ("README.md", 0o644), ("notes.txt", 0o644)]:
        p = os.path.join(listing, name)
        open(p, "w").write("x\n")
        os.chmod(p, mode)
    os.symlink("README.md", os.path.join(listing, "link.md"))
    for script in ("term-screen.sh", "term-edges.sh"):
        shutil.copy(os.path.join(HERE, script), tmp)
    shell = ["bash", "--noprofile", "--norc", "-i"]
    prompt = "export PS1='$ '\r"
    return [
        {
            "name": "shell",
            "cols": 80,
            "rows": 24,
            "argv": shell,
            "steps": [
                (prompt + "bash term-screen.sh\r", True),
                ("cd listing && ls --color=always -F && seq 1 60\r", True),
                ("cd .. && bash term-edges.sh\r", True),
            ],
        },
        {
            "name": "vim",
            "cols": 100,
            "rows": 30,
            "argv": ["vim", "-u", "NONE", "-i", "NONE", "-N", "sample.rs"],
            "steps": [
                (":syntax on\r:set number hlsearch\r", True),
                ("\x06\x06/wrapping\r", True),
                ("\x1b:q!\r", True),
            ],
        },
        {
            "name": "htop",
            "cols": 120,
            "rows": 32,
            # A private PID namespace, so the recording lists two sleeps and htop, not the host.
            "argv": ["unshare", "--pid", "--fork", "--mount-proc", "sh", "-c", "sleep 300 & sleep 301 & exec htop -d 10"],
            "settle": 3.0,
            "steps": [
                ("", True),
                ("t", True),
                ("q", True),
            ],
        },
    ]


def record(session, cwd):
    size = struct.pack("HHHH", session["rows"], session["cols"], 0, 0)
    pid, fd = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, size)
        os.chdir(cwd)
        env = dict(os.environ, TERM="xterm-256color", LANG="C.UTF-8", LC_ALL="C.UTF-8", HOME="/tmp")
        env.pop("PROMPT_COMMAND", None)
        os.execvpe(session["argv"][0], session["argv"], env)
    out = bytearray()
    checkpoints = []

    def drain(quiet, settle=0.0):
        start = last = time.monotonic()
        # A program that redraws on a timer (htop) is never quiet: stop it at a deadline.
        deadline = start + max(settle, quiet) + 3.0
        while (time.monotonic() - last < quiet or time.monotonic() - start < settle) and time.monotonic() < deadline:
            ready, _, _ = select.select([fd], [], [], 0.05)
            if not ready:
                continue
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                return
            if not chunk:
                return
            out.extend(chunk)
            last = time.monotonic()

    drain(1.5)
    for keys, checkpoint in session["steps"]:
        for ch in keys:
            os.write(fd, ch.encode())
            time.sleep(0.01)
        drain(QUIET * 2, session.get("settle", 0.0))
        if checkpoint:
            checkpoints.append(len(out))
    try:
        os.kill(pid, 9)
    except ProcessLookupError:
        pass
    os.waitpid(pid, 0)
    return bytes(out), checkpoints


def main():
    out_dir = sys.argv[1]
    os.makedirs(out_dir, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix="term-record-")
    try:
        for s in sessions(tmp):
            data, checkpoints = record(s, tmp)
            with open(os.path.join(out_dir, s["name"] + ".bin"), "wb") as f:
                f.write(data)
            meta = {"cols": s["cols"], "rows": s["rows"], "checkpoints": checkpoints}
            with open(os.path.join(out_dir, s["name"] + ".checkpoints"), "w") as f:
                json.dump(meta, f, indent=2)
                f.write("\n")
            print(f"{s['name']}: {len(data)} bytes, checkpoints {checkpoints}")
    finally:
        shutil.rmtree(tmp)


if __name__ == "__main__":
    main()
