#!/usr/bin/env python3
"""`S1-IL`: did a client's verdict about its own reachability turn over while its way in stood confirmed?

    python tools/read-reachability.py [runs-directory-or-run]

The client says *this client is reachable from the internet* or *this client is behind NAT* whenever AutoNAT's
answer differs from what it held. One turn per client is a client learning where it stands. A pair of turns
seconds apart, over and over, is the defect this row is about: AutoNAT v2 tests **one address per answer**, a
client offers several -- a LAN address, an IPv6 ULA, a circuit, and the one the world can dial -- and the flag
used to be the answer about whichever was tested last.

**What to read.** Per log: the shape (`P` public, `N` behind NAT) and the seconds between turns. Then:

* `TURNS WITHIN 30 s` -- a verdict that came back within half a minute. Nothing about a home connection changes
  that fast, so each one is an answer about another address and not about the client.
* `PUBLIC WITH NO WAY IN` -- a verdict of *reachable from the internet* taken while **no** confirmed address
  was one the internet can dial without a relay. That is what an AutoNAT answer about a **circuit** produces:
  the address carries the relay's public IP and the dial-back succeeds through the relay, so the old rule read
  it as this client's own reachability. Must be zero after the fix.
* `NAT WITH A WAY IN` -- a turn to *behind NAT* taken while the client held an address the internet can dial
  without a relay. **This is the defect itself**, and after the fix it must be zero: the verdict is read from the
  set, so it goes down only when the set holds no way in. It needs the instrument, which is the `(N of M confirmed
  address(es) can be dialled without a relay)` the client writes from `S1-IL` on -- **both** numbers, because a
  circuit is a confirmed address and is not a way in, so a client holding three circuits is truthfully *behind
  NAT* with three confirmed addresses. A log without the counts is reported as `NOT TOLD APART`, never as a zero.

**The control** is the same binary with `P2P_POKER_LAST_ADDRESS_VERDICT=1` (fault-harness), which restores the
old rule; `tools/table-run.ps1 -LastAddressVerdict` sets it for every seat of a run.
"""
import os
import re
import sys

PUBLIC = "this client is reachable from the internet"
NAT = "this client is behind NAT"
STAMP = re.compile(r"^(\d\d):(\d\d):(\d\d)\.(\d+)")
ELAPSED = re.compile(r"^\s*(\d+\.\d+)\s")
COUNT = re.compile(r"\((\d+) of (\d+) confirmed address")
QUICK_S = 30


def at(line):
    """The second the line was written, from either stamp: the wall clock of a split run, or the seconds
    since the start of a `table-run.ps1` one. Only differences are ever read, so either will do."""
    m = STAMP.match(line)
    if m:
        h, mi, s = (int(x) for x in m.groups()[:3])
        return h * 3600 + mi * 60 + s + int(m.group(4)[:3]) / 1000.0
    e = ELAPSED.match(line)
    return float(e.group(1)) if e else None


def turns(path):
    """Every turn of the verdict in one log: `(second, public, ways in or None)`."""
    out = []
    try:
        lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    except OSError:
        return out
    for line in lines:
        if PUBLIC not in line and NAT not in line:
            continue
        told = COUNT.search(line)
        out.append((at(line), PUBLIC in line, int(told.group(1)) if told else None))
    return out


def read_run(d):
    said = []
    for dirpath, _, names in os.walk(d):
        for name in sorted(names):
            if not name.endswith((".log", ".txt")):
                continue
            path = os.path.join(dirpath, name)
            t = turns(path)
            if t:
                said.append((os.path.relpath(path, d), t))
    return said


def report(run, said):
    print("== %s" % os.path.basename(run.rstrip("\\/")))
    quick = nat_with_a_way_in = public_with_none = untold = 0
    for name, t in sorted(said):
        shape = "".join("P" if p else "N" for _, p, _ in t)
        gaps = [t[i][0] - t[i - 1][0] for i in range(1, len(t)) if t[i][0] is not None and t[i - 1][0] is not None]
        near = [g for g in gaps if 0 <= g <= QUICK_S]
        quick += len(near)
        told = [c for _, _, c in t if c is not None]
        untold += len(t) - len(told)
        wrong = [c for _, p, c in t if not p and c is not None and c > 0]
        nat_with_a_way_in += len(wrong)
        empty = [c for _, p, c in t if p and c is not None and c == 0]
        public_with_none += len(empty)
        print("   %-28s %2d turn(s) %-14s %s%s%s%s" % (
            name[:28], len(t), shape[:14],
            "gaps " + "/".join("%.0f" % g for g in gaps[:8]) + " s" if gaps else "one verdict, and it stood",
            "   %d WITHIN %d s" % (len(near), QUICK_S) if near else "",
            "   %d NAT WITH A WAY IN (%s)" % (len(wrong), ", ".join(str(c) for c in wrong)) if wrong else "",
            "   %d PUBLIC WITH NO WAY IN" % len(empty) if empty else ""))
    print("   -- %d log(s), %d turn(s), %d within %d s, NAT with a way in: %s, public with none: %s" % (
        len(said), sum(len(t) for _, t in said), quick, QUICK_S,
        "%d" % nat_with_a_way_in if untold == 0 else
        "%d of the %d turns that were told apart (%d without the instrument: NOT TOLD APART)" % (
            nat_with_a_way_in, sum(len(t) for _, t in said) - untold, untold),
        "%d" % public_with_none if untold == 0 else "%d told apart" % public_with_none))
    return quick, nat_with_a_way_in, untold, public_with_none


def main():
    where = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "runs")
    # A directory of runs is read run by run; one run is read alone. Asked about a directory of runs, the
    # walk below would otherwise find every log under it and call the lot one run.
    inside = [os.path.join(where, d) for d in sorted(os.listdir(where)) if os.path.isdir(os.path.join(where, d))]
    runs = inside if any(read_run(d) for d in inside) else [where]
    totals = [0, 0, 0, 0]
    seen = 0
    for run in runs:
        said = read_run(run)
        if not said:
            continue
        seen += 1
        for i, v in enumerate(report(run, said)):
            totals[i] += v
    if not seen:
        print("no log under %s says anything about reachability" % where)
        return 1
    print()
    print("%d run(s): %d turn(s) within %d s of each other, %s" % (
        seen, totals[0], QUICK_S,
        "no turn to behind-NAT while a way in stood confirmed, and no verdict of public with none" if totals[1] == 0 and totals[2] == 0 and totals[3] == 0 else
        "%d turn(s) to behind-NAT with a way in, %d verdict(s) of public with none" % (totals[1], totals[3]) if totals[2] == 0 else
        "%d told apart, %d without the instrument (NOT TOLD APART)" % (totals[1], totals[2])))
    return 0


if __name__ == "__main__":
    sys.exit(main())
