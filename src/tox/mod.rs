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

pub mod nodes;
pub mod sys;
pub mod table;

use std::ffi::{c_int, c_void, CString};


/// Something toxcore reported during one [`iterate`](Tox::iterate).
///
/// **None of these is authority.** A packet arriving over the table's group
/// says nothing about who wrote it: the `chat_id` travels in a public lobby
/// advertisement, so anybody invited can send. Who signed a message is decided
/// after reassembly, by the signature inside it against the ratified roster,
/// exactly as `table::transport` says. `peer` and `friend` here are routing
/// hints and rate-limiting keys and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A fragment of the game protocol. Goes to `table::fragment`.
    GroupPacket {
        group: u32,
        peer: u32,
        data: Vec<u8>,
    },
    /// A friend invited this client into a group.
    ///
    /// Answered with [`accept_invite`](Tox::accept_invite) **only** when the
    /// friend is one this client added from a ratified roster. An invitation
    /// from anybody else is an invitation to a table this client is not
    /// joining.
    GroupInvite { friend: u32, invite: Vec<u8> },
    /// A friend's connection came up or went down. `0` is down, `1` TCP,
    /// `2` UDP - `Tox_Connection`'s own values.
    FriendConnection { friend: u32, status: i32 },
    /// A friend request arrived. **Never accepted**: this client adds friends
    /// from the roster with `tox_friend_add_norequest` and answers no requests,
    /// because a request from a stranger is a stranger. Reported so that a
    /// flood of them is visible rather than silent.
    FriendRequestIgnored,
    /// **This client's own join into a group completed.**
    ///
    /// The one signal in the whole API that separates *holding a group number*
    /// from *being in the group*, and nothing registered it.
    ///
    /// `tox_group_join_invite` returns a group number nine steps before the
    /// joiner is a confirmed peer: the chat is created at `CS_CONNECTING` with
    /// an address-less entry for the inviter, and `confirmed = true` is set in
    /// exactly one place in `group_chats.c`, after a handshake, an invite
    /// request, a sync request and a peer-info response. `self_join` fires at
    /// the sync response, which is the first point at which anything this
    /// client sends can reach anybody.
    ///
    /// The inviter's entry is reaped after twelve seconds if the handshake has
    /// not finished — silently, with no `peer_exit` and no log this client
    /// could see — and after that a fresh invitation is refused inside
    /// `Messenger.c` because a chat with that id already exists. Destroying the
    /// chat is the only way back, which is what
    /// [`table`](crate::tox::table)'s recovery does.
    ///
    /// Without it, `group 0/3` and `group 3/3` are the only numbers available
    /// and neither says whether the join finished — and `peer_count` counts
    /// **unconfirmed** peers, so even a full count is not proof.
    GroupSelfJoin { group: u32 },
    /// A join attempt was abandoned, with toxcore's own reason code.
    ///
    /// `TOX_GROUP_JOIN_FAIL_PEER_LIMIT` = 0, `..._PASSWORD` = 1,
    /// `..._UNKNOWN` = 2.
    GroupJoinFail { group: u32, reason: i32 },
    /// **Another peer became confirmed in the group.**
    ///
    /// The flag this reports is the one
    /// `send_gc_lossless_packet_all_peers` requires: it skips a peer that is
    /// not confirmed and reports success anyway when there are none. So the set
    /// of peers this event has fired for is exactly the set a broadcast reaches
    /// — which `peer_count` is not, because it counts unconfirmed entries too.
    ///
    /// That gap is why `group N/N` was never proof that a message would be
    /// delivered, and why every reading of the status line before this had to
    /// hedge.
    GroupPeerJoin { group: u32, peer: u32 },
    /// A confirmed peer left, was kicked, or timed out. `key` is its group
    /// key, read inside the callback while the peer still exists; `quit`
    /// is `TOX_GROUP_EXIT_TYPE_QUIT` -- the peer left on purpose -- as
    /// against a timeout or a severed connection (`D-035`).
    GroupPeerExit {
        group: u32,
        peer: u32,
        key: Option<[u8; 32]>,
        quit: bool,
    },
}

/// Where the C callbacks put what they are given, for the length of one
/// `tox_iterate` and no longer.
///
/// toxcore documents that callbacks fire only from inside `tox_iterate`, which
/// is what makes a stack-allocated sink correct: it is passed as `user_data`,
/// every callback runs before that call returns, and the pointer is dead
/// afterwards. Nothing here outlives the call, so there is no shared mutable
/// state and no lock.
#[derive(Default)]
struct Sink {
    events: Vec<Event>,
}

/// Turn a `user_data` pointer back into the sink, or do nothing.
///
/// A null pointer means somebody called `tox_iterate` without a sink, which
/// this module never does — but a callback that dereferenced null would be a
/// crash in C, so it is checked rather than assumed.
unsafe fn sink<'a>(user_data: *mut c_void) -> Option<&'a mut Sink> {
    if user_data.is_null() {
        None
    } else {
        Some(&mut *user_data.cast::<Sink>())
    }
}

unsafe extern "C" fn on_group_packet(
    _tox: *mut sys::Tox,
    group: u32,
    peer: u32,
    data: *const u8,
    len: usize,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    // A null with a non-zero length would be a toxcore bug; treated as empty
    // rather than dereferenced, because this is the one place a C mistake
    // becomes a Rust crash.
    let bytes = if data.is_null() || len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data, len).to_vec()
    };
    s.events.push(Event::GroupPacket {
        group,
        peer,
        data: bytes,
    });
}

/// toxcore's own log line, forwarded to stderr in a harness build.
///
/// Filtered to the group code, because the whole library at `DEBUG` buries the
/// six lines that matter under thousands about the DHT. A `NULL` field is
/// printed as `?` rather than skipped: which one is missing is itself a fact
/// about where the library gave up.
#[cfg(feature = "fault-harness")]
unsafe extern "C" fn on_tox_log(
    _tox: *mut sys::Tox,
    level: c_int,
    file: *const std::ffi::c_char,
    line: u32,
    func: *const std::ffi::c_char,
    message: *const std::ffi::c_char,
    _user_data: *mut c_void,
) {
    let s = |p: *const std::ffi::c_char| -> String {
        if p.is_null() {
            "?".to_owned()
        } else {
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    };
    let file = s(file);
    // **`group` alone dropped the file the answer is in.** `S1-AA`'s next
    // obstacle is `send_packet_tcp_connection` returning -1, which lives in
    // `TCP_connection.c` -- so patch 0017's line, added precisely to say why,
    // would have been filtered out before anybody read it. The transport files
    // are quiet in a healthy run; they are loud only when a relay cannot carry
    // a packet, which is the case this exists for.
    if !file.contains("group")
        && !file.contains("TCP_connection")
        && !file.contains("TCP_client")
    {
        return;
    }
    let out = format!("toxcore[{level}] {file}:{line} {} — {}", s(func), s(message));

    // **Consecutive duplicates are collapsed, with the count.**
    //
    // A transport line fires once per packet, so one unreachable peer produced
    // **1 578 identical lines in 42 seconds** — a third of that node's whole
    // log, saying one thing. The state it reports changes rarely and the
    // repetition carries nothing, but the count does: *how long* a peer had no
    // relay is exactly what a reader wants next.
    //
    // Nothing is dropped. The run is closed by the next different line, which
    // prints how many were folded into it, so a log can still be counted.
    //
    // Process-wide is per seat here: every seat is its own `p2p-poker.exe`.
    static LAST: std::sync::Mutex<Option<(String, u64)>> = std::sync::Mutex::new(None);
    let Ok(mut last) = LAST.lock() else {
        eprintln!("{out}");
        return;
    };
    match last.as_mut() {
        Some((prev, n)) if *prev == out => {
            *n += 1;
            return;
        }
        Some((prev, n)) => {
            if *n > 0 {
                eprintln!("toxcore: the line above repeated {n} more time(s)");
            }
            *prev = out.clone();
            *n = 0;
        }
        None => *last = Some((out.clone(), 0)),
    }
    eprintln!("{out}");
}

/// A peer became confirmed. See [`Event::GroupPeerJoin`].
unsafe extern "C" fn on_group_peer_join(
    _tox: *mut sys::Tox,
    group: u32,
    peer: u32,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    s.events.push(Event::GroupPeerJoin { group, peer });
}

/// A confirmed peer left. The name and part message are not read: a display
/// name is not an identity here and a part message is a string a stranger
/// chose.
#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn on_group_peer_exit(
    tox: *mut sys::Tox,
    group: u32,
    peer: u32,
    exit_type: c_int,
    _name: *const u8,
    _name_len: usize,
    _part: *const u8,
    _part_len: usize,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    // `D-035`: the key is readable only now, while the peer still exists.
    let mut key = [0u8; 32];
    let mut err: c_int = 0;
    // SAFETY: the callback runs inside `tox_iterate` with a live `tox`, and
    // `key` is the thirty-two bytes the call writes.
    let got = unsafe { sys::tox_group_peer_get_public_key(tox, group, peer, key.as_mut_ptr(), &mut err) };
    // `TOX_GROUP_EXIT_TYPE_QUIT` is the first value of the enum.
    s.events.push(Event::GroupPeerExit {
        group,
        peer,
        key: got.then_some(key),
        quit: exit_type == 0,
    });
}

/// This client's own join finished. See [`Event::GroupSelfJoin`].
unsafe extern "C" fn on_group_self_join(
    _tox: *mut sys::Tox,
    group: u32,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    s.events.push(Event::GroupSelfJoin { group });
}

/// A join was abandoned. Carries toxcore's reason rather than a boolean,
/// because *peer limit* and *unknown* want different answers from a caller.
unsafe extern "C" fn on_group_join_fail(
    _tox: *mut sys::Tox,
    group: u32,
    reason: c_int,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    s.events.push(Event::GroupJoinFail {
        group,
        reason,
    });
}

unsafe extern "C" fn on_group_invite(
    _tox: *mut sys::Tox,
    friend: u32,
    invite: *const u8,
    invite_len: usize,
    _group_name: *const u8,
    _group_name_len: usize,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    let bytes = if invite.is_null() || invite_len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(invite, invite_len).to_vec()
    };
    // The group's own name is not read. It is a string the inviter chose, it
    // decides nothing, and a table is identified by its `table_id` and its
    // roster - never by what somebody called a chat room.
    s.events.push(Event::GroupInvite {
        friend,
        invite: bytes,
    });
}

unsafe extern "C" fn on_friend_connection(
    _tox: *mut sys::Tox,
    friend: u32,
    status: c_int,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    s.events.push(Event::FriendConnection { friend, status });
}

unsafe extern "C" fn on_friend_request(
    _tox: *mut sys::Tox,
    _public_key: *const u8,
    _message: *const u8,
    _length: usize,
    user_data: *mut c_void,
) {
    let Some(s) = sink(user_data) else { return };
    // Deliberately not acted on. See `Event::FriendRequestIgnored`.
    s.events.push(Event::FriendRequestIgnored);
}

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
    /// A toxcore call refused. The name is the C function, so a log line can be
    /// looked up in `tox.h` without guessing which call it came from, and the
    /// number is that call's own error enum.
    Api { call: &'static str, error: c_int },
    /// A packet handed to [`send`](Tox::send) that the transport cannot carry.
    ///
    /// Never reached from the protocol's own path: `table::fragment` cuts every
    /// message to the MTU first. It is here so that a caller which forgets is
    /// refused rather than truncated.
    TooLong { len: usize },
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Options(e) => write!(f, "tox options could not be allocated ({e})"),
            Self::New(e) => write!(f, "the tox instance could not be created (Tox_Err_New {e})"),
            Self::Address(w) => write!(f, "a bootstrap address is unusable: {w}"),
            Self::Api { call, error } => write!(f, "{call} refused ({error})"),
            Self::TooLong { len } => {
                write!(f, "a {len}-byte packet, over what a Tox packet carries")
            }
        }
    }
}

/// Whether LAN discovery is on. Always `true` outside the fault harness.
#[cfg(feature = "fault-harness")]
fn local_discovery() -> bool {
    std::env::var("P2P_POKER_NO_LAN_DISCOVERY").is_err()
}

/// The product default, in every build that did not ask for the harness.
#[cfg(not(feature = "fault-harness"))]
fn local_discovery() -> bool {
    true
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
        Self::open(None, true)
    }

    /// An instance whose identity is fixed by a secret key.
    ///
    /// The Tox address that follows is then the same on every start, which is
    /// what makes a two-machine measurement reproducible: both ends can be told
    /// the other's public key before either of them runs, instead of one having
    /// to learn it from the other at run time.
    ///
    /// **Not for a player's identity.** The client's own keys come from
    /// `storage::profile`; this exists for `examples/tox_link` and for tests,
    /// where a stable, disposable identity is the point.
    pub fn with_secret_key(secret: &[u8; 32]) -> Result<Self, Failed> {
        Self::open(Some(secret), true)
    }

    /// An instance that will not use UDP at all, so every connection goes
    /// through a TCP relay.
    ///
    /// # Why this exists, and it is a measurement finding rather than a taste
    ///
    /// Two machines behind **one** router but on **different subnets** cannot
    /// reach each other over Tox's UDP path, and the reason is structural: the
    /// DHT publishes both under the same public address, so a hole punch asks
    /// the router to hairpin a packet back to itself, and Tox's only
    /// non-public path is LAN discovery, which is multicast and does not cross
    /// a subnet boundary. Measured 2026-08-31: both ends UDP-connected to the
    /// DHT in nine seconds and no friend connection in a hundred and fifty.
    ///
    /// libp2p succeeds in that topology because DCUtR exchanges **private**
    /// address candidates as well as public ones, and the two subnets are
    /// routable to each other. Tox has no equivalent.
    ///
    /// A TCP relay is a third party with an address of its own, so nothing has
    /// to hairpin. That makes this the fallback for the one topology where
    /// D-019's transport is otherwise worse than the one it replaces.
    pub fn tcp_only(secret: &[u8; 32]) -> Result<Self, Failed> {
        Self::open(Some(secret), false)
    }

    fn open(secret: Option<&[u8; 32]>, udp: bool) -> Result<Self, Failed> {
        // SAFETY: `tox_options_new` either returns a valid pointer or null and
        // sets the error, which is checked before the pointer is used.
        unsafe {
            let mut err: c_int = 0;
            let opts = sys::tox_options_new(&mut err);
            if opts.is_null() || err != sys::TOX_ERR_OPTIONS_NEW_OK {
                return Err(Failed::Options(err));
            }

            sys::tox_options_set_ipv6_enabled(opts, true);
            sys::tox_options_set_udp_enabled(opts, udp);
            // On, and said so rather than left to the default: a client that
            // depends on a default is a client that changes behaviour when the
            // vendored tree moves.
            sys::tox_options_set_hole_punching_enabled(opts, true);
            // **The library's own diagnostics, in the harness build only.**
            //
            // Every `LOGGER_WARNING` and `LOGGER_ERROR` in the vendored tree is
            // a no-op while no callback is set, and the group invite path has
            // six distinct failure branches — four that log and two that return
            // silently. So `S1-AA` shape (i) looks from outside like a healthy
            // transport that will not admit a peer, and two measured attempts at
            // a leave-and-rejoin recovery did not cure it with nothing to say
            // why.
            //
            // **Not in a release build.** These lines are noisy, they name
            // files and functions inside a vendored C library, and a player has
            // no use for them. `--features fault-harness` is the same gate the
            // divergence and stall knobs use.
            #[cfg(feature = "fault-harness")]
            sys::tox_options_set_log_callback(opts, Some(on_tox_log));
            // Two clients on one wire find each other without the DHT. It is
            // also the one discovery path that keeps working when the internet
            // does not.
            //
            // **Switchable under the fault harness, and only there.** toxcore's
            // `onion_client.c` decides whether it is *"UDP connected"* with
            // `dht_isconnected`, which counts LAN peers — so several processes
            // on one machine can LAN-discover each other, take the UDP path for
            // their onion, and never touch a relay, while
            // `tox_self_get_connection_status` still answers TCP. That is a
            // candidate cause of a whole evening of runs where four co-located
            // processes reported `tox self tcp, tox friends up 0` with 45 relays
            // accepted and general UDP working. A default cannot be tested by
            // reading it; this makes the experiment one environment variable.
            sys::tox_options_set_local_discovery_enabled(opts, local_discovery());

            if let Some(key) = secret {
                sys::tox_options_set_savedata_type(opts, sys::TOX_SAVEDATA_TYPE_SECRET_KEY);
                // The pointer must outlive `tox_new`, which toxcore documents
                // and which is why `key` is borrowed by the caller's frame
                // rather than built here: a temporary would be dropped before
                // the call below reads it.
                sys::tox_options_set_savedata_data(opts, key.as_ptr(), key.len());
            }

            let mut err: c_int = 0;
            let ptr = sys::tox_new(opts, &mut err);
            sys::tox_options_free(opts);
            if ptr.is_null() || err != sys::TOX_ERR_NEW_OK {
                return Err(Failed::New(err));
            }

            // Registered once, here, rather than by a caller who might forget:
            // an instance with no callbacks receives nothing and reports no
            // error, which is the quietest way this could fail.
            sys::tox_callback_group_custom_packet(ptr, Some(on_group_packet));
            sys::tox_callback_group_invite(ptr, Some(on_group_invite));
            // **The two that were missing, and the reason a seat could sit at
            // `group 0/3` for a whole run with nothing to say about it.**
            //
            // # Registering `self_join` is not free, and it was checked
            //
            // `group_chats.c` sets `chat->time_connected` **inside** the block
            // guarded by `c->self_join != nullptr`, so registering the callback
            // changes library state that has been zero for the whole life of
            // this client. That is a behaviour change wearing an instrument's
            // clothes, and it is exactly the shape of thing this project has
            // been bitten by.
            //
            // It is safe here, and the reason is narrow: `time_connected` has
            // exactly two readers in the vendored tree and both are about group
            // **topics** — whether to send a peer the topic on sync, and the
            // topic-reversion window. **This client uses no topics at all**; the
            // game rides custom packets. So the field starts being set and
            // nothing in this build reads it.
            //
            // (The founder's side already sets it directly when its own group
            // reaches `CS_CONNECTED`, so only the joiner's was ever zero.)
            sys::tox_callback_group_self_join(ptr, Some(on_group_self_join));
            sys::tox_callback_group_join_fail(ptr, Some(on_group_join_fail));
            // **The confirmed-peer set, which `peer_count` is not.**
            sys::tox_callback_group_peer_join(ptr, Some(on_group_peer_join));
            sys::tox_callback_group_peer_exit(ptr, Some(on_group_peer_exit));
            sys::tox_callback_friend_connection_status(ptr, Some(on_friend_connection));
            sys::tox_callback_friend_request(ptr, Some(on_friend_request));

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

    /// Add one TCP relay.
    ///
    /// **A different list from [`bootstrap`](Tox::bootstrap)'s.** `tox_bootstrap`
    /// takes DHT nodes, which are reached over UDP; a client with UDP disabled
    /// bootstraps through relays and reaches nothing without at least one of
    /// these. Most public Tox nodes are both, at the same address and key, but
    /// they have to be handed over twice because toxcore keeps two lists.
    ///
    /// Worth adding even when UDP is on: it is the fallback toxcore uses when
    /// a peer cannot be reached directly, and a relay is a third party with an
    /// address of its own — which is exactly what two hosts behind one router
    /// need, since neither can hairpin a packet to the other.
    pub fn add_tcp_relay(&mut self, host: &str, port: u16, public_key: &[u8; 32]) -> Result<(), Failed> {
        let host = CString::new(host).map_err(|_| Failed::Address("it contains a NUL byte"))?;
        let mut err: c_int = 0;
        // SAFETY: `host` outlives the call; the key is exactly 32 bytes.
        let ok = unsafe {
            sys::tox_add_tcp_relay(self.ptr, host.as_ptr(), port, public_key.as_ptr(), &mut err)
        };
        Self::ok(ok && err == sys::TOX_ERR_BOOTSTRAP_OK, "tox_add_tcp_relay", err)
    }

    /// Whether this instance has reached the Tox network at all.
    ///
    /// `0` none, `1` through a TCP relay, `2` UDP — `Tox_Connection`'s own
    /// values. **The first thing to look at when nothing happens**: two
    /// instances on one machine find each other by local discovery without ever
    /// touching the DHT, so a test that passes there says nothing about
    /// whether bootstrapping worked. Across two networks it is the whole
    /// question, and without this the answer looks identical to a firewall.
    pub fn connection(&self) -> i32 {
        // SAFETY: the pointer is valid for the lifetime of `self`.
        unsafe { sys::tox_self_get_connection_status(self.ptr) }
    }

    /// What toxcore thinks of one friendship: `0` none, `1` TCP, `2` UDP.
    ///
    /// **The number `S1-N` needed and did not have.** The driver tracked
    /// friendships from `FriendConnection` events alone, which says what has
    /// *changed* and never what *is* — so a connection that came up before the
    /// driver was listening, or one whose event was missed, was invisible.
    /// Asked directly, this cannot disagree with toxcore, because it is
    /// toxcore's own answer.
    ///
    /// `-1` when the friend number is not one this instance issued.
    pub fn friend_connection(&self, friend: u32) -> i32 {
        let mut err: c_int = 0;
        // SAFETY: the pointer is valid for the lifetime of `self`; an unknown
        // friend number is reported through `err` rather than by misbehaving.
        let n = unsafe { sys::tox_friend_get_connection_status(self.ptr, friend, &mut err) };
        if err == 0 {
            n
        } else {
            -1
        }
    }

    /// How long toxcore wants before the next [`iterate`](Tox::iterate).
    pub fn interval(&self) -> std::time::Duration {
        // SAFETY: the pointer is valid for the lifetime of `self`.
        std::time::Duration::from_millis(u64::from(unsafe {
            sys::tox_iteration_interval(self.ptr)
        }))
    }

    /// One turn of toxcore's own loop, and what it reported.
    ///
    /// Everything toxcore does happens here: packets are received, callbacks
    /// fire, timers run. The sink lives on the stack for exactly the length of
    /// the call, which is correct because toxcore fires callbacks only from
    /// inside `tox_iterate` — so the pointer handed to C cannot outlive the
    /// frame that owns it.
    pub fn iterate(&mut self) -> Vec<Event> {
        let mut sink = Sink::default();
        // SAFETY: valid pointer; `sink` outlives the call and the callbacks
        // that read it can only run inside it.
        unsafe {
            sys::tox_iterate(self.ptr, (&raw mut sink).cast::<c_void>());
        }
        sink.events
    }

    // -----------------------------------------------------------------------
    // Friends, which exist here only so that a group invitation is possible
    // -----------------------------------------------------------------------

    /// Add a friend from a public key, with no request and nothing to accept.
    ///
    /// The key comes from the ratified roster and from nowhere else. This is
    /// the whole of this client's friend policy: it adds who the roster says
    /// and answers no requests, so a stranger cannot become a friend by asking.
    pub fn add_friend(&mut self, public_key: &[u8; 32]) -> Result<u32, Failed> {
        let mut err: c_int = 0;
        // SAFETY: the key is exactly `TOX_PUBLIC_KEY_SIZE`.
        let n = unsafe { sys::tox_friend_add_norequest(self.ptr, public_key.as_ptr(), &mut err) };
        if err == 0 {
            Ok(n)
        } else {
            Err(Failed::Api {
                call: "tox_friend_add_norequest",
                error: err,
            })
        }
    }

    // -----------------------------------------------------------------------
    // The group that carries the table
    // -----------------------------------------------------------------------

    /// Create the table's group. The founder does this and nobody else.
    ///
    /// **Private, not public.** A public group announces itself in Tox's DHT,
    /// and that announcement is the path measured on 2026-08-27 to work only
    /// while a group is new — a host up twenty seconds was found in thirty-one,
    /// a host up six minutes was never found in three hundred. Members arrive
    /// here by invitation, so the announcement is not wanted: it is a way to be
    /// found by people who are not being invited.
    pub fn new_group(&mut self, name: &str, self_name: &str) -> Result<u32, Failed> {
        let mut err: c_int = 0;
        // SAFETY: both slices are valid for the call and their lengths are
        // passed alongside them.
        let g = unsafe {
            sys::tox_group_new(
                self.ptr,
                sys::TOX_GROUP_PRIVACY_STATE_PRIVATE,
                name.as_ptr(),
                name.len(),
                self_name.as_ptr(),
                self_name.len(),
                &mut err,
            )
        };
        if err == 0 {
            Ok(g)
        } else {
            Err(Failed::Api {
                call: "tox_group_new",
                error: err,
            })
        }
    }

    /// The group's chat id, which is what the table advertises.
    pub fn chat_id(&self, group: u32) -> Result<[u8; sys::TOX_GROUP_CHAT_ID_SIZE], Failed> {
        let mut out = [0u8; sys::TOX_GROUP_CHAT_ID_SIZE];
        let mut err: c_int = 0;
        // SAFETY: the buffer is exactly `TOX_GROUP_CHAT_ID_SIZE`.
        let ok = unsafe {
            sys::tox_group_get_chat_id(self.ptr, group, out.as_mut_ptr(), &mut err)
        };
        if ok && err == 0 {
            Ok(out)
        } else {
            Err(Failed::Api {
                call: "tox_group_get_chat_id",
                error: err,
            })
        }
    }

    /// Invite a friend into the group. The founder does this for each seat the
    /// roster ratified.
    pub fn invite(&mut self, group: u32, friend: u32) -> Result<(), Failed> {
        let mut err: c_int = 0;
        // SAFETY: valid pointer; both numbers are toxcore's own handles.
        let ok = unsafe { sys::tox_group_invite_friend(self.ptr, group, friend, &mut err) };
        Self::ok(ok && err == 0, "tox_group_invite_friend", err)
    }

    /// Accept an invitation from a friend this client added from the roster.
    ///
    /// The caller checks that: an invitation is an offer, and this client joins
    /// tables it decided to join.
    pub fn accept_invite(&mut self, friend: u32, invite: &[u8], self_name: &str) -> Result<u32, Failed> {
        let mut err: c_int = 0;
        // SAFETY: both slices are valid for the call; no password is used,
        // which is a null pointer and a zero length.
        let g = unsafe {
            sys::tox_group_invite_accept(
                self.ptr,
                friend,
                invite.as_ptr(),
                invite.len(),
                self_name.as_ptr(),
                self_name.len(),
                std::ptr::null(),
                0,
                &mut err,
            )
        };
        if err == 0 {
            Ok(g)
        } else {
            Err(Failed::Api {
                call: "tox_group_invite_accept",
                error: err,
            })
        }
    }

    /// Send one fragment of the game protocol, losslessly.
    ///
    /// **Lossless**, always: this carries chained events, and a chain with a
    /// hole in it is not a chain. `table::fragment` has already cut the message
    /// to fit, and a caller that hands over more than the packet holds gets a
    /// refusal rather than a truncation.
    pub fn send(&mut self, group: u32, data: &[u8]) -> Result<(), Failed> {
        if data.len() > sys::TOX_GROUP_MAX_CUSTOM_LOSSLESS_PACKET_LENGTH {
            return Err(Failed::TooLong { len: data.len() });
        }
        let mut err: c_int = 0;
        // SAFETY: the slice is valid for the call and its length is passed.
        let ok = unsafe {
            sys::tox_group_send_custom_packet(self.ptr, group, true, data.as_ptr(), data.len(), &mut err)
        };
        Self::ok(ok && err == 0, "tox_group_send_custom_packet", err)
    }

    /// A peer's public key, for matching a group member against the roster.
    /// How far a peer-id scan goes.
    ///
    /// Peer ids are small and dense and a table is at most ten seats, so this
    /// is generous. It is a constant rather than a literal because two places
    /// scan and a scan that stopped short in one of them would under-count
    /// silently.
    const PEER_SCAN: u32 = 64;

    /// Forget a friend, so the next `add_friend` searches for it afresh.
    ///
    /// **This exists for one case: a peer whose client restarted.** toxcore
    /// keeps trying the address it last knew, and its own constants say for how
    /// long — `FRIEND_CONNECTION_TIMEOUT` is `FRIEND_PING_INTERVAL * 4`, thirty-
    /// two seconds (`friend_connection.h:34,37`), and the DHT entry behind it
    /// only goes bad at `FRIEND_DHT_TIMEOUT = BAD_NODE_TIMEOUT`, which is
    /// `PING_INTERVAL + 1 * (PING_INTERVAL + PING_ROUNDTRIP)` = **122 seconds**
    /// (`DHT.h:52-57`, `friend_connection.h:40`). So a restarted peer is
    /// invisible for something over two and a half minutes before the search
    /// even begins again.
    ///
    /// Measured: a client back after twenty seconds, and the founder reporting
    /// `tox friends up 2` of three for the rest of a five-minute run while it
    /// sent fifteen invitations to the two it could still reach. D-022 gives
    /// that seat two hands — about twenty seconds — to come back, so waiting
    /// out toxcore is not an option.
    ///
    /// Deleting drops the cached address with the friendship, and adding the
    /// same key again starts a fresh search.
    pub fn forget_friend(&mut self, friend: u32) -> Result<(), Failed> {
        let mut err: c_int = 0;
        // SAFETY: `friend` is a friend number this instance issued.
        let ok = unsafe { sys::tox_friend_delete(self.ptr, friend, &mut err) };
        if ok && err == 0 {
            Ok(())
        } else {
            Err(Failed::Api {
                call: "tox_friend_delete",
                error: err,
            })
        }
    }

    /// How many other peers this client can see in the group.
    ///
    /// **A count, not a set, and that distinction is forced by the API.**
    /// `tox_group_peer_get_public_key` returns the peer's **group** public key
    /// (`tox.h:3823`), which is a per-group identity and is *not* the friend
    /// key a roster holds — so a scan cannot say *which* seat it is looking at,
    /// only that somebody is there. This version of toxcore has no
    /// `tox_group_peer_count`; the NGC API offers none, and the one in `tox.h`
    /// belongs to the old conference API.
    ///
    /// So this scans peer ids and counts the ones that resolve, leaving out
    /// this client's own. Ten seats at most and a scan is cheap; it is called
    /// once every five seconds.
    pub fn peer_count(&self, group: u32) -> usize {
        let mut err: c_int = 0;
        // SAFETY: `group` is a live group number; the error out-pointer is valid.
        let me = unsafe { sys::tox_group_self_get_peer_id(self.ptr, group, &mut err) };
        let me = if err == 0 { Some(me) } else { None };
        (0..Self::PEER_SCAN)
            .filter(|p| Some(*p) != me && self.peer_key(group, *p).is_ok())
            .count()
    }

    /// Ask `peer` for the message this client is missing from it.
    ///
    /// **The repair the carrier already had and never reached.** A lost group
    /// message is normally recovered in one round trip: the receiver sees a
    /// hole and sends `GR_ACK_REQ`, and the sender answers with an immediate
    /// retransmission and no backoff — 36 to 326 ms on the two-machine bed.
    /// But a hole is only visible once something *later* has arrived, and a
    /// stage waiting on one seat usually has nothing later. Recovery then falls
    /// to the blind ladder, whose attempts land in the seconds T+3, T+5, T+9,
    /// T+17 and T+33 — and the stage budget is 30 s, so the seat is voted out
    /// for a message that was in flight and would have arrived.
    ///
    /// The application knows what it is waiting for. This is how it says so.
    ///
    /// Returns false when the group or the peer is not found. Says nothing
    /// about whether the message existed: a request for a message the peer
    /// never sent is looked up in its send array and quietly does nothing,
    /// which is exactly why this cannot help a seat that is genuinely silent.
    pub fn request_missing(&self, group: u32, peer_key: &[u8; 32]) -> bool {
        // SAFETY: `peer_key` is 32 bytes, which is `TOX_PUBLIC_KEY_SIZE`.
        unsafe { sys::tox_group_peer_request_missing(self.ptr, group, peer_key.as_ptr()) }
    }

    /// How many of `peer`'s messages are waiting behind a hole.
    ///
    /// Non-zero means the peer is sending and the carrier has not finished
    /// delivering — which is the one thing that separates *late* from *silent*,
    /// and the thing a timeout vote must know before it accuses a seat.
    pub fn recv_pending(&self, group: u32, peer_key: &[u8; 32]) -> u16 {
        // SAFETY: as above; the call only reads.
        unsafe { sys::tox_group_peer_recv_pending(self.ptr, group, peer_key.as_ptr()) }
    }

    pub fn peer_key(&self, group: u32, peer: u32) -> Result<[u8; 32], Failed> {
        let mut out = [0u8; 32];
        let mut err: c_int = 0;
        // SAFETY: the buffer is exactly `TOX_PUBLIC_KEY_SIZE`.
        let ok = unsafe {
            sys::tox_group_peer_get_public_key(self.ptr, group, peer, out.as_mut_ptr(), &mut err)
        };
        if ok && err == 0 {
            Ok(out)
        } else {
            Err(Failed::Api {
                call: "tox_group_peer_get_public_key",
                error: err,
            })
        }
    }

    /// Remove a peer the roster no longer seats. Founder only.
    pub fn kick(&mut self, group: u32, peer: u32) -> Result<(), Failed> {
        let mut err: c_int = 0;
        // SAFETY: valid pointer; both numbers are toxcore's own handles.
        let ok = unsafe { sys::tox_group_kick_peer(self.ptr, group, peer, &mut err) };
        Self::ok(ok && err == 0, "tox_group_kick_peer", err)
    }

    /// Leave the group, which is what leaving the table does.
    pub fn leave(&mut self, group: u32) -> Result<(), Failed> {
        let mut err: c_int = 0;
        // SAFETY: valid pointer; a null part message with zero length is the
        // documented way to leave without one.
        let ok = unsafe { sys::tox_group_leave(self.ptr, group, std::ptr::null(), 0, &mut err) };
        Self::ok(ok && err == 0, "tox_group_leave", err)
    }

    fn ok(good: bool, call: &'static str, error: c_int) -> Result<(), Failed> {
        if good {
            Ok(())
        } else {
            Err(Failed::Api { call, error })
        }
    }
}

/// # Safety
///
/// `Tox` owns its pointer exclusively — nothing else holds a copy, `tox_kill`
/// runs exactly once in [`Drop`], and every method takes `&self` or `&mut self`
/// so the borrow checker already forbids two threads touching one instance at
/// the same time. What `Send` adds is the right to *move* it to another thread,
/// which `tox::table` needs: `tox_iterate` wants a steady loop on one thread of
/// its own.
///
/// **`Sync` is deliberately not implemented.** Two threads calling into one
/// instance concurrently is exactly what toxcore forbids without its
/// experimental thread-safety option, which this client does not set.
unsafe impl Send for Tox {}

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

    /// The group exists locally before anybody is connected to anything.
    ///
    /// `tox_group_new` is local state, so this runs offline and in a
    /// millisecond — which matters, because it is the part of the group flow a
    /// test can check without a network, and the rest cannot be checked without
    /// one.
    #[test]
    fn a_group_is_created_and_has_a_chat_id() {
        let mut tox = Tox::new().expect("a tox instance");
        let group = tox.new_group("TwoNet", "host").expect("a group");
        let id = tox.chat_id(group).expect("its chat id");
        assert!(
            id.iter().any(|b| *b != 0),
            "a chat id of thirty-two zero bytes is one nothing wrote"
        );
        assert_eq!(tox.chat_id(group).unwrap(), id, "and it does not move");

        // A second group is a second identity, which is what makes a chat id
        // worth publishing in an advertisement.
        let other = tox.new_group("Other", "host").expect("a second group");
        assert_ne!(tox.chat_id(other).unwrap(), id);

        tox.leave(group).expect("leaving is allowed");
    }

    /// The MTU is refused rather than truncated.
    ///
    /// `table::fragment` cuts every message to fit, so nothing on the
    /// protocol's own path reaches this — it is here so that a caller which
    /// forgets loses a packet loudly instead of sending most of one.
    #[test]
    fn a_packet_over_the_mtu_is_refused() {
        let mut tox = Tox::new().expect("a tox instance");
        let group = tox.new_group("TwoNet", "host").expect("a group");
        let too_big = vec![0u8; sys::TOX_GROUP_MAX_CUSTOM_LOSSLESS_PACKET_LENGTH + 1];
        assert_eq!(
            tox.send(group, &too_big),
            Err(Failed::TooLong {
                len: too_big.len()
            })
        );
        // And the size `table::fragment` actually produces is accepted by the
        // length check. It fails later for want of a peer, which is a different
        // refusal and is the one that proves the bound is not the blocker.
        let biggest = vec![0u8; sys::TOX_GROUP_MAX_CUSTOM_LOSSLESS_PACKET_LENGTH];
        assert!(
            !matches!(tox.send(group, &biggest), Err(Failed::TooLong { .. })),
            "a full packet must not be refused for its size"
        );
    }

    /// Two instances add each other from public keys alone, with no request and
    /// nothing to accept — which is what makes "a joining player is
    /// automatically invited" possible without a click.
    #[test]
    fn two_instances_add_each_other_without_a_request() {
        let mut a = Tox::new().expect("a");
        let mut b = Tox::new().expect("b");
        // The first 32 bytes of a Tox address are the public key; the rest is
        // nospam and a checksum, which `tox_friend_add_norequest` does not take.
        let a_key: [u8; 32] = a.address()[..32].try_into().unwrap();
        let b_key: [u8; 32] = b.address()[..32].try_into().unwrap();

        let fb = a.add_friend(&b_key).expect("a adds b");
        let fa = b.add_friend(&a_key).expect("b adds a");
        assert_eq!(fb, 0, "the first friend is number zero");
        assert_eq!(fa, 0);

        // No friend request was sent, so neither reports one. `iterate` is
        // called because that is the only thing that would deliver one.
        for _ in 0..5 {
            for e in a.iterate() {
                assert_ne!(e, Event::FriendRequestIgnored, "nothing was requested");
            }
            for e in b.iterate() {
                assert_ne!(e, Event::FriendRequestIgnored);
            }
        }
    }

    /// **The whole group flow over a real network**: two instances befriend
    /// each other, connect, the founder invites, the joiner accepts, and a
    /// custom lossless packet crosses.
    ///
    /// `#[ignore]` because it needs the network and takes tens of seconds — a
    /// friend connection over local discovery is usually seconds, over the DHT
    /// it is not. Run it deliberately:
    ///
    /// ```text
    /// cargo test --features tox -- --ignored the_group_carries_a_packet
    /// ```
    ///
    /// It is the first end-to-end evidence D-019 can have, and it is here
    /// rather than in `tests/` because it needs `sys` and the private helpers.
    #[test]
    #[ignore = "needs a network and tens of seconds"]
    fn the_group_carries_a_packet() {
        let mut a = Tox::new().expect("a");
        let mut b = Tox::new().expect("b");
        let a_key: [u8; 32] = a.address()[..32].try_into().unwrap();
        let b_key: [u8; 32] = b.address()[..32].try_into().unwrap();
        let friend_of_a = a.add_friend(&b_key).expect("a adds b");
        b.add_friend(&a_key).expect("b adds a");

        let group = a.new_group("TwoNet", "host").expect("a group");
        let mut invited = false;
        let mut joined: Option<u32> = None;
        let mut got: Option<Vec<u8>> = None;
        let payload = b"the deck is shuffled and sealed";

        // Sixty seconds of turning both loops. Long, because a friend
        // connection is not instant and there is nothing useful to do until it
        // is up.
        for _ in 0..6_000 {
            for e in a.iterate() {
                if let Event::FriendConnection { friend, status } = e {
                    // Up, either over UDP or through a TCP relay. Which one it
                    // is does not matter here and is exactly the thing that
                    // matters for D-019 across two networks.
                    if friend == friend_of_a && status != 0 && !invited {
                        a.invite(group, friend).expect("the invitation goes");
                        invited = true;
                    }
                }
            }
            for e in b.iterate() {
                match e {
                    Event::GroupInvite { friend, invite } => {
                        joined = Some(
                            b.accept_invite(friend, &invite, "player")
                                .expect("the invitation is accepted"),
                        );
                    }
                    Event::GroupPacket { data, .. } => got = Some(data),
                    _ => {}
                }
            }
            if joined.is_some() && got.is_none() {
                // Sent every turn until one arrives: the joiner is in the group
                // before the founder knows it, so the first few are sent to
                // nobody.
                let _ = a.send(group, payload);
            }
            if got.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(invited, "the friend connection never came up");
        assert!(joined.is_some(), "the invitation was never accepted");
        assert_eq!(
            got.as_deref(),
            Some(payload.as_slice()),
            "a custom lossless packet crossed the group"
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
