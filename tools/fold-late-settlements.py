# -*- coding: utf-8 -*-
"""How often a hand ends by abort and is then settled late -- and how.

`S1-CL` found that a seat which gave up before settling adopts whichever peer's
`HAND_COMPLETE` arrives first, and fixed it so a borrowed body closes the late
stage only when every heard body agrees. Until 2026-09-08 the late path printed
a line only on **disagreement**, so the corpus could say the fixed branch had
never fired -- zero disagreement notes across 1 569 aborts -- and could not say
how often the late path fired at all. It prints on every close now.

The lines, all on the `settle_note` channel, all greppable:

    the late settlement of hand #N closed: this client's own body, every
        required seat heard                       -- gave up WHILE settling
    the late settlement of hand #N closed: a peer's body, every required
        seat heard                                -- gave up BEFORE settling,
                                                     adopted the peers' body
    ... not all agreeing                          -- an own body closed over
                                                     a dissent (own bodies are
                                                     never gated)
    late settlement disagreement with seat S: ... -- a later body differed
    ... its emitters disagree ... adopts none of them
                                                  -- S1-CL's fixed branch: a
                                                     borrowed body under
                                                     disagreement, refused
    a settlement of hand #N from seat S arrived during hand #N+1: not applied
                                                  -- S1-CN: the settlement came
                                                     after the 800 ms the late
                                                     path is open for; this seat
                                                     is on the abort's branch

Two facts in one call fold into one line, disagreement first, so a line can
match more than one bucket; the buckets below count LINES, and the "of which"
rows say how the overlap falls.

Usage:
    python tools/fold-late-settlements.py                  # every run on disk
    python tools/fold-late-settlements.py split115754-9    # one run

A run taken before the close line existed reports its disagreement and
refusal counts and says the close count is not recorded, rather than
printing a zero.
"""
from __future__ import print_function
import io, os, re, sys

# The local deadline abort carried a run of spaces inside the literal until
# 2026-09-08 (`every stack is                                  restored`), so a
# fold anchored on the plain phrase missed exactly the abort this tool is
# about; the source is fixed and the older logs are read as they are.
ABORT = re.compile(r'every stack is\s+restored')
# S1-CN: a settlement of the aborted hand arrived after the 800 ms in which
# the late path could have taken it.
NOT_APPLIED = re.compile(r'a settlement of hand #\d+ from seat .* arrived during hand #\d+: not applied')
CLOSED_OWN = re.compile(r"late settlement of hand #\d+ closed: this client's own body")
CLOSED_PEER = re.compile(r"late settlement of hand #\d+ closed: a peer's body")
NOT_ALL = re.compile(r'not all agreeing')
DISAGREE = re.compile(r'late settlement disagreement with seat')
REFUSED = re.compile(r'adopts none of them')


def fold(run):
    d = os.path.join('runs', run)
    c = dict(aborts=0, own=0, peer=0, not_all=0, disagree=0, refused=0, missed=0, lines=0)
    for f in sorted(os.listdir(d)):
        if not f.endswith('.log'):
            continue
        for raw in io.open(os.path.join(d, f), encoding='utf-8', errors='replace'):
            if ABORT.search(raw):
                c['aborts'] += 1
            hit = False
            if CLOSED_OWN.search(raw):
                c['own'] += 1; hit = True
            if CLOSED_PEER.search(raw):
                c['peer'] += 1; hit = True
            if NOT_ALL.search(raw):
                c['not_all'] += 1; hit = True
            if DISAGREE.search(raw):
                c['disagree'] += 1; hit = True
            if REFUSED.search(raw):
                c['refused'] += 1; hit = True
            if NOT_APPLIED.search(raw):
                c['missed'] += 1; hit = True
            if hit:
                c['lines'] += 1
    return c


def main():
    root = 'runs'
    want = sys.argv[1:] or sorted(
        r for r in os.listdir(root) if os.path.isdir(os.path.join(root, r)))
    tot = dict(aborts=0, own=0, peer=0, not_all=0, disagree=0, refused=0, missed=0, runs=0)
    silent = 0
    for run in want:
        if not os.path.isdir(os.path.join(root, run)):
            print('%s: no such run' % run)
            continue
        c = fold(run)
        tot['runs'] += 1
        for k in ('aborts', 'own', 'peer', 'not_all', 'disagree', 'refused', 'missed'):
            tot[k] += c[k]
        if c['lines'] == 0 and c['aborts'] == 0:
            silent += 1
            continue
        if len(want) <= 6:
            print(run)
            print('  aborts %d' % c['aborts'])
            if c['own'] + c['peer'] == 0 and c['disagree'] + c['refused'] + c['missed'] == 0:
                print('  late settlements: none printed -- either none happened, or the run '
                      'predates the close line (2026-09-08) and had no disagreement')
            else:
                print('  late stage closed on an OWN body   %d%s'
                      % (c['own'], ('  (of which over a dissent %d)' % c['not_all']) if c['not_all'] else ''))
                print("  late stage closed on a PEER's body  %d" % c['peer'])
                print('  disagreement notes                  %d' % c['disagree'])
                print('  refused (S1-CL fixed branch)        %d' % c['refused'])
                print('  missed the 800 ms window (S1-CN)    %d' % c['missed'])
            print()
    print('%d run(s): aborts %d; late closes -- own %d (over a dissent %d), peer %d; '
          'disagreements %d; refusals %d; settlements that missed the 800 ms window %d'
          % (tot['runs'], tot['aborts'], tot['own'], tot['not_all'], tot['peer'],
             tot['disagree'], tot['refused'], tot['missed']))
    if silent:
        print('  %d run(s) had neither an abort nor a late-settlement line.' % silent)
    print('  Close counts are recorded only from 2026-09-08 on; older runs contribute '
          'aborts and disagreements and nothing else, which is an absent instrument '
          'and not a zero.')


if __name__ == '__main__':
    main()
