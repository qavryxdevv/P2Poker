# -*- coding: utf-8 -*-
"""Read one -Deaf run against the prediction booked for it (S1-CM, S1-CO).

A -Deaf window (patch 0016) drops every group packet at one node before the
ring sees it, so the senders keep re-sending and the node receives what it
missed when the window ends -- the brief-outage model, under the 58 s ring
timeout. What decides the run is not whether the frames come back but HOW
LONG AFTER the window they come, against the table's 30 s clock on the seat.

Prints, for the deaf node:
  * the per-node table from run.txt (opened / finished per node);
  * the join anchor the window is measured from, and the window itself;
  * the node's own timeline around the window, lobby noise and toxcore removed;
  * the first hand-level line after the window -- the repair latency;
  * every certificate and abort in the run, with the node that logged it;
  * each sender's answers to the deaf node's requests ("Re-sent requested
    packet N") from the window's end on, so a drain of one id per second is
    visible as one, and a burst as a burst.

Usage:
    python tools/read-deaf-run.py split164858-9            # node n3 (the harness default)
    python tools/read-deaf-run.py split164858-9 n5 250 20  # node, -DeafAt, -DeafFor

The harness numbers NODES and the table numbers SEATS: the deaf node's seat
is whichever number is missing from its own "seats on the line" heartbeat,
and the certificate names the seat. Both are printed so they are not confused.
"""
from __future__ import print_function
import io, os, re, sys

STAMP = re.compile(r'^(\d\d:\d\d:\d\d\.\d\d\d)\s+([0-9.]+)\s+(.*)$')
NOISE = re.compile(r'joined|found 12D3|lobby|player\(s\)|left the network|another poker client|^table |advert|stays relayed|toxcore\[|NotNewer|is waiting for|autoplay|your turn|prover ctx')


def lines(path):
    for raw in io.open(path, encoding='utf-8', errors='replace'):
        m = STAMP.match(raw.rstrip('\n'))
        if m:
            yield m.group(1), float(m.group(2)), m.group(3)


def wall_seconds(wall):
    h, m, s = wall.split(':')
    return int(h) * 3600 + int(m) * 60 + float(s)


def main():
    run = sys.argv[1]
    node = sys.argv[2] if len(sys.argv) > 2 else 'n3'
    deaf_at = float(sys.argv[3]) if len(sys.argv) > 3 else None
    deaf_for = float(sys.argv[4]) if len(sys.argv) > 4 else None
    d = os.path.join('runs', run)
    if not os.path.isdir(d):
        print('%s: no such run' % run)
        return

    print('=== run.txt')
    for l in io.open(os.path.join(d, 'run.txt'), encoding='utf-8', errors='replace'):
        if re.match(r'^(node|here-n\d|far-n\d|--- per node|deaf|link)', l):
            print('  ' + l.rstrip()[:150])
            m = re.search(r'from (\d+) s for (\d+) s', l)
            if l.startswith('deaf') and m and deaf_at is None:
                deaf_at, deaf_for = float(m.group(1)), float(m.group(2))
    if deaf_at is None:
        print('  no deaf window in run.txt and none given; pass -DeafAt and -DeafFor as arguments')
        return

    own = list(lines(os.path.join(d, node + '.log')))
    anchor = next((s for _, s, m in own if 'Tox group' in m and 'rides it' in m), None)
    if anchor is None:
        print('  %s never joined the group; nothing to read' % node)
        return
    w0, w1 = anchor + deaf_at, anchor + deaf_at + deaf_for
    seat = None
    for _, s, m in own:
        if 'seats on the line:' in m:
            roll = m.split('seats on the line:')[1].split(';')[0].strip()
            present = set(int(x) for x in re.findall(r'(?:^|, )(\d+) (?:\d+ms|silent|never)', roll))
            missing = sorted(set(range(len(present) + 1)) - present)
            if len(missing) == 1:
                seat = missing[0]
                break
    print('\n=== %s joined the group at %.1f s; the window is about %.1f..%.1f s (anchor + %g, %g s); it holds seat %s'
          % (node, anchor, w0, w1, deaf_at, deaf_for, seat if seat is not None else '?'))
    print('    The C measures the window from the first group packet it handled, which precedes the join line above by')
    print('    up to several seconds (4 s in split120908-9); take the true edges from the last frame before the gap and')
    print("    the senders' burst offsets below, not from this arithmetic.")

    lo, hi = w0 - 25, w1 + 40
    print('\n=== %s between %.0f and %.0f s (noise removed)' % (node, lo, hi))
    first_after = None
    for wall, s, msg in own:
        if lo <= s <= hi and not NOISE.search(msg):
            mark = '  <-- window opens' if abs(s - w0) < 0.05 else ''
            print('  %7.1f  %s%s' % (s, msg[:140], mark))
            if s > w1 and first_after is None and re.search(r'^hand #\d+', msg):
                first_after = (wall, s, msg)
    if first_after:
        print('\n=== first hand-level line after the window: %.1f s, %.1f s after it closed -- "%s"'
              % (first_after[1], first_after[1] - w1, first_after[2][:90]))
        w1_wall = wall_seconds(first_after[0]) - (first_after[1] - w1)
    else:
        print('\n=== no hand-level line after the window at all')
        w1_wall = None

    print('\n=== certificates and aborts, every log')
    logs = sorted(f for f in os.listdir(d) if f.endswith('.log'))
    for f in logs:
        for wall, s, msg in lines(os.path.join(d, f)):
            if re.search(r'has certified|every stack is\s+restored|ran out of time|this client is out', msg):
                print('  %-10s %7.1f  %s' % (f, s, msg[:120]))

    if w1_wall is not None:
        print("\n=== each sender's answers to %s from the window's end on (wall seconds after it), first twelve" % node)
        for f in logs:
            if f == node + '.log':
                continue
            answers = []
            for wall, s, msg in lines(os.path.join(d, f)):
                m = re.search(r'Re-sent requested packet (\d+)', msg)
                if m:
                    dt = wall_seconds(wall) - w1_wall
                    if -0.5 <= dt <= 20:
                        answers.append((dt, int(m.group(1))))
            if answers:
                print('  %-10s %s' % (f, '  '.join('%+.1f:%d' % a for a in answers[:12])))
        print('  A run of consecutive ids at the same offset is a burst; ids one apart at offsets one or more apart is the one-per-second drain.')

        # The uplink half (patch 0025, -DeafBothWays): what the deaf node itself
        # re-sent on request once its line was back is the mirror image -- the
        # peers drawing its stuck backlog. The line is the deaf node's own.
        mute = [s for _, s, m in own if 'the wire is mute' in m]
        answers = []
        for wall, s, msg in own:
            m = re.search(r'Re-sent requested packet (\d+)', msg)
            if m:
                dt = wall_seconds(wall) - w1_wall
                if -0.5 <= dt <= 25:
                    answers.append((dt, int(m.group(1))))
        if mute:
            print("\n=== the wire was mute too (patch 0025, first line at %.1f s); %s's own answers to its peers' requests from the window's end on, first twenty" % (mute[0], node))
            if answers:
                print('  ' + '  '.join('%+.1f:%d' % a for a in answers[:20]))
                print('  Ids one apart at offsets two seconds apart is the peers drawing the backlog one probe at a time; a run at one offset is a burst they could size.')
            else:
                print('  none -- either nothing was stuck in the uplink or the backlog left by the blind schedule alone')
        else:
            print('\n=== the wire was not mute in this run (downlink half only); %s answered %d request(s) in the 25 s after the window' % (node, len(answers)))

    # The relay layer (patch 0026, S1-CQ): what every node's relays did around the
    # window, and who lost whom. A "goes to sleep" line whose out-of-band count is
    # above zero is a relay-only peer's path being taken; "no relay carried" in
    # the minute after is the loss; "deleting group peer ... exit type 1" is the
    # 58 s timeout that follows.
    print('\n=== the relay layer around the window (patch 0026 lines; absent in runs before it)')
    lo, hi = w0 - 5, w1 + 40
    for f in logs:
        n = f[:-4]
        sleeps, wakes, to_sleep, to_wake, failed, timeouts = [], [], 0, 0, 0, []
        for wall, s, msg in lines(os.path.join(d, f)):
            if 'goes to sleep' in msg and lo <= s <= hi:
                m = re.search(r'relay (\d+) goes to sleep \(lock_count (\d+) == sleep_count (\d+)\): (\d+) online slot\(s\), (\d+) registered, (\d+) of them', msg)
                if m:
                    sleeps.append('%.1f:relay %s lock %s online %s reg %s oob-awake %s' % (s, m.group(1), m.group(2), m.group(4), m.group(5), m.group(6)))
            elif 'wakes up' in msg and lo <= s <= hi:
                m = re.search(r'relay (\d+) wakes up', msg)
                if m:
                    wakes.append('%.1f:relay %s' % (s, m.group(1)))
            elif 'connection-to' in msg and 'sleeps (peer direct)' in msg and lo <= s <= hi:
                to_sleep += 1
            elif 'connection-to' in msg and 'wakes (peer not direct)' in msg and lo <= s <= hi:
                to_wake += 1
            elif 'no relay carried' in msg and w1 <= s <= w1 + 60:
                failed += 1
            elif 'deleting group peer' in msg and 'exit type 1' in msg and s > w0:
                m = re.search(r'deleting group peer (\d+)', msg)
                timeouts.append('%.1f:peer %s' % (s, m.group(1) if m else '?'))
        if not (sleeps or wakes or to_sleep or to_wake or failed or timeouts):
            continue
        print('  %-8s connection-to woke %d, slept %d; relay sleeps: %s; wakes: %s; failed sends in the minute after: %d; timeouts: %s'
              % (n, to_wake, to_sleep, '  '.join(sleeps) or 'none', '  '.join(wakes) or 'none', failed,
                 '  '.join(timeouts) or 'none'))
    print('  How to read it (S1-CQ): before patch 0027 a sleep with lock == sleep could sit on top of an AWAKE peer that was ONLINE')
    print('  (a phantom sleeper the wake path never returned) and every slot on that relay went to NONE -- the failed sends and the')
    print('  timeouts in the columns after it are that. With 0027 in the binary, lock == sleep means every ONLINE user is asleep,')
    print('  the relay may sleep, and the columns after it should read 0 and none.')


if __name__ == '__main__':
    main()
