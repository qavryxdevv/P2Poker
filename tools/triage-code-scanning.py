#!/usr/bin/env python3
"""S1-IU: read GitHub's open code-scanning alerts and dismiss the two kinds that are never a fault here.

    python tools/triage-code-scanning.py                 the plan: what would be dismissed and why; changes nothing
    python tools/triage-code-scanning.py --apply         dismisses those, each with its reason
    python tools/triage-code-scanning.py --review        the same plan over the alerts ALREADY dismissed: a self-check
                                                         of the rules below against verdicts a person gave

CodeQL's default setup flags by name. Every test that checks a signature or a password-protected backup needs a key
or a password it can name, and each such literal is a *critical* `rust/hard-coded-cryptographic-value`; the vendored
C this client never compiles (toxcore's audio and video, upstream's tests, its bootstrap daemon and fuzzers,
libsodium's tests) is analysed as if it shipped. Those two kinds are recognised here BY WHERE THE CODE IS, never by a
line number:

  * test code -- a file under `tests/`, or a line inside a module that begins `#[cfg(test)]` / `mod name {` at the
    left margin and ends at the `}` at the left margin, read from the file AS IT WAS at the commit the alert names;
  * vendored code that `build.rs` does not build.

Everything else is left open and printed, because an alert in code this client compiles and ships is read by a
person: by what the flagged value IS, not by the name it has (`S1-IU` in docs/DECISIONS.md).

Needs the GitHub CLI, signed in with access to the repository's security events. Reads the repository from the
`origin` remote. Exit code 0 when nothing is left open for a person, 1 otherwise.
"""
import json
import re
import subprocess
import sys

SEE = " S1-IU in docs/DECISIONS.md."
NOT_BUILT = ("vendor/c-toxcore/toxav/", "vendor/c-toxcore/testing/", "vendor/c-toxcore/other/",
             "vendor/c-toxcore/auto_tests/", "vendor/libsodium/test/")
TEST_RULES = ("rust/hard-coded-cryptographic-value", "rust/cleartext-logging")


def run(*cmd):
    return subprocess.run(list(cmd), capture_output=True, text=True, encoding="utf-8", errors="replace")


def repository():
    url = run("git", "remote", "get-url", "origin").stdout.strip()
    m = re.search(r"github\.com[:/]+([^/]+/[^/]+?)(?:\.git)?$", url)
    if not m:
        raise SystemExit("the origin remote is not a GitHub repository: %s" % url)
    return m.group(1)


_files = {}


def test_regions(sha, path):
    """(first, last) line numbers of every test module of `path` at commit `sha`; None when it cannot be read."""
    key = (sha, path)
    if key not in _files:
        got = run("git", "show", "%s:%s" % (sha, path))
        if got.returncode != 0:
            _files[key] = None
        else:
            lines = got.stdout.split("\n")
            regions, i = [], 0
            while i < len(lines) - 1:
                if lines[i].rstrip() == "#[cfg(test)]" and re.match(r"^(pub(\([a-z]+\))? )?mod \w+ \{\s*$", lines[i + 1]):
                    end = next((j for j in range(i + 2, len(lines)) if lines[j].rstrip() == "}"), None)
                    if end is None:
                        break
                    regions.append((i + 1, end + 1))
                    i = end
                i += 1
            _files[key] = regions
    return _files[key]


def verdict(alert):
    rule = alert["rule"]["id"]
    inst = alert["most_recent_instance"]
    path, line, sha = inst["location"]["path"], inst["location"]["start_line"], inst["commit_sha"]
    if rule in TEST_RULES:
        if path.startswith("tests/"):
            return "used in tests", "An integration test under tests/: a test that checks a signed or sealed object needs a fixed value it can name." + SEE
        regions = test_regions(sha, path)
        if regions is None:
            return None
        if any(a <= line <= b for a, b in regions):
            return "used in tests", "Inside this file's #[cfg(test)] module: a fixed value of a test, never compiled into the client." + SEE
        return None
    if rule.startswith("cpp/"):
        if path.startswith(NOT_BUILT) or path.endswith(("_test.cc", "_test.c")):
            return "won't fix", "Vendored upstream code this client never compiles: build.rs builds toxcore_SOURCES only and asserts no toxav/ source is in it." + SEE
        return None
    return None


def alerts(repo, state):
    got = run("gh", "api", "repos/%s/code-scanning/alerts?state=%s&per_page=100" % (repo, state), "--paginate",
              "--jq", ".[] | @json")
    if got.returncode != 0:
        raise SystemExit("cannot read the alerts: %s" % (got.stderr or got.stdout).strip()[:200])
    return [json.loads(l) for l in got.stdout.splitlines() if l.strip()]


def main():
    apply, review = "--apply" in sys.argv, "--review" in sys.argv
    if apply and review:
        raise SystemExit("--review changes nothing and cannot be combined with --apply")
    repo = repository()
    found = alerts(repo, "dismissed" if review else "open")
    plan, unknown = [], []
    for a in found:
        v = verdict(a)
        (plan if v else unknown).append((a, v))
    print("%s: %d %s alert(s); recognised %d; left for a person %d" % (
        repo, len(found), "dismissed" if review else "open", len(plan), len(unknown)))
    for a, (reason, _) in plan:
        loc = a["most_recent_instance"]["location"]
        was = "  (a person said: %s)" % a.get("dismissed_reason") if review else ""
        print("   %-14s #%-4d %s  %s:%d%s" % (reason, a["number"], a["rule"]["id"], loc["path"], loc["start_line"], was))
    for a, _ in unknown:
        loc = a["most_recent_instance"]["location"]
        was = "  (a person said: %s)" % a.get("dismissed_reason") if review else ""
        print("   %-14s #%-4d %s  %s:%d%s" % ("FOR A PERSON", a["number"], a["rule"]["id"], loc["path"], loc["start_line"], was))
    if review:
        disagree = [a for a, (reason, _) in plan if a.get("dismissed_reason") != reason]
        print("the rules disagree with a person's verdict on %d alert(s)" % len(disagree))
        return 1 if disagree else 0
    if apply:
        done = failed = 0
        for a, (reason, comment) in plan:
            assert len(comment) <= 280, (a["number"], len(comment))
            r = run("gh", "api", "-X", "PATCH", "repos/%s/code-scanning/alerts/%d" % (repo, a["number"]),
                    "-f", "state=dismissed", "-f", "dismissed_reason=%s" % reason, "-f", "dismissed_comment=%s" % comment)
            done += r.returncode == 0
            failed += r.returncode != 0
            if r.returncode != 0:
                print("   FAILED #%d: %s" % (a["number"], (r.stderr or r.stdout).strip()[:160]))
        print("dismissed %d, failed %d" % (done, failed))
        if failed:
            return 1
    return 1 if unknown else 0


if __name__ == "__main__":
    sys.exit(main())
