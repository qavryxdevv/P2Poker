# -*- coding: utf-8 -*-
"""S1-BR's fold, made permanent.

The row's stopping rule is a fold, and the row said so: *"the fold is four
lines of Python over the instrument's own line and should be re-run against any
future stall rather than re-derived."* It was re-derived four times anyway,
because it lived nowhere — and each re-derivation had to rediscover that **the
entry count is not the line count**, since one line names every seat the client
is waiting for. That correction is now in the code instead of in somebody's
memory.

`Hand::vote_state` prints, at most every 30 s, on any client whose open stage
has outlived its deadline while it has not voted about a seat it waits for:

    not voting: stage 22 open 30 s, deadline Some(30) s for Some(DeckCommit),
    long past false, my seat 1, sequence 22; seat 0: subject yes,
    voters [1, 2, ...], I voted false, votes held 0, mid-delivery true

Every per-seat entry falls in one of six buckets, and **only the last one is
what S1-BR is open for**: a vote owed, eligible, past twice the stage budget,
and not cast.

  already voted  this client voted about that seat and is waiting on others
  bystander      this client is not in the voter set at all -- a seat
                 certified out and still watching, `S1-BW`'s bystander seen
                 from the vote's side
  no subject     the stage names no subject for that seat yet
  lever          `S1-BK`: the vote is held while the carrier is still
                 delivering that seat's traffic, and the gate yields at
                 `long_past_stage`, twice *that stage's* budget -- so a wait
                 longer than the printed `deadline` is not an overrun
  alone          the voter set is this client alone: D-036's floor needs two
                 voters, so heads-up nobody certifies anybody (D-007, D-034)
                 and the vote is deliberately not cast
  UNMET          eligible, not voted, long past, and not mid-delivery

Usage:
    python tools/fold-vote-state.py                # every run on disk
    python tools/fold-vote-state.py split092359-10 # one run
"""
from __future__ import print_function
import io, os, re, sys
from collections import Counter

HEAD = re.compile(
    r'not voting: stage (\d+) open (\d+) s, deadline (\w+)(?:\((\d+)\))? s '
    r'for (\w+)(?:\((\w+)\))?, long past (\w+), my seat (\d+), sequence (\d+); (.*)$')
SEAT = re.compile(
    r'seat (\d+): subject (\w+), voters \[([0-9, ]*)\], I voted (\w+), '
    r'votes held (\d+), mid-delivery (\w+)')


def fold(paths):
    cat = Counter()
    owed = Counter()
    worst = {}
    unmet = []
    lines = 0
    entries = 0
    for path in paths:
        node = os.path.basename(os.path.dirname(path)) + '/' + \
               os.path.basename(path).replace('.log', '')
        for raw in io.open(path, encoding='utf-8', errors='replace'):
            m = HEAD.search(raw)
            if not m:
                continue
            lines += 1
            age = int(m.group(2))
            budget = m.group(4)
            typ = m.group(6) or m.group(5)
            long_past = m.group(7) == 'true'
            me = int(m.group(8))
            for s in SEAT.finditer(m.group(10)):
                entries += 1
                seat = int(s.group(1))
                subject = s.group(2) == 'yes'
                voters = [int(v) for v in s.group(3).replace(' ', '').split(',') if v != '']
                voted = s.group(4) == 'true'
                mid = s.group(6) == 'true'
                if voted:
                    k = 'already voted'
                elif me not in voters:
                    k = 'bystander'
                elif len(voters) < 2:
                    k = 'alone (D-036 floor)'
                elif not subject:
                    k = 'no subject'
                elif not long_past:
                    k = 'lever' if mid else 'within the stage budget'
                elif mid:
                    k = 'lever, past the printed deadline'
                else:
                    k = 'UNMET'
                    unmet.append('%s  stage %s, %s s (budget %s), owed %s, '
                                 'seat %d, voters %s'
                                 % (node, m.group(1), age, budget, typ, seat, voters))
                cat[k] += 1
                owed[(k, typ)] += 1
                if age > worst.get(k, (-1, ''))[0]:
                    worst[k] = (age, '%s s against a %s s budget, %s, %s'
                                     % (age, budget, typ, node))
    return cat, owed, worst, unmet, lines, entries


def main():
    root = 'runs'
    if not os.path.isdir(root):
        sys.exit('no runs/ directory here; run this from the repository root')
    want = sys.argv[1:]
    paths = []
    for run in sorted(os.listdir(root)):
        if want and run not in want:
            continue
        d = os.path.join(root, run)
        if not os.path.isdir(d):
            continue
        for f in sorted(os.listdir(d)):
            if f.endswith('.log'):
                paths.append(os.path.join(d, f))
    if not paths:
        sys.exit('no logs matched %s' % (want or 'runs/*'))

    cat, owed, worst, unmet, lines, entries = fold(paths)
    runs = len(set(os.path.dirname(p) for p in paths))
    print('%d run(s), %d log(s), %d instrument line(s), %d per-seat entries'
          % (runs, len(paths), lines, entries))
    if lines:
        print('  (%.1f entries to a line -- one line names every seat the '
              'client waits for)' % (float(entries) / lines))
    print()
    for k in ('lever', 'lever, past the printed deadline', 'within the stage budget',
              'bystander', 'already voted', 'no subject', 'alone (D-036 floor)', 'UNMET'):
        if cat.get(k) is None and k != 'UNMET':
            continue
        print('  %-32s %d' % (k, cat.get(k, 0)))
        if k in worst:
            print('  %-32s longest: %s' % ('', worst[k][1]))
    print()
    print('  owed type by bucket:')
    for (k, t), n in sorted(owed.items(), key=lambda kv: (-kv[1], kv[0])):
        print('    %-32s %-14s %d' % (k, t, n))
    print()
    if unmet:
        print('  *** UNMET, which is what S1-BR is open for: %d ***' % len(unmet))
        for u in unmet[:40]:
            print('    ' + u)
        if len(unmet) > 40:
            print('    ... and %d more' % (len(unmet) - 40))
    else:
        print('  UNMET 0 -- no vote owed, eligible, past twice the budget and '
              'not cast, anywhere in this corpus.')


if __name__ == '__main__':
    main()
