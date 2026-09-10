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

The prediction booked for the first S1-CR run (2026-09-10, after commit 21f2683;
`tools/table-run.ps1 -Seats 3 -Seconds 300 -DropAt 60 -DropFor 20`, which
kills n1 at 60 s and starts it again at 80 s with the same profile and
`--resume`):

  1. the returning n1 prints *resuming the unfinished session at <table>* and
     *rejoining ... from the session record*; the founder answers *already
     holds a seat* and the roster; n1 enters the group and learns the session;
  2. the table certified n1's seat out while it was away (a TIMEOUT_CERT names
     its seat) and plays on heads-up at one genesis;
  3. n1 prints *resumed at hand #k as a bystander (seat S) from 2 copies* for
     the first hand whose copies it holds after the session is set, and
     follows it to the settlement;
  4. at that boundary n1 asks to sit in, both others vote, the certificate
     banks everywhere, and the hand after opens on all three clients with
     three seats at one genesis, n1 shuffling in it; n1 prints *back in the
     roster ... deriving hands again*;
  5. no fork: one genesis per hand on every client, before and after.

What refutes it: n1 without a hand for the rest of the run (*cannot be
adopted*, or no *resumed at hand* line), a roster row with two values, or the
founder refusing the rejoin.

The prediction booked for the second S1-CR run (2026-09-10 20:26, after the
ratification echo and the kill-at-turn knob; `tools/table-run.ps1 -Seats 3
-Seconds 960 -DropAt 60 -DropFor 20 -DropOnTurn`). The first run of this shape,
`run195623-3`, rejoined and then sat at *ratified 1/3* for the rest of the run
because nobody sent the ratifications again, and its table stalled to the hand
deadline because the seat died mid-shuffle (D-015):

  1. n1 prints *fault-harness: stopping at my turn* at its first own turn at
     or after 60 s; the table certifies its seat out on that action (a
     TIMEOUT_CERT with a fold effect) and plays on heads-up at one genesis --
     no *hand deadline* stall;
  2. n1-again rejoins from the record, enters the group, prints *in the group
     with the roster and no session yet; saying my ratification again*, and
     both members print *a ratification copy arrived after the table was set;
     said N message(s) again over the group*; n1-again's status line reaches
     *ratified 3/3* within a minute of entering the group;
  3. n1-again prints *resumed at hand #k as a bystander (seat 1) from 2
     copies* and follows that hand to its settlement;
  4. at that boundary it asks to sit in, the certificate banks everywhere,
     the next hand opens at all three clients with three seats at one genesis,
     and n1-again prints *back in the roster from hand #b on*;
  5. no fork, no give-up, no forgotten record before the return.

What refutes it: *ratified 1/3* past a minute in the group (the echo did not
fire or did not arrive), a table waiting on seat 1 to the hand deadline (the
stop did not land on a turn), or any of the first prediction's refuters.

What the second run (`run202634-3`) did: 1 and 2 held -- n1 stopped at its
turn at 62 s, the table certified its seat out and played on heads-up, both
members answered the ratification over the group -- and 3 failed in a new way:
n1-again settled on session `ed73f924` while the table's was `9718345e`,
because it ratified ANEW (a new timestamp, a new event hash) instead of saying
its original ratification again; every deck key of the hand it adopted was
refused, the key's ownership proof being bound to the session id. The record's
first write on the rejoin also replaced hand #4 / stack 10200 with hand #0 /
the buy-in.

The prediction booked for the third S1-CR run (2026-09-10 20:41, after the
record carries the client's own TABLE_READY verbatim and a resume says it
again; same harness line as the second run):

  1. as 1 and 2 of the second run's prediction;
  2. every *the table is set: session* line of the run names ONE value, the
     n1-again lines included -- the same session at every node;
  3. n1-again prints *resumed at hand #k as a bystander (seat 1)* and NO
     *a held frame was refused by the adopted hand* line: the deck keys
     verify, the adopted hand follows to its settlement;
  4. and 4 and 5 of the second run's prediction: the return certificate, the
     three-seat hand at one genesis everywhere, *back in the roster*;
  5. the record is not written on the rejoin before the first boundary (no
     *session record: hand #0* line in n1-again).

What refutes it: two session values among the *the table is set* lines, a
*recorded ratification did not fit* line, a refused held frame in the adopted
hand, or any earlier refuter.

Prints, per node, the roster of every hand it opened, then the return road's
own lines in time order across every node, then the per-hand roster
agreement table. The harness numbers NODES and the table numbers SEATS: the
certificate and the return name the SEAT.

Usage:
    python tools/read-return-run.py split150102-9
"""
from __future__ import print_function
import io, os, re, sys

# The split harness stamps `HH:MM:SS.mmm  elapsed  text`; the one-machine harness
# only `elapsed  text`. Both are read.
STAMP = re.compile(r'^(?:(\d\d:\d\d:\d\d\.\d\d\d)\s+)?\s*([0-9.]+)\s+(.*)$')
# "hand #5 opens at genesis c41cdc63 with seats [0, 1, 2], ..." and the S1-BS
# re-open "hand #9 re-opens at genesis ... with seats [...] (was ...)"; the later
# line wins, so the table shows the roster the node ended up on.
OPENS = re.compile(r'hand #(\d+) (?:re-)?opens at genesis [0-9a-f]+ with seats \[([^\]]*)\]')
GENESIS = re.compile(r'genesis ([0-9a-f]{6,})')
ROAD = re.compile(
    r'asked to sit in|sit-in request|sit in at the next hand|return: |certified back in|'
    r'certified seat .* return|RETURN_|a return vote|re-opens|re-derived|'
    r'has certified seat|certified out|cert: about seat|banked|fault-harness: .*mute|muted|'
    r'holding the next deal|no return certificate|dealing on|no longer held|'
    # S1-CR: the rejoin road of a restarted client
    r'resuming the unfinished|unfinished session|rejoining |resumed at hand|cannot be adopted|would not open|'
    r'still outside the roster|back in the roster|session record|forgotten|gave up|already holds a seat|'
    r'stopping at my turn|ratification copy arrived|saying my ratification again|hand deadline|'
    # the session every node settled on: a value that differs is the run202634-3 defect
    r'the table is set: session|recorded ratification')


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
