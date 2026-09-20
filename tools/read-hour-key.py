#!/usr/bin/env python3
"""D-070: what the lobby's hour key did in one run, seat by seat.

    python tools/read-hour-key.py <run>          (a directory under runs/, or a path)

For every seat's log: when the walk of the hour's record finished and when a node
returned the record, the sizes of the answers about an hour's key beside those about
the lobby's own key, when an hour's answer first named somebody who is not this client,
when the seat met its first poker client at all -- and, of the poker clients it met,
how many it met ON A CONNECTION AN HOUR'S ANSWER DIALLED and how many the hour's key
had only named while the meeting came another way.

**Named is not reached.** The first run read with this tool (`split174430-9`) had every
seat named all eight others by the hour's key, and the far seat had met three of them
and asked to join before any hour's answer named anybody. A client's own store answers
a lookup first and names the client itself, so an answer of one at three seconds was
the seat reading its own record. Distinct peers are counted, never lines: a peer met
again is said again.

The last line is the run's. `REACHED` when every seat announced under the hour's key
and at least one seat met a poker client on a connection an hour's answer dialled;
`NAMED ONLY` when the key stored and returned records and made no meeting; `NAMED, DIALS
NOT TOLD APART` for a log from before the dials were kept apart, where no meeting made
by the key could have been said -- an absent instrument, not a zero; `NOT SHOWN`
otherwise. A log with no D-070 line at all is a client built before D-070, and is said
as that rather than counted as a seat the key failed.

**Which build wrote the log (`S1-IV`).** Only a binary built to be measured -- the
`fault-harness` feature, which is what the bed runs -- says every ANSWER of a lookup, and
the sizes of the answers and the moment one first named somebody else are read from
those lines. A player's build says one line a LOOKUP, when it ends: who all its answers
named. A log that holds only those is read as that -- its lookups are counted and the
first that named somebody else is timed at its END -- and never as a key that returned
nothing: an absent instrument, not a zero.
"""
import pathlib
import re
import statistics
import sys

LINE = re.compile(r"^\ufeff?(\d\d:\d\d:\d\d\.\d{3})\s+(-?[0-9.]+)\s\s(.*)$")
HOUR_ANSWER = re.compile(
    r"^(\d+) player\(s\) in the public lobby within the hour(?:, (\d+) of them not this client)? \(D-070\)")
LOBBY_ANSWER = re.compile(r"^(\d+) player\(s\) in the public lobby$")
# `S1-IV`: the line of a whole lookup, which every build says when the lookup ends.
HOUR_LOOKUP = re.compile(
    r"^(?:at least )?(\d+) player\(s\) in the public lobby within the hour, (\d+) of them not this client \(D-070; ")
LOBBY_LOOKUP = re.compile(r"^(?:at least )?(\d+) player\(s\) in the public lobby, (\d+) of them not this client \(")
REACHED = re.compile(r"^(\S+) is a poker client, reached by a dial an answer in the public lobby within the hour issued")
NAMED = re.compile(r"^(\S+) is a poker client, named in the public lobby within the hour")
MET = re.compile(r"^another poker client: (\S+)")
ANNOUNCED = "announced in the public lobby for this hour (D-070)"
RETURNED = "this client's own record for this hour came back from the network (D-070)"


def read(path):
    out = []
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = LINE.match(raw)
        if m:
            try:
                out.append((float(m.group(2)), m.group(3).strip()))
            except ValueError:
                pass
    return out


def summary(sizes):
    if not sizes:
        return "none"
    return "%d answers, %d of them empty, median of the rest %s, max %d" % (
        len(sizes), sum(1 for s in sizes if s == 0),
        statistics.median([s for s in sizes if s] or [0]), max(sizes))


def sec(v):
    return "-" if v is None else "%.1f s" % v


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    run = pathlib.Path(sys.argv[1])
    if not run.is_dir():
        run = pathlib.Path(__file__).resolve().parent.parent / "runs" / sys.argv[1]
    logs = sorted(run.glob("*.log"))
    if not logs:
        print("no logs under", run)
        return 2

    seats = announced = seats_reached = seats_named = blind = 0
    for log in logs:
        lines = read(log)
        hour, lobby = [], []
        reached, named, met = {}, {}, {}
        walked = returned = first_other = asked = None
        counts_others = True
        lookups, first_other_lookup = [], None
        lobby_lookups = []
        for t, text in lines:
            m = HOUR_ANSWER.match(text)
            if m:
                hour.append(int(m.group(1)))
                if m.group(2) is None:
                    counts_others = False
                elif first_other is None and int(m.group(2)) > 0:
                    first_other = t
                continue
            m = HOUR_LOOKUP.match(text)
            if m:
                lookups.append((int(m.group(1)), int(m.group(2))))
                if first_other_lookup is None and int(m.group(2)) > 0:
                    first_other_lookup = t
                continue
            m = LOBBY_LOOKUP.match(text)
            if m:
                lobby_lookups.append((int(m.group(1)), int(m.group(2))))
                continue
            m = LOBBY_ANSWER.match(text)
            if m:
                lobby.append(int(m.group(1)))
                continue
            m = REACHED.match(text)
            if m:
                reached.setdefault(m.group(1), t)
                continue
            m = NAMED.match(text)
            if m:
                named.setdefault(m.group(1), t)
                continue
            m = MET.match(text)
            if m:
                met.setdefault(m.group(1), t)
                continue
            if text == ANNOUNCED and walked is None:
                walked = t
            elif RETURNED in text and returned is None:
                returned = t
            elif text.startswith("asking to join") and asked is None:
                asked = t
        if not hour and not lookups and walked is None and not named and not reached:
            print("%-10s no D-070 line: a client built before it, or a log that is not a seat's" % log.stem)
            continue
        seats += 1
        announced += walked is not None
        only_named = {p: t for p, t in named.items() if p not in reached}
        seats_reached += bool(reached)
        seats_named += bool(reached or only_named)
        first_met = min(met.values()) if met else None
        print("%-10s the hour's walk finished %s, a node returned the record %s" % (log.stem, sec(walked), sec(returned)))
        print("%-10s   first poker client met %s; asked to join %s; an hour's answer first named somebody else %s" % (
            "", sec(first_met), sec(asked),
            sec(first_other) if counts_others else "? (a log from before the line counted others)"))
        if hour or not lookups:
            print("%-10s   the hour's key: %s" % ("", summary(hour)))
        if lookups:
            print("%-10s   the hour's lookups: %d ended, the most one named %d (%d of them not this client)%s" % (
                "", len(lookups), max(n for n, _ in lookups), max(o for _, o in lookups),
                "" if hour else "; A PLAYER'S BUILD -- it says no answer by itself (S1-IV), so the answers' sizes are"
                " not in this log, and the first LOOKUP that named somebody else ended %s" % sec(first_other_lookup)))
        if lobby or not lobby_lookups:
            print("%-10s   the lobby's own: %s" % ("", summary(lobby)))
        if lobby_lookups:
            print("%-10s   the lobby's own lookups: %d ended, the most one named %d (%d of them not this client)" % (
                "", len(lobby_lookups), max(n for n, _ in lobby_lookups), max(o for _, o in lobby_lookups)))
        if counts_others:
            print("%-10s   poker clients met: %d; on a connection an hour's answer dialled: %d%s; named by the hour's key and met another way: %d" % (
                "", len(met), len(reached),
                "" if not reached else " (the first at %.1f s)" % min(reached.values()),
                len(only_named)))
        else:
            # The line that counts others and the book of hour-issued dials came in
            # one change, so a log without the first cannot hold the second: an
            # absent instrument, which is not a measured zero.
            blind += 1
            print("%-10s   poker clients met: %d; named by the hour's key: %d; WHICH DIAL REACHED THEM IS NOT IN THIS LOG (a build from before dials were told apart)" % (
                "", len(met), len(only_named)))

    if blind:
        word = "NAMED, DIALS NOT TOLD APART" if seats_named else "NOT SHOWN"
    elif seats and announced == seats and seats_reached:
        word = "REACHED"
    elif seats and announced == seats and seats_named:
        word = "NAMED ONLY"
    else:
        word = "NOT SHOWN"
    if blind:
        print("%s: %d seat(s), %d announced under the hour's key, %d were named a poker client by it; %d log(s) cannot say which dial reached anybody" % (
            word, seats, announced, seats_named, blind))
    else:
        print("%s: %d seat(s), %d announced under the hour's key, %d met a poker client on a connection an hour's answer dialled, %d were named one" % (
            word, seats, announced, seats_reached, seats_named))
    return 0 if word == "REACHED" else 1


if __name__ == "__main__":
    sys.exit(main())
