//! `S1-IT`: a failed dial, said in a line a person can read.
//!
//! `DialError`'s own words are every address that was tried with the whole
//! chain of what went wrong at it -- two kilobytes for one peer behind a relay,
//! which is why the client keeps the first twelve of a run and counts the rest
//! (`app::AppState`). The window that leaves is the first thirteen seconds.
//! `split190546-9` needed minute seven: a seat and its founder dialled each
//! other for 420 s, every dial failed, and after the twelfth line nothing said
//! how.
//!
//! So the failed dial of a peer **that matters** -- the founder of the table
//! being joined, a poker client the lobby named -- is said every time, folded:
//! how many addresses, through which relay, and what each road ended in, by
//! kind. No address is printed; a relay is named by the end of its id.

use libp2p::multiaddr::Protocol;
use libp2p::swarm::DialError;
use libp2p::{Multiaddr, PeerId, TransportError};
use std::collections::BTreeMap;

/// The end of a peer id, which is where two ids differ.
pub fn short(peer: &PeerId) -> String {
    let s = peer.to_string();
    s[s.len().saturating_sub(6)..].to_owned()
}

/// What one road ended in, by kind. The words are matched in the error's whole
/// chain, because `libp2p` nests the relay's answer two causes deep.
fn kind(chain: &str) -> &'static str {
    let c = chain.to_ascii_lowercase();
    if c.contains("no reservation") {
        "the relay holds no reservation for it"
    } else if c.contains("canceled") {
        "given up before the relay was reached"
    } else if c.contains("resource limit") {
        "the relay is at its limit"
    } else if c.contains("failed to connect to destination") {
        "the relay could not connect to it"
    } else if c.contains("unsupported") || c.contains("not supported") {
        "no transport for the address"
    } else if c.contains("refused") {
        "refused"
    } else if c.contains("timed out") || c.contains("timeout") {
        "timed out"
    } else if c.contains("reset") || c.contains("aborted") || c.contains("10054") || c.contains("10053") {
        "cut off by the far end"
    } else if c.contains("multistream") || c.contains("negotiat") {
        "no protocol in common"
    // The system's verdict where only its WORDS came through -- a library that
    // flattened the error into its own text. The words are in the player's
    // language; the number beside them is not.
    } else if ["os error 10051)", "os error 10065)", "os error 101)", "os error 113)"].iter().any(|n| c.contains(n)) {
        "no route to the address"
    } else if c.contains("os error 10060)") || c.contains("os error 110)") {
        "timed out"
    } else if c.contains("os error 10061)") || c.contains("os error 111)") {
        "refused"
    } else {
        "failed otherwise"
    }
}

/// The system's own verdict on a road, where there is one in the chain.
///
/// **By kind and by number, never by words**: the system says them in the
/// player's language. The bed's first run with this module filed 705 of 969
/// failed dials under *failed otherwise*, and the raw lines beside them read
/// *Stavajici pripojeni bylo vynucene ukonceno vzdalenym hostitelem* -- a reset,
/// in Czech, which no list of English words will ever match.
fn system_kind(e: &TransportError<std::io::Error>) -> Option<&'static str> {
    use std::io::ErrorKind as K;
    let TransportError::Other(top) = e else { return None };
    let mut next: Option<&(dyn std::error::Error + 'static)> = Some(top);
    while let Some(err) = next {
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            let by_kind = match io.kind() {
                K::ConnectionRefused => Some("refused"),
                K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe | K::UnexpectedEof => Some("cut off by the far end"),
                K::TimedOut => Some("timed out"),
                K::AddrNotAvailable => Some("no route to the address"),
                _ => None,
            };
            // Winsock's and errno's numbers for *network* and *host unreachable*,
            // which not every toolchain gives a kind of their own.
            let by_number = match io.raw_os_error() {
                Some(10051 | 10065 | 101 | 113) => Some("no route to the address"),
                Some(10060 | 110) => Some("timed out"),
                Some(10061 | 111) => Some("refused"),
                Some(10053 | 10054 | 104) => Some("cut off by the far end"),
                _ => None,
            };
            if let Some(k) = by_kind.or(by_number) {
                return Some(k);
            }
            next = io.get_ref().map(|inner| inner as &(dyn std::error::Error + 'static)).or_else(|| err.source());
        } else {
            next = err.source();
        }
    }
    None
}

fn chain(e: &TransportError<std::io::Error>) -> String {
    let mut out = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    out
}

/// The relay a circuit address goes through, if it is one.
fn relay(addr: &Multiaddr) -> Option<PeerId> {
    let mut last = None;
    for p in addr.iter() {
        match p {
            Protocol::P2p(id) => last = Some(id),
            Protocol::P2pCircuit => return last,
            _ => {}
        }
    }
    None
}

/// The failed dial in one line.
pub fn fold(error: &DialError) -> String {
    match error {
        DialError::NoAddresses => "no address is held for it".to_owned(),
        DialError::DialPeerConditionFalse(_) => "a dial of it was already under way".to_owned(),
        DialError::Denied { .. } => "refused by this client's own connection limit".to_owned(),
        DialError::Aborted => "the dial was given up".to_owned(),
        DialError::LocalPeerId { .. } => "the address is this client's own".to_owned(),
        DialError::WrongPeerId { .. } => "somebody else answered at its address".to_owned(),
        DialError::Transport(roads) => {
            // Relay (or none, for a road straight to it) -> kind -> how many.
            let mut by: BTreeMap<Option<String>, BTreeMap<&'static str, usize>> = BTreeMap::new();
            for (addr, e) in roads {
                // The libraries' words first -- a relay's answer arrives wrapped in
                // an error of no kind -- and the system's verdict where they say nothing.
                let k = match kind(&chain(e)) {
                    "failed otherwise" => system_kind(e).unwrap_or("failed otherwise"),
                    said => said,
                };
                *by.entry(relay(addr).map(|r| short(&r))).or_default().entry(k).or_default() += 1;
            }
            let said: Vec<String> = by
                .into_iter()
                .map(|(via, kinds)| {
                    let kinds: Vec<String> = kinds.into_iter().map(|(k, n)| format!("{n} {k}")).collect();
                    match via {
                        Some(r) => format!("through relay ..{r}: {}", kinds.join(", ")),
                        None => format!("directly: {}", kinds.join(", ")),
                    }
                })
                .collect();
            format!("{} address(es); {}", roads.len(), said.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Multiaddr {
        s.parse().expect("a literal")
    }

    fn other(words: &str) -> TransportError<std::io::Error> {
        TransportError::Other(std::io::Error::other(words.to_owned()))
    }

    /// The dial `split190546-9` kept, as the line it would have been: three
    /// roads through one relay, two given up and one answered by the relay.
    ///
    /// The break that must make this fail: read a road's relay as the LAST
    /// peer id of the address, which is the peer being dialled.
    #[test]
    fn a_dial_through_a_relay_is_folded_by_relay_and_by_kind() {
        let relay = "12D3KooWLZ18MNzm9xu7GZRQ16c6k3CZLN529CbUqrYaWNqyYko9";
        let player = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";
        let through = |leg: &str| a(&format!("{leg}/p2p/{relay}/p2p-circuit/p2p/{player}"));
        let error = DialError::Transport(vec![
            (through("/ip4/147.75.87.27/tcp/4001"), other("Response from behaviour was canceled: oneshot canceled")),
            (through("/ip4/147.75.87.27/udp/4001/quic-v1/webtransport"), other("Response from behaviour was canceled: oneshot canceled")),
            (through("/ip4/147.75.87.27/udp/4001/quic-v1"), other("Failed to connect to destination.: Relay has no reservation for destination")),
            (a(&format!("/ip4/1.1.1.1/tcp/4001/p2p/{player}")), other("Timeout has been reached")),
            (a("/ip4/1.1.1.1/udp/4001/webrtc-direct"), TransportError::MultiaddrNotSupported(a("/ip4/1.1.1.1/udp/4001/webrtc-direct"))),
        ]);
        assert_eq!(
            fold(&error),
            "5 address(es); directly: 1 no transport for the address, 1 timed out; \
             through relay ..qyYko9: 2 given up before the relay was reached, 1 the relay holds no reservation for it"
        );
        // No address and no whole id: the line goes into a log a player may post.
        assert!(!fold(&error).contains("147.75") && !fold(&error).contains(relay));
    }

    /// The three refusals `Swarm::dial` returns by value, which leave no event
    /// behind them, each in its own words: they are what tells *this client
    /// holds no address for the founder* from *a dial is on its way*.
    #[test]
    fn the_refusals_that_never_leave_this_machine_have_their_own_words() {
        assert_eq!(fold(&DialError::NoAddresses), "no address is held for it");
        assert_eq!(
            fold(&DialError::DialPeerConditionFalse(libp2p::swarm::dial_opts::PeerCondition::DisconnectedAndNotDialing)),
            "a dial of it was already under way"
        );
        assert_eq!(fold(&DialError::Aborted), "the dial was given up");
    }

    /// The system's verdict is read by its kind and its number: its words are in
    /// the player's language. The reset below is said as the bed's machine says it.
    ///
    /// The break that must make this fail: read the words only.
    #[test]
    fn the_systems_verdict_is_read_by_kind_and_number_not_by_its_words() {
        let player = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";
        let at = |n: u8| a(&format!("/ip4/1.1.1.{n}/tcp/4001/p2p/{player}"));
        let wrapped = |inner: std::io::Error| TransportError::Other(std::io::Error::other(inner));
        let error = DialError::Transport(vec![
            (at(1), TransportError::Other(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "Stavajici pripojeni bylo vynucene ukonceno vzdalenym hostitelem."))),
            (at(2), wrapped(std::io::Error::from_raw_os_error(10051))),
            (at(3), wrapped(std::io::Error::from_raw_os_error(10060))),
            (at(4), wrapped(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "Pripojeni odmitnuto"))),
            (at(5), other("slova, ktera nikdo nezna")),
        ]);
        assert_eq!(
            fold(&error),
            "5 address(es); directly: 1 cut off by the far end, 1 failed otherwise, 1 no route to the address, 1 refused, 1 timed out"
        );
    }

    /// Every kind a run has shown, by the words `libp2p` and the system use.
    #[test]
    fn the_kinds_are_read_from_the_words_the_libraries_use() {
        for (words, want) in [
            ("Relay has no reservation for destination", "the relay holds no reservation for it"),
            ("Response from behaviour was canceled: oneshot canceled", "given up before the relay was reached"),
            ("Failed to connect to destination.", "the relay could not connect to it"),
            ("Resource limit exceeded", "the relay is at its limit"),
            ("Unsupported resolved address: /ip4/1.1.1.1/udp/1/webrtc-direct", "no transport for the address"),
            ("Connection refused (os error 10061)", "refused"),
            ("Timeout has been reached", "timed out"),
            ("aborted by peer: the server refused to accept a new connection", "refused"),
            ("An existing connection was forcibly closed by the remote host. (os error 10054)", "cut off by the far end"),
            ("Multistream select failed: Protocol negotiation failed.", "no protocol in common"),
            ("Doslo k pokusu o operaci se soketem v okamziku nedosazitelnosti site. (os error 10051)", "no route to the address"),
            ("Pokus o pripojeni selhal, protoze pripojena strana neodpovedela. (os error 10060)", "timed out"),
            ("something nobody has seen", "failed otherwise"),
        ] {
            assert_eq!(kind(words), want, "{words}");
        }
    }
}
