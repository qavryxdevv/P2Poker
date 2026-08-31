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

extern "C" {
    /// `tox_options.h:452`
    pub fn tox_options_new(error: *mut c_int) -> *mut Tox_Options;
    /// `tox_options.h:460`
    pub fn tox_options_free(options: *mut Tox_Options);
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
    /// `tox.h:672`
    pub fn tox_iteration_interval(tox: *const Tox) -> u32;
    /// `tox.h:680`
    pub fn tox_iterate(tox: *mut Tox, user_data: *mut c_void);
    /// `tox.h:698`
    pub fn tox_self_get_address(tox: *const Tox, address: *mut u8);
}
