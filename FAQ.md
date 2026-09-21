# P2Poker — questions and answers

The questions people ask first, answered once. Something missing? Settings → About → *Report a bug* opens a form,
and ideas are welcome there too.

## Is this real-money poker?

No. **Play money only.** There is nothing to buy, nothing to win and no rake. The project accepts voluntary
donations ([DONATE.md](DONATE.md)); they buy nothing in the game.

## Is it a blockchain or crypto-token thing?

No. No blockchain, no token, no wallet. *Cryptography* here means the mathematics that lets the players deal the
cards fairly without anybody holding the deck.

## How can a game be fair without a server?

The players are the dealer. Every seat shuffles the deck under encryption and proves, in zero knowledge, that it
shuffled honestly and swapped no card (mental poker: Barnett–Smart with Bayer–Groth shuffle proofs). A card opens
only when **every** seat gives its share, so nobody — no player, no observer — sees a card they are not entitled to.
Every action is signed and chained, so two players who disagree about what happened can prove who is wrong.

## What if somebody disconnects or stops acting?

A seat that goes silent is timed out by a certificate the other seats sign, and the game goes on. A seat whose clock
runs out checks or folds until its player is back. A player whose client or connection dropped comes back to the
same game.

## Why does Windows warn me before the first start?

The program is not signed with a Windows code-signing certificate, so SmartScreen asks: *More info*, then *Run
anyway*. What it **is** signed with is a statement from GitHub: every release is built by GitHub from this public
source, and GitHub signs which commit it came from. To check a download yourself:

```
gh attestation verify p2p-poker.exe --repo qavryxdevv/P2Poker
```

The SHA-256 of each release is also on its release page.

## Does it install anything? Does it need administrator rights?

On the first start it offers to install itself: a copy in your user folder and a shortcut on the desktop and in
the Start menu. No administrator rights, nothing in the registry, nothing that starts by itself. You can also run it
from the folder it is in. To remove it, delete its folder and the shortcuts — your player profile is in that
folder, so back it up first (Settings → Profile) if you want to stay the same player.

On Linux, the `.deb` and the `.rpm` install it as any program is installed (`sudo apt install ./p2poker_….deb`, or
`sudo dnf install ./p2poker-….rpm`), and the AppImage is one file you make executable and start. Your player profile
is then in `~/.local/share/p2poker/profile`.

## Mac or Linux?

**Linux, yes** — since version 0.1.3, for x86_64 with glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and
later). The [release page](https://github.com/qavryxdevv/P2Poker/releases/latest) has an **AppImage** for any
distribution, a **.deb** for Ubuntu, Debian and Mint, an **.rpm** for Fedora and openSUSE, and a **.tar.gz**. The
Linux client plays no sounds yet. **Mac: not yet.**

## Why does it ask me to update?

It is a beta, and its protocol still changes from one version to the next, so an older client cannot play with a
newer one. When the window opens it asks GitHub whether a newer release is out; if one is, it says which, hands the
download to your browser and does not sit down at a table until the new version is started. If GitHub cannot be
reached, it plays. It never downloads or installs anything itself.

## What does it send, and to whom?

- **No account, no email, no password.** Your identity is a key in your profile folder.
- To play, it connects to other players' clients, which — as in any peer-to-peer program — see your network address.
- It finds other players through the public libp2p network and its relays.
- By default it can also relay traffic between two players who cannot reach each other directly. The first start
  tells you so, and Settings → Network turns it off.
- When its window opens, it asks GitHub for the list of releases. Nothing about you goes with that question.

## The lobby is empty. How do I find a game?

Press **Find a game** and choose a format — heads-up, six seats, ten, or whatever fills first. If nobody else is
looking, it opens a table of its own and waits there, and players who search after you are seated with you. The
project is new, so it is quiet most of the time.

## Can I play privately with friends?

Not yet. Every table is public for now.

## Where are my results, album and settings?

In the profile folder: beside the program on Windows, `~/.local/share/p2poker/profile` on Linux. Settings → Profile
makes a backup protected by a password, and restores it on another computer — Windows or Linux: you are the same
player there.

## How do I report a bug?

Settings → About → *Report a bug*, or [open an issue](https://github.com/qavryxdevv/P2Poker/issues/new/choose). The
version (Settings → About) and what you did help most. The program keeps a log, `client.log`, in its profile folder;
lines from around the time it went wrong help too — but the log can contain network addresses, so share it only if
you are comfortable with that.

## Who runs it?

Nobody runs it: there is no server to run. It is a hobby project by one developer, open source under the
GPL-3.0-or-later ([LICENSE](LICENSE)).
