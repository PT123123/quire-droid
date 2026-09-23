// LAN sync between two Quire installs (desktop ↔ Android), modelled on
// aw-server-plus's aw-sync: UDP discovery, a trust-on-first-use pairing,
// full-snapshot exchange over HTTP, and a three-way merge whose loser is
// logged rather than dropped. The transport is the same dependency-free
// std::net HTTP the LAN share (`services::lan_server`) uses — no async
// runtime, no new heavy crates, one code path both platforms compile.
//
// Ownership map:
//   model     — the wire structs and their conversions to/from `core` rows
//   merge     — the pure three-way merge (unit-tested, no app types)
//   transport — the HTTP client and the UDP discovery halves
//   server    — the inbound HTTP endpoints
//   engine    — threads, the UI-thread pump, and the peer/config persistence
//
// The sessions's own state is `Rc`-bound to the UI thread, so every job that
// reads or writes the workspace travels through a channel to a Slint timer on
// that thread (`engine::install`); background threads only ever move bytes
// and JSON.

pub mod engine;
pub mod merge;
pub mod model;
pub mod server;
pub mod transport;

/// The TCP port the sync server listens on. The read-only LAN share keeps
/// 5877; sync sits next to it.
pub const SYNC_PORT: u16 = 5878;
/// The UDP port discovery announcements go out on.
pub const DISCOVERY_PORT: u16 = 5879;
