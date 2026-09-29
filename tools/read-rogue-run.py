# -*- coding: utf-8 -*-
"""Read one `-Rogue` run (S1-JR): what the honest seats did about a rogue's client.

The owner's rule (2026-09-29): no player at a table may hold its game up for
ever; what the table's certificates cannot settle, the player is told, and
asked whether to leave. A run with `tools/table-run.ps1 -Rogue <kinds>
-RogueNode <n>` makes one node play a rogue's client; this reads, per node:

  * the hands opened, played out, and called off -- by why (a shuffle proof or a
    card share that does not hold, found here or proven by another seat; the
    deadline; anything else);
  * the first time the node said the table is not safe, how long after its
    first hand, and why; how often it said so, and how often with the running
    hand standing (the window may cover that hand, and no other);
  * the seats certified out and put out for good;
  * the genesis of every hand at every node: a hand with two values is a fork
    between honest seats, the worst outcome there is.

Usage:
    python tools/read-rogue-run.py run153012-3
"""
from __future__ import print_function
import io, os, re, sys

STAMP = re.compile(r'^(?:(\d\d:\d\d:\d\d\.\d\d\d)\s+)?\s*([0-9.]+)\s+(.*)$')
OPENS = re.compile(r'hand #(\d+) (?:re-)?opens at genesis ([0-9a-f]+) with seats \[([^\]]*)\]')
OVER = re.compile(r'hand #(\d+) is over')
NOT_SAFE = re.compile(r'this table is not safe: (.*)')
CERTIFIED = re.compile(r'certified (?:seat \d+|out)|out of the table for good|out for good')
VOID = [
    ('shuffle proof, found here', re.compile(r"seat \d+'s shuffle proof does not hold")),
    ('card share, found here', re.compile(r"seat \d+'s reveal proof does not hold")),
    ('shuffle proof, proven by a peer', re.compile(r'a peer proved a shuffle did not hold')),
    ('card share, proven by a peer', re.compile(r'a peer proved a reveal share did not hold')),
    ('deadline or certificate', re.compile(r'a peer ended the hand on its own deadline')),
    ('stage budget, given up here', re.compile(r'the hand ran out of time')),
]


def lines(path):
    for raw in io.open(path, encoding='utf-8', errors='replace'):
        m = STAMP.match(raw.rstrip('\n'))
        if m:
            yield float(m.group(2)), m.group(3)


def read_node(path):
    node = {
        'opened': [], 'over': [], 'voided': {}, 'not_safe': None, 'certified': 0,
        'first_hand_at': None, 'genesis': {}, 'said': 0, 'stands': [], 'asked_opponent': [],
    }
    current = None
    counted = set()
    for secs, text in lines(path):
        m = OPENS.search(text)
        if m:
            hand = int(m.group(1))
            current = hand
            node['opened'].append(hand)
            node['genesis'][hand] = m.group(2)[:8]
            if node['first_hand_at'] is None:
                node['first_hand_at'] = secs
            continue
        # A hand called off is the hand running when its abort was said, once:
        # the window's "is over" line is not said for every one of them.
        for label, rx in VOID:
            if rx.search(text) and current is not None and current not in counted:
                counted.add(current)
                node['voided'].setdefault(label, []).append(current)
                break
        m = OVER.search(text)
        if m:
            node['over'].append(int(m.group(1)))
            continue
        m = NOT_SAFE.search(text)
        if m:
            node['said'] += 1
            if node['not_safe'] is None:
                node['not_safe'] = (secs, m.group(1))
        if 'the running hand stands (S1-JR)' in text:
            node['stands'].append(secs)
        # Heads-up, the window's own question about the opponent (D-007).
        if 'held back its part of the cards' in text or 'on the clock for' in text:
            node['asked_opponent'].append(secs)
        if CERTIFIED.search(text):
            node['certified'] += 1
    return node


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

    nodes = {log[:-4]: read_node(os.path.join(root, log)) for log in logs}
    print('\n--- per node')
    for name, n in sorted(nodes.items()):
        voided = sum(len(v) for v in n['voided'].values())
        played = len(set(n['over']) - set(h for v in n['voided'].values() for h in v))
        print('%-6s opened %3d: played out %3d, called off %3d' % (
            name, len(set(n['opened'])), played, voided))
        for label, hands in sorted(n['voided'].items()):
            print('         called off -- %-32s %3d  hands %s' % (label, len(hands), hands[:12]))
        if n['not_safe']:
            secs, why = n['not_safe']
            after = '' if n['first_hand_at'] is None else ', %.0f s after its first hand' % (secs - n['first_hand_at'])
            print('         NOT SAFE at %.1f s%s: %s' % (secs, after, why[:200]))
            print('         asked %d times; the running hand said to stand %d times%s' % (
                n['said'], len(n['stands']),
                '' if not n['stands'] else ', first at %.1f s' % n['stands'][0]))
        else:
            print('         never said the table is not safe')
        if n['asked_opponent']:
            print('         asked about the heads-up opponent %d times, first at %.1f s' % (
                len(n['asked_opponent']), n['asked_opponent'][0]))
        if n['certified']:
            print('         certificate lines: %d' % n['certified'])

    print('\n--- genesis per hand per node; a hand with two values is a fork')
    hands = sorted(set(h for n in nodes.values() for h in n['genesis']))
    forks = 0
    for h in hands:
        values = set(n['genesis'][h] for n in nodes.values() if h in n['genesis'])
        if len(values) > 1:
            forks += 1
            print('hand %-4d %s   <-- %d values' % (
                h, ' '.join('%s=%s' % (k, n['genesis'].get(h, '-')) for k, n in sorted(nodes.items())), len(values)))
    print('forks: %d of %d hands' % (forks, len(hands)))
    return 0


if __name__ == '__main__':
    sys.exit(main())
