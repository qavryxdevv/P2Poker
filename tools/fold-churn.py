"""Read a run of tools/table-run-churn.ps1: what every node did, when, and what the table came to.

    python tools/fold-churn.py runs/churn123456-10

Every log line carries the seconds since the run's own start (both machines, one
clock), so the lines of all nodes merge into one timeline. The plan beside the
logs says what was done to whom; this says what the table did about it:

  * the timeline of the formation: arrivals, seats, the group, lines cut and
    back, kills, leaves, seats given back (and why), asks again, the set, the
    first hands;
  * the outcome: when the table was set and hand #1 opened, with which seats,
    measured from the end of the churn;
  * how long the founder waited before giving an unhealthy seat back, from the
    moment its fault began;
  * findings: a panic, a node that left or lost its table, a first hand dealt
    into a group that did not hold everybody, a hand #1 short of a full table.
"""

import json
import re
import sys
from pathlib import Path

LINE = re.compile(r"^\ufeff?(\d\d:\d\d:\d\d\.\d{3})\s+(-?[0-9.]+)\s\s(.*)$")

PATTERNS = [
    ("hosting", re.compile(r"^hosting ([0-9a-f]{8})")),
    ("seated", re.compile(r"^seat (\d+) at ([0-9a-f]{8})")),
    ("group", re.compile(r"^in the table's Tox group")),
    ("roster", re.compile(r"^(\d+) seated$")),
    ("set", re.compile(r"^the table is set: session ([0-9a-f]+)")),
    ("ready", re.compile(r"^this client hears every seat of the roster \(serial (\d+)\) and says it is ready")),
    ("goes on", re.compile(r"the table goes on at the table seat \d+ founded|this client founds the table's continuation|waiting for seat \d+ to found|did not found the table's continuation|founded this table's continuation|its seats go on without it")),
    ("left word", re.compile(r"left the table before the first hand: its own signed word|an old word that seat \d+ left")),
    ("open", re.compile(r"^hand #(\d+) opens at genesis ([0-9a-f]+) with seats \[([^\]]*)\]")),
    ("over", re.compile(r"^hand #(\d+) is over")),
    ("released", re.compile(r"^seat (\d+) (.+) and the seat is free again")),
    ("cut", re.compile(r"^fault-harness: the internet goes away")),
    ("back", re.compile(r"^fault-harness: the internet is back")),
    ("at set", re.compile(r"^fault-harness: .* as the table is set, as P2P_POKER_AT_SET asked|^fault-harness: leaving the table as it is set")),
    ("leave", re.compile(r"^fault-harness: leaving the table")),
    ("given back", re.compile(r"^the founder gave this seat away.*asks for a seat again")),
    ("asks again", re.compile(r"^asking for a seat at the table again \(S1-FY\)")),
    ("waits founder", re.compile(r"^(.*); the table cannot start without its founder")),
    ("founder heard", re.compile(r"^the founder can be heard again \(S1-FY\)")),
    ("refused", re.compile(r"^the founder says no: (.*)")),
    ("not answered", re.compile(r"^the founder did not answer(.*)")),
    ("forced deal", re.compile(r"stopped filling .*; dealing anyway")),
    ("certified", re.compile(r"^seat (\d+) is certified out|certified out of hand #(\d+)")),
    ("panic", re.compile(r"panicked at")),
    ("gone", re.compile(r"the table is gone|back to the lobby$|join again from the lobby$")),
]

QUIET = {"over", "roster"}


def read_log(path):
    out = []
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = LINE.match(raw)
        if not m:
            continue
        try:
            t = float(m.group(2))
        except ValueError:
            continue
        text = m.group(3).strip()
        for kind, pat in PATTERNS:
            mm = pat.search(text)
            if mm:
                out.append((t, kind, mm, text))
                break
    return out


def main():
    run = Path(sys.argv[1])
    plan = json.loads((run / "plan.json").read_text(encoding="utf-8-sig"))
    seats = plan["seats"]
    churn_until = plan["churn_until"]
    lives = plan["lives"]
    if isinstance(lives, dict):
        lives = [lives]
    far_raw = plan.get("far") or []
    far = set(far_raw if isinstance(far_raw, list) else [far_raw])

    events = []  # (t, node, life, kind, match, text)
    for life in lives:
        path = run / f"n{life['Node']}-{life['Life']}.log"
        if not path.exists():
            events.append((life["Start"], life["Node"], life["Life"], "no log", None, "no log was collected"))
            continue
        for t, kind, m, text in read_log(path):
            events.append((t, life["Node"], life["Life"], kind, m, text))
    events.sort(key=lambda e: e[0])

    print(f"run    {plan['table']}  seats {seats}  seed {plan['seed']}  faults before {churn_until} s")
    roles = plan.get("roles") or {}
    for life in sorted(lives, key=lambda l: (l["Node"], l["Life"])):
        where = "far " if life["Node"] in far else "here"
        what = []
        if life["End"] == "kill":
            what.append(f"leaves at {life['Start'] + life['LeaveAt']} s" if life["LeaveAt"] else f"killed at {life['Stop']} s")
        if life["OfflineFor"]:
            s = life["Start"] + life["OfflineAt"]
            what.append(f"line cut {s}-{s + life['OfflineFor']} s")
        role = roles.get(str(life["Node"]), "founder" if life["Node"] == 0 else "calm")
        print(f"  n{life['Node']}.{life['Life']} {where} {role:<7} from {life['Start']:>4} s  {', '.join(what)}")

    print("\n--- timeline ---")
    for t, node, life, kind, m, text in events:
        if kind in QUIET:
            continue
        if kind == "open" and int(m.group(1)) > 3:
            continue
        print(f"{t:7.1f}  n{node}.{life}  {kind:<13} {text[:150]}")

    founder = [e for e in events if e[1] == 0]
    sets = [e for e in founder if e[3] == "set"]
    opens = [e for e in founder if e[3] == "open"]
    overs = [e for e in founder if e[3] == "over"]
    print("\n--- outcome ---")
    # The table the nodes ended at: the session most of them last set. With a
    # continuation (D-061) it is not the founder's first table.
    last_set_of = {}
    for t, node, life, kind, m, text in events:
        if kind == "set":
            last_set_of[node] = (m.group(1), t)
    tally = {}
    for node, (session, _) in last_set_of.items():
        tally.setdefault(session, []).append(node)
    if tally:
        session, nodes = max(tally.items(), key=lambda kv: len(kv[1]))
        first = min(t for t, node, life, kind, m, text in events if kind == "set" and m.group(1) == session)
        print(f"the table ended at session {session}: {len(nodes)} of {len({l['Node'] for l in lives})} node(s) at it"
              f" ({', '.join('n%d' % n for n in sorted(nodes))}), first set at {first:.1f} s ({first - churn_until:+.1f} s from the end of the churn)")
        opens_after = [e for e in events if e[3] == "open" and e[4].group(1) == "1" and e[0] >= first - 1]
        if opens_after:
            g = {}
            for e in opens_after:
                g.setdefault(e[4].group(2), []).append(e)
            genesis, those = max(g.items(), key=lambda kv: len(kv[1]))
            t0 = min(e[0] for e in those)
            dealt = [x.strip() for x in those[0][4].group(3).split(",") if x.strip()]
            print(f"its hand #1 opened at {t0:.1f} s with {len(dealt)} of {seats} seats: [{those[0][4].group(3)}]")
    if sets:
        first_set = sets[0][0]
        last_set = sets[-1][0]
        print(f"set at the founder: {len(sets)} time(s), first {first_set:.1f} s, last {last_set:.1f} s"
              f" ({last_set - churn_until:+.1f} s from the end of the churn)")
    else:
        print("the table was NEVER set at the founder")
    if opens:
        t, _, _, _, m, _ = opens[0]
        dealt = [s.strip() for s in m.group(3).split(",") if s.strip()]
        print(f"hand #1 opened at {t:.1f} s ({t - churn_until:+.1f} s from the end of the churn) with {len(dealt)} of {seats} seats: [{m.group(3)}]")
        print(f"hands the founder opened: {len(opens)}, saw over: {len(overs)}")
    else:
        print("NO hand was opened by the founder")

    # Per node, its last life: dealt in and over.
    print("\n--- per node (last life) ---")
    for node in sorted({l["Node"] for l in lives}):
        last = max(l["Life"] for l in lives if l["Node"] == node)
        mine = [e for e in events if e[1] == node and e[2] == last]
        # The seat at the last table this life sat at: a continuation's founder
        # (D-061) is seat 0 of the table it hosts.
        seat = "0" if node == 0 else None
        for e in mine:
            if e[3] == "seated":
                seat = e[4].group(1)
            elif e[3] == "hosting":
                seat = "0"
        dealt = 0
        hands = set()
        for e in mine:
            if e[3] == "open" and seat is not None and seat in [s.strip() for s in e[4].group(3).split(",")]:
                dealt += 1
                hands.add(e[4].group(1))
        over = sum(1 for e in mine if e[3] == "over" and e[4].group(1) in hands)
        group = next((e[0] for e in reversed(mine) if e[3] == "group"), None)
        print(f"  n{node}.{last} seat {seat if seat is not None else '-':>2}  group {('%.1f s' % group) if group is not None else 'never':>8}  dealt {dealt:>3}  over {over:>3}")

    # How long the founder waited for an unhealthy seat, from its fault's start.
    print("\n--- seats given back ---")
    seat_of = {}
    for t, node, life, kind, m, text in events:
        if kind == "seated":
            seat_of.setdefault(node, []).append((t, m.group(1)))
    faults = {}
    for life in lives:
        n = life["Node"]
        if life["OfflineFor"]:
            faults.setdefault(n, []).append((life["Start"] + life["OfflineAt"], "line cut"))
        if life["End"] == "kill":
            if life["LeaveAt"]:
                faults.setdefault(n, []).append((life["Start"] + life["LeaveAt"], "left"))
            else:
                faults.setdefault(n, []).append((life["Stop"], "killed"))
    for t, node, life, kind, m, text in events:
        if kind == "at set":
            faults.setdefault(node, []).append((t, "met its fault as the table was set"))
    for t, node, life, kind, m, text in founder:
        if kind != "released":
            continue
        seat = m.group(1)
        who = None
        for n, seated in seat_of.items():
            held = [s for (ts, s) in seated if ts <= t]
            if held and held[-1] == seat:
                who = n
        since = ""
        if who is not None and who in faults:
            before = sorted(f for f in faults[who] if f[0] <= t)
            if before:
                f = before[-1]
                since = f"  -- {t - f[0]:.1f} s after it {f[1]} at {f[0]} s"
        print(f"{t:7.1f}  seat {seat} (n{who if who is not None else '?'}) {m.group(2)}{since}")

    print("\n--- findings ---")
    found = False
    for t, node, life, kind, m, text in events:
        if kind in ("panic", "gone", "forced deal", "no log"):
            found = True
            print(f"{t:7.1f}  n{node}.{life}  {kind}: {text[:200]}")
    if opens:
        dealt = [s.strip() for s in opens[0][4].group(3).split(",") if s.strip()]
        if len(dealt) < seats:
            found = True
            print(f"hand #1 was dealt to {len(dealt)} of {seats} seats")
    if not sets:
        found = True
        print("no set: the table never started")
    # One table: every node's last session is the founder's, and every hand
    # opened at one genesis.
    last_life = {}
    for life in lives:
        last_life[life["Node"]] = max(last_life.get(life["Node"], 0), life["Life"])
    final = {}
    for t, node, life, kind, m, text in events:
        if kind == "set" and life == last_life[node]:
            final[node] = m.group(1)
    if 0 in final:
        for node, session in sorted(final.items()):
            if session != final[0]:
                found = True
                print(f"SPLIT: n{node} is at session {session}, the founder at {final[0]}")
    # Hands by genesis, per session: a continuation's hand #1 is another table's.
    geneses = {}
    session_of = {}
    for t, node, life, kind, m, text in events:
        if kind == "set":
            session_of[(node, life)] = m.group(1)
        if kind == "open":
            key = (session_of.get((node, life), "?"), m.group(1))
            geneses.setdefault(key, {}).setdefault(m.group(2), set()).add(f"n{node}.{life}")
    for (session, hand), g in sorted(geneses.items(), key=lambda x: (x[0][0], int(x[0][1]))):
        if len(g) > 1:
            found = True
            ranked = sorted(g.items(), key=lambda kv: -len(kv[1]))
            others = "; ".join(f"{gen} at {', '.join(sorted(n))}" for gen, n in ranked[1:])
            print(f"hand #{hand} of session {session} at more than one genesis: {ranked[0][0]} at {len(ranked[0][1])} node(s), and {others} -- a seat adrift, or a split")
    if not found:
        print("none")


if __name__ == "__main__":
    main()
