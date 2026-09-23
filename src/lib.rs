// Quire library crate: all modules live here so `tests/` integration tests
// and the thin binary share one compiled unit. The binary (main.rs) only
// parses args, builds the window, and runs the event loop.
//
// The model and the store moved out into their own repository as `quire-core`
// (ADR-0093 drew the boundary, ADR-0094 moved it out of this checkout), which is
// the crate the Android port consumes. These re-exports are what makes that move
// invisible from inside the shell: `crate::core::…` here and
// `quire::services::…` from an integration test both resolve through them, so
// the split is a dependency edge and not a 950-line path rewrite. Delete them
// when the shell starts naming `quire_core::` directly.

pub mod app;
pub mod platform;

// The Android entry (M9.pre). It is the crate's `main` on that platform — the
// desktop `main` in src/main.rs and this one are the only two entry points, and
// each exists solely to hand the launcher its platform's answers.
#[cfg(target_os = "android")]
pub mod android;

pub use quire_core::{core, services, storage, testing};

slint::include_modules!();
