//! The raw `c-toxcore` entry points this client uses, and no others.
//!
//! Hand-written rather than bindgen-generated. `tox.h` is 4 700 lines and this
//! client needs about thirty calls of it: a generated binding would be forty
//! thousand lines of surface nobody reads, in which the handful that matter are
//! indistinguishable from the hundreds that do not. Every declaration here is
//! one somebody chose, and a declaration that does not match the header is a
//! link error rather than a silent corruption — the symbols are C, so the
//! linker checks the names and nothing checks the types.
//!
//! **Therefore the rule for this file: never guess a signature.** Copy it from
//! `vendor/c-toxcore/toxcore/tox.h` and leave the line number in the comment.
//! There is no compiler on either side of this boundary that will catch a
//! wrong one.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_void};

/// `tox.h:277`
pub const TOX_ADDRESS_SIZE: usize = 38;
/// `tox.h:212`
pub const TOX_PUBLIC_KEY_SIZE: usize = 32;
/// `tox.h:3127`
pub const TOX_GROUP_CHAT_ID_SIZE: usize = 32;

/// Opaque. Never constructed here; only ever behind a pointer from `tox_new`.
#[repr(C)]
pub struct Tox {
    _private: [u8; 0],
}

/// `tox_options.h:90`. The library's own log line, which is discarded unless
/// this is set.
///
/// **Every `LOGGER_WARNING` and `LOGGER_ERROR` in the vendored tree is a no-op
/// without it** (`logger.c`), and the group invite path has six distinct
/// failure branches — four that log and two that return with no diagnostic at
/// all. None of them could reach an operator, which is why `S1-AA` shape (i)
/// looks from outside like a healthy transport that simply will not admit a
/// peer.
pub type tox_log_cb = Option<
    unsafe extern "C" fn(
        tox: *mut Tox,
        level: c_int,
        file: *const c_char,
        line: u32,
        func: *const c_char,
        message: *const c_char,
        user_data: *mut c_void,
    ),
>;

/// Opaque, from `tox_options_new`.
#[repr(C)]
pub struct Tox_Options {
    _private: [u8; 0],
}

/// `tox.h` — `Tox_Err_New`. Only `OK` is named because only `OK` is a
/// success; everything else is reported by its number, which is what the
/// header's own enum order gives.
pub const TOX_ERR_NEW_OK: c_int = 0;
pub const TOX_ERR_BOOTSTRAP_OK: c_int = 0;
pub const TOX_ERR_OPTIONS_NEW_OK: c_int = 0;

/// `tox_options.h:54`
pub const TOX_SAVEDATA_TYPE_NONE: c_int = 0;
/// `tox_options.h:64`. Thirty-two bytes of secret key, and the identity that
/// follows from it is the same every start — which is what makes a two-machine
/// measurement reproducible instead of needing each end to learn the other's
/// key at run time.
pub const TOX_SAVEDATA_TYPE_SECRET_KEY: c_int = 2;

extern "C" {
    /// `tox_options.h:452`
    pub fn tox_options_new(error: *mut c_int) -> *mut Tox_Options;
    /// `tox_options.h:460`
    pub fn tox_options_free(options: *mut Tox_Options);
    /// `tox_options.h:377`
    pub fn tox_options_set_log_callback(options: *mut Tox_Options, callback: tox_log_cb);
    /// `tox_options.h:319`
    pub fn tox_options_set_ipv6_enabled(options: *mut Tox_Options, enabled: bool);
    /// `tox_options.h:323`
    pub fn tox_options_set_udp_enabled(options: *mut Tox_Options, enabled: bool);
    /// `tox_options.h:327`
    pub fn tox_options_set_local_discovery_enabled(options: *mut Tox_Options, enabled: bool);
    /// `tox_options.h:360`
    pub fn tox_options_set_hole_punching_enabled(options: *mut Tox_Options, enabled: bool);
    /// `tox_options.h:348`
    pub fn tox_options_set_start_port(options: *mut Tox_Options, port: u16);
    /// `tox_options.h:352`
    pub fn tox_options_set_end_port(options: *mut Tox_Options, port: u16);
    /// `tox_options.h:364`
    pub fn tox_options_set_savedata_type(options: *mut Tox_Options, savedata_type: c_int);
    /// `tox_options.h:368`. The pointer must outlive `tox_new`; toxcore reads
    /// it there and does not copy.
    pub fn tox_options_set_savedata_data(
        options: *mut Tox_Options,
        data: *const u8,
        length: usize,
    ) -> bool;

    /// `tox.h:504`
    pub fn tox_new(options: *const Tox_Options, error: *mut c_int) -> *mut Tox;
    /// `tox.h:513`
    pub fn tox_kill(tox: *mut Tox);
    /// `tox.h:584`
    pub fn tox_bootstrap(
        tox: *mut Tox,
        host: *const c_char,
        port: u16,
        public_key: *const u8,
        error: *mut c_int,
    ) -> bool;
    /// `tox.h:600`. A **TCP relay**, which is a different list from the DHT
    /// nodes `tox_bootstrap` takes: with UDP disabled, bootstrapping reaches
    /// nothing without at least one of these. Measured 2026-08-31 — with
    /// `udp_enabled(false)` and no relay added, `tox_self_get_connection_status`
    /// stays at `none` for ever and it looks exactly like a dead network.
    pub fn tox_add_tcp_relay(
        tox: *mut Tox,
        host: *const c_char,
        port: u16,
        public_key: *const u8,
        error: *mut c_int,
    ) -> bool;
    /// `tox.h:672`
    pub fn tox_iteration_interval(tox: *const Tox) -> u32;
    /// `tox.h:680`
    pub fn tox_iterate(tox: *mut Tox, user_data: *mut c_void);
    /// `tox.h:698`
    pub fn tox_self_get_address(tox: *const Tox, address: *mut u8);
    /// `tox.h:646`. `Tox_Connection`: 0 none, 1 TCP, 2 UDP.
    pub fn tox_self_get_connection_status(tox: *const Tox) -> c_int;
}

// ---------------------------------------------------------------------------
// Friends
// ---------------------------------------------------------------------------
//
// A group invitation needs a **friend number**, not a public key
// (`tox_group_invite_friend`, `tox.h:4693`), so the founder and the joiner must
// be Tox friends before an invitation is possible at all. That would be a user
// interaction and a round trip, except for `tox_friend_add_norequest`: it adds
// a friend from a 32-byte public key alone, with no request sent and nothing to
// accept. Both ends add each other from the ratified roster and the invitation
// follows.
//
// This is what keeps D-019's central claim true. The decision says group
// discovery through Tox's DHT is not on the critical path *because members
// arrive by invitation* - and joining by `chat_id` with `tox_group_join` IS
// that path, the one measured on 2026-08-27 to work only while a group is new.

extern "C" {
    /// `tox.h:958`
    pub fn tox_friend_add_norequest(
        tox: *mut Tox,
        public_key: *const u8,
        error: *mut c_int,
    ) -> u32;
    /// `tox.h:1489`
    pub fn tox_callback_friend_request(tox: *mut Tox, callback: tox_friend_request_cb);
    /// `tox.h:1306`
    pub fn tox_callback_friend_connection_status(
        tox: *mut Tox,
        callback: tox_friend_connection_status_cb,
    );
}

/// `tox.h:1477`
pub type tox_friend_request_cb = Option<
    unsafe extern "C" fn(
        tox: *mut Tox,
        public_key: *const u8,
        message: *const u8,
        length: usize,
        user_data: *mut c_void,
    ),
>;

/// `tox.h:1292`. `Tox_Connection` is an enum: 0 none, 1 TCP, 2 UDP.
pub type tox_friend_connection_status_cb = Option<
    unsafe extern "C" fn(
        tox: *mut Tox,
        friend_number: u32,
        connection_status: c_int,
        user_data: *mut c_void,
    ),
>;

// ---------------------------------------------------------------------------
// Groups (NGC)
// ---------------------------------------------------------------------------

/// `tox.h:3162`. The group announces itself in the DHT, which is the path that
/// decays; this client creates **private** groups and invites into them.
pub const TOX_GROUP_PRIVACY_STATE_PUBLIC: c_int = 0;
/// `tox.h:3173`
pub const TOX_GROUP_PRIVACY_STATE_PRIVATE: c_int = 1;

/// `tox.h:3106`. The reason `table::fragment` exists.
pub const TOX_GROUP_MAX_CUSTOM_LOSSLESS_PACKET_LENGTH: usize = 1373;

extern "C" {
    /// `tox.h:3330`
    pub fn tox_group_new(
        tox: *mut Tox,
        privacy_state: c_int,
        group_name: *const u8,
        group_name_length: usize,
        name: *const u8,
        name_length: usize,
        error: *mut c_int,
    ) -> u32;
    /// `tox.h:3539`
    pub fn tox_group_leave(
        tox: *mut Tox,
        group_number: u32,
        part_message: *const u8,
        length: usize,
        error: *mut c_int,
    ) -> bool;
    /// `tox.h:4040`
    pub fn tox_group_get_chat_id(
        tox: *const Tox,
        group_number: u32,
        chat_id: *mut u8,
        error: *mut c_int,
    ) -> bool;
    /// **`patches/0011`.** Ask one peer to re-send the message we are missing
    /// from it — the fast repair path, answered by an immediate retransmission
    /// with no backoff, against the blind ladder's T+3/+5/+9/+17/+33.
    ///
    /// Costs one small lossy packet and toxcore throttles it to one per second
    /// per connection, so calling it on every tick is safe. It does nothing at
    /// all if the peer never sent the message, which is what keeps it from
    /// helping a seat that is simply silent.
    pub fn tox_group_peer_request_missing(
        tox: *const Tox,
        group_number: u32,
        peer_public_key: *const u8,
    ) -> bool;
    /// **`patches/0011`.** How many messages from this peer are stalled behind
    /// a hole: positive evidence that it is talking and the carrier is
    /// mid-delivery. Reads local memory and sends nothing.
    pub fn tox_group_peer_recv_pending(
        tox: *const Tox,
        group_number: u32,
        peer_public_key: *const u8,
    ) -> u16;
    /// **`patches/0032`.** Seconds since this peer's last packet, by the
    /// group's clock; `u64::MAX` when unknown. Reads local memory only.
    pub fn tox_group_peer_quiet_secs(
        tox: *const Tox,
        group_number: u32,
        peer_public_key: *const u8,
    ) -> u64;
    /// **`patches/0033`.** The friend number this member's invitation
    /// travelled over; `u32::MAX` when unknown. Reads local memory only.
    pub fn tox_group_peer_friend_number(
        tox: *const Tox,
        group_number: u32,
        peer_public_key: *const u8,
    ) -> u32;
    /// `tox.h:4475`
    pub fn tox_group_send_custom_packet(
        tox: *const Tox,
        group_number: u32,
        lossless: bool,
        data: *const u8,
        length: usize,
        error: *mut c_int,
    ) -> bool;
    /// `tox.h:4693`
    pub fn tox_group_invite_friend(
        tox: *const Tox,
        group_number: u32,
        friend_number: u32,
        error: *mut c_int,
    ) -> bool;
    /// `tox.h:4766`
    pub fn tox_group_invite_accept(
        tox: *mut Tox,
        friend_number: u32,
        invite_data: *const u8,
        length: usize,
        name: *const u8,
        name_length: usize,
        password: *const u8,
        password_length: usize,
        error: *mut c_int,
    ) -> u32;
    /// `tox.h:5393`
    pub fn tox_group_kick_peer(
        tox: *const Tox,
        group_number: u32,
        peer_id: u32,
        error: *mut c_int,
    ) -> bool;
    /// `tox.h:3840`
    /// `tox.h:1283`. What toxcore thinks of one friendship: `0` none, `1` TCP,
    /// `2` UDP. The one number that says whether an invitation has anywhere to
    /// go, and it was not asked for until `S1-N`.
    pub fn tox_friend_get_connection_status(
        tox: *const Tox,
        friend_number: u32,
        error: *mut c_int,
    ) -> c_int;

    /// `tox.h:989`. Forget a friend, which discards toxcore's cached idea of
    /// where it is. See `Tox::forget_friend`.
    pub fn tox_friend_delete(tox: *mut Tox, friend_number: u32, error: *mut c_int) -> bool;

    /// `tox.h:3698`. This client's own peer id inside the group, so a scan that
    /// counts members can leave itself out of the count.
    pub fn tox_group_self_get_peer_id(
        tox: *const Tox,
        group_number: u32,
        error: *mut c_int,
    ) -> u32;

    pub fn tox_group_peer_get_public_key(
        tox: *const Tox,
        group_number: u32,
        peer_id: u32,
        public_key: *mut u8,
        error: *mut c_int,
    ) -> bool;

    /// `tox.h:4793`
    pub fn tox_callback_group_invite(tox: *mut Tox, callback: tox_group_invite_cb);
    /// `tox.h:4617`
    pub fn tox_callback_group_custom_packet(tox: *mut Tox, callback: tox_group_custom_packet_cb);
    /// `tox.h:4885`
    pub fn tox_callback_group_self_join(tox: *mut Tox, callback: tox_group_self_join_cb);
    /// `tox.h:4925`
    pub fn tox_callback_group_join_fail(tox: *mut Tox, callback: tox_group_join_fail_cb);
    /// `tox.h:4808`
    pub fn tox_callback_group_peer_join(tox: *mut Tox, callback: tox_group_peer_join_cb);
    /// `tox.h:4872`
    pub fn tox_callback_group_peer_exit(tox: *mut Tox, callback: tox_group_peer_exit_cb);
}

/// `tox.h:4801`. Fires when another peer becomes **confirmed** — the same flag
/// the group send path requires, so this callback set is exactly the set a
/// broadcast will reach.
pub type tox_group_peer_join_cb = Option<
    unsafe extern "C" fn(tox: *mut Tox, group_number: u32, peer_id: u32, user_data: *mut c_void),
>;

/// `tox.h:4862`.
#[allow(clippy::type_complexity)]
pub type tox_group_peer_exit_cb = Option<
    unsafe extern "C" fn(
        tox: *mut Tox,
        group_number: u32,
        peer_id: u32,
        exit_type: c_int,
        name: *const u8,
        name_length: usize,
        part_message: *const u8,
        part_message_length: usize,
        user_data: *mut c_void,
    ),
>;

/// `tox.h:4877`. Fires **once**, when this client's own join completes.
pub type tox_group_self_join_cb =
    Option<unsafe extern "C" fn(tox: *mut Tox, group_number: u32, user_data: *mut c_void)>;

/// `tox.h:4918`. Fires when a join attempt is abandoned, with a reason.
pub type tox_group_join_fail_cb = Option<
    unsafe extern "C" fn(tox: *mut Tox, group_number: u32, fail_type: c_int, user_data: *mut c_void),
>;

/// `tox.h:4780`
pub type tox_group_invite_cb = Option<
    unsafe extern "C" fn(
        tox: *mut Tox,
        friend_number: u32,
        invite_data: *const u8,
        invite_data_length: usize,
        group_name: *const u8,
        group_name_length: usize,
        user_data: *mut c_void,
    ),
>;

/// `tox.h:4608`
pub type tox_group_custom_packet_cb = Option<
    unsafe extern "C" fn(
        tox: *mut Tox,
        group_number: u32,
        peer_id: u32,
        data: *const u8,
        data_length: usize,
        user_data: *mut c_void,
    ),
>;
