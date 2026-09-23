# Architecture Decision Records

Format: decision → context → consequences. Newest first.

## ADR-0096 · Touch mode is a property on the `UIState` global, not a second UI tree

Decision: `ui/Types.slint`'s `UIState` gains one property —
`in-out property <bool> touch-mode: false` — set once by the launcher before the
first component is realized, and read in seven places across two files that
already existed. There is no second `.slint` entry for the phone: no
`ui/AndroidWindow.slint`, no per-platform copy of a component, one `AppWindow`
on both platforms and therefore one set of the eight popups. The only new UI file
is `ui/components/MobileBar.slint`, whose five buttons call `UIState` callbacks
that already existed and add no Rust surface at all.

Context: `docs/ANDROID_NOTES.md` measured this job on 2026-09-20 as "all of
`ui/`" — 12 000 lines, 31 hover affordances, 8 popup windows, 68 `in-out`
properties on one global, row heights that mostly miss the 44 dp touch minimum.
What that estimate did not count is the surface the host actually holds. Rust
addresses exactly two things: `ui.global::<UIState>()`, and `ui.window()` for a
size and a rendering notifier. Nothing in `src/` names a component, a row
delegate or a popup, so the tree could be re-shaped without either side having
to know the other's file list.

Consequences:

- **The fork's cost would have been paid twice a day.** A second tree is a second
  place every one of those 68 properties has to agree with, on a platform nobody
  here can run — the divergence would have been discovered by a phone, which is
  the last instrument this project owns.
- **Seven reads, and every one of them is an `if`.** `TopBar` gates a divider and
  the three window controls (a phone has no window manager to serve from a title
  bar); `AppShell` gates the rail, the drawer and the thumb bar. A reviewer can
  hold that list in one screen, which is the test a decision of this shape should
  pass before anyone calls it small.
- **The rail and the drawer are mutually exclusive, and that is a correctness
  rule rather than a layout choice.** `Sidebar` binds `content-y <=>
  UIState.tree-viewport-y`; two live instances would leave the tree's scroll
  position owned by whichever list last moved, because an invisible or
  zero-width element keeps both its bindings and its layout slot. The desktop
  loses nothing — its rail was already a plain `Sidebar {}` in a layout — and the
  phone loses a slide animation, since an element behind an `if` has no previous
  geometry to animate from. On a 400 px screen the page keeping its full width is
  the better half of that trade.
- **`touch-mode` being runtime state means it is reachable at runtime**, which is
  how the layout was seen without a phone: `--touch` on the desktop binary draws
  the window the way a device would. It is not a supported mode and the flag
  doc says so — no 44 dp audit stands behind it. The direction cannot be
  reversed by a user: Android never reads argv, so there is no spelling that
  flips a phone back to the desktop shape.
- **Ordering is load-bearing and invisible.** `set_touch_mode` runs before
  `controller::bind`, and `bind` reads the flag when it decides the sidebar
  default. A phone's drawer starts shut — an open drawer over the document is not
  a drawer, and the setting that remembers the desktop rail has nothing to
  remember here. Move `bind` above `set_touch_mode` and the drawer silently
  reopens on the next launch, on a platform where nobody would see it until a
  user does.
- **What this deliberately does not adapt is the expensive half.** The block
  kit's hover chrome and the touch-target question stay open, and the number is
  on the record: **186 height literals under 44 px across 28 files of `ui/`** —
  23 of them ≤ 2 px, so dividers and spacers that no finger aims at, and **163 in
  the 3–43 px band** a real audit would walk (`DatabaseView.slint` alone holds
  38). The pattern is spelled out because this count has drifted three times:
  `(min-|max-)?height: [0-9]+px` over `ui/**/*.slint`, which reads 196 literals
  across 29 files in total, 186 of them below 44 px and 10 at or above it. The
  first sweep matched non-`px` heights too and reported 221 across 25 files; a
  re-count that anchored `height:` to the start of the line lost every `min-`
  and `max-` match and read 181. Not all of the 163 are touch targets either — a
  20 px chevron inside a 36 px row is not one, and which of them are is a
  question a desktop cannot answer. A flag
  that reaches seven places is a decision; a flag that reaches a hundred and
  sixty is a second design pretending to be a property, and that one deserves
  its own milestone.

## ADR-0095 · The shell splits by target operating system, and one launch sequence serves both

Decision: four moves, none of them a runtime check. **`slint` is named twice** in
`Cargo.toml`, once per mutually exclusive target cfg — `backend-winit` on the
desktop side, `backend-android-activity-06` plus `renderer-skia` on the Android
one — and `rfd` sits on the desktop side alone. **`[lib] crate-type` is
`["rlib", "cdylib"]`**, unconditional. **The two questions the launch asks the
operating system moved behind `src/platform/`**: `data_dir()` for where a library
may live, `picker::Picker` for whether a file dialog exists. **The start-up
sequence moved out of `main.rs`** into `app::launcher::run(start, touch_mode)`,
leaving `src/main.rs` at ten lines and `src/android.rs` at fifty-five — each of
them a thread spawn plus two arguments.

Context: ADR-0093 and ADR-0094 gave the model and the store their own repository
"so a second shell can compile them without a window". This is that second
shell's first compile, and it stalled on the shell crate itself: every file that
touched a window or a dialog named a desktop-only crate at the top of the file,
and `main.rs` owned the launch. The milestone line in `docs/ANDROID_NOTES.md` was
written against exactly this list, and its acceptance test was one cargo command.

Consequences:

- **Cargo accepts one dependency named twice because the two cfgs cannot both be
  true.** The alternative — a single `slint` line whose feature list a build
  script decides — is what Slint's own guidance warns against and what broke the
  first probe build on 2026-09-20.
- **An Android build's flag list is one long.** `--no-default-features`, and
  nothing else: the default arm is FemtoVG, and Slint cfg's FemtoVG out of this
  platform. The renderer the platform actually uses is named in the dependency
  table rather than behind the desktop's `skia` feature, so a build that forgets
  the desktop's vocabulary still gets a renderer — both spellings were built
  here, and both link.
- **`[workspace]` is gone from the manifest, and `cargo apk` is the reason.**
  cargo-apk 0.10 refuses to read a manifest that declares one — its parser dies
  on a file holding both `[workspace]` and `[package]` — and the APK is the
  only route to a phone. The table had already stopped doing work:
  `[workspace.dependencies]` pinned `image` and `rusqlite` so that "the shell and
  the store agree", but `quire-core` is a *git dependency*, not a member —
  inheritance does not cross a repository boundary, and what keeps one copy of
  each library is a compatible version requirement plus `Cargo.lock`. The same
  two strings moved into `[dependencies]`. `just check`'s `--workspace` now names
  one package that is not a workspace, which is what it already meant. The lock
  did move, and not because of that: +170/−45 lines, the Android side bringing in
  `android-activity`, `ndk`, `jni` and their `ndk-sys`, and the desktop dropping
  the three crates `system-tray` needed — `ksni`, `pastey`, `task-local`.
- **Packaging the APK taught two more manifest lessons.** `[package.metadata.android]`
  is inert to every build but cargo-apk's, so the release-signing table it needs lives
  beside a keystore that `scripts/android-build.ps1` generates on first use into
  `.scratch/` — a throwaway identity, because `--release` refuses the debug key cargo-apk
  embeds and a real one is M9.1's decision, not a script's. And **cargo-apk 0.10 does not
  ask cargo what it built**: it parses the manifest itself, collects the lib plus every
  `[[bin]]` and `examples/` file it can see, and hands each to an APK — an artifact table
  that only knows how to name a *cdylib*, so the first binary it reaches dies with
  `Bin is not compatible with Cdylib` (cargo-subcommand 0.12, `artifact.rs:51`), *after*
  the library's own APK has already been aligned and signed. The fix is the flag that says
  what this platform packages: `cargo apk build --release --lib`, which leaves one artifact
  in the list, no panic, and no `quire-typing` compiled for a phone that will never run it.
  The script still judges the run by the file — fresh `quire_shell.apk` on disk, size
  printed — because "the toolchain panicked at the end" must never again be read as "the
  package is broken", nor the reverse. That rule paid for itself the same day: after the
  library rename below, the run asserted `quire.apk`, found the previous morning's file,
  and threw — over an APK that cargo-apk had built, aligned and signed two lines earlier
  under the new name. The filename follows the library target, so all three names move
  together.
- **The `cdylib` costs the Windows build an artifact, not a runtime — and the
  library had to be renamed to pay for it honestly.** cargo has no per-target
  `crate-type`, so a `libquire_shell.dll` is linked beside `quire.exe` and nothing
  loads it. It is not named `quire` because a cdylib and a binary that share a
  target name also share `target/<profile>/quire.pdb`, which cargo warns about as
  something that "may become a hard error" (rust-lang/cargo#6313) and which this
  repository measured rather than argued: after a full build, one
  `cargo build --lib` rewrote `target/debug/quire.pdb` from the 413 MB file
  `quire.exe` was built against into the DLL's 187 MB one, leaving a live binary
  whose recorded symbol file no longer matches it. Every consumer keeps the old
  spelling with one line — `use quire_shell as quire;` — so the rename stops at
  five files and the test bodies never mention it. Rebuilt after the rename, the
  same full build writes `target/debug/quire.pdb` (416 952 320 B, the exe's) and
  `target/debug/quire_shell.pdb` (188 518 400 B, the library's) side by side with
  no warning in the log, and the desktop suite is unchanged at 134 passing. The
  Android side is the reason
  the name cannot simply be `quire_lib`: cargo-apk derives the packaged `.so` filename,
  the manifest's `android.app.lib_name` *and the output APK's own name* from the library
  target, so the three cannot drift apart — `libquire_shell.so`, `android.app.lib_name =
  quire_shell` and `quire_shell.apk` are what the 2026-09-23 re-run read back out of the
  manifest it generated and the archive it signed.
- **A per-target feature list is where a silent capability removal hides, and it
  nearly happened here.** The first version of this split dropped slint's
  `accessibility` feature from both sides on the argument that no code in the
  shell reads it — which is precisely the wrong argument, because AccessKit is
  the bridge a screen reader uses without any call from the application. It is
  back on the desktop table. Android's does not belong there: the platform's
  accessibility service walks a view hierarchy, and this app hands Android one
  `NativeActivity` surface, so there is nothing for it to walk. `system-tray`
  stayed off both, and that one the argument does hold — no code, and no
  setting, shows a tray icon.
- **`--all-targets` for Android compiles the desktop's other targets** —
  `quire-typing` and both integration harnesses — and they pass. That is a
  statement about the type checker, not about running them there, and the list
  has one hole worth naming: `quire-shot` declares
  `required-features = ["software"]`, so an Android build that enables no
  renderer feature does not compile it at all. `--all-targets` passing is
  therefore not "everything in this manifest", and saying it as though it were
  is how a skipped target becomes invisible. No cfg was added to hide any of
  them, because a cfg would have to be maintained against the day one of them
  does run there.
- **The dialog path gained a third answer.** `Picker::pick()` returns a path, a
  cancel, or `Unsupported(&str)` — the last of which is a case the call sites
  could not express when each of them held an `rfd::FileDialog`. All seven go
  through one `ask()`, which puts the reason in the notice bar ("Android cannot
  save a file outside the app yet") instead of leaving a button that does
  nothing. The desktop's answers are byte-for-byte what they were; the Android
  door — Storage Access Framework, an activity result and a `ContentResolver`
  copy — is M9.5.
- **`quire-core` changed nothing for this slice, which was the point of
  splitting it.** `data_location::decide()` and `migration()` already took the
  per-user path as a parameter and `logging::init_at(dir)` already existed, so
  the phone's `internal_data_path()` is supplied from the shell and no commit had
  to be pushed to a public repository on the strength of a guess about a second
  platform. The seam was designed in ADR-0093 for a consumer that did not exist
  yet; this is the first evidence that the design was right, and the second is
  that it cost nothing to use.
- **ADR-0009's 8 MB stack stopped being a Windows note and became a platform
  rule.** It is asserted in `desktop_main()` and again in `android_main()`,
  because the reason is Slint evaluating the component tree recursively on the C
  stack, not the operating system underneath. Two spawns of one thread is
  duplication kept on purpose: the alternative is a helper that pretends to know
  which entry a platform has.
- **What this does not deliver is the part that needs a device.** No first-paint
  number, no IME behaviour, no gesture. The packaging steps are proven up to the
  boundary of the phone: the manifest is written, both `.so` entries are aligned
  and verified, the archive is signed, and `apksigner verify` on the result says
  **v2 and v3 pass, v1 (JAR) does not** — which is fine at `min_sdk_version = 24`
  and is exactly the line below which it would not be. What comes out is a
  24.29 MiB two-ABI `quire_shell.apk`, re-signed under that name after the library
  rename. Nothing past that boundary has been run — no
  `adb install`, no logcat, no view of the icon, the orientation flag or the
  `INTERNET` permission as a package manager sees them. The bar was set at
  "compiles, and watch the cost" by explicit decision on 2026-09-23, so the
  milestone closed on a build result and `docs/ANDROID_NOTES.md`'s risk 1, 2 and
  3 are exactly where they were.

## ADR-0094 · The extracted crate leaves this repository and comes back as a pinned git dependency

Decision: `crates/data/` is deleted here. Its content is now the repository
`github.com/PT123123/quire-core`, branch `main`, and the shell consumes it as
`quire-core = { git = "…", rev = "f3044e22…" }`. Four choices were made before
touching anything, in this order: split the workspace first and only then move the
directory (ADR-0093 was that rehearsal), carry `core/` + `storage/` + `services/`
and nothing else, take the git history out with `git filter-repo` rather than
starting flat, and have the shell depend on a **pinned rev** rather than a path or
a branch. `src/lib.rs` still re-exports the four modules, so the 959 references
survived the second move too — only the spelling of the crate changed.

Why a repository and not just a crate: the Android port is the second consumer, and
a second consumer that lives in a different checkout cannot import a relative path.
The sync module is the second *reason*: it belongs next to the storage it fronts,
not next to a renderer.

Consequences:

- **The name is `quire-core`, and ADR-0093 still says `quire-data`.** Both are
  correct on their own date. The extracted repository's first commit is what pays
  for the rename — a name on a remote is harder to change than a name in a manifest.
- **The gate in this checkout got smaller, and no cargo flag makes it bigger.**
  `quire-core` is a dependency, and `--workspace` means "the members", so
  `cargo test --workspace --all-targets` here runs the shell and *builds* the store
  without running one of its 431 tests. The number is no longer 565. `just check`
  says this in its comment; a reported green has to name which repository it ran in.
- **Cargo's own git cannot fetch a public repository.** With the dependency in
  place, `cargo check` died on `no authentication methods succeeded` while
  `git ls-remote` on the same HTTPS URL returned the ref. libgit2 offers credentials
  an anonymous URL never asked for and GitHub answers the challenge with 401.
  `.cargo/config.toml` now sets `net.git-fetch-with-cli = true`, which is also the
  setting a machine with no SSH key needs. The remote is consumed over **HTTPS** and
  pushed over **SSH**: fetch has to work for a stranger, push has to work for the
  owner.
- **The history was scrubbed before it was published.** 46 of the 71 carried commits
  still carried a personal QQ address as both author and committer; a
  `filter-repo --mailmap` pass rewrote them to the GitHub noreply identity, and the
  remote was re-read after the push to confirm 71/71 on both fields rather than
  trusting the local log. The rewrite moved every SHA in the range. It did not touch
  a file: measured against the blobs this repository still holds at
  `09ef5aa:crates/data/`, **38 of 45** source files hash byte-identical (after
  normalising the checkout's CRLF) and the remaining **7** hash identical once the
  `quire-core` → `quire-data` rename is reversed — so the only content delta across
  the whole extraction is the crate's own name.
- **The same pass then went over this repository, and that is a bigger blast.** The
  scan found the identical address on **137 of 174** commits here and on the tagger of
  `v0.1.0-rc1` — on a **public** remote, where it had been since the first push. ADR
  records and Track 3's handoff had claimed the metadata was clean; what had actually
  been scanned was the diff, never `%ae`/`%ce`, and `docs/REPORT_TRACK3.md` now says so
  instead of keeping the wrong sentence. `--mailmap` rewrote the whole range, and the
  same run took `--replace-text`: **22 of the 147 tracked files** — every one a
  `benchmarks/results/*.jsonl` — had recorded its `exe` and `db` fixtures as absolute
  paths carrying the local username, and one older revision of a report named that
  username in prose. The control that makes that claim mean anything: replaying the
  same substitution file over the 22 original blobs reproduces **22 of 22** rewritten
  hashes, so the delta inside a measured row is the path and not the measurement, and
  the other 125 files are byte-identical. A first attempt at that run reported
  success and changed nothing, because `--replace-text` separates on `==>`, not `=>`,
  and silently treated each whole line as a pattern that could never match — the tree
  diff against the pre-rewrite commit is what caught it. Every SHA quoted anywhere in
  these documents predates that pass, including the `09ef5aa` above and the pre-split
  control arm's `0089008`; the commit subjects still find them.
- **What the rewrite reaches, and what it deliberately does not.** It reaches the
  network and every ref: `origin/master` and `v0.1.0-rc1` went out under
  `--force-with-lease` pinned to the exact values this repository had published minutes
  earlier, so the push could not have clobbered anything it had not itself written, and
  the remote was then read back through the API — 177/177 on author, 177/177 on
  committer, and the tag's tagger — rather than believed from the local log. It does not
  reach the objects in this checkout: the old commits are unreachable but still on local
  disk until a `reflog expire` and `gc --prune`, which would also be the only way back.
  Not taken, on purpose — the leak being closed is the public one, and a machine that
  already names that user in twenty file paths gains nothing from local amnesia. The run
  left `commit-map` behind, so every old SHA still resolves to its replacement on paper.
- **A scrub of published history is worth nothing if the next run writes the same
  paths back, and the next run did.** All four row generators recorded `$Exe` and the
  pinned database as absolute paths, so `benchmarks/scripts/redact.ps1` now turns the
  checkout root, `%TEMP%`, `%LOCALAPPDATA%` and `%USERPROFILE%` into `<repo>`, `<temp>`,
  `<localappdata>` and `<user>` — the same four spellings the history pass used, longest
  prefix first, since each of those nests inside the next. Two traps in it are worth
  keeping: redaction has to happen **before** the backslashes are doubled for JSON,
  because a rule built from a real path carries one separator and would never match
  `C:\\Users\\…`; and PowerShell 5.1's `ConvertTo-Json` re-escapes the placeholders'
  angle brackets, which three of the writers route bench.ps1's line through, so
  `Open-JsonPlaceholders` opens them back up — a `\u003c` that is really in a value
  survives that, because it arrives as `\\u003c`. The last line of defence is a warning
  rather than a wider filter: an account name still standing as a whole path segment
  means a prefix this file does not know, and it says so instead of writing quietly.
  Measured on ten rows from all four writers: 0 carry a drive-absolute path, 10 carry
  `<repo>`, 6 carry `<temp>`, 0 still read `\u003c`, and all 10 parse.
- `Cargo.lock` is committed **in the extracted repository** as well, which a library
  normally would not: the Android shell will consume it as a git dependency with no
  workspace root above it to inherit a lock from, and `rusqlite` is `bundled` there,
  so that file is what pins SQLite.
- What this does **not** do: no tag and no release cadence, so the rev is a raw SHA
  and updating it is a hand edit. `docs/SPEC.md` §三十九/四十, `docs/DECISIONS.md`
  and `docs/PERFORMANCE.md` stay here — they describe the product, not one crate —
  so `quire-core`'s README points at this repository instead of copying them.
  `cargo check -p quire-core --target aarch64-linux-android` has still not been run;
  the `-p` no longer resolves here, and the check now belongs in that checkout.

## ADR-0093 · The model and the store become their own crate, so a second shell can compile them without a window

Decision: the repository is now a cargo workspace of two packages. `quire-data`
(`crates/data/`) holds `core/`, `storage/`, `services/` and `testing.rs` — the
document and database model, SQLite behind it, and the work that runs without a
window: import, export, search, attachments, settings, and the LAN framing a
sync module will grow out of. The root package `quire` keeps `app/`, `platform/`,
`ui/`, `build.rs` and the three binaries. The rule the split exists to keep is
one sentence: **nothing under `crates/data/src` may name Slint, a file dialog, a
clipboard or a platform API.**

Why now: M9 (Android) is parked, not cancelled, and the thing that would make it
cheap is the ability to build the model for `aarch64-linux-android` with no GPU
renderer, no `rfd`, no `embed-resource` and no `.slint` in the graph. Measured
before deciding: `core/` + `storage/` + `services/` referenced **nothing** outside
themselves (`grep 'crate::app\|crate::services\|crate::platform'` over them: 0
hits), and `slint::` appeared in exactly `app/`, `bin/`, `main.rs` and one line of
`lib.rs`. The boundary was already there; it was only never compiled separately.

What the 950 references did **not** become: this is not a path rewrite. `src/lib.rs`
now says `pub use quire_data::{core, services, storage, testing};`, which keeps
`crate::core::…` inside the shell and `quire::services::…` from an integration
test resolving exactly as before — so the 959 cross-layer references (398
`crate::core`, 30 `crate::storage`, 23 `crate::services`, 57 `crate::testing`,
118 spelled through `quire::`) stayed untouched, and the five moved test targets
were the only files sed'd (59 `quire::` → `quire_data::`). The shims are marked
in `src/lib.rs` as deletable. Trading a 950-line diff for four lines was worth it
because a diff that large hides the one thing worth finding, and the compiler
found it: `services::search_service::snap_to_words` was `pub(crate)` and called
from `app::workspace.rs`, which was legal only while one crate held both. It is
`pub` now, and its doc comment says why.

Consequences:

- **The gate command changed, and that is the load-bearing part.** `cargo test` at a
  workspace root tests the *root package only*. Caught on the first run here: bare
  `cargo test --all-targets` reported **134 passed / 0 failed and exit 0** while
  `quire-data`'s 431 tests never executed. `just check` now passes `--workspace` to all
  three cargo lines, and a future green gate has to name which scope it ran.
- The suite is proved *carried*, not merely counted: the two runs' test **name** sets are
  identical (`comm` both directions empty, 589 names) and the totals match the D10 gate
  exactly — 565 passed / 0 failed / 23 ignored, now 134 on the shell side and 431 on the
  data side. That split was first written as 389, which is a hand-summed number that
  dropped `storage_test`'s 42; the figure above is the same tree run standalone in the
  extracted repository, not an arithmetic. Sum per-target lines with `awk`, never by eye.
- Pixels prove the shell did not move, with each arm identifying itself: the control was
  built and swept at `0089008` **before any file moved** (`.scratch/split-control`, exe
  md5 `63f021af…`), the post-split arm swept the same tree afterwards
  (`.scratch/split-after`, exe md5 `1851e577…`) → **0 changed of 131**. Reusing an older
  baseline would have mixed this slice's result with whatever had drifted since it was
  shot.
- `crates/data` depends on `image` and `rusqlite` and nothing else, both pinned once in
  `[workspace.dependencies]` — two crates choosing different feature sets for `rusqlite`
  would compile SQLite twice, which is the kind of thing that only surfaces as a
  mysterious binary size.
- `workspace_test` and `persistence_test` stayed on the shell side on purpose: both name
  `app::state`, and pulling them in would mean pulling `AppState` into the data crate,
  which is exactly the edge this split exists to forbid.
- What this does **not** do yet: it is still one repository, and `path = "crates/data"`
  is the entire coupling. The `git filter-repo` pass has since been run over a throwaway
  clone (69 of 172 commits, the data-layer paths kept at their original spelling so
  nothing in the blame chain breaks) and the extracted tree builds and tests standalone at
  431 passed / 0 failed — but it has **no remote**, so the shell still consumes the crate
  by path. Pushing it, and switching to a pinned `git = ` dependency, are one decision away.
  **(They were taken the same day; see ADR-0094. Everything above this line is the state at
  the moment of writing, including the crate's first name — it is `quire-core` now, and
  `crates/data/` no longer exists in either tree.)**
- One seam left visible rather than papered over: `storage::data_location::roaming_root()`
  is the only function in the data crate that reads an environment variable
  (`%APPDATA%`), and its neighbour `app_data(&Path)` already takes the value as a
  parameter — an Android shell hands its own documents dir in rather than restructuring
  the module. The crate's only two `#[cfg(windows)]` blocks are a probe's
  `K32GetProcessMemoryInfo` call, and the `not(windows)` fallback beside it already
  returns `None`.

## ADR-0092 · A mark the user has to see is drawn from the icon set, because the UI face has no tick

Decision: a mark that carries meaning — which row of a chooser is the answer, which button
clears a rule — is a `Path` from `ui/Icons.slint` (`Icon { name: "todo-check" }`,
`Icon { name: "x" }`), never a `Text` holding a code point. Prose that *names* a drawn button
keeps to a code point the UI face actually carries: `…` (U+2026), not `⋯` (U+22EF).

Why this is a rule and not a bug report: `Typography.ui-font` is `"Segoe UI"`, and the
character map read out of `C:\Windows\Fonts\segoeui.ttf` (3996 code points) does **not**
contain U+2713 ✓, U+2715 ✕ or U+22EF ⋯. It does contain U+2039 ‹, U+2191 ↑, U+2193 ↓,
U+2026 … and U+00B7 ·, which is why the picker's back-header, the palette's navigation hint
and this app's every other typographic mark have always looked fine. The three that are
missing are exactly the three this project used as *meaning*.

The evidence is pixels, not reasoning. In the `database-kinds` scene the tick's `Text` painted
**nothing**: the row's right gutter (x 695..742 of the 224 px popup, all 17 rows) is white
except the card's own border, while the same delegate's name and refusal caption painted
normally — so the row arrived with its fields intact and only the glyph was absent. Swapping
it for the drawn check put a 8 × 7 px mark at x 721..728, the row's own gutter, and the sweep
manifest moved by exactly that. `page-lock` said the same thing in prose: the bar read
`Page locked ·  to unlock`, with the hole where the button's name belongs.

Both were found by **measuring a scene that had just been added to the default sweep**. The
group popup's two ticks (D4, shipped for six slices) had no scene at all, so they were never
photographed and are still unphotographed; they are fixed here by the same rule, not by the
same evidence.

What this ADR deliberately does **not** claim: that the shipped window cannot draw a ✓.
`seguisym.ttf` (Segoe UI Symbol, 7536 code points) carries all three, and a DirectWrite
fallback chain could reach it — the headless software renderer demonstrably does not. That
question is the reason for the rule: a meaning carried by a glyph is rendered by whatever
fallback the platform happens to own, in a colour that is not the theme's, and no one has to
look at it again. A `Path` takes `color:` from `Colors.accent-text`, so it is the *same mark*
in the light and dark shot, which is now measured (`dark-database-kinds`,
`dark-database-relation`, `dark-database-rollup`).

Consequences:

* Four call sites became paths: `DatabasePickList`'s chosen-row tick (the type menu, the
  relation picker, the rollup's three choosers — one component, five questions), the group
  popup's two ticks, and the filter panel's clear cross. The cross is the one whose refusal
  was *silent in a different way*: its `TouchArea` worked, so the button deleted the rule
  while looking like an empty box.
* Two strings Rust writes into the notice bar changed (`AppState::note_locked`, the
  save-as-template notice). An audit therefore has to read `.slint` **and** `.rs`: every
  string literal under `ui/**/*.slint` and `src/**/*.rs`, filtered against the face's cmap,
  ignoring CJK (which the app demonstrably renders — the sidebar's 写作与中文测试 proves it).
  What is left uncovered after this ADR is emoji (deliberate: `Typography` asks for the emoji
  face by name) and test fixtures.
* `core/math.rs`'s symbol table is **out** of this rule and unmeasured: ~70 of its code points
  are outside Segoe UI. The `math` and `math-inline` scenes happen to use only covered ones
  (`≤ √ ≠ ∫` and the superscript digits all read `True` against the cmap), so no hole has been
  seen there. A formula asking for `\aleph` or `\therefore` is an open question, named rather
  than answered.
* The sweep's default scene list gains the database scenes (`sweep.ps1`'s `$all`), which is
  how this class became visible at all: two of the §三十九 scenes had spent six slices
  photographing the *same* table because nothing in the harness was comparing them.

## ADR-0091 · A version is a database file with one page left in it, and the cap is twenty

Decision: SPEC §三十八's version history stores a named version as **a whole
SQLite file** — `versions/p<page>-<created>.db` beside the library — produced by
the same `VACUUM INTO` statement §二十五's `backup::snapshot` runs, then narrowed
to the one page. There is no `versions` table, no `page_versions` table, no blob
column and **no migration**: `PRAGMA user_version` stays at 14, and the five
M12 slices before this one were each a column (v10 … v14, ADR-0044 through
ADR-0049) while this one is nothing. What
a version adds over a `.bak<N>` is a name, a page, and the fact that §二十五's
rotation must not eat it. The visible contract is three actions — 命名 / 对比 /
恢复 — and one number: `versions::MAX_PER_PAGE = 20` per page, oldest going.

Why "reuse §二十五's snapshot, do not build a second store" is read literally: the
sentence 复用 §二十五 的 snapshot 机制，不另造一套存储 names a mechanism, and the
mechanism is one statement plus a durability bargain plus a validator. So a
version copies with `synchronous=OFF` for that one write (restored to `FULL`
immediately after, because every real write still runs at `FULL`), then deletes
every page but its own — one statement, because `blocks.page … ON DELETE CASCADE`
takes the block rows and `block_children` / `marks` take theirs — then `VACUUM`s
so the file is the size of what is left rather than the size of what was deleted.
The copy is validated by `Database::open`, which migrates and integrity-checks
it: the same door a recovered `.bak` comes through.

The payoff is the part that is usually paid for twice. A version **is** the
format, so it cannot drift from the format: `versions::read` opens the file with
`SqliteRepository` and calls `Repository::load`, the loader the app opens the
library with, and gets back a page with its marks, its colours, its `lang`, its
table grid, its attachment ids and its `Page` references. A second reader — a
JSON body, a serialised `Block` list, a per-version table with its own column
list — is a thing this repo has to keep in step with `Block` forever. There is no
such thing here, which is also how §三十八's other hard line ("不得引入第二套内容
格式", argued the same way for templates in ADR-0049) gets obeyed without a rule.

Why the index is two `metadata` rows and not a table: which versions exist, and
what the user called them, has to survive a restart, so `version/<page>/<created>`
holds the label and `version-files/<page>/<created>` holds the attachment ids the
version's rows point at. The second row is not a convenience — §三十七's reclaim
scan answers "does anything still point at this file?" out of the **live**
database, and a version file is a database it never opens. Without the mirror, a
version whose page later dropped its picture would restore as a missing image
with nothing left to explain it, which is ADR-0047's objection arriving from the
other direction. `AppState::reclaim_attachments` grows one term for it
(`version_pinned_attachments`), so §三十七's scan counts a version's pictures as
referencers. Two meta
rows per version are cheaper than one query that opens twenty files.

Why the FTS tables have to be emptied with more than a `DELETE`, measured: a
version is a copy of the library until it is narrowed, and the two FTS tables are
the one part of it that holds *the text of every page* while reporting no
foreign keys. The first version of this code did `DELETE FROM search_blocks` and
the tests confirmed 0 rows — and a 201-page library's version file still weighed
**1.4 MB more than it should**, because FTS5 keeps its term dictionary in a
`<table>_data` b-tree that a plain delete leaves standing. `clear_index` now runs
the `DELETE` and then the FTS5 `rebuild` command, which re-derives `_data` from
the (already empty) content tables. `delete-all` is the shorter statement and
SQLite refuses it here: it is only for contentless or external-content tables, and
this index stores its own content. The assertion is now on `_data`'s row count,
not on the tables a reader would query — a row count was exactly what lied.

Why the cap is 20 and goes from the old end: 保留策略必须给出磁盘与 RAM 数字，不接受
无限增长. Twenty answers "the last few drafts of this week" for a page whose
version is measured in kilobytes (numbers in docs/PERFORMANCE.md: a 60-line page's
version is **120 KB**, 1% of a 9.9 MB library; a 5 000-line page's is 790 KB, 7%;
twenty of the long page are 15.8 MB in 20 files). A cap that drops the *newest*
entry is worse than one that drops the oldest, and a refusal with no way to free
room is a dead end, so the panel's own caption says the number out loud ("Quire
keeps the 20 newest and lets the oldest go") and the prune that follows a save
deletes the file before it deletes the rows naming it — the other way round
leaves a row the panel offers and `read` refuses. RAM is the smaller half of the
question, and it is bounded by the same shape: a version is only ever in memory
one at a time, through a transient connection that closes when the row is read.

Why the save flushes first: `VACUUM INTO` reads the **file**, not this session's
memory, so a version taken over a queue of unwritten edits would be a picture of
the page as of the last flush — a thing named "this" delivering "ten seconds ago".
`save_page_version` therefore calls `persistence_force_flush()` before the copy,
which is also why the same-second collision retries (up to four times, ≈ five
versions a second) rather than letting the clock be picked silently: the file name
and the metadata key are both that second, and two versions with one timestamp is
a lost version whose label the user typed.

Why restore is content, through the command system: `restore_version` is one
`exec_all` batch — an `InsertForest { anchor: None }` of the version's rows, then
one `DeleteBlock` per current **root** (a delete takes its subtree, which is how a
table's cells and a columns layout's boxes go too), so it lands as **one Ctrl+Z**
and the rows behave like anything typed since: fresh ids, re-derived order keys,
ordinary change feed, FTS updated by the same path. Swapping files was not on the
table: it would put the page's rows behind the editor's back, cost the undo stack
its page, and leave the database with a file the app no longer has open. The page
keeps its title, icon, cover and font on purpose — a version is of a page's
*content*, and rolling back a name the user chose after the snapshot is not what
"restore this version" reads as. Two gates, both inherited: `locked_refusal()`
(ADR-0048) and "only the page on screen", because the command system addresses the
open page and a stale row after a page switch must not write into the wrong one.

Why compare is line-level and bounded: `core::diff::compare` takes identity from
the block id — a snapshot carries the page's own rows, so an unchanged line has
the same id on both sides — trims the common head and tail, and aligns only what
survives the trim, which is what makes editing one line of a long page cost one
comparison instead of one per line. The middle is LCS-shaped, `ALIGN_CELLS = 1 <<
18` (a 512 × 512 middle, ≈1 MB of `u32`), and past that the middle is *reported*
as a wholesale rewrite rather than aligned: the extra rows would still be true,
and a panel that takes a second to open is worse than one that shows more of them.
A row that changed in place arrives as a `Removed`/`Added` pair with the same kind
label — word-level diffing would need a second format inside a block, which is
the thing §三十八 forbids one level up. `same_body` excludes id / page / order and
`folded`, the last because §三十七 registers it as view state: leaving a section
collapsed is not an edit to what the page says.

Why one popup with two views, and why a row is an index: the list and the
comparison are the same four questions (which page, which version, what changed,
what does the button do), so they share a 460px window whose height is fixed —
clicking a row must not move the panel out from under the pointer. Naming is the
field at the bottom (always there, because "now" is worth a version at any
moment), comparing is a row click, and restoring is a button that only exists
*after* a comparison: nothing restores a version the user has not looked at,
because a restore replaces the page. Rows carry a **row index**, not the
timestamp, because Slint's `int` is 32-bit and a unix second is not —
`version_at` turns the index back into `(created, label)` and returns `None` for
a row a delete removed between the click and here. Ages are `diff::age_text` over
`now_secs() - created`, which keeps the app's rule of no timezone and no calendar
arithmetic. The scene captures and the click handlers read the *same* four
projection functions (`version_rows`, `versions_note`, `version_heading`,
`version_diff_note`), so a pixel in the panel is produced by runtime code rather
than by a copy of it — the discipline A4's sweeps exist to keep honest.

Consequences:

* A session with no database file has no versions, and that is the panel's empty
  state rather than an error line: a version *is* a file, so a session that
  writes no file can keep no version. The headless bench and the captures paint
  their list through the projection functions for exactly this reason.
* `versions/` is the third folder beside the library, after `attachments/` and
  the `.bak<N>` family. Its housekeeping is its own: `sweep_orphans` deletes any
  file the index no longer names (a save that died between the copy and its
  metadata row) and takes `p<page>-<created>.db{,-wal,-shm}` only — a name it did
  not write is somebody's other data. A `readme.txt` in that folder survives.
* Deleting a page forgets its versions, its pins and its files
  (`forget_versions`, called for the page and each descendant `delete_page`
  takes), so a picture only a version pointed at is freed by the same action
  that made it unreachable. The test that pins it restarts the session first, so
  the undo stack cannot be what is secretly holding the file alive.
* The library's own bytes are unaffected by how many versions exist — the whole
  cost is in `versions/`, one file per version, and that folder is invisible to
  every number the app says out loud: §三十七's reclaim line reports only the
  attachment bytes it just freed, and never counts `versions/`. The arithmetic a
  user cannot see is written down in docs/PERFORMANCE.md instead — measured worst
  case there: 159% of the library for twenty versions of a 5 000-line page.
* §二十五's rotation and §三十八's versions share one mechanism and no lifecycle:
  `.bak<N>` is five generations and seven days, a version is twenty per page and
  no age. A version is *named by the user*, so nothing may prune it by age.
* Restore does not mark the page modified in any way the user has to learn: the
  change feed it produces is ordinary `BlockInserted` / `BlockDeleted`, so the
  flush, the FTS index, the LAN share and a restart all see a page that simply
  changed — the same deal `InsertForest` made for templates in ADR-0049.
* Three sweep scenes were added (`page-versions`, `page-versions-diff`,
  `dark-page-versions`), so the baseline is `.scratch/sweep39` at 83 scenes; the
  manifest diff against the previous baseline was 78 identical / 3 new /
  2 expected-changed before anyone looked at a picture.
* **Hand-test owed**: ⋯ → Version history → type a name → Save → click the row →
  Restore, including Ctrl+Z afterwards. The captures paint both views but cannot
  type into the name field, so the focus-on-open and the "one restore, one undo"
  feel are the parts a human still has to press. That is on top of the cover,
  colour-emoji and template-dialog arms owed since ADR-0046 / -0047 / -0049.
* Track 3's draft does not need a `versions` table either: if a database row ever
  wants version history, this is the same two metadata rows and the same folder.

* **Numbering note (2026-09-22).** This decision landed as `ADR-0050`, which Track 2
  had already taken for its mention/date storage shape — the two briefs overlapped
  (Track 1 was told `ADR-0044…0050`, Track 2 `ADR-0050…0059`), and the tracks each
  appended on their own side of this file, so the collision stayed invisible until
  the four tracks met. It is renumbered `ADR-0091`: the first free number after
  `ADR-0090`, since `ADR-0080`/`ADR-0081` are Track 4's (still uncommitted on its own
  tree) and `ADR-0053…0059` are left as the unused tail of Track 2's range. Track 2's
  `ADR-0050` keeps its number and every pointer that meant it; the pointers that meant
  *this* one — in `SPEC.md`, `CHANGELOG.md`, `PERFORMANCE.md`, `ROADMAP.md`,
  `UI_ARCHITECTURE.md`, `PLAN.md` and the version-history comments in `src/` — moved
  with it. `docs/REPORT_TRACK1.md` keeps writing `ADR-0050` in its own narrative,
  because that is the number it had when the report was filed.

## ADR-0049 · A template is a page nobody can open, and that costs one column

Decision: SPEC §三十八's template is `pages.template INTEGER NOT NULL DEFAULT 0`
(schema **v14**) on an ordinary page row. There is no `templates` table, no
`template_blocks` table, and no template file format: a template *is* a page
whose block sequence is a body to copy from, and the one column is the only fact
that page carries which an ordinary page does not. It is invisible everywhere a
page shows up — tree, sidebar, palette, Move-to, recents, search, the LAN share —
and it is reachable from exactly two surfaces: the slash / "+"-menu tail that
inserts it into the page being typed in, and ⋯ → **Templates**, whose six rows
insert, start a page from, save, export, import and delete. Five built-ins
(`core::template::PRESETS`) land on a library's first start through the same
import path the menu's Import row uses.

Why one column is the whole representation: §三十八's hard line is 模板的表示必须是
「块序列的副本」，不得引入第二套内容格式. The cheapest way to obey a prohibition is to
have nothing to violate — a template's body is `Block` rows in the same table, so
every field a block can carry (marks, colors, `lang`, `columns`, `img_percent`, a
`Page` reference) rides along in a copy without a single line of new mapping code.
`fill_template` is twenty lines because it clones rows, mints fresh ids from the
document's own allocator so a copy can never collide with its source, keeps the
order keys (a key only means something inside one page, and a new template has
nothing to collide with), and remaps parent links onto the copies — which is what
holds a table's grid and a toggle's children together. The alternative, a
`templates(id, body_json)` column, would have needed a serializer, a deserializer,
a migration for its own inner format, and a rule about which of the two bodies
wins when they disagree.

Why the flag has one write path: `PageCreated` carries a whole `Page`, so
`create_template` records the flag with no change variant of its own, and there is
deliberally **no** `Change::PageTemplateSet`. A page you can open is not a body to
copy from, and a row that flipped the flag on an existing page would need the
whole invisibility apparatus to follow it in both directions. The way to change a
template is the way the menu says: start a page from it, edit that, save it as a
template, delete the older one — which is also why there is no "edit template"
row. The cost of that choice is written down rather than hidden: saving never
overwrites, so the older copy stays in the library until the user deletes it, and
two templates may share a name because the library sorts by age, not title.

Why invisibility is non-attachment: `Workspace::create_template` makes a page and
flips its flag, and never puts it in `roots` or any parent's `children`. So every
enumerator that walks the tree skips a template for free — there is no list here
to remember to filter, which is the property that makes the feature safe to grow.
Four doors do not walk the tree, and each got its own term: `open_page` returns
before `mark_opened`, because opening would write `recents` and the `current-page`
meta — two more places a template must not appear; `search_index::matches` gained
`AND p.template = 0` in its join; and the LAN share filters its page list, its
child walk, and answers **404** rather than the Markdown for a template id, since
the other machine has no way to say "template" and would land it as an ordinary
page. A fifth door was considered and rejected: not indexing a template's rows.
That would make `insert_block` ask whether the page it is filing under is a
template — a second source of truth about a fact one join already has — and
`rebuild` would have to disagree with `insert` about which rows belong, so a
template would start appearing in search after a rebuild. The read-side term keeps
a template unfindable from both doors, and the test that pins it asserts the raw
`search_blocks` count as well as the empty hit list, so the exclusion cannot be
explained by missing data.

Why the built-ins are seeded and not migrated: migration 14 is `sql: ""` plus one
guarded `ADD COLUMN` through the shared `add_page_columns`, shaped exactly like
migration 13's for the same reason — "not a template" is a value, so no backfill
statement is needed and a v13 library opens with none. Writing the five preset
bodies there would mean hand-keeping order keys, `block_children` rows and both
FTS indexes in step, when `import_template` — the very function the menu's Import
row calls — already does all three. So the built-ins are imported, not invented,
and one code path can be wrong instead of two. Two guards make it once-per-library:
a settings flag (`builtin-templates-seeded`), without which deleting all five and
restarting would resurrect them and the menu's Delete row would be a lie; and a
name check, without which a session that died halfway through the seed lands the
library twice. The flag is recorded *after* the bodies, so a session that flushed
nothing retries rather than records a lie. It refuses outright when there is no
library to seed — which is also why the headless bench scenes and the visual
captures paint their own library instead of seeing these five.

Why insert is one command: `Command::InsertForest` lands a whole forest in one
`Ctrl+Z` step, so undoing a template removes eleven blocks rather than leaving
nine of them on the page, and the change list it produces is ordinary
`BlockInserted`s — the flush, the FTS index and a restart all see a page that
simply grew. An empty paragraph at the anchor is *replaced* in the same batch,
because the "+" line and a brand-new page's first row are both empty and a
template that arrives one row below the caret reads as a miss; that is legal only
because `exec_all` plans every command against the pre-state. It returns the first
inserted id so the caret can follow the copy, and `fill_template` records no undo
step at all, because the history stack belongs to the page the user is typing in
and a template page is never open.

Why the menu is one row, and why its labels are short: the page ⋯ menu gained
**Templates** rather than six entries, because the row's label is the feature's
name and the choices belong in the submenu. The submenu's labels then had to fit
the popup every menu in the app shares — `ContextMenu` is 184px wide and its rows
elide instead of wrapping — so they read "Use as new page", "Save as template",
"Export Markdown", "Import Markdown". That is a real loss of explicitness, and the
first render of the scene showed four of six rows ending in an ellipsis. The
judgement: the object is already named twice over, by the submenu the user is
standing in and by the library picker that follows, so the row that survives is
the one a user can read at a glance. Widening the popup was not on the table: it
moves every menu in the app and re-judges most of the baseline for one submenu's
wording.

What the round trip caught, in someone else's feature: the test that exports a
template and reads it back failed, and the defect was in §二十六's channel, not in
templates — `export_page` dropped an **empty** list item, so the row a template
leaves open for somebody to fill in vanished on the way out and came back as
nothing. `prefix_lines` now writes a marker for an empty block (`-`, `1.`, `>`,
`#`) and the numbered arm keeps its count without a trailing space. This changes
ordinary page exports too: a page with an empty bullet now round-trips through its
own Markdown instead of silently losing the line, which is the same deal an empty
heading already made. The rule is now stated in the export header's layout list,
and `an_empty_block_exports_its_bare_marker_and_comes_back_as_itself` walks all
five kinds that can be empty.

Consequences:

* Storage needed nothing else. A template's rows are block rows in the book, so
  the §三十七 reclaim sweep already counts an image block inside a template as a
  referencer through `doc.all_blocks()` — `cover_ids()` and the undo vote did not
  grow a template term, because the flag itself points at no file.
* The repository reads the column `unwrap_or(0)`: a half-migrated library loses a
  template rather than refusing to load, and that is the right direction of
  failure — a template that reads as an ordinary page is visible and editable,
  where a page that refuses to load is nothing.
* `AppState::delete_page` returns whether the deleted page was the one on screen,
  not whether it succeeded, so it answers `false` for every template. Two of this
  slice's tests asserted on that bool and were wrong twice over; they now ask the
  workspace whether the row is there before and after, which is the assertion the
  menu's Delete row actually makes.
* A locked page refuses an insert like any other edit — the gate is
  `exec_all_on_open_page`'s and needs no template-specific term — and still
  offers Save as template, since copying a body out of a page is not writing to
  it. The refusal line must not overwrite the lock's own line, so the emptiness
  notice is gated on `!page_locked()`.
* Search still *indexes* a template, so a library that gains one pays a little
  index and no results; the palette, the sidebar and the Move-to walks pay nothing
  at all, which is the point of non-attachment.
* Track 1's migration priority is spent a third time: v14 is the template flag, so
  any draft numbered 14 or higher moves up — Track 3's 草案 included.
* The page ⋯ menu is thirteen rows now, and the two scenes that show it moved
  differently: `menu.png` at 6 sampled pixels in one column (x 414, y 682..692 —
  the ListView's scrollbar thumb, because that popup was already clamped by
  `min(rows * 30px + 8px, window-h - menu-y - 20px)` before this row existed), and
  `page-lock-menu.png` at 428 sampled pixels across x 240..422 / y 558..640,
  because that one is anchored high enough to draw all thirteen and every row
  below the insertion shifts. Same menu, two anchors; reading the smaller number
  as "barely changed" would be the trap.
* **Hand-test owed**: ⋯ → Templates → each of the six rows, including the two
  `rfd` file dialogs, which no headless capture can reach. That is on top of the
  cover and colour-emoji arms still owed since ADR-0047.

## ADR-0048 · A lock is one column on the page, and a refusal has to be heard

Decision: SPEC §三十八's lock is `pages.locked INTEGER NOT NULL DEFAULT 0`
(schema **v13**) — one bool per page, no table of what is locked within it. It
covers the document: every block's content and the page's own title. It does not
cover the page's look (icon, cover, Style, favourite stay offered), the tree
(move, delete, duplicate), or reading (navigation, search, fold). One command is
exempt inside a locked page: `ToggleFold`. And a refusal is never silent: the
notice bar says which switch to flip, and the page says it too in a pill above
its title, in the ⋯ row that now reads "Unlock page", and in a ⋮⋮ menu left with
only its two read-only rows.

Why one column and not a flag per block is the shape of the sentence the user
clicks: "this page is read-only" is a statement about the whole document, and a
half-locked document — three rows editable, the fourth not — is not a state
Notion has a word for, and not one a migration could reach. So the gate sits
where the writes already funnel: `AppState::exec_editor`, which the 77 call
sites share, rather than 77 places that each have to remember. `NOT NULL
DEFAULT 0` makes the migration one `ADD COLUMN` with no backfill statement, and
a v12 library opens with nothing locked — "not locked" is a value, not an
absence, which is why this column is not nullable where ADR-0047's cover is.

The audit behind "every write": `exec_editor` is the only route to
`core::command::exec`, and the ninety-odd call sites of it and
`exec_on_open_page` share that gate. `grep doc.borrow_mut()` lists what is left,
by name. Four hits are the
bench scene fixtures, which paint a state rather than accept an edit. Two are the
Markdown import paths: they write blocks into a page they created a moment
earlier, and `create_page` hands out an unlocked row, so a lock can never sit
between a user and their own file. Two more are tree operations the lock
deliberately does not cover (`duplicate_page`, `delete_page`). And three are real
entry points beside the funnel: the todo checkbox, whose Slint callback calls the
command layer directly to update one row; `clear_block_ref`; and
`duplicate_page_block`, which mints a child page through the tree *before* the
funnel ever sees the block command.

The last two were holes, and they are the shape worth remembering: both sit
behind a caller that wrote `let _ = exec(...)`, so the refusal was discarded and
the follow-up write landed anyway — a locked page whose Page block lost its
reference, or which quietly gained a second child page. So the rule this slice
adds is not just "gate the funnel" but **a write that follows a command must be
conditioned on that command's result**, and the three entry points beside the
funnel each carry their own gate. The test that pins it asserts the child-page
count as well as the block list, because a refusal that leaves the tree fatter
than it found it is still a write.

Why fold is the exception: §三十七 already files it as the one command whose
result is persisted *view* state, and locking a page must not cost the user its
outline. A named exception inside a guard is exactly the thing that rots, so it
is pinned by an assertion that `ToggleFold` still returns `Some` while the ten
commands beside it return `None`.

Why the notice bar carries the refusal rather than a disabled cursor: the
section's own words are 不能静默吞输入, and a click that does nothing is
indistinguishable from a click that missed. The bar is sticky until dismissed,
so the dedup reads what the bar currently says — the drag-hover path answers
once per frame of one gesture, and rewriting the same sentence sixty times is
its own defect. The hover check itself stays silent on purpose: the absence of a
drop line is that gesture's feedback, and the drop that follows is the moment
worth a sentence. The ⋮⋮ menu's editing rows are **dropped**, not greyed,
because that menu has no disabled state to grey them with.

Consequences:

* The lock is a page property, so it is not a Ctrl+Z step (ADR-0044's rule for
  a look). That is different from *undo being disabled*: the stack built before
  the lock survives it intact and is refused while it is on, so unlocking hands
  back exactly the history the user had, which the test asserts by re-typing
  into the same rows afterwards.
* A duplicated page does **not** inherit the lock, which is the one place this
  slice departs from ADR-0047: font, icon and cover are parts of what a page
  looks like, so a copy that dropped them would open differently from how it
  looked; the lock is a gate on editing, and duplicating a locked page is what
  a user does to edit something like it without touching the original.
* The refusal is therefore two gates, not one. Rust refuses the write;
  `EditorBlock.editing` carries a `!UIState.page-locked` term so the caret
  cannot survive a lock/unlock/relock cycle into a row that would otherwise
  still be typing. A gate on the command layer alone would leave a live input
  on screen that refused nothing until Enter.
* Storage stays boring on purpose: `locked` joins the guarded
  `add_page_columns` list, `insert_page` and the load `SELECT` carry it, and
  `Change::PageLockedSet` is one `UPDATE`. It reaches neither `cover_ids()` nor
  the undo vote in the reclaim sweep (ADR-0037/0047), because a lock stores no
  pointer to any file.
* It stays out of the Markdown channel (§二十六 carries content; a page's
  permissions are not its content) and out of the LAN server, which builds
  `Page` values to read and export and never writes one.
* Track 1's migration priority is spent again: v13 is the lock, so any draft
  numbered 13 or higher moves up — Track 3's草案 included.
* The page ⋯ menu grows a twelfth row. `ContextMenu` height is
  `min(rows * 30px + 8px, window-h - menu-y - 20px)`, so in the anchored-low
  baseline scene the popup was already clamped before this row existed — which is
  why `menu.png` moves as its scrollbar thumb (8 sampled px, one column at
  x 414) and nowhere else, exactly as it did when the cover added a row. A new
  menu row cannot clip a command; it can only make the thumb shorter.

## ADR-0047 · A cover stores the attachment, and the veil's worst case is arithmetic

Decision: SPEC §三十八's cover is stored as `pages.cover INTEGER NULL` (schema
**v12**) holding **an `AttachmentId`** — never a path, never a filename. The
bytes stay the attachment store's, exactly as they do for an image block. The
hero draws them in a band (`Typography.size-cover-height` 168 px) with the page
title inside the band's bottom edge, under one fixed veil
(`Colors.cover-scrim` = `#0000009e`), and the ink over that veil is
`Colors.text-on-accent` for **both** the title and the page's own emoji. Entry
point: page ⋯ → **Set cover** / **Change cover** (one id, the label answers
"is there one?") and **Remove cover**, which only exists when there is. Like
Style and icon, setting a cover is not a Ctrl+Z step, and a duplicated page
starts with its source's.

Why an id and not a path is the same answer ADR-0046 paid to reach: §三十七's
reclaim deletes "the attachments nothing points at", and it can only answer that
question out of the database. A path in a column is a string no sweep can resolve
back to a file, so the user's cover would be the next reclaim's casualty. The
column is therefore **nullable rather than `DEFAULT 0`** — `''` is unambiguous
for an emoji because no emoji is empty, while `0` is a perfectly good attachment
id, so "no cover" had to stay a third thing. The referencer list got two entries
rather than one: `workspace::cover_ids()` joins the reclaim sweep beside the
blocks, and `Change::PageCoverSet` answers `attachment_ids_in`, because an
outstanding undo step is still a pointer — the test that pins this deletes an
image block, restarts the session so undo no longer votes, and requires the
picture to survive *as the page's cover* and then to go as soon as the page lets
go.

Why one fixed veil instead of reading the picture: §三十八 requires the title's
contrast to pass §二十一 and forbids 最弱配色, but the app cannot ask an arbitrary
JPEG what its brightest pixel is without decoding it at hero size on every page
open, and a per-page adaptive colour would be a derived value wanting storage that
ADR-0039 refuses to give it. A constant veil makes the requirement arithmetic with
a bound that holds for every picture: the scrim is black at α = 0.6196, and because
that α is a byte (`0x9e` = 158) the composite is exact — **any** photo pixel lands
at ≤ 255 − 158 = **97** sRGB, a relative luminance ≤ 0.1195, and white ink on that
is **6.19:1**, against a 4.5:1 floor whose own threshold is α ≥ 0.535. The worst
case is a pure-white picture, so
`create_solid_fixture` writes exactly one and `page-cover-white` /
`dark-page-cover-white` are the control scenes; `benchmarks/scripts/contrast_probe.ps1`
then reads the rendered PNG and reports the ratio off pixels (worst ground
`#616161`, 152 002 px measured), with a known-answer arm inside it — 21:1, 1:1 and
6.19:1 — and a synthetic must-fail scene at 1.61:1 so the gate is shown to be able
to say no.

Consequences:

* A picture has no typeface, so the two band constants live in `Typography`, not
  in the per-page `PageType` — but the band's *height* is derived:
  `max(168px, hero bottom + spacing-md)`, because an icon above the title pushes
  the whole hero down and a constant band would leave the white ink standing on
  the page background, which is the one thing the veil cannot protect.
* The first pixel pass found a defect this ADR had not anticipated: the hero's
  emoji still read `Colors.text-primary`, i.e. a `#1f2328` rocket on a `#232439`
  photo — relative luminances 0.0165 against 0.0191, a **1.04:1** ratio, i.e.
  invisible. It now takes the same conditional as the title.
  The headless software renderer draws an emoji as a monochrome glyph so this is
  measurable; the GPU renderers draw colour bitmaps and ignore `color`, so that
  arm is still a hand test (as it has been since ADR-0045).
* Track 1's migration priority (v11–v13) is spent here: v12 is the cover, and any
  draft numbered 12 or higher has to move up.
* The cover stays out of the Markdown channel (§二十六 carries content, and a
  page's picture is not its content) and out of undo, both per ADR-0044's rule for
  a look.
* The page menu is now eleven rows and its popup was already taller than the space
  below its anchor at ten, so the one baseline scene that moved (`menu.png`, 9
  sampled px) moved as the ListView's **scrollbar thumb**, not as a clipped row.

## ADR-0046 · A page's icon is an emoji, and a picture goes on the cover instead

Decision: SPEC §三十八 asks for "icon：emoji 选择器 **+ 本地图片**". The emoji half
shipped as ADR-0045; the local-image half is **decided against, here, for this
slot** — `pages.icon` holds an emoji or nothing, and a page's picture lives in
**cover**, the next group in the same section. This is the "write the cost down
rather than leave the line open" branch of the choice.

The cost is not the picker, it is ownership and the one tier that exists.
Attachments (ADR-0029/0030) keep one file plus **one** downscaled copy at
`MAX_EDGE = 1280` — sized for the ~780 px editor column, 6.5 MB of RGBA at
full size — and `display_path` hands the UI that copy. An icon slot is 16 px in
the sidebar and 46 px in the hero. Riding the existing channel means either
painting a 1280-edge raster into a 16 px box on every sidebar row (the tree
rebuilds on every mutation, so that working set is paid per rebuild for a mark
the size of a letter), or adding a second tier — a second file per attachment,
a second entry in `remove`, and a second thing the M10 reclaim has to know about.
That last one is the real price: reclaim deletes "the attachments nothing points
at" by walking block references, and an icon is a **page** pointer, so a page's
icon is exactly the kind of file that gets reclaimed as garbage by a scanner that
only knows the referencer it was written against. Making that safe is a
referencer list, not a flag.

The column would change shape too. `pages.icon` today is a glyph, and `""` means
unset; a file reference makes it "either an emoji or an attachment id", which is
the tagged union this schema has avoided everywhere it has a choice (`kind` and
`lang` are strings for that reason). And the value is thin: a photograph reduced
to 16 px is the least legible version of the picture the user just put on the
page, and Notion itself keeps its icon picker emoji-only with uploads on the
cover — so this is not a parity gap being papered over.

Consequences:

* §三十八's icon line now says emoji, and points here; the picture requirement is
  carried by the cover line, which already asks for 换图 / 移除 and a contrast
  check — a raster with somewhere to be legible.
* The reopen trigger is concrete: if a raster ever has to live in the 16 px slot
  for another reason (a workspace mark, an avatar), then the second downsample
  tier and the reclaim referencer list get built **together**, and this ADR is
  where the cost was already written down.
* ADR-0045's "local image belongs to slice 3" is superseded by this line — slice
  3 is cover, and it inherits the picture half of the requirement.

## ADR-0045 · A page's icon is the emoji itself, and the placeholder is read three ways

Decision: SPEC §三十八's icon is stored as `pages.icon TEXT NOT NULL DEFAULT ''`
(schema v11) holding **the emoji character**, not an index into the picker. The
picker's whole catalogue lives in Rust (`core::icon::PICKER`, 96 emoji in 12 rows
of 8, with `PER_ROW` shared with the `.slint` grid) and is copied into a
`ModelRc` at open, so no `.slint` file names an emoji. Entry point: page ⋯ →
**Set icon**, which replaces the menu with a `IconPicker` popup rather than
nesting under it.

Why the glyph and not an index: an index makes the catalogue a wire format, so
adding one emoji in the middle silently re-points every page written before it.
Storing the glyph means this list can grow, shrink or reorder and nobody's page
changes — which is also why the entries are written as `char` escapes in the
source, several being a base code point plus a variation selector that an
"invisible characters" cleanup pass would quietly drop.

The harder decision is what "未设置时用标题首字符占位" means in three places,
because the empty slot does not mean the same thing in all of them:

* **Tree rows** show the title's first character (`icon::slot`). That slot held
  a generic page glyph, which said nothing about *this* page; an initial says
  something.
* **Favorites / Recent rows** show the stored emoji *only*, and otherwise keep
  the star and the clock. Those two marks are the section's own identity — a
  placeholder there erases information rather than standing in for missing
  information. Hence two functions, `slot` and `icon_mark`, not one.
* **The editor hero** draws nothing above an iconless title. Repeating the
  title's first character at 46 px directly above the title at 40 px is an echo,
  not a placeholder.

The initial is the first non-whitespace **scalar value**, not the first grapheme:
a title opening on a ZWJ sequence would split, which for a 16 px slot is a
cosmetic risk taken on record rather than a new unicode dependency.

Migration 11 is one conditional `ALTER TABLE` and no rows, and it goes through a
new shared `add_page_columns(conn, &[(&str, &str)])` that migration 10's body was
rewritten to call too. The helper exists because every late `pages` column asks
the same question ("is it there yet?"), and a partially-migrated file — a column
an older build added by hand, a backup restored mid-step — should converge
instead of erroring on a duplicate name.

Consequences:

* The sidebar's parent rows had to give up a second box: a parent already spends
  its 16 px slot on the chevron, and adding an icon box next to it pushed every
  parent's label one indent past its own children. The emoji now shares the
  chevron's box and hands it back on hover, so the tree's geometry is unchanged
  for every row that has no icon.
* Emoji are drawn through one named face, `Typography.emoji-font`
  ("Segoe UI Emoji"), which is the only place in `.slint` that names a font —
  Slint resolves per glyph, and the token exists to make the choice visible
  rather than to be load-bearing. Under the headless **software** renderer the
  emoji come out monochrome and follow the text colour, so `dark-page-icon` was
  added to the sweep to answer whether the mark survives the dark theme. The GPU
  renderers were **not** measured here — whether they paint the colour layers is
  a hand-test item, not a claim in this file.
* Setting and clearing an icon are persisted as `Change::PageIconSet` and are
  **not** on the Ctrl+Z stack, same as Favorite and Style before them: a property
  of the page is not an edit to the document. `duplicate` carries the source's
  icon in memory and in the persisted `PageCreated`, which is the bug ADR-0044
  caught once already.
* The picker's grid is data, so the `.slint` side has no emoji list to drift:
  `fill_icon_picker` copies `PICKER` into the model on every open, and adding a
  row to the catalogue is a one-line change in one file.
* Gate: **no RAM bench owed**, and the reason is the shape — one short string per
  page, one property write on open, and one `Text` per row that replaced a `Text`
  that was already there. The pixel evidence is the substitute, and it is the
  wide kind: **all 64 existing scenes moved**, 72 937 px between them, because
  every iconless tree row now shows an initial where it showed a generic glyph.
  The movers are the icon column and nothing else — the modal scene is
  1 153 px inside **x 13..52 / y 365..713**, which is the 16 px slot at depths
  0–2 — and re-running the same comparison restricted to **x 53 and beyond**
  returns **0 px for 63 of the 64 scenes**, with `menu.png` the exception at
  345 px inside x 254..415 / y 723..775 (its own popup, one row taller). So the
  labels did not shift a pixel — the indent fix below is measured, not assumed —
  and the document area moved by zero everywhere, which is the number that says
  an iconless row costs exactly what it cost before.
* Sweep 64 → **67** scenes (baseline `.scratch/sweep33`): `page-icon`, `icon-picker`
  and a dark arm. The picker scene deliberately opens on an iconless page, since
  that is the state a user is in when they reach for it.
* The hero icon is display-only this slice: there is no hover "Add icon" strip
  above the title, so the menu is the only way in. That affordance and the cover
  behind the title are the next slice's, and they want the same seam.
* Remaining §三十八 groups: **cover**, **lock**, **version history** (still owes
  the disk-and-RAM retention numbers), **templates**. The local-image half of
  icon is not on that list — ADR-0046 decided against it the same day, and the
  picture requirement now sits on cover.

## ADR-0044 · A page's look is derived, so no block ever holds a size

Decision: SPEC §三十八's three switches — font (default / serif / mono), full
width, small text — are stored on the **page** (`pages.font TEXT`,
`pages.layout INTEGER`, schema v10) and reach the screen through one new global,
`PageType`, that *derives* the document tier from three `UIState` properties. No
block gains a size, a family or a flag. Every document-tier call site was
repointed from `Typography.*` to `PageType.*` — 119 of them across four files —
and the chrome (sidebar, menus, palette, settings, and a block's own caption
line) still reads `Typography`, which is why it provably cannot move.

Why a second global instead of overriding at the call sites: the alternative was
119 ternaries on a page property, i.e. the same token question re-asked at every
site, which is the rule the type scale exists to enforce. And why not mutate
`Typography` itself: it is the one place chrome and document share, so a page
that wants small text would shrink the sidebar with it. `PageType` is a *child*
of `Typography` — it multiplies the document tier by one factor and re-points
the family — so the two tiers stay separable and the derivation is readable in
one file.

The three switches are stored as one string and one bit field, not three columns.
`font` is TEXT because every other catalogue in this schema is a string
(`blocks.kind`, `blocks.lang`), and because the failure mode of an integer code is
that a reordering silently re-points every existing row: `PageFont::try_from_str`
turns `"comic sans"` into `Default`, so a page written by a future build opens as
an ordinary page rather than as an error. `layout` is one INTEGER because the
two switches always travel together — `Change::PageLayoutSet` carries both, the
menu writes both, and the reader tests two bits — and two BOOLEAN columns would
add a second `ALTER TABLE` to a step that already has to be conditional on each
column's absence.

Small text is one factor, 0.87, applied to the whole document tier including the
headings. That is 13.5 / 15.5 — the ratio between this app's own code and body
sizes, which keeps the two tiers' relationship intact. Line heights are **not**
scaled: they are factors of the natural line box, so a smaller face already gets
a proportionally smaller box, and multiplying them too would tighten leading on
top of shrinking glyphs.

Two seams needed a second look. Marked runs name an italic *family* rather than
setting `font-italic`, so a serif page would have italicised its emphasis in
Segoe while every other word was Georgia — hence `PageType.italic-font-family`.
And the app-wide `code-probe` measures the monospace advance that the highlight
layers (ADR-0042) wrap against: it now reads `PageType.size-code`, so a small-text
page's colour layers stay measured against the glyphs they colour.

Consequences:

* The page menu gained a Style submenu (Back, three fonts with a check on the
  live one, Full width, Small text) and the whole rest of the sweep is
  byte-identical: 56 of 57 scenes unchanged, `menu.png` the only mover at 1 430 px
  inside x 240..423 / y 542..779 — the submenu's own box. A token change that
  touched chrome would show up as 56 movers.
* The seven new scenes are the menu's own shot plus six measurements, each taken
  against `default.png` with a self-vs-self control at 0 px: serif 63 638 px,
  mono 62 780, small text 55 285, full width 51 533, all three at once 68 779
  against the serif shot, and the dark arm 63 140. Full width is the only one
  whose bounding box starts at x 284 (sidebar 260 + `Theme.spacing-xl` 24) where
  the others start at 390 — that is the gutter moving, and it is the number that
  separates a real layout switch from a font swap.
* `duplicate_page` carries the source's style, and `workspace.duplicate` had to be
  fixed to match: it created the copy with defaults while the persisted
  `PageCreated` carried the source's look, so a duplicated page read differently
  before and after a restart.
* `open_page()` re-applies the three properties, so the derived tier follows
  selection with no per-page model churn, and a page with no stored look costs
  three writes of the defaults.
* Migration 10 adds two columns and no rows: a v9 library's every page reads
  `font = ''`, `layout = 0`, i.e. exactly what it looked like before, which is
  what `the_v10_step_adds_the_page_look_to_a_v9_database` insists on.
* The remaining §三十八 groups are untouched: icon, cover, lock, version history
  and templates still need their own decisions — `pages.layout` is a bit field
  with room left, but this ADR does not spend it.

## ADR-0043 · A find hit is a cell, and its border is the marker

Decision: the in-page find bar (SPEC §二十) paints every occurrence as **one
word-run cell** (ADR-0041). `TextRun` gains `hit: bool`; `build_runs` takes that
block's hit ranges as two more byte boundaries each; the projection carries them
as `FindHits = HashMap<i32, Vec<(usize, usize)>>`; and the delegate draws a
`Rectangle` under the glyphs of any cell whose `hit` is set. A hit that starts or
ends mid-word **splits the word**, because a cell is the smallest thing this
layout can paint and a cell painting half a match would be the same defect with a
tint on it.

Why not ADR-0042's colour layers: that trick works because a code block is
monospace, so six copies of one string stack glyph for glyph. The editor body is
proportional — there is no byte offset that predicts a pixel — so a per-character
colour is not available here. A background is: the runs flexbox already cuts a
line into cells at word boundaries, so adding the hit's two offsets as boundaries
makes a cell that is exactly the occurrence, and a `Rectangle` behind it is the
same decoration the inline-code box already uses.

Two rules fall out of the cell model. A hit **never cuts a mark**: a marked
stretch is one `Text`, and a formula's cell shows glyphs that its own byte space
does not describe (`\alpha` reads α), so a hit that lands inside a mark tints the
whole mark instead of part of it. And a row with neither marks nor hits keeps
`runs: []`, which is what tells the delegate to draw one wrapping `Text` — so a
search that never started costs nothing, and one that is closed costs a re-paint
of the rows it had touched.

The repaint is targeted rather than a re-projection. `paint_find_hits` walks the
row model once and rebuilds the rows that carry a hit **now or carried one on the
previous search** (`AppState::find_painted`), because the bar does this on every
keystroke and a 10 000-block page must not re-project to repaint a dozen cells. A
hit in a grid cell or a layout box has no row of its own (ADR-0028), so
`row_id_of` walks it up to the row that paints it and rebuilds that row's flat
`table_cells` / `column_items` lists whole — the hit rides on a row that is not
the one the block owns.

The palette pair is `Colors.find-hit` (fill) and `Colors.find-hit-border`, and the
split is measured, not styled: a fill bright enough to be a 3:1 marker on its own
would take every coloured-text pair below the floor ADR-0023 accepted (an amber
that reaches 1.39:1 on white drags the palette's own yellow text to 2.85). So the
fill stays pale and says only "this cell" — light #ffe9a8 keeps body text at
13.14:1 (from 15.80) and its worst pair at 3.29 (from 3.95); dark #3d3413 keeps
10.02 (from 14.06) and 3.83 (from 5.37) — and the **border** carries the marker:
#bd6408 and #a87718 read 4.21 and 4.39 on their page and 3.50 and 3.13 on the
fill they sit in, so the marker clears 3:1 against both things it touches.

Consequences:

* One hit is always exactly one cell — a hit's own range is `covered`, so the
  word-cut never runs inside it, and two adjacent boxes mid-match cannot happen.
* The hit box and the inline-code box are the same geometry, and the hit is
  declared second. A search that lands on a code span shows the match and loses
  the span's gray background: the bar's job is to say where the text is.
* `project_blocks`, `table_cells` and `column_projection` each take a `&FindHits`,
  which is `&FindHits::new()` at every call site that is not the find path. An
  empty map costs a row one failed hash lookup, so the closed bar is not free but
  is one lookup per row wide.
* The active hit is **not** distinguished from the other hits by this slice. It
  already is by the editor's own selection (the bar steps the caret there), and
  the two overlays read differently in both themes — so a "current match" colour
  is a separate decision with a separate measurement, not a missing flag.
* `find`, `find-grid`, `find-cols` and `find-callout` are the pixel evidence, one
  per surface a match can land on: a block's own line, a grid cell, a box inside
  a columns layout, and a callout's tinted frame. Each of the last three was a
  scene before it was a passing scene — `find-grid` needed the search term
  changed to a word that only exists inside the grid, because with the bar's own
  "the" it painted seven boxes on the page and none in the cell it was meant to
  prove.

* The runs flexbox is now the text renderer for a **quote and a callout** as
  well, not just for the plain kinds. They hold one line of body text each, and
  each draws it through its own single `Text`, so a match inside one had nowhere
  to sit while the counter still counted it: on the swept page that was 2 of 16
  hits, found by measuring the shot rather than by reading the code. They share
  the flexbox and bring only their own frame — `runs-x` / `runs-y` /
  `runs-width` / `runs-height` answer per kind, and the plain case reads exactly
  the expressions it used to.

* Paint order is part of that frame. The callout's tinted box was declared
  *after* the runs, and Slint paints later siblings on top, so the first version
  of this ADR's callout arm passed every gate and painted nothing: the box
  covered its own cells. The box and its emoji moved above the runs; the emoji
  and the text still sit on top of the box, which is the only ordering the block
  actually needs. The symptom was an empty callout — the single `Text` had
  stepped aside for runs, and the runs were there but hidden — which is the
  shape this trap always takes.

* A hit whose block is **being edited** is not a cell, and that is not a gap.
  Stepping the bar moves the caret onto the match, so that one block answers
  with the editor's own text selection; its other occurrences lose their cells
  for as long as it holds the caret. On the swept page 16 hits are 14 cells plus
  the 2 in the block under the caret, counted at a 3 000px-tall window so that
  nothing was simply scrolled off.

## ADR-0042 · One colour is one layer, not one run

Decision: a highlighted code block paints as **six copies of the same string**.
`core::highlight::layer(text, lang, frame, advance, kind)` lexes the block, gives
every character one of six colours, and returns the whole block with every
character that is not `kind` replaced by `U+00A0` — plus a hard newline wherever a
line passes a column budget (`floor(frame / advance)`; ASCII is one column, a tab
eight, anything else two). `code-text` asks for kind 0 and five `Text`s in one
conditional `Rectangle` ask for the other five, drawn 1, 3, 4, 5 and then 2. All
six have the same length, the same characters per line and the same hard newlines,
so they stack glyph for glyph and no second layout engine has to agree with the
first.
The row's height stays `code-text`'s, which is the point of counting kind 0 as a
layer: the string that measures the row is one of the strings that colour it.

Two seams hold it up. The advance of one character comes from **one** invisible
probe `Text` in `Editor.slint` that writes `UIState.code-advance` from a `changed
width` handler — not from a row, because which rows a `ListView` realizes depends
on the scroll position and where a block's lines break must not; `0px` before the
first measurement means "add no hard breaks", which is the safe answer since
Slint still wraps. And the colours are `Colors.code-token`, which maps the five
token kinds onto five slots of the block palette this app already has (keyword =
purple, comment = gray, string = green, number = orange, name = blue) rather than
shipping a second palette tuned for the same two backgrounds.

Language is the one new fact a block stores: `Lang` (Plain / Rust / Python / Js /
Ts / Md / Json / Bash) in `blocks.lang`, migration v9, undoable as
`Change::BlockLangSet`, picked from a Language submenu that only a code block's
⋮⋮ menu shows, and carried by the fence's info string in both Markdown directions
(```` ```rs ```` comes back as Rust and leaves as ```` ```rust ````; a language with
no lexer folds to `Plain`, which is no colour rather than a broken block).

Why: Slint 1.18 `Text` paints one colour, and ADR-0041 had just made the runs
channel break at word boundaries — but a run is still one layout *cell*, so a
per-token colour could not wrap and highlight was parked behind that wall. Six
whole-block strings dodge the wall instead of climbing it: wrapping is Slint's
job on each layer, the lexer never has to know about words, and typing pays
nothing because the layers exist only while the block is not being edited
(`is-highlighted && !editing`), so no keystroke lexes.

Consequences:

* **A character whose width this model cannot know is copied rather than blanked.**
  A non-ASCII character goes into all six layers verbatim, so on a line that
  carries both a comment and non-ASCII prose the comment colour wins — it is drawn
  last. That is the documented boundary (a CJK comment reads gray, a CJK string
  reads green, a CJK string on a commented line reads gray), and it errs towards
  showing the character rather than towards the wrong colour.
* **The model assumes a monospace font**, and `clip: true` on the layer Rectangle is
  the floor under that assumption: if the theme's font is not quite one, a layer
  ends up taller than the measure and the row keeps the measure's height.
* **Two defects that only a shot could catch, both invisible in the code.** A layer
  `Text` with no `width` does not wrap — it breaks only at the lexer's hard
  newlines — so the five layers drifted a line away from the measuring layer, and
  the first `code-hl` shot printed text that read as scrambled. The fix is
  `width: root.code-width` on every layer. And `visible: false` does not stop a
  binding from running, so those five lexer calls fired on every repaint of every
  row of the page until the Rectangle became a conditional element. Both rules are
  now in `docs/UI_ARCHITECTURE.md` with the box they showed up in.
* Six `Text` elements on a highlighted row (the measuring layer plus five colours);
  a plain code block has one, and every other row of the page gains none, because
  the `if` that guards the layers is the same guard the six are behind.
* No JS/WASM runtime and no new dependency, which is what SPEC §三十七 asked for.
  The lexer is ~800 lines of switch over six language families; a real tokenizer
  would be its own ADR.
## ADR-0041 · A marked line breaks where its words do, because a run is one layout cell

Decision: `build_runs` (src/app/state.rs) emits one run per **word** inside an
unmarked stretch, and leaves a marked stretch whole; all three places that draw
runs — `ui/components/EditorBlock.slint`, `TableBlock.slint`,
`ColumnItemRow.slint` — become `FlexboxLayout { flex-wrap: wrap }` where they
were a `HorizontalLayout`. Nothing else about the channel changes: each row's
height is still the invisible plain `Text` beside it — the one that already
measures the line count for unmarked text at the same width — the runs box is
still `clip: true` at that height, and the live `TextInput` still shows plain
text while the block is focused. Whitespace stays attached to the word it
follows, so the cells re-join to the original string byte for byte, which is
what the two new tests assert (one with a fixed expected cell list, one over a
leading mark, CJK text and a mark at the end).

Why: Slint 1.18 `Text` has no inline formatting at all — no per-run style, no
decoration, no `TextFormat` — so a mark can only be painted as a separate item,
and that was the documented platform wall (`docs/EDITOR_ARCHITECTURE.md`
§"Platform wall", A4's one open HIGH): a paragraph carrying marks lost its word
wrap and clipped mid-word, while the identical unmarked paragraph wrapped. A
layout cell cannot break, so the old per-mark run "the same delegate that
paints a ten thousand line page" was one unbreakable item. Cutting the plain
stretches to words hands the flexbox the same break opportunities the shaper
has. Marked stretches stay atomic on purpose: a link split at every space gives
each fragment its own underline and its own click target, and a code span split
per word is a row of separate boxes.

Consequences:
- **The wall moved; it did not vanish.** Two shapes still clip, and both are
  now the *narrow* case rather than the normal one: a marked phrase long enough
  to exceed the line on its own, and a marked line whose runs need more lines
  than the same words unmarked — bold and mono are wider than regular, and the
  height authority is the plain-text measure, so the extra line has nowhere to
  go. Fixing the second means giving the runs container its own height, which
  this slice tried first: binding `body-height` to the flex's
  `preferred-height` is a Slint compile error (`Cannot access id 'runs-flex'` —
  an element declared inside an `if` cannot be named from outside it), and
  hoisting the `if` so the flex always exists costs two items on all 10 000
  bench rows. The word cut is the version that fits the existing geometry.
- **No migration, no new kind, no model field.** `user_version` stays 8, the
  runs channel is still `Vec<TextRun>`, and storage, undo, Markdown and the LAN
  export are untouched — this is one pure function's output shape and one
  layout element.
- **The gate had to be fixed before it could be read.** Scene D has no marks at
  all, so the old 10 000-block RAM gate was blind to this change; publishing its
  ratio would have been a measurement that only looked like one. So the harness
  gained `--marks N` (main.rs, `HandleArgs.marks`, `bench_marks`, quire-typing,
  `bench.ps1 -Marks`) and `--dump-state` now prints `marked=N`, which lets each
  arm prove its own fixture instead of taking the script's label on faith. All
  twelve rows read `gs+atlas-blocks=10024 marked=1000`.
- **Gate: 1.03× and 1.02× the control's private bytes, in two batches** (raw
  rows `benchmarks/results/2026-09-21-m8-wordwrap-ram.jsonl`, 12 of them).
  Control = `2c5a25f` built in a clean worktree with *only* the bench knob
  ported, so its runs still render on one line (md5 `cdf6da5a…`, 22 074 368 B).
  Scene D, 10 000 blocks, 1 000 of them marked, arms alternating, one pinned
  database per arm, each arm's seeding run excluded:
  batch 1 (this tree md5 `f59edb1b…`, 22 081 024 B) control 112.7 / 111.4 →
  this tree 115.2 / 115.9 = **1.031×**; batch 2, after the table and column
  delegates joined (`4f9205ca…`, 22 087 680 B) control 114.3 / 111.1 → this
  tree 114.8 / 115.7 = **1.023×**. Both are well inside the ≤1.2× gate, and the
  honest reading is at the gate's own resolution: the +2.5…3.5 MB between the
  two means is smaller than the 3.6 MB the *control* arm spread across by itself
  in batch 2. What the two batches do pin down is that the paragraph change has
  a shape — the marked rows go from 3 runs to 11 cells each, and a cell is an
  item with its own measured `Text` — and that the two delegates scene D never
  realizes cost nothing on it, which is exactly what they should.
- **The projection cost of the cut, measured in both arms** (an `#[ignore]`d
  timing test in `state.rs`'s module — the same shape ADR-0039's projection
  number takes, and re-taken at another commit by copying that one test into a
  worktree, which is `#[cfg(test)]` code and touches nothing under measurement;
  50 rounds of `project_blocks` over 10 000 rows): unmarked 41.686 ms control /
  41.504 ms this tree — the two arms agree to 0.4 %, which is the control that
  the fixture and the machine are the same — and 1 000 rows marked 44.147
  (+2.461) / 46.942 (+5.438). So the word cut costs ≈2.8 ms per projection of a
  page whose every tenth line carries a mark, ≈2.8 µs per marked line, and
  nothing at all on an unmarked page.
- **Pixels.** `sweep23` → `sweep24`, 49 → 50 scenes: 3 moved, 46 byte-identical,
  1 new. `marks` and `dark-marks` both at 1 918–1 942 sampled px inside
  x 390..1148 / y 196..240 — one paragraph band, and the new shot shows the
  marked line as two complete lines instead of one clipped one. `math-inline`
  moved 360 px inside x 420..664 / y 194..210, which needed a second probe
  because it is the scene where the change should be invisible: an ink-edge scan
  puts the right-most glyph column at 661 before and 664 after, i.e. ≈3 px of
  extra line spread from measuring words separately instead of one 40-word
  string — sub-pixel per gap, and the shift probe says it is not a translation
  (a pure dx/dy shift does not zero the difference). New scene `marks-wrap`
  exists precisely because the old `marks` fixture could not show four lines of
  mixed marks: a ~380-char sentence with a bold phrase, an italic word, a code
  span and a link, and its needles now `.expect("needle present")` instead of
  `unwrap_or(0)` — silently marking offset 0 is how `marks` once demoed the
  wrong words.
- **Pixels, second half: the other two run rows, and a zero that had to be
  argued with.** `sweep24` → `sweep25` moved **0 of the 50** existing scenes and
  added 2 (`table-marks`, `columns-marks`). That zero is not the evidence — the
  two new shots are, because a wrapped cell and a wrapped column line are
  exactly what no scene in the baseline contained: the grid's fixture holds one
  word per cell and the layout's holds "Column 1 / Column 2", so the delegates
  changed and nothing was there to show it. Both new scenes put an over-long
  marked sentence into the narrowest box each surface has — the cell at a third
  of the grid, the line at half the page — and both render their wrapped lines
  with the bold phrase on the line the shaper put it on, the row grown to
  `cell-text.height` / `plain-text.height` around them. A scene that could not
  fail is how this slice would have shipped a silent no-op.
- **Still needs a human window.** No headless scene proves: clicking to a caret
  position on a *wrapped* marked line (the overlay is hidden while editing, so
  the mapping is plain-text, but it should be watched), a marked paragraph that
  grows a line while you type in it, a link whose cell is now one word rather
  than the whole phrase (hover target, and whether a single-word underline
  reads as a bug), and a marked CJK paragraph, which has no ASCII spaces to cut
  on and therefore keeps the old one-cell-per-mark behaviour entirely.

## ADR-0040 · A link card derives its two lines at paint time and hands the address to the system

Decision: `BlockKind::Embed` (row id 22, database string `"embed"`) stores the
address in `blocks.text` and nothing else. The card's two lines are computed
while drawing, through two pure callbacks on `UIState` — `embed-label(string)`
and `embed-url(string)` — whose Rust side is `core::embed::describe` and
`with_scheme`: the headline is the provider the host belongs to (21 named hosts,
plus Google's products read from either the subdomain or the first path
segment), or the host itself when nothing matches, or `Embed`/`Link`/`Email` for
the three shapes that have no host to name. The second line is the address the
Open button will actually use, so a bare `example.com/a` reads as
`https://example.com/a`. Clicking the card edits `block.text` the way a paragraph
does; clicking the arrow fires `open-link` and leaves the app. There is no
WebView, no fetch, no favicon and no oEmbed (SPEC §二 and §三十三), which means
the card *is* the feature rather than a stand-in for one. Markdown writes the
address alone on a line; import reads back only a line that is one token and
carries an explicit `http(s)://`.

Why: a link block that stored a title would be a copy of something that can
change, with no way to refresh it and no owner to invalidate it — the same
argument ADR-0039 makes for the contents block, and here it is cheaper still
because the derived value is a string function rather than a page walk. Storing
the raw address in the existing `text` column, rather than a url field or a JSON
payload, is what keeps this the fourth new kind in a row with no migration. And
the bare-address Markdown shape is the one a GFM renderer already turns into a
link: a `<…>` wrapper or a `<!-- quire:embed:… -->` marker would be an extra
thing to corrupt when the text is not a well-formed address, and it would render
as nothing everywhere else.

Consequences:
- **No migration, as a checked fact.** `user_version` stays 8. There are now 23
  block kinds, `BlockKind::ALL` lists them, and an unknown kind is still
  corruption-on-load, so an older build that meets an embed fails loudly instead
  of dropping the row.
- **Nothing is added to the row model.** The card's lines are pure callbacks
  evaluated for the rows that draw them, so a page with no embed pays nothing
  and a page with one pays two string functions per visible embed row — the
  ADR-0038 lesson applied where the alternative was per-row model fields.
- **The address is exported verbatim, without inline-mark rendering.** A url is
  full of `_ * ~ &`, and running it through the mark renderer would both change
  the address and make the card open something else than it shows.
- **Import is deliberately narrower than export.** `is_bare_address` demands one
  token and an explicit scheme, so "see https://example.com for details" stays a
  paragraph. The control test asserts exactly that, because the easy way to make
  the round trip pass is to widen the rule and lose prose.
- **A link's target is data, so the shell branch is now allowlisted.**
  `open-link` reaches `cmd /C start` on Windows, and `start` will *run* a path, a
  UNC share or an installed protocol as readily as it opens a url. `core::embed::
  is_openable` (http/https/mailto) now gates that branch. This is defence in
  depth, not a fix: a compiled probe showed std quotes the argument, so
  `… & echo PWNED` reached `start` as one literal string and did not split — the
  injection was never there. What did change is behaviour: `file:///…`,
  `\\share\x`, `javascript:…` and a bare `C:\…\calc.exe` in a link now do
  nothing at all, which is the intended trade for a document that can arrive
  from an import or the LAN server.
- **The gate found nothing again.** Control `4fa2b7b` from a clean worktree
  (md5 `9c5e153f…`, 22 017 024 B) versus this tree (md5 `19aea07c…`,
  22 072 320 B), scene D at 10 000 blocks, arms alternating, one pinned database
  each (raw rows `benchmarks/results/2026-09-21-m11-embed-ram.jsonl`). Steady
  private bytes: control 110.9 / 112.1, this tree 111.8 / 112.3 — **1.005×**,
  inside the ≤1.2× gate, and the 0.55 MB gap between the two means is *smaller
  than the 1.2 MB spread inside the control arm alone*, so the reading is no
  measurable cost rather than a small one. Each arm's first run (109.7 / 111.3,
  startup 807 / 1104 ms) is the pass that seeds its database and is excluded. The
  exe grew 55 296 B for one module, two callbacks and the card.
- **Pixels.** `sweep22` → `sweep23`, 49 scenes: 3 moved, 2 new (`embed`,
  `embed-empty`), 44 byte-identical. The three are the menus that gained a row —
  see `docs/UI_ARCHITECTURE.md` for the boxes.
- **Still needs a human window.** No headless scene presses Open, so these are
  unverified by this build: the browser actually coming up, the card mid-edit
  (typing an address and watching the headline re-derive), hover on the arrow,
  the dark-theme card, and Turn into → Embed → back to Text.

## ADR-0039 · A contents block stores that it is one, and reads its list off the page

Decision: `BlockKind::Toc` (row id 21, database string `"toc"`) holds no
content. Its body is the page's own headings, collected in `project_blocks` by
`toc_entries(blocks, shown)`, which walks the very list of visible indices the
projection just computed and keeps the rows whose kind answers
`BlockKind::heading_level()`. The row carries them as
`toc-entries: [TocEntry]` — `{ block, label, level }` — and the delegate paints
one line each, indented `(level - 1) * 16px`. Clicking a line fires
`callback toc-jump(int)`, and Rust walks the path a `quire://block/` anchor
already walked: `flush_pending_edit` → `set_editing_id(-1)` (recreate the
delegate so the input takes over) → `focus_block`. Markdown writes one marker
line, `<!-- quire:toc -->`, and reads that line back as the block.

Why: SPEC §三十七 批次 C says 派生数据不入库, and a copy of the headings is the
one derived thing in this app that is guaranteed to go stale — renaming a
heading would leave a directory listing the old name, with no edit that fixes
it. Deriving at projection time makes the copy impossible by construction and
costs no migration, because kinds are strings in the database (ADR-0030's
argument, now on its fourth kind). Reusing `shown` rather than re-walking the
page is the same argument one level down: a heading behind a fold or inside a
container has no row to scroll to, so a contents list built from a second,
independent walk would advertise links that go nowhere — and the two walks
would eventually disagree.

Consequences:
- **No migration, as a checked fact.** `user_version` stays 8. There are 22
  block kinds, `BlockKind::ALL` lists them, and an unknown kind is still
  corruption-on-load, so an older build opening a library containing a TOC
  fails loudly instead of dropping the row.
- **The walk is guarded by kind on the Rust side.** `toc_entries` runs only for
  a row whose kind is `Toc`, so a page with no contents block pays nothing for
  the feature and a page with one pays exactly one page scan per projection —
  the lesson ADR-0038 learned for the math binding, applied where the cost
  would be a scan rather than a string conversion.
- **Not editable, still selectable.** `editing` excludes kind 21 the way it
  excludes a divider, a picture and a page block: the row has no text to hold a
  caret, and the live TextEdit would show a source string the block does not
  have. Its TouchArea reports `MouseCursor.pointer`, because a contents line is
  a target rather than a place to type.
- **A converted line keeps its words, unpainted.** `SetBlockType` into `Toc`
  leaves `blocks.text` alone (the same precedent `Divider` set), so a paragraph
  turned into a contents block keeps its text in storage and in the FTS index
  while nothing draws it. That is inherited, not new, and it is why export
  writes the marker rather than the text: the marker is the only part of the
  block that means something outside this library.
- **The click moves the caret and does not scroll.** Slint 1.18's plain
  `ListView` has no `bring-into-view` — that function lives on
  `StandardListViewBase`, the fixed-row-height list a `ListView` gets its
  scrolling from — and nothing in `i-slint-core` moves a Flickable's viewport
  when a child takes focus. So a jump to an off-screen heading selects it
  without revealing it. `quire://block/` anchors from M8 have always behaved
  this way; the TOC inherits the limitation rather than hiding it, and a
  variable-height `reveal` is its own slice (it would fix both).
- **The gate ran against a same-session control and this time found nothing.**
  Control `cc7ccf0` from a clean worktree (md5 `a81ce6df…`, 21 928 960 B) versus
  this tree (md5 `f6d9a576…`, 22 017 024 B), scene D, arms alternating, one
  pinned database each (raw rows
  `benchmarks/results/2026-09-21-m11-toc-ram.jsonl`). Steady-state private
  bytes: control 110.5 / 112.1, this tree 112.5 / 111.6 — **1.007×**, inside the
  ≤1.2× gate, and the 0.75 MB gap is smaller than the 1.6 MB spread inside the
  control arm alone, so the honest reading is *no measurable cost* rather than a
  small one. Each arm's first run (112.7 / 110.0) is the pass that seeds its
  database and is excluded. The exe grew 88 KB: one enum arm, one struct, one
  callback and the delegate's contents list.
- **Pixels.** `sweep21` → `sweep22`, 47 scenes: 3 moved, 1 new (`toc`), 43
  byte-identical. The three are the menus that gained a row — see
  `docs/UI_ARCHITECTURE.md` for the boxes.
- **Still needs a human window.** No headless scene clicks a contents line, so
  hover feedback, the caret landing in the heading, and a contents block on a
  page with no headings yet (it paints "No headings on this page yet") are
  unverified by this build.

## ADR-0038 · A formula is stored as source, and its picture is derived on the way out

Decision: math is two surfaces over one renderer. `BlockKind::Math` (row id 20)
keeps LaTeX-subset source in `blocks.text`; `MarkKind::Math` keeps the same
source inline, as a span over the bytes *between* the `$`s. Neither stores a
rendered string. `core::math::to_unicode` is the only renderer, reached twice:
the block row asks for it through a `pure callback math-render(string) ->
string` on `UIState`, and `build_runs` calls it while projecting inline runs, so
the `TextRun` it hands the UI already holds glyphs. No layout engine, no schema
change, no migration. And because this scene is the first marked line in the
sweep short enough to leave slack inside its frame, all three run rows gain
`alignment: start` — a Slint layout's default `stretch` had been spending that
slack as gaps between the runs.

Why: SPEC §三十七 批次 C asks for a LaTeX subset and sets the bar at "渲染优先
Unicode 近似排版", gating a typesetting engine behind its own ADR plus memory
numbers. Unicode approximation crosses the bar without triggering that gate: no
engine, so no numbers to owe. Storing source rather than glyphs is what makes
the renderer replaceable — when an engine does arrive it replaces one function,
and no library written before it needs migrating, re-exporting, or a different
search index. It also keeps the two-way `text <=> UIState.editing-text` binding
honest (the user edits the formula, never its picture) and keeps Markdown a
round trip instead of a one-way render.

The renderer's contract is three rules, and they are what its tests pin:
- **the output never loses what the user typed.** An unknown command comes back
  as its own source, an environment likewise, so the worst reading is "this did
  not render" and never "this vanished";
- **whitespace in the source is content, not syntax.** TeX discards spaces in
  math mode; this does not, because in a single-line Unicode fallback the space
  the user typed *is* the only surviving expression of their spacing
  (`\alpha + \beta` and `\alpha+\beta` render differently, on purpose);
- **it is idempotent.** `to_unicode(to_unicode(x)) == to_unicode(x)`, which is
  what lets a row re-derive its text on every binding evaluation without
  drifting.

Consequences:
- **No migration, as a checked fact.** Kinds and mark kinds are strings in the
  database (`BlockKind::as_str` / `MarkKind::try_from_str`, read back in
  `storage/repository.rs`) with no CHECK list to widen — ADR-0030's argument for
  `file`, repeated because it keeps paying. `user_version` stays 8. There are 21
  block kinds now, and an unknown kind is still corruption-on-load, so an older
  build reading a math library fails loudly instead of silently dropping rows.
- **A derived binding is a per-element cost, not a per-kind one.** The math Text
  is `visible: false` on every other row, and invisible elements still evaluate
  their bindings, so the text reads `is-math ? UIState.math-render(…) : ""`.
  Without the guard a 10 000-row page would call the renderer 10 000 times per
  projection for paragraphs that have no formula in them. The cost is measured,
  not assumed: ≈0.61 µs per formula (`to_unicode` over five representative
  sources, release build, `core::math::tests::cost_per_formula`, `#[ignore]`d
  because it prints), i.e. ≈6 ms per projection for a page whose every line
  holds one inline formula — arithmetic on that measurement, not a frame this
  build has been observed to miss, and no whole-page projection is on record. Inline runs pay it at *projection* time for the same
  reason: a binding would pay it per repaint instead.
- **An empty formula still has to look like one.** A `math` block with no text
  renders `$$`, so the row has height and reads as a formula slot rather than a
  blank band; the block stays in `editing`'s editable set, so clicking it opens
  the source in the live TextEdit and the rendered Text hides itself.
- **The `$` guard is a pair, and both halves are the same predicate.** Import
  opens a span only TeX's flanking rule allows — a `$` followed by a non-space,
  closed by a `$` preceded by a non-space — so "costs $5 and $10" stays prose.
  Export escapes a `$` only when a pair could really re-form on the way back
  (`dollar_pair_ahead`), so prose dollars survive without a `\$` on every price.
  A `$$ … $$` fence is verbatim the way a code fence is, because `\alpha` must
  arrive with one backslash.
- **A formula span is the outermost thing on its range.** `kind_order` /
  `mark_order` put Math last (5), and export drops any mark a Math span
  contains: `$**x**$` has no reading in a renderer that does not parse markup
  inside a formula. A space-padded Math mark has no Markdown spelling either, so
  it is dropped rather than exported as a fence that would not re-open.
- **The new scene caught a real defect, and the pixel evidence is the
  baseline.** `math-inline` is a short marked line — the first in the sweep —
  and it came back with ~155 px gaps between three runs. Cause: the run
  `HorizontalLayout`s default to `alignment: stretch`, so leftover frame width
  was divided among the runs; the 44 existing scenes never showed it because
  every one of their marked lines overflows its frame and gets clipped
  (that clipping is the separate, already-documented platform wall in
  `docs/EDITOR_ARCHITECTURE.md` §"Platform wall"). `alignment: start` on all
  three run rows (block, table cell, column line) moved **0 of the 44 baseline
  scenes**, which is the control that says the fix only touches lines that had
  slack to waste.
- **The gate ran with a same-session control build, which 批次 B owed.** Both
  arms measured in one sitting, alternating, on their own pinned databases
  (raw rows `benchmarks/results/2026-09-21-m10-math-ram.jsonl`): control =
  `2e9de99` from a clean worktree (md5 `57eefe68…`, 21 873 152 B), math = this
  tree (md5 `5bfeceea…`, 21 929 472 B). Scene D steady state: control 135.7 /
  135.8 MB WS and 109.8 / 110.7 private, math 136.6 / 136.7 and 111.7 / 112.4 —
  **1.016× the control's private bytes**, inside the ≤1.2× gate. The +1.8 MB is
  on a page containing no formula at all, and the within-arm spread is 0.9 MB,
  so it is real but small and unattributed (the exe grew 56 KB; the rest is
  assumed to be the symbol tables and the extra model arm). Each arm's first run
  (143.3 / 143.7) is the seeding pass and is excluded.
- **What it does not do** (boundaries, not defects): no display-style layout, so
  `\frac{a}{b}` is one-line `a/b` and `\int_0^1` is a glyph plus Unicode
  sub/superscripts where the font has them, else `^(…)`; `\begin{pmatrix}`
  echoes verbatim; no KaTeX parity; no math font (the row uses the UI face); no
  `\( … \)` or `\[ … \]` delimiters; the `file`/`image` style of per-block
  affordances is absent — a formula has no menu of its own beyond ⋮.
- **Still needs a human window.** No headless scene shows a math block *being
  edited* (source in the live TextEdit) or Ctrl+M over a selection, because
  `quire-shot` never focuses a row. The two new scenes prove the derived
  rendering; they do not prove the editing path.

## ADR-0037 · Orphaned attachments get one reclamation path, and it cannot outrun undo

Decision: `reclaim_attachments()` in `AppState`, reachable as a **Reclaim**
button on the settings dialog's STORAGE row, deletes every `attachments` row
nothing can reach any more and, once the row is gone, the files beside it. The
reachable set is deliberately wider than "what is on screen": every block of
every page in `Document` (not just the open one), every attachment id named
inside any `Entry` on any page's undo **or** redo stack, and the block sitting in
the internal clipboard. It is the only emitter of the new
`Change::AttachmentDeleted`, and it reports through the same `db-notice` toast as
"Back up". The ordering rules are the substance: flush the debounced queue first,
write the row deletions synchronously, then delete bytes.

Why: orphans have three producers and no consumer. Undo removes a *reference*
and never the file, which `Change::AttachmentAdded` has said out loud since
ADR-0029 ("an orphaned picture is recoverable, a deleted one is not");
`delete_page` is not an undo step, so its pictures are unreachable the moment
the page goes; and `replace_all` leaves the whole table alone because it cannot
see the incoming references — the comment there called the cost "orphans, which
no user action can see". A local-first app with no cloud quota can live with
that for a while, but not forever, and the file it cannot see is the one it
cannot explain. The scan errs generous because the two failure directions are
not symmetric: leaving an orphan costs disk, deleting a picture Ctrl+Z was about
to restore costs the user's bytes.

Consequences:
- **The undo contract is unchanged, and now has to be proved.** A step still on
  the stack protects its ids from the sweep, in both directions — redo is one
  keystroke from putting a block back exactly like undo is. `History` grew the
  enumeration API it never had (`referenced_attachments()`), because until now
  nothing outside the module needed to know what the stacks hold.
- **The cap is the boundary of the promise.** 100 steps per page is what a
  picture is protected for, not "forever". `both_stacks_protect_and_the_cap_ends_
  the_protection` pins that boundary, and the settings row says in one line what
  the button deletes, because a toast is not a warning the user reads first.
- **One reader for "does this change name an attachment".**
  `core::persistence::attachment_ids_in` matches the arms that carry an id
  (`BlockInserted`, `BlockAttachmentSet`, `AttachmentAdded`) and nothing else, so
  the undo-shape tests and the reclaim cannot disagree about which arms count.
  `BlockDeleted` is not one of them: it is the change that *drops* a reference,
  and its undo carries the `BlockInserted` that names the id again.
- **Rows before bytes, queue before either.** Applying a DELETE out of order
  ahead of a still-queued `AttachmentAdded` would re-create the row after the
  sweep deleted the files it points at — a permanent dangling reference. So the
  reclaim flushes first and refuses to sweep if that write fails. A failure in
  the other direction only leaves bytes no row claims, which the next sweep
  removes.
- **What it does not reclaim: a file with no row.** A directory sweep looks
  tempting and is the one way to lose a library: if `load_attachments` failed
  this session, the in-memory book is empty, every file on disk looks
  unreferenced, and "I cannot see the references" would be answered by deleting
  them. The reclaim therefore reads the book and never the folder, so a broken
  load removes nothing. Unrowed bytes stay a known limitation.
- The decode cache gives its weight back for a deleted row (`evict_image`), so
  the 32 MiB ceiling ADR-0036 measured keeps accounting for rasters that can
  still be asked for.
- A reclaimed id can be **minted again** after a restart: `next_attachment_id`
  is `max(row ids)+1` at load, so reclaiming the highest row lets the next
  session reuse that number and its `<id>.png` name. Inside one session it
  cannot happen, and the session's own cache entry is evicted, so no stale
  raster survives the reuse.
- **It runs on the UI thread, so its cost is a frozen window and it was
  measured.** 1 000 orphans sweep in 549.9 / 556.7 / 567.6 ms (raw rows
  `benchmarks/results/2026-09-21-m10-reclaim-timing.jsonl`; an independent
  earlier batch read 530.6 / 542.3 / 573.0), and the same call with an empty
  book reads 0.0 ms — so the reachability scan is not on the clock and ≈0.55 ms
  per orphan is the `DELETE` transaction plus the `remove_file` calls,
  unattributed between them. That is a fraction of a second to a couple of
  seconds for a library a user actually accumulates, which is why there is no
  progress UI: the notice bar is the feedback, and a second click on a sweep
  that already ran deletes nothing new. Numbers in `docs/PERFORMANCE.md`.
- **The sweep cannot show the new button, and showed the row's other half
  anyway.** `storage-available` is false for the headless shot tool (it builds
  `AppState::new(&args, None)`), so no scene renders Reclaim — the settings
  dialog with a real database still needs a human window. What the re-sweep did
  catch is that a `visible: false` item keeps its slot in a Slint layout: the
  two hidden buttons were already costing the label width (it elided two words
  early) and the third made it worse, while the hidden caption below them cost a
  band of empty height. Both are visible in sweep17's `settings.png`; the row's
  actions are conditional children now, and sweep18 is the baseline.

## ADR-0036 · A picture page is measured by scrolling it, and the scroll had the wrong sign

Decision: give the media benchmark a real scene instead of another deferred
bullet. `--pictures N` turns every `rows/N`-th row of the bench page into an
`image` block; the fixtures behind them are `create_fixture`-generated 1280×720
gradient PNGs, written **only when the library is fresh**, so the measured passes
load a page that already exists rather than re-encoding one. `bench_picture_plan`
is the single source of the stride and the 200-file pool, which is why the row
builder and the seeder cannot drift apart. The app reports its own decode-cache
state — `attachment_cache_report()` prints one JSON line to stderr, and `--dump-state`
is what makes the bench harness read it — because a process working-set counter
cannot see a cache the size of a raster. `docs/PERFORMANCE.md` carries the nine
scenes.

Why: four consecutive batches ended with the same sentence — *a page of pictures
being scrolled is unmeasured, and the 32 MiB ceiling is a construction bound plus
a unit test, not a reading* — and a unit test is exactly the wrong instrument for
a claim about a ListView, a frame clock and a GPU texture upload. The seeding rule
is the honest half of the design: if the fixtures were built inside the timed
window, the arm would measure PNG encoding. The pool is capped at 200 rather than
one file per picture row because 5 000 rasters of distinct 1280×720 PNGs would be
a disk and warm-up artefact, not a memory measurement — and the cache cannot tell
the difference anyway, since it bounds bytes, not identities.

Consequences:
- **Scene F had been scrolling nowhere since M2.** `ListView`'s `viewport-y` is
  negative downward; the timer did `cur + 8.0`, so every 16 ms tick wrote a value
  the clamp immediately rounded back to 0, and the "continuous scroll" arm measured
  a stationary page. This is not a reading to reinterpret — it is every scroll
  number in this document taken before today. The fix is `cur - step`, plus a stall
  counter that only wraps when a position the app actually *put* there is the one
  that comes back.
- **A null reading was the bug, and pixels were the only proof.** The batch's first
  two runs printed an unchanged cache and the same CPU as the static scene. Rather
  than conclude "images are cheap", `--scroll-y` was added to `quire-shot` as a
  pixel control: at `0` and `+2000` it wrote **byte-identical PNGs**, and only
  `−2000` differed. That is the harness defect, photographed. It also means the
  contaminated rows were deleted and the whole batch re-run on the fixed binary;
  the pre-fix file is kept out of the repo (`%TEMP%\media-ram-pre-fix.jsonl`) as
  the record that the instrument, not the app, was wrong.
- **Scroll tests now need a direction test.** "The property changed" is not
  evidence of motion. `--scroll-step` exists so a flick can be measured separately
  from a wheel tick (83–88 % vs 36–38 % of a core), and `--scroll-y` stays as the
  control any future scroll arm should run before quoting its CPU.
- **The readings, briefly.** The cache stops at 9 rasters / 33 177 600 of
  33 554 432 bytes (98.9 %) in every scrolled media arm, at 500 or 5 000 pictures
  and on 1 000 or 10 000 rows — the ceiling is reached before the viewport could
  show a tenth frame, so it needs no raising. 500 pictures on a page you never
  scroll to is indistinguishable from no pictures. And an on-screen picture costs
  ≈9 MB of process memory, not the 3.7 MB of its raster: **the viewport, not the
  32 MiB cache, is what constrains a photo page**, which is the number §二十二's
  arithmetic now has to carry.
- `MUST NOT` the media arms into the §三十七 ≤1.2× gate. They are reported outside
  it deliberately — that gate's job is to catch a per-row projection regression on
  an ordinary page, and a scene that adds GPU textures would fail it for the wrong
  reason. ADOPT the same-session control for the batch's own arms (A, D and the
  media shapes all ran in one sitting), which is what the layout batch asked for.
- **Invalidating a number obliges re-running it, in both arms.** `scroll_ab.ps1`
  measures the fixed scene against femtovg and skia/GL in one sitting, each
  binary pre-flighted against its own reported renderer so a skia build that kept
  the femtovg default cannot pass for an A/B. The result: the verdict's direction
  survives — skia is ≈18 % cheaper at a wheel tick and ≈21 % at a flick — but the
  size halves (the M7 row read 18.2 % vs 27.7 % on a page that was not moving),
  and the price is ≈+34…43 MB of working set while skia's *private* bytes are a
  wash or lower. femtovg therefore stays the default on the same grounds as
  before, with "skia for scroll-heavy use" now a proven and smaller offer. The
  media ceiling turned out to be renderer-independent — skia reports the same 9
  rasters and the same 33 177 600 peak — because the LRU lives in `AppState`,
  above whichever renderer is painting.
- **A filter that matches nothing is not a measurement, and it used to exit 0.**
  Reproducing the new matrix scenes through `-Only A,B` found that
  `powershell -File` hands a comma list to a `[string[]]` parameter as **one
  string**, so a multi-token filter matched no scene, wrote no rows and reported
  success — the silent-probe shape this harness has been bitten by before.
  `bench_matrix.ps1` now splits the tokens and throws when nothing matched. The
  header comment has advertised the two-token form since scene E's follow-ups, so
  any older batch that claims a `-Only A,B` re-run is worth checking for rows
  before its numbers get quoted.
- `create_fixture` writes a gradient, which compresses well and is therefore not a
  photo; the RAM arms above are unaffected (weight is `w·h·4` regardless) but the
  disk footprint and any future decode-time arm are optimistic. Recorded as a
  limitation, not fixed here.

## ADR-0035 · A screenshot decodes in a file that has never seen the clipboard

Decision: 批次 A's last open code item — paste a picture from the clipboard with
Ctrl+V (SPEC §三十七, §二十七's "clipboard rich content") — is split into three
parts that can each be tested without the others. `platform::read_clipboard_image()`
touches Win32 only: it picks `CF_DIBV5` over `CF_DIB` with
`IsClipboardFormatAvailable`, retries `OpenClipboard` five times like the text
reader does, and copies `GlobalSize` bytes out. `platform/dib.rs` turns those
bytes into PNG and contains no Win32 at all. `AppState::paste_image()` decides
which block receives the picture. In `on_rich_paste` **text outranks image**: the
picture branch runs only when `read_clipboard()` returned nothing. No clipboard
crate, no new `image` feature — the PNG encoder the paste needs was already
compiled in for the store.

Why: ADR-0025 settled that clipboard access is hand-declared FFI, and this is
that decision's second half rather than a revision of it. The split is what makes
the risky half testable: a DIB is a header plus a bottom-up pixel array whose
masks, palette, row padding and bit widths are all chosen by whatever process
wrote the clipboard, and every one of those is a decoder bug — while none of it
needs a clipboard, so eight asserting tests build DIBs by hand and run on any
machine without touching the user's real one (the standing rule here is no
scripted desktop UI: no synthetic key events, no foreground stealing).
`CF_DIBV5` is
tried first because it is the only one of the two that can carry a real alpha
channel; `CF_DIB` is what everything actually writes.

Consequences:
- **The zeroed-alpha trap.** A 32-bit `BI_RGB` screenshot has no alpha, so its
  fourth byte is whatever the source had — usually all zero — and PNG *keeps*
  transparency: pasted through unchanged it renders an invisible picture, which
  the user reads as a failed paste. `decode` therefore paints the raster opaque
  when an alpha mask is declared and *every* alpha byte is zero, and leaves a
  picture with one non-zero alpha byte alone. Two tests pin both sides.
- **A hostile header stops being our problem.** `parse` accepts header sizes
  40/56/108/124, bit counts 1/4/8/16/24/32, and only `BI_RGB`/`BI_BITFIELDS`;
  it refuses `width > 16 384`, `height > 16 384` or `width × height > 64 M`, and
  checks the pixel array is actually there before allocating it. Without those,
  one buggy or malicious clipboard write — a header claiming 30 000×30 000 —
  would be an out-of-memory exit on the paste path, i.e. §二十二's low-RAM
  promise broken by a Ctrl+V.
- **Text wins because pasting a paragraph as a picture of itself loses the
  paragraph.** A "Copy" from a rich app puts both formats on the clipboard, so
  the order is a behaviour decision, not an accident of implementation.
- The paste reuses `AttachmentStore::import_bytes`, which is how a picked file is
  stored, so a screenshot gets the same `MAX_EDGE = 1280` downscaled display
  copy, the same content-sniffed extension and MIME (always `.png` here), and the
  same undo contract (ADR-0029: undo drops the reference, never the bytes). It
  stores under the name "Pasted image". The cost of the reuse is two decodes for
  one picture — DIB→RGBA→PNG, then the store's PNG decode — timed in
  `docs/PERFORMANCE.md` rather than assumed: 19 ms / 74 ms for the first leg at
  1080p / 4K, and 59 ms / 196 ms for the store leg at the same sizes with
  deliberately incompressible pixels. Both ends of that range are labelled as a
  floor and a ceiling rather than passed off as the real number.
- Where the picture lands follows the caret, so a paste never leaves a stray
  empty line: an empty block *becomes* the image, a block with text gets the
  image below it — the same shape the "/" and "+" doors have.
- Windows-only, like `read_clipboard`: other targets return `None`.
- Evidence: 8 decoder tests on hand-built DIBs (bottom-up flip, row padding,
  zeroed and real alpha, 5-6-5 vs 5-5-5 masks, palette with 2 and with 256
  entries, 1-bit MSB-first unpacking, truncated and RLE payloads refused), one
  state test with a real SQLite library in a scratch folder (block kind,
  attachment name/MIME/size, bytes on disk under `attachments/`, text preserved
  below, one paste = one undo step, undo keeps the bytes), and one `#[ignore]`d
  test that reads the *user's* clipboard — the only proof the FFI sees bytes
  another process wrote; on this machine it printed `clipboard picture: 1280x800
  from 134168 PNG bytes`. A green sweep proves none of this: the slice changes
  zero pixels (`.scratch/sweep15` vs `.scratch/sweep14`: 0 of 44 moved).

## ADR-0033 · An empty page writes its own first block

Decision: the M8 A4 item filed as "the empty state still says the block editor
arrives in a later milestone" was a functional dead end, not stale copy, and it
is closed with a new command. `Command::AppendBlock { kind, text }` puts one
block at the end of the page with no anchor; `AppState::start_page()` calls it
with an empty paragraph and hands back the new id so the controller can focus
it. Two doors use it: the empty panel is now a `TouchArea` (`empty-page-started`),
and committing the page title on a page with zero rows starts the first block
under it. Both are one undo step, and `plan()` refuses a container kind, the
same rule `InsertBlockAfter` carries.

Why: every existing insert command takes an anchor `BlockId` — after a block,
into a table cell, into a column box — and a page with no blocks has nothing to
anchor to, so `create_page` produced a page that could not be typed into at all
(`open_page` seeds nothing either). The alternative fixes were worse: seeding a
paragraph at `create_page` would litter the library with empty blocks on every
cancelled "New page", and doing it at `open_page` would make the empty state
unreachable and break the workspace test that asserts a fresh page projects zero
rows. Making the page produce its own first row on the user's explicit
interrupt keeps the document honest — a page is empty until someone means to
write in it.

Consequences: the empty state's copy is now an instruction ("Click here to
write, or press Enter in the title above.") and the panel must stay a click
target, so it cannot be turned back into a passive placeholder. Chasing that
one string surfaced a second stale promise of the same family in the demo
pages' placeholder paragraph — "changes live in memory for now; SQLite
persistence lands with the next milestone", false since M3 — now rewritten as
what actually happens; it is fixture text in the memory-only session the shot
harness runs, so it moves no scene pixels and no sweep can confirm it.
`AppendBlock` is deliberately
page-scoped and always appends — it is not a general "insert at position" and
does not become one; the 10 000-block page's "+" and Enter paths still go
through the anchored commands. Verified headlessly: 3 command tests (empty page
takes one paragraph and undo empties it again, an append lands below a
container's whole subtree, a grid or a layout is refused), the workspace test
now asserts `start_page()` yields exactly one row with the returned id, and the
click is proven on pixels — `quire-shot --scene empty --click 640,458` renders
the title plus one focused empty row with a caret where the panel used to be.
The sweep's `empty` scene moved 724 sampled pixels, all inside x 506..1032 /
y 456..466, i.e. only the copy line.

## ADR-0034 · Palette ids resolve through a Rust function the tests can walk

Decision: the command palette's integer id space is now decoded by
`palette_action(id) -> PaletteAction` in `src/app/state.rs`, and the
controller's dispatch is a `match` over that enum **with no wildcard arm**. The
`OpenPage(i32)` case carries the page id; an id that is neither a declared
command nor ≥ `CMD_PAGE_BASE` maps to `PaletteAction::None`. Two tests guard it:
one walks `mock_commands()` and asserts every row resolves to its own distinct
action (and no row to `None`), the other pins the boundaries (0, 14, 9 999, −1).

Why: the palette rows arrive in Rust as a bare `i32`, and the dispatch used to
be `match id { 1 => …, 2 => … }` with the named constants written as bare
identifiers. A constant that is not imported silently becomes a *catch-all
binding* in pattern position, so `aaa3763` shipped with every id ≥ 9 running
`copy_current_page_markdown` — no page could be opened from the palette — while
`cargo test` stayed green because nothing walked the dispatch. rustc warned
(unreachable pattern) and the warning was the only correct signal in the
pipeline. The repair ROADMAP.md:81 named as "still not written" is a test over
the registry, and a test can only exist if the mapping is a function: an enum
arm that cannot be shadowed by a missing `use`, and a match the compiler forces
to stay exhaustive when a variant is added.

Consequences: adding a palette command is now three edits in one file (a
constant, a `PaletteAction` variant, an arm) plus the action's body in the
controller, and forgetting the middle one is a compile error rather than a
silent hijack. The id constants stay in `state.rs` because that is where
`mock_commands` builds them. This closes the class for the palette only — any
other callback that crosses the Slint boundary as a bare `i32` has the same
exposure, so a future one should ride an enum the same way. Proven by the
registry test rather than by pixels: this fix changes 7 sampled pixels in the
`palette` scene (the sidebar hint now reads `Ctrl+\`), which is exactly the
evidence that a green visual sweep says nothing about behaviour.

## ADR-0032 · A layout is two levels of ordinary blocks, and it draws itself inside its own row

Decision: `columns` (SPEC §三十七 批次 B, M10's fifth slice) stores a layout as
**two levels of child blocks** and adds **no schema**. The `columns` block carries
its box count in the very same `blocks.columns` column v8 introduced for a grid;
each box is a `BlockKind::Column` child of it; each line inside a box is an
ordinary child of that box. One `Change` still carries the shape
(`BlockColumnsSet`) and every other edit is `BlockInserted` / `BlockDeleted` /
`BlockMoved` / `BlockTextSet` / `BlockKindSet`, so undo, redo and §十八 keep
working without knowing what a layout is. The UI projects the whole layout into
the layout's **single** delegate as a flat `[ColumnItem]` plus a `[ColumnBox]`
shape table (`id`, `column`, `first`, `size`) that slices it, and
`visible_block_indices` hides both the boxes and their lines — the same filter
ADR-0028 uses for a folded subtree and ADR-0031 for cells, so a layout is one
row however wide it is. Tiling is Slint 1.18's `FlexboxLayout` (one row,
`flex-wrap: no-wrap`, `alignment: stretch`, `horizontal-stretch: 1` per box), not
hand-written widths. The hover strip reuses `TableEdge` for Add / Delete column,
and `plan()` refuses below two and above three boxes.

Why: Slint has no recursive components, so a box cannot hold an `EditorBlock`
that itself holds boxes — the only place a layout's content can be drawn is the
layout's own row delegate, which is precisely what §三十七's "分栏只在可见窗口内
展开" asks for and what §十二's virtualization premise requires. Reusing v8's
integer is the same argument ADR-0030 made about v7: a box count and a column
count are one fact ("how many boxes does this container hold"), and a schema
version for a rename is not worth a migration. A box always holds at least one
line, and `ColumnsAddBlock` exists because an empty box is the one thing on the
page a click cannot put a caret in: the empty box says so, and the click asks for
its first paragraph.

Consequences: a box has no row of its own, so nothing outside the delegate can
address its lines by row index — Tab walks them through `column-item-move` and
stops dead at either end, ↑/↓ move the caret inside the line rather than leaving
the layout, and leaving a box is a click. Markdown export flattens: the
containers write no marker of their own and their lines come out at page depth in
reading order, which a test pins in both directions (re-import keeps every word
and loses only the shape), and the importer learns no columns syntax — the same
degradation ADR-0031 accepted for a grid. `InsertBlockAfter` had to stop treating
a container's row as its own slot: it now inserts after the container's whole
subtree and inherits the anchor's parent, which closes the identical hole for a
grid — "+" on a table used to be able to wedge a top-level block between the
table and its first cell, and a page's blocks are one flat slice sorted by
`order`, so a container whose subtree is not contiguous stops being one
container: `subtree()` walks off it, and every index-based reader with it. Two QOML findings came out of this slice and both
belong in UI_ARCHITECTURE.md: `for … : if … : Component` is a parse error, so a
filtered repetition has to become a shape table the delegate slices
(`ColumnBox.first` / `size`), and an identifier on the right-hand side of a
property inside a component instantiation resolves in the **enclosing**
component, so a child cannot be handed its own parent's `root.x` — the layout
passes `line-y`, `line-x` and `each-width` by plain names. The pointer-claim
protocol ADR-0031 needed for a grid crosses a component boundary here as
`in property pointer` plus `callback pointer-claimed(int)`, because N two-way
bindings onto one parent property is not something Slint lets you write. And the
"/" popup anchored itself with a hardcoded 350 px window reserve, which this
slice is the first to outgrow — the anchor now measures the real list height the
way the "+" menu already did. Deliberately not built: per-box width control
(Notion's resize handle), more than three boxes, and a layout nested inside a
box.

## ADR-0031 · A table is a block that owns its cells, and the grid is one editor row

Decision: `table` (SPEC §三十七 批次 B, M10's fourth slice) stores the grid as
**child blocks**, not as a payload. `BlockKind::Table` gains one stored number —
`blocks.columns INTEGER NOT NULL DEFAULT 0`, schema **v8** — and each cell is a
`BlockKind::TableCell` child of it, kept in row-major order by its ordinary
`order` key. Row count is therefore *derived* (`cells / columns`), never stored,
so adding a row is inserting `columns` blocks and nothing else moves. One new
`Change` carries the shape (`BlockColumnsSet`); every other table edit is plain
`BlockInserted` / `BlockDeleted` / `BlockTextSet` / `BlockKindSet`, which means
undo, redo and the §十八 storage contract all keep working without knowing
what a table is. The UI projects the whole grid
into the table's single delegate as a flat `[TableCell]` + `columns`, and
`visible_block_indices` filters cells out the same way ADR-0028 filters a folded
subtree — a table is one row, always, however big it gets.
Why: the alternatives are a JSON blob column (a cell would stop being a block,
so inline marks, undo granularity and §三十九's later relation/rollup work all
have to be reinvented outside the model) or a real table-per-row schema (§三十九
owns that, and SPEC is explicit that this kind is *not* a database view). Cells
as blocks cost one integer column and reuse everything. The two rules ADR-0028
added for dynamically-rowed kinds apply here and are honoured: the projection
really drops the hidden rows, and every row-index consumer goes through
`visible_block_indices`. A grid whose cell count is not a multiple of `columns`
is refused by `grid()` rather than repaired — `plan()` says no instead of
guessing which row is short, and `DuplicateBlock` will not copy such a table at
all, because a cell-less grid opens broken.
Consequences: cells never appear in the row list, so no code that reads
`rows[i]` can be handed a cell — `SetBlockType` refuses a cell as both source
and target, and a cell's kind belongs to its grid, not to the Turn-into menu.
Turning a block *into* a table moves its whole line into the top-left cell of a
fresh 3×2 grid rather than dropping it, because a table block draws no text of
its own; flattening it back turns each cell into a paragraph, so the words
survive both directions — the *marks* do not, since `new_cell` starts clean and
a mark range pointing at the old block's text would mean nothing inside one
cell. Adding a column is the one command that must insert
`rows` blocks into `rows` different gaps at once, which is why
`OrderKey::STRIDE` is 1<<16 and why `renumber_page` now spreads with it: one
midpoint per insert halves a gap, so a batch has to be keyed whole. The
projection reads a table's cells by scanning the page, so it is guarded on
`kind == Table` — unguarded it ran that scan for all 10 000 rows of a bench page
and allocated an empty model with each, which measured ≈2 MB of working set on
D and nothing on the 24-row A (`docs/PERFORMANCE.md`). Markdown
export writes GitHub-flavoured tables (first row is the header, `|` escapes to
`\|`, a newline in a cell becomes a space); **import does not read them** — a
`|a|b|` line stays a paragraph, pinned by a test in `markdown_test.rs`, because
the importer's shape is line-at-a-time and a table needs lookahead. Deliberately
not built: Enter splits a cell, backspace merges, Up/Down cross rows, Ctrl+L
marks a link in a cell, and a cell's text width does not feed back into column
widths.
Hover-only UI needed a new shot capability: `quire-shot --hover x,y` dispatches
`PointerMoved` without a button, which is the only headless way to light the
edge toolbar; `table` and `table-edit` are in the sweep but the 22 px strip they
show is verified by a manual `--hover` shot, and hovering a table reflows the
content below it by exactly that much. Two Slint 1.18 geometry findings came out
of this slice and both belong in UI_ARCHITECTURE.md: an element in a non-layout
parent that binds `height` but leaves `y` unbound is rendered **centred** in that
parent, so `DocumentRow.head` had been sitting 34 px below its binding in every
scene ever shipped and the table — the first tall *first* block — finally painted
over the page title; the fix is explicit `x: 0px; y: 0px;`. And `absolute-position`
readings inside a ListView delegate are not trustworthy (row 1 reported its head
below its own body with both at `y: 0`), so the delegate's real geometry is
`parent.y + head.height`, which is what `row-y` already passes down. Verified
headlessly: `cargo test --features software` green, and the 37-of-42 changed
scenes in the new baseline all differ **only** inside the title band, which is
the fix above and not the table; the projection guard was re-swept afterwards
and came back 42-of-42 byte-identical, so it is a memory change and not a
visual one.

## ADR-0030 · A file attachment is stored without ever being looked at

Decision: `file` (SPEC §三十七 批次 A, M10's third slice) adds **no schema**. A
file is a block kind, and the v7 columns ADR-0029 introduced for pictures —
the `attachments` row plus `blocks.attachment` — describe it exactly as well,
so the slice ships with `user_version` unchanged. What is new is that the bytes
never enter the process: `AttachmentStore::import_any_file` `fs::copy`s the
picked file straight to `<library>/attachments/<id>.<ext>` and records its
length, with no decode, no `image` call and deliberately **no size cap** — a
2 GB attachment is stored in 2 GB of kernel-side copying and 0 of working set.
The row paints three things from the database line alone: the name, a
`format_size` label, and two buttons. Open goes to `platform::open_with_default`
(a hand-declared `ShellExecuteW`); Save-as goes to `rfd` + `export_to`, which
copies the stored bytes out under `save_name`. Undo drops the reference and
never the bytes, unchanged from ADR-0029.
Why: the priority list in ROADMAP.md puts low RAM above feature count, and the
§二十二 promise is the one this kind would break first — the obvious
implementation, read the file into a `Vec` and write it back, is a 2 GB working
set for a note. The picture path solves the same problem by downsampling; the
file path solves it by never looking. `ShellExecuteW` rather than
`spawn("explorer.exe", path)` because explorer returns 0x1 **on success** by
design, so a subprocess can never tell a launched app from a blocked file
association, while the FFI answers `> 32` only for a real launch — and one
extern declaration keeps the `windows` crate out, the same rule ADR-0025
follows for the clipboard.
Consequences: the label keeps its extension while a picture's drops it, because
for a picture the content is the truth and for a `.zip` the name is; that split
is why `save_name` exists (it rejoins stem + extension for pictures, and leaves
a file name alone — case-insensitively, since `file` lowercases the extension
and `name` keeps the user's). Open is an explicit button rather than
click-the-row, because a row that launches an arbitrary executable on a stray
click is not a row you can safely select text in; the two buttons stay drawn
rather than hover-revealed, since a file block that shows nothing to press
reads as a broken attachment. The size is a `pure callback attachment-size(int)`
rather than a `BlockRow` field so the string is not rebuilt for every row on
every keystroke — the same viewport-not-document argument ADR-0029 makes for
the rasters. Markdown export writes `[name](quire://attachment/<id>)`, a
*link* rather than the picture's `![](...)`, and that is the one place the two
kinds differ in the exporter: the importer has no picture shape but it does have
a link shape, so a file's reference survives an export/import round trip and a
picture's does not. Still open here when this was written: orphaned bytes (no
FK, no cascade, same gap as pictures — ADR-0037 gives it one reclamation
path), clipboard-bitmap paste (ADR-0035), and the PDF first-page thumbnail,
which the user deferred on 2026-09-20 — until it lands a PDF and a `.zip` look
identical apart from their names. Verified headlessly, not by eye: 36 of 40
scenes byte-identical against the previous sweep, `slash`/`plus`/`dark-slash`
moved only because the menus gained the File row, and `file` is new — a
760×48 px box at x 390..1149 with the label left, "1.8 MiB" right-aligned, and
two 26 px buttons at x 1086..1112 / 1116..1142. The fixture's first draft put
`std::process::id()` in the file name, which made the label different every
run; the id moved to the temp *folder* so the scene is reproducible.

## ADR-0029 · Pictures are files beside the database, and the editor never loads the original raster

Decision: `image` (SPEC §三十七 批次 A, M10's second slice) stores its bytes in
`<library>/attachments/<id>.<ext>` and keeps only a reference in SQLite — a new
`attachments` table plus `blocks.attachment INTEGER` (schema v7). The column
carries **no foreign key on purpose**: a block whose file row has vanished must
still load and render as a missing picture, because a user who copies the `.db`
without its folder owns a library, not a crash. Import downsamples anything
longer than `MAX_EDGE = 1280` px into a second file, `<id>.cache.png`, and
`AttachmentStore::display_path` hands the UI only that copy — the original
stays byte-identical on disk, so viewing never degrades the user's file. The
decoded rasters live in a cache owned by `AppState`, keyed by attachment id and
capped at 32 MiB weighted by RGBA bytes, evicting the least-recently-realized
entry first. Rows reach it through the `image-for` / `image-aspect` **callbacks**
rather than model fields, because Slint invokes a binding only for the rows it
realizes. Undo drops the reference and never the bytes: `Change::AttachmentAdded`
is an upsert and there is deliberately no `AttachmentDeleted`.
Why: Slint's own decode cache is a thread-local `CLruCache` capped at 5 MiB
weighted by decoded bytes and keyed by path + mtime (`i-slint-core-1.18.0`,
`graphics/image/cache.rs`). One 1280×720 RGBA frame is 3.7 MiB, so that budget
is a single photograph — scrolling a page of pictures through it re-decodes on
every frame. The obvious alternative, an uncapped cache of our own, is how a
low-RAM app dies quietly, and §二十二's promise sits on the first slot of the
priority list in ROADMAP.md. Callbacks are what keep the cost proportional to
the viewport instead of to the document: a page with five hundred pictures
holds about ten.
Consequences: the 32 MiB ceiling is ≈eight full-width frames, and it is a
construction bound, tested as one (`the_picture_cache_spends_its_budget_and_
drops_the_stalest_first` also pins LRU over FIFO by re-showing an evicted id
mid-scroll). A failed decode caches as a zero-weight blank so a missing file is
not re-opened per repaint — which also means replacing a file on disk in place
needs a restart to show up, since our key is the id and not the mtime. The
extension and MIME come from sniffing the bytes, so a screenshot saved as
`.jpg` but encoded PNG is stored as the PNG it is. `blocks.img_percent`
(25 / 50 / 100, default 100) is the width tier; setting it back to the default
plans no change, so the menu cannot leave an undo step that does nothing.
Markdown export writes `![name](quire://attachment/<id>)`; the importer has no
picture shape, so the line survives as literal text and the file name is never
lost — the reference goes out, nothing comes back, which is the same asymmetry
§三十七 accepted for toggle folds (ADR-0030 gives `file` the link form, so that
one does round-trip). Of what this kind left open, the clipboard-bitmap paste
has since landed (ADR-0035); the PDF first-page thumbnail reuses this store
unchanged and is what the user deferred on 2026-09-20.

## ADR-0028 · A folded subtree gets zero realized rows, so row indexes stop being model indexes

Decision: `toggle` (SPEC §三十七, M10 批次 B's first slice) persists its fold
as a `blocks.folded` column (schema v6) and hides its subtree by **filtering
it out of the projection** — `project_blocks` builds rows through
`visible_block_indices`, so a hidden block has no `BlockRow`, no delegate and
no height. There is no `visible: false` row. Because that makes the editor's
row numbering independent of the document's, every consumer that treats a row
index as a model index must translate first; the one that exists today (the
§八 drag landing) goes through the new `AppState::drop_index_for_row`, which
shares `visible_block_indices` with the projection.
Why: Slint's `ListView` realizes exactly one `for` child per item, so a
0-height delegate would still cost a component, a binding and a slotmap
lookup per hidden block — on a page whose top section is folded that is the
whole point of folding, paid for and thrown away. Deleting the row is also
what makes the behaviour correct by construction: a hidden block cannot be
tabbed into, dragged, found-by-⌘F or renumbered if it is not in the model.
The cost is that two numbering systems now coexist, and the repo had been
using them interchangeably.
Consequences: fold is undoable view state, not content — `Command::ToggleFold`
emits `Change::BlockFoldedSet`, the same shape `PageExpandedSet` already uses
for the sidebar, so it rides the persistence queue and never bumps a document.
Only a Toggle draws the chevron, so `SetBlockType` away from Toggle clears the
fold (undo restores it) rather than stranding a subtree with no way back;
`SplitBlock` and `DuplicateBlock` force `folded: false`, because neither one
copies a subtree and a fold over nothing is a trap. Markdown export degrades a
toggle to a quote line — CommonMark has no fold syntax, so it degrades exactly
like a callout already did, while the subtree still rides along indented and
import never restores the fold (§三十七 asks for precisely that asymmetry).
And SPEC §三十七's 硬性约束 gained a rule for the next dynamic-row kind
(`table`, `columns`): the projection must really delete rows, and new
row-index consumers must translate. Verified headlessly, not by eye: 32 of 35
baseline scenes are byte-identical after the change, `slash`/`plus`/
`dark-slash` moved only because the menus gained the Toggle row, and
`toggle` vs `toggle-fold` differ by 6 006 px in exactly four places — the
triangle glyph, the vanished child line, the one-row reflow below it, and a
2 px taller scrollbar thumb.

## ADR-0027 · The parked feature set is unscheduled, not cancelled; only six capabilities stay out

Decision: of everything the SPEC had deferred, only sync, real-time
collaboration, cloud, plugin market, AI, publish-to-site and
comments/discussions/reactions remain out of scope (SPEC §三十三, extended
2026-09-20 with the last two). Everything else that §九 and §十七 had
written as "后续再加" is now a scheduled phase with a milestone: §三十七 →
M10/M11 (image, file, PDF, table, toggle, columns, then highlighting,
bookmark, embed, math, TOC), §三十八 → M12 (icon, cover, page font /
full-width / small-text, lock, version history, templates), §四十 → M13
(@-mention, backlinks, synced block), §三十九 → M14 (Database). Ordering
follows "how fast a daily note hits the wall", not Notion's feature
alphabet: media and structure blocks before the database, and the database
only after the reference layer exists, because `relation` and the simple
table would otherwise each get their own ad-hoc version of it.
Why: an audit of SPEC.md against Notion found three different things being
reported as one — items genuinely ruled out (§三十三), items deliberately
deferred with a spec line (§九), and items never written down at all
(page-level properties, backlinks, templates, version history, all views
and property types of the database layer, i.e. the largest single gap).
The third group was the problem: §三十五's "feature count ranks last"
had been read as licence to leave them unwritten, so they could not be
planned, estimated or refused. Recording them costs nothing now and makes
"第一期不做" an explicit statement per item instead of a blanket.
Consequences: §三十五's priority order still binds each of these phases,
so every one carries a measured gate rather than a checkbox — M10 must
keep the 10 000-block scene inside 1.2× the current RAM baseline (image
downsampling is where a low-RAM app normally dies), M14 must do filter and
sort in SQL and never realize 10 000 rows (SPEC §三十九's red lines), and
math/highlighting/embed may not smuggle in a JS, WASM or WebView runtime
that §二 forbids. `person` degrades to a local name list because there is
no account model to point at. New sections were appended as §三十七–§四十
rather than inserted in phase order: existing cross-references (ROADMAP
cites §六/§十六/§三十五, ADR-0026 cites §八) are all by section number, and
renumbering would have silently broken every one of them.

## ADR-0026 · Page and Link-to-page blocks share blocks.page_ref; ownership is a kind-level contract
Decision: both page-bearing block kinds — `Page` (kind 11) and `Link`
(kind 12) — point at a page through the SAME nullable `blocks.page_ref`
column (schema v5) and the same `BlockRefSet` change; no per-kind column or
variant. What differs is **ownership**, decided by the kind:
- `Page` **owns** its child page (created in the same batch as the block).
  Deleting the block deletes the child; duplicating the block deep-copies
  the child and retargets the copy; pasting one lands the title as plain
  text (two blocks must never share an owned page). One `BlockRefSet`
  without a live `PageCreated` is still legal — storage stores the pointer,
  the lifecycle is the caller's composition.
- `Link` **references** an existing page picked in the page picker. Delete,
  duplicate and paste never touch the target; sharing a reference is the
  point. Turning either kind into another kind via the ⋮⋮ menu drops the
  reference (`BlockRefSet` → `None`); a Page's child survives in the tree,
  unowned.
Why: Notion's Page block and Link-to-page differ exactly in lifecycle, not
in data shape — one column keeps schema and export uniform (`[title]
(quire://page/<id>)` for both, the ownership difference deliberately does
not survive Markdown), and the contract stays at one change variant
instead of two. The page picker reuses the slash popup in a
`slash-pick-page` mode rather than a fourth popup, so anchor/keyboard/
close behavior has one implementation.
Consequences: a dangling `page_ref` (child deleted from the sidebar) renders
"(deleted page)" muted, and the block still opens nothing — no cascade from
block to tree outside the explicit delete path. A duplicated page's own
embedded Page blocks still reference the ORIGINAL children (recursive copy
is v2). Keyboard focus-move can land on a page row; the row is
never-editable by kind, so the input simply does not appear.

## ADR-0025 · Clipboard reads are direct Win32 FFI — no clipboard crate, no subprocess
Decision: `platform::read_clipboard` opens the clipboard through
`OpenClipboard` / `GetClipboardData(CF_UNICODETEXT)` / `GlobalLock` declared
in-module (`user32`/`kernel32`, already linked for winit) — writes stay on
`clip.exe` stdin.
Why: rich paste (§二十七) needs the text *any* source app put on the
clipboard. The zero-dependency alternatives both fail: `clip.exe` is
write-only, and a `Get-Clipboard` subprocess measured **7–10 s** on the dev
desktop (PowerShell startup under AV) — no paste can wait for that. A
clipboard crate (arboard) would be the first new runtime dependency since
M3 to buy ~40 lines of FFI the toolchain already links. CF_UNICODETEXT is
the format every text source supplies; UTF-16 → `String` via
`from_utf16_lossy`, and a 5×2 ms retry rides out transient clipboard locks
held by other processes. Writes keep `clip.exe` because the only payloads
the app writes (`quire://` links, ADR-0023) are ASCII, so the
OEM-codepage stdin caveat is moot.
Consequences: reads are microseconds and CJK-safe from any source app;
writing non-ASCII *from the app* would garble through `clip.exe` — the day
a feature needs that ("copy page as markdown"), switch the write path to
the same FFI (`SetClipboardData`) rather than adding a crate.

Update (2026-09-20, same day): the day came with "Copy Page as Markdown" —
the write path now runs through `SetClipboardData`/`GMEM_MOVEABLE` FFI too
(`copy_to_clipboard` swapped its clip.exe internals for the FFI; callers
unchanged, no crate). `clip.exe` is fully retired from the codebase; the
round-trip test (`clipboard_write_and_read_round_trip_unicode`) pins CJK
through write → read.

## ADR-0024 · The release profile stays as shipped — no fat LTO, no panic = abort
Decision: `[profile.release]` keeps thin LTO + `codegen-units = 1` +
`strip = "debuginfo"`. Fat LTO is rejected, `panic = "abort"` is rejected,
`codegen-units = 16` is rejected.
Why: SPEC §二十四 only keeps levers that win on runtime memory / CPU /
startup — exe size ranks last. The 2026-09-20 four-way comparison
(`docs/PERFORMANCE.md` "M8 · release profile audit", Track B A3) measured
idle private bytes 88–90 MB, a 10 000-block page at 97–99 MB, idle CPU
0–0.6 %, typing 24–30 % and warm window-up 85–192 ms across all four
profiles — one noise band. Fat LTO saves 2.5 MB of exe for a ×2.5 build
time; `codegen-units = 16` *adds* 2 MB; `panic = "abort"` is the only
"smaller and faster" option and is vetoed on behavior, not numbers: abort
does not unwind, so `logging::install`'s panic hook never runs,
`panic-report.txt` never lands and `last_session_aborted` is blind exactly
when it matters — the ADR-0018 / SPEC §二十五 crash-recovery chain dies
with it. `strip = "debuginfo"` is the symbol policy: linker debuginfo
stripped, the COFF symbol table kept; "debug artifacts separated" is
`just dist` preserving `target/release` originals, not symbol stripping.
Consequences: none at runtime — this ADR mainly pins what must NOT change.
The conclusion expires when a lever moves idle private bytes or typing CPU
beyond ≈2 MB / ≈3 pp of the measured noise floor; re-run
`benchmarks/scripts/profile_bench.ps1` per profile before believing any
single-run delta (the audit caught a parallel-build-polluted batch that
looked like a 20 % win and was not).

## ADR-0023 · The ⋮⋮ menu gets Notion's remaining items; block color crosses the persistence contract
Decision: the block handle menu carries Copy link to block, Move to, and
Text/Background color (Comment / Suggest edits / Ask AI stay out with the
rest of collab+AI). "Copy link to block" puts `quire://block/<id>` on the
system clipboard via `clip.exe` (ASCII-only payload, so no clipboard crate
— dependency policy holds); `open-link` now resolves `quire://block/` and
`quire://page/` in-app first (jump to the page, focus the block) and only
shells out for foreign URLs, which also makes Ctrl+L links to internal
anchors clickable. "Move to" lists every other page depth-indented and
commits `Command::MoveBlockToPage`: the root lands appended to the target
page's top level, the subtree travels as one `Change::BlockMovedToPage`
per block (children keep their parent pointers and orders), one undo step
end to end. Color is a block-level pair (`ColorKind` text + background,
10 slots, Default = theme), one `SetBlockColor` command, one
`BlockColorSet` change, and schema v4 (two TEXT columns on `blocks`,
'' = default; the ALTER runs conditionally in code because SQLite has no
`ADD COLUMN IF NOT EXISTS` and the migration test legitimately re-runs v4
on a hand-downgraded file). A Callout block joins the kind set — the last
Basic kind that neither a symbol shortcut nor the v1 exclusions cover;
it renders a tinted rounded box with an emoji and exports as a quote.
Why: the user asked for the ⋮⋮ menu to match Notion's, minus what typing
symbols already reaches (ADR-0022 curation unchanged and now spanning the
new kinds).
Consequences: colors are cosmetic by design — no Markdown representation,
so export/import round trips drop them (callouts degrade to quotes);
`clip.exe` means copy-link is a Windows-only nicety until a clipboard
crate earns its place; menu popups size their anchor clamp from the live
row count, so the tall root menu and the 11-row color palettes stay
on-screen; the "current color" check is drawn as an icon because the
software renderer has no font fallback (a ✓ glyph rendered as nothing in
headless captures).

Amended 2026-09-21 · the ten slots and `text-muted` are now measured, not
approximated. WCAG relative luminance over the four light surfaces said the
third text tier sat at 2.51–2.71:1 and the orange and yellow slots at 2.81
and 2.48 on their own tints — the two weakest pairs in the palette, and the
reason the A4 sweep's "dimmest text in a light UI" finding was real. Light
`text-muted` is `#75787d` (4.43 on white, 4.10 on the sidebar), slot 3 is
`#bd6408` (3.61 on its tint, 4.21 on plain), slot 4 is `#a87718` (3.56,
3.95). Dark did not move: its muted tier had always read 3.9–4.4, which is
now the band both themes agree on. Two consequences worth stating: the
hierarchy is capped by that band, not by AA — pushing muted past ~`#71747a`
closes the gap to `text-secondary` (5.91) and there is then nothing left
that reads as a whisper — and the same arithmetic retracted the finding it
was written to check, because the pairs the sweep named by eye as the
weakest (green-on-olive, red-on-black) measure 3.86 and 4.72 and were never
the problem.

## ADR-0022 · Markdown line-shortcuts; the menus list only what symbols can't reach
Decision: typing a trigger at the block start converts the block live —
`# `/`## `/`### ` → Heading 1/2/3, `- `/`* ` → Bullet, `12. ` → Numbered,
`[] `/`[ ] ` → To-do, `[x] ` → checked To-do, `> ` → Quote, `---` →
Divider, ` ``` ` → Code — as one undo step (`exec_all`, which now skips
no-op parts of a compound command instead of failing wholesale). The slash
menu and the ⋮⋮ "Turn into" list carry only Text/Code/Divider: every other
kind has a symbol path. Notion's Comment / Suggest edits / Ask AI / Color /
Copy-link / Move-to stay out (collab+AI are v1 out-of-scope; no block
anchors or cross-page moves yet).
Why: the user wants the pickers short — entries reachable by blind typing
are noise; the symbol is the only path to the removed kinds, which keeps
the menus honest.
Consequences: code/divider blocks are exempt from conversion (their text
legitimately starts with these characters); converting back among symbol
kinds works by typing the other symbol at the block start. Fixed while
wiring: the ⋮⋮ popup was driven by a `block-menu-open` bool only the bench
scene ever set — a real grip click raised just `block-menu-open-id`, so the
menu never appeared and had no coordinates. The popup now follows the id
and the bool is gone; menu geometry anchors beside the handle from
delegate-reported viewport coordinates (DocumentRow layout-y + handle
offset), which also fixes the slash menu anchoring every block at the first
row's offset.

## ADR-0021 · Drag-reorder rides Slint's built-in `DragArea`/`DropArea`
Decision: the grip handle wraps its TouchArea in a 1.18 `DragArea`
(`allow-move`, payload = plain-text `slint-notion/block:<id>` built in the
controller); every editor row is a `DropArea` whose `can-drop` validates the
landing through a read-only `can_move_block_to` check and parks the accent
drop line in `UIState.drop-line-row`; the drop commits one new
`Command::MoveBlockTo` (insert-above flat index), so one undo step and one
persistence entry cover a multi-row move.
Why: SPEC §1564 says to prefer Slint's built-ins over hand-rolling; the
in-window drag path lives in core (press-filter → threshold → DragMove/Drop
routing), so it works with every renderer on Windows without OS DnD, and a
click below the threshold still reaches the menu TouchArea. Reordering on
drop (not per-hover swap) sidesteps the ListView delegate-reuse trap, where
mutating the model mid-drag would swap the data under the dragging delegate.
Consequences: the landing must not split a subtree (top-level inserts may
only sit above another top-level block; nested items stay adjacent to their
sibling run) and nested-swap `MoveBlock` now re-sets the same parent instead
of flattening children to top level (pre-existing bug found while planning).
Dragging across a virtualized viewport does not auto-scroll the ListView
yet; revisit when long-document drag matters.

## ADR-0020 · The library moves to `%APPDATA%\Quire`; `--db` and `--portable`
stay
Decision: `storage::data_location` is the single answer to "where is the
library". Opening the pre-D12 default — `appdata/quire.db`, relative to the
working directory — is redirected to `%APPDATA%\Quire\quire.db`, and the first
such redirect carries the old library across whole: the main file, the
`.bak<N>` family, the `-wal`/`-shm`/`.corrupt` sidecars a crashed session left,
and the `quire.log` family that shares the folder. A caller that names a file
of its own (including `--db <path>`) is taken at its word and never rerouted;
`--portable` asks for the old behavior out loud, which is what a stick install
wants and what the benchmark harness uses to keep out of real notes. Each file
moves as copy → open-the-copy → one rename → delete the source, staged through
a `<destination>.migrating-<pid>` name. The idempotence key is the destination:
once `%APPDATA%\Quire\quire.db` exists there is nothing to migrate, so a restart
— or a move interrupted after the first file — cannot overwrite a library that
has since been edited.
Why: with the installer in place (ADR-0017) the working directory is whatever
the shortcut's "Start in" happens to say, so launching Quire from two folders
creates — and seeds — two unrelated libraries, and the user finds out when a
note is missing. That same relative path is also what forces the per-user
install; once the data has its own home the premise behind ADR-0017's
"the install folder must be writable" disappears. The copy is opened before the
source is retired because a `fs::copy` of a database another session is writing
can be torn, and a damaged library discovered at the new path has no source left
to fall back to; refusing the move keeps the old folder — snapshots included —
exactly where `backup::open_with_recovery` expects it. Falling back to the
legacy path on any error rather than opening the empty per-user file is the
difference between "it did not move" and "my notes vanished".
Consequences: the log follows the database, since `services::logging::data_dir()`
now asks storage — a `--db` run writes its log beside that file, which also
keeps the benchmark runs out of the checkout. Three things this track could not
finish: `main.rs` still creates `appdata/` beside the working directory before
opening it, `LaunchArgs` has no `portable` field so the two flags are re-read
from the command line here (`scan`), and `OpenReport` has no field to say a move
happened, so the user only ever sees one stderr line. All three are in
`M8_FEEDBACK.md` #13 with the lines to write. `%APPDATA%` roams with the
domain profile, which is what the milestone asked for and is fine at this
database size; if a roaming profile ever turns out to be slow for a live SQLite
file, `roaming_root()` is the one function to change.

## ADR-0019 · Backup retention is two windows; the periodic snapshot rides the
flush tick
Decision: `storage::backup` keeps five generations (ADR-0015 had three) *and*
drops any generation whose modified time is older than `MAX_AGE` = 7 days. Both
are applied by one `prune(path, now)` that `snapshot()` calls after a successful
copy and whose return value is discarded: cleanup is housekeeping, and a locked
or vanished old file must not turn a good snapshot into a reported failure.
`prune` scans `KEEP + 2` slots so a family left behind by a larger `KEEP` cannot
survive unboundedly (`recover` only walks `1..=KEEP`, so anything past it is
unreadable weight). Mid-session insurance is `PersistenceService`'s: a snapshot
hook attached with `with_snapshotter(interval_ms, hook)` — or the app's one-liner
`with_database_snapshots(&repo)` — runs from the flush path that is already
ticking (`flush_if_due`, `force_flush`), gated on two facts: the period
(default `DEFAULT_SNAPSHOT_INTERVAL_MS`, ten minutes) elapsed since the last
snapshot, *and* something was written since then. `SqliteRepository` now records
the path it opened and exposes `snapshot()`, so the hook is a method call on the
`Arc` the app already holds; a failure is stored for the UI to take
(`take_snapshot_error`) and logged, never returned as the flush's error.
Why: the milestone forbids a second resident thread, and the app already arms a
timer per recorded burst — a snapshot that rides that tick costs no new
scheduling and, because of the `pending` gate, no work at all in a session that
changed nothing. Count-only retention keeps a five-week-old `.bak5` alive for a
user who opens Quire once a week; age-only keeps five copies of a scene-D
workspace forever, which is the memory ADR-0015 accepted at three generations
and should not silently triple. Age comes from the file's own `modified()`
because the snapshot's whole life is that one write, and `now` is a parameter for
the same reason the debounce window takes a clock: a test ages a file with
`File::set_times` rather than waiting a week. Gating on the write rather than
running on the wall clock is what keeps a 2.3 MB `VACUUM INTO` from repeating
while the user reads.
Consequences: ADR-0015's "deliberate omission" (a long session had no insurance
until the next open) is closed — the loss window is now "since the last tick",
but only for a session that keeps editing, because the app's timer is
single-shot: an idle window takes no snapshots, and ten minutes is a minimum gap
rather than a cadence. Making it exact is an app-side `TimerMode::Repeated`
(M8_FEEDBACK #12 records the wiring line and this reading). A monthly user can
end up holding one generation instead of five — age wins, which is the point of
the second window. `services/persistence.rs` now names `SqliteRepository`
concretely, the same trade ADR-0014 made for `search`, and `snapshot()` takes the
connection mutex, so a snapshot serialises against writes by construction rather
than by a new lock. Retention covers `.bak<N>` only: the `.corrupt` corpse and
D9's log family still have their own rules.

## ADR-0018 · Logging is one rotating file plus a panic report the next start
reads back
Decision: `services::logging` owns the app's only log file, `quire.log` beside
the database, with a family of three (`quire.log`, `.1`, `.2`) and a 1 MB ceiling
per file — size is checked before each append, so a line may overshoot by its own
length and nothing else does. Records are one physical line: `2026-09-19T08:21:55
.169Z [info] …`, UTC, with newlines in a message escaped to the two-character
`\n`. `init()` is the single line `main` calls; it creates the directory, writes
the startup record, and installs a panic hook that chains whatever hook was
already there. On panic the hook appends a `[panic]` line and writes
`panic-report.txt`; the *next* `start()` reads that file, records it as the
`last_session_aborted` entry of `session.meta`, logs the same fact, and deletes
the report so a restart cannot blame the old crash twice. `session.meta` is a
line-oriented key/value file — the file-backed twin of the `metadata` table — and
`meta_entries()` hands it to whoever holds a repository.
Why: the alternatives all cost more than a crash trace is worth. A `log`/`tracing`
facade would add dependencies to a four-crate tree to gain levels nobody tunes,
and an async writer would need the thread this milestone is explicitly avoiding.
Writing the abort *metadata* on the next launch rather than inside the panic is
the same argument in miniature: a panicking process may die at any moment, so it
gets two plain writes while the read-modify-write of a metadata file runs on a
thread that is known to be healthy. UTC rather than local time because a log
timestamp and a file's modified time must be comparable when the two disagree
about nothing else. The one line in `main.rs` is the ceiling the milestone allows,
which is also why the logger cannot reach the database: it starts before the
repository exists, and `src/app/**` is off-limits (M8_FEEDBACK #9 records the
wiring Track A still owes).
Consequences: a hard crash (access violation, kill, power loss) leaves no report,
so `last_session_aborted` means "a Rust panic", not "the session did not end
cleanly" — closing that gap needs an exit-path call in `main.rs` (one more line)
or a clean-shutdown hook in the app layer. Logs currently land in the working
directory's `appdata/` until D12 moves them into `%APPDATA%\Quire\` with the
database, which also means a portable run keeps its log beside its own database.
The `Logger` is constructed per directory and every write opens the file, so
tests rotate against a 200-byte limit instead of a megabyte.

Update (2026-09-20, D13): the exit-path call landed — `main` notes
`END_RECORD` ("session ended") after the final flush, and `start()` reads its
absence, with no panic report, as an unclean end. The gap this ADR originally
recorded is closed: `last_session_aborted` now means "the session did not end
cleanly", with panic summaries still verbatim ("panicked at …") and the new
shape worded apart ("did not shut down cleanly (killed, crashed natively, or
lost power)"). The tail check follows the rotation family, so an end-record
that shifted into `.1` still reads as clean, and an empty family is a first
run, not a kill. Two residual false-positive sources are accepted: a second
instance reading a live session's tail (no single-instance guard), and any
external kill of a bench/scene run — both *were* unclean ends, the banner
just cannot say who did the killing. The app consumes the session.meta entry
on the next start (M8_FEEDBACK #9 wiring): it lands in the `metadata` table
and in the notice bar, then is deleted so it cannot re-report forever.

## ADR-0017 · Shell identity is a build-time resource; the installer installs
per-user
Decision: the exe's icon and version block come from a resource script that
`build.rs` generates — `IDI_MAIN` pointing at `install/quire.ico` plus a
`VS_VERSION_INFO` whose numbers it reads from `CARGO_PKG_VERSION` — and hands
to `rc.exe` through the `embed-resource` build-dependency. `install/make_icon.ps1`
renders that `.ico` with GDI+ (rounded gradient tile, ring + tail for the Q,
seven PNG-compressed frames from 16 to 256 px), so no image tool enters the
repo. `install/quire.iss` (Inno Setup 6), driven by
`install/build-installer.ps1`, produces `dist/Quire-<version>-windows-x64-setup.exe`
with a Start-menu entry, an optional desktop shortcut, an uninstaller, and an
unchecked task that adds Quire to the "Open with" list for `.md` — never a
default handler.
Why: two crates can embed a resource; `winres` wants a resource compiler
already on `PATH`, while `embed-resource` locates `rc.exe` through `vswhom`
and reports `NotAttempted` on a machine without the SDK instead of failing the
build — a clone that cannot package still compiles. The version stays written
once: `build.rs` reads `Cargo.toml`, and the `.iss` reads it back off the built
exe with `GetFileVersion`. Per-user install (`PrivilegesRequired=lowest`,
`{localappdata}\Programs\Quire`) is forced by the app itself: its default
database path is relative to the working directory (`appdata/quire.db`), so a
Program Files install would start with a non-writable one and fall back to
memory without saying so. (ADR-0020 moved the library to `%APPDATA%\Quire`, so
that premise is gone; the per-user install stays, because it is also what keeps
uninstalling from touching the notes.) Keeping the whole tree in the user
profile also means uninstalling leaves the notes alone — Inno removes only
directories it emptied.
Consequences: the window icon and the `--open <path>` dispatch the `.md`
association invoke both live in files this track may not edit (`ui/**`,
`src/app/**`, `src/main.rs`), so they went to Track A in `M8_FEEDBACK.md` with
the exact lines; until they land, the association only launches the app. An
`rc.exe` rejection now fails the build rather than shipping a plain exe.
`make_icon.ps1` is a manual step: change the palette and the `.ico` does not
follow until it runs again.

## ADR-0016 · Inline marks are the Markdown interchange format, spans in and
spans out
Decision: `import_service::parse_markdown` reads `**bold**`, `*italic*`,
`` `code` ``, `~~strike~~` and `[text](url)` into `core::types::Mark` spans
(byte offsets on char boundaries, the same sorted shape `Command::ToggleMark`
maintains), and `export_service::export_page` writes those spans back as
markers. Neither side builds an AST: the importer recurses into the span it
just matched, the exporter walks the mark *boundaries* and opens/closes markers
there, so nesting is expressed by the ranges — which is the shape the renderer
already consumes. Two shapes have no CommonMark spelling, because its inline
tree is strictly nested: marks of different kinds that only partially overlap,
and styling inside a code span (whose content is literal). The exporter cuts
the first at the crossing — every character keeps its text and each piece
keeps its kind — and drops the second. A literal marker in text is written as
an escape (`\*`), and a code span whose content starts or ends with a backtick,
or is itself padded with spaces, takes the space wrapper that CommonMark
strips back off.
Why: the document model is a flat span list, so anything that parsed a real
CommonMark tree would have to flatten it straight away; keeping the flat list
on both sides makes the pair testable by round trip (`export ∘ import` and
`import ∘ export` land on the same blocks) instead of by a conformance suite
this app cannot afford. Degrading an unrepresentable span into pieces, rather
than dropping it silently, keeps the text byte-exact — the property users
notice when they re-import their own notes.
Consequences: the round trip is a fixpoint from the second pass, not
byte-identical on the first for the crossing case (it comes back as two bold
and two italic pieces). Block-level ambiguity is out of scope and stays
literal: the exporter escapes inline markers but not a line-initial `#`, `-`,
`>` or `1.`, so a paragraph that *starts* with list syntax does not survive
re-import as a paragraph. Tables, images, setext headings and footnotes are
imported as text for the same reason.

## ADR-0015 · Crash recovery: rotating `VACUUM INTO` snapshots, restore at
open; settings and metadata as a diffed key/value layer
Decision: the durability story from M3 stands and is now on the record as
measured: `journal_mode=WAL` is stored in the file header (a later open reads
back `wal`), `synchronous=FULL` (=2), `locking_mode=normal`, `page_size=4096`,
`wal_autocheckpoint=1000` pages ≈ 4 MB, so SQLite folds the WAL back into the
main file by itself during a long session and a clean close checkpoints and
removes `-wal`. On top of that, `src/storage/backup.rs` gives SPEC §二十五 a
backup policy: every successful open shifts `<path>.bak1 → .bak2 → .bak3`
(dropping the oldest) and writes a fresh `.bak1`, and `SqliteRepository::open`
goes through `backup::open_with_recovery` — a main file that fails the startup
`integrity_check` is repaired before the app ever sees an error. Recovery
walks `.bak1 … .bak3`, validates each candidate by really opening it
(migrations + `PRAGMA integrity_check`), moves the unreadable main file aside
as `<path>.corrupt` and deletes its `-wal`/`-shm`, then *moves* (not copies)
the good snapshot into the main path and opens that. `Database::open` itself is
unchanged, so the existing "corruption is reported, not hidden" behavior is
still reachable (and still asserted by `storage::database::tests`).
For the UI's remembered state, `src/services/settings_store.rs` wraps the
frozen contract: `Settings` is a `BTreeMap` with the two keys the panels need
(`theme`, `sidebar.expanded`), and `SettingsStore` over `Arc<dyn Repository>`
offers `load_settings`/`load_meta`, `save_settings`/`save_meta` and the
non-writing `settings_changes`/`meta_changes`, which diff against what the
repository holds and emit only `SettingSet`/`MetaSet` changes — so a burst of
window-resize saves can be queued through `PersistenceService` and stay inside
the debounce window instead of writing per event.
Why `VACUUM INTO ?1` rather than a file copy or rusqlite's `Backup`: a copy of
`workspace.db` misses whatever still lives in `-wal` and can catch a torn page,
while `VACUUM INTO` reads through the live connection (WAL included), writes one
self-contained compacted file with no sidecar, and is a single statement on the
connection the snapshot already locks — so it is one consistent point in the
change stream, not a race. rusqlite's `Backup` offers the same consistency
page-by-page but needs a second destination connection; the statement is
simpler. The copy runs with `synchronous=OFF` and restores `FULL` afterwards,
which is safe because a snapshot is expendable (a torn copy fails its own
integrity check when recovery tries it, and the next open rewrites it) while
`VACUUM INTO` only reads the main database — measured at 2.3 MB: ≈23–49 ms
relaxed vs ≈45–256 ms at `FULL`, same process alternating rounds.
Consequences: startup pays ≈40 ms per 2.3 MB of workspace (PERFORMANCE.md,
M8 addendum) and the folder holds up to 3 extra copies of the database (5 since
ADR-0019) — both are the price of never opening a blank app after one bad
write. The loss
window is by design: `.bak1` is the database *as of the last successful open*,
so a corruption that arrives mid-session costs the edits made since startup;
closing that would mean rewriting the whole file every flush, which §三十三
rules out in spirit — Track A can call `backup::snapshot` from a "save a copy"
menu item if a real case appears. (ADR-0019 later closed this from the inside:
the family is five generations deep and `PersistenceService` snapshots on the
flush tick, so the window is "since the last snapshot", not "since startup".)
Recovery only answers *structural* damage: an
unknown `blocks.kind` still surfaces as `Corrupt` from `load()` (ADR-0013)
after a clean open, and that path is the app's to handle (M8_FEEDBACK.md). A
snapshot failure is logged and ignored — a read-only or full directory must
never block opening the document — so tests that want the unrecoverable case
have to delete the `.bak<N>` family first. Because the FTS5 mirror lives in the
same file (ADR-0014), a recovered database is searchable immediately, with no
rebuild; and because `Settings` treats an empty value as absent (the contract
has no `SettingDelete`), a stored-but-empty setting is indistinguishable from a
removed one.

Addendum (M8, branch `m8-rc`): that recovery is now *reportable*.
`backup::open_with_recovery` returns `(Database, OpenReport)` with
`recovered_from: Option<PathBuf>` (the snapshot that was moved into the main
path) and `backup_failed: bool` (this session has no snapshot behind it),
reached through the new `SqliteRepository::open_with_report`; `open()` keeps
its old signature and discards the report, so every existing caller and test
stays as it was. `OpenReport::log()` prints the two facts in the `eprintln!`
convention startup already uses, which is what `main.rs` calls now — a UI
warning can be built from the same fields without touching storage again
(closes M8_FEEDBACK #4's "no way to tell the user").

## ADR-0014 · Full-text search: FTS5 mirror inside the apply transaction,
CJK indexed by hand-built segmentation
Decision: search (SPEC §二十) uses two FTS5 virtual tables added by schema
version 2 — `search_pages(rowid = page id, title)` and
`search_blocks(rowid = block id, page_id UNINDEXED, text)` — written by
`src/storage/search_index.rs` from *inside* `SqliteRepository::apply`'s and
`replace_all`'s transaction: `insert_page`/`insert_block` index as they
insert, `PageTitleSet`/`BlockTextSet` re-index by rowid, and one orphan
sweep (`prune`) runs per batch that contained a delete, so the FK cascades
that remove blocks and pages need no per-row bookkeeping. The index can
therefore never drift from the document: an aborted batch rolls the index
back with the rows (ADR-0012). `Repository` and `Change` are untouched —
the query API is `SqliteRepository::search(&SearchRequest)` plus
`src/services/search_service.rs`, which aggregates raw matches into one
ranked `Hit` per page (bm25, block matches beat title matches for the
snippet slot) and offers `search_async` → `PendingSearch::poll` so the UI
thread never waits on SQLite (ARCHITECTURE hard rule 1).
Tokenizer: `unicode61`, which never splits inside a run of Han characters
("写作与中文测试" is one token, so "中文" would never match). Indexing
therefore stores a *segmented* copy — `segment()` gives every CJK character
(Han incl. ext. A/B–E, kana, Hangul) its own token — and queries are
segmented the same way, then issued as a *phrase* (`"中 文"*`) so only
adjacent characters match, reproducing the substring semantics of the M2
in-memory scan. Single words keep a trailing `*` for type-ahead.
Why not FTS5's `trigram` tokenizer (the usual CJK answer), measured with
the `#[ignore]`d `search_index::tests::fts5_capabilities` probe on the
bundled SQLite 3.53.2: trigrams need ≥ 3 characters, so a two-character
Chinese term — "中文", "字体", "行高", by far the common case — matches
nothing (`trigram "中文" -> []`, `"中文测" -> [1]`). It also indexes every
offset, inflating the DB for Latin text. Segmentation costs one extra pass
per write and keeps exact adjacency. The probe further confirms
`bro*` → "brown" (prefix works), `"quick br*"` → ∅ (`*` is only legal
*after* a whole phrase), and that an *unsegmented* Chinese phrase matches
nothing — i.e. query segmentation is mandatory, not cosmetic. FTS5 needed
no new Cargo feature: rusqlite 0.40 `bundled` (libsqlite3-sys 0.38) already
compiles SQLite with `-DSQLITE_ENABLE_FTS5`.
Consequences: a keystroke rewrites exactly one index row by rowid, so the
debounced write cost stays O(edited blocks) rather than O(page size) — the
10 000-block page of SPEC §二十二 keeps typing cheap (numbers in
PERFORMANCE.md "M3 · save latency"); the M2 linear-scan search in
`app/workspace.rs` stays in place until Track A wires the panel (both are
consistent with each other, no behavior change in this branch); migrating a
v1 database runs a one-time `search_index::rebuild` backfill after the
step commits (it needs its own transaction), and `check_schema` now also
requires the two FTS tables; `Arc<SqliteRepository>` must be kept around
by the app layer to build a `SearchService` (unsized coercion gives the
`Arc<dyn Repository>` the persistence pipeline wants — a plain
`Arc<dyn Repository>` cannot be downcast back); index rows for pages that
hold no text are simply absent, so an empty page is unsearchable by body
and by title alike; and the index is derived data — a rebuild is always
safe, which D4's recovery path relies on.

## ADR-0013 · Storage schema: cascade-FK tree + split block_children, one
transaction per contract call
Decision: the SQLite file uses the six SPEC §十八 tables — `workspaces`
(kept as the forward-compatible root, single row for now), `pages`
(self-referencing `parent` FK), `blocks` (identity + payload: page, kind,
text, checked), `block_children` (tree placement: `parent` FK + `ord`),
`metadata`, `settings`. Ids are the `core` u64 newtypes stored as `INTEGER`;
`OrderKey` maps u64→i64 by flipping the sign bit so signed storage keeps
unsigned ordering. Every mutation funnels through `Repository`: `apply`
takes one ordered `Change` list inside a single transaction (SPEC §十八),
deletions are recursive-CTE subtree deletes with `ON DELETE CASCADE` as
backstop, `replace_all` defers FK checks to commit and adds an explicit
acyclicity check (FKs alone cannot see an A→B→A cycle). Durability is
WAL + `synchronous=FULL` with a `PRAGMA integrity_check` at open
(SPEC §二十五); schema upgrades are forward-only steps in
`src/storage/migrations.rs` tracked by `PRAGMA user_version`.
Why: the split matches the SPEC's table list and keeps the hot payload
(text) on its own table for cheap `BlockTextSet` writes; cascade + CTE
makes delete semantics single-statement and testable; FULL is paid for
once per debounced burst (≈2–3 ms measured), not per keystroke (SPEC
§三十三 forbids the latter); cycle validation protects the sidebar tree.
Consequences: unknown `kind` strings or unreadable pages surface as
`StorageError::Corrupt` at startup instead of silent data loss; the
`workspaces` table is a placeholder until a real multi-workspace model
lands (feedback to Track A if M4+ needs it in `PersistedState`); a newer
`user_version` refuses to open rather than downgrade the file.

## ADR-0012 · Persistence contract lives in `core/`, storage implements it
Decision: `src/core/types.rs` defines the persisted model (`PageId`,
`BlockId`, `OrderKey`, `BlockKind`, `Block`, `Page`, `PersistedState`);
`src/core/persistence.rs` defines `Repository` (`load` / `apply(&[Change])
/ `replace_all`) plus the `Change` mutation enum. The M3 storage layer
(`src/storage/`, SQLite) implements the trait; the M4 editor emits
`Change`s from commands. The contract was frozen in one commit before the
two work streams started in parallel.
Why: two agents can then build storage and editor simultaneously without
interface drift; `core` stays free of Slint and of SQL details, and undo
(M4) replays inverse `Change`s rather than re-reading the DB (SPEC §十四).
Consequences: sibling order is a `u64` `OrderKey` with midpoint insertion
(`OrderKey::between`, renumber on exhaustion) — no per-insert row shifts;
deletes cascade in storage (undo replays captured `BlockInserted`s); `text`
is plain UTF-8 until M6 inline spans extend it. Any contract change goes
through one owner only (Track A) with the other side filing a feedback
note, never editing both sides at once.

## ADR-0011 · Headless visual regression via `quire-shot`
Decision: UI screenshots are produced by a second binary
(`src/bin/quire_shot.rs`) that installs a custom `Platform` whose window
adapter is Slint's `MinimalSoftwareWindow` (software renderer), renders the
real `AppWindow` into an RGB buffer, and writes a BMP; `just shot <scene>`
converts to PNG. Scenes (`--scene menu|rename|…`) set `UIState`/controller
state directly — no input injection.
Why: OS foreground policy blocks headless SendKeys, screen capture loses
to overlapping windows, and `PrintWindow` returns black pixels for
GPU-composited (GL) windows. The offscreen path is deterministic,
occlusion-proof, and doubles as the visual-regression harness for M3+.
Consequences: `quire-shot` builds only with `--features software`
(`required-features`), so the shipped binary stays lean; PopupWindow
show/close is exercised through the same code path as production.

## ADR-0010 · Page search = substring over a text blob; palette = commands only
Decision: search (Ctrl+P) scans `Page.search_text` (title + block text,
ASCII case-folding, char-boundary snippets, capped at 20 hits); the
command palette (Ctrl+K) lists commands only, generated from the
workspace. Both share `fuzzy_subsequence`-style matching only where it
helps (palette).
Why: content search must find unopened pages, which requires an inverted
index (SQLite FTS, M3+) or a flat blob; the blob is honest, fast at this
scale, and swappable. Separating palette and search mirrors the Notion
model and keeps command resolution unambiguous.
Consequences: `Workspace::search` is the single seam — M3 replaces its
body with FTS queries without touching UI or controller.

## ADR-0009 · UI event loop runs on an 8 MB-stack thread
Decision: both binaries spawn the Slint event loop / render on a thread
with an 8 MB stack (`std::thread::Builder::stack_size`).
Why: Slint 1.18 evaluates the component tree's initial property and
layout bindings recursively on the C stack; Quire's shell (sidebar tree
delegates + editor + five popup trees) needs slightly over the 1 MB
Windows default in debug builds, and popup open/close adds depth at
runtime. Verified by bisection: any single component removed masks it,
512 MB survives it — finite but deep. 8 MB is the standard Linux default
and costs only address-space reservation.
Consequences: crashes-in-the-field from stack exhaustion are off the
table for the planned M3–M6 growth; if a future Slint flattens binding
evaluation, the wrapper can be dropped in one place.

## ADR-0008 · Popup lifecycle is state-driven, never `is-open`-read
Decision: `CommandPalette` uses `close-policy: no-auto-close`; show/hide is
driven exclusively by `UIState.palette-open` (mirrored in AppWindow with
`show()`/`close()` handlers).
Why: Slint 1.18.0's const-propagation pass panics (`const_propagation.rs:
464`, no diagnostics) whenever a popup component *reads* its own `is-open`.
Verified by bisection with the `QUIRE_PROBE` subset-compile hook in build.rs.
Consequences: closing logic lives in UIState, which also makes the palette
testable from Rust; revisit if a future Slint fixes the crash.

## ADR-0007 · Page chrome rides inside delegates (ListView single-`for`)
Decision: a `ListView` may contain exactly one `for`; the page title and the
bottom spacer therefore live inside the first/last `DocumentRow` delegate
(`first` index check, `BlockRow.tail` flag set by Rust).
Why: 1.18 markup rejects sibling elements of a ListView `for`, and models
expose no `.length` to compute "last row" in the UI.
Consequences: Rust owns the `tail` invariant; re-sorting or appending blocks
must re-flag it (see `with_tail` in state.rs).

## ADR-0006 · M1 UI state via a single `UIState` global
Decision: transient UI state (dark, sidebar-open, palette state, selection
ids) lives in one Slint global; business actions are callbacks on that same
global, wired in `src/app/controller.rs`.
Why: components stay independently restorable; Rust reaches everything
through one generated accessor; there is exactly one place an agent must
read to understand UI state flow.
Consequences: property names are a contract — renaming requires touching
controller.rs and any component in the same change.

## ADR-0005 · Mock content is served through Slint models
Decision: sidebar tree, blocks, and command rows arrive from Rust as
`ModelRc<VecModel<struct>>`, even for M1 mock data.
Why: establishes the node-editor-cpp pattern (backend owns models, UI only
renders) before any real document model exists, so M3/M4 replace data
sources, not plumbing.
Consequences: mock data has the same shape discipline as real data.

## ADR-0004 · Renderer is a per-binary compile-time choice
Decision: `slint` is compiled with `default-features = false`; exactly one
renderer feature is enabled per build (`femtovg` default; `femtovg-wgpu`,
`skia`, `skia-opengl`, `software` selectable). Benchmarks build one binary
per renderer into its own target dir.
Why: comparing GPU stacks at runtime inside one binary would distort
memory numbers; separate binaries keep idle-RAM measurements honest.
Consequences: CI builds at least two renderer configurations.

## ADR-0003 · No TextEdit per block
Decision: static blocks render as `Text` + shapes; only the focused block
will own a real editing surface (M4), plus ListView-based virtualization
from the first commit.
Why: 10 000 interactive widgets is a guaranteed memory/CPU failure mode.
Consequences: caret/selection rendering inside the focused block must be
built deliberately (Slint TextInput + our selection model), not assumed.

## ADR-0002 · Windows IME = Slint + DirectWrite, no custom TSF
Decision: rely on Slint's winit text input (which uses OS IME composition
events); write a platform adapter only if a reproducible Slint/Windows bug
forces it.
Why: the previous input-method project proved TSF integration is the most
expensive kind of platform coupling; this app must stay agent-maintainable.
Consequences: Chinese input quality is a test item at M4, not a feature to
build.

## ADR-0001 · Slint + Rust, single process, no web runtime
Decision: Slint 1.18.x UI in the same process as the Rust core; SQLite for
persistence (M3); no Electron/Tauri/WebView/React/Vue anywhere.
Why: GPU-accelerated native rendering with real low-RAM/low-idle-CPU
behavior, and one language boundary (`.slint` ↔ Rust) that coding agents
can maintain for years.
Consequences: rich text editing and IME polish must be built rather than
inherited from a browser engine; this is accepted deliberately.

## Dependency policy
Every crate must answer: why needed / can std do it / runtime memory cost /
extra threads / build complexity. Current set: slint + slint-build (M1),
rusqlite with the bundled SQLite (M3 — persistence has no std answer), rfd
(M8 dialogs — native file pickers, no UI toolkit dependency), and
embed-resource as a *build* dependency only (M8 installer — it runs rc.exe and
adds nothing to the binary). Anything else waits for a milestone that cannot be
built without it.

---

## ADR-0050 · mention 与 date 存储形态 & Markdown 往返语法

**状态**：已决定
**日期**：2026-09-23
**驱动**：SPEC §四十 M13（引用、提及与反向链接）

---

### 上下文

SPEC §四十 要求在正文中支持两类新的原子标记：
1. **@page mention**：引用某一页面，渲染为 chip，存储引用的目标 page_id
2. **@date**：内联日期，渲染为 chip，存储 ISO 日期字符串

现有 `marks` 表结构：

```sql
CREATE TABLE marks (
  block  TEXT NOT NULL,
  start  INTEGER NOT NULL,
  end    INTEGER NOT NULL,
  kind   TEXT NOT NULL,
  url    TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (block, start, kind)
);
```

`MarkKind` 现有六种：`Bold | Italic | Strike | Code | Link | Math`。
`Mark` 结构体字段：`start: u32, end: u32, kind: MarkKind, url: String`。

---

### 决策一：存储形态

**复用 `marks.url` 列，不新增列，不改主键。**

- mention：写入 `url = "quire://page/<page_id>"`
- date：写入 `url = ""`（空），日期内容存入 `Mark.date: Option<String>`

理由：
- ADR-0026 已约定 block 级引用存 `blocks.page_ref = "quire://page/<id>"`；inline mention 共用同一 URI 格式是自然的延伸。
- 主键 `(block, start, kind)` 不变。mention 与 link 在同一位置可以共存——两者 `kind` 不同，PK 允许共存，无需迁移。
- `MarkKind` 枚举末尾追加 `Mention` 和 `Date` 两个-variant，对应字符串标识符 `"mention"` 和 `"date"`。
- `Mark` 结构体末尾追加 `date: Option<String>` 字段（`""` 或 null 等价，SQL 层用 NULL 表示 None）。

**存储映射**：

| kind | `url` 字段 | `date` 字段 |
|------|-----------|-------------|
| `mention` | `"quire://page/<id>"` | NULL |
| `date` | `""` | `"2026-09-22"` |

---

### 决策二：Markdown 往返语法

**Mention**：`@[Page Title](quire://page/<id>)`
- 导出（Rust → Markdown）：输出 `@[Title](quire://page/<id>)`，其中 Title 从 `blocks.title` 或内存中的 page title cache 读取
- 导入（Markdown → Rust）：正则 `` `@\[\]\((quire://page/[^)]+)\)` `` → 解析出 page_id，构造 `Mark { kind: Mention, url: "quire://page/<id>", date: None }`
- 渲染：chip 显示 Title，点击导航到目标页

**Date**：`@[2026-09-22]`
- 导出：输出 `@[YYYY-MM-DD]`
- 导入：正则 `` `@\[(\d{4}-\d{2}-\d{2})\]` `` → 构造 `Mark { kind: Date, url: "", date: Some("2026-09-22") }`
- 渲染：chip 显示格式化日期

**共存约束**：同一 `(block, start)` 可同时存在 kind=`mention` 和 kind=`link`（PK 允许）；它们在 Slint 端通过两个独立的 cell/callback 渲染，互不干扰。

---

### 影响

- **DB 迁移**：`MarkKind` 的两个新值**不需要迁移**——`marks.kind` 自 v3 起就是 TEXT，
  存的是 kind 字符串本身。真正需要迁移的是反向链接的**索引**，那是 ADR-0051 / migration 16。
- **Rust 类型**：`MarkKind` 末尾加 `Mention, Date`；`Mark` 加 `date: Option<String>`。
- **repository.rs**：`load_marks` / `store_marks` 需解析新的 kind 值；`import_markdown_run` 需识别两个 regex；`export_markdown_run` 需输出 `@[...]` 格式。
- **ADR-0051（反向链接索引）**依赖本 ADR：反向链接查询走 `marks` 表的
  `kind='mention' AND url='quire://page/<id>'`（精确匹配，不是 `LIKE`）加另一条
  `blocks.page_ref`，两条都靠 migration 16 的索引做 seek，不额外存储、不建派生表。

---

### 未验证边界

- mention chip 的 Title 在离线场景下（目标页已被删除）应显示什么兜底？（待 T2.4 dangling ref 处理）
- date chip 的显示格式是否需要 locale-aware？（当前约定 ISO 纯展示，待 UI 验证）
- `@[Title](url)` 中的 Title 与存储的 page_id 是否做一致性校验？（导入时不做，写入时只存 id）

---

## ADR-0051 · 反向链接不做索引表：两条索引 + 每次投影现算

**状态**：已决定（**取代本 ADR 早先的 FTS5 token 草案**）
**日期**：2026-09-22
**驱动**：SPEC §四十（反向链接面板）+ SPEC §二十（搜索类索引不许全库扫）+ SPEC §二十二（页面打开 <50 ms）

---

### 上下文

§四十 要求页面底部列出"所有引用本页的块"。这份面板**每次投影一个页面都要算一遍**
（和 §三十八 的 TOC 一样），所以它站在交互路径上，不是后台任务。

引用在库里**已经存了恰好一次**，而且存的是两处：

| 形态 | 存在哪 | 由哪条 ADR 定 |
|---|---|---|
| `@page mention`（正文里的一颗 chip） | `marks.url = "quire://page/<id>"`，`kind='mention'` | ADR-0050 |
| 块级引用（`Page` / `Link to page` 这类"这一块就是引用"） | `blocks.page_ref = <id>` | ADR-0026 |

`marks` 表自 migration v3 起只有一列载荷 `url`，主键 `(block, start, kind)`。

---

### 决策

**不加表、不加列、不挂 FTS5：加两条索引，每次投影时直接查这两处。**

migration 16（`src/storage/migrations.rs`）：

```sql
CREATE INDEX IF NOT EXISTS idx_marks_reference ON marks(kind, url);
CREATE INDEX IF NOT EXISTS idx_blocks_page_ref ON blocks(page_ref);
```

查询在 `src/storage/backlinks.rs`，一次 UNION 拿两路来源：

```sql
SELECT b.id, b.page, b.text, 0 AS block_level
  FROM marks m JOIN blocks b ON b.id = m.block
 WHERE m.kind = 'mention' AND m.url = ?1
UNION
SELECT b.id, b.page, b.text, 1
  FROM blocks b
 WHERE b.page_ref = ?2
 ORDER BY 2, 1
```

`refresh_backlinks()`（`src/app/state.rs`）每次投影做两次读：先 `count(*)`，**只在计数非零时**
再取一个窗口（折叠 5 行 / 展开 50 行）。

---

### 为什么不是"派生表/派生列"

这是本 ADR 唯一值得写下来的地方：**反链表是「一份只写一次就没有第二份可漂移」的数据**。

- 引用已经存在两处真实事实里（`marks.url`、`blocks.page_ref`），而这两处正是**编辑器已经
  在渲染的东西**。再存一份反链，就等于把同一句话抄第二遍，然后需要一条写入路径去维护它、
  一个 rebuild 步骤去补救它、一次 sweep 去证明它没漂。
- 索引不是这样：B-tree 由 SQLite 从**已经在磁盘上的行**建出来，没有写入路径、没有 rebuild、
  没有"忘了同步"的状态。项目里唯一一类从不需要 sweep 的派生数据就是这个形状。
- 反过来，FTS5 token 方案（本 ADR 的早期草案）恰好踩在这条线上：它要在 `search_blocks.content`
  里塞 `__backlink:<id>` token，于是**删一条 mention 就得把旧 token 从 content 里摘掉**，
  否则面板显示幽灵引用；摘的时候又会把用户正文一起重写。草案自己把这一条记成了 bug 待修——
  那不是待修，那是这条路本身就有的形状：多一份派生数据，就多一条必须维护它的写入路径。

### 为什么不是"每次打开页时扫 `marks`"

这是 §二十 明令禁止的那一条，本 ADR 把它量了出来（`docs/PERFORMANCE.md` "T2 · the backlink
panel is one index seek"）：1 200 页 / 100 200 条 mark 的库里，同一个折叠读
**有索引 78 µs、去掉索引 6 068 µs**（78×）。控制臂就是同一进程里把 `idx_marks_reference`
drop 掉再跑同一条查询——不是发布配置，是这句"不许全库扫"到底在说什么。

`(kind, url)` 的顺序不是随手写的：谓词正好是这两列，而**只用最左列 `kind` 不够**——
那还是要把同 kind 的每一行扫一遍，而一个一万个加粗 span 的页面一条 mention 都没有。

---

### 后果

- **面板是投影，不是数据。** 它和目录（ADR-0039）同一条规则：每次投影现算，不入库，
  所以**不可能过期**——没有副本可以过期。
- **改名自然跟随。** 引用存 id（ADR-0050），行上的页面名是投影时向 workspace 问的，
  所以改一个页面的名字，chip、面板分组标题、Markdown 导出同时变，且**没有任何一处被重写**。
  代价的另一面也在这里：目标页被删时不会有谁去改引用，所以**退化必须是投影层自己的职责**——
  `delete_page` 因此在删掉非当前页之后补一次 `reproject_blocks()`（T2 修的一个真 bug：
  此前 chip 会一直显示旧标题）。
- **成本落在打开页面上，且是可加的。** 实测：没有反链的页 +0（只做一次 `count(*)`），
  200 条反链的页 **+0.06 ms**（折叠）/ **+0.07 ms**（展开），对 §二十二 的 50 ms 预算。
- **展开的窗口也是窗口。** 折叠 5 行、展开 50 行，超过 50 条时面板给的是"总量"而不是全列表——
  一句被引 200 次的话是**一个数字**，不是页面底部放得下的列表。它折叠回去而不是假装展开了全部。
- **未验证边界**：① 面板的窗口没有虚拟化（它是一个文档行里的 `for`），所以 50 是硬上限，
  没有量过 1000 条展开会怎样——因为那条路不存在；② 打开页面的两个读数取自**单块页面**，
  因此只隔离了面板本身，对"大页面的投影成本"没有发言权（那个数字归 ADR-0039）；
  ③ `count(*)` 是不带筛选的单页计数，带筛选的引用查询没有实现也没有量。

## ADR-0060 · A database is its own entity behind a block, and the six view layouts are one kind

Decision: SPEC §三十九's `database` is a **new entity**, not a page flag and
not a block payload: one row in `databases(id INTEGER PRIMARY KEY, name TEXT NOT
NULL DEFAULT '')`, reached from a page through one **new block kind**,
`BlockKind::Database` (`as_str` = `"database"`), pointing at it through a new
nullable `blocks.db_ref INTEGER` — the shape ADR-0026 gave `blocks.page_ref`,
and for the same reason. A "full-page database" is not a second entity: it is an
ordinary page whose first block is a `Database` block, so there is one schema,
one storage path and one set of lifecycle rules, and a page that holds a
database is still a page with prose above and below it. SPEC's
`table → board → list → calendar → gallery → timeline → form → chart` — and the
six muted `INSERT_ITEMS` rows in `state.rs` (`Table view`, `Board`, `Gallery`,
`List view`, `Calendar`, `Timeline`, all `id = -1`) — are **layouts of that one
entity** (`db_views.layout`), not six or eight block kinds: choosing one creates
a `Database` block whose first view has that layout, which is what lets the
placeholders be lit one phase at a time (D5) without a new kind each time, and
what makes `linked database` (D7) a pointer at an existing view rather than a
ninth kind.

Why not a page: `pages` has no schema, and a boolean "this page is a database"
would have to be re-read by every path that lists pages (§十七's tree,
Favorites, Recents, the search index, the sidebar's drag) while still needing
the property and value tables anyway. It also cannot express D7's "show another
database's view here", because a page can only ever be itself. Why not a payload
column on the block: that is ADR-0031's rejected alternative again — a payload
stops a row from being a page and a cell from carrying inline marks or its own
undo granularity, and §三十九 says outright that a record may *be* a page, so the
row has to keep the identity a page has (`pages.id` — the thing §四十's `@page
mention` points at).

Consequences:

* This ADR fixes the shape; D0 ships **no** kind. Schema stays at v11 through
  D0, so the six wiring points §三十七 lists (types, kind string, Markdown,
  Turn into, slash/insert menu, screenshot scenes) land in one phase together
  with the delegate that draws a view. That is also why the D0 sweep is
  byte-identical: no menu moved because no kind exists yet.
* The block is a **leaf**, unlike `table` and `columns`: it owns no child blocks
  (its rows are records, its cells are values), so the projection has nothing to
  hide and no row-index consumer has to translate. What it does own is the
  `databases` row: deleting the block deletes the entity the way deleting a
  `Page` block deletes its child page, and a dangling `db_ref` (the entity gone,
  the block back through an undo) renders one muted, non-editable line —
  "(deleted database)" — exactly as a dangling `page_ref` does.
* The window is what makes a 10 000-row database safe inside one page, and D0
  proved the channel exists before any of it was drawn: `core::database::window`
  realizes **31 rows of 10 000** at the top of a 720 px viewport with 32 px rows
  (39 mid-scroll, 31 at the bottom), and the realized rows cost **6 806 B** of
  heap against **2 259 800 B** for the table's own row objects
  (`benchmarks/results/2026-09-22-track3-probe.jsonl`).
* Still unverified: nothing draws a view yet, so that number is the projection's
  and not a frame's; `row_height` 32 px and `overscan` 8 are this slice's
  assumptions and D3 re-measures both; and until D5 the six `INSERT_ITEMS`
  placeholders keep promising views that do not exist, which is a visible
  promise the menu is still not keeping.

## ADR-0061 · The schema is rows (`db_properties`), and only a column's options are JSON

Decision: a database's columns are rows, not a JSON column on `databases`:

```sql
CREATE TABLE db_properties (
    id     INTEGER PRIMARY KEY,
    db     INTEGER NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
    name   TEXT NOT NULL,
    kind   TEXT NOT NULL,             -- the property type, a short stable string
    config TEXT NOT NULL DEFAULT '',  -- that type's own settings, JSON
    ord    INTEGER NOT NULL,
    UNIQUE (db, name)
);
```

`kind` is a string in the same spirit as `blocks.kind` and `blocks.lang`, but
with the failure **folded rather than fatal**: an unknown kind loads as `text`
(the rule ADR-0044 sets with `PageFont::try_from_str`), because a library
written by a build that knows `relation` must still open in one that does not,
and the cell draws as text. `config` is JSON *inside the row* and holds exactly
what SQL never filters on: a select/status option list
(`{"options":[{"id":7,"name":"Done","color":"green"}]}`), a number's format, a
date's format, a rollup's target. Options carry their own **ids**, so renaming
an option is one JSON edit that touches no value — the "store the id, not the
label" rule ADR-0026 already uses for page references.

Why rows for the schema and not one `databases.props_json` blob: the filter and
sort compiler emits SQL that names a property *by number*
(`db_values.property = 7`), so a JSON schema forces every read path through
`json_extract` to learn an id, a kind and a name — awkward but survivable. What
is not survivable is the invariant: `UNIQUE (db, name)` is what makes "rename a
column" well defined, and no JSON blob can enforce it, so a rename racing
against itself would leave two columns called `Status` that only Rust can
detect. What rows cost is ordering (`ord` is an app invariant like
`block_children.ord`, not a constraint) — accepted, because moving a column is
one UPDATE.

Consequences:

* Every database is created with its `title` property (`kind = 'title'`,
  `ord = 0`) and one view (`layout = 'table'`) in the same batch as the
  `databases` row: a database with no title property or no view cannot be drawn,
  so no path may create one.
* A property's values are reached through the `property` FK and die with it
  (`ON DELETE CASCADE` in ADR-0062's tables) — the one cascade this design
  wants, because the schema is the parent of its values.
* `person` degrades to `text` (SPEC's 降级处理: with no account model, a local
  name list would need its own table, its own picker and its own merge rules for
  zero extra data), and `formula` / `rollup` / `relation` are kinds whose value
  is **not** stored (ADR-0062, ADR-0039).
* Still unverified: deleting a property cannot clean the view documents that
  name it (no foreign key reaches inside ADR-0064's JSON), so the compiler has to
  ignore unknown ids and D4 pins that with a test; and "exactly one `title` per
  database" is an app invariant of the insert path, not a constraint, so a
  repair could break it without SQL noticing.

## ADR-0062 · A value is one row per (record, property), typed by column

Decision: values live in one table with the columns SQLite needs in order to
compare them in its own type system, plus one child table for the types that
hold a list:

```sql
CREATE TABLE db_values (
    record   INTEGER NOT NULL REFERENCES db_records(id) ON DELETE CASCADE,
    property INTEGER NOT NULL REFERENCES db_properties(id) ON DELETE CASCADE,
    text     TEXT NOT NULL DEFAULT '',
    num      REAL,
    flag     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (record, property)
);

CREATE TABLE db_value_items (
    record   INTEGER NOT NULL REFERENCES db_records(id) ON DELETE CASCADE,
    property INTEGER NOT NULL REFERENCES db_properties(id) ON DELETE CASCADE,
    ord      INTEGER NOT NULL,
    value    TEXT NOT NULL,
    PRIMARY KEY (record, property, ord)
);
```

`text` carries title / text / url / email / phone / select-option-id /
status-option-id / date; `num` carries number; `flag` carries checkbox;
`db_value_items` carries multi-select option ids and `files` attachment ids —
ADR-0029/0030's store, so a column of files is the same bytes-beside-the-database
channel and not a second one. A date is stored as its fixed-width ISO-8601 text
(`YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`, local wall time, no UTC conversion because
this app has one clock and no accounts), which is why it needs no second column:
the writer is the only producer of that exact form and the parser rejects
anything else, so text order *is* time order.

Why typed columns and not one TEXT column: §三十九 puts filter and sort in SQL,
so the comparison has to happen in SQLite's own type system. One TEXT column
makes `ORDER BY` lexicographic — `10` lands before `9` — and the fix,
`CAST(text AS REAL)`, cannot use an index and silently sorts a malformed value
as 0. `num REAL` is indexable, so a number property's sort is
`ORDER BY v.num, r.ord` over the one LEFT JOIN the row query already has. Why one
table and not one per type (`db_values_number`, `db_values_date`, …): a view
reads every visible property of its window in one query, and per-type tables
turn that into a join count that varies with the view's shape — string-built SQL
with fourteen arms — while one row per (record, property) is one join per
*sorted or filtered* property and the same row for everything else.

Consequences:

* `formula`, `rollup` and `relation` store **nothing**: they are computed at
  projection time for the window only, which is where §三十九's 禁止每次输入全库
  重算 will have to be demonstrated (D6, with the recomputed-row count as its
  number). `created time` and `last edited time` are the two §三十九 types this
  ADR does **not** place: `created time` needs one real column as its source
  (nothing in `pages` or `db_records` records a creation instant today) and
  `last edited time` needs a source only the write path can keep honest —
  writing either into `db_values` would be the double write ADR-0039 forbids, so
  D2 lands them with their own ADR and a measured story.
* "Empty" and "not a number" are the same thing: the row is absent or
  `num IS NULL`, never `0`. The sort's empty placement is emitted explicitly
  (`ORDER BY v.num IS NULL, v.num`) because SQLite puts NULLs first and "the
  blank rows floated to the top" is not what a user means by "sort by number";
  D4 pins it with a test.
* A multi-select filter is `EXISTS (SELECT 1 FROM db_value_items WHERE record =
  r.id AND property = :p AND value = :option)` — an index probe on the PK's
  prefix — and `files` gets the same `EXISTS` shape for "has an attachment",
  which is why the list types are rows and not a JSON array hidden in `text`.
* Still unverified: nothing here measures a 10 000-row × 5-property filter (D4's
  number), and the one-row-per-(record, property) shape makes a cell write an
  `INSERT OR REPLACE` whose row count D6 will read as its dependency edge.

## ADR-0063 · A record owns its page, the title has one home, and both deletes are one undo step

Decision:

```sql
CREATE TABLE db_records (
    id   INTEGER PRIMARY KEY,
    db   INTEGER NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
    page INTEGER REFERENCES pages(id) ON DELETE CASCADE,   -- NULL = a bare record
    ord  INTEGER NOT NULL,
    UNIQUE (page)
);
```

* **Ownership, in ADR-0026's vocabulary.** A record *owns* its page the way a
  `Page` block owns its child page; `UNIQUE (page)` makes the reverse true too —
  a page is the face of at most one record, and two rows can never share one.
  `linked database` (D7) is the *referencing* case, symmetrical to `Link` vs
  `Page`.
* **The title has exactly one home, decided by whether the record has a page.**
  A page-backed record's title is `pages.title` and nothing else; a bare
  record's title is the `db_values` row of its `title` property. The view's row
  query already LEFT JOINs `pages` for the page-backed case, so the title column
  reads `COALESCE(p.title, v.text)` and there is no second copy to go stale —
  §三十九's record-is-a-page without ADR-0039's double write.
* **A record is bare until something needs its page.** Creating a row creates no
  page. The page arrives with `Open` (or "Turn into page"), as a child of the
  page that holds the `Database` block, and that one command *moves* the title
  from `db_values` into `pages.title` in the same batch. Its exact inverse,
  "Turn into a plain record", moves the title back and clears the pointer, and
  **leaves the page in the tree** — a page the user made is theirs to delete;
  this operation is about the pointer.
* **Delete the row** (the view's row menu): one `Command::DeleteRecord` whose
  plan is `[DbValueDeleted…, DbRecordDeleted, PageDeleted?]` with the captured
  rows as its inverse — the record, its values and, when it is page-backed, the
  page it owns. **Delete the page** (the sidebar, or a parent page's recursive
  subtree delete): `db_records.page`'s `ON DELETE CASCADE` is the SQL backstop,
  so a row cannot outlive the page it is the face of even when the deletion
  arrives from SQL rather than from the command layer. Both paths end in the same
  state, and that is the property to test: *a database never holds a row whose
  page is gone, and never loses a page while its row survives.*
* Why "both go" rather than "the row survives as a bare record": the alternative
  silently resurrects a row — in a view nobody is looking at, named after a page
  the user deliberately deleted — and it has to write a title back on a delete
  path, which is how a delete acquires a failure mode.

Why the relationship is a pointer at all: §三十九's 「record 可以同时是一个 page，
这是 Notion 的核心而不是装饰」. A row that can be a page has to keep a page's
identity — `pages.id`, the thing §四十's mentions point at and the thing the tree
draws — so a record can never be "the page's data"; the pointer is the only
shape in which both exist without one being derived from the other.

Consequences:

* Undo is one step in both directions because a command plans `apply` and
  `revert` together (`core::document::Entry`), so "delete the row and its page"
  is one Ctrl+Z, and so is the convert-and-move-title pair. The one place
  §三十九's 「删 record 与删页面的行为…都进 undo」 is **not** yet true is the
  sidebar's own page delete: that path (`AppState::delete_page` →
  `Change::PageDeleted`) is confirmed by a dialog and has never been on the undo
  stack, so a row lost through it is lost. The cheapest fix is to route that
  confirmation through a plan of the same shape; this ADR does not claim the gap
  is closed.
* Because a bare record's title lives in `db_values`, `title` is the one column
  whose filter and sort compile differently per record (a `COALESCE` over a LEFT
  JOIN). One query shape covers both, and D4 owes the test that sorting by title
  interleaves bare and page-backed rows in a single order.
* Lazy page creation is what keeps the tree honest: a 10 000-row database whose
  rows nobody opened creates 0 pages, and each `Open` costs one page, one title
  move and one undo step. It also means `db_records.page` is NULL for most rows,
  so SQLite's tolerance of many NULLs under `UNIQUE (page)` is load-bearing
  here, not incidental.
* Still unverified: neither delete path has a test yet (they are D1's), and the
  sidebar does not mark a page as a database's row, so nothing warns a user
  before they delete a page that a database still points at it.

## ADR-0064 · A view is a row with a name and a layout, and its rules are one JSON document

Decision: `db_views` stores the parts SQL has to list and the rules in one
document:

```sql
CREATE TABLE db_views (
    id         INTEGER PRIMARY KEY,
    db         INTEGER NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    layout     TEXT NOT NULL DEFAULT 'table',
    definition TEXT NOT NULL DEFAULT '',   -- filter + sorts + groups + visible columns
    ord        INTEGER NOT NULL,
    UNIQUE (db, name)
);
```

`layout` is SPEC's eight-view list as a string (`table` / `board` / `list` /
`calendar` / `gallery` / `timeline` / `form` / `chart`), with an unknown value
folding to `table` the way `blocks.lang` folds an unknown fence to `Plain`.
`definition` is one JSON document — `{"v":1,"filter":…,"sorts":[…],
"groups":[…],"columns":[…],"widths":{…}}` — whose filter is a recursive node
(`{"and":[…]}`, `{"or":[…]}`, `{"property":7,"op":"eq","value":…}`), because a
filter is a tree and its nesting depth is not bounded by anything a user can see.

Why the rules are JSON where ADR-0061's and ADR-0062's data is not — the same
test, opposite answers: **what does SQL have to filter on?** Properties and
values are filtered and sorted *by*, so they are rows with typed columns; a
view's rules are only ever compiled *into* a query and never filtered on, so
their shape should be whatever the compiler reads best. A filter tree in rows
needs a parent-pointer table plus recursive assembly, and every query would
reassemble the tree it had just been handed, for zero SQL benefit. The name and
the layout stay columns because the view switcher lists them without parsing
anything, and because a view's name has to be unique per database to be a
switcher entry at all.

Consequences:

* Property references inside `definition` are ids (ADR-0061) and **the compiler
  drops ids that no longer exist**: no foreign key reaches inside a JSON
  document, so a view whose filter names a deleted property loses that clause
  and shows more rows instead of failing to open. The same rule covers a sort and
  the visible-column list, and D4 pins all three.
* A document that does not parse — truncated, hand-edited — degrades to "no
  rules", so the view opens showing everything: a database that cannot be opened
  is worse than one that is not filtered. `"v":1` inside the document is what
  lets a later build add a key without an older build reading it as corruption.
* A compiled plan (the SQL text and its bind values, for one definition and one
  property list) is **derived** and never stored (ADR-0039): it is rebuilt at
  open and cached for the session. `linked database` (D7) stores `(db, view)`
  and never a copy of the definition, so a linked view cannot drift from its
  source.
* Widths are a map keyed by property id rather than an array parallel to
  `columns`, so deleting a column cannot leave a width pointing at the wrong
  one; the cost is that an entry for a column no longer visible is dead weight
  the panel has to clear.
* Still unverified: nothing parses a definition yet, so forward compatibility is
  a decision rather than a test; and where a *group* header's own row lands in
  the window (it is not a record row) is D4's shape to fix, not this ADR's.

## ADR-0065 · A database exports as the table it is showing, and imports as text

Decision: the Markdown channel (§二十六) renders a `Database` block as a
GitHub-flavoured table of the view it is **currently showing** — the title column
first, then the visible properties in view order, one line per record in the
order and membership the view shows, filters and sorts included, because the file
should say what the user sees rather than what the table holds. Cells render by
type: text / url / email / phone verbatim; number as the stored number without
its display format; checkbox as `Yes` / `No`; select and status as the option's
**name**; multi-select as names joined by `, `; date as its ISO text; files as
`[name](quire://attachment/<id>)` (ADR-0030's link shape); person as the stored
name; `relation` as the target records' titles; `formula` / `rollup` as their
**computed** value, computed on the way out, which is legal precisely because it
is never stored (ADR-0038/0039). A row whose record is page-backed writes its
title as `[title](quire://page/<id>)` — ADR-0026's shape for a `Page` block — so
the file keeps the only durable handle a reader has on that page, while a bare
record's title stays plain text. The block writes **no marker line**, and the
export does **not** recurse into record pages: a record's page is an ordinary
page, exported when someone exports that page.

Import is unchanged, and knowingly asymmetric: `parse_markdown` reads line at a
time, so a pipe-separated line stays a paragraph — ADR-0031 pinned exactly that
for the simple grid, with a test, for exactly this reason. A database therefore
exports to a table that reads in any Markdown renderer and comes back as text:
schema, property types, record identities and views do not survive the channel,
and nothing pretends otherwise.

Why a table and not a one-line marker like ADR-0039's `<!-- quire:toc -->`: a
contents block has no other representation, because its body is derived from the
page, so a marker is the only thing that could mean it. A database's rows are
content, and the table is a real representation of them that a human and another
tool can both use; a marker would be a second and weaker copy of the same fact,
and one that names an id nothing can resolve on import is a dangling promise of
the kind ADR-0026 renders as "(deleted page)" rather than as a feature.

Consequences:

* `export_page(blocks: &[Block]) -> String` cannot see records or values, and it
  must not learn to (ADR-0044's boundary already recorded that the exporter never
  receives a page object; handing it a repository would give the content channel
  a second data path). The database's table therefore arrives as **pre-rendered
  rows**: the caller that already reads the database passes the header and the
  rows, and the exporter only lays them out — the same division
  `attachment-size` and the math glyphs already use, where a renderer asks for a
  value and never fetches it. That is a signature change to `export_page`, and
  the callers that only have blocks (clipboard, "Copy page as Markdown") pass
  none.
* Property values cross the channel as display strings. That is lossy by
  decision, and the cost is worth one line: a database is the first thing in this
  app whose export cannot be re-imported even in principle, because its content
  is not its blocks.
* Still unverified: no code path renders a database to Markdown yet (D3/D8), so
  the per-type table above is a specification and not a test; and an exported
  `formula` column inherits D6's recompute correctness, so a wrong formula is a
  wrong file.

## ADR-0081 · `bookmark` is closed rather than deferred, and the embed card is what stands in its place

Decision: SPEC §三十七 批次 C's `bookmark` line — 「链接卡片，抓标题与 favicon；
离线或抓取失败退化为纯链接，且不得阻塞输入」 — is **withdrawn**, not postponed.
Quire does not fetch a page's title or favicon, and no code is written for it in
this milestone.

Why: the premise the backlog item rested on ("Quire 至今没有任何网络客户端") is not
accurate, and the correction is what changes the price. Quire **has** an HTTP
client: `src/services/lan_client.rs` is a dependency-free HTTP/1.0 client — one
`TcpStream`, one `GET`, read to EOF — behind `main.rs`'s `--pull <url>` for the
LAN share feature. So the accurate statement is: **Quire has no TLS and no
request it was not told to make.** The opt-in is real (server side behind the
off-by-default `lan.share` setting, client side only for a URL typed on the
command line), which means the gap is one of *transport security* and of
*consent*, not of "does this app do networking".

Against that, the three routes and their prices:

1. **Transport.** `http_get` cannot reach an `https` URL at all, so route (a)
   needs `rustls` (`ring` or `aws-lc-rs`) or `native-tls`/SChannel — a second
   new dependency tree in the same milestone as the PDF thumbnail's, several MB of code,
   and a certificate-store story. `http://`-only would be a downgrade no real
   site's favicon is worth.
2. **Consent.** SPEC answers the failure half, so the frame would have to be
   "user pastes a URL or presses Fetch → a worker does the GET → the card
   repaints or stays a plain link", never a projection, never a paint, never a
   timer. The question that has to be answered first is whether Quire ever makes
   a request the user did not just ask for — on import, on page open, to refresh
   a stale title — and the honest answer for a local-first notes app is no.
3. **Storage.** A title and a favicon have to be persisted or the card changes
   shape whenever the machine is offline; that is a disk cap, an eviction rule,
   and a "same host on ten cards" rule that need numbers, plus one more derived
   copy of something that lives elsewhere (the argument ADR-0039/0040 already
   make against derived data with no owner to invalidate it).

With the transport cost, the consent question and the storage rules all landing
in one milestone whose whole purpose is closing the backlog, and with the card
shape SPEC actually asks for already delivered by the non-fetching embed card
(ADR-0040) — a card that names its provider, shows the address it will hand the
system, and degrades to nothing — the feature is closed. This is a decision and
not an omission: the SPEC text is rewritten to say so, so the next reader does
not re-open it as an oversight.

Consequences:

* SPEC §三十七 批次 C loses the `bookmark` promise; the embed card's "边界（不算
  缺陷）" line loses its cross-reference to it (「那是 bookmark 的活」).
* `CHANGELOG.md`'s Known-limitations entry for `bookmark` is re-worded from an
  open item to a closed decision, so the RC's list of what is missing is
  accurate rather than aspirational.
* If it is ever wanted, the cheapest honest version is already specified by the
  routes above: **explicit user action only, `https` only, a fixed timeout, no
  refresh, and SPEC's existing degradation to a plain link** — with the title
  frozen at fetch time, because there is no owner to invalidate it.
* The dependency would be `rustls` (or `native-tls`) and would have to arrive in
  a milestone of its own; it must not ride along with a renderer change, which
  is why it is not being taken now.

## ADR-0066 · The bulk replace keeps the database layer, and a record dies with the page the incoming state dropped

Decision: `replace_all` — the checkpoint, the repair, the LAN pull — replaces the
**document**: pages, blocks, metadata, settings. The database layer is not part
of `PersistedState`, and it does not become part of it. But `DELETE FROM pages`
**cascades** through `db_records.page` (ADR-0063's foreign key), so the six
tables are read into a snapshot inside the same transaction, before the delete,
and written back after the new state is in. The rule for what comes back:

* a database, its properties and its views always survive untouched — the bulk
  path never knew about them and may not quietly lose them;
* a record survives if it is bare or if the state still has the page it is the
  face of;
* a record whose page the incoming state dropped is dropped with its values and
  its list items, which is ADR-0063's invariant ("a database never holds a row
  whose page is gone") applied to a path that arrives from SQL rather than from
  a command.

Why not the alternative of adding records, values and items to `PersistedState`:
that state is what a checkpoint *is* and what the LAN pull puts on the wire
(`services::lan_server` sends a `PersistedState`). Filling it with records means
every row and every cell of every database travels through a checkpoint's memory
and through the wire format — the exact cost ADR-0067 exists to prevent, and one
that grows with the tables the bulk path is supposed to know nothing about. The
bulk path's job is to replace prose; the cascade is the only reason it is
involved with rows at all, so the fix belongs at the cascade and not in the
state.

Consequences:

* The bulk path now runs two more statements (a snapshot read and a restore
  write). It is a rare, already-expensive path: a checkpoint walks every block
  and every page in the library.
* A record deleted this way is deleted *whole*: its title lived on the page that
  went, and nothing resurrects it as a bare record with an empty title (that
  would be ADR-0063's rejected alternative, arriving through a second door).
* `attachments` and the database layer are now the two things `replace_all`
  deliberately carries across; the comment in `replace_all` says so beside the
  attachment one, because the next reader will ask which rows survive.
* Still unverified: no test drives a **LAN pull** of a library that has records
  (`services::lan_server` is off by default and its own slice's tests use states
  without databases), so the rule above is tested against `replace_all` directly
  and against the pull only by construction; and the snapshot is held in memory
  for the duration of the transaction, so a library with a million records pays
  for it — bounded by the same rebuild the bulk path already does, but not
  measured.

## ADR-0067 · A row exists only inside a window, so the store never loads records

Decision: the layer that reads is the layer that is asked for a window.

* `load_databases` — the startup read — carries databases, properties and views.
  It deliberately carries **no records and no cells**: those are the two things
  whose size is a table's and not a schema's.
* A view reads rows through `window_rows`, which runs the window D0's projection
  computed (`core::database::window`) as the query's `LIMIT`/`OFFSET`, and the
  number of rows that come back *is* the window's length. `record_count` (one
  `COUNT(*)`) is what the window needs first; D4's filters narrow the same pair
  rather than adding a path.
* The list-valued columns (`multi-select`, `files`) are read in a second query
  bounded by the window's own records, because one row per item would otherwise
  multiply the window by the longest list in it.
* `unwindowed_rows` exists, is documented as **the control arm of the
  measurement** and has no reader in the app. ADR-0065's Markdown export is the
  one future caller that has a viewport-free reason to ask for a whole table, and
  it should ask for a streaming read instead.
* A cell read on its own (`cell`) is a debug and test path; a view reads cells
  with its window.

Why: SPEC §三十九's first red line — 「10 000 行的库不得全量 realize；视图先算可见窗口再取行」.
D0 proved the projection exists and is bounded by the viewport (31 rows of
10 000). A projection is only half the claim: if the load path materialized
records, then ten databases of 10 000 rows would become 100 000 objects at
startup and the window would be decoration. Making the window the *only* read
path is what turns "we compute a window" into "the store is asked for a window".

Consequences:

* The write path does not change shape: a cell is one `Change` (`CellSet`), a row
  is one `Change` (`RecordCreated` / `RecordDeleted`), and the app never holds a
  row object it has to keep in sync — which is also why D3's commands must
  capture the rows they delete in order to undo them.
* Counting is now on the read path: every window read starts with a `COUNT(*)`
  for the same database. It is an index walk (`idx_db_records_db_ord`), and it is
  measured (D1's report) rather than assumed.
* Keeping rows out of `PersistedState` is what makes ADR-0066's snapshot the
  smallest possible exception rather than a design change.
* Still unverified: no UI calls any of this yet, so "the only read path" is a
  statement about the API and not about a frame; the Markdown export has not been
  written, so nothing yet proves that an export of a 10 000-row database is
  acceptable; and the count's cost is measured on an unfiltered database only
  (D4 owns the filtered number).
## ADR-0068 · A record's two instants are columns of the record, and no cell is ever written for them

Decision: `created time` and `last edited time` — SPEC §三十九's last two kinds —
are projected from two columns of the row itself, and those columns are the
**only** source of those cells:

```sql
ALTER TABLE db_records ADD COLUMN created TEXT NOT NULL DEFAULT '';  -- step 17
ALTER TABLE db_records ADD COLUMN edited  TEXT NOT NULL DEFAULT '';
```

The shape is ADR-0062's stored date: `YYYY-MM-DDTHH:MM`, local wall time,
sixteen bytes of ASCII, `''` for "not known". Fixed width is what makes these two
kinds sort by the same rule every other date-shaped value does (bytes are
chronological bytes), and it keeps a calendar out of Rust entirely. **The write
path stamps them, and it is the only thing that does**: `insert_record` runs
SQLite's `strftime('%Y-%m-%dT%H:%M','now','localtime')` inside the `INSERT`
itself, and `set_cell` / a page rename move `edited` with the same expression.
Nothing takes a time from a caller, and `Record` — the struct a `Change` carries
— has no field for either instant, so no change can name a birthday it invented.

Refresh timing, which is the whole of the rule:

| write | `created` | `edited` |
|-------|-----------|----------|
| a new record (`RecordCreated`) | stamped, once | stamped, the same instant |
| a cell (`CellSet`), including a write that clears one | untouched | moved to now |
| the title of the page it owns (`PageTitleSet`) | untouched | moved to now — a page-backed record's title *is* `pages.title` (ADR-0063), which is why `repository::apply_one` calls back into the store for this one |
| its place in the listing (`RecordOrdSet`) | untouched | untouched |
| pointing it at a page (`RecordPageSet`) | untouched | untouched |
| any read | untouched | untouched |

"Content" is the rule behind that table: a record's content is its cells and its
title; its place in a listing and which page it points at are its *frame*.
Dragging a row is not editing it, and Notion's own behaviour agrees.

Why not a `db_values` row (which ADR-0039 forbids for derived data, and which
ADR-0062 predicted would not be where these land): "last edited" cached as a cell
would have to be *noticed* by the same write path that changes the cells it
describes, and the first write path that forgot would leave a stamp that lies —
the classic double write. So the read path never consults `db_values` for these
two kinds at all: `cell()` and the window query read `db_records`, and a row
written at a derived column by a caller that ignored the contract is simply not a
value any read consults. The D2 test writes such a row and watches it be ignored,
rather than pretending the write is impossible.

Consequences:

* Two `TEXT` columns on `db_records`, and one extra `UPDATE` per cell write: D8's
  "cost of editing one cell" number carries it.
* A step of its own (v17) rather than a line added to v14, because a v14 file in
  the wild must keep meaning what it meant; a v17 file with no stamps shows empty
  cells rather than 1970, and no upgrade invents a birthday.
* `SELECT`s and the bulk path carry them: `snapshot_tables` / `restore_tables`
  keep both columns, so a checkpoint, a repair or a LAN pull that keeps a record
  keeps its birthday (ADR-0066's rule applied to two more columns).
* Minute resolution, and a *redo* of a creation stamps a new moment — the row
  really was made again.
* Still unverified: no view draws these, so "it moves when a user expects it to"
  is a statement about statements and tests (every row of the table above is
  asserted); and the stamps are one machine's local wall time, so a file carried
  across time zones reads the strings it was written with — ADR-0062's rule for
  dates, and *not* what Track 2's UTC date *atoms* do (`core::date`). The
  integrator may want one answer for both.

## ADR-0069 · A cell is typed on the way in and painted on the way out, and the three string kinds are never rewritten

Decision: every one of the fourteen kinds gets its input rule and its paint rule
written as code (`core::database_property`), and no other module decides what a
cell means:

* **Empty input is the absence of a value**, for every kind: `parse_one` answers
  `CellValue::Empty`, which is the absence of a row (ADR-0062). `Text("")` stays
  reachable for a writer that says so on purpose; the two paint the same.
* **Text is stored verbatim** — no trim, no length cap, no case folding — because
  a space can be the content. Every other kind trims before it parses, because
  `" 2 "` is a number someone typed.
* **`url` / `email` / `phone` are never rewritten and never refused.** Nothing is
  normalised (`HTTP://Example.COM` stays), nothing is rejected (`not a url`
  stays), and `looks_valid` is a *hint* a cell editor may dot a cell with. The
  alternative — a gate — turns a notebook into a form, and this app has no web
  runtime to make a link mean anything anyway (ADR-0001).
* **`number` is parsed or refused by name**: finite decimals, either sign,
  `e`-notation; `inf`, `NaN`, `2,5` and prose are refused with the text they came
  from. `f64::from_str` accepts the first three, and a NaN in the `num` column
  would sort by bit pattern and compare as nothing.
* **A date's gate is the stored *shape*, not the calendar**: `YYYY-MM-DD` or
  `YYYY-MM-DDTHH:MM`, zero-padded, month/day/hour/minute in range. `2026-02-30`
  is stored as typed (this is a notebook, not a scheduler); `2026-9-2` is
  refused, because a date column whose bytes are not fixed width sorts wrong —
  the one thing ADR-0062 buys with the ISO form, and the D2 test writes an
  unpadded date past the parser to watch the order break.
* **A select/status cell stores an option's id**, never its label (ADR-0061), and
  a name the column does not list is *refused* rather than invented: an option
  list is a second change in the same batch, and `PropertyOptions::option_named`
  is the get-or-add the caller wants. Renaming an option is then one edit of the
  document that touches no value — the D2 test renames one and shows the stored
  value untouched.
* **`files` stores attachment ids** — ADR-0029/ADR-0030's one attachment channel,
  never a second copy of the bytes — and paints the *name* out of `attachments`.
* **Two folds, both visible rather than silent**: an option id the column no
  longer lists paints **itself**, and a file id whose attachment row is gone
  paints **itself**. A blank cell would say the value was never there.
* **Settings are read from `config`, and an unknown setting folds to the
  default**: `NumberFormat` (`plain` / `integer` / `percent`) and `DateFormat`
  (`date` / `datetime`, whose default depends on the kind — a stamp shows its
  minute, since two rows written the same day must not look identical). The
  stored text is the truth either way: a `datetime` cell holding a date prints
  ten bytes rather than inventing `00:00`.

Consequences:

* `CellValue::display` stays as the *value's* own form, and `paint` falls back to
  it, so the two can never disagree about a number; a kind with settings goes
  through the column.
* A cell's value is checked where it is *written* (the input path), not inside
  `set_cell`: ADR-0062's typed columns are chosen by the caller's shape, and
  putting a `SELECT` on the cell-write path is the cost D1 refused and D6 will
  measure. The store's tests therefore include writing a value of the wrong shape
  and reading the empty cell that results — the contract is a test and not a hope
  (D1's ADR-0062, restated here because D2 is where it can bite).
* Still unverified: no cell editor exists, so these rules have no UI caller yet,
  and `looks_valid`'s thresholds are the author's — nothing measures how often
  they disagree with what a user meant.

## ADR-0070 · A sort is an `ORDER BY` in the statement, and the blanks are placed by a term of their own

Decision: §三十九's "filter and sort happen in SQL, not in the UI" is a *shape*
here and not a discipline: `RowRequest::sort` is a compiled term
(`SortSpec { property, column, descending }`) that the row query turns into its
`ORDER BY`, and **nothing in this crate sorts a `Vec` of rows.** This slice
compiles one term; D4's view document may name several, and that is the slice
that grows the term into a list.

Which column the comparison runs in is a decision per kind, and the decision is
the point:

| kind | column | what that buys |
|------|--------|----------------|
| `number` | `db_values.num` (`REAL`) | `2` sorts before `10`; a text column sorts `10` first, and the D2 test shows both orders side by side |
| `date`, `created time`, `last edited time` | `text` / `db_records.created` | bytes are chronological *because* the stored shape is fixed width (ADR-0062) |
| `title` | ADR-0063's `COALESCE(p.title, t.text)` | a page-backed row's title is the page's, value row or not |
| `checkbox` | `flag` | `false` before `true` |
| everything text-shaped | `text` | bytes — the order a user sees in a sorted list of words |
| `multi-select`, `files`, `formula`, `rollup`, `relation` | — | `SortSpec::of` answers `None`: "sort by a multi-select" is a question about the column's *options*, and no fallback would be honest. Silently ordering by row position would look like it worked |

The blank rows are placed explicitly, always last, in both directions:

```sql
ORDER BY (v1.num IS NULL) ASC, v1.num ASC, r.ord, r.id
```

for the nullness kinds, `(v1.text IS NULL OR v1.text = '')` for the text ones (a
`Text("")` the user blanked is as blank as an absent row), and `r.created = ''`
for the two stamps. SQLite puts NULLs first, and "the rows with no number floated
to the top" is not what anyone means by "sort by number" (ADR-0062's rule); the
nullness term is always ascending, so a descending sort turns the values around
and leaves the blanks where they were. The last two terms are the tie-break: the
database's own listing order, ascending, so equal rows always come back in one
order and a re-read of the same window is the same rows.

A column the view *hides* gets a join of its own (`s0`) rather than being
unsortable: a view document may sort by one (ADR-0064), and one extra index probe
per row is the price.

Consequences:

* A window is a **slice of the order**: `LIMIT`/`OFFSET` apply to the sorted
  result, so the second page of a sorted read is the second page of that order.
  Slicing in Rust and sorting afterwards would be the wrong rows; the D2 test
  asserts the slice is the sorted slice.
* The order costs one temp B-tree per read (no index serves a `LEFT JOIN`'s order
  for every row), and the plan says so: `EXPLAIN QUERY PLAN` is the evidence, the
  D2 test asserts the temp B-tree while no `db_values` scan appears, and the D2
  probe prints the plan beside its timings.
* A sort by a hidden column also costs a join per row that the visible read did
  not have — which D4's compiler may weigh when a view offers both, a decision it
  can make because the term is data.
* Still unverified: no view compiles a `SortSpec` from ADR-0064's JSON yet, so
  the multi-term and group-header shapes are unnamed; and nothing yet measures a
  *filtered* sorted read (D4's number).

## ADR-0071 · `person` is a name in a text cell, and the member list is those values

Decision: SPEC §三十九's 降级 for `person` is taken literally and kept small:

* **There is no member table, no member id, and no account.** A person is a
  string in the `text` column, exactly as ADR-0061 folded it: `PropertyKind` has
  no `Person` variant, and `from_stored("person")` answers `Text`, so a library
  written by a build that knows `person` opens here with the column drawing as
  text.
* **The workspace's local member list is derived, not stored**:
  `SqliteRepository::workspace_people()` reads the distinct non-empty values of
  the columns whose *stored* kind is `person`, in `NOCASE` order. The predicate
  is the stored string precisely because the fold happens at load: SQL still sees
  the word the file was written with, and the Rust side has no variant to hang
  behaviour on. Nothing is copied, so nothing goes stale, and nothing needs
  merging when two spellings of one person appear — they are two names, which is
  what a plain string means.
* **Renaming a person is editing a string.** There are no ids to reconcile and no
  cascade to run, which is the whole argument for the degradation: an account
  model would bring a table, a picker with state of its own, merge rules for
  duplicates and a permission question, for zero extra data.

Consequences:

* A file this build creates has no `person` column at all (nothing here can write
  the kind), so `workspace_people()` answers empty for it until a later build
  grows the variant — and the day it does, the same query answers for its cells.
  The D2 test writes the column the way such a build would leave it, pinning the
  fold and the list together.
* The list is one string per cell, so it reads `db_values` and not
  `db_value_items`; a multi-person kind would add a `UNION` and nothing else.
* Still unverified: no picker consumes the list, so its cost (one `DISTINCT` over
  the file) is unmeasured; and "two spellings are two people" is a decision a UI
  may soften with a case-insensitive match — a UI decision, not this one.

## ADR-0052 · synced block：源块持有内容，镜像只持有一根指针

**状态**：已决定
**日期**：2026-09-22
**驱动**：SPEC §四十 末句「synced block 建在这一层之上：一个块被多处引用，编辑任意一处全部生效」+ SPEC §三十七 批次 C（依赖 §四十）+ SPEC §三十九（环检测在**保存时**做，不在渲染时）

---

### 上下文

§四十 的引用基础设施已经落地，并且验证了同一条纪律的三种写法：

| 引用 | 存的是 | 画的时候才解析的是 | 由谁定 |
|---|---|---|---|
| `@page mention` | `marks.url = "quire://page/<id>"` | 目标页的**当前**标题 | ADR-0050 |
| `Page` / `Link to page` 块 | `blocks.page_ref = <id>` | 同上 | ADR-0026 |
| 反向链接面板 | **什么都不存** | 谁在指本页 | ADR-0051 |

三者的共同点是：**渲染时解析，不做第二次写入**。synced block 是同一条纪律的第四种形态，
也是它第一次作用在「块的内容」而不是「块的标题」上：一份内容出现在两个位置。

它动摇的是那个最核心的假设 —— **谁拥有这份内容？**

---

### 决策

**一块内容只有一份。`Synced` 块自己不持有文本，它持有一根指向源块的指针。**

```sql
blocks.sync_ref  INTEGER NULL   -- NULL = 没有源（刚建还没选源，或源已被删）
```

一个 `Synced` 块的形状：

```rust
kind     = BlockKind::Synced     // as_str() == "synced"，UI int 24（23 是 Database，编号跟在它后面）
sync_ref = Some(source)          // 源块的 BlockId
text     = ""                    // 恒空：写它就违反本 ADR
```

**为什么是「引用另一个块」而不是「同一个块出现在两处」**：`blocks.page` 是单值的，一个块
只能属于一页。「同一个 BlockId 出现在两个位置」在关系模型里没有落脚点，除非再引入一张位置表
—— 那是把整个编辑器的地址规则重写一遍，只为了省一根指针。前者照 ADR-0026 `page_ref`
的形状，几乎不新造东西。

**为什么只同步一个块，不同步整块子树**：Notion 的 synced block 可以是一棵子树。子树意味着
「行是动态的」—— `project_blocks` 要真删/插一段变长子树，§三十七 那两处附加改动（真删子树、
row→model 换算）全都要跟上。那是另一个量级的一刀。本刀先交单块版本，并且把这条限制**写在这份
ADR 里**，而不是塞进「已知问题」。

---

### 四个必须先回答的语义

#### 1. 谁拥有内容 —— 源块

镜像的 `text` **恒为空串**，任何写它的人都违反这份 ADR。它的代价也是空的：源块被删之后镜像
什么都不剩 —— 而这恰恰是想要的结果（见下）。

#### 2. 删除语义

| 删掉谁 | 发生什么 |
|---|---|
| **镜像** | 只有这一行消失。源块和它的每一个其它镜像原样 —— **没有外键、没有级联**。`sync_ref` 是一列整数而不是一个关系，级联会让「删掉一个视图」连带毁掉内容，这是本 ADR 最贵的一次拒绝。 |
| **源块** | 镜像**保留**，可见退化成「（源块已删除）」灰字，并且**变为只读**（解析不到源，编辑绑定就无处可落）。一次 `DeleteBlock` 不惩罚页面上别的任何东西。 |
| **源块所在的整页** | 同上。孤儿镜像是**可见的** —— 留着还是删掉由用户决定，系统不替他猜。 |

#### 3. undo 语义 —— 一次编辑，一步撤销

「两处同时变」听起来需要一个写两份的实现，于是听起来需要一个合并撤销的机制。**两者都不需要**：
既然只有源块持有内容，一次编辑**真的只写一处**。第二个位置的更新发生在下一次投影（投影不入库，
§三十八），所以一次 Ctrl+Z 撤的就是那一次写。

这是「没有第二份东西」买到的最实在的一件东西，也正是它比「双写 + 同步器」便宜的全部理由。

#### 4. 环检测 —— 在**建立链路那一刻**做，不在渲染时

`A → B → A` 是一个手就能改出来的状态。照 §三十九 对 relation 的要求，检测点在**保存**：

- 建立或改这条链路时（`AppState::set_sync_source`）：从候选源沿 `sync_ref` 走最多
  `SYNC_CHAIN_MAX` 跳，中途碰到自己就**拒绝**（返回 `false`，UI 拿到一个没变的视图）；
  `source == self` 单独拒绝。
- 渲染时是**有上界的解析**（最多 `SYNC_RESOLVE_MAX` 跳），所以即使一个**旧备份被手改成环**，
  最坏情况也只是多走几跳，不会挂住。这条上界必须落在代码里的常量上、带注释 ——
  「反正环不会出现」是任何样本都证明不了的一句话。

---

### 存储

migration **19**（`src/storage/migrations.rs`）：

```sql
ALTER TABLE blocks ADD COLUMN sync_ref INTEGER;   -- NULL = 无源
```

**为什么是 19 而不是 17**：17 和 18 都是 Track 3 的 database 步（记录时间戳、`blocks.db_ref`），
其中一个已经提交（`d3e4a0a`）。迁移是本项目唯一不可逆的东西，两个打磨不同事情的 step 撞同一个号，
比临时跳号贵。
当前在 `track/2-references` 这棵孤立树里 17 是空的 —— 那两棵树合并的一瞬间它就是满的；
我不替整合者提前占用。

`block_children` 不动：一个同步镜像没有子节点。

---

### 六个接点（SPEC §三十七 的硬性约束，少一处即视为未完成）

| 接点 | 落在哪 |
|---|---|
| `core/types.rs` 的 `BlockKind` | `Synced` 变体（**加在枚举末尾**，永不重编号），`ALL` 变 24 项，`as_str() == "synced"` |
| storage 的 kind 与列 | `kind_to_int` / `kind_from_int` = **24**（23 是 §三十九 的 `Database`，它在枚举里排在前面）；新列 `sync_ref` 由 migration 19 加 |
| Markdown **导出** | **摊平**：镜像行导出成源块的那一行内容（照 ADR-0032 columns 的先例） |
| Markdown **导入** | **有意地不认新语法** —— 理由见下 |
| ⋮⋮ 的 Turn into | `TURN_INTO_ITEMS = SLASH_ITEMS`，那里加一行两者就都有了 |
| slash 菜单 | `SLASH_ITEMS` 加「镜像块」；`INSERT_ITEMS` 同样加 |
| 截图场景 | `synced` / `synced-source-gone` / `dark-synced` |

**导入为什么什么都不认**：导出摊平之后，同一份 Markdown 再导回来就是一段普通的文字。
这不是偷懒，是边界 —— **§二十六 把 Markdown 定成内容通道，不是保真格式**；而 block id
在库与库之间也毫无意义。给镜像发明一种记号（`<!-- quire:synced -->` 之类）只会多产出一种东西：
一个导进来立刻失去源、只能画成「（源块已删除）」的块 —— 比一段普通文字更糟。所以
**同步关系不跨这条边界**，而且这句话要说在 ADR 里，不能藏在实现里。

---

### 「行是动态的」那两处附加改动 —— 本刀用不到，但要写下来

§三十七 规定凡「行数会变」的块都要多改两处。一个同步镜像**没有子节点、占一行、行数恒定**，所以
① `project_blocks` 不需要删行；② 拿 row index 当 model index 用的地方不需要换算。
这两条是**在这一刀被判定的**，不是被漏掉的 —— 将来若把它升成「同步整棵子树」，
第一个要回头的地方就是这里。

---

### 代价

- 只同步一个块，不同步子树（Notion 的同步块可以是一整棵子树）。
- Markdown 往返丢同步关系。
- 每次投影为每个镜像行做一次 id 查 —— 一次哈希表查，不走 I/O，但它在**交互路径**上。
- 源与镜像同时在屏时，两行都会画成「正在编辑」的样子（两者绑的是同一个 id 的同一份编辑态）。
  这是我们想要的效果（Notion 也是两边一起动），但它的手感 headless 证明不了，见「未验证」。

---

### 未验证

- headless 场景证明不了**真键盘输入**、两个输入框之间的焦点争用、真点一次跳转。
- 「源被删 → 镜像只读」这条有单元测试钉住投影结果，但**没有**真的用鼠标点上去试。
- 性能：本刀给每次投影加了「每个镜像行一次查表」。它是 O(1) 查表而不是 O(n) 扫描，所以
  **没有欠量 RAM 闸的理由**；但**也没有数字**。若后来发现某页上有几十个镜像行，那个数字还欠着。

## ADR-0072 · The six tables' ids are session watermarks, seeded from the store and never re-read

Decision: the drawn layer allocates database ids from four counters in
`AppState` (`next_db_id` / `next_property_id` / `next_record_id` /
`next_view_id`), each seeded once at startup from the highest id its table
holds (`database_store::maximum`) and incremented only when the command that
was to spend the id was actually planned. `Command::MakeDatabase`,
`AddDatabaseRecord` and `AddDatabaseProperty` carry their ids **in** — the
plan layer can allocate block ids and nothing else, which
`Command::InsertImage` set the precedent for — so the counters are the one
place the drawn layer's ids come from.

Why not a `MAX(id)` query per creation: the write path is debounced
(`PersistenceService`), so two creations in the same batch cannot collide
even though neither row is in the file yet, and the app deliberately never
holds the rows of a 10 000-row database to find the highest one (ADR-0067) —
putting that question back on every click of "New row" is the exact cost the
window exists to avoid. The seed answers the only question that matters
("which ids are taken *before this session*") in one query per table at
startup.

Consequences:

* A refused command burns no id: the counters move only after
  `exec_all_on_open_page` returned changes, so `MakeDatabase`'s refusals (a
  cell, a container's child, a block that already has an entity) leave the
  watermark where it was.
* An id a session allocated and never wrote is forgotten at restart; nothing
  references it, so no gap is observable. A batch that wrote it is in the
  file before anything can point at it, because the change list that carries
  the reference carries the row.
* The bulk path does not disturb the watermarks: ADR-0066's snapshot carries
  the rows across, so the highest ids survive it, and the session's next
  allocation is re-seeded only by a restart.
* Still unverified: two processes writing one file concurrently is outside
  the model (a single-process app; the LAN share is read-only), so no test
  covers the counters against a foreign writer — a hand-edited library can
  collide, and the failure is the store's UNIQUE constraint, reported, not
  silent.

## ADR-0073 · Which view a block is showing is session state, not a column

Decision: `db_active_view` is a map in `AppState` (`RefCell<HashMap<i32,
ViewId>>`), written by `db_pick_view` and read by every projection; nothing
is persisted, and a restart opens the database's first view.

Why not a column on `db_views` (a `selected` flag): "which view am I looking
at" is a fact about a **window**, not about the document — two blocks may
show the same database and each is looking at its own view, so the fact is
per (block, session), which is exactly the shape a column cannot have. And
not a document change: making a switch a `Change` would cost an undo step
and a write for a fact nothing else depends on, and Ctrl+Z would move the
user's view back to a view they deliberately left. The document holds the
view *definitions* (ADR-0064); the session holds which one is on screen.

Consequences:

* The choice dies with the session. That is the recorded cost, and it is
  honest: a view switcher that remembered across restarts needs a place to
  remember *in*, which is a schema question for the milestone that also
  brings a second view (D5).
* Undo and redo never move it — the map is not in the change path, which is
  also why the in-memory catalog's fold (ADR-0075) has no arm for it.
* Switching invalidates the block's cached window (`db_windows.remove`): a
  different view has different columns, and a row set painted against the
  old ones must not survive the switch.
* Still unverified: no number for the switch cost — one view per database
  today, so nothing can be switched *to*; the number is D5's, with the same
  caveat the shared-tree session drift always carries.

## ADR-0074 · A view's definition is read, edited and written back as text, and the keys this build does not own pass through untouched

Decision: `core::database_view::ViewDefinition` keeps ADR-0064's document as
the parsed JSON it arrived as and rewrites exactly two keys — `columns` and
`widths`, the ones D3 owns. Every other key (`filter`, `sorts`, `groups`,
`v`) is carried through in the position it was found. `db_edit_definition`
in `AppState` is the only writer, and it is read-edit-write of the **text**:
the stored document is parsed, one edit function runs, the result is
serialised, and the whole text is what `SetDatabaseViewDefinition` carries
as its `from` and `to`.

Why: this build owns two keys and a later build owns the rest. A struct of
the fields this build knows would silently drop the keys it has no field
for — "hide a column" would quietly clear a filter — and re-serialising
only the known keys has the same effect with more code. The document is the
view's only copy of its rules (ADR-0064 put them there because SQL never
filters on them), so a writer that eats keys is not a round-off, it is data
loss.

Consequences:

* A document that does not parse degrades to "no rules" (ADR-0064's fold),
  and a *newly written* one is always an object with the two keys present —
  an empty `widths` map is stored as `{}` rather than removed, because "I
  own this key and it is empty" is a different statement from "I have never
  heard of it".
* The undo of a width drag is the previous **text**, so it restores a later
  build's key edits too — the `Change` carries the bytes, not a delta.
* A hand-edited width below the floor reads as the floor, and `0` reads as
  "auto" (an equal share); a drag can store neither, which is why the two
  cannot be confused.
* Still unverified: the pass-through is a property of the code's shape
  (`put` keeps unknown fields) and the fold is pinned by `core` tests; a
  round-trip test that drives a document with foreign keys through a D3 edit
  is on the final unified test's plan.

## ADR-0075 · The in-memory catalog learns the schema from change lists, and records never enter it

Decision: `AppState::db_absorb` folds every change batch the session records
into the `DatabaseCatalog` the read path consults: `DatabaseCreated` /
`DatabaseRenamed` / `DatabaseDeleted`, `PropertyAdded` / `PropertyRenamed` /
`PropertyKindSet` / `PropertyOrdSet` / `PropertyDeleted`, `ViewAdded` /
`ViewRenamed` / `ViewLayoutSet` / `ViewDefinitionSet` / `ViewOrdSet` /
`ViewDeleted`. The funnel is `record()` — the one place apply, undo and redo
all arrive — so no write path has to remember to teach the catalog
itself. Records, values and list items are deliberately absent: they are not
in the catalog at all (ADR-0067), and a row's life is a window's business.

Why: the catalog is what every projection reads, and until it learned, a
freshly made database existed in SQL and not in memory — its own block drew
ADR-0060's "(deleted database)" until the next restart, which is the kind of
defect that survives every unit test of the layers below it. Undo is the
reason this is a fold over changes and not code at the call sites: an undo
has no call site, and its batch must teach the catalog the same way an apply
does.

Consequences:

* **A change names what happened, not which direction it ran** — the same
  contract `core::document`'s apply/revert already runs on. `DatabaseCreated`
  always means "the row exists now", whether the user created it or undid a
  deletion, so the fold is idempotent (an insert that finds the row already
  there keeps it) and the catalog cannot disagree with storage about what a
  batch means.
* `DatabaseDeleted` cascades in memory exactly as `ON DELETE CASCADE` does in
  SQL (the entity's columns and views go with it); `PropertyDeleted` does
  **not** clean view documents, because no foreign key reaches inside
  ADR-0064's JSON — the compiler drops the ids it does not find (ADR-0074's
  unknown-key rule, applied to reads).
* A fresh batch that adds a property sorts the catalog by `(db, ord)` after
  the insert: `ord` is the schema's order (ADR-0061) and the store returns
  columns by it, so the in-memory order is what a restart would load.
* Still unverified: a LAN pull's change list reaches `record()` the same way
  (the bulk path's database snapshot is ADR-0066's, and is not a change
  batch), so the fold is exercised by the app's own paths only — a mixed
  replay test is on the final unified test's plan.

## ADR-0076 · A view's rules compile into the statement, and a filter that cannot be read is dropped with a visible note

Decision: SPEC §三十九's red line — 「filter / sort 在 SQL 侧完成，不在 UI 侧过滤」 — is a
**module boundary** in this codebase, not a discipline. `RowRequest` carries the view's rules
themselves: `sorts: &[SortSpec]` (D2's single term grown into the list ADR-0070 said D4 would
grow) and `filter: Option<&FilterNode>` (ADR-0064's recursive tree, parsed by
`core::database_view` against the schema). `storage::database_query` is the only module that
turns rules into SQL text — `WHERE r.db = ? AND (tree)`, one `ORDER BY` term per sort key
(blank-placement term per key, always ascending, then the value's direction, then
`r.ord, r.id` as the tie-break after the last key) — and `database_store` is the only module
that executes it. No function between the document and the window ever holds a row to throw
one away.

The count obeys the same boundary: a filtered view's window is computed from
`SELECT count(*)` over the **same** `FROM` and `WHERE` the row read runs
(`filtered_count`), so filtering a 10 000-row database down to 3 rows realizes 3 rows — the
count is SQL's and it happens *before* the window, which is the contract the unified test
must pin (see REPORT_TRACK3 §D4, the 对照 number: filtered window read vs. fetch-10 000-then-
filter-in-Rust).

A comparison's *column* is the decision `SortSpec` already made per kind (ADR-0070), and the
filter reuses it: `contains` is `INSTR(LOWER(expr), LOWER(?)) > 0` (no `LIKE`, so the value's
own `%` means itself; `LOWER` folds ASCII — the boundary every text search here has); a list
column's `has` is one `EXISTS` probe on `db_value_items`' primary-key prefix, exactly the
shape ADR-0062 predicted; `is any of` is an `IN` over the bound option **ids**; number
comparisons bind `REAL` and date comparisons bind the stored fixed-width text, so "before"
and "after" are byte comparisons because ADR-0062 stores dates fixed-width. `ne` is
`NOT (eq-form)`, so three-valued logic makes an empty cell match neither `is` nor `is not` —
"holds a value outside this one". A checkbox's `is unchecked` is `(expr = 0 OR expr IS NULL)`
— an untouched checkbox *is* unchecked — while `is not checked` is its negation, excluding
untouched rows. A clause whose value was never filled in (`FilterValue::Missing`, stored as
JSON `null`) compiles to `1`: an unfinished rule filters nothing, so "add a rule" cannot hide
rows before the user has said what the rule is.

**Degradation is a decision, not an accident** (ADR-0064 named two of these; D4 states all of
them):

* the `filter` document does not parse as a tree this build reads (a group whose children are
  not an array, nesting past depth 8) → **the whole tree is dropped** and the view draws the
  note — "This view's filter could not be read and was ignored." — where the row count would
  be, in the danger color. Not silent, not a crash, not a toast that outlives one frame: a
  filter that is not being applied is a fact about every row on screen.
* one clause names a deleted property, asks a comparison its kind does not have
  (`contains` on a number), or carries a value of the wrong shape (`next tuesday` as a date —
  the stored shapes are the only ones comparable, because fixed width is what makes bytes be
  time) → **that clause is dropped and counted** ("N filter rule(s) were dropped …"). This is
  ADR-0064's deleted-property rule applied to the other ways one clause can be unreadable; a
  `not` around a dropped clause is dropped with it, so a vanished rule cannot come back as
  its own negation hiding every row.
* a sort term or a group that cannot compile is dropped **quietly**: an order and a grouping
  are ways of looking at rows, never ways of hiding them, so the honest failure is visible in
  the first frame.
* an explicitly empty group (`{"and":[]}`, the panel's "no rules" state) is no filter at all
  and produces no note.

Why the panel edits a *flat subset*: one `and`/`or` root over clauses, each optionally
inverted, is the shape a popup with one rule list can honestly draw. A tree outside the
subset — a group inside a group — still **filters** (the compiler reads the full recursive
shape), but the panel **refuses to edit** it rather than reshaping the user's rules into
something it can represent; the refusal is a notice, and the table keeps filtering. Nested
groups wait for a panel that can draw them (D5's board editor), which is an honest smaller UI,
not a smaller compiler.

Consequences:

* D4 now owns three more keys of the definition document (`filter`, `sorts`, `groups`), and
  `ViewDefinition::set_filter/set_sorts/set_group` replace exactly those keys — ADR-0074's
  read-edit-write of the text, so a width drag still cannot eat a filter and a filter edit
  still cannot eat a width. An undo restores the whole document, all five owned keys together.
* The export follows the view for free: `db_markdown_table` builds the same `RowRequest`
  (filter + sorts) and runs the control read, which is ADR-0065's 「过滤排序照做」 finally
  having a rules compiler to mean. The group is *screen* furniture — a file has no viewport,
  so a grouped view exports its rows in the view's order without headers.
* `row_query`/`row_binds` (the tests' and the probe's evidence helpers) now return
  heterogeneous binds (`rusqlite::types::Value`), because a filter's binds are heterogeneous
  by design: an id, a `REAL`, a string, the window pair.
* Still unverified: **no test ran** (the slice's iron rule). The unified test owes: the
  filtered-window contract (filter to 3 rows, realize 3 rows), each op's predicate against
  hand-built rows (contains case-folding, `any-of` over ids, `ne`'s empty-cell exclusion,
  number order vs. byte order), the two degradation notes, the pass-through of foreign keys
  across a filter edit (ADR-0074's round-trip, now with rules on both sides), and the
  对照 number below. Also unverified: `INSTR`'s cost versus a FTS index for a 10 000-row
  contains-filter — the D2 note about adding `db_values(property, num)` indexes applies to
  filters too, and the probe prints the plan so the next slice can see what SQLite chose.

## ADR-0077 · Group by is an entry projection over an option-bounded column, and a header is never a row

Decision: grouping is **one column**, and the column must be one whose distinct values the
schema already bounds — `checkbox`, `select`, `status` (plus "no value"). ADR-0064 stores
`groups` as an array so a later build can nest; this build reads its first entry and only
that. The grouped view's scroll surface is a list of **entries** — per group, one header
entry, then its rows — and the window D0's `core::database::window` computes over
`total_entries = Σ(count + 1)` is mapped onto queries by `core::database_view::group_window`:
the headers that fall inside the window, and one `(group, skip, len)` slice per group that
overlaps it. Each slice is fetched with its own `LIMIT`/`OFFSET` **inside the group**
(`row_query_in_group`: the group's predicate joins the `WHERE`, the view's filter and sort
compile in as always), so a group holding all 10 000 rows realizes the same 31 rows it would
ungrouped, and a group costs **one entry, never one row per group**.

The group list itself is one `GROUP BY` query over an option-bounded column
(`group_counts`), normalized in the store into `GroupKey::{Empty, Option(id), Checked,
Unchecked}` — SQL's `NULL` and `''` are one group, and a checkbox's `0` and its absence are
one group, because an untouched checkbox is unchecked. The list is **unordered in SQL on
purpose**: the order a user means is the schema's own option order (ADR-0061), which lives in
the column's config JSON where SQL cannot see it, so the few headers are ordered in Rust from
that same config (known options in config order, an id the config forgot in byte order after
them — ADR-0069's fold applied to a header — a checkbox unchecked-then-checked, "No value"
last). That is ordering a handful of headers, not the red line bent: the rows are SQL's, each
slice from its own ordered query.

Why the kinds are restricted: a group header is an entity the view has to place in the scroll
surface, so the list of headers has to be small enough to compute **in full** — that is what
makes the header walk O(groups). Grouping by `text` or a date would make the group list as
long as the table (10 000 headers to realize is exactly what "group by must not become
10 000 rows" forbids); grouping by a number needs buckets ("what are the buckets" is a
different question, and guessing it would be inventing a histogram nobody asked for). The
picker offers only the bounded kinds and refuses the rest by name.

Consequences:

* The delegate's rows model carries both shapes: `DbRow.header` is a group header's label
  ("" for a data row), so the same window arithmetic, the same `db-row-start` row→model
  conversion (§三十七) and the same block height serve grouped and ungrouped views. The count
  text counts entries when grouped — honest, if slightly odd wording at 10 000.
* A group edit goes through the same `SetDatabaseViewDefinition` batch as every other rule
  edit: one change, one Ctrl+Z, the catalog learns it through `db_absorb` (ADR-0075), and the
  window cache invalidates on the definition text.
* Still unverified: no test ran. The unified test owes: a grouped read of a 10 000-row
  database realizes headers + one window (the entry count is `Σ(count+1)`, asserted against
  the group list), scrolling across a group boundary fetches only the groups the window
  touches, a value whose option was deleted groups under its own id, and the "No value" group
  contains exactly the rows the `IS NULL OR ''` predicate admits.

## ADR-0078 · Each view layout windows its own unit, and the calendar's fold is the same red line in a grid

Decision: SPEC §三十九's six layouts after the table are **one entity's layouts**, and what a
layout changes is what the window is computed *on* — one answer per shape, all of them ending
in "a count from SQL, then exactly the window's objects":

| layout | the window's unit | the count it comes from | what is realized |
|--------|-------------------|-------------------------|------------------|
| table | a row (or, grouped, an entry — ADR-0077) | `COUNT(*)` / the group counts | the viewport's rows, 31 of 10 000 |
| list | a row (44 px: title + two preview cells) | the same | the same |
| board | a **card slot** — one horizontal band across every column | `max(group counts)` over one `GROUP BY` | per column, its own slice of the band (`board_window`) |
| gallery | a **card row** (`per_row` cards) | `COUNT(*)` | one slice of `per_row × rows` cards |
| calendar | a **day cell** (the grid is fixed 6×7) | one `GROUP BY` over the date column for the month (≤ 31 keys) | ≤ `CALENDAR_PEEK` records per day, the rest folded into the cell's count |
| timeline | a **lane** (one dated record) | `COUNT(*)` with an `is not empty` clause on the date column | the viewport's lanes; one `min`/`max` query for the axis |
| form | — (the field list is the schema's size) | `COUNT(*)` for the count text | no records at all: it creates one |

Three consequences that are decisions and not implementation details:

1. **Board columns are the group list, not a second read.** A board *is* a grouping seen
   horizontally, so it reuses D4's `groups` key, its `GROUP BY`, and its header labels; the
   columns are realized in full (a handful of small rectangles from an option-bounded column)
   while each column's *cards* are fetched through a slice of the slot window. A column
   holding 10 000 cards realizes the same handful the ungrouped table would. Board's fallback
   grouping (the first option-bounded column, when the view has no `groups` key) is **not
   written back** — a default the user never chose must not become a rule they have to undo.
2. **The calendar folds by day, and the fold is the virtualization.** A day with 500 records
   realizes three and says "and 497 more"; both numbers come from one `GROUP BY` over the
   month, whose key count is bounded by the calendar itself (≤ 31 days, plus each stored shape
   of a day — a `2026-09-22` and a `2026-09-22T10:00` are two keys and one day, so the counts
   are folded by *day*, not by key). This is why a **date column may group here** even though
   D4's picker refuses it: a header per day is 31 objects; a header per text value would be
   the table. The month's records are read through day-range clauses (`>= the day`, `< the
   next`) compiled by the same clause compiler the panel drives — bytes-are-time (ADR-0062)
   doing the date arithmetic in SQL, in one place.
3. **The timeline's axis is one aggregate, and undated rows never enter the statement.** The
   lanes are windowed like rows; the axis under them is `min`/`max` over the *same* predicate
   (one scan, no rows); and 「无日期不显示」 is an `is not empty` clause ANDed with the view's
   own filter — SQL's row set, not a Rust `retain`. A row with no date is not fetched and then
   dropped; it is never fetched. The bar's two ends are day numbers computed in Rust from the
   painted date cells (a painted date always starts with the stored day), so a bar costs no
   second query per row; a missing or earlier end makes a point («起=止=同一天时画点»).

Where the views' own settings live: **one new document key pair** —
`date` (which column is the time axis) and `end` (the optional partner) — in the same
`db_views.definition` JSON ADR-0064 defined. One key serves both the calendar and the
timeline because they ask the same question ("which column is this view's time axis"), and
the schema's own first date column is the fallback when the key is absent (never written
back). No new table, no new column, no new migration step; the gallery's cards-per-row is
**session state**, because it is a fact about the window's width — the delegate reports it
(`db-gallery-shaped`) the way it reports the scroll anchor.

Consequences:

* `LayoutSupport` now draws seven of the eight layouts; `chart` stays refused by name until
  D7, and the switcher's `+` menu lists it as its own muted row rather than hiding it.
* Every layout's geometry lives in `core::database_view::layout_metrics` (row height + header
  height) and its surface height is computed once, in Rust (`DbWindow::body`), so the block's
  height, the delegate's placement and the window arithmetic cannot disagree — the D3
  row-height-zero bug class is closed by construction for all seven.
* The window cache key grows two fields (`layout`, `stamp`): the calendar's month and the
  gallery's per-row change which model the same view, document and total would produce.
* Still unverified: no test ran. The unified test owes: a board of 10 000 cards realizes
  `columns × viewport` cards (not cards), a 500-record day realizes three and reports 500, a
  filtered month's `GROUP BY` counts sum to the count line, a timeline excludes undated rows
  in SQL (the statement text shows the clause), and the `date`/`end` keys survive an unrelated
  rule edit (ADR-0074's read-edit-write).

## ADR-0079 · A view is created by one change, a record's page is minted by opening it, and chart is refused by name

Decision: the switcher's `+` adds a row to `db_views` through **one new command**
(`Command::AddDatabaseView { block, view }` → `[Change::ViewAdded]`, revert `[ViewDeleted]`),
carrying the whole row because the plan layer can allocate no ids (`MakeDatabase`'s rule
applied to the one table the app keeps out of memory). The name defaults to the layout's own
label and the `ord` is past the last view, so a new view lands at the switcher's end; the app
then **switches to it**, because a view created and not looked at is half a gesture. `chart`
is **refused with a notice** rather than created: a menu row that made a view this build
cannot draw would be a promise the switcher has to un-draw on the next frame, and D7 owns that
layout.

Opening a record — the board card click, the list row click, the gallery card click — is one
gesture with two halves, and both are ADR-0063's lazy page finally getting its UI trigger:
a page-backed record navigates; a **bare record mints its page now**, named after the
record's title, as a child of the page the database sits on, in **one batch**
(`[PageCreated, RecordPageSet{page}]`) — one Ctrl+Z, and the 「打开」 half of ADR-0063 that D1
tested at the storage level is now reachable from the UI. The parent is the open page rather
than the sidebar root because that is where a Notion database's rows live, and the record's
title is read from the row's own value (a bare record's title has its second home in
`db_values`, ADR-0063), falling back to "Untitled".

Consequences:

* The insert menu's four remaining database placeholders (`Board` / `Gallery` / `List view` /
  `Calendar` / `Timeline`) are **still muted**: lighting them means teaching the insert path
  "a database whose first view is X", and this build's one honest path for that is the
  switcher's `+`. The placeholders stay a promise the roadmap is readable off, and the report
  says so rather than pretending otherwise.
* Chart's row in the `+` menu is visible and inert (its own wording), so the menu never opens
  a view the delegate would have to draw as "not in this build yet".
* Still unverified: no test ran. The unified test owes: `AddDatabaseView`'s undo removes the
  row (and a redo puts it back with the same id and `ord`), a second view of a database does
  not disturb the first's document, opening a bare record leaves a page whose title is the
  record's title and whose parent is the database's page, and undoing that open leaves the
  record bare again.

## ADR-0082 · A formula stores its expression in its own config, computes every value on the way out, and is a pure lexer plus a hand-written interpreter

Decision: the `formula` kind (ADR-0062's first computed kind to get an engine)
keeps its **expression** in the one JSON document ADR-0061 gives every column
(`config`, key `"formula"`), written through one new change
(`PropertyConfigSet { id, config }` — the document **replaced whole**, the
read-edit-write discipline ADR-0074 applied to a column instead of a view; the
command layer's `SetDatabaseFormula` carries the whole before/after documents,
so one expression edit is one change and one Ctrl+Z, and the keys this build
does not own pass through untouched). The **value is stored nowhere** — SPEC's
「不存值，投影时现算」 is ADR-0062's own note and ADR-0039's discipline: a
`db_values` row for a formula cell would be a derived copy, and the only thing
that can go stale is everything.

The engine (`core::database_formula`) is what SPEC's sentence demands: **纯词法
+ 自写解释器, no JS / WASM runtime, no formula-parsing crate** — a lexer, a
recursive-descent parser and a tree-walking interpreter in a few hundred lines
of Rust, with no dependency added. Its shape:

* **Four types plus one absence** (`Val`): number, text, boolean, date (a
  stored fixed-width ISO text, ADR-0062), and `Empty`, which is contagious —
  an operand with no value makes the result empty, the same 「空」= 没有行
  rule ADR-0062 states for storage. The two exceptions are explicit: `if`
  short-circuits (only the taken branch evaluates), and `text(Empty)` is `""`.
* **No implicit conversions, in these places**: `"Total: " + [Points]` is a
  type error, not a concatenation; `length([Points])` is a type error;
  `min("a", 1)` is a type error. The one **explicit** conversion is
  `text(x)` (number → its `Display` form, boolean → `Yes` / `No` per
  ADR-0065, date → its stored ISO text, `Empty` → `""`). A text may be
  *compared* to a date — both sides are bytes, and ADR-0062's fixed width is
  what makes bytes chronological; that is the storage shape, not a conversion.
* **Seven functions**: `if(c,a,b)`, `length`, `round`, `abs`, `min`, `max`,
  `text`; arithmetic `+ - * /` (where `+` on two texts concatenates), unary
  minus, comparisons `== != < <= > >=`, `and` / `or` / `not`, literals
  `true` / `false` / numbers / `"strings"`, and `[Column]` — a reference to a
  column of **this row**, resolved by exact name at parse time against the
  schema, so an unknown name is a syntax error and a refused save rather than
  a blank cell.
* **Finite evaluation as constants, not as a promise**
  (SPEC: 「表达式必须有限求值」): `FORMULA_MAX_TOKENS = 2 048`,
  `FORMULA_MAX_DEPTH = 32`, `FORMULA_MAX_STEPS = 10 000`,
  `FORMULA_RESULT_MAX = 65 536`. The grammar has no loops and no
  user-defined functions, and the engine has **no clock and no I/O** —
  `today()` is deliberately absent, because a formula that read the clock
  would paint a different value on every frame for the same document.
* **The cycle check happens at save time** (SPEC: 「relation 环检测在保存时
  做，不在渲染时做」, applied to the engine this build has): a formula may
  name another *formula* column, and `would_cycle` walks the dependency graph
  **before** the change is stored, refusing a chain that comes back to itself
  while the text that would create it is on screen. The render path's only
  defence is the depth cap, which paints `Error` instead of hanging — that is
  for documents that never went through the save door, not a licence.
* **Two folds, both honest**: a `select` / `status` cell referenced by a
  formula reads as `Empty` (its stored value is an option *id*, and an id in
  arithmetic is worse than a blank), and so do `multi-select` / `files`. A
  cell whose formula cannot evaluate on its row paints `Error` — one word;
  the *sentence* lives in the editor's preview and error line.

Consequences:

* `sort_column` returns `None` for the computed kinds and `FilterOp::ops_for`
  is empty for them (D2/D4's placeholders) — now a **decision with a reason**
  rather than a gap: "sort by a computed value" in SQL means computing it for
  every row first, which is exactly what ADR-0083's red line forbids. A
  formula column sorts when someone writes its value into SQL, with an ADR
  for the materialization that takes.
* The config key is **removed** for an empty expression (ADR-0062's one
  representation of "nothing"), and a config that is not a document is
  replaced by one — there is nothing to preserve and an expression has to
  live somewhere.
* Still unverified: no test ran. The unified test owes every function and
  operator one evaluation test including the refusals, the empty-propagation
  table, `[Name]` resolution (exact match, unknown = refusal), all four
  budgets, `would_cycle`'s yes/no set, and the config round-trip
  (REPORT_TRACK3 §D6).

## ADR-0083 · The recompute unit is the window, the dependency is the row, and the counter is the contract

Decision: SPEC's red line 「formula / rollup 必须可增量重算，禁止每次输入全库
重算」 lands as three shapes, not as a discipline:

1. **Formulas are evaluated at projection time only, over the realized
   window.** `db_paint_formulas` (state) runs after `table_rows` on every
   layout branch through one shared `db_table_rows` — table, list, board
   cards, calendar peeks, gallery, timeline lanes — so a computed column
   costs "realized rows × visible formula columns" and nothing else. There is
   **no path that walks all of `db_records` to evaluate anything**, and the
   two features that would force one (sort, filter by a formula column) are
   refused upstream (ADR-0082). An edit of one cell re-reads the window the
   same way every other column's repaint does; the *evaluation count* after
   that edit grows with the window (≤ ~39 rows), never with `COUNT(*)`.
2. **Dependencies are same-row by construction.** The engine's cell callback
   has no record parameter and the adapter (`FormulaSource`) is built for one
   record — a formula *cannot* address another row even in principle. Editing
   cell `(r, q)` can therefore change a formula value only on row `r`, and
   only in columns whose parsed dependencies, transitively through other
   formula columns, name `q`: the set `{ (r, P) | q ∈ deps*(P) }`. Every
   other evaluation in the window returns, byte for byte, what it returned
   before — determinism is what makes "recompute the window" the same answer
   as "recompute the dependents", minus the bookkeeping.
3. **The counter is the number the test measures.** `db_formula_evals`
   (AppState) bumps once per projected formula evaluation and is read by no
   logic. The unified test's two numbers: **(a)** the counter's increment
   after one cell edit — it must equal the window's formula cells (window
   rows × visible formula columns), identical on a 10 000-row database and a
   5-row one; **(b)** the set of (row, column) whose *painted value* changed —
   it must be a subset of `{r} × {P | q ∈ deps*(P)}`. Both together are the
   red line stated as arithmetic: window-bounded work, dependency-precise
   effect, and no number in either grows with the table.

Consequences:

* **No cross-refresh value cache.** A cached painted value would skip the
  redundant re-evaluations, but its invalidation must cover every write path —
  cell edits, undo, redo, form submits, a LAN pull's bulk replace — and the
  funnel for those is `record()`, which database changes do not all pass
  through in every future shape. One missed path paints a *stale* value, which
  is worse than a redundant deterministic microsecond-scale evaluation. If
  D8's numbers say the redundancy matters, the clean place for a dirty set is
  the `record()` funnel (every batch, both directions) — an optimization with
  its own ADR, not a silent cache.
* **The Markdown export computes the whole view it renders** (ADR-0065's
  boundary made concrete): a file has no viewport, so `db_markdown_table`
  evaluates formulas for every exported row — an explicit artifact's own
  cost, and not the red line's subject (which is *input* latency). The
  dependencies come from **one indexed sweep per column**
  (`SqliteRepository::column_values`) expanded transitively, so the cost is
  O(rows × formula columns + dep columns × table) rather than one point read
  per row per dependency.
* The eval-time depth cap and the save-time cycle check (ADR-0082) are two
  halves of one sentence: the write path keeps user documents acyclic, the
  read path keeps documents that arrived any other way finite.
* Still unverified: no test ran, and no number was measured this knife (the
  counter exists; the probe that prints it is the unified test's first D6
  item).

## ADR-0084 · rollup and relation wait for §四十's reference infrastructure — their shape is written down, their code is not written

Decision: **D6 delivers `formula` only.** `relation` and `rollup` are
deliberately not implemented, per the brief's own rule — 「relation 用 §四十 的
基础设施（Track 2），不自己写一套 id 表」 — and per the check this knife ran
first: Track 2's reference layer (`src/core/reference.rs`,
`src/storage/backlinks.rs`, the `marks`-payload mention storage and migration
16's two indexes) is **uncommitted work in the shared tree** — untracked files
and uncommitted hunks, none of it in `HEAD`. Building a relation on top of
uncommitted infrastructure would either drag their files into this knife's
commit (forbidden) or fork a second reference mechanism (the exact thing the
brief forbids). So: nothing here touches references, and the shape below is
the contract the relation knife will implement against.

The shape, written down so the waiting knife does not have to rediscover it:

* **A relation column stores what the user picked — a *fact*, unlike
  formula/rollup's derived values — so it needs real storage**, which ADR-0062
  deliberately did not give it. The candidate that fits the existing shapes is
  `db_value_items` (one target id per row, the multi-select mechanism, so
  "is this record related to that one" is the same index probe a filter uses);
  the decision belongs to the relation knife, with its own ADR, once Track 2's
  ids are committable. What is fixed *here*: relations store **ids, never
  titles** (ADR-0051's discipline — a renamed target shows its live name),
  and a relation points at §四十's reference layer for its chips, its
  backlinks and its open-link path rather than at any table this track owns.
* **Two-way relations are one write, not two**: the forward change and the
  back-pointers land in **one change batch** (one Ctrl+Z), the way
  `DeleteDatabaseRecord` plans its values, record and page together. A
  half-written pair is not a state any undo direction can name.
* **Cycle detection at save time** — the same rule ADR-0082 applied to
  formulas, and the same reason: a relation cycle has no rendering order that
  saves it, so the write path refuses it while the user is looking, and the
  read path's depth cap exists for documents that arrived another way.
* **A rollup aggregates over the records a relation points at**: the six
  starting aggregates are `sum` / `count` / `min` / `max` / `average` / `none`
  over one target column of the related records; its configuration (the
  relation column and the target column) lives in the rollup column's own
  `config` document (ADR-0061's one bit of JSON per column, the same home
  ADR-0082 gave the formula expression); its values are **computed at
  projection time and stored nowhere** (ADR-0062), and its recompute follows
  ADR-0083's contract — which is why the contract was written for formulas
  first: rollup inherits it whole, with "the window" replaced by "the realized
  rollup cells".

Consequences:

* SPEC §三十九's 「需计算：formula / rollup / relation」 is one third delivered
  this knife; the report says so in its first section rather than burying it.
* The formula editor, the `PropertyConfigSet` write path, the projection hook
  and the counter are all relation/rollup-independent: nothing in this knife's
  code will need to be reshaped when they land, only added to.
* Still unverified: nothing to verify — this ADR is a decision and a
  handover, not a feature.

## ADR-0085 · A linked database is a second block with the same `db_ref`, and no second entity exists to drift

Decision: SPEC §三十九 「操作」's `linked database` — 「引用另一个库的某个视图，不复制数据」 —
is **not a new block kind and not a new column**. A linked database is an ordinary
`BlockKind::Database` block whose `blocks.db_ref` names a `databases` row that already exists;
`Command::LinkDatabase { id, db }` is the whole write (the block's kind and its pointer, in one
batch, with `BlockDbRefSet` reverted by undo and **no `DatabaseDeleted`** — the source entity
outlives every link, which is the one sentence that separates this command from
`Command::MakeDatabase`).

Why the shape is this thin. Every read and every write in this layer already resolves *through
the block's `db_ref`*: `db_ref_of` → the catalog (columns, views, definitions), the window read
(`RowRequest.db`), every cell write (`SetDatabaseCell` → `db_ref?`), the view switcher, the
filter/sort documents. So "it reads the source's data and the source's view definitions, and
writes land on the source" is not something this feature implements — it is something it
*cannot avoid*, because there is exactly one `databases` row, one set of `db_properties`, one
set of `db_views` and one set of records, and both blocks name it. There is no copy anywhere for
a linked database to drift from.

A new kind (say `BlockKind::LinkedDatabase`) was rejected for ADR-0060's reason: it would
duplicate all six of §三十七's integration points (kind string, Markdown channel, Turn-into,
menus, scenes, the delegate) to express a fact that is *the same* for both blocks — and the
delegate would then have to be kept in step between two kinds forever. A second pointer column
was rejected because it would be a second spelling of `db_ref` for the same question.

**The view half of SPEC's `(db, view)` sketch is session state** (ADR-0073), not a column: the
stored reference is the database; "which view" is what the block is showing *now*, defaulting to
the source's first view when the link is made. Pinning a `view` id in storage would add a second
dangling case (a view deleted while linked) with no honest rendering that the database's own
dangling state does not already have, and would fight ADR-0073's "which view is session state"
on the very blocks that most need to follow it.

Dangling: the source entity can die only one way — **undoing the block that created it**
(`Change::DatabaseDeleted` has exactly one producer, `MakeDatabase`'s revert; deleting a
`Database` *block* leaves the entity orphaned, which is today's behaviour for a single block
too). A linked block whose entity is gone resolves to `db_exists == false` and draws the one
muted line ADR-0060 already defines — `(deleted database)` — through the untouched existing
path. Deleting a linked *block* deletes nothing else.

Creation path: the slash / insert menu's `Linked view` row (`LINKED_VIEW_ROW = -2`, the picker
rows' negative-id convention from `MENTION_DATE_ROW`) switches the popup to a **database
picker** (`open_slash_links` — every live database by name, with its view count as the hint),
and the pick runs `db_make_linked`: the line gives up its words, the kind and the pointer land
in one batch, one Ctrl+Z. `plan` cannot see the catalog, so the *caller* checks the id is live;
if it died in between, the write still lands and the next projection draws the dangling state —
visible, not silent.

Consequences:

* Zero migration, zero new `Change` variant, zero new `Block` field: the pointer is v18's
  `blocks.db_ref`, already there. The linked database's cost is one command, two state helpers,
  one picker mode and one menu row.
* `MakeDatabase` already refuses a block with `db_ref.is_some()`, so a linked block cannot be
  "re-made" into an owner; `SetBlockType` refuses `Database` by design (D3). There is no
  user-facing "delete this database" action anywhere, so "who owns the entity" never has to be
  asked — and the ADR records that it is deliberately unrepresentable rather than guessed.
* Writes from a linked block land on the source: adding a column from a linked block adds it to
  the source's schema, editing a cell edits the source's record. That is what a linked view
  means, and it costs no code because it is the only row set that exists.
* Unverified (no cargo was run this knife): the picker's two-step flow end to end, the dangling
  rendering of a linked block, and two blocks of the same entity on one page's pixels. The
  unified test list (REPORT_TRACK3 §D7) names each.

## ADR-0086 · A record template is a copy of stored cells on the `databases` row, applied in the creation batch

Decision: SPEC §三十九 「操作」's 数据库模板 is **one JSON document in a new
`databases.template` column** (migration v20; `''` = no template), shaped

    {"cells":{"<property id>": <string|number|bool|array of strings>}}

with every value in the **exact shape `CellValue` stores** (ADR-0062's three columns plus the
items list). That is the discipline the brief states for templates on both tracks —
「模板是内容的副本、不引入第二套内容格式」 — spelled for a record: the store's own value shape
*is* the content format, so a template cell is a stored cell written down and applying a
template is the ordinary `SetDatabaseCell` write, parsed by nothing and converted by nothing.

Where it lives and why: a column on `databases`, not a table and not a per-view document
(ADR-0064's judgement, applied at the database level): SQL never filters on a template, so a
second table would be a join nobody runs; and a template is about the records a database
*creates*, which every layout creates the same way, so per-view would be a second copy of one
fact. The document is replaced whole by `Change::DatabaseTemplateSet` (the whole-document family
`db_views.definition` and `db_properties.config` already belong to) and rides the row through
load, insert, snapshot/restore (a checkpoint that dropped it would un-template every database —
ADR-0066's class of loss) and `db_absorb`.

Authoring: the row action slot's **T** (beside the row's x) runs `db_template_from_row`, which
reads that row's cells *as stored* — computed and derived kinds are skipped by kind, because
they have no stored value (ADR-0062/0068) — and writes the document in one change (one Ctrl+Z).
Copying an empty row makes the empty template, which is how the affordance clears one: a copy of
nothing is "prefill nothing", said the same way.

Applying: `db_add_record` and `db_form_submit` append the template's cells as ordinary
`SetDatabaseCell` commands **in the creation batch**, so a new row arrives complete and one
Ctrl+Z takes record and prefill back together. In the form, a field the user typed always wins;
a blank field prefills (an empty draft field is "not typed", not "cleared" — the form has no
clear gesture). A template cell naming a column the schema has since lost has no command to
name, which is the filter.

Consequences:

* No second content format, no second write path, no template *record* (Notion's gallery of
  template rows is a stored flag plus a projection rule this build does not need) and no
  per-view template — each mentioned here because each is the cheaper-looking shape this ADR
  refuses.
* The one thing a template cannot carry: a computed value (a formula's answer is not stored,
  ADR-0062) and a stamp (created/edited are the record's own columns, ADR-0068). A copied row's
  formula columns and stamps are whatever the *new* record computes — the honest answer.
* Version/undo: `DatabaseTemplateSet`'s revert names the previous document, so Ctrl+Z restores
  the whole template; a re-save of the same row is not an undo step.
* Seam with Track 1's page templates: **there is no shared shape to arbitrate** — a page
  template is a copy of a block sequence (their territory, their storage), a record template is
  a copy of cells (this column). Both follow the same *discipline* (a copy in the existing
  format) and neither introduces a format, but no type, column or function is shared, so nothing
  crosses the tracks. If the integrator later wants "a page template can seed a database row",
  that is a projection-time translation between two existing formats, and it needs its own ADR.
* Unverified (no cargo was run): the v20 column's convergence on a v19 file, the T affordance's
  pixels, the form's draft-wins path, and the prefill's undo as one step. Named in REPORT §D7.

## ADR-0087 · The view's search is a predicate in the same statement, not a turn of the global index

Decision: SPEC §三十九 「操作」's 视图内搜索 is compiled into **the window read's own `WHERE`** —
`RowRequest.search: Option<&str>`, one `INSTR(LOWER(expr), LOWER(?)) > 0` per text-bearing column
the request carries (the title through its `COALESCE`, plus text / url / email / phone / the
stored date / the two record stamps), OR'd, sharing one bind. The needle is **session state**
(`db_search` by block id; the header's count slot opens the box) — not a rule in the view's
document — so it is not persisted and not an undo step.

The rejected path, and why. §二十's index (ADR-0014) is an FTS5 mirror of **page titles and
block texts**: nothing indexes `db_values`, and a database's cells are not blocks. Routing view
search through it would mean either (a) indexing every cell into the mirror — a derived copy
with a write path on every cell edit, a prune rule on every bulk path (checkpoint, repair, LAN
pull), and a lag boundary measured in "which writers remembered to index", all to answer a
question the live predicate answers inside the statement the view was already running; or
(b) joining `search_blocks`' rowid list back into the row query — which searches *blocks*, not
cells, respects no view rule (a search must narrow the same membership the count and the group
headers see), and would make the count and the rows answer two different questions.

What the chosen path costs, said plainly:

* **`INSTR` is a scan, not a seek.** On a 10 000-row database a keystroke re-runs the count and
  the window read over the predicate — the same class of cost D4's `contains` filter already
  has, with the same honest remedy if it ever feels slow (a debounce on the callback; not a
  second row set in Rust).
* **ASCII-only case folding** (`LOWER`), the boundary every text search in this app has.
* **What is not searched**: numbers (their text form is the *projection's*, and comparing
  numbers is the filter panel's job), checkboxes, select/status (the stored text is the option
  **id** — the name lives in the config JSON, where SQL cannot see it), the two list kinds
  (their values are `db_value_items` rows) and the computed kinds (they store nothing). A view
  whose visible columns hold no prose therefore answers `0` rows for any needle — "nothing here
  can match it" said as an empty result, not as a full table.
* **No lag**, because there is no copy: a search reads the values the cell writes wrote, in the
  same transaction the writes flush into. That is the one boundary the FTS path would have
  introduced and this one does not have.

It rides the same clause as the filter in every statement `database_query` builds — the window
read, the `count(*)`, the group query and each group's slice, the range query — so a searched
view's count, its group headers and its rows all answer one predicate, and no path downstream
can disagree about what the needle found. The Markdown export passes `search: None` on purpose:
the file is the *view* the user configured, and a search is a question being asked of the
screen.

Consequences:

* SPEC §二十's index is untouched: the global search panel's semantics (and its CJK
  segmentation) are unchanged by this knife, and the two searches differ visibly in what they
  can match — documented here rather than discovered later.
* The needle is compiled per keystroke but the *rows* are still windowed: a searched 10 000-row
  database realizes its viewport's rows, never the matches.
* Unverified (no cargo was run): the per-keystroke cost, the OR-of-INSTR SQL text against
  `EXPLAIN QUERY PLAN`, and the box's pixels. Named in REPORT §D7.

## ADR-0088 · A relation is a list of target record ids, and its mirror is an involution rather than a graph to walk

Decision: SPEC §三十九's 「需计算：formula / rollup / relation（含双向关系）」, the third
of the three, which ADR-0084 left unbuilt on purpose. This knife builds it, and the shape
ADR-0084 wrote down is the shape that landed:

* **A relation column stores what the user picked — target `RecordId`s — as
  `CellValue::Items`**, one `db_value_items` row per target, in display order. So
  `PropertyKind::Relation` stops being a computed kind: `is_computed()` is false for it,
  `is_list()` is true, and `value_kind()` is `Items`. Nothing about ADR-0062's storage
  shape changes; a relation is the third user of the list mechanism after multi-select and
  files, which is exactly why ADR-0084 named it as the candidate that fits. **No
  migration.**

* **The target identity is the `RecordId`, not the `PageId`** — a deliberate departure from
  ADR-0084's letter, which said a relation 「points at §四十's reference layer for its chips,
  its backlinks and its open-link path」. §四十's reference layer addresses *pages*
  (`quire://page/<id>`, ADR-0026/0050), and ADR-0063 makes a record page-less until someone
  opens it: a relation storing page ids could not name most of the rows it points at, and
  would have to create a page per pick — the opposite of the laziness ADR-0063 chose
  deliberately. What ADR-0084 was protecting is the *discipline*, not the plumbing:
  **store ids and never titles, so a renamed target shows its live name with no rewrite.**
  That discipline is inherited whole, and it needed no new lookup to do it — the chip's text
  is read at projection time through the existing `SqliteRepository::record_title`, which is
  ADR-0063's `COALESCE` and already the one place a record's title lives (the page's title
  when it has a page, the `title` value row when it does not). The open-link path is the
  row's existing Open action, which is what already turns a bare record into a page.

* **Config** (ADR-0061's one document per column): `{"target": <database id>, "mirror":
  <property id>}`. `target` is the database the relation points at — a relation is *about*
  one other database, and a relation with no readable target paints nothing and refuses a
  pick. `mirror` is optional and names the **back-pointer column in the target database**,
  which is the whole of what "two-way" means here.

* **Two-way is one write.** `Command::SetRelation` carries the forward cell change *and*
  every back-pointer change, and `plan` emits them as one `Entry` — one `apply` list, one
  `revert` list, one Ctrl+Z. This is ADR-0084's requirement taken literally (「the forward
  change and the back-pointers land in one change batch」), and it is the shape
  `DeleteDatabaseRecord` already has for its values + record + page together.

* **The mirror map is an involution with no fixed points, not a graph to walk.** Writing a
  forward change touches the direct mirror and never descends further, so the write
  terminates by construction — *if* the pairing is legal. It is legal exactly when
  `mirror(mirror(a)) == a` and `mirror(a) != a`. So the save refuses four things: a column
  that would be its own mirror; a `mirror` naming a property that is not a `relation` in
  the target database; a `mirror` whose own target is neither this column's database nor
  *absent* (a column pointing at a third database is not a back-pointer, it is a coincidence
  of ids — while an absent target is a brand-new column the accepted declaration points
  back itself, so the one-shot pairing a user actually makes is possible); and a `mirror`
  that already declares a *different* partner. The first accepted declaration writes both
  sides in one batch; after that the two are each other's, and the refusal is what keeps
  them so.

  This is ADR-0084's 「relation 环检测在保存时做」 read precisely. The only cycle a relation
  can have is a cycle in the mirror map, because a relation's *value* is a stored fact and
  not a computed one — there is no dependency order for it to be missing from. Making the
  map an involution is stronger than refusing a cycle after the fact: **cycles are not
  representable**, so the read path needs no depth cap to survive one.

* **A dangling target degrades, it does not fail.** A target record that was deleted, or an
  id naming no row in this library (a hand-edited file, an old backup), paints the one word
  ADR-0051 gave a dangling mention. The cell still counts it: the stored value is the fact
  "the user picked this", and "what is it called now" is a separate question the store may
  answer with "nothing".

Consequences:

* `PropertyKind::is_computed` loses `Relation`; `is_list` and `value_kind` gain it. The
  three call sites that used `is_computed` to mean "this column stores no cell"
  (`database_property::parse_one`, `table_rows`'s `editable`, `database_store`'s two read
  paths) now mean what they say. `parse_one` keeps refusing a relation — it *is* a list
  kind, and its input is `parse_many`'s — and `parse_many` gains an arm that accepts only
  strings that spell record ids.
* ADR-0084's handover is discharged: all four of its fixed points (ids not titles, one
  batch, save-time refusal, rollup over the relation's targets) are implemented, and the one
  place this ADR departs from its letter — record ids rather than page ids — is stated above
  rather than buried.
* Unverified: real clicking in the picker, the chip's pixels, and the cost of a pick on a
  10 000-row target database. Named in `docs/REPORT_TRACK3.md`.

## ADR-0089 · A rollup aggregates the related records' one column at projection time, and cannot depend on another rollup

Decision: the last third of SPEC §三十九's 「需计算：formula / rollup / relation」. ADR-0084
fixed the shape; this is the code, and every part of it is that ADR's handover taken up.

* **Config**: `{"relation": <property id>, "column": <property id>, "aggregate": "<word>"}` in
  the rollup column's own document — ADR-0061's one bit of JSON per column, the same home
  ADR-0082 gave the formula expression, and the same read-edit-write discipline (ADR-0074)
  when it is set. `relation` names a relation column **of this rollup's own database** (the
  rollup's row supplies the targets); `column` names a property **of the relation's target
  database** (what gets aggregated); `aggregate` is one of six — `sum` / `count` / `min` /
  `max` / `average` / `none`.

* **`none` is one of the six aggregates.** `count` counts the related records, `sum` /
  `min` / `max` / `average` fold the target column's values with the same `Val` arithmetic
  `core::database_formula` already defines — so a rollup and a formula agree about what `min`
  of two dates is, because it is literally the same function, not a second implementation
  that could drift. `none` is the identity: the cell paints nothing. That is what a rollup is
  *before* its two columns are picked, and it is the same one-representation-of-nothing rule
  ADR-0062 uses for cells and ADR-0086 for templates.

* **Values are computed at projection time and stored nowhere** (ADR-0062), and the recompute
  unit is ADR-0083's, unchanged: the realized window × the visible rollup columns. For one
  refresh the projection makes **one batched read per rollup column**, not one per cell — the
  window's rows hand over every target id they name, and a single statement answers
  `record IN (…) AND property = ?`. The work is bounded by the window's *fan-out* (how many
  records the window's rows point at) and never by the target table's size: there is no path
  that walks the target database, for the same reason ADR-0067 has no path that walks the row
  table.

* **A rollup's target column may not itself be computed, which makes a dependency cycle
  unrepresentable rather than refused.** This is stronger than ADR-0084's save-time check, and
  it is the honest version of it. A rollup has exactly two inputs: a *stored* relation cell on
  its own row, and one column of the records that relation names. A formula is same-row by
  construction (ADR-0083: the engine's cell callback has no record parameter), so a rollup
  targeting a formula reads a value that cannot reach back to it. The one shape that could
  close a loop is a rollup targeting *another rollup*, and the config write refuses that —
  along with a target that is a relation column (aggregating ids is not a thing a user asked
  for) and a target that is not a column of the relation's target database at all. The two
  derived stamps (`created time` / `last edited time`) *are* admitted, because they are
  readable through the same one batched read and aggregating them is a common rollup. What
  remains for the read path is therefore **nothing to cap**: a hand-edited document that nests
  a rollup where the writer would have refused reads `nothing` rather than recursing, because
  `values_of` answers an empty map for a computed column and an empty map folds to `Empty` —
  the read path's depth cap is zero, and it is the strictest cap there is. (An earlier draft
  of this ADR promised a `ROLLUP_MAX_DEPTH` constant here; the implementation showed it was
  unnecessary, and this sentence is the correction rather than a code path kept alive by a
  decision that no longer needs it.)

Consequences:

* ADR-0084's 「rollup inherits it whole」 is what happened: the window bound, the counter and
  the "no cross-refresh value cache" decision all carry over unchanged, and
  `db_formula_evals` now counts rollup evaluations too — so the number the unified tests
  measure (「editing one cell recomputes the window, never the table」) is one number for both
  computed kinds rather than two that could quietly disagree.
* The Markdown export gets rollup cells the way it already gets formula cells: by *rendering
  the view*, not by learning about rollups (ADR-0065's discipline). No export-side code knows
  a rollup exists.
* Unverified: the six aggregates' pixels, the rollup editor's behaviour, and the measured
  cost of a rollup column over a 10 000-row target database. Named in the report.

## ADR-0090 · A window caches painted rows, so its key carries a content stamp and not only a shape

Decision: the window cache (`AppState::db_windows`) gains the half of its key it never had. A
window holds **painted** cells — a text cell's string, a formula's value, a relation's live
targets, a rollup's folded number — and every one of those is a function of stored data that
the rest of the key cannot see. The view, the definition text, the layout, the session stamp,
the total and the row window are all *identical* after a cell edit, after a rename in another
database, and after a computed column's config changes. So the key gains one number:
`AppState::db_content_stamp`, incremented once per recorded change batch in the one funnel
every change passes (`AppState::record`), stored into the window as it is rebuilt and compared
when the next refresh asks whether it may skip the read.

Deliberately blunt: **any** recorded change moves it, rather than an enumeration of "changes
that can alter a painted cell" (a cell, a page title a relation names, a column's kind, a
column's config, a record gaining a page…). That list is one every future column kind would
have to be appended to, and forgetting it *is* this bug; "any change may have" is always true.
The asymmetry the cache exists for is preserved — the stamp is *consumed* by the rebuild, so a
burst of edits costs one re-read per visible database window rather than one per edit, and a
scroll inside an unchanged window still touches no query at all.

Two consequences the shape-only key could not express:

* **A writer hands the chance to its peers.** A pick writes the target row's back-pointer
  cell, which is a cell of a *different* database; a rename in one database moves the title a
  relation cell in another one paints. `AppState::db_refresh_page(skip)` re-reads every
  database block on the open page — bounded by that page's blocks, never by the library — and
  is called from the five write paths that can reach another database's paint. Undo and redo
  call it with no `skip`, because a step moves stored data with no write call site of its own.
* **The D8 cost story is unchanged where it was measured.** Those numbers were taken on scenes
  with one database block per page, where the peer pass is a no-op and a scroll records
  nothing. `the_content_stamp_costs_one_reread_after_a_change_and_nothing_when_cached` prints
  the price on both shapes rather than asking anyone to take it on faith.

Why this is an ADR rather than a footnote: the first version of the cache was keyed on *shape*,
and that key is **correct for the queries it gates and wrong for the pixels it keeps** — two
different questions, which the D3 sweep could not tell apart because a scene is a still
photograph of a shape. M14 made the difference visible, because a relation cell that paints a
live title and a rollup that folds at projection time are claims about *content*; the first
end-to-end test written for them failed on a plain committed cell.

Consequences:

* `DbWindow` gains one field, `AppState` one `Cell<u64>`, `record` one increment, `db_refresh`
  one comparison and one assignment. No migration, no schema change, no store change.
* The defect this fixes is **not M14's**: a committed cell (D3), a formula edit (D6), and an
  undo of either all failed to repaint before this ADR. It surfaced now because a feature whose
  whole point is a computed paint is the first thing that could *prove* it.
* Unverified: the real cost of `db_refresh_page` on a page holding several large databases —
  the peer pass is bounded by the page's blocks, but three 10 000-row databases on one page is
  a page nobody has drawn yet. Named in `docs/REPORT_TRACK3.md`.
