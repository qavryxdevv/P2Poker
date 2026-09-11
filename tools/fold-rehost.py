# -*- coding: utf-8 -*-
"""The second table's timeline, per node, from a table-run.ps1 -RehostAt run
(D-042, S1-DN): when the instance came up, when the first table's hands ran,
when the tournament ended (with -StartStack) and its group was left, when
the founder hosted the second table, when each joiner heard of it, left,
was seated, entered the group, and when the second table's first hand
opened. Prints one block per node and the deltas that matter.

    python tools/fold-rehost.py runs/run<stamp>-3

Every node's log carries its own wall-clock offset (the harness stamps each
line with seconds since that process started; the joiners start later than
the founder), so the deltas are within one node and never across nodes.
"""
import re
import sys
from pathlib import Path

LINE = re.compile(r'^\s*(\d+\.\d)\s+(.*)$')

MARKS = [
    ('instance up', re.compile(r"tox: this client's instance is up from the start")),
    ('no instance', re.compile(r"tox: no instance at the client's start")),
    ('hosting', re.compile(r'^hosting  (\S+)')),
    ('stack', re.compile(r'fault-harness: every seat starts with (\d+) chips')),
    ('waiting to be invited', re.compile(r"waiting to be invited")),
    ('in the group', re.compile(r"in the table's Tox group")),
    ('hand opens', re.compile(r'^hand #(\d+) opens at genesis')),
    ('hand over', re.compile(r'^hand #(\d+) is over')),
    ('no next hand', re.compile(r'the table has no next hand to deal')),
    ('tournament over', re.compile(r'the tournament is over: this client left')),
    ('leaving for another', re.compile(r'leaving the table for another')),
    ('left', re.compile(r'^left the table')),
    ('heard', re.compile(r'lobby: table \S+ \((\S+)\) heard by asking')),
    ('looking', re.compile(r'^looking for (\S+) \(the second table\)')),
    ('seated', re.compile(r'^seat (\d+) at ')),
    # The certificate's own words ("certified seat 1's timeout, unanimously
    # among [0, 2]"), not the isolated seat's "will certify it out" nor the
    # roster derivation's "by_certificate=".
    ('certified', re.compile(r"certified seat \d+'s")),
]
FRIENDS = re.compile(r'tox friends up (\d+)')


def fold(log: Path):
    events = []
    friends = []
    for raw in log.read_text(encoding='utf-8', errors='replace').splitlines():
        m = LINE.match(raw)
        if not m:
            continue
        t, text = float(m.group(1)), m.group(2)
        fm = FRIENDS.search(text)
        if fm:
            friends.append((t, int(fm.group(1))))
        for name, pat in MARKS:
            pm = pat.search(text)
            if pm:
                detail = pm.group(1) if pm.groups() else ''
                events.append((t, name, detail, text[:110]))
                break
    return events, friends


def main():
    run = Path(sys.argv[1] if len(sys.argv) > 1 else '.')
    logs = sorted(run.glob('n*.log'))
    if not logs:
        print('no n*.log under', run)
        return
    for log in logs:
        events, friends = fold(log)
        print('==', log.name)
        hands = [e for e in events if e[1] == 'hand opens']
        # The second table's hand #1 is the second "hand #1 opens" line.
        firsts = [e for e in hands if e[2] == '1']
        hosting = [e for e in events if e[1] == 'hosting']
        for e in events:
            if e[1] in ('hand opens', 'hand over') and e not in firsts:
                continue
            if e[1] == 'seated' and len([x for x in events if x[1] == 'seated' and x[0] <= e[0]]) > 2:
                continue
            if e[1] == 'certified':
                continue
            print('  %7.1f  %-22s %s' % (e[0], e[1], e[2] or e[3]))
        if len(hosting) >= 2 and len(firsts) >= 2:
            print('  -> second table: hosted at %.1f, its hand #1 at %.1f, delta %.1f s' % (
                hosting[1][0], firsts[1][0], firsts[1][0] - hosting[1][0]))
        lefts = [e for e in events if e[1] == 'left']
        waits = [e for e in events if e[1] == 'waiting to be invited']
        groups = [e for e in events if e[1] == 'in the group']
        if lefts and len(waits) >= 2 and len(groups) >= 2 and len(firsts) >= 2:
            print('  -> joiner: left at %.1f, seated at the second table %.1f (+%.1f), in its group %.1f (+%.1f after seating), hand #1 %.1f (+%.1f after leaving)' % (
                lefts[-1][0], waits[-1][0], waits[-1][0] - lefts[-1][0],
                groups[-1][0], groups[-1][0] - waits[-1][0],
                firsts[1][0], firsts[1][0] - lefts[-1][0]))
        over = [e for e in events if e[1] == 'no next hand']
        gone = [e for e in events if e[1] == 'tournament over']
        if over and gone:
            print('  -> tournament: no next hand at %.1f, group left at %.1f (+%.1f)' % (
                over[0][0], gone[0][0], gone[0][0] - over[0][0]))
        certs = [e for e in events if e[1] == 'certified']
        late = [e for e in certs if over and e[0] > over[0][0]]
        if late:
            print('  -> %d certificate line(s) AFTER the end: %s' % (len(late), late[0][3]))
        if hosting and friends:
            around = [f for f in friends if hosting[-1][0] - 5 <= f[0] <= hosting[-1][0] + 60]
            if around:
                print('  -> tox friends up around the last hosting: %s' % ' '.join('%.0fs=%d' % f for f in around))


if __name__ == '__main__':
    main()
