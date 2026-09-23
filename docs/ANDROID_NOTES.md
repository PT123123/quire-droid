# M9 Android — pre-research notes, then the first slice that shipped

Written 2026-09-19 against Slint 1.18. Windows RC comes first (SPEC §二十八);
this page was the homework for the day we start, and since 2026-09-23 it is also
the record of that day.

Updated 2026-09-20 after Track B measured the toolchain instead of trusting
this page. The evidence — three builds, their commands and their exact output —
is in `.scratch/m9/report.md`; the changed claims carry a `measured` marker
below. **The headline: the stack we doubted builds and links for Android, and
the one hard blocker in our dependency graph is a file-dialog crate this page
never mentions.**

## M9.pre is delivered (2026-09-23)

The shell now compiles and links for Android, and the desktop is unchanged
where nothing asked it to move. Acceptance for this milestone, as written below
before it existed, was "`cargo check --target x86_64-linux-android --all-targets`
clears our own crates, not just the graph" — it does, for **both** ABIs. One hole
in that phrase, found while writing it down: `--all-targets` with
`--no-default-features` does not build `quire-shot` at all, because that bin asks
for the `software` feature, so the line means "the library, `quire-typing` and
both test harnesses" rather than "everything in the manifest".

What is in the tree:

- **Dependencies split by target**, not by feature: `slint` appears twice in
  `Cargo.toml`, once under `[target.'cfg(not(target_os = "android"))'.dependencies]`
  with the winit backend and once under the Android cfg with
  `backend-android-activity-06` + `renderer-skia`. `rfd` is on the desktop side
  only. The `[features]` renderer matrix is untouched, which is why an Android
  cargo call carries `--no-default-features` — the default arm is FemtoVG and
  Slint cfg's FemtoVG out of this platform (ADR-0095). That is the *only* flag
  the platform needs: because the Android dependency section names its renderer
  itself, `cargo check --target … --no-default-features` with nothing else
  resolves against Skia/GLES (measured; the longer `--features skia` spelling
  works too, and asks for what the manifest already says).
- **`src/platform/picker.rs`** — the file-dialog seam. Seven call sites in
  `app/controller.rs` built dialogs with `rfd` directly (two when this page was
  written; the block kit added the rest); all seven now build a `Picker`, and
  one helper `ask()` turns the answer into "the path, or nothing happened". On
  Android the answer is `Unsupported` and the notice bar says which half is
  missing ("cannot open…" vs "cannot save…"), so no button is silently dead.
  The SAF door is M9.5.
- **`src/platform::data_dir()`** — the per-user directory the placement rules
  were missing, supplied by the shell: on Android from the Activity's
  `internal_data_path()` + `/Quire`, on desktop from the crate's own `%APPDATA%`
  read. `data_location::decide()` already took it as a parameter, so
  `quire-core` did not change for this. A phone that answers no data path says
  so loudly instead of running an in-memory session that looks saved.
- **`src/app/launcher.rs`** — the start-up sequence moved out of `main.rs`,
  which is now ten lines. `run(start, touch_mode)` is the whole launch for both
  platforms; `desktop_main()` keeps the 8 MB-stack thread (ADR-0009) and
  `src/android.rs` reproduces it on the other side, because the reason for that
  thread is Slint's recursive layout, not Windows.
- **One adaptive UI, not a second tree** — `UIState.touch-mode` (ADR-0096). It is
  declared once in `ui/Types.slint` and read in seven places across two files:
  `TopBar` drops its divider and the three window controls, `AppShell` swaps the
  layout rail for a drawer and mounts the thumb bar. `Theme.slint` contributes
  the 56 px bar height and no read of the flag. `ui/components/MobileBar.slint`
  is the only new component: five actions, all of them existing `UIState`
  callbacks, so the bar adds no Rust surface at all.

Build entries (there is no CI for this, on purpose):

```
powershell -File scripts\android-build.ps1 -Task check   # both ABIs, cargo check --all-targets
powershell -File scripts\android-build.ps1 -Task lib     # both ABIs, release cdylib — this one links
powershell -File scripts\android-build.ps1 -Task apk     # one APK through cargo-apk
```

What each of those can and cannot say:

- `check --all-targets` is the whole Android test suite this repository has.
  `cargo test --target …-linux-android` needs a device to run on, so the Android
  `cfg` branches — `picker::pick`'s `Unsupported` answer, `data_dir()` reading the
  `OnceLock`, `android_main` itself — are covered by the type checker and by
  nothing else. They are compiled, not exercised.
- `check` never links, which is why `-Task lib` exists: the claim under test is
  that a `cdylib` named `libquire_shell.so` comes out of the crate, and only a
  build can say that. The name is not cosmetic — the split between the library
  target and the binaries is what keeps them off each other's `.pdb`, and the
  APK's `android.app.lib_name` has to match the packaged file.
- `-Task apk` costs far more than `-Task lib` on the same tree, which is the
  reason to run the latter while iterating. Measured, first pass: 16 m 55 s and
  22 m 56 s for the two ABIs. Second pass, after one `Cargo.toml` edit that
  touched only `[package.metadata]`: 12 m 40 s and 7.6 s, with a single
  `Compiling quire` line in the log. The usual explanation — cargo-apk carries
  its own `RUSTFLAGS`, so its artifacts cannot share a cache with plain cargo
  builds — was **not** separated out here; the wall times are the fact, the
  reason is a hypothesis, and the pair is in `docs/PERFORMANCE.md`.
- The one thing here that can be *looked* at is `--touch` on the desktop binary:
  it draws the phone's shape in a Windows window, which is how the drawer, the
  thumb bar and the stripped title bar got read at all. It is not a test, and it
  is not a substitute for the 44 dp question below.
- `-Task apk` produced its first signed package on 2026-09-23: **24.29 MiB with both
  ABIs inside**, each ABI ≈12 MiB of that compressed and 31.3 MiB once installed. It is
  `target/release/apk/quire_shell.apk`, 25 473 461 bytes, named after the library target
  like the `.so` it carries; the first pass that same morning wrote the same package as
  `quire.apk` at 25 473 449 bytes, before the rename. The
  whole table is `docs/PERFORMANCE.md`'s M9.pre section.
- That last task has to be read from its artifact, not its exit code.
  cargo-apk 0.10 does not ask cargo what it built — it parses the manifest and packages
  the lib *and every `[[bin]]` it can see*, then reaches for a cdylib filename it does not
  have for a binary: `Bin is not compatible with Cdylib`
  (`cargo-subcommand-0.12.0/src/artifact.rs:51`), printed *after* the line that says it
  signed the APK, next to a 1 021-byte `quire-shot-unaligned.apk` that is the corpse of the
  attempt. `--lib` is the fix (`cargo apk build --release --lib`): one artifact in the
  list, no panic, and no desktop bin compiled for a phone. The script still throws unless
  `quire_shell.apk` is on disk and newer than the run, so a future failure that is not this one
  cannot hide behind the panic — and it earned that severity the same day, when the rename
  moved the output name out from under it and a fully signed APK sat in the directory while
  the task reported failure over the file it was looking for.

Corrections to this page, in the order the page got them wrong:

- `AndroidApp::data_dir()` **does not exist** in android-activity 0.6.1. The
  line below that recommended it was written from a doc memory; the method that
  answers is `internal_data_path()`.
- The `rfd` count was 2 call sites when it was measured and 7 when the seam was
  built. Both are true on their own date.
- `cargo-apk` is installed (0.10.0), so risk 4's tooling half is closed. Its
  hardware half is not — see below.
- **`[workspace]` is gone from `Cargo.toml`** (ADR-0095): cargo-apk 0.10 refuses
  to read a manifest that declares one, and the two entries of
  `[workspace.dependencies]` it existed for were inherited by nothing — the
  git dependency never saw them. `image` and `rusqlite` are pinned inline now.
- **`cargo check --target … --all-targets` does not build `quire-shot`.** That
  bin asks for the `software` feature through `required-features`, and an Android
  call carries `--no-default-features`, so the line means "the library,
  `quire-typing` and both test harnesses" — not "everything in the manifest".
- The release signing identity is a **generated throwaway** in `.scratch/`, not a
  keystore anyone would ship; `--release` would not package without one, and
  choosing a real identity is M9.1's decision.

What this slice does **not** claim: that any of it works on a phone. There is
still no device and no AVD (`adb devices` empty, no `system-images/`), the user
set the bar at "compiles, and watch performance" rather than "install it", so
everything below the linker is unverified: first paint, the IME, the drawer's
gestures, and every number in `docs/PERFORMANCE.md`'s Android section, which is
a size table and not a latency table. The M9.0 input spike and the read-only
viewer still stand as the next two gates, in that order.

## What ports for free

- **`src/core/**`** — the document model, commands, and history are pure
  Rust with no Slint and no I/O. They compile for any target as-is.
- **`src/storage/**`** — rusqlite bundled builds for Android (measured:
  `libsqlite3-sys` compiled and linked for `x86_64-linux-android` with NDK
  30.0.15729638); the DB path moves to the app's data dir — supplied by
  `android-activity`'s `AndroidApp::data_dir()`, which is already in the graph
  via Slint, so no separate `ndk-context` is needed. *Corrected 2026-09-23:*
  there is no `data_dir()` on that type in android-activity 0.6.1; the method
  that answers is `internal_data_path()`, and it is what `src/android.rs` uses.
  *New caveat:* the only
  environment read in the crate is `%APPDATA%`
  (`src/storage/data_location.rs:112`), and with no per-user directory the
  placement falls back to `appdata/quire.db` relative to the working directory
  — not writable on Android, so the app silently runs in memory. The seam is
  good news, though: `decide()`/`migration()` already take the per-user path as
  a parameter (`data_location.rs:166`), so this is "supply the path", not a
  policy rewrite.
- **The persistence contract** (ADR-0012) is UI-agnostic by design; the
  editor commands (SPEC §十四) carry over unchanged.
- *Measured 2026-09-20:* `src/core|storage|services` import Slint nowhere
  (0 files), and `src/platform/mod.rs` is 169 lines / 3 functions, each already
  cfg-gated with a "report failure instead of pretending" fallback. The one
  blocker the graph hits first is **`rfd`** — `Cargo.toml:10` pulls it
  unconditionally for two `FileDialog` call sites
  (`src/app/controller.rs:1776`, `:1791`), and it fails to *compile* for
  Android (12 errors, no backend). It is not an Android-runtime problem to
  discover later; it is one cfg-gated dependency plus two call sites routed
  through `platform/`. *Corrected 2026-09-23:* by the day the seam was built
  there were **seven** call sites, not two — the M10 block kit added five — and
  all seven go through `platform::picker`, which is 103 lines.

## What must be rebuilt

- **All of `ui/`** — the shell assumes a pointer, a 1280×800 window, a
  frameless title bar, and hover affordances. Android needs a
  bottom-bar/touch navigation, larger touch targets (≥44 px), and a
  single-column editor layout. *Measured size of that job:* 31 `has-hover`
  affordances, 25 `TouchArea` / 62 `clicked` handlers, 68 `in-out` properties on
  the `UIState` contract (`ui/Types.slint`, ~244 property declarations across
  `ui/`), and row heights of 28–32 px in the block menus / 36–40 px in the
  palette, sidebar and find bar — **the 52 px search result row is the only
  interactive element that clears the 44 dp minimum.** `PopupWindow` itself is
  fine there (8 components; upstream maintains Android popup cursor/handle/
  safe-area fixes), but note the compiler rule that a popup cannot be the
  exported entry file passed to `slint-build` — Quire's shape already respects
  it.
- *Partly rebuilt 2026-09-23:* the shell's own frame — chrome, navigation, the
  page list — turned out to need seven reads of one `touch-mode` property rather
  than a rewrite (ADR-0096), and window-size restore is skipped in touch mode,
  since a remembered 1280×800 is a desktop window fighting a phone. What the
  delivery did **not** touch is the number this bullet measured first: **186
  height literals under 44 px across 28 files of `ui/`**, 23 of them
  ≤ 2 px dividers, so **163 in the 3–43 px band** (`DatabaseView.slint` holds 38
  of them). The pattern is `(min-|max-)?height: [0-9]+px` over `ui/**/*.slint`,
  which reads 196 literals across 29 files. Not all 163 are touch targets — a
  20 px chevron inside a 36 px row is not one — which is exactly why that sweep
  waits for a device: ranking them from a desktop is guessing. *Corrected twice
  2026-09-23:* this line first read "221 across 25 files", from a pattern that
  also matched non-`px` heights; the "189 across 28 / 26 dividers" figure that
  replaced it did not reproduce under either of the two sweeps tried here (one
  that anchored `height:` to the start of the line, and so lost every `min-` and
  `max-` match, read 181). The recipe above is why the numbers now carry it. The
  `has-hover`
  count below it was stale the same way: **118 reads of `has-hover` across 29
  files of `ui/`** as of this re-count, where the note said 31.
- **The block renderer's hover model** — BlockHandle menus become
  long-press menus; the find bar moves to a system-style search field.
  *Partly closed 2026-09-23 (M9.a, ADR-0097): the long-press landed — a held
  press on a row opens the ⋮⋮ menu through the same callback the grip click
  uses, three guards keep a scroll from firing it, and the menu gains a
  touch-only "Insert below" row so the "+" has a door too. The three row
  menus stand at 44 dp in touch mode, with every Rust anchor multiplying the
  same number; the desktop baseline provably did not move (131 shared sweep
  scenes byte-identical against a `dc2399f` control build, the one new file
  `touch-menu.png`). Still open: the long-press itself has never been felt —
  headless has no press-and-hold, so it joins the IME spike as the two things
  a finger owes this milestone. Drag-reorder is still handle-only (the
  DragArea lives on the invisible strip; Move up/down in the menu is the
  phone's reorder path), and the rest of the sub-44 px inventory — database
  popups, settings rows, the file block's 26 px buttons — is desktop-sized.*
- **Window size persistence** — replaced by safe-area + fullscreen.

## Known risks (SPEC §二十八)

1. **Slint's Android TextInput** has open issues (history of composition/
   IME gaps). The editor's whole UX rides on it — budget a dedicated
   spike: a minimal IME test app on a real device before committing.
   *Confirmed, with names (11 open `a:platform-android` issues as of
   2026-09-20):* #9240 *Android TextInput issues with Microsoft SwiftKey
   Keyboard* — still open — reproduces items straight off
   `docs/IME_CHECKLIST.md` ("can't backspace/clear existing text", "Enter then
   backspace produces junk", "after clearing, nothing types"); #11810
   *Keyboard/input problems on Android* is open and newer; #6162/#6531 cover
   OSK events and misplaced selection.
2. **Our own caret model** (`cursor-position-byte-offset` +
   `set-selection-offsets`) is exercised the same way, but composition
   state handling may differ per keyboard (Gboard vs Samsung). *Measured
   supplement:* that property and its two-way `text <=>` binding do codegen and
   type-check for the Android target, so the risk is behaviour on hardware, not
   the build. Treat the Windows checklist as the test plan, run per keyboard.
3. **Performance** — FemtoVG on GLES vs the desktop GL path; re-run the
   scene matrix on-device before promising anything. *Corrected:* it is not
   FemtoVG. Slint 1.18 gates `i-slint-renderer-femtovg` behind
   `cfg(not(target_os = "android"))`; the Android renderer is **Skia on GLES
   (Ganesh)**, and `skia-bindings` fetches a prebuilt binary from GitHub rather
   than compiling Skia (so the first build of a given revision needs network).
   Two consequences: our `[features]` renderer matrix means something different
   on Android, and the desktop skia comparison it made mandatory is now
   measured in `docs/PERFORMANCE.md` (A2 follow-up #3) — Android numbers will
   be read against the **skia-on-OpenGL** desktop arm, since the default
   desktop skia build delivers no rendering-notifier events at all and has no
   first-paint number. Same instrument, same caveat on device: the notifier
   only fires on surfaces whose `Surface::with_graphics_api` is real, so a
   silent `--measure-startup` run on a phone means "this surface does not
   notify", never "no frame was drawn".
4. **Hardware, added 2026-09-20.** This is the critical path, not code: `adb
   devices` is empty and the local SDK has no `system-images/`, so there is no
   device and no AVD to make one. `cargo-apk` is not installed either
   (`cargo-ndk`, NDK 25–30 and platforms 34–37 are).
   *Still the critical path 2026-09-23.* `cargo-apk` 0.10 is now installed and
   a signed two-ABI APK is on disk — `target/release/apk/quire_shell.apk`, 24.29 MiB,
   ≈12 MiB of it the one ABI a phone fetches and 31.3 MiB once installed
   (`docs/PERFORMANCE.md`, M9.pre section). That was the cheap half of this line.
   The user set M9.pre's bar at "compiles, and watch the cost" rather than
   "install it on a phone", so the milestone closed on a build result and every
   behavioural risk above it — 1, 2 and 3 — is untouched. The toolchain block for
   the two ABIs lives in `scripts/android-build.ps1`, and `adb install` has never
   been run against this APK: the signature is a throwaway key, the `INTERNET`
   permission is declared because `--share` opens a socket, and neither has been
   looked at by a package manager.

## Build tooling sketch

- `cargo apk` (android-activity) or a slint-android template; CI needs an
  NDK job. The lib becomes a cdylib; `src/main.rs` splits into a
  platform entry per OS behind a cfg. *Measured 2026-09-20:* the cdylib shape
  links (192 MB debug/unstripped `libm9_probe.so`, 70 s warm), so "the lib
  becomes a cdylib" is a fact rather than a risk. *Closed 2026-09-23:* the
  release shape is measured too — 31.6 MiB un-stripped per ABI, 31.3 MiB after
  strip, 24.29 MiB for the signed APK that carries both, and the strip step is
  worth only 0.39/0.15 MiB of it. And per this project's no-CI decision, that
  "NDK job" is a local script, not a pipeline.
- Signing/debugging loop is the slow part — prototype on a physical
  device early. *Half-closed 2026-09-23:* the signing step is now automated
  against a generated throwaway key, so packaging reaches the end of its own
  accord. The *debugging* loop is the unclosed half and it needs hardware: no
  `adb install`, no logcat, no stack trace from this APK has ever been read.

## Suggested first milestone (M9.0)

A read-only Android viewer: load the SQLite file, list pages, render
blocks (no editing). It proves storage + renderer + build pipeline with
none of the IME risk, and gives the team a device target for the editor
spike that follows.

*Revised 2026-09-20: the ordering above was built on a guess that has since
inverted.* The viewer's risk — storage, renderer, build pipeline — is exactly
the part this desktop can already prove, so it should not be first. What cannot
be proven without hardware is text input, and that is the risk that can
invalidate the milestone.

1. **M9.pre (code, no device needed).** cfg-gate `rfd` behind
   `cfg(not(target_os = "android"))` and route its two pickers through
   `platform/`; add a `platform::data_dir()` that feeds the existing pure
   `data_location::decide()`, and make "no per-user directory" a loud error
   instead of a silent in-memory session; fix the Slint feature list per target
   (Android wants `backend-android-activity-06`; `system-tray` is dead code on
   desktop too). Acceptance: `cargo check --target x86_64-linux-android
   --all-targets` clears our own crates, not just the graph.
   **Delivered 2026-09-23** — see the section at the top of this page. It took
   the acceptance line literally: both ABIs, `--all-targets`, plus a release
   `cdylib` that links, because `check` never links and the claim under test was
   about a shared library.
2. **M9.0 input spike (hardware).** One `TextInput`, one `PopupWindow` beside
   it, on a real device, running `docs/IME_CHECKLIST.md`'s composition section
   under SwiftKey / Gboard / Samsung. Go/no-go: it passes without patching
   Slint.
3. **M9.1 read-only viewer.** What M9.0 used to be, demoted to second because
   it carries none of the risk it was designed to dodge.
