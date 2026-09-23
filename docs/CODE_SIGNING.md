# Code signing policy

**Status, 2026-09-23.** The `p2p-poker.exe` on the release page is **not** signed with a Windows code-signing
certificate, so SmartScreen asks before the first start (*More info*, then *Run anyway*). Two things are being done
about it, and this page says where each stands.

* **The Microsoft Store** (`D-080`): the same program, wrapped in an MSIX package, which **Microsoft signs itself**
  after certification -- a player who installs it from the Store is asked nothing. Being prepared; not published yet.
* **A free certificate for the release page**: the project applied to the [SignPath Foundation](https://signpath.org/),
  which signs open-source releases free of charge, and was **refused on 2026-09-22** for want of public visibility --
  community adoption, outside references, that kind of signal -- with an invitation to apply again as the project
  becomes better known. The policy below is what applies the day it is granted.

## What is signed, and by whom

**From the Microsoft Store:** Microsoft re-signs the package with its own certificate, and the publisher shown is the
Store account. Nothing of this project's is signed by hand for it.

**From the release page, once the Foundation has granted it:** free code signing provided by
[SignPath.io](https://signpath.io/), certificate by [SignPath Foundation](https://signpath.org/). The certificate is
the Foundation's, so the publisher Windows shows is the Foundation and not a person.

Signed: the Windows program of every release, `p2p-poker.exe`, as built by GitHub Actions from this repository's
tagged commit (`.github/workflows/release.yml`). The Linux packages carry no Authenticode signature -- Windows is
what asks for one -- and every file of every release, on both systems, carries the build attestation below.

## Who does what

| Role | Who | What they may do |
| --- | --- | --- |
| Author | the repository owner (GitHub [`qavryxdev`](https://github.com/qavryxdev)) | writes and commits the code |
| Reviewer | the repository owner | approves any change that did not come from a committer |
| Approver | the repository owner | approves each signing request at SignPath |

Every account in these roles has two-factor authentication on GitHub and at SignPath. **Every signing request is
approved by a person**: nothing is signed by a run of the workflow alone, and the signing key is never on the build
machine -- the unsigned file is handed to the signing service, and the signed file comes back.

## Privacy

The project collects nothing about the people who use it: no telemetry, no accounts, no analytics, and no crash
reports sent anywhere. P2Poker is a peer-to-peer program, so while it runs it does speak to the network by itself: it
announces itself in a public distributed hash table to find other players, carries the traffic of a table it is at,
and asks GitHub for the newest release's version. What it sends is the nickname its player chose, the public keys of
their client, and what a poker table must say to play a hand. None of it reaches the project's maintainer, who runs
no server. A player's profile -- keys, settings, game history and rewards -- stays in a folder on their own computer.

## What a download carries today, signature or not

Every release is built by GitHub Actions from this public source, and GitHub signs a statement of which commit it was
built from:

```
gh attestation verify p2p-poker.exe --repo qavryxdevv/P2Poker
```

and the SHA-256 of every file is published on its release page. That is provenance, which a person or a tool can
check; it is not an Authenticode signature, and Windows does not read it.
