//! `S1-IT`: which of the addresses a DHT record names this client can dial at
//! all, and what those records say of the relays they go through.
//!
//! A player behind a router is found in the public lobby by a relay's circuit
//! address, and a public relay has many addresses: TCP, QUIC, WebTransport,
//! WebRTC, secure WebSocket, each over IPv4, IPv6 and a name -- ten to fifteen
//! of them. This client speaks **TCP and QUIC**. The rest it cannot dial, and
//! in a circuit address they do worse than fail:
//!
//! `libp2p-relay`'s client takes one *dial request* for every circuit address
//! of a peer. For the first it dials the relay -- **at the one address that
//! request names** -- and every other request for the same relay is refused
//! while that dial is under way (*a dial is already in progress*) and dropped.
//! So the whole dial of the peer lives or dies by whichever of the relay's
//! addresses came first in the record: if that is its WebTransport address,
//! the relay is never reached and all fifteen attempts end *Response from
//! behaviour was canceled*. Measured, `split190546-9`: five seats' logs hold
//! the failed dial of one relay at its WebTransport address, *Unsupported
//! resolved address*, beside fifteen cancelled circuit attempts through it --
//! at a relay that every one of them could reach over TCP.
//!
//! Two things are done about it, both where the DHT's addresses pass on their
//! way to a dial (`swarm::Bogonless`):
//!
//! * an address with a transport this client lacks is **not offered**
//!   ([`lacks_transport`]), so the first dial request is one that can work;
//! * the relay's own addresses, read out of the circuit addresses that name it,
//!   are **remembered** ([`RelayBook`]) and offered when the relay itself is
//!   dialled -- so the relay is tried at every address this client can speak
//!   to, whichever request came first.
//!
//! Nothing here touches the swarm; this is the part that can be wrong in a test.

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use std::collections::{HashMap, VecDeque};

/// The most relays remembered. A lobby of NATed players names a relay each; the
/// oldest is forgotten first.
pub const RELAYS_REMEMBERED_MAX: usize = 512;

/// The most addresses remembered of one relay. A relay this client can dial has
/// four -- TCP and QUIC over two address families -- and a name or two.
pub const RELAY_ADDRESSES_MAX: usize = 8;

/// Whether an address needs a transport this client was not built with.
///
/// Judged by what the address **carries**, not by what it lacks: an address
/// this does not recognise is kept, as `not_a_bogon` keeps what it cannot
/// judge. For a circuit address it is the relay's leg that is read; what comes
/// after `p2p-circuit` is only the peer.
pub fn lacks_transport(addr: &Multiaddr) -> bool {
    addr.iter().take_while(|p| !matches!(p, Protocol::P2pCircuit)).any(|p| {
        matches!(
            p,
            Protocol::Ws(_)
                | Protocol::Wss(_)
                | Protocol::Tls
                | Protocol::WebTransport
                | Protocol::WebRTCDirect
                | Protocol::Certhash(_)
                // QUIC's draft-29, which `libp2p-quic` speaks only when asked to.
                | Protocol::Quic
                | Protocol::Http
                | Protocol::Https
        )
    })
}

/// The relay a circuit address goes through, and that relay's own address --
/// the part before its `/p2p/<relay>`. `None` for an address that is no circuit
/// or names no relay.
pub fn relay_of(addr: &Multiaddr) -> Option<(PeerId, Multiaddr)> {
    let mut leg = Multiaddr::empty();
    let mut relay = None;
    for p in addr.iter() {
        match p {
            Protocol::P2pCircuit => return relay.filter(|_| !leg.is_empty()).map(|r| (r, leg)),
            Protocol::P2p(id) => relay = Some(id),
            other => {
                // Anything after a peer id and before the circuit is no relay's leg.
                if relay.is_some() {
                    return None;
                }
                leg.push(other);
            }
        }
    }
    None
}

/// The relays the DHT's records have named, and where each can be dialled.
#[derive(Debug, Default)]
pub struct RelayBook {
    relays: HashMap<PeerId, Vec<Multiaddr>>,
    order: VecDeque<PeerId>,
}

impl RelayBook {
    /// Read the relays out of the addresses offered for a dial. Only what this
    /// client could dial is worth remembering, so the caller filters first.
    pub fn learn<'a>(&mut self, offered: impl IntoIterator<Item = &'a Multiaddr>) {
        for (relay, leg) in offered.into_iter().filter_map(relay_of) {
            if !self.relays.contains_key(&relay) {
                if self.order.len() >= RELAYS_REMEMBERED_MAX {
                    if let Some(oldest) = self.order.pop_front() {
                        self.relays.remove(&oldest);
                    }
                }
                self.order.push_back(relay);
            }
            let known = self.relays.entry(relay).or_default();
            if known.len() < RELAY_ADDRESSES_MAX && !known.contains(&leg) {
                known.push(leg);
            }
        }
    }

    /// Where a peer can be dialled, if it is a relay a record has named.
    pub fn addresses_of(&self, peer: &PeerId) -> &[Multiaddr] {
        self.relays.get(peer).map_or(&[], Vec::as_slice)
    }

    /// Whether a record has named this peer as somebody's relay.
    pub fn knows(&self, peer: &PeerId) -> bool {
        self.relays.contains_key(peer)
    }

    pub fn len(&self) -> usize {
        self.relays.len()
    }

    pub fn is_empty(&self) -> bool {
        self.relays.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Multiaddr {
        s.parse().expect("a literal")
    }

    const RELAY: &str = "12D3KooWLZ18MNzm9xu7GZRQ16c6k3CZLN529CbUqrYaWNqyYko9";
    const PLAYER: &str = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";
    const HASH: &str = "uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g";

    /// The ten shapes a public relay's reservation came back with in
    /// `split190546-9`, and which of them this client can dial: TCP and QUIC,
    /// over either family -- four of ten.
    ///
    /// The break that must make this fail: take WebTransport for QUIC (drop
    /// `Protocol::WebTransport` from the list).
    #[test]
    fn a_relays_addresses_are_told_apart_by_what_this_client_speaks() {
        let circuit = |leg: &str| a(&format!("{leg}/p2p/{RELAY}/p2p-circuit/p2p/{PLAYER}"));
        let can = ["/ip4/147.75.87.27/tcp/4001", "/ip4/147.75.87.27/udp/4001/quic-v1", "/ip6/2606:4700:4700::1111/tcp/4001", "/ip6/2606:4700:4700::1111/udp/4001/quic-v1"];
        let cannot = [
            format!("/ip4/147.75.87.27/udp/4001/webrtc-direct/certhash/{HASH}"),
            format!("/ip4/147.75.87.27/udp/4001/quic-v1/webtransport/certhash/{HASH}/certhash/{HASH}"),
            "/ip4/147.75.87.27/udp/4001/quic-v1/webtransport".to_owned(),
            format!("/ip6/2606:4700:4700::1111/udp/4001/webrtc-direct/certhash/{HASH}"),
            "/dns4/relay.example.org/tcp/4001/tls/ws".to_owned(),
            "/dns6/relay.example.org/tcp/443/wss".to_owned(),
            "/ip4/147.75.87.27/udp/4001/quic".to_owned(),
        ];
        for leg in can {
            assert!(!lacks_transport(&circuit(leg)), "{leg} through a circuit");
            assert!(!lacks_transport(&a(&format!("{leg}/p2p/{RELAY}"))), "{leg} itself");
        }
        for leg in &cannot {
            assert!(lacks_transport(&circuit(leg)), "{leg} through a circuit");
            assert!(lacks_transport(&a(leg)), "{leg} itself");
        }
        // What it does not recognise, it keeps -- and a name is an address.
        assert!(!lacks_transport(&a("/dnsaddr/bootstrap.libp2p.io")));
        assert!(!lacks_transport(&a("/dns4/relay.example.org/tcp/4001")));
        assert!(!lacks_transport(&a("/memory/7")));
    }

    /// It is the RELAY's leg that is read. A circuit is a road to a peer whose
    /// own addresses are nobody's business here, and nothing after
    /// `p2p-circuit` is a transport this client has to speak.
    #[test]
    fn only_the_relays_leg_of_a_circuit_is_read() {
        let through_tcp = a(&format!("/ip4/147.75.87.27/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{PLAYER}"));
        assert!(!lacks_transport(&through_tcp));
        assert_eq!(relay_of(&through_tcp), Some((RELAY.parse().unwrap(), a("/ip4/147.75.87.27/tcp/4001"))));
        // A reservation request's own address: the circuit with no peer behind it.
        let listen = a(&format!("/ip4/147.75.87.27/udp/4001/quic-v1/p2p/{RELAY}/p2p-circuit"));
        assert_eq!(relay_of(&listen), Some((RELAY.parse().unwrap(), a("/ip4/147.75.87.27/udp/4001/quic-v1"))));
        // No circuit, no relay; and a circuit that names no relay is nobody's.
        assert_eq!(relay_of(&a(&format!("/ip4/147.75.87.27/tcp/4001/p2p/{RELAY}"))), None);
        assert_eq!(relay_of(&a("/ip4/147.75.87.27/tcp/4001/p2p-circuit")), None);
        assert_eq!(relay_of(&a(&format!("/p2p/{RELAY}/p2p-circuit/p2p/{PLAYER}"))), None, "a relay with no address teaches nothing");
    }

    /// The book holds each relay's addresses once, holds no more than its
    /// bounds, and forgets the oldest relay first.
    ///
    /// The break that must make this fail: remember without a bound.
    #[test]
    fn the_book_of_relays_is_bounded_and_forgets_the_oldest() {
        let mut book = RelayBook::default();
        let relay: PeerId = RELAY.parse().unwrap();
        let record = [
            a(&format!("/ip4/147.75.87.27/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{PLAYER}")),
            a(&format!("/ip4/147.75.87.27/udp/4001/quic-v1/p2p/{RELAY}/p2p-circuit/p2p/{PLAYER}")),
            a(&format!("/ip4/147.75.87.27/tcp/4001/p2p/{RELAY}/p2p-circuit/p2p/{PLAYER}")),
            a("/ip4/1.1.1.1/tcp/4001"),
        ];
        book.learn(&record);
        assert_eq!(book.addresses_of(&relay), [a("/ip4/147.75.87.27/tcp/4001"), a("/ip4/147.75.87.27/udp/4001/quic-v1")]);
        assert!(book.knows(&relay) && book.len() == 1);
        assert!(book.addresses_of(&PLAYER.parse().unwrap()).is_empty(), "the player behind the relay is no relay");

        // One relay, a hundred addresses: a record from an open DHT may say anything.
        let many: Vec<Multiaddr> = (1..=100u16).map(|p| a(&format!("/ip4/147.75.87.27/tcp/{p}/p2p/{RELAY}/p2p-circuit"))).collect();
        book.learn(&many);
        assert_eq!(book.addresses_of(&relay).len(), RELAY_ADDRESSES_MAX);

        // And more relays than the book holds: the first ones named are the ones forgotten.
        let others: Vec<(PeerId, Multiaddr)> = (0..RELAYS_REMEMBERED_MAX)
            .map(|_| {
                let id = PeerId::random();
                (id, a(&format!("/ip4/147.75.87.27/tcp/4001/p2p/{id}/p2p-circuit")))
            })
            .collect();
        book.learn(others.iter().map(|(_, addr)| addr));
        assert_eq!(book.len(), RELAYS_REMEMBERED_MAX);
        assert!(!book.knows(&relay), "the oldest went first");
        assert!(book.knows(&others[RELAYS_REMEMBERED_MAX - 1].0));
    }
}
