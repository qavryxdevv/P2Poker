# -*- coding: utf-8 -*-
"""D-034's clock from the other seats' side: how long each observer waited for
a seat's action, from the moment it saw the turn pass to that seat until it
saw the action land.

The headless log says `hand #k: waiting for seat S` when the turn passes to S
(the window's own note on `NotYourTurn`), `hand #k: your turn` when it is this
seat's, and the wait ends at the next line about the same hand that names a
turn or the hand's end. A run with `-ThinkMs 60000` (or the split harness's
`-Think 60000`) makes every seat sit past the table's thirty seconds, so the
seat's own client acts for it and the wait measures the whole road: the
thirty seconds on the actor's own clock, anchored to when the turn was given
(the D-034 amendment), plus one delivery.

Prints, per log: the waits' count, median and longest, and every wait over
the table's thirty seconds by more than five seconds.

Usage:
    python tools/fold-clock.py split193449-3
    python tools/fold-clock.py               # the newest run
"""
from __future__ import print_function
import io
import os
import re
import sys

STAMP = re.compile(r'^(?:\d\d:\d\d:\d\d\.\d+\s+)?\s*([0-9.]+)\s+(.*)$')
WAITING = re.compile(r'^hand #(\d+)(?: is|:) waiting for seat (\d+)$')
YOURS = re.compile(r'^hand #(\d+): your turn')
OVER = re.compile(r'^hand #(\d+) is over')
ACTED = re.compile(r'^your clock ran out|^autoplay: ')


def waits_of(path):
    """-> [(hand, seat, seconds)]"""
    out = []
    open_wait = None  # (hand, seat, since)
    for raw in io.open(path, encoding='utf-8', errors='replace'):
        m = STAMP.match(raw)
        if not m:
            continue
        t = float(m.group(1))
        line = m.group(2).strip()
        w = WAITING.match(line)
        y = YOURS.match(line)
        o = OVER.match(line)
        if open_wait is not None:
            hand, seat, since = open_wait
            ended = False
            if w and (int(w.group(1)) != hand or int(w.group(2)) != seat):
                ended = True
            if y and int(y.group(1)) == hand:
                ended = True
            if o and int(o.group(1)) == hand:
                ended = True
            if ended:
                out.append((hand, seat, t - since))
                open_wait = None
        if w and open_wait is None:
            open_wait = (int(w.group(1)), int(w.group(2)), t)
    return out


def main():
    root = 'runs'
    if sys.argv[1:]:
        run = sys.argv[1]
    else:
        run = sorted(os.listdir(root), key=lambda d: os.path.getmtime(os.path.join(root, d)))[-1]
    d = os.path.join(root, run)
    print(run)
    for f in sorted(x for x in os.listdir(d) if x.endswith('.log')):
        waits = waits_of(os.path.join(d, f))
        secs = sorted(s for _, _, s in waits)
        if not secs:
            print('  %-10s no waits' % f.replace('.log', ''))
            continue
        med = secs[len(secs) // 2]
        print('  %-10s %3d waits, median %5.1f s, longest %5.1f s' % (f.replace('.log', ''), len(secs), med, secs[-1]))
        for hand, seat, s in waits:
            if s > 35.0:
                print('             hand #%d, seat %d: %.1f s' % (hand, seat, s))


if __name__ == '__main__':
    main()
