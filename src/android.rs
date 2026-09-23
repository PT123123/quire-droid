//! The Android entry point (M9.pre).
//!
//! The Activity loads the crate's cdylib — `libquire_shell.so`, named after the
//! `[lib]` target, which is also what the manifest's `android.app.lib_name`
//! carries — and calls `android_main`; from there the app is the desktop app —
//! one start-up sequence in `app::launcher`, one set of Slint components, one
//! database contract. What this file owns is the three things the platform
//! answers differently: where the files go, which backend owns the window, and
//! how much stack the first layout pass needs.

/// The name the loader looks for; `android-activity` hands over the `AndroidApp`
/// that everything else on this platform is reached through.
#[no_mangle]
pub fn android_main(app: slint::android::AndroidApp) {
    let start = std::time::Instant::now();

    // Where the library lives: `internalDataPath` is the app's private
    // `/data/user/0/<pkg>/files`, the only writable directory an app with no
    // permissions has. The `Quire` inside it is the same folder name a desktop
    // install makes under `%APPDATA%`, so the placement rules in `data_location`
    // see a per-user directory on both platforms and the shared code never asks
    // which one it is on.
    match app.internal_data_path() {
        Some(dir) => crate::platform::set_data_dir(dir.join("Quire")),
        // Loud on purpose (docs/ANDROID_NOTES.md, M9.pre): with no answer here
        // the placement rules fall back to the working directory, which on
        // Android is not writable, and the session would run in memory while
        // looking like it had saved.
        None => eprintln!("quire: the activity gave us no internal data path"),
    }

    // Installs Slint's Android platform backend. Nothing may construct a
    // component before this, including the `AppWindow::new` further down the
    // launcher.
    if let Err(e) = slint::android::init(app) {
        eprintln!("quire: the android backend could not be installed: {e}");
        return;
    }

    // The event loop runs on *this* thread, and that is the platform's rule
    // rather than a preference — measured on a TB320FC, Android 15, where the
    // desktop shape below reported `The Slint platform was initialized in
    // another thread` and the Activity finished before a frame was drawn. Two
    // reasons, both structural: `set_platform` files the Slint context in a
    // thread-local, so `AppWindow::new` on a child thread finds no platform at
    // all; and the backend's `run_event_loop` calls `AndroidApp::poll_events`,
    // which android-activity documents as android_main-thread-only and panics
    // anywhere else. The desktop's "move the UI onto a big thread" shape
    // (ADR-0009) is therefore unusable here, and the 8 MB it buys is bought back
    // from the ELF init array below instead.
    //
    // Blocking until the loop ends is correct: this thread returning is the
    // Activity ending.
    match crate::app::launcher::run(start, true) {
        Ok(()) => {}
        Err(e) => eprintln!("quire: {e}"),
    }
}

/// Give `android_main` the 8 MB stack ADR-0009 asks for, before the thread that
/// runs it exists.
///
/// `android_native_app_glue` hands `android_main` a thread of its own making —
/// `android-activity` 0.6.1 spawns it with a bare `std::thread::spawn`, so its
/// size is std's default — and that default, measured on a TB320FC / Android 15,
/// is a 2 MiB mapping (`anon:stack_and_tls:<tid>`, `0x203000` rw-), which the
/// first instantiation of the component tree overruns: the tombstone reads
/// `Cause: stack pointer is not in a rw map; likely due to stack overflow`, with
/// `sp` sitting on the guard page below it. The desktop thread asks for 8 MB for
/// the same reason (ADR-0009), and a thread's stack is fixed when it is created,
/// so the number has to be chosen before `android_main` runs.
///
/// `RUST_MIN_STACK` is std's own lever for that default, read once at the first
/// spawn. It must be set from the ELF init array rather than from `android_main`
/// — by then the thread already exists — and the array runs when the dynamic
/// linker loads this cdylib, which the framework does before it calls
/// `ANativeActivity_onCreate` and therefore before `android-activity` spawns
/// anything. A `JNI_OnLoad` hook does not work here and is not kept: it is
/// exported and loadable, but the measurement above is what a 2 MiB thread looks
/// like, so whatever the JVM does with that symbol, this variable was not set by
/// the time std read it.
#[used]
#[link_section = ".init_array"]
static RAISE_ANDROID_MAIN_STACK: extern "C" fn() = raise_android_main_stack;

extern "C" fn raise_android_main_stack() {
    std::env::set_var("RUST_MIN_STACK", (8 * 1024 * 1024).to_string());
}
