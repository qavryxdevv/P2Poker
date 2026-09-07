# -*- coding: utf-8 -*-
"""What patch 0019 sees: relays killed with a group's slots still on them.

`kill_tcp_relay_connection` removes the relay's slot from **every** `con_to`,
and patch 0019 counts them before the removal because afterwards the answer is
always zero. The line it prints is

    p2p-poker: a TCP relay is being killed and 7 connection(s) lose a slot
    (0 online, 3 registered, 4 neither). Relay 0, status 2, onion 0,
    lock_count 0, sleep_count 0

Patch 0020 added the bracketed breakdown, and it is what the total alone cannot
say: seven working out-of-band paths and seven attempts that never registered
are the same `7` and mean opposite things. Runs taken between 0019 and 0020
have the total and not the breakdown, and this tool says *not recorded* for
them rather than printing three zeroes.

`status` is the thing to read, and it decides which of two doors swung:

  status 2  = TCP_CONN_CONNECTED -> `kill_nonused_tcp`, the announce timeout.
              The relay WAS connected; it is reaped because `lock_count` is 0,
              and `lock_count` is incremented at exactly one site -- a slot
              going to `TCP_CONNECTIONS_STATUS_ONLINE`. Carrying out-of-band
              traffic for a REGISTERED slot therefore does not count as use.
  anything  = the relay that never reached TCP_CONN_CONNECTED, killed by
  else        `do_tcp_conns` rather than reconnected. This is the door `S1-AA`
              went looking for; in the first run to carry the instrument it did
              not swing once.

Usage:
    python tools/fold-relay-kills.py                    # every run on disk
    python tools/fold-relay-kills.py split144000-9      # one run
    python tools/fold-relay-kills.py a b                # compare two runs

A run taken before 0019 landed prints nothing, and that is an absence of the
instrument rather than an absence of kills -- the tool says so rather than
printing a zero.
"""
from __future__ import print_function
import io, os, re, sys
from collections import Counter

# Two shapes, because runs taken between 0019 and 0020 have no breakdown and
# are still worth folding. The `(...)` group is optional and absent means
# "this run's instrument could not answer that question", which is different
# from three zeroes -- the tool keeps them apart.
LINE = re.compile(
    r'killed and (\d+) connection\(s\) lose a slot'
    r'(?: \((\d+) online, (\d+) registered, (\d+) neither\))?\. '
    r'Relay (\d+), status (\d+), onion (\d+), lock_count (\d+), sleep_count (\d+)')
STAMP = re.compile(r'^\d\d:\d\d:\d\d\.\d\d\d\s+([0-9.]+)\s')
CONNECTED = 2


def fold(run):
    d = os.path.join('runs', run)
    kills = 0
    slots = 0
    widest = 0
    lost_online = 0
    lost_registered = 0
    lost_neither = 0
    no_breakdown = 0
    by_status = Counter()
    by_lock = Counter()
    per_node = Counter()
    first = None
    last = None
    for f in sorted(os.listdir(d)):
        if not f.endswith('.log'):
            continue
        for raw in io.open(os.path.join(d, f), encoding='utf-8', errors='replace'):
            m = LINE.search(raw)
            if not m:
                continue
            g = m.groups()
            n = int(g[0])
            if g[1] is None:
                online = registered = neither = None
            else:
                online, registered, neither = (int(x) for x in g[1:4])
            status = int(g[5])
            lock = int(g[7])
            if online is None:
                no_breakdown += 1
            else:
                lost_online += online
                lost_registered += registered
                lost_neither += neither
            kills += 1
            slots += n
            widest = max(widest, n)
            by_status[status] += 1
            by_lock[lock] += 1
            per_node[f[:-4]] += 1
            s = STAMP.match(raw)
            if s:
                t = float(s.group(1))
                first = t if first is None else min(first, t)
                last = t if last is None else max(last, t)
    return (kills, slots, widest, by_status, by_lock, per_node, first, last,
            lost_online, lost_registered, lost_neither, no_breakdown)


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
        (kills, slots, widest, by_status, by_lock, per_node, first, last,
         lost_online, lost_registered, lost_neither, no_breakdown) = fold(run)
        if kills == 0:
            silent.append(run)
            continue
        reaper = by_status.get(CONNECTED, 0)
        never = kills - reaper
        print('%s' % run)
        print('  %d kill(s), %d slot(s) lost, widest single kill %d, '
              'on %d node(s)' % (kills, slots, widest, len(per_node)))
        print('  %d by kill_nonused_tcp (status 2), %d by the never-connected '
              'branch' % (reaper, never))
        print('  lock_count at the kill: %s' % dict(by_lock))
        if no_breakdown == kills:
            print('  what the slots WERE: not recorded -- this run predates '
                  'patch 0020')
        else:
            print('  what the slots WERE: %d online, %d registered, %d neither'
                  % (lost_online, lost_registered, lost_neither))
            if no_breakdown:
                print('  (%d of the %d kills predate 0020 and are not in that '
                      'breakdown)' % (no_breakdown, kills))
        if first is not None:
            print('  between %.0f s and %.0f s into the run' % (first, last))
        print('  per node: %s' % dict(per_node))
        print()
    if silent:
        if len(silent) == len(want):
            print('no run carried the instrument.')
        print('%d run(s) printed nothing. A run taken before patch 0019 landed '
              'has no instrument, which is not the same as no kills.'
              % len(silent))
        if len(silent) <= 12:
            print('  ' + ', '.join(silent))


if __name__ == '__main__':
    main()
