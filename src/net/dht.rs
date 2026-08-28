//! Global discovery over BitTorrent Mainline DHT, `mainline = "=8.0.0"`.
//!
//! Only `announce_peer` and `get_peers` under one fixed `LOBBY_INFOHASH`. Game
//! state never travels here.
//!
//! The peer list is a hint about where to try, never a claim about who is
//! there - identity is settled by the libp2p handshake (D-003).
//!
//! Note: `mainline` panics on IPv6 - `unimplemented!` in `common/id.rs:93` and
//! `rpc/socket.rs:54` - so v6 addresses must be filtered before they reach it.
