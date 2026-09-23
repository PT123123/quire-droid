# PLAN — living status tracker (update after every milestone)

## M0 · Toolchain & baseline — ✅ (2026-09-18)
- [x] slint-rust-template as starting point; project renamed to `quire`
- [x] Slint 1.18.0 (current stable), MSVC, Debug + Release profiles
- [x] Renderer feature matrix in Cargo.toml (femtovg / femtovg-wgpu / skia / skia-opengl / software)
- [x] Renderer benchmark harness: `benchmarks/scripts/bench.ps1`
- [x] Baseline numbers recorded in docs/PERFORMANCE.md
- [x] Slint LSP config (.vscode)（CI workflow 已按要求移除，本地手动构建）
- [x] Directory structure + ARCHITECTURE.md / DECISIONS.md / PLAN.md / PERFORMANCE.md

## M1 · Design system & product-grade shell — ✅ (2026-09-18)
- [x] Theme.slint: spacing scale, radii, motion tokens, dark flag
- [x] Colors.slint: full light/dark palette (12+ semantic tokens)
- [x] Typography.slint: UI + document type scale, mono stack
- [x] Icons.slint: original vector path icons, theme-aware stroke color
- [x] AppShell / TopBar (custom title bar + window controls) / Sidebar /
      PageTree / Editor placeholder (block-styled mock document) /
      CommandPalette mock / Button / IconButton / BlockHandle
- [x] Mock data served through Slint models from Rust (ADR-0005)
- [x] Light/Dark toggle; one visual polish pass done
- [x] Release idle CPU/RAM re-measured with shell open (see PERFORMANCE.md)

## M0/M1 completion report (2026-09-18)
Delivered:
- Compiling Rust+Slint 1.18 workspace (`quire`), debug + 3 release renderer
  builds (FemtoVG·GL / FemtoVG·wgpu / Skia), all verified rendering real
  pixels via screenshots (light + dark + palette-open, Chinese text OK).
- Full M1 shell: frameless window, custom title bar, sidebar with pinned
  rows + page tree, mock document (headings/paragraphs/lists/todo/quote/
  code/divider), command palette mock with live filter plumbing to Rust.
- Idle: ≈0 % CPU (≤0.4 % of one core), 85–212 MB private by renderer;
  10 000-block page costs ≈3 MB over the empty shell (virtualization works).
- Docs: ARCHITECTURE / DECISIONS (ADR-0001…0008) / PERFORMANCE (baseline
  table + method + scene list) / this PLAN.

Compiler landmines found in Slint 1.18.0 (workarounds in ADR-0007/0008):
- reading a PopupWindow's own `is-open` panics the compiler;
- ListView accepts exactly one `for` child.

Known gaps carried into M2 (not blockers):
- palette keyboard/mouse interaction verified by construction + static
  screenshots, not by scripted key injection (OS foreground policy blocks
  headless SendKeys on this desktop);
- Scene F (continuous scroll) still a manual pass;
- window "startup_ms" measures window-up, not first paint.

## M2 · App shell navigation on mock pages — ✅ (2026-09-19)
- [x] `app/workspace.rs`: pure page-tree model (create/rename/duplicate/
      delete, favorites, recents with cap, expand-ancestors, search over
      title + content blob) — unit-tested without Slint
- [x] Live sidebar: Favorites / Recent / Workspace tree projection,
      collapse/expand round-trip, selection + fallback landing page,
      "+ New page" row
- [x] Page switching: per-page mock content, TopBar breadcrumb
      (Workspace › parent › page), Todo toggle kept
- [x] Context menu (right-click on tree rows, ⋯ on TopBar): new subpage,
      rename (inline), duplicate (deep copy), favorite toggle, delete
      with confirmation dialog (subtree-size aware)
- [x] Search panel Ctrl+P: titles + content, keyboard navigation,
      recents as the empty-query view; palette (Ctrl+K) = commands only
- [x] Settings dialog (appearance + about), delete confirm dialog,
      empty-page state, Ctrl+N new page flow
- [x] lib/bin split: `src/lib.rs` exposes the crate for `tests/`
      integration tests (6) + unit tests (7) — all green
- [x] `quire-shot` headless visual-regression tool (ADR-0011) +
      `just shot <scene>`; 9 M2 scenes rendered and reviewed (9/9 pass)
- [x] Benchmarks: scenes F + G measured for the first time
      (PERFORMANCE.md), scenes A/D re-measured on the M2 build

Delivered in the same pass (former known-gaps from M0/M1):
- Scene F now has a programmatic proxy measurement; scene G delivered
  with M2 as planned. Palette interaction verified by real renders of
  each interactive state (headless), not just static construction.
- Stack-overflow on startup found & fixed (ADR-0009): Slint 1.18's
  recursive binding evaluation exceeded the 1 MB Windows default with
  the full M2 shell; the event loop now runs on an 8 MB-stack thread.
  Also fixed by the visual review pass: page-title descender collision,
  TopBar child-geometry (Slint centers width-only children), zero-length
  icon path segments invisible in the software renderer.

Deferred (not blockers, tracked for M3+):
- window `startup_ms` still measures window-up, not first paint;
- scene F remains a programmatic proxy until wheel-input injection is
  available; real input pass stays manual.

## M3 · Local documents (SQLite) — ✅ (2026-09-19, branch `m3-storage`)
- [x] `storage/`: rusqlite (bundled) + six-table schema per SPEC §十八 in
      `migrations.rs` (`PRAGMA user_version`, forward-only upgrades),
      `database.rs` (WAL + synchronous=FULL + startup `integrity_check`,
      SPEC §二十五), `repository.rs` implementing the `core::Repository`
      contract — `apply` one transaction per change list (SPEC §十八),
      cascade + recursive-CTE subtree deletes, `replace_all` with deferred
      FK checks and an explicit parent-cycle guard
- [x] `services/persistence.rs`: dirty queue → 300 ms debounce (injectable
      `Clock`) → one batched `apply`; `force_flush` for Ctrl+S/shutdown;
      failed writes keep the queue ordered and retry at the next deadline
      (SPEC §十九). Slint- and thread-free: the app layer's timer calls
      `flush_if_due`, any thread may call `force_flush`
- [x] Tests: 22 lib unit + 13 storage integration + 3 persistence
      integration — round-trip (incl. OrderKey extremes and CJK text),
      migration of a v0 file, rollback of a failing batch, corrupt-file and
      unknown-kind reporting, fake-clock debounce behavior, and a
      subprocess kill -9 crash test (committed rows survive, an
      in-flight transaction never surfaces)
- [x] `cargo check --all-targets` + `cargo test` green on this branch;
      save-latency numbers recorded in docs/PERFORMANCE.md; schema
      rationale in ADR-0013

Not in this pass (per track split): controller/UI wiring of
`PersistenceService` and retiring `app/workspace.rs` mocks — Track A
after the M3/M4 merge. DB file location/first-run bootstrap is part of
that wiring decision, not the storage layer.

## M3 integration wiring — ✅ (2026-09-19, Track A, `de43e95`)
- [x] main.rs opens `appdata/quire.db` (migrate + integrity check);
      `--db` override; failed open = memory-only session (never writes
      over an unreadable database); final flush on window close
- [x] fresh DB seeds the session once (PageCreated + BlockInserted batch
      flushed immediately); non-empty DB rebuilds workspace tree + Document
      from PersistedState
- [x] every mutation records contract Changes: page create/rename/
      duplicate/delete/favorite/expand, editor commands, undo/redo;
      600 ms quiet-period flush timer + Ctrl+S
- [x] verified end to end: seed run (14 pages / 24 blocks) -> restart
      loads identical state (`--dump-state`)
- known drift: a duplicated page may sort last among siblings after a
  restart (gap-exhaustion fallback appends); session view keeps it
  adjacent. Bench `--blocks` overrides the loaded page in memory only.
- still mock: recent list is session-local; settings (theme) persistence
  lands with the settings UI work (M8); M4 editor polish continues
  (block-type switching menu in M5, IME acceptance = user pass)

## M4/M5/M6 progress — Track A (2026-09-19, `cda0cd8`+)
- [x] M5 slash menu: Rust-owned descriptors, filter-as-you-type, keyboard
      first (SPEC §十五); applying type+text cleanup is ONE undo step
- [x] M5 block handle menu (+ / ⋮⋮): move up/down, duplicate, copy block,
      paste below, delete; cross-block clipboard (menu-gated)
- [x] M6 inline marks: Mark/MarkKind in the contract (BlockMarksSet),
      ToggleMark command (add/remove/replace semantics), marks table in
      schema v2, Ctrl+B/I/E/Shift+X over the selection; runs render with a
      documented wrap limitation (Slint Text has no inline formatting)
- [x] settings persistence: theme + recents survive restart (settings/
      metadata tables); palette gained Rename/Duplicate/Delete Page
- [x] platform notes: PopupWindow cannot be conditional/repeated → one
      instance per row with changed-driven show; delegate-inline menus get
      painted over by following ListView rows (window-level popups win)
- [ ] user pass: Chinese IME acceptance; 1000-block typing check lands
      with Track B's scene E; drag-to-reorder (mouse) deferred to M7 polish

## M5/M6/M7/M8 wiring — Track A round 2 (2026-09-19)
- [x] merged Track B m8-markdown into m3-storage: schema v2 (FTS search
      index) + v3 (inline marks) coexist; CURRENT_VERSION = 3
- [x] search panel switched to the FTS SearchService (ranked, Chinese-
      capable via segmentation); blob scan remains the no-DB fallback
- [x] import/export wired: palette "Export Page as Markdown…" /
      "Import Markdown…" with native file dialogs (rfd); import creates a
      page from the file and opens it
- [x] M6 link UI: Ctrl+L over a selection opens an Add-link dialog
      (URL input, Apply/Remove); Link marks render accent-colored
- [x] settings persistence: theme + recents survive restart; palette
      gained Rename/Duplicate/Delete Page; Ctrl+Shift+Up/Down moves the
      focused block
- [x] judge review of slash / block-menu / marks / link scenes: 4/4 pass

## Track A round 3 (2026-09-19)
- [x] async FTS search in the GUI: query goes to the worker thread, the
      controller polls on a re-arming 30 ms timer, generation counter drops
      superseded results; headless scenes use a blocking sync path
- [x] link runs open in the browser (shell open, cfg-gated)
- [x] restore notice banner (OpenReport consumption, M8 feedback #4) +
      load-corrupt notice path (feedback #5 UI half)
- [x] mouse-drag reorder evaluated and dropped (PointerEvent carries no
      position; TouchArea coords move with the swapped block) — menu +
      Ctrl+Shift+arrows are the reorder UX
- [x] IME manual checklist (docs/IME_CHECKLIST.md) — USER PASS PENDING
- [x] judge: 14-scene regression sweep 14/14 pass
- [ ] final build/test/render verification rides on Track B's in-flight
      D6 export/import mark parsing (services/export_service.rs WIP)

## Track A round 4 (2026-09-19)
- [x] dark-mode QA of every new surface (slash / block menu / marks /
      find bar / link dialog / title edit): combined dark scenes added,
      all render correctly on dark surfaces
- [x] find-as-you-type (session rebuilds per keystroke)
- [x] README screenshots (docs/screenshots, 6 shots) + window size
      persistence (settings table, restore before first paint, save at
      close) + version footer cleanup
- [x] M8_FEEDBACK all 8 items triaged: 4 resolved, 2 accepted with
      rationale, 2 partially resolved pending Track B's D5-D6 landing
      (now landed and merged)

### D11 · M7 performance matrix ✅ (Track B, 2026-09-19)
- [x] full matrix recorded (benchmarks/results/*.jsonl): femtovg + skia,
      scenes A–G incl. typing at 1k/10k; bench.ps1 gained -PinnedDb
      (idle scenes pinned to a temp DB) and legal JSON output; new
      bench_matrix.ps1 driver
- [x] conclusions page in docs/PERFORMANCE.md (per-scene comparison +
      SPEC §六 checklist + ranked M7 follow-ups)

### D12 · Data location migration ✅ (Track B + Track A wiring, 2026-09-19)
- [x] storage/data_location.rs: Placement (Fixed / Portable / Roaming),
      argv scan, per-user migration carrying db + snapshots + log family
- [x] open_with_report resolves through it (idempotent); logging::data_dir
      follows the database
- [x] Track A wiring: #12 periodic snapshots powered
      (with_database_snapshots on the flush tick, 10-min floor); #13
      main.rs create_dir_all removed (storage owns its dirs); startup
      notice bar reports the library move and backup restores
- [x] verified: full suite green post-merge; LAN smoke re-run on the
      merged tree

## Next: M4 · Block editor MVP (remaining)
- block-type switching menu (BlockHandle affordance), cross-block
  clipboard, 1000-block editing responsiveness check, Chinese IME
  acceptance pass (user), editor visual scenes re-shot.

## Later (unchanged from brief)
M4 block editor MVP (IME = test item, ADR-0002) → M5 slash menu + command
palette real wiring → M6 rich text → M7 virtualization/performance →
M8 Windows RC (packaging, crash recovery, import/export) → M9 Android
(separate IME/editor test plan).

## Explicitly out of scope for v1
Sync, collaboration, cloud, plugin market, AI, multi-process IPC,
custom TSF/IME implementation, image/table/toggle/database blocks.

## Track A round 5 — ⋮⋮ menu completion + Callout (2026-09-19, `m8-hardening`)
- [x] block menu = Notion's set minus collab/AI: Copy link to block
      (`quire://block/<id>` via clip.exe), Move to (cross-page subtree
      move, one undo step), Text/Background color (swatch palettes with a
      live current-pick check); quire://block & quire://page links resolve
      in-app (page jump + block focus)
- [x] Callout block (kind 10): tinted box + emoji + text; slash menu /
      Turn-into gain it; exports as a quote
- [x] block color in the contract: ColorKind pair on Block, SetBlockColor
      command, BlockColorSet change, schema v4 (conditional column add),
      theme-aware palette in Colors.slint, rendering across text kinds +
      the editing input
- [x] menu popup anchor clamp now follows the live row count; MenuRow
      gained swatch/check fields; link + arrow-right + palette icons
- [x] tests: move-to-page subtree/undo, color undo, colors+moves storage
      round-trip; suite green; scenes block-menu / move-to / text-color /
      bg-color / block-colors / dark-block-colors rendered and reviewed
- [x] markdown-shortcut curation (ADR-0022) re-verified: H1-3 / bullet /
      numbered / to-do / quote stay out of both menus
- deferred (user-visible gaps, next rounds): Page & Link-to-page blocks
  (need a child-page column + lifecycle), Table/Toggle (v1 exclusions),
  Comment/Suggest edits/Ask AI (collab/AI stay out of scope)

## Track A round 6 — popup dismissal + the "+" insert menu (2026-09-19, `m8-hardening`)

User-reported: none of the three menus (sidebar page menu, block ⋮⋮ menu,
slash menu) close when clicking outside, and the "+" handle only inserted
an empty line instead of offering block choices.

- [x] Root cause: Slint 1.18 dispatches *no event into the window content*
      while a popup is open — clicks outside a `no-auto-close` popup are
      swallowed by the engine (i-slint-core `window.rs` dispatch loop), so
      the AppShell scrim never fired and the old comment about it was
      wrong. All popups were `no-auto-close`.
- [x] Menus switch to `close-on-click-outside` (outside click AND Escape):
      sidebar/TopBar context menu, slash popup, block ⋮⋮ popup, command
      palette, search panel. Dialogs (link, confirm, settings) keep
      `no-auto-close` deliberately — they are modals with explicit
      buttons, and outside clicks stay inert.
- [x] State resync: the engine flips the popup's `is-open` on auto-dismiss;
      AppWindow mirrors each popup's `is-open` parent-side (reading a
      popup's own `is-open` is the 1.18 const-prop crash) and folds it
      back into UIState (`menu-open`, `slash-open`, `block-menu-open-id`,
      `palette-open`, `search-open`). Without this the AppShell scrim
      would stay enabled and the window would look frozen.
- [x] The closing press consumes the click (engine semantics), so a grip
      toggle can't re-open; side effect, documented in CHANGELOG: moving
      directly from one open menu to another costs two clicks.
- [x] "+" handle = Notion's insert menu: `block-plus(id, row-bottom, x)`
      inserts the empty paragraph below the clicked block, focuses it, and
      opens the slash popup in a new insert mode (slash-insert) anchored
      below that line. INSERT_ITEMS (state.rs) carries the full list; the
      curated "/" menu is untouched (ADR-0022 holds there).
- [x] Insert mode: typing filters on the whole line (no "/" prefix), the
      anchor stays put, applying discards the typed filter text, and every
      close path clears the mode in one place (`changed slash-open`).
      Markdown shortcuts still win over the menu, matching the typing flow.
- [x] Notion-parity placeholders: Page, Toggle list, and the database views
      (Table, Board, Gallery, List, Calendar, Timeline) render muted with
      "· later" hints, are skipped by arrow-key navigation, and applying to
      one is a no-op — v1 exclusions stay visible without pretending to
      work (PLAN "out of scope" list unchanged).
- [x] quire-shot grew behavioral probes: `--click x,y` / `--key escape`
      dispatch real input through the engine and print before/after popup
      flags; `--probe-blocks` prints the id:kind projection; scene "plus"
      opens the insert menu headlessly.
- [x] Probe evidence (software renderer): menu / slash / block-menu /
      palette / search all open (before) and dismiss on outside click and
      Escape (after); plus-menu apply converts the new block
      (Paragraph→H2 observed); clicking the disabled Table row keeps the
      menu open and the block untouched.
- deferred (unchanged): real Page / Toggle / database blocks stay out of
  scope for v1; the placeholder rows only surface the roadmap.

## M8 hardening · Track B (2026-09-19, branch `m8-hardening`)

Based on `master` at `f2eb855` (after M8 D5–D8 and the merged markdown pair).
Four deliverables, one commit each: D9 logging, D10 backup retention, D11 the
M7 performance matrix, D12 the data-location move.

### D9 · log infrastructure ✅
- `services/logging.rs`: `quire.log` beside the database, family of three
  (`quire.log`, `.1`, `.2`) at ≤1 MB each, one physical line per record with a
  UTC stamp, `Level::{Info,Warn,Error,Panic}`.
- `init()` is the only thing `main.rs` gained (one line). It creates the
  directory, writes the startup record and installs a panic hook that chains the
  previous one, so stderr keeps the usual message.
- A panic appends `[panic] …` and writes `panic-report.txt`; the next `start()`
  turns it into the `last_session_aborted` entry of `session.meta`, logs it once,
  and deletes the report. `meta_entries()` is the seam for Track A to carry it
  into the `metadata` table (M8_FEEDBACK #9).
- Tests (9, in-module because `Cargo.toml` is out of reach — feedback #11):
  rotation caps the family at three files and drops the oldest, generations
  shift the right way, a panic → abort-metadata → consumed-once chain, a clean
  start reports no abort, metadata escaping across a reopen, the hook firing
  through a real `catch_unwind` panic, and UTC stamp arithmetic.
- Checked end to end: a scratch-directory run writes
  `2026-09-19T08:21:55.169Z [info] session started (pid 67564)` to
  `appdata/quire.log` and exits 0.
- Known limit (ADR-0018): a kill or a native crash leaves no report, so
  `last_session_aborted` means "panicked", not "ended badly".

### D10 · backup retention + periodic snapshot ✅
- `storage/backup.rs`: `KEEP` 3 → 5 and a second window, `MAX_AGE` = 7 days.
  New `prune(path, now)` applies both (plus two slots past `KEEP`, so a family
  left by a larger setting cannot linger) and `snapshot()` runs it after a
  successful copy, ignoring its error — cleanup never turns a good snapshot into
  a reported failure. `recover` now walks five generations.
- `services/persistence.rs`: `with_snapshotter(interval_ms, hook)` and the app's
  shortcut `with_database_snapshots(&repo)`, default
  `DEFAULT_SNAPSHOT_INTERVAL_MS` = 10 min. The check runs at the end of
  `flush_if_due` / `force_flush`, so no thread and no second timer; it fires only
  when the period elapsed *and* something was written since the last snapshot.
  A failing snapshot goes to `take_snapshot_error()` + `logging::warn`, never into
  the flush's `Result`, and the period restarts either way (a dead disk is not
  retried per tick).
- `storage/repository.rs`: `SqliteRepository` remembers the path it opened
  (`path()`) and gained `snapshot()`, which takes the same connection mutex every
  write takes.
- Tests: 6 new in `services::persistence` (fake clock — period *and* write gate,
  period restarts from the snapshot not the write, `force_flush` carries it, an
  idle session takes none, a failing hook leaves the data write intact, no hook =
  old behavior); 3 new + 1 generalised in `tests/integration/backup_test.rs`
  (rotation to five generations, age window via `File::set_times`, oversized
  family, retention from inside a real open); 1 new in `persistence_test.rs`
  against a real SQLite file, asserting `.bak1` holds the mid-session edit and
  the startup copy moved to `.bak2`.
- Landed as two commits, because the parallel track's `git add` swept
  `src/storage/backup.rs` into `67b23f1` while D10 was unfinished; the retention
  half is there and the snapshot half is here.
- Wiring owed (M8_FEEDBACK #12): one line in `app/state.rs` where
  `PersistenceService::with_default_clock` is built. Until then the periodic
  snapshot is tested but not armed, exactly like the startup one was before.
- Known reading (ADR-0019): ten minutes is a *minimum* gap on a timer the app
  re-arms per burst, so an idle window takes no snapshot — deliberate, since
  there is nothing new to protect.

## Track A round 7 (2026-09-20, `m8-hardening`)

- [x] round 6 (popup dismissal + "+" insert menu) committed as `3f56148`
      after the full suite went green
- [x] M8_FEEDBACK #1 closed: the contract gains `SettingDelete`/`MetaDelete`;
      `repository.rs` applies them as plain DELETEs inside the change
      transaction; `settings_store` retires the empty-value tombstone (the
      read-side filter stays for legacy rows and the next save vacuums one);
      storage round-trip + settings-diff tests added, the backup suite's
      tombstone expectation updated to the new contract
- [x] M8_FEEDBACK #9 wired in `AppState::new`: session.meta entries are
      copied into the `metadata` table in one transaction and consumed with
      `MetaDelete`s; an abort summary becomes the notice bar's first line
      (`db_notice` is a queue now, so it composes with the restore and
      library-move notices from `main.rs`)
- [x] last-open page restore fixed: `current-page` was parsed at startup but
      never used, so every restart landed on Getting Started; the recorded
      page now wins unless it was deleted
- [x] `lan_server.rs` test-only imports moved into `mod tests` (last lib
      warning); `main.rs`'s dead `library_moved` initializer dropped

### D13 · clean-exit marker ✅ (Track B agent + Track A wiring, 2026-09-20)

- [x] `main.rs` notes `logging::END_RECORD` after the final flush (normal
      exit path only); `Logger::start()` reads its absence — with no panic
      report — as "did not shut down cleanly (killed, crashed natively, or
      lost power)" and reports it exactly like a panic. The tail check
      follows the rotation family (an end-record shifted into `.1` still
      reads clean), an empty family is a first run, panic reports take
      precedence, a torn last line aborts
- [x] five in-module logging tests (end-record clean, kill incl.
      start-line-only and rotated variants, panic precedence, empty log) —
      lib suite at 102; verified end to end on a scratch database: clean
      `--auto-exit` leaves the record, deleting it or hard-killing the
      process makes the next start log the Warn + meta entry
- [x] the abort summary reaches the user through #9's consumer: notice bar
      first line + a queryable `metadata` row; ADR-0018 carries the update,
      including the accepted false-positive sources (second instance,
      externally killed bench run)
- [x] full suite green on the merged tree (197 pass / 4 ignored by design);
      M8 hardening feedback #1/#9/#10 all resolved — branch merged to
      `master`

## Track A round 8 — Page block (2026-09-20, branch `m8-page-block`)

The round-5 deferral ("Page & Link-to-page blocks need a child-page
column + lifecycle") lands, on `master` after the D13 merge.

- [x] data layer (`08a2e1f`): BlockKind::Page ("page"), `Block.page_ref:
      Option<PageId>`, schema v5 (`blocks.page_ref`, conditional column
      add like v4), `Change::BlockRefSet`; repository insert/load/apply
      carry the column; export renders a Page block as
      `[title](quire://page/<id>)` (re-imports as a clickable link mark)
- [x] lifecycle (`b869afa`): "+" insert menu's Page row is real — one
      batch creates the child page (under the current page) and converts
      the row; the row shows the child's live title (rename path
      reprojections), click opens (block-activate routes kind 11 to
      open_page; the row is never editable), delete takes the child page,
      duplicate deep-copies the child and retargets the copy, paste lands
      the title as plain text (no shared targets)
- [x] document.apply gained the BlockRefSet arm — its absence was caught
      by the headless scene (row rendered "(deleted page)" while the
      sidebar had the child): the scene-first workflow paying off
- [x] tests: storage round-trip (ref set/cleared survive), markdown
      export shape (link + dangling fallback); suite green (199 pass /
      4 ignored); scene `page-block` rendered and reviewed (icon + live
      title + sidebar child, light theme)
- deferred (follow-ups): turning a Page block into another kind keeps the
  child in the tree; a duplicated page's embedded page blocks still share
  the original's child references (recursive copy is v2)

## Track A round 9 — B-package (2026-09-20, on `master`, parallel with Track B's A1–A5)

First round under the split-brief protocol (`docs/AGENT_BRIEF_M8_TAIL.md`):
Track B owns storage/packaging/bench files, Track A owns contract/app/UI;
both commit per task on `master` with surgical staging. No collisions.

- [x] B1 · Link-to-page block (`9f0f496`): BlockKind::Link ("link_to_page",
      UI kind 12) reuses `blocks.page_ref` — no schema change, no new
      Change variant — and points at an EXISTING page it does not own.
      The "+" menu's "Link to page" row flips the slash popup into a page
      picker (`slash-pick-page` mode: every page in tree order, title
      filter, breadcrumb hints, applying converts the line, Escape keeps
      it). Delete leaves the target page alone; duplicate/paste share the
      ref freely (unowned); rendering shares the page-row path with a link
      icon; export matches Page. Scene `link-block` seeded + reviewed.
- [x] B2 · drag-and-drop file import — deferred with a finding: Slint
      1.18.0 handles zero external file-drop events (no
      `DroppedFile`/`HoveredFile` anywhere in the vendored winit backend or
      core), so Explorer drops never reach `DropArea`. Revisit on a Slint
      upgrade or via a Win32 `IDropTarget` hook in `platform/` (needs a COM
      dependency — cost/benefit against §二十七's post-MVP status).
- [x] B3 · rich paste (`b235829` + `cb4334e`): Ctrl+V reads the clipboard;
      text with block structure lands as blocks (empty row converts in
      place, further rows insert after, marks replay as sequential
      ToggleMark commands), plain paragraphs fall through to the native
      caret paste. The first cut read the clipboard through a
      `Get-Clipboard` subprocess — measured 7–10 s on this desktop — so
      the read is direct Win32 FFI (`OpenClipboard`/`CF_UNICODETEXT`/
      `GlobalLock`, microseconds, no new crate), verified end to end with
      a clip.exe round trip.
- [x] gate tests: `parse_if_block_structure` admits multi-block and
      non-paragraph lines, rejects plain paragraphs (marks included);
      suite green throughout (201 pass / 4 ignored) alongside Track B's
      in-flight A1/A3 edits in the shared tree.
- known costs: an N-block paste is several undo steps (inserts chain on
  ids the command planner cannot know upfront); Ctrl+V in a non-empty
  block inserts after it rather than splitting at the caret (Notion does
  the split; v1 keeps the simpler shape). CJK round-trips through the
  FFI read (any source app's CF_UNICODETEXT); the app's own
  `copy_to_clipboard` write path remains ASCII-by-design (clip.exe).

## Track A round 10 — Move page (2026-09-20, on `master`)

While Track B runs its A-package, the B-side found a real SPEC gap:
`PageMoved` existed in the contract and storage since M3, but nothing in
the app ever constructed it — SPEC §十七's "Move page" never shipped.

- [x] workspace: `move_page` (detach + attach, refuses moving a page into
      its own subtree via the ancestor chain, expands the target parent on
      arrival) and `swap_with_neighbor` (sibling ±1 with edge refusal)
- [x] state: `move_page` / `move_page_by` mirror the tree edits into the
      `page_order` map and record `PageMoved` changes (one for a reparent,
      two for a swap); sidebar rebuilds
- [x] sidebar page menu: Move up / Move down / Move to — the second-level
      submenu lists Top level plus every page outside the moved subtree
      (indented tree walk; cycles impossible by construction), keeps the
      popup open like the block menu's mover, and re-anchors the taller
      list so it stays on the window
- [x] small closes riding along: Turn-into away from Page/Link drops the
      block's reference (a Page's child survives, unowned); ⋮⋮ Copy-link
      on a Page/Link row copies `quire://page/<ref>` so the link opens the
      target everywhere (the in-app resolver jumps straight to it)
- [x] test `move_page_reparents_refuses_cycles_and_swaps_siblings` through
      the public API (reparent/refuse-cycle/swap/edge/back-to-root); scene
      `page-move-to` rendered and reviewed (Back / Top level / indented
      targets, the moved subtree correctly absent); suite green (202 pass /
      4 ignored)
- known limit: a Move-to list taller than the window still overflows
  (the clamp moves the anchor, cannot shrink a menu) — same class as the
  block menu's mover; a scrollable menu is a later polish item

## Track A round 12 — sidebar page-tree drag (2026-09-20, on `master`)

The last interaction in SPEC §八's Sidebar list: drag a page row onto
another page to nest it (one PageMoved), onto the Workspace header for
the top level. Same DragArea/DropArea rails as the block handle; the
landing row tints; favorites/recents/new-page rows are not targets;
workspace::can_move_page (extracted, read-only) refuses cycles per hover
frame. Payload MIME 'slint-notion/page:' keeps page drags distinct from
block drags. Test covers nest / refused cycle / non-target rejection /
top-level return; default scene render unchanged (`13a011a`).




## Track A round 13 — Copy Page as Markdown (2026-09-20, on `master`)

While Track B reviews the A4 sweep (Track A holds ui/**; this round is
ui-free), ADR-0025's promised write-path upgrade shipped: palette command
"Copy Page as Markdown" (CMD_COPY_MD) runs the open page through
export_page and puts it on the clipboard — `copy_to_clipboard` swapped its
clip.exe internals for `SetClipboardData`/`GMEM_MOVEABLE` FFI (callers
unchanged, no crate, clip.exe retired entirely). Round-trip test pins CJK
through write → read (`clipboard_write_and_read_round_trip_unicode`);
notice-bar feedback for empty pages and clipboard failures.

## Track B A-package progress (2026-09-20, dispatched via docs/AGENT_BRIEF_M8_TAIL.md)

- [x] A1 · feedback #13 closed (`87ec1dc`): --portable is a real
      LaunchArgs flag, one resolve per start, OpenReport::migrated_from
- [x] A3 · release profile audit (`32f6d48`): four profiles compared on
      scenes A/D/E — all in one noise band; fat LTO and cgu16 rejected on
      build-time/size, panic=abort rejected on behavior (no unwind = the
      panic hook never runs, the crash-recovery chain goes blind);
      profile_bench.ps1 added; the shipped profile wins. Decision pinned
      as ADR-0024; the audit caught and documented a
      parallel-build-polluted measurement batch
- [x] Track A round 11 riding along: settings STORAGE row (data folder +
      open-folder + back-up-now, hidden for memory-only sessions,
      `643bd7d`) and the duplicate-page id-range reservation fix closing
      feedback #2 (`c405a29`)
- [x] A5 · ROADMAP refresh (`2ba401c`): status column current through
      Track A round 11; menu-overflow listed as an explicit M8 remainder
      (kept there — CHANGELOG Known limitations mirrors it; no ADR needed
      for a polish item)
- [x] A2 · first-paint measurement: startup_ms (window-up) and
      first_paint_ms (first frame) are separate measures now, documented
      side by side in PERFORMANCE.md (≈3.9×/5.6× apart) — the M0-era
      "startup_ms is not first paint" gap is closed; terminology note
      recorded so CHANGELOG/PLAN never blend the two. Follow-ups (real_main
      stage timing, skia comparison) live in PERFORMANCE.md
- [x] A6 · installer end-to-end (`2fd633b`, verify-installer.ps1): silent
      install with the .md association task → installed exe launches and
      exits 0 on a scratch db → extracted icon pixel-matches quire.ico,
      version resources correct → Quire.Markdown registered without taking
      the default handler, and the verb's --open path proven live (pages
      14→15) → silent uninstall leaves zero residue (progid, .md candidate,
      shortcuts, Add/Remove); the real per-user library untouched
      (timestamps checked)
- [x] A6 finding folded: install/quire.png ships for nobody — Slint 1.18
      has no Window::set_icon, nothing references the file, and shell
      identity comes from build.rs's embedded IDI_MAIN resource. Track A
      decision: drop the 7.4 KB asset from the installer (the M8_FEEDBACK
      #4 @image-url plan stays retired)
- [x] A4 · visual sweep (`7e6f07b` harness + `2ec7c17` verdict, re-swept as
      sweep2 after Track A's fixes): 13/34 clean at first pass; the three
      HIGHs and most MEDIUMs fixed by Track A (`e4878f1`, `6c115b5`) —
      D1's "overprint" was the marks SCENE's hardcoded offsets (fixed by
      word-derived offsets; the wrap limitation stays documented as
      accepted design debt, not a regression). Remaining: 4 LOW-ish items
      (notice-bar layout, link-dialog gap, swatch contrast, snippet elide)
- [x] A1 follow-up: `install/verify-portable.ps1` — 24/24 over the
      portable layout, log following, migration suppression, --db
      precedence, legacy migration, %APPDATA% hash untouched
- [x] A2 follow-ups: real_main phase stamps (the pre-paint gap split);
      scaling answer LINEAR (state_new ≈ 4 + 13.1 ms per 1000 blocks,
      max residual 1.5 ms); repo_open flat ≈51 ms across 0–10k blocks
      (re-pins ADR-0015's floor); first-paint floor ≈370 ms flat over a
      20× document range; same-batch-only comparison rule recorded
- [x] `benchmarks/scripts/audit_results.ps1`: recomputes every stored
      summary from raw lines, exit 1 on mismatch — tables are renewable
- [x] M8 verification snapshot at `6c115b5`: 209 passed / 0 failed /
      4 ignored, release build clean, installer E2E re-run 4/4 on the new
      exe (7.94 MB, --open 14→15, zero uninstall residue)

### CRITICAL fix — palette dispatch shadowing (Track A, round 13's own bug)

`CMD_COPY_MD` was missing from controller.rs's import list, so the match
arm became a catch-all binding: every palette command with id ≥ 9
(Export/Import/Copy Markdown and every Jump-to-page) ran
copy_current_page_markdown. Caught by Track B's audit — the tests never
walk this dispatch, and the rustc warnings went unheeded by Track A.
Fixed by the import plus converting ALL const match patterns in both
dispatch closures to fully qualified paths, which the compiler rejects
instead of silently binding.

### Pending decisions/actions

- quire.png: CONFIRMED dropped from the installer (nothing references it;
  shell identity = build.rs IDI_MAIN). Track B updates quire.iss +
  verify-installer.ps1's payload assertion together.
- Shortcuts carry no explicit WorkingDirectory — only relevant if a
  --portable installer option ever ships (recorded, no action).
- Track A next: skia comparison is blocked by the shared target lock
  (needs a CARGO_TARGET_DIR policy decision); renderer_name() duplication
  in main.rs vs controller.rs is Track A's to consolidate.

## Track A round 14 — Go Back / Go Forward, and the IME pass closes (2026-09-20)

- **M4 signed off**: the user walked `docs/IME_CHECKLIST.md` by hand and all
  17 items are checked. Recorded in the file itself as a verbal attestation,
  not a measurement — an item found wrong later gets un-checked there.
  That was M4's last open item and M8's last non-code gate.
- **M9 parked** by explicit decision ("安卓先不做"); ROADMAP says so and the
  measured evaluation in `.scratch/m9/report.md` stands as the record.
- **SPEC §十六 Go Back / Go Forward implemented** — the last palette item
  SPEC names that had no code behind it.
  - `NavHistory` (`src/app/state.rs`, next to `AppState`): two stacks, newest
    last, `NAV_MAX` 50 entries, browser semantics (a new navigation drops the
    forward branch). Deliberately a plain struct with no Slint and no
    database in it, so the stepping rules — which are the whole feature —
    are unit-testable; 5 tests in `state.rs::tests` cover retrace/rewind,
    the forward-drop, the bounded stack, the first open, and one row that
    asserts both palette rows exist with their ids (the id *is* the dispatch
    key, which is what the round-13 CRITICAL shadowed).
  - Deleted pages are skipped rather than opened: `nav_step` takes a
    liveness predicate and pops until it finds a page still in the workspace.
  - Recording happens in the controller's `open()` funnel and in
    `create_page` (creating navigates, so Back returns where you were).
    `navigate()` must not re-record — `nav_step` already parked the page
    being left on the opposite stack.
  - Alt+← / Alt+→ (`AppWindow.slint` KeyBinding → two `*-requested`
    callbacks, the convention the rest of the shortcuts use) and the two
    palette rows in a `Navigate` section, with a new `arrow-left` icon
    mirroring `arrow-right`. Settings ▸ SHORTCUTS lists them.
  - **Found on the way**: the typing flush is debounced 300 ms and resolves
    against `state.open_page`, so navigating inside that window dropped the
    last keystrokes — `flush_pending_edit` ran against a page that had
    already changed. `open()` and `navigate()` now flush first. This fixed
    every existing navigation path too (sidebar click, palette jump, search
    result, Page block), not just the new one.
  - **Behavior change worth knowing**: clicking a Page block now goes
    through `open()`, so the top bar title and the sidebar highlight follow
    it the way they do for every other navigation. Previously it called
    `state.open_page()` directly and left the old title on screen.
- **No page-management limitation existed**: a stale doc fragment claimed
  CHANGELOG listed "create, rename, move, favorite pages" as missing. It
  does not — grep finds no such line anywhere. Recorded so nobody re-adds it.
- **Two audit errors of mine, corrected against the files**:
  `.vscode/extensions.json` recommends `Slint.slint` and `settings.json`
  configures its language server (I had it backwards, and reported it to the
  user that way); `renderer_name()` exists once (`controller.rs:171`), so the
  "Track A next" line above about consolidating it is stale. What §四 task 6
  genuinely still lacks: rust-analyzer settings and a verified live preview.
- **SPEC audit, still open** (each one re-read from source, not from this
  ledger): §十二 block virtualization is not windowed — `reproject_blocks`
  sets every row and `Editor.slint:237` is a plain `for` in a `ListView`, so
  10 000 blocks are 10 000 realized rows each carrying a TextInput; M7's
  "virtualization ✅" overstates it. §廿六 plain text absent (md only, dialog
  filter `&["md"]`). §廿三 memory attribution absent (2 counters total, and
  the checklist ticks "GPU-side memory separate ✓" with no GPU reading ever
  taken). §廿七 tray / native menu / startup options / global shortcut: none,
  and `system-tray` is compiled in unused. §卅一 DPI: 125/150/200% never
  measured. §十三 `composition_state` never named.
- **Visual gate, and what it exposed** (sweep5 vs the sweep4 baseline, hash
  diff so only moved pixels get re-judged): 32 of 34 scenes byte-identical,
  exactly the two that should have changed — `palette.png` (52 px, the scroll
  thumb shortening) and `settings.png`. 52 px is the evidence that the two new
  palette rows are *in the model but below the palette's visible fold* at an
  empty query, so the sweep could not see them at all. Added a `palette-nav`
  scene (`apply_scene_overlay`, query `go`) and it renders both rows with the
  new `arrow-left` icon and the `Alt+Left` / `Alt+Right` hints intact — that
  scene is now the permanent coverage for §十六's rows.
- **Pre-existing defect, not this change's** (pixel-cropped from the baseline,
  not eyeballed): the Settings popup is taller than the 1280x800 window, so
  ▸ SHORTCUTS ▸ ABOUT ▸ the **Done button** have been off-screen since before
  this commit — sweep4's last visible row was already `Ctrl+S / Save now`. The
  new `Alt+Left / Right` row lands exactly on the cut line, visible but the
  last thing on screen. Fixing it means making the popup scroll or clamping its
  height, which is its own item — recorded, not done here.

## M10–M14 · 规格已定，未开工（2026-09-20）

范围由用户点定：除云同步 / 协作 / AI / 插件 / 发布站点 / 评论外，
Notion 的其余能力全部进入排期。规格见 SPEC §三十七–§四十，
里程碑与验收门槛见 SPEC §二十九 与 ROADMAP 的 M10–M14，
决定记录为 ADR-0027。开工前仍按 §三十 的流程走：读架构 → 最小改动 →
编译 → 测试 → 检查 UI → 记录性能影响。

建议起手：M10 批次 A 的 image 块（附件落盘 + 降采样缓存是这批里唯一
会动摇低 RAM 卖点的部分，值得最先测量）。

## M10 批次 B · slice 1 — toggle 折叠块（2026-09-20，on `master`）

§三十七 的第一条完整竖切。没按批次 A 起手，是为了先把「行的生死」这件事
做对：toggle 是这一批里唯一会改变 projection 输出的 kind，而 table / columns
只会重复它。

**改动面**（六处接线，SPEC §三十七 的门槛）：`BlockKind::Toggle`（int 13，
`as_str` = `"toggle"`）；`blocks.folded` 列 = schema v6（条件 ALTER，走
`pragma_table_info` 探测，和晚近几列同一写法）；`Command::ToggleFold` →
`Change::BlockFoldedSet`（可撤销，和侧栏 `PageExpandedSet` 同形，属视图状态
不入正文）；slash / Turn into / ⋮ 菜单各一行；`EditorBlock.slint` 的
chevron（`chevron-down` / `chevron-right`）；scene `toggle` + `toggle-fold`。

**projection 是这次真正的决定**：折叠的子树不产生 row。`project_blocks` 经
`visible_block_indices` 过滤，同一个函数也被新的 `drop_index_for_row` 用——
两条编号（row / model）从此不再等价，§八 的拖拽落点必须换算。这个缝只留
一个，防止两边漂移。`SetBlockType` 离开 Toggle 时会清掉 fold（undo 会还原），
否则子树被藏起来且没有任何出口；`SplitBlock` / `DuplicateBlock` 强制
`folded: false`，因为它们都不复制子树。Markdown 导出把 toggle 降级成 quote 行
（CommonMark 没有折叠语法，和 callout 的降级同一条路），子树仍按 depth 缩进带上，
导入侧不还原折叠——§三十七 要的就是这个不对称。

**验证**：`just check` 全绿（check --all-targets / 119+13+9+35+5+17+16+13 测试
/ release build 3m16s）。新增测试：v6 迁移（先 `DROP COLUMN folded` +
`user_version=5` 造旧库，带 control 断言证明夹具真的缺这列，再跑两遍验幂等）、
fold 往返 + 不存在 id 报错、折叠子树零 row、展开回到源顺序、编号列表不因隐藏
而改号、拖拽落点换算（并证明朴素的 row index 1 会被 `can_move_block_to` 拒）。
三处故意变异（关掉 filter、`can_fold` 恒真、关掉 SetBlockType 的守卫）各自
只打挂预期的测试后回滚——绿得不算数的测试我不留。

**像素**（headless `quire-shot`，`.scratch/sweep6`，基线 sweep5）：35 个旧
scene 里 32 个逐字节相同；只有 `slash` / `plus` / `dark-slash` 动了，原因就是
菜单多了 Toggle 一行。`toggle` vs `nest` 差 1 257 px，全在父块那一行（18 px
高的一条），即 chevron + 22 px gutter 生效且没有波及；`toggle` vs
`toggle-fold` 差 6 006 px，分成四块且每块都有解释：三角形字形、消失的子块
行、其下方整体上移一行、右侧滚动条滑块高了两像素。gutter 只按 kind 给、不按
`can-fold` 给，所以第一个子块出现时文字不会跳。

**未验证**：交互。桌面 UI 不归我点——chevron 的 hover 命中、折叠后继续打字、
⌘F 命中隐藏块、拖进隐藏区这几条要用户手测。另外 `blocks.folded` 目前没有任何
UI 入口能折叠「非 Toggle」的父块，这是刻意的（没有出口的状态不算状态）。

## M10 批次 A · slice 2 — image 块（2026-09-20，on `master`）

回到 §三十七 批次 A。这一条是 M10 里唯一会动摇低 RAM 卖点的 kind，所以
先做它、并且真的去量它，而不是等 table / columns 把媒体层逼到墙角再补。

**改动面**（§三十七 的六处接线，逐条对上）：`BlockKind::Image`（int 14，
`as_str` = `"image"`）；schema v7 = 新表 `attachments` + `blocks.attachment` +
`blocks.img_percent DEFAULT 100`；新的 `services/attachment_store.rs`（落盘、
嗅探格式、降采样）；`Command::InsertImage` / `SetBlockImage` / `SetImageWidth`
三个命令，各自 `Change` 可撤销；slash 与 Turn into 各一行 Image，⋮ 菜单对
图片块多开一个 `Image width` 子菜单（25 / 50 / 100，action id 基址
`IMAGE_WIDTH_BASE = 500_000`）；Markdown 导出 `![name](quire://attachment/<id>)`、
导入侧没有图片形状，整行按字面文本留在段落里（文件名因此不丢）；
`EditorBlock.slint` 的图片行 + `AppWindow.slint` 的点击放大遮罩层；
scene `image` + `image-half`。

**三个决定**：

1. **字节在库里不在库外**——`<library>/attachments/<id>.<ext>`，SQLite 只留一行
   引用。`blocks.attachment` 刻意**不建外键**：文件行丢了，块必须还能载入并画成
   「缺图」，而不是让整本库打不开（用户只拷走 `.db` 是常态）。这条由
   `a_picture_whose_attachment_row_vanished_still_loads` 用裸 SQL 删行钉住。
2. **编辑器永远看不到原图**。超过 `MAX_EDGE = 1280` 的导入会另写一张
   `<id>.cache.png`，`display_path` 只交这张；原始字节一个 bit 都不动，看画
   不该降级用户的文件。扩展名与 MIME 由 `image::guess_format` 嗅字节得出，
   不看文件名——存成 `.jpg` 的 PNG 要按它真的是 PNG 来存。
3. **解码缓存自己管，且有上限**。Slint 的 `i-slint-core` 图片缓存是
   thread-local `CLruCache`，按解码字节加权、上限 **5 MiB**、键是 path+mtime
   ——一张 1280×720 RGBA 就 3.7 MiB，即整个缓存装得下一张照片，滚过图页每帧
   重解码。反过来做一张无上限的自有 map，就是 §二十二 的 RAM 承诺死在第一条
   截图墙上。所以是 `AppState` 里一张按 attachment id 的 LRU，权重 w·h·4，
   上限 `MAX_ATTACHMENT_CACHE_BYTES = 32 MiB`（≈八张全宽帧）。行通过
   `image-for` / `image-aspect` **回调**取图而不是模型字段——Slint 只为它
   realize 的行求值，于是成本跟视口走，不跟文档走。

`Command::InsertImage` 的 apply 是 `AttachmentAdded`（`INSERT OR REPLACE` 的
upsert）+ `BlockInserted`，revert 只删引用、**没有** `AttachmentDeleted`：撤销
一次插入不该删磁盘上的字节，redo 也因此不需要碰盘。Image 块复制粘贴共享同一
个 attachment 指针而不是拥有两份；⋮ 的宽度设回默认 100 时 `plan()` 返回
`None`，菜单点不出空撤销步。

**验证**：`cargo test --workspace` 全绿（129 + 13 + 9 + 36 + 5 + 17 + 19 + 13 =
241 个测试，EXIT=0），`just check` 的 `cargo check --all-targets` 与 release
build（3m11s）在本 slice 的代码收尾处跑过，之后只动过文档。新增测试里值得点名
的：v7 迁移（先造 v6 旧库 + control 断言证明表和列真的不存在，再跑两遍验幂等）、
附件往返、悬空引用仍能载入、`attachment_store` 五个（含 `MAX_EDGE + 1` 卡在
分支边界上的降采样，以及「不是图片的字节被拒绝且不落盘」）、markdown 的
`quire://attachment/` 往返、command 层四个撤销用例。缓存那条
`the_picture_cache_spends_its_budget_and_drops_the_stalest_first` 先证明预算
**真的被花掉**（`spent > MAX/2`，否则断言在空缓存上恒真），再证明淘汰的是
最久没被 realize 的那张而不是最早插入的那张（把一张已淘汰的 id 重新 show 出来，
再灌四张新的，它必须还在、它的邻居必须不在）。这条测试抓到过一个真 bug：
`cache_image` 插入后没把新条目的权重计进 running total，于是 11 张全留着、
一个字节都没淘汰——断言把它逼出来了。

**RAM**（release `quire.exe`，`bench.ps1`，每场景 seed 遍 + 测量遍、独立 pinned
`--db`，原始行 `benchmarks/results/2026-09-20-m10-image-ram.jsonl`）：A 空壳
116.5–116.6 MB WS / 88.1–88.9 private；D 10 000 blocks 126.4–126.5 /
100.9–101.3，四遍的散布 ≤0.4 MB。对 M7 matrix 的同名两行（114.3 / 88.2 与
121.5 / 97.4）是 **1.02× 与 1.04×**，M10 的 ≤1.2× 门槛守住；idle CPU
0.2–0.59%。注意这一遍量的是「加了 v7 列、加了缓存字段、但页面上没有一张图」
的回归基线，它证明的不是图便宜，只是没有把别的东西弄贵。

**像素**（headless `quire-shot`，`.scratch/sweep7`，基线 sweep6）：37 个旧
scene 里 34 个逐字节相同；`slash` / `plus` / `dark-slash` 动了，裁出来就是
菜单尾部（Callout / Code / Divider 整体下移一行）和引用块，原因就是菜单多了
Image 一行。两张新 scene 是宽度档位的几何证据，不是眼看：同一张 640×400 的
fixture，`image` 画到 **760×474**、`image-half` 画到 **380×237**，左边和上边
都还在 x=390 / y 同一条线上——横竖都精确减半，比例没变形；两图差 339 118 px，
`image-half` 里图片下方已经回流进「Why a local-first editor」整段，而 `image`
那一屏只有图。行高由 `pic-aspect` 在拿到像素之前算出，所以半档那行是真的矮了，
不是被裁了。

**未验证**：交互。桌面 UI 不归我点——文件选择器（`rfd`）挑图、点图放大、
遮罩层点击关闭、⋮ 里 `Image width` 子菜单的命中，这几条要用户手测。RAM 也
只量了无图的场景；「一万块 + 一屏图」滚动时的 RSS 需要一个 bench 场景，
目前没有，32 MiB 上限是构造性保证 + 单测证明，不是测量结果。原地替换磁盘上
的同名文件要重启才生效（我们的键是 id，Slint 的键才带 mtime）。粘贴剪贴板
位图（`CF_DIBV5`，`platform/`）与 file / PDF 缩略图仍在本批次里没做，它们复用
同一个 store。

下一步仍是 §三十七 批次 A：file + PDF 缩略图（复用 attachment store），
然后批次 B 的 table + columns。

## M10 批次 A · slice 3 — file 附件块（2026-09-20，on `master`）

批次 A 的第二条。用户 2026-09-20 指定「pdf 附件的先跳过，做其他的」，所以
这一条交付 file 本体，PDF 首页缩略图留在原地。

**改动面**（§三十七 的六处接线）：`BlockKind::File`（int 15，`as_str` =
`"file"`）；**schema 不动**，`user_version` 停在 7——v7 为图片开的 `attachments`
行与 `blocks.attachment` 列描述一个 zip 和描述一张 png 一模一样；
`attachment_store.rs` 加 `import_any_file`（`fs::copy` 流式落盘）、`export_to`、
`save_name`、`format_size`、`create_file_fixture`；`Command::InsertFile` /
`SetBlockFile`，并且图片那两条的 `plan()` 助手被泛化成 `insert_attachment` /
`set_block_attachment`（只有 `kind` 一个参数不同），`AppState` 侧四个方法同样
收成两个带 kind 的 `insert_attachment` / `set_block_attachment`；slash、insert
（+）、Turn into 三个菜单各加一行 File，⋮ 的 `Image width` 子菜单**仍然只对图片
开**；Markdown 导出 `[name](quire://attachment/<id>)`；`Types.slint` 三个回调
`attachment-size`（pure）/ `attachment-opened` / `attachment-saved`，
`EditorBlock.slint` 的 kind 15 行；`platform::open_with_default`；scene `file`。
页脚字数把 File 和 Image 一起跳过——附件的 text 是文件名，不是散文。

**三个决定**：

1. **不加 schema**。这不是省事，是 v7 当初就按「一个块指向一坨存起来的字节」
   设计的，图片只是它的第一种用法。于是这一条 slice 没有迁移、没有幂等测试、
   没有旧库升级路径要验，`PRAGMA user_version` 那条 v7 迁移测试原样通过。
2. **字节不进进程**。`import_any_file` 用 `fs::copy` 把源文件直接抄进
   `attachments` 目录，只记长度：不解码、不调 `image`、**刻意不设体积上限**。
   图片那条路靠降采样守 §二十二，文件这条路靠「根本不看」守——一张 2 GB 的
   附件和一个 2 KB 的附件在编辑器里花一样的钱，直到用户去按按钮。
   `fs::copy` 返回的字节数与 `metadata().len()` 不符就删掉半成品并报错，
   所以不存在「半个附件」留在目录里。
3. **打开是显式按钮，不是点行**。一行点击就调用系统默认程序启动任意可执行文件
   的块，是不敢在里面选文字的块；所以行本身照旧走 `block-activate`（可选、可
   复制），右侧两个 26 px 按钮才做事。两个按钮**常画**而不是 hover 才出现：
   一个看上去没东西可按的文件块，读起来就是坏掉的附件，那是这类块唯一不能给的
   印象。`ShellExecuteW` 而不是 `spawn("explorer.exe", path)`——explorer 成功时
   也返回 0x1（它的设计），子进程分不出「拉起了应用」和「扩展名被拦」，而这个
   FFI 只在真拉起来时答 `> 32`；一个 extern 声明，`windows` crate 不进依赖树，
   跟 ADR-0025 读剪贴板同一条规矩。

**这一条里改过的两个设计**：文件的名字**保留扩展名**（`import_any_file` 取
`file_name()` 而不是 `file_stem()`），图片仍只留词干——一张图的内容就是它的
类型说明，一个 `quarterly-report` 什么都不说。这带来 `save_name` 的一个坑：
存进去的 `file` 列扩展名被小写过（`22.pdf`），`name` 保留用户大小写
（`Report.PDF`），拼回去时必须忽略大小写比较，否则给文件名加一遍后缀。
另一个是 fixture 的确定性：`create_file_fixture` 第一版把 `std::process::id()`
拼进文件名，而文件名就是行上那个 label，于是 scene 每跑一遍哈希就变一次；
pid 移到临时**目录**名上，文件名回到干净。这条是在看截图时发现的，不是靠眼看
判缺陷。

**验证**：`cargo check --all-targets` 干净；`cargo test` 全绿
**249 passed / 0 failed / 4 ignored**（11 个 target：lib 136、13、9、37、5、
17、19、13），`cargo build --release` exit 0。本 slice 新增 8 个测试：
`command.rs` 三个（文件块把文件名当 text、撤销只丢引用且 redo 不碰盘、
Turn into 换掉文字并还原、一张图和一个文件可以指向同一行 attachment）、
`attachment_store.rs` 四个（任意文件逐字节抄进去且从不解码、copy 失败不留半成品
含无扩展名回落到 `.bin`、`save_name` 的两种拼法、`format_size` 的读数）、
`markdown_test.rs` 一个（导出成链接且导入侧真的把 `quire://attachment/12` 带回来
——这是 file 与 image 在导出上唯一的方向性差别）。`attachment_store` 那条
`format_size` 测试当场抓到一个真缺陷：`bytes` 为负时 `< 1024` 分支打印的是原始
值而不是钳过的值，于是返回 `"-5 B"`；断言逼出来了。

**RAM**（release `quire.exe`，`bench.ps1`，原始行
`benchmarks/results/2026-09-20-m10-file-ram.jsonl`）：D 10 000 blocks
126.5–126.6 MB WS / 101.1–101.3 private，与 image 那遍的 126.4–126.5 /
100.9–101.3 差 ≤0.2 MB——**1.04×**，门槛 ≤1.2× 守住。A 空壳 119.2–120.7 /
91.7–92.5，比 image 那遍的 116.5–116.6 / 88.1–88.9 高 2.6–4.1 MB。**这个差
没有归因**：两遍 A 场景里都没有任何附件，不可能是文件行的钱，而且这一遍两趟
自己就散 1.5 MB（上一遍散 0.1 MB），说明空壳读数本身没那么可复现。写下来而不
是圆过去。第一趟 A（`m10file-A1`）因为 `-PinnedDb` 被 bash 吞成 `:TEMP\...`
而废掉，那一臂无法自证开了哪个库，已从 jsonl 里剔除。

**像素**（headless `quire-shot`，`.scratch/sweep8`，基线 `.scratch/sweep7` 的
39 个 scene）：36 个逐字节相同，`slash` / `plus` / `dark-slash` 动了，裁出来
就是菜单里多出 File 一行、下面整体下移，`slash.png` 全图能看到
Text / Toggle list / Image / **File — Attach a file of any type** / Callout 的
顺序。`file` 是新 scene，量出来的几何：盒子 x 390..1149（760 宽）、
y 265..312（48 高，正是 `size-body × line-body + 2 × spacing-md`），左边
`page` 图标 + 文件名，右侧右对齐 "1.8 MiB"，两个 26 px 按钮落在 x 1086..1112
与 1116..1142——按钮无 hover 时透明，所以列占用图上只有两处图标笔画（x≈1099、
x≈1135），这与「常画但只在 hover 才有底色」是两件事，别混。**新基线是
`.scratch/sweep8`（40 scene）**。

**未验证**：全部交互。文件选择器（`rfd`，All files 过滤器）、Open 按钮真的把
字节交给系统、Save-as 对话框给的文件名、两个按钮的 hover 命中、⋮ 菜单、
撤销/重做、Turn into 成 File、以及把窗口拉窄时 `width: parent.width - self.x -
140px` 那条 elide 会不会吃掉文件名——都要用户手测。另外两个已知没做的：
PDF 首页缩略图（按指示推迟，渲染路线未定），以及**附件永不回收**——没有外键、
没有级联，删掉最后一个指向它的块，字节还在 `attachments` 目录里；这是刻意的
（撤销要能不碰盘地找回引用），但对用户就是一条看不见的磁盘泄漏。

**下一步**：批次 A 收尾的仍是那张「一万块 + 一屏媒体」的滚动 bench 场景
（image 欠的，file 让它欠了两次），然后批次 B 的 table + columns。

## M10 批次 A · 收尾 1+2 — 临时目录卫生，和一条并不是死代码的 GIF 分支（2026-09-20，on `master`）

file 竖切之后那份盘点里的头两项，用户点定「先做 1+2，然后 commit，再往下走
table」。两项都不是功能，但第二条在做的过程中翻出一个前提错误，所以也记一笔。

**1. 测试在漏 `%TEMP%`。** 症状是 1648 个 `quire-log-*`；查下来是仓库级的问题，
不是一处：`logging.rs`、`storage/database.rs`、`storage/data_location.rs`、
`services/attachment_store.rs`、四个 `tests/integration/*`，加上三个 bench 脚本，
一共十几个前缀、3639 个条目。每个 helper 都建一个「pid + 计数器 + 时钟」的独
立目录并且注释里郑重解释为什么要唯一——**唯一性之所以必要，恰恰因为从来没人
删**。所以收法不是给每个 helper 补一句 `remove_dir_all`，而是加一个会自己删除
的守卫：`src/testing.rs` 的 `ScratchDir`（`Drop` 删树，`Deref<Target = Path>`
让 `dir.join(..)` / `&dir` 原样可用）。它必须是 `pub` 而不是 `#[cfg(test)]`：
集成测试是另一个 crate，编译 lib 时不带那个 flag，`tests/integration/**` 看不
到 `#[cfg(test)]` 的东西。

换下来的写法里有三处值得记：

* `scratch_logger` 原来返回 `Arc<Logger>`，现在返回一个把 logger 和守卫一起
  装进去的 `ScratchLogger`，靠 deref 让测试体一个字不用改。返回 `(Logger,
  ScratchDir)` 也行，但那会让 12 个调用点都变成解构。
* `Logger::new(scratch("clean")) 这种一行式必须拆开绑成变量：临时守卫在语句结
  束时就 drop，目录会在 logger 还在往里写的时候被删掉——不报错，只是下一行又
  把它建回来，于是白改。
* `backup_test.rs` 的 14 个测试每个末尾都有一句 `remove_dir_all(&dir).unwrap()`，
  它只在绿的时候执行；`persistence_test.rs` 里那段「先把上一轮的 `.db.bak<N>`
  删干净」的手扫，在目录名必然全新的前提下是死代码，一起删了。
* `data_location.rs` 的 `per_user_dir()` 原本返回 `PathBuf`，而它派生自的那个根
  目录才是守卫；两者拆开会留下悬空路径，所以并成一个 `Root { _appdata, per_user }`。

**量具**：`cargo test` 全绿（139 + 13 + 9 + 37 + 5 + 17 + 19 + 13 = **252**，
EXIT=0），跑完后 `%TEMP%` 里 `quire-*` 的总数从 3639 变成 **3639**，按 mtime
筛 10 分钟内新建的只剩 `quire-attachments/` 一条——那是 `for_db(None)` 的兜底目
录，固定路径、内容确定（`1.png` / `1.pdf`），不随运行增长，所以这次不动它。
被删掉的守卫自己也有测试：`testing::tests` 两条，一条证明绑定时目录在、离开作用
域后不在（断言写在 `Drop` 之外，否则自己给自己打分），一条证明同名两次调用不会
撞车。

**PS1 那一半**：`bench.ps1 -Typing` 和 `startup_bench.ps1` 原来只在跑之前清
`.db` / `-wal` / `-shm`，从不清跑之后。证据是对 `%TEMP%` 里 91 个
`quire-firstpaint-*` 分类：71 个 `.db.bak1` + 10 个 `.db.bak2` + 10 个
`.db.bak3`，全是应用自己写的首开快照，正好落在旧清理清单的缝里。现在两处都改成
`Get-ChildItem "$db*" | Remove-Item -Force`（整个文件族），`bench.ps1` 顺手删自
己的报告 json。`bench_matrix.ps1` / `profile_bench.ps1` 不动：它们的 scratch 是
单个固定目录（`quire-matrix` / `quire-profile`），有界且本来就是复用。

**2. GIF 分支不是死代码——我原来的判断是错的。** 盘点时我说 `ext_of` /
`mime_of` 的 `ImageFormat::Gif` 两条到不了，理由是 `Cargo.toml` 只要了
`png`/`jpeg`/`bmp`。查 `cargo tree -e features -i image`：`image feature "gif"`
**是开着的**，路径是 `slint "image-default-formats"` → `i-slint-core` →
`image "default-formats"` → `gif`（顺带 avif/exr/tiff/webp 全都在）。也即
`load_from_memory` 今天真能解 GIF，那两条分支活得好好的；按原计划删掉它们，反而
会把一张真收进来的 GIF 命名成 `.img` + `application/octet-stream`。

所以修法是反向的：把 `gif` 显式写进我们自己的 features（不额外编译任何东西，
但行的格式从此是我们声明的，不是依赖的选择），选择器加上 `gif`（否则那条分支
在 UI 里到不了，等于白留），并补一条测试 `a_gif_is_stored_as_a_still_gif`——它
用**同一个 crate 编码**一张 GIF 再导入，编不出来就直接 fail，而不是在一条走不到
的分支上恒真。gif 在这里是静图：解码器给首帧，App 里没有动画时钟，SPEC §三十七
新增的那条格式条目把这个边界写死了（webp / tiff 能解但不进选择器，因为它们的行
扩展名只能落到 `.img` 兜底）。

**docs**：SPEC §三十七 批次 A `image` 加格式条目；CHANGELOG 新增 `### Build &
test` 节（`just check`、`ScratchDir`、脚本清理）并在 Image 条里补格式；
PERFORMANCE scene E 脚注说明 harness 现在会删自己的库和快照。PLAN 就是本节。

**顺手清掉的历史遗留**：`%TEMP%` 里那 3639 个 `quire-*` 条目是这些泄漏的账本，
测试已经不再生产它们，所以按前缀列出来核对（1648 log / 621 test / 599 location /
447 persist / 124 search / 91 firstpaint / 71 typing / 10 backup，尾上是
matrix / dbg / imgbench / integrity / ab / lan / d9 / attachments 这些一次性探测）
后全部删除，现在 `ls $TEMP/quire-*` 是 0 条。没有 quire 进程在跑，所以删的都是
已完结会话的遗留。

**视觉**：这一轮没有改动任何会被画出来的东西，但 `create_file_fixture` 换了临时
目录的形状（pid 从名字里挪进目录名），而 `file` scene 的行标签正是从那次导入得出
的，所以复扫一遍 40 个 scene 对照 `.scratch/sweep8`：**changed 0 / identical 40**。
基线不变，`gif` 那条特性与选择器条目也就此确认没有把任何一张图挪动过。

## M10 批次 B · slice 4 — table 网格（2026-09-20，on `master`，ADR-0031）

批次 B 的第一条。SPEC §三十七 写死它是「简单表格，不是 Database」，所以这条 slice
的全部设计压力都落在一个点上：**怎么让网格不变成第二套内容模型**。

**改动面**（§三十七 的六处接线）：`BlockKind::Table` + `BlockKind::TableCell`
（`as_str` = `"table"` / `"table_cell"`）；schema **v8**，`blocks.columns INTEGER
NOT NULL DEFAULT 0`，`add_columns_column` 一步；`Command` 四条
`TableAddRow` / `TableAddColumn` / `TableDeleteRow` / `TableDeleteColumn`，但它们在
`plan()` 里全部摊成一串普通的 `BlockInserted` / `BlockDeleted` / `BlockTextSet`
外加唯一一个新 `Change`——`BlockColumnsSet`；`Types.slint` 加 `TableCell` struct，
`BlockRow` 复用 v8 那个 `columns: int`（kind 16 = table、18 = columns 的栏数）并加
`table-cells`；新组件 `TableBlock.slint`（画整张网格并
宿主那一个活的 `TextInput`）与 `TableEdge.slint`（hover 边缘条，Add row / Add
column / Delete row / Delete column）；`UIState.table-cell-move` 走 Tab /
Shift-Tab；slash、insert（+）、Turn into 三个菜单各一行；Markdown 导出 GFM 表格；
scene `table` 与 `table-edit`。

**四个决定**：

1. **格子是子块，不是 payload**。三条路里选的最窄的一条：JSON blob 会让格子不再是
   块，于是 inline marks、undo 粒度、§三十九 的 relation/rollup 全得在模型外面重造
   一遍；真·每行一条记录的 schema 是 §三十九 的活，而 SPEC 明说这个 kind **不是**
   数据库视图。子块只多花一列整数，其余全部复用。代价是 `rows` 不能存，只能派生
   （`cells / columns`），所以行主序的 `order` 是这套结构唯一的责任人。
2. **整张网格占一个编辑器行**。`visible_block_indices` 把格子从 rows 里删掉——
   与 ADR-0028 藏折叠子树同一个过滤器、同一个位置。这张表因此不论多大都只花一条
   delegate，而 §十二 的虚拟化前提不被破坏。
3. **加列是这轮唯一真正的难点**：它要在 `rows` 个不同的缝里同时插 `rows` 个块。
   `OrderKey::STRIDE` 定成 `1 << 16`、`renumber_page` 改用同一个 stride 铺开，就是
   为了这个批量——每插一个就取一次中点会把缝对半砍掉，缝很快就没了。新助手
   `keys_between` / `keys_in_gaps` 一次算完整批。
4. **残缺网格只读**。`grid()` 见到 `cells % columns != 0` 直接 `None`，`plan()`
   于是拒绝一切编辑——与其猜哪一行缺了，不如不动。`DuplicateBlock` 对这种表也拒绝
   复制，因为没有格子的网格一打开就是坏的。

**这一轮翻出来的一个老缺陷**：`DocumentRow.head` 绑了 `height` 却没绑 `y`，而
Slint 对这种子元素做的是**在父元素里垂直居中**。M1 起每个 scene 的页面标题就一直
画在它绑定位置的下面 ~34 px，只是以前的首块全是一行高，居中量看不出来；table 是
第一个「高的首块」，于是网格直接压在标题上（~60 px）。显式写 `x: 0px; y: 0px;`
修掉。同一条 scene 复扫时又发现 `absolute-position` 在 ListView delegate 里不可信
（row 1 报出自己的 head 在自己的 body 下面，而两者 `y` 都是 0），真实几何只能用
`parent.y + head.height`，也就是 `row-y` 已经在往下传的那个值。两条都记进
`docs/UI_ARCHITECTURE.md` 的「Slint geometry traps」。

**验证**：`cargo check --all-targets` 干净、`cargo test` 全绿、`cargo build --release`
exit 0。本 slice 新增的代表性测试：`command.rs` 侧 `a_line_becomes_a_grid_that_holds_its_words`、
`adding_a_column_puts_one_cell_in_every_row`、`cells_key_into_one_gap_as_a_batch`、
`a_ragged_grid_is_not_editable`、`every_row_index_consumer_shares_the_one_visible_list`、
`flattening_a_grid_gives_the_words_back_as_paragraphs`、`undoing_a_table_delete_brings_the_grid_back`；
`storage_test.rs` 的 `a_grid_and_its_cells_round_trip_through_storage` 与
`the_v8_step_adds_columns_to_a_v7_database`；`markdown_test.rs` 四条（GFM 带表头、
格子里的 marks 与转义、空格子导出成空、以及 `|a|b|` **导入回段落**这条反直觉的边界）。

**RAM**（release `quire.exe`，原始行 `benchmarks/results/2026-09-20-m10-table-ram.jsonl`）：
D 10 000 blocks 130.8–131.0 / 104.5–105.7 = **1.08×**，A 空壳 124.8–125.3 / 96.7–98.3。
门槛 ≤1.2× 守住。两个值得记的点：一是与上一批比 A +4.1…6.1、D +4.2…4.5，**两臂同向
同幅**，所以那笔移动不是表格的钱（空壳里一个网格都没有），记为 session drift；
二是这批里唯一可归因的 A/B——投影里 `table_cells` 不加 `kind == Table` 保护时，
每行都扫一遍页面并分配一个空 `VecModel`，D 高 ≈2 MB WS / ≈2.5 MB private，A 在自己
的散布内不动（空壳只有 24 行，付 24 次）。启动时间两臂都没动（475–525 vs 498–552 ms），
因为 `startup_ms` 停在窗口句柄那里，而投影发生在那之后——保护留着是因为内存读数加
渐近线，不是因为某个数字动了。

**像素**：`--hover x,y` 是这条 slice 给 `quire-shot` 加的能力（只发 `PointerMoved`
不按下），因为边缘条只活在 hover 上，这是唯一的 headless 路子。`table` /
`table-edit` 进 sweep，22 px 那条边缘条另有手工 `--hover` 一张。标题居中的修复
**重基线了 42 个 scene 里的 37 个**（`.scratch/sweep10`），判法是新工具
`benchmarks/scripts/diffbbox.ps1 -OldDir A -NewDir B`：逐 scene 打印变化像素的
bounding box，37 个框全部落在标题带里，于是一个结论管住整套，而不是看 37 张图。
投影保护那次复扫（`.scratch/sweep11`）**42/42 逐字节相同**，所以它是内存改动不是
视觉改动。

**未验证**：格子内 Ctrl+L（链接）没接；Enter 不拆格、退格不并格、上下键不跨行，
只有 Tab / Shift-Tab 走格；带 marks 的一行 Turn into 成表格时字留下、marks 丢掉；
hover 时边缘条作为一行挤进布局，所以表格下方内容在指针停留时下移 22 px。这些全写
进 CHANGELOG 的 Known limitations 与 SPEC 批次 B 的边界条目，不当缺陷处理。

**下一步**：批次 B 剩下的 columns。

## M10 批次 B · slice 5 — columns 分栏（2026-09-20，on `master`，ADR-0032）

批次 B 收尾。**这一条没有新迁移**：栏数就存在上一条为网格开的那个
`blocks.columns` 里。理由与 ADR-0030 对 v7 的说法同一个——「这个容器有几个栏」和
「这个网格有几列」是同一个事实，为一个改名付一次迁移不值。

**改动面**：`BlockKind::Columns` + `BlockKind::Column`；`Command` 三条
`ColumnsAddColumn` / `ColumnsDeleteColumn` / `ColumnsAddBlock`，同样摊成子块的
增删改 + 一个 `BlockColumnsSet`；`Types.slint` 两个 struct——`ColumnItem`（一个摊平
的 `[ColumnItem]`，带 `column` / `depth` / `kind` / `text` / `runs` / `checked` /
`number` / `folded` / `color` / `bg` / `attachment`）与 `ColumnBox`（`id` /
`column` / `first` / `size`，形状表）；新组件 `ColumnsBlock.slint` 与
`ColumnItemRow.slint`；`UIState.column-item-move`（Tab / Shift-Tab 在栏间走字）与
`column-fill`（空栏的点击）；slash / insert / Turn into 三处；Markdown 导出摊平成
页面级段落；scene `columns` 与 `columns-3`。

**三个决定**：

1. **layout 的内容画在 layout 自己那一行里**。Slint 没有递归组件，一个栏不可能装
   一个 `EditorBlock` 而后者又装栏，所以唯一能画的地方就是 layout 那一条 delegate
   ——这恰好就是 §三十七 那句「分栏只在可见窗口内展开」要的，也是 §十二 虚拟化前提
   要的。`visible_block_indices` 把栏和栏里的行一起藏掉。
2. **平铺交给 `FlexboxLayout`**（单行、`flex-wrap: no-wrap`、`alignment: stretch`、
   每格 `horizontal-stretch: 1`），不自己写宽度。SPEC 批次 B 本来就点名要这条。
3. **空栏必须能被点开**。一页上只有这一种东西是指针能命中而光标进不去的，所以它
   写「Empty column」，点它 = `ColumnsAddBlock` 给它第一个段落，而不是让点击静默。

**两条 QOML 的坑**（都进 UI_ARCHITECTURE.md）：`for … : if … : Component` 是解析
错误，所以「过滤后的重复」只能做成形状表让 delegate 自己切片（`ColumnBox.first` /
`size`）；组件实例化里 property 右侧的标识符是在**外层**组件解析的，所以子组件拿不到
自己父母的 `root.x`——layout 只能用普通名字往下传 `line-y` / `line-x` /
`each-width`。另外 ADR-0031 那套「指针认领」协议在这里跨了一次组件边界，变成
`in property pointer` + `callback pointer-claimed(int)`，因为 N 个子组件写两条
双向绑定指向父组件同一条 property，Slint 不让写。

**顺手修掉的一个真 bug**：`InsertBlockAfter` 以前把 `parent` 强制成 `None`，并且以
容器自己那一行作为落点。于是一个「+」或者「Paste below」可以把一个顶层块塞进容器
的子树里——而一页的块是按 `order` 排的一片平铺，**子树不连续的容器就不再是一个容器**：
`subtree()` 会走过去，所有按索引读的地方跟着一起错。现在它按锚点的 kind 分岔：容器
之后插在整棵子树之后，槽位（`TableCell` / `Column`）直接拒绝，其余地方继承锚点的
parent。这条同时把网格上同一个洞补了。另一个是「/」菜单的锚定写死了 350 px 的窗口
预留，这批加了一种块就溢出窗口下沿，改成按「+」菜单已有的办法量自己的高度。

**验证**：`cargo check --all-targets` 干净；`cargo test` 全绿 **288 passed / 0 failed /
4 ignored**；`cargo build --release` exit 0 且**零警告**。本 slice 新增的代表性测试：
`a_line_becomes_a_layout_that_holds_its_words`、
`adding_a_box_gives_it_a_line_and_deleting_one_reflows_its_words`、
`an_empty_box_takes_the_click_as_a_request_for_a_line`、
`editing_inside_a_box_stays_inside_it`、
`a_layout_copy_carries_its_own_boxes`、`undoing_a_layout_delete_brings_its_boxes_back`、
`a_box_is_not_a_prose_block`；`markdown_test.rs` 四条（按阅读顺序摊平、嵌套也摊平、
空 layout 导出成空、往返只丢形状不丢字）；`storage_test.rs` 的
`a_columns_layout_and_its_boxes_round_trip_through_storage`。

**RAM**（原始行 `benchmarks/results/2026-09-20-m10-columns-ram.jsonl`）：D 10 000
135.5–137.0 / 109.9–110.7 = **1.13×**，A 126.5–127.0 / 98.4–99.6 = 1.11×。门槛内，
但这是这批离门槛最远的一次。归因写得很清楚：投影只给 `kind == Columns` 的行建那两条
模型，其余行拿 `ModelRc::default()`——Slint 1.18 里它是 `ModelRc(None)`，
`i-slint-core-1.18.0/model.rs:891` 查过，**不分配**——所以一页没有 layout 时每行多
的只是两个 8 字节指针，一万行 ≈160 KB，而 D − A 涨了 ≈3.5 MB。算术到不了读数，
所以读数记成 drift 不认领。**而这已经是连续第四批同向漂移**（A 116.5 → 127.0，
+10.7 MB），于是门槛本身的方法成了结论：拿另一个 session 的矩阵当分母，比值里有一半
是 session。PERFORMANCE.md 里写下的是修法而不是放宽阈值——**下一条 kind 欠一个同
session 的 control build**（把 slice 前一个提交单独编到自己的 target dir，两臂一次测完），
ADR-0031 的投影保护就是这么定下来的。

**像素**：sweep 长到 44 个 scene（`.scratch/sweep12`）。`columns` 与 `columns-3`
是这对平铺场景，1 111 个差异像素全部落在 layout 自己那条带子里；另外三个菜单 scene
动了，因为列表多了一行。判法依旧是 `diffbbox.ps1` 的框，不是眼看 44 张图。

**未验证**：栏宽不能拖（Notion 的 resize handle 没做）、三栏以上不做、栏里再套 layout
不做；栏内 ↑/↓ 只动光标不出 layout，出栏靠点击。Markdown 导入侧本来就没有分栏语法。

**下一步**：批次 A 欠的三样（PDF 首页缩略图按用户指示仍推迟、剪贴板位图粘贴要在
`platform/` 里读 `CF_DIBV5`、孤儿附件的 GC），以及那条欠了两批的「一万块 + 一屏媒体」
滚动场景；批次 B 本身已收完。

## M8 tail · A4 收尾批 — 空页面、调色板分发、模态遮罩（2026-09-21，on `master`，ADR-0033 / ADR-0034）

M10 批次 B 收完之后，回到 M8 那条 A4 缺陷清单。这一批把里面**能靠代码关掉的四条**
关掉了，其中一条比它的标题严重得多。

**改动面**：`Command::AppendBlock { kind, text }`（`plan()` 里拒绝容器 kind，落点是
`page_blocks(page).last()` 的 order 之后）；`AppState::start_page()`；`UIState`
回调 `empty-page-started`；`Editor.slint` 空状态面板加 `TouchArea` + 换文案；
`on_title_commit` 里零行页面提交标题后顺手起一行；`PaletteAction` 枚举 +
`palette_action(id)` + controller 里那条**没有 wildcard arm** 的 `match`；
`AppShell.slint` 的 `modal-open` 与 `Colors.scrim`；Toggle Sidebar 从
`Control+B` 改 `Control+BackSlash`（`AppWindow.slint` 的 `KeyBinding`、Settings 的
`ShortcutRow`、palette 的 hint 三处一起改）；demo 页占位段落那句
「SQLite persistence lands with the next milestone」改成现在真实的行为。

**四条决定**：

1. **空页面自己造第一块，而不是在 create/open 时塞一块**。清单上写的是「文案过期」，
   实际是死路：每条插入命令都要一个锚点 `BlockId`，而 `create_page` 和 `open_page`
   都不播种子块，所以新建的页面**根本打不了字**。修法有三种——建页时塞一块、开页时
   塞一块、页面被明确要求写时自己长出来。前两种会让每次取消掉的「New page」在库里留
   一个空块，第二种还会让空状态永远不可达（并撞掉那条断言新页面投影 0 行的
   workspace 测试）。选了第三种：`AppendBlock` 是这条路的正门，一次撤销就能回到空。
2. **调色板的 id 空间在 Rust 里解码**。原来 controller 是 `match id { 1 => … }`，
   常量没 import 就在 pattern 位置变成 catch-all binding——`aaa3763` 之前 id ≥ 9 的
   每一行都在跑 `copy_current_page_markdown`，测试全绿，因为没有任何东西走这条分发。
   ROADMAP 里那句「durable repair 还没写」要的是一个覆盖命令注册表的测试，而测试要
   能存在，映射必须先是**一个函数**：`palette_action()` 返回枚举，`match` 不留
   wildcard，加一条命令忘了加一个 arm 就是编译错误。
3. **遮罩只给模态**。dialog / Settings / Add link 是模态，压 `Colors.scrim`；slash、
   「+」、⋮⋮、Move to、palette、search 是**锚定 popup**，故意不压——一个让你给某一行
   选类型的菜单，把那一行藏起来没有道理。锚定 popup 本来就靠 click-outside 自己关，
   全窗口那块 Rectangle 留着当 `UIState` 失同步的兜底，非模态时它是 transparent。
   规则写进 `docs/UI_ARCHITECTURE.md` §"Overlay layering"。
4. **Ctrl+B 只属于 bold**。同一个和弦在两处标着两件事，是清单上的 LOW；挪到 Ctrl+\
   而不是挪 bold，因为 bold 是别的编辑器都这么写的，而侧栏开关本来就少人记。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets` 全绿 **293
passed / 0 failed / 4 ignored**；`cargo build --release` 零警告。本批新增测试：
`an_empty_page_takes_one_paragraph_and_undo_empties_it_again`、
`appending_lands_below_a_containers_whole_subtree`、
`a_grid_or_a_layout_is_not_a_bare_append`、
`every_palette_row_resolves_to_its_own_action`（走 `mock_commands()`，断言每行都
解析到**自己那条**动作、非页面动作互不重复、没有一行落到 `None`）、
`a_jump_row_carries_its_page_id_and_an_unknown_id_does_nothing`（0 / 14 / 9 999 / −1）；
`workspace_test.rs` 里 `create_then_open_page_lands_empty` 现在接着断言
`start_page()` 恰好给出一行、且 id 与返回值一致。

**像素**：`.scratch/sweep13` 对 `.scratch/sweep12`（44 scene）——38 张字节相同，
6 张动了，每张的 bbox 都能被这条改动解释：`empty` 724 个采样像素全在
x 506..1032 / y 456..466（只有文案那一行）、`palette` 7 个像素（hint 的和弦）、
`link` / `dialog` / `settings` / `dark-link` 整窗（遮罩；`settings` 另外因为多一条
`ShortcutRow` 长了一截）。行为不看像素，看那条注册表测试——**这次修的东西本身只动了
7 个像素**，正好是「绿的视觉门只证明画法、不证明行为」的现场教材。
`--scene empty --click 640,458` 单独一张（`.scratch/a4fix/empty-clicked.png`）证明
点击真的建出了带光标的聚焦首行。改完 demo 文案后再扫一遍（`.scratch/sweep14` 对
sweep13）：**0 / 44 动**，所以 ADR 里那句「这段字在任何 scene 里都不出现」是读数而不
是断言。

**RAM**：本批不欠 bench——没有新 kind、没有新 schema 列、没有新的每行状态，
`AppendBlock` 只在零行页面上跑。批次 B 欠的那个**同 session control build** 仍然欠着，
记在下一条 kind 头上。

**未验证**：`--click` 那张图没有走真实键盘，输入第一句话之后的换行/撤销链没测；
Enter-in-title 起行只测了「零行页面」这一支，非空页面提交标题不动块列表这件事靠的是
`row_count() == 0` 这个条件本身；遮罩在 dark 下几乎看不出来（背景本来就暗），好不好看
要人眼看一次；A4 剩下的 1 HIGH（带 inline mark 的段落不折行）、1 MEDIUM（find 在失焦
块上算得 16 次却一个都不画）、LOW（最暗提示行、最弱配色对）与 `renderer_name()` 那条
编译期限制，都不在这批。

**下一步**：先请用户手测一次（新建页面 → 点空面板 → 打字 → Ctrl+Z；Ctrl+\ 开合侧栏；
打开 Settings 看遮罩），然后进批次 C 的第一条。批次 C 的两条硬限制在开工前已经查清：
Slint 1.18 没有 `Text.rich-text`，语法高亮只能走已有的单行 runs 通道；TOC 的点击跳转
没有可靠机制（没有 focus 自动滚动，`ListView.bring-into-view` 假设等高行）。

## M10 批次 A · 剪贴板里的截图能粘进 Quire 了（2026-09-21，on `master`，ADR-0035）

批次 A 收尾。image 那一栏只剩一条写在 SPEC §三十七 和 CHANGELOG 里的空缺：
**从剪贴板粘贴**。这一批把它补上，顺带把 §二十七「clipboard rich content」的位图
那一半也交了。

**改动面**：新文件 `src/platform/dib.rs`（DIB → RGBA → PNG，里面没有一行 Win32）；
`src/platform/mod.rs` 加 `read_clipboard_image()`（`CF_DIBV5` 优先、`CF_DIB` 兜底，
和文字读取一样的五次 `OpenClipboard` 重试，按 `GlobalSize` 取字节）；
`AppState::paste_image()`；`on_rich_paste` 里「没有文字才轮到图片」那一支。没有新
依赖：`image` 的 png 编码器本来就是附件存储在用，clipboard 依旧手写 FFI（ADR-0025
的后半，不是推翻它）。

**四条决定**：

1. **解码与 FFI 分成两个文件**。这不是洁癖：DIB 的头、位掩码、调色板、行对齐、
   bottom-up 全是别的进程说了算，每一条都是解码器的坑；而这一切都不需要剪贴板。
   分开之后八个断言测试在任意机器上跑自己造的 DIB，既不碰用户真正的剪贴板，也不
   违反这台的规矩（不脚本点桌面 UI：不伪造按键、不抢前台）。唯一需要真剪贴板的那条
   是 `#[ignore]` 的。
2. **文字优先于图片**。富文本应用的「复制」会同时往剪贴板放 `CF_UNICODETEXT` 和
   位图，这时候粘出来的是段落还是段落的一张照片，是行为决定而不是实现顺序。段落变
   成图片就把可编辑的东西弄丢了，所以图片只在「根本没有文字」时才上场。
3. **截图的 alpha 是全零的垃圾字节**。32 位 `BI_RGB` 位图没有 alpha 通道，第四个
   字节通常是 0，而 PNG 会保留透明度——照抄过去就是一张**看不见的图片**，用户读到
   的是「粘贴失败」。修法只在「声明了 alpha 且整个 alpha 平面全零」时把整张涂成不
   透明，只要有一个字节非零就原样保留真实的透明。两条测试各钉一边。
4. **落点跟着光标，绝不覆盖已写的字**。空块直接*变成*图片，有字的块在下方得到图片，
   一次撤销一步——和「/」「+」那两扇门的形状一致，粘贴永远不在页面上留一行空壳。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets` **302 passed /
0 failed / 7 ignored**（较 24a3432 的 293 增 9 条断言测试，ignored 从 4 到 7 是三条
打印型测量 + 一条读真剪贴板）；`cargo build --release` 增量 3m21s、零警告。新测试：
`dib.rs` 八条（bottom-up 翻转、行补齐、全零 alpha、真实 alpha、565 与 555 的掩码
区别、`biClrUsed`=2 与 256 色表、1 bit MSB 优先、截断与 RLE 载荷直接拒绝）、
`state.rs` 一条（在 ScratchDir 里开**真库**，断言 kind 与 id、附件名/MIME/字节数、
`attachments/` 下确有文件、下方段落文字仍在、撤销后引用没了字节还在盘上）、
`platform` 一条 `#[ignore]` 读用户剪贴板——它是唯一能证明 FFI 读到的是**别的进程**
写的字节的证据，本机跑出来 `clipboard picture: 1280x800 from 134168 PNG bytes`。

**性能**：这批欠的不是 RAM 臂（没有新 kind、没有新列、没有新的每行状态），而是一次
按键的延迟，所以量的是延迟，写进 `docs/PERFORMANCE.md`：DIB→RGBA 在 1080p / 4K 上
19 ms / 74 ms，存储那条腿（PNG 解码 + 降采样 + 缓存编码 + 两次写盘）在刻意不可压缩
的位图上 59 ms / 196 ms。两头都标了是地板还是天花板：单色图形的 PNG 编码只有 1 /
6 ms，真实屏幕内容比它贵；随机噪声比任何真实截图都贵。

**像素**：`.scratch/sweep15` 对 `.scratch/sweep14`，**44 张全部字节相同**。这批一行
UI 都没改，这条 0/44 就是那句话的读数而不是断言。行为不靠像素——那八个解码测试和
那条 state 测试才是门。

**未验证**：真人在 Quire 里 Ctrl+V 一张截图（Snip-and-Sketch 或 PrtSc）还没跑过，
这是本机规矩要请用户手测的一条；`GlobalLock` 那次拷贝本身没计时；4K 之外、以及
`CF_DIBV5` 带真实透明通道的应用（Photoshop 一层透明 PNG）粘进来长什么样，只有单元
测试的合成位图覆盖，没有真应用验证过。

**批次 A 的账现在只剩两条**：PDF 首页缩略图（2026-09-20 用户指示推迟，渲染路线待定）
和孤立附件的回收（没有外键也没有级联，删掉最后一个指向某段字节的块，字节就永久留在
`attachments/` 里）。另外「一页图片被滚动时」的实测从 image 那批一直欠到 columns 那
批，现在第四批仍然欠着，而且批次 B 欠的**同 session control build** 还没跑。下一步按
这个顺序走：先补媒体滚动的 bench scene（把那笔三次点名的欠账变成读数），再收孤立附
件回收，然后进批次 C——批次 C 六条里只有 math 不撞平台墙（高亮要单行 runs 通道、TOC
要跳转、bookmark / embed 要有 TLS 客户端），开工前再确认一次。

## M10 · 媒体 bench scene，和那条从来没滚动的 scene F（2026-09-21，on `master`，ADR-0036）

这批交付的是**量具**，不是功能。写它之前，从 image 那批起的每一批结尾都留了同一句话：
一页图片被滚动时的代价没测，32 MiB 解码上限只是「构造加上
一条单测」而不是读数。这批把它变成读数——然后发现那个读数本来就不成立。

**改动面**：`HandleArgs` 多一个 `pictures`，`AppState::new` 在**空库**时用
`create_fixture` 播 200 张 1280×720 的 PNG 进 `attachments/`，`bench_pictures` 把 bench
页每 `stride` 行变成 image 块；`attachment_cache_peak` + `attachment_cache_report()`
让应用自己在 `--dump-state` 时往 stderr 打一行 JSON（`bytes` / `peak_bytes` /
`entries` / `budget` / `scroll_y`），`bench.ps1` 把它内联成 jsonl 的
`attachment_cache` 字段；`main.rs` 多 `--pictures` 与 `--scroll-step`，
`quire_shot` 多 `--scroll-y`；`bench_matrix.ps1` 加四个 D+F·P scene，跑前清掉
`attachments`。零 UI 改动，零 schema 改动。

**四条决定**：

1. **缓存由应用自己报，不由进程计数器猜**。工作集看得见 Slint 的路径缓存和纹理上传，
   看不见我们那个以「一张栅格」为单位键的 LRU；一个看不见的上限不可能被读数否定。
2. **fixture 只在库是空的时候写**。否则 measured pass 计的是 PNG 编码而不是图片。
   这条不是靠注释保证的：`the_pictures_scene_seeds_its_pool_once_and_then_only_loads_it`
   在两次 `AppState::new` 之间**删掉一个文件**，如果第二次把它写回来，那条断言就是为
   错误的原因通过——所以它测的是加载路径而不是计数。
3. **池上限 200，不是每张一个文件**。5 000 张各不相同的 1280×720 PNG 会让 seed pass
   变成磁盘与预热的人质，而缓存分辨不出区别——它按字节权重淘汰，不按身份。图片行 k
   取 fixture `k % 200`，相邻图片行仍是不同栅格，这正是缓存要按之定容的东西。
4. **stride 和 pool 只有一个来源**（`bench_picture_plan`），建行的和解种的都是问它，
   两处不会漂。

**真正的发现：scene F 从 M2 起就没滚过。**先不当它是坏仪器、也不当它是阴性读数——
前两批跑出「5 000 张图的 10 000 行页，8 秒后缓存没动、`scroll_y` +840」，把步长从
8 调到 200 反而**每秒移动得更少**，一个慢渲染器不会这样。于是报告里加 `scroll_y`，
`quire-shot` 里加 `--scroll-y`，用像素把它钉死：`--scroll-y 0` 和 `--scroll-y 2000`
产出**逐字节相同**的 PNG，只有 `-2000` 不同。Slint 的列表 content offset 向下为负
（它自己按 `round(-content-y / item-height)` 算首个可见行），而 M2 装的定时器一直在
**加**。这就是为什么之前两批 36 行里凡是 F 的行都不作数：删掉、在修好的二进制上整批
重跑成 27 行；污染的那份留在 `%TEMP%\media-ram-pre-fix.jsonl`，作为「错的是仪器不是
应用」的记录。修法除符号之外还要一个 stall 计数——ListView 只知道自己已经 realized
的行有多高，所以 clamp 比滚动慢一帧，一个卡住的 tick 不等于到底了。

**读数**（`benchmarks/results/2026-09-21-m10-media-ram.jsonl`，九个 scene 一个 sitting，
详见 `docs/PERFORMANCE.md` 末节）：32 MiB 预算**按字节权重停在 9 张栅格**，
33 177 600 / 33 554 432 = 98.9 %，而且 500 张、5 000 张、1 000 行、10 000 行四个形状
停在同一个数字上——上限在视口能显示第十张之前就已经用完，所以它不需要抬；一页你没滚
过去的图片和没有图片无法区分（0 次解码，D+500 = D）；而**屏幕上**一张图片约值 9 MB
进程内存，不是它栅格的 3.7 MB——所以约束照片页的是视口不是缓存，这条进了 §二十二
该带的算术。gate 本身同一 session 重跑：D 1.12× / A 1.10×，媒体臂**故意**留在 gate 外。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets` **305 passed /
0 failed / 7 ignored**（比上批 302 多三条）；`cargo build --release --all-targets`
零警告。新测试：plan 的四点（0 行、`--pictures 0`、两个真实 scene 的 stride、要的比行
数还多不能凭空造行）、250 张图落在 1 000 行上的形状（stride 不是 off-by-one、图片行
是唯一带附件的行、栅格互不相同、池耗尽才 wrap、图片行没有文字可排）、上面第 2 条那条
种子/加载。全程 headless：真进程 bench + `quire-shot`，没有伪造按键、没有抢前台。

**像素**：`.scratch/sweep16` 对 `.scratch/sweep15`，**44 张全部字节相同**——这批一行
UI 都没动。但这批真正的像素证据是 `--scroll-y` 那两张：同一份代码、两个都「合法」的
偏移、一张图都不同才叫滚动。绿门只证明没画坏，不证明量具在量。

**同日跟进 · 把 verdict 欠的那半句也量了**：修好的 scene F 不能只有一边有数。
`benchmarks/scripts/scroll_ab.ps1` 一次 sitting 里跑两个二进制（`target\release` 与
`--no-default-features --features skia-opengl --target-dir target-skia` 的那一个），
scene 在外、arm 在内，让机器漂移同时移动一对里的两边；开工前**两臂各自自证身份**——
读自己 `first_paint` 行里的 `renderer`，不对就整批停，因为一个忘了关默认的 skia 构建
会渲染成 femtovg，把 A/B 变成同一个二进制跟自己比。结果（`benchmarks/results/`
`2026-09-21-m10-scroll-skia.jsonl`，每臂一次 seed + 两次 measured）：方向留着，尺寸
缩水——skia/GL 滚起来确实便宜，滚轮 ≈18 %、flick ≈21 %，不是 M7 那行声称的三分之一
（那两个数都是在页面没动的情况下量的），代价是 +34…43 MB 工作集，而它的 private
字节在 flick 臂上反而比 femtovg 低 6 MB（惩罚在共享的 GPU 映射页，不在堆上）。所以
**femtovg 仍然是默认**，理由跟以前一样，只是「scroll 重负载可以换 skia」从继承来的
句子变成了证过的、更小的报价。另一条意外：skia 臂报出的仍是 **9 张栅格 /
33 177 600 peak**，和 femtovg 一模一样——LRU 在 `AppState` 里，在任何渲染器之上，
所以 §三十七 的媒体上限不随渲染器动。控制点：本批 femtovg 臂（150.4–152.0 MB /
图片页 78.0–78.5 %）复现了当天早上那一批独立跑出的（150.0–150.8 MB / 77.4–78.6 %）。

**量具的第二个 bug，是在补 scene 时撞出来的**：把媒体批次里两条手跑的 arm 加回
`bench_matrix.ps1` 之后，用 `-Only A,B` 去只跑它们，脚本**什么都没写、退出码 0**。原因
是 `powershell -File` 把逗号列表当成**一个字符串**塞给 `[string[]]` 参数，于是一个 scene
都没匹配上。现在它自己切 token，并且在「一条都没匹配」时 throw——过滤器匹配不到东西不是
一次测量。顺手把两条 scene 补齐（文本页的 flick、1 000 行的图片页），发布过的每一行现在
都能从脚本重跑；两条都验过：156.3 MB / 85.78 % 与 184.2 MB / 9 张栅格，和早上那两条独立
跑的对上。

**未验证 / 欠账**：真实照片的熵
（fixture 是渐变，而且 `create_fixture` 不随 id 变像素，所以 200 个名字后面是同一张
图——这里的解码数字全是地板）、HiDPI、图片夹在段落之间而不是等距排布；孤立附件回收仍
欠；批次 B 欠的**同 session control build**——这批在同一 sitting 里对照了 A/D/媒体三个
形状、也对照了两个渲染器，但那不是「切片前一版编译出来对照」，仍然欠到下一条 kind 头
上。skia 的 typing WS 漂移（M7 那条 320 MB vs 128 MB）没解释也没重跑，默认是 femtovg
期间它不咬人。

**下一步**：请用户手测两件事——截图后 Ctrl+V 进 Quire，以及现在这台机器上真实滚动的
顺不顺（scene F 修好之后，长页滚动第一次有了诚实的 CPU 数：8 px/帧约 37…43 %，
200 px/帧约 86…87 % 单核，两个渲染器都是这个量级。CPU 数不是流畅度，harness 看不见
掉帧，所以这一条只能由人眼定）。之后按 docs 上剩的账走：孤立附件回收，或进批次 C 的
math——批次 C 六条里只有 math 不撞平台墙。


## M10 批次 A · 孤立附件回收：Settings → STORAGE → Reclaim（2026-09-21，on `master`，ADR-0037）

批次 A 剩的两条账，这次收的是**孤立附件**那条（PDF 首页缩略图仍按 2026-09-20 的用户指
示推迟）。

**改动面**：`Change::AttachmentDeleted { id }`（`core/persistence.rs`，只有回收会发它，
任何命令计划都不会），以及新的单一读者 `attachment_ids_in()`；`Document::all_blocks()`；
`History::referenced_attachments()`（历史模块第一次对外界交代栈里有什么）；
`SqliteRepository` 的 DELETE 分支；`AttachmentStore::remove()`（`file` 与 `thumb` 都处
理，`NotFound` 不算失败，删不掉的文件名回传给提示条）；`AppState::reclaim_attachments()`
+ `evict_image()`；`controller.rs` 的 `on_reclaim_attachments`；`ui/Types.slint` 一个
callback；`SettingsDialog.slint` 的 STORAGE 行多一个 **Reclaim** 按钮和一行说明。没有迁
移，schema 停在 v8。

**四条决定**：

1. **可达集合故意比屏幕上有的宽**：所有页面的所有块 ∪ 每一页 undo **和 redo** 栈里的每
   一个附件 id ∪ 复制板上那一块。因为两个失败方向不对称——留下孤儿只损失磁盘，删掉一张
   Ctrl+Z 正要还原的图损失的是用户的数据。
2. **顺序就是内容**：先 `force_flush()` 把防抖队列写空，再同步删行，最后删文件。反过来
   做，一条还躺在队列里的 `AttachmentAdded` 会在 DELETE 之后重放，造出一行**指向已经不存
   在的文件**的永久引用。写失败就整体放弃；删文件失败只是留下没有行认领的字节，下次扫描
   还会清掉。
3. **绝不列目录**。这条看着最省事、也最致命：如果这一次 `load_attachments` 失败，内存里
   的账本是空的，磁盘上每个文件都「没人引用」，于是「我看不见引用」被回答成「把它们全删
   了」。回收只读账本，所以加载坏掉时它删不掉任何东西。代价写在 Known limitations：**没
   有行的文件回收不了**。
4. **100 步上限就是承诺的边界**，不写「永远」。`both_stacks_protect_and_the_cap_ends_the_
   protection` 把这条边界钉住（100 步全保，第 101 步松开最早那个 id），而按钮旁边那一行
   字必须先被读到——提示条是一次 toast，不是一句警告。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets -- --skip
clipboard_write_and_read_round_trip_unicode` → **312 passed / 0 failed / 8 ignored**（剪
贴板那一条见下；它跑起来的 session 就是 313 passed / 8 ignored，比上批 305 多 8 条断言 +
1 条打印型计时）；`cargo build --release --all-targets` 零警告。**一条与本切片无关的失
败**：`platform::tests::clipboard_write_and_read_round_trip_unicode` 现在这台机器上过不了，
`copy_to_clipboard` 返回 false。原因不在我们的代码里：一段**不经过本仓库任何函数**的 C#
探针（`user32!OpenClipboard`）在这个 shell 里连续 12 秒每次都是 `ERROR_ACCESS_DENIED`
（5）、`GetClipboardOwner()` 为 0，也就是当前有别的进程独占着剪贴板；而 `git diff` 证明
`src/platform/*` 这批一行都没改。所以它是这个 session 的失败，不是构建的失败——Quire 自己
的 Ctrl+C 现在同样会静默失败，这一点值得用户留意（不是这批引入的）。新测试：`history.rs`
1 条（两条栈都保护、id 只数一次、CAP 收尾）；`state.rs` 6 条，
全部在 `ScratchDir` 里开**真库**——撤销能还原的图不动、重启之后新 session 能回收前任删掉
的那张、复制板上的图在它那一页被删后再活下来、同一 session 里删整页即可回收（不用重启）、
行还在文件已经没了的孤儿也把行删掉并且报出那个文件名、内存态 session 直接答「无可回
收」；`storage_test.rs` 1 条（DELETE 幂等，且删行不碰块上那条悬空引用）。控制断言：先证
明这一页**确实带着那一行图**再断言它活着，否则「什么都没删」会因为「什么都没加载」而通
过。

**性能**：这批欠的不是 RAM 臂（没有新 kind、没有新列、没有新的每行状态，bench scene 看
不见它），而是**一次点击在 UI 线程上冻多久**——回收是这台应用里唯一一个在 UI 线程上扫磁盘
的功能。所以量的是那个：`a_reclaim_of_a_thousand_orphans_is_timed`（`#[ignore]`，打一行
JSON）在 `ScratchDir` 里造 1 000 张「有行、有文件、没有任何块指向它」的附件，再计时。存进
`benchmarks/results/2026-09-21-m10-reclaim-timing.jsonl` 的那三个 sitting：**549.9 /
556.7 / 567.6 ms**，每张孤儿约 0.55 ms；另有一批独立先跑的读数 530.6 / 542.3 /
573.0 ms，批间漂移和批内一样大。同一句调用在空账本上是 **0.0 ms**，所以可达性扫描没有在时
钟上，那 0.55 s 全在「1 000 条 DELETE 装一个事务 + 1 000 次 `remove_file`」里，两者谁占多
数没归因。控制断言在数字前面：先证文件夹里确有 1 000 个文件、表里确有 1 000 行，扫完再证
两者都为 0——否则「快」和「这里本来就没东西」无法区分。顺带一个仪表器的教训：第一次跑它用
了 `--exact` 加裸测试名，**一个测试都没匹配、打印 0 passed、退出码 0**，看着像绿了。写进
`docs/PERFORMANCE.md`。

**像素**：`.scratch/sweep16` → `.scratch/sweep18`，44 张里只有 `settings.png` 变，而这一变化正是这次要加的按钮。中间那一版（sweep17）是一次真缺陷，而且是像素抓出来的：同一个
flag 下第三个 `visible:` Button 和被隐藏的说明 Text 都**留在布局里占位**——隐藏的两行文
字吃掉约 40 px 高度，隐藏按钮抢走标签宽度，于是「Running in memory — no database
attached」被截成三个词（bbox `x 440..838 / y 0..798`，10 908 px）。改成条件子元素
（`if UIState.storage-available : …`）后消失，规则写进 `docs/UI_ARCHITECTURE.md` §"Slint
geometry traps"。headless 拍不到新按钮本身：`quire_shot` 建的是 `AppState::new(&args,
None)`，`storage-available` 恒为 false。

**未验证**：①**真人点一次 Reclaim**。headless 拍不到这个按钮——`quire_shot` 建的是
`AppState::new(&args, None)`，`storage-available` 恒为 false，所以带真库的设置对话框只能由
人眼确认（按钮在不在、那一行说明读不读得清、提示条报的数字对不对）。②真实库里孤儿的形
状：计时用的是 1 000 个 24×18 的小 PNG（173 KiB 总），文件名与数量都对，但真实附件的**大
小**分布没有进过这条测量；删文件的代价随数量走、不随大小走，所以这条影响的是磁盘回收的
字节数而不是延迟。③Windows 上「文件被别的程序占用」那条分支（`remove` 返回卡住的文件
名）只有单元测试的合成形状，没有真的锁住一个 `.png` 再扫一次。

**批次 A 的账现在只剩一条**：PDF 首页缩略图（2026-09-20 用户指示推迟，渲染路线待定）。
另外批次 B 欠的**同 session control build** 仍然挂着——回收这一批不改任何 delegate，所以
这次没有把它再欠一遍，但也没有还。下一步按 SPEC §三十七 进批次 C（highlight / bookmark /
embed / math / TOC / synced block），批次 C 六条里只有 math 不撞平台墙（高亮要 inline runs
的单行通道、TOC 要跳转、bookmark / embed 要有 TLS 客户端），开工前再确认一次。

## M11 批次 C · slice 1 — math：公式存源，图片是导出路上算出来的（2026-09-21，on `master`，ADR-0038）

批次 C 六条里那条不撞墙的。SPEC §三十七 要「LaTeX 子集」并把标准写成「渲染优先 Unicode
近似排版」，同时规定**引入排版引擎必须先出 ADR 并附内存数字**——走 Unicode 近似就跨过了
标准而没有触发那道闸：没有引擎，也就没有欠的内存数字。

**改动面**：新文件 `src/core/math.rs`（`to_unicode`，唯一的渲染器）；`core/mod.rs` 挂模块；
`core/types.rs` 的 `BlockKind::Math`（`ALL` 20→21）和 `MarkKind::Math`；`app/state.rs` 的行
号 20、slash / insert 菜单各一行、以及 `build_runs` 在**投影**时把公式 run 换成字形；
`app/controller.rs` 的 `"$$ "` 行首快捷键、`on_math_render`、`on_toggle_mark` 的 arm 4、两
个新 bench scene；`services/import_service.rs` 的 `$$ … $$` 围栏 + 行内 `$…$` + TeX 的
flanking rule；`services/export_service.rs` 的围栏、`dollar_pair_ahead` 转义与 span 排序；
`ui/Types.slint` 一个 `pure callback math-render(string) -> string`；`EditorBlock.slint` 的
公式框；`SettingsDialog.slint` 的快捷键行。**没有迁移**，schema 停在 v8：kind 与 mark kind
在库里都是字符串（ADR-0030 那套论证），没有 CHECK 列表要放宽，而未知 kind 依旧是「读到就
报损坏」，所以旧构建打开一个带公式的库是响亮地失败，不是悄悄丢行。

**七条决定**：

1. **存源不存字形**。`blocks.text` 里是 `\frac{a+b}{2}`，不是 `(a+b)/2`。这让渲染器可替换：
   将来引擎到位是换一个函数，之前写下的每个库都不需要迁移、重导出或重建搜索索引；它也保住了
   `text <=> UIState.editing-text` 双向绑定的诚实（用户编辑的是公式，永远不是它的图片），以及
   Markdown 的往返（而不是一次性渲染）。
2. **渲染器三条契约**，测试钉的就是这三条：输出**永远不丢用户打下的东西**（未知命令原样回
   来，最坏读成「这条没渲染出来」而不是「这条不见了」）；源里的空格**是内容不是语法**（TeX 在
   数学模式丢空格，这里不丢——在单行近似里，用户打的那个空格是他间距唯一的幸存表达，
   `\alpha + \beta` 和 `\alpha+\beta` 故意不同）；**幂等**（所以每行每次绑定求值都能重derive而
   不漂移）。
3. **派生绑定是按元素的成本，不是按 kind 的**。`visible: false` 的 `Text` 照样求值它的绑定，
   所以公式那行读 `is-math ? UIState.math-render(…) : ""`；不加守卫，一页 10 000 行会为了 1
   行公式调用渲染器 10 000 次。行内公式同理，但走的是投影而不是绑定：绑定是每帧重付。
4. **公式 span 是它那段字节上最外层的东西**（`kind_order` / `mark_order` 里 Math = 5）。被它
   包含的 mark 在导出时丢掉，与它**共享边界**的 mark 也丢掉——因为 `$**a**$` 会把两颗星导进公
   式源里；首尾带空格的源没有 `$…$` 写法（importer 要求两侧都是非空格），于是 mark 走、字留。
5. **`$` 的守卫是成对的，两半是同一个谓词**。import 只开 TeX flanking rule 允许的对（紧跟非空
   格、闭合前非空格），所以「costs $5 and $10」是散文；export 只在真的会重新成对时才转义
   （`dollar_pair_ahead`），所以散文里的美元符号不必每个价格前面挂一个 `\$`。`$$ … $$` 围栏像
   代码围栏一样逐字，因为 `\alpha` 必须带着那一个反斜杠回来。
6. **空公式也要看起来是个公式**：无文本时渲染 `$$`，行有高度、读起来是一个槽位而不是一条空白
   带；而 kind 仍在可编辑集合里，所以点它就把源开在活的 TextEdit 中、渲染 Text 自己让位。
7. **21 个 kind 这条事实是核对过的**：`BlockKind::ALL` 从 20 变 21，而 `docs/ROADMAP.md` 里
   那句「16 个 kind」是 columns 批次留下的旧账，一并改到 21。

**顺带抓到一个真缺陷**：`math-inline` 是这一集里第一条**短到在自己框里留下余量**的带 mark 的
行，而那个余量把三处 run 行的布局问题照出来了——`HorizontalLayout` 的默认 `alignment` 是
`stretch`，每个 item 的 `min-width` 钉在 `Text.preferred-width`、`horizontal-stretch: 0`，这套
读起来像「用自然宽度」但**不是**：一处 stretch 都不给，多余宽度照样被摊到 item 之间。结果是三
条 run 之间 155 px 的空隙。控制断言不是假设：改成 `start` 之后，基线里 44 张 scene **一字节都
没动**（`marks`、`table-edit`、两张 `columns` 全部 byte-identical），因为它们全都溢出自己的框、
从来就有余量可浪费。两条 Slint 陷阱（默认 stretch、`visible: false` 不挡求值）都写进
`docs/UI_ARCHITECTURE.md` §"Slint geometry traps"。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets -- --skip
clipboard_write_and_read_round_trip_unicode` → **335 passed / 0 failed / 9 ignored**（比上一批
的 312 / 8 正好多这次的 23 条断言 + 1 条打印型计时；跳掉的那条是本 session 环境的剪贴板独占，
见上一批的记录）；`cargo build --release --all-targets` 零警告、4 m 11 s。新测试：`core/math.rs`
15 条（希腊字母 / 关系符 / 分式 / 根式 / 上下标 / 环境 / 未知命令原样 / 幂等 / 空格是内容 / 空
公式…），`markdown_test.rs` 9 条（两种围栏形状、行内往返、含两个 `$` 的散文、导出转义、公式吞
掉别的样式、首尾空格降级、双形状 import）。控制断言在关键处：「散文里两个美元」那条先证明它
**没有**产生 mark，否则一个什么都没解析的测试也会绿。

**性能**：`to_unicode` 五例平均 **658.5 / 625.8 / 608.3 ns**（release，200 000 轮 × 5 例，
`#[ignore]` 打印型）⇒ **≈0.61 µs 一条公式** ⇒ 每行都挂一个公式的 10 000 行页，一次投影的上界
≈6 ms（10 000 × 0.61 µs 的算术，**不是量出来的帧**：仓库里从来没有一条整页投影的读数，最接近
的两个各自都不是它——打字那行的按键处理中位数 47–66 µs 是一个事件，存储那行的去抖 32 变更
`apply` 中位数 3.42 ms 是一次 SQLite 提交）。6 ms 是一句「渲染器最坏能被记多少账」的上界，
也是「不在换行的行内渲染公式」和「转换放在投影里」两个决定的理由：那样一页一秒重画 60 次，放
在绑定里就要把 6 ms 再乘 60。RAM 闸这次**带了自己的 control**（批次 B 欠的方法账，
现在还）：把前一个 commit `2e9de99` 在 `git worktree` 里编出来（`git status --short` 为空、树
里没有 `src/core/math.rs`），两个 exe 在同一 sitting 里交替跑 scene D——control 135.7 / 135.8
WS · 109.8 / 110.7 private，math 136.6 / 136.7 · 111.7 / 112.4，**1.016×**（闸 ≤1.2×）。种子
那次（143.3 / 143.7）不进稳态。两臂各自自证身份（大小 + md5），所以谁也不能冒充谁。那 +1.8 MB
比臂内 0.9 MB 的散布大，所以**记下而不抹平**：exe 本身大了 56 KB，其余推测是两个 `match` 各多
一个 arm 加符号表，未归因。同一 scene 的空闲 CPU 这次读 2.5–4.5 %、媒体批次读 0.0–0.2 %，这也
是闸只按臂间比值讲、从不报绝对值的原因。数据在
`benchmarks/results/2026-09-21-m10-math-ram.jsonl`，论证在 `docs/PERFORMANCE.md`。

**像素**：`.scratch/sweep18` → `.scratch/sweep21`，44 张里动 4 张、新增 2 张（`math`、
`math-inline`），40 张 byte-identical。四个 bbox 全在改动能解释的带内：`slash` 621 px 与
`dark-slash` 2 483 px 同 `x 340..618 / y 532..586`（菜单多了「Math — or type $$」那一行）、
`plus` 10 983 px 覆盖 `x 600..878 / y 0..798`（插入菜单整列高了一行，所以它画的每一行都位移）、
`settings` 77 px 在 `x 548..724 / y 490..500`（快捷键行现在写 Ctrl+B / I / E / M）。46 张是新的
基线。这一批**不是**靠肉眼判的：先算哈希、只重看变化的 6 张。

**未验证**：①**人眼**看一个光标下的 Math block（活 TextEdit 里编辑源）和选中文字按 Ctrl+M——
`quire_shot` 从不聚焦某一行，两张新 scene 因此都是渲染态，编辑态没有像素。②真实 LaTeX 语料：
「常见子集之外一律原样回来」是策略不是验证，`\begin{aligned}` 那类多行环境没有排版。③代码高
亮那条硬要求（长代码块打字延迟不可测）还没测过——批次 C 的下一条若走同一条 runs 通道，会直接
复用这次的 ≈0.61 µs/次的结论。

**批次 C 还剩**：highlight / bookmark / embed / TOC / synced block。上一批结尾那句判断要改一
半：**TOC 的墙已经不在了**——`quire://block` 锚点在应用内跳转从 M8 就能用，缺的只是收集标题
和刷新，跟 §二十五 的快照机制没有冲突；highlight 撞的是 runs 的单行通道（这次没动它，但公式
run 走的就是那条道）；bookmark / embed 仍然要有 TLS 客户端，而 SPEC §二/§三十三 禁 WebView 与
JS 运行时，所以它们需要一条不含网络的形状（embed 至少要能画一张占位卡）。建议顺序：TOC → 代码
高亮 → embed 占位卡 → bookmark → synced block。

## M11 批次 C · slice 2 — TOC：目录不存内容，每次投影从页面上现算（2026-09-21，on `master`，ADR-0039）

批次 C 六条里的第二条。上一条结尾那句判断（「TOC 的墙已经不在了」）这次得到验证：它确实
只是「收集标题 + 在投影里出一份列表」，`user_version` 依旧停在 **8**，一行文档代价是
`blocks.kind` 里的一个字符串 `"toc"`。

**改动面**：`core/types.rs` 加 `BlockKind::Toc`（int 21、库里 `"toc"`、`ALL` 21→22）；
`ui/Types.slint` 加 `struct TocEntry { block, label, level }`、`BlockRow.toc-entries` 一条
列表字段、`callback toc-jump(int)`；`app/state.rs` 的 kind↔int 两个 arm、`BLOCK_TOC`、slash
与 insert 菜单各一行、`project_blocks` 把 `visible_block_indices(blocks)` 提到函数开头（原来
只在一处用）、新增 `fn toc_entries(blocks, shown)`，以及 mock `block()` 补字段；
`app/controller.rs` 注册 `on_toc_jump` 与 `"toc"` bench scene；`EditorBlock.slint` 的
`is-toc`、可编辑集合排除、上边距、`body-height`、指针光标，和那个 `toc-list` 委托；
`services/export_service.rs` / `import_service.rs` 的标记行；`benchmarks/scripts/sweep.ps1`
多一张 scene。**没有迁移**，论证同 math 那批（kind 在库里是字符串，未知 kind 是「读到就报
损坏」，所以旧构建打开带目录的库响亮地失败，不是悄悄少一行）。

**六条决定**：

1. **目录是派生的，一行都不存**。列表每次投影现算，于是改标题就改目录、删标题就少一行、
   折叠起来的标题自动不在目录里——不是「同步」出来的，是同一份数据。反过来若存快照，就要
   发明刷新时机，而那一时刻一到，目录和正文必然有不一致的时候。
2. **可见集合是借来的，不是自己算的**。`toc_entries` 走的是投影自己已经算出来的
   `visible_block_indices`，所以「折叠的标题不进目录」不是加的一条过滤，而是复用同一份
   「这一页现在能显示哪些行」。同一函数顺带把 block-tree 与容器两种隐藏一起解决了。
3. **Markdown 只写一行 `<!-- quire:toc -->`**。把 1 000 条目录行导进文档，等于把派生数据
   连同本库内部的 block id 一起塞进一个跨库交换格式——导到别的工具里是死字，导回来还会
   造出一批指向不存在 block 的链接。
4. **目录行可选、不可编辑；转换进来的那行文字留着不画**。前者因为它没有自己的文字可改，
   点它是选那一块；后者走 divider 的先例，所以 Turn into 来回不丢字。空标题给 `"Untitled"`
   而不给空行：空行没有可点的东西，但那一行仍然是个落点。
5. **点击只移动光标，不滚视口**——这条是先查证再写的。Slint 1.18 的裸 `ListView` 没有
   bring-into-view（只有 `StandardListViewBase` 有，见
   `i-slint-compiler-1.18.0/widgets/common/listview.slint`），而 `i-slint-core` 里没有任何
   代码在焦点变化时挪动 `Flickable` 的 viewport。M8 的 `quire://block/` 锚点早就带着同一个
   限制，所以这是已知边界，不是这里新造的坑；`reveal`（把目标行滚进视口）单独排成一条
   slice，两个入口一起受益。不在这次偷偷塞一个靠估算的滚动条位置。
6. **`toc-jump` 不走 `open-link`**。目录行标的就是本页的 block，id 已经在行数据里是个
   int；套 `quire://block/` 那条链要先拼字符串再解析回来，还顺带过一次页面查找。路径复用
   `flush_pending_edit` → `set_editing_id(-1)`（重建委托，让 TextEdit 接管）→ `focus_block`。

**顺带修的两处 UI 形状**：目录块的 `top-margin` 组里漏了 kind 21（跟在其他非文本块后面补
上），以及 `toc-jump` 之后 `body-height` 得由 `toc-list.preferred-height` 给——一个
`Rectangle` 不能拿子 `Text` 的高度来定尺寸（和 Rectangle 默认 100 % 宽度互为循环），所以每
行高度用的是字体度量加 `overflow: elide`。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets -- --skip
clipboard_write_and_read_round_trip_unicode` → **340 passed / 0 failed / 10 ignored**（比上一
批的 335 / 9 正好多这次的 5 条 + 1 条打印型计时；跳掉的那条是本 session 环境的剪贴板独占，
见上一批的记录）；`cargo build --release` 零警告。新测试：`state.rs` 2 条（一页里有 H1/
折叠 H2/H3/空标题，目录读出 `(2,"Top",1) (5,"Second",2) (6,"Untitled",3)`——**控制断言是
那个折叠的 H2 不在里面**，同时每一条非目录行的列表都得是空的，否则派生就漏到了别的块上；
另一条改标题后目录跟着改，证明确实没有拷贝）、`markdown_test.rs` 3 条（导出只有标记行、
标记行不吞邻行、往返之后不带目录的副本）。

**性能**：RAM 闸按上一批立的规矩重跑了一遍——前一个 commit `cc7ccf0` 在 `git worktree` 里
编出来（`git status --short` 为空），两个 exe 同一 sitting 交替跑 scene D，各自先用 md5 +
大小自证身份（control md5 `a81ce6df…` 21 928 960 B，本树 `f6d9a576…` 22 017 024 B）：
control 稳态 135.9 / 137.0 WS · 110.5 / 112.1 private，toc 137.0 / 137.3 · 111.6 / 112.5 ⇒
**1.007×**。而且和 math 那批不同：两臂均值之差 0.75 MB **小于 control 臂内部的 1.6 MB
散布**，所以这次连「小成本」都不能说，只能说量不出来。种子那次（112.7 / 110.0）不进稳态。
exe 大了 88 064 B：一个 enum arm、一个 struct、一个 callback、一个委托。
闸看不见的那半单独量了：**scene D 里没有目录块**，所以 RAM 比值讲的是「多一个 kind」的
账，不是「多一条列表」的账。`cost_of_one_contents_block_on_a_ten_thousand_row_page`
（`#[ignore]`，release）对 10 000 行、每十行一个标题的页面计时 `project_blocks`，行 0 分别
当段落和当目录（1 000 条），三轮 39.12 / 37.40 / 37.17 vs 37.71 / 37.85 / 38.18 ms ⇒
差值 −1.42 / +0.45 / +1.01 ms，**和付账那一臂自己的轮间散布分不开**；能给 1 000 条派生
条目设的上界是 ≈1 ms，基数是同一投影本身的 ≈38 ms。顺带说一句：**≈38 ms 是仓库里第一条
整页投影的读数**——上一批引 ≈6 ms 时明说那是算术不是量出来的，因为从来没人量过公式落进
哪次投影；两个数一起读，问题的形状变了：每条公式的成本是一次投影的百分比，而一次投影不是
一帧。这条是 Rust 侧数字（block 进、`BlockRow` 出，不 realize 委托、不重绘），对「10 000
行画出来要多少」一个字没说，已公布的 UI 数字照旧（按键处理中位数 47–66 µs、32 变更去抖
`apply` 中位数 3.42 ms）。数据 `benchmarks/results/2026-09-21-m11-toc-ram.jsonl`，论证在
`docs/PERFORMANCE.md`。

**像素**：`.scratch/sweep21` → `.scratch/sweep22`，46 张里动 3 张、新增 1 张（`toc`），43
张 byte-identical。三个 bbox 全在改动能解释的带内：`slash` 683 px 与 `dark-slash` 2 574 px
同在 `x 340..618 / y 564..618`（菜单多了「Table of contents」那一行）、`plus` 1 249 px 在
`x 612..864 / y 624..792`（插入菜单多一行）。`settings` 这次没动——快捷键没加。47 张是新
基线。新 scene 走的是**把样例页第一段转换成交集块**，而不是往 `mock_blocks_sample()` 里加
一行：那样 ~30 张默认页的 scene 才会原样不动，而「43 张 byte-identical」就是这句话的控制
断言。这一批同样不是肉眼判的：先算 manifest 哈希、只重看变化的 4 张。

**未验证**：①**人眼**点一次目录行——hover 变色、光标落到目标标题末尾、以及「不滚动」在真实
长页上的观感，`quire_shot` 从不聚焦某一行，所以点击路径一个像素都没有。②`reveal`（把目标
滚进视口）没有实现，这条限制只是被复述、没有被解除。③中文长标题 elide 成一行后的读感。
④一页几百个标题时目录块自己的高度（列表用 `preferred-height`，会顶开整块，但没在真实语料
上看过）。

**批次 C 还剩**：embed / highlight / bookmark / synced block。建议顺序仍是 embed 占位卡 →
代码高亮 → bookmark → synced block：embed 只需要一张不含网络的卡（SPEC §二/§三十三 禁
WebView 与 JS 运行时），而高亮那条硬要求（长代码块打字延迟不可测）大概会复用 math 与这次
的同一份 runs 通道。`reveal` 不属于批次 C 的任何一条，但 TOC 和 `quire://block` 锚点现在都
在等它，谁做谁受益。

## M11 批次 C · slice 3 — embed：卡片只存地址，两行是画的时候现算的（2026-09-21，on `master`，ADR-0040）

批次 C 六条里的第三条，兑现的正是上一批结尾那句「embed 只需要一张不含网络的卡」。SPEC
§二/§三十三 禁 WebView 与 JS 运行时，这次没有绕那道禁令，也没有把它当成降级：一行文档的
代价仍然是 `blocks.kind` 里的一个字符串 `"embed"` 加地址本身，`user_version` 依旧停在
**8**。

**改动面**：`src/core/embed.rs`（新增）、`src/core/mod.rs`、`src/core/types.rs`、
`src/app/state.rs`、`src/app/controller.rs`、`ui/Types.slint`、
`ui/components/EditorBlock.slint`、`src/services/export_service.rs`、
`src/services/import_service.rs`、`tests/integration/markdown_test.rs`、
`benchmarks/scripts/sweep.ps1`。

**六条决定**：

1. **`BlockKind::Embed` 是 int 22**，库里字符串 `"embed"`，`text` 存的就是地址本身，不新增
   列 ⇒ `user_version` 仍是 **8**，这是第五个不需要迁移的 kind。未知 kind 依旧按「加载即
   损坏」处理，所以老版本打开一个带 embed 的库会响亮地失败，而不是悄悄丢一行。
2. **卡片那两行走 `UIState` 的两个 pure callback**（`embed-label` / `embed-url`，Rust 侧
   `core::embed::describe` / `with_scheme`），不加 model 字段——ADR-0038 的教训照搬过来。
   绑定被 `is-embed ? … : ""` 守住，因为 `visible: false` 不阻止 binding 运行。21 个已收录
   站点，google.com 既认子域也认第一段 path；认不出来的域名就以域名本身当标签；没有域名可
   说的三种形状分别是 `Embed`（空）/ `Link`（一句散文）/ `Email`（mailto）。
3. **卡片可以编辑（和 toc 相反）**：点卡片改的就是地址，边打字标题边重算；空卡说
   「Embed / No address yet」而不是一个空盒子。没有 WebView（SPEC §二/§三十三），所以「卡 +
   交给系统打开」就是功能全部，不是降级形态。
4. **Markdown 是一行裸地址**：GFM 本来就把它渲染成链接，尖括号或注释标记只是多一个「文本不
   是合法地址时会写坏」的东西；导入侧 `is_bare_address` 只认「整行一个 token 且带显式
   `http(s)://`」。
5. **导出走 code / divider / math 那条 verbatim 分支**，不过 inline-mark 渲染器——url 里全
   是 `_ * ~ &`。
6. **`open-link` 的 shell 分支前面加了 scheme 白名单** `core::embed::is_openable`
   （http/https/mailto）。**这条要写清楚不是修漏洞**：编译探针证明 std 会给参数加引号，
   `… & echo PWNED` 是作为一个整体字面串到达 `start` 的，注入从来不存在；真正变的是行为
   ——`file:///…`、`\\share\x`、`javascript:…` 和裸 `C:\…\calc.exe` 现在什么都不做，因为
   链接目标是「从文件里来的数据」，而 `start` 运行路径和打开 url 一样乐意。

**自己写的测试抓到两个真 bug**（改的是代码不是断言）：`maps.google.com/?q=x` 读成
"Google"（product 只从 path 读）；`HTTPS://WWW.Site.COM` 没去掉 `www.`（先去前缀再小写）。
另外 `TextInput` 在 Slint 1.18 没有 `placeholder-text` 属性，编译失败后把空卡提示改成由两个
`Text` 的 `visible` 表达。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets -- --skip
clipboard_write_and_read_round_trip_unicode` → **352 passed / 0 failed / 10 ignored**（上一批
340 / 0 / 10，这次 +8 条 `core::embed` 单测 +4 条 markdown）；markdown 那一个 target 单独跑
61 passed；`cargo build --release` 零警告。新测试里最值钱的是控制断言：句子里的 url 仍然是
句子（`an_address_in_a_sentence_stays_a_sentence`），以及 `evilyoutube.com` 不算 YouTube
（整标签后缀匹配）。跳掉的那条仍是本 session 环境的剪贴板独占（`OpenClipboard` err=5）。

**性能**：闸按上一批立的规矩第三次跑——control `4fa2b7b` 在 `git worktree` 里编
（`src/core/embed.rs` 不在），两臂同一 sitting 交替跑 scene D，各自先用 md5 + 大小自证身份
（control md5 `9c5e153f…` 22 017 024 B，本树 `19aea07c…` 22 072 320 B）：control 稳态
136.7 / 137.0 WS · 110.9 / 112.1 private，本树 138.2 / 138.9 · 111.8 / 112.3 ⇒ **1.005×**。
两臂均值之差 0.55 MB 小于 control 臂内部 1.2 MB 散布，所以仍是「量不出来」。种子那次
（109.7 / 111.3，startup 807 / 1104 ms）不进稳态。idle CPU 这次是本树更低（3.12–4.09 vs
4.29–5.85），这是同一个「分不出」的另一种写法。exe 大了 55 296 B。**没有做计时**：卡片不像
目录要扫页，它的活里没有「页」这个量——这是构造上界，不是读数。一个方法结论进了
`docs/PERFORMANCE.md`：三批连在 1 MB 内 ⇒ 闸有 ≈1 MB 的下限，低于它的 per-row 成本该写成
「分辨不出」，而不是报一个看着像测量的比值。数据
`benchmarks/results/2026-09-21-m11-embed-ram.jsonl`。

**像素**：`.scratch/sweep22` → `.scratch/sweep23`，47 张里动 3 张、新增 2 张（`embed`、
`embed-empty`）、44 张 byte-identical。bbox 全在改动能解释的带内：`slash` 774 px 与
`dark-slash` 2 528 px 同在 `x 340..618 / y 596..650`（比上一批的带低一档，因为菜单又多了一
行）、`plus` 1 062 px 在 `x 612..860 / y 656..792`；`dark-slash` 的框和 `slash` 逐像素相同，
所以一条判决覆盖一对。两张新 scene 仍然走「把样例页第一段转换成 embed」而不是往 fixture 里
加行——44 张不动就是这句话的控制断言。判图前先看的是 manifest 哈希 diff，不是肉眼。49 张是
新基线。

**未验证**：①**人眼**按一次 Open——浏览器真起来、卡片编辑中（边打边重算标题）、arrow 的
hover、暗色主题的卡、Turn into Embed 再转回 Text，`quire_shot` 不点 TouchArea，所以这条链路
一个像素都没有（headless 点它会真的开浏览器）。②`is_openable` 之后 `file://` 一类链接「点了
没反应」是不是用户要的行为——行为改了，没有测试能证明这被接受。③未收录域名的标签宽度（长
host 在卡里 elide 的读感）。④bookmark 的边界：SPEC §三十七 里它仍然是「抓标题与 favicon」，
这次没有替它做网络那一半，卡片形状已经在那儿了。

**批次 C 还剩**：highlight / bookmark / synced block。建议顺序不变：代码高亮 → bookmark →
synced block；高亮的硬要求（长代码块打字延迟不可测）大概会复用 math 与 TOC 的同一份 runs
通道，bookmark 现在只差一个 TLS 客户端，synced block 等 §四十。`reveal` 仍不属于批次 C 任何
一条，但 TOC 和 `quire://block` 锚点还在等它。


## M8 收尾 · A4 最后一个 HIGH — 带标记的一行现在按词断行（2026-09-21，on `master`，ADR-0041）

批次 C 三条之间插进来的这一刀，还的是 M8 的账：A4 视觉清扫从 `42cb158` 起就挂着一条 HIGH，
「带 inline marks 的段落丢掉换行、在词中间被裁掉，而同样一段文字不加粗时是正常换行的」。它一直被
写成「Slint 平台墙」，于是这一批先去看墙是不是真的——是真的（Slint 1.18 的 `Text` 没有
`TextFormat`、没有 per-run style），但墙后面还剩一条能走的路：**一个 layout 单元不可断行，那就
让单元等于一个词**。

**改动面**：`src/app/state.rs`（`build_runs` + 两条单测 + 一条 `#[ignore]` 计时）、
`ui/components/EditorBlock.slint`、`ui/components/TableBlock.slint`、
`ui/components/ColumnItemRow.slint`、`src/app/controller.rs`（3 个 scene）、
`src/main.rs` / `src/bin/quire_typing.rs` / `src/bin/quire_shot.rs`（`--marks`）、
`benchmarks/scripts/sweep.ps1`、`benchmarks/scripts/bench.ps1`、
`tests/integration/{persistence,workspace}_test.rs`（`HandleArgs` 字面量）。

**六条决定**：

1. **只切未标记的片段**：`build_runs` 在无标记的连续文本里每个词起头切一刀，标记片段一个 cell
   到底。理由写进注释了——下划线、code 底框、链接的点击目标按空格切开，比它断不了的那个粗体短
   语更糟。
2. **空白跟着它前面的词**，所以 cell 拼回去就是原文，逐字节。两条新测试一条钉死期望的 cell 列
   表（`["one ", "two ", "three ", "bold words", " four ", "five"]`），一条覆盖「标记在行首 /
   CJK / 标记在行尾」并断言拼回与无空 cell。
3. **三处画 runs 的 delegate 一起换成 `FlexboxLayout { flex-wrap: wrap }`**：块行、表格单元格、
   分栏里的行。只改第一处等于把同一面墙留在另外两个表面上。
4. **行高权威不动**，仍是旁边那个不可见的 plain `Text`（同字同宽，量的就是行数），runs 容器
   `clip: true` 在那个高度上。先试过把 `body-height` 绑到 flex 自己的 `preferred-height`：**Slint
   编译错误 `Cannot access id 'runs-flex'`**——`if` 里声明的元素在 `if` 外面不可引用；把 `if` 提
   出来让 flex 常驻，代价是 10 000 行 bench 页每行两个 item。所以断行点从 Rust 侧给。
5. **harness 先修再量**：scene D 一个标记都没有，直接跑闸会得到「1.00× 的空气」。于是
   `--marks N`（每 `rows/N` 行把第二个词加粗）进了 main / quire-typing / bench.ps1，
   `--dump-state` 多印一行 `marked=N`，让每条臂报出自己真正建了什么。
6. **不迁移、不加 kind、不加 model 字段**：`user_version` 仍是 8，runs 通道仍是
   `Vec<TextRun>`，存储 / undo / Markdown / LAN 导出全都不知道这次改动。

**墙移到了哪儿，还剩什么**（这句必须写准）：①一个长过一行的标记短语仍然裁；②一行标记文字比同样
词数的未标记文字**需要更多行**时仍然裁（粗体和 mono 更宽，而高度是 plain text 量的）；③没有
ASCII 空格的文本（中文段落）仍是一个 mark 一个 cell，因为切词用的是 `is_ascii_whitespace`。这三
条现在是窄例，不是常态，`docs/EDITOR_ARCHITECTURE.md` §"Platform wall" 逐条写着。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets -- --skip
clipboard_write_and_read_round_trip_unicode` → **354 passed / 0 failed / 11 ignored**（HEAD 是
352 / 0 / 10，这次 +2 单测 +1 条 `#[ignore]` 计时）；`cargo build --release` 零警告。跳掉的仍
是本机环境的剪贴板独占（`OpenClipboard` err=5）。计时那条试过搬成独立的 `[[test]]` target（这样
往 control worktree 里复制时不必改被测文件），最后没有搬：ADR-0039 的投影数字已经是 lib 里同一种
`#[ignore]` 测试，多一个 target 换不到一致性，而且复制的只是 `#[cfg(test)]` 代码，不碰被测路径。

**性能**：闸跑了**两批**（原始行 `benchmarks/results/2026-09-21-m8-wordwrap-ram.jsonl`，12
行）。control = `2c5a25f` 在 clean worktree 里编，**只**把 bench 旋钮搬过去，所以它的 runs 还是
一行裁掉的（md5 `cdf6da5a…` 22 074 368 B）。scene D 10 000 行 / 1 000 行带标记，两臂交替，各自
pinned db，每臂种子那次不计：第一批（本树 `f59edb1b…` 22 081 024 B）control 112.7 / 111.4 →
本树 115.2 / 115.9 ⇒ **1.031×**；第二批（表格与分栏 delegate 也改完，`4f9205ca…` 22 087 680 B）
control 114.3 / 111.1 → 本树 114.8 / 115.7 ⇒ **1.023×**。两批都在 ≤1.2× 内，但诚实的读法是：
两臂均值差 2.5…3.5 MB **小于 control 臂自己在第二批的 3.6 MB 散布**，所以这是「贴着闸的分辨率」。
数字有形状：一条 bench 标记行从 3 个 run 变 11 个 cell，一个 cell 就是一个带自己 `Text` 的
item。投影侧另有一条两臂同 sitting 的计时（`#[ignore]`，release，50 轮 `project_blocks`）：
未标记 41.686 / 41.504 ms（两臂差 0.4 %，这就是「同一台机器同一份 fixture」的控制），1 000 行
标记 44.147 (+2.461) / 46.942 (+5.438) ⇒ 切词大约 **+2.8 ms / 万次投影**，即每条标记行
≈2.8 µs，**全页无标记时是 0**——这也是 `block.runs.length > 0` 那道守卫留着的原因。

**像素**：`.scratch/sweep23` → `.scratch/sweep24`，49 → 50 张：动 3、新增 1（`marks-wrap`）、
46 张 byte-identical。`marks` 与 `dark-marks` 1 942 / 1 918 px 落在同一个框
`x 390..1148 / y 196..240`（一段被裁成一行 → 两整行）。`math-inline` 动 360 px 在
`x 420..664 / y 194..210`——这张本来该「完全不动」，所以又补了两个探针：位移探针（找能把差值归零
的 dx/dy）说它不是平移，ink-edge 扫描说它是**最右有墨列从 661 变成 664**，即一行按词测量比按整
串测量多出 ≈3 px 的间隙累积（每个间隙不到 1 px）。`.scratch/sweep24` → `.scratch/sweep25`，
52 张：**已有 50 张动 0 张**，新增 2 张（`table-marks`、`columns-marks`）——那个 0 不是证据，
两张新图才是：grid 的 fixture 每格一个词、layout 的是「Column 1 / Column 2」，两处 delegate 改
完没有任何一张图会失败，于是各造一个「塞进最窄的盒子、句子长到必须换行、中间一个粗体短语」的
scene。`marks-wrap` 同理，而且它的 needle 全改成 `.expect("needle present")`：早先
`unwrap_or(0)` 把「break between cells」拼错成找不到时，静默标到了偏移 0，画面看着仍然像那么回
事。判图前看的仍是 manifest 哈希 diff。52 张是新基线。

**未验证**：①**人眼**——在换行后的标记行里点一下 caret 落在哪、编辑中标记段长高一行、链接现在
只有一个词那么宽（hover 目标与「一个词的 underline 算不算 bug」）、中文标记段落（还是整段一
cell）、表格格与分栏里编辑时的行高。②`--marks` 只喂 bold，italic/code/link 混排的内存形状没
量。③两批比值之差（1.031 / 1.023）本身没有解释，只能记成噪声。

**批次 C 还剩**：highlight / bookmark / synced block。**高亮前面那面墙没了**——per-token 的颜
色 run 现在能断行，这条正是它当初排不进来的理由；bookmark 仍差一个 TLS 客户端（embed 已经把卡
片形状占了，缺的是抓标题与 favicon 那一半网络），synced block 等 §四十。`reveal`（变高
scroll-into-view）不属于批次 C，但 TOC 跳转和 `quire://block` 锚点还在等它。
（写在下条之后的更正：**「墙没了」不是高亮能走 runs 通道的意思**——ADR-0042 最后没走那条路，
理由在下条第 1 条。）

## M11 批次 C · slice 4 — code 高亮：一个颜色是一层，不是一段 run（2026-09-21，on `master`，ADR-0042）

SPEC §三十七 批次 C 的第四条，也是这批里唯一先撞墙再绕过去的那条。上一条结尾说「高亮前面那面
墙没了」，指的是 ADR-0041 让 run 能断行；真去量 token 的时候发现墙其实还在，只是换了个位置：
**token 不是词**。一条字符串字面量可以跨行，一段注释可以，一条模板串也可以，而 ADR-0041 的 cell
按 `is_ascii_whitespace` 切，切出来的单元天生不含换行。所以 per-token 上色走 runs 通道的话，
一个 token 要么占满一整个 layout 单元（跨行时无法断），要么被按空格切开（字符串里的空格就不是
颜色的一部分了）。两条都不要，于是改成一整块字符串叠六层。

**改动面**：`src/core/highlight.rs`（新增，1 005 行，含 16 条单测）、`src/core/types.rs`
（`Lang` + `Block.lang`）、`src/core/{command,history,persistence,mod}.rs`、
`src/storage/migrations.rs`（**v9** `add_lang_column`）、`src/storage/repository.rs`、
`src/app/state.rs`（投影带 `lang`、`code-hl` 的 fixture `bench_code_source` / `bench_code`、
`--code` 的 10 000 行构造）、`src/app/controller.rs`（`on_code_layer`、`SetCodeLang`、两个
scene）、`src/main.rs` / `src/bin/quire_typing.rs`（`--code` + `--dump-state` 的 `coloured=`）、
`src/bin/quire_shot.rs`、`src/services/{import,export}_service.rs`（fence info string）、
`src/services/{settings_store,lan_server}.rs`、`ui/{Colors,Types}.slint`、
`ui/components/Editor.slint`（advance 探针）、`ui/components/EditorBlock.slint`（六层）、
`benchmarks/scripts/{sweep,bench}.ps1`、`tests/integration/*`（`HandleArgs` 字面量 + 新断言）、
docs 六份。

**七条决定**：

1. **一层，不是一个 run**。`highlight::layer(text, lang, frame, advance, kind)` 返回**整个
   block**，把不属于这个颜色的字符换成 `U+00A0`（不可断空格——这是它不用普通空格的原因：空白列
   不能成为断点），源码里的空白原样保留。六串等长、在同一批硬换行处断，所以它们是逐字叠在一起
   的：不需要两个排版引擎互相同意，而**量行高的那一层（`Kind::Plain`）本身就是叠出来的一层**。
2. **换行是派生的，不是存的**。列预算 `columns = floor(frame / advance)`，ASCII 记 1 列、tab
   记 8、其余记 2（非 ASCII 的真实宽度这个模型不知道，见第 6 条）。`advance` 为 0（第一次量到
   之前）就只上色不加换行，退化成未上色块的软换行——那是更朴素的渲染，不是坏掉的渲染。
3. **一次全应用的字体测量**：`Editor.slint` 里一个 `visible: false`、摆在 `-1000px` 的
   `code-probe := Text { text: "0000000000"; changed width => UIState.code-advance =
   self.width / 10 }`。十个数字是因为小数 advance 在长串上舍得更准。没有第二个地方猜字号。
4. **颜色复用块调色板的槽位**：`Colors.code-token(kind)` 把 keyword/comment/string/number/name
   映到 block-text 的 7/1/5/3/6（紫/灰/绿/橙/蓝），暗色是同一组槽位的另一列。不新增颜色常量，
   主题开关不用碰。
5. **上色只在没在编辑时**，而且是**条件元素不是 `visible`**：`if root.is-highlighted &&
   !editing : Rectangle { for kl in 5 : Text {…} }`。`visible: false` 不阻止 binding 运行——
   第一版就是这么写的，结果是页面每行每次重绘跑五次 lexer。改成条件元素后，光标在块里时那五层
   根本没被 realize；这也是「打字延迟不可测」的机制，不是运气。
6. **画序 1,3,4,5,2，注释压最后**。非 ASCII 字符宽度未知 ⇒ 模型不敢把它 blank 掉 ⇒ 它被逐字抄
   进每一层，于是它的颜色由最上面那层决定。散文（中文注释、字符串里的中文）最可能是非 ASCII，
   所以让注释色赢：写在注释里的中文读起来是灰的，这是取舍不是 bug，SPEC 边界里写着。
7. **语言是块存的唯一新东西**：`blocks.lang`（v9，本批第一次也是唯一一次迁移——math/TOC/
   embed 四类都不存新东西）；入口是块自己的 ⋮ → Language（submenu id 14，只在 code 块出现；
   `CODE_LANG_BASE = 600_000 + index`），撤销走 `Change::BlockLangSet`。`Lang::try_from_str`
   折别名（`rs py python3 jsx tsx markdown jsonc sh shell zsh`），认不出的折成 `Plain`——**一
   个这份构建认不出的语言是没有颜色，不是一个坏掉的块**。Markdown 双向带 info string（进来什
   么别名都认，出去写规范名 `rust`，`Plain` 写空 fence），富文本粘贴带 `lang`。

**两个只有截图能抓到的缺陷**（都进 ADR-0042 与 `docs/UI_ARCHITECTURE.md` 的几何陷阱表）：
①**层与量尺的错位**——`code-hl` 第一张图上，五种颜色整块浮在字上方一行。根因是 Slint 的
`Text` 没绑 `width` 就永不换行（它的宽度变成 `preferred-width`），所以五层只在 lexer 自己插入
的硬换行处断，而量高那层在软换行。修法：五层各绑 `width: root.code-width`。**复证不是在
1 280 px 而是 760 px**：fixture 里没有一行长到需要断的时候，这个缺陷在任何宽屏图上都看不见。
②`if cond : { Rectangle {…} }` 编译不过（`expected Identifier`，EditorBlock.slint:607），条件
元素的冒号后面必须有元素名。写文档时还出过第三次自己的错：把「彩色臂 private 更低」解释成
「代码块文字更短」，而 fixture 是 ≈58 字节的 lorem 行换成 330 字节七行——PERFORMANCE 那节已按
实际数据改掉，理由见下。

**验证**：`cargo check --all-targets` 干净；`cargo test --all-targets -- --skip
clipboard_write_and_read_round_trip_unicode` → **374 passed / 0 failed / 11 ignored**（上一条
ADR-0041 是 354 / 0 / 11，这次 +20）。其中 16 条在 `highlight.rs`：五种语言各自的词法（注释里的
`//` 不是注释、`'a` 是 lifetime 不是字符、JSON 的 key 与 value、bash 里词中的 `#`）、层与源串等
长、所有 kind 共享同一批断点（`every_kind_shares_the_breaks_so_the_layers_stack`）、列预算
（`a_wrapped_line_never_exceeds_what_the_row_can_show`）、宽字符只抄不 blank、`U+00A0` 不是断点
而真空格是。别名折叠那条在 `types.rs`（`lang_strings_round_trip_and_aliases_fold`），迁移与读写
在 storage/markdown 两处集成测试里。`cargo build --release` 零警告。跳掉的仍是本机剪贴板独占。

**性能**：三臂两 exe 一个 sitting（原始行
`benchmarks/results/2026-09-21-m11-highlight-ram.jsonl`，15 行）。control = `2b62498` 在 clean
worktree 里编（md5 `bde7faec…` / 22 087 680 B）；candidate 是本树 release（md5 `961ff115…` /
22 194 176 B），并跑 `--code 0` 与 `--code 2000` 两臂——**同一支 exe**，所以「树的账」与「上色
的账」是两个读数。每臂自己的 pinned db、交替、种子那次不计、每条臂用 `coloured=` 自证 fixture
（不带该字段的二进制印不出这行，这就是它的身份证）。稳态 private：control 97.4 / 96.9，
candidate 无彩色 97.5 / 98.4，彩色 2 000 行 90.6 / 90.4 ⇒ **1.008×** 与 **0.924×**。第一个数
是这批欠闸的数：0.8 MB 对 control 臂自己 0.5 MB 的散布，就是「量不出来的成本」。**第二个数不
是成绩**，而且它的第一版解释是错的：彩色臂的行**更长**（330 B 对 58 B）、**更高**（7 行对 1
行），文字量涨、行高涨，private 反而掉 7.5 MB。而且它连符号都不稳：200 行彩色 ⇒ +2.75 KB/行，
2 000 ⇒ −4.4 KB/行，5 000 ⇒ −1.85 KB/行。一个在自己量程内变号的量没在被测量，所以这里发布的
是「**彩色行多 25 倍，进程没有变大**」，不是那 7.5 MB。它是 committed-not-touched：三臂的
working set 全在 132.2–133.4 MB，落掉的 7.5 MB 从来不在内存里，这次坐实的只是一句——不同分配
序列留下的 arena，本闸不定价。idle CPU 稳态 0.2–0.59 %（种子那三次 2.53–5.65 %）。exe 大 106
496 B。SPEC §三十七 真正写下来的是延迟，scene E 答这条：同一支 exe，`--code` 0 对 2000，各两
轮（`benchmarks/results/2026-09-21-m11-highlight-typing.jsonl`）——handler 中位 **89 µs** 对
92–112 µs，p95 154–167 对 154–207，两列都不输，而未彩色臂自己跨轮散布（92→112）比两臂之差还
宽。「长代码块打字延迟不可测」达成。彩色臂打字时 CPU 确实高（29.7–30.8 % 对 24.3–27.1 %），那是
软件渲染器每 tick 重画一行高亮的开销，进程有富余；若哪天把五层挪出 `!editing` 守卫，要重读的
就是它。

**像素**：`.scratch/sweep25` → `.scratch/sweep26`，52 → **54** 张：**已有 52 张动 0 张**，新增
`code-hl` 与 `dark-code-hl`（`sweep.ps1` 的 scene 表在 `embed-empty` 与 `dark-link` 之后各加一
行）。两张新图是这一批唯一的证据，所以不能只看「有颜色」：用 PIL 对 PNG 做了**颜色普查**，五
种 token 色在两张图里都出现（light 5 色 / dark 5 色），而 `default` 与 `dark` 两张基线图里各只
有 4 与 1 个杂点落在这些值上（那是 UI 别处的相近色，不是高亮）。这一步是必要的：一个「paints
nothing」的高亮器不会让其余 52 张里任何一张变化，也不会让上面任何一个内存数字变坏。另外补的
两张对照（`.scratch/hlfix/`，非基线）：1 280 px 的原图与 760 px 的窄图——后者才真的逼出列预算
那条路径，错位缺陷就是在这两张之间从「看不出来」变成「看不对」的。54 张是新基线。

**未验证**：①**人眼**——⋮ → Language 菜单的实际观感（一级菜单里第 14 项、8 个语言项）、编辑中
五层消失 / 退出编辑后出现、中文注释读成灰色的取舍在真实页面上能不能接受、暗色主题下的对比度、
以及 caret 落在高亮块里时的行高。headless 一张图都没有按下 TouchArea。②`Lang::Plain` 的块在一
个已经上色的页面里会不会被误当彩色（守卫是 `block.lang != ""`，代码路径有单测，画面没有）。
③非 ASCII 的列宽模型（第 6 条）只按 2 列估，一条全是中文的「代码」在窄框里会比实际宽，`clip:
true` 兜住不溢出，但没有图证明这读起来是对的。

**批次 C 还剩**：bookmark / synced block。bookmark 仍只差一个 TLS 客户端（embed 已经把卡片形状
占了，缺的是抓标题与 favicon 那一半网络）；synced block 等 §四十。高亮这条不再需要 runs 通道，
所以 ADR-0041 那三条残留（长标记短语、标记行需更多行、无空格文本）与它无关了。`reveal`（变高
scroll-into-view）仍不属于批次 C，但 TOC 跳转和 `quire://block` 锚点还在等它。


## 工具诚实 · quire-shot 不再冒充 FemtoVG（2026-09-21，on `master`）

批次 C 第四条之后顺手还的账：A4 报告里那条「known limitation, not a defect」——sweep 每张图的
侧栏页脚都写着 `FemtoVG · GL`，可 `quire-shot` 装的是 `HeadlessPlatform`（Slint 的软件光栅器），
而 `--features software` 并不会关掉默认的 `femtovg`，于是编译期按 feature 顺序答出来的名字说的是
一个从没跑过的渲染器。这不是观感问题：sweep 是本项目关于「渲染」的证据，它一直在给自己的证据贴
错标签——`scroll_ab.ps1` 早就为 skia 臂记过同一个坑（`--features skia` 不关 femtovg）。

**改动**：`src/bin/quire_shot.rs` 在 `controller::wire()` 之后把 `UIState.renderer-name` 覆写成
`Software · headless`。app 二进制一个字没动——它跑的确实是自己的 feature 选中的后端，页脚写的就是
对的；只有「自己装 Platform」的那个进程需要说真话。

**验证**：`cargo check --all-targets`、`cargo build --release`、`cargo build --release --features
software --bin quire-shot` 三条全零警告（ROADMAP 里那句「shot 构建还剩一个 `unused variable: ui`
警告」已经作废：`render()` 早就不接 `ui` 参数了，这次撤回）。sweep26 → sweep27：54 张**全动**，
而「全动」正是这一刀的预期——`diffbbox` 说 53 张只动了 `x 96..192 / y 778..784` 里 118…125 个采样
点（页脚那一行 caption），`settings` 动 244 px 覆盖 `x 96..722 / y 692..784`（页脚 + 对话框的
Renderer 行）。读这个属性的地方一共两处，变化的框也正好两处，别处一个像素都没有。两张放大裁图
（`.scratch/shotlabel/`）直接读到旧图 `FemtoVG · GL`、新图 `Software · headless`——两臂各自自证
身份。新基线 `.scratch/sweep27`（54 张）。

**顺带清掉的文档债**：ROADMAP 的 M8 verification snapshot 多了一节「Re-run at `63a4081`」，把测试
数（374 / 0 / 11）、三条构建、sweep 结果写清，并明确 installer / portable 两条**这次没重跑**；A4
的 open list 逐条对代码复核后落成——D1（ADR-0041）、D9（ADR-0033，`ui/` 里已无「later milestone」
字样）、模态遮罩、`Ctrl+B` 让位给 `Ctrl+\`、渲染器标签，五条全闭；只剩 D7（MEDIUM）与两条对比度
LOW。D7 这次也拿到了代码证据：`EditorBlock.slint` 里没有任何 find 的引用，所以「caret 不在这一块
就画不出命中」不是坏掉，是**功能不存在**——它要的是行级命中区间，撞的正是 ADR-0042 绕开的那面
「一个 Text 一种颜色」的墙，因此按新 feature 排期，不按 bug 修。

**未验证**：`--no-default-features --features software` 这一支 shot 构建没跑过——现在标签与 feature
无关，所以那个组合不再影响图的正确性，但它的构建时间与产物没量过。

## 工具诚实 · 浅色板最暗的那层字是量出来的，不是看出来的（2026-09-21，on `master`，ADR-0023 修订）

**缘起**：A4 视觉回归留了两条对比度 LOW，都是眼睛判的。眼睛在这份 sweep 上已经错过五次假缺陷，
所以这次先算：`.scratch/contrast.py` 把 `ui/Colors.slint` 的每一档文字色放到四个浅色表面上求
WCAG 相对亮度比，再把 9×9 的 block 文字/底色矩阵全跑一遍（own-tint / plain / 跨色族最差）。数字
站在 A4 那边一半，站在另一边一半。

**测出来的三处**：浅色 `text-muted` 在白底 2.71、在侧栏 2.51——它是块把手、页脚、侧栏和 palette /
search 每一行 hint 的字色，也就是整个浅色 UI 里最暗的一层；block 槽 3（orange）在自己底色上 2.81、
槽 4（yellow）2.48，是全板最差的两对。于是 `#9b9da2 → #75787d`（4.43 / 4.10）、`#d9730d →
#bd6408`（own-tint 3.61、plain 4.21）、`#cb912f → #a87718`（3.56 / 3.95）。深色一个字没改。

**撤回的那半**：A4 点名的「green-on-olive 和 red-on-black 是最弱两对」是误读——它们量出来 3.86 和
4.72，从来不是问题。这条写进 ADR-0023 的修订和 Colors.slint 的注释里，因为「按眼睛排期」和「按数字
排期」在这两条上给出了不同的答案。

**上限是推理出来的，不是撞到的**：muted 没有一路推到 4.5。再往下就是 `text-secondary`（5.91），
一档字比上一档只高 1.0 之比就没有第三层了；而深色的 muted 一直是 3.9–4.4。所以两条主题在「这一层
有多轻」上会合在这个 band，而不是会合在 AA 线上——这条写进注释，免得下次有人顺手把它推到 4.6。

**像素**：sweep27 → sweep28 动了 45/54，9 张 dark 逐字节相同（md5 相等），这正是「只改浅色」必须
拿到的读数。调色板 diff 没有可信的 bbox（muted 到处都在），所以约束改成方向性的：54 张里**没有任何
一个像素在任何通道上变亮**，全表最大单通道降幅是 38——正好等于 muted 的 R 降幅 155→117；`dialog`
和 `link` 只到 25，因为这两张在基线里满覆盖的 muted 像素是 0（`default` 有 575 个），同一个 38 被
抗锯齿按比例缩小而已。逐像素归因（`.scratch/confine.py`：是否存在
一个覆盖率 a 和一个表面 s 使 before=a·T_old+(1-a)·s、after=a·T_new+(1-a)·s）在六个场景里把 38 283
个变化像素中的 38 176 个记到这三个 token 头上；剩下 107 个全在 `code-hl`，每通道 ≤4/3/1，全在橙色
字形压着底色的边上——那里光栅化做的不是模型假设的 sRGB 线性混合。分类器的 control 用上一刀的 diff：
拿 `sweep26 → sweep27`（这三个 token 没参与）跑同一个脚本，`settings` 的 1 215 个变化像素里 897 个
被判「无法解释」——它会说不。旧值的残留也解释了：tol 4 下还有 1 214 个像素「像旧 muted」，tol 0 是
0，那些是新灰在白底 ~73 % 覆盖的抗锯齿边缘，数值上正好路过旧值的邻域。

**这一刀真正没被画过**：`block-colors` 场景只染了 blue / red / green / 蓝底四块，orange 和 yellow
作为文字从来没出现在任何一张图里，只有菜单圆点那 120 px。也就是说这次改的两个值，之前唯一的像素
证据是一个不着色的圆。于是 sweep28 → sweep29 给 fixture 补了两行（orange-on-orange、
yellow-on-yellow，明暗各一份），只动 2/54，其余 52 张逐字节相同；census 现在在浅色里数到 289 px
`#bd6408` 与 164 px `#a87718`（sweep28 是 0），深色同一行是 `#e28d50`/`#dcae3f`。放大裁图看着仍是
橙与黄，只是偏赭与橄榄。

**验证**：`cargo check --all-targets`、`cargo test --all-targets`（375 passed / 0 failed / 11
ignored，10 个 target）、`cargo build --release`、`cargo build --features software --bin
quire-shot`，四条零警告。测试数比上一刀记的 374 多一个：`grep -rn "#\[test\]" src tests | wc -l`
= 386 = 375 + 11，树上每个测试都在册，所以 374 那个数是漏计，不是这次多了东西。这一刀没跑 RAM 闸：
三个改动是编译期颜色字面量，另一处是给一个只在 headless 场景里跑的 fixture 多了两条染色记录，
投影与存储都不为此多分配——没有可归因的分配，就没有可测的臂。

**未验证**：真实显示器上的观感（软件光栅器的 gamma 与 GPU 路径不完全一致，这一层只能请人眼）；
WCAG 不是感知色差模型，它只保证亮度比，不保证色相在低对比下还分得开；没有做色觉模拟——orange
和 yellow 加深以后与 brown 槽（`#9f6b53`）在红绿色盲下可能更近，这一条留给下一次有人拿
`.scratch/contrast.py` 加一列的时候。

## 页内查找 · 计数条报 16，页面上就要数得出 16（2026-09-21，on `master`，ADR-0043）

**缘起**：A4 视觉回归的最后一条 D7（MEDIUM）：「per-page find 报 2 / 16，页面上一个命中都看不见」。
它撞的是 ADR-0042 绕开的那面「一个 `Text` 一种颜色」的墙，但绕法不能照抄：代码块靠等宽字体把六份
同样的字符串逐字叠起来，正文是比例字体，没有任何字节偏移能预测像素。底色可以。ADR-0041 已经把一行
切成 word-run 格子，把命中的两个字节边界当成多出来的切点，一格就正好是一个命中，格子底下垫一个
`Rectangle`——和 inline code 的灰底是同一件东西。标记与底色分工是量出来的：没有任何一种底色能同时
「自己在白纸上到 3:1」又「不压垮九种块文字色」（能到 3:1 的琥珀会把 yellow 槽拖到 2.85），所以底色
只说「这一格」（浅 #ffe9a8 / 深 #3d3413），边框才是标记（#bd6408 / #a87718，对页面 4.21 与 4.39，
对自己压着的底色 3.50 与 3.13）。

**「画完了」是数出来的**：抽样张那一页 16 个命中。800px 窗口里 9 个框，把窗口拉到 3000px 是 14 个，
剩下 2 个永远画不出框——它们在被光标占住的那一块里，那一块走的是编辑器的实时选中态，不是单元格。
这条对账（14 + 2 = 16）不是猜的：先按像素连通域数框，再用一次性探针把 `FindHits` 与行模型逐块打出来，
两者对得上才算闭。探针跑完就删了。

**撤回的两条**：一、上一版文档写着「`columns` fixture 的盒子是空的，所以 `find-cols` 画不出东西，
只能靠投影测试兜」。错。把已经进册两轮的 `columns-marks` 裁出来看，盒子里明明白白是「Column 1 /
Column 2」——那条结论是从没打开过的图上读出来的，而按它写出来的场景本来会成为这套图里的洞。二、我
一度认定页首那块灰底的是 callout，据此排了一个「callout 少画 2 个」的修复；探针显示它是 kind=0 的段落
（带一个灰底槽），少画的原因在上面那段。两条都在 ADR 与 ROADMAP 里改回来了。

**真正的缺口是引用块和高亮块**：它们各只有一个自己的 `Text`，正文不走 runs flexbox，所以命中被计数条
数进去了、页面上却没有格子。改法是让这两种也共用那条 flexbox，各自带自己的边框（引用块缩进
`spacing-md`，高亮块让开 emoji 那 40px），plain 那一支读的还是原来那几个表达式。

**z-order 这一坑值得单独记**：高亮块的染色底框在 delegate 里声明在 runs **之后**，Slint 后声明的兄弟
画在上面，于是第一版 `find-callout` 四条 gate 全绿、sweep 一切正常，图上是一个**空的高亮块**——单个
`Text` 已经为 runs 让位，runs 在但被盖住。症状永远是这个形状：文字不见了，而不是颜色不对。底框和
emoji 移到 runs 之前，字形和文本仍在其上，这才是这个块真正需要的顺序。

**四张表面四张图**：`find`（正文行）、`find-grid`（表格单元，命中没有自己的行，只能搭画它的那一行，
ADR-0028）、`find-cols`（分栏盒子，同上）、`find-callout`（高亮块自己的框）。`find-grid` 的检索词是
从「the」改成「North」才成立的：用「the」它会在页面上画 7 个框、网格里一个都没有——一张两种情况都
通过的图不算证据。

**像素读数**：sweep29 → sweep31（57 张）动了 **2**、新增 **3**、其余 **52** 逐字节相同。`find.png` 与
`dark-find.png` 各自增加 4 428 px 填充与 756 px 边框，两个主题在**同一批坐标**上各 9 个框——几何属于
标记、不属于调色板，这是唯一能这样证的证据。新三张：网格里 1 个 39×30，分栏里 2 个 52×30，高亮块里
2 个 21×30。

**验证**：`cargo check --all-targets`、`cargo test --all-targets`（377 passed / 0 failed / 12 ignored，
10 个 target）、`cargo build --release`（4m53s）、`cargo build --features software --bin quire-shot`，
四条零警告；`grep -rn "#\[test\]" src tests | wc -l` = 389 = 377 + 12，树上每个测试都在册。投影开销
另测（`--release`，10 000 行 / 1 000 行命中）：干净 46.979 ms，带命中 47.622 ms（**+0.643 ms**），
而每次按键真正付的是 `paint_find_hits` 里那趟「这块归哪一行画」的上溯，1 000 次合计 **1.753 ms**——
这就是为什么重绘只走命中的行、不做整页 re-project。这一刀没跑 RAM 闸：Slint 侧只是给已有格子多画一个
`Rectangle`，Rust 侧的分配上一刀已经量过（+0.643 ms 那一趟），没有新增可归因的常驻结构。

**未验证**：真人手测一条都没有——Ctrl+F 边打字边看格子跟不跟手、↑/↓ 走过 16 个命中、在引用块和高亮块
里敲字时格子会不会闪、深色主题下的观感。软件光栅器能证明几何与颜色，证明不了逐键重绘的手感。另外
「当前命中」和其他命中在画面上仍然不分级（它靠编辑器自己的选中态区分），这是一条独立的设计决定，
不在这刀里。

## M12 版式 · 一页的长相存在页上，字号一个都没落到块上（2026-09-21，on `master`，ADR-0044）

**缘起**：§三十八 的第一刀，也是 M12 唯一不需要新决策的前半段——font / full width / small text 三档，
规格已经写死了，包括那句约束：「三者只作用于当前页的排版 token，不得下沉成 per-block 字号」。
难点全在这句怎么落地。三条路：给每个调用点加三元表达式（119 处，等于把 token 的问题在 119 个地方
重新问一遍）；直接改 `Typography` 本身（它是 chrome 和文档唯一共用的一层，一页想要小字会把侧边栏
一起缩小）；再加一个派生的全局。选了第三个。`PageType` 是 `Typography` 的**孩子**——它把文档层的
字号乘一个系数、把字族换一个来源，两层因此可分，且推导全都在一个文件里读得完。119 处调用点从
`Typography.*` 改读 `PageType.*`，chrome 一处没动，`Block` 结构一个字段没加。

**存成什么形状**：Schema **v10**，`pages` 两列。`font` 是 TEXT，理由和 `blocks.kind` 是字符串同一个：
整数码的失败模式是「重排 enum 就把库里每一行的含义悄悄改了」，而 `PageFont::try_from_str` 把
`"comic sans"` 折成 `Default`，未来某个版本写的页在今天会是一张普通页而不是一次打开失败。
`layout` 是一个 INTEGER 位段（1 = full width，2 = small text），因为这两个开关永远一起走——
`Change::PageLayoutSet` 一次带两个、菜单一次写两个、读的时候一次测两位；拆成两个 BOOLEAN 就是在一个
本来已经要按列判存的步骤里再加一条 `ALTER`。v9 库升上来，每一页读回 `font = ''`、`layout = 0`，
也就是它升上来之前的样子——这条是 `the_v10_step_adds_the_page_look_to_a_v9_database` 断言的，不是假设的。

**小字是一个系数，不是一组新字号**：0.87，整个文档层一起缩，标题也算。这个数是 13.5 / 15.5，即本应用
自己的 code : body 比，所以两层之间的相对关系原样保留。行高 token **故意不乘**：它们是自然行盒的系数，
字面小了行盒本来就跟着小，再乘一次等于缩了字还压行距。

**两个接缝不看一眼就会漏**：一、带标记的 run 是**点名斜体字族**而不是设 `font-italic`，所以 serif 页
第一版会把强调词用 Segoe 斜体画出来、其余全是 Georgia——补了 `PageType.italic-font-family`。二、全应用
那个 `code-probe` 量的是等宽字体的步进，而 ADR-0042 的六层颜色是按这个步进断行的；它现在读
`PageType.size-code`，否则小字页上的颜色层是照原字号排的，会一层层错开。

**复制页带不带长相**：带。这一条抓到一个真 bug——`workspace.duplicate` 用默认值建新页，而持久化的
`PageCreated` 写的是源页的长相，于是复制出来的页在重启前后读出来不一样。修完之后 session 与重启一致，
`a_page_look_is_set_duplicated_and_left_alone_by_its_copy` 看着的就是这个不一致。

**入口与撤销**：顶栏 ⋯ → Style 子菜单（Back / 三个字体带勾 / Full width / Small text）。**不进 undo**，
和它上面的 Favorite 一样：长相是页的属性，不是对页的编辑，在页里打了一段字之后 Ctrl+Z 不该掉出一个字体。

**像素读数（这一刀的证据主体）**：sweep31 → sweep32 动了 **1 of 57**、新增 **7**，其余 **56 逐字节相同**。
唯一动的是 `menu.png`，1 430 px，全在 x 240..423 / y 542..779——子菜单自己的盒子。这是「改 token 层而
不碰 chrome」唯一能从外面证明的形式：真漏进 chrome，读数会是 56 张全动。六个内容臂各自对着
`default.png` 量（control：同一张对自己比，0 px）：serif 63 638、mono 62 780、small 55 285、full 51 533、
三档全开对 serif 臂 68 779、深色 serif 63 140。分界线在 x：**390** 对 **284**（= 侧栏 260 +
`Theme.spacing-xl` 24）——前三臂是换字族，只有 full width 那两臂的文字列起点左移了，也就是栏距真的动了。
这一位数字是把「布局开关」和「换个字体」分开的唯一读数。

**没跑 RAM 闸，理由写在账上**：一行读的是一个全局属性，改前改后形状一样，推导只在全局自己那次求值里
多一个乘法；块表没加字段、行模型没加字段、每行没有新状态。上面那 56 张逐字节相同就是替代证据。

**验证**：`cargo check --all-targets`、`cargo test --all-targets`（**382 passed / 0 failed / 12 ignored**，
8 个 target 各报各的）、`cargo build --release`（3m31s）、
`cargo build --features software --bin quire-shot`，四条零警告；`grep -rn "#\[test\]" src tests | wc -l`
= 394 = 382 + 12，树上每个测试在册。（本段最初记的是 380 + 12 = 394，加号左边那个数错了：
394 与 12 都是当场量的，380 是把八个 target 的 passed 相加时漏了两个，2026-09-22 重跑对回 382。）

**未验证 / 已知边界**：一、真人一次都没点过——Style 子菜单里真点一下 Serif / Full width / Small text 是
什么手感、切页时三档跟不跟得上、勾选态对不对，全都需要一个真窗口（headless 场景走的是同一批
setter，但它证明不了鼠标）。二、Georgia / Consolas 是 Windows 自带字体，这一条在 Windows 上成立，
Android（M9）上不成立，届时 `serif-font` 得跟着平台走。三、版式不进 Markdown 通道：
`export_page(blocks)` 连页对象都拿不到，导出的 .md 不带这三档，重新导入也回不来——§二十六 的通道
本来就是内容通道，不是属性通道，这一条是现状而不是遗漏，但如果以后要带 front-matter，得先出决定。
四、`duplicate_page` 只给被复制的那一页写长相，`copy_children` 带出来的子页仍走默认——本刀没动它。

## M12 图标 · 页存的是 emoji 本身，占位按位置读成三条（2026-09-22，ADR-0045）

**缘起**：§三十八 的第二刀。「icon：emoji 选择器 + 本地图片；未设置时用标题首字符占位，侧边栏与页面
标题同步显示」——这一句里唯一写死的是「存 emoji」还是「存下标」之外的东西，而真正需要决定的恰好是
它没写的两个问题：一个页到底存什么，以及「占位」在三个位置分别意味着什么。

**存成什么形状**：Schema **v11**，`pages.icon TEXT NOT NULL DEFAULT ''`，一列一个版本。存的是 **emoji
字符本身**，不是选择器在列表里的下标——下标会把一份 UI 清单变成线路格式，往中间插一个 emoji 就会把
昨天写的每一页悄悄改指别处；存字符则这份清单可增可删可重排，谁的页都不动。清单在 Rust
（`core::icon::PICKER`，96 个，12 行 × 8，`PER_ROW` 与 .slint 的每行格数共用），`.slint` 侧一个 emoji
都没写：`fill_icon_picker` 每次打开把清单拷进 `ModelRc`，所以格子和存储能接受的值不会各说各话。清单
里的条目一律写成转义，因为其中几个是「基码位 + 变体选择符」，一次「清理不可见字符」的编辑器好意就
会把某格变成另一个样子。迁移 11 顺手把「按列判存」从迁移 10 里提出来做成共用的 `add_page_columns`，
于是半途的库（有人手工加过列、从备份恢复到步骤中间）是收敛，而不是撞在重复列名上。

**「占位」按位置读成三条**（这一刀真正的决定，`docs/SPEC.md` §三十八 已把这段读法写进规格）：
一、树里的行取标题首字符——那个槽原来画的是一个通用页图标，它对「这一页是谁」什么也没说，首字符说了
一点；二、Favorites / Recent 只显示存下来的 emoji，没有就保留星形与时钟——那两处的小标记是**分节**
的身份证，被首字符吃掉是丢信息，不是补空缺，所以是 `icon_slot` 与 `icon_mark` 两个函数而不是一个；
三、编辑器大标题上方什么都不画——把标题首字符放大到 46px 再画在标题正上方，是回声不是占位。「同步
显示」落在写入路径上：一次 `set_page_icon` 同时刷新 hero 与整棵侧边栏，两处读的是同一个存储值。首字符
取第一个非空白**标量值**而不是第一个字素簇：以 ZWJ 序列开头的标题会被切开，16px 的槽里这是记档的
外观风险，不是新依赖。

**两个几何陷阱，都是像素抓出来的**：一、父页那一行本来就把 16px 的槽花在折叠箭头上，再给它一个图标框
就把**所有**父页的标签推得比自己的子页深一级——改成 emoji 与箭头共用同一个框、悬停时让回箭头；二、
第一版把首字符占位也套到了 Favorites / Recent 上，星形和时钟全变成了字母。两处都在 64 张基线场景里
留下了读数，也都在修完之后被同一个读数确认。

**入口与撤销**：顶栏 ⋯ → **Set icon**（新图标 `IcSmile`），选择器**替换**菜单而不是嵌在菜单下面——
锚点直接取菜单自己的 x/y，省掉一套 per-delegate 几何测量。写与清都持久化为 `Change::PageIconSet`，
**不进 Ctrl+Z**，和它上面的 Favorite、Style 同一条理由：页的属性不是对文档的编辑。`duplicate` 在内存
和持久化的 `PageCreated` 里都带上源页的图标。

**像素读数（这一刀的证据主体，形式与上一刀相反）**：sweep32 → sweep33 动了 **64 of 64**、新增 **3**。
这是**应该**的——每个没图标的树行都从通用页图标换成了标题首字符，基线不动才说明功能没画出来。把这条
和「改崩了所有场景」分开的是像素的**位置**：总计 72 937 px，众数场景 1 153 px 落在 x 13..52 /
y 365..713（深度 0..2 的那个 16px 槽），而同一趟比较只扫 x 53 往右，**64 张里 63 张是 0 px**，唯一
的例外 `menu.png` 345 px 在 x 254..415 / y 723..775——它自己那高一行的弹窗。上面那两个陷阱任何一个
没修，这个数就会变成 64 张全在 x 53 右边动。三张新场景：`page-icon`（hero 那一行）、`icon-picker`
（故意开在一张没有图标的页上，那才是用户去点它时的状态）、`dark-page-icon`（headless 软件光栅器把
emoji 画成单色、跟随文字颜色，深色下还认不认得只能在那里回答）。

**没跑 RAM 闸，理由写在账上**：每页一个短字符串；一次开页一次属性写入；每次侧边栏重建每行多两个小
String（拷 emoji、退回首字符），但同一行本来就把 title 拷进 `label`，分配的种类没变、只是常数因子，
且这条路走的是树变更而不是每帧。选择器是这刀唯一有尺寸的新分配：96 个 `SharedString`，一次菜单点击
几 KB，弹窗关掉即释放。

**本地图片那一半：不做，并且把代价写下来（ADR-0046）**。§三十八 的原文是「emoji 选择器 + 本地图片」，
这一刀只交了前半。这不是漏，是拍过板的：附件通道（ADR-0029/0030）只有**一档**降采样，
`MAX_EDGE = 1280`，那是为 ~780px 的正文列准备的、满打 6.5 MB 的 RGBA；而图标槽是侧栏 16px、hero 46px。
要骑既有通道，要么让每次侧栏重建去解一张 1280 边长的图来画一个字母大的记号，要么加第二档缓存——而第二档
真正的价钱不在像素，在**所有权**：M10 的回收扫描按「没有块引用它」删附件，icon 是**页**指向附件，不改引用者
列表，用户刚设的图标就是一次回收的牺牲品。加上 `pages.icon` 会从「一个字符或空」变成「一个字符或一个附件
id」的 tagged union（这张表在 `kind` / `lang` 上都是刻意躲开这个形状的），以及 Notion 自己也是图标只给
emoji、图片放封面——所以「放一张图」这件事由 cover 承接，那里本来就要求换图 / 移除与标题对比度。重开条件写
在 ADR-0046 里：真有别的理由要往 16px 槽里塞位图，第二档缓存与回收引用者**必须一起**做。

**验证**：`cargo check --all-targets`、`cargo test --all-targets`（**388 passed / 0 failed / 12 ignored**，
8 个 target 各报各的）、`cargo build --release`、`cargo build --features software --bin quire-shot`，
四条零警告；`grep -rn "#\[test\]" src tests | wc -l` = 400 = 388 + 12，树上每个测试在册。新增测试：
`core::icon` 三条（清单是整行且无重复、首字符、存值优先于占位）、`the_v11_step_adds_the_page_icon_to_a_v10_database`
（先控制断言列不存在，再要求它带着 v10 的两列一起活着）、`the_sidebar_slot_takes_the_emoji_and_only_the_tree_takes_the_placeholder`
（同一页在树里与在 Favorites 里，清空前后各读一次）、插入路径那条改成带真 emoji 写、带回来。

**未验证 / 已知边界**：一、真人一次都没点过：⋯ → Set icon → 选一个 → 再 None，这条路径的鼠标行为
需要真窗口；二、GPU 渲染器下的彩色 emoji **没量过**，只有软件光栅器的单色读数，这是手测项而不是结论；
三、hero 图标这一刀只是显示，标题上方悬停出现的「Add icon」入口留给下一刀；本地图片图标**不做**，理由与
重开条件在 ADR-0046；四、图标不进 Markdown 通道，理由与版式同（§二十六 是内容通道）；五、封面未动，所以
「封面之上的标题对比度」那条验收还没人替它说话——而 ADR-0046 之后，「给页放一张图」这件事全压在 cover 上，
下一刀要按这个分量做。（**当天下一刀已交**：封面见本节下面那一节，ADR-0047。）

## M12 封面 · 一层固定遮罩把「不得用最弱配色」变成一道能说不的算术（2026-09-22，on `track/1-page-appearance`，ADR-0047）

**缘起**：§三十八 第一组的最后一行。「cover：本地图片，可换图 / 移除；封面之上的标题对比度必须过
§二十一 的可读性要求，不得用最弱配色」。ADR-0046 之后，「给页放一张图」这件事全压在这一刀上，而它
自己还带着一句整个仓库到目前为止最难验收的话：一条**不能只靠看一眼**的对比度要求。

**存成什么形状**：Schema **v12**，一列 `pages.cover INTEGER NULL`，值是 **AttachmentId**。两个决定都
不漂亮，但都是被逼出来的：一、存 id 不存路径——§三十七 的回收扫描按「没有谁引用它」删附件，而它只能
从数据库里回答这个问题，一列路径是它解不回来的字符串，于是用户自己的封面会变成下一次回收的牺牲品（这
正是 ADR-0046 拒绝让 icon 走的那扇门，现在从另一侧走回来了）；二、**可空**而不是 `DEFAULT 0`——
`''` 对 emoji 无歧义，因为没有一个 emoji 是空的，而 `0` 是一个完完全全合法的附件号，所以「没有封面」
必须是第三种值。引用者列表因此加了两条：`workspace::cover_ids()` 与块并列进扫描，`Change::PageCoverSet`
回答 `attachment_ids_in`——**一条还没撤销的 undo 步骤也是一个指针**，否则「删掉唯一指着它的块然后
Ctrl+Z」这条路会把用户的数据先删了再假装恢复。

**「不得用最弱配色」怎么落成验收**：应用不可能在每次开页时按 hero 尺寸去解一张任意 JPEG，问它最亮的
像素是多少；逐图自适应的色又是一个 ADR-0039 拒绝给存储的派生值。所以选**一层固定遮罩**，把要求变成
有界算术：`Colors.cover-scrim` = `#0000009e`，黑，α 本身就是一个字节（`0x9e` = 158，即 0.6196），于是**任何**照片像素合成后 ≤ 255 − 158
= 97 sRGB，相对亮度 ≤ 0.1195，白字在上面是 **6.19:1**；而 4.5:1 这条线自己的门槛是 α ≥ 0.535，也就是
这一层的厚度不是品味，是一个解出来的区间。最坏情况是一张纯白的图，所以 `create_solid_fixture` 就写这么
一张（1280×400 全白），`page-cover-white` / `dark-page-cover-white` 是它的控制场景。

**量在像素上，不在规范上**：`benchmarks/scripts/contrast_probe.ps1` 读渲染出来的 PNG，在窗口里找声称的
墨色，取膨胀一圈后**最亮**的那个底色像素，报 WCAG 比值，默认要求 ≥ 4.5。这个脚本自己带三条臂，缺一条
都不算门禁：known-answer（21:1 / 1:1 / 6.19:1 三个已知颜色对，算错就 exit 2，先怀疑验证器）、**人造的
必然失败臂**（把同一个窗口放在 `#CCCCCC` 底上，1.61:1 → 说 FAIL 并以 1 退出；一个从没说过不的门禁等于
没有门禁），以及**找不到墨就 exit 2** 而不是「没报错所以通过」。实测：白色封面两个主题都是 **6.19:1**
（最坏底色 `#616161`，正好是算术里的 97），照片场景 14.04:1，带 emoji 的那张 14.09:1。

**这一刀真正的收获是让门禁抓到了一个 ADR 没预料到的缺陷**：hero 的 emoji 还在读 `Colors.text-primary`，
也就是 `#1f2328` 的火箭坐在 `#232439` 的照片上——相对亮度 0.0165 对 0.0191，**1.04:1**，等于不存在。
标题走了 `text-on-accent`，图标没走，而「封面之上」四个字不分标题还是图标。修完之后 `page-cover-icon.png`
是五张新场景里**唯一**变化的那张，于是它本身就是 A/B 证据。软件光栅器把 emoji 画成单色字形、跟随
`color`，这条才测得到；GPU 渲染器画彩色位图、忽略 `color`，所以那一路仍然是手测项（自 ADR-0045 起就欠着）。

**几何**：带高不是常量，是 `max(Typography.size-cover-height 168px, hero 底 + Theme.spacing-md)`。因为
icon 会把整个 hero 往下推，而固定带高会让白字直接站在**页面背景**上——那是遮罩唯一护不住的位置，也是
这一刀里唯一一个「看起来对、算起来全错」的形状。图片 `image-fit: cover`，`source` 走
`UIState.image-for(page-cover-id)`，和 image 行同一个回调。

**入口与撤销**：页面 ⋯ → **Set cover** / **Change cover**（同一个 id，标签回答「现在有没有」）/
**Remove cover**（只在有封面时存在——一个灰掉的条目是在提问，而这个菜单已经够长了）。选择器
`pick_cover` 与 image 行同一个图片过滤器、同一套导入，因为封面不是第二种文件，只是**又一个指着附件表的
东西**。跟 Favorite、Style、icon 同一条理由：**不进 Ctrl+Z**，页的属性不是对文档的编辑。`duplicate` 带上
源页的封面——副本是**第二个引用者**，指向同一份字节，所以清掉一页不能把另一页正在画的东西放走。

**像素读数（这一片最安静的一次）**：sweep33 → sweep35 动了 **1 of 67**、新增 **5**。66 张字节相同，唯一
动过的 `menu.png` 是 **9 个采样像素落在单列 x 414 / y 706..722**——页面菜单从十行变十一行之后
**ListView 滚动条滑块**的位置。这里差点读错：弹窗在这之前就已经被
`min(rows*30+8px, window-h - menu-y - 20px)` 夹住了，所以多一行剪不掉任何内容，能动的只有滑块；把它读成
「弹窗变矮了」会引出一次不存在的大改。判别方法是插桩打印真实行数（`fill_menu` 在页面 106 上给 11 行），
然后放大 2× 各看一张裁剪图。五张新场景承担正面证据：`page-cover`（真图）、`page-cover-white`（算术对照）、
`page-cover-icon`（带 icon 的高 hero，缺陷就住在这张的上一版里）、`dark-page-cover`、
`dark-page-cover-white`。

**没跑 RAM 闸，理由不是形状而是一个已经存在的上限**：封面不是 per-row 的活，一个 `Image` 元素、带子里、
只画在标题那一行；`image_for`（state.rs:1898）就是 image 行调的那个，于是一张 ≤ 1280 边的显示副本进的是
`MAX_ATTACHMENT_CACHE_BYTES = 32 MiB` 的加权 LRU，底下还有 Slint 自己 5 MiB 的路径缓存。也就是说「有封面的
页」和「顶部放一个 image 块的同一页」**等价**——这已经是这个功能能有的最便宜的做法，而且是存 id 而不是存
路径换来的。`id <= 0` 在碰到存储之前就返回默认图，所以 67 张既有场景一张都不付费，它们的字节相同就是证据。
回收扫描多一条 `cover_ids()` 查询，走的是 M10 的维护路径，不是每帧。

**验证**：`cargo check --all-targets`、`cargo test --all-targets`（**391 passed / 0 failed / 12 ignored**）、
`cargo build --release --all-targets` 零警告（7m 24s）、`cargo build --features software --bin quire-shot`；
`grep -rn "#\[test\]" src tests | wc -l` = 403 = 391 + 12，树上每个测试在册。
新增三条测试各钉一件事：`the_v12_step_adds_the_page_cover_to_a_v11_database`（先**控制断言**列不存在，
否则 NULL 什么也证明不了，再要求它带着 v11 的 icon 与 v10 的两列一起活着）、
`a_page_cover_points_at_an_attachment_and_survives_a_copy`（副本是第二个引用者、`cover_ids()` 听得见两次、
清掉一页不许放走另一页在画的字节）、`a_pages_cover_keeps_its_bytes_through_a_reclaim`（**重启会话**让 undo
不再投票，然后要求那张图先作为页的封面活下来、页面松手之后立刻走掉）。

**未验证 / 已知边界**：一、真人一次都没点过：⋯ → Set cover → 系统文件框 → 选一张，以及 GPU 渲染器下彩色
emoji 盖在封面之上；二、封面不进 Markdown 通道（§二十六 是内容通道，一页的图不是它的内容），也不进 undo；
三、遮罩不随图片明暗变化——这是**决定**而不是没做完，代价是一条极亮的照片上封面标题会显得比必要的更暗，
换回来的是「不用每次开页解一张 JPEG」和一句能证伪的验收；四、Track 1 的迁移优先级（v11–v13）在这里花掉
了 v12，编号 ≥ 12 的草案要往上挪；五、§三十八 剩下 **lock**、**version history**（仍欠它自己不肯留无限的
磁盘/内存保留数字）、**templates**。

## M12 锁定 · 门开在漏斗上，而拒绝对用户要能听见（2026-09-22，on `track/1-page-appearance`，ADR-0048）

**缘起**：§三十八 第二组的第一行。「lock：只读开关。TextInput、slash 菜单、拖拽、⋮⋮ 的编辑项全部关闭，
并且给出可见的锁定状态，**不能静默吞输入**」。前半句是一个星期能做完的东西，后半句是这个仓库到目前为止
唯一一条**关于失败体验**的验收：一个被吞掉的点击，和用户以为没点到的点击，长得一模一样。

**存成什么形状**：Schema **v13**，一列 `pages.locked INTEGER NOT NULL DEFAULT 0`。可空那件事在 cover 上是
被 `0` 逼出来的，这里正相反：「没锁」是一个**值**而不是一种缺席，所以 `NOT NULL DEFAULT 0` 让这条 `ADD
COLUMN` 不需要任何回填语句，一个 v12 的库升上来是每一页都开着——写错方向的默认值会表现为「我打不了字」，
而这个特性的默认值必须是「什么都不挡」。列加在 `pages` 而不是 `blocks`，因为用户点的那句话是关于整份文档
的；半锁的文档（三行能改第四行不能）不是 Notion 有的状态，也不是一次迁移能到达的状态。

**门开在哪里**：`exec_editor`，约九十处 `exec_editor` / `exec_on_open_page` 调用点共用这一道。替代方案是那
九十处各自记得，而「记得」正是这类门会烂掉的地方。但漏斗不是全部：`grep doc.borrow_mut()` 把剩下的入口一个
一个点出来——四张 bench 场景的造数据、两条 Markdown 导入（写进自己一秒前刚 `create_page` 出来的页，那页
必然是解锁的，所以锁永远不站在用户和他的文件之间）、两条树操作（`duplicate_page` / `delete_page`，锁不覆盖
树），以及**三个真入口**。审计在这里抓到两个洞，两个都是同一个形状：

```rust
let _ = s.exec_on_open_page(Command::SetBlockType { .. });   // 锁：返回 None
if old_kind == Page || Link { s.clear_block_ref(id); }        // 照写：引用被清掉了
```

`duplicate_page_block` 更糟：它在命令层看到这块**之前**已经按树造出一个子页，于是「被拒绝的复制」在侧栏里
留下一个孤儿页。所以这一片立的规则不是「把门设好」，而是**跟着一次命令走的写，必须看那次命令的返回值**；
三个漏斗外的入口（checkbox 的回调、`clear_block_ref`、`duplicate_page_block`）各自带门，测试除了比块列表
还比子页计数——一次让树变肥的拒绝仍然是一次写。

**两层门，不是一层**：Rust 那一层拒的是写。.slint 那一层拒的是光标：`EditorBlock.editing` 多出一个
`!UIState.page-locked` 项。为什么两个地方都要设门：`editing` 是绑在 `UIState.editing-id` 上的，而那个 id 是
控制器的 `on_block_activate` 设的——只在命令层设门，等于允许一个「按 Enter 才说不」的活输入框在重新加锁之后
继续存在。大标题那颗 pill 的点击走 `lock-nudge()` 回调回 Rust，因为措辞只能有一份：标题、行、把手、键盘，
四处说的是同一句话。

**「不能静默吞输入」落成什么**：通知条一句 `This page is locked — ⋯ → Unlock page to edit.`，加上三处像素
（标题上方的 pill、⋯ 里那一行自己变成 Unlock page、变短的 ⋮⋮）。**去重的依据是屏幕上那一行字**，不是队列
也不是布尔：bar 是粘的（dismiss 才走），而拖拽的 hover 每个重绘帧都要问一次 `can_move_block_to`，一个手势
几十帧就是几十次同一句话，那是另一种缺陷。hover 自己**保持安静**——落点线消失本身就是那个手势的反馈，
紧随其后的那一次 drop 才值得一句；这条不对称是写在两个调用点上的注释，而不只是代码。⋮⋮ 的编辑项是**删掉**
不是置灰：那个菜单没有 disabled 状态可以说「不行」，而一个谎报自己要干什么的条目比一个短菜单更糟；留下的两
行是只取的（Copy link to block / Copy block）。

**锁不住的四样东西，各自有理由**：一、`ToggleFold` 仍然执行——§三十七 已经把它登记成唯一持久化的*视图*状
态，加锁不该让用户失去这页的大纲；门里写着一个具名例外，就得用断言钉住它（其余十条命令返回 `None` 时它必须
还是 `Some`）。二、页面自己的外观照写——icon / cover / Style / favorite 是 §三十八 前面几段的特性，锁管的是
文档说什么，不是文档长什么样。三、树操作照做——移动、删除、复制页是结构而不是内容。四、复制页**不**继承这把
锁，跟继承字体 / icon / 封面正好相反：外观跟着走是因为副本该长成源的样子，锁不跟着走是因为「复制一个 locked
的页」正是用户想改它又不动原版的动作。撤销与这三条都不同：**拒的不是丢**，锁之前建起来的 undo 栈原样留着，
解锁之后接着用，测试靠「解锁后往同一批行里再打字」把这句话钉住。

**像素读数**：sweep35 → sweep36，动了 **1 of 72**、新增 **4**。71 张字节相同，唯一动过的 `menu.png` 是
**8 个采样像素落在单列 x 414 / y 692..706**，即页面菜单为第十二行改形的**滚动条滑块**——弹窗在加这一行之前
就已按 `min(rows*30+8px, …)` 夹住，多一行剪不掉任何内容，能动的只有滑块（与上一片同一个陷阱，这一次没有再读错）。
四张新场景每张都另找对照来自证画出了东西，而不是「新文件所以肯定对」：`page-lock` 对
`default.png` 是 **244 px 落在 x 390..560 / y 52..74**——标题上方那一颗 pill 的精确位置，如果 `page-locked`
没真的传到 .slint，这个 diff 会是 0，而 0 在这套流程里长得和成功一模一样；`page-lock-block-menu` 对
`block-menu.png` 是 2 101 px，而且是**干净**的一对（那张场景逐字抄了 overlay 分支：同一个块、同一个 x 320 /
y 300），差异就只剩「八行没了 + 一颗 pill」——它自己对 `default.png` 809 px，未加锁那张 2 055 px；
`page-lock-menu` 要记一条老实话：它开的是编辑器当前那页的 ⋯，而 `menu.png` 锚在 106 那一行，两个盒子根本不在
同一个 y，所以 4 586 px（对 `menu.png`）读的是「两盒 + 一个标签」，对它自己的 `default.png` 才是 3 316 px /
x 240..560 / y 52..610（12 行的弹窗 + pill）；`dark-page-lock` 是暗色的 pill，和所有 `dark-*` 一样整幅重画。

**没跑 RAM 闸，理由是可测的东西不存在**：一 bool 一页，读它的是 `locked_of`（一次 HashMap 查找，和它旁边的
`cover_of` 同一个），hover 门是一次比较而不是一次分配，`note_locked` 每次事件最多进队列一条。没有每帧成本
可量，所以证据是 71 of 72 字节相同——既有场景一张都没多付钱。

**验证**：`cargo check --all-targets` 零警告、`cargo test --all-targets`（**395 passed / 0 failed / 12
ignored**）、`cargo build --release --all-targets` 零警告、`cargo build --features software --bin
quire-shot`；`grep -rn "#\[test\]" src tests | wc -l` = **407 = 395 + 12**，树上每个测试在册。四条新测试各钉
一件事：`the_v13_step_adds_the_page_lock_to_a_v12_database`（先**控制断言**列真的不在——`DROP COLUMN` 之后
不先证明这一点，NULL 什么也说明不了——再要求它带着 v12 的 cover 与 v10 的两列一起活着，然后往返
`PageLockedSet` true/false/true 并顺带改名，钉住「锁是页的一列，不是罩在其余列上的影子」）、
`a_locked_page_refuses_every_edit_and_leaves_the_document_as_it_was`（十五条写入口全部返回 `None`，块列表
逐字节等于加锁之前，解锁之后同样的调用全部落地）、
`a_locked_page_refuses_once_per_gesture_and_still_folds`（三十次拒绝只留一句话、hover 一句都不留、
`ToggleFold` 仍然执行、锁住的页也不是别人的落点）、
`a_locked_page_stays_a_page_and_a_copy_of_it_is_open`（副本不带锁、源页仍锁着）。
另外做了一次**变异检查**：把门表达式强行短路成 `false`，两条新测试必须失败（实测失败信息分别是「the editing
input」和「thirty frames of one gesture, one line on the bar」）。能说「不」的门禁才称上过话——这一条和上一
片对比度探针的必然失败臂是同一件事。

**未验证 / 已知边界**：一、真人一次都没点过：⋯ → Lock page → 点一行 → 看见 pill 与通知条（连同 cover 那两
片欠的换封面与 GPU 彩色 emoji 盖图）；二、`locked` 不进 Markdown 通道（§二十六 是内容通道，一页的权限不是
它的内容），也不进 undo（ADR-0044 那条「长相是属性不是编辑」在这里改成「门是属性」）；三、LAN 服务不需要
门——它构造 `Page` 值来读和导出，从不写；四、锁不做**密码**：它挡的是用户自己的手，不是别人的手，一列
`INTEGER` 没有加密，SPEC §三十八 的「只读开关」就是这个意思，别把它当权限写进任何面向第三方的文档；五、
Track 1 的迁移优先级在这里花掉 v13，`CURRENT_VERSION = 13`，编号 ≥ 13 的草案要往上挪（含 Track 3 那份
database 草案）；六、§三十八 剩下 **version history**（仍欠它自己不肯留无限的磁盘/内存保留数字）、
**templates**。

## M12 模板 · 一个特性最重要的产物是它没有造出来的东西（2026-09-22，on `track/1-page-appearance`，ADR-0049）

**缘起**：§三十八 的第三组：「页面内模板按钮 + 新建页面时选模板」「workspace 模板库：预置若干本地模板，
导入导出走 §二十六 的 Markdown 通道」，以及这一阶段唯一一条写成禁令的验收——**「模板的表示必须是『块序列
的副本』，不得引入第二套内容格式」**。前两条是一个星期能做完的东西，第三条是一个决定：一个 Notion 式的
模板系统，最自然的写法是 `templates(id, body_json)` 或者一份 `.template.json` 文件，而那正是这句话禁掉的
形状。于是这一片的产物主要是**没有造出来的东西**：没有 `templates` 表、没有 `template_blocks` 表、没有模板
文件格式、没有第二套序列化。

**存成什么形状**：Schema **v14**，一列 `pages.template INTEGER NOT NULL DEFAULT 0`，与 v13 那条 locked 逐
字同形（「不是模板」也是一个值，所以 `ADD COLUMN` 自带默认值、不需要回填语句，一个 v13 的库升上来是**一个
模板都没有**）。模板就是一张页，那一列是它身上唯一一条普通页没有的事实，正文与所有页面共用 `blocks` 表。
禁令于是被**结构**满足而不是被纪律维持：`fill_template` 复制的是 `Block` 行，marks / 颜色 / `lang` /
`columns` / `img_percent` / 一个 `Page` 引用一律随行，一行映射代码都不写。它二十行就够，做三件事：id 由
文档自己的分配器另发（副本永不会与源相撞）、order key 原样留着（一个 key 只在一页之内有意义，而新模板没有
东西可与它撞）、parent 指针经一张 `HashMap` 重映射到副本上——把一张表格的格子与一个 toggle 的孩子绑在一起
的就是最后这一步，漏掉它的副本会长成一堆散根。

**旗标只有一条写入路径，这是被删出来的**：`PageCreated` 带着整个 `Page`，所以 `create_template` 记录它时顺
带就把旗标写进库了，**没有** `Change::PageTemplateSet`。这不是省事，是砍掉一个开关：一个「把现有页变成模板」
的入口会要求 invisibility 那套东西双向跟随（一张正在被编辑的页忽然不能打开了？它的 undo 栈归谁？），而 §三
十八 的语义里改一个模板的方式本来就是「拿它开一页、编辑、存回去、删掉旧的」——那已经是子菜单里的三行。代价
老实写着：**保存从不覆盖**，所以旧副本会留在库里直到用户删掉它，而库里可以有两条叫 "Meeting notes" 的行（库
按年龄排序，不按标题；两个同名是用户的业务，不是重排菜单的理由）。

**「看不见」读成不挂树，而不是一长串过滤器**：`Workspace::create_template` 造一页、翻一个旗标，既不进
`roots` 也不进任何父亲的 `children`。于是侧栏、页面树、命令面板、Move-to 的遍历、Recents 全部**免费**跳过
它——没有一份「后来人记得要过滤」的清单需要维护，而这正是这类特性会烂掉的地方。四把不走树的门各自补一项，
一项一处：`open_page` 在 `mark_opened` 之前返回（打开会写 `recents` 与 `current-page` meta，那是模板再多不
能出现的两个地方，所以这扇门不是「列表里藏起来」而是「没有门」）；`search_index::matches` 的 join 上多一个
`AND p.template = 0`；LAN 分享过滤页列表与子页遍历，并且 `/api/page/<模板 id>.md` 回答 **404** 而不是它自己
的 Markdown——对岸的 `unframe_workspace` 没有办法表达「模板」，那会把它落成一张普通页，而模板有正是它没有的
那一列。

**第五把门考虑过并且拒绝**：不给模板的行建索引（写侧排除）。它看起来更干净，实际要付两样：`insert_block`
得去问自己脚下这页是不是模板（一个 join 已经知道的事实出现**第二个真相源**），而 `rebuild` 必须与 `insert`
对「哪些行属于索引」持不同意见——否则模板会在一次重建之后开始出现在搜索结果里。钉住这条的测试因此不能只断
言「搜不到」：`a_template_is_indexed_and_never_found` 先要求命中列表为空，再拿一条裸 SQL 断言
`search_blocks` 里**确实有两行**指这段文字，然后 `DROP` 掉两张 FTS 表 + 把 `user_version` 拨回 1 触发重建，
再要求两件事同时仍然成立。一次「看不见」如果是没数据造成的，这个测试会红。

**一次插入 = 一步撤销**：`Command::InsertForest { anchor, blocks }` 是这一片新造的命令，它存在的理由是
`Ctrl+Z` 的粒度——十一块的模板按一次撤销就该全走，而不是留下九块。它产出的变化清单就是普通
`BlockInserted`，所以 flush、FTS 与重启看到的都只是「这页长了几行」。两处细节值得记：一、锚点上那行**空的**
paragraph 在同一批里被替换掉（`+` 那一行与新建页的第一行都是空的，模板落在光标下面一行看起来像没生效），
这只有 `exec_all` 拿**前态**给每条命令做计划才成立，换成逐条执行就是删掉一个已经不存在的块；二、它返回第一
个插入块的 id，因为这条命令可能**删掉**用户点的那一行，光标留在一个已不存在的块上是一次点击之外的编辑失
败。`fill_template` 反过来**不产生** undo 步骤，这是设计：历史栈属于用户正在打字的那一页，而模板页从来不是
开着的——存错的回头路是 Templates > Delete。

**内置库是导入进来的，不是迁移写进去的**：五个预置（`core::template::PRESETS`：Meeting notes / Weekly
review / Project brief / Bug report / Long-form draft，按「多久会有人需要」排序而不是字母序）是 `&'static str`
的 Markdown，落库走 `import_template` —— 也就是菜单里 Import 那一行调的同一个函数。写进 migration 的话就
得手工对齐 order key、`block_children` 与两张 FTS 表，而这条路径已经把三件事都做对了；于是可能出错的代码路
径只有一条。两道门各管一个失败方向：settings 旗标 `builtin-templates-seeded` 让「删掉五个」活得过重启（否
则菜单的 Delete 行是一句谎， built-ins 像家具一样钉在地板上），名字检查让一次半途而废的 seed 可以重试（旗标
在正文**之后**才记，什么都没写进去的会话应当重试而不是旗标为已种）。`persistence.is_none()` 直接拒绝：mock
工作区没有地方存旗标，放行就是每次启动多五张隐藏页——这也是 headless 的 bench 场景与视觉捕获必须自己画一份
库（`seed_template_library`）而不是看见这五个的原因。

**页面内那一半开在两个已经存在的入口上**：slash 菜单与「+」把手的插入菜单，两者的候选表尾都接
`template_slash_rows`，hint 列写死一个 `Template` 词（模板不在树里，没有面包屑可给）。id 编码成
`TEMPLATE_SLASH_BASE + 库里的下标`，两个决定各有理由：**下标而不是页 id**，因为一行必须活过「弹窗开着而库
变了」；**基址远在任何块类型整数之上**，因为 `slash_selected_kind` 要能分清「这行是模板」与「这行是我不认识
的类型」，而后者会被 `kind_from_int` 折成 Paragraph——把用户的字变成一个空段落是这里唯一一种**静默**错法的
结局。`template_slash_rows` 里 `.enumerate()` 必须在 `.filter()` **之前**：一行模板的 id 索引的是库，不是当
前可见的那几行。这一条做了变异检查——把 `enumerate` 挪到 `filter` 之后（第一版改写编译不过，因为元组形状变
了，换成能编译的等价错法才跑成），新断言按设计失败并给出 `left: Some((7, "Meeting notes")) / right:
Some((8, "Weekly review"))`，然后原样改回、确认通过。

**入口是一行，不是六行**：页面 ⋯ 菜单多了 **Templates**（菜单因此十三行），子菜单才有六行。标签短是被迫的
而且是可查的：全应用共用一个 184 px 的 `ContextMenu`，它的行是 elide 而不是换行，第一版渲染出来六行里有四
行结尾是省略号。改的是标签（"Use as new page" / "Save as template" / "Export Markdown" / "Import
Markdown"），**不是**弹窗宽度——加宽会动到应用里每一个菜单，并要求重判大半基线，为一子菜单的措辞。对象已经
被用户站着的那个子菜单与紧随其后的库选择器各说了一遍，所以一眼读完的那一行值得留下。Delete 不再弹确认框
（那是用户从自己起的名字里挑的第二下点击，行本身已经画成危险色——库选择器是全应用唯一一处整列走危险色的地
方，因为在那一列里每一行都是你要毁掉的那一行），但它必须说话：通知条点名被删掉的是哪一个。

**测试顺手抓到 §二十六 通道里一处真缺陷**（不是模板的）：`a_template_round_trips_through_the_markdown_channel`
报「Meeting notes 活不过它自己的导出」，九行变八行。原因在 `export_service`：一个**空的**列表项会被整行丢掉，
于是「往返」这件事在空行上一直是假的。对普通页面这只是少一行，对模板是致命的——一个空的 bullet 就是模板的
**内容**，它是留给别人填的那一行。修法：`prefix_lines` 为空块也写出标记（`-`、`>`、`#`），编号那一臂写 `1.`
且不带尾随空格（与空标题写 `#` 是同一笔交易）。这条改了**所有**页面导出的行为，所以规则同时进了 §二十六
正文与 export 文件头的布局清单，测试 `an_empty_block_exports_its_bare_marker_and_comes_back_as_itself` 让五
种能为空的块各走一遍。

**另一处账是测试自己写错的方向**：`AppState::delete_page` 回答的是「删掉的是不是屏幕上那一页」，不是「成功
没有」，所以对任何模板它恒为 `false`。两条新测试一开始 `assert!(state.delete_page(...))`，失败得莫名其妙；
现在改成前后两次 `workspace.contains()`——那才是菜单的 Delete 行需要为真的那句话。

**回收扫描一项都没加**：模板的行就是 book 里的块行，`doc.all_blocks()` 已经把它们指着的图片算作引用者；
`cover_ids()` 与 undo 的投票都没长出自己的模板项，因为旗标自己不指向任何文件。反过来说，一个装着 image 块
的模板会替那张图说话，这是「用真实行当表示」白买的一笔。

**像素读数**：sweep36 → sweep38，动了 **2 of 76**、新增 **4**。74 张字节相同；两个动过的都是页面 ⋯ 菜单，
而它们是**同一个菜单在两个锚点上的两种数字**——`menu.png` 只有 **6 个采样像素落在单列 x 414 / y 682..692**
（那张弹窗在此之前已被 `min(rows*30+8, window-h - menu-y - 20)` 夹住，多一行剪不掉任何东西，动的只有
ListView 的滚动条滑块，与上一片同一个陷阱），`page-lock-menu.png` 是 **428 px 在 x 240..422 / y 558..640**
（那张锚得够高，十三行全画出来，插入点之下的每一行都位移）。把小数字读成「几乎没变」是这一片要防的错法。
四张新场景各自对 `default.png` 自证：子菜单 2 115 px 在 x 240..422 / y 244..460（一个 7 行的盒子），库选择
器 1 714 px 在同一条 x 带里 y 244..430，slash 那一行 2 332 px 在 x 340..618 / y 260..298——三个盒子都超出
自己的边界 0 像素，也就是背后那页一个像素都没动。标签改短之后**又跑了一次全量**：**78 of 80 字节相同**，两
张动过的正是 `page-templates` 与 `dark-page-templates`，这就是「改标签没有碰到别的菜单」的证据而不是推断。

**没跑 RAM 闸，理由不是「形状简单」而是有一个数**：内置库是唯一让每个新库变大的部分，所以它被**量**过而不是
被论证过——一个临时探针（跑完即删）在真实种好的 `AppState` 上读出 **5 张隐藏页、48 个块行、文本合计不到
2 KB**（markdown 源与存下来的 text 加在一起 1 900 字节）。那是一次性写入，不是运行成本：行是普通 `pages` /
`blocks` 行，FTS 多索引同样那 48 行，旗标记完之后没有任何启动会再去看预置。读侧排除的代价也就是这 48 行在
索引里搜不到——那是「一个 join 项」换掉「两个真相源」的价格，写在 ADR 里。其余都是每页一 bool（`is_template`
一次 HashMap 查找，和它旁边的 `locked_of` / `cover_of` 同一个形状），只在填菜单与换页时被读，没有每帧路径。

**验证**：`cargo check --all-targets` 零警告、`cargo test --all-targets`（**409 passed / 0 failed / 12
ignored**）、`cargo build --release` 零警告、`cargo build --features software --bin quire-shot`；
`grep -rn "#\[test\]" src tests | wc -l` = **421 = 409 + 12**，树上每个测试在册（第一版把「各 target 的
passed 之和」当成了总数、又去减 12，得到 397 + 12 = 409 这样一个自洽但错的数；`-- --list` 数出 lib 的 266 条
之后逐 target 重算才对平——总数与在册数对不上时先怀疑加法）。十四新测试各钉一件事：lib 11 条
（`app::state` 8：看不见 / 插入替换空行且一步撤销 / 一行模板不是块类型 / 存两次不覆盖 / 内置库只落一次且删
除活得过重启 / 通道往返 / 锁页拒绝插入但仍提供保存 / 从模板来的页是普通页；`core::command` 2：森林副本保形
状且一步撤销、拒绝它落不下的输入；`services::lan_server` 1：种子模板与普通页**只差那一列**，于是三个断言是
三把独立的门）、storage 1（v13→v14，先 `DROP COLUMN` 再**控制断言**列真的不在，然后要求旧库读回 0 且
locked/cover 原样，最后 `PageCreated` 带旗标 + 两行正文往返）、search 1（上面那条）、markdown 1（五种空块）。

**未验证 / 已知边界**：一、真人一次都没点过整个 Templates 子菜单，其中 Export / Import 两行走的是 `rfd`
文件对话框，headless 到不了；连同 cover 两片欠的换封面与 GPU 彩色 emoji、lock 那片欠的 pill 与通知条。二、
`template` 不进 Markdown 通道（§二十六 是内容通道），也不进 undo（一张你能打开的页不是一份可抄的正文），也
不进 `duplicate`（复制出来的是页而不是模板，`create_page` 给的就是 `template: false`）。三、预置模板只能装
Markdown 装得下的东西：callout / table / 颜色 / 分栏在 §二十六 没有语法，要了就会悄悄变成一个段落，所以内置
库画不出那些形状，而用户自己存的可以。四、保存从不覆盖 → 库会积累同名副本，除了 Delete 没有整理入口（没有
重命名、没有排序、没有「用这个替换那个」）。五、模板**不是**权限：它不加密、不隐藏给「另一个用户」看，LAN
那三扇门是因为对岸没有表达它的形状，而不是为了保密。六、Track 1 的迁移优先级在这里第三次花掉——v14，
`CURRENT_VERSION = 14`，编号 ≥ 14 的草案要往上挪（含 Track 3 那份 database 草案）。七、§三十八 只剩
**version history**，它落在下一片（ADR-0091）：那一片一条迁移都不加，而它欠的那个「保留数字」最后是量出来
的字节与毫秒。

---

## M12 版本历史 · 一个版本是一个数据库文件，所以这一片一条迁移都不加（2026-09-22，on `track/1-page-appearance`，ADR-0091）

**缘起**：§三十八 的最后一组，三条验收写得比前几片都具体：**「三个动作都必须可用：命名 / 对比 / 恢复」**、
**「对比要给出可读的差异，不能只说『有两个版本』」**、**「保留策略必须落到数字，不接受无限增长」**。前两条
是 UI，第三条是一个不许用「合理」搪塞的账。而这一组还带着一句与模板那片同一形状的指令：**「复用 §二十五
的 snapshot 机制，不另造一套存储」**——上一片读到的是"不引入第二套内容格式"，这一片读到的是"不引入第二个
存储"，两处的做法也一样：把要存的东西做成已有的东西，禁令就没有可违反的对象。

**存成什么形状**：一个版本 = `versions/p<page>-<created>.db`，一个真的 SQLite 文件。造法是 §二十五
`backup::snapshot` 那句 `VACUUM INTO` 原样搬过来，然后**缩到一页**：一条 `DELETE FROM pages WHERE id != ?1`
（`blocks.page … ON DELETE CASCADE` 顺手带走块行，`block_children` 与 `marks` 再跟着自己的那条链），两张
FTS 表清空，最后 `VACUUM` 一次把删掉的页还给文件系统。拷贝那一次写走 `synchronous=OFF`，写完立刻回 `FULL`
（真实写入一条都不降级），然后**用 `Database::open` 重新打开它**当作校验——会跑迁移、会 integrity check，
与 §二十五 恢复一个 `.bak` 走的是同一道门。没有 `versions` 表、没有 `page_versions` 表、没有 blob 列、
**没有迁移**：`PRAGMA user_version` 停在 14。M12 六片里前五片各花掉一列（v10 … v14），第六片一个都没花，
Track 1 的迁移优先级在这条线上第一次不用往上挪。

**「复用」照字面读，回报在读取侧**：版本**就是**那个格式，所以它不可能与格式漂移。`versions::read` 用
`SqliteRepository` 打开文件、调 `Repository::load`——就是 App 打开库时用的那个 loader——拿回来的一页带着
marks、颜色、`lang`、表格的格子、attachment id、`Page` 引用，**一行新映射代码都不写**。反过来说，一个
`versions(id, body_json)` 表造出来之后，从此每一次给 `Block` 加字段都要记得同步它，而忘了的那次不会报错、
只会让恢复出来的页少一样东西。§三十八 那句「不得引入第二套内容格式」在模板那片用同一个论证满足了一次，这里
第二次。

**索引是 metadata 两行，不是一张表**：「有哪些版本、用户管它叫什么」要活过重启，所以
`version/<page>/<created>` 存标签，`version-files/<page>/<created>` 存那一页块行指着的 attachment id。第二
行不是顺手——§三十七 的回收扫描问的是「还有东西指着这个文件吗」，而它查的是**活库**，版本文件是一个它永远
不会打开的库。没有这面镜子，一张后来换掉封面的页会让它自己的版本恢复出来是一个缺失图片，那正是 ADR-0047
的反对意见从反方向走来。于是 `AppState::reclaim_attachments` 长出一项（`version_pinned_attachments`），测
试 `a_version_pins_the_picture_its_page_no_longer_shows` 钉住它。每个版本两行 meta，比一次查询打开二十个文
件便宜。

**这一片抓到一处真缺陷，而且它是被量出来的不是被想出来的**：第一版的清空只有 `DELETE FROM search_blocks`
/ `search_pages`，测试同意——两张表 0 行。而一个 201 页库的版本文件重 **1 560 576 字节**，比它该有的
122 880 多了 1.4 MB：FTS5 的词表与会讯在 `<table>_data` 那棵 b-tree 里，普通 delete 会走过去并且**把它留
在原地**（实测 368 行 / 1 441 792 字节），也就是那个"只有一页"的文件里存着全库每一页的词。`clear_index`
现在跑 `DELETE` 之后再跑 FTS5 的 `rebuild` 命令，让它从（已经空的）内容表重新导出 `_data`；`delete-all` 更
短，SQLite 在这里直接拒——它只服务 contentless / external-content 表，而这份索引自己存内容。修法之外更重要
的是断言换了地方：现在钉的是 `_data` 的行数，**读者会查的那两张表的行数正是说谎的那个**。与 memory 里"静默
探针 ≠ 阴性"是同一条课，这次说谎的不是脚本，是 schema。

**数字（写在 docs/PERFORMANCE.md，探针留在树上一支 `#[ignore]`）**：夹具 201 页 / 17 000 块 / 库在盘上
9 888 480 字节（含 `-wal` 与 `-shm`，因为 `VACUUM INTO` 看得到主文件还没收下的行）。一页 60 行的版本
**122 880 B = 库的 1%**，5 000 行的 **790 528 B = 7%**，**20 个长页版本 15 810 560 B = 159%**（清空 FTS
之前这三格是 15% / 22% / 450%）。一次保存 release 下 1 031 / 469 / 643 ms，Debug 下 614 / 469 / 577：
**字节跨 profile 相同、毫秒不可比**，与 §二十五 那条 backup 行的结论一致——这是文件 I/O 与 b-tree 重建，优化
器无事可干。120 KB 是 schema 的地板（30 页 × 4096：`blocks` 12 288、`sqlite_schema` 12 288，其余每张表与
索引一个 root page），一行字的页与六十行字的页同价，所以帽**挂在每页**而不是全局——一个全局帽会让一张忙页把
所有安静页饿掉，而价钱一样。RAM 半边由构造界定：一次只开一个版本、临时连接、没设 `cache_size` 所以 SQLite
默认 −2000 KiB ≈ 2 MB 是顶；对比两侧各拿 `size_of::<Block>() = 136 B`（5 000 行 = 680 000 B）加
`ALIGN_CELLS = 1 << 18` 的 1 MiB LCS 表；**留着二十个版本花 0 字节 RAM**，因为它们是文件。

**三个动作、一个弹窗、两个视图**：列表与对比是同一批四个问题（哪一页、哪一版、改了什么、按钮干什么），所以
共用一个 460 px 宽、**高度固定**的窗口——点一行不能把面板从指针底下挪走。命名是底部那条一直在的输入框（"now"
在任何时刻都值得一个版本），对比是一行点击，恢复是一个**看过对比之后才存在**的按钮：一个替换整页的动作不该第
二下就点得到。行携带 **row index** 而不是时间戳，因为 Slint 的 `int` 是 32 位而 unix second 不是，
`version_at` 把 index 换回 `(created, label)` 并且对一次删除之后剩下的空位返回 `None`；年龄走
`diff::age_text`，保持全应用那条「不做时区、不做日历」的规矩。

**保存的顺序不是讲究，是正确性**：`VACUUM INTO` 读的是**文件**，不是这个 session 的内存，所以队列里还压着
未写编辑时拍的版本是"上次 flush 时的页"，一个名叫"这个"的东西交付"十秒前"。`save_page_version` 因此先
`persistence_force_flush()` 再拷贝；同一个文件名就是同一个 unix second，撞了**重试四次**（≈ 五版/秒）而不
是悄悄挑一个钟点——文件名与 meta key 都是那一秒，两个同秒的版本是一个丢了名字的版本。

**对比是可读的差异，且被界定**：`core::diff::compare(before = 版本, after = 当前)` 用 block id 认人（快照
带的就是这一页的行，没动的行两边同 id），剪掉公共头尾，只对活下来的中段做 LCS，`ALIGN_CELLS = 1 << 18`（
512 × 512，≈1 MB 的 `u32`）之外**报告**成整页重写而不是继续对齐——多出来的行仍然真，而一个要一秒才打开的面
板比一个多显示几行的面板差。就地改一行到一对 `Removed`/`Added`（同一种块），词级 diff 要在块里面再放一套格
式，那是 §三十八 上一级禁掉的事。`same_body` 排除 id / page / order 与 `folded`，最后那个因为 §三十七 把
它登记成视图状态：收起一个 section 不是对页面说了什么做了编辑。

**恢复走命令系统，于是一步撤销**：`restore_version` 是一个 `exec_all` 批次——`InsertForest { anchor: None }`
装版本的行，然后给当前**每个根**一条 `DeleteBlock`（一条删除带走整棵子树，表格的格子与分栏的盒子就是这样一起
走的）——所以整页替换是**一次 Ctrl+Z**，落进来的行与此后手打的行完全同类（新 id、重算 order key、普通
change feed、同一条路更新 FTS）。换文件从没上过桌：它会让编辑器背地里换掉页、让 undo 栈丢掉自己那页、让库手
里多一个没人 open 的文件。标题 / icon / 封面 / 字体**故意**不滚回——版本是页的**正文**的版本，把快照之后用户
自己改的名字撤掉不是"恢复这个版本"读起来的意思。两道门都是继承来的：`locked_refusal()`（ADR-0048）与「只有
屏幕上那页」，因为命令系统只寻当前页，翻页之后残留的一行不能写进错的页里。

**删页与清扫**：`delete_page` 顺带 `forget_versions`（连同它带走的子页），所以"只被一个版本指着"的图片在同
一个动作里变成不可达并被回收，钉这条的测试先重启一次 session，免得是 undo 栈偷偷让文件活着。`sweep_orphans`
删掉索引不再点名的文件，而且**只**碰 `p<page>-<created>.db{,-wal,-shm}` 这个它自己写过的形状——一个它没写过
的名字是别人家的数据，那个目录里放一个 `readme.txt` 会活下来。剪枝先删文件再删点名它的行，反过来会留一行面
板 offering、`read` 拒绝。

**面板的字与像素出自同一批函数**：`version_rows` / `versions_note` / `version_heading` / `version_diff_note`
四个纯投影既填运行时也填 `quire-shot`——后者的场景**没有仓库**，所以 `seed_versions` 只能走投影，而这正是
想要的：面板里一个像素漂移会表现为一次哈希变化，而不是一张"抄来的旧截图"。没有数据库的 session 里版本面板是
空的而不是报错——版本是一个文件，不写文件的 session 存不下版本。

**像素读数**：sweep38 → sweep39，动了 **2 of 80**、新增 **3**（83 张）。两个动过的又是页面 ⋯ 菜单的两个锚
点，因为它长了第十四行：`menu.png` 只有 **5 个采样像素落在单列 x 414 / y 672..680**（弹窗早被
`min(rows*30+8, window-h - menu-y - 20)` 夹住，能动的只剩 ListView 的滚动条滑块），`page-lock-menu.png` 是
**575 px 在 x 240..422 / y 558..670**（那张锚得够高，十四行全画出来，新行以下每一行都位移）。三张新场景各
自对 `default.png` 自证，要紧的是它们**停在哪**：`page-versions` 7 689 px、`page-versions-diff` 8 726 px，
两者都关在 **x 410..868 / y 162..638** 这个面板自己的 460 px 盒子里，背后那页一个像素没动；
`dark-page-versions` 与所有 `dark-*` 一样重画整帧（255 505 px）。两张浅色场景彼此差的就是行点击干了什么：
五行名字与年龄变成四格 `−`/`+`，说明行变成那一版自己的名字与年龄，Save 变成 Back / Delete version / Restore。

**验证**：`cargo check --all-targets` 零警告、`cargo test --all-targets`（**438 passed / 0 failed / 13
ignored**）、`cargo build --release` 零警告、`cargo build --features software --bin quire-shot`；
`grep -rn "#\[test\]" src tests | wc -l` = **451 = 438 + 13**，与上一片记在册的 421 差 **30**，正是这一片
的新测试（lib 22：`app::state` 12 + `core::diff` 10；storage 8，其中 `version_cost` 带 `#[ignore]`）。三十
条各钉一件事，最容易漏的几条是：版本以**点击时**的页为准而不是上次 flush（`a_version_names_the_page_as_the_
click_saw_it…`）、无名版本按"第几版"被命名、**最旧的那些才是被丢掉的**、版本活得过它自己的 session、锁页拒绝
它被要求的那次恢复、只在原页恢复、删页忘掉版本、版本钉住图片、面板的行与句子读出同一批数字。

**未验证 / 已知边界**：一、真人一次都没点过整个面板——**打字命名**尤其没有，场景能画两个视图却不能往输入框里
打字，所以 focus-on-open 与"一次恢复一次撤销"的体感仍欠人手（叠在 ADR-0046 / -0047 / -0049 欠的换封面、彩色
emoji、模板对话框之上）。二、版本只含**正文**：标题、icon、封面、字体、锁定一律不滚回，这是决定不是遗漏，写
在 ADR-0091 里。三、`versions/` 不出现在应用说出口的任何一个磁盘数字里（§三十七 那行只报它刚释放的附件字节
），账写在 PERFORMANCE 而不是 UI。四、没有自动快照、没有并排视图、没有词级 diff——§三十八 没要，而第 2 点那
条"看过的版本才能恢复"已经限制了误触。五、LAN 分享不带版本（对岸没有表达它的形状，同模板那三条门）。六、
Track 3 的草案**也不需要** `versions` 表：一条数据库行若要历史，还是这两行 meta 与同一个目录。

## M13 · 引用、提及与反向链接（Track 2）— ✅ T2.1–T2.5 完成
分支：`track/2-references`（提交 `19201c3` 四刀 + 合并提交收口 T2.5，逐刀报告在
`docs/REPORT_TRACK2.md`）

- **ADR-0050**（mention/date 复用 `marks.url`）与 **ADR-0051**（反向链接 =
  两条索引 + 派生投影，取代 brief 的 FTS5 token 草案）：已落，随 `19201c3`。
- **T2.1 `@page mention`** ✅ —— chip 存目标页 id、画时问标题；slash 第四模式
  `slash-pick-mention`；`InsertReference` 一条命令一个 undo 步；改名/移父/删页
  一条测试钉死。
- **T2.2 `@date`** ✅ —— 同一枚 chip 的第二种载荷，ISO 格式只有 `core::date`
  一个定义。
- **T2.3 反向链接区** ✅ —— migration 16 两条索引；折叠 5 行 / 展开 50 行 /
  "and N more"；78 µs 折叠读（去索引反事实 6 068 µs），数字进
  `docs/PERFORMANCE.md`。
- **T2.4 页面别名与悬空** ✅ —— 改名/移父/id 落空三件事一条测试覆盖。
- **T2.5 synced block** ✅（2026-09-22 上午收口）—— **ADR-0052**：源块持有内容，
  镜像只持一根指针（`blocks.sync_ref`，migration 19；`BlockKind::Synced`，
  kind 字符串 `"synced"`，UI int 24）。删除 = 镜像退化 `(deleted source)` 只读；
  undo 不需要合并（只有一份内容）；环检测在写入时（`sync_would_cycle`，
  上界 32）；Markdown 导出按段落摊平、导入有意不认新语法；六接点全部点亮
  （slash 的第五种 picker `slash-pick-synced`、Turn into、场景
  `synced` / `synced-source-gone` 含 dark 臂）。lib 测试两条：
  画源+退化、自指/环/非 Synced 拒绝。
- **验证**：check 干净 / 按 target 全绿（lib 314+13ig、storage 39+2ig、
  markdown 69 ……）/ release 零警告；sweep 的 synced 场景数字留给整合者
  （共享工作树出不了干净对照）。
- **未验证边界**：真键盘输入、双焦点争用、真点跳转（headless 证明不了，
  见报告 §7.2）；镜像每行一次 O(1) 查表的成本没有单独数字。

## Track 3 · D0 决策与探针（2026-09-22，on `track/3-database`，ADR-0060…ADR-0065）

Database 这条 track 的**第一条纪律是先证明通道存在**：SPEC §三十九 性能红线的第一句是「10 000 行的库
不得全量 realize；视图先算可见窗口再取行」。所以 D0 不写功能，先把这句话在**选定的形状**下变成数字，
再把形状写成六个 ADR（D0 的问题 → ADR 的对应见报告 `docs/REPORT_TRACK3.md`）。

**通道**：新模块 `src/core/database.rs`（外加 `src/core/mod.rs` 一行）是一个纯投影——没有 SQL、没有
Slint、没有时钟。`window(total, ViewGeometry, scroll_y) -> RowWindow` 先算 `[start, end)`，
`RowWindow::fetch()` 就是那次取行的 `LIMIT`/`OFFSET`，而 `RealizedRows::scroll_to` 是唯一构造器、
只接受这个窗口，所以「先算窗口再取行」不是纪律而是类型；偏移先按内容夹紧（Slint 就是这么夹的），
所以一张比视口还短的表不会因为一个陈旧的偏移把顶部几行丢掉。

**数字**（10 000 行 / 32 px 行高 / 720 px 视口 / 8 行 overscan，release）：realize **31 行**（窗口
0..31；滚到中间 `scroll_y = 4000` 是 39 行；滚到底 31 行），而 100 行、1 000 行、1 000 000 行在同样
几何下算出的窗口是**同一个 0..31**——界住它的是视口不是表。内存用一个**只在测量线程计数**的全局分配器量
（`thread_local` + `const` 初始化，分配器里不再分配；别的测试线程并行跑但没 arm，互不污染）：窗口那
31 行的行对象 **6 806 B**，同一张表 10 000 行全 realize **2 259 800 B**（**332×**），只取 id 的
`Vec<u64>` 是 80 000 B；构造耗时 6.317 ms / 0.061 ms / **0.0174 ms**。进程读数（测试内手写
`K32GetProcessMemoryInfo` 声明，与 `bench.ps1` 报的两个数同源）private 2.0 → 5.1 MB、working set
11.0 → 14.0 MB：把 10 000 行真拿进内存约 **3.1 MB 私有**，窗口是 **6.8 KB**。两次运行堆数字逐字节相同。
原始行：`benchmarks/results/2026-09-22-track3-probe.jsonl`；断言在 `cargo test --lib database::`。

**它证明/没证明什么**：证明的是**投影**（窗口由视口界定、取行计划由窗口给出、行对象只存在于窗口里），
没证明**帧**——D0 没有任何 `.slint` 视图，所以没跑 `bench.ps1`（它要一个窗口才采得到样），SQL 侧的
`LIMIT` 也只在计划里、一次都没执行过（表在 D1）。

**六个 ADR**：1) database 是什么 → **ADR-0060**（自己的 `databases` 行 + 新的 `Database` 块经
`blocks.db_ref` 指过去；整页数据库就是首块是它的普通页；八个视图是八种 `db_views.layout`，「+」菜单
那六行 muted 占位由此点亮，不是八个块种类）。2) schema 存哪 → **ADR-0061**（`db_properties` 行表，
`UNIQUE(db,name)` 是行表才有的不变量；只有 select 的选项列表是行内 JSON，选项带自己的 id）。
3) 值怎么存 → **ADR-0062**（`db_values` 一行一 (record, property)，`text`/`num`/`flag` 三列 +
`db_value_items` 给 multi-select / files；判据是「比较必须发生在 SQLite 自己的类型系统里」）。
4) record 与 page → **ADR-0063**（record 拥有它的 page，`UNIQUE(page)` + `ON DELETE CASCADE`；
标题只有一个家，有页在 `pages.title`、无页在 `db_values`、读时 `COALESCE`；record 懒建页；两个删除
方向各一批 change、一次 Ctrl+Z）。5) 视图定义 → **ADR-0064**（`db_views` 行：名字/layout/顺序是列，
规则是一份 JSON 文档，过滤是递归树）。6) Markdown 通道 → **ADR-0065**（导出当前视图的 GFM 表格，
页-backed 的标题写成 `[title](quire://page/<id>)`；不写标记行；导入侧不改，管道行回来仍是段落）。

**验证**：`cargo check --all-targets` 干净（0 warning，强制重编后复测）；`cargo test --all-targets`
全绿 —— 10 个 target 合计 **394 passed / 0 failed / 13 ignored**（lib 247/10、backup 13/1、
find 9、markdown 62、persistence 5、search 17、storage 27/2、workspace 14，main 与 quire_typing 各 0）；
`cargo build --release` 零警告。视觉：这一刀**不碰 UI**（新模块没有被任何 UI 引用），
`sweep.ps1 -OutDir .scratch/track3-sweep -Baseline .scratch/sweep33` 得 **changed 0 / identical 67**。
（没有用 `-OutDir .scratch/sweep34`：开工时那个目录正被另一条 track 的 sweep 占着。）

**未验证 / 已知边界**：一、没有 `.slint` 视图，所以窗口数字是投影的不是帧的，`row_height` 32 px 与
`overscan` 8 是本刀的假设，行 payload 也是代表性的（title + 5 个文本 cell），D2 的类型化 cell 会换掉它
（窗口算术不随之变）。二、SQL 侧一次都没跑：`fetch()` 的 `LIMIT`/`OFFSET` 只是计划，`COUNT(*)` 还没有。
三、进程读数来自测试进程内部的 FFI，与 `bench.ps1` 的 `ram_private_mb` 同源但不同二进制，**不能**与
既有 jsonl 行直接相除。四、ADR-0063 的两条删除路径测试未写（D1）；侧边栏删页不进 undo 是既有缺口，
本刀只把它写在明面上，没改。五、ADR-0061/0062/0064/0065 的表一行 SQL 都还没建，`CURRENT_VERSION`
仍是 **11**，D1 的迁移号在提交那一刻取 `CURRENT_VERSION + 1`（串行接缝）。

**下一步**：D1 —— `databases` / `db_properties` / `db_views` / `db_records` / `db_values` /
`db_value_items` 的迁移、repository 读写与 core 对象模型；record 与 page 的所有权契约（含两条删除
路径的测试）在这里落地。

## Track 3 · D1 数据层（2026-09-22，on `track/3-database`，schema v12–v15，ADR-0066…ADR-0067）

D0 证明了通道（投影先算窗口），D1 让通道**真的从 SQL 里取行**，并把 SPEC §三十九 的四层对象落成表。
一刀之内交付：六张表 + 四步迁移、`core` 对象模型、`storage` 的读写与窗口查询、record↔page 的生命周期
契约与测试、重建（关掉再打开逐字段一致），以及本刀欠的性能数字。

**表与迁移**（`src/storage/migrations.rs`，动手前读到的 `CURRENT_VERSION` 是 **11**，采用 **v12–v15**）：
一次迁移 = 一个可独立回滚的语义单位，所以 v12 = `databases`、v13 = `db_properties`、v14 = `db_records` +
`db_values` + `db_value_items`（record 与它的值不可能各自存在，是一个单位）、v15 = `db_views`。
每步都用 `CREATE TABLE IF NOT EXISTS`（`add_page_columns()` 那种「缺哪列补哪列」的收敛范式在 `CREATE`
上的对应写法），并配「vN-1 库升上来读回原值」的测试：v12/V13/V14/V15 各一条，fixture 走**应用自己的
写路径**（这样 `ord` 的编码也真的是存储的编码），然后降版本、迁移、断言旧行逐字段没变 + 新表真的可用。

**对象模型与写路径**：`core::database`（D0 的投影旁边）加了 `Database` / `Property` / `PropertyKind` /
`View` / `ViewLayout` / `Record` / `CellValue` / `DatabaseCatalog` / `RowRequest`。`Change` 在**末尾追加**
十九个变体（`DatabaseCreated` … `ViewDeleted`），`storage::database_store` 放 SQL，`repository::apply_one`
的 match 保持 exhaustive（新变体是编译错误，不是悄悄丢的写入）。「空」有唯一表示：没有行——不是空串，
不是 0；`person` 折成 `text`、未知 kind 折成 `text`、未知 layout 折成 `table`（ADR-0061/0064 的 fold）。

**窗口真的执行了**：`window_rows` 拿 D0 的 `RowWindow::fetch()` 当 `LIMIT`/`OFFSET`，
`realized_rows` 把 `COUNT(*)` → `core::database::window` → 一次 SQL 串起来；SELECT 里**每个可见属性一个
`LEFT JOIN`**、标题走 ADR-0063 的 `COALESCE(pages.title, db_values.text)`，列表型（multi-select / files）
另用一次「只限本窗口 record」的查询，否则一行的三个选项会把窗口乘三。`EXPLAIN QUERY PLAN` 显示计划是
`SEARCH r USING INDEX idx_db_records_db_ord (db=?)` + 每个值一个 `sqlite_autoindex_db_values_1` 探测。

**数字**（10 000 条 record × 5 列 = 60 000 个 change，release，`benchmarks/results/2026-09-22-track3-d1-window.jsonl`）：

| 读数 | 值（三次运行） |
|------|----------------|
| 落库 10 000 行（每行 5 格） | 848 ms → **84.8 µs/行**（另两次 635.8 / 939.8 ms） |
| 窗口读（顶，`LIMIT 31 OFFSET 0`） | **463.5 µs**（另两次 247.2 / 658.7） |
| 窗口读（中间，`LIMIT 39 OFFSET 4992`） | 7.5 ms（另两次 5.1 / 8.8） |
| 窗口读（底，`LIMIT 31 OFFSET 9969`） | 12.9 ms（另两次 8.5 / 13.3） |
| 对照：一次取回全部 10 000 行 | 56.0 ms、堆 **1 842 780 B**（另两次 46.8 / 69.9 ms，堆逐字节相同） |
| 窗口那 31 行占堆 | **5 576 B**（与全表 **330×**，三次都是 330–332×） |

**这一刀量出来的真问题**：窗口界住的是**行与字节**（红线那句），不是**工作量**。裸索引走 9 969 行只要
55.3 µs，所以贵不在 `OFFSET` 的走位，而在 `LEFT JOIN` **在跳过的行上照样执行**：同一个窗口改用游标
（`(r.ord, r.id) > (?, ?)`，一行一个 key）只要 **251 µs**，比 `OFFSET` 版本的 12.9 ms 快 **51×**。
读契约（`fetch() -> (limit, offset)` 是 D0 定的）因此欠一笔：D3/D4 拿着真帧的数字把它换成游标。

**record 与 page 的契约**（ADR-0063 落地）：`UNIQUE (page)` 让所有权互相唯一、`ON DELETE CASCADE`
是 SQL 的兜底、标题只有一个家（有页在 `pages.title`，无页在 `db_values`，读时 `COALESCE`）、新 record
默认没有页（懒建页，`db_records.page` 大量 NULL 是设计而不是巧合）。两条删除路径的测试都在
`tests/integration/storage_test.rs` 的 `database_layer` 模块：删 record（一批 `[RecordDeleted,
PageDeleted]`）与删页面（只有 `PageDeleted`，靠 CASCADE）end in the same state；批量路径
（`replace_all`）不许把这一层弄丢——ADR-0066 的规则 + 测试，包括「状态里没有的页，它的 record 一起走」。

**验证**：`cargo check --all-targets` 干净（0 warning）；`cargo test --all-targets` 按 target 分开报（见
报告）；视觉 **changed 0**（纯数据层，没有 `.slint` 被碰）；`cargo build --release` 零警告。

**未验证**（诚实清单）：没有 UI 臂——没有帧、没有 `bench.ps1`、`LIMIT`/`OFFSET` 之外没有别的读路径被
任何绘制代码调用；`OFFSET` 的时间只在本机、与本刀自己的对照比过；LAN pull 没端到端跑过；改 kind 不迁移
已存的值（D2 的逐类型转换）；`config` 只被原样存取、没有解析器（D2）；ADR-0065 的 Markdown 导出仍是规格
（没有代码路径）。

**下一步**：D2 的 property 系统（14 种类型 + 每型往返 + date/number 的数值序），之后 D3 的 table view
把窗口接到真帧上，并回答上面那个游标问题。

## Track 3 · D2 属性系统（2026-09-22，on `track/3-database`，schema v17，ADR-0068…ADR-0071）

D1 让**通道**从 SQL 里取行，D2 让窗口里的每一格**有意义**：SPEC §三十九 的 14 种属性类型各自的输入
与渲染规则、两个派生时间列、以及「排序必须在 SQL 侧」这条红线。仍然**没有 UI**（没有 `.slint`，六行
插入菜单占位仍不可选，D3 才点亮）。

**属性的语义搬进 `core::database_property`（新文件）**：这是这一刀的主体。`PropertyOptions` 读/写
select / status / multi-select 的 `config`（选项带自己的 id，改名是一处文档编辑、零个值被碰）、
`NumberFormat` / `DateFormat` 从 config 读（未知设置折回默认），`parse_one` / `parse_many` 是**写入口**
（每种类型的容忍度逐条成文：数字要有限、日期的**形状**是门而日历不是、`url`/`email`/`phone` **从不改写
也从不拒绝**、空输入 = 无值、select 不接受列表里没有的名字——选项列表是同一批里的第二个 change），
`paint` 是**读出口**（选项 id 变名字、数字过格式、文件过附件名、多选拼名字）。core 里还放了**全仓库
唯一的 JSON 阅读器**（ADR-0061 的选项列表今天用，ADR-0064 的视图文档 D4 用），带深度上限——没有 serde，
ADR-0001 的「一个进程一个运行时」比省这百来行重要。两种 fold 都写明是**可见的**而不是静默的：未知的
选项 id 显示它自己，已被删的附件 id 也显示它自己。

**两个派生时间列**（ADR-0068，`db_records.created` / `.edited`，v17）：`YYYY-MM-DDTHH:MM` 本地墙钟、
定宽、`''` = 未知，**由写路径盖章**（`insert_record` / `set_cell` / 页标题改名里跑 SQLite 自己的
`strftime`），**永不写进 `db_values`**——读路径对这两种 kind 一眼都不看值表（测试写一行进去，然后看它
被忽略）。刷新时机是明写的规则：内容是格子与标题，位置（`ord`）与指向哪页是「相框」，拖动一行不算编辑。
`Record` 结构体**没有**这两个字段：`Change` 里的 record 不能给自己编一个生日。

**排序是语句里的 `ORDER BY`**（ADR-0070）：`RowRequest::sort` 是一个编译好的 `SortSpec`
（property + 列 + 方向），store 把它写成 SQL —— 全仓库**没有任何地方给 `Vec` 排序**。比哪一列是每类型的
决定：`number` 比 `db_values.num`（`2` 在 `10` 前）、日期型比定宽文本（字节序就是时间序）、`checkbox`
比 `flag`、title 走 ADR-0063 的 `COALESCE`、隐藏列自己加一个 join，而 multi-select / files / 计算列
**没有 `SortSpec`**（「按多选排」是关于选项顺序的问题，不能假装答应）。空值显式排在最后（`ORDER BY
(v1.num IS NULL) ASC, v1.num ASC, r.ord, r.id`），**升降序都在最后**，末两项是稳定的 tie-break。

**`person` 的降级**（ADR-0071）：没有成员表、没有成员 id、没有账号，值就是 `text` 列里的一个字符串
（ADR-0061 的折叠原样不动），而**本地成员名单是现算的**：`workspace_people()` 读「存储 kind 为
`person`」的列的去重非空值。改名就是改一个字符串——这正是降级省下的东西。

**数字**（10 000 条 record × 2 列，release，`benchmarks/results/2026-09-22-track3-d2-sort.jsonl`，三次）：

| 读数 | 值（三次运行） |
|------|----------------|
| SQL 排序窗口（顶，`LIMIT 31 OFFSET 0`） | **6.4 / 6.4 / 6.2 ms** |
| SQL 排序窗口（底，`LIMIT 31 OFFSET 9969`） | 13.1 / 13.2 / 13.1 ms |
| 同一个底部窗口**不排序**（D1 的读法） | 5.9 / 4.9 / 4.5 ms —— 排序给一次滚动加 **~8 ms** |
| 排序但不加窗口（全部 10 000 行） | 21.6 / 19.1 / 18.9 ms —— 顺序本身的价钱（临时 B 树） |
| 对照：取回全部再在内存里排 | 14.0 / 13.3 / 15.5 ms、堆 **1 220 000 B（1.16 MB）** |
| 排序窗口的堆 | **3 782 B**（全表的 **323×**，三次逐字节相同） |
| 一次单元格写入（自己一个事务 / 批量） | 2.8–7.4 ms / **10.6–14.9 µs** —— 差值就是 commit |
| `EXPLAIN QUERY PLAN` | 五次 `SEARCH … USING INDEX` + **`USE TEMP B-TREE FOR ORDER BY`** |

**这一刀量出来的真问题（诚实的一面）**：排序在 SQL 侧赢得**内存**（323×）与**红线**（顺序是数据库的，
不是副本的），但**时间上只赢 2×**（6.4 ms vs 14.0 ms）——因为 `db_values.num` 上没有索引，SQL 要为
10 000 行建临时 B 树；Rust 排 10 000 个 f64 只要 0.36 ms，贵的是**取回那 10 000 行**（13 ms）。
记录在案，D4 的编译器可以据此决定「隐藏列排序要不要加索引」，但窗口与排序的**形状**不能变。

**验证**：`cargo check --all-targets` 干净（0 warning）；`cargo test --all-targets` 全绿（312 + 13 + 53 +
39 + 14 …，见报告，按 target 分开）；视觉 **changed 0**（67 个既有场景逐字节相同；新出现的 10 个
`mention` / `date` / `backlinks` / `dangling` 场景是 Track 2 的，不是本刀的）；`cargo build --release`
零警告。新测试 **33 条 + 1 条打印型探针**（`core::database_property` 20、`core::database` 1、
`storage::database_store::tests` 10、集成 2）。

**未验证**（诚实清单）：没有 UI 臂——没有任何格子被画出来，`looks_valid` 的阈值没有用户量过；视图文档
还没被编译成 `SortSpec`（D4），多列排序与分组头的形状未命名；没有过滤的数字（D4）；`workspace_people()`
的开销没量；**改 kind 不迁移值**仍是 D1 的状态；选项列表的「加一个选项」还没有 `Change` 臂（跟着 D3 的
选项编辑器一起加）。

**下一步**：D3 的第一个视图（table view）——把窗口接到真帧上，点亮六行占位，回答 D1 留下的游标问题，
并把单元格编辑器接到 `parse_one` / `paint` 上。

## Track 3 · D3 table view（2026-09-22，on `track/3-database`，schema v18，ADR-0072…ADR-0075）

**缘起**：D0 证明通道（窗口有界），D1 让通道真的从 SQL 取行，D2 让窗口里的每一格有意义——D3 把
它们画出来，并把 ADR-0060 的六个接点整批点亮：块种类（`BlockKind::Database`，kind 字符串
`database`）、`blocks.db_ref`（迁移 **v18**，`add_db_ref_column`，一步一语义单位）、`/` 与「+」菜单
（`Table view` 行真 id，其余五行仍 muted）、Turn into（三个入口都走 `Command::MakeDatabase`，
`SetBlockType { kind: Database }` 被显式拒绝）、Markdown 导出（ADR-0065）、截图场景
（`database-table` / `dark-database-table`）。

**通道形状（一次说清）**：页面滚动（Slint）→ `db-viewport`（块的 body 顶相对视口的 px）→
`core::database::window`（由视口与行高算窗口）→ `database_store::window_rows`（`LIMIT`/`OFFSET`）
→ `core::database_view`（列、已绘制的格、row→y 算术）→ `ModelRc<DbRow>` → 委托只画。四个接缝各有
一条纪律：窗口只由 `core::database::window` 算；只有窗口越过 overscan 才重新取行；行模型就地替换
（不重建页面的行列表）；块高 = 表高（页面的滚动就是视图的滚动，没有第二个滚动区）。

**「行是动态的」两条规则**：projection 真的**删掉**不可见的行（`db-windows` 里的模型只装窗口，不是
`visible: false` 的全表）；模型下标与行下标差一个 `db-row-start`，由 Rust 算出、写进行数据，委托只做
`(db-row-start + index) * db-row-height` 的摆放（§三十七 对「行动态」块的两条附加规则）。

**交付物**：`src/core/database_view.rs`（视图文档读写 + 列/行投影 + 15 种 kind 的格形状）；
`ui/components/DatabaseView.slint` / `DatabaseCell.slint` / `DatabaseSwitcher.slint`；
`ui/AppWindow.slint` 的 `DatabaseColumnsPopup`（窗口级隐藏列 popup，title 列锁定）；
`EditorBlock.slint` 的 kind 23 臂（`db-height` 与 `database` 正文）；`controller.rs` 的 12 个
`UIState.db-*` callback（激活/提交/取消/勾选/选选项/加行/删行/列宽/开合列 popup/切视图/视口报告）；
`state.rs` 的 `make_database` / `db_add_record` / `db_set_cell_text` / `db_toggle_checkbox` /
`db_pick_option` / `db_delete_record` / `db_set_column_width` / `db_toggle_column` / `db_pick_view`
/ `db_watch` / `db_markdown_table` 与 `db_absorb`（ADR-0075）。

**行内编辑的覆盖（D3 的诚实边界）**：title / text / number 是行内 `TextInput`，checkbox 是整格点击，
select / status 是格内选项列表；其余九种（multi-select / date / url / email / phone / files /
created time / last edited time / formula / rollup / relation）今天**只画不编**——`editable` 为
false 的格是惰性的，日期与列表的输入控件是 D5/D6 的。

**验证**（见报告 D3 一节的完整读数）：`cargo check --all-targets` / `cargo test --all-targets` /
`cargo build --release` 三条门槛，加上提交树单独跑一遍；视觉用 sweep 对照 D1/D2 的基线
（`database-table` 与 `dark-database-table` 是新增场景，其余场景应当不动）；性能收口 SPEC §三十九 的
三个数字里 D3 能给的（切换视图耗时、行内编辑到重绘的路径成本）。

**未验证**（诚实清单，详见报告）：没有真键盘输入与鼠标的端到端测试（headless 只能证明投影）；
`absolute-position` 的窗口坐标语义只在文档层面确认（统一测试要看 popup 是否落在按钮正下方）；
10 000 行滚动时的读延迟仍受 D1 量出的 `OFFSET` + `LEFT JOIN` 影响（D4 的游标读）；
`formula` / `rollup` 列在导出里的计算值是 D6 的正确性。

## Track 3 · D4 filter / sort / group（2026-09-22，on track/3-database，ADR-0076/ADR-0077）

**一行 cargo 都没跑**（本刀的铁律：只写代码；编译、测试、视觉、性能统一留给全部代码生成完之后的总测试）。

### 缘起：把红线做成模块边界

SPEC §三十九 的红线第二条（「filter / sort 在 SQL 侧完成，不在 UI 侧过滤」）是最容易违反的一条，因为
「取回再过滤」写起来最短。本刀把它变成结构而不是纪律：`RowRequest` 带着**规则本身**
（`sorts: &[SortSpec]` + `filter: Option<&FilterNode>`），`storage::database_query` 是唯一把规则变成 SQL
的模块（`WHERE` / 多键 `ORDER BY` / 组谓词），`database_store` 是唯一执行它的模块，而
`core::database::window` 仍然先开窗——只是过滤后的窗口开在**过滤后的计数**上（`SELECT count(*)` 跑在
与行读同样的 `FROM`/`WHERE` 上）。没有任何一层拿得到行去丢。

### 交付物

* **过滤**：ADR-0064 的递归树（`and` / `or` / `not` / 子句）在 `core::database_view` 里按 schema 解析成
  `FilterNode`，在 `storage::database_query` 里编译成 `WHERE`：text 是
  `INSTR(LOWER(expr), LOWER(?)) > 0`；list 是 `db_value_items` 上的一次 `EXISTS` 探针（ADR-0062 预言过的形状）；
  `any-of` 是选项 id 的 `IN`；number 绑 `REAL`（`2` 在 `10` 前）；date 绑定宽文本（字节序即时间序）；
  `ne` 是 `NOT (eq 形)`（三值逻辑让空值两边都不匹配 =「有一个值且不是这个」）；checkbox 的「未勾选」含 `NULL`
  （没碰过的勾选框就是没勾）。**降级都在 ADR-0076 里定死**：树整棵读不开 → 丢掉 + 在视图上可见提示；
  单条子句不可读（列没了 / 比较符不适用于该 kind / 值不是该列存的形状）→ 丢那一条 + 计数提示；
  排序项与分组编不出来 → 静默丢弃（顺序与分组只改变看法，不藏行，第一帧就看得出来）；空 `{"and":[]}`
  （面板删空规则留下的状态）不是过滤、也不提示。
* **排序**：多键——文档里每个 `sorts` 项各成 `ORDER BY` 的一段，每段自带空值置后项（永远升序），
  最后统一 `r.ord, r.id` 作稳定 tie-break；列头点击循环 无→升→降→无，编辑的是第一项（决定顺序的头部）。
* **分组**：**条目投影**——组头是条目不是行。`group_window(counts, window)` 把条目窗口映射成
  「窗口内的组头 + 每组的 `(skip, len)` 切片」，每片用**组内自己的** `LIMIT`/`OFFSET` 取，所以一组装
  10 000 行也只 realize 31 行、每个组只多一个条目；条目总数 Σ(count+1) 由 `GROUP BY` 的计数算出，
  窗口照常先开。组列表只对 option-bounded 的 checkbox / select / status 开放（`COUNT(*) GROUP BY`
  是一小把行）；组头顺序按 schema 自己的选项顺序在 Rust 里排（选项表在 config JSON 里，SQL 看不见）。
* **UI**：`Filter` 按钮 + 面板（一个 popup 四个状态：规则 / 选列 / 选比较符 / 选值；根 all | any，
  子句级 ¬，值按 kind 分别是文本输入 / 勾选 / 选项列表，值在 Enter 提交）；`Group` 按钮 + picker
  （含 No group）；列头点击排序 + 箭头；组头行（同一个窗口算术与 `db-row-start` 摆放）。
  规则全部写回 `db_views.definition`——`filter` / `sorts` / `groups` 三把键归 D4，ADR-0074 的文本读改写不变。
* **场景**：`database-filter` / `dark-database-filter`（5 行里 `Points > 5` 留 3 行，走的是面板同一套写路径）。
* **没有新迁移**：规则本来就在 `db_views.definition` 这份 JSON 文档里（ADR-0064），本刀一个字节都没有加列。

### 验证（留给总测试）

三条门槛（`cargo check --all-targets` / `cargo test --all-targets` / `cargo build --release`，零警告）＋
视觉 sweep（既有 67 场景应逐字节相同，`database-filter` / `dark-database-filter` 为 new）＋ SPEC 要的
**对照数字**：10 000 行的库加一个过滤条件的窗口读耗时 vs 取回 10 000 行再在内存里过滤的耗时
（量法写在 REPORT_TRACK3 §D4 的测试计划里）。

### 未验证（诚实清单）

本刀一行 cargo 都没跑，所以编译、测试、视觉、性能全部未验证，静态自查的清单在报告里。
已知约束：共享文件只加自己的部分（controller / state / Types / AppWindow）；ADR 号 0076/0077 接在
0075 后；迁移号不动（工作树仍是 19，本刀提交树仍是 18）。

## Track 3 · D5 视图族（2026-09-22，on track/3-database，ADR-0078/ADR-0079）

SPEC §三十九「视图」的最后六种布局（chart 留 D7）一次落齐：**board / list / calendar /
gallery / timeline / form 全部交付**，每个都有虚拟化、规则持久化、切换器入口与场景臂
（`database-board` … `database-form` 及其 `dark-` 对）。核心是把 D0/D4 的「计数先算、窗口后
开」按布局各自的意思落了一遍：board 的窗口开在**卡片槽位**上（列=组列表，每列取自己的切
片）、gallery 开在**卡片行**上（一次取 `per_row × 行` 的切片）、calendar 的窗口开在**天**
里（整月一次 `GROUP BY` ≤ 31 键 + 每天至多 3 条、其余折叠计数）、timeline 的窗口开在**泳
道**上（「无日期不显示」编译成 is-not-empty 子句进语句，轴是一次 min/max）、form **不读
行**（字段表 = schema 的大小，提交才建行）。持久化零新表零新列：board 复用 `groups`，
calendar/timeline 用文档新键 `date`/`end`，gallery 的每行卡数是会话态（delegate 报告）。
切换器「+」点亮（`AddDatabaseView` 一个 change，创建即切换，chart 以名字拒绝）；board 卡片
/list 行/gallery 卡片的点击接 `db-open-record`——懒建页（ADR-0063）的 UI 触发，一批两个
change。顺手闭合 D4 的两个提交缺口：`DatabaseView.slint`（D4 的 filter/group 按钮当时未入
提交）与 `state.rs` 的 `record()` 漏斗 `db_absorb` 接线。

### 未验证（诚实清单）

本刀一行 cargo 都没跑：编译、测试、视觉、性能全部未验证（静态自查清单在
REPORT_TRACK3 §D5）。已知边界：插入菜单四行 database 占位仍 muted（ADR-0079 写明理由）；
gallery 封面是首字母占位（附件缩略图是 Track 4/D8 的地盘）；timeline 只有单日期列 +
可选 end 列，没有 Notion 的双属性吸排序；迁移号不动（工作树 19，本刀提交树仍是 18）。

## Track 3 · D6 计算属性 formula（2026-09-22，on track/3-database，ADR-0082…ADR-0084）

SPEC §三十九「需计算」三件套（formula / rollup / relation）里的**第一件落地**：`formula`
列有了引擎、编辑器、投影求值与导出通路。硬约束全部照 SPEC 原文：

* **纯词法 + 自写解释器**（不引入 JS / WASM 运行时、不引公式解析库、零新依赖）：词法器 +
  递归下降解析器 + 树遍历解释器都在 `src/core/database_formula.rs`，类型四类
  （number / text / boolean / date）+ Empty（传染，`text(x)` 是唯一显式转换），函数七个
  （`if length round abs min max text`），外加算术四则、比较、`and/or/not`、`[Column]`
  本行引用。
* **有限求值**：四个常量钉死（tokens 2 048 / depth 32 / steps 10 000 / result 65 536），
  文法无循环无自定义函数，引擎无时钟无 I/O（`today()` 刻意缺席）。
* **表达式存 `db_properties.config`（ADR-0061 的一列一文档），值不入库**——投影时现算
  （ADR-0062/0039 的纪律）；写入经新 change `PropertyConfigSet`（整文档替换）+ 新命令
  `SetDatabaseFormula`，一次编辑一步 undo，ADR-0074 的读改写纪律用于列。
* **可增量重算**（ADR-0083）：求值只在投影窗口（六布局共用的 `db_table_rows` →
  `db_paint_formulas`），**没有全表求值路径**；排序/过滤对公式列拒绝（要比较就得先算全列）；
  依赖是本行的（引擎回调无 record 参数——跨行是 rollup/relation 的事，ADR-0084）；
  `db_formula_evals` 计数器把契约变成可量的两个数（窗口有界、依赖精确）。
* **环检测在保存时做**（SPEC 原文）：`would_cycle` 在 `db_formula_accept` 里跑，会自指的
  表达式当场被拒并说明；渲染时只有深度上限兜旧文档（画 `Error` 不挂）。
* UI：公式编辑器（多行输入 + 每键现算预览 + 错误行，单元格点击打开；Columns popup 有
  「+ New formula column」一行创建入口）；公式列单元格只读展示（`editable=false` 不变，
  点击开编辑器）。场景 `database-formula` / `dark-database-formula`，种子走真写路径
  （`db_add_column` → `db_formula_accept` 含保存时检查）。

**rollup / relation 本轮未做，原因是 Track 2 的引用基础设施未落地**（工作树的
`src/core/reference.rs` / `src/storage/backlinks.rs` 是未跟踪文件，mention 的 `marks` 载荷
与迁移 16 也都没提交）：brief 明说 relation 用 §四十 的基础设施、不另造轮子，所以这两件
**留一刀等 Track 2**，形态（relation 存 id 不存标题、双向一批写、保存时环检测；rollup 对
relation 目标的六个聚合、配置存 config、值投影时现算）已在 ADR-0084 写死。

### 未验证（诚实清单）

本刀**一行 cargo 都没跑**（铁律）：编译、测试、视觉、性能全部未验证——静态自查做了括号
平衡（词法器处理 lifetime 后全部归零）、回调三件套 grep 核对、`Change` 全部 match 点的臂。
已知边界（全在 REPORT_TRACK3 §D6）：公式不能引用 list/pick 列的值（id 对算术无意义，
读作 Empty）、没有日期运算函数、没有跨刷新值缓存（理由见 ADR-0083）、导出按行现算整个
视图（显式产物的成本，不是输入红线）。迁移号不动（工作树 19 / 提交树 18，本轮零迁移）。

## Track 3 · D7 高级特性（2026-09-22，on track/3-database，ADR-0085…0087 借号）

D0 通道、D1 存储、D2 属性、D3 table、D4 规则、D5 视图族、D6 formula 之后，D7 收掉 SPEC §三十九
「视图」「操作」剩下的四件：**chart（第八种视图）**、**linked database**、**数据库模板**、
**视图内搜索**。铁律照旧：**只写代码，一行 cargo 都没跑**——编译、测试、视觉、性能全部留给
总测试（D8）。

### 缘起与形状

* **chart**（ADR-0078 的窗口单位契约落地，不另出 ADR）：plot 画的是**聚合不是行**——一次
  `GROUP BY`（复用 D4 的 `group_counts`，与 board 的列是同一查询）给出 (键, 计数)，10 000 行
  realize **0** 行；bar / line / pie 三种全用现有 primitive（bar 是等宽 Rectangle、line 是
  viewbox 缩放多段线、pie 是 Rust 端 κ 近似三次曲线的逐片 Path），**零图表库零新依赖**；形状
  存视图文档的 `chart` 键（ADR-0074），切换器「+」第八行点亮、`db_add_view` 的拒绝撤下。
* **linked database**（ADR-0085）：同一个 Database 块 + 同一根 `db_ref`——不是新 kind、不是
  第二列、零迁移零新 Change；`Command::LinkDatabase` 一批落地，读写全部经既有 `db_ref` 解析
  落源库，源死走 ADR-0060 既有的 `(deleted database)`；入口 = slash/插入菜单 `Linked view` 行
  （`LINKED_VIEW_ROW = -2`）→ 数据库 picker → `db_make_linked`。
* **数据库模板**（ADR-0086）：`databases.template` 一列 JSON（**v20**），值是 `CellValue`
  存储形状的原样副本（「不引入第二套内容格式」）；行槽 T 存模板、`db_add_record` /
  `db_form_submit` 在**建行同批**预填；与 Track 1 页面模板**没有需要仲裁的共享形状**（一边是
  块序列的副本、一边是格值的副本，类型/列/函数零共享，报告已说明）。
* **视图内搜索**（ADR-0087）：选**数据库自己的 SQL 谓词**（`INSTR(LOWER,LOWER)` 的 OR，编译进
  同一 `WHERE`），不挂 §二十 的 FTS5——`db_values` 不在镜像里，挂进去 = 每格写入一条维护路径
  + 每条批量路径一条清理规则 + 索引滞后边界；`INSTR` 全扫（D4 contains 同价）但零副本零滞后；
  needle 是会话态，导出 `search: None`。

### 验证

未验证（诚实清单）：**全部**。一行 cargo 都没跑（铁律），静态自查做了括号平衡（剥注释/字符串
的检查器，全部文件与 HEAD delta 归零）、回调三件套 grep（5 个新回调 + 1 个会话 flag：声明 /
使用 / 绑定逐个核对）、新 `Change` 变体的全部 match 点（document / settings_store /
attachment_ids_in 的 `_` 兜底 + repository 穷尽 match 已加臂）、`RowRequest` 全部 9 处字面量
补 `search`、`Database` 字面量仅 store 的 load 一处（已带 template 列）。四个新场景
（database-chart / -search / -linked / -template + 各自 dark-）从未渲染。性能：INSTR 全扫的
每键成本、pie 几何的构造成本均未量——量法记在 REPORT_TRACK3 §D7 的测试计划。

### 迁移号（串行接缝）

动手前读 `src/storage/migrations.rs`：工作树 `CURRENT_VERSION = 19`（T4 的 sync_ref 未提交）。
本刀取 **v20**（`databases.template`，「缺哪列补哪列」的收敛范式）。**提交 blob 里
`CURRENT_VERSION = 20`、数组缺 19**（HEAD = 18 + 本刀 v20）——runner 按序应用「version > 文件
当前版本」的步，跳号安全（v17 落地已证明）；合并后 12…20 连续。

### 决策号（借号，请整合者确认）

号段 0060…0079 早已用尽，D6 借了 0082–0084，本刀**继续借 0085 / 0086 / 0087**（linked
database / 模板 / 视图内搜索）。工作树里 Track 4 的 0080/0081、Track 2 的 0050–0052 各归其主；
若整合时要重编号，请以内容为准搬迁（正文交叉引用按内容书写）。

## Track 3 · D8 性能收口（2026-09-22，on `track/3-database`，无新 ADR：测量刀，不新增机制）

**这一刀只做两件事：把合并后的树跑到门槛上，再把 SPEC §三十九 欠的三个数字量出来。**

1. **收口债务**。D6/D7 的约 2 300 行「一行 cargo 都没跑」，在 `fa467a0` 合并树上有 13 条编译
   warning。逐个清掉，全部落在本 track 的 territory：`controller.rs` 六处只 clone/upgrade 却不用的
   `gw`/`g`/`s`/`block`（行内回调里 Slint 镜像已接管状态，那几行是纯守卫、删之无副作用）、
   `state.rs` 三个写了从不读的 `db_formula_block/property/record` Cell（公式弹窗的 id 实际存在
   UIState 镜像里，这三个 AppState 字段是 D6 设计改道后的残骸，连同它的说明注释一起删）、
   从不被调的 `db_definition`/`db_columns` 两个私有方法、`database_formula::Parser::end_at`（且实现
   是坏的：取的是最后一个 token 的位置不是「输入末尾」）、`db_refresh` 里 `body/total/wanted` 三个
   从不被读的初值（改成无初值的定值赋值，顺带 `mut` 也不 needed 了）、以及 `command.rs` 一处 Track 1
   遗留的未用 `PageFont` import。清完 `cargo check --all-targets` **0 warning**、
   `cargo test --all-targets` **476 passed / 0 failed**（与本 track 合并时同数，删的都是死码）。
2. **三个数字**（详见 `docs/PERFORMANCE.md` 的 `## M14`）：① 10 000 行的库开在窗口上只占**几 KB**
   行对象，全量 realize 才 1.16–2.26 MB（≈330×，counting allocator，逐次相同）；② 切到视图顶部
   ≈ **0.35–1.0 ms**（解码 + `COUNT(*)` + 一次窗口读），但 `OFFSET` 走到表尾 16–25 ms（cursor 实测
   212–356 µs 是退役它的现成路子），分组 `GROUP BY` 再 +5–18 ms；③ 打开公式编辑器 = 解析式子
   **1.6–3.4 µs**、单格求值 68–129 ns，全表重算是窗口重算的 **108×** 且投影无路径去做（ADR-0083）。
   新探针两个：`storage::database_store::probe::a_view_switch_...` 与
   `core::database_formula::perf::a_formula_...`（都 `#[ignore]` 打印型，复用 D0 的窗口几何与 D1 的
   建库夹具）。原始行 `benchmarks/results/2026-09-22-track3-d8.jsonl`。

**明确未做（诚实）**：**帧没测**——Slint 重画 31 个 delegate / 日历 42 格 / chart 构 path 的墙钟，
需要真窗口 + `bench.ps1` 的 RAM/像素臂，headless 探针替不了（与 D0 §2.3 同一条界限，SPEC 那句
「没有 UI 臂就进不了 §Method」正是说它）。要在真窗口上补这三个墙钟，得人手开一次 GUI 采一轮；
本环境不脚本点桌面 UI，故未做，写在此处不含糊过去。

**rollup / relation 仍欠**（ADR-0084）：Track 2 的引用层已随合并落地，relation 的依赖不再阻塞，
但它是一整刀新语义（存 id 不存标题、双向一批写、保存时环检测、rollup 六种聚合），形态已写死在
ADR-0084，不属 D8 的「测量与收口」范围。

## Track 3 · D9 relation + rollup 落地 —— 而窗口缓存补上它缺的那一半（2026-09-22，on `m14-database`，ADR-0088…0090）

D8 收尾时留着「rollup / relation 仍欠（ADR-0084）」。这一刀把它欠完，做法与代价如下。

**缘起**：ADR-0084 把两者的形状写死了（存 id 不存标题、双向一批写、保存时环检测、rollup 六种
聚合），D8 报告说它「是一整刀新语义，不属 D8 的测量范围」——这一刀就是那一整刀。

**存成什么形状**：

1. **relation = 一串目标 `RecordId`，存在既有 `db_value_items` 里**（ADR-0088）。这是 ADR-0062
   列表机制的第三个用户（多选、文件之后），所以 **`CURRENT_VERSION` 不动、零迁移**。与
   ADR-0084 字面有一处有意偏离：它写「指向 §四十 的引用层」，而 §四十 的引用层地址是
   `PageId`，ADR-0063 的 record 在没人打开前**没有页**——存 page id 会给大多数行造页，正是
   ADR-0063 刻意避免的。存 `RecordId`、投影时用既有的 `record_title`（ADR-0063 的 `COALESCE`）
   取活标题，**纪律照抄、管道换掉**，理由写在 ADR-0088 里。
2. **双向 = 一次写**。`Command::SetRelation` 同时携带正向格与全部反向格，`plan` 编成**一个
   `Entry`** —— 一次 Ctrl+Z 把一对关系整个回退（ADR-0084 的「一批」照字面做到）。
3. **mirror 是对合，不是一个要走的图**（ADR-0088）。写只碰直接镜像、绝不下行，所以终止性由构造
   保证——**前提是配对合法**，合法 = `mirror(mirror(a)) == a` 且 `mirror(a) != a`。四种拒绝：
   自己是自己的镜像 / 对方不是 relation 列 / 对方不指向本库 / 对方已经是别人的另一半。
   **环不可表示**，比「事后检测环」强：读路径不需要为环准备深度上限。
   *实现中改了一处*：原本还要求对方**已经**声明本库为目标，可这次配对本身就是写它的那次写，
   于是「一次手势配双向」在实现上不可能；改成「指向本库**或尚未指向任何库**」，环仍然不可
   表示（映射仍是大小 ≤ 2 的轨道）。
4. **rollup = 配置 + 投影时折叠**（ADR-0089）：`{"relation": …, "column": …, "aggregate": …}`，
   六个词里 `none`/`count` 不读列。`sum`/`average` 走 `database_formula::arith`、
   `min`/`max` 走 `extreme`（为此把这两个函数与三个枚举开到 `pub(crate)`），所以一个 rollup 和
   一个公式不可能对「两个日期的最小值」给出两种答案；多出来的只有**严格性**——sum/avg 要求数字，
   因为「文本列求和」是配置错误，画一个 `Error` 比悄悄粘出一个字符串好。**值不落盘、跨刷新不
   缓存**（ADR-0083 整条继承），配置检查拒绝「聚合另一个计算列」——这是唯一能闭环的形状，所以
   **一个 rollup 依赖另一个 rollup 不可表示**（连跨库的那种也拒）。
5. **窗口缓存补上「内容」那一半**（ADR-0090，本刀撞出来的**既有缺陷**）：`unchanged` 的键是
   `(view, definition, layout, stamp, total, window)`，全是形状——一次同窗口的 cell 写入、
   一次公式配置修改、一次撤销都不会重读窗口。修法是 `db_content_stamp`（`record` 这个唯一漏斗
   里 `+1`，故意钝、不做枚举）+ 写者把机会交给**同页其他数据库块**（`db_refresh_page`，上界是
   当前页的块数）。D3 的 sweep 是静帧，对不出这个 bug；M14 整个卖点就是计算出来的像素，所以
   它是第一个能证明它的特性。

**接缝**：`core/types.rs` 没碰；`storage/migrations.rs` 没碰；依赖/feature/profile 没碰；
`.slint` **一个字都没改**（所以没跑 sweep，也不声称像素逐字节相同）。新增 `core/database_relation.rs`
与 `core/database_rollup.rs`；`command.rs` 三条命令；`database_store.rs` 四个批量读
（`record_titles`/`cell_items`/`values_of`/`records_named`，全部按**窗口的行集**取键，没有一条
按目标表的大小）；`state.rs` 四个 entry point、两个投影 pass、缓存键与撤回刷新。

**验证**：`cargo check --all-targets` 0 warning；`cargo test --all-targets` **549 passed / 0 failed
/ 19 ignored**（lib 371，比 D8 的 359 多 12 条）；`cargo build --release` 零警告；三个
`#[ignore]` 打印探针在 release 下出数（原始行 `benchmarks/results/2026-09-22-m14-relation-rollup.jsonl`）。

**数字**（全文 `docs/PERFORMANCE.md` `## M14`（D9 节））：picker 在 10 000 行里取 20 = **59.7 µs**
（与表大小无关）；一次 pick 200 个目标端到端 = **13.0 ms**（代价是 fan-out，比读整表 22 ms 便宜
~2×）；一个窗口行的 rollup 在 **fan-out = 10 000** 时 = **28.5 ms**，**比「把 10 000 个值读出来
再折」的对照（9.1 ms）慢 3.12×**——窗口同时为活标题与值各付一次大 `IN (…)`，所以上界诚实地写成
**fan-out 而不是表大小**，而 fan-out 今天**没有上限**（picker 的 `limit` 是调用者的选择，这是
留给产品的一个决定）；缓存刷新（什么都没变）= **29 µs**，也就是说「不变窗口里的滚动仍然免费」
这条 D8 数字仍然成立；peer pass 在一库页 / 两库页 = **20.4 / 29.4 µs**，D8 那三个数字的场景是
「一库一页」，所以不受影响。

**未验证（诚实）**：① **UI 一个字都没写**——没有列类型选单（D5 的欠账）、没有记录选择弹窗、
没有 rollup 配置编辑器，所以**状态层完整且被测过，用户视角特性仍不可达**，ADR 里「picker 的
点击、chip 的像素」未验证的原因正是「没有可点的东西」；② 六个聚合的像素、多目标 relation 格的
截断、chip 的样子全没看（帧没测这条界限与 D8 相同）；③ fan-out 无上限（见上）；④ 「一页挂三个
10 000 行的库」的 peer pass 只有形状上的界、没有数字；⑤ 本刀没跑 sweep（没碰渲染面，但没有基线
对照就不声称逐字节相同）。

**顺带修掉一处 ADR 的不实**：ADR-0089 写着读路径有 `ROLLUP_MAX_DEPTH` 上限，实际代码的深度上限
是 **0**（`values_of` 对计算列回空 map，空 map 折叠成 `Empty`），常量从未存在。改成如实描述，
而不是为了对齐 ADR 去加一个常量。

## Track 3 · D10 那三个入口落地，而像素在四处说了「不」（2026-09-23，on `m14-database`，ADR-0092）

D9 的诚实清单第一条是「UI 一个字都没写」：状态层齐了、测试绿了，用户够不着。这一刀把 D5 留下
的那笔欠账清掉，并且**第一次用像素而不是断言来验收这一 track**——`cargo build --features software
--bin quire-shot` + `benchmarks/scripts/sweep.ps1`，16 亮 + 16 暗 database 场景进基线。

**接的三个入口**：① 列类型菜单是 Columns 弹层的第二个面板（`db-columns-panel == 1`），行由 Rust
推成 `DbPickRow`——把「不能这么改」的拒绝写在**灰掉的行**上，而不是弹一条事后的 notice；② relation
编辑器 `DatabaseRelationPopup`，一窗三态（target 库 / back-pointer / 带 needle 的 record picker），
未配置的列从 1 开、已配置的从 0 开；③ rollup 配置器 `DatabaseRollupPopup`，一句三段话，每段开自己
的 `DatabasePickList`，chooser 逐行走 `check_config` ⇒ 不接受的列在选择器里就是灰的。三者共用新组件
`ui/components/DatabasePickList.slint`（一套列表形状回答三个问题）。

**为此补的状态层**（不是 UI 的附属，是 UI 一接上就暴露的洞）：
`Command::SetDatabasePropertyKind`（ADR-0062 的 `PropertyKindSet`，值不转换；改到计算列给「得先定
义」的提示；点同一个 kind 零 change，于是一个会重画的菜单不烧 undo 步）；`FilterOp::ops_for(kind)`
把算子按类型收敛；`build_clause` 认 `FilterValue::Missing`（ ⇄ `Json::Null`）为「规则在、值没填」
⇒ 不加约束——这一条在 D4 起就是错的，只是没人能在没有算子选择器的时候把它点出来；`layout_total`
认 `request.search`，`DbWindow` 把 needle 算进缓存键（换搜索词不重画，是 D7 搜索自带的洞）。
这条修复有价：10 000 行的库上 header 的计数从 `record_count` 的 **~0.18 ms** 换成带 needle 的
`filtered_count` **3.4–3.7 ms**（每次刷新一条语句，`LIKE '%…%'` 用不上索引），而且**needle 越窄窗口
读得越贵**（1 命中 8.3 ms vs 1 111 命中 3.6 ms——读按 `ord` 走到窗满为止，夹具里那一命中恰好是最后
一行；D8 的「OFFSET 走完它跳过的行」从 filter 这一侧再现）。数字与新探针见 `docs/PERFORMANCE.md`
的 D10 一节。

**四个测出来的缺陷**（详见 `docs/REPORT_TRACK3.md` §D10 与 ADR-0092）：`✓` `✕` `⋯` 三个码位不在
Segoe UI 的字体脸里（fontTools 数 cmap：3 996 个码位，三个全无；`segoeui` 有 `‹ … ≤ √ ≠`），
所以「勾选」在两个弹层里画的是**零像素**——改成 `Icon` 的 path 后，A/B 渲染只有 kinds+relation 的
哈希移动、relation 图 90 px 变化。另两处是上面的 filter/search 洞，第四处是**我自己的两个 seed
bug**（把 block id 当 `DatabaseId` 传；rollup 折叠了近侧列），都是读图读出来的，不是读代码。

**门槛**（收尾那两处改动之后全部重跑）：`cargo check --all-targets` **0 warning**；
`cargo test --all-targets --no-fail-fast` **565 passed / 0 failed / 23 ignored**（+18 新测试，
+1 新 `#[ignore]` 探针；中途曾记 564/1，唯一失败是读真实剪贴板的
`platform::tests::clipboard_..._unicode`，本机剪贴板被占，重跑即绿——数字以重跑为准）；
`cargo build --release` **0 warning**（收尾重跑 9 m 34 s，与同批 test 抢 CPU，故不比首轮 4 m 42 s 的长短；
比的是两轮都零警告）。
sweep：d11→d12 99 同 / 32 新（新场景），d12→d13 4 变（预期：三处字形 + lock 文案）/ 127 同，
d13→d14 **0 变 / 131 同**（复位只动回调，而场景驱动器不经过回调 —— 逐字节不变是这一处可证的预言）。

**未验证（诚实）**：① **真窗口手测仍欠**——本环境禁止脚本点桌面 UI，所以 Escape、点击外部、
开屏焦点、tick 的重画只有 headless 场景的证据，没有真窗口鼠标键盘的证据；② 分组弹层（
`DatabaseGroupPopup`）没有自己的场景，它的两处字形是按规则改的、不是按像素改的；
③ 收尾时又发现一处同类洞：从类型面板里关闭 Columns 弹层会把 `db-columns-panel` 留在 1，下次打开
直接落在过期的类型菜单上——已修（`on_db_columns_closed` 里复位 panel/property/title），但**它没有
测试**，因为四个 `*_closed` 回调（columns / formula / relation / rollup）都要真 UI 句柄，而 headless
场景驱动器摆状态、不经过它们；④ relation 的**双向扇出无上限**
（D9 的 28.5 ms / 3.12× 数字仍然成立），是产品决策不是 bug，等用户定；⑤ `core/math.rs` 的符号表
里还有约 70 个码位不在 Segoe UI 脸上（ADR-0092 明写它**不在**本刀这条规则内，因为那是另一条
territory 与另一刀的事）——现有的 `math` / `math-inline` 两张场景恰好只用查过的 `≤ √ ≠ ∫`
与上标数字，所以没看见洞，那是**未答的问题**不是已验的事实。

## 拆分 · 数据层成了自己的 crate（2026-09-23，on `split/quire-data`，ADR-0093）

**缘起**：用户要开始做安卓版本，并打算往数据层里加 Rust 同步模块。所以先把「模型 + 存储 +
不需要窗口的服务」从 Slint 壳里剥出来，剥成 cargo workspace 的两枚 package。四条决定是用户
当轮点定的：**先在本仓拆 workspace、验证边界之后再物理搬**；范围**含 services**；新仓**用
`git filter-repo` 带走历史**；壳仓**以锁 rev 的 git 依赖**消费它。本刀只做前两条。

**形状**：`crates/data/`（31 455 行 src + 6 009 行测试）= `core/` + `storage/` +
`services/` + `testing.rs`，外部依赖只有 `image` 与 `rusqlite`，两者都钉在
`[workspace.dependencies]` 一处（防两枚 crate 给出不同的 feature 而把 SQLite 编两遍）。根包
`quire`（26 972 行 + `ui/` 11 910 行 + `build.rs`）留下壳。规则一句话：`crates/data/src` 里
不许出现 Slint、文件对话框、剪贴板或平台 API。

**动手前先量的**：core/storage/services 对 `crate::app|services|platform` 的引用 **0 处**，
`slint::` 只出现在 `app/`、`bin/`、`main.rs` 和 `lib.rs` 那一行 `include_modules!`。边界本来
就在，只是从没被单独编译过。

**959 处跨层引用一处没改**：`src/lib.rs` 加 `pub use quire_data::{core, services, storage,
testing};`，于是壳里的 `crate::core::…` 与集成测试里的 `quire::services::…` 原样解析。省掉那
950 行 diff 是有代价的取舍，理由是**这么大的 diff 会埋掉唯一值得发现的东西**——而编译器确实抓到
了它：`services::search_service::snap_to_words` 是 `pub(crate)`，被 `app/workspace.rs` 调用，
那只在一枚 crate 时合法。改成 `pub`，doc 注释写明原因。这是本刀**唯一**一处代码修复。只有搬走
的 5 个测试文件被 sed 过（59 处 `quire::` → `quire_data::`）。

**门槛（全在 `0089008` 之上、拆分后的树上跑）**：

| 关 | 命令 | 结果 |
|---|---|---|
| 类型 | `cargo check --workspace --all-targets` | Finished，0 warning |
| 测试 | `cargo test --workspace --all-targets --no-fail-fast` | **565 passed / 0 failed / 23 ignored**，与 D10 门槛逐字相同；壳侧 134、数据侧 431 |
| 名单 | `comm` 双向比对 D10 的 `gate-test-d15.txt` | 589 个测试名，**双向 0 差**——不是「数字对得上」，是「同一批测试」 |
| release | `cargo build --workspace --release` | Finished in **5m31s、0 warning**（`target/release/quire.exe` 27 846 656 B）；这是最后一处 doc 注释改动之后重跑的那一轮，它之前一轮 6m36s——长短差在缓存不在代码，能比的只有两轮都 0 warning |
| 像素 | `sweep.ps1 -OutDir .scratch/split-after -Baseline .scratch/split-control` | **0 changed / 131 identical**；控制臂在任何文件移动之前建并扫（exe md5 `63f021af…`），后臂 `1851e577…` |

**本刀买到最重要的一条不是编译，是门禁命令本身**：`cargo test` 在工作区根**只测根包**。第一次
跑报的是 134 passed / 0 failed / **exit 0**，把 `quire-data` 的 431 个测试静默跳过了——这正是
「静默探针 ≠ 阴性」的形状。`just check` 三行都补了 `--workspace`，注释写明不加会怎样。
（这一段初稿写的测试数是 389，那是**用眼睛加** per-target 行、漏了 `storage_test` 的 42 个。
431 是同一棵树在剥出来的独立仓里自己跑出来的数，`awk` 加的，不是心算的。）

**同日的物理剥离**（`.scratch/wt/quire-data-extract/repo`，一次性 clone，主树未动一根毛）：
`git filter-repo --path src/core --path src/storage --path src/services --path src/testing.rs
--path <5 个测试>` → **172 枚里留下 69 枚**，从 `feat(core): persistence contract — model
types + Repository trait (ADR-0012)` 一路到 D10，路径全部保持原样（数据层一辈子住在
`src/core` 那些位置，所以没有一枚 commit 被改名打断，`git blame` 仍然逐行成立）。之后两枚本地
commit 把边界补上：`Cargo.toml`（依赖只有 `image` + `rusqlite`，外加一枚**空的** `[workspace]`
表——这棵树目前物理上嵌在本仓目录里，不加它 cargo 会往上找到父 workspace 直接拒绝编译）、
`src/lib.rs`、`tests/` 上移一层、`.gitignore` / `.gitattributes` / `README.md`。
**它自己跑 `cargo test --all-targets` = 431 passed / 0 failed / 13 ignored，0 warning**；与
本仓 `crates/data` 的内容逐文件比对，差异只有换行符（5 个文件的 CRLF 是本仓 checkout 的老毛病，
`tr -d '\r'` 之后全等）。**没有远端**，所以壳仓仍然走 `path = "crates/data"`，改成锁 rev 的
git 依赖要等仓名和推送落地。

**未验证（诚实）**：① `cargo check -p quire-data --target aarch64-linux-android` **没跑过**。
ADR-0093 说的是「这条边界是安卓的前提」，不等于「已经能为安卓编出来」；`rusqlite` bundled 要
NDK 的 clang，M9 那次实测是在拆分前的树上；② `benchmarks/` 与 `install/` 下的 ps1 只在新布局上
验过 `sweep.ps1` 一支，`dist.ps1`、`verify-installer.ps1`、`verify-portable.ps1` 没跑过——而
master 本来就没在当前 head 上跑过它们（ROADMAP:108 自己记着），这条债本刀没还也没弄坏；③
`data_location::roaming_root()` 仍读 `%APPDATA%`，安卓侧要自己注入路径，本刀只是把那个接缝
**量出来**了（同文件的 `app_data(&Path)` 已经是收参数的可测那半），没改调用方；④ `crate::core`
这类拼写靠 4 行 re-export 撑着，边界目前是「编译强制 + 注释维持的命名习惯」的混合体，物理搬仓
之后要删掉 re-export 才算真强制；⑤ **剥离本身做完了，交付没有**：那枚仓目前只是
`.scratch/wt/` 底下的一棵树，**没有远端、没有推**，仓名待定；壳仓改成锁 rev 的 git 依赖之后
`just check` 是否仍全绿**没跑过**（现在仍是 `path = "crates/data"`）；⑥ 新仓的
`Cargo.toml` 里那枚空 `[workspace]` 是为「它暂时无处可去」写的，搬出本仓目录之后它变成多余的
一行——留着无害，但要认得它为什么在那儿。

## 剥离落地 · 那枚仓有了远端，本仓改成锁 rev（2026-09-23，ADR-0094，on `split/quire-data`）

仓名由用户定：`github.com/PT123123/quire-core`，推走 SSH。上一节那两枚本地 commit 因此重做
（`build(core): …`），全树 `quire-data`→`quire-core`、`quire_data`→`quire_core`，改完 tracked
文件里旧拼写 0 处。

**推送前扫了 identity，扫出泄露**：71 枚里 **46 枚** 的 author + committer 都是
一个个人 QQ 邮箱（地址不写在这里——它已经在 137 枚 commit 的元数据里，本仓是公开的，没必要再多
一份可搜索的副本）。`filter-repo --mailmap` 一遍洗成 GitHub noreply（名字不变、只有 SHA 动），
本地确认 71/71 之后**再回读远端**（`gh api …/commits --paginate`）复算一遍，不信任本地日志。
本仓 174 枚里 **137 枚** 带同一个地址，而 `PT123123/quire` 是 PUBLIC——泄露是既成事实。
**用户点了头，所以本仓整条历史也过了同一遍 `--mailmap`**，连 tag `v0.1.0-rc1` 的 tagger 一起洗，
`--force-with-lease` 重推。同一趟带上 `--replace-text`：147 个 tracked 文件里有 **22 个**
`benchmarks/results/*.jsonl` 把 `exe` / `db` 记成了带本机用户名的绝对路径（我最初只数出 2 行，
因为正则只覆盖了正斜杠那种写法，背斜杠转义的那 20 个文件全漏了——数完才改口），另有一枚旧版本的
报告在正文里直接写了那个用户名。**control**：拿同一份替换表重跑那 22 个原始 blob，22/22 复现出
重写后的哈希——行内的差只有路径，没有一个测量数字动过；其余 125 个文件逐字节不变。
**第一趟重写是静默空转的**：`--replace-text` 的分隔符是 `==>` 不是 `=>`，整行被当成一个永远匹配
不上的 pattern，它 exit 0 且报「New history written」，是「重写前后 tree 的 diff 为空」这条
control 把它揪出来的。代价是**本仓所有 SHA 都换了**，本文与 ADR 里引用的每一枚号（`09ef5aa`、
`0089008`、`3ae8124`…）都是重写前的旧值，按 commit 标题去找。**`quire-core` 那 71 枚不受影响**
（另一个仓），所以依赖里锁的 `f3044e2` 照旧有效。

**内容零漂移的证据（这条是本刀最硬的 control）**：拿本仓仍留着的 `09ef5aa:crates/data/` 那些
blob 逐文件比，45 个源文件里 **38 个去 CRLF 后逐字节相等**，另 **7 个**（6 个 doc 注释里带
`-p quire-data` 的 + `lib.rs`）把 `quire-core`→`quire-data` 反 sed 回去后哈希也相等。
也就是说整个剥离 + 改名 + 历史重写，落地的唯一内容差就是那个 crate 的名字。

| 门 | 命令 | 结果 |
|---|---|---|
| 新仓自证 | `cargo test --all-targets`（在 quire-core） | 431 passed / 0 failed / 13 ignored，0 warning |
| check | `cargo check --workspace --all-targets` | exit 0，0 warning，47.5 s |
| test（本仓） | `cargo test --workspace --all-targets` | **134 passed / 0 failed / 10 ignored**，0 warning |
| release | `cargo build --workspace --release` | Finished in **6m 49s**，0 warning，`quire.exe` 27 847 168 B |
| 像素 | `sweep.ps1 -OutDir .scratch/swap-core -Baseline .scratch/split-after` | **changed 0 / identical 131** |

**本刀买到最重要的一条：`--workspace` 从「承重」降级成「装饰」，而且是结构性的。**
上一刀的结论是「workspace root 上裸 `cargo test` 只测 root package，所以必须加 `--workspace`」；
现在 `quire-core` 是 **dependency**，不是 member——**没有任何一个 flag 能让本仓跑到它那 431 个测试**。
本仓的绿色从此按定义只是壳的绿色（134），`just check` 的注释已经改口这么写。谁以后引用这个数，
必须同时说清是哪个仓。

**第二学到：cargo 自带的 libgit2 拉不动一个公开仓。** 依赖一挂上，`cargo check` 就死在
`no authentication methods succeeded`，而同一条 HTTPS URL `git ls-remote` 干净返回 ref——
libgit2 主动递了凭证，服务器回 401，报错长得像权限问题，其实是客户端多事。
解法是 `.cargo/config.toml` 里 `net.git-fetch-with-cli = true`（本仓第一次有这文件）。
**读走 HTTPS、写走 SSH**：fetch 要在没有 SSH key 的机器上也能成，push 才需要身份。

`Cargo.lock` 的改动只有 **3 入 2 出**：`quire-data` 那枚换名并多出 `source = "git+…#f3044e22"`
一行，`image` / `rusqlite` 的解析版本一个没动（本仓和那仓要的版本与 feature 本来就一致）。

**没动的一条**：`src/lib.rs` 那 4 行 re-export 仍然撑着 `crate::core::…` 的 959 处拼写——
上一节列的④「物理搬仓之后要删掉 re-export 才算真强制」这条债**本刀没还**，跨仓之后它反而更明显了。

**重写落地的对照表**（`origin/master` 新尖 = `f475026`；本文和 ADR 里被引用的旧号一律按这张表找）：

| 旧 | 新 | 那枚 commit |
|---|---|---|
| `0089008` | `3afd5e0` | ADR-0093 像素门的控制臂 |
| `09ef5aa` | `531634a` | 两 crate 的那一刀（blob 比对的基准就在这枚的树里） |
| `3ae8124` | `ac4da23` | 431 不是 389 的那次自纠 |
| `b2e3cd2` | `5f8d3e3` | M14 D10，`merge-t3` 归档 worktree 现在指这枚 |
| `382fd58` | `e6758a6` | 本仓删 `crates/data` 改锁 rev |
| `28502d6` | `ef905eb` | 决定重写、改正经的那枚文档 |

`filter-repo` 写的 `.git/filter-repo/commit-map` 是全量那张表（177 行）。**已核实**：远端按 author
和 committer 各扫一遍都是 177/177 noreply，tag `v0.1.0-rc1` 的 tagger 也是（对象号
`16aed13`→`1dbbde4`）。**未清的部分**：主仓本地仍留着旧 SHA 的对象（`split/quire-data` 已跟着挪走、
`merge-t3` 已重挂，但没有 `reflog expire` + `gc --prune`，那批旧 commit 在本地还捞得回来）。这是
本机磁盘上的事，公网那侧已经干净；要连本地一起抹需要另开一刀，动的是恢复手段本身。

**未验证（诚实）**：① `cargo check --target aarch64-linux-android` 仍**没跑过**，而且现在只能在
`quire-core` 那棵树里跑（`-p quire-core` 在本仓已经不解析）；② `dist.ps1` / `verify-installer.ps1`
/ `verify-portable.ps1` 依旧没在新布局上验过；③ 新仓没有 tag、没有 release 节奏，rev 是一枚裸
SHA，升级它是一次手改 + 一次全部门槛；④ 换 rev 之后 `Cargo.lock` 里会不会同时留两条
`quire-core` 记录（旧 rev 那条在被裁掉之前）没验过——本仓这次是第一次有 git 依赖，只有一条。

**补刀 · 写入侧（同日，紧跟着上面那把重写）**：历史洗干净了，可四枚行生成器照旧把 `$Exe` 和
pinned db 记成绝对路径——下一趟 `bench_matrix.ps1` 就会把同一份泄露再写进一次提交。现在它们都先过
`benchmarks/scripts/redact.ps1`：checkout 根 / `%TEMP%` / `%LOCALAPPDATA%` / `%USERPROFILE%` 分别
换成 `<repo>` `<temp>` `<localappdata>` `<user>`（跟历史那一趟用的是同一套写法，免得新旧行读起来是
两样）。两个坑记下来：① **必须在把反斜杠翻倍成 JSON 之前洗**——规则是从真实路径拼的，只有一个分隔
符，永远match不上 `C:\\Users\\…`；② PS 5.1 的 `ConvertTo-Json` 会把 `<` `>` 转义成 `\u003c`
`\u003e`，而 bench_matrix / profile_bench 是把 bench.ps1 那一行反序列化后再序列化写盘的，所以
`Open-JsonPlaceholders` 把它们开回来（值里**真的**含 `\u003c` 的情况不受影响，那份是 `\\u003c`）。
兜底是一句 `Write-Warning` 而不是一条更宽的规则：账号名仍以完整路径段出现，说明有一个前缀是这文件
不认识的，那就嚷出来，别安静地写下去。

**这刀的自证**（四枚写盘脚本各跑一遍，产物只落 `.scratch/`）：10 行输出里 **0 行**还带
`[A-Za-z]:[\\/]` 形状的绝对路径（这条不依赖知道本机账号名），**0 行**提到 `Users\`，10/10 带
`<repo>`、6/10 带 `<temp>`，`\u003` 剩 0，10 行全部 `ConvertFrom-Json` 通过。control 三枚：一条不
在任何已知前缀下的 `D:\other\thing.exe` **原样返回**（证明它不是「把一切都换成 `<repo>`」）；换到
E 盘的另一个 home 被兜底规则洗成 `Users\<user>`；`profile_bench.ps1` 那枚 meta 行是从脚本里**读出
那一行源码再 Invoke-Expression** 求值的（不是抄一份），它带着 `<repo>` 且 `exe_bytes` 仍是真实的
27 847 168。有一处误报要说清：按「账号名作为子串」判会命中 1 行，那是英文单词里偶然含到的三个
字母，不是路径；所以真正的判据是上面那两条不依赖账号名的（drive 前缀 + `Users\`）。

## M9.pre · 安卓壳（2026-09-23，on `master`）

用户点定三件事：**编译过就行、不装真机、注意性能**；外壳**复用桌面组件**，不重写那 12k 行 `ui/`。
下面每一句都按这三条收口（ADR-0095 / ADR-0096）。

**做了什么**

- `Cargo.toml` 按 target OS 分裂：`slint` 出现两次——桌面那侧 `backend-winit` + `accessibility`，
  安卓那侧 `backend-android-activity-06` + `renderer-skia`——`rfd` 只在桌面。`[lib] crate-type`
  加了 `cdylib`（cargo 没有按 target 的 crate-type，所以 Windows 这侧白多出一个没人加载的 `quire_shell.dll`）。
  安卓需要传的 flag 只剩一个 `--no-default-features`：渲染器写在依赖表里，不必再 `--features skia`。
- 两道缝进 `src/platform/`：`data_dir()`（安卓是 Activity 的 `internal_data_path()/Quire`，桌面照旧
  `%APPDATA%`）、`picker::Picker`（7 处 `rfd::FileDialog` 全改走它；安卓是第三种答 `Unsupported`，
  通知栏会分清「打不开」和「存不了」，于是没有一颗按钮是闷着不响的）。`quire-core` 为这一刀**没改过
  一个字**——`decide()`/`migration()` 本来就把 per-user 路径当参数收。
- `src/main.rs` 的启动序列搬进 `src/app/launcher.rs::run(start, touch_mode)`，`main.rs` 剩 10 行；
  `src/android.rs` 55 行就是 `android_main`，照 ADR-0009 再开一条 8 MB 栈的线程（理由是 Slint 递归
  布局，不是 Windows）。
- UI 不 fork：`UIState.touch-mode` 一处声明、两个文件七处 `if` 读；新组件只有
  `ui/components/MobileBar.slint`（五个动作全是既有回调，Rust 一行没加）。抽屉与侧栏是**互斥实例**——
  两个 `Sidebar` 会去抢同一条 `content-y <=> tree-viewport-y`。桌面想看不用真机：`--touch`。

**踩到并改掉的五个坑**

1. 头一版分裂把 slint 的 `accessibility` 一并删了，理由是「`src/` 里 0 处引用」——这条论证对**桥接类**
   特性根本不成立：删掉它 Narrator / NVDA 就读不到这个窗口，而我们的代码本来就一行都不用写。已放回桌面
   那侧；安卓那侧不该有，是因为平台的可访问性服务走 view hierarchy，而这里只有一个 `NativeActivity` 表面。
2. cargo-apk 0.10 见到清单里有 `[workspace]` 直接罢工（`Did not expect a [workspace]`）。而那张表其实
   早就没在跨仓统一任何东西——`quire-core` 是 git 依赖、不是成员——所以整段删掉，`image`/`rusqlite`
   两条 pin 原样搬回 `[dependencies]`，两条的版本与 feature 一字未改。
3. `--all-targets` 不等于「清单里全都编过」：`quire-shot` 挂着 `required-features = ["software"]`，
   `--no-default-features` 那趟根本没编它。所以那句「两 ABI 全绿」说的是库、`quire-typing` 和两枚集成
   测试壳，不多不少。
4. `cargo apk build` 读的是清单、不是 cargo 的构建计划，它会把包里**每一个**产物都往 APK 里塞，而它只
   认得 `cdylib` 的名字：撞到第一个 `[[bin]]` 就 panic（`Bin is not compatible with Cdylib`，
   cargo-subcommand 0.12 `artifact.rs:51`）。加 `--lib` 是把这个平台唯一能打包的目标先说清楚——不 panic，
   也不再为一部永远跑不到它的手机去链 `quire-typing`。
5. 无条件加 `cdylib` 换来一个**我自己制造的回归**：库和 `quire.exe` 共用一个 target 名，也就共用
   `target/debug/quire.pdb`。cargo 对这件事是有警告的（rust-lang/cargo#6313，"this may become a hard error"），
   而覆盖是真实发生的——量到的：整树编完再 `cargo build --lib`，`quire.exe` 那枚 413 MB 的符号文件被
   DLL 的 187 MB 换掉了。修法不是把 `src/` 与 `tests/` 里那 72 处 `quire::` 路径重写，而是 `[lib] name = "quire_shell"`
   + 五个消费方各一行 `use quire_shell as quire;`（rustfmt 会把 `quire` 排在 `quire_shell` 前面，所以别名
   要放在那一组的**下面**）。安卓这侧同名同源：`android.app.lib_name` 与打包的 `libquire_shell.so` 都从
   `[lib] name` 读出来，两者不会漂移。

**验证**（NDK 30.0.15729638 / API 24 / cargo-apk 0.10）

- `cargo check --target x86_64-linux-android --all-targets` 1m03s、`aarch64` 1m48s，两趟都是
  0 error。中途唯一的警告是 `Picker.kind` 在安卓 cfg 下成了死字段——改法不是 `#[allow]`，而是让
  `Unsupported` 按 open/save 各说一句话，警告消掉、通知栏也更准。改名之后两 ABI 一趟 169 s。
- `check` 从不链接，所以 release `cdylib` 是真编过的：`libquire_shell.so` aarch64 33,244,216 B、
  x86_64 33,234,768 B（未压缩 62.87 MiB，两 ABI 只差 9.4 KB）；cargo-apk 的 `llvm-strip` 只再省
  0.39 / 0.15 MiB（`[profile.release]` 的 `strip = "debuginfo"` 已经把大头做完了）。
  `-Task lib` 两 ABI 1,182 s。
- 包出来了：`target/release/apk/quire_shell.apk` **25,473,461 B = 24.29 MiB**，两个 ABI 都在里面
  （arm64 deflate 12,575,762 B、x86_64 12,886,840 B），`apksigner verify` 说 v2 + v3 过、v1（JAR）
  不过，`min_sdk_version = 24` 下这正好够用。体积对照表在 `docs/PERFORMANCE.md` §1（桌面 release
  `quire.exe` 那一行是改名之前量的，并行会话当时正在重链 `target/release`，所以没有重抄）。
- 桌面门槛按 `just check` 复跑（分裂依赖 + 删 `[workspace]` + 改库名，动的都是清单不是逻辑）：
  `check --all-targets` 133 s、`cargo build`（dev，专门用来看 PDB 警告还在不在）243 s 且**没有**警告、
  `cargo test` 218 s / 134 passed 0 failed；`target/debug/` 里 `quire.pdb`（416,952,320 B，exe 的）与
  `quire_shell.pdb`（188,518,400 B，库的）同时存在，就是上面第 5 个坑修好的样子。

**没验的（诚实）**：没设备、也没 AVD，用户明确说了不用装真机。首帧、帧时、内存、IME、抽屉手势**一个数都
没有**——`docs/PERFORMANCE.md` 新加的那节是体积表，不是延迟表，别拿桌面 skia/GL 那臂去读它。签名那一步走
的是 `.scratch/` 里现生成的一次性 key（清单里那条 signing 表就是给它指路的），真正的发布身份留给 M9.1。
M9.0（真机输入尖峰）和 `ui/` 里那 163 处 3–43 px 的 `height:` 字面量（44 dp 那一刀的量法见
`docs/ANDROID_NOTES.md`），仍旧排在后面。

## 压测 · 阶梯推到矩阵之外，而一支量具先修了它自己的谎（2026-09-23，on `master`，无新 ADR：测量刀）

**这一刀不做机制，做三件事：把树压到断、把断的形状钉住、把本文件此前一句错的收回来。**

1. **身份先于数字。** 树是脏的（M9 那摊未提交，`docs/PERFORMANCE.md` 末尾就是他们那节），
   所以全批次锁在一枚二进制上：`target/release/quire.exe` SHA-256 `57c105e6…cf7fda4c`、
   `quire-typing.exe` `9e9fa909…a31f`，跑完复算未变。为防并行 `cargo build` 把路径下的 exe
   重链，阶梯与诊断臂都跑 `%TEMP%\quire-bench-57c105e6\quire.exe` 那份逐字节副本；
   每一行自己吐 exe 哈希和 app 自己的 `dump-state: pages=… gs+atlas-blocks=… coloured=…`，
   诊断批次末尾一句 `arms=5 unproven=0`。
   写入侧在提交前被抓到一次：`redact.ps1` 的前缀规则只认反斜杠，而阶梯拿到的 `$Exe` 是正斜杠，
   于是 `<temp>` 规则整个漏过、只剩账号名那条兜底，行里留下 `C:/Users/<user>/AppData/…` 这种
   「看着已经洗过、其实还在点名这台机器」的路径。规则改成分段转义后用 `[/\]` 重连（两种分隔符
   都认），r2 那 18 行 `exe` 一并改回 `<temp>/…`。ADR-0094 那个洞的下一个同类。
2. **矩阵复跑（30 行，全 exit 0）**：A 129.6 / 102.6 MB，D10000 151.4 / 124.6，Flick 100.9 % CPU，
   打字 29.68 / 29.0 / 29.81（请求 30）、117.17（请求 120），handler 中位 90 / 140 / 82 µs。
   对 M7 全线 +13…36 %，但**能免疫会话漂移的那个比值越线了**：同场次内 D − A = +21.8 MB WS /
   **+22.0 MB private**，M7 是 +7.2 / +9.2，09-21 是 +10 / +12.5 —— D÷A 的 private **1.214×**，
   门槛 1.2×。每行成本自 09-21 起大约翻倍，这是 §六 那条 memory-delta 从 ✓ 改回来的原因；
   归因**没做**，写成了 bisect 待办而不是指控。
3. **新量具 `benchmarks/scripts/stress_ladder.ps1`。** 矩阵的 15 s 窗口 / 60 s 退出钟在 10 000 行
   以上正好把发现吃掉：晚到的窗口本身就是结论，而被杀掉的进程把解释它的那个数一起带走。
   阶梯换宽钟、采样峰值 WS、加 `settle_wait_ms` / `settled`（「空转」= 进程自己安静下来，
   并且把等了多久公布出来），退出码分成 `0` / `harness-grace` / `refused-to-exit`，`-Only`
   零命中就抛。9 臂 × 2 趟 = 18 行，无 hung、无 exited_early。50 000 行装载 1 186 ms、
   12.9 s 安静到 0.57 % / 198 MB；100 000 行 260.8 MB、49.8 s；500 页侧栏切换 132.3 MB / 0.39 % / exit 0。
4. **撤回一句本会话早上写下的话。** 「20 000 行以上的页面永不空转（50k 40.7–50.2 %、100k
   95.4–97.9 %，而 20k 只有 0.19 %——那条斜率看着就像机制）」
   **是量具的伪影**：8 s 采样落在窗口出现后 2 s，也就是正卡在首次写回里。同一枚二进制、同一个页面，
   改成每 2 s 一采样（`.scratch/idle_out.txt`）：CPU 顶到 t≈43 s，t≈47 s 归 0，之后 135 s 一直 0.0–0.8 % /
   236 MB 平。留下不退的那半：**S100000-seed**（`settled: false`、95.12 %、`refused-to-exit`）是全新库
   上第一次写回，90 s 内真的没写完——这是一个「不该有人到的尺寸」上的真代价，如实记着。
5. **两个断掉的形状，各带自己的对照。**
   - **每一行都是图的页面**（`--pictures == --blocks` ⇒ `bench_picture_plan` 的 stride = 1）无界增长：
     101 s 到 3 099 MB 还在爬（≈+30 MB/s），早前两趟 4.4 GB / 90 s 空转、6.5 GB / 120 s 滚动。
     三臂把它们夹死：A（10 000 行 / 5 000 图，stride 2）166 MB 平、exit 0；B（10 000 / 10 000，stride 1）
     3 099 MB 不退出；B2（20 000 行 / **同样 10 000 个图行**，stride 2）180 MB 平、t≈56 s 起安静。
     ⇒ **不是图行数量，是那个「整页无一行非图」的形状**；也不是解码缓存（32 MiB 上限和九帧天花板在
     本文件每个媒体臂上都照常生效，这里数据库一直是 4 KB，而缓存报告压根没打印——进程没活到打印它的那
     个回调）。**没认领原因**，只写了两个各值一小时的可疑处（Slint 自己的 path/texture 缓存；每格都是
     image delegate 时的重新 realize）。
   - **10 000 行带高亮的 code 页 + 滚动**：空转完全免费（4.8 / 5.4 s 安静、0.00 / 0.58 %、139.7 MB、
     干净退出），一滚就是 92.35 / 98.50 % 占用一核、WS **平**（142.7 → 143.6 MB，不是泄漏，是纯重画），
     并且**两趟都没跑到自己的 `--auto-exit`**；100 s 复核里 CPU 全程 82–101 %。对照是同场次同形状、
     只差 `--code` 的 D 臂：普通文本页同样 200 px  flick，走了 1 639 836 px 之后 **t=82 s 自己 exit 0**。
     ⇒ 不是滚动，不是页长，是「高亮 code 行 × 视口在动」。副作用记两条：事件循环忙到不吃 `slint::Timer`
     意味着用户点关闭时只能被杀；ADR-0042 那句「上色无每行成本」仍然成立，因为门槛只量空转，
     而门槛不滚带标记的行。

**没测的（诚实）**：① 数据层那批 `#[ignore]` 探针**这次跑了但作废不发布**——它们与一条阶梯臂同场
竞争，同一批探针对照记录读了 3.5–15 倍（rollup 窗口中位 114.6 ms vs 记录 28.5 ms；1 000 孤儿回收
8 815 ms vs 550 ms），只有 §三十九 真正在乎的比值活了下来（窗口/全表 3.58× vs 3.12×）。欠一次干净
复跑，而且它现在还是另一个问题：数据层已搬进锁 rev 的 `quire-core`，09-22 那些行本就不是同一份代码。
② 全程 FemtoVG，阶梯从没在 skia 上跑过，而 defect A 完全可能是只有 FemtoVG 才有的纹理缓存行为——
这是该跑的理由，不是结论。③ 夹具是 1280×720 渐变，最便宜的解码，所以媒体数字是地板。
④ 帧与卡顿仍然看不见（harness 量 CPU/私有字节/`scroll_y`），§六 那句「no hitching observed」还是欠
一次人手滚动。⑤ `--code` 未过 10 000、`--pictures` 未试 1 000–9 999、图不在 stride 上的真实页面未测。

**改动清单**：新增 `benchmarks/scripts/stress_ladder.ps1`；新增原始行
`benchmarks/results/2026-09-23-m14-matrix-vg.jsonl`（30）、`…-stress-ladder-vg-r2.jsonl`（18；
r1 那 18 行是换 settle 之前的旧词汇，留着不改，`exit_code` 词表不同）；文档落
`docs/PERFORMANCE.md`（§"Stress · the ladder past the matrix"、Method 一段、M7 follow-ups 重排、
§六 memory-delta 改回未达标）、`docs/ROADMAP.md`（M7 行）、`CHANGELOG.md`（Build & test）；
`benchmarks/scripts/redact.ps1` 改的是上面那条分隔符规则。
`benchmarks/scripts/audit_results.ps1` 复跑绿（12 first-paint + 28 bench）。诊断脚本留在 `.scratch/`
（`diag_size2.ps1`、`diag_final.ps1`、`idle_out.txt`、`leak_out.txt`、`step_out.txt`、`density_out.txt`、
`probes_out.txt`），它们是上面每条断言的证据路径，不进清单。
