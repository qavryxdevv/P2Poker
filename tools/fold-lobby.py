# -*- coding: utf-8 -*-
"""The lobby measurement (D-040): when each node saw the founder's table, by
which road, and when and why it went.

Reads a run's logs and prints, per node other than the founder:

    n1  saw at   3.4 s (by asking)      gone at  62.1 s (withdrawn: its founder no longer offers it; 2.0 s after the founder left)
    n2  saw at  31.2 s (by the mesh)    gone at  63.0 s (...)

The founder's own lines give the marks: `hosting <hash>` (the table exists),
`advertised, N bytes` (its first advert left), `left the table` or
`fault-harness: leaving the table` (it closed). A watcher's lines:
`table <key> (<name>)` is the window's own note when a table row appears;
`lobby: table <key> (<name>) heard by asking <peer>` says the question carried
it rather than the mesh; `lobby: table <key> (<name>) withdrawn: ...` and
`N table(s) expired` say why it went.

Usage:
    python tools/fold-lobby.py run173830-3
    python tools/fold-lobby.py            # the newest run
"""
from __future__ import print_function
import io
import os
import re
import sys

# A split run's far logs carry a wall-clock prefix before the run second.
STAMP = re.compile(r'^(?:\d\d:\d\d:\d\d\.\d+\s+)?\s*([0-9.]+)\s+(.*)$')
SEEN = re.compile(r'^table ([0-9a-f]+) \((.+)\)$')
ASKED = re.compile(r'^lobby: table ([0-9a-f]+) \((.+?)\) heard by asking')
WITHDRAWN = re.compile(r'^lobby: table ([0-9a-f]+) \((.+?)\) withdrawn: (.*)$')
EXPIRED = re.compile(r'^(\d+) tables? expired$')
GONE_NOTE = re.compile(r'^table ([0-9a-f]+) gone: (.*)$')


def lines_of(path):
    out = []
    for raw in io.open(path, encoding='utf-8', errors='replace'):
        m = STAMP.match(raw)
        if m:
            out.append((float(m.group(1)), m.group(2).strip()))
    return out


def founder_marks(lines):
    hosting = advertised = left = None
    for t, l in lines:
        if hosting is None and l.startswith('hosting '):
            hosting = t
        if advertised is None and l.startswith('advertised, '):
            advertised = t
        if left is None and (l == 'left the table' or l.startswith('fault-harness: leaving the table')):
            left = t
    return hosting, advertised, left


def watcher_marks(lines):
    seen = None
    road = None
    gone = None
    why = None
    for i, (t, l) in enumerate(lines):
        if seen is None and (SEEN.match(l) or ASKED.match(l)):
            seen = t
            # The window's note comes first and the road's line right after it,
            # in the same instant: read the next line before calling it the mesh.
            road = 'by the mesh'
            for t2, l2 in lines[i:i + 3]:
                if ASKED.match(l2) and t2 - t <= 0.3:
                    road = 'by asking'
        if seen is not None and gone is None:
            m = WITHDRAWN.match(l)
            if m:
                gone, why = t, 'withdrawn: ' + m.group(3)
                continue
            m = GONE_NOTE.match(l)
            if m:
                gone, why = t, m.group(2)
                continue
            if EXPIRED.match(l):
                gone, why = t, 'expired (nothing heard for AD_TTL_MS)'
    return seen, road, gone, why


def main():
    root = 'runs'
    if not os.path.isdir(root):
        sys.exit('no runs/ directory here; run this from the repository root')
    if sys.argv[1:]:
        run = sys.argv[1]
    else:
        run = sorted(os.listdir(root), key=lambda d: os.path.getmtime(os.path.join(root, d)))[-1]
    d = os.path.join(root, run)
    logs = sorted(f for f in os.listdir(d) if f.endswith('.log'))
    if 'n0.log' not in logs:
        sys.exit('no n0.log in %s' % d)
    hosting, advertised, left = founder_marks(lines_of(os.path.join(d, 'n0.log')))
    print('%s: founder hosting at %s s, first advert at %s s, left at %s s'
          % (run, hosting, advertised, left if left is not None else 'never'))
    for f in logs:
        if f == 'n0.log':
            continue
        seen, road, gone, why = watcher_marks(lines_of(os.path.join(d, f)))
        node = f.replace('.log', '')
        if seen is None:
            print('  %-10s never saw the table' % node)
            continue
        line = '  %-10s saw at %6.1f s (%s)' % (node, seen, road)
        if gone is not None:
            after = ''
            if left is not None:
                after = '; %.1f s after the founder left' % (gone - left)
            line += '   gone at %6.1f s (%s%s)' % (gone, why, after)
        elif left is not None:
            line += '   still listed when the run ended, %s s after the founder left' % 'many'
        print(line)


if __name__ == '__main__':
    main()
