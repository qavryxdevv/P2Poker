//! Persistent identity. DPAPI on Windows as a convenience keyslot, plus a
//! portable passphrase-derived keyslot, because section 22 requires the profile
//! to survive being copied to another machine and DPAPI does not.
