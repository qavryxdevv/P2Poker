# What comes from PokerTH, and under what terms

The table window imitates the table of the PokerTH client in its *Green Casino*
style (the owner, 2026-09-13). Everything in this directory is PokerTH's, copied
unchanged from one commit:

- **Repository:** `https://github.com/pokerth/pokerth`
- **Commit:** `944e8b83` ("android: check 16 KB compatibility on the universal apk too")
- **Licence of the PokerTH tree:** GNU Affero General Public License, version 3
  or later. The text is [`COPYING`](COPYING), copied from the same commit.

The per-file sources below are PokerTH's own words, from its
`data/data-copyright.txt` at that commit, unless a line says otherwise.

## Files

| Here | In PokerTH | Source, as PokerTH records it |
|---|---|---|
| `greencasino/table.png` | `data/gfx/qml/table/greencasino/table.png` | "contributed by narmod, generated with ChatGPT (OpenAI)" |
| `greencasino/greencasinotablestyle.xml` | `data/gfx/qml/table/greencasino/` | the style's own maintainer field: PokerTH Development Team |
| `greencasino/actionFold.svg`, `actionCall.svg`, `actionRaise.svg`, `actionAllIn.svg` | `data/gfx/qml/table/greencasino/` | as above |
| `greencasino/dealerPuck.svg`, `smallblindPuck.svg`, `bigblindPuck.svg` | `data/gfx/qml/table/greencasino/` | as above |
| `sounds/yourturn.wav` | `data/sounds/default/` | KDE, `knotify/sounds/KDE_Beep_Connect.ogg` |
| `sounds/lobbychatnotify.wav` | `data/sounds/default/` | KDE, `knotify/sounds/KDE_Beep.ogg` |
| `sounds/onlinegameready.wav` | `data/sounds/default/` | kde-look.org, *kJazz* (content 5099) |
| `sounds/allin.wav`, `bet.wav`, `raise.wav`, `check.wav`, `call.wav`, `dealtwocards.wav` | `data/sounds/default/` | "donated by pokerth.net user: doc_dos" |
| `sounds/blinds_raises_level1.wav` to `level3.wav` | `data/sounds/default/` | "donated by pokerth.net user: texas_outlaw" |
| `sounds/fold.wav`, `sounds/playerconnected.wav` | `data/sounds/default/` | **not listed** in `data-copyright.txt` |
| `icons/doorExit.svg`, `gameChat.svg`, `gameLog.svg`, `settings.svg`, `trophy.svg`, `send.svg` | `src/gui/qt6-qml/resources/` | Material Design icons (Google, Apache-2.0; `send.svg` says so in its own header) |
| `fonts/Inter-VariableFont.ttf`, `fonts/Inter-OFL.txt` | `src/gui/qt6-qml/resources/` | Inter, (c) The Inter Project Authors, SIL Open Font License 1.1 (the text beside it) |

`allin.wav`, `bet.wav` and `raise.wav` are the same file in PokerTH, byte for
byte, and stay three files here so the names match PokerTH's.

**Not PokerTH's:** `assets/sounds/muck.wav`, outside this directory. PokerTH has
no muck and no sound for one; this project generates it from a fixed seed with
`tools/make-muck-sound.py` (D-050), under the client's own licence.

## What the code takes

Not only files. These modules port PokerTH's QML client and carry its values:

- `src/gui/table/seats.rs` -- `src/gui/qt6-qml/pages/seatlayout.js` and the
  layout parts of `GamePage.qml`, ported to Rust;
- `src/gui/table/style.rs` -- the colours, gradients, badges, pucks, buttons
  and sizes of `config/Theme.qml`, the `components/` it names and the Green
  Casino style XML;
- `src/gui/table/bar.rs` -- `components/GameActionBar.qml`;
- `src/gui/table/panels.rs` -- `components/GameSidePanel.qml`, `ChatBox.qml`,
  `GameInfoPanel.qml`;
- `src/gui/table/icons.rs` -- draws the SVG icons above;
- `src/sound.rs` -- the sound files above, and when PokerTH plays each of them
  (`src/gui/qt/sound/soundevents.cpp` and `components/SoundSettings.qml`);
- `src/app/tablelog.rs` -- the wording of PokerTH's hand log.

## The terms, as the licences state them

This client is GPL-3.0-or-later (`LICENSE`). GPLv3 section 13 permits combining
a GPLv3 work with a work under AGPLv3 and conveying the result; the GPL keeps
applying to the GPL part, and the special requirement of AGPLv3 section 13 --
offering the Corresponding Source to users who interact with a modified version
remotely through a computer network -- applies to the combination as such.

Two facts a reader of a public repository should have in front of them, stated
as facts and not as legal advice:

1. PokerTH records a source for the donated and the kde-look sounds but no
   licence of their own; they are distributed inside PokerTH's AGPL tree.
2. `fold.wav` and `playerconnected.wav` have no recorded source at all.

## Where this client departs from Green Casino

The owner's written description wins over `preview.png` where the two differ.
The drawing code says so at each place:

- the dealer, small blind and big blind pucks are all drawn as gold chips (Green
  Casino's small blind is blue, its big blind red);
- a bet or a raise wears a gold badge;
- a seat's bet stands above its box, not in the strip under it;
- the avatars are round, with the player's initial.

The cards are not PokerTH's: they are drawn exactly as this client drew them
before (the owner: leave the cards as they are).
