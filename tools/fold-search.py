"""Read a run of tools/table-run-search.ps1: what every searching client did, and what it came to.

    python tools/fold-search.py runs/search123456-6

Every log line carries the seconds since the run's own start (both machines, one
clock), so the lines of all nodes merge into one timeline. The plan beside the
logs says what was done to whom; this says what the search did about it:

  * the timeline: searches started, seats asked for and reserved, tables
    founded, seats armed and released, games started, cancels, lines cut and
    back, kills, searches ended;
  * per node: how long the first game took from the search's start, how many
    games it played, how many hands, how the search ended;
  * the games: each session, its seats, when it set;
  * findings: a panic, a seat reserved after the search ended (a leak), a
    search that never ended and never found a game, a cancel that left a seat
    behind, more games running at once than asked for, a founded table that
    never started.
"""

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

LINE = re.compile(r"^\ufeff?(\d\d:\d\d:\d\d\.\d{3})\s+(-?[0-9.]+)\s\s(.*)$")

PATTERNS = [
    ("started", re.compile(r"^search: started \((.*)\)")),
    ("looking", re.compile(r"^search: looking for a (.*) game")),
    ("asking", re.compile(r"^search: asking for a seat at ([0-9a-f]{8}) \((\d+)/(\d+)")),
    ("reserved", re.compile(r"^search: reserved at ([0-9a-f]{8}) \((\d+)/(\d+), slot (\d+)\)")),
    ("founding", re.compile(r"^search: nothing to sit at after (\d+) s; founding (.*)")),
    ("founded", re.compile(r"^search: founded ([0-9a-f]{8}) \((\d+) seats, slot (\d+)\)")),
    ("armed", re.compile(r"^search: armed ([0-9a-f]{8}) \((\d+)/(\d+) at capacity (\d+)\)")),
    ("disarmed", re.compile(r"^search: disarmed ([0-9a-f]{8})")),
    ("released", re.compile(r"^search: released ([0-9a-f]{8}): (.*)")),
    ("gone", re.compile(r"^search: the seat at ([0-9a-f]{8}) is gone|^search: no seat at (.*) in (\d+) s")),
    ("game", re.compile(r"^search: a game started at ([0-9a-f]{8}) \(slot (\d+), (\d+) seats\)")),
    ("ended", re.compile(r"^search: ended -- (.*) after (\d+) s; (\d+) reservation\(s\) released")),
    ("cancelled", re.compile(r"^search: cancelled \((.*)\); (\d+) reservation\(s\) released after (\d+) s")),
    ("tick", re.compile(r"^search: (\d+) s, eta (.*), queue (\d+), reserved (\d+) of up to (\d+) \[(.*)\], playing (\d+)/(\d+)(.*)")),
    ("left", re.compile(r"^search: a game started elsewhere|^search: the search was given up|^search: its founder could not|^search: the table was lost|^search: more games started|^search: no game found|^search: the seat given back")),
    ("set", re.compile(r"^the table is set: session ([0-9a-f]+)")),
    ("open", re.compile(r"^hand #(\d+) opens at genesis ([0-9a-f]+) with seats \[([^\]]*)\]")),
    ("won", re.compile(r"^you won the tournament|^out of chips: finished")),
    ("cut", re.compile(r"^fault-harness: the internet goes away")),
    ("back", re.compile(r"^fault-harness: the internet is back")),
    ("cancel", re.compile(r"^fault-harness: cancelling the search")),
    ("again", re.compile(r"^fault-harness: searching again|^searching again, as --search-again asked")),
    ("formed", re.compile(r"^TABLE FORMED session=([0-9a-f]+) seats=(\d+)(?: slot=(\d+))?")),
    ("panic", re.compile(r"panicked at")),
    ("queue", re.compile(r"^(\d+) player\(s\) in the public lobby")),
]

QUIET = {"tick", "queue", "open", "formed"}


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
    lives = plan["lives"]
    if isinstance(lives, dict):
        lives = [lives]
    far_raw = plan.get("far")
    if far_raw is None:
        far_raw = []
    far = set(far_raw if isinstance(far_raw, list) else [far_raw])

    events = []  # (t, node, life, kind, match, text)
    missing = []
    for life in lives:
        path = run / f"n{life['Node']}-{life['Life']}.log"
        if not path.exists():
            missing.append(f"n{life['Node']}.{life['Life']}")
            continue
        for t, kind, m, text in read_log(path):
            events.append((t, life["Node"], life["Life"], kind, m, text))
    events.sort(key=lambda e: e[0])

    print(f"run    {plan['table']}  nodes {plan['nodes']}  seed {plan['seed']}  faults before {plan['churn_until']} s  formats {','.join(plan.get('formats') or [])}")
    roles = plan.get("roles") or {}
    for life in sorted(lives, key=lambda l: (l["Node"], l["Life"])):
        where = "far " if life["Node"] in far else "here"
        what = []
        if life["End"] == "kill":
            what.append(f"killed at {life['Stop']} s")
        if life.get("OfflineFor"):
            s = life["Start"] + life["OfflineAt"]
            what.append(f"line cut {s}-{s + life['OfflineFor']} s")
        if life.get("CancelAt"):
            what.append(f"cancels at {life['Start'] + life['CancelAt']} s, again at {life['Start'] + life['AgainAt']} s")
        role = roles.get(str(life["Node"]), "calm")
        print(f"  n{life['Node']}.{life['Life']} {where} {role:<6} {life.get('Format', '?'):<4} x{life.get('Tables', 1)} from {life['Start']:>4} s  {', '.join(what)}")
    if missing:
        print(f"  no log for: {', '.join(missing)}")

    print("\n--- timeline ---")
    for t, node, life, kind, m, text in events:
        if kind in QUIET:
            continue
        print(f"{t:7.1f}  n{node}.{life}  {kind:<10} {text[:140]}")

    # --- per node -----------------------------------------------------------
    print("\n--- per node ---")
    findings = []
    games_by_session = defaultdict(set)   # session -> {(node, life)}
    set_at = {}
    for t, node, life, kind, m, text in events:
        if kind == "set":
            games_by_session[m.group(1)[:8]].add((node, life))
            set_at.setdefault(m.group(1)[:8], t)
        if kind == "formed":
            games_by_session[m.group(1)[:8]].add((node, life))
    for life in sorted(lives, key=lambda l: (l["Node"], l["Life"])):
        mine = [e for e in events if e[1] == life["Node"] and e[2] == life["Life"]]
        starts = [e for e in mine if e[3] == "started"]
        games = [e for e in mine if e[3] == "game"]
        ends = [e for e in mine if e[3] in ("ended", "cancelled")]
        hands = {int(e[4].group(1)) for e in mine if e[3] == "open"}
        panics = [e for e in mine if e[3] == "panic"]
        reserved = [e for e in mine if e[3] == "reserved"]
        founded = [e for e in mine if e[3] == "founded"]
        first_game = None
        if starts and games:
            first_game = games[0][0] - starts[0][0]
        searches = []
        # Pair every start with what ended it.
        for i, s in enumerate(starts):
            later = [e for e in mine if e[0] >= s[0] and e[3] in ("ended", "cancelled", "game")]
            outcome = next((e for e in later if e[3] in ("ended", "cancelled")), None)
            first = next((e for e in later if e[3] == "game"), None)
            if outcome is None:
                searches.append(f"#{i + 1} at {s[0]:.0f} s: never ended")
            elif outcome[3] == "cancelled":
                searches.append(f"#{i + 1} at {s[0]:.0f} s: cancelled at {outcome[0]:.0f} s ({outcome[4].group(2)} released)")
            else:
                took = outcome[0] - s[0]
                searches.append(f"#{i + 1} at {s[0]:.0f} s: {outcome[4].group(1)} in {took:.0f} s")
        print(
            f"  n{life['Node']}.{life['Life']}: {len(starts)} search(es), {len(reserved)} seat(s) reserved, "
            f"{len(founded)} founded, {len(games)} game(s), {len(hands)} hand(s)"
            + (f", first game {first_game:.0f} s after the search began" if first_game is not None else "")
        )
        for s in searches:
            print(f"      {s}")
        if panics:
            findings.append(f"n{life['Node']}.{life['Life']} panicked: {panics[0][5][:120]}")
        # A seat reserved after the last end, with no start after it: a leak.
        last_end = max((e[0] for e in ends), default=None)
        last_start = max((e[0] for e in starts), default=None)
        if last_end is not None and (last_start is None or last_start < last_end):
            after = [e for e in reserved if e[0] > last_end]
            if after:
                findings.append(f"n{life['Node']}.{life['Life']} reserved a seat {after[0][0]:.0f} s, after its search ended at {last_end:.0f} s (a leak)")
        # A search that never ended and never found a game, from a client that
        # lived long enough.
        if starts and not ends and not games and life["End"] != "kill":
            since = starts[-1][0]
            findings.append(f"n{life['Node']}.{life['Life']} searched from {since:.0f} s to the end and found no game")
        # More games running at once than asked for.
        ticks = [e for e in mine if e[3] == "tick"]
        for e in ticks:
            playing, limit = int(e[4].group(7)), int(e[4].group(8))
            if playing > limit:
                findings.append(f"n{life['Node']}.{life['Life']} played {playing} games with {limit} asked for at {e[0]:.0f} s")
                break
        # A founded table that never became a game while the search went on to its end.
        for f in founded:
            key = f[4].group(1)
            started = any(e[3] == "game" and e[4].group(1) == key for e in mine)
            released = any(e[3] == "released" and e[4].group(1) == key for e in mine)
            if not started and not released and not ends:
                findings.append(f"n{life['Node']}.{life['Life']} founded {key} at {f[0]:.0f} s and it never started nor was given back")

    # --- the games ------------------------------------------------------------
    print("\n--- games ---")
    if not games_by_session:
        print("  none")
    for session, seats in sorted(games_by_session.items(), key=lambda kv: set_at.get(kv[0], 0)):
        who = ", ".join(f"n{n}.{l}" for n, l in sorted(seats))
        when = set_at.get(session)
        print(f"  {session}  set at {when:.0f} s  seats here: {len(seats)} ({who})" if when is not None else f"  {session}  seats here: {len(seats)} ({who})")

    # --- the queue's own count -------------------------------------------------
    counts = [int(e[4].group(3)) for e in events if e[3] == "tick"]
    if counts:
        print(f"\nqueue  the searches saw {min(counts)}..{max(counts)} other searcher(s)")

    print("\n--- findings ---")
    if findings:
        for f in findings:
            print(f"  {f}")
    else:
        print("  none")


if __name__ == "__main__":
    main()
