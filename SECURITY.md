# Security policy

## Reporting a vulnerability

**Report it privately, through GitHub:** on this repository's *Security* tab choose *Report a vulnerability*
([the form](https://github.com/qavryxdevv/P2Poker/security/advisories/new)). The report goes to the maintainer and to
nobody else, and it stays private until a fix is out. No e-mail address is published for this project, deliberately.

Please do **not** open a public issue for something that lets one player cheat another, read cards that are not
theirs, take a seat or chips that are not theirs, knock a table over, or learn who a player is. Everything else — a
crash, a wrong pot, a window that draws badly — is an ordinary bug, and the client's own *Settings, About, Report a bug*
opens the right page.

What helps most: the version (it is on the About page), what you did, what happened, what you expected, and — for
anything on the wire — which documents of `docs/` it contradicts. `docs/THREAT_MODEL.md` says what this project claims
to defend against and, as importantly, what it does not.

## What to expect

This is a beta written by one maintainer. A report is read within a few days. If it is accepted, the fix is made in
private, released, and then described in `docs/DECISIONS.md` with credit to the reporter unless they ask otherwise. If
it is declined, the answer says why.

## Supported versions

Only the newest release is supported. Every seat at a table should run the same build.

## Checking a download

Releases are built by this repository's own workflow on GitHub's machines, and GitHub signs a statement of that
(`D-074`). To check that a file is what this repository built from its tag:

```
gh attestation verify p2p-poker.exe --repo qavryxdevv/P2Poker
```

The SHA-256 of every released file is on its release page. The client is **not** signed with a Windows code-signing
certificate, so SmartScreen asks before the first start; that question is answered by the check above, not by the
absence of the warning.
