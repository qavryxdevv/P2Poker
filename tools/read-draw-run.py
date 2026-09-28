#!/usr/bin/env python3
"""D-083's bed: what a table-run.ps1 run says about its seating draw.

Reads every n*.log of one run directory and prints, per node, the moments that
decide whether a forming table ever froze: the rogue knob reaching the node,
every seat it was given (the draw moves players), every seat the founder gave
back and why, when the table was set, and how many hands it opened.

    python tools/read-draw-run.py runs/run123456-4

A table that was never set at the founder is said in capitals, because that is
the one outcome the owner ruled out.
"""

import re
import sys
from pathlib import Path

# `table-run.ps1` writes the seconds since the node started and the line;
# `table-run-split.ps1` puts the wall clock in front.
LINE = re.compile(r"^\s*(?:(\d\d:\d\d:\d\d\.\d+)\s+)?(\d+\.\d+)\s+(.*)$")

PATTERNS = [
    ("rogue", re.compile(r"^fault-harness: this client gives the seating draw no (\w+ ?\w*)")),
    ("seat", re.compile(r"^seat (\d+) at ([0-9a-f]{8})")),
    ("released", re.compile(r"^seat (\d+) (.+) and the seat is free again")),
    ("given back", re.compile(r"^the founder gave this seat away")),
    ("set", re.compile(r"^the table is set: session ([0-9a-f]+)")),
    ("open", re.compile(r"^hand #(\d+) opens at genesis ([0-9a-f]+) with seats \[([^\]]*)\]")),
    ("panic", re.compile(r"panicked at")),
]


def read(path):
    events = []
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = LINE.match(raw)
        if not m:
            continue
        t = float(m.group(2))
        text = m.group(3).strip()
        for kind, pat in PATTERNS:
            mm = pat.search(text)
            if mm:
                events.append((t, kind, mm, text))
                break
    return events


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    run = Path(sys.argv[1])
    logs = sorted(run.glob("n*.log"), key=lambda p: (len(p.stem), p.stem))
    if not logs:
        sys.exit(f"no node logs in {run}")
    print(f"run {run.name}: {len(logs)} node log(s)")
    founder_set = None
    for log in logs:
        ev = read(log)
        rogue = next((e[2].group(1) for e in ev if e[1] == "rogue"), None)
        seats = [f"{e[2].group(1)}@{e[0]:.1f}" for e in ev if e[1] == "seat"]
        sets = [e for e in ev if e[1] == "set"]
        opens = [e for e in ev if e[1] == "open"]
        print(f"\n{log.stem}{'  ROGUE: gives no ' + rogue if rogue else ''}")
        print(f"  seats  {' -> '.join(seats) if seats else '-'}")
        for e in ev:
            if e[1] == "released":
                print(f"  {e[0]:7.1f}  gave seat {e[2].group(1)} back: {e[2].group(2)}")
            elif e[1] == "given back":
                print(f"  {e[0]:7.1f}  given back: {e[3][:120]}")
            elif e[1] == "panic":
                print(f"  {e[0]:7.1f}  PANIC: {e[3][:160]}")
        if sets:
            print(f"  set    {len(sets)} time(s): " + ", ".join(f"{e[0]:.1f} s ({e[2].group(1)[:8]})" for e in sets))
        else:
            print("  set    never")
        if opens:
            first = opens[0]
            print(f"  hands  {len(opens)} opened; #1 at {first[0]:.1f} s with seats [{first[2].group(3)}]")
        if log.stem == "n0":
            founder_set = sets[0][0] if sets else None
    print()
    if founder_set is None:
        print("THE TABLE WAS NEVER SET AT THE FOUNDER")
    else:
        print(f"the table was set at the founder at {founder_set:.1f} s")


if __name__ == "__main__":
    main()
