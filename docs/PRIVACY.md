# Privacy policy

**P2Poker collects nothing about you.** There is no account, no telemetry, no analytics, no advertising and no
crash reporting. Nobody runs a server for this game: the project's maintainer receives no data from your client at
all, because there is nowhere for it to be sent.

Last changed 2026-09-23. It applies to every copy of P2Poker, whether it came from the
[release page](https://github.com/qavryxdevv/P2Poker/releases) or from the Microsoft Store.

## What stays on your computer

Your **profile folder** holds everything the client remembers: the two keys that are your identity, the nickname you
chose, your settings, the history and rewards of games you played, and the client's own log. It is a folder on your
own disk -- beside the program in a portable copy, in your user folder for an installed one, and in the app's own
data folder for a copy from the Microsoft Store. Nothing in it is uploaded anywhere. Deleting the folder deletes all
of it; the Profile tab makes a password-protected backup if you want to keep it or move it to another computer.

## What leaves your computer, and to whom

P2Poker is peer-to-peer: it talks to other players' clients directly, and to the public networks that let clients
find each other. While it runs, it sends

* **your nickname and your client's public keys** -- that is what another player sees of you, and it is what you
  chose plus random numbers your client generated;
* **what a poker table has to say to play a hand** -- the signed actions, the encrypted deck and the proofs that the
  shuffle was honest -- to the other players at your table;
* **an announcement that a client is here**, to the public distributed hash table other clients look in, so that
  players can find each other. Like every internet connection, this means the peers you talk to, and the nodes of
  that public network, can see your IP address.

It also asks **GitHub** once, as the window opens, which the newest released version is, so that it can tell you when
your version can no longer play with the others. That request carries no information about you.

**Real names, e-mail addresses and payment details are never asked for**, because the game has no accounts and plays
for play money only.

## Chat

The chat at a table and in the lobby goes straight to the other players, and it is not stored or moderated by
anybody. Write nothing there you would not say to strangers.

## Children

P2Poker simulates gambling with play money, so it is rated for adults and is not meant for children.

## Questions

Ask them at <https://github.com/qavryxdevv/P2Poker/issues>. If this policy changes, the new version is committed to
the same file in the public repository, so its whole history can be read there.
