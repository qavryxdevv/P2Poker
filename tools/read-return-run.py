# -*- coding: utf-8 -*-
"""Read one -MuteOnTurn run for S1-BM's return certificate (D-028).

The prediction booked before the first such run (2026-09-10, split after
commit 0d31932):

  1. the muted node is certified out at the action it went quiet on -- a
     TIMEOUT_CERT names its SEAT, and every node opens the next hand without
     that seat, at one genesis;
  2. at the first SETTLED boundary after the mute ends where the seat is
     outside the roster, the muted node says *outside the roster with chips,
     so I asked to sit in*, and every other node says *seat S asked to sit in
     at the boundary of hand K*;
  3. every voter -- the roster less the certified, so every healthy seat --
     votes, the certificate completes at every node ("certified back in,
     unanimously among [...]") and banks ("return: seat S banked");
  4. the hand after that boundary opens at EVERY node with the seat in its
     roster, at one genesis, and the seat is dealt in;
  5. nothing returns at an aborted boundary, and no node forks.

What refutes it: a node that opens hand K+1 with a different roster from the
others (a fork through IN(k)); a certificate that completes at some nodes and
not at others without a "held" line explaining it; a return at a boundary any
node ended by an abort.

Prints, per node, the roster of every hand it opened, then the return road's
own lines in time order across every node, then the per-hand roster
agreement table. The harness numbers NODES and the table numbers SEATS: the
certificate and the return name the SEAT.

Usage:
    python tools/read-return-run.py split150102-9
"""
from __future__ import print_function
import io, os, re, sys

STAMP = re.compile(r'^(\d\d:\d\d:\d\d\.\d\d\d)\s+([0-9.]+)\s+(.*)$')
# "hand #5 opens at genesis c41cdc63 with seats [0, 1, 2], ..." and the S1-BS
# re-open "hand #9 re-opens at genesis ... with seats [...] (was ...)"; the later
# line wins, so the table shows the roster the node ended up on.
OPENS = re.compile(r'hand #(\d+) (?:re-)?opens at genesis [0-9a-f]+ with seats \[([^\]]*)\]')
GENESIS = re.compile(r'genesis ([0-9a-f]{6,})')
ROAD = re.compile(
    r'asked to sit in|sit-in request|sit in at the next hand|return: |certified back in|'
    r'certified seat .* return|RETURN_|a return vote|re-opens|re-derived|'
    r'has certified seat|certified out|cert: about seat|banked|fault-harness: .*mute|muted|'
    r'holding the next deal|no return certificate|dealing on|no longer held')


def lines(path):
    for raw in io.open(path, encoding='utf-8', errors='replace'):
        m = STAMP.match(raw.rstrip('\n'))
        if m:
            yield m.group(1), float(m.group(2)), m.group(3)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    run = sys.argv[1]
    root = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'runs', run)
    if not os.path.isdir(root):
        root = run
    logs = sorted(f for f in os.listdir(root) if f.endswith('.log'))
    print('run %s: %s' % (run, ', '.join(logs)))
    rt = os.path.join(root, 'run.txt')
    if os.path.exists(rt):
        print('--- run.txt')
        print(io.open(rt, encoding='utf-8', errors='replace').read().rstrip())

    rosters = {}   # (hand, node) -> (seats, genesis)
    road = []      # (secs, node, text)
    for log in logs:
        node = log[:-4]
        for _, secs, text in lines(os.path.join(root, log)):
            m = OPENS.search(text)
            if m:
                g = GENESIS.search(text)
                rosters[(int(m.group(1)), node)] = (m.group(2).replace(' ', ''), g.group(1)[:8] if g else '?')
            if ROAD.search(text):
                road.append((secs, node, text))

    print('\n--- the return road, every node, in time order')
    for secs, node, text in sorted(road):
        print('%8.1f  %-7s %s' % (secs, node, text[:220]))

    hands = sorted(set(h for h, _ in rosters))
    nodes = sorted(set(n for _, n in rosters))
    print('\n--- roster per hand per node (seats@genesis); a row with two values is a fork')
    print('%-6s %s' % ('hand', ' '.join('%-22s' % n for n in nodes)))
    for h in hands:
        cells = []
        values = set()
        for n in nodes:
            v = rosters.get((h, n))
            if v:
                values.add(v)
                cells.append('%-22s' % ('%s@%s' % v))
            else:
                cells.append('%-22s' % '-')
        flag = '' if len(values) <= 1 else '   <-- %d values' % len(values)
        print('%-6d %s%s' % (h, ' '.join(cells), flag))
    return 0


if __name__ == '__main__':
    sys.exit(main())
