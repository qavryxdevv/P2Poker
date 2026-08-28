# Phase 0 integration check: does the whole stack actually coexist?

Date: 2026-08-28
Machine: Windows 10, x86_64-pc-windows-msvc, 24 cores, rustc 1.95.0 / cargo 1.95.0

Seven research documents each verified their own slice by compiling. None of
them verified that the slices **build together**. The arkworks, dalek and rand
ecosystems are exactly where that breaks, so this was checked before writing any
code on top.

---

## 1. Result: the full stack compiles

`Cargo.toml` at the repository root declares every recommendation from
`LIBP2P.md`, `MAINLINE_DHT.md`, `MENTAL_POKER.md`, `CRYPTO_LIBS.md` and
`GUI_STACK.md` simultaneously.

```
$ cargo check
    Checking libp2p-relay v0.21.1
    Checking libp2p-stream v0.4.0-alpha
    Checking ark-secp256k1 v0.5.0
    Checking ziffle v0.1.0
    Checking eframe v0.36.1
    Checking libp2p v0.56.0
    Checking p2p-poker v0.1.0
    Finished `dev` profile in 1m 05s
$ echo $?
0
```

**425 crates** are actually compiled (`cargo tree --edges normal`, unique
name+version pairs). `Cargo.lock` records 611, because a lockfile is
feature-independent — see §3, this distinction matters.

425 is a lot against `SPEC_CS.md` section 28's "minimum number of
dependencies". It is what libp2p, arkworks and egui cost together, and the
number is recorded here rather than glossed over. Roughly: libp2p and its
transports dominate, arkworks arrives with `ziffle`, and egui/eframe brings the
windowing and font stack.

---

## 2. Duplicate major versions, and why they are not a defect

Several security-critical crates appear at two or three majors at once:

| Crate | Versions present |
|---|---|
| `curve25519-dalek` | 4.1.3, 5.0.0 |
| `ed25519-dalek` | 2.2.0, 3.0.0 |
| `ed25519` | 2.2.3, 3.0.0 |
| `signature` | 2.2.0, 3.0.0 |
| `sha2` | 0.10.9, 0.11.0 |
| `digest` | 0.10.7, 0.11.3 |
| `rand` | 0.8.8, 0.9.5, 0.10.2 |
| `rand_core` | 0.6.4, 0.9.5, 0.10.1 |
| `getrandom` | 0.2.17, 0.3.4, 0.4.3 |

The older majors belong to `libp2p-identity`; the newer ones are what our own
code was pinned to in `CRYPTO_LIBS.md`. They coexist because Cargo treats
different majors as different crates.

This is not accidental damage — it enforces something the spec already wants.
`SPEC_CS.md` section 20 requires the libp2p `PeerId` and the poker application
identity to be **separate identities**, and warns that a `PeerId` says which
socket you are talking to, not who is playing. Because the two Ed25519 types
come from different crate majors, they are different Rust types, and the
compiler will refuse any accidental attempt to use one where the other belongs.
The separation is enforced mechanically rather than by discipline.

### One research claim is corrected here

`CRYPTO_LIBS.md` section 10 states, of its dependency block, "No `rand` at any
depth." That was true of its isolated probe. **It is false in the integrated
tree**: `rand` 0.8.8, 0.9.5 and 0.10.2 are all present, pulled in by
`libp2p-autonat` and by the arkworks crates under `ziffle`.

The claim cannot be met and should not be restated. What survives is the
discipline behind it, which is what `SPEC_CS.md` section 7 actually requires:

> Our own code draws cryptographic randomness only from `getrandom::SysRng`.
> `rand`'s `SmallRng`, `StdRng` and any self-seeded generator are never used
> for keys, masking factors, permutations or commitments.

That is a lint we can enforce, unlike an absence we cannot.

---

## 3. `cargo audit`: two vulnerabilities, and a tooling subtlety

```
Scanning Cargo.lock for vulnerabilities (611 crate dependencies)

hickory-proto 0.25.2  RUSTSEC-2026-0118  NSEC3 closest-encloser proof validation
                                         enters unbounded loop on cross-zone responses
                                         -> no fixed upgrade available
hickory-proto 0.25.2  RUSTSEC-2026-0119  CPU exhaustion during message encoding,
                                         O(n^2) name compression -> fixed in >= 0.26.1
paste 1.0.15          RUSTSEC-2024-0436  unmaintained (warning)
lru 0.16.4            RUSTSEC-2026-0253  unsound: potential use-after-free from
                                         missing panic safety in LruCache::pop() (warning)

error: 2 vulnerabilities found!
```

Provenance:

```
hickory-proto <- hickory-resolver <- libp2p-dns <- libp2p   (the "dns" feature)
lru           <- mainline
```

Both hickory advisories are denial-of-service in DNS handling, reachable only
by an attacker who can shape the DNS responses we receive. `libp2p` 0.56.0 pins
`hickory-resolver` 0.25.2, so we cannot upgrade past it without libp2p moving
first, and RUSTSEC-2026-0118 has no fix at any version.

### Dropping the `dns` feature removes them from the build

Verified: with `"dns"` removed from the libp2p feature list, `cargo check`
still succeeds, the compiled tree drops from 425 to 413 crates, and

```
$ cargo tree --edges normal -i hickory-proto
warning: nothing to print.
```

hickory is no longer compiled at all.

**But `cargo audit` still reports it.** A `Cargo.lock` is feature-independent —
it records the union of all optional dependencies in the resolved graph — and
`cargo audit` reads the lockfile, not the build graph. It has no feature
awareness. So the advisory count does not change when the code does.

This is worth knowing before anyone treats a clean `cargo audit` as evidence:
here it over-reports, and `cargo tree --edges normal` is the tool that answers
what is actually in the binary. `SPEC_CS.md` section 28 asks for a security
status per dependency, and "flagged by audit but not compiled" is a different
status from "compiled and vulnerable".

### What was decided, and what was not

The `dns` feature is **kept for now**. Removing it would quietly narrow D-004:
layers 2 and 3 of the all-NAT story lean on reaching public relays, and the
usual bootstrap addresses for those are `/dnsaddr/` names. Silently dropping
that capability to make a scanner quiet is the wrong trade to make unilaterally.

The alternative is real and attractive, and is recorded as an open decision:
run **all** discovery through Mainline DHT — players under `LOBBY_INFOHASH`,
relay volunteers under a second infohash — which would let both `dns` and `kad`
go, remove both vulnerabilities from the build, shrink the tree further, and
leave one discovery mechanism instead of two. Its cost is losing access to the
public relay commons, which makes D-004's floor thinner while the user base is
small.

---

## 4. Reproducing this

```bash
cargo check                                    # whole stack
cargo tree --edges normal --prefix none | sed 's/ (\*)//' \
  | awk '{print $1" "$2}' | sort -u | wc -l    # crates actually built
cargo tree --duplicates --edges normal         # duplicate majors
cargo audit                                    # lockfile-based, over-reports
cargo tree --edges normal -i <crate>           # is it really in the build?
```
