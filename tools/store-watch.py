#!/usr/bin/env python3
"""D-080: publish the release that waits for the Microsoft Store, once the Store serves its version.

    python tools/store-watch.py              publish every draft release the Store has caught up with
    python tools/store-watch.py --dry-run    say what would be published, and why; change nothing
    python tools/store-watch.py --self-test  check the reading of versions and the choice of drafts, offline

A release is made as a draft while the Store certifies the same version (`RELEASE_WAITS_FOR_STORE`, release.yml):
the client asks GitHub which version is the newest and closes its lobby when it is behind (D-077), and a copy from
the Store cannot update before Microsoft has published the new one. So the draft is published when the Store serves
that version -- and this is what notices it: `.github/workflows/store-watch.yml` runs it every hour.

What the Store serves is read from its public catalog, the one the Store app itself asks, which needs no account:
every package of the product is named there as `<identity>_<version>_<arch>__<publisher id>`. The version counted
is the one BOTH markets asked about serve (`MARKETS`), so a catalog half-way through an update publishes nothing.

Needs the GitHub CLI signed in with the right to edit the repository's releases, and three values from the
environment: `STORE_PRODUCT_ID` (the Store's id of the product), `MSIX_IDENTITY_NAME` (the package identity's name)
and `GITHUB_REPOSITORY` (owner/name; read from the `origin` remote when it is not set). Nothing is published when a
value is missing, when the catalog does not answer, or when it names no package of this identity. Exit code 0 when
there was nothing to do or everything due was published; 1 when a draft waits and the catalog could not be read, so
that a watcher that went blind says so rather than waiting for ever.
"""
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request

CATALOG = "https://displaycatalog.mp.microsoft.com/v7.0/products?bigIds={id}&market={market}&languages=en-us"
MARKETS = ("US", "CZ")
TAG = re.compile(r"^v(\d{1,6})\.(\d{1,6})\.(\d{1,6})$")


def run(*args):
    return subprocess.run(args, capture_output=True, text=True, encoding="utf-8")


def repository():
    got = os.environ.get("GITHUB_REPOSITORY", "").strip()
    if got:
        return got
    url = run("git", "remote", "get-url", "origin").stdout.strip()
    m = re.search(r"github\.com[:/]([^/]+/[^/.]+?)(?:\.git)?$", url)
    return m.group(1) if m else ""


def tag_version(tag):
    """(major, minor, patch) of a plain release tag, or None."""
    m = TAG.match(tag or "")
    return tuple(int(x) for x in m.groups()) if m else None


def package_versions(catalog, identity):
    """Every (major, minor, patch, revision) the catalog's answer names a package of `identity` at."""
    found = set()
    pattern = re.compile(r"^" + re.escape(identity) + r"_(\d{1,6})\.(\d{1,6})\.(\d{1,6})\.(\d{1,6})_[A-Za-z0-9]+__[a-z0-9]+$")

    def walk(value):
        if isinstance(value, dict):
            for k, v in value.items():
                if k == "PackageFullName" and isinstance(v, str):
                    m = pattern.match(v)
                    if m:
                        found.add(tuple(int(x) for x in m.groups()))
                else:
                    walk(v)
        elif isinstance(value, list):
            for v in value:
                walk(v)

    walk(catalog)
    return found


def served(answers, identity):
    """The highest version every market's answer serves -- (major, minor, patch) -- or None."""
    per_market = []
    for answer in answers:
        versions = package_versions(answer, identity)
        if not versions:
            return None
        per_market.append(max(versions)[:3])
    return min(per_market) if per_market else None


def due(drafts, store):
    """The drafts to publish, oldest first: those whose version the Store serves already."""
    ready = [(v, tag) for tag in drafts if (v := tag_version(tag)) is not None and store is not None and v <= store]
    return [tag for _, tag in sorted(ready)]


def drafts_of(repo):
    got = run("gh", "api", "repos/%s/releases?per_page=30" % repo, "--jq", ".[] | select(.draft) | .tag_name")
    if got.returncode != 0:
        raise SystemExit("cannot read the releases of %s: %s" % (repo, (got.stderr or got.stdout).strip()[:200]))
    return [t for t in got.stdout.split() if tag_version(t) is not None]


def catalog(product, market):
    request = urllib.request.Request(CATALOG.format(id=product, market=market), headers={"User-Agent": "P2Poker-store-watch"})
    with urllib.request.urlopen(request, timeout=30) as answer:
        return json.loads(answer.read(4_000_000).decode("utf-8"))


def summary(lines):
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    for line in lines:
        print(line)


def self_test():
    identity = "Example.P2Poker"
    answer = {"Products": [{"DisplaySkuAvailabilities": [{"Sku": {"Properties": {"Packages": [
        {"PackageFullName": "Example.P2Poker_0.1.5.0_x64__abc123"},
        {"PackageFullName": "Example.P2Poker_0.2.0.0_x64__abc123"},
        {"PackageFullName": "Other.App_9.9.9.0_x64__abc123"},
        {"PackageFullName": "Example.P2Poker_0.3.0.0_x64__abc123; rm -rf /"},
    ]}}}]}]}
    assert package_versions(answer, identity) == {(0, 1, 5, 0), (0, 2, 0, 0)}, package_versions(answer, identity)
    old = {"Products": [{"Packages": [{"PackageFullName": "Example.P2Poker_0.1.5.0_x64__abc123"}]}]}
    assert served([answer, answer], identity) == (0, 2, 0)
    assert served([answer, old], identity) == (0, 1, 5), "a market still on the old version holds the draft"
    assert served([answer, {}], identity) is None, "a market that names no package holds it too"
    assert served([], identity) is None
    assert tag_version("v0.2.0") == (0, 2, 0) and tag_version("v0.2.0-rc1") is None and tag_version("0.2.0") is None
    assert due(["v0.2.0"], (0, 1, 5)) == [], "the Store is behind: nothing"
    assert due(["v0.2.0"], (0, 2, 0)) == ["v0.2.0"], "the Store serves it: published"
    assert due(["v0.3.0", "v0.2.0"], (0, 2, 0)) == ["v0.2.0"], "only what the Store serves"
    assert due(["v0.3.0", "v0.2.1"], (0, 3, 0)) == ["v0.2.1", "v0.3.0"], "oldest first, the newest last"
    assert due(["v0.2.0"], None) == [], "no reading of the Store: nothing"
    print("self-test: ok")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    dry = "--dry-run" in sys.argv
    product = os.environ.get("STORE_PRODUCT_ID", "").strip()
    identity = os.environ.get("MSIX_IDENTITY_NAME", "").strip()
    repo = repository()
    if not (product and identity and repo):
        summary(["store-watch: STORE_PRODUCT_ID, MSIX_IDENTITY_NAME or the repository is not set; nothing is published."])
        return 0
    drafts = drafts_of(repo)
    if not drafts:
        summary(["store-watch: no release waits for the Store."])
        return 0
    try:
        answers = [catalog(product, m) for m in MARKETS]
    except (urllib.error.URLError, TimeoutError, ValueError, OSError) as e:
        summary(["store-watch: %s wait(s) for the Store, and its catalog could not be read (%s); nothing is published."
                 % (", ".join(drafts), str(e)[:200])])
        return 1
    store = served(answers, identity)
    if store is None:
        summary(["store-watch: %s wait(s) for the Store, and its catalog names no package of %s in %s; nothing is published."
                 % (", ".join(drafts), identity, " and ".join(MARKETS))])
        return 1
    shown = "%d.%d.%d" % store
    publish = due(drafts, store)
    waiting = [t for t in drafts if t not in publish]
    lines = ["store-watch: the Store serves %s (%s)." % (shown, " and ".join(MARKETS))]
    for tag in publish:
        latest = tag == publish[-1]
        args = ["gh", "release", "edit", tag, "--repo", repo, "--draft=false"] + (["--latest"] if latest else [])
        if dry:
            lines.append("would publish %s%s" % (tag, " as the latest" if latest else ""))
            continue
        got = run(*args)
        if got.returncode != 0:
            summary(lines + ["publishing %s failed: %s" % (tag, (got.stderr or got.stdout).strip()[:200])])
            return 1
        lines.append("published %s%s" % (tag, " as the latest" if latest else ""))
    for tag in waiting:
        lines.append("%s still waits: the Store serves %s." % (tag, shown))
    summary(lines)
    return 0


if __name__ == "__main__":
    sys.exit(main())
