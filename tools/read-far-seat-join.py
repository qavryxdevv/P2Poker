#!/usr/bin/env python3
"""S1-AA, patch 0040: how the seats here and the relay-only far seat found each other, one run at a time.

    python tools/read-far-seat-join.py <run> [...]        (run names under runs/, or paths)

Per seat here, toward the far seat's entry in the table's group: when it was first TOLD of it, when the search
was WIDENED to the seat's own relays and by how many (patch 0040's line), when the two made CONTACT, how many
attempts NOTHING carried, and whether the seat REAPED the entry in the join phase. Then the far seat's own reaps,
the seats the founder gave back for not hearing the table (`D-060`), and the run against the prediction patch
0040 booked before its first run:

  1. no seat reaps the far seat's entry in the join phase, and the far seat reaps nobody;
  2. every seat here that is told of the far seat is in contact with it within fifteen seconds of being told;
  3. no seat is given back for not hearing the table.

**Contact is read from either side's line.** The asking side prints *handshaked with peer*; the answering side
prints only that its response left -- which it sends because the other side's request ARRIVED. A reader that
knows the first line alone reports half the handshakes of a healthy run as missing.

**The far seat's entry** is the peer hash the seats here name and the far seat's own log never does. A run
without a far seat says so. A log with no carrier line is a build without the fault harness, and says so: the
carrier writes nothing there, and an absent instrument is not a zero. Counts only; no address or key is printed.
"""
import collections
import glob
import io
import os
import re
import sys

LINE = re.compile(r"^\ufeff?(\d\d:\d\d:\d\d\.\d{3})\s+(-?[0-9.]+)\s\s(.*)$")
HASH = re.compile(r"peer (?:hash )?(\d{6,})")
WIDE = re.compile(r"peer (\d+) did not answer the first handshake attempt; looked for on (\d+) of this client's own relays")
JOIN_PHASE_S = 150.0
CONTACT_WITHIN_S = 15.0
HERE = os.path.dirname(os.path.abspath(__file__))


def runs_root():
    for base in (os.path.join(HERE, "..", "runs"), os.path.join(HERE, "runs")):
        if os.path.isdir(base):
            return os.path.abspath(base)
    return os.path.abspath("runs")


def read(name):
    out = []
    for raw in io.open(name, encoding="utf-8", errors="replace").read().splitlines():
        m = LINE.match(raw)
        if m:
            out.append((float(m.group(2)), m.group(3).strip()))
    return out


def is_reap(x):
    return "deleting group peer" in x and "handshaked 0" in x


def fold(path):
    name = os.path.basename(os.path.normpath(path))
    logs = {os.path.basename(n)[:-4]: read(n) for n in sorted(glob.glob(os.path.join(path, "*.log")))}
    if not logs:
        print("%s: no logs" % name)
        return
    if not any(x.startswith("toxcore") for L in logs.values() for t, x in L):
        print("%s: no carrier line in any log -- not a fault-harness build, so nothing here can be read" % name)
        return
    far_tags = [t for t in logs if t.startswith("far-")]
    if not far_tags:
        print("%s: no far seat in this run" % name)
        return
    far = [row for t in far_tags for row in logs[t]]
    named_by_far = set(h for t, x in far for h in HASH.findall(x))
    counts = collections.Counter()
    for tag, L in logs.items():
        if tag in far_tags:
            continue
        for h in set(h for t, x in L if t < JOIN_PHASE_S for h in HASH.findall(x)):
            if h not in named_by_far:
                counts[h] += 1
    if not counts:
        print("%s: no entry that the seats here name and the far seat does not" % name)
        return
    far_hash = counts.most_common(1)[0][0]
    patched = any(WIDE.search(x) for L in logs.values() for t, x in L)
    print("%s: the far seat's entry is named by %d seat(s) here%s" % (
        name, counts[far_hash], "" if patched else "; no *looked for on* line in any log -- a build before patch 0040, or 0040 switched off"))
    late, reaped_here = [], 0
    for tag, L in logs.items():
        if tag in far_tags:
            continue
        told = wide = contact = added = None
        nothing, reaps = 0, []
        for t, x in L:
            w = WIDE.search(x)
            if w and w.group(1) == far_hash and wide is None:
                wide, added = t, int(w.group(2))
            if far_hash not in x:
                continue
            if told is None:
                told = t
            if "no TCP relay carried it either" in x:
                nothing += 1
            elif contact is None and ("handshaked with peer" in x or ("handshake response attempt" in x and "left by" in x)):
                contact = t
            elif is_reap(x) and t <= JOIN_PHASE_S:
                reaps.append(t)
        reaped_here += len(reaps)
        took = None if (told is None or contact is None) else contact - told
        if told is not None and (took is None or took > CONTACT_WITHIN_S):
            late.append(tag)
        sec = lambda v: "-" if v is None else "%.1f" % v
        print("   %-5s told %6s  widened %6s (%s)  contact %6s%s  nothing carried %2d  reaped %s" % (
            tag, sec(told), sec(wide), "-" if added is None else "%d own relay(s)" % added, sec(contact),
            "" if took is None else " (+%.1f s)" % took, nothing, ",".join("%.0f" % r for r in reaps) or "-"))
    far_reaps = sum(1 for t, x in far if t <= JOIN_PHASE_S and is_reap(x))
    given = sum(1 for L in logs.values() for t, x in L if "could not hear" in x and "is free again" in x)
    full = next((t for t, x in far if re.search(r"group (\d+) seen/\1 confirmed/\1 wanted", x)), None)
    print("   the far seat reaped %d of its own entries in the join phase; read the whole group first at %s" % (
        far_reaps, "-" if full is None else "%.1f s" % full))
    print("   seats given back for not hearing the table: %d" % given)
    held = not far_reaps and not reaped_here and not given and not late
    print("   the booked prediction: %s%s" % (
        "HELD" if held else "NOT HELD",
        "" if not late else " -- no contact within %d s of being told: %s" % (CONTACT_WITHIN_S, ", ".join(late))))


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    root = runs_root()
    for a in args:
        fold(a if os.path.isdir(a) else os.path.join(root, a))
    return 0


if __name__ == "__main__":
    sys.exit(main())
