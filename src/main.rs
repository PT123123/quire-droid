// Quire — a local, GPU-accelerated, Notion-like document workspace.
// The desktop binary: one `main`, one crate attribute. Everything that happens
// at start-up lives in the library's launcher, which the Android entry calls
// through the same door (src/app/launcher.rs, src/android.rs).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The library is `quire_shell`, not `quire`, because a cdylib and a binary that
// share a target name share their PDB too — see the `[lib]` comment in
// Cargo.toml. The alias is the whole cost: every path below still reads `quire::`.
use quire_shell as quire;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    quire::app::launcher::desktop_main()
}
