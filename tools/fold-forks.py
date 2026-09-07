# -*- coding: utf-8 -*-
"""The fork census, made permanent, because S1-CE's theorem rests on it.

A **fork** is one hand id for which two clients printed different `GENESIS(k)`
values. `S1-CE` is one *direction* of that -- this client's roster a strict
subset of the table's -- and the census exists to say how rare that direction
is against the others.

Two lines are read, and the second one is a trap this file has to dodge:

    hand #9 opens at genesis 1b982887 with seats [0, 2, 3, ...]
    hand #9 re-opens at genesis 43fd0b05 with seats [...] (was ff5edbee ...)
    hand #9 has begun

`has begun` is printed off `Hand::dealt`, which is
`!matches!(self.phase, Phase::Init(_))` -- **true for `Phase::Aborted` too**
(`S1-CG`). A hand that aborts at stage 0 therefore prints *has begun* and *is
over* milliseconds apart, and a census that counts the first line as "this
branch left stage 0" counts every stage-0 abort as a branch that advanced.
Measured: `split092359-10/far-n1.log` prints them 3 ms apart. So a branch counts
as ADVANCED only if `has begun` is not followed by `is over` for the same hand
within one second.

Usage:
    python tools/fold-forks.py            # every run on disk
    python tools/fold-forks.py split165440-9
"""
from __future__ import print_function
import io, os, re, sys
from collections import defaultdict

OPENS = re.compile(r'hand #(\d+) (?:re-)?opens at genesis ([0-9a-f]+) with seats \[([0-9, ]*)\]')
BEGUN = re.compile(r'hand #(\d+) has begun')
OVER = re.compile(r'hand #(\d+) is over')
STAMP = re.compile(r'^\d\d:\d\d:\d\d\.\d\d\d\s+([0-9.]+)\s')

ADVANCED_GAP = 1.0   # seconds; below this an "is over" means a stage-0 abort


def read_node(path):
    """-> {hand: genesis}, {hand: advanced?}"""
    genesis = {}
    begun = {}
    over = {}
    for raw in io.open(path, encoding='utf-8', errors='replace'):
        m = STAMP.match(raw)
        t = float(m.group(1)) if m else None
        o = OPENS.search(raw)
        if o:
            genesis[int(o.group(1))] = o.group(2)          # last open wins
            continue
        b = BEGUN.search(raw)
        if b and t is not None:
            begun.setdefault(int(b.group(1)), t)
            continue
        v = OVER.search(raw)
        if v and t is not None:
            over.setdefault(int(v.group(1)), t)
    advanced = {}
    for h, tb in begun.items():
        to = over.get(h)
        advanced[h] = (to is None) or (to - tb > ADVANCED_GAP)
    return genesis, advanced


def main():
    root = 'runs'
    args = sys.argv[1:]
    # **The falsification switch, and it is in the tool rather than in a memo.**
    # Passing it counts a stage-0 abort as a branch that advanced -- which is
    # the reading `S1-CG` describes -- so anyone can see for themselves what the
    # filter is worth on this corpus rather than taking this file's word for it.
    global ADVANCED_GAP
    if '--count-aborts-as-advanced' in args:
        args.remove('--count-aborts-as-advanced')
        ADVANCED_GAP = -1.0
    want = args
    runs = 0
    forks = 0
    by_advancing = defaultdict(int)
    subset = []
    superset = []
    same_seats = []
    two_branches = []
    for run in sorted(os.listdir(root)):
        d = os.path.join(root, run)
        if not os.path.isdir(d) or (want and run not in want):
            continue
        logs = [f for f in sorted(os.listdir(d)) if f.endswith('.log')]
        if not logs:
            continue
        runs += 1
        nodes = {}
        for f in logs:
            nodes[f[:-4]] = read_node(os.path.join(d, f))
        hands = set()
        for g, _ in nodes.values():
            hands |= set(g)
        for h in sorted(hands):
            seen = {}
            for name, (g, _) in nodes.items():
                if h in g:
                    seen.setdefault(g[h], []).append(name)
            if len(seen) < 2:
                continue
            forks += 1
            adv = {}
            for val, names in seen.items():
                adv[val] = any(nodes[n][1].get(h, False) for n in names)
            n_adv = sum(1 for v in adv.values() if v)
            by_advancing[n_adv] += 1
            if n_adv >= 2:
                two_branches.append('%s #%d %s' % (run, h, sorted(adv)))
            # The minority value's holders against the majority's. This is a
            # count of CLIENTS at each genesis, not a comparison of the rosters
            # inside them -- enough to say which side was left behind, and not
            # enough to say subset from superset. `S1-CE` needs the latter and
            # this tool does not claim it.
            order = sorted(seen.items(), key=lambda kv: -len(kv[1]))
            maj, mino = order[0], order[-1]
            tag = '%s #%d  %s(%d) vs %s(%d)' % (
                run, h, maj[0][:8], len(maj[1]), mino[0][:8], len(mino[1]))
            if n_adv == 1:
                if adv.get(maj[0]) and not adv.get(mino[0]):
                    subset.append(tag)
                else:
                    superset.append(tag)
            elif n_adv == 0:
                same_seats.append(tag)

    print('%d run(s) with logs, %d forked hand(s)' % (runs, forks))
    for k in sorted(by_advancing):
        print('  %d branch(es) advanced past stage 0: %d fork(s)' % (k, by_advancing[k]))
    print()
    print('  forks where NO branch advanced   : %d' % by_advancing.get(0, 0))
    print('  forks where exactly one advanced : %d' % by_advancing.get(1, 0))
    print('  forks where TWO or more advanced : %d' % sum(
        v for k, v in by_advancing.items() if k >= 2))
    if two_branches:
        print()
        print('  *** two branches past stage 0 -- this is what S1-CE would need: ***')
        for s in two_branches[:20]:
            print('    ' + s)
    if want and (subset or superset):
        print()
        print('  majority advanced (minority stranded):')
        for s in subset[:20]:
            print('    ' + s)
        print('  minority advanced:')
        for s in superset[:20]:
            print('    ' + s)


if __name__ == '__main__':
    main()
