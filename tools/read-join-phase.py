# -*- coding: utf-8 -*-
"""Read one run's join phase through the handshake instruments (S1-AA).

The far seat -- the relay-only one -- reaps unconfirmed peers at 12 s in
most runs, each with four or five handshake attempts on the counter and
relay slots that were up within seconds (patches 0021, 0026). Patch 0028
makes the exchange legible: one line per handshake attempt with the door
it left by, one per silent drop on the way in with the reason, one per
completed handshake. This folds them per node and cross-references every
peer a node reaped with what the other end logged about it.

Prints, for every node:
  * reaps before the cut-off (default 150 s): peer hash, confirmed /
    handshaked, attempts, seconds since the last packet;
  * handshake attempts sent, by door (UDP / TCP fallback turn / TCP after
    a UDP failure) and by request vs response;
  * drops with their reason, and completed handshakes;
  * the first heartbeat that reads N/N confirmed.
Then, for every peer hash any node reaped: the reaper's attempts toward it,
and every line at every other node that names that hash (its attempts back,
its drops), so the place the exchange died is visible on one screen.

Usage:
    python tools/read-join-phase.py split134210-9          # cut-off 150 s
    python tools/read-join-phase.py split134210-9 200      # cut-off 200 s

A run taken before patch 0028 has reaps and no attempt or drop lines, and
the tool says so instead of printing zeros.
"""
from __future__ import print_function
import io, os, re, sys
from collections import defaultdict, Counter

STAMP = re.compile(r'^(\d\d:\d\d:\d\d\.\d\d\d)\s+([0-9.]+)\s+(.*)$')
REAP = re.compile(r'deleting group peer (\d+), exit type 1, confirmed (\d), handshaked (\d), handshake attempts (\d+), tcp_connection_num (-?\d+), peer hash (\d+), oob handshake \d, ever heard from \d, last packet (\d+) s ago')
ATTEMPT = re.compile(r'p2p-poker: handshake (request|response) attempt (\d+) to peer (\d+) left by ([^(]+) \(request type (\d+)')
NOTHING = re.compile(r'UDP handshake failed and no TCP relay carried it either\. Type 0x(..), peer (\d+)')
DROP = re.compile(r'p2p-poker: (out-of-band packet dropped: .*|out-of-band handshake from peer (\d+) refused by the handler|handshake packet dropped: .*|handshake response dropped: no peer entry for (\d+)|handshake response dropped: peer \d+ has no connection|handshake response from peer (\d+) .*|handshake request from known peer (\d+) dropped: .*)')
DONE = re.compile(r'p2p-poker: handshaked with peer (\d+) \(request type (\d+)\)')
FULL = re.compile(r'group (\d+) seen/(\d+) confirmed/(\d+) wanted')


def lines(path):
    for raw in io.open(path, encoding='utf-8', errors='replace'):
        m = STAMP.match(raw.rstrip('\n'))
        if m:
            yield float(m.group(2)), m.group(3)


def main():
    run = sys.argv[1]
    cutoff = float(sys.argv[2]) if len(sys.argv) > 2 else 150.0
    d = os.path.join('runs', run)
    if not os.path.isdir(d):
        print('%s: no such run' % run)
        return
    logs = sorted(f for f in os.listdir(d) if f.endswith('.log'))
    per = {}
    any_instrument = False
    for f in logs:
        n = f[:-4]
        reaps, attempts, nothing, drops, done, full = [], [], [], [], [], None
        for s, msg in lines(os.path.join(d, f)):
            if s > cutoff:
                break
            m = REAP.search(msg)
            if m:
                reaps.append((s, m.group(6), m.group(2), m.group(3), int(m.group(4)), int(m.group(7))))
                continue
            m = ATTEMPT.search(msg)
            if m:
                attempts.append((s, m.group(1), int(m.group(2)), m.group(3), m.group(4).strip(), int(m.group(5))))
                any_instrument = True
                continue
            m = NOTHING.search(msg)
            if m:
                nothing.append((s, m.group(2), m.group(1)))
                continue
            m = DROP.search(msg)
            if m:
                drops.append((s, m.group(1)))
                any_instrument = True
                continue
            m = DONE.search(msg)
            if m:
                done.append((s, m.group(1), int(m.group(2))))
                any_instrument = True
                continue
            m = FULL.search(msg)
            # "group 0 seen/0 confirmed/0 wanted" is a node still waiting to be
            # invited, not a full table: wanted must be above zero.
            if m and full is None and int(m.group(3)) > 0 and m.group(1) == m.group(3) and m.group(2) == m.group(3):
                full = s
        per[n] = dict(reaps=reaps, attempts=attempts, nothing=nothing, drops=drops, done=done, full=full)

    print('=== %s, join phase to %.0f s' % (run, cutoff))
    if not any_instrument:
        print('  no attempt, drop or handshaked lines: this run predates patch 0028; only the reaps below are readable')
    for n in sorted(per, key=lambda x: (x != 'far-n0', x)):
        p = per[n]
        doors = Counter(a[4] for a in p['attempts'])
        kinds = Counter(a[1] for a in p['attempts'])
        print('\n--- %s: %d reap(s), %d attempt(s) [%s] [%s], %d carried by nothing, %d drop(s), %d handshake(s) completed, full table first at %s'
              % (n, len(p['reaps']), len(p['attempts']),
                 ', '.join('%s %d' % kv for kv in sorted(doors.items())) or '-',
                 ', '.join('%s %d' % kv for kv in sorted(kinds.items())) or '-',
                 len(p['nothing']), len(p['drops']), len(p['done']),
                 ('%.1f s' % p['full']) if p['full'] is not None else 'never before the cut-off'))
        for s, h, conf, hs, att, last in p['reaps']:
            print('  %6.1f  reaped peer %s: confirmed %s, handshaked %s, attempts %d, last packet %d s ago' % (s, h, conf, hs, att, last))
        for s, why in p['drops'][:12]:
            print('  %6.1f  drop: %s' % (s, why[:120]))
        if len(p['drops']) > 12:
            print('          ... %d more drop line(s)' % (len(p['drops']) - 12))

    # cross-reference every reaped hash
    reaped = defaultdict(list)
    for n, p in per.items():
        for s, h, conf, hs, att, last in p['reaps']:
            reaped[h].append((n, s, att))
    if reaped:
        print('\n=== every reaped peer hash: the reaper\'s attempts toward it, and what every node logged about that hash')
    for h in sorted(reaped, key=lambda x: reaped[x][0][1]):
        for reaper, s, att in reaped[h]:
            p = per[reaper]
            mine = [(a[0], a[1], a[2], a[4]) for a in p['attempts'] if a[3] == h and a[0] <= s + 0.5]
            none = [(x[0]) for x in p['nothing'] if x[1] == h and x[0] <= s + 0.5]
            print('\n  peer %s, reaped by %s at %.1f s after %d attempt(s):' % (h, reaper, s, att))
            print('    %s\'s attempts toward it: %s' % (reaper, '  '.join('%.1f:%s#%d %s' % a for a in mine) or 'none printed'))
            if none:
                print('    carried by nothing at: %s' % '  '.join('%.1f' % x for x in none))
            for other in sorted(per):
                if other == reaper:
                    continue
                q = per[other]
                back = [(a[0], a[1], a[2], a[4]) for a in q['attempts'] if a[3] == h]
                dr = [(x[0], x[1]) for x in q['drops'] if h in x[1]]
                dn = [(x[0]) for x in q['done'] if x[1] == h]
                if back or dr or dn:
                    print('    %s about %s: attempts %s; drops %s; handshaked %s'
                          % (other, h, '  '.join('%.1f:%s#%d %s' % a for a in back[:8]) or '-',
                             '  '.join('%.1f:%s' % (x[0], x[1][:60]) for x in dr[:6]) or '-',
                             '  '.join('%.1f' % x for x in dn) or '-'))
        print('    (a hash names the same key at every node; a node never prints its own)')


if __name__ == '__main__':
    main()
