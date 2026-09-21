#!/usr/bin/env python3
"""`S1-IT`: did every pair of seats of a run MEET over libp2p, how soon, and what did the dials that failed end in?

    python tools/read-first-meetings.py <run>                 (a directory under runs/, or a path)
    python tools/read-first-meetings.py <run> --pair n0 n5    and what those two said of each other

A table sets only when every seat has met its founder, and two seats behind routers meet through a relay's
circuit. `S1-IT` was a table of nine that never set because one seat and the founder never met in 420 s -- and the
run could not say why, because the client kept the full words of a failed dial for the first twelve of a run. The
lines this reads are the ones that row added, said for as long as the client runs:

  * `another poker client: <peer>`                       a meeting -- the matrix is built from these
  * `could not reach [the founder ]<peer>: ...`          a failed dial of a peer that matters, folded by relay and kind
  * `could not reach relay ..xxxxxx: ...`                a relay's own failed dial (a binary built to be measured only)
  * `the ask cannot reach the founder: ...`              an ask whose dial could not even start
  * `the way in through relay ..xxxxxx is gone (...)`    a reservation that went away, and why
  * `this client's own connection limit refused ...`     once a discovery tick (a binary built to be measured only)

**Incidents, not lines.** A dial through two relays that ends three ways is ONE failed dial and is counted once
under each kind it shows. A pair is met when EITHER seat says so; the time is the earlier of the two, on the
saying seat's own clock -- the seats start seconds apart, so times from two logs are never subtracted.

**An absent instrument is not a zero.** A log from before `S1-IT` holds no `could not reach` line at all: the
kinds then read *none said*, which says nothing about how its dials ended. The header's `dials` and `waysin` lines
(`tools/table-run-split.ps1`) say which client ran -- the one as built, or the control (`-OfferEveryAddress
-OneWayIn -CapForAll`).

The last line is the run's: `ALL MET` when every pair met and every seat but the founder sat down, `NOT ALL MET`
otherwise, with the pairs that never did. Exit code 0 for the first, 1 for the second.
"""
import os
import re
import sys
from collections import Counter, OrderedDict

LINE = re.compile(r"^\ufeff?(\d\d:\d\d:\d\d\.\d+)\s+(-?[\d.]+)\s\s(.*)$")
PEER = re.compile(r"\b(?:12D3KooW[1-9A-HJ-NP-Za-km-z]{44}|Qm[1-9A-HJ-NP-Za-km-z]{44})\b")
KIND = re.compile(r"(\d+) ([a-z][a-z' ]+?)(?:,|;|$)")


def lines_of(path):
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = LINE.match(raw.rstrip("\n"))
        if m:
            yield float(m.group(2)), m.group(3)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    pair = None
    if "--pair" in sys.argv:
        at = sys.argv.index("--pair")
        pair = tuple(sys.argv[at + 1: at + 3])
        args = [a for a in args if a not in pair]
    if len(args) != 1 or (pair is not None and len(pair) != 2):
        print(__doc__)
        return 2
    run = args[0]
    if not os.path.isdir(run):
        run = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "runs", args[0])
    logs = sorted(n for n in os.listdir(run) if n.endswith(".log"))
    if not logs:
        print("no logs under", run)
        return 2
    ids = {}
    for name in logs:
        for raw in open(os.path.join(run, name), encoding="utf-8", errors="replace"):
            m = re.search(r"peer id\s+(\S+)", raw)
            if m:
                ids[m.group(1)] = name[:-4]
                break
    header = []
    try:
        for raw in open(os.path.join(run, "run.txt"), encoding="utf-8", errors="replace"):
            if raw.startswith(("dials ", "waysin ")):
                header.append(raw.strip())
    except OSError:
        pass
    print(os.path.basename(os.path.normpath(run)))
    for h in header or ["(no `dials` line in run.txt: a harness from before S1-IT)"]:
        print("   " + h)

    met, kinds, relay_kinds, unsent, gone = {}, Counter(), Counter(), Counter(), Counter()
    unreached = asks = refused = came_in = came_in_relay = 0
    seated, reservations = {}, Counter()
    for name in logs:
        seat = name[:-4]
        for t, text in lines_of(os.path.join(run, name)):
            if text.startswith("another poker client: "):
                p = text.split(": ", 1)[1].strip()
                if p in ids:
                    met.setdefault(seat, {}).setdefault(ids[p], t)
            elif text.startswith("could not reach relay "):
                body = text.split(": ", 1)[1] if ": " in text else text
                for k in set(k for _, k in KIND.findall(body)) or {body[:40]}:
                    relay_kinds[k] += 1
            elif text.startswith("could not reach "):
                unreached += 1
                body = text.split(": ", 1)[1] if ": " in text else text
                for k in set(k for _, k in KIND.findall(body)) or {body[:40]}:
                    kinds[k] += 1
            elif text.startswith("the ask cannot reach the founder: "):
                unsent[text.split(": ", 1)[1][:60]] += 1
            elif text.startswith("asking to join"):
                asks += 1
            elif text.startswith("the way in through relay "):
                gone["its connection closed" if "its connection closed" in text else "the relay ended it"] += 1
            elif text.startswith("relay ") and " bytes / " in text:
                reservations[seat] += 1
            elif text.startswith("this client's own connection limit refused "):
                refused += int(re.search(r"refused (\d+)", text).group(1))
                m = re.search(r"and (\d+) that came in, (\d+) of them through its relay", text)
                if m:
                    came_in += int(m.group(1))
                    came_in_relay += int(m.group(2))
            elif re.match(r"^seat \d+ at ", text):
                seated.setdefault(seat, t)

    names = [n[:-4] for n in logs]
    founder = next((n for n in names if n not in seated and not n.startswith("far")), names[0])
    first = {}
    for i, a in enumerate(names):
        for b in names[i + 1:]:
            ts = [x for x in (met.get(a, {}).get(b), met.get(b, {}).get(a)) if x is not None]
            first[(a, b)] = min(ts) if ts else None
    never = [p for p, t in first.items() if t is None]
    times = sorted(t for t in first.values() if t is not None)
    if times:
        print("pairs %d; met %d: median %.0f s, nine in ten by %.0f s, the last at %.0f s; within 30 s %d" % (
            len(first), len(times), times[len(times) // 2], times[max(0, int(len(times) * 0.9) - 1)], times[-1],
            sum(1 for t in times if t <= 30)))
    col = sorted(first.get((min(founder, o), max(founder, o)), None) or -1 for o in names if o != founder)
    print("the founder (%s) was first met at: %s" % (founder, " ".join("-" if t < 0 else "%.0f" % t for t in col)))
    print("seated %d of %d: %s" % (len(seated), len(names) - 1,
                                    " ".join("%s@%.0f" % kv for kv in sorted(seated.items(), key=lambda kv: kv[1]))))
    print("reservations accepted, renewals included: %s; ways in that went away: %s" % (
        " ".join("%s:%d" % kv for kv in sorted(reservations.items())) or "none said", dict(gone) or "none"))
    print("failed dials of peers that matter: %d; by kind of ending:%s" % (unreached, "" if kinds else " none said"))
    for k, n in kinds.most_common(8):
        print("   %5d  %s" % (n, k))
    print("the relays' own failed dials, by kind:%s" % ("" if relay_kinds else " none said"))
    for k, n in relay_kinds.most_common(8):
        print("   %5d  %s" % (n, k))
    print("asks %d; asks whose dial could not start: %s" % (asks, dict(unsent) or "none said"))
    print("refused by the clients' own connection limit: %d going out, %d coming in (%d of those through the client's own relay)" % (
        refused, came_in, came_in_relay))

    if pair:
        about(run, ids, *pair)

    whole = not never and len(seated) == len(names) - 1
    print("%s: %d of %d pairs met%s%s" % (
        "ALL MET" if whole else "NOT ALL MET", len(times), len(first),
        "" if not never else "; never: " + " ".join("%s-%s" % p for p in never[:10]),
        "" if len(seated) == len(names) - 1 else "; %d of %d seats sat down" % (len(seated), len(names) - 1)))
    return 0 if whole else 1


def about(run, ids, a, b):
    """What two seats said of each other, folded by the shape of the line, with the times."""
    seat_id = {n: p for p, n in ids.items()}
    for me, other in ((a, b), (b, a)):
        if me not in seat_id or other not in seat_id:
            print("no such seat in this run: %s or %s" % (me, other))
            return
        listens = OrderedDict()
        shapes = OrderedDict()
        for t, text in lines_of(os.path.join(run, me + ".log")):
            if text.startswith("listening on") and "p2p-circuit" in text:
                m = re.search(r"/p2p/([^/\s]+)/p2p-circuit", text)
                if m:
                    listens.setdefault(m.group(1)[-6:], t)
            if seat_id[other] not in text:
                continue
            text = text.replace(seat_id[other], "<%s>" % other)
            text = PEER.sub("<peer>", text)
            if text.startswith(("could not reach", "found ", "another poker client", "a second look")):
                shapes.setdefault(re.sub(r"\b\d+\b", "#", text), []).append(t)
        print("== %s listens through %s; about %s it says:" % (
            me, ", ".join("..%s from %.0f s" % kv for kv in listens.items()) or "no relay", other))
        for shape, ts in shapes.items():
            print("   x%-3d at %s%s" % (len(ts), " ".join("%.0f" % t for t in ts[:12]), " ..." if len(ts) > 12 else ""))
            print("        %s" % shape[:300])


if __name__ == "__main__":
    sys.exit(main())
