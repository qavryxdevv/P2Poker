#!/usr/bin/env python3
"""The donation page: its addresses, its QR codes, and the one way to change them.

    python tools/donation-page.py --check            verify what the repository holds
    python tools/donation-page.py --write            write DONATE.md and the QR codes again from the addresses it holds
    python tools/donation-page.py --set [--lang cs]  ask for new addresses, verify, write, commit and push
    python tools/donation-page.py --selftest         the QR encoder against OpenCV on a text of every length it takes

`DONATE.md` is what the client's *Support the project* button opens, so an address in it is money: a wrong
character sends somebody's gift to nobody, for good. Everything here is built around that.

* **An address is taken only if its checksum holds and it is of the kind the page says.** Bitcoin: a mainnet
  native SegWit address (`bc1...`, bech32 for witness version 0, bech32m above it -- BIP-173, BIP-350). Tether on
  TRON: Base58Check, 21 bytes beginning 0x41. A testnet address, a legacy one, an Ethereum one are refused by name.
* **A QR code is published only if an independent decoder reads the address back out of it.** The encoder below is
  this file's own (byte mode, level M, versions 1 to 6); the decoder is OpenCV's, which shares no code with it. The
  SVG file itself is what is checked: it is parsed back into modules, drawn, decoded, and once more with six of its
  modules flipped, which only passes if the error-correction codewords are right as well. See `read_back` for the
  two readings asked of every code, and why one detector's *not found* is not held against a code.
  Without OpenCV (`pip install opencv-python-headless`) nothing is written: a QR code nobody has read is a guess.
* **The page is derived, never edited by hand**: `--write` produces the same bytes from the same addresses, and
  `--check` says so. `tests/donation_page.rs` checks the addresses' checksums again, in Rust, on every test run.
* **`--set` shows what changes before anything is written**, in groups of four characters, against what is
  published now, and wants the word typed. It warns of the one attack a checksum cannot see: malware that swaps an
  address in the clipboard for the thief's own, which is a perfectly valid address.

Nothing here touches a key. It needs Python 3.8 or later, `git` for `--set`, and OpenCV with NumPy.
"""
import hashlib
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
PAGE = os.path.join(ROOT, "DONATE.md")
ASSETS = os.path.join(ROOT, "docs", "donate")

# ---------------------------------------------------------------------------------------------------------
# The addresses' checksums
# ---------------------------------------------------------------------------------------------------------
BECH32 = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
BECH32M_CONST = 0x2BC830A3
BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def _polymod(values):
    gen = [0x3B6A57B2, 0x26508E6D, 0x1EA119FA, 0x3D4233DD, 0x2A1462B3]
    chk = 1
    for v in values:
        top = chk >> 25
        chk = ((chk & 0x1FFFFFF) << 5) ^ v
        for i in range(5):
            if (top >> i) & 1:
                chk ^= gen[i]
    return chk


def _regroup(data, frombits, tobits):
    acc = bits = 0
    out = []
    for v in data:
        acc = (acc << frombits) | v
        bits += frombits
        while bits >= tobits:
            bits -= tobits
            out.append((acc >> bits) & ((1 << tobits) - 1))
    if bits >= frombits or ((acc << (tobits - bits)) & ((1 << tobits) - 1)):
        return None
    return out


def bitcoin_problem(addr):
    """None for a mainnet native SegWit address, else what is wrong with it, in words."""
    if not addr:
        return "empty"
    if addr[0] in "13" and 26 <= len(addr) <= 35:
        return "a legacy address (1... or 3...); the page says native SegWit, which begins bc1"
    if addr.lower().startswith("tb1") or addr.lower().startswith("bcrt1"):
        return "a TESTNET address: coins sent to it on the real network are lost"
    if addr.lower() != addr and addr.upper() != addr:
        return "upper and lower case mixed, which bech32 forbids"
    a = addr.lower()
    pos = a.rfind("1")
    if pos < 1 or pos + 7 > len(a) or len(a) > 90:
        return "not a bech32 address"
    hrp, rest = a[:pos], a[pos + 1:]
    if hrp != "bc":
        return "not a Bitcoin mainnet address (it must begin bc1)"
    if any(c not in BECH32 for c in rest):
        return "a character that bech32 does not have"
    data = [BECH32.find(c) for c in rest]
    const = _polymod([ord(x) >> 5 for x in hrp] + [0] + [ord(x) & 31 for x in hrp] + data)
    if const not in (1, BECH32M_CONST):
        return "the checksum does not hold: a character is wrong"
    version = data[0]
    program = _regroup(data[1:-6], 5, 8)
    if program is None or not 2 <= len(program) <= 40 or version > 16:
        return "not a witness program"
    if version == 0 and const != 1:
        return "witness version 0 under the wrong checksum"
    if version != 0 and const != BECH32M_CONST:
        return "witness version 1 or above under the wrong checksum"
    if version == 0 and len(program) not in (20, 32):
        return "a version 0 program of the wrong length"
    return None


def bitcoin_kind(addr):
    data = [BECH32.find(c) for c in addr.lower()[3:]]
    version, program = data[0], _regroup(data[1:-6], 5, 8)
    if version == 0:
        return "native SegWit, P2WPKH" if len(program) == 20 else "native SegWit, P2WSH"
    return "Taproot" if version == 1 else "witness version %d" % version


def tron_problem(addr):
    """None for a TRON mainnet address, else what is wrong with it, in words."""
    if not addr:
        return "empty"
    if addr.startswith("0x"):
        return "an Ethereum-style address: this page takes USDT on the TRON network, whose addresses begin T"
    if any(c not in BASE58 for c in addr):
        return "a character that base58 does not have (0, O, I and l are not in it)"
    n = 0
    for c in addr:
        n = n * 58 + BASE58.index(c)
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    raw = b"\x00" * (len(addr) - len(addr.lstrip("1"))) + raw
    if len(raw) != 25:
        return "not 25 bytes under Base58Check"
    body, check = raw[:-4], raw[-4:]
    if hashlib.sha256(hashlib.sha256(body).digest()).digest()[:4] != check:
        return "the checksum does not hold: a character is wrong"
    if body[0] != 0x41:
        return "not a TRON mainnet address (they begin T)"
    return None


# ---------------------------------------------------------------------------------------------------------
# A QR code: byte mode, error correction level M, versions 1 to 6 (ISO/IEC 18004)
# ---------------------------------------------------------------------------------------------------------
# version: (error-correction codewords per block, data codewords of each block)
QR_M = {1: (10, [16]), 2: (16, [28]), 3: (26, [44]), 4: (18, [32, 32]), 5: (24, [43, 43]), 6: (16, [27, 27, 27, 27])}
QR_ALIGN = {1: None, 2: 18, 3: 22, 4: 26, 5: 30, 6: 34}

_EXP = [0] * 512
_LOG = [0] * 256
_x = 1
for _i in range(255):
    _EXP[_i] = _x
    _LOG[_x] = _i
    _x <<= 1
    if _x & 0x100:
        _x ^= 0x11D
for _i in range(255, 512):
    _EXP[_i] = _EXP[_i - 255]


def _gf_mul(a, b):
    return 0 if a == 0 or b == 0 else _EXP[_LOG[a] + _LOG[b]]


def _rs_remainder(data, degree):
    gen = [1]
    for i in range(degree):
        nxt = [0] * (len(gen) + 1)
        for j, g in enumerate(gen):
            nxt[j] ^= g
            nxt[j + 1] ^= _gf_mul(g, _EXP[i])
        gen = nxt
    rem = [0] * degree
    for byte in data:
        factor = byte ^ rem[0]
        rem = rem[1:] + [0]
        for j in range(degree):
            rem[j] ^= _gf_mul(gen[j + 1], factor)
    return rem


def qr_matrix(text):
    """The modules of the smallest level-M code that holds `text`: a list of rows of booleans, no quiet zone."""
    payload = text.encode("utf-8")
    version = next((v for v in sorted(QR_M) if (sum(QR_M[v][1]) * 8 - 12) // 8 >= len(payload)), None)
    if version is None:
        raise ValueError("%d bytes do not fit a version 6 code" % len(payload))
    ec_len, blocks = QR_M[version]
    capacity = sum(blocks) * 8

    bits = [0, 1, 0, 0] + [(len(payload) >> i) & 1 for i in range(7, -1, -1)]
    for byte in payload:
        bits += [(byte >> i) & 1 for i in range(7, -1, -1)]
    bits += [0] * min(4, capacity - len(bits))
    bits += [0] * (-len(bits) % 8)
    pad = 0xEC
    while len(bits) < capacity:
        bits += [(pad >> i) & 1 for i in range(7, -1, -1)]
        pad ^= 0xEC ^ 0x11
    words = [int("".join(map(str, bits[i:i + 8])), 2) for i in range(0, capacity, 8)]

    data_blocks, at = [], 0
    for n in blocks:
        data_blocks.append(words[at:at + n])
        at += n
    ec_blocks = [_rs_remainder(b, ec_len) for b in data_blocks]
    stream = []
    for i in range(max(blocks)):
        stream += [b[i] for b in data_blocks if i < len(b)]
    for i in range(ec_len):
        stream += [b[i] for b in ec_blocks]
    stream_bits = [(w >> i) & 1 for w in stream for i in range(7, -1, -1)]
    stream_bits += [0] * (0 if version == 1 else 7)

    size = 17 + 4 * version
    dark = [[False] * size for _ in range(size)]
    fixed = [[False] * size for _ in range(size)]

    def put(row, col, value):
        if 0 <= row < size and 0 <= col < size:
            dark[row][col] = value
            fixed[row][col] = True

    for top, left in ((0, 0), (0, size - 7), (size - 7, 0)):
        for r in range(-1, 8):
            for c in range(-1, 8):
                ring = max(abs(r - 3), abs(c - 3))
                put(top + r, left + c, ring in (0, 1, 3))
    for i in range(8, size - 8):
        put(6, i, i % 2 == 0)
        put(i, 6, i % 2 == 0)
    centre = QR_ALIGN[version]
    if centre is not None:
        for r in range(-2, 3):
            for c in range(-2, 3):
                put(centre + r, centre + c, max(abs(r), abs(c)) != 1)
    put(size - 8, 8, True)
    for i in range(9):
        for row, col in ((8, i), (i, 8)):
            if not fixed[row][col]:
                put(row, col, False)
    for i in range(8):
        put(8, size - 1 - i, False)
        put(size - 1 - i, 8, dark[size - 1 - i][8])

    index = 0
    col = size - 1
    upward = True
    while col > 0:
        if col == 6:
            col -= 1
        for step in range(size):
            row = size - 1 - step if upward else step
            for c in (col, col - 1):
                if not fixed[row][c]:
                    dark[row][c] = index < len(stream_bits) and stream_bits[index] == 1
                    index += 1
        upward = not upward
        col -= 2
    if index != len(stream_bits):
        raise AssertionError("placed %d bits of %d" % (index, len(stream_bits)))

    masks = [
        lambda r, c: (r + c) % 2 == 0,
        lambda r, c: r % 2 == 0,
        lambda r, c: c % 3 == 0,
        lambda r, c: (r + c) % 3 == 0,
        lambda r, c: (r // 2 + c // 3) % 2 == 0,
        lambda r, c: (r * c) % 2 + (r * c) % 3 == 0,
        lambda r, c: ((r * c) % 2 + (r * c) % 3) % 2 == 0,
        lambda r, c: ((r + c) % 2 + (r * c) % 3) % 2 == 0,
    ]

    def finished(mask):
        m = [[dark[r][c] ^ (masks[mask](r, c) and not fixed[r][c]) for c in range(size)] for r in range(size)]
        data = mask  # level M is 00
        rem = data
        for _ in range(10):
            rem = (rem << 1) ^ ((rem >> 9) * 0x537)
        fmt = ((data << 10) | rem) ^ 0x5412
        bit = lambda i: (fmt >> i) & 1 == 1
        for i in range(6):
            m[i][8] = bit(i)
        m[7][8], m[8][8], m[8][7] = bit(6), bit(7), bit(8)
        for i in range(9, 15):
            m[8][14 - i] = bit(i)
        for i in range(8):
            m[8][size - 1 - i] = bit(i)
        for i in range(8, 15):
            m[size - 15 + i][8] = bit(i)
        m[size - 8][8] = True
        return m

    def penalty(m):
        score = 0
        lines = [row for row in m] + [[m[r][c] for r in range(size)] for c in range(size)]
        for line in lines:
            run = 1
            for i in range(1, size):
                if line[i] == line[i - 1]:
                    run += 1
                else:
                    score += run - 2 if run >= 5 else 0
                    run = 1
            score += run - 2 if run >= 5 else 0
            text_line = "".join("1" if v else "0" for v in line)
            for pattern in ("10111010000", "00001011101"):
                start = text_line.find(pattern)
                while start >= 0:
                    score += 40
                    start = text_line.find(pattern, start + 1)
        for r in range(size - 1):
            for c in range(size - 1):
                if m[r][c] == m[r][c + 1] == m[r + 1][c] == m[r + 1][c + 1]:
                    score += 3
        total = sum(sum(1 for v in row if v) for row in m)
        score += 10 * (abs(total * 20 - size * size * 10) // (size * size))
        return score

    return min((finished(k) for k in range(8)), key=penalty)


QUIET = 4


def qr_svg(matrix, says):
    """The code as an SVG file, titled with the text it says: a reader of the file, and `tests/donation_page.rs`,
    can see what it is for without decoding it. The title is a label; `qr_problems` is the proof."""
    size = len(matrix)
    runs = []
    for r, row in enumerate(matrix):
        c = 0
        while c < size:
            if row[c]:
                start = c
                while c < size and row[c]:
                    c += 1
                runs.append("M%d,%dh%dv1h-%dz" % (start + QUIET, r + QUIET, c - start, c - start))
            else:
                c += 1
    full = size + 2 * QUIET
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" width="%d" height="%d" '
        'shape-rendering="crispEdges"><title>%s</title><rect width="%d" height="%d" fill="#ffffff"/>'
        '<path d="%s" fill="#000000"/></svg>\n' % (
            full, full, full * 8, full * 8,
            says.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"), full, full, "".join(runs))
    )


def svg_matrix(svg):
    """The modules an SVG written by `qr_svg` draws, quiet zone included -- read from the file, not remembered."""
    full = int(re.search(r'viewBox="0 0 (\d+) \d+"', svg).group(1))
    grid = [[False] * full for _ in range(full)]
    for x, y, w in re.findall(r"M(\d+),(\d+)h(\d+)v1h-\d+z", re.search(r'<path d="([^"]*)"', svg).group(1)):
        for c in range(int(x), int(x) + int(w)):
            grid[int(y)][c] = True
    return grid


DAMAGED_MODULES = 6
# Pixels a module, in the order they are tried by a detector that has to find the symbol itself.
PIXELS = (10, 8, 6, 12)


def _damaged(grid):
    """`grid` with `DAMAGED_MODULES` different modules flipped in the middle of the symbol, clear of every finder,
    timing and alignment pattern: at most six codewords wrong, which level M repairs -- and only if the
    error-correction codewords are right. Versions 2 to 6 repair eight or more in each block. Version 1 repairs
    four, and its middle is three modules by three, which no more than four codewords pass through."""
    grid = [row[:] for row in grid]
    inner = len(grid) - 2 * QUIET
    state, flipped = 12345, set()
    while len(flipped) < DAMAGED_MODULES:
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        r = 9 + state % (inner - 18)
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        c = 9 + state % (inner - 18)
        if (r, c) not in flipped:
            flipped.add((r, c))
            grid[QUIET + r][QUIET + c] = not grid[QUIET + r][QUIET + c]
    return grid


def _picture(grid, pixels, soft=False):
    import cv2
    import numpy

    image = numpy.full((len(grid) * pixels, len(grid) * pixels), 255, dtype=numpy.uint8)
    for r, row in enumerate(grid):
        for c, on in enumerate(row):
            if on:
                image[r * pixels:(r + 1) * pixels, c * pixels:(c + 1) * pixels] = 0
    return cv2.GaussianBlur(image, (3, 3), 0) if soft else image


def read_back(grid, damage=False):
    """What OpenCV, which shares no code with the encoder above, reads out of these modules: `(told, found)`.

    `told` is its decoder's answer once it is told where the symbol's corners are -- this file drew the picture,
    so it knows them. That is the proof of the encoder, and every code must pass it, whole and damaged.

    `found` is every answer of a detector that had to find the symbol in the picture first, as a camera has to:
    `(detector, pixels a module, soft, text)`. OpenCV's classic detector misses some perfectly good symbols in a
    perfectly sharp picture -- measured with `--selftest` on OpenCV 4.12: of 106 random texts it did not find
    seven at ten pixels a module and one of those at no size tried, while its own decoder read every one of them
    once told the corners and the other detector found all 106 at the first size -- so the picture is tried at
    several sizes, sharp and a little soft, by both detectors, until each has read it once. An empty answer
    means *not found*; an answer that is not the text is a wrong code, whoever gives it.
    """
    import cv2
    import numpy

    grid = _damaged(grid) if damage else grid
    near, far = QUIET * PIXELS[0], (len(grid) - QUIET) * PIXELS[0]
    corners = numpy.array([[[near, near], [far, near], [far, far], [near, far]]], dtype=numpy.float32)
    told = cv2.QRCodeDetector().decode(_picture(grid, PIXELS[0]), corners)[0]
    found = []
    for name in ("QRCodeDetector", "QRCodeDetectorAruco"):
        if not hasattr(cv2, name):
            continue
        for pixels, soft in [(p, s) for s in (False, True) for p in PIXELS]:
            text = getattr(cv2, name)().detectAndDecode(_picture(grid, pixels, soft))[0]
            found.append((name, pixels, soft, text))
            if text:
                break
    return told, found


def qr_problems(grid, want):
    """Every reason the code these modules draw must not be published as `want`."""
    problems = []
    for damage in (False, True):
        state = "with %d of its modules flipped" % DAMAGED_MODULES if damage else "whole"
        told, found = read_back(grid, damage)
        if told != want:
            problems.append("OpenCV's decoder, told where the code is, read %r out of it %s; it should say %r" % (
                told, state, want))
        for name, pixels, soft, text in found:
            if text and text != want:
                problems.append("%s read %r out of the code %s (%d pixels a module%s); it should say %r" % (
                    name, text, state, pixels, ", soft" if soft else "", want))
        if not damage and not any(text == want for _, _, _, text in found):
            problems.append("no detector of OpenCV's found the code in its picture at %s pixels a module, sharp "
                            "or soft, though the decoder reads it" % "/".join(map(str, PIXELS)))
    return problems


LONGEST = 106


def selftest(lengths=None):
    """The encoder against OpenCV on a random text of every length it takes, 1 to 106 bytes (versions 1 to 6)."""
    import random

    lengths = list(lengths or range(1, LONGEST + 1))
    alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:/?=&.-_"
    per_version, missed, wrong = {}, [], []
    for n in lengths:
        # Seeded by the length, so a length's text is the same whichever lengths are asked for.
        rng = random.Random(20260920 * 1000 + n)
        text = "".join(rng.choice(alphabet) for _ in range(n))
        matrix = qr_matrix(text)
        version = (len(matrix) - 17) // 4
        per_version[version] = per_version.get(version, 0) + 1
        grid = svg_matrix(qr_svg(matrix, text))
        if [row[QUIET:-QUIET] for row in grid[QUIET:-QUIET]] != matrix:
            wrong.append("length %d: the SVG does not draw the modules it was made from" % n)
        wrong += ["length %d: %s" % (n, p) for p in qr_problems(grid, text)]
        _, found = read_back(grid)
        missed += ["length %d (version %d): %s did not find it at %d pixels a module%s" % (
            n, version, name, pixels, ", soft" if soft else "") for name, pixels, soft, got in found if not got]
    try:
        qr_matrix("x" * (LONGEST + 1))
        wrong.append("%d bytes were taken, and do not fit" % (LONGEST + 1))
    except ValueError:
        pass
    wrong += refusals_problems()
    print("addresses refused, each for its own reason and each reason with a line in Czech: %d" % len(REFUSED))
    print("codes made, by version:", ", ".join("%d: %d" % kv for kv in sorted(per_version.items())))
    print("not found by a detector in some picture (its decoder read every one of them, told the corners): %d" % len(missed))
    for line in missed:
        print("   " + line)
    for line in wrong:
        print("WRONG:", line)
    if not wrong:
        print("OK: all %d codes were read back, whole and with %d modules flipped, by a decoder told their corners "
              "and by a detector that found them itself; %d bytes are refused" % (
                  len(lengths), DAMAGED_MODULES, LONGEST + 1))
    return 1 if wrong else 0


# ---------------------------------------------------------------------------------------------------------
# The page
# ---------------------------------------------------------------------------------------------------------
COINS = [
    # key, file stem, what the QR code says, the checker
    ("bitcoin", "qr-bitcoin", lambda a: "bitcoin:" + a, bitcoin_problem),
    ("tether-tron", "qr-tether-tron", lambda a: a, tron_problem),
]

BITCOIN_LOGO = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
<circle cx="32" cy="32" r="32" fill="#F7931A"/>
<g transform="rotate(14 32 32)" fill="#ffffff" fill-rule="evenodd">
<path d="M21 15h7v34h-7z"/>
<path d="M27 15h9.5a8 8 0 0 1 0 16H27zM27 20.5v5h8.5a2.5 2.5 0 0 0 0-5z"/>
<path d="M27 31h11a9 9 0 0 1 0 18H27zM27 36.5v7h10a3.5 3.5 0 0 0 0-7z"/>
<path d="M26 9h3.4v7H26zM32.6 9H36v7h-3.4zM26 48h3.4v7H26zM32.6 48H36v7h-3.4z"/>
</g>
</svg>
"""

TETHER_LOGO = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
<circle cx="32" cy="32" r="32" fill="#26A17B"/>
<g fill="#ffffff">
<path d="M16 15h32v7.5H16z"/>
<path d="M28 22h8v31h-8z"/>
<path d="M19 27.5h26v4.5H19z"/>
</g>
</svg>
"""

TEMPLATE = """<!-- Written by tools/donation-page.py. Do not edit by hand: change an address with
     `python tools/donation-page.py --set`, which verifies it and draws its QR code again. -->

<h1 align="center">&#9829; Support P2Poker</h1>

<p align="center">
<b>No house. No server. No rake. No ads.</b><br>
P2Poker is free, open source and peer to peer &mdash; nobody earns a cent when you play.<br>
If it gave you a good evening at the tables, this is how it keeps going.
</p>

<p align="center"><sub>A donation is a gift. It buys no chips, no rank and no advantage at any table:
the game is play money, and it stays fair for everybody.</sub></p>

---

<h2 align="center"><img src="docs/donate/bitcoin.svg" width="36" height="36" align="top" alt=""> Bitcoin</h2>

<p align="center"><sub>{bitcoin_kind} &middot; Bitcoin main network</sub></p>

<p align="center"><img src="docs/donate/qr-bitcoin.svg" width="260" height="260" alt="QR code of the Bitcoin address below"></p>

<!-- donation:bitcoin -->
```
{bitcoin}
```
<!-- /donation:bitcoin -->

<p align="center"><sub>Send <b>Bitcoin only</b> to this address.</sub></p>

---

<h2 align="center"><img src="docs/donate/tether.svg" width="36" height="36" align="top" alt=""> Tether &middot; USDT</h2>

<p align="center"><sub>TRC-20 &middot; <b>TRON network</b></sub></p>

<p align="center"><img src="docs/donate/qr-tether-tron.svg" width="260" height="260" alt="QR code of the TRON address below"></p>

<!-- donation:tether-tron -->
```
{tether-tron}
```
<!-- /donation:tether-tron -->

<p align="center"><sub>Send USDT over the <b>TRON network (TRC-20)</b> only. The same coin sent over
Ethereum, BNB Chain or any other network does not arrive.</sub></p>

---

### Before you send

* **Compare the first six and the last six characters** of the address in your wallet with the ones on this
  page, after you scan or paste. Malware that swaps an address in the clipboard for a thief's own is real, and
  the address it puts there is a perfectly valid one.
* **This page is the only place these addresses are published.** The client's *Support the project* button
  opens exactly this file, and the repository's history shows every change ever made to it.
* Both QR codes say nothing but the address above them (`bitcoin:` and the address, for Bitcoin).

<p align="center"><b>Thank you.</b> &#9824;&#65039; &#9829;&#65039; &#9830;&#65039; &#9827;&#65039;</p>

<sub>The addresses and their QR codes are written by <code>tools/donation-page.py</code>, which takes an address
only if its checksum holds and publishes a QR code only after an independent decoder has read the address back
out of it. <code>python tools/donation-page.py --check</code> repeats both for what is here now.</sub>
"""


def page_addresses(text):
    found = {}
    for key, _, _, _ in COINS:
        m = re.search(r"<!-- donation:%s -->\s*```\s*(\S+)\s*```\s*<!-- /donation:%s -->" % (key, key), text)
        if m:
            found[key] = m.group(1)
    return found


def render(addresses):
    text = TEMPLATE.replace("{bitcoin_kind}", bitcoin_kind(addresses["bitcoin"]))
    for key, _, _, _ in COINS:
        text = text.replace("{%s}" % key, addresses[key])
    files = {"DONATE.md": text, "docs/donate/bitcoin.svg": BITCOIN_LOGO, "docs/donate/tether.svg": TETHER_LOGO}
    for key, stem, says, _ in COINS:
        files["docs/donate/%s.svg" % stem] = qr_svg(qr_matrix(says(addresses[key])), says(addresses[key]))
    return files


def verify(addresses, files):
    """Every reason these files must not be published; an empty list when there is none."""
    problems = []
    for key, stem, says, problem in COINS:
        wrong = problem(addresses.get(key, ""))
        if wrong:
            problems.append("%s: %s" % (key, wrong))
            continue
        try:
            grid = svg_matrix(files["docs/donate/%s.svg" % stem])
            problems += ["%s: %s" % (key, p) for p in qr_problems(grid, says(addresses[key]))]
        except ImportError:
            problems.append("OpenCV is not installed, so no QR code can be read back: pip install opencv-python-headless")
            break
    return problems


def read_disk():
    files = {}
    for rel in ["DONATE.md", "docs/donate/bitcoin.svg", "docs/donate/tether.svg"] + [
            "docs/donate/%s.svg" % stem for _, stem, _, _ in COINS]:
        path = os.path.join(ROOT, *rel.split("/"))
        files[rel] = open(path, encoding="utf-8", newline="").read().replace("\r\n", "\n") if os.path.exists(path) else None
    return files


def write_disk(files):
    os.makedirs(ASSETS, exist_ok=True)
    for rel, text in files.items():
        path = os.path.join(ROOT, *rel.split("/"))
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)


def check():
    disk = read_disk()
    if disk["DONATE.md"] is None:
        print("DONATE.md is not there")
        return 1
    addresses = page_addresses(disk["DONATE.md"])
    problems = []
    for key, _, _, _ in COINS:
        if key not in addresses:
            problems.append("%s: no address between its markers in DONATE.md" % key)
    if not problems:
        problems = verify(addresses, disk if all(v is not None for v in disk.values()) else render(addresses))
        fresh = render(addresses)
        for rel, text in fresh.items():
            if disk.get(rel) != text:
                problems.append("%s is not what --write makes of these addresses: edited by hand, or never written" % rel)
    for key, address in addresses.items():
        print("%-12s %s" % (key, address))
    for p in problems:
        print("WRONG:", p)
    if not problems:
        print("OK: both checksums hold, both QR codes were read back by OpenCV -- whole, and with %d of their "
              "modules flipped -- and every file is what --write makes" % DAMAGED_MODULES)
    return 1 if problems else 0


# ---------------------------------------------------------------------------------------------------------
# --set
# ---------------------------------------------------------------------------------------------------------
WORDS = {
    "en": {
        "intro": "Change the donation addresses of P2Poker on GitHub.\nNothing is written or sent before you confirm at the end. Ctrl+C leaves at any moment.",
        "clipboard": "WARNING: malware can swap an address in the clipboard for a thief's. After pasting, compare the FIRST SIX\nand the LAST SIX characters with your wallet. A checksum cannot see that attack: the thief's address is valid too.",
        "now": "published now",
        "ask": "new address (Enter keeps the one published now)",
        "bitcoin": "Bitcoin, native SegWit (bc1...)",
        "tether-tron": "Tether USDT on the TRON network (T...)",
        "refused": "REFUSED",
        "kept": "kept as it is",
        "nothing": "Nothing changes. Nothing was written.",
        "summary": "THIS IS WHAT WILL BE PUBLISHED",
        "was": "was",
        "will": "will be",
        "same": "unchanged",
        "verified": "Both checksums hold, and OpenCV read both QR codes back, whole and damaged.",
        "confirm": "Type YES to write, commit and push this to GitHub; anything else leaves everything as it was",
        "yes": "YES",
        "left": "Left as it was. Nothing was written.",
        "dirty": "The repository has other changes to these files that are not committed. Commit or undo them first:",
        "git": "git failed; nothing was pushed",
        "restored": "The files were put back as they were.",
        "done": "Published, and GitHub holds it. Look at the page, and scan both QR codes with a wallet before you tell anybody:",
        "problems": "Not published, because:",
        "branch": "The page the client opens is on the branch '%s', and this clone is on '%s'. Nothing was written.",
        "fetch": "GitHub could not be reached, so nothing was written:",
        "ahead": "This clone holds %s commit(s) not pushed to GitHub yet, and a push would publish them as well.\nPush or drop them first. Nothing was written.",
        "behind": "This clone is behind GitHub and could not be brought up to date. Nothing was written:",
        "unseen": "The push reported no error, but GitHub does not show the new commit. Look at the repository before anything else.",
        "nobody": "git does not know who commits in this clone (user.name and user.email are not set). Nothing was written.",
    },
    "cs": {
        "intro": "Zmena darovacich adres P2Poker na GitHubu.\nDokud na konci nepotvrdite, nic se nezapise ani neodesle. Ctrl+C kdykoli skonci.",
        "clipboard": "POZOR: skodlivy program umi adresu ve schrance vymenit za zlodejovu. Po vlozeni porovnejte PRVNICH SEST\na POSLEDNICH SEST znaku se svou penezenkou. Kontrolni soucet tenhle utok nepozna: i zlodejova adresa je platna.",
        "now": "nyni zverejneno",
        "ask": "nova adresa (Enter ponecha tu zverejnenou)",
        "bitcoin": "Bitcoin, native SegWit (bc1...)",
        "tether-tron": "Tether USDT v siti TRON (T...)",
        "refused": "ODMITNUTO",
        "kept": "ponechano beze zmeny",
        "nothing": "Nic se nemeni. Nic nebylo zapsano.",
        "summary": "TOTO BUDE ZVEREJNENO",
        "was": "bylo",
        "will": "bude",
        "same": "beze zmeny",
        "verified": "Oba kontrolni soucty sedi a OpenCV precetlo oba QR kody zpet, cele i poskozene.",
        "confirm": "Napiste ANO pro zapis, commit a odeslani na GitHub; cokoli jineho necha vse, jak bylo",
        "yes": "ANO",
        "left": "Ponechano, jak bylo. Nic nebylo zapsano.",
        "dirty": "V repozitari jsou u techto souboru jine nezapsane zmeny. Nejdriv je commitnete nebo vratte:",
        "git": "git selhal; nic nebylo odeslano",
        "restored": "Soubory byly vraceny do puvodniho stavu.",
        "done": "Zverejneno a GitHub to ma. Podivejte se na stranku a oba QR kody nactete penezenkou, nez to komukoli reknete:",
        "problems": "Nezverejneno, protoze:",
        "branch": "Stranka, kterou klient otevira, je ve vetvi '%s', a tento klon je ve vetvi '%s'. Nic nebylo zapsano.",
        "fetch": "GitHub neni dostupny, takze nic nebylo zapsano:",
        "ahead": "Tento klon obsahuje %s commit(u), ktere jeste nejsou na GitHubu, a push by zverejnil i je.\nNejdriv je odeslete nebo zruste. Nic nebylo zapsano.",
        "behind": "Tento klon je pozadu za GitHubem a nepodarilo se ho srovnat. Nic nebylo zapsano:",
        "unseen": "Push nehlasil chybu, ale GitHub novy commit neukazuje. Nejdriv se podivejte do repozitare.",
        "nobody": "git nevi, kdo v tomto klonu commituje (neni nastaveno user.name a user.email). Nic nebylo zapsano.",
    },
}
# Why an address was refused, in the owner's language. The checkers answer in English, and `--selftest` holds every
# answer they can give to a line here.
REASONS_CS = {
    "empty": "prazdne",
    "a legacy address (1... or 3...); the page says native SegWit, which begins bc1":
        "stara adresa (1... nebo 3...); stranka uvadi native SegWit, ktera zacina bc1",
    "a TESTNET address: coins sent to it on the real network are lost":
        "adresa TESTOVACI site: mince poslane na ni v ostre siti jsou ztracene",
    "upper and lower case mixed, which bech32 forbids": "smichana velka a mala pismena, coz bech32 zakazuje",
    "not a bech32 address": "neni to adresa bech32",
    "not a Bitcoin mainnet address (it must begin bc1)": "neni to adresa hlavni site Bitcoinu (musi zacinat bc1)",
    "a character that bech32 does not have": "znak, ktery bech32 nema",
    "the checksum does not hold: a character is wrong": "kontrolni soucet nesedi: nektery znak je spatne",
    "not a witness program": "neni to witness program",
    "witness version 0 under the wrong checksum": "witness verze 0 pod spatnym druhem kontrolniho souctu",
    "witness version 1 or above under the wrong checksum": "witness verze 1 a vyssi pod spatnym druhem kontrolniho souctu",
    "a version 0 program of the wrong length": "program verze 0 ma spatnou delku",
    "an Ethereum-style address: this page takes USDT on the TRON network, whose addresses begin T":
        "adresa ve stylu Etherea: tato stranka prijima USDT v siti TRON, jejiz adresy zacinaji T",
    "a character that base58 does not have (0, O, I and l are not in it)":
        "znak, ktery base58 nema (0, O, I a l v nem nejsou)",
    "not 25 bytes under Base58Check": "po dekodovani Base58Check to neni 25 bajtu",
    "not a TRON mainnet address (they begin T)": "neni to adresa hlavni site TRON (ty zacinaji T)",
}
def _segwit_string(version, program, const):
    """A `bc1` string of this witness version and program under this checksum constant -- well formed or not: the
    refusals below need strings whose checksum holds and whose contents are wrong, which nobody publishes."""
    acc = bits = 0
    data = [version]
    for byte in program:
        acc = (acc << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            data.append((acc >> bits) & 31)
    if bits:
        data.append((acc << (5 - bits)) & 31)
    mod = _polymod([3, 3, 0, 2, 3] + data + [0] * 6) ^ const
    return "bc1" + "".join(BECH32[d] for d in data + [(mod >> 5 * (5 - i)) & 31 for i in range(6)])


# An address for every answer the two checkers can give, and the answer it must get.
REFUSED = [
    (bitcoin_problem, _segwit_string(0, bytes(41), 1), "not a witness program"),
    (bitcoin_problem, _segwit_string(0, bytes(21), 1), "a version 0 program of the wrong length"),
    (bitcoin_problem, _segwit_string(0, bytes(20), BECH32M_CONST), "witness version 0 under the wrong checksum"),
    (bitcoin_problem, _segwit_string(1, bytes(32), 1), "witness version 1 or above under the wrong checksum"),
    (bitcoin_problem, "", "empty"),
    (bitcoin_problem, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", "a legacy address (1... or 3...); the page says native SegWit, which begins bc1"),
    (bitcoin_problem, "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx", "a TESTNET address: coins sent to it on the real network are lost"),
    (bitcoin_problem, "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kV8f3t4", "upper and lower case mixed, which bech32 forbids"),
    (bitcoin_problem, "bc1q", "not a bech32 address"),
    (bitcoin_problem, "ltc1qw508d6qejxtdg4y5r3zarvary0c5xw7kgmn4n9", "not a Bitcoin mainnet address (it must begin bc1)"),
    (bitcoin_problem, "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3tb", "a character that bech32 does not have"),
    (bitcoin_problem, "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5", "the checksum does not hold: a character is wrong"),
    (bitcoin_problem, "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kemeawh", "witness version 0 under the wrong checksum"),
    (bitcoin_problem, "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqh2y7hd", "witness version 1 or above under the wrong checksum"),
    (tron_problem, "", "empty"),
    (tron_problem, "0x52908400098527886E0F7030069857D2E4169EE7", "an Ethereum-style address: this page takes USDT on the TRON network, whose addresses begin T"),
    (tron_problem, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj60", "a character that base58 does not have (0, O, I and l are not in it)"),
    (tron_problem, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj", "not 25 bytes under Base58Check"),
    (tron_problem, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6u", "the checksum does not hold: a character is wrong"),
    (tron_problem, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", "not a TRON mainnet address (they begin T)"),
]


def refusals_problems():
    """What is wrong with the refusals: an address that gets another answer than it must, or an answer that the
    owner's language has no line for."""
    wrong = []
    # The strings made here are made right: BIP-173's own program gives BIP-173's own address, and it is taken.
    made = _segwit_string(0, bytes.fromhex("751e76e8199196d454941c45d1b3a323f1433bd6"), 1)
    if made != "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4" or bitcoin_problem(made) is not None:
        wrong.append("BIP-173's program makes %r here, which is not its address" % made)
    for checker, address, want in REFUSED:
        got = checker(address)
        if got != want:
            wrong.append("%s(%r) answers %r, and should answer %r" % (checker.__name__, address, got, want))
        if got is not None and got not in REASONS_CS:
            wrong.append("no Czech line for the answer %r" % got)
    return wrong
TRACKED = ["DONATE.md", "docs/donate"]
# The branch the client's button opens the page on (`DONATION_URL` in src/gui/render.rs).
BRANCH = "master"


def grouped(address):
    return " ".join(address[i:i + 4] for i in range(0, len(address), 4))


def git(*args):
    return subprocess.run(["git", "-C", ROOT] + list(args), capture_output=True, text=True)


def repository_problem(w):
    """Why nothing may be published from this clone right now, in the user's words; None when it may.

    The client's button opens the file on `master`, so that is the only branch published from. A commit that
    waits here unpushed would go out with the page, and whoever made it has not said it is ready; and a clone
    behind GitHub is brought up to date first, so the push cannot be refused after the question was answered."""
    dirty = git("status", "--porcelain", "--", *TRACKED)
    if dirty.returncode != 0:
        return w["git"] + ": " + dirty.stderr.strip()
    if dirty.stdout.strip():
        return w["dirty"] + "\n" + dirty.stdout.rstrip()
    branch = git("rev-parse", "--abbrev-ref", "HEAD").stdout.strip()
    if branch != BRANCH:
        return w["branch"] % (BRANCH, branch)
    # Asked now, not found out at the commit: a clone with no author would take the answers and then refuse.
    if not all(git("config", "--get", key).stdout.strip() for key in ("user.name", "user.email")):
        return w["nobody"]
    fetched = git("fetch", "--quiet", "origin", BRANCH)
    if fetched.returncode != 0:
        return w["fetch"] + "\n" + fetched.stderr.strip()
    ahead = git("rev-list", "--count", "origin/%s..HEAD" % BRANCH).stdout.strip()
    if ahead != "0":
        return w["ahead"] % ahead
    behind = git("rev-list", "--count", "HEAD..origin/%s" % BRANCH).stdout.strip()
    if behind != "0":
        pulled = git("merge", "--ff-only", "--quiet", "origin/%s" % BRANCH)
        if pulled.returncode != 0:
            return w["behind"] + "\n" + (pulled.stderr or pulled.stdout).strip()
    return None


def set_addresses(lang):
    w = WORDS[lang]
    print(w["intro"] + "\n\n" + w["clipboard"] + "\n")
    stopped = repository_problem(w)
    if stopped:
        print(stopped)
        return 1
    before = read_disk()
    current = page_addresses(before["DONATE.md"] or "")
    chosen = dict(current)
    for key, _, _, problem in COINS:
        print("%s\n   %s: %s" % (w[key], w["now"], grouped(current.get(key, "-"))))
        while True:
            typed = "".join(input("   %s: " % w["ask"]).split())
            if not typed and key in current:
                print("   " + w["kept"])
                break
            wrong = problem(typed)
            if wrong is None:
                chosen[key] = typed
                break
            print("   %s: %s" % (w["refused"], REASONS_CS.get(wrong, wrong) if lang == "cs" else wrong))
        print()
    if chosen == current:
        print(w["nothing"])
        return 0
    files = render(chosen)
    problems = verify(chosen, files)
    if problems:
        print(w["problems"])
        for p in problems:
            print("  ", p)
        return 1
    print("=" * 78 + "\n" + w["summary"] + "\n" + "=" * 78)
    for key, _, _, _ in COINS:
        print(w[key])
        if chosen[key] == current.get(key):
            print("   %-9s %s" % (w["same"] + ":", grouped(chosen[key])))
        else:
            print("   %-9s %s" % (w["was"] + ":", grouped(current.get(key, "-"))))
            print("   %-9s %s" % (w["will"] + ":", grouped(chosen[key])))
    print("\n" + w["verified"] + "\n")
    if input(w["confirm"] + ": ").strip() != w["yes"]:
        print(w["left"])
        return 0
    write_disk(files)
    steps = [
        ("add", "--", *TRACKED),
        ("commit", "-m", "Donation addresses changed by tools/donation-page.py --set", "--", *TRACKED),
        ("push", "origin", BRANCH),
    ]
    for number, step in enumerate(steps):
        done = git(*step)
        if done.returncode != 0:
            print(w["git"] + " (git %s):\n%s" % (step[0], (done.stderr or done.stdout).strip()))
            # Everything as it was, the commit included: a commit left here unpushed would stop the next run,
            # and a page changed on this disk alone is a page nobody can check against GitHub.
            if number == 2:
                git("reset", "-q", "--soft", "HEAD~1")
            git("reset", "-q", "--", *TRACKED)
            write_disk({k: v for k, v in before.items() if v is not None})
            for rel in (k for k, v in before.items() if v is None):
                path = os.path.join(ROOT, *rel.split("/"))
                if os.path.exists(path):
                    os.remove(path)
            print(w["restored"])
            return 1
    here = git("rev-parse", "HEAD").stdout.strip()
    there = git("ls-remote", "origin", "refs/heads/%s" % BRANCH).stdout.split()
    if not there or there[0] != here:
        print(w["unseen"])
        return 1
    remote = re.sub(r"\.git$", "", git("remote", "get-url", "origin").stdout.strip())
    print(w["done"] + "\n   %s/blob/%s/DONATE.md" % (remote, BRANCH))
    return 0


def main():
    args = sys.argv[1:]
    lang = "cs" if "--lang" in args and args[args.index("--lang") + 1:][:1] == ["cs"] else "en"
    if "--check" in args:
        return check()
    if "--selftest" in args:
        return selftest()
    if "--write" in args:
        addresses = page_addresses((read_disk()["DONATE.md"] or ""))
        if len(addresses) != len(COINS):
            print("DONATE.md holds no addresses yet: use --set, or --first ADDRESS ADDRESS")
            return 1
        files = render(addresses)
        problems = verify(addresses, files)
        for p in problems:
            print("WRONG:", p)
        if problems:
            return 1
        write_disk(files)
        return check()
    if "--first" in args:
        given = args[args.index("--first") + 1:][:2]
        addresses = dict(zip([c[0] for c in COINS], given))
        files = render(addresses) if len(given) == 2 and not any(c[3](addresses[c[0]]) for c in COINS) else None
        problems = verify(addresses, files or {}) if files else ["two valid addresses are wanted: Bitcoin, then TRON"]
        for p in problems:
            print("WRONG:", p)
        if problems:
            return 1
        write_disk(files)
        return check()
    if "--set" in args:
        try:
            return set_addresses(lang)
        except (KeyboardInterrupt, EOFError):
            print("\n" + WORDS[lang]["left"])
            return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
