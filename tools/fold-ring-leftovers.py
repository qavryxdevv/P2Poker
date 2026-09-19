#!/usr/bin/env python3
"""S1-II, patch 0039: did the receive ring replay a leftover, and did a hand stall on it?

    python tools/fold-ring-leftovers.py [<run> ...]      (run names under runs/, or paths; none = every run)

The carrier keeps each connection's messages in a ring of 2048 slots. A copy stored out of order and then
overtaken by its own retransmission stayed in its slot, and a lap of the ring later the drain took it for the
awaited message: replayed it, acknowledged it under its old id and moved the counter past a message that had not
arrived, which was then dropped as a duplicate. Per run this prints

  * REPLAYED   -- the sender's half, upstream's own line: `Wrap-around on message N` with N above zero. Before
                  patch 0039 each is a message a lossless stream lost to one receiver. After it there should be
                  none (N = 0 is another, older fault and is left out);
  * OVERTAKEN  -- patch 0039's line at the source: a message arrived in order while a copy of it waited in the
                  ring, and the copy went with it. How often the race is run;
  * CLEARED    -- patch 0039's line at the drain: a leftover found in the awaited slot and cleared, not replayed.
                  The safety net; above zero means a leftover still gets into the ring by a road 0039 does not
                  close, and the line names the slot and both ids;
  * the hands that stalled: `my clock has run out on seat S` grouped by hand and by S, because seven seats
    saying it of one seat is one incident and not seven (the mistake S1-II was first filed with).

A log from before patch 0039 has no OVERTAKEN or CLEARED line to give and is said as that: an absent instrument
is not a zero. Counts only; nothing of a peer or an address is printed.
"""
import collections
import glob
import io
import os
import re
import statistics
import sys

LINE = re.compile(r"^\ufeff?(\d\d:\d\d:\d\d\.\d{3})\s+(-?[0-9.]+)\s\s(.*)$")
WRAP = re.compile(r"Wrap-around on message (\d+)")
OVERTAKEN = "arrived in order while a copy of it waited in the ring"
CLEARED = re.compile(r"a leftover in the receive ring is cleared, not replayed: slot (\d+) held message (\d+) while (\d+) is awaited")
CLOCK = re.compile(r"^my clock has run out on seat (\d+)")
OPENS = re.compile(r"^hand #(\d+) opens")
HERE = os.path.dirname(os.path.abspath(__file__))


def runs_root():
    for base in (os.path.join(HERE, "..", "runs"), os.path.join(HERE, "runs")):
        if os.path.isdir(base):
            return os.path.abspath(base)
    return os.path.abspath("runs")


def fold(path):
    logs = sorted(glob.glob(os.path.join(path, "*.log")))
    if not logs:
        print("%s: no logs" % os.path.basename(path))
        return
    replayed = overtaken = cleared = toxlines = 0
    first_replay = None
    incidents = collections.defaultdict(list)
    opens = []
    laps = []
    for name in logs:
        hand = None
        for raw in io.open(name, encoding="utf-8", errors="replace").read().splitlines():
            m = LINE.match(raw)
            if not m:
                continue
            t, x = float(m.group(2)), m.group(3).strip()
            if x.startswith("toxcore"):
                toxlines += 1
                w = WRAP.search(x)
                if w and int(w.group(1)) > 0:
                    replayed += 1
                    first_replay = t if first_replay is None else min(first_replay, t)
                elif OVERTAKEN in x:
                    overtaken += 1
                else:
                    c = CLEARED.search(x)
                    if c:
                        cleared += 1
                        laps.append((int(c.group(3)) - int(c.group(2))) // 2048)
                continue
            o = OPENS.match(x)
            if o:
                hand = int(o.group(1))
                if name.endswith("n0.log") and not name.endswith("far-n0.log"):
                    opens.append(t)
                continue
            c = CLOCK.match(x)
            if c:
                incidents[(hand, int(c.group(1)))].append(t)
    gaps = [b - a for a, b in zip(opens, opens[1:])]
    print("%s: %d seat log(s)" % (os.path.basename(path), len(logs)))
    if toxlines == 0:
        print("   no carrier line in any log: not a fault-harness build, so none of the three counts can be read")
    else:
        patched = overtaken > 0 or cleared > 0
        print("   REPLAYED %d%s" % (replayed, "" if first_replay is None else " (the first at %.0f s)" % first_replay))
        if patched:
            print("   OVERTAKEN %d, CLEARED %d%s" % (
                overtaken, cleared,
                "" if not laps else " (leftovers %s lap(s) of the ring old)" % ", ".join(str(n) for n in sorted(set(laps)))))
        else:
            print("   OVERTAKEN and CLEARED: no such line -- a build from before patch 0039, or a run in which the race was never run")
    print("   hands opened at the founder %d; between two openings median %s s, longest %s s" % (
        len(opens), "%.1f" % statistics.median(gaps) if gaps else "-", "%.1f" % max(gaps) if gaps else "-"))
    if incidents:
        for (hand, seat), ts in sorted(incidents.items(), key=lambda kv: min(kv[1])):
            print("   STALLED: hand #%s waited on seat %d -- %d seat(s) ran out of clock on it, %.0f to %.0f s" % (
                hand, seat, len(ts), min(ts), max(ts)))
    else:
        print("   no clock ran out: no hand stalled on a seat")


def main():
    args = sys.argv[1:]
    root = runs_root()
    if not args:
        args = sorted(d for d in os.listdir(root) if os.path.isdir(os.path.join(root, d)))
    for a in args:
        fold(a if os.path.isdir(a) else os.path.join(root, a))
    return 0


if __name__ == "__main__":
    sys.exit(main())
