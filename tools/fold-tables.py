# -*- coding: utf-8 -*-
"""One node's log split by table (D-043): a client at two tables writes both
tables' hands into one log, and the hand numbers overlap. The tables are
told apart by the genesis chain -- every "hand #k opens at genesis X" line is
assigned to the table whose previous hand was k-1, and a hand #1 starts a
new table. Prints, per table: the hands opened and finished, the first and
last, and the seconds between hands.

    python tools/fold-tables.py runs/run<stamp>-5/n1.log
"""
import re
import sys
from pathlib import Path

LINE = re.compile(r'^\s*(\d+\.\d)\s+(.*)$')
OPENS = re.compile(r'^hand #(\d+) opens at genesis (\w+)')
OVER = re.compile(r'^hand #(\d+) is over')
TURN = re.compile(r'^hand #(\d+): your turn')


def main():
    log = Path(sys.argv[1])
    tables = []  # each: {'hands': [(k, t, genesis)], 'over': [(k, t)], 'turns': int}
    last_over = None
    for raw in log.read_text(encoding='utf-8', errors='replace').splitlines():
        m = LINE.match(raw)
        if not m:
            continue
        t, text = float(m.group(1)), m.group(2)
        mo = OPENS.match(text)
        if mo:
            k, g = int(mo.group(1)), mo.group(2)
            # the table whose last opened hand is k-1; a hand #1 is a new table
            home = None
            if k > 1:
                for tb in tables:
                    if tb['hands'] and tb['hands'][-1][0] == k - 1:
                        home = tb
                        break
            if home is None:
                home = {'hands': [], 'over': [], 'turns': 0}
                tables.append(home)
            home['hands'].append((k, t, g))
            continue
        mv = OVER.match(text)
        if mv:
            k = int(mv.group(1))
            for tb in tables:
                if tb['hands'] and tb['hands'][-1][0] == k:
                    tb['over'].append((k, t))
                    break
            continue
        mt = TURN.match(text)
        if mt:
            k = int(mt.group(1))
            for tb in tables:
                if tb['hands'] and tb['hands'][-1][0] == k:
                    tb['turns'] += 1
                    break
    print('==', log.name, '--', len(tables), 'table(s)')
    for i, tb in enumerate(tables, 1):
        hands = tb['hands']
        gaps = [b[1] - a[1] for a, b in zip(hands, hands[1:])]
        print('  table %d: %d hands opened (#%d at %.1f s .. #%d at %.1f s), %d over, %d turns, genesis %s..%s%s' % (
            i, len(hands), hands[0][0], hands[0][1], hands[-1][0], hands[-1][1], len(tb['over']), tb['turns'],
            hands[0][2], hands[-1][2],
            (', %.1f s between hands' % (sum(gaps) / len(gaps))) if gaps else ''))
    if len(tables) >= 2:
        a, b = tables[0]['hands'], tables[1]['hands']
        overlap_from = max(a[0][1], b[0][1])
        overlap_to = min(a[-1][1], b[-1][1])
        if overlap_to > overlap_from:
            both = [h for tb in tables[:2] for h in tb['hands'] if overlap_from <= h[1] <= overlap_to]
            print('  -> both tables running from %.1f to %.1f s: %d hands opened across them in that time' % (
                overlap_from, overlap_to, len(both)))


if __name__ == '__main__':
    main()
