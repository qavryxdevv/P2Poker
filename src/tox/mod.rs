//! A Tox instance, for the table's traffic (D-019).
//!
//! The lobby stays on libp2p. Once a table forms, its game traffic rides a Tox
//! NGC group created by the founder: the `chat_id` travels in the table's
//! advertisement, members arrive **by invitation**, and the protocol rides
//! custom lossless packets while ordinary group messages carry human chat.
//!
//! # What this buys, and it is one thing
//!
//! A libp2p relay circuit grants 128 KiB and two minutes. Measured across two
//! networks on 2026-08-31: where DCUtR hole-punched, five hands played and both
//! ends agreed on all five genesis hashes; where it did not, the hand reached
//! the deal, the circuit ran out, and every publish afterwards answered
//! `NoPeersSubscribedToTopic`. Tox has no per-circuit byte cap.
//!
//! # What it costs
//!
//! `c-toxcore` is GPL-3.0. Linking it makes this whole client GPL-3.0, which is
//! why it is behind `--features tox` — the feature flag is the line where the
//! licence changes, and D-019 calls it a one-way door in those words.
//!
//! # The rule that does not change
//!
//! **Nothing here is authority.** Who sent a message is decided by the
//! signature inside it against the ratified roster, never by which Tox peer a
//! packet arrived from — a chat id travels in a public advertisement, so
//! "it came over the table's group" is worth exactly nothing as a claim about
//! authorship. That is `table::transport`'s rule and this module is one more
//! implementation under it.

pub mod sys;

use std::ffi::{c_int, CString};

/// A running Tox instance.
///
/// Owns the C object and kills it on drop. Not `Sync`: `tox_iterate` and every
/// call around it must happen on one thread unless the instance was built with
/// the experimental thread-safety option, which this client does not set.
pub struct Tox {
    ptr: *mut sys::Tox,
}

/// Why an instance could not be created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failed {
    /// `tox_options_new` failed, which means allocation failed.
    Options(c_int),
    /// `tox_new` failed. The number is `Tox_Err_New`'s own, from the header.
    New(c_int),
    /// A bootstrap address this client could not even hand to toxcore.
    Address(&'static str),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Options(e) => write!(f, "tox options could not be allocated ({e})"),
            Self::New(e) => write!(f, "the tox instance could not be created (Tox_Err_New {e})"),
            Self::Address(w) => write!(f, "a bootstrap address is unusable: {w}"),
        }
    }
}

impl Tox {
    /// A new instance, with the network settings D-019 asks for.
    ///
    /// # UPnP and NAT-PMP are not here, and cannot be
    ///
    /// D-019's requirements say the Tox instance opens its own port *"through
    /// NAT-PMP and UPnP, and both are on without anybody choosing them"*.
    /// **`c-toxcore` has neither.** The whole vendored tree — every `.c`, every
    /// `.h`, every CMake file — contains one occurrence of either word, and it
    /// is a sentence in `docs/TCP_Network.txt` saying they *can help*. There is
    /// no option to set and no build flag to turn on.
    ///
    /// What Tox does instead is UDP hole punching plus TCP relays, and that is
    /// its actual answer to NAT: `hole_punching_enabled` is on here explicitly
    /// rather than by default, so the setting is visible. Unlike a libp2p
    /// relay circuit, a Tox TCP relay carries a session with no byte cap, which
    /// is the property D-019 was bought for.
    ///
    /// If genuine port mapping is still wanted, it belongs to **this client**
    /// and not to toxcore: map a port with the IGD machinery already in the
    /// tree for libp2p, then pin Tox to it with `start_port`/`end_port`. That
    /// is a separate piece of work and is not pretended to here.
    pub fn new() -> Result<Self, Failed> {
        // SAFETY: `tox_options_new` either returns a valid pointer or null and
        // sets the error, which is checked before the pointer is used.
        unsafe {
            let mut err: c_int = 0;
            let opts = sys::tox_options_new(&mut err);
            if opts.is_null() || err != sys::TOX_ERR_OPTIONS_NEW_OK {
                return Err(Failed::Options(err));
            }

            sys::tox_options_set_ipv6_enabled(opts, true);
            sys::tox_options_set_udp_enabled(opts, true);
            // On, and said so rather than left to the default: a client that
            // depends on a default is a client that changes behaviour when the
            // vendored tree moves.
            sys::tox_options_set_hole_punching_enabled(opts, true);
            // Two clients on one wire find each other without the DHT. It is
            // also the one discovery path that keeps working when the internet
            // does not.
            sys::tox_options_set_local_discovery_enabled(opts, true);

            let mut err: c_int = 0;
            let ptr = sys::tox_new(opts, &mut err);
            sys::tox_options_free(opts);
            if ptr.is_null() || err != sys::TOX_ERR_NEW_OK {
                return Err(Failed::New(err));
            }
            Ok(Self { ptr })
        }
    }

    /// This instance's Tox address: public key, nospam and checksum.
    pub fn address(&self) -> [u8; sys::TOX_ADDRESS_SIZE] {
        let mut out = [0u8; sys::TOX_ADDRESS_SIZE];
        // SAFETY: the buffer is exactly `TOX_ADDRESS_SIZE`, which is what the
        // header says the call writes.
        unsafe { sys::tox_self_get_address(self.ptr, out.as_mut_ptr()) };
        out
    }

    /// Hand toxcore one DHT bootstrap node.
    ///
    /// Reports whether toxcore accepted the address, which is **not** whether
    /// the node answered: bootstrapping is asynchronous and the only evidence
    /// of it working is connectivity later.
    pub fn bootstrap(&mut self, host: &str, port: u16, public_key: &[u8; 32]) -> Result<(), Failed> {
        let host = CString::new(host).map_err(|_| Failed::Address("it contains a NUL byte"))?;
        let mut err: c_int = 0;
        // SAFETY: `host` outlives the call; the key is exactly 32 bytes, which
        // is `Tox_Dht_Id`.
        let ok = unsafe {
            sys::tox_bootstrap(
                self.ptr,
                host.as_ptr(),
                port,
                public_key.as_ptr(),
                &mut err,
            )
        };
        if ok && err == sys::TOX_ERR_BOOTSTRAP_OK {
            Ok(())
        } else {
            Err(Failed::New(err))
        }
    }

    /// How long toxcore wants before the next [`iterate`](Tox::iterate).
    pub fn interval(&self) -> std::time::Duration {
        // SAFETY: the pointer is valid for the lifetime of `self`.
        std::time::Duration::from_millis(u64::from(unsafe {
            sys::tox_iteration_interval(self.ptr)
        }))
    }

    /// One turn of toxcore's own loop. Everything it does happens here.
    pub fn iterate(&mut self) {
        // SAFETY: valid pointer, and no user data is passed because no callback
        // that would read it is registered yet.
        unsafe { sys::tox_iterate(self.ptr, std::ptr::null_mut()) };
    }
}

impl Drop for Tox {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `tox_new` and is killed exactly once.
        unsafe { sys::tox_kill(self.ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The linkage test.** It proves the whole vendored chain resolves —
    /// sixty C files, libsodium's static archive, and winsock — which nothing
    /// else does: `cargo build` compiles the archive happily whether or not a
    /// single symbol in it can be linked.
    ///
    /// It also proves an instance really comes up, since `tox_new` binds a
    /// socket.
    #[test]
    fn an_instance_starts_and_has_an_address() {
        let tox = Tox::new().expect("a tox instance");
        let address = tox.address();
        assert_eq!(address.len(), sys::TOX_ADDRESS_SIZE);
        assert!(
            address.iter().any(|b| *b != 0),
            "an address of thirty-eight zero bytes is an address nothing wrote"
        );

        // Two instances have two identities. A fresh keypair per instance is
        // what makes the address worth publishing at all.
        let other = Tox::new().expect("a second instance");
        assert_ne!(
            tox.address(),
            other.address(),
            "two instances must not share an identity"
        );
    }

    /// A bootstrap address is accepted and the loop turns. Neither proves the
    /// network answered — that needs two machines, which is
    /// `tools/two-network-ssh.ps1`'s job.
    #[test]
    fn it_accepts_a_bootstrap_node_and_iterates() {
        let mut tox = Tox::new().expect("a tox instance");
        // A node from toxcore's own list. Its answering is not asserted.
        let key = [0x11u8; 32];
        tox.bootstrap("tox.abilinski.com", 33445, &key)
            .expect("toxcore accepted the address");
        for _ in 0..3 {
            tox.iterate();
        }
        assert!(tox.interval() <= std::time::Duration::from_secs(1));
    }
}
