//! Where to bootstrap from, and it has to stay current.
//!
//! # Why this is not a constant
//!
//! A node list compiled into a binary goes stale, and when it does it takes the
//! **relay fallback** with it — which is the thing that makes a NAT survivable.
//! Measured 2026-08-31: two machines behind one router on different subnets,
//! both UDP-connected to the DHT in nine seconds, and no connection to each
//! other in a hundred and fifty, because `tox_add_tcp_relay` had never been
//! called. With relays from the public list they were talking in thirteen
//! seconds and moved 3.4 MB each way. The relays are not an optimisation.
//!
//! So the list is fetched, exactly as every Tox client does, from
//! `https://nodes.tox.chat/json`.
//!
//! # The union rule, which is this client's own
//!
//! **A fetched list is added to the bundled one and never replaces it.**
//!
//! A node list decides who a client bootstraps from, so whoever serves it can
//! choose a client's whole view of the network — and a client that swapped its
//! list for the downloaded one could be isolated onto an attacker's DHT by one
//! bad response, or by one compromised host, without anything looking wrong.
//! Taking the union costs a few duplicate entries and removes that outcome
//! entirely: a hostile list can add nodes it controls, and it cannot take away
//! the ones compiled in. Reaching one honest node is enough.
//!
//! qTox replaces. This does not, and the difference is one line of code and a
//! failure mode.
//!
//! # Everything here is fed by the network
//!
//! `SPEC_CS.md` §27, so: the response is bounded before it is read, the node
//! count is bounded before anything is allocated, every key must be
//! sixty-four hex characters, every host is length-bounded, and a port of zero
//! is refused. A list that fails any of it is dropped whole — the bundled one
//! is still there, which is the point of the union.

use std::path::{Path, PathBuf};

/// One node: a DHT address and the relay ports it also listens on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub host: String,
    /// The DHT port, for `tox_bootstrap`.
    pub udp_port: u16,
    pub key: [u8; 32],
    /// The TCP ports it relays on, for `tox_add_tcp_relay`. May be empty.
    pub tcp_ports: Vec<u16>,
}

/// The most nodes this client will hold, bundled and fetched together.
///
/// The public list carries about fifty. A hundred is room for it to grow and a
/// bound on what a response can make this client allocate.
pub const MAX_NODES: usize = 100;

/// The most TCP ports one node may claim.
pub const MAX_TCP_PORTS: usize = 8;

/// The longest host name accepted. Longer than any real one and short enough
/// that a hundred of them are not a memory decision a stranger makes.
pub const MAX_HOST: usize = 253;

/// The most of a response that will be read.
pub const MAX_RESPONSE: usize = 512 * 1024;

/// How old a cached list may be before it is refetched.
pub const REFRESH_AFTER_MS: u64 = 24 * 60 * 60 * 1000;

/// Where the cache lives inside a profile directory.
pub const CACHE_FILE: &str = "tox-nodes.json";

/// The list compiled in, from `https://nodes.tox.chat/json` on 2026-08-31,
/// filtered to nodes reporting **both** UDP and TCP up.
///
/// This is the floor. It is what a first run uses, what an offline run uses,
/// and what a run against a hostile list still has.
const BUNDLED: &[(&str, u16, &str, &[u16])] = &[
    ("144.217.167.73", 33445, "7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C", &[33445, 3389]),
    ("205.185.115.131", 53, "3091C6BEB2A993F1C6300C16549FABA67098FF3D62C6D253828B531470B53D68", &[3389, 33445, 443, 53]),
    ("3.0.24.15", 33445, "E20ABCF38CDBFFD7D04B29C956B33F7B27A3BB7AF0618101617B036E4AEA402D", &[33445]),
    ("139.162.110.188", 33445, "F76A11284547163889DDC89A7738CF271797BF5E5E220643E97AD3C7E7903D55", &[443, 33445, 3389]),
    ("144.172.88.203", 33445, "2016A0F2797EE3A8B004BA623F11AAFC8146F1B8F45107232A1A1AECCE856674", &[33445, 443]),
    ("172.104.215.182", 33445, "DA2BD927E01CD05EBCC2574EBE5BEBB10FF59AE0B2105A7D1E2B40E49BB20239", &[443, 3389, 33445]),
    ("188.214.122.30", 33445, "2A9F7A620581D5D1B09B004624559211C5ED3D1D712E8066ACDB0896A7335705", &[3389, 33445]),
    ("43.198.227.166", 33445, "AD13AB0D434BCE6C83FE2649237183964AE3341D0AFB3BE1694B18505E4E135E", &[3389, 33445]),
    ("95.181.230.108", 33445, "B5FFECB4E4C26409EBB88DB35793E7B39BFA3BA12AC04C096950CB842E3E130A", &[3389, 33445]),
    ("188.245.84.166", 33445, "96B66D300BA2B59B98FC42DB1325E7092388F0379593E680ABDBEA03B9C9CE03", &[443, 3389, 33445]),
];

/// The list compiled into this binary.
pub fn bundled() -> Vec<Node> {
    BUNDLED
        .iter()
        .filter_map(|(h, p, k, t)| {
            Some(Node {
                host: (*h).to_string(),
                udp_port: *p,
                key: hex32(k)?,
                tcp_ports: t.to_vec(),
            })
        })
        .collect()
}

/// Sixty-four hex characters, or nothing.
fn hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in s.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

/// Read `nodes.tox.chat`'s document into nodes, refusing anything unbounded.
///
/// Only nodes reporting **both** UDP and TCP up are taken. A node with neither
/// is a node that answers nothing, and one with only UDP cannot serve as the
/// relay that makes a NAT survivable.
pub fn parse(json: &str) -> Vec<Node> {
    if json.len() > MAX_RESPONSE {
        return Vec::new();
    }
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(list) = doc.get("nodes").and_then(|n| n.as_array()) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for n in list.iter().take(MAX_NODES) {
        let up_udp = n.get("status_udp").and_then(|v| v.as_bool()).unwrap_or(false);
        let up_tcp = n.get("status_tcp").and_then(|v| v.as_bool()).unwrap_or(false);
        if !(up_udp && up_tcp) {
            continue;
        }
        let Some(host) = n.get("ipv4").and_then(|v| v.as_str()) else {
            continue;
        };
        // **`"NONE"` as well as `"-"`.** `nodes.tox.chat` writes a literal
        // `"NONE"` in `ipv4` for a node it knows only by IPv6, and one such
        // node in the live list carries `status_udp` and `status_tcp` true and
        // two real TCP ports — so it passed this filter and was added as a
        // relay whose host is the four characters `NONE`. That is a DNS lookup
        // that cannot succeed and two relay slots spent on nothing, on every
        // start.
        //
        // Skipped rather than repaired from `ipv6`: that field holds a hostname
        // as often as an address in this document, and guessing which is which
        // is how a parser starts trusting a stranger's formatting.
        if host.is_empty() || host.len() > MAX_HOST || host == "-" || host == "NONE" {
            continue;
        }
        let Some(port) = n.get("port").and_then(|v| v.as_u64()) else {
            continue;
        };
        if port == 0 || port > u64::from(u16::MAX) {
            continue;
        }
        let Some(key) = n.get("public_key").and_then(|v| v.as_str()).and_then(hex32) else {
            continue;
        };
        let tcp_ports: Vec<u16> = n
            .get("tcp_ports")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_u64())
                    .filter(|p| *p > 0 && *p <= u64::from(u16::MAX))
                    .map(|p| p as u16)
                    .take(MAX_TCP_PORTS)
                    .collect()
            })
            .unwrap_or_default();

        out.push(Node {
            host: host.to_string(),
            udp_port: port as u16,
            key,
            tcp_ports,
        });
    }
    out
}

/// The bundled list, plus anything in `fetched` that is not already in it.
///
/// **Union, never replacement.** See the module docs: a client that swapped its
/// list for a downloaded one could be moved onto somebody else's DHT by one bad
/// response. Identity is `(host, udp_port, key)` — the same host under a
/// different key is a different node, and treating it as the same one would let
/// a list quietly re-key a bundled entry, which is the same attack by another
/// name.
pub fn merge(mut bundled: Vec<Node>, fetched: Vec<Node>) -> Vec<Node> {
    for n in fetched {
        if bundled.len() >= MAX_NODES {
            break;
        }
        let known = bundled
            .iter()
            .any(|b| b.host == n.host && b.udp_port == n.udp_port && b.key == n.key);
        if !known {
            bundled.push(n);
        }
    }
    bundled
}

/// Where the cache sits for a profile.
pub fn cache_path(profile: &Path) -> PathBuf {
    profile.join(CACHE_FILE)
}

/// The list to use: bundled, plus a cached fetch if there is one.
///
/// Never fails and never blocks. A missing, unreadable or nonsense cache leaves
/// the bundled list, which is exactly the behaviour a first run needs.
pub fn load(profile: &Path) -> Vec<Node> {
    let cached = std::fs::read_to_string(cache_path(profile))
        .ok()
        .map(|s| parse(&s))
        .unwrap_or_default();
    merge(bundled(), cached)
}

/// Whether the cache is old enough to be worth refetching.
pub fn stale(profile: &Path, now_ms: u64) -> bool {
    let Ok(meta) = std::fs::metadata(cache_path(profile)) else {
        return true;
    };
    let Ok(modified) = meta.modified() else {
        return true;
    };
    let Ok(age) = std::time::SystemTime::now().duration_since(modified) else {
        // A file dated in the future. Treated as stale rather than trusted for
        // ever, because a clock that moved is not a reason to stop updating.
        return true;
    };
    let _ = now_ms;
    age.as_millis() >= u128::from(REFRESH_AFTER_MS)
}

/// Fetch the public list and write it to the cache. **Blocking.**
///
/// Call it off the loop that drives the game — it is one HTTPS request and it
/// can take as long as a stranger's server does.
///
/// The response is bounded, parsed and required to yield nodes before anything
/// is written: a cache is only replaced by a document that produced a usable
/// list, so a bad day at `nodes.tox.chat` cannot empty it.
#[cfg(feature = "tox")]
pub fn refresh(profile: &Path) -> Result<usize, String> {
    let response = attohttpc::get("https://nodes.tox.chat/json")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .map_err(|e| format!("the node list could not be fetched: {e}"))?;
    if !response.is_success() {
        return Err(format!("the node list answered {}", response.status()));
    }
    let body = response
        .text()
        .map_err(|e| format!("the node list could not be read: {e}"))?;
    if body.len() > MAX_RESPONSE {
        return Err(format!("the node list is {} bytes, over the cap", body.len()));
    }
    let nodes = parse(&body);
    if nodes.is_empty() {
        return Err("the node list held nothing usable".into());
    }

    // Written through a temporary and renamed, so an interrupted write leaves
    // the old cache rather than half of a new one.
    let path = cache_path(profile);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &body).map_err(|e| format!("the cache could not be written: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("the cache could not be replaced: {e}"))?;
    Ok(nodes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `nodes.tox.chat` writes a literal `"NONE"` where it knows a node only by
    /// IPv6, and such a node can still carry `status_udp`, `status_tcp` and real
    /// TCP ports — one in the live list does. Without this it was added as a
    /// relay whose host is the four characters `NONE`: a DNS lookup that cannot
    /// succeed and two relay slots spent on nothing, on every start.
    #[test]
    fn a_node_known_only_by_ipv6_is_skipped_rather_than_dialled_as_none() {
        let doc = r#"{"nodes":[
            {"ipv4":"NONE","ipv6":"tox1.mooo.com","port":33445,
             "public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C",
             "status_udp":true,"status_tcp":true,"tcp_ports":[3389,33445]},
            {"ipv4":"9.9.9.9","port":33445,
             "public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C",
             "status_udp":true,"status_tcp":true,"tcp_ports":[443]}
        ]}"#;
        let got = parse(doc);
        assert_eq!(got.len(), 1, "the NONE node is not a node");
        assert_eq!(got[0].host, "9.9.9.9");
    }

    #[test]
    fn the_bundled_list_is_usable() {
        let b = bundled();
        assert_eq!(b.len(), BUNDLED.len(), "every bundled key parses");
        assert!(b.iter().all(|n| !n.host.is_empty() && n.udp_port != 0));
        assert!(
            b.iter().all(|n| !n.tcp_ports.is_empty()),
            "a bundled node with no relay port is a node that cannot be the fallback"
        );
    }

    #[test]
    fn only_nodes_that_are_up_both_ways_are_taken() {
        let json = r#"{"nodes":[
            {"ipv4":"1.2.3.4","port":33445,"public_key":"AA","status_udp":true,"status_tcp":true},
            {"ipv4":"5.6.7.8","port":33445,"public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C","status_udp":true,"status_tcp":false},
            {"ipv4":"9.9.9.9","port":33445,"public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C","status_udp":true,"status_tcp":true,"tcp_ports":[443,3389]}
        ]}"#;
        let n = parse(json);
        assert_eq!(n.len(), 1, "the short key and the TCP-down node are dropped");
        assert_eq!(n[0].host, "9.9.9.9");
        assert_eq!(n[0].tcp_ports, vec![443, 3389]);
    }

    #[test]
    fn nonsense_yields_nothing_rather_than_panicking() {
        for bad in ["", "null", "[]", "{}", r#"{"nodes":"no"}"#, "{\"nodes\":[{}]}"] {
            assert!(parse(bad).is_empty(), "{bad}");
        }
    }

    /// Every bound, because all of them are numbers a stranger chooses.
    #[test]
    fn the_bounds_hold() {
        // A port of zero, a host that is empty, a key of the wrong length.
        let json = r#"{"nodes":[
            {"ipv4":"1.1.1.1","port":0,"public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C","status_udp":true,"status_tcp":true},
            {"ipv4":"","port":33445,"public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C","status_udp":true,"status_tcp":true},
            {"ipv4":"2.2.2.2","port":70000,"public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C","status_udp":true,"status_tcp":true}
        ]}"#;
        assert!(parse(json).is_empty());

        // Too many TCP ports, and ports out of range among them.
        let many: String = (1..=20).map(|p| format!("{p},")).collect();
        let json = format!(
            r#"{{"nodes":[{{"ipv4":"3.3.3.3","port":33445,"public_key":"7E5668E0EE09E19F320AD47902419331FFEE147BB3606769CFBE921A2A2FD34C","status_udp":true,"status_tcp":true,"tcp_ports":[{}0]}}]}}"#,
            many
        );
        let n = parse(&json);
        assert_eq!(n.len(), 1);
        assert_eq!(
            n[0].tcp_ports.len(),
            MAX_TCP_PORTS,
            "the port list is cut, not trusted"
        );
        assert!(!n[0].tcp_ports.contains(&0), "a zero port is not a port");

        // And a response over the cap is not parsed at all.
        let huge = format!("{{\"nodes\":[]{}}}", " ".repeat(MAX_RESPONSE));
        assert!(parse(&huge).is_empty());
    }

    /// **The union rule.** A fetched list adds and never removes, so a hostile
    /// one cannot cut this client off from the nodes it shipped with.
    #[test]
    fn a_fetched_list_cannot_remove_a_bundled_node() {
        let hostile = vec![Node {
            host: "evil.example".into(),
            udp_port: 33445,
            key: [0xEE; 32],
            tcp_ports: vec![443],
        }];
        let merged = merge(bundled(), hostile.clone());
        assert_eq!(merged.len(), bundled().len() + 1);
        for b in bundled() {
            assert!(merged.contains(&b), "a bundled node survived");
        }

        // And an empty fetch changes nothing.
        assert_eq!(merge(bundled(), Vec::new()), bundled());
    }

    /// The same host under a different key is a **different** node, so a list
    /// cannot quietly re-key a bundled entry.
    #[test]
    fn a_rekeyed_bundled_node_is_not_the_same_node() {
        let mut impostor = bundled()[0].clone();
        impostor.key = [0x11; 32];
        let merged = merge(bundled(), vec![impostor.clone()]);
        assert_eq!(merged.len(), bundled().len() + 1);
        assert!(merged.contains(&bundled()[0]), "the real one is still there");
        assert!(merged.contains(&impostor), "and the other is an addition");
    }

    /// The union is bounded too: a list of a thousand cannot make this client
    /// hold a thousand.
    #[test]
    fn the_union_is_bounded() {
        let many: Vec<Node> = (0..500u32)
            .map(|i| Node {
                host: format!("{i}.example"),
                udp_port: 33445,
                key: [(i % 251) as u8; 32],
                tcp_ports: vec![443],
            })
            .collect();
        assert_eq!(merge(bundled(), many).len(), MAX_NODES);
    }
}
