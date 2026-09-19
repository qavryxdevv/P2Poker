# The music, where it comes from and how it got here

`the-dealers-shuffle.ogg` is *The Dealer's Shuffle*, the tune the lobby plays
while the automatic search looks for a game (`D-069`). It is inside the
executable (`src/music.rs`, `MUSIC_TRACK`), like every other asset of the client.

## Where it comes from

**Provided by the project owner on 2026-09-19**, as an MP3 (192 kbit/s, 44.1 kHz,
stereo, 3 min 01 s), to be shipped with the client.

**It was generated, not recorded.** The MP3 carried C2PA content credentials
issued by Google -- a signed manifest saying *created by Google Generative AI*,
with the digital source type `trainedAlgorithmicMedia`. There is no composer or
performer to credit, and nothing in it is somebody else's recording.

Those credentials are bound to the bytes of the original file and cannot be
carried into a re-encoding, so this file does not hold them; this note is where
the origin is kept instead. The owner keeps the original.

**Licence, in the owner's words: to be added here.** Until then: distributed with
this client by the owner's decision, with the client's own licence.

## How it got here

| | the owner's MP3 | this file |
|---|---|---|
| format | MPEG-1 layer 3, 192 kbit/s | Ogg Vorbis, quality 3 (about 110 kbit/s, variable) |
| size | 4 360 791 bytes | 2 503 449 bytes |
| length | 181.4 s | 181.38 s (7 998 912 frames at 44 100 Hz, stereo) |

Transcoded with VLC's libVorbis (`--sout-vorbis-quality=3`, 44.1 kHz, two
channels). **Why Vorbis and not the MP3 as it came:** it is a little over half the
size at a quality a lobby cannot tell apart; an MP3 begins with a gap its encoder
put there, which is heard every time a loop goes round, and a Vorbis stream does
not; and its decoder here (`lewton`) is Rust and nothing else, where an MP3 would
have meant a second decoder or a codec of the operating system's.

**Checked after the transcoding:** one Vorbis stream, nothing after its last
page; the comments hold the encoder's name and nothing else -- no title, no
account, no tool's identifier, and not the credentials; decoded, it is the length
of the original to the hundredth of a second, lines up with it sample for sample
(correlation 0.9988, no offset), at the same level (ratio 1.000). The track ends
in its own fade to silence and begins quietly, so going round needs no help.
