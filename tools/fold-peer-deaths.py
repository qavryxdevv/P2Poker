# -*- coding: utf-8 -*-
"""What patch 0021 sees: every group peer entry that was deleted, and what it was.

A peer entry used to disappear in silence, and that silence was `S1-AA`'s
remaining question: a seat that never enters the group holds no peer entry, and
nothing on disk could tell *the entry was created and then reaped* from *it was
never created*. Those are different bugs at different layers.

`do_peer_delete` is the one site every deletion passes through. The line reads

    p2p-poker: deleting group peer 3, exit type 1, confirmed 0, handshaked 0,
    handshake attempts 4, tcp_connection_num 5, peer hash 744691702,
    oob handshake 0, ever heard from 1, last packet 13 s ago,
    7 peer(s) before this one

and the fields that decide things are:

  exit type      0 QUIT, 1 TIMEOUT, 2 DISCONNECTED, 3 SELF_DISCONNECTED,
                 4 KICKED, 5 SYNC_ERR, 6 NO_CALLBACK. Type 0 with
                 `confirmed 1, handshaked 1` is the orderly shutdown at the end
                 of a run: it is this instrument's BACKGROUND, not a fault.
  confirmed /    how far the entry got before it died.
  handshaked
  attempts       how many handshake packets this node sent for it. **The JOINER
                 initiates** -- `handle_gc_invite_confirmed_packet` sets
                 `pending_handshake_type = HS_INVITE_REQUEST`
                 (`group_chats.c:8423`) -- so `attempts 0` at a seat that was
                 joining is a peer that never performed its own role.
  ever heard     `last_received_packet_time > 0`. Zero means the peer was never
                 heard from once, which separates "reaped after going quiet"
                 from "never answered at all".
  peer hash      distinct hashes are distinct PEERS. Counting lines rather than
                 hashes reads six peers lost as one peer lost six times, which
                 is the mistake this tool exists to stop.

Usage:
    python tools/fold-peer-deaths.py                  # every run on disk
    python tools/fold-peer-deaths.py split161548-9    # one run
    python tools/fold-peer-deaths.py a b              # compare two

A run taken before patch 0021 prints nothing, and the tool says so rather than
printing a zero.
"""
from __future__ import print_function
import io, os, re, sys
from collections import Counter

LINE = re.compile(
    r'deleting group peer (\d+), exit type (\d+), confirmed (\d+), handshaked (\d+), '
    r'handshake attempts (\d+), tcp_connection_num (-?\d+), peer hash (\d+), '
    r'oob handshake (\d+), ever heard from (\d+), last packet (\d+) s ago, '
    r'(\d+) peer\(s\) before this one')

EXIT = {0: 'QUIT', 1: 'TIMEOUT', 2: 'DISCONNECTED', 3: 'SELF_DISCONNECTED',
        4: 'KICKED', 5: 'SYNC_ERR', 6: 'NO_CALLBACK'}


def fold(run):
    d = os.path.join('runs', run)
    rows = []
    for f in sorted(os.listdir(d)):
        if not f.endswith('.log'):
            continue
        for raw in io.open(os.path.join(d, f), encoding='utf-8', errors='replace'):
            m = LINE.search(raw)
            if m:
                g = [int(x) for x in m.groups()]
                rows.append((f[:-4], g))
    return rows


def report(run, rows):
    print(run)
    peers = set(g[6] for _, g in rows)
    print('  %d deletion(s) naming %d distinct peer(s), on %d node(s)'
          % (len(rows), len(peers), len(set(n for n, _ in rows))))

    # The orderly shutdown is the background and is separated first, so it
    # cannot pad a fault count.
    shutdown = [r for r in rows if r[1][1] == 0 and r[1][2] == 1 and r[1][3] == 1]
    rest = [r for r in rows if r not in shutdown]
    print('  %d of those are the orderly shutdown (QUIT, confirmed, handshaked) '
          '-- background, not a fault' % len(shutdown))
    if not rest:
        print('  nothing else. No peer was lost while the table was playing.')
        print()
        return

    print('  %d remain, naming %d distinct peer(s):'
          % (len(rest), len(set(g[6] for _, g in rest))))
    by = Counter()
    for _, g in rest:
        by[(EXIT.get(g[1], g[1]), g[2], g[3], g[4], g[8])] += 1
    for (et, conf, hs, att, heard), n in sorted(by.items(), key=lambda kv: -kv[1]):
        print('    %-9s confirmed %d, handshaked %d, attempts %d, ever heard %d  x%d'
              % (et, conf, hs, att, heard, n))
    lost = [r for r in rest if r[1][2] == 1 and r[1][3] == 1]
    if lost:
        print('  *** %d were FULLY JOINED peers (confirmed and handshaked) and '
              'were dropped anyway ***' % len(lost))
        for n, g in lost[:10]:
            print('      %-9s peer hash %d, attempts %d, last packet %d s ago'
                  % (n, g[6], g[4], g[9]))
    per = Counter(n for n, _ in rest)
    print('  per node: %s' % dict(per))
    print()


def main():
    root = 'runs'
    want = sys.argv[1:] or sorted(
        r for r in os.listdir(root) if os.path.isdir(os.path.join(root, r)))
    silent = []
    for run in want:
        d = os.path.join(root, run)
        if not os.path.isdir(d):
            print('%s: no such run' % run)
            continue
        rows = fold(run)
        if not rows:
            silent.append(run)
            continue
        report(run, rows)
    if silent:
        if len(silent) == len(want):
            print('no run carried the instrument.')
        print('%d run(s) printed nothing. A run taken before patch 0021 has no '
              'instrument, which is not the same as no deletions.' % len(silent))
        if len(silent) <= 12:
            print('  ' + ', '.join(silent))


if __name__ == '__main__':
    main()
