//! Asking the router to open a port: PCP first, then NAT-PMP.
//!
//! A third way in, beside the two this client already has.
//!
//! * **UPnP IGD** is in the behaviour (`libp2p::upnp`) and is what most consumer
//!   routers speak, when it is switched on.
//! * **Circuit Relay v2 and DCUtR** are the fallback when nothing opens a port
//!   at all.
//! * **PCP (RFC 6887) and NAT-PMP (RFC 6886)**, here. Apple's routers speak
//!   NAT-PMP and not UPnP; OpenWrt, pfSense and a good deal of ISP equipment
//!   speak PCP, which is the IETF's replacement for both and the one a carrier
//!   NAT is most likely to answer.
//!
//! They are tried together and the first that answers wins. This is not
//! redundancy for its own sake: a hand played over a relay costs somebody else's
//! bandwidth and a hole punch that fails costs the table a seat, so a port that
//! opens is worth three protocols' worth of trying.
//!
//! # What this does **not** decide
//!
//! Whether this client is reachable. A mapping that the router confirms is a
//! claim by the router, and a router that is itself behind a carrier NAT will
//! confirm one happily while the port stays shut from outside. **AutoNAT is the
//! only thing that may decide reachability** — that rule is in
//! [`run`](super::run) and this does not touch it. All this does is open the
//! door so that AutoNAT's probe has something to find.
//!
//! # Renewal is the part that is easy to get wrong
//!
//! A mapping has a lifetime and both protocols expect the client to renew it.
//! RFC 6886 §3.3 says renew at half the lifetime, which is what this does, and
//! the reason for the halving rather than a renewal just before expiry is a
//! lost packet: one dropped renewal at 90 % leaves no time for a second attempt,
//! and the port closes in the middle of a hand.

use std::net::IpAddr;
use std::num::NonZeroU16;
use std::time::Duration;

use crab_nat::{GatewayAddress, InternetProtocol, PortMapping, PortMappingOptions, TimeoutConfig};

use super::node::NodeEvent;

/// What was asked for, and how long it should live.
///
/// Two hours: long enough that a lost renewal has three more chances before the
/// port shuts, short enough that a client which crashes does not leave a hole in
/// somebody's router until they reboot it.
pub const LIFETIME_SECONDS: u32 = 7_200;

/// The fraction of the lifetime at which the mapping is renewed (RFC 6886 §3.3).
pub const RENEW_AT: u32 = 2;

/// How long to wait on a gateway that may not be there.
///
/// Both RFCs prescribe an exponential backoff over roughly two minutes, which is
/// right for a client whose only job is this and wrong for one that has a poker
/// table to run: the answer this needs is *is there a gateway here*, and a
/// gateway that has not replied in a few seconds is not going to.
///
/// Three tries over about seven seconds. A network with a gateway answers on the
/// first; a network without one is discovered before anybody notices.
fn patience() -> TimeoutConfig {
    TimeoutConfig {
        initial_timeout: Duration::from_secs(1),
        max_retries: 2,
        max_retry_timeout: Some(Duration::from_secs(4)),
    }
}

/// What happened, for the log and for the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mapped {
    /// The router opened a port. `external` is what it gave, which is **not**
    /// necessarily what was asked for.
    Opened {
        external: u16,
        internal: u16,
        /// `"PCP"` or `"NAT-PMP"`, for the log. A router that answers one and
        /// not the other is worth knowing about when a connection fails.
        how: &'static str,
        lifetime_seconds: u32,
    },
    /// Nothing answered. Ordinary rather than an error: most networks a client
    /// runs on have no gateway that speaks either protocol, and the relay path
    /// exists for exactly this.
    NoGateway,
    /// A gateway answered and refused.
    Refused(String),
}

/// The default gateway and this machine's address on the way to it.
///
/// `crab_nat` deliberately does not look this up — it makes no assumption about
/// how it is used — so it is looked up here.
pub fn gateway() -> Option<(GatewayAddress, IpAddr)> {
    let interface = netdev::get_default_interface().ok()?;
    let gw = interface.gateway.as_ref()?;

    // IPv4 first. A NAT that needs a mapping is an IPv4 NAT by definition:
    // IPv6 has no address translation and a client with a working IPv6 route
    // needs a firewall pinhole rather than a port map, which is PCP's job and
    // not something to guess at.
    if let (Some(g), Some(local)) = (gw.ipv4.first(), interface.ipv4.first()) {
        return Some((GatewayAddress::IpV4(*g), IpAddr::V4(local.addr())));
    }
    if let (Some(g), Some(local)) = (gw.ipv6.first(), interface.ipv6.first()) {
        return Some((
            GatewayAddress::IpV6(*g, None),
            IpAddr::V6(local.addr()),
        ));
    }
    None
}

/// Ask for a mapping of `port`, once.
///
/// `crab_nat` tries PCP and falls back to NAT-PMP, so this is one call for both.
pub async fn request(port: u16) -> (Mapped, Option<PortMapping>) {
    let Some(internal) = NonZeroU16::new(port) else {
        return (Mapped::Refused("port zero cannot be mapped".into()), None);
    };
    let Some((gw, client)) = gateway() else {
        return (Mapped::NoGateway, None);
    };

    let options = PortMappingOptions {
        // The same number outside as inside, when the router will give it. A
        // client that advertises one port and listens on another is a client
        // whose own address is wrong, and the router is free to refuse and
        // hand back a different one — which is why the answer is read from the
        // mapping rather than assumed.
        external_port: Some(internal),
        lifetime_seconds: Some(LIFETIME_SECONDS),
        timeout_config: Some(patience()),
    };

    match PortMapping::new(gw, client, InternetProtocol::Udp, internal, options).await {
        Ok(mapping) => {
            let how = match mapping.mapping_type() {
                crab_nat::PortMappingType::NatPmp => "NAT-PMP",
                crab_nat::PortMappingType::Pcp { .. } => "PCP",
            };
            let opened = Mapped::Opened {
                external: mapping.external_port().get(),
                internal: port,
                how,
                lifetime_seconds: mapping.lifetime(),
            };
            (opened, Some(mapping))
        }
        // A gateway that does not answer is not a gateway that refused. Both
        // arrive here as an error, and the difference matters: a refusal is a
        // router saying no, which is worth a line and worth asking again about;
        // silence means nothing here speaks the protocol, which is the ordinary
        // case on most networks and is said once.
        Err(e) => {
            let text = e.to_string();
            let silent = text.contains("did not respond") || text.contains("timeout");
            if silent {
                (Mapped::NoGateway, None)
            } else {
                (Mapped::Refused(text), None)
            }
        }
    }
}

/// How long to wait before renewing a mapping of this lifetime.
///
/// Half of it, per RFC 6886 §3.3, and never less than a minute — a gateway that
/// hands back a two-second lifetime would otherwise have this client talking to
/// it continuously.
pub fn renew_after(lifetime_seconds: u32) -> Duration {
    Duration::from_secs((lifetime_seconds / RENEW_AT).max(60) as u64)
}

/// Keep a port open for as long as this task lives.
///
/// **Its own task, and that is the point.** Both protocols are UDP to an address
/// that may not answer, and `crab_nat` waits and retries — so a request made
/// inside the node's event loop stops the node: every swarm event, every gossip
/// message and every join request waits behind a router that is not there. The
/// first version of this did exactly that.
///
/// It owns the mapping and **releases it when the client shuts down**, which it
/// learns from the event channel closing — the receiver goes when the
/// application does. A client that exits without releasing leaves a hole in a
/// stranger's router for the two hours until it expires, which is litter.
pub async fn keep_open(port: u16, events: super::node::Events) {
    // A router does not learn the protocol while this client runs, so a network
    // without one is a settled question — asked again only often enough that a
    // laptop moving between networks finds out.
    const ASK_AGAIN: Duration = Duration::from_secs(600);
    const AFTER_A_REFUSAL: Duration = Duration::from_secs(300);

    let mut mapping: Option<PortMapping> = None;
    let mut said_no_gateway = false;

    loop {
        let wait = match mapping.as_mut() {
            // Renew, and say nothing when it works. A renewal that succeeds
            // every hour is not news, and a log that says so is a log nobody
            // reads.
            Some(m) => {
                let lifetime = m.lifetime();
                match m.renew().await {
                    Ok(()) => renew_after(lifetime),
                    Err(e) => {
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "the port mapping was not renewed: {e}"
                            )))
                            .await;
                        // Dropped rather than kept. A mapping this client
                        // believes in and the router has forgotten is worse than
                        // none, because it stops this client asking again.
                        mapping = None;
                        Duration::from_secs(60)
                    }
                }
            }
            None => {
                let (what, held) = request(port).await;
                mapping = held;
                match what {
                    Mapped::Opened {
                        external,
                        how,
                        lifetime_seconds,
                        ..
                    } => {
                        let _ = events
                            .send(NodeEvent::PortMapped { how, external })
                            .await;
                        renew_after(lifetime_seconds)
                    }
                    Mapped::NoGateway => {
                        // Ordinary rather than a fault: most networks a client
                        // runs on have no gateway that speaks either protocol,
                        // and the relay path exists for exactly this. Said once.
                        if !said_no_gateway {
                            said_no_gateway = true;
                            let _ = events
                                .send(NodeEvent::Warning(
                                    "no router here speaks PCP or NAT-PMP".into(),
                                ))
                                .await;
                        }
                        ASK_AGAIN
                    }
                    Mapped::Refused(why) => {
                        let _ = events
                            .send(NodeEvent::Warning(format!(
                                "the router would not open a port: {why}"
                            )))
                            .await;
                        AFTER_A_REFUSAL
                    }
                }
            }
        };
        // Either the wait passes, or the client is shutting down. `closed()`
        // completes when the receiving end of the event channel is dropped,
        // which is the application going away — and it is the only chance this
        // task gets to hand the port back.
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = events.closed() => {
                if let Some(m) = mapping.take() {
                    let _ = m.try_drop().await;
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renew at half the lifetime, which is the RFC's number and not a
    /// preference. The reason for halving rather than renewing just before
    /// expiry is a lost packet: one dropped renewal at ninety per cent leaves no
    /// time for a second attempt and the port shuts mid-hand.
    #[test]
    fn a_mapping_is_renewed_at_half_its_life() {
        assert_eq!(renew_after(7_200), Duration::from_secs(3_600));
        assert_eq!(renew_after(600), Duration::from_secs(300));
    }

    /// And never so often that this client is talking to the router
    /// continuously. A gateway is free to hand back any lifetime it likes,
    /// including two seconds.
    #[test]
    fn an_absurd_lifetime_does_not_become_a_flood() {
        for silly in [0, 1, 2, 59, 119] {
            assert!(
                renew_after(silly) >= Duration::from_secs(60),
                "a {silly}-second lifetime would be renewed every {:?}",
                renew_after(silly)
            );
        }
    }

    /// The lifetime asked for is a compromise, and both ends of it matter: long
    /// enough that a lost renewal has more chances before the port shuts, short
    /// enough that a client which crashes does not leave a hole in somebody's
    /// router until they reboot it.
    #[test]
    fn the_lifetime_leaves_room_for_lost_renewals() {
        let renewals_before_expiry = LIFETIME_SECONDS / renew_after(LIFETIME_SECONDS).as_secs() as u32;
        assert!(renewals_before_expiry >= 2);
        // A mapping that outlives the client by a day is litter in a
        // stranger's router. Checked at compile time, because it is a
        // relationship between two constants and not a behaviour.
        const _: () = assert!(LIFETIME_SECONDS <= 24 * 3_600);
    }

    /// Port zero is not a port. It reaches here from a swarm that has not
    /// finished binding, and asking a router to map it would be asking for
    /// something meaningless.
    #[tokio::test]
    async fn port_zero_is_refused_without_touching_the_network() {
        let (what, mapping) = request(0).await;
        assert!(matches!(what, Mapped::Refused(_)));
        assert!(mapping.is_none());
    }

    /// Looking for the gateway must not panic, whatever this machine's network
    /// looks like — including a machine with no default route at all, which is
    /// every build agent.
    #[test]
    fn finding_the_gateway_never_panics() {
        let _ = gateway();
    }

    /// The whole point of the task: asking a router that is not there must not
    /// take longer than a few seconds, because the answer *is there a gateway
    /// here* is what the client needs and a silent one has already given it.
    ///
    /// Both RFCs prescribe a backoff over about two minutes, which is right for
    /// a tool whose only job is this and wrong for a poker client.
    #[test]
    fn a_silent_router_is_given_seconds_and_not_minutes() {
        let t = patience();
        // The worst case: the first timeout, then each retry, doubling but
        // capped.
        let mut total = t.initial_timeout;
        let mut next = t.initial_timeout;
        for _ in 0..t.max_retries {
            next *= 2;
            if let Some(cap) = t.max_retry_timeout {
                next = next.min(cap);
            }
            total += next;
        }
        assert!(
            total <= Duration::from_secs(15),
            "a silent gateway would be waited on for {total:?}"
        );
        assert!(t.max_retries >= 1, "one lost packet must not settle it");
    }
}
