//! `D-071`: the donation page the client opens, held to the repository that
//! publishes it.
//!
//! # What this exists for
//!
//! The lobby's *Support the project* button opens one address,
//! `gui::render::DONATION_URL`, and on the page behind it are two addresses a
//! stranger sends money to. A character wrong in either is somebody's gift gone
//! to nobody, for good, and nobody finds out. `tools/donation-page.py` is the
//! one way those addresses are meant to change, and it refuses a bad one -- but
//! `DONATE.md` is a text file, and a text file gets edited. So the checksums
//! are checked again here, in another language, by code that shares nothing
//! with the tool's, on every test run and on a machine that has no Python.
//!
//! # What it checks
//!
//! * the link is `https`, to GitHub, to `DONATE.md` on the branch the tool
//!   publishes from -- and that file is in this repository;
//! * the page holds one address between each pair of markers, the Bitcoin one a
//!   mainnet native SegWit address under bech32 or bech32m (BIP-173, BIP-350)
//!   and the TRON one 21 bytes beginning `0x41` under Base58Check;
//! * every picture the page shows is a file that is here, and each QR code's
//!   file is titled with exactly what the page says it encodes;
//! * its own checkers refuse a wrong character, a test network's address and the
//!   wrong kind of address -- or the checks above would prove nothing.
//!
//! # What it does not check
//!
//! That a QR code's modules really say its title. Reading a QR code takes a
//! decoder, and the one that does it is OpenCV's, in
//! `python tools/donation-page.py --check`, which also draws every file again
//! and compares. The title held here catches the edit that tool cannot be
//! asked about: an address changed by hand in the page and not in the picture.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn page() -> String {
    fs::read_to_string(root().join("DONATE.md")).expect("DONATE.md is in the repository")
}

/// The one token between a pair of the tool's markers.
fn between_markers(page: &str, key: &str) -> String {
    let (open, close) = (format!("<!-- donation:{key} -->"), format!("<!-- /donation:{key} -->"));
    assert_eq!(page.matches(&open).count(), 1, "{key}: one opening marker");
    assert_eq!(page.matches(&close).count(), 1, "{key}: one closing marker");
    let inside = &page[page.find(&open).unwrap() + open.len()..page.find(&close).unwrap()];
    let words: Vec<&str> = inside.split_whitespace().filter(|w| *w != "```").collect();
    assert_eq!(words.len(), 1, "{key}: exactly one address between the markers, found {words:?}");
    words[0].to_owned()
}

// ---------------------------------------------------------------------------
// bech32 and bech32m, from BIP-173 and BIP-350
// ---------------------------------------------------------------------------

const BECH32: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BECH32M: u32 = 0x2bc8_30a3;

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a_57b2, 0x2650_8e6d, 0x1ea1_19fa, 0x3d42_33dd, 0x2a14_62b3];
    let mut chk: u32 = 1;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01ff_ffff) << 5) ^ u32::from(*v);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

/// `Ok((witness version, witness program))` for a Bitcoin **mainnet** native
/// SegWit address, else why not.
fn bitcoin(addr: &str) -> Result<(u8, Vec<u8>), &'static str> {
    if addr.chars().any(|c| c.is_ascii_uppercase()) && addr.chars().any(|c| c.is_ascii_lowercase()) {
        return Err("mixed case");
    }
    let addr = addr.to_ascii_lowercase();
    let at = addr.rfind('1').ok_or("no separator")?;
    if at < 1 || at + 7 > addr.len() || addr.len() > 90 {
        return Err("not the shape of a bech32 string");
    }
    let (hrp, rest) = (&addr[..at], &addr[at + 1..]);
    if hrp != "bc" {
        return Err("not the main network's prefix");
    }
    let mut data = Vec::with_capacity(rest.len());
    for c in rest.bytes() {
        data.push(u8::try_from(BECH32.iter().position(|b| *b == c).ok_or("a character outside bech32")?).unwrap());
    }
    let mut values: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    values.push(0);
    values.extend(hrp.bytes().map(|b| b & 31));
    values.extend(&data);
    let constant = polymod(&values);
    if constant != 1 && constant != BECH32M {
        return Err("the checksum does not hold");
    }
    let version = data[0];
    // Five bits a character into eight a byte, with no padding left over but zeros.
    let (mut acc, mut bits, mut program) = (0u32, 0u32, Vec::new());
    for v in &data[1..data.len() - 6] {
        acc = (acc << 5) | u32::from(*v);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            program.push(((acc >> bits) & 0xff) as u8);
        }
    }
    if bits >= 5 || (acc << (8 - bits)) & 0xff != 0 {
        return Err("padding that is not zero");
    }
    if version > 16 || program.len() < 2 || program.len() > 40 {
        return Err("not a witness program");
    }
    match (version, constant) {
        (0, 1) if program.len() == 20 || program.len() == 32 => Ok((version, program)),
        (0, _) => Err("version 0 of the wrong length, or under bech32m"),
        (_, BECH32M) => Ok((version, program)),
        _ => Err("version 1 or above under bech32"),
    }
}

// ---------------------------------------------------------------------------
// Base58Check
// ---------------------------------------------------------------------------

const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// `Ok(the 21 bytes)` for a TRON main network address, else why not.
fn tron(addr: &str) -> Result<Vec<u8>, &'static str> {
    let mut bytes: Vec<u8> = Vec::new();
    for c in addr.bytes() {
        let mut carry = u32::try_from(BASE58.iter().position(|b| *b == c).ok_or("a character outside base58")?).unwrap();
        for b in bytes.iter_mut() {
            carry += u32::from(*b) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    bytes.extend(addr.bytes().take_while(|c| *c == b'1').map(|_| 0));
    bytes.reverse();
    if bytes.len() != 25 {
        return Err("not 25 bytes");
    }
    let (body, check) = bytes.split_at(21);
    let twice = Sha256::digest(Sha256::digest(body));
    if twice.as_slice()[..4] != *check {
        return Err("the checksum does not hold");
    }
    if body[0] != 0x41 {
        return Err("not the TRON main network's first byte");
    }
    Ok(body.to_vec())
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[test]
fn the_button_opens_the_page_this_repository_holds() {
    let url = p2p_poker::gui::render::DONATION_URL;
    let path = url.strip_prefix("https://github.com/").expect("https, and GitHub");
    let parts: Vec<&str> = path.split('/').collect();
    assert_eq!(parts.len(), 5, "owner, repository, blob, branch, file: {url}");
    assert_eq!(parts[2], "blob");
    assert_eq!(parts[4], "DONATE.md");
    assert!(root().join(parts[4]).is_file(), "the file the link names is in the repository");

    // The tool publishes from one branch, and the link must name the same one,
    // or an address changed by the tool is not the address the button shows.
    let tool = fs::read_to_string(root().join("tools/donation-page.py")).expect("the tool");
    assert!(
        tool.contains(&format!("\nBRANCH = \"{}\"\n", parts[3])),
        "tools/donation-page.py publishes from another branch than {}",
        parts[3]
    );
}

#[test]
fn the_pages_addresses_pass_their_checksums() {
    let page = page();
    let address = between_markers(&page, "bitcoin");
    let (version, program) = bitcoin(&address).unwrap_or_else(|why| panic!("the Bitcoin address {address}: {why}"));
    // What the page says beside it is what it is.
    let kind = match (version, program.len()) {
        (0, 20) => "native SegWit, P2WPKH",
        (0, 32) => "native SegWit, P2WSH",
        (1, _) => "Taproot",
        _ => panic!("the Bitcoin address {address} is of a kind the page has no words for"),
    };
    assert!(page.contains(kind), "the page does not call its Bitcoin address {kind}");

    let address = between_markers(&page, "tether-tron");
    tron(&address).unwrap_or_else(|why| panic!("the TRON address {address}: {why}"));
}

#[test]
fn the_checkers_refuse_what_they_must() {
    // The BIPs' own vectors, a published TRON address, and the page's two: taken.
    let page = page();
    let (ours, ours_tron) = (between_markers(&page, "bitcoin"), between_markers(&page, "tether-tron"));
    let (_, program) = bitcoin("BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4").expect("BIP-173's P2WPKH vector");
    let hex: String = program.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, "751e76e8199196d454941c45d1b3a323f1433bd6", "BIP-173's own witness program");
    bitcoin("bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3").expect("BIP-173's P2WSH vector");
    bitcoin("bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0").expect("BIP-350's Taproot vector");
    tron("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t").expect("the USDT contract's address on TRON");

    // Refused: a test network, BIP-350's invalid vectors, the wrong chain's address.
    assert!(bitcoin("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx").is_err(), "a testnet address");
    assert!(bitcoin("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kemeawh").is_err(), "version 0 under bech32m");
    assert!(bitcoin("bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqh2y7hd").is_err(), "version 1 under bech32");
    assert!(bitcoin("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kV8f3t4").is_err(), "mixed case");
    assert!(tron("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").is_err(), "a Bitcoin address where TRON's belongs");
    assert!(bitcoin(&ours_tron).is_err() && tron(&ours).is_err(), "the page's two, each given to the other's checker");

    // Every single wrong character in the page's own two addresses is refused.
    // bech32 promises that outright; Base58Check's four bytes miss one change in
    // four thousand million, and none of these.
    for (address, alphabet, keep, check) in [
        (&ours, BECH32, 3usize, (|a: &str| bitcoin(a).is_ok()) as fn(&str) -> bool),
        (&ours_tron, BASE58, 0usize, (|a: &str| tron(a).is_ok()) as fn(&str) -> bool),
    ] {
        let mut tried = 0;
        for at in keep..address.len() {
            for c in alphabet {
                if address.as_bytes()[at] == *c {
                    continue;
                }
                let mut wrong = address.clone().into_bytes();
                wrong[at] = *c;
                let wrong = String::from_utf8(wrong).unwrap();
                assert!(!check(&wrong), "{wrong} differs from {address} in one character and was taken");
                tried += 1;
            }
        }
        assert!(tried > 1_000, "{tried} wrong characters tried in {address}");
    }
}

#[test]
fn the_page_shows_files_that_are_here_and_each_code_is_titled_with_what_it_says() {
    let page = page();
    let mut shown = 0;
    for part in page.split("src=\"").skip(1) {
        let file = &part[..part.find('"').expect("a closing quote")];
        assert!(!file.contains("://"), "the page shows {file}, which is not in this repository");
        assert!(root().join(file).is_file(), "the page shows {file}, which is not here");
        shown += 1;
    }
    assert_eq!(shown, 4, "two logos and two QR codes");

    for (key, file, says) in [
        ("bitcoin", "docs/donate/qr-bitcoin.svg", "bitcoin:"),
        ("tether-tron", "docs/donate/qr-tether-tron.svg", ""),
    ] {
        let want = format!("<title>{says}{}</title>", between_markers(&page, key));
        let svg = fs::read_to_string(root().join(file)).expect("the QR code's file");
        assert!(page.contains(file), "the page does not show {file}");
        assert!(
            svg.contains(&want),
            "{file} is not titled {want}: the address was changed in the page and not in the picture -- \
             python tools/donation-page.py --set changes both"
        );
    }
}
