// Application state + mock content for M2/M4.
//
// The page tree lives in `workspace.rs` (pure, unit-tested); the block
// content of every page lives in `core::Document` since M4 (the editing
// truth, mutated only through commands). This module is the view-
// projection layer between the two and the Slint models (SidebarNode /
// BlockRow / CommandRow / SearchRow / MenuRow).

use crate::app::workspace::{SearchHit, Workspace, BENCH_ID_BASE, MAX_RECENTS};
use crate::core::database::{
    CellValue, DatabaseCatalog, DatabaseDraft, DatabaseId, Property, PropertyId, PropertyKind,
    RecordId, RowRequest, RowWindow, SortSpec, ViewGeometry, ViewId,
};
use crate::core::database_formula::{
    self, FormulaError, Program, Val, FORMULA_ERROR_PAINT, FORMULA_MAX_DEPTH,
};
use crate::core::database_property::PropertyOptions;
use crate::core::database_relation;
use crate::core::database_rollup::{self, Aggregate};
use crate::core::database_view::{
    all_columns, board_slots, board_window, chart_line_path, chart_pie_paths, date_key,
    day_number_of, day_of, days_in_month, group_window, is_stored_date, layout_metrics,
    month_cells, month_key, month_label, shift_month, table_columns, table_rows, view_columns,
    CALENDAR_PEEK, ChartKind, FilterClause, FilterNode, FilterOp, FilterValue, FlatClause,
    FlatFilter, GroupKey, GroupSpec, LayoutSupport, TableColumn, TableRowView, TableView,
    ViewDefinition, ViewRules, ViewTab, WIDTH_AUTO, WIDTH_MIN, WIDTH_UNIT,
};
use crate::core::database_template;
use crate::core::persistence::{Change, Repository};
use crate::core::{
    Attachment, AttachmentId, Block, BlockId, BlockKind, ColorKind, Command, Document, History, Lang,
    Mark, OrderKey, PageFont, Page, PageId,
};
use crate::services::find_service::FindSession;
use crate::services::persistence::PersistenceService;
use crate::services::search_service::SearchService;
use crate::storage::search_index::SearchRequest;
use crate::storage::SqliteRepository;
use crate::storage::versions;
use crate::{BacklinkRow, BlockRow, ColumnBox, ColumnItem, CommandRow, DbCell, DbColumn, DbOption, DbRow, DbViewTab, DiffRow, MenuRow, SearchRow, SidebarNode, SlashRow, TableCell, TextRun, TocEntry, VersionRow};
use slint::{Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

pub struct AppState {
    pub workspace: RefCell<Workspace>,
    pub sidebar: Rc<VecModel<SidebarNode>>,
    pub blocks: Rc<VecModel<BlockRow>>,
    /// The backlink panel's window (SPEC §四十, ADR-0051). A model, but not
    /// state: every projection of a page rebuilds it from `marks`, so nothing
    /// it holds is anything but a reading of the page that is already open.
    pub backlinks: Rc<VecModel<BacklinkRow>>,
    pub commands: Rc<VecModel<CommandRow>>,
    pub search: Rc<VecModel<SearchRow>>,
    pub menu: Rc<VecModel<MenuRow>>,
    pub slash: Rc<VecModel<SlashRow>>,
    pub block_menu: Rc<VecModel<MenuRow>>,
    /// The version panel's two views (SPEC §三十八, ADR-0091): the page's list
    /// of named versions, and the comparison of one of them with the page. Both
    /// are models rather than a `Vec` handed over on open because the panel
    /// stays on screen while a save, a delete or a restore changes what it
    /// shows — and a projection pushed into a live model redraws it in place.
    pub versions: Rc<VecModel<VersionRow>>,
    pub version_diff: Rc<VecModel<DiffRow>>,
    /// Full command list before query filtering.
    pub all_commands: Vec<CommandRow>,
    /// The editing truth for every page's blocks (M4).
    pub doc: RefCell<Document>,
    /// Per-page undo/redo stacks.
    pub history: RefCell<History>,
    /// Debounced persistence pipeline (M3). `None` = headless/test mode.
    pub persistence: Option<Arc<PersistenceService>>,
    /// The database this session runs on (`None` = headless or memory-only).
    /// The settings dialog's storage row and "Back up now" ride it.
    pub repo: Option<Arc<crate::storage::SqliteRepository>>,
    /// FTS-backed search (M7). `None` falls back to the in-memory scan.
    pub search_service: Option<Arc<SearchService>>,
    /// Where attachments live (SPEC §三十七 批次 A). Beside the database, or
    /// in the temp folder for a headless session with no database at all.
    pub store: crate::services::attachment_store::AttachmentStore,
    /// Attachment rows by id, loaded at startup and appended on import.
    pub attachments: RefCell<BTreeMap<i64, Attachment>>,
    /// Decoded rasters by attachment id. Populated only for a row the ListView
    /// realizes, and capped by `MAX_ATTACHMENT_CACHE_BYTES` — Slint's own
    /// image cache is keyed by path and holds 5 MB, which is one photograph,
    /// so scrolling a photo page through it re-decodes on every frame.
    attachment_images: RefCell<BTreeMap<i64, CachedImage>>,
    attachment_cache_bytes: Cell<usize>,
    /// Highest the cache has ever been this session — what the bench harness
    /// prints, because a ceiling nobody reads is only a promise.
    attachment_cache_peak: Cell<usize>,
    attachment_tick: Cell<u64>,
    /// Next attachment id: one past the highest row this session loaded.
    next_attachment_id: Cell<i64>,
    /// In-flight async search with its generation; superseded queries drop
    /// their result instead of overwriting newer ones.
    pending_search: RefCell<Option<(u64, crate::services::search_service::PendingSearch)>>,
    /// Ctrl+F in-page find session (Track B's FindSession).
    find_session: RefCell<Option<FindSession>>,
    find_label: RefCell<String>,
    /// Rows whose runs currently carry a hit cell. The next search has to
    /// un-paint those too, and they are not the rows the new term hits.
    find_painted: RefCell<Vec<i32>>,
    /// Folded or unfolded, the backlink panel's one piece of session state.
    /// Not persisted: it is a reading preference about a panel, and the folded
    /// shape is the default because it is the one that keeps the prose on
    /// screen when a page is quoted two hundred times (SPEC §四十).
    backlinks_expanded: Cell<bool>,

    search_generation: Cell<u64>,
    /// Persisted sibling order of every page (drives PageCreated/Moved).
    page_order: RefCell<HashMap<i32, OrderKey>>,
    /// Installed by the controller: restarts the flush timer on record().
    flush_hook: RefCell<Option<Rc<dyn Fn()>>>,
    /// Cross-block clipboard (menu-driven copy/paste of one block).
    clipboard: RefCell<Option<Block>>,
    /// Persisted settings (theme etc.), loaded from storage at startup.
    settings: RefCell<HashMap<String, String>>,
    /// UI weak handle, installed by the controller at wire time — lets the
    /// state push display-only projections (page stats) without a callback.
    ui: RefCell<Option<slint::Weak<crate::UIState<'static>>>>,
    /// Persisted recent-page ids, restored before first open.
    recents_restored: Cell<Vec<i32>>,
    /// Startup notices (abort banner, backup restore, library move) — more
    /// than one can queue; the controller drains them as one line and shows
    /// it once in the shell.
    db_notice: RefCell<Vec<String>>,
    /// Currently open page (0 = none / empty workspace).
    pub open_page: Cell<i32>,
    /// Go Back / Go Forward stacks (SPEC §十六). Session-only: a restart
    /// starts with no history.
    nav: RefCell<NavHistory>,
    /// Page awaiting delete confirmation.
    pub pending_delete: Cell<Option<i32>>,
    /// Benchmark scroll bookkeeping (scene F): last viewport-y seen.
    pub last_scroll_y: Cell<f32>,
    /// Which ⋯ → Templates row opened the template picker, so a picked
    /// template knows what it is being picked *for* (SPEC §三十八). One of the
    /// `MENU_TEMPLATE_*` action ids; `fill_template_pick` writes it and the
    /// row click reads it. Nothing else uses it, so it never needs clearing.
    template_pick: Cell<i32>,
    /// Named versions this library holds, by page, newest first (SPEC §三十八
    /// "version history", ADR-0091). Mirrored from the `version/<page>/<when>`
    /// metadata rows at load rather than queried, so the panel fills without a
    /// read of its own and a session with no database — which can have no
    /// versions, since a version *is* a file — shows an empty list instead of
    /// an error.
    version_index: RefCell<BTreeMap<i32, Vec<(i64, String)>>>,
    /// The attachment ids each version's rows point at, from the matching
    /// `version-files/…` row. A version file is a pointer the §三十七 reclaim
    /// scan cannot otherwise see, and a picture the reclaim frees would make
    /// the version restore as a missing image with nothing to explain it —
    /// ADR-0047's objection to a cover, arriving from the other direction.
    version_pins: RefCell<BTreeMap<(i64, i64), BTreeSet<i64>>>,

    // ─── SPEC §三十九 Database (D3) ─────────────────────────────────────────
    /// The database layer's schema — every entity, its columns and its views,
    /// loaded once at startup and kept in step with every write this session
    /// makes. Deliberately **not** its records: ADR-0067 makes a window the only
    /// way a row reaches memory, so this is a schema's worth of data (a handful
    /// of rows) however large the databases it describes are.
    databases: RefCell<DatabaseCatalog>,
    /// One block's realized window, by block id. A cache with a rule: it is
    /// rebuilt when the projection's inputs change (the view, the definition,
    /// the geometry) and *not* when a scroll stays inside the window it already
    /// holds. `Rc<VecModel>` inside it because the delegate reads the rows
    /// through the block's row, and a re-read must update the UI without
    /// rebuilding the page's row list.
    db_windows: RefCell<HashMap<i32, DbWindow>>,
    /// How many change batches this session has recorded — the window cache's
    /// **content** half.
    ///
    /// A window caches *painted* rows (a cell's text, a relation's live titles,
    /// a rollup's folded value), and every one of those depends on stored data
    /// that the rest of the cache key cannot see: the view, the definition
    /// text, the layout, the total and the row window are all identical after a
    /// cell edit, after a rename in *another* database, and after a computed
    /// column's config changes. Without this number the cache answers "nothing
    /// changed" to all three and the user watches their edit not appear — the
    /// defect D3's sweep could not see, because a scene is a still photograph.
    ///
    /// Deliberately blunt: *any* recorded change bumps it, rather than an
    /// enumeration of "changes that can alter a painted cell" (a cell, a page
    /// title a relation names, a column's kind, a column's config, a record
    /// gaining a page…). That list is one every future kind would have to be
    /// added to, and missing it is exactly this bug; "any change may have" is
    /// always true. The cost is bounded by the same asymmetry the cache exists
    /// for: the stamp is *consumed* when a window is rebuilt, so a burst of
    /// edits costs one re-read of each visible database window — not one per
    /// edit, and never one per scroll frame (a scroll inside an unchanged
    /// window still touches no query).
    db_content_stamp: Cell<u64>,
    /// Which view each database block is showing. Session state (ADR-0073) and
    /// not a column: it is a fact about a window, not about a document.
    db_active_view: RefCell<HashMap<i32, ViewId>>,
    /// Where each database block's top sits relative to the editor viewport, as
    /// the block itself reported it (Rust cannot see Slint's layout). What the
    /// window is computed from, together with the page's scroll offset.
    db_anchor: RefCell<HashMap<i32, f32>>,
    // ─── D5: the view family's session state ─────────────────────────────────
    // Three maps, all keyed by block id, all session-only (ADR-0073's rule:
    // "which / how the user is looking" is a fact about a window, not about a
    // document — a restart opens the defaults).
    //
    // * `db_cal_month` — which month a calendar view shows. Defaults to the
    //   month that contains today, but the scene seeds pin it so a sweep
    //   photographs the same grid tomorrow.
    // * `db_gallery_per_row` — how many cards a gallery row has. **Reported by
    //   the delegate** (it is the layer that knows the grid's width, the same
    //   split as `db_anchor`'s top-in-view), and part of the window's cache key
    //   through `DbWindow::stamp` because the card slice is rows × per_row.
    // * `db_form` — a form view's draft, one `(property, text)` per filled
    //   field. A draft and not a record on purpose: nothing reaches SQL until
    //   Submit, and one submit is one undo step.
    db_cal_month: RefCell<HashMap<i32, (i32, u32)>>,
    db_gallery_per_row: RefCell<HashMap<i32, usize>>,
    db_form: RefCell<HashMap<i32, Vec<(u64, String)>>>,
    // ─── D7: the view's live search (SPEC §三十九 「操作」, ADR-0087) ────────
    // The needle a database block's header search box holds, by block id —
    // **session state, not a column** (ADR-0073's rule applied to a query: a
    // search is a question the user is currently asking, so a restart opens
    // an unsearched view and the view's document is untouched). Compiled into
    // the request's `search` half, which is what makes it part of the one
    // statement: count, window and group headers all answer the same rows.
    db_search: RefCell<HashMap<i32, String>>,
    // ─── D6/D8: the computed columns' session state ─────────────────────────
    /// How many **computed cells the projection has evaluated**, since startup.
    /// A counter nothing reads (no code path branches on it): it is the number
    /// ADR-0083's recompute contract is measured in — "edit one cell, and the
    /// evaluation count grows by the *window's* computed cells, never by the
    /// table's rows" is a claim about this number and about `COUNT(*)`.
    ///
    /// One number for both computed kinds (ADR-0089): a formula cell counts
    /// when the engine evaluates it, and a *configured* rollup cell counts when
    /// its fold runs, because "the window's computed cells" is the unit the
    /// contract is stated in and two counters could quietly disagree about it.
    /// A relation cell never counts — it is a stored list of ids with no engine
    /// behind it (ADR-0088). Bumped only in `db_paint_computed`: the editor's
    /// previews evaluate on demand and are not projections.
    db_computed_evals: Cell<u64>,
    /// The editor list's own height, in px, reported by `Editor.slint` when it
    /// changes. The window's *viewport height* — the number `core::database::window`
    /// divides by — and a property rather than a callback argument because it is
    /// one number for the whole page and every database on it wants the same one.
    pub editor_viewport_h: Cell<f32>,
    // The four id watermarks (ADR-0072): one counter per table, seeded from
    // `MAX(id)` at startup and never read again. `Cell<u64>` because a creation
    // is one counter step and nothing else in the layer is mutable state.
    next_db_id: Cell<u64>,
    next_property_id: Cell<u64>,
    next_record_id: Cell<u64>,
    next_view_id: Cell<u64>,
}

/// One decoded picture plus what it costs to keep it decoded.
#[derive(Clone)]
struct CachedImage {
    /// LRU stamp: the tick of the last projection that asked for this raster.
    used: u64,
    /// Decoded bytes (RGBA), which is what the budget below is spent against.
    bytes: usize,
    image: slint::Image,
}

/// Ceiling on the decoded rasters this session holds. A 1280x720 RGBA frame is
/// 3.7 MB, so this is roughly eight pictures — a screenful with room to scroll
/// back one page without re-decoding. Without a ceiling the map only ever
/// grows, and §二十二's low-RAM promise dies the first time someone pastes a
/// hundred screenshots.
const MAX_ATTACHMENT_CACHE_BYTES: usize = 32 * 1024 * 1024;

/// Go Back / Go Forward stacks (SPEC §十六), newest entry last. Kept as a
/// plain struct with no Slint or database in it so the stepping rules —
/// which are the whole feature — are unit-testable.
#[derive(Default)]
struct NavHistory {
    back: Vec<i32>,
    forward: Vec<i32>,
}

/// How far back Go Back reaches before it starts dropping entries.
const NAV_MAX: usize = 50;

impl NavHistory {
    /// A user-initiated move from one page to another. A new navigation
    /// drops the forward branch, the way a browser does.
    fn record(&mut self, from: i32, to: i32) {
        if from > 0 && from != to {
            self.back.push(from);
            if self.back.len() > NAV_MAX {
                self.back.remove(0);
            }
        }
        self.forward.clear();
    }

    /// Step one page back (or forward), skipping pages deleted since they
    /// were recorded, and push `current` onto the opposite stack. `live`
    /// answers "does this page still exist?".
    fn step(&mut self, forward: bool, current: i32, live: impl Fn(i32) -> bool) -> Option<i32> {
        let (from, to) = if forward {
            (&mut self.forward, &mut self.back)
        } else {
            (&mut self.back, &mut self.forward)
        };
        let target = loop {
            match from.pop() {
                Some(id) if live(id) => break id,
                Some(_) => continue,
                None => return None,
            }
        };
        if current > 0 && live(current) && to.last() != Some(&current) {
            to.push(current);
        }
        Some(target)
    }
}

/// Block ids start above this so they never collide with anything derived
/// from mock page ids.
const BLOCK_ID_BASE: u64 = 1_000_000_000;

pub struct HandleArgs {
    /// Number of mock blocks for benchmarks (0 = default sample document).
    pub blocks: usize,
    /// Quit the event loop after N seconds (0 = never).
    pub auto_exit_secs: f64,
    /// Scene G: number of extra flat pages to switch between.
    pub bench_pages: usize,
    /// Scene D/F with media: how many of the bench page's rows are pictures
    /// (SPEC §三十七's gate for a page of images being scrolled).
    pub pictures: usize,
    /// Scene D with inline marks: how many of the bench page's rows carry a
    /// bold mark. A page with no marks cannot show what the runs channel costs,
    /// and scene D has none without this.
    pub marks: usize,
    /// Scene D with highlighted code: how many of the bench page's rows are
    /// coloured code blocks (SPEC §三十七 批次 C's gate — a page of code
    /// scrolling, with six layers of text behind every visible block).
    pub code: usize,
}

/// Build the mock/bench session (fresh database or no persistence).
fn build_mock_session(args: &HandleArgs) -> (Workspace, Document, HashMap<i32, OrderKey>) {
    let mut workspace = if args.bench_pages > 0 {
        Workspace::with_bench_pages(args.bench_pages)
    } else {
        Workspace::sample()
    };
    let mut doc = Document::new(BLOCK_ID_BASE);
    for id in workspace.dfs_order() {
        let title = workspace.title_of(id).unwrap().to_string();
        let rows = if args.blocks > 0 && id == PAGE_ATLAS {
            mock_blocks_bench(args.blocks)
        } else if id >= BENCH_ID_BASE {
            mock_blocks_bench_page(&title)
        } else {
            mock_blocks_for_page(id, &title)
        };
        let blob = block_search_blob(&title, &rows);
        workspace.set_search_text(id, blob);
        let core_blocks = rows_to_blocks(id, rows, &mut doc);
        doc.set_page_blocks(core_page_id(id), core_blocks);
    }
    let page_order = assign_page_orders(&workspace);
    (workspace, doc, page_order)
}

/// Chain sibling order keys per parent group (deterministic seed order).
fn assign_page_orders(workspace: &Workspace) -> HashMap<i32, OrderKey> {
    fn walk(ws: &Workspace, parent: Option<i32>, map: &mut HashMap<i32, OrderKey>) {
        let mut prev: Option<OrderKey> = None;
        for id in ws.children_of(parent) {
            let key = OrderKey::between(prev, None).expect("append order exhausted");
            map.insert(id, key);
            prev = Some(key);
            walk(ws, Some(id), map);
        }
    }
    let mut map = HashMap::new();
    walk(workspace, None, &mut map);
    map
}

/// The instant a version is stamped with, in unix seconds. `SystemClock`'s
/// `now_ms` is *this session's* elapsed clock, which is exactly wrong here: a
/// version file outlives the session, and its name has to mean something to the
/// next one. Only seconds, because the file name and the metadata key are the
/// same number and a version taken twice in one second is one version.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The label a save stores: trimmed to one line and cut at a character
/// boundary, because a panel row that splits an emoji in half renders as a box.
/// Empty is not a name, so it gets one that says which version of this page it
/// is — and a label is metadata, never a file name, which is why a name with a
/// slash, a quote or both works.
fn version_label(label: &str, taken: usize) -> String {
    let one_line: String = label
        .trim()
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .take(60)
        .collect();
    if one_line.is_empty() {
        return format!("Version {}", taken + 1);
    }
    one_line
}

/// The panel's list rows, newest first: the projection `fill_versions` pushes
/// and a benchmark scene draws. Its own function because a headless session has
/// no database file, and a version *is* a database file — the scene supplies
/// rows that were never written, and this is the one place that says what a row
/// looks like, so the pixels still come from the real projection. Whether the
/// files behind those rows exist is what the storage tests answer.
pub fn version_rows(versions: &[(i64, String)]) -> Vec<VersionRow> {
    let now = now_secs();
    versions
        .iter()
        .enumerate()
        .map(|(index, (created, label))| VersionRow {
            index: index as i32,
            label: label.as_str().into(),
            when: crate::core::diff::age_text(now - *created).as_str().into(),
        })
        .collect()
}

/// The sentence under the panel's title. The only place the retention cap is
/// stated in prose, which is why it reads off `MAX_PER_PAGE` rather than a
/// number someone might stop updating.
pub fn versions_note(count: usize) -> String {
    format!(
        "{} of this page. Quire keeps the {} newest and lets the oldest go.",
        if count == 1 {
            "1 version".into()
        } else {
            format!("{count} versions")
        },
        crate::storage::versions::MAX_PER_PAGE,
    )
}

/// The comparison view's heading: which version is on screen and how long ago
/// it was taken, in the same words the list row uses for the same number.
pub fn version_heading(label: &str, created: i64) -> String {
    format!(
        "“{label}” · {}",
        crate::core::diff::age_text(now_secs() - created)
    )
}

/// And the line under it, which is the restore button's terms stated before
/// the button is pressed.
pub fn version_diff_note(lines: &[crate::core::diff::DiffLine]) -> String {
    let added = lines
        .iter()
        .filter(|l| l.mark == crate::core::diff::DiffMark::Added)
        .count();
    let removed = lines.len() - added;
    format!("{added} lines arrived, {removed} went. Restoring puts every one of them back.")
}

/// Mirror the library's version index out of the metadata rows it persisted
/// (SPEC §三十八, ADR-0091). Both halves come from the same scan: which
/// versions a page has, and which pictures each one is holding alive. A row
/// that parses as neither is skipped, so a half-written version reads as absent
/// rather than as a page zero.
fn restore_versions(
    persisted: &Option<crate::core::PersistedState>,
) -> (
    BTreeMap<i32, Vec<(i64, String)>>,
    BTreeMap<(i64, i64), BTreeSet<i64>>,
) {
    let mut index: BTreeMap<i32, Vec<(i64, String)>> = BTreeMap::new();
    let mut pins: BTreeMap<(i64, i64), BTreeSet<i64>> = BTreeMap::new();
    let Some(state) = persisted else {
        return (index, pins);
    };
    for (key, value) in &state.meta {
        if let Some((page, created)) = crate::storage::versions::parse_key(key) {
            index.entry(page as i32).or_default().push((created, value.clone()));
        } else if let Some(rest) = key.strip_prefix(crate::storage::versions::KEY_FILES) {
            if let Some((page, created)) = rest.split_once('/').filter(|(p, c)| {
                p.parse::<i64>().is_ok() && c.parse::<i64>().is_ok()
            }) {
                let (p, c) = (
                    page.parse::<i64>().unwrap_or_default(),
                    created.parse::<i64>().unwrap_or_default(),
                );
                pins.insert(
                    (p, c),
                    crate::storage::versions::parse_ids(value),
                );
            }
        }
    }
    for versions in index.values_mut() {
        versions.sort_by(|a, b| b.0.cmp(&a.0));
    }
    (index, pins)
}

fn workspace_from_persisted(
    state: &crate::core::PersistedState,
) -> (Workspace, HashMap<i32, OrderKey>) {
    let ws = Workspace::from_persisted(&state.pages);
    let mut map = HashMap::new();
    for p in &state.pages {
        map.insert(p.id.0 as i32, p.order);
    }
    (ws, map)
}

impl AppState {
    pub fn new(args: &HandleArgs, repo_in: Option<Arc<SqliteRepository>>) -> Rc<Self> {
        // Try to load persisted state. A failed load disables persistence
        // for the session rather than risking a seed-flush over live data.
        let mut repo = repo_in;
        let loaded = match repo.take() {
            Some(r) => match r.load() {
                Ok(state) => {
                    repo = Some(r);
                    Some(state)
                }
                Err(e) => {
                    eprintln!("quire: load failed ({e}); running without persistence");
                    None
                }
            },
            None => None,
        };
        let persisted = loaded.filter(|s| !s.pages.is_empty());
        let mut restored_settings: HashMap<String, String> = HashMap::new();
        let mut restored_recents: Vec<i32> = Vec::new();
        let mut restored_current: Option<i32> = None;
        if let Some(state0) = &persisted {
            restored_settings = state0
                .settings
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            restored_recents = state0
                .meta
                .get("recents")
                .map(|v| v.split(',').filter_map(|x| x.parse::<i32>().ok()).collect())
                .unwrap_or_default();
            restored_current = state0
                .meta
                .get("current-page")
                .and_then(|v| v.parse::<i32>().ok());
        }

        let (workspace, mut doc, page_order, seed) = match &persisted {
            Some(state) => {
                let (ws, orders) = workspace_from_persisted(state);
                let mut d = Document::new(BLOCK_ID_BASE);
                let mut by_page: HashMap<PageId, Vec<Block>> = HashMap::new();
                for b in &state.blocks {
                    by_page.entry(b.page).or_default().push(b.clone());
                }
                for (pid, blocks) in by_page {
                    d.set_page_blocks(pid, blocks);
                }
                (ws, d, orders, Vec::<Vec<Change>>::new())
            }
            None => {
                let (ws, d, orders) = build_mock_session(args);
                (ws, d, orders, Vec::<Vec<Change>>::new())
            }
        };

        // bench content overrides the loaded page (in memory only; never
        // recorded, so scene D stays deterministic across runs)
        if args.blocks > 0 {
            let title = workspace.title_of(PAGE_ATLAS).unwrap_or("").to_string();
            let rows = mock_blocks_bench(args.blocks);
            let mut core_blocks = rows_to_blocks(PAGE_ATLAS, rows, &mut doc);
            bench_pictures(&mut core_blocks, args.pictures);
            bench_marks(&mut core_blocks, args.marks);
            bench_code(&mut core_blocks, args.code);
            doc.set_page_blocks(core_page_id(PAGE_ATLAS), core_blocks);
            let _ = title;
        }

        // with_database_snapshots powers the periodic snapshots (M8, D10):
        // they ride the flush tick with a 10-minute minimum interval
        let persistence = repo.clone().map(|r| {
            Arc::new(
                PersistenceService::with_default_clock(r.clone())
                    .with_database_snapshots(&r),
            )
        });

        // M8_FEEDBACK #9: the panic logger runs before any repository
        // exists, so its facts land in <data dir>/session.meta. This is the
        // first place that holds both sides: copy the entries into the
        // metadata table (one transaction), then consume them from the
        // session file — session.meta is a handoff note to exactly this
        // next session, so anything left would be re-reported forever. An
        // abort summary also becomes the startup notice bar's first line.
        let mut abort_notice: Option<String> = None;
        if let (Some(r), Some(logger)) = (&repo, crate::services::logging::current()) {
            let entries = logger.meta_entries();
            if !entries.is_empty() {
                let mut changes: Vec<Change> = entries
                    .iter()
                    .map(|(k, v)| Change::MetaSet {
                        key: k.clone(),
                        value: v.clone(),
                    })
                    .collect();
                changes.extend(entries.iter().map(|(k, _)| Change::MetaDelete {
                    key: k.clone(),
                }));
                let _ = r.apply(&changes);
                if let Some((_, summary)) = entries.iter().find(|(k, _)| {
                    k == crate::services::logging::KEY_SESSION_ABORTED
                }) {
                    let first_line = summary.lines().next().unwrap_or(summary);
                    abort_notice = Some(format!(
                        "the previous session ended unexpectedly: {first_line}"
                    ));
                }
            }
        }

        let repo_for_state = repo.clone();
        let search_service = repo.map(crate::services::search_service::SearchService::new_arc);

        // Attachments ride the same database. Their rows are read once here —
        // a small table of metadata, not pixels — so painting a realized
        // image row is a map lookup plus (first time only) one decode. A
        // library that predates v7 has no rows at all. A failed read costs
        // the session its pictures, not its documents.
        let store = crate::services::attachment_store::AttachmentStore::for_db(
            repo_for_state.as_ref().and_then(|r| r.path()),
        );
        let mut attachments: BTreeMap<i64, Attachment> = BTreeMap::new();
        let mut next_attachment_id: i64 = 1;
        let mut attachment_notice: Option<String> = None;
        if let Some(r) = &repo_for_state {
            match r.load_attachments() {
                Ok(rows) => {
                    for a in rows {
                        let key = a.id.as_u64() as i64;
                        next_attachment_id = next_attachment_id.max(key + 1);
                        attachments.insert(key, a);
                    }
                }
                Err(e) => {
                    attachment_notice =
                        Some(format!("attachments could not be loaded: {e}"));
                }
            }
        }

        // The media scene's fixtures (SPEC §三十七's "a page of pictures being
        // scrolled"): write the pool the bench page points at, on a fresh
        // library only. The measured pass of a bench run then loads pictures
        // instead of paying for them twice, and `--pictures` with nothing to
        // persist costs the shot tool no files.
        let media_scene = args.pictures > 0 && args.blocks > 0;
        if media_scene && persisted.is_none() && repo_for_state.is_some() {
            let (_, pool) = bench_picture_plan(args.blocks, args.pictures);
            for k in 1..=pool as i64 {
                if let Some(att) = store.create_fixture(AttachmentId(k as u64), 1280, 720) {
                    attachments.insert(k, att);
                    next_attachment_id = next_attachment_id.max(k + 1);
                }
            }
        }

        // fresh database: record the whole session once so a restart
        // reproduces exactly this state
        if let (Some(p), true) = (&persistence, persisted.is_none()) {
            let mut batch = Vec::new();
            for (id, title, parent, favorite, expanded, font, full_width, small_text, icon) in
                workspace.page_seed_rows()
            {
                batch.push(Change::PageCreated(crate::core::Page {
                    id: PageId(id as u32 as u64),
                    title,
                    parent: parent.map(|v| PageId(v as u32 as u64)),
                    order: *page_order.get(&id).unwrap_or(&OrderKey::FIRST),
                    favorite,
                    expanded,
                    font,
                    full_width,
                    small_text,
                    icon,
                    cover: None,
                    locked: false,
                    template: false,
                }));
            }
            // rows before the blocks that point at them
            for a in attachments.values() {
                batch.push(Change::AttachmentAdded(a.clone()));
            }
            for id in workspace.dfs_order() {
                for b in doc.page_blocks(core_page_id(id)) {
                    batch.push(Change::BlockInserted(b.clone()));
                }
            }
            p.record_all(vec![batch]);
            // best effort: a failed first write surfaces on the next flush
            let _ = p.force_flush();
        }
        let _ = seed;

        let open = if args.blocks > 0 {
            PAGE_ATLAS
        } else {
            // reopen the page the last session had open (the "current-page"
            // meta every open_page writes), unless it no longer exists
            restored_current
                .filter(|id| workspace.contains(*id))
                .unwrap_or(PAGE_GETTING_STARTED)
        };
        let all_commands = mock_commands(&workspace);
        // The database layer's schema and its four id watermarks (ADR-0067 /
        // ADR-0072). A session with no file has no databases — and a file whose
        // read fails says so once, in the notice line, rather than refusing to
        // start: a database layer that cannot be read is a page that draws no
        // rows, which is recoverable, while a library that does not open is not.
        let (database_catalog, db_ids, db_notice_read) = match &repo_for_state {
            Some(repo) => match repo.load_databases() {
                Ok(catalog) => {
                    let maximum = |table| repo.max_id(table).unwrap_or(0);
                    let ids = (
                        maximum(crate::storage::database_store::DbTable::Databases) + 1,
                        maximum(crate::storage::database_store::DbTable::Properties) + 1,
                        maximum(crate::storage::database_store::DbTable::Records) + 1,
                        maximum(crate::storage::database_store::DbTable::Views) + 1,
                    );
                    (catalog, ids, None)
                }
                Err(e) => (
                    DatabaseCatalog::default(),
                    (1, 1, 1, 1),
                    Some(format!("database layer unreadable: {e}")),
                ),
            },
            None => (DatabaseCatalog::default(), (1, 1, 1, 1), None),
        };
        let blocks = Rc::new(VecModel::from(Vec::new()));
        let commands = Rc::new(VecModel::from(all_commands.clone()));
        // The named versions this library already carries (SPEC §三十八). Read
        // off the same metadata map the recents come from, because the rows are
        // the index: a file in `versions/` that no row names is invisible until
        // the next save sweeps it away.
        let (version_index, version_pins) = restore_versions(&persisted);
        let state = AppState {
            workspace: RefCell::new(workspace),
            sidebar: Rc::new(VecModel::from(Vec::new())),
            blocks,
            backlinks: Rc::new(VecModel::from(Vec::new())),
            commands,
            search: Rc::new(VecModel::from(Vec::new())),
            menu: Rc::new(VecModel::from(Vec::new())),
            slash: Rc::new(VecModel::from(slash_items(""))),
            block_menu: Rc::new(VecModel::from(Vec::new())),
            versions: Rc::new(VecModel::from(Vec::new())),
            version_diff: Rc::new(VecModel::from(Vec::new())),
            clipboard: RefCell::new(None),
            settings: RefCell::new(restored_settings),
            recents_restored: Cell::new(restored_recents),
            ui: RefCell::new(None),
            db_notice: RefCell::new(
                abort_notice
                    .into_iter()
                    .chain(attachment_notice)
                    .chain(db_notice_read)
                    .collect(),
            ),
            all_commands,
            doc: RefCell::new(doc),
            history: RefCell::new(History::default()),
            persistence,
            repo: repo_for_state,
            search_service,
            store,
            attachments: RefCell::new(attachments),
            attachment_images: RefCell::new(BTreeMap::new()),
            attachment_cache_bytes: Cell::new(0),
            attachment_cache_peak: Cell::new(0),
            attachment_tick: Cell::new(0),
            next_attachment_id: Cell::new(next_attachment_id),
            pending_search: RefCell::new(None),
            search_generation: Cell::new(0),
            find_session: RefCell::new(None),
            find_label: RefCell::new(String::new()),
            find_painted: RefCell::new(Vec::new()),
            backlinks_expanded: Cell::new(false),
            page_order: RefCell::new(page_order),
            flush_hook: RefCell::new(None),
            open_page: Cell::new(0),
            nav: RefCell::new(NavHistory::default()),
            pending_delete: Cell::new(None),
            last_scroll_y: Cell::new(0.0),
            template_pick: Cell::new(0),
            version_index: RefCell::new(version_index),
            version_pins: RefCell::new(version_pins),
            databases: RefCell::new(database_catalog),
            db_windows: RefCell::new(HashMap::new()),
            db_content_stamp: Cell::new(0),
            db_active_view: RefCell::new(HashMap::new()),
            db_anchor: RefCell::new(HashMap::new()),
            db_cal_month: RefCell::new(HashMap::new()),
            db_gallery_per_row: RefCell::new(HashMap::new()),
            db_form: RefCell::new(HashMap::new()),
            db_search: RefCell::new(HashMap::new()),
            db_computed_evals: Cell::new(0),
            editor_viewport_h: Cell::new(DEFAULT_EDITOR_VIEWPORT_H),
            next_db_id: Cell::new(db_ids.0),
            next_property_id: Cell::new(db_ids.1),
            next_record_id: Cell::new(db_ids.2),
            next_view_id: Cell::new(db_ids.3),
        };
        // restore persisted recents before the first open marks its page
        let state = Rc::new(state);
        {
            let recents = state.recents_restored.take();
            if !recents.is_empty() {
                state.workspace.borrow_mut().set_recents(recents);
            }
        }
        // The library exists before the first menu is filled: a template the
        // user inserts is read out of `doc`, so seeding here is what lets the
        // same start offer the built-ins and use one.
        state.seed_builtin_templates();
        state.open_page(open);
        state
    }

    // ---- models ----

    pub fn sidebar_model(&self) -> ModelRc<SidebarNode> {
        ModelRc::from(self.sidebar.clone())
    }
    pub fn blocks_model(&self) -> ModelRc<BlockRow> {
        ModelRc::from(self.blocks.clone())
    }
    pub fn backlinks_model(&self) -> ModelRc<BacklinkRow> {
        ModelRc::from(self.backlinks.clone())
    }
    pub fn commands_model(&self) -> ModelRc<CommandRow> {
        ModelRc::from(self.commands.clone())
    }
    pub fn search_model(&self) -> ModelRc<SearchRow> {
        ModelRc::from(self.search.clone())
    }
    pub fn menu_model(&self) -> ModelRc<MenuRow> {
        ModelRc::from(self.menu.clone())
    }
    pub fn slash_model(&self) -> ModelRc<SlashRow> {
        ModelRc::from(self.slash.clone())
    }
    pub fn block_menu_model(&self) -> ModelRc<MenuRow> {
        ModelRc::from(self.block_menu.clone())
    }
    pub fn versions_model(&self) -> ModelRc<VersionRow> {
        ModelRc::from(self.versions.clone())
    }
    pub fn version_diff_model(&self) -> ModelRc<DiffRow> {
        ModelRc::from(self.version_diff.clone())
    }

    // ---- projections ----

    /// Rebuild the flat sidebar model: Favorites, Recent, Workspace tree,
    /// and the trailing "New page" action row. `row.y` is the cumulative
    /// pixel offset inside the tree area (used to anchor the context menu).
    pub fn rebuild_sidebar(&self) {
        self.sidebar.set_vec(self.build_sidebar_rows());
        let empty = self.workspace.borrow().visible_page_count() == 0;
        if let Some(ui) = self.ui.borrow().clone() {
            ui.upgrade().unwrap().set_workspace_empty(empty);
        }
    }

    pub fn build_sidebar_rows(&self) -> Vec<SidebarNode> {
        let ws = self.workspace.borrow();
        let open = self.open_page.get();
        let mut rows: Vec<SidebarNode> = Vec::new();
        let mut y = 0;

        let push = |rows: &mut Vec<SidebarNode>, y: &mut i32, node: SidebarNode| {
            let mut n = node;
            n.y = *y;
            *y += if n.kind == "header" { 26 } else { 28 };
            rows.push(n);
        };

        let favorites = ws.favorites();
        if !favorites.is_empty() {
            push(&mut rows, &mut y, header("Favorites"));
            for (id, title) in favorites {
                push(
                    &mut rows,
                    &mut y,
                    leaf_row(id, &title, "favorite", false, icon_mark(&ws, id)),
                );
            }
        }
        let recents = ws.recents();
        if !recents.is_empty() {
            push(&mut rows, &mut y, header("Recent"));
            for (id, title) in recents.iter().take(MAX_RECENTS) {
                push(
                    &mut rows,
                    &mut y,
                    leaf_row(*id, title, "recent", false, icon_mark(&ws, *id)),
                );
            }
        }

        push(
            &mut rows,
            &mut y,
            SidebarNode {
                id: WORKSPACE_HEADER_ID,
                label: "Workspace".into(),
                kind: "header".into(),
                depth: 0,
                expanded: false,
                has_children: false,
                selected: false,
                y: 0,
                icon: "".into(),
            },
        );
        for r in ws.tree_rows() {
            let icon = icon_slot(&ws, r.id, &r.label);
            rows.push(SidebarNode {
                id: r.id,
                icon,
                label: r.label.into(),
                kind: "page".into(),
                depth: r.depth,
                expanded: r.expanded,
                has_children: r.has_children,
                selected: r.id == open,
                y: y,
            });
            y += 28;
        }
        // trailing "new page" action row
        rows.push(SidebarNode {
            id: ROW_NEW_PAGE,
            label: "New page".into(),
            kind: "new-page".into(),
            depth: 0,
            expanded: false,
            has_children: false,
            selected: false,
            y: y,
            icon: "".into(),
        });
        rows
    }

    pub fn sidebar_row_y(&self, id: i32) -> i32 {
        self.sidebar
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.y)
            .unwrap_or(0)
    }

    // ---- operations (called by the controller) ----

    /// Record that the user is moving from the page they had open to another
    /// one, so Go Back can retrace it (SPEC §十六).
    pub fn nav_record(&self, from: i32, to: i32) {
        self.nav.borrow_mut().record(from, to);
    }

    /// The page to navigate to, stepping back (or forward) through the
    /// session history; `None` when that direction is empty. `open_page` is
    /// still the page being left when this is called, which is what makes it
    /// the entry pushed onto the opposite stack.
    pub fn nav_step(&self, forward: bool) -> Option<i32> {
        let current = self.open_page.get();
        self.nav
            .borrow_mut()
            .step(forward, current, |id| {
                self.workspace.borrow().contains(id)
            })
    }

    pub fn open_page(&self, id: i32) {
        {
            let mut ws = self.workspace.borrow_mut();
            if !ws.contains(id) {
                return;
            }
            // A template has no door in (SPEC §三十八). It is not a list that
            // happens to hide the page: opening one would put it in `recents`
            // and in the `current-page` meta, which are two more places a
            // template must not appear. The template's own body is read through
            // `insert_template`, which never needs it open.
            if ws.is_template(id) {
                return;
            }
            ws.mark_opened(id);
            ws.expand_ancestors(id);
        }
        // persist the recent list + last-opened page
        self.record(vec![Change::MetaSet {
            key: "current-page".into(),
            value: id.to_string(),
        }]);
        let recents = self.workspace.borrow().recents_ids();
        self.record(vec![Change::MetaSet {
            key: "recents".into(),
            value: recents
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
        }]);
        self.open_page.set(id);
        self.apply_page_style();
        self.reproject_blocks();
        // a fresh page starts at the top (the old viewport offset would
        // otherwise leak across pages)
        if let Some(ui) = self.ui.borrow().clone() {
            ui.upgrade()
                .unwrap()
                .set_editor_scroll_y(0.0);
        }
        self.rebuild_sidebar();
    }

    /// Rebuild the editor rows from the Document (page switch, undo/redo,
    /// structural edits). Typing never goes through here.
    pub fn reproject_blocks(&self) {
        let page = self.open_page.get();
        let mut rows = {
            let doc = self.doc.borrow();
            let hits = self.find_hits();
            let blocks = doc.page_blocks(core_page_id(page));
            // the mention chips read titles that live in the workspace, not in
            // the block, so the projection is handed them as it builds
            let titles = {
                let ws = self.workspace.borrow();
                MentionTitles::of_blocks(blocks, |id| ws.title_of(id).map(str::to_string))
            };
            project_blocks(blocks, &hits, &titles)
        };
        // a Page or Link block shows the target page's live title, not stale
        // text; a reference whose page is gone reads as deleted
        let ws = self.workspace.borrow();
        for row in rows
            .iter_mut()
            .filter(|r| r.kind == BLOCK_PAGE || r.kind == BLOCK_LINK)
        {
            row.text = ws.title_of(row.page_ref).unwrap_or(DELETED_PAGE_LABEL).into();
        }
        // A mirror draws what its source holds, and it is read here rather
        // than stored there (ADR-0052 §1) — for the same reason a mention
        // stores an id instead of a title. It cannot move up into
        // `project_blocks`: that function only ever sees the open page's slice
        // of the document, and the whole point of the feature is the source
        // being somewhere else.
        {
            let doc = self.doc.borrow();
            let ws_titles = self.workspace.borrow();
            for row in rows.iter_mut().filter(|r| r.kind == BLOCK_SYNCED) {
                let target = if row.sync_source > 0 {
                    sync_target(&doc, BlockId(row.sync_source as u64))
                } else {
                    None
                };
                match target {
                    Some(src) => {
                        row.text = src.text.clone().into();
                        // Its marks come along: a mention inside mirrored
                        // prose is still a mention, and still reads today's
                        // title rather than the one it was written with.
                        let titles = MentionTitles::of_blocks(
                            std::slice::from_ref(src),
                            |id| ws_titles.title_of(id).map(str::to_string),
                        );
                        row.runs = runs_to_model(src, &[], &titles);
                        row.sync_source = src.id.0 as i32;
                    }
                    None => {
                        // Nothing to draw and nothing to edit through. -1 is
                        // read by the row as "unbound", which makes it read
                        // only: an edit aimed at a source nobody can find has
                        // nowhere honest to land (ADR-0052 §2).
                        row.text = DELETED_SOURCE_LABEL.into();
                        row.runs = ModelRc::default();
                        row.sync_source = -1;
                    }
                }
            }
        }
        drop(ws);
        // SPEC §三十九: a `Database` block's row is the one thing this
        // projection cannot build from blocks alone — its rows are records in
        // six tables, so they come from the state's realized window and the
        // window comes from `core::database::window`. The guard is the same
        // shape as the table's and the toc's: the walk happens only for the rows
        // that want it, so a page with one database pays one read and a page
        // without one pays nothing.
        for row in rows.iter_mut().filter(|r| r.kind == BLOCK_DATABASE) {
            self.db_fill_row(row);
        }
        self.blocks.set_vec(rows);
        // Every row was just rebuilt with the current hits in it, so the list
        // the next search un-paints from has to say the same.
        *self.find_painted.borrow_mut() = self.find_hit_rows();
        self.refresh_backlinks();
        self.update_page_stats();
    }

    /// Rebuild the backlink panel for the open page (SPEC §四十, ADR-0051).
    ///
    /// **Two reads, and on the common page only one of them runs.** The count
    /// is an index walk, and the window is fetched only when the count is
    /// non-zero — so a page nobody quotes pays one `count(*)` and a page with
    /// 200 references still draws only `BACKLINK_WINDOW` rows of text.
    /// Migration 16's two indexes are what make both reads seeks; that is the
    /// whole reason this layer has no derived table to keep in step.
    ///
    /// A read that *fails* degrades to an empty panel rather than an error
    /// banner: this runs on every projection, and the same failure has already
    /// been reported by the load that produced the page (see `open`).
    pub fn refresh_backlinks(&self) {
        let page = core_page_id(self.open_page.get());
        let expanded = self.backlinks_expanded.get();
        let window = if expanded {
            BACKLINK_EXPANDED
        } else {
            BACKLINK_WINDOW
        };
        let (total, refs) = match self.repo.as_ref() {
            None => (0, Vec::new()),
            Some(repo) => {
                let total = repo.reference_count(page).unwrap_or(0);
                let refs = if total == 0 {
                    Vec::new()
                } else {
                    repo.references(page, window).unwrap_or_default()
                };
                (total, refs)
            }
        };
        self.backlinks.set_vec(self.backlink_rows(&refs));
        if let Some(ui) = self.ui.borrow().clone() {
            let g = ui.upgrade().unwrap();
            g.set_backlink_total(total as i32);
            g.set_backlink_count_label(backlink_count_label(total).into());
            g.set_backlink_fold_label(
                backlink_fold_label(total, refs.len(), expanded).into(),
            );
            // The fold state travels with the window: Rust chose the window
            // from it, so telling the UI the flag *after* the rows is what
            // keeps the header and the list describing the same thing.
            g.set_backlinks_expanded(expanded);
        }
    }

    /// Turn the repository's references into panel rows, adding the two things
    /// only the workspace can answer: the source page's name *as it is now*,
    /// and whether this row opens a new group.
    fn backlink_rows(&self, refs: &[crate::storage::backlinks::Reference]) -> Vec<BacklinkRow> {
        let ws = self.workspace.borrow();
        let mut prev: Option<PageId> = None;
        refs.iter()
            .map(|r| {
                let first_on_page = prev != Some(r.page);
                prev = Some(r.page);
                BacklinkRow {
                    block: r.block.0 as i32,
                    page: r.page.as_u64() as i32,
                    // A page that cannot be named is one this workspace does
                    // not have — a library edited underneath us, or a block
                    // that outlived its page. It reads as deleted, the same
                    // words a dangling mention chip uses, because it is the
                    // same fact.
                    title: ws
                        .title_of(r.page.as_u64() as i32)
                        .map(str::to_string)
                        .unwrap_or_else(|| DELETED_PAGE_LABEL.to_string())
                        .into(),
                    text: r.text.trim().into(),
                    block_level: r.block_level,
                    first_on_page,
                }
            })
            .collect()
    }

    /// Fold or unfold the backlink panel. The only thing that changes is how
    /// many rows are asked for, so the refresh is the same read one size up.
    pub fn toggle_backlinks(&self) {
        self.backlinks_expanded.set(!self.backlinks_expanded.get());
        self.refresh_backlinks();
    }

    /// The block a backlink row names. The panel hands it to the same
    /// `quire://block/` path a link in the text walks, so a reference on
    /// another page opens that page first and a reference on this one does
    /// not — one jump rule for both, because they are the same kind of jump.
    pub fn backlink_target(&self, block: i32) -> Option<i32> {
        let doc = self.doc.borrow();
        doc.block(BlockId(block as u32 as u64))
            .filter(|b| self.workspace.borrow().contains(b.page.0 as i32))
            .map(|b| b.id.0 as i32)
    }

    /// Controller installs the UI weak handle at wire time.
    pub fn set_ui(&self, ui: slint::Weak<crate::UIState<'static>>) {
        *self.ui.borrow_mut() = Some(ui);
    }

    /// Word/char counts for the editor footer (display-only push).
    pub fn update_page_stats(&self) {
        let page = core_page_id(self.open_page.get());
        let (words, chars) = {
            let doc = self.doc.borrow();
            let mut words = 0;
            let mut chars = 0;
            for b in doc.page_blocks(page) {
                // An attachment's text is the file name: on a picture row it
                // is invisible metadata, on a file row it is a label rather
                // than prose. Neither belongs in the word count.
                if matches!(
                    b.kind,
                    crate::core::BlockKind::Image | crate::core::BlockKind::File
                ) {
                    continue;
                }
                let t = b.text.trim();
                if !t.is_empty() {
                    words += t.split_whitespace().count();
                }
                chars += b.text.chars().count();
            }
            (words, chars)
        };
        if let Some(ui) = self.ui.borrow().clone() {
            let g = ui.upgrade().unwrap();
            g.set_page_stats(format!("{words} words · {chars} chars").into());
        }
    }

    /// Controller installs the UI weak handle at wire time.

    /// Poll the in-flight search; called by the controller on a short
    /// timer while a query is pending. Returns rows when a result landed.
    fn next_search_generation(&self) -> u64 {
        let g = self.search_generation.get() + 1;
        self.search_generation.set(g);
        g
    }

    pub fn poll_search(&self) -> Option<Vec<SearchRow>> {
        let (gen, pending) = self.pending_search.borrow_mut().take()?;
        if gen != self.search_generation.get() {
            return Some(Vec::new()); // superseded: show nothing
        }
        match pending.poll() {
            Some(Ok(hits)) => {
                let rows: Vec<SearchRow> = hits
                    .iter()
                    .map(|h| SearchRow {
                        page_id: h.page.0 as i32,
                        title: h.title.clone().into(),
                        snippet: if h.snippet.is_empty() {
                            self.workspace.borrow().breadcrumb(h.page.0 as i32).into()
                        } else {
                            h.snippet.clone().into()
                        },
                        hit_block_id: h.block.map(|b| b.0 as i32).unwrap_or(-1),
                    })
                    .collect();
                Some(rows)
            }
            Some(Err(_)) => Some(Vec::new()),
            None => {
                // still running: put it back
                *self.pending_search.borrow_mut() = Some((gen, pending));
                None
            }
        }
    }

    pub fn search_in_flight(&self) -> bool {
        self.pending_search.borrow().is_some()
    }

    /// Blocking search for tools/headless scenes (the GUI path is async).
    pub fn set_search_rows_sync(&self, query: &str) {
        if let Some(svc) = &self.search_service {
            if !query.trim().is_empty() {
                let rows: Vec<SearchRow> = match svc.query(query) {
                    Ok(hits) => hits
                        .iter()
                        .map(|h| SearchRow {
                            page_id: h.page.0 as i32,
                            title: h.title.clone().into(),
                            snippet: if h.snippet.is_empty() {
                                self.workspace.borrow().breadcrumb(h.page.0 as i32).into()
                            } else {
                                h.snippet.clone().into()
                            },
                            hit_block_id: h.block.map(|b| b.0 as i32).unwrap_or(-1),
                        })
                        .collect(),
                    Err(_) => Vec::new(),
                };
                self.search.set_vec(rows);
                return;
            }
        }
        self.set_search_query(query);
    }

    pub fn set_db_notice(&self, notice: String) {
        self.db_notice.borrow_mut().push(notice);
    }

    /// Drain everything queued as one line. Startup can stack several facts
    /// (the previous session aborted, a backup was restored, the library
    /// moved); they read better joined than overwriting each other.
    pub fn take_db_notice(&self) -> Option<String> {
        let queue = self.db_notice.borrow_mut();
        if queue.is_empty() {
            return None;
        }
        let mut line = queue.join("; ");
        if !line.ends_with('.') {
            line.push('.');
        }
        Some(line)
    }

    /// Say one thing in the notice line, **now**: [`AppState::note_locked`]'s
    /// split for the same reason — a window is what the user is looking at, so
    /// the bar is written directly; with none attached the queue is both the
    /// memory and the proof (a headless session can only assert what it can
    /// read back).
    ///
    /// The queue alone is not enough for a write that *succeeded*: unlike a
    /// formula draft, whose error line the editor is already showing, a column
    /// type that moved has nothing on screen but the notice to say what
    /// happened to the values under it.
    fn db_say(&self, line: String) {
        match self.ui.borrow().clone().and_then(|u| u.upgrade()) {
            Some(g) => g.set_db_notice(line.into()),
            None => self.db_notice.borrow_mut().push(line),
        }
    }

    pub fn page_order_of(&self, id: i32) -> OrderKey {
        *self
            .page_order
            .borrow()
            .get(&id)
            .unwrap_or(&OrderKey::FIRST)
    }

    /// Queue a change batch for the debounced flush and arm the timer.
    pub fn record(&self, changes: Vec<Change>) {
        if changes.is_empty() {
            return;
        }
        // SPEC §三十九: the database layer's *schema* has a second reader — the
        // in-memory catalog — and this is the one funnel every change list
        // passes through, in both directions (`exec` and `undo`/`redo` hand back
        // the list as it was applied). Learning it here rather than at each call
        // site is what keeps the catalog from drifting: a `MakeDatabase` arrives
        // as `DatabaseCreated` + `PropertyAdded` + `ViewAdded`, its undo as the
        // three deletions, and no call site has to remember to say so.
        self.db_absorb(&changes);
        // …and the window cache's content half is spent with it (see
        // `db_content_stamp`): whatever the change was, what a database cell
        // paints may have moved, and no other part of the cache key can see it.
        self.db_content_stamp.set(self.db_content_stamp.get() + 1);
        if let Some(p) = &self.persistence {
            p.record(changes);
        }
        if let Some(hook) = &*self.flush_hook.borrow() {
            hook();
        }
    }

    /// Controller installs the flush-timer trigger at wiring time.
    pub fn install_flush_hook(&self, hook: Box<dyn Fn()>) {
        *self.flush_hook.borrow_mut() = Some(Rc::new(hook));
    }

    /// Write everything queued so far (Ctrl+S, app exit).
    pub fn persistence_force_flush(&self) {
        if let Some(p) = &self.persistence {
            // errors surface through take_last_error on the next call;
            // nothing actionable at the exit path
            let _ = p.force_flush();
        }
    }

    /// Does the page on screen refuse edits right now (SPEC §三十八 "lock")?
    /// One question, asked at the one funnel every document command passes,
    /// rather than at each of the ~70 call sites that would otherwise have to
    /// remember it.
    pub fn page_locked(&self) -> bool {
        self.workspace.borrow().locked_of(self.open_page.get())
    }

    /// Say out loud that a lock just swallowed something. SPEC §三十八 asks for
    /// the refusal to be visible, and a silently-ignored click is the failure
    /// mode that section names. The controller calls it for the entry points
    /// that never reach a command (a click on a row, a drag hover).
    ///
    /// What this deduplicates against is what the user can see. The notice bar
    /// is sticky until dismissed, so a bar already carrying the line is a
    /// visible refusal — and the drag-hover path answers once per frame of one
    /// gesture, so writing it again would be its own defect. Without a window
    /// the queue plays both roles: it remembers the refusal, and in a headless
    /// session it is the only way one can be proved at all.
    pub fn note_locked(&self) {
        // `…` and not `⋯`: U+22EF is outside Segoe UI's face, so the midline
        // dots painted as a hole where the button's name should be.
        const LINE: &str = "This page is locked — … → Unlock page to edit.";
        match self.ui.borrow().clone().and_then(|ui| ui.upgrade()) {
            Some(g) => {
                if g.get_db_notice().as_str() != LINE {
                    g.set_db_notice(LINE.into());
                }
            }
            None => {
                if self.db_notice.borrow().last().is_some_and(|last| last == LINE) {
                    return;
                }
                self.set_db_notice(LINE.to_string());
            }
        }
    }

    /// The one-line form every write entry point uses: refuse, and say why.
    /// Returns true when the caller must not touch the document.
    fn locked_refusal(&self) -> bool {
        if !self.page_locked() {
            return false;
        }
        self.note_locked();
        true
    }

    /// Run a command without reprojecting (typing): the caller keeps the
    /// delegate alive and syncs the single row itself.
    pub fn exec_editor(&self, cmd: Command) -> Option<Vec<Change>> {
        // The one command a locked page still runs: folding changes what is on
        // screen, not what the document says (§三十七 files it as persisted
        // view state, and `Change::BlockFoldedSet` is why it reaches storage at
        // all). Locking a page must not cost the user its outline.
        if !matches!(cmd, Command::ToggleFold { .. }) && self.locked_refusal() {
            return None;
        }
        let page = core_page_id(self.open_page.get());
        let changes = crate::core::command::exec(
            &mut self.doc.borrow_mut(),
            &mut self.history.borrow_mut(),
            page,
            cmd,
        )?;
        self.record(changes.clone());
        Some(changes)
    }

    /// Run a command and refresh the editor rows (structural edits).
    pub fn exec_on_open_page(&self, cmd: Command) -> Option<Vec<Change>> {
        let changes = self.exec_editor(cmd)?;
        self.reproject_blocks();
        Some(changes)
    }

    /// Translate a drag landing from the editor row the pointer is over into
    /// the position `MoveBlockTo` counts in. The two agree while nothing is
    /// folded; a collapsed subtree takes its rows out from under the count.
    pub fn drop_index_for_row(&self, row: i32, below: bool) -> Option<i32> {
        let doc = self.doc.borrow();
        let blocks = doc.page_blocks(core_page_id(self.open_page.get()));
        let model = *visible_block_indices(blocks).get(usize::try_from(row).ok()?)?;
        Some((model + below as usize) as i32)
    }

    /// Read-only check whether a drag landing is valid (hover feedback must
    /// not mutate the document).
    pub fn can_move_block_to(&self, id: i32, index: i32) -> bool {
        // A locked page shows no drop line at all: the hover path answers this
        // once per frame of the gesture, so it stays silent — the drop that
        // follows is the moment worth telling the user about.
        if self.page_locked() {
            return false;
        }
        let page = core_page_id(self.open_page.get());
        crate::core::command::can_move_block_to(
            &self.doc.borrow(),
            page,
            BlockId(id as u64),
            index,
        )
    }

    /// Set the theme and persist it (settings table; key spelling matches
    /// services/settings_store.rs `Settings::KEY_THEME`).
    pub fn set_dark(&self, dark: bool) {
        let value = if dark { "dark" } else { "light" };
        self.settings
            .borrow_mut()
            .insert("theme".into(), value.into());
        self.record(vec![Change::SettingSet {
            key: "theme".into(),
            value: value.into(),
        }]);
    }

    /// Persisted window size (physical px), if a previous session saved one.
    pub fn window_size_setting(&self) -> Option<(f64, f64)> {
        let map = self.settings.borrow();
        let (Some(w), Some(h)) = (map.get("window.w"), map.get("window.h")) else {
            return None;
        };
        match (w.parse::<f64>(), h.parse::<f64>()) {
            (Ok(w), Ok(h)) if w >= 400.0 && h >= 300.0 => Some((w, h)),
            _ => None,
        }
    }

    /// Record the window size as settings changes (flushed with the batch).
    pub fn record_window_size(&self, w: f64, h: f64) {
        self.settings
            .borrow_mut()
            .insert("window.w".into(), format!("{w}"));
        self.settings
            .borrow_mut()
            .insert("window.h".into(), format!("{h}"));
        self.record(vec![
            Change::SettingSet { key: "window.w".into(), value: format!("{w}") },
            Change::SettingSet { key: "window.h".into(), value: format!("{h}") },
        ]);
    }

    /// Read a persisted settings flag (value "1"/"0").
    pub fn setting_flag(&self, key: &str) -> bool {
        self.settings.borrow().get(key).map(|v| v == "1").unwrap_or(false)
    }

    /// Persist a settings flag (one batch, flushed with the session).
    pub fn record_setting(&self, key: &str, value: &str) {
        self.settings
            .borrow_mut()
            .insert(key.into(), value.into());
        self.record(vec![Change::SettingSet {
            key: key.into(),
            value: value.into(),
        }]);
    }

    pub fn dark_setting(&self) -> bool {
        self.settings
            .borrow()
            .get("theme")
            .map(|v| v == "dark")
            .unwrap_or(false)
    }

    // ---- in-page find (Ctrl+F; data layer = Track B's FindSession) ----

    /// The session's hits grouped by the block that carries them, which is the
    /// shape a projection reads. An empty map while the bar is closed costs a
    /// row one failed lookup, so a search that never started stays free.
    fn find_hits(&self) -> FindHits {
        let mut map: FindHits = HashMap::new();
        if let Some(session) = self.find_session.borrow().as_ref() {
            for hit in session.hits() {
                map.entry(hit.block.0 as i32)
                    .or_default()
                    .push((hit.start, hit.end));
            }
        }
        map
    }

    /// (Re)build the session for `term` over the open page's blocks.
    pub fn find_start(&self, term: &str) {
        let page = core_page_id(self.open_page.get());
        let blocks = self.doc.borrow().page_blocks(page).to_vec();
        let session = FindSession::new(term, &blocks);
        let label = if session.is_empty() {
            "no matches".to_string()
        } else {
            format!("0 / {}", session.total())
        };
        *self.find_label.borrow_mut() = label;
        *self.find_session.borrow_mut() = Some(session);
        self.paint_find_hits();
    }

    /// Step to the next/previous hit. Returns (block id as i32, start, end)
    /// for the UI to select, plus refreshes the position label.
    pub fn find_step(&self, next: bool) -> Option<(i32, usize, usize)> {
        let mut slot = self.find_session.borrow_mut();
        let session = slot.as_mut()?;
        let hit = if next { session.next() } else { session.prev() }?;
        let position = session.position()?;
        let total = session.total();
        drop(slot);
        *self.find_label.borrow_mut() = format!("{}/{}", position + 1, total);
        Some((hit.block.0 as i32, hit.start, hit.end))
    }

    pub fn find_label(&self) -> String {
        self.find_label.borrow().clone()
    }

    pub fn find_close(&self) {
        *self.find_session.borrow_mut() = None;
        *self.find_label.borrow_mut() = String::new();
        self.paint_find_hits();
    }

    /// The rows a current hit sits in, sorted and deduped — a table with ten
    /// matching cells is still one row to repaint.
    fn find_hit_rows(&self) -> Vec<i32> {
        let hits = self.find_hits();
        let doc = self.doc.borrow();
        let blocks = doc.page_blocks(core_page_id(self.open_page.get()));
        let mut ids: Vec<i32> = hits
            .keys()
            .map(|id| row_id_of(blocks, BlockId(*id as u64)))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Push the bar's hits into the rows that carry them, and pull the last
    /// search's out of the rows that carried those. A re-projection would
    /// rebuild every row of a 10 000-block page to repaint a dozen of them, and
    /// the bar does this on every keystroke.
    fn paint_find_hits(&self) {
        let hits = self.find_hits();
        let rows_with_hits = self.find_hit_rows();
        let mut ids: Vec<i32> = rows_with_hits.clone();
        ids.extend(self.find_painted.borrow().iter().copied());
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return;
        }
        let mut painted = Vec::new();
        {
            let doc = self.doc.borrow();
            let blocks = doc.page_blocks(core_page_id(self.open_page.get()));
            // the painted rows carry mention chips too, so their labels have to
            // come from the same place the full projection's do
            let titles = {
                let ws = self.workspace.borrow();
                MentionTitles::of_blocks(blocks, |id| ws.title_of(id).map(str::to_string))
            };
            for i in 0..self.blocks.row_count() {
                let Some(mut row) = self.blocks.row_data(i) else {
                    continue;
                };
                if ids.binary_search(&row.id).is_err() {
                    continue;
                }
                let Some(b) = blocks.iter().find(|x| x.id.0 as i32 == row.id) else {
                    continue;
                };
                row.runs = runs_to_model(b, hits_of(&hits, b.id), &titles);
                // A cell or a layout's block has no row of its own, so its hit
                // rides on the row that draws it -- which is the row this walk
                // just landed on, and it has to be rebuilt whole.
                if b.kind == BlockKind::Table {
                    let cells = table_cells(blocks, b, &hits, &titles);
                    row.table_cells = slint::ModelRc::from(Rc::new(VecModel::from(cells)));
                } else if b.kind == BlockKind::Columns {
                    let (items, boxes) = column_projection(blocks, b, &hits, &titles);
                    row.column_items = slint::ModelRc::from(Rc::new(VecModel::from(items)));
                    row.column_boxes = slint::ModelRc::from(Rc::new(VecModel::from(boxes)));
                }
                if rows_with_hits.binary_search(&row.id).is_ok() {
                    painted.push(row.id);
                }
                self.blocks.set_row_data(i, row);
            }
        }
        *self.find_painted.borrow_mut() = painted;
    }

    // ---- slash menu (descriptors owned by Rust, per SPEC §十五) ----

    /// Filter the block-kind descriptors by the text after "/", then append the
    /// templates that match too: typing `/meet` should offer both a Heading and
    /// the "Meeting notes" body, because from the user's side a template is just
    /// another thing that can appear on this line.
    pub fn open_slash(&self, filter: &str) {
        let needle = filter.to_lowercase();
        let mut rows: Vec<SlashRow> = SLASH_ITEMS
            .iter()
            .filter(|(_, label, _)| needle.is_empty() || label.to_lowercase().contains(&needle))
            .map(|(kind, label, hint)| SlashRow {
                id: kind_to_int(*kind),
                label: (*label).into(),
                hint: (*hint).into(),
                disabled: false,
            })
            .collect();
        rows.append(&mut self.template_slash_rows(&needle));
        self.slash.set_vec(rows);
    }

    /// The slash popup's template tail: one row per template whose name matches
    /// `needle`, id-encoded as `TEMPLATE_SLASH_BASE + index` into
    /// `template_list()` so the click handler can name the same row back.
    ///
    /// The index rather than the page id, because a row's id must survive being
    /// clicked *after* the library changed, and because `TEMPLATE_SLASH_BASE` is
    /// far above any block-kind int — which is what lets `slash_selected_kind`
    /// tell "this row is a template" from "this row is a kind it has no entry
    /// for". `hint` is the literal word: a template has no breadcrumb to show,
    /// it is not in the tree.
    fn template_slash_rows(&self, needle: &str) -> Vec<SlashRow> {
        self.template_list()
            .into_iter()
            .enumerate()
            .filter(|(_, (_, title))| needle.is_empty() || title.to_lowercase().contains(needle))
            .map(|(index, (_, title))| SlashRow {
                id: TEMPLATE_SLASH_BASE + index as i32,
                label: title.into(),
                hint: "Template".into(),
                disabled: false,
            })
            .collect()
    }

    /// Fill the "+"-handle insert menu, filtered by `filter` (the block's
    /// whole line in insert mode). Includes the disabled later-milestone
    /// placeholders — see INSERT_ITEMS.
    pub fn open_slash_insert(&self, filter: &str) {
        let needle = filter.to_lowercase();
        let mut rows: Vec<SlashRow> = INSERT_ITEMS
            .iter()
            .filter(|(_, label, _)| needle.is_empty() || label.to_lowercase().contains(&needle))
            .map(|(id, label, hint)| SlashRow {
                id: *id,
                label: (*label).into(),
                hint: (*hint).into(),
                disabled: *id < 0,
            })
            .collect();
        rows.append(&mut self.template_slash_rows(&needle));
        self.slash.set_vec(rows);
    }

    pub fn slash_focus_count(&self) -> i32 {
        self.slash.row_count() as i32
    }

    /// Arrow-key navigation for the slash/insert popup: clamps like before,
    /// but skips the not-selectable placeholder rows.
    pub fn slash_next_focus(&self, current: i32, delta: i32) -> i32 {
        let count = self.slash.row_count() as i32;
        if count == 0 {
            return 0;
        }
        let disabled =
            |i: i32| self.slash.row_data(i as usize).map(|r| r.id < 0).unwrap_or(true);
        let mut next = (current + delta).clamp(0, count - 1);
        if delta != 0 {
            let step = delta.signum();
            let mut guard = 0;
            while disabled(next) && guard < count {
                let candidate = next + step;
                if candidate < 0 || candidate >= count {
                    break;
                }
                next = candidate;
                guard += 1;
            }
        }
        next
    }

    pub fn slash_selected_kind(&self, focus: i32) -> Option<BlockKind> {
        let row = self.slash.row_data(focus.max(0) as usize)?;
        if row.id < 0 {
            // placeholder row (later-milestone kind): nothing to apply
            return None;
        }
        if row.id >= TEMPLATE_SLASH_BASE {
            // A template is not a block kind: `kind_from_int` would fold an
            // unknown number down into Paragraph, which is the one outcome that
            // is silently wrong — the line would become an empty paragraph and
            // the user's words would be gone. The controller asks
            // `slash_selected_template` about these rows instead.
            return None;
        }
        Some(kind_from_int(row.id))
    }

    /// The template behind a focused slash row, or `None` when that row is a
    /// block kind. Returns the template's **page id** plus its name, not an
    /// index: the id is what `insert_template` and `new_page_from_template`
    /// already take, and the name is what a refusal has to quote back, and the
    /// encoding stops at this one function.
    pub fn slash_selected_template(&self, focus: i32) -> Option<(i32, String)> {
        let row = self.slash.row_data(focus.max(0) as usize)?;
        if row.disabled || row.id < TEMPLATE_SLASH_BASE {
            return None;
        }
        self.template_picked(TEMPLATE_PICK_BASE + (row.id - TEMPLATE_SLASH_BASE))
    }

    // ---- block menu ----

    /// Action-id ranges of the ⋮⋮ menu (see `fill_block_menu`).
    pub const MOVE_TO_BASE: i32 = 100_000;
    pub const COLOR_TEXT_BASE: i32 = 200_000;
    pub const COLOR_BG_BASE: i32 = 300_000;
    /// Plus the display width in percent, so the id carries the pick.
    pub const IMAGE_WIDTH_BASE: i32 = 500_000;
    /// Plus the index into `Lang::ALL`, same reason.
    pub const CODE_LANG_BASE: i32 = 600_000;
    /// Touch-only row (see `fill_block_menu`): insert an empty paragraph
    /// below the block and open the "+" insert menu on it.
    pub const INSERT_BELOW_ACTION: i32 = 15;


    /// Fill the handle menu for one block — Notion's ⋮⋮ set, minus the
    /// collab/AI items that are v1 out of scope, plus the move/copy
    /// affordances earlier milestones added. Paste appears only when the
    /// internal clipboard holds a block. Submenus swap the rows and keep
    /// the popup open; the controller routes action ids:
    ///   1..8    root actions + Turn into (7) + Back (8)
    ///   9..14   copy link / Move to / Text color / Background color opens,
    ///           Image width (13, pictures only) and Language (14, code only)
    ///   15      Insert below (touch only)
    ///   100+k   Turn-into target kinds
    ///   MOVE_TO_BASE+pid / COLOR_TEXT_BASE+slot / COLOR_BG_BASE+slot
    ///   IMAGE_WIDTH_BASE+percent / CODE_LANG_BASE+index of `Lang::ALL`
    /// `touch` adds the row that stands in for the desktop's "+" handle: a
    /// finger has no hover, so the handle strip never becomes visible or
    /// tappable on a phone (ANDROID_NOTES), and the long-press that opens this
    /// menu has no "+" half of its own. The controller passes its
    /// `touch-mode`; headless scenes pass `false` so the desktop menu shape is
    /// unchanged everywhere the baseline was shot.
    pub fn fill_block_menu(&self, id: i32, touch: bool) {
        let mut rows = vec![
            row(7, "Turn into", "chevron-right", false, -1, false),
            row(3, "Duplicate", "copy", false, -1, false),
            row(9, "Copy link to block", "link", false, -1, false),
            row(10, "Move to", "arrow-right", false, -1, false),
            row(11, "Text color", "palette", false, -1, false),
            row(12, "Background color", "palette", false, -1, false),
            row(1, "Move up", "chevron-up", false, -1, false),
            row(2, "Move down", "chevron-down", false, -1, false),
            row(4, "Copy block", "copy", false, -1, false),
        ];
        if touch {
            rows.push(row(
                AppState::INSERT_BELOW_ACTION,
                "Insert below",
                "plus",
                false,
                -1,
                false,
            ));
        }
        if self.block_kind(id) == Some(BlockKind::Image) {
            rows.insert(1, row(13, "Image width", "chevron-right", false, -1, false));
        }
        if self.block_kind(id) == Some(BlockKind::Code) {
            rows.insert(1, row(14, "Language", "chevron-right", false, -1, false));
        }
        if self.clipboard.borrow().is_some() {
            rows.push(row(5, "Paste below", "import", false, -1, false));
        }
        rows.push(row(6, "Delete", "trash", true, -1, false));
        // A locked page keeps the two rows that only take information out
        // (SPEC §三十八: "⋮⋮ 的编辑项全部关闭"). They are dropped rather than
        // greyed because this menu has no disabled state, and a row that lies
        // about what it will do is worse than a short menu.
        let locked = self.page_locked();
        if locked {
            rows.retain(|r| r.id == 9 || r.id == 4);
        }
        self.block_menu.set_vec(rows);
    }

    /// Width submenu for a picture. Three stops, not Notion's four: the
    /// editor column is ~780 px, so a "fit content" tier below 25 % would
    /// resolve to a thumbnail nobody can read (SPEC §三十七).
    pub fn fill_block_menu_image_width(&self, id: i32) {
        let current = self
            .doc
            .borrow()
            .block(BlockId(id.max(0) as u64))
            .map(|b| b.img_percent as i32);
        let mut rows = vec![row(8, "Back", "chevron-left", false, -1, false)];
        for percent in [25, 50, 100] {
            let mut menu_row = row(
                AppState::IMAGE_WIDTH_BASE + percent,
                &format!("{percent}%"),
                "",
                false,
                -1,
                false,
            );
            menu_row.check = current == Some(percent);
            rows.push(menu_row);
        }
        self.block_menu.set_vec(rows);
    }

    /// Language submenu for a code block (SPEC §三十七 批次 C). Every language
    /// this build can lex, plus the plain one so a wrong pick is undoable by
    /// picking it. The check marks what the block stores, and stays on the row
    /// the user just clicked: colour is a preview-able property, like the
    /// palette two rows above it.
    pub fn fill_block_menu_code_lang(&self, id: i32) {
        let current = self
            .doc
            .borrow()
            .block(BlockId(id.max(0) as u64))
            .map(|b| b.lang);
        let mut rows = vec![row(8, "Back", "chevron-left", false, -1, false)];
        for (index, lang) in Lang::ALL.iter().enumerate() {
            let mut menu_row = row(
                AppState::CODE_LANG_BASE + index as i32,
                lang.label(),
                "",
                false,
                -1,
                false,
            );
            menu_row.check = current == Some(*lang);
            rows.push(menu_row);
        }
        self.block_menu.set_vec(rows);
    }

    /// Second-level "Turn into" menu for one block: the curated kinds minus
    /// the block's own. Action ids encode the target kind as 100 + kind int.
    pub fn fill_block_menu_turn_into(&self, id: i32) {
        let current = self.block_kind(id);
        let mut rows = vec![row(8, "Back", "chevron-left", false, -1, false)];
        for (kind, label, _) in TURN_INTO_ITEMS {
            if Some(*kind) != current {
                let icon = match kind {
                    BlockKind::Paragraph => "pencil",
                    BlockKind::Callout | BlockKind::Code => "page",
                    BlockKind::Toggle => "chevron-right",
                    BlockKind::Image => "image",
                    BlockKind::File => "page",
                    BlockKind::Table => "minimize",
                    // SPEC §三十九: a database view — the same glyph as the simple
                    // grid, because both are "a table of rows" to a reader who is
                    // picking a block from a list.
                    BlockKind::Database => "table",
                    _ => "minimize",
                };
                rows.push(row(
                    100 + kind_to_int(*kind),
                    label,
                    icon,
                    false,
                    -1,
                    false,
                ));
            }
        }
        self.block_menu.set_vec(rows);
    }

    /// "Move to" submenu: every page except the one the block lives on,
    /// depth-indented with em spaces. Targets are MOVE_TO_BASE + page id.
    pub fn fill_block_menu_move_to(&self) {
        let current = self.open_page.get();
        let mut rows = vec![row(8, "Back", "chevron-left", false, -1, false)];
        let ws = self.workspace.borrow();
        fn walk(
            ws: &Workspace,
            parent: Option<i32>,
            depth: usize,
            skip: i32,
            out: &mut Vec<MenuRow>,
        ) {
            for id in ws.children_of(parent) {
                if id != skip {
                    let indent = "\u{2003}".repeat(depth);
                    let title = ws.title_of(id).unwrap_or("Untitled");
                    out.push(row(
                        AppState::MOVE_TO_BASE + id,
                        format!("{indent}{title}"),
                        "page",
                        false,
                        -1,
                        false,
                    ));
                    walk(ws, Some(id), depth + 1, skip, out);
                }
            }
        }
        walk(&ws, None, 0, current, &mut rows);
        drop(ws);
        if rows.len() == 1 {
            // inert row (id -1 is swallowed by the controller's id guard)
            rows.push(row(-1, "No other page", "page", false, -1, false));
        }
        self.block_menu.set_vec(rows);
    }

    /// Color submenu: the palette as swatch rows. `background` selects the
    /// row-background palette; otherwise the text palette. Picks are
    /// COLOR_TEXT_BASE / COLOR_BG_BASE + palette slot; the current pick
    /// carries a check mark.
    pub fn fill_block_menu_colors(&self, id: i32, background: bool) {
        let current = self
            .doc
            .borrow()
            .block(BlockId(id.max(0) as u64))
            .map(|b| if background { b.background } else { b.color });
        let mut rows = vec![row(8, "Back", "chevron-left", false, -1, false)];
        for (i, kind) in ColorKind::ALL.iter().enumerate() {
            let base = if background { AppState::COLOR_BG_BASE } else { AppState::COLOR_TEXT_BASE };
            let label = match kind {
                ColorKind::Default => "Default",
                ColorKind::Gray => "Gray",
                ColorKind::Brown => "Brown",
                ColorKind::Orange => "Orange",
                ColorKind::Yellow => "Yellow",
                ColorKind::Green => "Green",
                ColorKind::Blue => "Blue",
                ColorKind::Purple => "Purple",
                ColorKind::Pink => "Pink",
                ColorKind::Red => "Red",
            };
            // the check renders as an icon (MenuRow.check): the software
            // renderer has no font fallback, so glyph marks are unreliable
            let mut menu_row = row(
                base + i as i32,
                label,
                "",
                false,
                i as i32,
                background,
            );
            menu_row.check = Some(*kind) == current;
            rows.push(menu_row);
        }
        self.block_menu.set_vec(rows);
    }

    /// The text that "Copy link to block" puts on the clipboard (the block
    /// anchor doubles as a link-mark URL; clicking one jumps in-app, see
    /// the controller's open-link wiring).
    pub fn block_link(&self, id: i32) -> String {
        // a Page/Link row anchors to its TARGET page, so a copied link opens
        // the page everywhere (the in-app resolver jumps to it directly)
        if matches!(
            self.block_kind_of(id),
            Some(BlockKind::Page) | Some(BlockKind::Link)
        ) {
            if let Some(r) = self.block_page_ref(id) {
                return format!("quire://page/{}", r);
            }
        }
        format!("quire://block/{}", id)
    }

    /// Drop a Page/Link block's reference — the Turn-into path leaving those
    /// kinds. A Page's child page survives in the tree, unowned from here on.
    pub fn clear_block_ref(&self, id: i32) {
        if self.locked_refusal() {
            return;
        }
        let change = Change::BlockRefSet {
            id: BlockId(id as u64),
            page: None,
        };
        self.doc.borrow_mut().apply(std::slice::from_ref(&change));
        self.record(vec![change]);
        self.reproject_blocks();
    }

    /// Cross-page move (⋮⋮ "Move to"): one undo step for the whole subtree.
    pub fn move_block_to_page(&self, id: i32, page: i32) -> bool {
        if page == self.open_page.get() {
            return false;
        }
        // The destination is locked as much as the source is: blocks arriving
        // in a read-only page is a write to it, and the menu that offers this
        // has no disabled state to say so with.
        if self.workspace.borrow().locked_of(page) {
            self.note_locked();
            return false;
        }
        self.exec_on_open_page(Command::MoveBlockToPage {
            id: BlockId(id as u64),
            page: PageId(page as u32 as u64),
        })
        .is_some()
    }

    /// One side of the block color pair, from a submenu pick.
    pub fn set_block_color_slot(&self, id: i32, background: bool, slot: i32) {
        let Some(color) = ColorKind::from_slot(slot) else {
            return;
        };
        let bid = BlockId(id as u64);
        let (c, b) = {
            let doc = self.doc.borrow();
            match doc.block(bid) {
                Some(b) => (b.color, b.background),
                None => return,
            }
        };
        let _ = self.exec_on_open_page(Command::SetBlockColor {
            id: bid,
            color: if background { c } else { color },
            background: if background { color } else { b },
        });
    }

    /// Kind of one block (editing-flow decisions: markdown shortcuts,
    /// Turn-into filtering).
    pub fn block_kind(&self, id: i32) -> Option<BlockKind> {
        self.doc.borrow().block(BlockId(id as u64)).map(|b| b.kind)
    }

    /// Where one cell sits in the grid: its table's id, then its row and
    /// column. `None` for anything that is not a cell of a live grid.
    pub fn table_cell_place(&self, cell: i32) -> Option<(i32, usize, usize)> {
        let (grid, at) = self.grid_of_cell(cell)?;
        Some((grid.table, at / grid.cols, at % grid.cols))
    }

    /// The grid a cell belongs to, and the cell's row-major index in it.
    fn grid_of_cell(&self, cell: i32) -> Option<(LiveGrid, usize)> {
        let parent = {
            let d = self.doc.borrow();
            let b = d.block(BlockId(cell.max(0) as u64))?;
            if b.kind != BlockKind::TableCell {
                return None;
            }
            b.parent?
        };
        let grid = self.live_grid(parent.as_u64() as i32)?;
        let at = grid.cells.iter().position(|c| *c == BlockId(cell as u64))?;
        Some((grid, at))
    }

    /// A block as a grid, when it is a table with columns (a table with none
    /// has no shape to edit, and the commands refuse it the same way).
    fn live_grid(&self, table: i32) -> Option<LiveGrid> {
        LiveGrid::of(&self.doc.borrow(), BlockId(table.max(0) as u64))
    }

    /// Tab / Shift-Tab through a grid. Stepping past the last cell appends a
    /// row first — a table grows as far as the typing goes, which is how
    /// Notion's Tab reads (SPEC §三十七 批次 B) — and stepping off the front
    /// stays put. Returns the cell to focus.
    pub fn table_step(&self, cell: i32, delta: i32) -> Option<i32> {
        let (grid, at) = self.grid_of_cell(cell)?;
        let target = at as i64 + delta as i64;
        if target < 0 {
            return None;
        }
        if target as usize >= grid.cells.len() {
            let changes = self.exec_on_open_page(Command::TableAddRow {
                id: BlockId(grid.table as u64),
                row: grid.rows(),
            })?;
            return changes.iter().find_map(|ch| match ch {
                Change::BlockInserted(b) if b.kind == BlockKind::TableCell => {
                    Some(b.id.as_u64() as i32)
                }
                _ => None,
            });
        }
        grid.cells.get(target as usize).map(|c| c.as_u64() as i32)
    }

    /// Where the grid's toolbar lands: the focused cell's row and column when
    /// the focus is a cell of this table, else its last row and column.
    fn table_anchor(&self, grid: &LiveGrid, focus: i32) -> (usize, usize) {
        match self.table_cell_place(focus) {
            Some((owner, row, col)) if owner == grid.table => (row, col),
            _ => (grid.rows().saturating_sub(1), grid.cols.saturating_sub(1)),
        }
    }

    /// The four pills on the grid's toolbar. Adding goes *after* the anchor so
    /// the row under the caret keeps its place; deleting takes the anchor, and
    /// the plan refuses a table's final row or column.
    pub fn table_add_row(&self, table: i32, focus: i32) -> bool {
        let Some(grid) = self.live_grid(table) else { return false };
        let (row, _) = self.table_anchor(&grid, focus);
        self.exec_on_open_page(Command::TableAddRow {
            id: BlockId(table as u64),
            row: (row + 1).min(grid.rows()),
        })
        .is_some()
    }

    pub fn table_add_column(&self, table: i32, focus: i32) -> bool {
        let Some(grid) = self.live_grid(table) else { return false };
        let (_, col) = self.table_anchor(&grid, focus);
        self.exec_on_open_page(Command::TableAddColumn {
            id: BlockId(table as u64),
            col: (col + 1).min(grid.cols),
        })
        .is_some()
    }

    pub fn table_delete_row(&self, table: i32, focus: i32) -> bool {
        let Some(grid) = self.live_grid(table) else { return false };
        let (row, _) = self.table_anchor(&grid, focus);
        self.exec_on_open_page(Command::TableDeleteRow {
            id: BlockId(table as u64),
            row,
        })
        .is_some()
    }

    pub fn table_delete_column(&self, table: i32, focus: i32) -> bool {
        let Some(grid) = self.live_grid(table) else { return false };
        let (_, col) = self.table_anchor(&grid, focus);
        self.exec_on_open_page(Command::TableDeleteColumn {
            id: BlockId(table as u64),
            col,
        })
        .is_some()
    }

    /// The cell's row-major slot in its grid. The coordinate a delete can
    /// reuse to hand the caret back where it was.
    pub fn table_cell_index(&self, cell: i32) -> Option<usize> {
        Some(self.grid_of_cell(cell)?.1)
    }

    /// The grid's cell at row-major slot `at`, clamped into whatever the grid
    /// is now — where focus lands after a delete took the cell under it.
    pub fn table_cell_at(&self, table: i32, at: usize) -> Option<i32> {
        let grid = self.live_grid(table)?;
        let at = at.min(grid.cells.len().saturating_sub(1));
        grid.cells.get(at).map(|c| c.as_u64() as i32)
    }

    /// True when `id` is a table cell — the row the editor must not turn,
    /// split, merge or paste block structure into (SPEC §三十七 批次 B).
    pub fn is_table_cell(&self, id: i32) -> bool {
        self.block_kind(id) == Some(BlockKind::TableCell)
    }

    /// Patch one committed cell's text into the row that carries it. A cell
    /// has no row of its own — its text lives in its table's `table-cells` —
    /// so the targeted row sync the typing flush does for every other block
    /// finds nothing here, and the grid would keep showing the old word the
    /// moment the caret left.
    pub fn sync_cell_text(&self, cell: i32, text: &str) {
        let Some((grid, at)) = self.grid_of_cell(cell) else { return };
        let mut i = 0;
        while let Some(mut row) = self.blocks.row_data(i) {
            if row.id == grid.table {
                let mut cells: Vec<TableCell> = (0..row.table_cells.row_count())
                    .filter_map(|c| row.table_cells.row_data(c))
                    .collect();
                if let Some(c) = cells.get_mut(at) {
                    c.text = text.into();
                    row.table_cells = ModelRc::from(Rc::new(VecModel::from(cells)));
                    self.blocks.set_row_data(i, row);
                }
                return;
            }
            i += 1;
        }
    }

    // ---- columns layout (SPEC §三十七 批次 B) ----

    /// A layout as the editor sees it: its blocks in reading order, the very
    /// list the layout's own row carries as `column-items`. Tab and the text
    /// flush walk that list, so focus and the model can never disagree.
    fn live_layout(&self, layout: i32) -> Option<LiveLayout> {
        LiveLayout::of(&self.doc.borrow(), BlockId(layout.max(0) as u64))
    }

    /// The layout a block belongs to, and the block's slot in it.
    fn layout_of_item(&self, item: i32) -> Option<(LiveLayout, usize)> {
        let owner = {
            let d = self.doc.borrow();
            let mut cur = d.block(BlockId(item.max(0) as u64))?.parent?;
            loop {
                let b = d.block(cur)?;
                if b.kind == BlockKind::Columns {
                    break cur;
                }
                cur = b.parent?;
            }
        };
        let layout = self.live_layout(owner.as_u64() as i32)?;
        let at = layout.items.iter().position(|x| *x == BlockId(item as u64))?;
        Some((layout, at))
    }

    /// Tab / Shift-Tab through a layout. Either end stops rather than leaving
    /// the layout: a box is exited by clicking out, and growing a box on Tab
    /// would invent a shape the reader did not ask for.
    pub fn column_step(&self, item: i32, delta: i32) -> Option<i32> {
        let (layout, at) = self.layout_of_item(item)?;
        let target = at as i64 + delta as i64;
        if target < 0 {
            return None;
        }
        layout.items.get(target as usize).map(|i| i.as_u64() as i32)
    }

    pub fn column_add(&self, layout: i32) -> bool {
        self.exec_on_open_page(Command::ColumnsAddColumn { id: BlockId(layout as u64) })
            .is_some()
    }

    pub fn column_remove(&self, layout: i32) -> bool {
        self.exec_on_open_page(Command::ColumnsDeleteColumn { id: BlockId(layout as u64) })
            .is_some()
    }

    /// Give one of the layout's boxes its first line, and report the block the
    /// caret should land on. A box is the only thing on the page a click cannot
    /// put a caret in, so this is that click's whole job.
    pub fn column_fill(&self, column: i32) -> Option<i32> {
        let changes =
            self.exec_on_open_page(Command::ColumnsAddBlock { id: BlockId(column as u64) })?;
        changes.iter().find_map(|c| match c {
            Change::BlockInserted(b) if b.kind == BlockKind::Paragraph => {
                Some(b.id.as_u64() as i32)
            }
            _ => None,
        })
    }

    /// True when `id` is drawn by a layout's delegate instead of by a row of
    /// its own: it has no row for the targeted sync, the menus or the search
    /// focus to work on (SPEC §三十七 批次 B).
    pub fn is_column_item(&self, id: i32) -> bool {
        self.layout_of_item(id).is_some()
    }

    /// Patch one committed item's text into the layout row that carries it —
    /// the same job `sync_cell_text` does for a grid.
    pub fn sync_column_text(&self, item: i32, text: &str) {
        let Some((layout, at)) = self.layout_of_item(item) else { return };
        let mut i = 0;
        while let Some(mut row) = self.blocks.row_data(i) {
            if row.id == layout.layout {
                let mut items: Vec<ColumnItem> = (0..row.column_items.row_count())
                    .filter_map(|c| row.column_items.row_data(c))
                    .collect();
                if let Some(it) = items.get_mut(at) {
                    it.text = text.into();
                    row.column_items = ModelRc::from(Rc::new(VecModel::from(items)));
                    self.blocks.set_row_data(i, row);
                }
                return;
            }
            i += 1;
        }
    }

    pub fn copy_block(&self, id: i32) {
        if let Some(b) = self.doc.borrow().block(BlockId(id as u64)) {
            *self.clipboard.borrow_mut() = Some(b.clone());
        }
    }

    /// The empty page's front door: put one paragraph on a page that has no
    /// rows, and hand back its id so the caller can land the caret in it.
    /// Every other insert is anchored to an existing block, so until this
    /// existed a page with zero blocks could not be typed into at all.
    pub fn start_page(&self) -> Option<i32> {
        let changes = self.exec_on_open_page(Command::AppendBlock {
            kind: BlockKind::Paragraph,
            text: String::new(),
        })?;
        changes.iter().find_map(|ch| match ch {
            Change::BlockInserted(b) => Some(b.id.0 as i32),
            _ => None,
        })
    }

    pub fn paste_below(&self, id: i32) -> bool {
        if self.locked_refusal() {
            return false;
        }
        let clip = self.clipboard.borrow().clone();
        let Some(c) = clip else { return false };
        // pasting a Page block would share the source's owned child page;
        // land the title as plain text instead. A Link block's target is
        // unowned, so the paste keeps the reference.
        if c.kind == BlockKind::Page {
            return self.exec_on_open_page(Command::InsertBlockAfter {
                id: BlockId(id as u64),
                kind: BlockKind::Paragraph,
                text: c.text,
            })
            .is_some();
        }
        if c.kind == BlockKind::Link {
            let Some(changes) = self.exec_on_open_page(Command::InsertBlockAfter {
                id: BlockId(id as u64),
                kind: BlockKind::Link,
                text: c.text,
            }) else {
                return false;
            };
            let Some(new_id) = changes.iter().find_map(|ch| match ch {
                Change::BlockInserted(b) => Some(b.id),
                _ => None,
            }) else {
                return false;
            };
            let change = Change::BlockRefSet {
                id: new_id,
                page: c.page_ref,
            };
            self.doc.borrow_mut().apply(std::slice::from_ref(&change));
            self.record(vec![change]);
            self.reproject_blocks();
            return true;
        }
        // An attachment block's pointer is shared, not owned: the paste shows
        // the same file from a second row. With no row to point at — an
        // attachment from a database this session never loaded — the name is
        // still worth more as text than as a broken reference.
        if matches!(c.kind, BlockKind::Image | BlockKind::File) {
            let row = c.attachment.and_then(|a| {
                self.attachments
                    .borrow()
                    .get(&(a.as_u64() as i64))
                    .cloned()
            });
            if let Some(att) = row {
                return self.insert_attachment(id, att, c.kind);
            }
            return self
                .exec_on_open_page(Command::InsertBlockAfter {
                    id: BlockId(id as u64),
                    kind: BlockKind::Paragraph,
                    text: c.text,
                })
                .is_some();
        }
        self.exec_on_open_page(Command::InsertBlockAfter {
            id: BlockId(id as u64),
            kind: c.kind,
            text: c.text,
        })
        .is_some()
    }

    // --- attachments (SPEC §三十七 批次 A) ---

    /// Id for the next attachment the session stores. Ids only have to be
    /// unique, not dense, so a cancelled file pick may burn one.
    pub fn claim_attachment_id(&self) -> AttachmentId {
        let id = self.next_attachment_id.get();
        self.next_attachment_id.set(id + 1);
        AttachmentId(id as u64)
    }

    /// The attachment lands as a new block below `after_id` — the "+" menu and
    /// block paste, which mean "a picture/file appears here". `kind` selects
    /// between the two attachment commands; nothing else in the app does.
    pub fn insert_attachment(&self, after_id: i32, attachment: Attachment, kind: BlockKind) -> bool {
        let id = BlockId(after_id.max(0) as u64);
        let cmd = match kind {
            BlockKind::Image => Command::InsertImage { id, attachment: attachment.clone() },
            BlockKind::File => Command::InsertFile { id, attachment: attachment.clone() },
            _ => return false,
        };
        self.run_attachment_command(cmd, attachment)
    }

    /// This block becomes the attachment — slash "/" and Turn into, which
    /// convert the block they were opened on. The id survives, so a row with
    /// children keeps them.
    pub fn set_block_attachment(&self, id: i32, attachment: Attachment, kind: BlockKind) -> bool {
        let bid = BlockId(id.max(0) as u64);
        let cmd = match kind {
            BlockKind::Image => Command::SetBlockImage { id: bid, attachment: attachment.clone() },
            BlockKind::File => Command::SetBlockFile { id: bid, attachment: attachment.clone() },
            _ => return false,
        };
        self.run_attachment_command(cmd, attachment)
    }

    /// A picture off the clipboard (SPEC §三十七 批次 A's last open item). The
    /// bytes are stored exactly like a picked file, and the block follows the
    /// caret: a block with nothing in it *becomes* the picture, a written one
    /// gets the picture below it, so a paste never leaves a stray empty line.
    /// `png` is what `platform::read_clipboard_image` already decoded and
    /// re-encoded; a false return means the page refused it, not the clipboard.
    pub fn paste_image(&self, id: i32, png: &[u8]) -> bool {
        let Ok(att) = self
            .store
            .import_bytes(self.claim_attachment_id(), "Pasted image", png)
        else {
            return false;
        };
        let empty = self
            .doc
            .borrow()
            .block(BlockId(id.max(0) as u64))
            .map(|b| b.text.is_empty())
            .unwrap_or(false);
        if empty {
            self.set_block_attachment(id, att, BlockKind::Image)
        } else {
            self.insert_attachment(id, att, BlockKind::Image)
        }
    }

    /// Both attachment edits put the `attachments` row into the database as
    /// part of the command's own batch, so persistence and undo stay
    /// single-tracked. The in-memory copy is the lookup `image_for` and
    /// `attachment_size` use; a rejected plan leaves a row nothing points at,
    /// which costs a file in the folder and nothing on screen.
    fn run_attachment_command(&self, cmd: Command, attachment: Attachment) -> bool {
        self.attachments
            .borrow_mut()
            .insert(attachment.id.as_u64() as i64, attachment);
        self.exec_on_open_page(cmd).is_some()
    }

    /// One attachment row as this session loaded it. `None` for an id from a
    /// database we never opened — a block copied in from another library,
    /// which has bytes on screen but nothing to hand the system.
    pub fn attachment_row(&self, id: i32) -> Option<Attachment> {
        if id <= 0 {
            return None;
        }
        self.attachments.borrow().get(&(id as i64)).cloned()
    }

    /// "1.4 MiB" for a file row's right-hand label; empty when there is no row
    /// to size. A callback rather than a model field so the string is not
    /// rebuilt for every row on every keystroke (§三十七, ADR-0029).
    pub fn attachment_size(&self, id: i32) -> String {
        let Some(att) = self.attachment_row(id) else {
            return String::new();
        };
        crate::services::attachment_store::format_size(att.bytes).into()
    }

    /// The display width of an image block (25 / 50 / 100, SPEC §三十七).
    pub fn set_image_width(&self, id: i32, percent: i32) -> bool {
        let percent = percent.clamp(1, 100) as u16;
        self.exec_on_open_page(Command::SetImageWidth {
            id: BlockId(id as u64),
            percent,
        })
        .is_some()
    }

    /// The language a code block colours itself with (SPEC §三十七 批次 C). A
    /// command, like the width two lines above: a pick is an edit, and undo has
    /// to be able to take it back.
    pub fn set_code_lang(&self, id: i32, lang: Lang) -> bool {
        self.exec_on_open_page(Command::SetCodeLang {
            id: BlockId(id as u64),
            lang,
        })
        .is_some()
    }

    /// The raster behind an image row, decoded the first time it is asked for.
    /// This is a callback rather than a model field precisely so it is asked
    /// only for rows the ListView realizes: a page with five hundred pictures
    /// holds about ten in memory (§二十二). Slint caches by path, and so do we.
    pub fn image_for(&self, id: i32) -> slint::Image {
        let key = id as i64;
        if id <= 0 {
            return slint::Image::default();
        }
        self.attachment_tick.set(self.attachment_tick.get() + 1);
        let tick = self.attachment_tick.get();
        if let Some(hit) = self.attachment_images.borrow_mut().get_mut(&key) {
            hit.used = tick;
            return hit.image.clone();
        }
        let Some(att) = self.attachments.borrow().get(&key).cloned() else {
            return slint::Image::default();
        };
        let path = self.store.display_path(&att);
        let img = slint::Image::load_from_path(&path).unwrap_or_default();
        // A file that cannot be decoded caches as blank: the row must not
        // re-open it on every repaint, and a zero-size raster costs the budget
        // nothing. Replacing the file on disk needs a restart to show up.
        let size = img.size();
        let bytes = size.width.max(0) as usize * size.height.max(0) as usize * 4;
        self.cache_image(key, CachedImage { used: tick, bytes, image: img.clone() });
        img
    }

    /// Insert, then spend the budget: the least-recently-realized raster goes
    /// first, so a fast scroll through a photo page cannot stack up frames.
    fn cache_image(&self, key: i64, entry: CachedImage) {
        let weight = entry.bytes;
        let mut cache = self.attachment_images.borrow_mut();
        if let Some(old) = cache.insert(key, entry) {
            self.attachment_cache_bytes.set(self.attachment_cache_bytes.get() - old.bytes);
        }
        let mut total = self.attachment_cache_bytes.get() + weight;
        while total > MAX_ATTACHMENT_CACHE_BYTES {
            // A dozen entries live here at most, so scanning for the oldest
            // use beats keeping a second, ordered structure in step.
            let Some((&oldest, _)) = cache.iter().min_by_key(|(_, v)| v.used) else { break };
            total -= cache.remove(&oldest).unwrap().bytes;
        }
        self.attachment_cache_bytes.set(total);
        self.attachment_cache_peak
            .set(self.attachment_cache_peak.get().max(total));
    }

    /// Drop one raster from the decode cache and give its weight back. Only the
    /// reclaim needs it: the budget loop otherwise decides when a picture
    /// leaves, and here the file itself is gone, so a later `image_for` for a
    /// reused id must not find the old bytes still cached (SPEC §三十七,
    /// ADR-0037).
    fn evict_image(&self, key: i64) {
        let Some(old) = self.attachment_images.borrow_mut().remove(&key) else {
            return;
        };
        self.attachment_cache_bytes
            .set(self.attachment_cache_bytes.get() - old.bytes);
    }

    /// One line the bench harness reads from stderr: what the decode cache
    /// holds now and the high-water mark it reached, plus where the scroll got
    /// to — a picture cache that never grew is only a finding if the page
    /// really moved. A constructed ceiling without a reading is a promise.
    pub fn attachment_cache_report(&self) -> String {
        format!(
            "{{\"event\":\"attachment_cache\",\"bytes\":{},\"peak_bytes\":{},\"entries\":{},\"budget\":{},\"scroll_y\":{:.0}}}\n",
            self.attachment_cache_bytes.get(),
            self.attachment_cache_peak.get(),
            self.attachment_images.borrow().len(),
            MAX_ATTACHMENT_CACHE_BYTES,
            self.last_scroll_y.get(),
        )
    }

    /// height / width of the raster `image_for` returns, 0 when there is
    /// nothing to show. The row needs the ratio to size itself before it has
    /// the picture, and `slint::Image`'s size is not readable from .slint.
    pub fn image_aspect(&self, id: i32) -> f32 {
        let size = self.image_for(id).size();
        if size.width <= 0 {
            return 0.0;
        }
        size.height as f32 / size.width as f32
    }

    /// The directory the database lives in (settings storage row). `None`
    /// for a memory-only session.
    pub fn data_dir(&self) -> Option<String> {
        self.repo.as_ref().and_then(|r| {
            r.path()
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
        })
    }

    /// Take a snapshot right now (settings "Back up now"). `Ok(message)` is
    /// user-visible confirmation; a memory-only session is a no-op that
    /// still reports success (there is nothing on disk to protect).
    pub fn backup_now(&self) -> Result<String, String> {
        match self.repo.as_ref().filter(|r| r.path().is_some()) {
            Some(r) => match r.snapshot() {
                Ok(()) => Ok("a fresh backup was created".into()),
                Err(e) => Err(e.to_string()),
            },
            None => Ok("running in memory — nothing to back up".into()),
        }
    }

    /// Reclaim the attachments nothing can reach any more (SPEC §三十七,
    /// ADR-0037): delete the `attachments` rows and, once the row is gone, the
    /// files beside them.
    ///
    /// "Reachable" is deliberately generous — every block of every page in the
    /// document, every cover the tree's pages point at, every id inside an
    /// outstanding undo *or* redo step, and the copied block in the internal
    /// clipboard. A reclaim that leaves an orphan
    /// behind costs disk; one that deletes a picture Ctrl+Z was about to bring
    /// back costs the user's bytes, so the scan errs to the former.
    ///
    /// It works from the loaded book, never from a directory listing: if the
    /// rows failed to load this session the book is empty and the sweep removes
    /// nothing, which is the only honest answer to "I cannot see the references".
    pub fn reclaim_attachments(&self) -> Result<String, String> {
        let Some(repo) = self.repo.as_ref().filter(|r| r.path().is_some()) else {
            return Ok("running in memory — nothing to reclaim".into());
        };
        // Queue first, sweep second. The debounced writer may still hold an
        // `AttachmentAdded` for a picture this session has already dropped from
        // the document; applying a DELETE out of order ahead of it would leave
        // the row *behind* pointing at files this call is about to remove.
        if let Some(p) = &self.persistence {
            if let Err(e) = p.force_flush() {
                return Err(format!("the queued edits could not be written ({e})"));
            }
        }

        let mut live: std::collections::BTreeSet<i64> = self
            .doc
            .borrow()
            .all_blocks()
            .filter_map(|b| b.attachment)
            .map(|a| a.as_u64() as i64)
            .collect();
        live.extend(self.history.borrow().referenced_attachments());
        // A cover is a page pointing at a file (SPEC §三十八, ADR-0046's whole
        // objection to an icon that is a picture). The book, not the open page:
        // a sweep that frees bytes another page still draws would be a data
        // loss disguised as housekeeping.
        live.extend(self.workspace.borrow().cover_ids());
        // And the same for a version's rows (SPEC §三十八, ADR-0091): a version
        // file is a database this scan never opens, so its pointers are mirrored
        // into the live library when the version is saved, and this is where the
        // mirror is read back. Freeing a picture a version points at would turn
        // that version into a restore of a missing image.
        live.extend(self.version_pinned_attachments());
        if let Some(id) = self.clipboard.borrow().as_ref().and_then(|b| b.attachment) {
            live.insert(id.as_u64() as i64);
        }

        let doomed: Vec<Attachment> = self
            .attachments
            .borrow()
            .values()
            .filter(|a| !live.contains(&(a.id.as_u64() as i64)))
            .cloned()
            .collect();
        if doomed.is_empty() {
            return Ok("no unused attachments to remove".into());
        }

        let changes: Vec<Change> = doomed
            .iter()
            .map(|a| Change::AttachmentDeleted { id: a.id })
            .collect();
        // Rows before bytes: a failed write leaves the picture exactly where it
        // was, while a failed delete only leaves bytes no row claims.
        repo.apply(&changes).map_err(|e| e.to_string())?;

        let mut freed = 0i64;
        let mut stuck: Vec<String> = Vec::new();
        for att in &doomed {
            let key = att.id.as_u64() as i64;
            stuck.extend(self.store.remove(att));
            self.attachments.borrow_mut().remove(&key);
            self.evict_image(key);
            freed += att.bytes.max(0);
        }

        let removed = doomed.len();
        let mut notice = format!(
            "{removed} unused attachment{} removed ({})",
            if removed == 1 { "" } else { "s" },
            crate::services::attachment_store::format_size(freed),
        );
        if !stuck.is_empty() {
            notice.push_str(&format!(
                ", but {} file{} could not be deleted",
                stuck.len(),
                if stuck.len() == 1 { "" } else { "s" }
            ));
        }
        Ok(notice)
    }

    /// Plan+apply several commands as ONE undo step, refresh the rows.
    pub fn exec_all_on_open_page(&self, cmds: Vec<Command>) -> Option<Vec<Change>> {
        if self.locked_refusal() {
            return None;
        }
        let page = core_page_id(self.open_page.get());
        let changes = crate::core::command::exec_all(
            &mut self.doc.borrow_mut(),
            &mut self.history.borrow_mut(),
            page,
            cmds,
        )?;
        self.record(changes.clone());
        self.reproject_blocks();
        Some(changes)
    }

    /// Rich paste (SPEC §二十七): land parsed markdown blocks into the
    /// document. An empty current row (the fresh "+" line, or a brand-new
    /// paragraph) is converted into the first parsed block in place;
    /// otherwise every parsed block inserts after it. Blocks go through the
    /// command system, and marks replay as one `ToggleMark` per span (the
    /// command plans against the pre-state, so a batch of them would
    /// overwrite each other) — an N-block paste is therefore several undo
    /// steps, accepted for v1 and noted in PLAN.
    pub fn paste_block_structure(
        &self,
        id: i32,
        parsed: &[crate::services::import_service::ParsedBlock],
    ) -> bool {
        if parsed.is_empty() {
            return false;
        }
        let page = core_page_id(self.open_page.get());
        let block_id = BlockId(id as u64);
        let current_empty = {
            let doc = self.doc.borrow();
            match doc.page_blocks(page).iter().find(|b| b.id == block_id) {
                Some(b) => b.kind == BlockKind::Paragraph && b.text.is_empty(),
                None => return false,
            }
        };
        let mut anchor = id;
        for (i, p) in parsed.iter().enumerate() {
            let target = if i == 0 && current_empty {
                let _ = self.exec_all_on_open_page(vec![
                    Command::ReplaceText {
                        id: block_id,
                        text: p.text.clone(),
                    },
                    Command::SetBlockType {
                        id: block_id,
                        kind: p.kind,
                    },
                ]);
                if p.checked {
                    let _ = self.exec_on_open_page(Command::ToggleTodoChecked {
                        id: block_id,
                    });
                }
                self.apply_marks(block_id, &p.marks);
                block_id
            } else {
                let Some(changes) = self.exec_on_open_page(Command::InsertBlockAfter {
                    id: BlockId(anchor as u64),
                    kind: p.kind,
                    text: p.text.clone(),
                }) else {
                    return false;
                };
                let Some(nid) = changes.iter().find_map(|c| match c {
                    Change::BlockInserted(b) => Some(b.id),
                    _ => None,
                }) else {
                    return false;
                };
                if p.checked {
                    let _ = self.exec_on_open_page(Command::ToggleTodoChecked { id: nid });
                }
                self.apply_marks(nid, &p.marks);
                anchor = nid.as_u64() as i32;
                nid
            };
            // A pasted fence's info string is part of what was copied, so it
            // rides over the same way `checked` and the marks do.
            if p.lang != Lang::Plain {
                let _ = self.exec_on_open_page(Command::SetCodeLang { id: target, lang: p.lang });
            }
        }
        true
    }

    /// Replay parsed inline-mark spans as sequential ToggleMark commands.
    fn apply_marks(&self, id: BlockId, marks: &[crate::core::types::Mark]) {
        for m in marks {
            let _ = self.exec_on_open_page(Command::ToggleMark {
                id,
                start: m.start,
                end: m.end,
                kind: m.kind,
                url: m.url.clone(),
                date: m.date.clone(),
            });
        }
    }

    pub fn undo_open_page(&self) -> Option<Vec<Change>> {
        // Undo is an edit like any other (SPEC §三十八): a stack of steps built
        // before the lock must not walk the document back through it.
        if self.page_locked() {
            self.note_locked();
            return None;
        }
        let page = core_page_id(self.open_page.get());
        let applied = crate::core::undo(
            &mut self.doc.borrow_mut(),
            &mut self.history.borrow_mut(),
            page,
        )?;
        self.record(applied.clone());
        self.reproject_blocks();
        // A step moves stored data with no write call site of its own, so the
        // page's database windows are the one place that has to be told.
        self.db_refresh_page(None);
        Some(applied)
    }

    pub fn redo_open_page(&self) -> Option<Vec<Change>> {
        if self.page_locked() {
            self.note_locked();
            return None;
        }
        let page = core_page_id(self.open_page.get());
        let applied = crate::core::redo(
            &mut self.doc.borrow_mut(),
            &mut self.history.borrow_mut(),
            page,
        )?;
        self.record(applied.clone());
        self.reproject_blocks();
        self.db_refresh_page(None);
        Some(applied)
    }

    /// Open a page and return its title + breadcrumb for the top bar.
    pub fn open_page_info(&self, id: i32) -> (String, String) {
        let ws = self.workspace.borrow();
        let title = ws.title_of(id).unwrap_or("").to_string();
        let crumb = ws.breadcrumb(id);
        (title, crumb)
    }

    pub fn create_page(&self, parent: Option<i32>) -> i32 {
        let id = self.workspace.borrow_mut().create(parent, "Untitled");
        // the new page is the last sibling: order = previous last + 1
        let order = {
            let kids = self.workspace.borrow().children_of(parent);
            let map = self.page_order.borrow();
            let prev = kids
                .len()
                .checked_sub(2)
                .and_then(|i| kids.get(i))
                .and_then(|pid| map.get(pid).copied());
            OrderKey::between(prev, None).expect("append order exhausted")
        };
        self.page_order.borrow_mut().insert(id, order);
        self.record(vec![Change::PageCreated(crate::core::Page {
            id: PageId(id as u32 as u64),
            title: "Untitled".into(),
            parent: parent.map(|v| PageId(v as u32 as u64)),
            order,
            favorite: false,
            expanded: false,
            font: crate::core::PageFont::default(),
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: false,
        })]);
        let from = self.open_page.get();
        self.open_page(id);
        // creating a page navigates to it, so Go Back returns where the user
        // was (SPEC §十六)
        self.nav.borrow_mut().record(from, id);
        id
    }

    /// The template library as the menus see it (SPEC §三十八 "模板"):
    /// `(id, name)`, oldest first. This is the only list a template appears in
    /// — the sidebar, the page tree, the palette, search and the Move-to walks
    /// never see one, because a template was never attached to the tree.
    pub fn template_list(&self) -> Vec<(i32, String)> {
        self.workspace.borrow().templates()
    }

    pub fn is_template(&self, id: i32) -> bool {
        self.workspace.borrow().is_template(id)
    }

    /// Add a template called `name`, empty for now, and record it.
    ///
    /// `PageCreated` carries a whole `Page`, so the flag needs no change variant
    /// of its own — and the row that lands in `pages` is the row an ordinary
    /// page writes, plus one column. Note what is *not* here: no `open_page` (a
    /// template cannot be opened, and that refusal is the feature), no nav
    /// entry, no recents. The caller fills the body with `fill_template`, or the
    /// template is a blank one the menus will offer.
    pub fn create_template(&self, name: &str) -> i32 {
        let id = self.workspace.borrow_mut().create_template(name);
        self.page_order.borrow_mut().insert(id, OrderKey::FIRST);
        self.record(vec![Change::PageCreated(crate::core::Page {
            id: PageId(id as u32 as u64),
            title: name.to_string(),
            parent: None,
            order: OrderKey::FIRST,
            favorite: false,
            expanded: false,
            font: PageFont::default(),
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: true,
        })]);
        id
    }

    /// Give template `template` the block sequence `src` (SPEC §三十八: "模板的
    /// 表示必须是「块序列的副本」"). `src` is read off some other page, in that
    /// page's display order, and every field a block can carry rides along: this
    /// is a copy of rows, not a re-encoding, which is why §三十八 can forbid a
    /// second content format and this function stays twenty lines rather than
    /// becoming a parser.
    ///
    /// Ids are minted from the allocator the editor uses, so a copy can never
    /// collide with its source; order keys are kept, because a key only means
    /// something inside one page and a fresh template has no rows to collide
    /// with; parent links are remapped onto the copies, which is what keeps a
    /// table's grid and a toggle's children together.
    ///
    /// No undo step, deliberately: the history stack is per page and belongs to
    /// the page the user is typing in, and a template page is never open. The
    /// way back from a wrong save is Templates > Delete, which is also how the
    /// built-in library is pruned.
    fn fill_template(&self, template: i32, src: &[Block]) {
        if src.is_empty() || !self.workspace.borrow().is_template(template) {
            return;
        }
        let pid = core_page_id(template);
        let mut copies: Vec<Block> = Vec::with_capacity(src.len());
        let mut remap: HashMap<BlockId, BlockId> = HashMap::new();
        {
            let mut doc = self.doc.borrow_mut();
            for b in src {
                let fresh = doc.alloc_block_id();
                remap.insert(b.id, fresh);
                let mut copy = b.clone();
                copy.id = fresh;
                copy.page = pid;
                copies.push(copy);
            }
            for copy in copies.iter_mut() {
                copy.parent = copy.parent.and_then(|p| remap.get(&p).copied());
            }
            doc.set_page_blocks(pid, copies.clone());
        }
        let changes: Vec<Change> = copies.into_iter().map(Change::BlockInserted).collect();
        self.record(changes);
    }

    /// Copy the page `page` is showing into a new template named after it
    /// (SPEC §三十八 "存为模板").
    ///
    /// The name is the page's title because a text box would be a dialog this
    /// app has no pattern for, and because it makes the menu entry say exactly
    /// what it does. Two templates may share a name — the library sorts by age,
    /// not title (`Workspace::templates`) — so saving the same page twice makes
    /// two templates rather than quietly overwriting one. Overwriting would be
    /// the destructive option: a template body is on nobody's undo stack, so the
    /// older copy would be gone for good.
    pub fn save_as_template(&self, page: i32) -> i32 {
        let name = self
            .workspace
            .borrow()
            .title_of(page)
            .filter(|t| !t.trim().is_empty())
            .unwrap_or("Untitled template")
            .to_string();
        let src = {
            let doc = self.doc.borrow();
            doc.page_blocks(core_page_id(page)).to_vec()
        };
        let id = self.create_template(&name);
        self.fill_template(id, &src);
        id
    }

    /// Insert a template's block sequence into the open page (SPEC §三十八 "在
    /// 页面内插入模板"). `anchor` is the row the menu was opened from; `None`
    /// means the page has no row to point at yet, and the copy appends.
    ///
    /// `InsertForest` is what makes this the right shape: one command, so one
    /// `Ctrl+Z` undoes the whole template instead of leaving nine of eleven
    /// blocks behind, and the change list is ordinary `BlockInserted`s, so the
    /// flush, the FTS index and a restart all see a page that simply grew.
    ///
    /// An empty paragraph at the anchor is *replaced* rather than followed, in
    /// the same batch: the "+" line and a brand-new page's first row are both
    /// empty, and a template that arrives one row below the caret looks like it
    /// missed. The delete can ride in the same `exec_all` because that plans
    /// every command against the pre-state — the anchor is still there to plan
    /// against — and its apply list is inserts-then-delete.
    ///
    /// A template holding a `Page`-kind block inserts a second block pointing at
    /// the *same* child page: the sequence is what is copied, and a reference is
    /// part of it. The page being written is `self.open_page`, so the lock gate
    /// is `exec_all_on_open_page`'s and a locked page refuses this like any
    /// other edit.
    ///
    /// Returns the first inserted block's id — the row the caret goes to. The
    /// caller needs it because this command can *remove* the row it was asked
    /// to anchor on, and a caret left pointing at a deleted block is a click
    /// away from editing nothing. `None` means nothing landed (locked, empty
    /// template, or an anchor that isn't on this page).
    pub fn insert_template(&self, anchor: Option<i32>, template: i32) -> Option<i32> {
        if !self.workspace.borrow().is_template(template) {
            return None;
        }
        let blocks = {
            let doc = self.doc.borrow();
            doc.page_blocks(core_page_id(template)).to_vec()
        };
        if blocks.is_empty() {
            return None;
        }
        let anchor_id = anchor.map(|a| BlockId(a as u64));
        let take_anchor = {
            let doc = self.doc.borrow();
            anchor_id.is_some_and(|id| {
                doc.page_blocks(core_page_id(self.open_page.get()))
                    .iter()
                    .any(|b| {
                        b.id == id
                            && b.kind == BlockKind::Paragraph
                            && b.text.is_empty()
                            && b.marks.is_empty()
                    })
            })
        };
        let mut cmds = vec![Command::InsertForest {
            anchor: anchor_id,
            blocks,
        }];
        if take_anchor {
            if let Some(id) = anchor_id {
                cmds.push(Command::DeleteBlock { id });
            }
        }
        let changes = self.exec_all_on_open_page(cmds)?;
        // `BlockInserted` in apply order, so the first one is the forest's first
        // root — which is what the user sees appear at the caret.
        changes
            .iter()
            .find_map(|c| match c {
                Change::BlockInserted(b) => Some(b.id.as_u64() as i32),
                _ => None,
            })
    }

    /// Create a page under `parent` whose body is a template's block sequence
    /// (SPEC §三十八 "新建页面时选模板").
    ///
    /// This is `create_page` and then the insert, which is the point: the new
    /// page is an ordinary page from its first change on — in the tree, in
    /// search, on its own undo stack — and the template stays hidden behind it.
    /// It takes the template's name, since "Untitled" for a page the user just
    /// chose a shape for says nothing. `create_page` already opened it, and a
    /// fresh page has no rows, so the copy lands with `anchor: None`.
    pub fn new_page_from_template(&self, parent: Option<i32>, template: i32) -> i32 {
        let id = self.create_page(parent);
        let name = self
            .workspace
            .borrow()
            .title_of(template)
            .unwrap_or("")
            .to_string();
        if self.insert_template(None, template).is_some() && !name.is_empty() {
            self.rename_page(id, &name);
        }
        id
    }

    /// The Markdown a template exports as (SPEC §三十八: "导入导出走 §二十六 的
    /// Markdown 通道"). `None` when the id is not a template — exporting the
    /// open page is a different menu entry, and this one must not answer for it.
    pub fn template_markdown(&self, template: i32) -> Option<String> {
        if !self.workspace.borrow().is_template(template) {
            return None;
        }
        let doc = self.doc.borrow();
        Some(crate::services::export_service::export_page(doc.page_blocks(
            core_page_id(template),
        )))
    }

    /// Land a Markdown file in the library as a new template (SPEC §三十八,
    /// through §二十六's parser).
    ///
    /// `import_service::import_markdown` already builds exactly this change
    /// list — `PageCreated` plus one `BlockInserted` per block, ids from the
    /// caller's allocator — and the `Page` it writes is the one handed in, so
    /// all a *template* import needs that a page import does not is
    /// `template: true` on that struct. Reusing the channel is what keeps the
    /// promise §三十八 makes: an imported template is a block sequence in the
    /// same rows, with no format of its own anywhere in the file.
    pub fn import_template(&self, name: &str, src: &str) -> i32 {
        let id = self.create_template(name);
        let core_page = crate::core::Page {
            id: PageId(id as u32 as u64),
            title: name.to_string(),
            parent: None,
            order: OrderKey::FIRST,
            favorite: false,
            expanded: false,
            font: PageFont::default(),
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: true,
        };
        let changes = {
            let mut doc = self.doc.borrow_mut();
            let mut alloc = || doc.alloc_block_id();
            crate::services::import_service::import_markdown(src, &core_page, &mut alloc)
        };
        // the service re-states the `PageCreated` `create_template` already
        // recorded; its rows are the part that is new here
        let rest: Vec<Change> = changes.into_iter().skip(1).collect();
        {
            let mut doc = self.doc.borrow_mut();
            doc.apply(&rest);
        }
        self.record(rest);
        id
    }

    /// Put the built-in library into a database that has never had one (SPEC
    /// §三十八 "预置若干本地模板"), once per library.
    ///
    /// Two guards, each doing one job. The settings flag is what makes a
    /// deletion stick: without it, deleting all five built-ins and restarting
    /// would resurrect them. The name check is what makes a retry safe: the
    /// writes flush in batches, so a session that died halfway through a seed
    /// would otherwise land the whole library twice, and a menu with two
    /// "Meeting notes" rows is a mess the user cannot undo.
    ///
    /// It is deliberately not a migration. Schema v14 adds the column and stops
    /// there, because a migration that wrote block rows itself would have to
    /// keep the order keys, the child links and both FTS indexes in step by hand
    /// — when `import_template`, the very function the menu's Import row calls,
    /// already does all three. So the built-ins are imported, not invented, and
    /// there is one code path that can be wrong.
    pub fn seed_builtin_templates(&self) {
        // A session with no library has no library to seed. The mock workspace
        // the tests and the bench scenes run on is not a place a template
        // belongs: it has nothing to persist to, so the flag could not be
        // written, and every start would re-add five hidden pages.
        if self.persistence.is_none() || self.setting_flag(SEEDED_BUILTIN_TEMPLATES) {
            return;
        }
        let have: Vec<String> = self
            .template_list()
            .into_iter()
            .map(|(_, title)| title)
            .collect();
        for preset in crate::core::template::PRESETS {
            if have.iter().any(|t| t == preset.name) {
                continue;
            }
            self.import_template(preset.name, preset.markdown);
        }
        // after the bodies, so a session that never got as far as flushing
        // anything retries the whole library rather than recording a lie
        self.record_setting(SEEDED_BUILTIN_TEMPLATES, "1");
    }

    pub fn rename_page(&self, id: i32, title: &str) {
        // The title is part of the document, so the lock covers it too — from
        // the sidebar's rename-in-place as well as from the hero. An import
        // names a page it just created, and that one is unlocked, so this gate
        // never sits between a user and their own file.
        if self.workspace.borrow().locked_of(id) {
            self.note_locked();
            return;
        }
        let title = title.trim();
        if title.is_empty() {
            self.rebuild_sidebar();
            return;
        }
        self.workspace.borrow_mut().rename(id, title);
        // searchable blob: refresh the title prefix in place
        let pid = core_page_id(id);
        let blob = {
            let doc = self.doc.borrow();
            let mut b = String::from(title);
            for block in doc.page_blocks(pid) {
                if !block.text.is_empty() {
                    b.push('\n');
                    b.push_str(&block.text);
                }
            }
            b
        };
        self.workspace.borrow_mut().set_search_text(id, blob);
        self.record(vec![Change::PageTitleSet {
            id: PageId(id as u32 as u64),
            title: title.to_string(),
        }]);
        self.rebuild_sidebar();
        // Page blocks embedding this page show its live title
        self.reproject_blocks();
    }

    pub fn duplicate_page(&self, id: i32) -> Option<i32> {
        let new_id = self.workspace.borrow_mut().duplicate(id);
        if let Some(nid) = new_id {
            // copy the source page's blocks with fresh ids, remapping
            // parent pointers through the same map (nested lists survive).
            // The id range is RESERVED on the document — ids minted without
            // reserving collided with the next allocation (M8_FEEDBACK #2).
            let src = core_page_id(id);
            let dst = core_page_id(nid);
            let (copies, _id_map) = {
                let mut doc = self.doc.borrow_mut();
                let src_blocks = doc.page_blocks(src).to_vec();
                let start = doc.reserve_block_ids(src_blocks.len() as u64);
                let mut map = HashMap::new();
                let copies: Vec<Block> = src_blocks
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        let new_id = BlockId(start + i as u64);
                        map.insert(b.id, new_id);
                        let mut c = b.clone();
                        c.id = new_id;
                        c.page = dst;
                        c
                    })
                    .collect();
                // second pass: parents point at the copies now
                let copies: Vec<Block> = copies
                    .into_iter()
                    .map(|mut c| {
                        if let Some(pid) = c.parent {
                            c.parent = map.get(&pid).copied();
                        }
                        c
                    })
                    .collect();
                (copies, map)
            };
            let title = self.workspace.borrow().title_of(nid).unwrap().to_string();
            let style = self.workspace.borrow().page_style(id).unwrap_or_default();
            let icon = self.workspace.borrow().icon_of(id);
            let cover = self.workspace.borrow().cover_of(id);
            let blob = {
                // the copy's blob is indexed once, so it resolves its mentions
                // with the same query a projection does
                let ws = self.workspace.borrow();
                let titles =
                    MentionTitles::of_blocks(&copies, |id| ws.title_of(id).map(str::to_string));
                block_search_blob(&title, &project_blocks(&copies, &FindHits::new(), &titles))
            };

            // order: right after the original when a gap exists, else the
            // end of the sibling run. The workspace children vec must agree
            // with the key in BOTH branches — duplicate() attaches the copy
            // adjacent to the original, so the append fallback re-attaches
            // at the end; otherwise the session view (vec order) and the
            // restart projection (key order) would disagree.
            let (parent, order, appended) = {
                let ws = self.workspace.borrow();
                let parent = ws.get(nid).and_then(|p| p.parent);
                let kids = ws.children_of(parent);
                // the sibling AFTER the original — skipping the fresh copy,
                // which duplicate() parked right there without a key yet
                let next = kids
                    .iter()
                    .skip_while(|&&k| k != id)
                    .skip(1)
                    .find(|&&k| k != nid)
                    .and_then(|k| self.page_order.borrow().get(k).copied());
                let orig = self.page_order.borrow().get(&id).copied();
                match OrderKey::between(orig, next) {
                    Some(key) => (parent, key, false),
                    None => {
                        let last = kids
                            .iter()
                            .filter(|&&k| k != nid)
                            .last()
                            .and_then(|k| self.page_order.borrow().get(k).copied());
                        (
                            parent,
                            OrderKey::between(last, None).expect("order space exhausted"),
                            true,
                        )
                    }
                }
            };
            self.page_order.borrow_mut().insert(nid, order);
            if appended {
                self.workspace.borrow_mut().move_page(nid, parent, None);
            }

            let mut batch = vec![Change::PageCreated(crate::core::Page {
                id: PageId(nid as u32 as u64),
                title,
                parent: parent.map(|v| PageId(v as u32 as u64)),
                order,
                favorite: false,
                expanded: false,
                // a duplicate is a copy of the page, and its look is part of
                // it; favorites are not, so that one stays false
                font: style.0,
                full_width: style.1,
                small_text: style.2,
                icon,
                cover,
                // Unlocked, like the in-memory copy `Workspace::duplicate`
                // just made: the look travels with a duplicate, the gate on
                // editing does not.
                locked: false,
                template: false,
            })];
            for b in &copies {
                batch.push(Change::BlockInserted(b.clone()));
            }
            self.doc.borrow_mut().set_page_blocks(dst, copies);
            self.workspace.borrow_mut().set_search_text(nid, blob);
            self.record(batch);
            self.rebuild_sidebar();
        }
        new_id
    }

    pub fn delete_page(&self, id: i32) -> bool {
        let removed = self.workspace.borrow_mut().delete(id);
        let had_open = removed.contains(&self.open_page.get());
        {
            let mut doc = self.doc.borrow_mut();
            for r in &removed {
                doc.drop_page(core_page_id(*r));
            }
        }
        let mut changes = vec![Change::PageDeleted {
            id: PageId(id as u32 as u64),
        }];
        // A page that is gone has no version worth keeping — and a picture one
        // of them pointed at has to stop being held alive by it, or the §三十七
        // reclaim would keep bytes nothing can ever reach again (ADR-0047).
        for removed_page in &removed {
            changes.extend(self.forget_versions(*removed_page));
        }
        self.record(changes);
        if had_open {
            // stale until the controller opens a fallback page
            self.open_page.set(0);
        }
        self.rebuild_sidebar();
        // A page that has just gone away is exactly what a mention chip, a
        // `Page`/`Link` row and the backlink panel were rendering titles for,
        // so the projection on screen is stale the moment this returns —
        // unless the page that went away *is* the one on screen, in which case
        // the caller is about to open another one and reprojecting here would
        // draw a page that no longer exists.
        //
        // Nothing was rewritten to make that happen: the reference still holds
        // the id it was given (SPEC §四十), which is why the degradation has to
        // be the projector's job rather than a fan-out over every mention.
        if !had_open {
            self.reproject_blocks();
        }
        had_open
    }

    /// The page a `Page`-kind block points at, if it is still there.
    pub fn block_page_ref(&self, id: i32) -> Option<i32> {
        let doc = self.doc.borrow();
        doc.block(BlockId(id as u64))
            .and_then(|b| b.page_ref)
            .map(|p| p.as_u64() as i32)
    }

    /// The kind of one block, for the controller's per-kind decisions.
    pub fn block_kind_of(&self, id: i32) -> Option<BlockKind> {
        let doc = self.doc.borrow();
        doc.block(BlockId(id as u64)).map(|b| b.kind)
    }

    /// Fill the slash popup with the page picker: every page in tree order,
    /// label = title, hint = breadcrumb. Typing filters by title.
    pub fn open_slash_pick(&self, filter: &str) {
        let needle = filter.to_lowercase();
        let ws = self.workspace.borrow();
        let rows: Vec<SlashRow> = ws
            .dfs_order()
            .into_iter()
            .filter_map(|id| {
                let title = ws.title_of(id)?;
                if !needle.is_empty() && !title.to_lowercase().contains(&needle) {
                    return None;
                }
                Some(SlashRow {
                    id,
                    label: title.into(),
                    hint: ws.breadcrumb(id).into(),
                    disabled: false,
                })
            })
            .collect();
        self.slash.set_vec(rows);
    }

    /// The page id behind the focused picker row.
    pub fn slash_selected_page(&self, focus: i32) -> Option<i32> {
        let row = self.slash.row_data(focus.max(0) as usize)?;
        if row.disabled {
            return None;
        }
        Some(row.id)
    }

    /// Fill the slash popup with the **mention picker** (SPEC §四十): the date
    /// item first, then every page in tree order, filtered by title.
    ///
    /// One picker and not two: `@` is a single trigger, so it has one menu, and
    /// the date item is the first row of it — a second popup keyed on the same
    /// character would race the first for the caret. The item's label is the
    /// ISO text the atom will actually hold (`core::date`), not a prettier
    /// "Today": what the picker shows is what the file gets.
    pub fn open_slash_mention(&self, filter: &str) {
        let needle = filter.to_lowercase();
        let ws = self.workspace.borrow();
        let today = crate::core::today_iso();
        let date_hit = needle.is_empty() || "today".contains(&needle) || today.contains(&needle);
        let mut rows: Vec<SlashRow> = Vec::new();
        if date_hit {
            rows.push(SlashRow {
                id: MENTION_DATE_ROW,
                label: today.into(),
                hint: "Today".into(),
                disabled: false,
            });
        }
        rows.extend(ws.dfs_order().into_iter().filter_map(|id| {
            let title = ws.title_of(id)?;
            if !needle.is_empty() && !title.to_lowercase().contains(&needle) {
                return None;
            }
            Some(SlashRow {
                id,
                label: title.into(),
                hint: ws.breadcrumb(id).into(),
                disabled: false,
            })
        }));
        self.slash.set_vec(rows);
    }

    /// The mention picker's focused row: `(id, label)`. `id == MENTION_DATE_ROW`
    /// means the date item, and every other id is a page. The label is what the
    /// atom's span text becomes — the page's title as it is *now*, which is why
    /// the atom keeps reading correctly after a rename.
    pub fn slash_selected_mention(&self, focus: i32) -> Option<(i32, String)> {
        let row = self.slash.row_data(focus.max(0) as usize)?;
        if row.disabled {
            return None;
        }
        Some((row.id, row.label.to_string()))
    }

    /// Apply the mention picker's focused row to the block `id`: the text from
    /// the `@` trigger onwards is replaced by the label, and the label's span
    /// becomes a Mention (address = the page's id) or a Date (payload = the ISO
    /// text) atom. One recorded batch, so one Ctrl+Z takes the whole insert
    /// back — the atom, its characters and the `@` alike.
    ///
    /// The label is stored as the span's text as well: a document has to read
    /// as prose in its own right (SPEC §二十六), and the *mark* is what says
    /// the characters are a reference rather than a typed title — the same
    /// split a link already has.
    pub fn apply_mention(&self, id: i32, at: usize, page: Option<i32>, label: &str) -> bool {
        let block_id = BlockId(id as u64);
        let Some(text) = self.block_text(id) else {
            return false;
        };
        if at > text.len() || !text.is_char_boundary(at) {
            return false;
        }
        let (kind, url, date) = match page {
            Some(p) => (
                crate::core::MarkKind::Mention,
                crate::core::page_uri(PageId(p as u32 as u64)),
                None,
            ),
            None => (
                crate::core::MarkKind::Date,
                String::new(),
                Some(label.to_string()),
            ),
        };
        self.exec_all_on_open_page(vec![Command::InsertReference {
                id: block_id,
                at,
                label: label.to_string(),
                kind,
                url,
                date,
            },
        ])
        .is_some()
    }

    /// The stored text of one block on the open page, for the mention apply
    /// path — which needs the bytes it is about to cut, not a row model.
    fn block_text(&self, id: i32) -> Option<String> {
        let doc = self.doc.borrow();
        doc.block(BlockId(id as u64)).map(|b| b.text.clone())
    }

    /// Convert the empty paragraph `id` (the "+" handle's fresh line) into a
    /// Link-to-page block pointing at `target`. The target is NOT owned:
    /// deleting the block leaves the page alone, so duplicates and pastes may
    /// share it freely. One recorded batch.
    // ─── synced blocks (SPEC §四十, ADR-0052) ───────────────────────────────
    //
    // Everything a mirror needs answered in three questions: *what does this
    // row draw* (`sync_target`), *what may it point at* (`sync_would_cycle`),
    // and *how does the pointer get written* (`set_sync_source`). None of it
    // ever writes into the mirror's own `text` — that column stays empty for
    // the life of the block, and letting it fill up is the one thing this
    // whole design is built to prevent.

    /// The block whose words a row draws (SPEC §四十, ADR-0052). It is the row
    /// itself for everything except a mirror, and the *source* for one — which
    /// single sentence is the whole of "edit either copy and both change".
    ///
    /// `-1` means there is nothing to type into: the sources cannot be walked
    /// to any more. Every caller has to be able to see that, because the
    /// alternative is an edit that silently goes nowhere.
    pub fn content_of(&self, id: i32) -> i32 {
        let doc = self.doc.borrow();
        let Some(b) = doc.block(BlockId(id as u64)) else {
            return -1;
        };
        if b.kind != BlockKind::Synced {
            return b.id.0 as i32;
        }
        match b.sync_ref.and_then(|sid| sync_target(&doc, sid)) {
            Some(src) => src.id.0 as i32,
            None => -1,
        }
    }

    /// Fill the popup with the blocks a mirror may point at.
    ///
    /// **The whole library, not the open page.** The feature exists so that a
    /// piece of writing can be looked at from somewhere else; a picker that
    /// could only see the page already on screen would be the same idea with
    /// its reason for existing removed. `Document::all_blocks` is an in-memory
    /// walk, which is why the filter can be answered per keystroke instead of
    /// being indexed.
    ///
    /// A mirror is deliberately not offered as a source: pointing at one adds a
    /// hop that every later projection has to walk for no words of its own.
    pub fn open_slash_block(&self, filter: &str) {
        let needle = filter.to_lowercase();
        let ws = self.workspace.borrow();
        let doc = self.doc.borrow();
        let mut rows: Vec<SlashRow> = Vec::new();
        for b in doc.all_blocks() {
            if b.kind == BlockKind::Synced || b.text.trim().is_empty() {
                continue;
            }
            let page_title = ws.title_of(b.page.as_u64() as i32).unwrap_or_default();
            if !needle.is_empty()
                && !b.text.to_lowercase().contains(&needle)
                && !page_title.to_lowercase().contains(&needle)
            {
                continue;
            }
            rows.push(SlashRow {
                id: b.id.0 as i32,
                label: first_line_of(&b.text).into(),
                hint: page_title.into(),
                disabled: false,
            });
            if rows.len() >= SYNC_PICKER_LIMIT {
                break;
            }
        }
        self.slash.set_vec(rows);
    }

    /// The focused row of that popup, which is a **block** id rather than a
    /// kind — the same field this list uses for kinds in every other mode, and
    /// precisely why the modes are separate flags instead of one integer.
    pub fn slash_selected_block(&self, focus: i32) -> Option<i32> {
        let row = self.slash.row_data(focus.max(0) as usize)?;
        if row.disabled || row.id <= 0 {
            return None;
        }
        Some(row.id)
    }

    /// Point a `Synced` block at `source`, or at nothing (`None` clears the
    /// pointer and leaves an ordinary, unresolvable mirror behind).
    ///
    /// **Every refusal here is visible rather than silent**, because the picker
    /// has nothing else to report through: `false` comes back when the block is
    /// not there, when it is not a mirror, and — the interesting one — when the
    /// pointer would **close a cycle**. ADR-0052 §4 puts that check at the
    /// moment of writing on purpose: a check done while drawing discovers the
    /// cycle once per frame for as long as it exists, and can never actually
    /// refuse it.
    ///
    /// The words go in the same batch as the pointer when there are any: this
    /// is the only place that can still say "this row owns nothing", and saying
    /// it in one recorded batch means one Ctrl+Z takes the whole conversion
    /// back rather than half of it.
    pub fn set_sync_source(&self, id: i32, source: Option<i32>) -> bool {
        if id <= 0 {
            return false;
        }
        let block_id = BlockId(id as u64);
        let source_id = source.filter(|s| *s > 0).map(|s| BlockId(s as u64));
        let owned_text = {
            let doc = self.doc.borrow();
            let Some(b) = doc.block(block_id) else {
                return false;
            };
            if b.kind != BlockKind::Synced {
                // a pointer on a block that is not a mirror is a mirror nobody
                // ever created: refusing here keeps `blocks.sync_ref` meaning
                // exactly one thing, which is the whole of what the column has
                // to offer a reader of the file ten years from now
                return false;
            }
            if let Some(src) = source_id {
                if sync_would_cycle(&doc, block_id, src) {
                    return false;
                }
            }
            !b.text.is_empty()
        };
        let mut changes = Vec::new();
        if source_id.is_some() && owned_text {
            changes.push(Change::BlockTextSet {
                id: block_id,
                text: String::new(),
            });
        }
        changes.push(Change::BlockSyncSet {
            id: block_id,
            source: source_id,
        });
        self.doc.borrow_mut().apply(&changes);
        self.record(changes);
        self.reproject_blocks();
        true
    }

    pub fn create_page_link_block(&self, id: i32, target: i32) -> bool {
        if self.locked_refusal() {
            return false;
        }
        let block_id = BlockId(id as u64);
        let page = self.open_page.get();
        {
            let doc = self.doc.borrow();
            let b = doc
                .page_blocks(core_page_id(page))
                .iter()
                .find(|b| b.id == block_id);
            let Some(b) = b else { return false };
            if b.kind != BlockKind::Paragraph || !b.text.is_empty() {
                return false;
            }
        }
        if !self.workspace.borrow().contains(target) {
            return false;
        }
        let changes = vec![
            Change::BlockRefSet {
                id: block_id,
                page: Some(PageId(target as u32 as u64)),
            },
            Change::BlockKindSet {
                id: block_id,
                kind: BlockKind::Link,
            },
        ];
        self.doc.borrow_mut().apply(&changes);
        self.record(changes);
        self.reproject_blocks();
        true
    }

    /// Turn the empty paragraph `after_id` (the "+" handle's fresh line, or a
    /// row the insert menu is applying to) into a Page block: one child page
    /// is created under the current page and the block points at it. One
    /// recorded batch. Page creation is not undoable (same as the sidebar
    /// flow), so undo restores the block kind but not the page.
    pub fn create_page_block(&self, after_id: i32) -> Option<i32> {
        if self.locked_refusal() {
            return None;
        }
        let block_id = BlockId(after_id as u64);
        let parent_page = self.open_page.get();
        {
            let doc = self.doc.borrow();
            let b = doc
                .page_blocks(core_page_id(parent_page))
                .iter()
                .find(|b| b.id == block_id)?;
            if b.kind != BlockKind::Paragraph || !b.text.is_empty() {
                return None;
            }
        }
        let child = self.workspace.borrow_mut().create(Some(parent_page), "Untitled");
        let order = {
            let kids = self.workspace.borrow().children_of(Some(parent_page));
            let map = self.page_order.borrow();
            let prev = kids
                .len()
                .checked_sub(2)
                .and_then(|i| kids.get(i))
                .and_then(|pid| map.get(pid).copied());
            OrderKey::between(prev, None).expect("append order exhausted")
        };
        self.page_order.borrow_mut().insert(child, order);
        self.workspace
            .borrow_mut()
            .set_search_text(child, "Untitled".into());
        let child_id = PageId(child as u32 as u64);
        let changes = vec![
            Change::PageCreated(crate::core::Page {
                id: child_id,
                title: "Untitled".into(),
                parent: Some(PageId(parent_page as u32 as u64)),
                order,
                favorite: false,
                expanded: false,
                font: crate::core::PageFont::default(),
                full_width: false,
                small_text: false,
                icon: String::new(),
                cover: None,
                locked: false,
                template: false,
            }),
            Change::BlockRefSet {
                id: block_id,
                page: Some(child_id),
            },
            Change::BlockKindSet {
                id: block_id,
                kind: BlockKind::Page,
            },
        ];
        self.doc.borrow_mut().apply(&changes);
        self.record(changes);
        self.rebuild_sidebar();
        self.reproject_blocks();
        Some(child)
    }

    /// Duplicate a Page block: the child page is deep-copied (sidebar
    /// semantics) and the fresh block points at the copy, so two blocks never
    /// share a target — deleting one would not orphan the other. A Link block
    /// does NOT take this path: its target is unowned, so the plain duplicate
    /// (which clones the ref) is safe.
    pub fn duplicate_page_block(&self, id: i32) -> Option<i32> {
        if self.locked_refusal() {
            return None;
        }
        if self.block_kind_of(id) != Some(BlockKind::Page) {
            return None;
        }
        let page_ref = self.block_page_ref(id)?;
        let copy_page = self.duplicate_page(page_ref)?;
        let changes = self.exec_on_open_page(Command::DuplicateBlock {
            id: BlockId(id as u64),
        })?;
        let new_id = changes.iter().find_map(|c| match c {
            Change::BlockInserted(b) => Some(b.id),
            _ => None,
        })?;
        let change = Change::BlockRefSet {
            id: new_id,
            page: Some(PageId(copy_page as u32 as u64)),
        };
        self.doc.borrow_mut().apply(std::slice::from_ref(&change));
        self.record(vec![change]);
        self.reproject_blocks();
        Some(new_id.as_u64() as i32)
    }

    /// Move a page (and its subtree) under `new_parent` (root when `None`),
    /// appended as the parent's last child. One `PageMoved` change; the
    /// workspace refuses cycles (a parent cannot move into its own subtree).
    pub fn move_page(&self, id: i32, new_parent: Option<i32>) -> bool {
        let order = {
            let ws = self.workspace.borrow();
            let last = ws
                .children_of(new_parent)
                .last()
                .and_then(|k| self.page_order.borrow().get(k).copied());
            OrderKey::between(last, None).expect("append order exhausted")
        };
        if !self
            .workspace
            .borrow_mut()
            .move_page(id, new_parent, None)
        {
            return false;
        }
        self.page_order.borrow_mut().insert(id, order);
        self.record(vec![Change::PageMoved {
            id: PageId(id as u32 as u64),
            parent: new_parent.map(|v| PageId(v as u32 as u64)),
            order,
        }]);
        self.rebuild_sidebar();
        true
    }

    /// The drop target behind a sidebar row index, for the page-tree drag:
    /// `Some(None)` = the Workspace header (top level), `Some(Some(pid))` =
    /// drop into that page, `None` = not a target (other headers, recents,
    /// the "New page" row).
    pub fn page_drop_target(&self, index: i32) -> Option<Option<i32>> {
        let row = self.sidebar.row_data(index.max(0) as usize)?;
        match row.kind.as_str() {
            "page" => Some(Some(row.id)),
            "header" if row.id == WORKSPACE_HEADER_ID => Some(None),
            _ => None,
        }
    }

    /// Drag-hover validity for the page tree (called per frame of the
    /// gesture — read-only, like the block drag's hover check).
    pub fn page_drop_target_valid(&self, id: i32, index: i32) -> bool {
        let Some(target) = self.page_drop_target(index) else {
            return false;
        };
        let ws = self.workspace.borrow();
        ws.can_move_page(id, target)
    }

    /// Commit a page-tree drag: the dragged page moves under the row it was
    /// dropped on (the Workspace header moves it to the top level).
    pub fn page_dropped(&self, id: i32, index: i32) -> bool {
        let Some(target) = self.page_drop_target(index) else {
            return false;
        };
        self.move_page(id, target)
    }

    /// Swap a page with the sibling one slot up (-1) / down (+1): the two
    /// order keys trade places, recorded as two `PageMoved` changes.
    pub fn move_page_by(&self, id: i32, delta: i32) -> bool {
        let (parent, neighbor) = {
            let ws = self.workspace.borrow();
            let Some(parent) = ws.get(id).map(|p| p.parent) else {
                return false;
            };
            let kids = ws.children_of(parent);
            let Some(idx) = kids.iter().position(|&c| c == id) else {
                return false;
            };
            let nidx = idx as isize + delta as isize;
            if nidx < 0 || nidx as usize >= kids.len() {
                return false;
            }
            (parent, kids[nidx as usize])
        };
        if !self.workspace.borrow_mut().swap_with_neighbor(id, delta) {
            return false;
        }
        let mut map = self.page_order.borrow_mut();
        let a = map.get(&id).copied().unwrap_or(OrderKey::FIRST);
        let b = map.get(&neighbor).copied().unwrap_or(OrderKey::FIRST);
        map.insert(id, b);
        map.insert(neighbor, a);
        drop(map);
        self.record(vec![
            Change::PageMoved {
                id: PageId(id as u32 as u64),
                parent: parent.map(|v| PageId(v as u32 as u64)),
                order: b,
            },
            Change::PageMoved {
                id: PageId(neighbor as u32 as u64),
                parent: parent.map(|v| PageId(v as u32 as u64)),
                order: a,
            },
        ]);
        self.rebuild_sidebar();
        true
    }

    /// Tell the editor which page it is drawing (SPEC §三十八). The four values
    /// are the only route a page's look takes: nothing in `ui/` reads the
    /// workspace, and no block carries a font. Called on every open-page
    /// change, so a page that never touches the menu still says 0/false/false.
    pub fn apply_page_style(&self) {
        let Some(ui) = self.ui.borrow().clone() else {
            return;
        };
        let (font, full_width, small_text, icon, cover, locked) = {
            let ws = self.workspace.borrow();
            let (font, full_width, small_text) = ws
                .page_style(self.open_page.get())
                .unwrap_or_default();
            (
                font,
                full_width,
                small_text,
                ws.icon_of(self.open_page.get()),
                ws.cover_of(self.open_page.get()),
                ws.locked_of(self.open_page.get()),
            )
        };
        let g = ui.upgrade().unwrap();
        g.set_page_font(font.slot());
        g.set_page_full_width(full_width);
        g.set_page_small_text(small_text);
        // The stored emoji, which for an iconless page is nothing: the editor
        // leaves the slot out rather than echoing the title's first character
        // at 60px. The sidebar does substitute that, because its slot is a
        // generic page glyph today and §三十八 wants it to say something about
        // *this* page.
        g.set_page_icon(icon.into());
        // An id, not the raster: the band asks for its own picture through the
        // same `image-for` callback an image row uses, so one attachment held by
        // a page and a block is decoded once, and a page with no cover never
        // evaluates the binding at all.
        g.set_page_cover_id(cover.map(|a| a.as_u64() as i32).unwrap_or(0));
        // The lock rides the same route as the look, because the editor needs
        // it at draw time: a row must not offer a caret it will not keep.
        g.set_page_locked(locked);
    }

    pub fn set_page_font(&self, id: i32, font: PageFont) {
        let font = self.workspace.borrow_mut().set_font(id, font);
        self.record(vec![Change::PageFontSet {
            id: PageId(id as u32 as u64),
            font,
        }]);
        self.apply_page_style();
    }

    /// Set — or with `""` clear — the page's icon (SPEC §三十八). Persisted,
    /// like the font, and deliberately outside undo: a look is a property of
    /// the page, not an edit to it. The sidebar redraws because every row's
    /// slot now answers something about its own page.
    pub fn set_page_icon(&self, id: i32, icon: &str) {
        let icon = self.workspace.borrow_mut().set_icon(id, icon);
        self.record(vec![Change::PageIconSet {
            id: PageId(id as u32 as u64),
            icon,
        }]);
        self.apply_page_style();
        self.rebuild_sidebar();
    }

    /// Set — or with `None` clear — the page's cover (SPEC §三十八). Persisted
    /// like the icon and outside undo for the same reason. Swapping a cover
    /// leaves its predecessor's bytes on disk on purpose, exactly the way
    /// replacing an image block does: the sweep in §三十七 is what frees them,
    /// once nothing — no block, and now no page — points at them.
    pub fn set_page_cover(&self, id: i32, cover: Option<crate::core::AttachmentId>) {
        let cover = self.workspace.borrow_mut().set_cover(id, cover);
        self.record(vec![Change::PageCoverSet {
            id: PageId(id as u32 as u64),
            cover,
        }]);
        self.apply_page_style();
    }

    /// Land a picture this session just imported behind the title (SPEC
    /// §三十八). The row is written before the page points at it — the same
    /// order a block's picture keeps, so a crash between the two leaves an
    /// orphan for the reclaim rather than a cover with nothing behind it.
    pub fn set_page_cover_from(&self, id: i32, attachment: Attachment) {
        let cover = attachment.id;
        self.attachments
            .borrow_mut()
            .insert(cover.as_u64() as i64, attachment);
        self.workspace.borrow_mut().set_cover(id, Some(cover));
        self.record(vec![
            Change::AttachmentAdded(self.attachments.borrow()[&(cover.as_u64() as i64)]
                .clone()),
            Change::PageCoverSet {
                id: PageId(id as u32 as u64),
                cover: Some(cover),
            },
        ]);
        self.apply_page_style();
    }

    /// Set or clear the page's read-only switch (SPEC §三十八 "lock"). Stored on
    /// the page and outside undo like every other page property (ADR-0044):
    /// locking is something you decide about a page, not something you did to
    /// its text, and an undo step that silently unlocked a page would be the
    /// one surprise this switch cannot allow.
    pub fn set_page_locked(&self, id: i32, locked: bool) {
        let Some(locked) = self.workspace.borrow_mut().set_locked(id, locked) else {
            return;
        };
        self.record(vec![Change::PageLockedSet {
            id: PageId(id as u32 as u64),
            locked,
        }]);
        self.apply_page_style();
    }

    /// Load the emoji grid with the picker's whole list and point it at `id`.
    /// The list is copied out of `core::icon::PICKER` — the .slint side holds no
    /// emoji of its own, so the grid cannot drift from what storage accepts,
    /// and adding a row to the catalogue is a one-line change there.
    pub fn fill_icon_picker(&self, id: i32) {
        let Some(ui) = self.ui.borrow().clone() else {
            return;
        };
        let g = ui.upgrade().unwrap();
        let items: Vec<slint::SharedString> = crate::core::icon::PICKER
            .iter()
            .map(|glyph| (*glyph).into())
            .collect();
        g.set_icon_picker_items(slint::ModelRc::from(std::rc::Rc::new(
            slint::VecModel::from(items),
        )));
        g.set_icon_picker_page(id);
    }

    /// Flip one of the two switches that share `pages.layout`. The pair is
    /// written because the column is one value; the other switch keeps its bit.
    fn set_page_layout(&self, id: i32, full_width: bool, small_text: bool) {
        let (full_width, small_text) = self
            .workspace
            .borrow_mut()
            .set_layout(id, full_width, small_text);
        self.record(vec![Change::PageLayoutSet {
            id: PageId(id as u32 as u64),
            full_width,
            small_text,
        }]);
        self.apply_page_style();
    }

    pub fn toggle_page_full_width(&self, id: i32) {
        let (_, fw, st) = self.workspace.borrow().page_style(id).unwrap_or_default();
        self.set_page_layout(id, !fw, st);
    }

    pub fn toggle_page_small_text(&self, id: i32) {
        let (_, fw, st) = self.workspace.borrow().page_style(id).unwrap_or_default();
        self.set_page_layout(id, fw, !st);
    }

    pub fn toggle_favorite(&self, id: i32) {
        self.workspace.borrow_mut().toggle_favorite(id);
        let favorite = self
            .workspace
            .borrow()
            .get(id)
            .map(|p| p.favorite)
            .unwrap_or(false);
        self.record(vec![Change::PageFavoriteSet {
            id: PageId(id as u32 as u64),
            favorite,
        }]);
        self.rebuild_sidebar();
    }

    pub fn toggle_expanded(&self, id: i32) {
        self.workspace.borrow_mut().toggle_expanded(id);
        let expanded = self
            .workspace
            .borrow()
            .get(id)
            .map(|p| p.expanded)
            .unwrap_or(false);
        self.record(vec![Change::PageExpandedSet {
            id: PageId(id as u32 as u64),
            expanded,
        }]);
        self.rebuild_sidebar();
    }

    /// Prepare the delete-confirmation dialog for `id`; the controller reads
    /// the text and shows the popup.
    pub fn delete_dialog_text(&self, id: i32) -> (String, String) {
        let ws = self.workspace.borrow();
        let title = ws.title_of(id).unwrap_or("").to_string();
        let n = ws.subtree_size(id);
        let message = if n > 1 {
            format!(
                "“{title}” and its {} sub-pages will be deleted. This cannot be undone.",
                n - 1
            )
        } else {
            format!("“{title}” will be deleted. This cannot be undone.")
        };
        self.pending_delete.set(Some(id));
        (title, message)
    }

    // ---- command palette ----

    pub fn set_query(&self, query: &str) {
        let filtered = if query.is_empty() {
            self.all_commands.clone()
        } else {
            self.all_commands
                .iter()
                .filter(|c| fuzzy_subsequence(query, &c.name))
                .cloned()
                .collect()
        };
        self.commands.set_vec(filtered);
    }

    // ---- search panel ----

    pub fn set_search_query(&self, query: &str) {
        if let Some(svc) = &self.search_service {
            if !query.trim().is_empty() {
                // hand the query to the worker thread; the controller polls
                // on a timer and the generation counter drops stale results
                let gen = self.next_search_generation();
                self.pending_search
                    .borrow_mut()
                    .replace((gen, svc.search_async(SearchRequest::new(query))));
                self.search.set_vec(Vec::new());
                return;
            }
            self.pending_search.borrow_mut().take();
        }
        let hits: Vec<SearchHit> = self.workspace.borrow().search(query);
        let rows: Vec<SearchRow> = hits
            .into_iter()
            .map(|h| SearchRow {
                page_id: h.id,
                title: h.title.into(),
                snippet: if h.snippet.is_empty() {
                    h.breadcrumb.into()
                } else {
                    h.snippet.into()
                },
                // the blob scan has no block addressing; only FTS hits do
                hit_block_id: -1,
            })
            .collect();
        self.search.set_vec(rows);
    }

    // ---- context menu ----

    /// Context-menu items for a page row (also used by the TopBar ⋯ menu).
    pub fn fill_menu(&self, id: i32) {
        let ws = self.workspace.borrow();
        let fav_label = if ws.get(id).map(|p| p.favorite).unwrap_or(false) {
            "Remove from favorites"
        } else {
            "Add to favorites"
        };
        let mut rows = vec![
            MenuRow {
                id: MENU_NEW_SUBPAGE,
                label: "New subpage".into(),
                icon: "plus".into(),
                danger: false,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
            MenuRow {
                id: MENU_RENAME,
                label: "Rename".into(),
                icon: "pencil".into(),
                danger: false,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
            MenuRow {
                id: MENU_DUPLICATE,
                label: "Duplicate".into(),
                icon: "copy".into(),
                danger: false,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
            MenuRow {
                id: MENU_MOVE_UP,
                label: "Move up".into(),
                icon: "chevron-up".into(),
                danger: false,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
            MenuRow {
                id: MENU_MOVE_DOWN,
                label: "Move down".into(),
                icon: "chevron-down".into(),
                danger: false,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
            MenuRow {
                id: MENU_MOVE_TO,
                label: "Move to".into(),
                icon: "arrow-right".into(),
                danger: false,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
            MenuRow {
                id: MENU_FAVORITE,
                label: fav_label.into(),
                icon: "star".into(),
                danger: false,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
            MenuRow {
                id: MENU_DELETE,
                label: "Delete".into(),
                icon: "trash".into(),
                danger: true,
                swatch: -1,
                swatch_bg: false,
                check: false,
            },
        ];
        // The look of the page itself, and above the one destructive row
        // (SPEC §三十八): the icon opens the emoji grid, the style submenu
        // holds the three switches.
        rows.insert(
            rows.len() - 1,
            row(MENU_PAGE_STYLE, "Style", "palette", false, -1, false),
        );
        rows.insert(
            rows.len() - 2,
            row(MENU_PAGE_ICON, "Set icon", "smile", false, -1, false),
        );
        // The cover sits beside the icon because it is the same kind of fact
        // about the page. Its label answers "is there one?", and the removal
        // only exists when there is — a greyed-out entry is a question, and
        // this menu is already long enough.
        let has_cover = ws.cover_of(id).is_some();
        let mut look = vec![row(
            MENU_PAGE_COVER,
            if has_cover { "Change cover" } else { "Set cover" },
            "image",
            false,
            -1,
            false,
        )];
        if has_cover {
            look.push(row(
                MENU_PAGE_COVER_REMOVE,
                "Remove cover",
                "trash",
                false,
                -1,
                false,
            ));
        }
        let at = rows.len() - 2;
        rows.splice(at..at, look);
        // The read-only switch, beside the other things a page decides about
        // itself (SPEC §三十八). Its label is the state, not the action's
        // opposite: "Lock page" on an open page, "Unlock page" on a shut one.
        rows.insert(
            rows.len() - 2,
            row(
                MENU_PAGE_LOCK,
                if ws.locked_of(id) { "Unlock page" } else { "Lock page" },
                "lock",
                false,
                -1,
                false,
            ),
        );
        // The library, as one row above the lock (SPEC §三十八 "模板"). It sits
        // with the things a page does rather than the things a page looks like,
        // and it is the only trace of templates this menu shows: what they hold
        // is in the submenu, and what the workspace holds is nobody's page.
        rows.insert(
            rows.len() - 2,
            row(MENU_PAGE_TEMPLATES, "Templates", "page", false, -1, false),
        );
        // Version history beside it (SPEC §三十八): the other row on this menu
        // about what the page *says*, rather than what it looks like or where it
        // sits. Found by id instead of by another `rows.len() - 2`, because the
        // five inserts above each re-decide what second-to-last means, and this
        // row's place is relative to a row, not to the end.
        let at = rows
            .iter()
            .position(|r| r.id == MENU_PAGE_TEMPLATES)
            .unwrap_or_else(|| rows.len() - 1);
        rows.insert(
            at,
            row(MENU_PAGE_VERSIONS, "Version history", "clock", false, -1, false),
        );
        self.menu.set_vec(rows);
    }

    /// The page menu's Style submenu: the three switches of SPEC §三十八's
    /// 页面版式, each showing what this page stores. Deliberately not
    /// undoable, like the favorite above it — a look is a property of the
    /// page, and Ctrl+Z on a page you were typing in must not be a font.
    pub fn fill_page_menu_style(&self, id: i32) {
        let (font, full_width, small_text) = self
            .workspace
            .borrow()
            .page_style(id)
            .unwrap_or_default();
        let mut rows = vec![row(MENU_BACK, "Back", "chevron-left", false, -1, false)];
        for (index, kind) in PageFont::ALL.iter().enumerate() {
            let mut menu_row = row(
                PAGE_FONT_BASE + index as i32,
                kind.label(),
                "",
                false,
                -1,
                false,
            );
            menu_row.check = *kind == font;
            rows.push(menu_row);
        }
        let mut width = row(MENU_PAGE_FULL_WIDTH, "Full width", "", false, -1, false);
        width.check = full_width;
        rows.push(width);
        let mut small = row(MENU_PAGE_SMALL_TEXT, "Small text", "", false, -1, false);
        small.check = small_text;
        rows.push(small);
        self.menu.set_vec(rows);
    }

    /// Second-level "Move to" menu for a page: every legal target — any page
    /// that is not the moved page and not inside its subtree (the walk skips
    /// the whole branch, so a cycle is impossible by construction) — plus
    /// "Top level" for moving to the root. Swaps the rows and keeps the
    /// popup open, like the block menu's mover.
    pub fn fill_page_menu_move_to(&self, id: i32) {
        let mut rows = vec![row(
            MENU_BACK,
            "Back",
            "chevron-left",
            false,
            -1,
            false,
        )];
        rows.push(row(
            PAGE_MOVE_TO_ROOT,
            "Top level",
            "export",
            false,
            -1,
            false,
        ));
        let ws = self.workspace.borrow();
        fn walk(
            ws: &Workspace,
            parent: Option<i32>,
            depth: usize,
            skip: i32,
            out: &mut Vec<MenuRow>,
        ) {
            for cid in ws.children_of(parent) {
                if cid == skip {
                    continue;
                }
                let indent = "\u{2003}".repeat(depth);
                let title = ws.title_of(cid).unwrap_or("Untitled");
                out.push(row(
                    PAGE_MOVE_TO_BASE + cid,
                    format!("{indent}{title}"),
                    "page",
                    false,
                    -1,
                    false,
                ));
                walk(ws, Some(cid), depth + 1, skip, out);
            }
        }
        walk(&ws, None, 0, id, &mut rows);
        drop(ws);
        self.menu.set_vec(rows);
    }

    /// ⋯ → Templates: the library's management surface (SPEC §三十八). The page
    /// menu gained one row for all of this rather than six, because the row's
    /// label is the feature's name and the submenu is where the choices live.
    ///
    /// There is no "edit template" row, and that is the design rather than an
    /// omission: a template is a body to copy from, so the way to change one is
    /// to start a page from it, edit the page, and save that as a template —
    /// which is two of the rows below plus Delete. It keeps a template off
    /// every editor surface, which is what makes it invisible in the first
    /// place. The cost is honest and written down: the older copy stays in the
    /// library until the user deletes it, because saving never overwrites.
    /// The labels are short on purpose: every popup in the app shares one
    /// 184px `ContextMenu`, and its Text rows elide rather than wrap, so a
    /// label that names its object twice — the submenu is already called
    /// Templates — ends in an ellipsis. The object is the row the user came
    /// from, and the picker that follows names it again.
    pub fn fill_template_menu(&self) {
        self.menu.set_vec(vec![
            row(MENU_BACK, "Back", "chevron-left", false, -1, false),
            row(MENU_TEMPLATE_INSERT, "Insert template", "plus", false, -1, false),
            row(MENU_TEMPLATE_NEW_PAGE, "Use as new page", "page", false, -1, false),
            row(MENU_TEMPLATE_SAVE, "Save as template", "copy", false, -1, false),
            row(MENU_TEMPLATE_EXPORT, "Export Markdown", "export", false, -1, false),
            row(MENU_TEMPLATE_IMPORT, "Import Markdown", "import", false, -1, false),
            row(MENU_TEMPLATE_DELETE, "Delete template", "trash", true, -1, false),
        ]);
    }

    /// The picker four of those rows open: one row per template, oldest first,
    /// labelled with its name and nothing else — the name is what the user chose
    /// and a hint column would only repeat the menu they came from.
    ///
    /// `action` is the row that opened this, remembered on `template_pick`
    /// because the menu model has no room for it and the click handler has no
    /// other way to know whether "Meeting notes" means *insert it*, *export it*
    /// or *delete it*. Back has its own id for the same reason: the generic
    /// `MENU_BACK` means "the page menu", and from here that would be wrong.
    pub fn fill_template_pick(&self, action: i32) {
        self.template_pick.set(action);
        let mut rows = vec![row(
            MENU_TEMPLATE_PICK_BACK,
            "Back",
            "chevron-left",
            false,
            -1,
            false,
        )];
        for (index, (_, title)) in self.template_list().iter().enumerate() {
            rows.push(row(
                TEMPLATE_PICK_BASE + index as i32,
                title.clone(),
                "page",
                action == MENU_TEMPLATE_DELETE,
                -1,
                false,
            ));
        }
        self.menu.set_vec(rows);
    }

    /// The template a picked row names, or `None` when the row was not a
    /// template. Out-of-band ids (an empty library, a row from the previous
    /// fill) read as nothing rather than as index 0.
    pub fn template_picked(&self, action: i32) -> Option<(i32, String)> {
        let index = action - TEMPLATE_PICK_BASE;
        if index < 0 {
            return None;
        }
        self.template_list().into_iter().nth(index as usize)
    }

    /// The action the picker was opened for. Read once when a row is clicked.
    pub fn template_pick_action(&self) -> i32 {
        self.template_pick.get()
    }

    // ---- version history (SPEC §三十八, ADR-0091) ----

    /// The library file a version would be copied from, or `None` for a session
    /// with no database at all. That `None` is not an error to report so much as
    /// the reason the panel has an empty state: a version *is* a file, so a
    /// session that writes no file can keep no version.
    fn version_db(&self) -> Option<std::path::PathBuf> {
        self.repo.as_ref()?.path().map(|p| p.to_path_buf())
    }

    /// One page's named versions, newest first, as `(created, label)`. Read from
    /// the mirror built at load and kept in step by every save and delete below,
    /// so filling the panel costs no read of its own.
    pub fn page_versions(&self, page: i32) -> Vec<(i64, String)> {
        self.version_index
            .borrow()
            .get(&page)
            .cloned()
            .unwrap_or_default()
    }

    /// The version a panel row names, as `(created, label)`. `row` is the row's
    /// index rather than a timestamp because Slint's `int` is 32 bits and a unix
    /// second is not; an index out of range is nothing, which is what a row that
    /// a delete removed between the click and here must resolve to.
    pub fn version_at(&self, page: i32, row: i32) -> Option<(i64, String)> {
        self.page_versions(page)
            .into_iter()
            .nth(row.max(0) as usize)
    }

    /// Name the way the page reads now, and keep it.
    ///
    /// The order of this is the feature. **Flush first**: `VACUUM INTO` reads the
    /// file, not this session's memory, so a version taken over a queue of
    /// unwritten edits would be a picture of the page as of the last flush — a
    /// version whose name promises "this" and delivers "ten seconds ago". Then
    /// the copy, then the two metadata rows that make it findable, then the
    /// retention cut, then the sweep of anything a crashed save left behind.
    ///
    /// A name that collides with a version taken in the same second moves
    /// forward a second rather than overwriting the older one: two versions with
    /// one timestamp is a lost version, and the user typed the label, so the
    /// label is the thing worth keeping.
    pub fn save_page_version(&self, page: i32, label: &str) -> Result<String, String> {
        let (Some(db_file), Some(repo)) = (self.version_db(), self.repo.clone()) else {
            return Err("this session has no database file, so a version has nowhere to live".into());
        };
        if page <= 0 {
            return Err("no page is open".into());
        }
        let label = version_label(label, self.page_versions(page).len());
        self.persistence_force_flush();

        let mut attempt = 0;
        let (created, ids) = loop {
            let created = now_secs() + attempt;
            match versions::save(&repo, page as i64, created) {
                Ok(Some(ids)) => break (created, ids),
                // the file name and the metadata key are both this second, so
                // four tries covers four saves in a row faster than a clock tick
                Ok(None) if attempt < 4 => attempt += 1,
                Ok(None) => return Err("a version taken this second already exists".into()),
                Err(e) => return Err(e.to_string()),
            }
        };

        // Past the cap the oldest go — the newest never, and never silently.
        let mut pruned: Vec<i64> = Vec::new();
        {
            let mut index = self.version_index.borrow_mut();
            let list = index.entry(page).or_default();
            list.push((created, label.clone()));
            list.sort_by(|a, b| b.0.cmp(&a.0));
            while list.len() > crate::storage::versions::MAX_PER_PAGE {
                match list.pop() {
                    Some((old, _)) => pruned.push(old),
                    None => break,
                }
            }
        }
        self.version_pins
            .borrow_mut()
            .insert((page as i64, created), ids.iter().copied().collect());

        let mut changes = vec![
            Change::MetaSet {
                key: versions::label_key(page as i64, created),
                value: label.clone(),
            },
            Change::MetaSet {
                key: versions::files_key(page as i64, created),
                value: ids
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            },
        ];
        for old in pruned {
            // The file goes before its rows are deleted: the other order leaves
            // a row naming a file that is gone, which the panel would offer and
            // `read` would refuse. A file left by this call failing instead is
            // what the sweep below is for.
            if let Err(e) = versions::remove(&db_file, page as i64, old) {
                eprintln!("quire: {e}");
            }
            self.version_pins.borrow_mut().remove(&(page as i64, old));
            changes.push(Change::MetaDelete {
                key: versions::label_key(page as i64, old),
            });
            changes.push(Change::MetaDelete {
                key: versions::files_key(page as i64, old),
            });
        }
        self.record(changes);
        self.sweep_version_files();
        Ok(label)
    }

    /// Forget every version file the index no longer names. A save that died
    /// between the copy and its metadata row is the case this exists for: with
    /// nothing left to list it, that file would be invisible to the panel and to
    /// the §三十七 disk numbers both.
    fn sweep_version_files(&self) {
        let Some(db_file) = self.version_db() else { return };
        let keep: BTreeSet<(i64, i64)> = self
            .version_index
            .borrow()
            .iter()
            .flat_map(|(page, list)| {
                list.iter()
                    .map(move |(created, _)| (*page as i64, *created))
            })
            .collect();
        if let Err(e) = versions::sweep_orphans(&db_file, &keep) {
            eprintln!("quire: {e}");
        }
    }

    /// The comparison rows between one version and what is on screen now
    /// (SPEC §三十八 "与当前版本对比"). The version's blocks come back through
    /// `Repository::load` — the loader the app opens the library with — so a
    /// version cannot be a format this code has to keep up with.
    pub fn version_diff(
        &self,
        page: i32,
        created: i64,
    ) -> Result<Vec<crate::core::diff::DiffLine>, String> {
        let db_file = self.version_db().ok_or("no database file")?;
        let (_version_page, blocks) =
            versions::read(&db_file, page as i64, created).map_err(|e| e.to_string())?;
        let current = {
            let doc = self.doc.borrow();
            doc.page_blocks(core_page_id(page)).to_vec()
        };
        Ok(crate::core::diff::compare(&blocks, &current))
    }

    /// Land a version's blocks in place of the page's current ones, as ONE undo
    /// step (SPEC §三十八 "恢复").
    ///
    /// This goes through the command system rather than swapping files, and that
    /// choice is the feature: the restored rows get fresh ids and re-derived
    /// order keys, so they behave like anything typed since, Ctrl+Z brings back
    /// exactly what this replaced, and the change feed puts them in the database
    /// the way every other edit arrives. Deleting only the page's *roots* is
    /// enough because a delete takes its subtree with it — which is also how a
    /// table's cells and a columns layout's boxes go.
    ///
    /// The page keeps its title, icon, cover and style on purpose. A version is
    /// of a page's *content*, and rolling back a name the user chose after the
    /// version was taken is not what "restore this version" reads as.
    pub fn restore_version(&self, page: i32, created: i64) -> Result<usize, String> {
        if page != self.open_page.get() {
            // The command system addresses the page on screen, and the panel
            // only ever shows the open page's versions, so reaching here for
            // another one means a row went stale under a page switch.
            return Err("open that page before restoring its version".into());
        }
        if self.locked_refusal() {
            return Err("this page is locked".into());
        }
        let db_file = self.version_db().ok_or("no database file")?;
        let (_version_page, blocks) =
            versions::read(&db_file, page as i64, created).map_err(|e| e.to_string())?;
        let roots: Vec<Command> = {
            let doc = self.doc.borrow();
            doc.page_blocks(core_page_id(page))
                .iter()
                .filter(|b| b.parent.is_none())
                .map(|b| Command::DeleteBlock { id: b.id })
                .collect()
        };
        let count = blocks.len();
        let mut cmds = vec![Command::InsertForest {
            anchor: None,
            blocks,
        }];
        // `exec_all` plans every command against the state before the batch, so
        // the insert lands after the rows the deletes are about to remove and
        // neither step has to know about the other.
        cmds.extend(roots);
        if self.exec_all_on_open_page(cmds).is_none() {
            return Err("there was nothing to restore".into());
        }
        Ok(count)
    }

    /// Drop a version: its file, its two metadata rows, and its entry in the
    /// mirror. The file goes first, for the same reason the retention cut does.
    pub fn delete_version(&self, page: i32, created: i64) -> Result<(), String> {
        let db_file = self.version_db().ok_or("no database file")?;
        versions::remove(&db_file, page as i64, created).map_err(|e| e.to_string())?;
        if let Some(list) = self.version_index.borrow_mut().get_mut(&page) {
            list.retain(|(at, _)| *at != created);
        }
        self.version_pins.borrow_mut().remove(&(page as i64, created));
        self.record(vec![
            Change::MetaDelete {
                key: versions::label_key(page as i64, created),
            },
            Change::MetaDelete {
                key: versions::files_key(page as i64, created),
            },
        ]);
        Ok(())
    }

    /// The attachment ids stored versions still point at, for §三十七's reclaim.
    /// A version file is a database the reclaim scan never opens, so its
    /// pointers are mirrored into the live library at save time — and a picture
    /// the reclaim freed would make that version restore as a missing image with
    /// nothing left to explain it (ADR-0047's objection, arriving from the other
    /// direction).
    pub fn version_pinned_attachments(&self) -> Vec<i64> {
        self.version_pins
            .borrow()
            .values()
            .flat_map(|set| set.iter().copied())
            .collect()
    }

    /// A page that is gone has no version worth keeping: the metadata rows, the
    /// files, and the pins that were holding its pictures alive.
    /// `delete_page` calls this for the page and each descendant it removes.
    fn forget_versions(&self, page: i32) -> Vec<Change> {
        let gone = self.version_index.borrow_mut().remove(&page).unwrap_or_default();
        let mut changes = Vec::with_capacity(gone.len() * 2);
        for (created, _) in gone {
            self.version_pins.borrow_mut().remove(&(page as i64, created));
            if let Some(db_file) = self.version_db() {
                if let Err(e) = versions::remove(&db_file, page as i64, created) {
                    eprintln!("quire: {e}");
                }
            }
            changes.push(Change::MetaDelete {
                key: versions::label_key(page as i64, created),
            });
            changes.push(Change::MetaDelete {
                key: versions::files_key(page as i64, created),
            });
        }
        changes
    }

    /// Push the panel's list view for one page. Called on open and after every
    /// save, delete and restore, so the rows the user sees are the rows the
    /// library has.
    pub fn fill_versions(&self, page: i32) {
        let rows = version_rows(&self.page_versions(page));
        let note = versions_note(rows.len());
        self.versions.set_vec(rows);
        if let Some(g) = self.ui.borrow().clone().and_then(|u| u.upgrade()) {
            g.set_versions_note(note.into());
        }
    }

    /// Push the panel's comparison view. `heading` names the version, `note`
    /// says what pressing the primary button would do.
    pub fn fill_version_diff(&self, lines: &[crate::core::diff::DiffLine], heading: &str, note: &str) {
        let rows: Vec<DiffRow> = lines
            .iter()
            .map(|l| DiffRow {
                added: l.mark == crate::core::diff::DiffMark::Added,
                kind: crate::core::diff::kind_label(l.kind).into(),
                text: l.text.as_str().into(),
            })
            .collect();
        self.version_diff.set_vec(rows);
        if let Some(g) = self.ui.borrow().clone().and_then(|u| u.upgrade()) {
            g.set_versions_heading(heading.into());
            g.set_versions_note(note.into());
        }
    }

    /// Benchmark scene F: the controller reads/writes the editor viewport-y
    /// property and uses this cell to detect "hit the bottom" (position
    /// stopped changing) so the scroll can wrap.
    pub fn last_scroll_y(&self) -> f32 {
        self.last_scroll_y.get()
    }
    pub fn set_last_scroll_y(&self, v: f32) {
        self.last_scroll_y.set(v);
    }
}

// Sample page ids (stable, used by scenes/tests).
pub const PAGE_WEEKLY_REVIEW: i32 = 100;
pub const PAGE_GETTING_STARTED: i32 = 102;
pub const PAGE_ATLAS: i32 = 105;
pub const PAGE_CHINESE: i32 = 112;
pub const PAGE_SCRATCHPAD: i32 = 113;

pub const ROW_NEW_PAGE: i32 = -2;

// Context-menu action ids.
pub const MENU_NEW_SUBPAGE: i32 = 1;
pub const MENU_RENAME: i32 = 2;
pub const MENU_DUPLICATE: i32 = 3;
pub const MENU_FAVORITE: i32 = 4;
pub const MENU_DELETE: i32 = 5;
pub const MENU_MOVE_UP: i32 = 6;
pub const MENU_MOVE_DOWN: i32 = 7;
pub const MENU_MOVE_TO: i32 = 8;
pub const MENU_BACK: i32 = 9;
/// The page menu's "Style" submenu (SPEC §三十八).
pub const MENU_PAGE_STYLE: i32 = 10;
pub const MENU_PAGE_FULL_WIDTH: i32 = 11;
pub const MENU_PAGE_SMALL_TEXT: i32 = 12;
/// The page menu's "Set icon" row, which opens the emoji grid (SPEC §三十八).
pub const MENU_PAGE_ICON: i32 = 13;
/// The page menu's cover rows (SPEC §三十八 "图标与封面"). "Set cover" and
/// "Change cover" are one id — both open the picture picker and store whatever
/// comes back — and the remove only appears when the page has a cover.
pub const MENU_PAGE_COVER: i32 = 14;
pub const MENU_PAGE_COVER_REMOVE: i32 = 15;
/// ⋯ → Lock page / Unlock page (SPEC §三十八 "lock"). One id for both
/// directions because the row's label already answers which one it is.
pub const MENU_PAGE_LOCK: i32 = 16;
/// ⋯ → Templates, and the seven rows of that submenu (SPEC §三十八 "模板").
/// Four of them (`INSERT`, `NEW_PAGE`, `EXPORT`, `DELETE`) open a picker that
/// lists the library; three act at once.
pub const MENU_PAGE_TEMPLATES: i32 = 17;
pub const MENU_TEMPLATE_INSERT: i32 = 18;
pub const MENU_TEMPLATE_NEW_PAGE: i32 = 19;
pub const MENU_TEMPLATE_SAVE: i32 = 20;
pub const MENU_TEMPLATE_EXPORT: i32 = 21;
pub const MENU_TEMPLATE_IMPORT: i32 = 22;
pub const MENU_TEMPLATE_DELETE: i32 = 23;
/// Back out of the template picker. Not `MENU_BACK`, which means "the page
/// menu" and would drop two levels at once.
pub const MENU_TEMPLATE_PICK_BACK: i32 = 24;
/// ⋯ → Version history, the one row the page menu gains for the whole feature
/// (SPEC §三十八, ADR-0091). One row and not three, because the page menu is
/// already thirteen deep and every popup in this app shares one 184px
/// `ContextMenu` whose rows elide rather than wrap: "Compare versions" and
/// "Restore version" would both arrive as "…". The panel this row opens holds
/// the list, the comparison and the restore, and says which version each is.
pub const MENU_PAGE_VERSIONS: i32 = 25;
/// The picker's rows: index into `template_list()`, oldest template first.
pub const TEMPLATE_PICK_BASE: i32 = 800_000;
/// The "+" / slash menu's template rows, in the same index space. A separate
/// band because that popup's other ids are block kinds, and `kind_from_int`
/// answers an unknown id with a paragraph.
pub const TEMPLATE_SLASH_BASE: i32 = 900_000;
/// The settings row that says this library has already been given the built-in
/// templates (SPEC §三十八). Its whole job is to make deleting one final.
pub const SEEDED_BUILTIN_TEMPLATES: &str = "builtin-templates-seeded";
/// "Top level" target of the page-menu Move-to submenu (root, `None` parent).
pub const PAGE_MOVE_TO_ROOT: i32 = 499_999;
/// Page-menu Move-to targets encode the destination page above this base.
pub const PAGE_MOVE_TO_BASE: i32 = 500_000;
/// Style-submenu font picks encode the index into `PageFont::ALL`.
pub const PAGE_FONT_BASE: i32 = 700_000;
/// SidebarNode id of the Workspace section header — the drag-drop target
/// that moves a page to the top level. Page rows target themselves.
pub const WORKSPACE_HEADER_ID: i32 = -100;

fn header(label: &str) -> SidebarNode {
    SidebarNode {
        id: -1,
        label: label.into(),
        kind: "header".into(),
        depth: 0,
        expanded: false,
        has_children: false,
        selected: false,
        y: 0,
        icon: "".into(),
    }
}

fn leaf_row(
    id: i32,
    label: &str,
    kind: &str,
    selected: bool,
    icon: slint::SharedString,
) -> SidebarNode {
    SidebarNode {
        id,
        label: label.into(),
        kind: kind.into(),
        depth: 0,
        expanded: false,
        has_children: false,
        selected,
        y: 0,
        icon,
    }
}

/// What one tree row's slot draws (SPEC §三十八): the page's own emoji, or the
/// placeholder an iconless page falls back to. A section header and the
/// "New page" row pass no id and keep the vector glyph the delegate draws.
fn icon_slot(ws: &Workspace, id: i32, title: &str) -> slint::SharedString {
    crate::core::icon::slot(&ws.icon_of(id), title).into()
}

/// What a Favorites / Recent row draws: the page's emoji **only**. The
/// first-character placeholder belongs to the tree, where it replaces a
/// generic page glyph; in a shortcut list it would erase the star and the
/// clock that say which section the row is in.
fn icon_mark(ws: &Workspace, id: i32) -> slint::SharedString {
    ws.icon_of(id).into()
}

// ---- block content ----

// ---- core <-> projection bridge ----

/// M2 mock page ids (i32) map into the u64 core id space unchanged.
pub fn core_page_id(id: i32) -> PageId {
    PageId(id as u32 as u64)
}

/// Slash-menu descriptors: Rust owns the list (SPEC §十五), the UI only
/// renders labels. ids are BlockKind ints (see kind_from_int). Kinds with a
/// markdown line-shortcut ("# ", "- ", …) are deliberately absent — typing
/// the symbol converts, so the menu only lists the rest (ADR-0022).
const SLASH_ITEMS: &[(BlockKind, &str, &str)] = &[
    (BlockKind::Paragraph, "Text", "Plain paragraph"),
    (BlockKind::Toggle, "Toggle list", "Collapsible section"),
    (BlockKind::Image, "Image", "Embed a picture from a file"),
    (BlockKind::File, "File", "Attach a file of any type"),
    (BlockKind::Table, "Table", "Simple grid of cells"),
    (BlockKind::Columns, "Columns", "Two columns of blocks, side by side"),
    (BlockKind::Callout, "Callout", "Highlighted box with an emoji"),
    (BlockKind::Code, "Code", "Monospaced block — or type ```"),
    (BlockKind::Math, "Math", "LaTeX formula — or type $$"),
    (
        BlockKind::Toc,
        "Table of contents",
        "Links to this page's headings",
    ),
    (
        BlockKind::Embed,
        "Embed",
        "A link as a card — paste an address",
    ),
    // SPEC §三十九 (D3): the database's first view. One entry, not six — the six
    // `INSERT_ITEMS` rows for the other layouts light up one phase at a time
    // (ADR-0060: they are `db_views.layout`, not block kinds), and this is the
    // curated "/" menu, which lists what exists rather than what is planned.
    (
        BlockKind::Database,
        "Table view",
        "A database, as a table",
    ),
    (BlockKind::Divider, "Divider", "Visual separator — or type ---"),
    (
        BlockKind::Synced,
        "Synced block",
        "A second view of another block — edit either one",
    ),
];

/// "Turn into" targets: the same curation as the slash menu.
const TURN_INTO_ITEMS: &[(BlockKind, &str, &str)] = SLASH_ITEMS;

/// Insert-menu ("+" handle) descriptors: the full Notion-style list, unlike
/// the curated "/" menu (ADR-0022). Rows whose id is a BlockKind int are
/// insertable today; id < 0 marks the kinds still on the roadmap (SPEC
/// §三十七, §三十九) as disabled placeholders, so the menu shape matches
/// Notion and the roadmap stays visible. Keyboard navigation skips
/// placeholders and applying to one is a no-op.
const INSERT_ITEMS: &[(i32, &str, &str)] = &[
    (kind_to_int(BlockKind::Paragraph), "Text", "Plain paragraph"),
    (kind_to_int(BlockKind::Page), "Page", "Embed a child page"),
    (kind_to_int(BlockKind::Link), "Link to page", "Point at an existing page"),
    (kind_to_int(BlockKind::Image), "Image", "Embed a picture from a file"),
    (kind_to_int(BlockKind::File), "File", "Attach a file of any type"),
    (kind_to_int(BlockKind::Todo), "To-do list", "Track tasks with a checkbox"),
    (kind_to_int(BlockKind::Heading1), "Heading 1", "Big section heading"),
    (kind_to_int(BlockKind::Heading2), "Heading 2", "Medium section heading"),
    (kind_to_int(BlockKind::Heading3), "Heading 3", "Small section heading"),
    (kind_to_int(BlockKind::Table), "Table", "Simple grid of cells"),
    (kind_to_int(BlockKind::Columns), "Columns", "Side-by-side columns"),
    (kind_to_int(BlockKind::Bullet), "Bulleted list", "Simple bulleted list"),
    (kind_to_int(BlockKind::Numbered), "Numbered list", "Ordered list"),
    (kind_to_int(BlockKind::Toggle), "Toggle list", "Collapsible section"),
    (kind_to_int(BlockKind::Quote), "Quote", "Capture a quote"),
    (kind_to_int(BlockKind::Divider), "Divider", "Visual separator"),
    (kind_to_int(BlockKind::Callout), "Callout", "Highlighted box with an emoji"),
    (kind_to_int(BlockKind::Code), "Code", "Monospaced block"),
    (kind_to_int(BlockKind::Math), "Math", "LaTeX formula, rendered as Unicode"),
    (
        kind_to_int(BlockKind::Toc),
        "Table of contents",
        "Links to this page's headings",
    ),
    (
        kind_to_int(BlockKind::Embed),
        "Embed",
        "A link as a card, opened in the browser",
    ),
    // SPEC §三十九's six placeholders, and D3 lights the first of them: a real
    // block-kind int makes this row insertable, and the menu shows it exactly as
    // it shows every other kind. The other five stay muted (`id < 0`) because
    // their layouts are D5's — the row is the promise, and D3 keeps one of them.
    (
        kind_to_int(BlockKind::Database),
        "Table view",
        "A database, as a table",
    ),
    (
        kind_to_int(BlockKind::Synced),
        "Synced block",
        "A second view of another block — edit either one",
    ),
    // D7 (ADR-0085): a *linked database* — a second block drawing an entity
    // that exists already. `LINKED_VIEW_ROW` is not a kind: the apply path
    // switches to the database picker instead of converting directly, because
    // "which database" is the one decision the row cannot make for you.
    (
        LINKED_VIEW_ROW,
        "Linked view",
        "A linked view of another database",
    ),
    (-1, "Board", "Board view · later"),
    (-1, "Gallery", "Gallery view · later"),
    (-1, "List view", "Database list · later"),
    (-1, "Calendar", "Calendar view · later"),
    (-1, "Timeline", "Timeline view · later"),
];

fn slash_items(filter: &str) -> Vec<SlashRow> {
    let needle = filter.to_lowercase();
    let matches = |label: &str| needle.is_empty() || label.to_lowercase().contains(&needle);
    let mut rows: Vec<SlashRow> = SLASH_ITEMS
        .iter()
        .filter(|(_, label, _)| matches(label))
        .map(|(kind, label, hint)| SlashRow {
            id: kind_to_int(*kind),
            label: (*label).into(),
            hint: (*hint).into(),
            disabled: false,
        })
        .collect();
    // D7 (ADR-0085): the linked database, listed by name like every other row.
    // Its id is the picker's, not a kind's (`LINKED_VIEW_ROW`), and the apply
    // path handles it before the kind mapping ever sees it.
    if matches("Linked view") {
        rows.push(SlashRow {
            id: LINKED_VIEW_ROW,
            label: "Linked view".into(),
            hint: "A linked view of another database".into(),
            disabled: false,
        });
    }
    rows
}

/// MenuRow constructor for the ⋮⋮ menu fillers (`swatch < 0` = no swatch).
#[allow(clippy::too_many_arguments)]
fn row(id: i32, label: impl AsRef<str>, icon: &str, danger: bool, swatch: i32, swatch_bg: bool) -> MenuRow {
    MenuRow {
        id,
        label: label.as_ref().into(),
        icon: icon.into(),
        danger,
        swatch,
        swatch_bg,
        check: false,
    }
}

/// BlockKind int (UI menu ids) -> kind. Public: the controller resolves
/// Turn-into menu actions with it.
pub fn kind_from_int(kind: i32) -> BlockKind {
    match kind {
        1 => BlockKind::Heading1,
        2 => BlockKind::Heading2,
        3 => BlockKind::Heading3,
        4 => BlockKind::Bullet,
        5 => BlockKind::Numbered,
        6 => BlockKind::Todo,
        7 => BlockKind::Quote,
        8 => BlockKind::Code,
        9 => BlockKind::Divider,
        10 => BlockKind::Callout,
        11 => BlockKind::Page,
        12 => BlockKind::Link,
        13 => BlockKind::Toggle,
        14 => BlockKind::Image,
        15 => BlockKind::File,
        16 => BlockKind::Table,
        17 => BlockKind::TableCell,
        18 => BlockKind::Columns,
        19 => BlockKind::Column,
        20 => BlockKind::Math,
        21 => BlockKind::Toc,
        22 => BlockKind::Embed,
        // SPEC §三十九's database view sits between Embed and Synced in the
        // enum, and this map follows declaration order, so 23 is its number.
        // Filled in here rather than left empty because a gap would silently
        // `kind_from_int(23)` into a paragraph the day somebody types it.
        23 => BlockKind::Database,
        // SPEC §四十 / ADR-0052: the mirror.
        24 => BlockKind::Synced,
        _ => BlockKind::Paragraph,
    }
}

const fn kind_to_int(kind: BlockKind) -> i32 {
    match kind {
        BlockKind::Heading1 => 1,
        BlockKind::Heading2 => 2,
        BlockKind::Heading3 => 3,
        BlockKind::Bullet => 4,
        BlockKind::Numbered => 5,
        BlockKind::Todo => 6,
        BlockKind::Quote => 7,
        BlockKind::Code => 8,
        BlockKind::Divider => 9,
        BlockKind::Callout => 10,
        BlockKind::Page => 11,
        BlockKind::Link => 12,
        BlockKind::Toggle => 13,
        BlockKind::Image => 14,
        BlockKind::File => 15,
        BlockKind::Table => 16,
        BlockKind::TableCell => 17,
        BlockKind::Columns => 18,
        BlockKind::Column => 19,
        BlockKind::Math => 20,
        BlockKind::Toc => 21,
        BlockKind::Embed => 22,
        BlockKind::Database => 23,
        BlockKind::Synced => 24,
        BlockKind::Paragraph => 0,
    }
}

/// Nesting depth of one block (ancestors within the same page), bounded —
/// M4 renders a single indent level.
fn block_depth(blocks: &[Block], b: &Block) -> i32 {
    let mut depth = 0i32;
    let mut parent = b.parent;
    while let Some(pid) = parent {
        depth += 1;
        if depth >= 4 {
            break;
        }
        match blocks.iter().find(|x| x.id == pid) {
            Some(x) => parent = x.parent,
            None => break,
        }
    }
    depth
}

/// The hits of one block, as a slice. An empty map is the common case: a page
/// with a find bar closed asks this for every row and gets `&[]` every time.
fn hits_of<'a>(hits: &'a FindHits, id: BlockId) -> &'a [(usize, usize)] {
    hits.get(&(id.0 as i32))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// What a projection knows about the page one mention points at.
pub enum MentionTitle<'a> {
    /// The page is there, and this is what it is called **now** — the whole
    /// point of storing an id (SPEC §四十 "页面别名").
    Live(&'a str),
    /// The id names no page in this library: deleted, or a restored file whose
    /// page never came with it. The chip says so; the projection continues
    /// (ADR-0026 / ADR-0029 — a dangling reference is a display state, never a
    /// failure).
    Missing,
    /// This projection did not collect the id. The span then keeps the
    /// characters it already holds — which is what the search index wants: a
    /// blob built for a page copy must not index "(deleted page)" over a
    /// title that is perfectly fine, and the exporter reads the block's own
    /// text rather than a projection anyway.
    Unresolved,
}

/// The live titles behind the page ids one projection's mention spans point
/// at (SPEC §四十). A mention stores an id, so the label is not in the block
/// at all — it is asked for here, once per projection.
///
/// Keyed on the ids the page actually mentions rather than on every page in
/// the workspace: a rename then moves every chip that points at the page with
/// no write anywhere, and a projection still costs O(mentions in this page)
/// instead of O(pages in the library), which is what keeps a 10 000-page
/// library's structural edits cheap.
#[derive(Default, Clone)]
pub struct MentionTitles {
    by_id: HashMap<i32, Option<String>>,
}

impl MentionTitles {
    /// No titles collected: every mention falls back to the text it stores.
    /// The projection tests want this; a live projection never does.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Collect every page id `blocks` mentions and resolve each once through
    /// `title_of` (`None` = the page is gone). Duplicate mentions of the same
    /// page cost one lookup between them.
    pub fn of_blocks(blocks: &[Block], title_of: impl Fn(i32) -> Option<String>) -> Self {
        let mut by_id: HashMap<i32, Option<String>> = HashMap::new();
        for b in blocks {
            for m in &b.marks {
                if m.kind != crate::core::MarkKind::Mention {
                    continue;
                }
                let Some(id) = crate::core::page_of(&m.url) else {
                    continue;
                };
                let id = id.as_u64() as i32;
                if !by_id.contains_key(&id) {
                    let t = title_of(id);
                    by_id.insert(id, t);
                }
            }
        }
        Self { by_id }
    }

    pub fn lookup(&self, id: i32) -> MentionTitle<'_> {
        match self.by_id.get(&id) {
            Some(Some(title)) => MentionTitle::Live(title),
            Some(None) => MentionTitle::Missing,
            None => MentionTitle::Unresolved,
        }
    }
}

/// The label a mention whose page is gone reads as. One string, one place:
/// the projection and its tests must not disagree about what gone looks like.
pub const DELETED_PAGE_LABEL: &str = "(deleted page)";
/// What a `Synced` row reads when its source cannot be found: the source was
/// deleted, or the file was edited by hand, or the pointer was never set.
/// All three are the same answer because "why" is not something the row can
/// know, and a guess would be worse than the honest blank (ADR-0052 §2).
pub const DELETED_SOURCE_LABEL: &str = "(deleted source)";
/// How far `set_sync_source` follows `sync_ref` looking for the block it is
/// about to point at. **The cycle check is bounded, not recursive**: a file
/// someone edited by hand can contain any shape at all, and the refusal has to
/// be decided in bounded time every time (ADR-0052 §4).
pub const SYNC_CHAIN_MAX: usize = 32;
/// How many hops the projection follows while looking for real words. A chain
/// of mirrors is legal; this is what keeps an old backup's cycle from being a
/// hang instead of a picture that is merely wrong.
pub const SYNC_RESOLVE_MAX: usize = 8;
/// Rows the mirror picker offers. Every block in the library is a candidate,
/// which is unbounded by construction, so the list is a window rather than an
/// index — a picker you have to scroll through a thousand rows of is a search
/// box wearing the wrong clothes.
pub const SYNC_PICKER_LIMIT: usize = 50;

fn runs_to_model(b: &Block, hits: &[(usize, usize)], titles: &MentionTitles) -> slint::ModelRc<TextRun> {
    slint::ModelRc::from(Rc::new(slint::VecModel::from(build_runs(
        &b.text, &b.marks, hits, titles,
    ))))
}

/// Convert mock template rows into core Blocks with fresh ids and order
/// keys (order = the row order).
fn rows_to_blocks(page: i32, rows: Vec<BlockRow>, doc: &mut Document) -> Vec<Block> {
    let pid = core_page_id(page);
    let mut prev: Option<OrderKey> = None;
    rows.into_iter()
        .map(|row| {
            let order = OrderKey::between(prev, None).expect("order space exhausted");
            prev = Some(order);
            Block {
                id: doc.alloc_block_id(),
                page: pid,
                parent: None,
                order,
                kind: kind_from_int(row.kind),
                text: row.text.to_string(),
                checked: row.checked,
                marks: Vec::new(),
                color: ColorKind::Default,
                background: ColorKind::Default,
                page_ref: None,
                folded: false,
                attachment: None,
                img_percent: 100,
                columns: 0,
                lang: Lang::Plain,
                db_ref: None,
                sync_ref: None,            }
        })
        .collect()
}

/// Split text into runs: one per mark change, and one per word inside an
/// unmarked stretch. The delegate lays the runs out with a wrapping flexbox
/// and a run is one cell, so it cannot break — cutting the plain stretches
/// to words is what gives a marked line anywhere to wrap (ADR-0041).
///
/// `hits` are the find bar's occurrences in this same byte space. Each is one
/// more boundary, and the cell it makes carries `hit` so the delegate can tint
/// it — which is also why a row with no marks but a hit returns runs at all:
/// an empty vec means "render `text` as one unbroken Text", and a row with a
/// match in it cannot say that.
fn build_runs(
    text: &str,
    marks: &[crate::core::Mark],
    hits: &[(usize, usize)],
    titles: &MentionTitles,
) -> Vec<TextRun> {
    if (marks.is_empty() && hits.is_empty()) || text.is_empty() {
        return Vec::new();
    }
    let len = text.len();
    let mut bounds: Vec<usize> = vec![0, len];
    for m in marks {
        for v in [m.start.min(len), m.end.min(len)] {
            if text.is_char_boundary(v) {
                bounds.push(v);
            }
        }
    }
    for &(hs, he) in hits {
        // A mark's span is never cut: the cell it makes is one `Text`, and a
        // formula cell in particular renders text this byte space does not
        // describe (`\alpha` shows as α). A hit that starts or ends inside a
        // mark therefore tints the whole mark rather than part of it.
        for v in [hs.min(len), he.min(len)] {
            let inside = marks.iter().any(|m| m.start < v && v < m.end);
            if !inside && text.is_char_boundary(v) {
                bounds.push(v);
            }
        }
    }
    bounds.sort_unstable();
    bounds.dedup();
    let covered = |s: usize, e: usize| {
        marks.iter().any(|m| m.start <= s && m.end >= e)
            || hits.iter().any(|(hs, he)| hs < &e && he > &s)
    };
    let mut split: Vec<usize> = Vec::with_capacity(bounds.len() + 8);
    split.push(bounds[0]);
    for w in bounds.windows(2) {
        let (s, e) = (w[0], w[1]);
        if s < e && !covered(s, e) {
            // cut at the start of every word but the first, so the whitespace
            // that ends a word stays on it — the cell then reads as the shaper
            // reads it: word, then the break, then the space it hung on.
            let mut word = false;
            let mut prev_ws = true;
            for (i, ch) in text[s..e].char_indices() {
                let ws = ch.is_ascii_whitespace();
                if word && !ws && prev_ws {
                    split.push(s + i);
                }
                word |= !ws;
                prev_ws = ws;
            }
        }
        split.push(e);
    }
    bounds = split;
    bounds
        .windows(2)
        .filter_map(|w| {
            let (s, e) = (w[0], w[1]);
            if s == e {
                return None;
            }
            let link_mark = marks
                .iter()
                .find(|m| m.kind == crate::core::MarkKind::Link && m.start <= s && m.end >= e);
            // a formula run shows its glyphs, not its source: the document
            // keeps `\alpha`, the row shows α. Converted here rather than in
            // the delegate, so the cost is one pass per projection and not one
            // per binding evaluation.
            let math = marks
                .iter()
                .any(|m| m.kind == crate::core::MarkKind::Math && m.start <= s && m.end >= e);
            // SPEC §四十: a mention is an atom like Math, and its *label* is
            // not in the block. The span holds the title as it was typed; what
            // gets drawn is whatever the target page is called now, or the
            // deleted notice when the id names no page. Resolved here, at
            // projection time, rather than in a binding: a binding pays per
            // repaint, and this walks the page once (the same reason math
            // converts here — ADR-0038's note in ROADMAP).
            let mention = marks
                .iter()
                .find(|m| m.kind == crate::core::MarkKind::Mention && m.start <= s && m.end >= e)
                .and_then(|m| crate::core::page_of(&m.url).map(|id| (m, id.as_u64() as i32)));
            let date = marks
                .iter()
                .any(|m| m.kind == crate::core::MarkKind::Date && m.start <= s && m.end >= e);
            let run_text = &text[s..e];
            let (run_text, mention_deleted) = match mention {
                Some((_, id)) => match titles.lookup(id) {
                    MentionTitle::Live(title) => (title.to_string(), false),
                    MentionTitle::Missing => (DELETED_PAGE_LABEL.to_string(), true),
                    MentionTitle::Unresolved => (run_text.to_string(), false),
                },
                None => (run_text.to_string(), false),
            };
            Some(TextRun {
                text: if math {
                    crate::core::math::to_unicode(&run_text).into()
                } else {
                    run_text.as_str().into()
                },
                bold: marks
                    .iter()
                    .any(|m| m.kind == crate::core::MarkKind::Bold && m.start <= s && m.end >= e),
                italic: marks
                    .iter()
                    .any(|m| m.kind == crate::core::MarkKind::Italic && m.start <= s && m.end >= e),
                strike: marks
                    .iter()
                    .any(|m| m.kind == crate::core::MarkKind::Strike && m.start <= s && m.end >= e),
                code: marks
                    .iter()
                    .any(|m| m.kind == crate::core::MarkKind::Code && m.start <= s && m.end >= e),
                link: link_mark.is_some(),
                // The address this run points at, for a link *and* for a
                // mention: the delegate's click path is one `open-link`, and
                // `quire://page/<id>` is already a route it knows
                // (ADR-0026 — a mention and a link-to-page end up in the
                // same place, which is what makes the chip's click need no new
                // command at all).
                url: link_mark
                    .map(|m| m.url.clone())
                    .or_else(|| mention.map(|(m, _)| m.url.clone()))
                    .unwrap_or_default()
                    .into(),
                hit: hits.iter().any(|(hs, he)| hs < &e && he > &s),
                mention: mention.map(|(_, id)| id).unwrap_or(-1),
                mention_deleted,
                date,
            })
        })
        .collect()
}

/// True when `b` gets no editor row at all. Two things hide a subtree
/// (SPEC §三十七, ADR-0028): a folded block anywhere above it, and a container
/// whose delegate draws its children itself — a table's grid, a columns
/// layout's boxes — so they must not also cost a row each. The page's blocks
/// are pre-order, so an ancestor walk is enough (no set to maintain).
fn hidden_by_ancestor(blocks: &[Block], b: &Block) -> bool {
    if matches!(b.kind, BlockKind::TableCell | BlockKind::Column) {
        return true;
    }
    let mut parent = b.parent;
    let mut guard = 0;
    while let Some(pid) = parent {
        let Some(p) = blocks.iter().find(|x| x.id == pid) else {
            break;
        };
        if p.folded || matches!(p.kind, BlockKind::Table | BlockKind::Columns | BlockKind::Column)
        {
            return true;
        }
        parent = p.parent;
        guard += 1;
        if guard >= 64 {
            break;
        }
    }
    false
}

/// The row that draws `id`: itself for a block with its own row, and otherwise
/// the nearest ancestor that has one — a table cell rides on its table's row, a
/// layout's block on the layout's. A find hit is addressed by the block it was
/// found in, so this is how the bar's repaint finds the row to touch.
fn row_id_of(blocks: &[Block], id: BlockId) -> i32 {
    let mut cur = id;
    for _ in 0..64 {
        let Some(b) = blocks.iter().find(|x| x.id == cur) else {
            break;
        };
        if !hidden_by_ancestor(blocks, b) {
            return b.id.0 as i32;
        }
        let Some(p) = b.parent else {
            break;
        };
        cur = p;
    }
    id.0 as i32
}

/// Positions (into the page's block list) of the blocks that get an editor
/// row: a folded block stays, its whole subtree does not, and so does a
/// table's grid. SPEC §三十七 is explicit that hiding a collapsed section
/// costs real rows rather than `visible: false` delegates, so every row-index
/// consumer shares this one list — see `drop_index_for_row`.
pub fn visible_block_indices(blocks: &[Block]) -> Vec<usize> {
    blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| !hidden_by_ancestor(blocks, b))
        .map(|(i, _)| i)
        .collect()
}

/// A table as the editor sees it: its cells in row-major order and the column
/// count. Cells are child blocks, and a page's block list is display order, so
/// filtering it yields the grid — the same reading `command::grid` uses.
struct LiveGrid {
    table: i32,
    cells: Vec<BlockId>,
    cols: usize,
}

/// A columns layout as the editor sees it: every block inside its boxes, in
/// reading order — the same list the row's `column-items` carries, built by
/// the same walk, so focus and the model cannot disagree about what is there.
struct LiveLayout {
    layout: i32,
    items: Vec<BlockId>,
}

impl LiveLayout {
    fn of(doc: &Document, id: BlockId) -> Option<Self> {
        let b = doc.block(id)?;
        if b.kind != BlockKind::Columns || b.columns == 0 {
            // a layout with no boxes has no shape to edit, and the commands
            // refuse it the same way a degenerate grid is refused
            return None;
        }
        let blocks = doc.page_blocks(b.page);
        let items = layout_slots(blocks, b)
            .into_iter()
            .map(|s| blocks[s.index].id)
            .collect();
        Some(Self { layout: id.as_u64() as i32, items })
    }
}

impl LiveGrid {
    fn of(doc: &Document, id: BlockId) -> Option<Self> {
        let b = doc.block(id)?;
        let cols = b.columns as usize;
        if b.kind != BlockKind::Table || cols == 0 {
            return None;
        }
        let cells = grid_blocks(doc.page_blocks(b.page), b)
            .into_iter()
            .map(|c| c.id)
            .collect();
        Some(Self { table: id.as_u64() as i32, cells, cols })
    }

    fn rows(&self) -> usize {
        self.cells.len() / self.cols
    }
}

/// A table's cells in row-major order — child blocks, and a page's block list
/// is display order, so filtering it yields the grid. The same reading
/// `command::grid` makes.
fn grid_blocks<'a>(blocks: &'a [Block], table: &Block) -> Vec<&'a Block> {
    let mut cells: Vec<&Block> = blocks
        .iter()
        .filter(|b| b.parent == Some(table.id) && b.kind == BlockKind::TableCell)
        .collect();
    // one table's cells live on one page, where the list is already display
    // order; the sort says so out loud
    cells.sort_by_key(|b| b.order);
    // whole rows only: the delegate chunks this list by `columns` and indexes
    // into it, so a ragged tail would be read out of range. Such a grid is not
    // editable either — `command::grid` refuses it — so the stray cells stay
    // in the document, unseen, rather than rendering as a broken row.
    let cols = table.columns.max(1) as usize;
    cells.truncate(cells.len() - cells.len() % cols);
    cells
}

/// The page's headings, as one `Toc` row's data. `shown` is the row list the
/// projection itself just built, so a heading hidden by a fold or by a
/// container's delegate stays out of the contents too — a link you cannot
/// scroll to is worse than no link. Reading order is the page's own.
/// The backlink panel's count line: "1 reference" / "5 references". A plural
/// is a decision Rust makes — Slint has no format, and the alternative is a
/// second property for the singular.
fn backlink_count_label(total: usize) -> String {
    match total {
        0 => String::new(),
        1 => "1 reference".to_string(),
        n => format!("{n} references"),
    }
}

/// The fold control's label, and `""` when there is nothing to fold — a
/// control that does nothing is worse than no control.
///
/// Beyond `BACKLINK_EXPANDED` the unfolded panel is *still* a window, not the
/// whole list: a page quoted 200 times is a number, not a list the foot of a
/// page can hold, and this says so by offering to fold again rather than by
/// pretending it showed everything.
fn backlink_fold_label(total: usize, drawn: usize, expanded: bool) -> String {
    if expanded {
        if total > BACKLINK_WINDOW {
            "Show less".to_string()
        } else {
            String::new()
        }
    } else {
        match total.saturating_sub(drawn) {
            0 => String::new(),
            n => format!("and {n} more"),
        }
    }
}

/// Follow a chain of mirrors to the block that actually holds the words
/// (ADR-0052). A chain is legal to write — A may mirror B which mirrors C — and
/// the row must show C's sentence either way.
///
/// **Bounded, and it stops on what it has.** `SYNC_RESOLVE_MAX` hops and no
/// recursion: a file can hold *any* shape at all once somebody edits it by
/// hand, and the difference between "an old backup draws the wrong sentence"
/// and "the app stopped drawing" is entirely this loop having a ceiling. When
/// the bound runs out, whatever is in hand gets drawn rather than nothing.
fn sync_target(doc: &Document, mut id: BlockId) -> Option<&Block> {
    for hop in 0..SYNC_RESOLVE_MAX {
        let b = doc.block(id)?;
        match b.sync_ref {
            None => return Some(b),
            Some(next) => {
                if hop + 1 >= SYNC_RESOLVE_MAX {
                    return Some(b);
                }
                id = next;
            }
        }
    }
    None
}

/// What a mirror's source says, handed to the Markdown exporter rather than a
/// `Document` (ADR-0052 §7). The export layer is given this answer because it
/// renders text it already has and fetches nothing — the same division the
/// attachment sizes, the math glyphs and the database tables follow.
///
/// `None` is "there is nothing behind this row", and the exporter writes an
/// empty line for it: a lost sentence, honestly marked, rather than a marker
/// that no importer could ever resolve.
pub fn sync_target_for_export(doc: &Document, id: BlockId) -> Option<(String, Vec<Mark>)> {
    let b = doc.block(id)?;
    let src = b.sync_ref.and_then(|sid| sync_target(doc, sid))?;
    Some((src.text.clone(), src.marks.clone()))
}

/// Would pointing `id` at `source` make a block end up mirroring itself,
/// directly or through the chain? This is the check ADR-0052 §4 puts at the
/// moment of writing: the same question asked while drawing is discovered once
/// per frame forever, and can never say no.
///
/// Also bounded, for the same reason as `sync_target`: `sync_would_cycle` runs
/// before the pointer is written, when the file's shape is whatever it happens
/// to be, not what this build would have written.
fn sync_would_cycle(doc: &Document, id: BlockId, source: BlockId) -> bool {
    if id == source {
        return true;
    }
    let mut cur = source;
    for _ in 0..SYNC_CHAIN_MAX {
        let Some(b) = doc.block(cur) else {
            return false;
        };
        let Some(next) = b.sync_ref else {
            return false;
        };
        if next == id {
            return true;
        }
        cur = next;
    }
    false
}

/// What the picker calls a block: its first line, cut short. A block can hold
/// a page's worth of words and a menu row holds about fifty characters, so the
/// row is an *excerpt* — enough to know which block it is, never the whole
/// thing.
fn first_line_of(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text).trim();
    match line.char_indices().nth(48) {
        Some((at, _)) => format!("{}…", line[..at].trim_end()),
        None => line.to_string(),
    }
}

fn toc_entries(blocks: &[Block], shown: &[usize]) -> Vec<TocEntry> {
    shown
        .iter()
        .filter_map(|&i| {
            let b = &blocks[i];
            let level = b.kind.heading_level()?;
            Some(TocEntry {
                block: b.id.0 as i32,
                // an empty heading still has a row to land on, so it still gets
                // an entry; a blank line is nothing to click
                label: if b.text.is_empty() {
                    "Untitled".into()
                } else {
                    b.text.clone().into()
                },
                level: level as i32,
            })
        })
        .collect()
}

// ─── SPEC §三十九 Database (D3: the table view) ─────────────────────────────
//
// The data flow, once, because three layers meet here and the red line is about
// which of them does what:
//
//     page scroll (Slint)  ->  `db_watch(block, top_in_view)`
//                                    |
//                     `core::database::window` (the window, from the viewport)
//                                    |
//                     `repo.window_rows` (`LIMIT`/`OFFSET`, 31 rows of 10 000)
//                                    |
//          `core::database_view` (columns, painted cells, row->y arithmetic)
//                                    |
//                  `ModelRc<DbRow>` ->  the block's delegate, which only draws
//
// Four things in here are deliberate and each has a reason:
//
// 1. **The window is computed by `core::database::window` and nowhere else**, so
//    "先算可见窗口再取行" is one function rather than a rule three callers
//    remember (ADR-0067).
// 2. **A scroll only re-reads when the window moved.** Overscan is what buys
//    that: the window is a screenful plus eight rows above and below, so 256 px
//    of scrolling costs one read instead of one per frame. The arithmetic runs
//    every time; the *query* runs when `window.start` changes.
// 3. **The rows live in a model of their own** (`DbWindow::rows`), cloned into
//    the block's row. A re-read therefore replaces one model and one outer row
//    (`set_row_data`) instead of rebuilding the page's whole row list, which is
//    what makes scrolling a 10 000-row database cost one query and one row.
// 4. **The anchor is reported, not measured.** Rust cannot see Slint's layout,
//    so the block tells it where its top is relative to the viewport
//    (`database-viewport`), and Rust turns that into "how far into the database
//    is the reader": `offset = scroll - top`, clamped at zero. One reported
//    number per scroll frame per database block, and no I/O while it is static.

/// One database block's realized window: the rows that exist as objects, which
/// window of the database they are, and the model the delegate reads.
///
/// The columns are part of the window rather than read fresh each projection,
/// because a row's cells and its header have to come from *one* definition: the
/// `RowView`s in `rows` were painted against exactly this column list, and a
/// header built from a newer definition would label the cells with the wrong
/// names — the kind of defect that looks like a sorting bug.
#[derive(Clone)]
struct DbWindow {
    /// The view this window was read for. A view change invalidates the window
    /// (different columns, and in D4 different rows), which is why it is here and
    /// not in a separate map.
    view: ViewId,
    /// The view's definition document **as text**, part of the cache key: the
    /// rules live in that document (ADR-0064), so a filter or sort edit — whose
    /// row count and row set may change while the view id does not — must
    /// invalidate exactly the way a view switch does. Compared as text rather
    /// than re-parsed: the document is the only copy of the rules, and two
    /// documents that differ as text can differ as rules.
    definition: String,
    /// The layout this window was read for. Part of the key for the same reason
    /// the view is: a layout set (D5's shapes) changes what the surface *is* —
    /// a board's slots are not a table's rows — while the definition text may
    /// not change at all.
    layout: crate::core::database::ViewLayout,
    /// The content stamp this window was read at (see
    /// [`AppState::db_content_stamp`]) — the half of the key that says whether
    /// what the rows *paint* can still be trusted, as opposed to whether the
    /// same rows are still the ones on screen.
    content: u64,
    /// A per-layout session nonce, part of the cache key: the calendar's month
    /// (year × 12 + month) and the gallery's cards-per-row both change what the
    /// realized model holds without touching the view, the document or the
    /// total — exactly the inputs the three fields above cover. Zero for the
    /// layouts that have no such dial.
    stamp: u64,
    /// The view's search needle (ADR-0087), part of the key for the same reason
    /// the stamp is: it is session state that changes the statement without
    /// changing the view, the document or the layout. Without it, typing a
    /// needle that happens to match every row would be reported to the caller
    /// as "nothing changed" — and the box would open over rows the last needle
    /// narrowed.
    search: Option<String>,
    /// The columns the read was made with, in view order.
    columns: Vec<TableColumn>,
    /// The count the window was computed from — the rows the view's rules
    /// admit (`COUNT(*)` / the group counts' sum), not the table's size.
    total: usize,
    /// The slice of `total` the model holds. When the view is grouped, the
    /// "rows" are *entries*: a group header is one entry, its rows follow it.
    window: RowWindow,
    /// The realized rows, as the delegate reads them. Cloned into the block's
    /// row, so a re-read updates the UI without touching the page's row list.
    rows: Rc<VecModel<DbRow>>,
    /// D5: the board's columns, each holding its own window of cards — the
    /// model the board delegate reads instead of `rows` (which stays the
    /// row-shaped layouts'). Empty for every other layout.
    board: Rc<VecModel<crate::DbBoardColumn>>,
    /// D5: the calendar's fixed grid — 42 day cells, the month's records
    /// already windowed inside each (`CALENDAR_PEEK` per day, the rest folded
    /// into the cell's count). Empty for every other layout.
    cal: Rc<VecModel<crate::DbCalendarDay>>,
    /// D5: the surface height **below the header**, in px, per layout — the
    /// number the block's own height is made of. Computed here rather than in
    /// the delegate because it is a *projection* fact (counts, per-row, the
    /// month shape), and a delegate that recomputed it would need every input
    /// the projection already had.
    body: f32,
    /// D5 (timeline): the axis the realized bars are drawn against — the first
    /// day (as a day number) and its length in days. One aggregate query per
    /// refresh (`column_bounds`); the delegate maps a bar's day numbers onto
    /// pixels with these two numbers and nothing else.
    tl_start: i64,
    tl_days: i64,
    /// D7 (chart): the plot's points — the group list (labels, counts, the
    /// bar/line fractions and the pie slice paths), all decided here so the
    /// delegate only places shapes. A chart realizes **zero rows** (the plot
    /// is the `GROUP BY`'s aggregates, never the records — the layout's own
    /// answer to the red line), so this model *is* the chart's whole payload.
    chart: Rc<VecModel<crate::DbChartPoint>>,
    /// The line chart's polyline, as one path in a 100×100 viewbox
    /// (`chart_line_path`); the `viewbox` scales it to the plot's real size,
    /// so no pixel width ever crosses the boundary. Empty for bar and pie,
    /// which draw from `chart` alone.
    chart_path: Rc<String>,
    /// The active shape (`ChartKind`'s index) — the three buttons' state. It
    /// is part of the window's read (the document was parsed for the refresh
    /// anyway) and rides the row so the delegate's buttons can tint the
    /// active one without re-parsing the document.
    chart_kind: i32,
}

/// How a database block's columns popup is built: every property of the
/// database, whether the active view shows it, and whether it may be toggled.
/// The title column is listed and *not* toggleable — a table with no title
/// column is a list of anonymous rows (ADR-0063), so the switch is absent
/// rather than present-and-lying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbColumnToggle {
    pub property: i32,
    pub name: String,
    pub kind: String,
    pub visible: bool,
    pub locked: bool,
}

/// One row of any of D10's chooser lists: the column type menu, a relation's
/// target database, a relation's back-pointer, a rollup's three parts, and the
/// records a relation cell holds. `id` is whatever that list's callback names —
/// a `PropertyKind::ALL` index, a property id, a database id, a record id, an
/// `Aggregate::ALL` index — which is why one struct serves all of them: every
/// list asks the same question ("which of these?"), and the three things a
/// delegate may draw are a word, a tick and a reason.
///
/// A `disabled` row is never blank: `note` carries the sentence that says why,
/// because a menu that greys something out with no reason is a guess the user
/// has to make. On a live row `note` is the row's own caption (what kind a
/// column is) or empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbPickRow {
    pub id: i32,
    pub name: String,
    pub chosen: bool,
    pub disabled: bool,
    pub note: String,
}

/// One filter-panel row (D4), as the panel draws it: the clause's ids plus the
/// display strings its column's kind needs — the comparison's word ("is",
/// "contains", "after"), and the value as stored or as its option's name. A
/// row with `has_value == false` is a rule nobody has filled in yet, which
/// filters nothing by design (`FilterValue::Missing` compiles to no
/// constraint).
#[derive(Debug, Clone, PartialEq)]
pub struct DbFilterPanelRow {
    pub property: i32,
    pub name: String,
    /// The `PropertyKind` int (`property_kind_int`'s legend in Types.slint) —
    /// what decides the value editor the row draws.
    pub kind: i32,
    /// The `FilterOp` index (`FILTER_OPS`'s order, the same legend the op
    /// picker's list is pushed from).
    pub op: i32,
    pub op_name: String,
    pub value: String,
    pub has_value: bool,
    /// A `not` around this clause ("is not", "does not contain").
    pub invert: bool,
}

/// The `PropertyKind` int a Slint delegate compares against. `PropertyKind::ALL`
/// is the list and the index is the int, so the numbering has exactly one source
/// and adding a kind appends a number instead of renumbering one — the same rule
/// `BlockKind`'s ints follow. The legend is written out in `ui/Types.slint`.
fn property_kind_int(kind: crate::core::database::PropertyKind) -> i32 {
    crate::core::database::PropertyKind::ALL
        .iter()
        .position(|k| *k == kind)
        .map(|at| at as i32)
        .unwrap_or(0)
}

// ─── D6: the projection's formula half (SPEC §三十九 「需计算」) ─────────────
//
// A formula column stores nothing (ADR-0062: 「不存值，投影时现算」), so its
// cells are computed on the way to the screen — here, in the projection layer,
// over exactly the rows a view realized. The engine is
// `core::database_formula` (pure, no store); this is the adapter that answers
// the engine's one question ("what is this column's value on the row being
// evaluated").

/// What one row's formula evaluation reads its cells through — the adapter
/// between the engine and the store.
///
/// **There is no record parameter in the engine's cell callback, and this
/// struct is the reason the contract holds by construction**: the source is
/// built *for one record*, so a formula's `[Column]` reference can only ever
/// reach that row's cells. Same-row references are the boundary (ADR-0083);
/// cross-row values are rollup / relation's, which this build does not have
/// (ADR-0084 — they wait for §四十's reference infrastructure, Track 2).
/// One visible rollup column, resolved once for a projection (ADR-0089):
/// the relation whose targets supply the rows, the column of the related
/// database that gets folded, that column's kind (which is what turns a
/// stored `CellValue` into the evaluator's `Val`), and the fold.
///
/// Resolved once per column per refresh and not once per cell: the config
/// parse and the two catalog lookups are the *column's* cost, and a window
/// of thirty rows should pay it once rather than thirty times.
struct RollupPlan {
    relation: PropertyId,
    column: Option<PropertyId>,
    target_kind: PropertyKind,
    aggregate: Aggregate,
}

struct FormulaSource<'a> {
    repo: &'a SqliteRepository,
    /// The one row this source knows how to read. Every path below reads
    /// *this* record and no other.
    record: u64,
    db: DatabaseId,
    catalog: &'a DatabaseCatalog,
    /// The row's cells, read on first use and remembered for the rest of the
    /// evaluation. Bounded by the formula's dependency count: each column is
    /// read at most once per row however many expressions name it.
    cells: RefCell<HashMap<u64, Val>>,
    /// Formula columns' parsed expressions, parsed on first use out of the
    /// catalog's `config` (ADR-0061/0082). `None` is a formula column whose
    /// expression does not parse or is absent — a reference to it reads as
    /// [`Val::Empty`], the same fold an unpaintable config gets everywhere else.
    programs: RefCell<HashMap<u64, Option<std::rc::Rc<Program>>>>,
    /// The Markdown export's preload (see `db_markdown_table`): `property →
    /// (record → value)`, one indexed sweep per dependency column instead of
    /// one point read per row. `None` on the window path, which reads through
    /// `repo.cell` — 31 rows × a few dependencies is smaller than the sweep.
    preload: Option<&'a HashMap<u64, HashMap<u64, CellValue>>>,
}

impl<'a> FormulaSource<'a> {
    fn new(
        repo: &'a SqliteRepository,
        record: u64,
        db: DatabaseId,
        catalog: &'a DatabaseCatalog,
        preload: Option<&'a HashMap<u64, HashMap<u64, CellValue>>>,
    ) -> Self {
        Self {
            repo,
            record,
            db,
            catalog,
            cells: RefCell::new(HashMap::new()),
            programs: RefCell::new(HashMap::new()),
            preload,
        }
    }

    fn kind_of(&self, id: PropertyId) -> Option<PropertyKind> {
        self.catalog
            .properties
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.kind)
    }

    /// A formula column's parsed expression, parsed once per evaluation and
    /// remembered. `None` folds to "no value" at the reference site.
    fn program_of(&self, id: PropertyId) -> Option<std::rc::Rc<Program>> {
        if let Some(cached) = self.programs.borrow().get(&id.as_u64()) {
            return cached.clone();
        }
        let parsed = self
            .catalog
            .properties
            .iter()
            .find(|p| p.id == id)
            .and_then(|property| database_formula::config_formula(&property.config))
            .and_then(|source| {
                let resolve = |name: &str| {
                    self.catalog
                        .properties_of(self.db)
                        .find(|p| p.name == name)
                        .map(|p| p.id)
                };
                Program::parse(&source, resolve).ok()
            })
            .map(std::rc::Rc::new);
        self.programs.borrow_mut().insert(id.as_u64(), parsed.clone());
        parsed
    }

    /// The engine's cell callback: the value of column `id` **on this source's
    /// row**, evaluating at `depth` — a reference to another *formula* column
    /// recurses through that column's program at the depth it was handed, and
    /// past [`FORMULA_MAX_DEPTH`] the answer is an error, not a hang (the
    /// save-time cycle check keeps user-written chains acyclic, ADR-0082; this
    /// is what keeps a document that arrived another way finite, which is the
    /// SPEC sentence 「表达式必须有限求值」 holding at render time too).
    fn value(&self, id: PropertyId, depth: u32) -> Result<Val, FormulaError> {
        if depth > FORMULA_MAX_DEPTH {
            return Err(FormulaError::Eval(format!(
                "formulas are nested more than {FORMULA_MAX_DEPTH} deep"
            )));
        }
        if let Some(cached) = self.cells.borrow().get(&id.as_u64()) {
            return Ok(cached.clone());
        }
        let Some(kind) = self.kind_of(id) else {
            // Unreachable through the UI (the parse resolves names against the
            // same catalog), reachable if the schema moved mid-evaluation; the
            // honest answer is an error the cell paints, not a fake empty.
            return Err(FormulaError::Eval(
                "the formula names a column this database does not have".into(),
            ));
        };
        let value = if kind == PropertyKind::Formula {
            match self.program_of(id) {
                Some(program) => program.eval_at(depth, &mut |next, next_depth| {
                    self.value(next, next_depth)
                })?,
                None => Val::Empty,
            }
        } else {
            match self.preload {
                Some(columns) => columns
                    .get(&id.as_u64())
                    .and_then(|column| column.get(&self.record))
                    .map(|cell| database_formula::val_of(kind, cell))
                    .unwrap_or(Val::Empty),
                None => {
                    let cell = self
                        .repo
                        .cell(RecordId(self.record), id)
                        .unwrap_or(CellValue::Empty);
                    database_formula::val_of(kind, &cell)
                }
            }
        };
        self.cells.borrow_mut().insert(id.as_u64(), value.clone());
        Ok(value)
    }
}

/// Painted rows as the delegate reads them. `header` is a group header's label
/// (D4) — a data row carries an empty one, which is the only thing the
/// delegate's conditional asks. One conversion, used by both realize paths
/// (the plain window's and the grouped one), so a row cannot come out shaped
/// differently depending on which path built it.
fn db_rows_of(rows: Vec<crate::core::database_view::TableRowView>) -> Vec<DbRow> {
    rows.into_iter()
        .map(|row| DbRow {
            record: row.record as i32,
            page: row.page.map(|p| p.as_u64() as i32).unwrap_or(-1),
            title: row.title.clone().into(),
            header: "".into(),
            // D5 (timeline): no bar until the timeline branch stamps the row's
            // day numbers in — `-1` is "no date", which every other layout's
            // delegate never reads.
            tl_from: -1,
            tl_to: -1,
            // D5 (gallery): the avatar's first character, cut here because a
            // delegate never does string surgery. A char boundary at index 1
            // is only valid when the first byte is ASCII, so the cut is by
            // chars — a non-ASCII first character still gives one character.
            letter: row.title.chars().next().map(String::from).unwrap_or_default().into(),
            cells: ModelRc::from(Rc::new(VecModel::from(row
                .cells
                .into_iter()
                .map(|cell| DbCell {
                    property: cell.property.as_u64() as i32,
                    kind: property_kind_int(cell.kind),
                    text: cell.painted.into(),
                    checked: cell.checked,
                    editable: cell.editable,
                })
                .collect::<Vec<_>>()))),
        })
        .collect()
}

impl DbRow {
    /// A placeholder entry — the slot a grouped view reserves in its entry
    /// list for a row that the per-group slices fill in. One constructor, so
    /// "an entry that is not yet a row" has one shape (and D5's fields have
    /// one place to be defaulted in).
    fn header() -> Self {
        DbRow {
            record: -1,
            page: -1,
            title: "".into(),
            header: "".into(),
            tl_from: -1,
            tl_to: -1,
            letter: "".into(),
            cells: ModelRc::default(),
        }
    }

    /// A group header entry (D4): its label came from Rust with the count in
    /// it, it carries no cells, and it is one slot of the scroll surface the
    /// window arithmetic already accounted for.
    fn header_with(label: &str) -> Self {
        DbRow {
            header: label.into(),
            ..DbRow::header()
        }
    }
}

// ─── D5: the view family's helpers ───────────────────────────────────────────
//
// Small free functions the layout branches above share. None of them holds a
// row: the first three turn SQL counts into the per-layout facts the window
// and the body are made of, and the last three build the date-range clauses a
// calendar and a timeline speak to SQL in — fixed-width ISO bounds compiled by
// the same clause compiler the filter panel writes (there is no separate "date
// query" in this codebase, on purpose).

/// The gallery's default cards-per-row, in px of editor width, for the first
/// refresh before the delegate has reported the grid's real width. The number
/// is the default editor content width, run through the same
/// [`TableView::gallery_per_row`] formula the delegate uses.
const GALLERY_DEFAULT_WIDTH: f32 = 760.0;

/// The ungrouped count a view's window is computed from: `COUNT(*)` over the
/// request's predicate when it has one, over the database alone when it has
/// none. The table branch inlined this before D5; the gallery and the form need
/// the same question, so the question got a name.
///
/// **Both** predicates count, not just the filter's: the view's search needle
/// (ADR-0087) narrows the same rows, and a header that answered `COUNT(*)` over
/// the whole database under a needle would print "5 rows" above a window of 3 —
/// and then `unchanged` would call that the same window as the unsearched one,
/// so the search would not happen at all.
fn layout_total(
    repo: &SqliteRepository,
    request: &RowRequest<'_>,
    db: DatabaseId,
) -> Result<usize, crate::core::persistence::StorageError> {
    if request.filter.is_some() || request.search.is_some() {
        repo.filtered_count(request)
    } else {
        repo.record_count(db)
    }
}

/// One date-range bound as a filter clause — the only way a calendar or a
/// timeline speaks to SQL. The bound is a **stored-shape ISO text**, so the
/// comparison is byte order, which is time order (ADR-0062's rule doing the
/// date arithmetic); the clause goes through the same compiler the panel's
/// rules use, so there is exactly one place that turns "after" into SQL.
fn date_bound(property: PropertyId, kind: PropertyKind, op: FilterOp, bound: &str) -> FilterNode {
    FilterNode::Clause(FilterClause {
        property,
        kind,
        op,
        value: FilterValue::Text(bound.to_string()),
    })
}

/// A month's bounds ANDed with whatever the view already filters by: `>= the
/// 1st` and `< the 1st of the next`. The view's tree is *inside* the `All`, so
/// a filter the user wrote keeps meaning what it meant — the month only narrows
/// it further.
fn month_clauses(
    base: &Option<FilterNode>,
    property: PropertyId,
    kind: PropertyKind,
    low: &str,
    high: &str,
) -> FilterNode {
    let mut children: Vec<FilterNode> = Vec::new();
    if let Some(tree) = base {
        children.push(tree.clone());
    }
    children.push(date_bound(property, kind, FilterOp::Gte, low));
    children.push(date_bound(property, kind, FilterOp::Lt, high));
    FilterNode::All(children)
}

/// The day of the month a group key names, when the key *is* a date text — the
/// fold that turns a `GROUP BY` over a date column into per-day counts. Keys
/// that are not dates (the empty group) have no day and no cell.
fn group_day(key: &GroupKey) -> Option<u32> {
    match key {
        GroupKey::Option(text) => day_of(text),
        _ => None,
    }
}

/// Append a property to a request's column list if it is not already there,
/// returning the position its painted cell will occupy. The timeline uses this
/// to carry its date (and optional end) columns even when the view hides them —
/// a bar cannot be drawn from a value the row was not handed. When the catalog
/// has no such row (a document naming a deleted column), the list is left
/// alone and the position is `usize::MAX` — an index `Vec::get` answers with
/// `None`, which the caller reads as "no such value" rather than guessing.
fn push_column(
    columns: &mut Vec<Property>,
    catalog: &DatabaseCatalog,
    property: PropertyId,
) -> usize {
    if let Some(at) = columns.iter().position(|p| p.id == property) {
        return at;
    }
    if let Some(row) = catalog.properties.iter().find(|p| p.id == property) {
        columns.push(row.clone());
        return columns.len() - 1;
    }
    usize::MAX
}

impl AppState {
    /// The entity a `Database` block draws, from the in-memory document — which
    /// is where the block's pointer lives (`Change::BlockDbRefSet` is applied to
    /// the document as well as to SQL).
    pub fn db_ref_of(&self, block: i32) -> Option<DatabaseId> {
        let doc = self.doc.borrow();
        doc.block(BlockId(block as u64)).and_then(|b| b.db_ref)
    }

    /// Whether a database entity exists for the block. `false` for a block whose
    /// entity was deleted and which came back through an undo — ADR-0060's
    /// "(deleted database)", the one thing a dangling ref renders as.
    pub fn db_exists(&self, block: i32) -> bool {
        match self.db_ref_of(block) {
            Some(id) => self.databases.borrow().database(id).is_some(),
            None => false,
        }
    }

    /// The view a block is showing: the one the session picked, else the
    /// database's first.
    ///
    /// The choice is **session state, not a column** (ADR-0073): "which view am
    /// I looking at" is a fact about a window and not about a document, and the
    /// cost of that decision is written down there — a restart opens the first
    /// view rather than the last one looked at.
    pub fn db_active_view(&self, block: i32) -> Option<ViewId> {
        let db = self.db_ref_of(block)?;
        if let Some(view) = self.db_active_view.borrow().get(&block) {
            if self.databases.borrow().views_of(db).any(|v| v.id == *view) {
                return Some(*view);
            }
        }
        self.databases.borrow().views_of(db).map(|v| v.id).next()
    }

    /// One view's rules, parsed against the schema — the read side of D4: what
    /// the header state (`db_fill_row`) and the two panels read. The write side
    /// goes through `db_edit_definition` and, for the filter, the panel's flat
    /// view of the tree. `ViewRules::note` is the visible degradation, so a
    /// caller never has to guess whether the document's rules were applied.
    fn db_rules(&self, db: DatabaseId, view: ViewId) -> ViewRules {
        let catalog = self.databases.borrow();
        catalog
            .views
            .iter()
            .find(|v| v.id == view)
            .map(|row| ViewDefinition::parse(&row.definition).rules(db, &catalog))
            .unwrap_or_default()
    }

    /// The store, or `None` in a headless session with no database. Every
    /// database read goes through this: a session without a file has no records
    /// to draw, which is a fact about the session and not an error.
    fn db_repo(&self) -> Option<&Arc<SqliteRepository>> {
        self.repo.as_ref()
    }

    /// Teach the in-memory catalog what a change list did (ADR-0075).
    ///
    /// The catalog is the schema the read path consults on every projection,
    /// and until this function existed nothing kept it in step with a write: a
    /// freshly made database was in SQL and not in memory, so its own block drew
    /// ADR-0060's "(deleted database)" until the next restart. `record` is the
    /// funnel, so apply, undo and redo all arrive here.
    ///
    /// **A change names what happened, not which direction it ran** (that is
    /// `core::document`'s contract for `apply`/`revert`): `DatabaseCreated`
    /// always means the row exists now, whether the user made it or undid its
    /// deletion. So this is a fold in the same direction as storage's, and the
    /// two cannot disagree about what a list means.
    ///
    /// Records and values are deliberately absent: they are not in the catalog
    /// at all (ADR-0067), and a row's life is a window's business.
    fn db_absorb(&self, changes: &[Change]) {
        let mut catalog = self.databases.borrow_mut();
        for change in changes {
            match change {
                Change::DatabaseCreated(db) => {
                    // An undo replays a creation that this session may already
                    // have learned (redo of a delete), so the insert is
                    // idempotent rather than a push that could double a row.
                    if catalog.database(db.id).is_none() {
                        catalog.databases.push(db.clone());
                    }
                }
                Change::DatabaseRenamed { id, name } => {
                    if let Some(row) = catalog.databases.iter_mut().find(|d| d.id == *id) {
                        row.name = name.clone();
                    }
                }
                // Deleting the entity takes its columns and views with it
                // (`ON DELETE CASCADE`), so the fold has to do the same or the
                // catalog would keep drawing a schema whose rows are gone.
                Change::DatabaseDeleted { id } => {
                    catalog.databases.retain(|d| d.id != *id);
                    catalog.properties.retain(|p| p.db != *id);
                    catalog.views.retain(|v| v.db != *id);
                }
                Change::PropertyAdded(property) => {
                    if !catalog.properties.iter().any(|p| p.id == property.id) {
                        catalog.properties.push(property.clone());
                    }
                    // `ord` is the schema's order (ADR-0061) and the store
                    // returns the columns by it, so the catalog keeps the same
                    // order in memory that a restart would load.
                    catalog.properties.sort_by_key(|p| (p.db, p.ord));
                }
                Change::PropertyRenamed { id, name } => {
                    if let Some(row) = catalog.properties.iter_mut().find(|p| p.id == *id) {
                        row.name = name.clone();
                    }
                }
                // ADR-0062's one write path that changes what a *cell means*:
                // the values stay where they are and the column's type moves, so
                // the next projection paints the same bytes through the new kind.
                Change::PropertyKindSet { id, kind } => {
                    if let Some(row) = catalog.properties.iter_mut().find(|p| p.id == *id) {
                        row.kind = *kind;
                    }
                }
                Change::PropertyOrdSet { id, ord } => {
                    if let Some(row) = catalog.properties.iter_mut().find(|p| p.id == *id) {
                        row.ord = *ord;
                    }
                    catalog.properties.sort_by_key(|p| (p.db, p.ord));
                }
                Change::PropertyDeleted { id } => {
                    catalog.properties.retain(|p| p.id != *id);
                }
                // D6 (ADR-0082): the column's `config` document, replaced
                // whole — the formula expression's write. Learned here, in the
                // one funnel apply/undo/redo share, so a Ctrl+Z of a formula
                // edit puts the previous expression back into the catalog the
                // projection reads, exactly the way a width drag's undo does.
                Change::PropertyConfigSet { id, config } => {
                    if let Some(row) = catalog.properties.iter_mut().find(|p| p.id == *id) {
                        row.config = config.clone();
                    }
                }
                // D7 (ADR-0086): the database's record template, replaced
                // whole — learned here for the same reason every other
                // document is: the *undo* of a template edit has no call site
                // of its own, and the prefill below reads the catalog's copy.
                Change::DatabaseTemplateSet { id, template } => {
                    if let Some(row) = catalog.databases.iter_mut().find(|d| d.id == *id) {
                        row.template = template.clone();
                    }
                }
                Change::ViewAdded(view) => {
                    if !catalog.views.iter().any(|v| v.id == view.id) {
                        catalog.views.push(view.clone());
                    }
                    catalog.views.sort_by_key(|v| (v.db, v.ord));
                }
                Change::ViewRenamed { id, name } => {
                    if let Some(row) = catalog.views.iter_mut().find(|v| v.id == *id) {
                        row.name = name.clone();
                    }
                }
                Change::ViewLayoutSet { id, layout } => {
                    if let Some(row) = catalog.views.iter_mut().find(|v| v.id == *id) {
                        row.layout = *layout;
                    }
                }
                // The one the width drag and the hide/show toggle write: the
                // view's rules document, replaced whole (ADR-0064). It is
                // learned here rather than at the drag's own call site because
                // its *undo* has to be learned too, and undo has no call site of
                // its own.
                Change::ViewDefinitionSet { id, definition } => {
                    if let Some(row) = catalog.views.iter_mut().find(|v| v.id == *id) {
                        row.definition = definition.clone();
                    }
                }
                Change::ViewOrdSet { id, ord } => {
                    if let Some(row) = catalog.views.iter_mut().find(|v| v.id == *id) {
                        row.ord = *ord;
                    }
                    catalog.views.sort_by_key(|v| (v.db, v.ord));
                }
                Change::ViewDeleted { id } => {
                    catalog.views.retain(|v| v.id != *id);
                }
                // Everything else in the enum belongs to the block tree, the
                // pages or the settings, none of which this catalog describes.
                _ => {}
            }
        }
    }

    /// Recompute one block's window from its reported geometry, and re-read when
    /// the window moved. Returns `true` when the model changed (the caller then
    /// updates that one row).
    ///
    /// `top_in_view` is the block's top edge relative to the editor viewport's
    /// top, in px, as the delegate measured it: negative when the block starts
    /// above the viewport, positive when it starts below it.
    pub fn db_watch(&self, block: i32, top_in_view: f32) -> bool {
        self.db_anchor.borrow_mut().insert(block, top_in_view);
        self.db_refresh(block)
    }

    /// The refresh itself, without recording the anchor — the path a scroll and
    /// the path that follows a structural edit share.
    ///
    /// **It settles the write queue first, and that is not an optimisation.**
    /// A row lives in SQL and nowhere else (ADR-0067) while every write in this
    /// app is debounced by 300 ms (`PersistenceService`), so a read that did not
    /// flush first would answer a cell commit with the *old* value — the defect
    /// that looks like "the edit did not take". Flushing here rather than in the
    /// four write helpers is deliberate: undo and redo also reach a read
    /// (`undo_open_page` reprojects) and they have no write call site to hang it
    /// on. `force_flush` with an empty queue is a no-op, so a scroll pays for
    /// this only when there is something to write — and the cost of the flush
    /// itself is D2's number: 5.5–7.4 ms for a lone cell write, 10.6–14.9 µs
    /// inside a batch.
    fn db_refresh(&self, block: i32) -> bool {
        let (Some(db), Some(view)) = (self.db_ref_of(block), self.db_active_view(block)) else {
            return false;
        };
        let Some(repo) = self.db_repo().cloned() else {
            return false;
        };
        if let Some(persistence) = &self.persistence {
            // A failed flush is reported by the write path's own error channel
            // (`take_last_error`); the read below then answers with whatever the
            // file holds, which is the honest thing to draw.
            let _ = persistence.force_flush();
        }
        // One catalog read for everything the refresh needs: the columns the
        // view shows, its definition **text** (the cache key's rules half), the
        // rules parsed against the schema — filter, sorts, group, and the
        // visible note if any of it could not be applied (D4's degradation,
        // `ViewRules::note`) — and, since D5, the layout, which decides what
        // the window below even opens on.
        let (definition_text, properties, columns, rules, layout) = {
            let catalog = self.databases.borrow();
            let Some(db_row) = catalog.views.iter().find(|v| v.id == view) else {
                return false;
            };
            let definition_text = db_row.definition.clone();
            let properties = view_columns(&catalog, db, db_row);
            let layout = db_row.layout;
            // One parse for both readers: the rules (filter/sorts/group/note)
            // and the widths the columns lay out at are the same document.
            let definition = ViewDefinition::parse(&definition_text);
            let rules = definition.rules(db, &catalog);
            let columns = table_columns(&properties, &definition);
            (definition_text, properties, columns, rules, layout)
        };

        // `top` is the anchor the delegate reports (`db-viewport`): the block
        // body's top — header included — **relative to the viewport's top
        // edge**. The reader's offset into the row surface is therefore its
        // negation: a body whose top is 500 px above the viewport has exactly
        // 500 px of rows already scrolled past it, and a body below the
        // viewport has nothing scrolled past yet, which is what the clamp
        // says. (Both halves must agree on the convention: the .slint side
        // reports `row-y - editor-scroll-y`, so `scroll` must NOT enter here
        // a second time — that double count was the one real arithmetic bug
        // the D3 wiring caught, and it would have fetched a window that does
        // not cover the viewport at any scroll position other than the top.)
        let top = *self.db_anchor.borrow().get(&block).unwrap_or(&0.0);
        let viewport = self.editor_viewport_h.get();
        let offset = (-top).max(0.0);
        // D5: the window's unit is the layout's own — a table row, a list row,
        // a timeline lane, a board slot, a gallery card row. `layout_metrics`
        // is the one source the delegate's placement reads too, so the two
        // halves cannot disagree about how tall a unit is (D3's row-height = 0
        // bug was exactly this disagreement, in another form).
        let metrics = layout_metrics(layout);
        let geometry = ViewGeometry::new(metrics.row_height, viewport);
        // The per-layout outputs the shared cache stores below: `stamp` is the
        // layout's session dial (the calendar's month, the gallery's per-row),
        // `body` the surface below the header, and the two `tl_` numbers the
        // timeline's axis. `stamp` and the `tl_` pair default and are filled by
        // the layouts that own them; every layout that reaches the tail fills
        // `body`, so it needs no default.
        let mut stamp: u64 = 0;
        // The change batches recorded since this session began: the cache key's
        // content half, read once here and written into the window at the tail.
        let content = self.db_content_stamp.get();
        let body: f32;
        let mut tl_start: i64 = 0;
        let mut tl_days: i64 = 0;
        // D7 (chart): the plot's payload — the points and, for a line, the one
        // polyline. Every other layout leaves them empty.
        let mut chart_model: Rc<VecModel<crate::DbChartPoint>> = Rc::new(VecModel::default());
        let mut chart_path = String::new();
        let mut chart_kind: i32 = 0;

        let title = properties.iter().find(|p| p.kind.is_title()).map(|p| p.id);
        let Some(title) = title else {
            // ADR-0061: a database without a title column cannot draw a row. The
            // invariant is the insert path's, so this is only reachable through a
            // hand-edited file — and the answer is an empty view, not a panic.
            return false;
        };
        // The request carries the view's rules themselves (ADR-0076): sorts and
        // the filter tree are compiled into the statement by the store, and the
        // borrows live exactly this long — the queries below are the request's
        // only readers. Nothing between here and SQL ever sees a row: that is
        // the red line (「filter / sort 在 SQL 侧完成，不在 UI 侧过滤」) as a
        // borrow, not a rule.
        //
        // D7 (ADR-0087): the view's live search rides the same request — the
        // count, the group list and the rows below all answer one statement,
        // and an empty needle is "not searching" (no constraint), the same
        // reading an empty filter group gets.
        let search = self
            .db_search
            .borrow()
            .get(&block)
            .filter(|needle| !needle.trim().is_empty())
            .cloned();
        let request = RowRequest {
            db,
            title,
            columns: &properties,
            sorts: &rules.sorts,
            filter: rules.filter.as_ref(),
            search: search.as_deref(),
        };

        // ── the layout's own read (D5) ──────────────────────────────────────
        // The red line is per layout, and so is the *unit* the window opens on:
        //
        // * **table / list** — rows, and entries when the view is grouped
        //   (D4's `group_window`): `COUNT(*)` / the group counts first, then
        //   exactly the window's rows.
        // * **board** — card *slots*: slot `s` is one horizontal band across
        //   every column, so the board is `max(column counts)` slots tall and
        //   each group fetches its own slice of that band (`board_window`).
        //   The columns themselves are the group list — one `GROUP BY` over an
        //   option-bounded column, a handful of rows, never one per card.
        // * **gallery** — card *rows*: one slice of `per_row × rows` cards.
        // * **calendar** — the month grid is fixed (6×7), so the window is
        //   *inside a day*: one `GROUP BY` over the date column for the month's
        //   counts (≤ 31 rows) and at most [`CALENDAR_PEEK`] records per day,
        //   the rest folded into the cell's count. A month filter — `>= the
        //   1st`, `< the 1st of the next` — is compiled by the same clause
        //   compiler the panel's rules use, so bytes-are-time (ADR-0062) is
        //   doing the date arithmetic in SQL.
        // * **timeline** — lanes like rows, plus one aggregate query for the
        //   axis (`column_bounds`: `min`/`max` over the same `WHERE`); a lane
        //   with no date never enters the statement, because the request is
        //   ANDed with an `is not empty` clause on the date column — the
        //   brief's 「无日期不显示」 as SQL, not as a Rust `retain`.
        // * **form** — nothing: the field list is the schema's size and it
        //   *creates* rows rather than reading them.
        //
        // Every branch computes (total, wanted) from SQL counts, checks the
        // cache, and only then fetches — so a scroll that stays inside a window
        // still costs the count queries and nothing else, for every layout.
        let total: usize;
        let wanted: RowWindow;
        // The two models the card-shaped layouts read (the row-shaped ones read
        // `rows`, filled below): a board's columns with their own card slices,
        // and the calendar's 42 day cells.
        let mut view_rows: Vec<DbRow> = Vec::new();
        let mut board_model: Rc<VecModel<crate::DbBoardColumn>> = Rc::new(VecModel::default());
        let mut cal_model: Rc<VecModel<crate::DbCalendarDay>> = Rc::new(VecModel::default());

        // Which rows are pages (ADR-0063), for the row's own Open/name column.
        // One query for the database, not one per row: lazy pages mean this is a
        // handful of rows however large the database is.
        let pages = repo.record_pages(db).unwrap_or_default();

        // Is the window already the answer? The whole point of overscan: a
        // scroll that stays inside the window costs the count queries above and
        // nothing else. The definition text is part of the key because the
        // rules — and with them the count and the row set — live in it (D4);
        // the layout and the session stamp are D5's additions, because the
        // calendar's month and the gallery's per-row change which model the
        // same view, document and total would produce; the needle is D7's
        // (ADR-0087), for the same reason — it is in the statement and in
        // nothing the other five fields see.
        let unchanged = |wanted: RowWindow, total: usize, stamp: u64| -> bool {
            let windows = self.db_windows.borrow();
            match windows.get(&block) {
                Some(existing) => {
                    existing.view == view
                        && existing.definition == definition_text
                        && existing.layout == layout
                        && existing.stamp == stamp
                        && existing.search.as_deref() == search.as_deref()
                        && existing.window == wanted
                        && existing.total == total
                        // …and nothing has been recorded since this window was
                        // read, because *what a row paints* is a fact about
                        // stored data and not about the window's shape.
                        && existing.content == content
                }
                None => false,
            }
        };

        match layout {
            // ── board: columns are groups, cards are records ────────────────
            crate::core::database::ViewLayout::Board => {
                // The board's grouping column: the view's own `groups` rule
                // (the same key D4's picker writes — a board *is* a grouping,
                // seen horizontally), else the first option-bounded column.
                // The fallback is not written back: a default the user never
                // chose must not become a rule they have to undo.
                let spec = match rules.group {
                    Some(spec) => Some(spec),
                    None => self
                        .databases
                        .borrow()
                        .properties_of(db)
                        .find(|p| GroupSpec::admits(p.kind))
                        .map(|p| GroupSpec {
                            property: p.id,
                            kind: p.kind,
                        }),
                };
                match spec {
                    Some(spec) => {
                        let counts = match repo.group_counts(&request, &spec) {
                            Ok(counts) => self.db_order_groups(&spec, counts),
                            Err(e) => {
                                self.db_notice
                                    .borrow_mut()
                                    .push(format!("database read failed: {e}"));
                                return false;
                            }
                        };
                        let slot_counts: Vec<usize> =
                            counts.iter().map(|(_, n)| *n).collect();
                        let slots = board_slots(&slot_counts);
                        total = slot_counts.iter().sum();
                        body = TableView::rows_surface_height(metrics.row_height, slots);
                        wanted = crate::core::database::window(slots, geometry, offset);
                        if unchanged(wanted, total, stamp) {
                            return false;
                        }
                        let slices = board_window(&slot_counts, wanted);
                        let mut columns_model: Vec<crate::DbBoardColumn> =
                            Vec::with_capacity(counts.len());
                        for (group, (key, count)) in counts.iter().enumerate() {
                            // Every column is realized (its header is one small
                            // rectangle and its count came from the `GROUP BY`);
                            // its *cards* are fetched only for the slice of the
                            // slot window it reaches into.
                            let cards = match slices.iter().find(|(g, _, _)| *g == group) {
                                Some((_, skip, len)) => {
                                    match repo.window_rows_in_group(
                                        &request, &spec, key, *skip, *len,
                                    ) {
                                        Ok(rows) => db_rows_of(self.db_table_rows(db, &rows, &columns, &pages, None)),
                                        Err(e) => {
                                            self.db_notice
                                                .borrow_mut()
                                                .push(format!("database read failed: {e}"));
                                            return false;
                                        }
                                    }
                                }
                                None => Vec::new(),
                            };
                            columns_model.push(crate::DbBoardColumn {
                                label: self.db_group_label(&spec, key).into(),
                                count: *count as i32,
                                cards: ModelRc::from(Rc::new(VecModel::from(cards))),
                            });
                        }
                        board_model = Rc::new(VecModel::from(columns_model));
                    }
                    None => {
                        // No column to group by: a board with nothing to divide
                        // records into draws the empty state (the delegate's
                        // `db-row-count == 0` arm) rather than inventing one
                        // group called "All".
                        total = 0;
                        body = 0.0;
                        wanted = crate::core::database::window(0, geometry, offset);
                        if unchanged(wanted, total, stamp) {
                            return false;
                        }
                    }
                }
            }

            // ── calendar: a fixed month grid, records folded per day ────────
            crate::core::database::ViewLayout::Calendar => {
                let definition = ViewDefinition::parse(&definition_text);
                let axis = self.db_time_axis(db, &definition);
                let (year, month) = self.db_calendar_month(block);
                stamp = year as u64 * 12 + month as u64 - 1;
                body = TableView::calendar_surface_height();
                let mut cells_model: Vec<crate::DbCalendarDay> = month_cells(year, month)
                    .iter()
                    .map(|&day| crate::DbCalendarDay {
                        day,
                        count: 0,
                        records: ModelRc::default(),
                    })
                    .collect();
                if let Some((date_prop, date_kind)) = axis {
                    let lo = month_key(year, month);
                    let (next_year, next_month) = shift_month(year, month, 1);
                    let hi = month_key(next_year, next_month);
                    // The month's per-day counts: one `GROUP BY` over the date
                    // column (≤ 31 keys), *bounded the way a group list is
                    // bounded* — which is why a date column may group here even
                    // though D4's picker does not offer it: a header per day is
                    // 31 objects, a header per text value would be the table.
                    let month_filter = month_clauses(&rules.filter, date_prop, date_kind, &lo, &hi);
                    let req_month = RowRequest {
                        db,
                        title,
                        columns: &properties,
                        sorts: &rules.sorts,
                        filter: Some(&month_filter),
                        search: search.as_deref(),
                    };
                    let date_spec = GroupSpec {
                        property: date_prop,
                        kind: date_kind,
                    };
                    let groups = match repo.group_counts(&req_month, &date_spec) {
                        Ok(groups) => groups,
                        Err(e) => {
                            self.db_notice
                                .borrow_mut()
                                .push(format!("database read failed: {e}"));
                            return false;
                        }
                    };
                    // Merge to days: `2026-09-22` and `2026-09-22T10:00` are
                    // two stored shapes and one day, so the counts are folded
                    // by *day* rather than by key.
                    let mut per_day: Vec<(u32, usize)> = Vec::new();
                    for (key, count) in groups {
                        let Some(day) = group_day(&key) else { continue };
                        match per_day.iter_mut().find(|(d, _)| *d == day) {
                            Some((_, at)) => *at += count,
                            None => per_day.push((day, count)),
                        }
                    }
                    per_day.sort_by_key(|(day, _)| *day);
                    total = per_day.iter().map(|(_, count)| *count).sum();
                    wanted = RowWindow { start: 0, end: 0 };
                    if unchanged(wanted, total, stamp) {
                        return false;
                    }
                    let days = days_in_month(year, month);
                    for (day, count) in per_day {
                        let Some(at) = cells_model
                            .iter()
                            .position(|cell| cell.day == day as i32)
                        else {
                            continue;
                        };
                        // One day's peek: the same two bounds one day apart,
                        // `LIMIT CALENDAR_PEEK`. The fold's number is the
                        // `GROUP BY`'s count — the cell says "and N more"
                        // without those N ever becoming objects.
                        let day_lo = date_key(year, month, day);
                        let day_hi = if day < days {
                            date_key(year, month, day + 1)
                        } else {
                            month_key(next_year, next_month)
                        };
                        let day_filter =
                            month_clauses(&rules.filter, date_prop, date_kind, &lo, &hi);
                        let day_filter = match day_filter {
                            FilterNode::All(mut children) => {
                                children.push(date_bound(date_prop, date_kind, FilterOp::Gte, &day_lo));
                                children.push(date_bound(date_prop, date_kind, FilterOp::Lt, &day_hi));
                                FilterNode::All(children)
                            }
                            other => other,
                        };
                        let req_day = RowRequest {
                            db,
                            title,
                            columns: &properties,
                            sorts: &rules.sorts,
                            filter: Some(&day_filter),
                            search: search.as_deref(),
                        };
                        let records = match repo.window_rows(
                            &req_day,
                            RowWindow {
                                start: 0,
                                end: CALENDAR_PEEK,
                            },
                        ) {
                            Ok(rows) => db_rows_of(self.db_table_rows(db, &rows, &columns, &pages, None)),
                            Err(e) => {
                                self.db_notice
                                    .borrow_mut()
                                    .push(format!("database read failed: {e}"));
                                return false;
                            }
                        };
                        cells_model[at] = crate::DbCalendarDay {
                            day: day as i32,
                            count: count as i32,
                            records: ModelRc::from(Rc::new(VecModel::from(records))),
                        };
                    }
                } else {
                    // No date column at all: the grid still draws (a month of
                    // blanks and the nav strip) — a calendar with no axis is a
                    // fact about the schema, not an error, and the empty state
                    // says "New row" rather than pretending to have days.
                    wanted = RowWindow { start: 0, end: 0 };
                    total = 0;
                    if unchanged(wanted, total, stamp) {
                        return false;
                    }
                }
                cal_model = Rc::new(VecModel::from(cells_model));
            }

            // ── gallery: a grid of cards, windowed by card rows ─────────────
            crate::core::database::ViewLayout::Gallery => {
                // How many cards fit in a row is the *delegate's* answer (it is
                // the layer that knows the grid's width) and it reports it back;
                // until the first report, the default is what the formula gives
                // for the default editor width.
                let per_row = self
                    .db_gallery_per_row
                    .borrow()
                    .get(&block)
                    .copied()
                    .unwrap_or_else(|| TableView::gallery_per_row(GALLERY_DEFAULT_WIDTH));
                stamp = per_row as u64;
                total = match layout_total(&repo, &request, db) {
                    Ok(total) => total,
                    Err(e) => {
                        self.db_notice
                            .borrow_mut()
                            .push(format!("database read failed: {e}"));
                        return false;
                    }
                };
                let rows_total = TableView::gallery_rows(total, per_row);
                body = TableView::gallery_surface_height(total, per_row);
                wanted = crate::core::database::window(rows_total, geometry, offset);
                if unchanged(wanted, total, stamp) {
                    return false;
                }
                // One slice for the whole window: `per_row × rows` cards, which
                // the delegate chunks by `per_row` (the same number it
                // reported) into the rows it draws.
                let window = RowWindow {
                    start: wanted.start * per_row,
                    end: (wanted.end * per_row).min(total),
                };
                match repo.window_rows(&request, window) {
                    Ok(rows) => view_rows = db_rows_of(self.db_table_rows(db, &rows, &columns, &pages, None)),
                    Err(e) => {
                        self.db_notice
                            .borrow_mut()
                            .push(format!("database read failed: {e}"));
                        return false;
                    }
                }
            }

            // ── timeline: one lane per dated record, bars on a shared axis ──
            crate::core::database::ViewLayout::Timeline => {
                let definition = ViewDefinition::parse(&definition_text);
                match self.db_time_axis(db, &definition) {
                    Some((date_prop, date_kind)) => {
                        // The date (and the optional end) must be *painted*
                        // even when the view hides them, because the lane's bar
                        // is read out of the row: the request's column list is
                        // the view's plus whichever of the two is missing.
                        let mut tl_columns = properties.clone();
                        let date_at =
                            push_column(&mut tl_columns, &self.databases.borrow(), date_prop);
                        let end_at: Option<usize> = definition
                            .end_column()
                            .filter(|id| *id != date_prop)
                            .and_then(|id| {
                                let catalog = self.databases.borrow();
                                let row = catalog.properties.iter().find(|p| p.id == id)?;
                                if matches!(
                                    row.kind,
                                    PropertyKind::Date
                                        | PropertyKind::CreatedTime
                                        | PropertyKind::LastEditedTime
                                ) {
                                    Some(push_column(&mut tl_columns, &catalog, id))
                                } else {
                                    None
                                }
                            });
                        // 「无日期不显示」: an `is not empty` clause on the date
                        // column, compiled by the clause compiler and ANDed with
                        // the view's own rules — the row set is SQL's, not a
                        // Rust `retain` on a fetched table.
                        let dated = FilterNode::Clause(FilterClause {
                            property: date_prop,
                            kind: date_kind,
                            op: FilterOp::IsNotEmpty,
                            value: FilterValue::Missing,
                        });
                        let filter = match &rules.filter {
                            Some(tree) => FilterNode::All(vec![tree.clone(), dated]),
                            None => dated,
                        };
                        let req_tl = RowRequest {
                            db,
                            title,
                            columns: &tl_columns,
                            sorts: &rules.sorts,
                            filter: Some(&filter),
                            search: search.as_deref(),
                        };
                        total = match repo.filtered_count(&req_tl) {
                            Ok(total) => total,
                            Err(e) => {
                                self.db_notice
                                    .borrow_mut()
                                    .push(format!("database read failed: {e}"));
                                return false;
                            }
                        };
                        // The axis: one `min`/`max` over the same predicate. A
                        // `None` (no admitted row holds a value) leaves a
                        // one-day axis, which draws nothing off the edge.
                        let bounds = repo.column_bounds(&req_tl, date_prop, date_kind);
                        let (first, last) = match bounds {
                            Ok(Some((low, high))) => (
                                day_number_of(&low).unwrap_or(0),
                                day_number_of(&high).unwrap_or(0),
                            ),
                            _ => (0, 0),
                        };
                        tl_start = first;
                        tl_days = (last - first + 1).max(1);
                        body = TableView::rows_surface_height(metrics.row_height, total);
                        wanted = crate::core::database::window(total, geometry, offset);
                        if unchanged(wanted, total, stamp) {
                            return false;
                        }
                        let rows = match repo.window_rows(&req_tl, wanted) {
                            Ok(rows) => rows,
                            Err(e) => {
                                self.db_notice
                                    .borrow_mut()
                                    .push(format!("database read failed: {e}"));
                                return false;
                            }
                        };
                        view_rows = db_rows_of(self.db_table_rows(db, &rows, &columns, &pages, None));
                        // The bar's day numbers, read out of the painted cells
                        // at the two positions the column list reserved. A
                        // painted date cell always starts with the stored day
                        // (`DateFormat` truncates a stamp to its date), which is
                        // what makes this one read per row instead of a second
                        // query per row.
                        for (at, row) in rows.iter().enumerate() {
                            let from = row
                                .cells
                                .get(date_at)
                                .and_then(|text| day_number_of(text));
                            let to = end_at
                                .and_then(|end| {
                                    row.cells.get(end).and_then(|text| day_number_of(text))
                                });
                            view_rows[at].tl_from = from.map(|day| day as i32).unwrap_or(-1);
                            // `起=止=同一天时画点`: no end column, or an end
                            // before the start, is a point at the start.
                            view_rows[at].tl_to = match (from, to) {
                                (Some(from), Some(to)) => to.max(from) as i32,
                                (Some(from), None) => from as i32,
                                _ => -1,
                            };
                        }
                    }
                    None => {
                        // No time axis: no lane can be placed, so the timeline
                        // treats the database as empty and the delegate says so
                        // — the honest answer, and the same one the calendar
                        // gives with no axis.
                        total = 0;
                        body = 0.0;
                        tl_start = 0;
                        tl_days = 1;
                        wanted = crate::core::database::window(0, geometry, offset);
                        if unchanged(wanted, total, stamp) {
                            return false;
                        }
                    }
                }
            }

            // ── form: the field list, not a read ────────────────────────────
            crate::core::database::ViewLayout::Form => {
                total = match layout_total(&repo, &request, db) {
                    Ok(total) => total,
                    Err(e) => {
                        self.db_notice
                            .borrow_mut()
                            .push(format!("database read failed: {e}"));
                        return false;
                    }
                };
                // One field per visible column plus the action row: the form's
                // height is the schema's, and no record is read to draw it. The
                // count text still answers "N rows" from a count query, because
                // the form is a view *of* the table even when it draws none of
                // it.
                body = TableView::form_surface_height(columns.len());
                wanted = crate::core::database::window(total, geometry, offset);
                if unchanged(wanted, total, stamp) {
                    return false;
                }
            }

            // ── chart (D7): the plot is the group list, and it realizes no row ─
            //
            // The red line here is not about *how many* rows a window takes —
            // it is about there being **no row window at all**. The chart's
            // surface is the constant `CHART_HEIGHT` (it does not grow with
            // the data), its shapes are the *group list* — one `GROUP BY` over
            // an option-bounded column, the exact query the board's columns
            // and the grouped table's headers already run — and the keys in it
            // are as bounded as those headers are: a checkbox has two, a
            // select/status its option list. So the plot of a 10 000-row
            // database is a handful of `(label, count)` scalars, the records
            // themselves never become objects, and the numbers inside the
            // shapes came from SQL like every other layout's counts.
            //
            // The shapes (ADR-0060's one kind, SPEC's bar / line / pie from
            // the existing primitives — no chart library):
            //
            // * **bar** — equal-width rectangles, height = `frac · plot`;
            // * **line** — one polyline through the fractions
            //   (`chart_line_path`, a 100×100 viewbox the delegate scales);
            // * **pie** — one filled path per slice
            //   (`chart_pie_paths`, cubic arcs built here, where trig lives).
            //
            // The grouping column is the view's own `groups` rule, else the
            // first option-bounded column — the board's fallback, and also not
            // written back: a default nobody chose is not a rule they have to
            // undo. With no column to group by, the chart draws its empty
            // state (the header's Group picker is how it gets one).
            crate::core::database::ViewLayout::Chart => {
                let spec = match rules.group {
                    Some(spec) => Some(spec),
                    None => self
                        .databases
                        .borrow()
                        .properties_of(db)
                        .find(|p| GroupSpec::admits(p.kind))
                        .map(|p| GroupSpec {
                            property: p.id,
                            kind: p.kind,
                        }),
                };
                match spec {
                    Some(spec) => {
                        let counts = match repo.group_counts(&request, &spec) {
                            Ok(counts) => self.db_order_groups(&spec, counts),
                            Err(e) => {
                                self.db_notice
                                    .borrow_mut()
                                    .push(format!("database read failed: {e}"));
                                return false;
                            }
                        };
                        total = counts.iter().map(|(_, n)| *n).sum();
                        body = TableView::chart_surface_height();
                        // No row window: the plot is finished with the records
                        // once the `GROUP BY` has answered. The `unchanged`
                        // cache still gates the rebuild below, keyed the same
                        // way every layout's is.
                        wanted = RowWindow {
                            start: 0,
                            end: 0,
                        };
                        if unchanged(wanted, total, stamp) {
                            return false;
                        }
                        // The chart's shape is the document's `chart` key —
                        // re-read here (the parse is once per refresh and the
                        // document is small), so a kind switch is the same
                        // definition edit every other rule is.
                        let kind = ViewDefinition::parse(&definition_text).chart_kind();
                        chart_kind = kind.index() as i32;
                        let max = counts.iter().map(|(_, n)| *n).max().unwrap_or(0) as f64;
                        let fracs: Vec<f64> = counts
                            .iter()
                            .map(|(_, n)| if max > 0.0 { *n as f64 / max } else { 0.0 })
                            .collect();
                        let slices = chart_pie_paths(
                            &counts.iter().map(|(_, n)| *n as f64).collect::<Vec<_>>(),
                        );
                        if kind == ChartKind::Line {
                            chart_path = chart_line_path(&fracs);
                        }
                        let points: Vec<crate::DbChartPoint> = counts
                            .iter()
                            .enumerate()
                            .map(|(at, (key, count))| crate::DbChartPoint {
                                // the header's own wording — a label is a
                                // word Rust made, not a lookup the delegate
                                // does (ADR-0076's rule, restated)
                                label: self.db_group_label(&spec, key).into(),
                                value: *count as i32,
                                // the palette slot the shape is tinted with:
                                // the block palette's nine slots cycling, until
                                // an option editor exists to carry real option
                                // colors (D5's undone half) — the slot word is
                                // decoration, the count is the datum
                                slot: (at % 9 + 1) as i32,
                                frac: fracs[at] as f32,
                                commands: slices[at].clone().into(),
                            })
                            .collect();
                        chart_model = Rc::new(VecModel::from(points));
                    }
                    None => {
                        // No grouping yet: the plot draws its "pick a column"
                        // state over the database's real count — the header
                        // still says how many rows the view has, because the
                        // chart is a view *of* the table even before it
                        // aggregates it. (Empty db → 0, and the empty state
                        // is the ordinary "No rows yet".)
                        total = match layout_total(&repo, &request, db) {
                            Ok(total) => total,
                            Err(e) => {
                                self.db_notice
                                    .borrow_mut()
                                    .push(format!("database read failed: {e}"));
                                return false;
                            }
                        };
                        // The surface keeps the chart's constant height even
                        // with nothing to plot: the "pick a column" state
                        // centers inside the same body a plotted chart has,
                        // and the block never collapses to a sliver.
                        body = TableView::chart_surface_height();
                        wanted = RowWindow {
                            start: 0,
                            end: 0,
                        };
                        if unchanged(wanted, total, stamp) {
                            return false;
                        }
                    }
                }
            }

            // ── table / list ────────────────────────────────────────────────
            _ => {
                // A grouped view counts by its group query (one `GROUP BY` over
                // an option-bounded column — the list of *headers*, a handful of
                // rows); its entries are then Σ(count + 1). An ungrouped view
                // counts with one `COUNT(*)` — over the filter's predicate when
                // it has one, over the database alone when it does not. Either
                // way the window arithmetic is computed FROM that number: filter
                // a 10 000-row database down to 3 rows and the window realizes 3
                // rows, and a grouped one realizes 3 rows plus its headers —
                // never the table, never one row per group.
                let counts = match &rules.group {
                    Some(spec) => match repo.group_counts(&request, spec) {
                        Ok(counts) => Some(self.db_order_groups(spec, counts)),
                        Err(e) => {
                            self.db_notice
                                .borrow_mut()
                                .push(format!("database read failed: {e}"));
                            return false;
                        }
                    },
                    None => None,
                };
                total = match &counts {
                    Some(counts) => counts.iter().map(|(_, n)| n + 1).sum(),
                    None => match layout_total(&repo, &request, db) {
                        Ok(total) => total,
                        Err(e) => {
                            self.db_notice
                                .borrow_mut()
                                .push(format!("database read failed: {e}"));
                            return false;
                        }
                    },
                };
                // The list's own row height comes from `layout_metrics`, so the
                // block's height and the window's division agree by
                // construction (a 44 px list row realizes 44 px of surface).
                body = TableView::rows_surface_height(metrics.row_height, total);
                wanted = crate::core::database::window(total, geometry, offset);
                if unchanged(wanted, total, stamp) {
                    return false;
                }

                match &counts {
                    None => {
                        let rows = match repo.window_rows(&request, wanted) {
                            Ok(rows) => rows,
                            Err(e) => {
                                self.db_notice
                                    .borrow_mut()
                                    .push(format!("database read failed: {e}"));
                                return false;
                            }
                        };
                        view_rows = db_rows_of(self.db_table_rows(db, &rows, &columns, &pages, None));
                    }
                    Some(counts) => {
                        let spec = rules.group.as_ref().expect("counts imply a group");
                        // A group header is one entry and its rows follow it:
                        // `group_window` maps the entry window onto per-group
                        // slices, so the rows realized are the viewport's
                        // wherever they sit relative to their header — a group
                        // holding all 10 000 rows realizes the same 31 rows it
                        // would ungrouped, and each group costs one header
                        // entry, never one row per group.
                        let surface =
                            group_window(&counts.iter().map(|(_, n)| *n).collect::<Vec<_>>(), wanted);
                        let mut entries: Vec<DbRow> = (0..wanted.len())
                            .map(|_| DbRow::header())
                            .collect();
                        for slice in &surface.rows {
                            let (key, _) = &counts[slice.group];
                            let rows = match repo.window_rows_in_group(
                                &request, spec, key, slice.skip, slice.len,
                            ) {
                                Ok(rows) => rows,
                                Err(e) => {
                                    self.db_notice
                                        .borrow_mut()
                                        .push(format!("database read failed: {e}"));
                                    return false;
                                }
                            };
                            for (at, row) in
                                db_rows_of(self.db_table_rows(db, &rows, &columns, &pages, None))
                                .into_iter()
                                .enumerate()
                            {
                                entries[slice.at - wanted.start + at] = row;
                            }
                        }
                        for (group, header_at) in &surface.headers {
                            let (key, count) = &counts[*group];
                            let label = self.db_group_label(spec, key);
                            entries[header_at - wanted.start] =
                                DbRow::header_with(&format!("{label} · {count}"));
                        }
                        view_rows = entries;
                    }
                }
            }
        }
        // (`ViewRules::note` — the visible degradation — is not toasted here:
        // it rides the block row, and `db_fill_row` draws it where the row
        // count would be, because a filter that is not being applied is a fact
        // about every frame the user looks at, not about the moment it was
        // noticed.)

        let mut windows = self.db_windows.borrow_mut();
        let entry = windows.entry(block).or_insert_with(|| DbWindow {
            view,
            definition: String::new(),
            layout,
            content,
            stamp: 0,
            search: None,
            columns: Vec::new(),
            total,
            window: wanted,
            rows: Rc::new(VecModel::from(Vec::new())),
            board: Rc::new(VecModel::default()),
            cal: Rc::new(VecModel::default()),
            body: 0.0,
            tl_start: 0,
            tl_days: 1,
            chart: Rc::new(VecModel::default()),
            chart_path: Rc::new(String::new()),
            chart_kind: 0,
        });
        entry.view = view;
        entry.definition = definition_text;
        entry.layout = layout;
        entry.content = content;
        entry.stamp = stamp;
        entry.search = search.clone();
        entry.columns = columns;
        entry.total = total;
        entry.window = wanted;
        entry.body = body;
        entry.tl_start = tl_start;
        entry.tl_days = tl_days;
        entry.rows.set_vec(view_rows);
        entry.board = board_model;
        entry.cal = cal_model;
        entry.chart = chart_model;
        entry.chart_path = Rc::new(chart_path);
        entry.chart_kind = chart_kind;
        true
    }

    /// Give every database block on the open page the chance to re-read its
    /// window, `skip` being the one whose window the caller has just settled.
    ///
    /// A database write can change what a *different* block paints, and M14 is
    /// the first slice that makes that true: a pick writes the target row's
    /// back-pointer cell (in the target database's own block), and a rename in
    /// one database moves the title a relation cell in another one shows. Both
    /// of those windows are caches whose own key cannot see the write — the same
    /// blindness [`AppState::db_content_stamp`] fixes for the writing block — so
    /// the writer has to hand the peers the chance, and the stamp makes the
    /// chance free when there is nothing to see.
    ///
    /// Undo and redo call it with no `skip`: a step walks stored data back (or
    /// forward) with no write call site of its own, which is exactly the shape
    /// that would otherwise leave the screen holding a value the file no longer
    /// has.
    ///
    /// Bounded by the **open page's** database blocks (a handful), never by the
    /// library: a block on another page re-reads when that page is projected,
    /// which is the same "a window is read where it is drawn" rule the row table
    /// follows. Nothing here writes, and nothing recurses — the peers are handed
    /// to `db_refresh` directly, so a page of three databases costs at most
    /// three reads for one edit.
    fn db_refresh_page(&self, skip: Option<i32>) {
        let page = core_page_id(self.open_page.get());
        let peers: Vec<i32> = {
            let doc = self.doc.borrow();
            doc.page_blocks(page)
                .iter()
                .filter(|b| b.db_ref.is_some() && skip != Some(b.id.0 as i32))
                .map(|b| b.id.0 as i32)
                .collect()
        };
        for peer in peers {
            self.db_refresh(peer);
        }
    }

    /// The group headers' order. SQL returned the keys unordered on purpose:
    /// the order a user means is the schema's own option order (ADR-0061),
    /// which lives in the column's config JSON where SQL cannot see it — so
    /// these few rows are ordered here, from that same config. A select/status
    /// follows its option list; an option id the config no longer has follows
    /// the known ones in byte order (ADR-0069's fold, applied to a header); a
    /// checkbox is unchecked-then-checked (the `false` before `true` ADR-0070
    /// sorts by); "no value" is last, so the empty group never floats over the
    /// real ones. This is ordering a handful of *headers* — the rows inside a
    /// group are SQL's, each slice from its own ordered query.
    fn db_order_groups(
        &self,
        spec: &GroupSpec,
        counts: Vec<(GroupKey, usize)>,
    ) -> Vec<(GroupKey, usize)> {
        let config = self.db_property_config(spec.property.as_u64() as i32);
        let options = PropertyOptions::from_config(&config);
        let rank = |key: &GroupKey| -> (u64, String) {
            match key {
                GroupKey::Unchecked => (0, String::new()),
                GroupKey::Checked => (1, String::new()),
                GroupKey::Option(id) => {
                    match options.iter().position(|o| o.id.as_u64().to_string() == *id) {
                        Some(at) => (2 + at as u64, String::new()),
                        None => (u64::MAX / 2, id.clone()),
                    }
                }
                GroupKey::Empty => (u64::MAX, String::new()),
            }
        };
        let mut ordered = counts;
        ordered.sort_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)));
        ordered
    }

    /// What one group's header says: the option's name as the column's config
    /// spells it, the checkbox's two states, and "No value" for the empty
    /// group. An id the config forgot names itself (ADR-0069's fold) — a group
    /// header that invented a name would be a second copy of the schema.
    fn db_group_label(&self, spec: &GroupSpec, key: &GroupKey) -> String {
        match key {
            GroupKey::Empty => "No value".to_string(),
            GroupKey::Unchecked => "Unchecked".to_string(),
            GroupKey::Checked => "Checked".to_string(),
            GroupKey::Option(id) => {
                let config = self.db_property_config(spec.property.as_u64() as i32);
                PropertyOptions::from_config(&config)
                    .iter()
                    .find(|o| o.id.as_u64().to_string() == *id)
                    .map(|o| o.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| id.clone())
            }
        }
    }

    /// The projection handed to a `Database` block's row: tabs, columns, the
    /// realized window, and the two numbers that place it in the scroll surface.
    /// `None` for a block with no entity, which the delegate draws as ADR-0060's
    /// "(deleted database)".
    pub fn db_table(&self, block: i32) -> Option<TableView> {
        let db = self.db_ref_of(block)?;
        let catalog = self.databases.borrow();
        let active = self.db_active_view(block)?;
        let row = catalog.views.iter().find(|v| v.id == active)?;
        let layout = row.layout;
        let definition = ViewDefinition::parse(&row.definition);
        let properties = view_columns(&catalog, db, row);
        let columns = table_columns(&properties, &definition);
        let tabs: Vec<ViewTab> = catalog
            .views_of(db)
            .map(|view| ViewTab {
                view: view.id,
                name: view.name.clone(),
                layout: view.layout,
                active: view.id == row.id,
            })
            .collect();
        drop(catalog);
        // The window is the cache's; a projection that found none (the first
        // one after a page switch, before any geometry report) refreshes once,
        // which is also what seeds the model.
        let _ = self.db_refresh(block);
        let windows = self.db_windows.borrow();
        let window = windows.get(&block)?;
        Some(TableView {
            tabs,
            columns,
            rows: Vec::new(),
            total: window.total,
            window: window.window,
            layout,
            support: LayoutSupport::of(layout),
        })
    }

    /// The model a `Database` block's row carries: the realized rows, which the
    /// delegate reads and no one else writes.
    fn db_rows_model(&self, block: i32) -> ModelRc<DbRow> {
        match self.db_windows.borrow().get(&block) {
            Some(window) => ModelRc::from(window.rows.clone()),
            None => ModelRc::default(),
        }
    }

    /// Fill one projected row's SPEC §三十九 fields, from the block's entity.
    ///
    /// Two callers, and the second is why this exists as a function rather than
    /// as a loop inside `reproject_blocks`:
    ///
    /// * the page projection, for every `Database` block on the page;
    /// * the **scroll** path, which must not rebuild the page's row list — a
    ///   window that moved past its overscan changes `db-row-start`, and the
    ///   delegate needs that number, but re-projecting a 10 000-block page to
    ///   deliver one integer would be the same defect as realizing the whole
    ///   table. The realized rows themselves arrive through `db_rows_model`'s
    ///   `VecModel`, which the refresh mutates in place.
    ///
    /// A block whose entity is gone gets every field cleared, which is what the
    /// delegate reads as ADR-0060's "(deleted database)": a row that was drawn
    /// before the entity was deleted must not keep drawing its last window.
    pub fn db_fill_row(&self, row: &mut BlockRow) {
        row.db_ok = false;
        row.db_title = "".into();
        row.db_rows = ModelRc::default();
        row.db_columns = ModelRc::default();
        row.db_views = ModelRc::default();
        row.db_row_start = 0;
        row.db_row_count = 0;
        row.db_layout = "".into();
        row.db_layout_index = 0;
        row.db_layout_ok = true;
        // The rules' header state (D4): no filter, no sort, no group, no note —
        // refilled below from the active view's document.
        row.db_filter_note = "".into();
        row.db_filter_count = 0;
        row.db_sort_property = -1;
        row.db_sort_desc = false;
        row.db_group_property = -1;
        // D5: the view family's own fields, refilled below from the window
        // cache and the session state — the surface height, the board's
        // columns, the calendar's grid, the gallery's shape, the timeline's
        // axis, the form's draft.
        row.db_body_height = 0.0;
        row.db_board_columns = ModelRc::default();
        row.db_cal_days = ModelRc::default();
        row.db_cal_label = "".into();
        row.db_gallery_per_row = 1;
        row.db_tl_start = 0;
        row.db_tl_days = 0;
        row.db_form = ModelRc::default();
        // D7 (chart): no points and no polyline until the refresh proves a
        // live window — the same reset every other layout's payload gets.
        row.db_chart_points = ModelRc::default();
        row.db_chart_path = "".into();
        row.db_chart_kind = 0;
        // The two constants the window arithmetic is laid out at, handed to the
        // delegate rather than restated in .slint: `core::database::window`
        // divides the scroll offset by the *layout's* row height, and the block
        // is as tall as HEADER_HEIGHT + its layout's surface — one source, or
        // the view would fetch a window that does not cover its own viewport.
        // Set before the early return: even a dangling entity's one muted line
        // is laid out at the same geometry.
        row.db_row_height = TableView::ROW_HEIGHT;
        row.db_header_height = TableView::HEADER_HEIGHT;
        // `db_ref` is not reset: it is the block's own pointer, projected from
        // the document by `block_row`, and it is what says the block *meant* to
        // draw a database at all.
        let block = row.id;
        let Some(view) = self.db_table(block) else {
            return;
        };
        row.db_ok = true;
        // D5: the layout's own geometry. The row height is the placement unit
        // the window was computed with (a table row, a list row, a timeline
        // lane, a board slot, a gallery card row), and the surface height is
        // the window cache's `body` — the block's height is now the layout's
        // own shape, not one formula in the delegate.
        let metrics = layout_metrics(view.layout);
        row.db_row_height = metrics.row_height;
        row.db_header_height = metrics.header_height;
        // The rules' header state, from the same document the window was read
        // with: how many clauses the filter holds (the button's chip and its
        // active tint), the first sort term (the header's arrow — the panel
        // edits that term; a document with more terms still sorts by all of
        // them), the group column, and the note (a filter that could not be
        // applied, drawn where the row count would be).
        if let Some(db) = self.db_ref_of(block) {
            if let Some(view_id) = self.db_active_view(block) {
                let rules = self.db_rules(db, view_id);
                row.db_filter_note = rules.note.clone().into();
                row.db_filter_count = rules
                    .filter
                    .as_ref()
                    .map(|f| f.clause_count() as i32)
                    .unwrap_or(0);
                if let Some(first) = rules.sorts.first() {
                    row.db_sort_property = first.property.as_u64() as i32;
                    row.db_sort_desc = first.descending;
                }
                if let Some(spec) = &rules.group {
                    row.db_group_property = spec.property.as_u64() as i32;
                }
            }
        }
        row.db_title = self
            .db_ref_of(block)
            .and_then(|db| self.databases.borrow().database(db).map(|d| d.name.clone()))
            .unwrap_or_default()
            .into();
        row.db_rows = self.db_rows_model(block);
        row.db_row_start = self.db_row_start(block);
        row.db_row_count = self.db_row_count(block);
        row.db_layout = view.layout.label().into();
        row.db_layout_index = view.layout.index();
        row.db_layout_ok = view.support.is_drawn();
        // An "auto" width is a *share*, and the share is Rust's to work out: a
        // delegate cannot fold a list, and Slint's layout gives an explicit
        // `width` binding the last word over `horizontal-stretch` — so the 0
        // this projection used to hand over was a column of no width at all,
        // which is how every table body stayed blank until D10 measured it.
        // What the dragged columns take is subtracted from the grid and divided
        // equally among the rest, never below the floor the document enforces.
        let dragged: i32 = view
            .columns
            .iter()
            .map(|c| if c.width == WIDTH_AUTO { 0 } else { c.width as i32 })
            .sum();
        let autos = view
            .columns
            .iter()
            .filter(|c| c.width == WIDTH_AUTO)
            .count() as i32;
        let share = if autos == 0 {
            0
        } else {
            ((WIDTH_UNIT as i32 - dragged).max(0) / autos).max(WIDTH_MIN as i32)
        };
        row.db_columns = ModelRc::from(Rc::new(VecModel::from(
            view.columns
                .iter()
                .map(|column| DbColumn {
                    property: column.property.as_u64() as i32,
                    name: column.name.clone().into(),
                    kind: property_kind_int(column.kind),
                    permille: if column.width == WIDTH_AUTO {
                        share
                    } else {
                        column.width as i32
                    },
                    title: column.title,
                    options: ModelRc::from(Rc::new(VecModel::from(column
                        .options
                        .iter()
                        .map(|option| DbOption {
                            id: option.id.clone().into(),
                            name: option.name.clone().into(),
                            color: option.color.clone().into(),
                        })
                        .collect::<Vec<_>>()))),
                })
                .collect::<Vec<_>>(),
        )));
        row.db_views = ModelRc::from(Rc::new(VecModel::from(
            view.tabs
                .iter()
                .map(|tab| DbViewTab {
                    view: tab.view.as_u64() as i32,
                    name: tab.name.clone().into(),
                    active: tab.active,
                })
                .collect::<Vec<_>>(),
        )));
        // ── D5: the layout's own payload, from the window cache ─────────────
        // The surface height, the timeline's axis, and the per-layout model the
        // delegate reads (a board's columns, the calendar's grid) — one read of
        // the cache the refresh just settled, so the block row carries the
        // layout's whole shape and the delegate does no arithmetic but the
        // placement the window already decided.
        let windows = self.db_windows.borrow();
        let Some(window) = windows.get(&block) else {
            return;
        };
        row.db_body_height = window.body;
        row.db_tl_start = window.tl_start as i32;
        row.db_tl_days = window.tl_days as i32;
        row.db_board_columns = ModelRc::from(window.board.clone());
        row.db_cal_days = ModelRc::from(window.cal.clone());
        let (cal_year, cal_month) = self.db_calendar_month(block);
        row.db_cal_label = month_label(cal_year, cal_month).into();
        row.db_gallery_per_row = self
            .db_gallery_per_row
            .borrow()
            .get(&block)
            .copied()
            .unwrap_or_else(|| TableView::gallery_per_row(GALLERY_DEFAULT_WIDTH)) as i32;
        // The form's draft: one field per visible column, the draft text (a
        // field left empty is `""` — an empty draft *is* the cleared state), in
        // the order the view shows the columns.
        let draft = self.db_form.borrow().get(&block).cloned().unwrap_or_default();
        row.db_form = ModelRc::from(Rc::new(VecModel::from(
            view.columns
                .iter()
                .map(|column| {
                    let text = draft
                        .iter()
                        .find(|(property, _)| *property == column.property.as_u64())
                        .map(|(_, text)| text.clone())
                        .unwrap_or_default();
                    crate::DbFormField {
                        property: column.property.as_u64() as i32,
                        name: column.name.clone().into(),
                        kind: property_kind_int(column.kind),
                        text: text.into(),
                    }
                })
                .collect::<Vec<_>>(),
        )));
        // D7 (chart): the plot's points and the line's polyline, from the
        // window cache the refresh settled — the delegate draws them and does
        // no arithmetic but the placement (the path is pre-scaled by its
        // viewbox).
        row.db_chart_points = ModelRc::from(window.chart.clone());
        row.db_chart_path = (*window.chart_path).clone().into();
        row.db_chart_kind = window.chart_kind;
    }

    /// The window's first row index — the row→model conversion §三十七 requires:
    /// the model holds the window's rows, and row *n* of the database is model
    /// index `n - start`.
    fn db_row_start(&self, block: i32) -> i32 {
        self.db_windows
            .borrow()
            .get(&block)
            .map(|w| w.window.start as i32)
            .unwrap_or(0)
    }

    fn db_row_count(&self, block: i32) -> i32 {
        self.db_windows
            .borrow()
            .get(&block)
            .map(|w| w.total as i32)
            .unwrap_or(0)
    }

    /// The columns of the block's active view, for the columns popup.
    pub fn db_column_toggles(&self, block: i32) -> Vec<DbColumnToggle> {
        let Some(db) = self.db_ref_of(block) else {
            return Vec::new();
        };
        let Some(view) = self.db_active_view(block) else {
            return Vec::new();
        };
        let catalog = self.databases.borrow();
        let Some(row) = catalog.views.iter().find(|v| v.id == view) else {
            return Vec::new();
        };
        let definition = ViewDefinition::parse(&row.definition);
        let all = all_columns(&catalog, db);
        catalog
            .properties_of(db)
            .map(|property| DbColumnToggle {
                property: property.id.as_u64() as i32,
                name: property.name.clone(),
                kind: property.kind.as_str().to_string(),
                visible: definition.shows(property.id, &all),
                // ADR-0063: the title column is a row's name, so it has no
                // switch — the popup shows it locked instead of offering a
                // toggle the table would have to refuse.
                locked: property.kind.is_title(),
            })
            .collect()
    }

    /// Turn a line into a database block: the block, the entity, its `title`
    /// column and its first view, in one `Entry` (ADR-0060/0061). Returns `true`
    /// when it happened.
    ///
    /// The ids are allocated here because this is the layer that holds the
    /// store's watermarks (ADR-0072) — one counter per table, seeded from
    /// `MAX(id)` at startup and never read again, so two creations in the same
    /// batch cannot collide even though neither row is in the file yet (the write
    /// path is debounced).
    pub fn make_database(&self, block: i32) -> bool {
        let name = {
            // The database's own name: the page it is first created on, which is
            // what a link to it would say. Renameable later (`DatabaseRenamed`).
            let ws = self.workspace.borrow();
            ws.title_of(self.open_page.get())
                .unwrap_or("Database")
                .to_string()
        };
        let draft = DatabaseDraft::new(
            DatabaseId(self.next_db_id.get()),
            PropertyId(self.next_property_id.get()),
            ViewId(self.next_view_id.get()),
            name,
        );
        // The line gives up its words in the same batch that makes it a
        // database: a `Database` block's `text` has no surface to be drawn on
        // (the view draws records), and words kept behind the view would be
        // two owners of one decision — the same rule the Synced conversion
        // follows (ADR-0052). One batch, one Ctrl+Z restores the line whole
        // (`exec_all` skips a command that plans to nothing, so the empty-line
        // case — the "+"-menu's usual caller — costs nothing).
        let Some(changes) = self.exec_all_on_open_page(vec![
            Command::ReplaceText {
                id: BlockId(block as u64),
                text: String::new(),
            },
            Command::MakeDatabase {
                id: BlockId(block as u64),
                draft,
            },
        ]) else {
            return false;
        };
        // The counters move only when the write is planned: a refused command
        // (`MakeDatabase` refuses a cell, a container's child, or a block that
        // already has an entity) must not burn an id.
        self.next_db_id.set(self.next_db_id.get() + 1);
        self.next_property_id.set(self.next_property_id.get() + 1);
        self.next_view_id.set(self.next_view_id.get() + 1);
        let _ = changes;
        self.db_refresh(block);
        true
    }

    /// Add one row to the database at `ord` (the end of the listing), starting
    /// from the database's **record template** (SPEC §三十九 「操作」,
    /// ADR-0086) when it has one. Returns the new record's id.
    ///
    /// The prefill is not a second write path: the template's cells — already
    /// in the shapes [`CellValue`] stores, because a template is a copy of
    /// stored content — become ordinary `SetDatabaseCell` commands in the
    /// **same batch** as the creation, so a new row arrives complete and one
    /// Ctrl+Z takes record and prefill back together. The template names
    /// values by property id, and a property the schema has since lost simply
    /// has no command to name (the catalog lookup in `db_template_cells` is
    /// the filter; a deleted column cannot prefill).
    pub fn db_add_record(&self, block: i32, ord: OrderKey) -> Option<i64> {
        let id = self.next_record_id.get();
        let mut cmds = vec![Command::AddDatabaseRecord {
            block: BlockId(block as u64),
            record: RecordId(id),
            ord,
        }];
        if let Some(db) = self.db_ref_of(block) {
            let cells = self.db_template_cells(db);
            cmds.extend(cells.into_iter().map(|(property, to)| Command::SetDatabaseCell {
                block: BlockId(block as u64),
                record: RecordId(id),
                property,
                // the record does not exist yet, so every prefill writes from
                // empty — the same `from` the form's submit uses, for the same
                // reason (the plan reads the document, not the file)
                from: CellValue::Empty,
                to,
            }));
        }
        self.exec_all_on_open_page(cmds)?;
        self.next_record_id.set(id + 1);
        // The new row is at the end: the window has to be recomputed, and a
        // database that fits on one screen re-reads instantly.
        self.db_refresh(block);
        Some(id as i64)
    }

    /// The next row's order key: one stride past the last row the *store*
    /// reports, asked once per new row (a `MAX(ord)` on an index, not a table
    /// scan) because the app deliberately does not hold the rows of a 10 000-row
    /// database to find the last one (ADR-0067).
    pub fn db_next_row_ord(&self, block: i32) -> OrderKey {
        let Some(db) = self.db_ref_of(block) else {
            return OrderKey::FIRST;
        };
        let Some(repo) = self.db_repo() else {
            return OrderKey::FIRST;
        };
        match repo.last_record_ord(db) {
            Ok(Some(ord)) => OrderKey::between(Some(ord), None).unwrap_or(OrderKey(ord.0 + OrderKey::STRIDE)),
            _ => OrderKey::FIRST,
        }
    }

    /// One new column at the end of the schema (ADR-0061's `ord`, past the last
    /// one). Returns the new property's id as an int, or `None` when the name is
    /// unusable.
    ///
    /// **Refused here rather than left to SQL**, for two reasons that are the
    /// same reason: `db_properties` has `UNIQUE (db, name)`, and a write that
    /// trips it fails inside the debounced flush — where nobody is listening —
    /// so the app would show a column that is not in the file. The other is that
    /// an unnamed column has an empty header and no way back (renaming is D5's),
    /// which is a column a user cannot find. A trim, then two checks: non-empty,
    /// and not already a name of this database.
    ///
    /// The kind is a parameter because the *scene* and the D3 test plan need
    /// columns of the four kinds the inline editors carry, while the UI's one
    /// button makes a text column — a kind picker is D5's, and a caller that
    /// passes another kind is not doing anything the storage layer minds
    /// (ADR-0062's shape is per kind, not per creation path).
    pub fn db_add_column(&self, block: i32, name: &str, kind: PropertyKind) -> Option<i32> {
        let db = self.db_ref_of(block)?;
        let name = name.trim();
        if name.is_empty() || kind.is_title() {
            return None;
        }
        let ord = {
            let catalog = self.databases.borrow();
            if catalog.properties_of(db).any(|p| p.name == name) {
                return None;
            }
            // The catalog holds every column (a schema is a handful of rows, and
            // ADR-0067 keeps only *records* out of memory), so the end of the
            // order is a fold in memory and not a `MAX(ord)` query.
            catalog
                .properties_of(db)
                .map(|p| p.ord)
                .max()
                .map(|last| {
                    OrderKey::between(Some(last), None).unwrap_or(OrderKey(last.0 + OrderKey::STRIDE))
                })
                .unwrap_or(OrderKey::FIRST)
        };
        let id = self.next_property_id.get();
        let property = Property {
            id: PropertyId(id),
            db,
            name: name.to_string(),
            kind,
            // Born with an empty config: a select's options, a number's format
            // and a rollup's target all live in this one document (ADR-0061) and
            // all of them are D5's editors to fill.
            config: String::new(),
            ord,
        };
        self.exec_on_open_page(Command::AddDatabaseProperty {
            block: BlockId(block as u64),
            property,
        })?;
        // The counter moves only for a write that was planned (ADR-0072).
        self.next_property_id.set(id + 1);
        // A new column changes every row's cells, so the window is re-read: the
        // store paints cells against the column list it was handed, and a header
        // drawn from a newer schema than the cells is the defect that looks like
        // a sorting bug.
        self.db_refresh(block);
        Some(id as i32)
    }

    // ─── D10 (SPEC §三十九 「属性」): the column type menu ────────────────────
    //
    // D5 left this half of the schema unreachable: every kind after `text` could
    // be *stored* (`AddDatabaseProperty` takes a kind, `PropertyKindSet` writes
    // one, `parse_one` reads one) and none of them could be *chosen*. The menu is
    // therefore two questions in one list — what does this column become, and
    // what does a new column start as — and one refusal that is not about the
    // kind at all: a column another column depends on cannot move without
    // stranding that dependency, and ADR-0088's involution and ADR-0089's
    // acyclic fold are both properties of the *schema*, not of one column.

    /// Why this column's type cannot change, if it cannot. `None` = the menu is
    /// live for it.
    ///
    /// Four shapes refuse, and each is one of the schema's own invariants rather
    /// than a general "is anything referring to this?" scan:
    ///
    /// * the **title** column — a row's name is its title (ADR-0063), so a
    ///   database without one has headers with nothing under them;
    /// * a relation that **is paired** — its partner's config names this column,
    ///   and a relation that stops being one leaves that pointer pointing at a
    ///   text column, which is ADR-0088's involution broken by a side door;
    /// * a column that **is somebody's mirror** — the same pair seen from the
    ///   other end;
    /// * a column a **rollup** folds through, as its relation or as its values —
    ///   ADR-0089's acyclicity is a statement about two configs agreeing, and
    ///   moving one of them under the other is how the cycle it refuses at
    ///   config time arrives by *edit* instead.
    fn kind_move_refusal(&self, property: PropertyId) -> Option<&'static str> {
        let catalog = self.databases.borrow();
        let Some(row) = catalog.properties.iter().find(|p| p.id == property) else {
            return Some("That column no longer exists.");
        };
        if row.kind.is_title() {
            return Some("A database's title column is what its rows are named by.");
        }
        if row.kind == PropertyKind::Relation {
            if database_relation::config_relation(&row.config).mirror.is_some() {
                return Some(
                    "This relation is two-way — clear its back-pointer before changing its type.",
                );
            }
            if catalog.properties.iter().any(|p| {
                p.kind == PropertyKind::Relation
                    && database_relation::config_relation(&p.config).mirror == Some(property)
            }) {
                return Some("Another relation points back through this column.");
            }
        }
        if catalog.properties.iter().any(|p| {
            if p.kind != PropertyKind::Rollup {
                return false;
            }
            let fold = database_rollup::config_rollup(&p.config);
            fold.relation == Some(property) || fold.column == Some(property)
        }) {
            return Some("A rollup folds through this column.");
        }
        None
    }

    /// The kind menu's rows: `PropertyKind::ALL`'s order — the same order the
    /// `kind` ints in every database callback mean — with the current kind
    /// marked and a row that cannot be chosen saying why in its own words.
    ///
    /// `property < 0` is the **creation** list: nothing is chosen, `Title` is the
    /// only refusal (a database is born with its title column, and a second one
    /// is not a thing the schema has a meaning for), and the caller creates the
    /// column rather than moving one.
    pub fn db_column_kinds(&self, property: i32) -> Vec<DbPickRow> {
        let current = (property >= 0)
            .then(|| self.db_property_kind(property))
            .flatten();
        // The refusal of the *column*, computed once: it is the same sentence on
        // every row, because it is about what hangs off this column and not about
        // what the user is about to pick.
        let moved = if property >= 0 {
            self.kind_move_refusal(PropertyId(property as u64))
        } else {
            None
        };
        PropertyKind::ALL
            .iter()
            .enumerate()
            .map(|(index, kind)| DbPickRow {
                id: index as i32,
                name: kind.label().to_string(),
                chosen: current == Some(*kind),
                // `title` is never a choice (a database is born with its one
                // title column), and a column something else depends on is not
                // a choice for *any* kind — hence the same sentence on every row.
                disabled: moved.is_some() || kind.is_title(),
                note: moved.unwrap_or(if kind.is_title() {
                    "A database has one title column."
                } else {
                    ""
                })
                .to_string(),
            })
            .collect()
    }

    /// A new column of one of the menu's kinds, named for it. The name is the
    /// kind's label, and only when it is free — `db_add_column` refuses a
    /// duplicate, and a user adding a second `Text` column is doing something
    /// ordinary that a name collision should not silently cancel. So the tail is
    /// counted up (`Text 2`, `Text 3`) rather than the click being dropped.
    pub fn db_column_add(&self, block: i32, kind: i32) -> Option<i32> {
        let db = self.db_ref_of(block)?;
        let kind = *PropertyKind::ALL.get(kind.max(0) as usize)?;
        if kind.is_title() {
            return None;
        }
        let base = kind.label();
        let name = {
            let catalog = self.databases.borrow();
            let taken = |name: &str| {
                catalog
                    .properties_of(db)
                    .any(|p| p.name.eq_ignore_ascii_case(name))
            };
            if !taken(base) {
                base.to_string()
            } else {
                (2..)
                    .map(|n| format!("{base} {n}"))
                    .find(|name| !taken(name))
                    .expect("a counted name eventually fits")
            }
        };
        self.db_add_column(block, &name, kind)
    }

    /// Move a column's type — the only producer `Change::PropertyKindSet` ever
    /// had (ADR-0062 wrote the change, D1 stored it, and nothing until the kind
    /// menu asked for it).
    ///
    /// ADR-0062's rule is what makes this one thin write: **the values stay**.
    /// This does not convert, clear or re-parse one cell, and the notice it says
    /// out loud when it lands is the honest form of that — a `number` column
    /// holding text shows nothing until its cells are typed again, and undoing
    /// the change shows the text again. What a conversion would need is a rule
    /// per type pair (D2's unbuilt half), and a type menu that quietly rewrote
    /// values would be a write path no undo step covers.
    pub fn db_column_kind_set(&self, block: i32, property: i32, kind: i32) -> Result<(), String> {
        let db = self.db_ref_of(block).ok_or("this block draws no database")?;
        let to = *PropertyKind::ALL
            .get(kind.max(0) as usize)
            .ok_or("that is not a column type")?;
        let property_id = PropertyId(property as u64);
        let from = {
            let catalog = self.databases.borrow();
            let Some(row) = catalog.properties.iter().find(|p| p.id == property_id) else {
                return Err("that column no longer exists".into());
            };
            if row.db != db {
                return Err("that column is not of this database".into());
            }
            row.kind
        };
        if let Some(why) = self.kind_move_refusal(property_id) {
            return Err(why.to_string());
        }
        if from == to {
            return Ok(());
        }
        let cmd = Command::SetDatabasePropertyKind {
            block: BlockId(block as u64),
            property: property_id,
            from,
            to,
        };
        if self.exec_editor(cmd).is_none() {
            return Err("the change could not be recorded".into());
        }
        // The cells change meaning, not existence, so the window is re-read
        // rather than the row re-filled: the same column list, painted through a
        // different kind.
        self.db_refresh(block);
        // A kind that computes or points has no meaning without its config, and
        // the column that just became one is the place the user is standing — so
        // the caller opens its editor (the controller), and the notice is what
        // the user hears when the type moved and the values did not.
        self.db_say(if to.is_computed() || to == PropertyKind::Relation {
            format!(
                "\"{}\" is now \"{}\" — its cells are empty until it is defined.",
                from.label(),
                to.label()
            )
        } else {
            format!(
                "\"{}\" is now \"{}\" — the stored values did not change.",
                from.label(),
                to.label()
            )
        });
        Ok(())
    }

    /// What a cell holds **as it is stored** — the text a live editor has to
    /// start with. A point read, made once per focus rather than once per
    /// projection: a painted cell is what a reader sees, and a number with a
    /// format paints (`50%`) differently from what an editor must accept
    /// (`0.5`), which is exactly the kind of difference that must not be
    /// guessed at from the painted string.
    pub fn db_cell_text(&self, block: i32, record: i64, property: i32) -> Option<String> {
        let _ = block;
        let repo = self.db_repo()?.clone();
        let value = repo
            .cell(RecordId(record as u64), PropertyId(property as u64))
            .ok()?;
        // The value's own form: `Display` for a number (so `0.5`), the stored
        // ISO text for a date, the id for a select — never the painted form.
        Some(value.display())
    }

    /// Write one cell from a text a user typed. The text is parsed **through the
    /// column's own rules** (`core::database_property::parse_one`), so a number
    /// column takes a number and a date column takes a date; the parse is the
    /// same one D2 tested, and its rejection is what the caller paints as a
    /// refusal. An empty input clears the cell (ADR-0062's one representation of
    /// empty).
    pub fn db_set_cell_text(&self, block: i32, record: i64, property: i32, text: &str) -> bool {
        let Some(kind) = self.db_property_kind(property) else {
            return false;
        };
        let config = self.db_property_config(property);
        let value = match crate::core::database_property::parse_one(kind, &config, text) {
            Ok(value) => value,
            // A rejected input keeps the old value and says so; the paint path
            // then puts the stored cell back on screen, so a bad entry reads as
            // "that did not take" instead of as a silently coerced number.
            Err(e) => {
                self.db_notice.borrow_mut().push(e.to_string());
                return false;
            }
        };
        self.db_write_cell(block, record, property, value)
    }

    /// Write one cell with an already-typed value — the checkbox and the picker
    /// paths, which know their own value and must not go through a text parse.
    pub fn db_set_cell_value(
        &self,
        block: i32,
        record: i64,
        property: i32,
        value: CellValue,
    ) -> bool {
        self.db_write_cell(block, record, property, value)
    }

    fn db_write_cell(
        &self,
        block: i32,
        record: i64,
        property: i32,
        value: CellValue,
    ) -> bool {
        let Some(repo) = self.db_repo().cloned() else {
            return false;
        };
        let record_id = RecordId(record as u64);
        let property_id = PropertyId(property as u64);
        // The old value, read here and not in the plan: `core::command::plan` has
        // no SQL, and an undo that guessed the previous value would put the wrong
        // thing back. One point read on the primary key of `db_values`.
        let from = repo.cell(record_id, property_id).unwrap_or(CellValue::Empty);
        let cmd = Command::SetDatabaseCell {
            block: BlockId(block as u64),
            record: record_id,
            property: property_id,
            from,
            to: value,
        };
        if self.exec_editor(cmd).is_none() {
            return false;
        }
        // The cell's paint is derived (an option id shows as a name, a number
        // through its format), so the row has to be re-read rather than patched:
        // one window read, which is the same cost the projection pays and the
        // only way the cell and its neighbours stay consistent.
        self.db_refresh(block);
        // …and the page's other databases, because a cell in *this* one can be
        // the title a relation cell over there paints (ADR-0088's live name).
        self.db_refresh_page(Some(block));
        true
    }

    /// A checkbox toggled: the inverse of what the cell shows, which is the
    /// stored flag (the painted word is ADR-0065's `Yes`/`No`).
    pub fn db_toggle_checkbox(&self, block: i32, record: i64, property: i32, checked: bool) -> bool {
        self.db_write_cell(block, record, property, CellValue::Flag(!checked))
    }

    /// Pick one option for a select / status cell. `option` is the **id** as a
    /// string (ADR-0061 stores ids, not labels), and picking the option that is
    /// already there clears the cell — the same gesture as unchecking a box.
    pub fn db_pick_option(
        &self,
        block: i32,
        record: i64,
        property: i32,
        option: &str,
        current: &str,
    ) -> bool {
        let value = if option == current {
            CellValue::Empty
        } else {
            CellValue::Text(option.to_string())
        };
        self.db_write_cell(block, record, property, value)
    }

    // ─── SPEC §三十九's relation and rollup (ADR-0088/0089) ─────────────────
    //
    // The data path these four functions complete is the whole of the feature
    // as far as the store is concerned: a relation is a list of record ids and
    // its mirror is an involution, a rollup is a config and a fold, and both
    // are enforced here because this is the only layer that holds a catalog and
    // a store at once. `core::command::plan` sees a `Document` and no SQL, so
    // every check that needs to know what a column *is* happens before the
    // command is built — the same split ADR-0082's formula save already uses.

    /// A relation column's settings as the UI reads them: `(target, mirror)`,
    /// each `-1` for "not set" — the negative-means-absent convention the
    /// group-by and sort slots already use for one id that may be nothing.
    pub fn db_relation_config(&self, property: i32) -> (i32, i32) {
        let config = database_relation::config_relation(&self.db_property_config(property));
        (
            config.target.map(|db| db.as_u64() as i32).unwrap_or(-1),
            config.mirror.map(|p| p.as_u64() as i32).unwrap_or(-1),
        )
    }

    /// The records a relation column may point at — the picker's list
    /// (ADR-0088), capped by the caller's `limit` so that opening a picker over
    /// a 10 000-row database is not the windowed read's red line undone by the
    /// back door.
    pub fn db_relation_candidates(
        &self,
        property: i32,
        needle: &str,
        limit: usize,
    ) -> Vec<(i32, String)> {
        let config = database_relation::config_relation(&self.db_property_config(property));
        let Some(target) = config.target else {
            return Vec::new();
        };
        let Some(repo) = self.db_repo() else {
            return Vec::new();
        };
        repo.records_named(target, needle, limit)
            .unwrap_or_default()
            .into_iter()
            .map(|(record, title)| (record as i32, title))
            .collect()
    }

    /// Declare a relation column's target database and its back-pointer
    /// (ADR-0088), or say why not.
    ///
    /// The refusals are `database_relation::check_pair`'s, checked here because
    /// they need the catalog. An accepted pairing writes **both** documents in
    /// one batch — this column's `mirror` key, and the other column's keys
    /// pointing back — because half a pair is a state the very next frame would
    /// paint as a back-pointer that never appears. A pairing that is *moved* or
    /// *cleared* unwrites the side being dropped in the same batch, and only
    /// when that side still names this column: a column re-paired from its own
    /// end is not ours to unwrite. So the mirror map is an involution after
    /// every accepted declaration, not merely after the lucky ones.
    pub fn db_relation_configure(
        &self,
        block: i32,
        property: i32,
        target: i32,
        mirror: i32,
    ) -> Result<(), String> {
        let Some(db) = self.db_ref_of(block) else {
            return Err("this block draws no database".into());
        };
        let property_id = PropertyId(property as u64);
        let target = (target >= 0).then(|| DatabaseId(target as u64));
        let wanted = (mirror >= 0).then(|| PropertyId(mirror as u64));
        let from;
        let to;
        let mut mirrors: Vec<(PropertyId, String, String)> = Vec::new();
        {
            let catalog = self.databases.borrow();
            let Some(forward) = catalog.properties.iter().find(|p| p.id == property_id) else {
                return Err("that column no longer exists".into());
            };
            if forward.db != db || forward.kind != PropertyKind::Relation {
                return Err("that column is not a relation of this database".into());
            }
            from = forward.config.clone();
            let before = database_relation::config_relation(&from);
            if let Some(dropped) = before.mirror.filter(|old| Some(*old) != wanted) {
                if let Some(other) = catalog.properties.iter().find(|p| p.id == dropped) {
                    let theirs = database_relation::config_relation(&other.config);
                    if theirs.mirror == Some(property_id) {
                        let after = database_relation::config_set_relation(
                            &other.config,
                            theirs.target,
                            None,
                        );
                        if after != other.config {
                            mirrors.push((dropped, other.config.clone(), after));
                        }
                    }
                }
            }
            if let (Some(target_db), Some(candidate)) = (target, wanted) {
                let Some(other) = catalog.properties.iter().find(|p| p.id == candidate) else {
                    return Err("that back-pointer column does not exist".into());
                };
                let theirs = database_relation::config_relation(&other.config);
                database_relation::check_pair(
                    property_id,
                    db,
                    target_db,
                    candidate,
                    &database_relation::ColumnFacts {
                        kind: other.kind,
                        db: other.db,
                        target: theirs.target,
                        mirror: theirs.mirror,
                    },
                )
                .map_err(|refusal| refusal.message().to_string())?;
                let after = database_relation::config_set_relation(
                    &other.config,
                    Some(db),
                    Some(property_id),
                );
                if after != other.config && !mirrors.iter().any(|(id, _, _)| *id == candidate) {
                    mirrors.push((candidate, other.config.clone(), after));
                }
            }
            to = database_relation::config_set_relation(&from, target, wanted);
        }
        if to == from && mirrors.is_empty() {
            return Ok(());
        }
        let cmd = Command::SetRelationConfig {
            block: BlockId(block as u64),
            property: property_id,
            from,
            to,
            mirrors,
        };
        if self.exec_editor(cmd).is_none() {
            return Err("the declaration could not be recorded".into());
        }
        self.db_refresh(block);
        // The back-pointer's own block is a different database on this page, and
        // its column headers/rows may now say something else.
        self.db_refresh_page(Some(block));
        Ok(())
    }

    /// Write a relation cell: the targets the user picked **and the
    /// back-pointers that pick implies**, in one command and therefore one
    /// Ctrl+Z (ADR-0088).
    ///
    /// An unconfigured relation refuses: a target id nothing can name is a
    /// value no cell could ever paint, and storing it would be writing a fact
    /// the database cannot show.
    pub fn db_pick_relation(
        &self,
        block: i32,
        record: i64,
        property: i32,
        targets: &[String],
    ) -> bool {
        let Some(repo) = self.db_repo().cloned() else {
            return false;
        };
        let config = database_relation::config_relation(&self.db_property_config(property));
        if !config.is_configured() {
            return false;
        }
        let property_id = PropertyId(property as u64);
        let Ok(to) = crate::core::database_property::parse_many(
            PropertyKind::Relation,
            "",
            targets,
        ) else {
            return false;
        };
        let record_id = RecordId(record as u64);
        let from = repo
            .cell(record_id, property_id)
            .unwrap_or(CellValue::Empty);
        let mirrors = match config.mirror {
            Some(mirror) => self.relation_mirrors(&repo, record_id, mirror, &from, &to),
            None => Vec::new(),
        };
        let cmd = Command::SetRelation {
            block: BlockId(block as u64),
            record: record_id,
            property: property_id,
            from,
            to,
            mirrors,
        };
        if self.exec_editor(cmd).is_none() {
            return false;
        }
        self.db_refresh(block);
        // The pick landed in the *target* database's rows too: the mirror cell
        // the write implied is a cell of another block on this page (ADR-0088).
        self.db_refresh_page(Some(block));
        true
    }

    /// The back-pointer writes one pick implies: a target that was **added**
    /// gains this record in the mirror column's list, a target that was
    /// **removed** loses it. Both directions are the same list edit on the other
    /// row, which is why this is one function and not two.
    ///
    /// A write that would change nothing is not returned at all — `plan` skips
    /// an unchanged `CellSet` anyway, and not building it keeps the command's
    /// contents equal to what actually changed, which is what makes the undo
    /// entry's size an honest number.
    fn relation_mirrors(
        &self,
        repo: &SqliteRepository,
        record: RecordId,
        mirror: PropertyId,
        from: &CellValue,
        to: &CellValue,
    ) -> Vec<(RecordId, PropertyId, CellValue, CellValue)> {
        let list_of = |value: &CellValue| match value {
            CellValue::Items(items) => items.clone(),
            _ => Vec::new(),
        };
        let (was, now) = (list_of(from), list_of(to));
        let me = record.as_u64().to_string();
        let mut out = Vec::new();
        let write = |target: &str, join: bool, out: &mut Vec<_>| {
            let Ok(id) = target.parse::<u64>() else {
                return;
            };
            let target = RecordId(id);
            let before = repo.cell(target, mirror).unwrap_or(CellValue::Empty);
            let mut list = list_of(&before);
            if join {
                if list.contains(&me) {
                    return;
                }
                list.push(me.clone());
            } else {
                let before_len = list.len();
                list.retain(|held| held != &me);
                if list.len() == before_len {
                    return;
                }
            }
            let after = if list.is_empty() {
                CellValue::Empty
            } else {
                CellValue::Items(list)
            };
            out.push((target, mirror, before, after));
        };
        for target in now.iter().filter(|t| !was.contains(t)) {
            write(target, true, &mut out);
        }
        for target in was.iter().filter(|t| !now.contains(t)) {
            write(target, false, &mut out);
        }
        out
    }

    /// A rollup column's settings as the UI reads them:
    /// `(relation, column, aggregate)`, the first two `-1` for "not set" and
    /// the third the aggregate's index in `Aggregate::ALL`.
    pub fn db_rollup_config(&self, property: i32) -> (i32, i32, i32) {
        let config = database_rollup::config_rollup(&self.db_property_config(property));
        (
            config.relation.map(|p| p.as_u64() as i32).unwrap_or(-1),
            config.column.map(|p| p.as_u64() as i32).unwrap_or(-1),
            Aggregate::ALL
                .iter()
                .position(|a| *a == config.aggregate)
                .unwrap_or(0) as i32,
        )
    }

    /// Set a rollup column's relation, target column and fold — or say why not
    /// (ADR-0089).
    ///
    /// Validated only when there are two names to validate: `none` and `count`
    /// are legal before the second half is picked, which is what lets a rollup
    /// be configured in two steps instead of refusing every intermediate state.
    /// The check is `database_rollup::check_config`'s, and the refusal it exists
    /// for is the one that makes a dependency cycle unrepresentable: a rollup
    /// may not aggregate another computed column.
    pub fn db_rollup_configure(
        &self,
        block: i32,
        property: i32,
        relation: i32,
        column: i32,
        aggregate: i32,
    ) -> Result<(), String> {
        let Some(db) = self.db_ref_of(block) else {
            return Err("this block draws no database".into());
        };
        let property_id = PropertyId(property as u64);
        let relation = (relation >= 0).then(|| PropertyId(relation as u64));
        let column = (column >= 0).then(|| PropertyId(column as u64));
        let aggregate = Aggregate::ALL
            .get(aggregate.max(0) as usize)
            .copied()
            .unwrap_or_default();
        let from;
        let to;
        {
            let catalog = self.databases.borrow();
            let Some(forward) = catalog.properties.iter().find(|p| p.id == property_id) else {
                return Err("that column no longer exists".into());
            };
            if forward.db != db || forward.kind != PropertyKind::Rollup {
                return Err("that column is not a rollup of this database".into());
            }
            from = forward.config.clone();
            if let (Some(relation_id), Some(column_id)) = (relation, column) {
                let Some(rel) = catalog.properties.iter().find(|p| p.id == relation_id) else {
                    return Err("that relation column no longer exists".into());
                };
                if rel.db != db {
                    return Err(database_rollup::ConfigRefusal::NotARelation
                        .message()
                        .into());
                }
                let rel_config = database_relation::config_relation(&rel.config);
                let Some(target_db) = rel_config.target else {
                    return Err(database_rollup::ConfigRefusal::NoTarget.message().into());
                };
                let Some(target) = catalog.properties.iter().find(|p| p.id == column_id) else {
                    return Err("that column does not exist".into());
                };
                database_rollup::check_config(&database_rollup::RollupFacts {
                    relation_kind: rel.kind,
                    relation_target: rel_config.target,
                    column_kind: target.kind,
                    column_is_in_target: target.db == target_db,
                })
                .map_err(|refusal| refusal.message().to_string())?;
            }
            to = database_rollup::config_set_rollup(&from, relation, column, aggregate);
        }
        if from == to {
            return Ok(());
        }
        let cmd = Command::SetDatabaseRollup {
            block: BlockId(block as u64),
            property: property_id,
            from,
            to,
        };
        if self.exec_editor(cmd).is_none() {
            return Err("the rollup could not be recorded".into());
        }
        self.db_refresh(block);
        // A rollup's second name is a column of the related database, whose own
        // block may be on this page and whose header the user is looking at.
        self.db_refresh_page(Some(block));
        Ok(())
    }

    // ─── D10 (SPEC §三十九 「属性」): what each editor may offer ──────────────
    //
    // The three lists below answer "what can this be set to, right now?" for
    // the relation picker and the rollup configurator. Each is built *with* the
    // check that gates it — `check_pair` for a back-pointer, `check_config` for
    // a rollup's target column — rather than with a copy of that check's rules:
    // the menu and the save path must not be able to disagree about what is
    // legal, and a disagreement would show up as a row that greys out and then
    // commits anyway.
    //
    // A row a check refuses is **listed and greyed**, with the refusal as its
    // caption. Hiding it would leave the user counting columns in a database
    // they can see, looking for the one that is missing.

    /// Every database a relation column may point at.
    ///
    /// The column's own database is on the list: ADR-0088 refuses a column
    /// being *its own* back-pointer, not a database pointing at itself, and a
    /// self-relation ("this task blocks that task") is the ordinary case the
    /// rule leaves open. The note says which row is that one.
    pub fn db_relation_databases(&self, block: i32, chosen: i32) -> Vec<DbPickRow> {
        let here = self.db_ref_of(block);
        let catalog = self.databases.borrow();
        catalog
            .databases
            .iter()
            .map(|d| DbPickRow {
                id: d.id.as_u64() as i32,
                name: d.name.clone(),
                chosen: chosen == d.id.as_u64() as i32,
                disabled: false,
                note: if Some(d.id) == here {
                    "this database".into()
                } else {
                    String::new()
                },
            })
            .collect()
    }

    /// The columns of `target` that may be this relation's back-pointer, with
    /// "no back-pointer" first (`id` -1) — the one-way relation ADR-0088 keeps
    /// legal because a mirror costs a second column to write into.
    ///
    /// Only a target database's *relation* columns can be a mirror (pairing
    /// with a text column would store the back-pointers nowhere), so the other
    /// kinds are absent rather than refused; among the relations, a row
    /// `check_pair` refuses is still drawn, greyed, saying why.
    pub fn db_relation_mirrors(&self, block: i32, property: i32, target: i32) -> Vec<DbPickRow> {
        let mut rows = vec![DbPickRow {
            id: -1,
            name: "No back-pointer".into(),
            chosen: self.db_relation_config(property).1 < 0,
            disabled: false,
            note: "one-way - the related rows will not name this one".into(),
        }];
        let Some(db) = self.db_ref_of(block) else {
            return rows;
        };
        if target < 0 {
            return rows;
        }
        let target_db = DatabaseId(target as u64);
        let forward = PropertyId(property as u64);
        let chosen = self.db_relation_config(property).1;
        let catalog = self.databases.borrow();
        rows.extend(catalog.properties_of(target_db).filter(|p| {
            p.kind == PropertyKind::Relation
        }).map(|p| {
            let theirs = database_relation::config_relation(&p.config);
            let refusal = database_relation::check_pair(
                forward,
                db,
                target_db,
                p.id,
                &database_relation::ColumnFacts {
                    kind: p.kind,
                    db: p.db,
                    target: theirs.target,
                    mirror: theirs.mirror,
                },
            )
            .err();
            DbPickRow {
                id: p.id.as_u64() as i32,
                name: p.name.clone(),
                chosen: chosen == p.id.as_u64() as i32,
                disabled: refusal.is_some(),
                note: refusal.map(|r| r.message().to_string()).unwrap_or_default(),
            }
        }));
        rows
    }

    /// The records one relation cell holds, as ids — what the picker ticks, and
    /// what a toggle adds to or takes away from.
    pub fn db_relation_held(&self, property: i32, record: i64) -> Vec<i32> {
        let Some(repo) = self.db_repo() else {
            return Vec::new();
        };
        let Ok(value) = repo.cell(RecordId(record as u64), PropertyId(property as u64)) else {
            return Vec::new();
        };
        match value {
            CellValue::Items(items) => items
                .iter()
                .filter_map(|target| target.parse::<u64>().ok())
                .map(|target| target as i32)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The picker's candidate list: the target database's rows by title, this
    /// cell's own targets ticked. `limit` caps it for the reason
    /// [`Self::db_relation_candidates`] gives.
    pub fn db_relation_rows(
        &self,
        property: i32,
        record: i64,
        needle: &str,
        limit: usize,
    ) -> Vec<DbPickRow> {
        let held = self.db_relation_held(property, record);
        self.db_relation_candidates(property, needle, limit)
            .into_iter()
            .map(|(id, name)| DbPickRow {
                id,
                name: if name.is_empty() { "Untitled".into() } else { name },
                chosen: held.contains(&id),
                disabled: false,
                note: String::new(),
            })
            .collect()
    }

    /// Add or remove one record from a relation cell — the picker's click.
    ///
    /// The whole list is rewritten rather than the one id, because
    /// [`Self::db_pick_relation`] writes a cell *and* every back-pointer the
    /// change implies as one command: a pick is one Ctrl+Z whether it added the
    /// third target or cleared the last. `false` means nothing was written —
    /// the column has no target database, or the write refused — and the caller
    /// is what says so out loud.
    pub fn db_relation_toggle(&self, block: i32, record: i64, property: i32, target: i32) -> bool {
        let mut held: Vec<String> = self
            .db_relation_held(property, record)
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        let id = target.to_string();
        match held.iter().position(|h| *h == id) {
            Some(at) => {
                held.remove(at);
            }
            None => held.push(id),
        }
        self.db_pick_relation(block, record, property, &held)
    }

    /// What one column is called, as every editor's header and summary line
    /// says it. A negative id and an id that no longer resolves both answer
    /// "Not set": clearing a slot and never having filled it are one state
    /// (ADR-0062's rule about `Empty`, applied to a config key).
    pub fn db_property_label(&self, property: i32) -> String {
        let catalog = self.databases.borrow();
        catalog
            .properties
            .iter()
            .find(|p| p.id == PropertyId(property as u64))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Not set".into())
    }

    /// What one database is called, as the relation editor's header says it.
    /// `""` for the absence (`target < 0`) rather than a word for it: the
    /// popup draws its own "no database yet" sentence, and a column named
    /// "Not set" would read like a database someone really has.
    pub fn db_database_name(&self, target: i32) -> String {
        if target < 0 {
            return String::new();
        }
        let catalog = self.databases.borrow();
        catalog
            .database(DatabaseId(target as u64))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "a database that is gone".into())
    }

    /// The relation columns of this database a rollup may fold through
    /// (ADR-0089's first name). One with no target yet is greyed with the very
    /// refusal the save path would answer with.
    pub fn db_rollup_relations(&self, block: i32, chosen: i32) -> Vec<DbPickRow> {
        let Some(db) = self.db_ref_of(block) else {
            return Vec::new();
        };
        let catalog = self.databases.borrow();
        catalog
            .properties_of(db)
            .filter(|p| p.kind == PropertyKind::Relation)
            .map(|p| {
                let unready = database_relation::config_relation(&p.config).target.is_none();
                DbPickRow {
                    id: p.id.as_u64() as i32,
                    name: p.name.clone(),
                    chosen: chosen == p.id.as_u64() as i32,
                    disabled: unready,
                    note: if unready {
                        database_rollup::ConfigRefusal::NoTarget.message().to_string()
                    } else {
                        String::new()
                    },
                }
            })
            .collect()
    }

    /// The columns a rollup may aggregate: every column of the database
    /// `relation` points at, each judged by [`database_rollup::check_config`]
    /// itself. "Not set" is offered first because `count` reads no column at all
    /// — it folds the *number* of related records.
    pub fn db_rollup_columns(&self, relation: i32, chosen: i32) -> Vec<DbPickRow> {
        let mut rows = vec![DbPickRow {
            id: -1,
            name: "No column".into(),
            chosen: chosen < 0,
            disabled: false,
            note: "count needs no column".into(),
        }];
        let catalog = self.databases.borrow();
        let Some(rel) = catalog.properties.iter().find(|p| p.id == PropertyId(relation as u64))
        else {
            return rows;
        };
        let Some(target_db) = database_relation::config_relation(&rel.config).target else {
            return rows;
        };
        rows.extend(catalog.properties_of(target_db).map(|p| {
            let refusal = database_rollup::check_config(&database_rollup::RollupFacts {
                relation_kind: rel.kind,
                relation_target: Some(target_db),
                column_kind: p.kind,
                column_is_in_target: true,
            })
            .err();
            DbPickRow {
                id: p.id.as_u64() as i32,
                name: p.name.clone(),
                chosen: chosen == p.id.as_u64() as i32,
                disabled: refusal.is_some(),
                // A live row's caption is the kind's own word, so the chooser
                // also answers "what sort of column is this one".
                note: refusal
                    .map(|r| r.message().to_string())
                    .unwrap_or_else(|| p.kind.label().to_string()),
            }
        }));
        rows
    }

    /// The six folds, in `Aggregate::ALL`'s order — the index every rollup
    /// callback hands back.
    pub fn db_rollup_aggregates(&self, chosen: i32) -> Vec<DbPickRow> {
        Aggregate::ALL
            .iter()
            .enumerate()
            .map(|(index, aggregate)| DbPickRow {
                id: index as i32,
                name: aggregate.label().to_string(),
                chosen: chosen == index as i32,
                disabled: false,
                note: String::new(),
            })
            .collect()
    }

    /// A rollup column's three settings as words, in the order the editor
    /// lists them: the relation, the column, the fold.
    pub fn db_rollup_labels(&self, property: i32) -> (String, String, String) {
        let (relation, column, aggregate) = self.db_rollup_config(property);
        (
            self.db_property_label(relation),
            self.db_property_label(column),
            Aggregate::ALL
                .get(aggregate as usize)
                .map(|a| a.label().to_string())
                .unwrap_or_else(|| "Not set".into()),
        )
    }

    /// Delete one row: its values, the record, and the page it owns when it is
    /// page-backed — one `Entry`, one Ctrl+Z (ADR-0063).
    ///
    /// The values and the page row are read here because the plan layer has no
    /// SQL: two reads (one query for the values, one point read for the record)
    /// against an undo that puts back exactly what was there.
    pub fn db_delete_record(&self, block: i32, record: i64) -> bool {
        let Some(repo) = self.db_repo().cloned() else {
            return false;
        };
        let record_id = RecordId(record as u64);
        let Some(row) = repo.record(record_id).ok().flatten() else {
            return false;
        };
        let values = repo.record_values(record_id).unwrap_or_default();
        // The page row itself, not just its id: the undo has to write the title
        // and the parent back, and a page rebuilt from an id would be a page
        // with the wrong name.
        let page = row.page.and_then(|id| self.page_row(id));
        let cmd = Command::DeleteDatabaseRecord {
            block: BlockId(block as u64),
            record: row,
            values,
            page,
        };
        if self.exec_on_open_page(cmd).is_none() {
            return false;
        }
        self.db_refresh(block);
        // A deleted record may be a relation target, so every other database on
        // this page has to be able to move to the degradation word (ADR-0051).
        self.db_refresh_page(Some(block));
        true
    }

    /// One page row as `core::Page`, for the delete's undo. Built from the
    /// workspace (which holds the title, the parent and the appearance) and the
    /// page-order map (which holds the sibling order) — the two places a page's
    /// own facts live in this layer. `None` for a page the session does not have,
    /// which is a record pointing at a page it does not own.
    fn page_row(&self, id: PageId) -> Option<Page> {
        let ws = self.workspace.borrow();
        let int = id.as_u64() as i32;
        let page = ws.get(int)?;
        Some(Page {
            id,
            title: page.title.clone(),
            parent: page.parent.map(|p| PageId(p as u32 as u64)),
            order: *self.page_order.borrow().get(&int).unwrap_or(&OrderKey::FIRST),
            favorite: page.favorite,
            expanded: page.expanded,
            font: page.font,
            full_width: page.full_width,
            small_text: page.small_text,
            icon: page.icon.clone(),
            cover: page.cover,
            locked: page.locked,
            template: page.template,
        })
    }

    /// Set one column's width, in permille of the grid (ADR-0064's `widths`).
    /// Persisted as the view's whole document, because that is what a view's
    /// rules are: one JSON blob, replaced whole.
    pub fn db_set_column_width(&self, block: i32, property: i32, permille: i32) -> bool {
        let width = if permille <= 0 {
            WIDTH_AUTO
        } else {
            permille.min(u16::MAX as i32) as u16
        };
        self.db_edit_definition(block, |definition| {
            definition.set_width(PropertyId(property as u64), width)
        })
    }

    /// Hide or show one column. The title column cannot be hidden (ADR-0063: it
    /// is what a row is called), and the call is refused rather than silently
    /// ignored so a caller cannot believe it worked.
    pub fn db_toggle_column(&self, block: i32, property: i32) -> bool {
        let Some(db) = self.db_ref_of(block) else {
            return false;
        };
        let locked = self
            .databases
            .borrow()
            .properties_of(db)
            .any(|p| p.id == PropertyId(property as u64) && p.kind.is_title());
        if locked {
            return false;
        }
        let all: Vec<PropertyId> = self
            .databases
            .borrow()
            .properties_of(db)
            .map(|p| p.id)
            .collect();
        self.db_edit_definition(block, move |definition| {
            let id = PropertyId(property as u64);
            if definition.shows(id, &all) {
                definition.hide(id, &all);
            } else {
                definition.show(id, &all);
            }
        })
    }

    /// Apply one edit to the active view's document and store the result. The
    /// document is *read, edited and written back as text* — never re-serialised
    /// from the fields this build knows, which is what keeps a later build's
    /// `filter` and `sorts` alive through a width drag (ADR-0074).
    fn db_edit_definition(&self, block: i32, edit: impl FnOnce(&mut ViewDefinition)) -> bool {
        let (Some(db), Some(view)) = (self.db_ref_of(block), self.db_active_view(block)) else {
            return false;
        };
        let _ = db;
        let from = self
            .databases
            .borrow()
            .views
            .iter()
            .find(|v| v.id == view)
            .map(|v| v.definition.clone());
        let Some(from) = from else {
            return false;
        };
        let mut definition = ViewDefinition::parse(&from);
        edit(&mut definition);
        let to = definition.to_text();
        if to == from {
            return false;
        }
        // The catalog learns the new document from the change batch itself
        // (`db_absorb`), on the way through `record` — so the projection below
        // reads the new widths, and the *undo* of a drag reads the old ones.
        let cmd = Command::SetDatabaseViewDefinition {
            block: BlockId(block as u64),
            view,
            from,
            to,
        };
        if self.exec_editor(cmd).is_none() {
            return false;
        }
        self.db_refresh(block);
        true
    }

    /// Switch the block to another view of the same database. Session state
    /// (ADR-0073): the document is untouched, and the window is re-read because
    /// a different view has different columns.
    pub fn db_pick_view(&self, block: i32, view: i32) -> bool {
        let Some(db) = self.db_ref_of(block) else {
            return false;
        };
        let wanted = ViewId(view as u64);
        if !self.databases.borrow().views_of(db).any(|v| v.id == wanted) {
            return false;
        }
        self.db_active_view.borrow_mut().insert(block, wanted);
        // The cached window belongs to the old view: dropping it is what makes
        // the next projection read the new view's columns rather than reuse a
        // row set painted against the old ones.
        self.db_windows.borrow_mut().remove(&block);
        self.db_refresh(block);
        true
    }

    // ─── D4: the view's rules, written back into the document (ADR-0076) ────
    //
    // Every helper here is the same three moves: read the active view's
    // document as text, apply one edit to the **typed** rules, write the
    // document back through `db_edit_definition` — which is where the
    // `SetDatabaseViewDefinition` change comes from, so an undo restores the
    // whole document (the filter, the sorts, the group and the two D3 keys
    // together) and `db_absorb` teaches the catalog. What none of them does is
    // touch a row: the *next* window read compiles the new rules into SQL, and
    // the rows the user sees are the ones that query returns.

    /// Edit the active view's filter as the panel represents it: one
    /// `and`/`or` root over clauses, each optionally inverted. A stored tree
    /// the panel cannot represent (a group inside a group) is **refused** —
    /// with a notice, not a silent reshaping of rules the user wrote elsewhere
    /// — while the table keeps filtering by the tree it has, because the
    /// compiler reads the whole recursive shape and only the panel is flat.
    fn db_edit_filter(&self, block: i32, edit: impl FnOnce(&mut FlatFilter)) -> bool {
        let (Some(db), Some(view)) = (self.db_ref_of(block), self.db_active_view(block)) else {
            return false;
        };
        let tree = {
            let catalog = self.databases.borrow();
            let Some(row) = catalog.views.iter().find(|v| v.id == view) else {
                return false;
            };
            let rules = ViewDefinition::parse(&row.definition).rules(db, &catalog);
            rules.filter
        };
        let mut flat = match FlatFilter::from_tree(tree.as_ref()) {
            Some(flat) => flat,
            None => {
                self.set_db_notice(
                    "This view's filter uses nesting the filter panel does not edit yet.".into(),
                );
                return false;
            }
        };
        edit(&mut flat);
        let tree = flat.to_tree();
        self.db_edit_definition(block, |definition| definition.set_filter(Some(&tree)))
    }

    /// The kind of the clause the panel has open at `index` — what the value
    /// editors validate against (a number must parse, a date must be one of
    /// the two stored shapes).
    fn db_filter_clause_kind(&self, block: i32, index: usize) -> Option<PropertyKind> {
        let (db, view) = (self.db_ref_of(block)?, self.db_active_view(block)?);
        let rules = self.db_rules(db, view);
        let flat = FlatFilter::from_tree(rules.filter.as_ref())?;
        flat.clauses.get(index).map(|c| c.clause.kind)
    }

    /// Whether the active view's filter is one the panel may edit. The popup
    /// reads this on open; a `false` leaves the table filtering by a tree the
    /// panel declines to reshaping.
    pub fn db_filter_editable(&self, block: i32) -> bool {
        let (Some(db), Some(view)) = (self.db_ref_of(block), self.db_active_view(block)) else {
            return false;
        };
        let rules = self.db_rules(db, view);
        FlatFilter::from_tree(rules.filter.as_ref()).is_some()
    }

    /// Match all (`and`) or match any (`or`) — the root's flavour.
    pub fn db_filter_set_match(&self, block: i32, any: bool) -> bool {
        self.db_edit_filter(block, |flat| flat.any = any)
    }

    /// Add one rule for `property`: the kind's first comparison, no value yet.
    /// The clause persists as `value: null` — [`FilterValue::Missing`], which
    /// compiles to no constraint — so "add a rule" never hides rows before the
    /// user has said what the rule is, and a half-written rule survives a
    /// restart as exactly what it is.
    pub fn db_filter_add_clause(&self, block: i32, property: i32) -> bool {
        let Some(kind) = self.db_property_kind(property) else {
            return false;
        };
        // A kind with no comparisons (formula / rollup / relation) has no rule
        // to add: refused here, where the picker should not have offered it.
        let Some(op) = FilterOp::ops_for(kind).first().copied() else {
            return false;
        };
        self.db_edit_filter(block, |flat| {
            flat.clauses.push(FlatClause {
                clause: FilterClause {
                    property: PropertyId(property as u64),
                    kind,
                    op,
                    value: FilterValue::Missing,
                },
                invert: false,
            });
        })
    }

    pub fn db_filter_remove_clause(&self, block: i32, index: usize) -> bool {
        self.db_edit_filter(block, |flat| {
            if index < flat.clauses.len() {
                flat.clauses.remove(index);
            }
        })
    }

    /// Switch a clause's comparison. The value resets: the stored shape of an
    /// `is` (an option id) is not the shape of `contains` (a substring), and a
    /// value carried across a comparison change would be a value the new
    /// comparison never asked for.
    pub fn db_filter_set_op(&self, block: i32, index: usize, op_index: usize) -> bool {
        let Some(op) = FilterOp::from_index(op_index) else {
            return false;
        };
        // The kind's own admissible set, not the global list: an op the parser
        // would drop on read-back is a rule that silently disappears, and the
        // panel's menu is built from this same list — so a row that went stale
        // under the menu says so instead of eating the clause.
        let Some(kind) = self.db_filter_clause_kind(block, index) else {
            return false;
        };
        if !FilterOp::ops_for(kind).contains(&op) {
            return false;
        }
        self.db_edit_filter(block, |flat| {
            if let Some(clause) = flat.clauses.get_mut(index) {
                clause.clause.op = op;
                clause.clause.value = FilterValue::Missing;
            }
        })
    }

    /// `¬` on one clause — a `not` around it, which is how "is not empty" and
    /// "does not contain" are built from the same comparisons.
    pub fn db_filter_toggle_not(&self, block: i32, index: usize) -> bool {
        self.db_edit_filter(block, |flat| {
            if let Some(clause) = flat.clauses.get_mut(index) {
                clause.invert = !clause.invert;
            }
        })
    }

    /// Set a clause's value from the text box, **validated by kind**: a number
    /// must parse (and be finite — a filter value of `NaN` compares as false
    /// against everything and would look like a broken rule), a date must be
    /// one of ADR-0062's two stored shapes, everything text-shaped is taken
    /// verbatim (ADR-0069: the three string kinds are never rewritten). A
    /// refused value returns `false` and the panel keeps the text; nothing is
    /// written and nothing is silently reworded.
    pub fn db_filter_set_text(&self, block: i32, index: usize, text: &str) -> bool {
        let Some(kind) = self.db_filter_clause_kind(block, index) else {
            return false;
        };
        let value = match kind {
            PropertyKind::Number => match text.trim().parse::<f64>() {
                Ok(num) if num.is_finite() => FilterValue::Number(num),
                _ => return false,
            },
            PropertyKind::Date | PropertyKind::CreatedTime | PropertyKind::LastEditedTime => {
                if !is_stored_date(text) {
                    return false;
                }
                FilterValue::Text(text.to_string())
            }
            _ => FilterValue::Text(text.to_string()),
        };
        self.db_edit_filter(block, move |flat| {
            if let Some(clause) = flat.clauses.get_mut(index) {
                clause.clause.value = value;
            }
        })
    }

    /// A checkbox clause's value: the pick button is the whole editor.
    pub fn db_filter_set_flag(&self, block: i32, index: usize, checked: bool) -> bool {
        self.db_edit_filter(block, |flat| {
            if let Some(clause) = flat.clauses.get_mut(index) {
                clause.clause.value = FilterValue::Flag(checked);
            }
        })
    }

    /// A pick (select / status) or list (multi-select / files) clause's value.
    /// Under `is` the pick writes the option **id** (ADR-0061 stores ids, not
    /// labels) and picking the one already held clears the rule back to
    /// unfilled; under `is any of` / `has any of` the pick toggles membership.
    pub fn db_filter_toggle_option(&self, block: i32, index: usize, option: &str) -> bool {
        self.db_edit_filter(block, |flat| {
            let Some(clause) = flat.clauses.get_mut(index) else {
                return;
            };
            match clause.clause.op {
                FilterOp::AnyOf => {
                    let mut items = match &clause.clause.value {
                        FilterValue::Any(items) => items.clone(),
                        FilterValue::Text(one) => vec![one.clone()],
                        _ => Vec::new(),
                    };
                    match items.iter().position(|item| item == option) {
                        Some(at) => {
                            items.remove(at);
                        }
                        None => items.push(option.to_string()),
                    }
                    clause.clause.value = if items.is_empty() {
                        FilterValue::Missing
                    } else {
                        FilterValue::Any(items)
                    };
                }
                _ => {
                    clause.clause.value = match &clause.clause.value {
                        FilterValue::Text(current) if current == option => FilterValue::Missing,
                        _ => FilterValue::Text(option.to_string()),
                    };
                }
            }
        })
    }

    /// Delete every rule. The document keeps the `filter` key as an empty
    /// group — "I own this key and it is empty" (ADR-0074's widths argument,
    /// applied to the key this slice owns) — which the parser reads back as no
    /// filter at all.
    pub fn db_filter_clear(&self, block: i32) -> bool {
        self.db_edit_filter(block, |flat| flat.clauses.clear())
    }

    /// Cycle one column's sort from the column header: none → ascending →
    /// descending → none; a different column starts at ascending. The header
    /// edits the **first** term — the one that decides the order's head. A
    /// document may hold more terms, which the read path sorts by (every term
    /// is in the `ORDER BY`) and this one control leaves alone: multi-key
    /// editing waits for a panel of its own, and the SQL side is already
    /// general. A kind with no order (ADR-0070's table) refuses quietly — the
    /// header is also the resize handle's row, so a click that does nothing
    /// must not be a click that lied.
    pub fn db_sort_cycle(&self, block: i32, property: i32) -> bool {
        let (Some(db), Some(view)) = (self.db_ref_of(block), self.db_active_view(block)) else {
            return false;
        };
        let rules = self.db_rules(db, view);
        let id = PropertyId(property as u64);
        let Some(row) = self
            .databases
            .borrow()
            .properties_of(db)
            .find(|p| p.id == id)
            .cloned()
        else {
            return false;
        };
        let descending = match rules.sorts.first() {
            // Same column: cycle the direction, clearing at the end.
            Some(first) if first.property == id => {
                if first.descending {
                    None
                } else {
                    Some(true)
                }
            }
            // Another column (or no sort): start at ascending.
            _ => Some(false),
        };
        let sorts: Vec<SortSpec> = match descending {
            None => Vec::new(),
            Some(descending) => match SortSpec::of(&row, descending) {
                Some(spec) => vec![spec],
                None => return false,
            },
        };
        self.db_edit_definition(block, move |definition| definition.set_sorts(&sorts))
    }

    /// Pick the column a view groups by — `-1` clears the grouping. Only the
    /// option-bounded kinds group (ADR-0076: the list of headers has to be
    /// small enough to compute in full, and a text column's distinct values
    /// are exactly what would make it one row per group); the popup offers
    /// only those, and a caller that insists on another is refused rather
    /// than silently ungrouped.
    pub fn db_group_pick(&self, block: i32, property: i32) -> bool {
        let group = if property >= 0 {
            let Some(kind) = self.db_property_kind(property) else {
                return false;
            };
            if !GroupSpec::admits(kind) {
                return false;
            }
            Some(PropertyId(property as u64))
        } else {
            None
        };
        self.db_edit_definition(block, |definition| definition.set_group(group))
    }

    /// The filter panel's rows: the active filter as the flat panel draws it.
    /// `None` (a block with no entity, or a tree the panel cannot represent)
    /// means an empty panel — the table still filters by the tree it has, and
    /// an edit attempt says why nothing happened.
    pub fn db_filter_panel(&self, block: i32) -> (bool, Vec<DbFilterPanelRow>) {
        let (Some(db), Some(view)) = (self.db_ref_of(block), self.db_active_view(block)) else {
            return (false, Vec::new());
        };
        let rules = self.db_rules(db, view);
        let Some(flat) = FlatFilter::from_tree(rules.filter.as_ref()) else {
            return (false, Vec::new());
        };
        let rows = flat
            .clauses
            .iter()
            .map(|flat_clause| {
                let clause = &flat_clause.clause;
                // The value as the panel shows it: a pick's option id is named
                // by the column's config (an id the config forgot names
                // itself, ADR-0069's fold); anything else displays as stored.
                let value = match &clause.value {
                    FilterValue::Text(id)
                        if matches!(clause.kind, PropertyKind::Select | PropertyKind::Status) =>
                    {
                        let config = self.db_property_config(clause.property.as_u64() as i32);
                        PropertyOptions::from_config(&config)
                            .iter()
                            .find(|o| o.id.as_u64().to_string() == *id)
                            .map(|o| o.name.clone())
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| id.clone())
                    }
                    other => other.display(),
                };
                let name = self
                    .databases
                    .borrow()
                    .properties
                    .iter()
                    .find(|p| p.id == clause.property)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                DbFilterPanelRow {
                    property: clause.property.as_u64() as i32,
                    name,
                    kind: property_kind_int(clause.kind),
                    op: clause.op.index() as i32,
                    op_name: clause.op.label(clause.kind).to_string(),
                    value,
                    has_value: clause.value.is_set(),
                    invert: flat_clause.invert,
                }
            })
            .collect();
        (flat.any, rows)
    }

    /// The comparisons one kind's panel may offer, as (op index, word) — the
    /// same list the parser admissibility-checks against, so the menu can
    /// never offer a comparison the compiler would refuse.
    pub fn db_ops_for_kind(&self, kind: i32) -> Vec<(i32, String)> {
        crate::core::database::PropertyKind::ALL
            .get(kind as usize)
            .map(|kind| {
                FilterOp::ops_for(*kind)
                    .iter()
                    .map(|op| (op.index() as i32, op.label(*kind).to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A column's options, as the pick-value editor offers them — (id, name,
    /// color), read from the config JSON once per open of the picker.
    pub fn db_property_options(&self, property: i32) -> Vec<(String, String, String)> {
        let config = self.db_property_config(property);
        PropertyOptions::from_config(&config)
            .iter()
            .map(|o| {
                (
                    o.id.as_u64().to_string(),
                    o.name.clone(),
                    o.color.clone(),
                )
            })
            .collect()
    }

    /// Which column the active view groups by, or `-1` — what the picker's
    /// rows (and its "No group" row) mark themselves against.
    pub fn db_group_current(&self, block: i32) -> i32 {
        let (Some(db), Some(view)) = (self.db_ref_of(block), self.db_active_view(block)) else {
            return -1;
        };
        self.db_rules(db, view)
            .group
            .map(|spec| spec.property.as_u64() as i32)
            .unwrap_or(-1)
    }

    /// The columns a view may group by — the option-bounded kinds only
    /// (ADR-0076), in schema order. Empty when nothing groups, which is what
    /// the popup says instead of offering a grouping that cannot be computed.
    pub fn db_group_choices(&self, block: i32) -> Vec<DbColumnToggle> {
        let Some(db) = self.db_ref_of(block) else {
            return Vec::new();
        };
        self.databases
            .borrow()
            .properties_of(db)
            .filter(|p| GroupSpec::admits(p.kind))
            .map(|p| DbColumnToggle {
                property: p.id.as_u64() as i32,
                name: p.name.clone(),
                kind: p.kind.as_str().to_string(),
                visible: true,
                locked: false,
            })
            .collect()
    }

    fn db_property_kind(&self, property: i32) -> Option<PropertyKind> {
        self.databases
            .borrow()
            .properties
            .iter()
            .find(|p| p.id == PropertyId(property as u64))
            .map(|p| p.kind)
    }

    fn db_property_config(&self, property: i32) -> String {
        self.databases
            .borrow()
            .properties
            .iter()
            .find(|p| p.id == PropertyId(property as u64))
            .map(|p| p.config.clone())
            .unwrap_or_default()
    }

    /// The Markdown export's version of one database block (ADR-0065): the
    /// header row and every row the view shows, as display strings, rendered
    /// **here** because this is the layer that can read records.
    ///
    /// The rows are the *whole* table, not a window: a file has no viewport, so
    /// the control read D1 measured (`unwindowed_rows`) is the honest one — and
    /// the price is written down in ADR-0065's boundary and again in D8's
    /// list: a ten-thousand-row database exports in one `Vec`, which is why the
    /// streaming read is D8's收口 and not this slice's.
    pub fn db_markdown_table(&self, block: i32) -> Option<crate::services::export_service::DatabaseTable> {
        // A file says what the user is *looking at*, so the write queue is
        // settled first — the same rule the window read follows (`db_refresh`).
        // A cell typed in the last debounce window must not be missing from
        // the exported table.
        if let Some(persistence) = &self.persistence {
            let _ = persistence.force_flush();
        }
        let db = self.db_ref_of(block)?;
        let view = self.db_active_view(block)?;
        let (properties, columns, rules) = {
            let catalog = self.databases.borrow();
            let row = catalog.views.iter().find(|v| v.id == view)?;
            let properties = view_columns(&catalog, db, row);
            let definition = ViewDefinition::parse(&row.definition);
            let rules = definition.rules(db, &catalog);
            (properties.clone(), table_columns(&properties, &definition), rules)
        };
        let title = properties.iter().find(|p| p.kind.is_title())?.id;
        let repo = self.db_repo()?.clone();
        // The export says what the view shows (ADR-0065: "按视图的顺序与成员，
        // 即过滤排序照做") — so the request carries the view's rules and the
        // control read runs the same statement the window read does, minus the
        // window. The group is *screen* furniture and a file has no screen: a
        // grouped view exports its rows in the view's order, without headers.
        // `search` stays `None` on purpose (ADR-0087): the search is session
        // state — a question being asked of the screen — and the file the user
        // asked for is the *view*, whose rules are the document's. A view that
        // looks filtered while searched exports its whole (filtered) row set,
        // the same way a board's columns are screen furniture the file does
        // not have.
        let request = RowRequest {
            db,
            title,
            columns: &properties,
            sorts: &rules.sorts,
            filter: rules.filter.as_ref(),
            search: None,
        };
        let rows = repo.unwindowed_rows(&request).ok()?;
        let pages = repo.record_pages(db).unwrap_or_default();
        // D6: the formula columns are computed here too — a file shows the same
        // values the screen does. The export renders the *whole* view (ADR-0065's
        // boundary), so a computed column is computed for the whole view: that
        // is an artifact's own cost, and the input path's red line (「禁止每次输
        // 入全库重算」) is about editing, not about the file the user explicitly
        // asked to produce. The dependencies' values come from **one indexed
        // sweep per column** (`column_values`, the same trade `unwindowed_rows`
        // makes) rather than one point read per row, and the sweep is expanded
        // transitively: a formula that names another formula column needs *its*
        // dependencies' stored values too.
        let mut preload: HashMap<u64, HashMap<u64, CellValue>> = HashMap::new();
        if columns.iter().any(|c| c.kind == PropertyKind::Formula) {
            let catalog = self.databases.borrow();
            let map = self.db_formula_map(db, &catalog);
            let mut seen: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
            let mut stack: Vec<u64> = Vec::new();
            for column in columns.iter().filter(|c| c.kind == PropertyKind::Formula) {
                if let Some(Some(program)) = map.get(&column.property.as_u64()) {
                    stack.extend(program.deps().iter().copied());
                }
            }
            while let Some(id) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                match map.get(&id).and_then(|program| program.as_ref()) {
                    // Another formula column: its own dependencies are wanted
                    // too (the eval walks it through the source's programs).
                    Some(program) => stack.extend(program.deps().iter().copied()),
                    // A stored column: sweep it once.
                    None => {
                        if let Ok(values) = repo.column_values(db, PropertyId(id)) {
                            preload.insert(id, values);
                        }
                    }
                }
            }
        }
        // The painted rows: `table_rows` + the formula pass, so an exported
        // formula cell and an on-screen one cannot disagree about their value.
        let painted = self.db_table_rows(db, &rows, &columns, &pages, Some(&preload));
        let mut header: Vec<String> = vec![properties
            .iter()
            .find(|p| p.kind.is_title())
            .map(|p| p.name.clone())
            .unwrap_or_default()];
        header.extend(columns.iter().skip(1).map(|c| c.name.clone()));
        let mut out = Vec::with_capacity(painted.len());
        for row in &painted {
            // ADR-0065's one link: a page-backed row writes its title as a
            // `quire://page/<id>` address (ADR-0026's shape for a Page block), so
            // the file keeps the only durable handle a reader has on that page; a
            // bare record's title stays plain text.
            let mut line = Vec::with_capacity(header.len());
            line.push(match pages.get(&row.record) {
                Some(page) => format!("[{}](quire://page/{})", row.title, page.as_u64()),
                None => row.title.clone(),
            });
            // The title column is the first cell of the row and the first entry
            // of the header, written once above; the rest follow in view order —
            // now as painted (and, for a formula column, computed) strings.
            line.extend(row.cells.iter().skip(1).map(|cell| cell.painted.clone()));
            out.push(line);
        }
        Some(crate::services::export_service::DatabaseTable {
            header,
            rows: out,
        })
    }

    // ─── D6: computed properties (SPEC §三十九 「需计算」, ADR-0082/0083) ────
    //
    // A formula column's cell is **computed at projection time** and never
    // stored (ADR-0062's 「不存值，投影时现算」; ADR-0039's discipline). Three
    // contracts live in this section, and each is a shape rather than a rule:
    //
    // 1. **The recompute unit is the window, never the table** (SPEC's red
    //    line 「禁止每次输入全库重算」). Formulas are evaluated in
    //    `db_paint_formulas`, over exactly the rows a view realized — the same
    //    31-or-so rows every other column paints for. There is no path that
    //    walks all of `db_records` to evaluate anything, and the two knobs that
    //    would force one — sorting and filtering by a formula column — are
    //    refused upstream (`PropertyKind::sort_column` is `None`,
    //    `FilterOp::ops_for` is empty for the computed kinds), because "sort by
    //    a computed value" in SQL means "compute it for every row first".
    // 2. **Dependencies are same-row, by construction.** `FormulaSource` is
    //    built for one record; the engine's cell callback has no record
    //    parameter. Editing cell (r, q) can therefore change formula values
    //    only on row r, and only in columns whose parsed dependencies
    //    (transitively, through other formula columns) name q — the dependency
    //    set the save-time cycle check and the export's preload both walk.
    //    The *evaluation count* for that edit is still "the window's formula
    //    cells" (the model is rebuilt on refresh, like every column's paint);
    //    what the dependency precision buys is the *guarantee about which
    //    values can differ*, and `db_computed_evals` is the counter the unified
    //    test measures it against — evaluations grow with the window, never
    //    with `COUNT(*)`.
    // 3. **Errors paint, they do not fail the frame.** A formula that cannot
    //    evaluate on one row (a type error, a division by zero, a reference
    //    chain gone deep) paints `Error` in that one cell; the editor's
    //    preview carries the sentence. A blank would read as "no value", which
    //    is a different fact.

    /// `table_rows` plus the computed pass — the one realize shape every layout
    /// branch uses (table, list, board cards, calendar peeks, gallery, timeline
    /// lanes), so a computed column cannot be painted in one layout and blank
    /// in another. `preload` is the export's per-column sweep; the window path
    /// passes `None` and point-reads.
    fn db_table_rows(
        &self,
        db: DatabaseId,
        rows: &[crate::core::database::RowView],
        columns: &[TableColumn],
        pages: &crate::core::database_view::RecordPages,
        preload: Option<&HashMap<u64, HashMap<u64, CellValue>>>,
    ) -> Vec<TableRowView> {
        let mut view = table_rows(rows, columns, pages);
        self.db_paint_computed(db, columns, &mut view, preload);
        view
    }

    /// The computed columns' one entry point (ADR-0082/0088/0089): formulas,
    /// then the live titles a relation cell paints, then rollups.
    ///
    /// Three passes rather than one because they read three different things —
    /// a row's stored cells, a set of other records by id, and one column of
    /// those records — and one entry point rather than three call sites so no
    /// layout can paint one computed kind in an arm and blank another. Each pass
    /// is a no-op when its kind is not visible, so a database with no computed
    /// column pays three folds over the column list.
    fn db_paint_computed(
        &self,
        db: DatabaseId,
        columns: &[TableColumn],
        rows: &mut [TableRowView],
        preload: Option<&HashMap<u64, HashMap<u64, CellValue>>>,
    ) {
        if rows.is_empty() {
            return;
        }
        self.db_paint_formulas(db, columns, rows, preload);
        self.db_paint_relations(columns, rows);
        self.db_paint_rollups(columns, rows);
    }

    /// Paint every visible **relation** cell with the live titles of its targets
    /// (ADR-0088). The stored value is a list of record ids; what a reader sees
    /// is what those records are called *now* — which is exactly why this is a
    /// projection pass and not a stored string: renaming a target moves every
    /// cell that names it without a single write, and there is no second copy
    /// that could fall out of step.
    ///
    /// **Two batched reads, however many cells are on screen**: one for the
    /// window's target ids (the same `db_value_items` query the row read runs
    /// for its list columns) and one for the union of the records those ids
    /// name. A window of thirty rows whose relation columns point at a hundred
    /// records costs two statements, not a hundred — and the target *database*
    /// is never walked, for the reason ADR-0067 gives about the row table.
    fn db_paint_relations(&self, columns: &[TableColumn], rows: &mut [TableRowView]) {
        let Some(repo) = self.db_repo() else {
            return;
        };
        let visible: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, column)| column.kind == PropertyKind::Relation)
            .map(|(at, _)| at)
            .collect();
        if visible.is_empty() {
            return;
        }
        let records: Vec<RecordId> = rows.iter().map(|row| RecordId(row.record)).collect();
        let mut items_of: Vec<(usize, HashMap<u64, Vec<String>>)> = Vec::new();
        let mut wanted: Vec<u64> = Vec::new();
        for at in visible {
            // A read that fails paints nothing for this column rather than
            // failing the frame: an unwritable cell is a blank one, and the
            // alternative is a table that refuses to draw because one relation
            // column could not be resolved.
            let Ok(items) = repo.cell_items(&records, columns[at].property) else {
                continue;
            };
            for targets in items.values() {
                wanted.extend(targets.iter().filter_map(|t| t.parse::<u64>().ok()));
            }
            items_of.push((at, items));
        }
        if items_of.is_empty() {
            return;
        }
        // One lookup for the union: a record two rows both point at is named
        // once, and the ids are sorted so the query's plan is the same shape
        // every refresh.
        wanted.sort_unstable();
        wanted.dedup();
        let targets: Vec<RecordId> = wanted.iter().copied().map(RecordId).collect();
        let names = repo.record_titles(&targets).unwrap_or_default();
        for row in rows.iter_mut() {
            for (at, items) in &items_of {
                let Some(targets) = items.get(&row.record) else {
                    continue;
                };
                let painted = database_relation::paint_targets(targets, |target| {
                    target
                        .parse::<u64>()
                        .ok()
                        .and_then(|id| names.get(&id).cloned())
                });
                if let Some(cell) = row.cells.get_mut(*at) {
                    cell.painted = painted;
                }
            }
        }
    }

    /// Paint every visible **rollup** cell (ADR-0089): fold one column of the
    /// records the row's relation points at, with the aggregate the column's
    /// config names.
    ///
    /// Three batched reads per rollup column per refresh — the relation's
    /// targets, the target column's values, and nothing else. The work is
    /// bounded by the window's **fan-out** (how many records its rows point at)
    /// and never by the target table's size: no path walks the related
    /// database, and no value is cached across refreshes (ADR-0083's decision,
    /// whose reasons are undo, redo and the bulk paths).
    ///
    /// A fold that cannot fold — a text column summed, an id that names no
    /// record — paints [`FORMULA_ERROR_PAINT`], the same word a formula's
    /// failure paints, because to a reader they are the same event.
    fn db_paint_rollups(&self, columns: &[TableColumn], rows: &mut [TableRowView]) {
        let Some(repo) = self.db_repo() else {
            return;
        };
        let catalog = self.databases.borrow();
        let plans: Vec<(usize, RollupPlan)> = columns
            .iter()
            .enumerate()
            .filter(|(_, column)| column.kind == PropertyKind::Rollup)
            .filter_map(|(at, column)| {
                let config = database_rollup::config_rollup(
                    &catalog
                        .properties
                        .iter()
                        .find(|p| p.id == column.property)?
                        .config,
                );
                let (relation, aggregate) = (config.relation?, config.aggregate);
                if !config.is_configured() {
                    return None;
                }
                // The fold's second input is a column of the *related*
                // database, so its kind is a second lookup: an unreadable one
                // (a hand-edited config naming a column that is gone, or one
                // that is computed) paints nothing rather than recursing.
                let target_kind = config
                    .column
                    .and_then(|column| catalog.properties.iter().find(|p| p.id == column))
                    .map(|property| property.kind);
                if aggregate.reads_column() && target_kind.is_none() {
                    return None;
                }
                Some((
                    at,
                    RollupPlan {
                        relation,
                        column: config.column,
                        target_kind: target_kind.unwrap_or(PropertyKind::Text),
                        aggregate,
                    },
                ))
            })
            .collect();
        if plans.is_empty() {
            return;
        }
        let records: Vec<RecordId> = rows.iter().map(|row| RecordId(row.record)).collect();
        for (at, plan) in plans {
            let Ok(items) = repo.cell_items(&records, plan.relation) else {
                continue;
            };
            // The values of the target column for every record the window's
            // relations name — one read, and an empty one for a fold that reads
            // no column (`count` still counts the *targets*, which is the
            // number it is asked for).
            let values = match plan.column {
                Some(property) if plan.aggregate.reads_column() => {
                    let mut targets: Vec<u64> = items
                        .values()
                        .flat_map(|list| list.iter().filter_map(|t| t.parse::<u64>().ok()))
                        .collect();
                    targets.sort_unstable();
                    targets.dedup();
                    let ids: Vec<RecordId> = targets.into_iter().map(RecordId).collect();
                    repo.values_of(&ids, property).unwrap_or_default()
                }
                _ => HashMap::new(),
            };
            for row in rows.iter_mut() {
                let targets = items.get(&row.record);
                let related = targets.map(|list| list.len()).unwrap_or(0);
                let folded: Vec<Val> = match (targets, plan.column) {
                    (Some(list), Some(_)) if plan.aggregate.reads_column() => list
                        .iter()
                        .filter_map(|target| {
                            let id = target.parse::<u64>().ok()?;
                            let value = values.get(&id)?;
                            Some(database_formula::val_of(plan.target_kind, value))
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let painted = match database_rollup::fold(plan.aggregate, &folded, related) {
                    Ok(value) => database_rollup::paint(&value),
                    Err(_) => FORMULA_ERROR_PAINT.to_string(),
                };
                // The counter ADR-0083/ADR-0089's contract is measured in: one
                // bump per (row × visible configured rollup column), the same
                // unit the formula loop below counts in.
                self.db_computed_evals.set(self.db_computed_evals.get() + 1);
                if let Some(cell) = row.cells.get_mut(at) {
                    cell.painted = painted;
                }
            }
        }
    }

    /// Compute and paint every **visible formula column** of `rows` — the one
    /// place a projection evaluates formulas (contract 1 above). A database
    /// with no visible formula column pays a config scan and nothing else; the
    /// parse happens once per column per refresh, not once per row.
    fn db_paint_formulas(
        &self,
        db: DatabaseId,
        columns: &[TableColumn],
        rows: &mut [TableRowView],
        preload: Option<&HashMap<u64, HashMap<u64, CellValue>>>,
    ) {
        if rows.is_empty() {
            return;
        }
        let catalog = self.databases.borrow();
        let resolve = |name: &str| {
            catalog
                .properties_of(db)
                .find(|p| p.name == name)
                .map(|p| p.id)
        };
        // The visible formula columns that have a parseable expression. A
        // formula column with no expression (or an unparsable one — a config
        // this build's parse refuses) paints nothing, the same fold every
        // unreadable setting gets: an error is for values, not for absence.
        let mut formulas: Vec<(usize, Program)> = Vec::new();
        for (at, column) in columns.iter().enumerate() {
            if column.kind != PropertyKind::Formula {
                continue;
            }
            let Some(source) = catalog
                .properties
                .iter()
                .find(|p| p.id == column.property)
                .and_then(|p| database_formula::config_formula(&p.config))
            else {
                continue;
            };
            if let Ok(program) = Program::parse(&source, resolve) {
                formulas.push((at, program));
            }
        }
        if formulas.is_empty() {
            return;
        }
        let Some(repo) = self.db_repo() else {
            return;
        };
        for row in rows.iter_mut() {
            let source = FormulaSource::new(repo, row.record, db, &catalog, preload);
            for (at, program) in &formulas {
                let painted = match program.eval(&mut |id, depth| source.value(id, depth)) {
                    Ok(value) => database_formula::display(&value),
                    Err(_) => FORMULA_ERROR_PAINT.to_string(),
                };
                // The counter ADR-0083's contract is measured in: one bump per
                // (row × visible formula column) evaluation. After editing one
                // cell it grows by the *window's* formula cells — never by the
                // table's rows (see the section comment, contract 1).
                self.db_computed_evals.set(self.db_computed_evals.get() + 1);
                if let Some(cell) = row.cells.get_mut(*at) {
                    cell.painted = painted;
                }
            }
        }
    }

    /// Every formula column of one database, parsed once: the map the save-time
    /// cycle check walks (`would_cycle`'s `deps_of`) and the export's dependency
    /// expansion reads. A column with no readable expression maps to `None` —
    /// a leaf, not an error: references to it read as empty at eval time and it
    /// cannot close a loop it does not traverse.
    fn db_formula_map(
        &self,
        db: DatabaseId,
        catalog: &DatabaseCatalog,
    ) -> HashMap<u64, Option<Program>> {
        let resolve = |name: &str| {
            catalog
                .properties_of(db)
                .find(|p| p.name == name)
                .map(|p| p.id)
        };
        catalog
            .properties_of(db)
            .filter(|p| p.kind == PropertyKind::Formula)
            .map(|p| {
                let program = database_formula::config_formula(&p.config)
                    .and_then(|source| Program::parse(&source, resolve).ok());
                (p.id.as_u64(), program)
            })
            .collect()
    }

    /// The expression a formula column holds, as the editor's starting text —
    /// the stored shape, never a painted one (the same rule `db_cell_text`
    /// states for cells). An absent expression is an empty draft.
    pub fn db_formula_current(&self, property: i32) -> String {
        let config = self.db_property_config(property);
        database_formula::config_formula(&config).unwrap_or_default()
    }

    /// The editor's live answer to the draft `text`: `(preview, error)`. The
    /// preview is the value the formula computes on the **sample row** (the
    /// `record` the caller read back from the UIState mirror — the row the user
    /// clicked, or "an empty row" when the table has none, which is the honest
    /// answer to "what would this formula show"). A parse failure blocks nothing
    /// here — the user is still typing —
    /// but its message is what the error line shows, and it is what a save
    /// refuses on.
    ///
    /// One evaluation per call, against one row: the preview is not a
    /// projection and does not touch `db_computed_evals` (the section comment's
    /// contract 1 measures projections).
    pub fn db_formula_preview(
        &self,
        record: i64,
        property: i32,
        text: &str,
    ) -> (String, String) {
        if text.trim().is_empty() {
            // An empty draft is a column with no expression: nothing computes,
            // nothing errors, every cell goes blank on save.
            return (String::new(), String::new());
        }
        let Some(property_row) = self
            .databases
            .borrow()
            .properties
            .iter()
            .find(|p| p.id == PropertyId(property as u64))
            .cloned()
        else {
            return (String::new(), String::new());
        };
        let (db, catalog) = (property_row.db, self.databases.borrow());
        let resolve = |name: &str| {
            catalog
                .properties_of(db)
                .find(|p| p.name == name)
                .map(|p| p.id)
        };
        let program = match Program::parse(text, resolve) {
            Ok(program) => program,
            Err(e) => return (String::new(), e.message().to_string()),
        };
        // The sample row's source — a record of `-1` reads every cell as
        // absent, which is the empty-row preview the doc comment promised.
        let Some(repo) = self.db_repo() else {
            return (String::new(), String::new());
        };
        let source = FormulaSource::new(repo, record as u64, db, &catalog, None);
        match program.eval(&mut |id, depth| source.value(id, depth)) {
            Ok(value) => (database_formula::display(&value), String::new()),
            Err(e) => (String::new(), e.message().to_string()),
        }
    }

    /// Commit the editor's draft as the column's expression: **the save-time
    /// checks** (ADR-0082 — SPEC's 「保存时做」) run here, before any change is
    /// recorded, and a refusal keeps the old expression and says why in the
    /// notice line. One accepted save is one `SetDatabaseFormula` command —
    /// one change, one Ctrl+Z — and the window is re-read because every cell
    /// of the column may now paint differently.
    ///
    /// The checks, in the order a user meets them:
    /// 1. *the column is a formula column* — nothing else has an expression;
    /// 2. *the expression parses and every `[Column]` name exists* (a typo is
    ///    refused here, not blanked on screen);
    /// 3. *the dependency graph stays acyclic* — the draft may name other
    ///    formula columns, and a chain that comes back to this column has no
    ///    value to compute; refused now, while the text that would create it
    ///    is on screen (`would_cycle`), with the eval-time depth cap kept only
    ///    for documents that never went through this door;
    /// 4. *an empty draft clears the column* — the config key is removed, not
    ///    stored empty, which is ADR-0062's one representation of "nothing".
    pub fn db_formula_accept(&self, block: i32, property: i32, text: &str) -> bool {
        let Some(row) = self
            .databases
            .borrow()
            .properties
            .iter()
            .find(|p| p.id == PropertyId(property as u64))
            .cloned()
        else {
            return false;
        };
        if row.kind != PropertyKind::Formula {
            return false;
        }
        let db = row.db;
        let from_config = row.config.clone();
        let to_config;
        if text.trim().is_empty() {
            to_config = database_formula::config_set_formula(&from_config, "");
        } else {
            let catalog = self.databases.borrow();
            let resolve = |name: &str| {
                catalog
                    .properties_of(db)
                    .find(|p| p.name == name)
                    .map(|p| p.id)
            };
            let program = match Program::parse(text, resolve) {
                Ok(program) => program,
                Err(e) => {
                    self.set_db_notice(e.message().to_string());
                    return false;
                }
            };
            // The cycle check: the draft's direct dependencies, expanded over
            // the formula columns this database already has.
            let map = self.db_formula_map(db, &catalog);
            let deps_of = |id: PropertyId| -> Option<std::collections::BTreeSet<u64>> {
                map.get(&id.as_u64())
                    .and_then(|program| program.as_ref())
                    .map(|program| program.deps().clone())
            };
            if database_formula::would_cycle(PropertyId(property as u64), program.deps(), deps_of) {
                self.set_db_notice(
                    "This formula would depend on itself, so it has no value to compute.".into(),
                );
                return false;
            }
            drop(catalog);
            to_config = database_formula::config_set_formula(&from_config, text);
        }
        if to_config == from_config {
            return false;
        }
        let cmd = Command::SetDatabaseFormula {
            block: BlockId(block as u64),
            property: PropertyId(property as u64),
            from: from_config,
            to: to_config,
        };
        if self.exec_editor(cmd).is_none() {
            return false;
        }
        self.db_refresh(block);
        true
    }

    // ─── D5: the view family (SPEC §三十九 「视图」) ─────────────────────────
    //
    // One new command (`AddDatabaseView`), one lazy-page path (`db_open_record`,
    // ADR-0063's 「打开 record 时建页」 finally getting its UI trigger), and the
    // three session dials the layouts read. Every view's *rules* still go
    // through `db_edit_definition` — a board's grouping **is** D4's `groups`
    // key, a calendar's and a timeline's time axis is the one new document key
    // (`date`/`end`, ADR-0064's one JSON document, no new table, no new
    // column), and the gallery's per-row is session state because it is a fact
    // about the window's width.

    /// The time axis a calendar places records by and a timeline draws bars
    /// from: the document's `date` key when it names an existing date-ish
    /// column, else the schema's own first date column, else the first derived
    /// stamp. The fallbacks are the schema's order — the column the user would
    /// have picked — and are *not* written back: a default must not become a
    /// rule.
    fn db_time_axis(
        &self,
        db: DatabaseId,
        definition: &ViewDefinition,
    ) -> Option<(PropertyId, PropertyKind)> {
        let date_ish = |kind: PropertyKind| {
            matches!(
                kind,
                PropertyKind::Date
                    | PropertyKind::CreatedTime
                    | PropertyKind::LastEditedTime
            )
        };
        let catalog = self.databases.borrow();
        if let Some(id) = definition.date_column() {
            if let Some(row) = catalog.properties_of(db).find(|p| p.id == id) {
                if date_ish(row.kind) {
                    return Some((id, row.kind));
                }
            }
        }
        let stored = catalog
            .properties_of(db)
            .find(|p| p.kind == PropertyKind::Date)
            .map(|p| (p.id, p.kind));
        stored.or_else(|| {
            catalog
                .properties_of(db)
                .find(|p| date_ish(p.kind))
                .map(|p| (p.id, p.kind))
        })
    }

    /// Which month the block's calendar shows. Session state (ADR-0073's rule
    /// applied to a dial): the default is the month that contains now, read
    /// from the same local clock the record stamps use (`local_month`, the
    /// store's one clock), so "this month" means what the timestamps mean.
    pub fn db_calendar_month(&self, block: i32) -> (i32, u32) {
        if let Some(month) = self.db_cal_month.borrow().get(&block) {
            return *month;
        }
        let now = match self.db_repo() {
            Some(repo) => repo.local_month().ok().flatten(),
            None => None,
        };
        now.unwrap_or((2026, 9))
    }

    /// Set the calendar's month outright — the seed's door, and `db_cal_shift`'s
    /// innards. A month outside `1..=12` is refused rather than folded: a
    /// caller that sends `13` is confused, not approximate.
    pub fn db_calendar_month_set(&self, block: i32, year: i32, month: u32) -> bool {
        if !(1..=12).contains(&month) {
            return false;
        }
        self.db_cal_month.borrow_mut().insert(block, (year, month));
        // The cached window holds the old month's day cells: dropping it is
        // what makes the next projection read the new month's counts.
        self.db_windows.borrow_mut().remove(&block);
        self.db_refresh(block);
        true
    }

    /// ‹ › : step the calendar one month. One refresh per press — the month's
    /// counts and day peeks are a handful of small queries over a range the
    /// month's own bounds set, and the grid itself is fixed.
    pub fn db_cal_shift(&self, block: i32, delta: i32) -> bool {
        let (year, month) = self.db_calendar_month(block);
        let (year, month) = shift_month(year, month, delta);
        self.db_calendar_month_set(block, year, month)
    }

    /// The delegate's reported cards-per-row (it is the layer that knows the
    /// grid's width — the same split as `db_anchor`'s top-in-view). Clamped,
    /// because a report of `0` would divide the card slice by zero in
    /// `gallery_rows`'s caller and a report of `100` would make one card row
    /// per card: a grid of one column is the honest floor.
    pub fn db_gallery_set_per_row(&self, block: i32, per_row: i32) -> bool {
        let per_row = per_row.clamp(1, 8) as usize;
        let changed = self
            .db_gallery_per_row
            .borrow_mut()
            .insert(block, per_row)
            != Some(per_row);
        if changed {
            // The card slice is rows × per_row: a new shape re-reads.
            self.db_windows.borrow_mut().remove(&block);
            self.db_refresh(block);
        }
        changed
    }

    /// One keystroke into a form field: the draft learns the text, and nothing
    /// else happens. Deliberately **not** a write and **not** a refresh — the
    /// field is a live input, and re-filling the block row per keystroke would
    /// rebuild the very input the user is typing into. Nothing reaches SQL
    /// until Submit.
    pub fn db_form_set_text(&self, block: i32, property: i32, text: &str) {
        let mut form = self.db_form.borrow_mut();
        let draft = form.entry(block).or_default();
        let property = property as u64;
        match draft.iter_mut().find(|(p, _)| *p == property) {
            Some((_, slot)) => *slot = text.to_string(),
            None => draft.push((property, text.to_string())),
        }
    }

    /// Throw the draft away. The fields go blank on the next projection; the
    /// caller refreshes the block row.
    pub fn db_form_clear(&self, block: i32) {
        self.db_form.borrow_mut().remove(&block);
    }

    /// Submit: parse every filled field through its column's own rules (the
    /// same `parse_one` a cell edit runs, ADR-0069), create **one** record, and
    /// write the values into it — one batch, so one Ctrl+Z takes the whole
    /// submission back. A field that does not parse refuses the whole submit
    /// and says which column refused: a form that half-created a record would
    /// be two facts about one gesture.
    ///
    /// Empty fields write nothing (ADR-0062's absence-is-empty), and a draft
    /// with no values at all creates a bare record — the same thing the table's
    /// "New row" makes.
    pub fn db_form_submit(&self, block: i32) -> bool {
        if self.db_ref_of(block).is_none() || self.db_repo().is_none() {
            return false;
        }
        let catalog = self.databases.borrow();
        let draft = self.db_form.borrow().get(&block).cloned().unwrap_or_default();
        let mut values: Vec<(PropertyId, CellValue)> = Vec::new();
        for (property, text) in &draft {
            if text.is_empty() {
                continue;
            }
            let Some(row) = catalog.properties.iter().find(|p| p.id == PropertyId(*property))
            else {
                continue;
            };
            let value =
                match crate::core::database_property::parse_one(row.kind, &row.config, text) {
                    Ok(value) => value,
                    Err(e) => {
                        self.db_notice.borrow_mut().push(format!(
                            "{}: {e}",
                            if row.name.is_empty() {
                                "a column".to_string()
                            } else {
                                row.name.clone()
                            }
                        ));
                        return false;
                    }
                };
            values.push((PropertyId(*property), value));
        }
        // D7 (ADR-0086): the template fills what the draft left blank — the
        // typed words always win over the prefill, and a field the user
        // deliberately left empty while the template carries a value still
        // prefills, because an empty *draft* field is "not typed", not
        // "cleared" (the form has no clear gesture; a template-carrying
        // column the user wants blank is blanked on the new row itself).
        {
            let filled: std::collections::BTreeSet<u64> =
                values.iter().map(|(property, _)| property.as_u64()).collect();
            if let Some(db) = self.db_ref_of(block) {
                for (property, cell) in self.db_template_cells(db) {
                    if !filled.contains(&property.as_u64()) {
                        values.push((property, cell));
                    }
                }
            }
        }
        drop(catalog);
        // One batch: the record and every parsed value, planned together and
        // reverted together. The `from` half of each cell write is `Empty`
        // because the record does not exist yet — the plan reads the document,
        // and the store applies the writes in order.
        let record = RecordId(self.next_record_id.get());
        let ord = self.db_next_row_ord(block);
        let mut cmds = vec![Command::AddDatabaseRecord {
            block: BlockId(block as u64),
            record,
            ord,
        }];
        cmds.extend(values.into_iter().map(|(property, to)| Command::SetDatabaseCell {
            block: BlockId(block as u64),
            record,
            property,
            from: CellValue::Empty,
            to,
        }));
        if self.exec_all_on_open_page(cmds).is_none() {
            return false;
        }
        self.next_record_id.set(self.next_record_id.get() + 1);
        self.db_form_clear(block);
        self.db_refresh(block);
        true
    }

    // ─── D7: the advanced features (SPEC §三十九 「操作」's last three) ─────
    //
    // The view's live search (ADR-0087), the record template (ADR-0086) and
    // the linked database (ADR-0085). What the three share is a rule the rest
    // of this impl has been stating since D3: *the stored thing is the only
    // copy*. The search is a query compiled into SQL, never a Rust-side
    // retain; the template is a document on the `databases` row applied
    // through the ordinary cell write; a link is a pointer to one entity, and
    // there is no second entity to drift from it.

    /// The view's live search (SPEC §三十九 「操作」's 视图内搜索, ADR-0087).
    /// Session state (ADR-0073's rule): the needle is a question the user is
    /// asking, so it lives in `db_search`, never in the view's document, and a
    /// restart opens an unsearched view. An empty needle clears the search.
    /// Returns `true` when the request changed (the caller re-fills the row).
    ///
    /// There is **no debounce** on the write path, and that is deliberate:
    /// the needle changes which rows a *read* returns, and a read is exactly
    /// what `db_refresh` already gates (`unchanged` skips the query when the
    /// window has not moved). A keystroke costs one `COUNT(*)` over the
    /// predicate — the same class of scan D4's `contains` filter costs — and
    /// the honest tuning knob if a 10 000-row view feels slow while typing is
    /// a debounce on this callback, not a second row set in memory.
    pub fn db_view_search_set(&self, block: i32, text: &str) -> bool {
        if self.db_ref_of(block).is_none() {
            return false;
        }
        let needle = text.trim().to_string();
        {
            let mut map = self.db_search.borrow_mut();
            if needle.is_empty() {
                map.remove(&block);
            } else {
                map.insert(block, needle);
            }
        }
        self.db_refresh(block)
    }

    /// The needle a block's header search box holds (`""` = not searching).
    /// The UIState property is the words being typed; this is the fact the
    /// queries compile from — the same split as the cell editor's ids here
    /// and its text there.
    pub fn db_search_needle(&self, block: i32) -> String {
        self.db_search
            .borrow()
            .get(&block)
            .cloned()
            .unwrap_or_default()
    }

    /// The template's cells, parsed against the catalog — the prefill every
    /// new record starts from (ADR-0086). Values arrive in the shapes
    /// [`CellValue`] stores (the template *is* a copy of stored content), and
    /// an unreadable document is no template: the same fold every JSON
    /// document in this layer takes.
    fn db_template_cells(&self, db: DatabaseId) -> Vec<(PropertyId, CellValue)> {
        let template = {
            let catalog = self.databases.borrow();
            catalog
                .database(db)
                .map(|d| d.template.clone())
                .unwrap_or_default()
        };
        database_template::cells_of(&template)
    }

    /// Save a row's values as the database's template (the row action's T).
    /// The cells are read **as stored** — computed and derived kinds have no
    /// stored value (ADR-0062/0068) and are skipped, so a template cannot
    /// carry a formula's answer or a stamp's date — and the document is built
    /// by `database_template::from_cells` in one pass. A row with no values
    /// makes the empty template, which is how the affordance clears one:
    /// copying an empty row *is* "prefill nothing".
    ///
    /// One change (`SetDatabaseTemplate`, replace-whole), one Ctrl+Z restores
    /// the previous document.
    pub fn db_template_from_row(&self, block: i32, record: i64) -> bool {
        let Some(db) = self.db_ref_of(block) else {
            return false;
        };
        let Some(repo) = self.db_repo() else {
            return false;
        };
        let cells: Vec<(u64, CellValue)> = {
            let catalog = self.databases.borrow();
            catalog
                .properties_of(db)
                .filter(|p| {
                    // A template copies what is *stored*; the five kinds
                    // without a stored cell are skipped by kind, not by value.
                    !matches!(
                        p.kind,
                        PropertyKind::Formula
                            | PropertyKind::Rollup
                            | PropertyKind::Relation
                            | PropertyKind::CreatedTime
                            | PropertyKind::LastEditedTime
                    )
                })
                .filter_map(|p| {
                    let value = repo.cell(RecordId(record as u64), p.id).ok()?;
                    if value.is_empty() {
                        None
                    } else {
                        Some((p.id.as_u64(), value))
                    }
                })
                .collect()
        };
        let to = database_template::from_cells(&cells);
        let from = {
            let catalog = self.databases.borrow();
            catalog
                .database(db)
                .map(|d| d.template.clone())
                .unwrap_or_default()
        };
        if from == to {
            // Saving the same template twice is not an undo step (the rule
            // `SetDatabaseFormula` states), and the copy that changed nothing
            // says so by doing nothing.
            return false;
        }
        if self
            .exec_editor(Command::SetDatabaseTemplate {
                block: BlockId(block as u64),
                db,
                from,
                to,
            })
            .is_none()
        {
            return false;
        }
        // The prefill's source moved: the next projection shows the state the
        // change recorded (the catalog learned it through `db_absorb`).
        self.db_refresh(block);
        true
    }

    /// The chart body's three buttons (bar / line / pie): one view-document
    /// edit (`set_chart_kind`), the same ADR-0074 read-edit-write every other
    /// rule edit is — and the definition text is the window cache's key, so
    /// the refresh below re-reads the plot against the new shape. The *data*
    /// never changes with the shape: the same group counts feed all three.
    pub fn db_chart_kind_set(&self, block: i32, index: i32) -> bool {
        let kind = ChartKind::from_index(index.max(0) as usize);
        self.db_edit_definition(block, |definition| {
            definition.set_chart_kind(kind);
        })
    }

    /// Turn the line into a **linked database** (SPEC §三十九 「操作」,
    /// ADR-0085): a `Database` block whose `db_ref` names an entity another
    /// block already owns — the words go, the kind changes, and *nothing is
    /// created or copied*. Reads and writes resolve through the same `db_ref`
    /// every database block uses, so "the data and the view definitions are
    /// the source's, and writes land on the source" holds by construction;
    /// and when the source entity dies (only by undoing the block that created
    /// it — `DatabaseDeleted` has no other producer), this block dangles and
    /// draws the same one muted line a deleted database always has (ADR-0060).
    ///
    /// The caller checked the target exists; if it died since, the plan's
    /// write would still land and the next projection draws the dangling
    /// state — visible, not silent.
    pub fn db_make_linked(&self, block: i32, db: i32) -> bool {
        let db = DatabaseId(db.max(0) as u64);
        if !self.databases.borrow().database(db).is_some() {
            return false;
        }
        if self
            .exec_all_on_open_page(vec![
                Command::ReplaceText {
                    id: BlockId(block as u64),
                    text: String::new(),
                },
                Command::LinkDatabase {
                    id: BlockId(block as u64),
                    db,
                },
            ])
            .is_none()
        {
            return false;
        }
        self.db_refresh(block);
        true
    }

    /// The linked-database picker's rows: every live database, named as its
    /// own (`databases.name` — what a link says, not a row's title), with the
    /// view count as the hint, filtered the way every picker filters. One
    /// picker and not a wizard: a link has exactly one decision in it.
    pub fn open_slash_links(&self, filter: &str) {
        let needle = filter.to_lowercase();
        let rows: Vec<SlashRow> = {
            let catalog = self.databases.borrow();
            catalog
                .databases
                .iter()
                .filter(|d| needle.is_empty() || d.name.to_lowercase().contains(&needle))
                .map(|d| {
                    let views = catalog.views_of(d.id).count();
                    SlashRow {
                        id: d.id.as_u64() as i32,
                        label: d.name.clone().into(),
                        hint: if views == 1 {
                            "1 view".into()
                        } else {
                            format!("{views} views").into()
                        },
                        disabled: false,
                    }
                })
                .collect()
        };
        self.slash.set_vec(rows);
    }

    /// The database id behind the focused link-picker row.
    pub fn slash_selected_link(&self, focus: i32) -> Option<i32> {
        let row = self.slash.row_data(focus.max(0) as usize)?;
        if row.disabled {
            return None;
        }
        Some(row.id)
    }

    /// One new view of this database, in `ViewLayout::ALL`'s order — the
    /// switcher `+`'s menu row picked. The view is born named after its layout
    /// (renameable later, `ViewRenamed`), at the switcher's end, with an empty
    /// rules document; the app then *switches to it*, because a view created
    /// and not looked at is a gesture that did half of what it said.
    ///
    /// All eight layouts create: D7 drew the last one (`chart`, whose plot is
    /// the group list the board's columns already come from), so `db_add_view`
    /// has no refusal left — a menu row that cannot create is a promise the
    /// switcher keeps visibly instead.
    pub fn db_add_view(&self, block: i32, layout_index: i32) -> bool {
        let Some(db) = self.db_ref_of(block) else {
            return false;
        };
        let Some(layout) = crate::core::database::ViewLayout::ALL.get(layout_index as usize)
            .copied()
        else {
            return false;
        };
        // The end of the switcher's order, from the catalog (a schema is a
        // handful of rows; no `MAX(ord)` query — the same fold the columns'
        // ord uses).
        let ord = {
            let catalog = self.databases.borrow();
            catalog
                .views_of(db)
                .map(|v| v.ord)
                .max()
                .map(|last| {
                    OrderKey::between(Some(last), None)
                        .unwrap_or(OrderKey(last.0 + OrderKey::STRIDE))
                })
                .unwrap_or(OrderKey::FIRST)
        };
        let id = self.next_view_id.get();
        let view = crate::core::database::View {
            id: ViewId(id),
            db,
            name: layout.label().to_string(),
            layout,
            definition: String::new(),
            ord,
        };
        if self
            .exec_editor(Command::AddDatabaseView {
                block: BlockId(block as u64),
                view,
            })
            .is_none()
        {
            return false;
        }
        self.next_view_id.set(id + 1);
        // Switch to it: the same two lines `db_pick_view` runs, inline, because
        // the new view is the answer to the click that made it.
        self.db_active_view.borrow_mut().insert(block, ViewId(id));
        self.db_windows.borrow_mut().remove(&block);
        self.db_refresh(block);
        true
    }

    /// Open a record: to its page when it has one; otherwise the page is
    /// **minted now** — ADR-0063's lazy page, whose other half (the pointer)
    /// lands in the same batch, so the first Open is one undo step and a
    /// deleted page's record can be opened again into a fresh page.
    ///
    /// The page is created as a child of the page the database sits on, named
    /// after the record's title (a bare record's title lives in `db_values`,
    /// ADR-0063's second home). Returns the page's id; the caller navigates.
    /// `None` for a session without a store, or a record that is not there —
    /// both are facts about the session, not errors.
    pub fn db_open_record(&self, block: i32, record: i64) -> Option<i32> {
        let db = self.db_ref_of(block)?;
        let repo = self.db_repo().cloned()?;
        let record_id = RecordId(record as u64);
        let row = repo.record(record_id).ok().flatten()?;
        if let Some(page) = row.page {
            return Some(page.as_u64() as i32);
        }
        // The title, from the record's own title value (an empty title names
        // the page "Untitled", which is what a row nobody named is).
        let title = {
            let catalog = self.databases.borrow();
            let named = catalog
                .properties_of(db)
                .find(|p| p.kind.is_title())
                .and_then(|p| repo.cell(record_id, p.id).ok())
                .map(|value| value.display())
                .filter(|title| !title.trim().is_empty());
            named.unwrap_or_else(|| "Untitled".to_string())
        };
        // The page's own facts, the way `create_page` writes them: a child of
        // the page this database sits on, last among its siblings. Duplicated
        // rather than called because `create_page` *navigates* — and the caller
        // of an Open already owns the navigation.
        let parent = self.open_page.get();
        let id = self
            .workspace
            .borrow_mut()
            .create(if parent > 0 { Some(parent) } else { None }, &title);
        let order = {
            let kids = self.workspace.borrow().children_of(if parent > 0 {
                Some(parent)
            } else {
                None
            });
            let map = self.page_order.borrow();
            let prev = kids
                .len()
                .checked_sub(2)
                .and_then(|i| kids.get(i))
                .and_then(|pid| map.get(pid).copied());
            OrderKey::between(prev, None).expect("append order exhausted")
        };
        let page = crate::core::Page {
            id: PageId(id as u32 as u64),
            title,
            parent: if parent > 0 {
                Some(PageId(parent as u32 as u64))
            } else {
                None
            },
            order,
            favorite: false,
            expanded: false,
            font: crate::core::PageFont::default(),
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: false,
        };
        // One batch: the page exists and the record points at it, or neither
        // happened. This is ADR-0063's 「打开」 in one Ctrl+Z.
        self.record(vec![
            Change::PageCreated(page),
            Change::RecordPageSet {
                id: record_id,
                page: Some(PageId(id as u32 as u64)),
            },
        ]);
        Some(id)
    }
}

/// The cells of one table, as the row's data.
fn table_cells(
    blocks: &[Block],
    table: &Block,
    hits: &FindHits,
    titles: &MentionTitles,
) -> Vec<TableCell> {
    grid_blocks(blocks, table)
        .into_iter()
        .map(|c| TableCell {
            id: c.id.0 as i32,
            text: c.text.clone().into(),
            runs: runs_to_model(c, hits_of(hits, c.id), titles),
        })
        .collect()
}

/// One block of a columns layout, placed in the flat list the delegate draws.
struct ColumnSlot {
    /// which box, 0-based
    column: i32,
    /// how far inside that box (0 = a box's own child), for the indent
    depth: i32,
    /// position in the page's block list
    index: usize,
    /// has children, so the toggle chevron shows
    can_fold: bool,
}

/// A block's direct children, as positions in the page list. The list is kept
/// sorted by order key, so the sort is only saying so out loud.
fn child_indices(blocks: &[Block], parent: BlockId) -> Vec<usize> {
    let mut v: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.parent == Some(parent))
        .map(|(i, _)| i)
        .collect();
    v.sort_by_key(|i| blocks[*i].order);
    v
}

/// Emit `i` and everything below it, in reading order. A folded block keeps
/// its own slot and loses its subtree, exactly as the row list does.
fn column_slots(blocks: &[Block], i: usize, column: i32, depth: i32, out: &mut Vec<ColumnSlot>) {
    let b = &blocks[i];
    let kids = child_indices(blocks, b.id);
    out.push(ColumnSlot { column, depth, index: i, can_fold: !kids.is_empty() });
    // the depth cap is only a corrupt-data guard: nothing in the app nests
    // blocks eight deep inside one box
    if b.folded || depth >= 8 {
        return;
    }
    for k in kids {
        if blocks[k].kind == BlockKind::Column {
            continue; // a box is a container, never content
        }
        column_slots(blocks, k, column, depth + 1, out);
    }
}

/// A layout's content in reading order: every box's blocks, left to right.
/// The boxes are the layout's `Column` children — the same reading
/// `command::column_blocks` makes.
fn layout_slots(blocks: &[Block], layout: &Block) -> Vec<ColumnSlot> {
    let mut out = Vec::new();
    let mut box_n = 0i32;
    for c in child_indices(blocks, layout.id) {
        let b = &blocks[c];
        if b.kind != BlockKind::Column {
            // content hanging off the layout itself rather than a box has no
            // box to name, so it reads as the first one's
            column_slots(blocks, c, box_n, 0, &mut out);
            continue;
        }
        for k in child_indices(blocks, b.id) {
            if blocks[k].kind == BlockKind::Column {
                continue;
            }
            column_slots(blocks, k, box_n, 0, &mut out);
        }
        box_n += 1;
    }
    out
}

/// The blocks inside one layout, as the row's data, and its boxes as the
/// delegate sees them. Numbering restarts per box, which is what a reader
/// sees; the boxes carry where each group starts in the flat item list, since
/// Slint has no recursive component to work it out itself.
fn column_projection(
    blocks: &[Block],
    layout: &Block,
    hits: &FindHits,
    titles: &MentionTitles,
) -> (Vec<ColumnItem>, Vec<ColumnBox>) {
    // the layout's own boxes, left to right — the same reading
    // `command::column_blocks` makes
    let mut box_ids: Vec<BlockId> = Vec::new();
    for c in child_indices(blocks, layout.id) {
        if blocks[c].kind == BlockKind::Column {
            box_ids.push(blocks[c].id);
        }
    }
    let mut sizes = vec![0i32; box_ids.len()];
    let mut items = Vec::new();
    let mut numbers: Vec<(i32, i32)> = Vec::new();
    for s in layout_slots(blocks, layout) {
        let b = &blocks[s.index];
        let number = if b.kind == BlockKind::Numbered {
            match numbers.iter_mut().find(|(c, _)| *c == s.column) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    numbers.push((s.column, 1));
                    1
                }
            }
        } else {
            0
        };
        if let Some(n) = sizes.get_mut(s.column as usize) {
            *n += 1;
        }
        items.push(ColumnItem {
            id: b.id.0 as i32,
            column: s.column,
            depth: s.depth,
            kind: kind_to_int(b.kind),
            text: b.text.clone().into(),
            runs: runs_to_model(b, hits_of(hits, b.id), titles),
            checked: b.checked,
            number,
            folded: b.folded,
            can_fold: s.can_fold,
            color: b.color.slot(),
            bg: b.background.slot(),
            attachment: b.attachment.map(|a| a.as_u64() as i32).unwrap_or(0),
        });
    }
    // `layout_slots` walks box by box, so the groups are already contiguous
    let mut first = 0i32;
    let boxes = box_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let b = ColumnBox {
                id: id.0 as i32,
                column: i as i32,
                first,
                size: sizes[i],
            };
            first += sizes[i];
            b
        })
        .collect();
    (items, boxes)
}

/// Project a page's blocks into editor rows: numbered items renumbered by
/// position, the last row flagged as the tail spacer carrier. Folded
/// subtrees are left out entirely.
/// Where the find bar's hits sit, grouped by the block that carries them.
/// The map is empty while the bar is closed, and an empty map costs a row one
/// failed lookup -- the projection is not asked to pay for a search nobody
/// started.
pub type FindHits = HashMap<i32, Vec<(usize, usize)>>;

pub fn project_blocks(blocks: &[Block], hits: &FindHits, titles: &MentionTitles) -> Vec<BlockRow> {
    let shown = visible_block_indices(blocks);
    let mut out: Vec<BlockRow> = shown
        .iter()
        .map(|&i| {
            let b = &blocks[i];
            // one walk per layout row, at most: the pair is built together
            let (column_items, column_boxes) = if b.kind == BlockKind::Columns {
                let (items, boxes) = column_projection(blocks, b, hits, titles);
                (
                    slint::ModelRc::from(Rc::new(VecModel::from(items))),
                    slint::ModelRc::from(Rc::new(VecModel::from(boxes))),
                )
            } else {
                (ModelRc::default(), ModelRc::default())
            };
            BlockRow {
                id: b.id.0 as i32,
                kind: kind_to_int(b.kind),
                text: b.text.clone().into(),
                checked: b.checked,
                number: 0,
                tail: false,
                runs: runs_to_model(b, hits_of(hits, b.id), titles),
                depth: block_depth(blocks, b),
                color: b.color.slot(),
                bg: b.background.slot(),
                page_ref: b.page_ref.map(|p| p.as_u64() as i32).unwrap_or(-1),
                // SPEC §四十 / ADR-0052: the source this row mirrors, as
                // *stored* — 0 when there is none. `reproject_blocks` replaces
                // it with the block actually holding the words, or with -1
                // when nothing does; a row alone never sees enough to say.
                sync_source: b.sync_ref.map(|r| r.as_u64() as i32).unwrap_or(0),
                folded: b.folded,
                // 0 = none: the UI resolves an id through the attachment cache
                attachment: b.attachment.map(|a| a.as_u64() as i32).unwrap_or(0),
                img_percent: b.img_percent as i32,
                // any block can be a parent; EditorBlock only draws the
                // chevron for a Toggle, and a table's children are its grid
                // and a layout's its boxes, which no fold can reveal
                can_fold: blocks.iter().any(|x| x.parent == Some(b.id))
                    && !matches!(b.kind, BlockKind::Table | BlockKind::Columns),
                columns: b.columns as i32,
                // the language is only ever read back through `code-layer`, so
                // the row carries the stored string rather than a code
                lang: b.lang.as_str().into(),
                // the guards are the whole cost of these fields: both walks
                // scan the page, so calling them for every row would make the
                // projection quadratic on a 10 000-block page
                table_cells: if b.kind == BlockKind::Table {
                    slint::ModelRc::from(Rc::new(VecModel::from(table_cells(blocks, b, hits, titles))))
                } else {
                    ModelRc::default()
                },
                // the same guard as above: a contents walk is a page scan, and
                // a page with one TOC block has exactly one row that wants it
                toc_entries: if b.kind == BlockKind::Toc {
                    slint::ModelRc::from(Rc::new(VecModel::from(toc_entries(blocks, &shown))))
                } else {
                    ModelRc::default()
                },
                column_items, column_boxes,
                // SPEC §三十九: the entity this block draws, -1 for none. The
                // rest of the database fields are filled by `reproject_blocks`,
                // which is where the state (the catalog and the realized window)
                // is in reach — this function is pure and takes blocks only.
                db_ref: b.db_ref.map(|d| d.as_u64() as i32).unwrap_or(-1),
                db_ok: false,
                db_title: "".into(),
                db_rows: ModelRc::default(),
                db_columns: ModelRc::default(),
                db_views: ModelRc::default(),
                db_row_start: 0,
                db_row_count: 0,
                db_row_height: TableView::ROW_HEIGHT,
                db_header_height: TableView::HEADER_HEIGHT,
                db_layout: "".into(),
                db_layout_index: 0,
                db_layout_ok: true,
                // the rules' header state (D4): neutral here — `db_fill_row`
                // reads the view's document and fills these for a live block
                db_filter_note: "".into(),
                db_filter_count: 0,
                db_sort_property: -1,
                db_sort_desc: false,
                db_group_property: -1,
                // D5: the view family's fields, all neutral until `db_fill_row`
                // fills them for a live block
                db_body_height: 0.0,
                db_board_columns: ModelRc::default(),
                db_cal_days: ModelRc::default(),
                db_cal_label: "".into(),
                db_gallery_per_row: 1,
                db_tl_start: 0,
                db_tl_days: 0,
                db_form: ModelRc::default(),
                // D7 (chart): the plot's payload, neutral until `db_fill_row`
                // fills it for a live block
                db_chart_points: ModelRc::default(),
                db_chart_path: "".into(),
                db_chart_kind: 0,
            }
        })
        .collect();
    let mut n = 0;
    for r in &mut out {
        if r.kind == BLOCK_NUMBERED {
            n += 1;
            r.number = n;
        }
    }
    if let Some(last) = out.last_mut() {
        last.tail = true;
    }
    out
}

pub const BLOCK_PARAGRAPH: i32 = 0;
pub const BLOCK_H1: i32 = 1;
pub const BLOCK_H2: i32 = 2;
pub const BLOCK_H3: i32 = 3;
pub const BLOCK_BULLET: i32 = 4;
pub const BLOCK_NUMBERED: i32 = 5;
pub const BLOCK_TODO: i32 = 6;
pub const BLOCK_QUOTE: i32 = 7;
pub const BLOCK_CODE: i32 = 8;
pub const BLOCK_DIVIDER: i32 = 9;
pub const BLOCK_CALLOUT: i32 = 10;
pub const BLOCK_PAGE: i32 = 11;
pub const BLOCK_LINK: i32 = 12;
pub const BLOCK_TOGGLE: i32 = 13;
pub const BLOCK_IMAGE: i32 = 14;
pub const BLOCK_FILE: i32 = 15;
pub const BLOCK_TABLE: i32 = 16;
pub const BLOCK_TABLE_CELL: i32 = 17;
pub const BLOCK_COLUMNS: i32 = 18;
pub const BLOCK_COLUMN: i32 = 19;

/// The mention picker's date row (SPEC §四十). A page id is a positive
/// integer, so a negative one can never collide with a real page — and unlike
/// the `disabled` placeholder rows, this one is selectable: it is the second
/// thing `@` can produce.
pub const MENTION_DATE_ROW: i32 = -1;
/// The slash / insert menu's "Linked view" row (SPEC §三十九 「操作」,
/// ADR-0085): not a block kind, so its id is negative — the picker rows'
/// convention (`MENTION_DATE_ROW` is `-1`), and the apply path reads it
/// before any kind mapping. Picking it switches the popup to the
/// database picker (`open_slash_links`); picking a database there makes
/// this line a linked database (`db_make_linked`).
pub const LINKED_VIEW_ROW: i32 = -2;
pub const BLOCK_MATH: i32 = 20;
pub const BLOCK_TOC: i32 = 21;
pub const BLOCK_EMBED: i32 = 22;
/// UI integer of a `Synced` block (kind 24 in declaration order — **after**
/// §三十九's `Database`, which is declared before it).
pub const BLOCK_SYNCED: i32 = 24;
/// A database view (SPEC §三十九, ADR-0060). The *layout* — table / board / … —
/// is `db_views.layout` and not a kind int: one kind, eight layouts, which is
/// what lets the six insert-menu placeholders light one at a time.
pub const BLOCK_DATABASE: i32 = 23;

/// The editor viewport height assumed before Slint has reported the real one, in
/// px: `benchmarks/scripts/bench.ps1` renders at 1280×800, and the editor is
/// most of it below the top bar. The window arithmetic divides by a *viewport*,
/// so the first projection after startup (before any layout has run) would
/// otherwise realize the whole table for one frame — the number only has to be
/// the right order of magnitude, and `Editor.slint` replaces it on the first
/// layout pass.
pub const DEFAULT_EDITOR_VIEWPORT_H: f32 = 720.0;

/// How many references the *folded* panel draws. A drawing budget, not a limit
/// on the answer: a page quoted two hundred times must not push its own prose
/// off the screen (SPEC §四十), so the list is a window and the folded line
/// carries the real count instead of the rows it is not drawing.
pub const BACKLINK_WINDOW: usize = 5;

/// How many it draws once the reader asks for the rest. Still bounded, and for
/// a different reason: the panel is a `for` inside one document row rather
/// than a `ListView` of its own, so every row it is handed is a row the frame
/// lays out — 50 is the point where a folded panel stops being a link to the
/// rest and becomes a page of its own.
pub const BACKLINK_EXPANDED: usize = 50;

fn block(kind: i32, text: &str) -> BlockRow {
    BlockRow {
        id: 0,
        kind,
        text: text.into(),
        checked: false,
        number: 0,
        tail: false,
        runs: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        depth: 0,
        color: 0,
        bg: 0,
        page_ref: -1,
        folded: false,
        attachment: 0,
        img_percent: 100,
        can_fold: false,
        columns: 0,
        lang: "".into(),
        table_cells: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        column_items: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        column_boxes: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        toc_entries: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        // database (23) + synced (24): neutral defaults; real projections
        // fill these in (db_ref_of / sync_target), the helper only compiles.
        db_ref: -1,
        db_ok: false,
        db_title: "".into(),
        db_rows: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        db_columns: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        db_views: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
        db_row_start: 0,
        db_row_count: 0,
        db_row_height: 0.0,
        db_header_height: 0.0,
        db_layout: "".into(),
        db_layout_index: 0,
        db_layout_ok: false,
        // the rules' header state (D4): neutral here like the rest, filled by
        // db_fill_row for a live block
        db_filter_note: "".into(),
        db_filter_count: 0,
        db_sort_property: -1,
        db_sort_desc: false,
        db_group_property: -1,
        // D5: the view family's fields, neutral in the test factory like the
        // rest — a projection without a database has no board, grid, axis or
        // draft to fill them from
        db_body_height: 0.0,
        db_board_columns: ModelRc::default(),
        db_cal_days: ModelRc::default(),
        db_cal_label: "".into(),
        db_gallery_per_row: 1,
        db_tl_start: 0,
        db_tl_days: 0,
        db_form: ModelRc::default(),
        // D7 (chart): neutral, like the rest of the family's fields here
        db_chart_points: ModelRc::default(),
        db_chart_path: "".into(),
        db_chart_kind: 0,
        sync_source: 0,
    }
}

/// Flag the last row so its delegate renders the page's bottom spacer
/// (a ListView allows a single `for`, so chrome must live inside a row).
fn with_tail(mut b: Vec<BlockRow>) -> Vec<BlockRow> {
    if let Some(last) = b.last_mut() {
        last.tail = true;
    }
    b
}

fn number_renumber(b: &mut [BlockRow]) {
    let mut n = 0;
    for r in b.iter_mut() {
        if r.kind == BLOCK_NUMBERED {
            n += 1;
            r.number = n;
        }
    }
}

fn mock_blocks_for_page(id: i32, title: &str) -> Vec<BlockRow> {
    match id {
        PAGE_GETTING_STARTED => mock_blocks_sample(),
        PAGE_CHINESE => mock_blocks_chinese(),
        PAGE_SCRATCHPAD => Vec::new(),
        _ => mock_blocks_generic(title),
    }
}

fn mock_blocks_sample() -> Vec<BlockRow> {
    let mut b = vec![
        block(
            BLOCK_PARAGRAPH,
            "A quiet home for thinking. This workspace collects notes, plans, and references for the Atlas project — and doubles as the visual test document for Quire itself.",
        ),
        block(BLOCK_DIVIDER, ""),
        block(BLOCK_H2, "Why a local-first editor"),
        block(
            BLOCK_PARAGRAPH,
            "Cloud apps are great until the laptop fan sounds like a jet engine. Quire keeps documents in a local SQLite database, renders with the GPU, and stays out of the way.",
        ),
        block(BLOCK_QUOTE, "Simplicity is the ultimate sophistication — but performance is the ultimate courtesy."),
        block(BLOCK_H3, "Principles"),
        block(BLOCK_BULLET, "One process, one document model, no hidden servers"),
        block(BLOCK_BULLET, "Only the focused block owns a real text cursor"),
        block(BLOCK_BULLET, "Nothing animates unless the user asks for it"),
        block(BLOCK_NUMBERED, "Write instantly, even on a five-year-old laptop"),
        block(BLOCK_NUMBERED, "Scroll a 10 000-block page without hitching"),
        block(BLOCK_NUMBERED, "Close the lid, reopen, and everything is there"),
        block(BLOCK_TODO, "Block editor MVP"),
        block(BLOCK_TODO, "Slash menu — type \"/\" at the start of a line"),
        block(BLOCK_H2, "Try the interactions"),
        block(
            BLOCK_PARAGRAPH,
            "Everything below already works: select text and press Ctrl+B / Ctrl+I / Ctrl+E for bold, italic, and inline code; Ctrl+L links it; Tab indents a list item and Shift+Tab promotes it back.",
        ),
        block(BLOCK_BULLET, "Ctrl+P searches every page — titles and content"),
        block(BLOCK_BULLET, "Ctrl+K opens the command palette"),
        block(BLOCK_BULLET, "Right-click a page in the sidebar for its menu"),
        block(BLOCK_BULLET, "Click this page's big title to rename it in place"),
        block(BLOCK_H2, "运行与中文"),
        block(
            BLOCK_PARAGRAPH,
            "中文段落用于验证字体回退与行高：排版本应稳定，不出现字符裁剪；标点悬挂与换行位置符合预期。",
        ),
        block(BLOCK_CODE, "cargo run --release  # 140 ms to first paint, hopefully"),
        block(
            BLOCK_PARAGRAPH,
            "Start typing, or press Ctrl+K to open the command palette.",
        ),
    ];
    for (i, row) in b.iter_mut().enumerate() {
        row.id = i as i32;
    }
    number_renumber(&mut b);
    with_tail(b)
}

fn mock_blocks_chinese() -> Vec<BlockRow> {
    let mut b = vec![
        block(BLOCK_H2, "写作与中文测试"),
        block(
            BLOCK_PARAGRAPH,
            "中文段落用于验证字体回退与行高：排版本应稳定，不出现字符裁剪；标点悬挂与换行位置符合预期。",
        ),
        block(
            BLOCK_PARAGRAPH,
            "在长段落中混排 English words 与数字（如 2026 年 9 月）时，基线应保持一致，中西文之间留有恰当的间隙。",
        ),
        block(BLOCK_TODO, "检查行高在 125% 缩放下是否稳定"),
        block(BLOCK_TODO, "检查标点挤压与引号方向"),
        block(BLOCK_QUOTE, "好的排版是看不见的 —— 读者只注意到内容本身。"),
        block(BLOCK_CODE, "cargo run --release --features skia"),
    ];
    for (i, row) in b.iter_mut().enumerate() {
        row.id = i as i32;
    }
    with_tail(b)
}

/// Placeholder content for regular pages. No H1: the editor header already
/// renders the page title.
fn mock_blocks_generic(_title: &str) -> Vec<BlockRow> {
    let mut b = vec![
        block(
            BLOCK_PARAGRAPH,
            "This page is empty. Start typing, or use the + handle beside any block to insert one — the slash menu offers every kind Quire knows.",
        ),
        block(BLOCK_DIVIDER, ""),
        block(
            BLOCK_PARAGRAPH,
            "Use the sidebar to create, rename, duplicate, and delete pages — everything you type is written to a local SQLite library and is there again after a restart.",
        ),
        block(BLOCK_BULLET, "Ctrl+P searches every page, titles and content"),
        block(BLOCK_BULLET, "Ctrl+K opens the command palette"),
        block(BLOCK_BULLET, "Right-click a page for its context menu"),
    ];
    for (i, row) in b.iter_mut().enumerate() {
        row.id = i as i32;
    }
    with_tail(b)
}

/// Small page body for scene G's bench pages.
fn mock_blocks_bench_page(title: &str) -> Vec<BlockRow> {
    let mut b = vec![
        block(
            BLOCK_PARAGRAPH,
            "Benchmark fixture page. Switching between these pages exercises model swap + delegate rebuild.",
        ),
        block(BLOCK_H2, title),
        block(BLOCK_PARAGRAPH, "The quick brown fox jumps over the lazy dog."),
        block(BLOCK_BULLET, "Pack my box with five dozen liquor jugs."),
        block(BLOCK_TODO, "Sphinx of black quartz, judge my vow."),
    ];
    for (i, row) in b.iter_mut().enumerate() {
        row.id = i as i32;
    }
    with_tail(b)
}

fn mock_blocks_bench(count: usize) -> Vec<BlockRow> {
    let lorem = [
        "The quick brown fox jumps over the lazy dog.",
        "Pack my box with five dozen liquor jugs, then verify rendering.",
        "中文基准段落：用于长文档下的内存与滚动性能测试。",
        "Sphinx of black quartz, judge my vow — line after line after line.",
    ];
    with_tail(
        (0..count)
            .map(|i| {
                let kind = match i % 20 {
                    0 => BLOCK_H2,
                    5 | 6 => BLOCK_BULLET,
                    9 => BLOCK_TODO,
                    13 => BLOCK_QUOTE,
                    _ => BLOCK_PARAGRAPH,
                };
                let mut row = block(kind, lorem[i % lorem.len()]);
                row.id = i as i32;
                row
            })
            .collect(),
    )
}

/// How many distinct pictures a bench page of `--pictures N` stores. Rows
/// reuse the pool *spread across the page* (image row k takes fixture
/// `k % pool`), so consecutive picture rows still carry different rasters —
/// which is what a photo page does, and what the decode cache is sized for —
/// while the seed pass stays short and the folder stays under ~40 MB.
const BENCH_FIXTURE_POOL: usize = 200;

/// `(stride, pool)` for a bench page: one picture row every `stride` blocks,
/// drawn from `pool` fixtures. Both the row builder and the seeder ask, so
/// neither can drift from the other.
fn bench_picture_plan(rows: usize, pictures: usize) -> (usize, usize) {
    if pictures == 0 || rows == 0 {
        return (0, 0);
    }
    let n = pictures.min(rows);
    ((rows / n).max(1), n.min(BENCH_FIXTURE_POOL))
}

/// Turn every `stride`-th row of the bench page into an image block.
fn bench_pictures(blocks: &mut [Block], pictures: usize) {
    let (stride, pool) = bench_picture_plan(blocks.len(), pictures);
    if stride == 0 {
        return;
    }
    for (i, b) in blocks.iter_mut().enumerate() {
        if i % stride != stride - 1 {
            continue;
        }
        b.kind = BlockKind::Image;
        b.text = String::new();
        b.attachment = Some(AttachmentId(((i / stride) % pool) as u64 + 1));
    }
}

/// Bold the second word of every `stride`-th bench row, so the bench page has
/// marked paragraphs and the gate can see the runs channel at all (ADR-0041).
/// A row with no space to bold — the Chinese fixture line — is left alone,
/// which is the point: it takes no runs.
fn bench_marks(blocks: &mut [Block], marks: usize) {
    if marks == 0 {
        return;
    }
    let stride = (blocks.len() / marks.min(blocks.len())).max(1);
    for (i, b) in blocks.iter_mut().enumerate() {
        if i % stride != stride - 1 {
            continue;
        }
        let t = b.text.as_str();
        let Some(start) = t.find(' ').map(|p| p + 1) else {
            continue;
        };
        let Some(rel) = t[start..].find(' ') else {
            continue;
        };
        b.marks = vec![crate::core::Mark {
            start,
            end: start + rel,
            kind: crate::core::MarkKind::Bold,
            url: String::new(),
            date: None,
        }];
    }
}

/// A code block that reads like source: a comment line, keywords, a string,
/// numbers, an identifier-heavy line long enough to need a break, and two CJK
/// words so the rule for characters this model cannot size is on the page for
/// the gate to price. Same fixture on every row, because the gate measures the
/// layers, not the lexer's interest in novelty.
const BENCH_CODE_LINES: [&str; 7] = [
    r#"// measure the bytes a page of code actually costs"#,
    r#"fn measure(config: &Config) -> Result<u32, Error> {"#,
    r#"    let mut total = 0; // running bytes, in pages"#,
    r#"    for row in config.rows.iter().take(4096) { total += row.private_bytes; }"#,
    r#"    println!("合计 {total} 字节 across {} rows", config.rows.len());"#,
    r#"    log::trace!("done"); Ok(total)"#,
    r#"}"#,
];

/// The code fixture the bench page scrolls, spelled out. The `code-hl`
/// screenshot scene paints the same text, so the memory gate and the pixels
/// look at one block rather than two that can drift apart.
pub fn bench_code_source() -> String {
    BENCH_CODE_LINES.join("\n")
}

/// Turn every `stride`-th row of the bench page into a highlighted code block
/// (--code N, SPEC §三十七 批次 C). The languages rotate over everything the
/// lexer knows except `Plain`: an uncoloured block costs six fewer `Text`s, so
/// leaving one in the ring would under-price exactly what is being gated.
fn bench_code(blocks: &mut [Block], code: usize) {
    if code == 0 {
        return;
    }
    let stride = (blocks.len() / code.min(blocks.len())).max(1);
    let langs: Vec<Lang> = Lang::ALL.iter().copied().filter(|l| *l != Lang::Plain).collect();
    let mut taken = 0;
    for (i, b) in blocks.iter_mut().enumerate() {
        if i % stride != stride - 1 {
            continue;
        }
        b.kind = BlockKind::Code;
        b.text = bench_code_source();
        b.lang = langs[taken % langs.len()];
        taken += 1;
    }
}

/// Title + block texts, the blob the search scans.
fn block_search_blob(title: &str, blocks: &[BlockRow]) -> String {
    let mut blob = String::from(title);
    for b in blocks {
        if !b.text.is_empty() {
            blob.push('\n');
            blob.push_str(&b.text);
        }
    }
    blob
}

// ---- command palette ----

const CMD_NEW_PAGE: i32 = 1;
const CMD_SEARCH: i32 = 2;
const CMD_TOGGLE_SIDEBAR: i32 = 3;
const CMD_TOGGLE_THEME: i32 = 4;
const CMD_SETTINGS: i32 = 5;
pub const CMD_RENAME_PAGE: i32 = 6;
pub const CMD_DUPLICATE_PAGE: i32 = 7;
pub const CMD_DELETE_PAGE: i32 = 8;
pub const CMD_EXPORT_PAGE: i32 = 9;
pub const CMD_IMPORT_MD: i32 = 10;
/// Copy the open page's markdown onto the clipboard (FFI write, ADR-0025).
pub const CMD_COPY_MD: i32 = 11;
/// Retrace / re-advance the session's page navigation (SPEC §十六).
pub const CMD_NAV_BACK: i32 = 12;
pub const CMD_NAV_FORWARD: i32 = 13;
/// Jump-to-page commands are 10 000 + page id.
pub const CMD_PAGE_BASE: i32 = 10_000;

/// What a palette row means, resolved from its id in exactly one place.
///
/// The controller matches on this instead of on the raw id, and that match has
/// no wildcard arm: the palette dispatch used to be a `match id` over numeric
/// literals, and one constant whose import was missing turned its arm into a
/// catch-all *binding* — every command with id >= 9 silently ran `CopyMd`
/// (`aaa3763`). rustc warned, and nothing tested it. A row that maps to no
/// action is now a compile error at the dispatch and a failed assertion here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    NewPage,
    SearchPages,
    ToggleSidebar,
    ToggleTheme,
    Settings,
    RenamePage,
    DuplicatePage,
    DeletePage,
    ExportMarkdown,
    ImportMarkdown,
    CopyMarkdown,
    NavigateBack,
    NavigateForward,
    OpenPage(i32),
    None,
}

pub fn palette_action(id: i32) -> PaletteAction {
    match id {
        CMD_NEW_PAGE => PaletteAction::NewPage,
        CMD_SEARCH => PaletteAction::SearchPages,
        CMD_TOGGLE_SIDEBAR => PaletteAction::ToggleSidebar,
        CMD_TOGGLE_THEME => PaletteAction::ToggleTheme,
        CMD_SETTINGS => PaletteAction::Settings,
        CMD_RENAME_PAGE => PaletteAction::RenamePage,
        CMD_DUPLICATE_PAGE => PaletteAction::DuplicatePage,
        CMD_DELETE_PAGE => PaletteAction::DeletePage,
        CMD_EXPORT_PAGE => PaletteAction::ExportMarkdown,
        CMD_IMPORT_MD => PaletteAction::ImportMarkdown,
        CMD_COPY_MD => PaletteAction::CopyMarkdown,
        CMD_NAV_BACK => PaletteAction::NavigateBack,
        CMD_NAV_FORWARD => PaletteAction::NavigateForward,
        page if page >= CMD_PAGE_BASE => PaletteAction::OpenPage(page - CMD_PAGE_BASE),
        _ => PaletteAction::None,
    }
}

fn mock_commands(ws: &Workspace) -> Vec<CommandRow> {
    let mut v = Vec::new();
    let mut cmd = |id: i32, name: &str, hint: &str, section: &str, icon: &str| {
        v.push(CommandRow {
            id,
            name: name.into(),
            hint: hint.into(),
            section: section.into(),
            icon: icon.into(),
        })
    };
    cmd(CMD_NEW_PAGE, "New Page", "Ctrl+N", "Editor", "plus");
    cmd(CMD_SEARCH, "Search Pages…", "Ctrl+P", "Navigate", "search");
    cmd(
        CMD_TOGGLE_SIDEBAR,
        "Toggle Sidebar",
        "Ctrl+\\",
        "Interface",
        "panel-left",
    );
    cmd(
        CMD_TOGGLE_THEME,
        "Toggle Dark Mode",
        "Ctrl+Shift+L",
        "Interface",
        "moon",
    );
    cmd(CMD_SETTINGS, "Settings", "", "Navigate", "settings");
    cmd(CMD_RENAME_PAGE, "Rename Page", "F2", "Page", "pencil");
    cmd(CMD_DUPLICATE_PAGE, "Duplicate Page", "", "Page", "copy");
    cmd(CMD_DELETE_PAGE, "Delete Page", "", "Page", "trash");
    cmd(
        CMD_EXPORT_PAGE,
        "Export Page as Markdown…",
        "",
        "Page",
        "export",
    );
    cmd(CMD_IMPORT_MD, "Import Markdown…", "", "Page", "import");
    cmd(
        CMD_COPY_MD,
        "Copy Page as Markdown",
        "",
        "Page",
        "copy",
    );
    cmd(CMD_NAV_BACK, "Go Back", "Alt+Left", "Navigate", "arrow-left");
    cmd(
        CMD_NAV_FORWARD,
        "Go Forward",
        "Alt+Right",
        "Navigate",
        "arrow-right",
    );
    for id in ws.dfs_order() {
        if id >= BENCH_ID_BASE {
            continue;
        }
        if let Some(title) = ws.title_of(id) {
            cmd(CMD_PAGE_BASE + id, title, "", "Jump to page", "page");
        }
    }
    v
}

/// Cheap subsequence fuzzy match, case-insensitive, ASCII-only scoring.
fn fuzzy_subsequence(query: &str, target: &str) -> bool {
    let target = target.to_lowercase();
    let mut it = target.chars();
    query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .all(|q| it.any(|t| t == q))
}

#[cfg(test)]
mod tests {
    use super::{
        mock_commands, palette_action, FindHits, Lang, NavHistory, CMD_NAV_BACK,
        CMD_NAV_FORWARD, CMD_PAGE_BASE, NAV_MAX, PaletteAction,
    };
    use crate::app::workspace::Workspace;
    use crate::core::persistence::Change;

    /// A block in display order, page 1, no marks — the shape the projections
    /// below read.
    fn blk(
        id: u64,
        parent: Option<u64>,
        order: u64,
        kind: crate::core::BlockKind,
        folded: bool,
        text: &str,
    ) -> crate::core::Block {
        use crate::core::{Block, BlockId, ColorKind, OrderKey, PageId};
        Block {
            id: BlockId(id),
            page: PageId(1),
            parent: parent.map(|p| BlockId(p)),
            order: OrderKey(order),
            kind,
            text: text.into(),
            checked: false,
            marks: Vec::new(),
            color: ColorKind::Default,
            background: ColorKind::Default,
            page_ref: None,
            folded,
            attachment: None,
            img_percent: 100,
            columns: 0,
            lang: Lang::Plain,
            db_ref: None,
            sync_ref: None,        }
    }

    /// A page's blocks in display order: a folded toggle with two children
    /// (one of them a grandchild of the other) and a trailing paragraph.
    fn fold_scene() -> Vec<crate::core::Block> {
        use crate::core::BlockKind;
        vec![
            blk(1, None, 10, BlockKind::Toggle, true, "section"),
            blk(2, Some(1), 11, BlockKind::Paragraph, false, "a"),
            blk(3, Some(2), 12, BlockKind::Bullet, false, "a/1"),
            blk(4, None, 13, BlockKind::Toggle, false, "open section"),
            blk(5, Some(4), 14, BlockKind::Paragraph, false, "b"),
            blk(6, None, 15, BlockKind::Paragraph, false, "tail"),
        ]
    }

    /// What a `Toc` row's list currently says, as (block, label, indent).
    fn toc_of(row: &crate::BlockRow) -> Vec<(i32, String, i32)> {
        use slint::Model as _;
        (0..row.toc_entries.row_count())
            .map(|i| {
                let e = row.toc_entries.row_data(i).unwrap();
                (e.block, e.label.to_string(), e.level)
            })
            .collect()
    }

    #[test]
    fn a_folded_subtree_produces_no_rows_at_all() {
        let blocks = fold_scene();
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        // SPEC §三十七: the collapsed section costs real rows, not hidden
        // delegates — block 2 and its own child 3 drop out with their parent.
        let ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![1, 4, 5, 6]);
        assert_eq!(super::visible_block_indices(&blocks), vec![0, 3, 4, 5]);
    }

    #[test]
    fn unfolding_returns_the_subtree_in_its_source_order() {
        let mut blocks = fold_scene();
        blocks[0].folded = false;
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        let ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn the_fold_flag_reports_children_not_kind() {
        let blocks = fold_scene();
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        assert!(rows.iter().find(|r| r.id == 1).unwrap().can_fold);
        assert!(rows.iter().find(|r| r.id == 4).unwrap().can_fold);
        // a section with nothing in it reports false: no chevron to click.
        // The stored flag stays whatever it was — it just hides nothing.
        let lone = vec![{
            let mut b = blocks[0].clone();
            b.id = crate::core::BlockId(9);
            b.parent = None;
            b
        }];
        let rows = super::project_blocks(&lone, &FindHits::new(), &super::MentionTitles::empty());
        assert!(!rows[0].can_fold, "toggle with no children");
        assert_eq!(rows[0].kind, super::BLOCK_TOGGLE);
        assert!(rows[0].folded, "the flag rides along with the block");
    }

    #[test]
    fn hidden_rows_do_not_rename_the_numbered_list() {
        use crate::core::BlockKind;
        let mut blocks = fold_scene();
        blocks[0].kind = BlockKind::Numbered;
        blocks[1].kind = BlockKind::Numbered;
        blocks[2].kind = BlockKind::Numbered;
        blocks[5].kind = BlockKind::Numbered;
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        // 2 and 3 are inside the fold: the visible list is 1 then 2, not 4
        let nums: Vec<i32> = rows.iter().filter(|r| r.kind == 5).map(|r| r.number).collect();
        assert_eq!(nums, vec![1, 2]);
    }

    /// Pages 1..=9 are alive; anything else was deleted.
    fn live(id: i32) -> bool {
        (1..=9).contains(&id)
    }

    #[test]
    fn a_contents_row_lists_the_headings_the_page_can_show() {
        use crate::core::BlockKind;
        let blocks = vec![
            blk(1, None, 10, BlockKind::Toc, false, ""),
            blk(2, None, 11, BlockKind::Heading1, false, "Top"),
            blk(3, None, 12, BlockKind::Toggle, true, "Hidden section"),
            blk(4, Some(3), 13, BlockKind::Heading2, false, "Inside the fold"),
            blk(5, None, 14, BlockKind::Heading2, false, "Second"),
            blk(6, None, 15, BlockKind::Heading3, false, ""),
            blk(7, None, 16, BlockKind::Paragraph, false, "prose"),
        ];
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        let toc = rows
            .iter()
            .find(|r| r.kind == super::BLOCK_TOC)
            .expect("a toc row");
        // 4 is behind its folded parent: the list is built from the rows the
        // projection itself made, so a heading nobody can reach is not here.
        // An untitled heading still gets a line, the same word a tab shows.
        assert_eq!(
            toc_of(toc),
            vec![
                (2, "Top".into(), 1),
                (5, "Second".into(), 2),
                (6, "Untitled".into(), 3),
            ]
        );
        // …and the walk runs for the one row that asks for it
        assert!(
            rows.iter()
                .filter(|r| r.kind != super::BLOCK_TOC)
                .all(|r| toc_of(r).is_empty()),
            "a paragraph carries a contents list"
        );
    }

    #[test]
    fn renaming_a_heading_renames_its_line_because_nothing_was_copied() {
        use crate::core::BlockKind;
        let mut blocks = vec![
            blk(1, None, 10, BlockKind::Toc, false, ""),
            blk(2, None, 11, BlockKind::Heading2, false, "Before"),
        ];
        assert_eq!(toc_of(&super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty())[0])[0].1, "Before");
        blocks[1].text = "After".into();
        assert_eq!(toc_of(&super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty())[0])[0].1, "After");
        // the block's own row keeps whatever text it was made from, like a
        // divider does — painted by no one, and the list never read it
        blocks[0].text = "stale".into();
        assert_eq!(toc_of(&super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty())[0])[0].1, "After");
    }

    /// What one contents block adds to a projection, on the shape the RAM gate
    /// measures: 10 000 rows, a heading every tenth, release profile.
    #[test]
    #[ignore = "prints a timing; run with --release"]
    fn cost_of_one_contents_block_on_a_ten_thousand_row_page() {
        use crate::core::BlockKind;
        use std::time::Instant;
        let rounds = 50u32;
        let build = |toc: bool| -> Vec<crate::core::Block> {
            (0..10_000u64)
                .map(|i| {
                    let kind = if toc && i == 0 {
                        BlockKind::Toc
                    } else if i % 10 == 5 {
                        BlockKind::Heading2
                    } else {
                        BlockKind::Paragraph
                    };
                    blk(i + 2, None, i, kind, false, "a line of the bench page")
                })
                .collect()
        };
        let time = |blocks: &[crate::core::Block]| {
            let t = Instant::now();
            for _ in 0..rounds {
                std::hint::black_box(super::project_blocks(blocks, &FindHits::new(), &super::MentionTitles::empty()).len());
            }
            t.elapsed().as_secs_f64() * 1e3 / rounds as f64
        };
        let (without, with) = (time(&build(false)), time(&build(true)));
        println!(
            "projection: no contents block {without:.3} ms, one on 10 000 rows {with:.3} ms \
             (+{:.3} ms, 1 000 headings)",
            with - without
        );
    }

    /// What the word cut costs a projection: the same 10 000 rows, once with
    /// no marks at all and once with a bold mark on every tenth row, so 1 000
    /// paragraphs go from three runs to fifteen. Printed, not asserted — its
    /// number is the A/B between the two arms of the RAM gate, and only the
    /// control build still has the three-run shape.
    #[test]
    #[ignore = "prints a timing; run with --release"]
    fn cost_of_a_marked_page_on_the_projection() {
        use crate::core::{BlockKind, Mark, MarkKind};
        use std::time::Instant;
        let rounds = 50u32;
        let build = |marked: bool| -> Vec<crate::core::Block> {
            (0..10_000u64)
                .map(|i| {
                    let mut b = blk(
                        i + 2,
                        None,
                        i,
                        BlockKind::Paragraph,
                        false,
                        "Pack my box with five dozen liquor jugs, then verify rendering.",
                    );
                    if marked && i % 10 == 3 {
                        let t = b.text.as_str();
                        let start = t.find(' ').unwrap() + 1;
                        let end = start + t[start..].find(' ').unwrap();
                        b.marks = vec![Mark {
                            start,
                            end,
                            kind: MarkKind::Bold,
                            url: String::new(),
                            date: None,
                        }];
                    }
                    b
                })
                .collect()
        };
        let time = |blocks: &[crate::core::Block]| {
            let t = Instant::now();
            for _ in 0..rounds {
                std::hint::black_box(super::project_blocks(blocks, &FindHits::new(), &super::MentionTitles::empty()).len());
            }
            t.elapsed().as_secs_f64() * 1e3 / rounds as f64
        };
        let (plain, with_marks) = (time(&build(false)), time(&build(true)));
        println!(
            "projection: 10 000 unmarked rows {plain:.3} ms, 1 000 of them marked \
             {with_marks:.3} ms (+{:.3} ms)",
            with_marks - plain
        );
    }

    /// What the find bar costs the two things it touches: a projection that
    /// splits 1 000 of 10 000 rows into hit cells, and the walk that answers
    /// "which row paints this block" once per hit. Printed, not asserted — the
    /// second number is the one that decides whether the walk is allowed to be
    /// per-hit, since the bar does it on every keystroke.
    #[test]
    #[ignore = "prints a timing; run with --release"]
    fn cost_of_a_find_session_on_the_page_it_marks() {
        use crate::core::{BlockId, BlockKind};
        use std::time::Instant;
        let rounds = 50u32;
        let blocks: Vec<crate::core::Block> = (0..10_000u64)
            .map(|i| {
                blk(
                    i + 2,
                    None,
                    i,
                    BlockKind::Paragraph,
                    false,
                    "Pack my box with five dozen liquor jugs, then verify rendering.",
                )
            })
            .collect();
        let at = blocks[0].text.find("five").unwrap();
        let mut hits = FindHits::new();
        for b in blocks.iter().step_by(10) {
            hits.entry(b.id.0 as i32)
                .or_default()
                .push((at, at + 4));
        }
        let total = hits.len();
        let time = |hits: &FindHits| {
            let t = Instant::now();
            for _ in 0..rounds {
                std::hint::black_box(super::project_blocks(&blocks, hits, &super::MentionTitles::empty()).len());
            }
            t.elapsed().as_secs_f64() * 1e3 / rounds as f64
        };
        let (clean, marked) = (time(&FindHits::new()), time(&hits));
        let t = Instant::now();
        for _ in 0..rounds {
            for id in hits.keys() {
                std::hint::black_box(super::row_id_of(&blocks, BlockId(*id as u64)));
            }
        }
        let walk = t.elapsed().as_secs_f64() * 1e3 / rounds as f64;
        println!(
            "find: {total} hits on 10 000 rows — projection {clean:.3} ms clean, \
             {marked:.3} ms marked (+{:.3} ms); the per-hit row walk alone {walk:.3} ms",
            marked - clean
        );
    }

    /// A run is one flex cell, and a cell cannot break — so the unmarked
    /// stretches of a marked line are cut to a word each, which is where a
    /// marked paragraph wraps (ADR-0041). Marked stretches stay whole: an
    /// underline or code box split at every space is worse than the long bold
    /// phrase it cannot break, and a link's click target has to stay one run.
    #[test]
    fn a_marked_line_is_cut_to_words_between_the_marks() {
        use crate::core::{Mark, MarkKind};
        let text = "one two three bold words four five";
        let span = |s: &str| text.find(s).unwrap();
        let marks = [Mark {
            start: span("bold"),
            end: span("bold") + "bold words".len(),
            kind: MarkKind::Bold,
            url: String::new(),
            date: None,
        }];
        let runs = super::build_runs(text, &marks, &[], &super::MentionTitles::empty());
        let cells: Vec<&str> = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            cells,
            vec!["one ", "two ", "three ", "bold words", " four ", "five"],
            "the marked stretch stays one cell, the plain stretches do not"
        );
        // only a stretch that opens right after a mark carries its space, and
        // that is one cell wide — the words inside a stretch start clean
        assert!(cells[1..4].iter().all(|c| !c.starts_with(' ')));
        assert_eq!(
            runs.iter()
                .map(|r| (r.bold, r.italic, r.strike, r.code, r.link))
                .filter(|f| *f != (false, false, false, false, false))
                .count(),
            1,
            "a mark leaked onto a plain word"
        );
        // cutting is a re-join, never a rewrite
        let back: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(back, text);
    }

    /// Two shapes the cut must survive: a mark that opens the line, and text
    /// whose words are not ASCII.
    #[test]
    fn the_word_cut_joins_back_to_the_text_it_came_from() {
        use crate::core::{Mark, MarkKind};
        for (text, marks) in [
            (
                "code here and  more",
                vec![Mark {
                    start: 0,
                    end: 4,
                    kind: MarkKind::Code,
                    url: String::new(),
                    date: None,
                }],
            ),
            (
                "写作与中文测试 link 结尾",
                vec![Mark {
                    start: "写作与中文测试 ".len(),
                    end: "写作与中文测试 link".len(),
                    kind: MarkKind::Italic,
                    url: String::new(),
                    date: None,
                }],
            ),
            // a mark whose end is the end of the line leaves no tail to cut
            (
                "tail only",
                vec![Mark {
                    start: 5,
                    end: 9,
                    kind: MarkKind::Strike,
                    url: String::new(),
                    date: None,
                }],
            ),
        ] {
            let runs = super::build_runs(text, &marks, &[], &super::MentionTitles::empty());
            let back: String = runs.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(back, text, "the runs of {text:?} do not re-join");
            assert!(
                runs.iter().all(|r| !r.text.is_empty()),
                "an empty cell in {text:?} costs an item and paints nothing"
            );
        }
    }

    /// The find bar's cell (A4 D7): every hit is a boundary, so the cell it
    /// makes is exactly one occurrence and the cells around it are not. A hit
    /// that lands mid-word still splits the word, because a cell is the
    /// smallest thing this layout can paint — a cell painting "part of a
    /// match" would be the same defect with a tint on it.
    #[test]
    fn a_hit_is_its_own_cell_and_no_cell_is_part_of_a_hit() {
        fn cells(runs: &[crate::TextRun]) -> Vec<&str> {
            runs.iter().map(|r| r.text.as_str()).collect()
        }
        // a needle's byte span, counted from `from` so a repeat is addressable
        let span = |text: &str, needle: &str, from: usize| {
            let s = text[from..].find(needle).unwrap() + from;
            (s, s + needle.len())
        };

        // No marks at all, yet the row has to be runs: an empty vec means
        // "paint this as one unbroken Text", which cannot show a match.
        let text = "alpha beta gamma beta";
        let runs = super::build_runs(text, &[], &[span(text, "beta", 0), span(text, "beta", 11)], &super::MentionTitles::empty());
        assert_eq!(
            cells(&runs),
            vec!["alpha ", "beta", " gamma ", "beta"],
            "the two matches are two cells, the plain stretches are words"
        );
        let flagged: Vec<&str> = runs
            .iter()
            .filter(|r| r.hit)
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(flagged, vec!["beta", "beta"], "a tint leaked, or a match went unpainted");

        // a mid-word hit cuts the word it sits in
        let text = "unforgettable";
        let runs = super::build_runs(text, &[], &[span(text, "forget", 0)], &super::MentionTitles::empty());
        assert_eq!(cells(&runs), vec!["un", "forget", "table"]);
        assert!(runs[1].hit && !runs[0].hit && !runs[2].hit);
        let back: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(back, text);

        // a hit inside a mark moves to the mark, which is never cut: the cell
        // is one Text, and a formula's glyphs are not its source bytes
        let text = "see the bold words now";
        let bold = span(text, "bold", 0);
        let marks = [crate::core::Mark {
            start: bold.0,
            end: bold.1 + " words".len(),
            kind: crate::core::MarkKind::Bold,
            url: String::new(),
            date: None,
        }];
        let runs = super::build_runs(text, &marks, &[span(text, "old wor", 0)], &super::MentionTitles::empty());
        assert_eq!(
            cells(&runs),
            vec!["see ", "the ", "bold words", " now"],
            "a hit cut a marked stretch in two"
        );
        assert!(runs[2].hit, "the mark the hit landed in is the cell");
        assert!(runs[2].bold);
    }

    /// SPEC §四十: a mention's label is not in the block. The span holds the
    /// title as it was typed, and what is drawn is what the target page is
    /// called *now* — the entire mechanism behind "页面别名", and the reason
    /// the projection is handed titles instead of reading the characters.
    #[test]
    fn a_mention_run_reads_the_live_title_and_degrades_when_the_page_is_gone() {
        use crate::core::{BlockKind, Mark, MarkKind, PageId};
        use slint::Model as _;
        let mut b = blk(1, None, 10, BlockKind::Paragraph, false, "see Project Atlas");
        b.marks = vec![Mark {
            start: 4,
            end: 17,
            kind: MarkKind::Mention,
            url: crate::core::page_uri(PageId(12)),
            date: None,
        }];
        let blocks = vec![b];

        // the page is there: the chip reads its *current* title, and the run
        // still carries the address a click needs
        let live = super::MentionTitles::of_blocks(&blocks, |_| Some("Atlas (renamed)".into()));
        let row = &super::project_blocks(&blocks, &FindHits::new(), &live)[0];
        let chip = row.runs.row_data(1).unwrap();
        assert_eq!(chip.text, "Atlas (renamed)");
        assert_eq!(chip.mention, 12);
        assert!(!chip.mention_deleted);
        assert!(!chip.link, "a mention is not a link: it draws its own chip");
        assert_eq!(chip.url, "quire://page/12", "no address, nowhere to go");

        // the page is gone: the atom stays visible and says so
        let gone = super::MentionTitles::of_blocks(&blocks, |_| None);
        let row = &super::project_blocks(&blocks, &FindHits::new(), &gone)[0];
        let dead = row.runs.row_data(1).unwrap();
        assert_eq!(dead.text, super::DELETED_PAGE_LABEL);
        assert!(dead.mention_deleted);
        assert_eq!(dead.mention, 12, "the id outlives the page it names");

        // a projection that collected no titles keeps the stored characters
        let row =
            &super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty())[0];
        let plain = row.runs.row_data(1).unwrap();
        assert_eq!(plain.text, "Project Atlas");
        assert!(!plain.mention_deleted);
    }

    /// SPEC §四十: a date's payload *is* its text, so the run needs no lookup —
    /// and it must not be read as a mention, which is exactly what its empty
    /// address says.
    #[test]
    fn a_date_run_carries_its_own_text_and_no_address() {
        use crate::core::{BlockKind, Mark, MarkKind};
        use slint::Model as _;
        let mut b = blk(1, None, 10, BlockKind::Paragraph, false, "due 2026-09-22 ok");
        b.marks = vec![Mark {
            start: 4,
            end: 14,
            kind: MarkKind::Date,
            url: String::new(),
            date: Some("2026-09-22".into()),
        }];
        let row = &super::project_blocks(&[b], &FindHits::new(), &super::MentionTitles::empty())[0];
        let atom = row.runs.row_data(1).unwrap();
        assert!(atom.date);
        assert_eq!(atom.text, "2026-09-22");
        assert_eq!(atom.mention, -1, "a date is not a mention");
        assert!(!atom.mention_deleted);
        assert!(atom.url.is_empty(), "a date points at nothing");
    }

    /// The panel's two lines that are not rows. Both are decisions Slint cannot
    /// make, which is why they are made here: a plural, and how much of the
    /// answer the folded list is *not* drawing.
    #[test]
    fn the_panel_says_how_many_and_how_many_it_is_not_showing() {
        use super::{backlink_count_label, backlink_fold_label, BACKLINK_WINDOW};
        assert_eq!(backlink_count_label(0), "", "nothing to count, nothing to say");
        assert_eq!(backlink_count_label(1), "1 reference", "one is not 'one references'");
        assert_eq!(backlink_count_label(7), "7 references");
        // a page quoted fewer times than the window: no control at all, because
        // folding would hide nothing and a dead control is worse than none
        assert_eq!(backlink_fold_label(3, 3, false), "");
        assert_eq!(backlink_fold_label(BACKLINK_WINDOW, BACKLINK_WINDOW, false), "");
        // quoted more: the folded line is the rest of the answer
        assert_eq!(backlink_fold_label(200, BACKLINK_WINDOW, false), "and 195 more");
        // unfolded, the way back — offered while the unfolded list is still a
        // window, so a page quoted 200 times can fold again rather than pretend
        assert_eq!(backlink_fold_label(6, BACKLINK_WINDOW, true), "Show less");
        assert_eq!(backlink_fold_label(200, 50, true), "Show less");
        assert_eq!(backlink_fold_label(3, 3, true), "");
    }

    /// SPEC §四十 + T2.4: three ways a reference can go stale, and the two that
    /// are *visible*. A mention stores an id, so deleting the page rewrites
    /// nothing — which is exactly why the projector has to notice by itself,
    /// and why "the chip still shows the old title" is a bug rather than a
    /// cosmetic lag: nothing on screen would ever correct it.
    ///
    /// (The third way — a target that this library never had, from a hand-edited
    /// database — is the same code path as the deletion: a page the workspace
    /// cannot name. Both read as one string, because it is one fact.)
    #[test]
    fn deleting_a_page_degrades_the_chip_that_named_it() {
        use crate::core::{BlockKind, PageId};
        use slint::Model as _;

        let state = super::AppState::new(&plain_args(), None);
        let doomed = state.create_page(None);
        state.rename_page(doomed, "Doomed page");
        let source = state.create_page(None);
        assert_ne!(source, doomed);
        let line = state.exec_on_open_page(crate::core::Command::AppendBlock {
            kind: BlockKind::Paragraph,
            text: String::new(),
        }).as_deref().and_then(|changes| {
            changes.iter().find_map(|c| match c {
                crate::core::Change::BlockInserted(b) => Some(b.id.0 as i32),
                _ => None,
            })
        }).expect("a line to write on");
        assert!(state.apply_mention(line, 0, Some(doomed), "Doomed page"));

        // alive: the chip reads the page's own name
        let chip = |st: &super::AppState| {
            st.blocks
                .row_data(0)
                .expect("the line is the page's first row")
                .runs
                // the line is nothing but the atom, so there is no plain run
                // before it to skip
                .row_data(0)
                .expect("the chip is the line's only run")
        };
        let alive = chip(&state);
        assert_eq!(alive.text, "Doomed page");
        assert_eq!(alive.mention, doomed);
        assert!(!alive.mention_deleted);

        // the page goes, and the chip *on screen* has to say so — this is the
        // assertion that fails if the deletion forgets to reproject
        assert!(!state.delete_page(doomed), "the open page was not the one deleted");
        assert_eq!(state.open_page.get(), source);
        let dead = chip(&state);
        assert_eq!(dead.text, super::DELETED_PAGE_LABEL);
        assert!(dead.mention_deleted);
        assert_eq!(dead.mention, doomed, "the id outlives the name it pointed at");
        assert_eq!(
            dead.url,
            crate::core::page_uri(PageId(doomed as u32 as u64)).as_str(),
            "and the address is unchanged: nothing was rewritten, only re-read"
        );
        // the workspace really has no such page any more, which is what makes
        // the string above the honest one
        assert!(state.workspace.borrow().title_of(doomed).is_none());
    }

    /// T2.4 · 页面别名与悬空引用. SPEC §四十 offers one sentence as a feature —
    /// "因为引用存的是 ID，重命名后所有引用自然显示新标题" — and there is nothing
    /// to build to get it: storing an id instead of a title already means every
    /// surface reads the name at draw time. What is owed instead is **the proof
    /// that all four surfaces do**, pinned at once so that a future one cannot
    /// quietly take a copy: the mention chip, the sidebar row, the Markdown
    /// export, and the block's own stored bytes staying untouched.
    ///
    /// The same mechanism then has three ways of *losing* its page, and the
    /// brief asks for each to be visible rather than silent: **deleted**,
    /// **moved under another parent** (not a loss at all — which is the point),
    /// and an **id that names nothing here** (a hand-edited file, a backup whose
    /// other half never came back).
    #[test]
    fn renaming_moving_and_losing_the_page_a_reference_points_at() {
        use crate::core::{BlockId, BlockKind};
        use crate::services::export_service::export_page_with;
        use slint::Model as _;

        let state = super::AppState::new(&plain_args(), None);
        let atlas = state.create_page(None);
        state.rename_page(atlas, "Project Atlas");
        // a source page to write the mention into. Creating a page also opens
        // it, which is why its id is kept: everything below has to come back
        // here after another page is made.
        let source = state.create_page(None);
        let line = |st: &super::AppState, text: &str| -> i32 {
            st.exec_on_open_page(crate::core::Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: text.to_string(),
            })
            .as_deref()
            .and_then(|cs| {
                cs.iter().find_map(|c| match c {
                    crate::core::Change::BlockInserted(b) => Some(b.id.0 as i32),
                    _ => None,
                })
            })
            .expect("a line to write on")
        };
        let first = line(&state, "");
        assert!(state.apply_mention(first, 0, Some(atlas), "Project Atlas"));

        // the chip of a given row, whoever else is on the page with it
        let chip = |id: i32| {
            let st = &state;
            let rows: Vec<i32> = (0..st.blocks.row_count())
                .filter_map(|i| st.blocks.row_data(i).map(|r| r.id))
                .collect();
            (0..st.blocks.row_count())
                .find(|i| st.blocks.row_data(*i).map(|r| r.id == id).unwrap_or(false))
                .and_then(|i| st.blocks.row_data(i).and_then(|r| r.runs.row_data(0)))
                .unwrap_or_else(|| panic!("the line's first run is its chip; rows={rows:?}, want={id}"))
        };
        let stored = || {
            let d = state.doc.borrow();
            let b = d.block(BlockId(first as u64)).expect("the line is still there");
            (b.text.clone(), b.marks.len(), b.marks[0].url.clone())
        };

        // ── 1 · rename ───────────────────────────────────────────────────────
        let before = stored();
        assert_eq!(chip(first).text, "Project Atlas");
        state.rename_page(atlas, "Atlas 2026");

        // a: the chip — what is drawn changes although nothing was rewritten
        assert_eq!(chip(first).text, "Atlas 2026");
        assert!(!chip(first).mention_deleted, "a new name is not a loss");
        // b: the block is byte-identical. This is the sentence "所有引用自然显示
        // 新标题" costs nothing: renaming a page quoted from 3 000 places writes
        // to exactly one page.
        assert_eq!(stored(), before, "no reference was rewritten");
        // c: the sidebar row is the page's own current title too
        let sidebar_of = |id: i32| {
            (0..state.sidebar.row_count())
                .filter_map(|i| state.sidebar.row_data(i))
                .find(|n| n.id == id)
                .unwrap_or_else(|| panic!("page {id} has a sidebar row"))
        };
        assert_eq!(sidebar_of(atlas).label, "Atlas 2026");
        // d: and so is the export, which reads the workspace rather than the run
        let md = {
            let d = state.doc.borrow();
            let blocks = d.page_blocks(crate::core::PageId(source as u32 as u64));
            export_page_with(&blocks, &|id| {
                state.workspace.borrow().title_of(id.0 as i32).map(str::to_string)
            },
            &|_| None)
        };
        assert!(md.contains("Atlas 2026"), "the Markdown names it as it is now\n{md}");
        assert!(!md.contains("Project Atlas"), "and not as it was when typed\n{md}");

        // ── 2 · moved under another parent ───────────────────────────────────
        // A move reparents the page; it does not become a different page. The
        // id is the whole reference, so every chip that pointed at it still
        // does, and this is where a "title + parent path" store would break.
        let archive = state.create_page(None);
        state.rename_page(archive, "Archive");
        assert!(state.move_page(atlas, Some(archive)), "the move happens");
        // back to the page the chip is on — making Archive moved the caret away
        state.open_page(source);
        assert!(
            state
                .workspace
                .borrow()
                .children_of(Some(archive))
                .contains(&atlas),
            "it really is a child now"
        );
        assert_eq!(chip(first).text, "Atlas 2026");
        assert!(!chip(first).mention_deleted);
        assert_eq!(stored(), before, "and still nothing was rewritten");

        // ── 3 · an id that names nothing in this library ─────────────────────
        // The realistic route in is not a broken write but a restored file: the
        // payload is well-formed, and the page it names simply is not here.
        let ghost = atlas + 900_000;
        let second = line(&state, "");
        assert!(state.apply_mention(second, 0, Some(ghost), "Ghost page"));
        let orphan = chip(second);
        assert_eq!(
            orphan.text,
            super::DELETED_PAGE_LABEL,
            "the chip says so instead of naming a page that is not here"
        );
        assert!(orphan.mention_deleted);
        assert_eq!(orphan.mention, ghost, "and keeps the id a repair would want");
        // the deletion route lands in the same state, which is the point of the
        // two being one rule rather than two cases
        state.delete_page(atlas);
        let dead = chip(first);
        assert_eq!(dead.text, super::DELETED_PAGE_LABEL);
        assert!(dead.mention_deleted);
        assert_eq!(dead.mention, atlas, "still the id it was written with");
    }

    /// What the backlink panel costs a page open (SPEC §四十, ADR-0051).
    ///
    /// The panel is a read on the *interactive* path — it runs on every
    /// projection of a page — so the question is not "how fast is one query",
    /// it is "does opening a page cost more because the library is bigger".
    /// Four arms, one database, one sitting, release:
    ///
    /// | arm | library | page opened |
    /// |-----|---------|-------------|
    /// | A | 1 200 pages, 100 000 marks that are **not** mentions | nothing points at it |
    /// | B | the same | 200 references |
    /// | C | the same, with migration 16's `idx_marks_reference` dropped | 200 references |
    /// | D | the same | 200 references, panel unfolded |
    ///
    /// **C is the control the claim needs.** Migration 16 exists so that the
    /// reference read is an index seek rather than a scan of `marks`; taking
    /// the index away and running the same query is what says how much of the
    /// sentence is the index and how much is the machine. It is not a
    /// configuration the app ships — it is the counterfactual that "no full
    /// scan" is a statement about.
    /// A mirror draws what its source holds, and the row keeps drawing the page
    /// after the source is gone (ADR-0052 §1 and §2).
    ///
    /// The two halves that matter are the ones a bug would hide: the words come
    /// from **another page** (so nothing on this page's slices could have
    /// answered it), and deleting the source is a `DeleteBlock` on the source's
    /// own page, after which the mirror is still a row that says something.
    /// The id of the block an append inserted, read back out of the
    /// command's own changes: `exec_on_open_page` answers in changes,
    /// and a test that wants to aim the next command at the new row
    /// needs its id, not its text.
    fn find_inserted_block_id(changes: &[Change]) -> Option<i32> {
        changes.iter().find_map(|c| match c {
            Change::BlockInserted(b) => Some(b.id.0 as i32),
            _ => None,
        })
    }

    #[test]
    fn a_mirror_draws_its_source_and_degrades_when_the_source_goes() {
        use crate::core::{BlockId, BlockKind, Command};
        use slint::Model as _;

        let state = super::AppState::new(&plain_args(), None);
        let source_page = state.create_page(None);
        state.rename_page(source_page, "Source page");
        let appended = state.exec_on_open_page(Command::AppendBlock {
            kind: BlockKind::Paragraph,
            text: "The words live here, and nowhere else.".to_string(),
        });
        let source = appended
            .as_deref()
            .and_then(find_inserted_block_id)
            .expect("a source line");

        // the page that will hold the mirror — created second, so it is open
        let home = state.create_page(None);
        state.rename_page(home, "Mirror page");
        let mirror = state
            .exec_on_open_page(Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: String::new(),
            })
            .as_deref()
            .and_then(find_inserted_block_id)
            .expect("a line to become the mirror");
        state.exec_on_open_page(Command::SetBlockType {
            id: BlockId(mirror as u64),
            kind: BlockKind::Synced,
        });
        assert!(state.set_sync_source(mirror, Some(source)), "the pointer lands");

        let row_of = |id: i32| {
            (0..state.blocks.row_count())
                .filter_map(|i| state.blocks.row_data(i))
                .find(|r| r.id == id)
                .unwrap_or_else(|| panic!("row {id} is on screen"))
        };

        // ── 1 · the words, from a page that is not this one ──────────────────
        let row = row_of(mirror);
        assert_eq!(row.kind, super::BLOCK_SYNCED);
        assert_eq!(row.text, "The words live here, and nowhere else.");
        assert_eq!(row.sync_source, source, "the row names the block it draws");
        // and the block itself still owns nothing
        assert!(state
            .doc
            .borrow()
            .block(BlockId(mirror as u64))
            .map(|b| b.text.is_empty())
            .unwrap_or(false),
            "a mirror holding words would have two owners of one sentence");

        // ── 2 · the source is deleted, on its own page ───────────────────────
        state.open_page(source_page);
        state.exec_on_open_page(Command::DeleteBlock {
            id: BlockId(source as u64),
        });
        state.open_page(home);

        let row = row_of(mirror);
        assert_eq!(row.text, super::DELETED_SOURCE_LABEL);
        // -1 is what makes the row read-only: an edit aimed at a source nobody
        // can find has nowhere honest to land (§2)
        assert_eq!(row.sync_source, -1);
        assert_eq!(state.content_of(mirror), -1, "and Rust agrees with the row");
        // the page still projects — a dangling mirror is a picture, not a crash
        assert!(state.blocks.row_count() > 0);
    }

    /// The cycle refusal, which is the one question in this slice that has to be
    /// answered **before** the pointer is written (§三十九's rule, §4 of ADR-0052).
    ///
    /// Both directions are pinned: pointing at yourself, and pointing at a block
    /// that already mirrors you. The third case — a chain longer than the bound —
    /// is deliberately *not* pinned here, because "a file somebody edited by
    /// hand" is not a state this test can build honestly.
    #[test]
    fn a_mirror_refuses_to_point_at_itself_or_close_a_cycle() {
        use crate::core::{BlockId, BlockKind, Command};

        let state = super::AppState::new(&plain_args(), None);
        let home = state.create_page(None);
        let line = |st: &super::AppState| -> i32 {
            st.exec_on_open_page(Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: String::new(),
            })
            .as_deref()
            .and_then(find_inserted_block_id)
            .expect("a line")
        };
        let make_mirror = |st: &super::AppState, id: i32| {
            st.exec_on_open_page(Command::SetBlockType {
                id: BlockId(id as u64),
                kind: BlockKind::Synced,
            });
        };

        let a = line(&state);
        let b = line(&state);
        make_mirror(&state, a);
        make_mirror(&state, b);

        // ── 1 · itself ───────────────────────────────────────────────────────
        assert!(!state.set_sync_source(a, Some(a)), "nobody mirrors themselves");

        // ── 2 · the two-hop cycle: b already mirrors a ────────────────────────
        assert!(state.set_sync_source(b, Some(a)), "b mirrors a");
        assert!(
            !state.set_sync_source(a, Some(b)),
            "a -> b -> a is refused rather than drawn once per frame forever"
        );
        // and the refusal changed nothing: b still mirrors a
        assert_eq!(
            state.doc.borrow().block(BlockId(b as u64)).and_then(|x| x.sync_ref),
            Some(BlockId(a as u64))
        );

        // ── 3 · a pointer onto a block that is not a mirror is refused ────────
        let plain = line(&state);
        assert!(
            !state.set_sync_source(plain, Some(a)),
            "sync_ref means one thing; keeping it that way is a refusal"
        );
        let _ = home;
    }

    #[test]
    #[ignore = "prints a timing; run with --release"]
    fn cost_of_the_backlink_panel_on_a_page_open() {
        use crate::core::{
            Block, BlockId, BlockKind, Change, Mark, MarkKind, OrderKey, Page, PageFont, PageId,
        };
        use crate::core::persistence::Repository as _;
        use crate::testing::ScratchDir;
        use slint::Model as _;
        use std::time::Instant;
        use super::core_page_id;

        const PAGES: u64 = 1_200;
        const REFERRERS: u64 = 200;
        const BLOAT_BLOCKS: u64 = 1_000;
        const BLOAT_MARKS: usize = 100;
        const ROUNDS: u32 = 20;
        const WINDOW: usize = 5;

        let page_row = |id: u64, title: String| Page {
            id: PageId(id),
            title,
            parent: None,
            // far apart, so the tree order is the id order and nothing clumps
            order: OrderKey(id * 0x1_0000),
            favorite: false,
            expanded: false,
            font: PageFont::default(),
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: false,
        };
        let block_row = |id: u64, page: u64, text: &str| Block {
            id: BlockId(id),
            page: PageId(page),
            parent: None,
            order: OrderKey::FIRST,
            kind: BlockKind::Paragraph,
            text: text.into(),
            checked: false,
            marks: Vec::new(),
            color: crate::core::ColorKind::Default,
            background: crate::core::ColorKind::Default,
            page_ref: None,
            folded: false,
            attachment: None,
            img_percent: 100,
            columns: 0,
            lang: Lang::Plain,
            db_ref: None,
            sync_ref: None,        };

        let dir = ScratchDir::new("backlink-cost");
        let repo = scratch_repo(&dir);
        let target = PAGES as i32; // the page that is quoted, by REFERRERS of them
        let plain = PAGES as i32 - 1; // and one nobody quotes, in the same library

        let mut batch: Vec<Change> = Vec::new();
        let mut mark_sets: Vec<(u64, Vec<Mark>)> = Vec::new();
        for i in 1..=PAGES {
            batch.push(Change::PageCreated(page_row(i, format!("Page {i}"))));
            let bid = 1_000_000 + i;
            let (text, marks) = if i <= REFERRERS {
                (
                    "See Project Atlas".to_string(),
                    vec![Mark {
                        start: 4,
                        end: 17,
                        kind: MarkKind::Mention,
                        url: crate::core::page_uri(PageId(PAGES)),
                        date: None,
                    }],
                )
            } else if i <= REFERRERS + BLOAT_BLOCKS {
                // the table filler: real marks on real lines, none of them a
                // reference to anything
                (
                    "word ".repeat(BLOAT_MARKS),
                    (0..BLOAT_MARKS)
                        .map(|k| Mark {
                            start: k * 5,
                            end: k * 5 + 4,
                            kind: MarkKind::Bold,
                            url: String::new(),
                            date: None,
                        })
                        .collect(),
                )
            } else {
                ("a line of its own".to_string(), Vec::new())
            };
            batch.push(Change::BlockInserted(block_row(bid, i, &text)));
            if !marks.is_empty() {
                mark_sets.push((bid, marks));
            }
        }
        let total_marks: usize = mark_sets.iter().map(|(_, m)| m.len()).sum();
        for (id, marks) in mark_sets {
            batch.push(Change::BlockMarksSet {
                id: BlockId(id),
                marks,
            });
        }
        let t = Instant::now();
        repo.apply(&batch).unwrap();
        let seeded = t.elapsed().as_secs_f64();
        assert_eq!(
            repo.load().unwrap().pages.len(),
            PAGES as usize,
            "the fixture really is the library it claims to be"
        );
        println!(
            "seeded {PAGES} pages / {total_marks} marks in {seeded:.2} s \
             ({REFERRERS} of them references to page {target})"
        );

        // One read of the pair `refresh_backlinks` makes: the count, then the
        // folded window. Microseconds, so the panel's own cost is not lost in
        // the rounding of a millisecond.
        let read = |repo: &crate::storage::SqliteRepository, page: i32, window: usize| {
            let t = Instant::now();
            let mut seen = 0usize;
            for _ in 0..ROUNDS {
                let n = repo.reference_count(core_page_id(page)).unwrap();
                let rows = repo.references(core_page_id(page), window).unwrap();
                seen += n + rows.len();
            }
            std::hint::black_box(seen);
            t.elapsed().as_secs_f64() * 1e6 / ROUNDS as f64
        };

        // The whole open, through the same call the UI makes — the panel's read
        // is inside it, which is the only way the number means "opening a page".
        let state = super::AppState::new(&plain_args(), Some(repo.clone()));
        let open_ms = |page: i32| {
            let t = Instant::now();
            for _ in 0..ROUNDS {
                state.open_page(page);
            }
            std::hint::black_box(state.backlinks.row_count());
            t.elapsed().as_secs_f64() * 1e3 / ROUNDS as f64
        };

        let a_read = read(&repo, plain, WINDOW);
        let b_read = read(&repo, target, WINDOW);
        let d_read = read(&repo, target, 50);
        let a_open = open_ms(plain);
        let b_open = open_ms(target);
        state.toggle_backlinks();
        let d_open = open_ms(target);

        // the counterfactual: the same two reads with the index migration 16
        // built taken away. Not a configuration the app ships.
        let conn = rusqlite::Connection::open(repo.path().unwrap()).unwrap();
        conn.execute("DROP INDEX idx_marks_reference", []).unwrap();
        let c_read = read(&repo, target, WINDOW);
        let c_count: usize = {
            let t = Instant::now();
            std::hint::black_box(repo.reference_count(core_page_id(target)).unwrap());
            t.elapsed().as_micros() as usize
        };
        conn.execute(
            "CREATE INDEX idx_marks_reference ON marks(kind, url)",
            [],
        )
        .unwrap();
        drop(conn);

        println!(
            "reference read (count + window), µs/read over {ROUNDS} rounds:\n  \
             A page nothing points at   {a_read:8.1}\n  \
             B 200 references, folded   {b_read:8.1}\n  \
             D 200 references, unfolded {d_read:8.1}\n  \
             C B with the index dropped {c_read:8.1}   <- counterfactual, no index"
        );
        println!(
            "open_page, ms/open over {ROUNDS} rounds:\n  \
             A nothing points at it     {a_open:8.2}\n  \
             B 200 references, folded   {b_open:8.2}\n  \
             D 200 references, unfolded {d_open:8.2}"
        );
        println!(
            "the panel's own share: A {:.2} ms, B {:.2} ms, D {:.2} ms (open minus open)",
            a_open - a_open,
            b_open - a_open,
            d_open - a_open
        );
        println!("first count on the unindexed table: {c_count} µs");
    }

    /// SPEC §四十 lists "all the blocks that reference this page", and T2.4
    /// says a rename has to follow everywhere. So the panel is read end to end
    /// over a real library: four references from two pages, one of them a block
    /// that *is* a reference rather than a mention inside one, every row
    /// carrying the source block's own text — and then the source page is
    /// renamed and the panel says the new name with nothing rewritten.
    #[test]
    fn the_backlink_panel_groups_by_page_and_names_each_page_as_it_is_called_now() {
        use crate::testing::ScratchDir;
        use slint::Model as _;

        let dir = ScratchDir::new("backlinks-state");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        // the page that gets quoted, and two pages that quote it
        let target = state.create_page(None);
        let notes = state.create_page(None);
        // three references on `notes`: two mentions in prose, and one link
        let a = state.start_page().expect("notes takes a line");
        assert!(state.apply_mention(a, 0, Some(target), "Project Atlas"));
        let b = state.start_page().expect("notes takes a second line");
        assert!(state.apply_mention(b, 0, Some(target), "Project Atlas"));
        let c = state.start_page().expect("notes takes a third line");
        assert!(state.create_page_link_block(c, target), "the line *becomes* a link");
        // and one on a second page, so the grouping has two groups to make
        let index = state.create_page(None);
        let d = state.start_page().expect("index takes a line");
        assert!(state.apply_mention(d, 0, Some(target), "Project Atlas"));

        // The panel is derived from the database, so the writes have to land
        // before the page that reads them is opened — the debounce is the only
        // thing between the two in the app, and it is not part of the claim.
        state.persistence_force_flush();
        state.open_page(target);

        let rows: Vec<crate::BacklinkRow> = (0..state.backlinks.row_count())
            .map(|i| state.backlinks.row_data(i).unwrap())
            .collect();
        assert_eq!(rows.len(), 4, "every reference, and nothing that is not one");
        assert_eq!(
            rows.iter().filter(|r| r.first_on_page).count(),
            2,
            "one group header per source page"
        );
        assert_eq!(
            rows.iter().filter(|r| r.block_level).count(),
            1,
            "only the link is a block-level reference"
        );
        // grouped: a page's rows are contiguous, which is what lets the first
        // one carry the header and the rest go without
        let pages: Vec<i32> = rows.iter().map(|r| r.page).collect();
        let distinct = {
            let mut p = pages.clone();
            p.dedup();
            p
        };
        assert_eq!(distinct.len(), 2, "two source pages, contiguous");
        assert!(rows.iter().all(|r| r.page == notes || r.page == index));
        // the quoted text is the *source block's own*, and a mention's row is
        // the line it was typed into
        assert_eq!(rows.iter().filter(|r| r.text == "Project Atlas").count(), 3);
        assert!(
            rows.iter().all(|r| r.title == "Untitled" || !r.title.is_empty()),
            "a row can always name the page it came from"
        );

        // T2.4: rename the source page and the panel follows, because it asked
        // the workspace rather than a copy taken at reference time
        state.rename_page(notes, "Meeting notes");
        state.open_page(index);
        state.open_page(target);
        let rows: Vec<crate::BacklinkRow> = (0..state.backlinks.row_count())
            .map(|i| state.backlinks.row_data(i).unwrap())
            .collect();
        assert!(
            rows.iter().any(|r| r.title == "Meeting notes"),
            "the group header reads the name the page has now"
        );
        assert!(
            rows.iter().filter(|r| r.title == "Meeting notes").count() == 3,
            "every row of that group moved with the name"
        );
    }

    /// The hits reach all three places a line is drawn: a row, a grid cell,
    /// and a block inside a columns box. A match in a grid has no row of its
    /// own, so it rides on the row that paints it.
    #[test]
    fn the_projection_carries_hits_into_rows_cells_and_columns() {
        use crate::core::BlockKind;
        use slint::Model;
        let mut blocks = grid_scene();
        let layout = blk(20, None, 400, BlockKind::Columns, false, "");
        let left = blk(21, Some(20), 410, BlockKind::Column, false, "");
        let para = blk(22, Some(21), 420, BlockKind::Paragraph, false, "needle in a column");
        for (id, text) in [(1u64, "needle before"), (4, "needle in a cell")] {
            if let Some(b) = blocks.iter_mut().find(|b| b.id.0 == id) {
                b.text = text.into();
            }
        }
        blocks.extend([layout, left, para]);
        let mut hits: FindHits = FindHits::new();
        // "needle" opens all three texts, so the span is the same for each
        for id in [1u64, 4, 22] {
            hits.entry(id as i32).or_default().push((0, 6));
        }
        let rows = super::project_blocks(&blocks, &hits, &super::MentionTitles::empty());

        let hit_cells = |runs: &slint::ModelRc<crate::TextRun>| -> Vec<String> {
            (0..runs.row_count())
                .filter_map(|i| runs.row_data(i))
                .filter(|r| r.hit)
                .map(|r| r.text.to_string())
                .collect()
        };
        assert_eq!(
            hit_cells(&rows[0].runs),
            vec!["needle".to_string()],
            "a paragraph's hit is a cell of its own"
        );
        // the grid has no row for its cell, so the cell list carries it
        let grid = rows.iter().find(|r| r.id == 8).unwrap();
        let painted: Vec<String> = (0..grid.table_cells.row_count())
            .filter_map(|i| grid.table_cells.row_data(i))
            .flat_map(|c| hit_cells(&c.runs))
            .collect();
        assert_eq!(painted, vec!["needle".to_string()], "a hit in a cell never reached it");
        // and so does a columns box, whose blocks the delegate lays out itself
        let boxes = rows.iter().find(|r| r.id == 20).unwrap();
        let painted: Vec<String> = (0..boxes.column_items.row_count())
            .filter_map(|i| boxes.column_items.row_data(i))
            .flat_map(|i| hit_cells(&i.runs))
            .collect();
        assert_eq!(
            painted,
            vec!["needle".to_string()],
            "a hit in a column never reached it"
        );
        // a search that never started leaves a markless line on the
        // single-Text path (cell A1 is the one marked block here)
        let clean = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        assert_eq!(clean[0].runs.row_count(), 0, "a closed bar still splits");
        assert_eq!(
            clean
                .iter()
                .find(|r| r.id == 8)
                .unwrap()
                .table_cells
                .row_count(),
            grid.table_cells.row_count()
        );
        let marked = clean
            .iter()
            .find(|r| r.id == 8)
            .unwrap()
            .table_cells
            .row_data(1)
            .unwrap();
        assert_eq!(hit_cells(&marked.runs), Vec::<String>::new(), "a mark is not a hit");
    }

    /// SPEC §十六 names these two as palette commands, and the id a row
    /// carries is exactly what the dispatch matches on — a drifted row is a
    /// silently dead command, which is how the id >= 9 shadowing bug hid.
    #[test]
    fn the_palette_carries_the_navigation_commands() {
        let cmds = mock_commands(&Workspace::sample());
        for (id, name) in [(CMD_NAV_BACK, "Go Back"), (CMD_NAV_FORWARD, "Go Forward")] {
            let row = cmds.iter().find(|c| c.id == id).expect("row present");
            assert_eq!(row.name.as_str(), name);
            assert_eq!(row.section.as_str(), "Navigate");
        }
    }

    /// The whole registry, walked: every row must resolve to an action of its
    /// own. This is the durable repair for `aaa3763`, where a missing import
    /// turned one `match` arm into a catch-all binding and every row with id
    /// >= 9 ran `Copy Page as Markdown` — green tests, because nothing walked
    /// the registry until now.
    #[test]
    fn every_palette_row_resolves_to_its_own_action() {
        let cmds = mock_commands(&Workspace::sample());
        assert!(!cmds.is_empty());
        let mut actions: Vec<PaletteAction> = Vec::new();
        for row in &cmds {
            let action = palette_action(row.id);
            assert_ne!(
                action,
                PaletteAction::None,
                "row {:?} (id {}) maps to no action",
                row.name.as_str(),
                row.id
            );
            if !matches!(action, PaletteAction::OpenPage(_)) {
                assert!(
                    !actions.contains(&action),
                    "two rows share the action {action:?} — one of them is dead"
                );
                actions.push(action);
            }
        }
        // the ids themselves are unique too, or the palette cannot select one
        let mut ids: Vec<i32> = cmds.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "two palette rows carry the same id");
    }

    #[test]
    fn a_jump_row_carries_its_page_id_and_an_unknown_id_does_nothing() {
        assert_eq!(
            palette_action(CMD_PAGE_BASE + 42),
            PaletteAction::OpenPage(42)
        );
        // nothing between the command block and the page block, and nothing
        // the palette could invent, may reach a handler
        for id in [0, 14, 9_999, -1] {
            assert_eq!(palette_action(id), PaletteAction::None, "id {id}");
        }
    }

    #[test]
    fn back_retraces_and_forward_rewinds() {
        let mut nav = NavHistory::default();
        nav.record(1, 2);
        nav.record(2, 3);
        assert_eq!(nav.step(false, 3, live), Some(2));
        assert_eq!(nav.step(false, 2, live), Some(1));
        // nothing left behind the first page
        assert_eq!(nav.step(false, 1, live), None);
        assert_eq!(nav.step(true, 1, live), Some(2));
    }

    #[test]
    fn a_new_navigation_drops_the_forward_branch() {
        let mut nav = NavHistory::default();
        nav.record(1, 2);
        assert_eq!(nav.step(false, 2, live), Some(1));
        nav.record(1, 9);
        assert_eq!(nav.step(true, 9, live), None);
    }

    #[test]
    fn deleted_pages_are_skipped_not_opened() {
        let mut nav = NavHistory::default();
        nav.record(1, 2);
        // 99 sat in the history and has since been deleted
        nav.record(99, 3);
        assert_eq!(nav.step(false, 3, live), Some(1));
        // the page that was left stays reachable in the direction it came from
        assert_eq!(nav.step(true, 1, live), Some(3));
    }

    #[test]
    fn the_first_open_records_nothing_and_the_stack_stays_bounded() {
        let mut nav = NavHistory::default();
        nav.record(0, 1);
        assert_eq!(nav.step(false, 1, live), None);

        for i in 0..(NAV_MAX + 20) {
            nav.record((i % 9 + 1) as i32, (i % 9 + 2) as i32);
        }
        assert_eq!(nav.back.len(), NAV_MAX);
        for _ in 0..NAV_MAX {
            assert!(nav.step(false, 0, live).is_some());
        }
        assert_eq!(nav.step(false, 0, live), None);
    }

    /// §二十二's promise in one number: a page of photographs must not cost
    /// the session more decoded rasters than the budget allows.
    #[test]
    fn the_picture_cache_spends_its_budget_and_drops_the_stalest_first() {
        use super::{AppState, HandleArgs, MAX_ATTACHMENT_CACHE_BYTES};
        use crate::core::AttachmentId;

        let state = AppState::new(
            &HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 },
            None,
        );
        // No database, so the store is the temp fallback; the high id range
        // keeps this fixture away from anything else writing there.
        let base: i64 = 900_000;

        // One pixel over MAX_EDGE, so the store hands the editor a 1280x720
        // raster = 3.7 MB. Eleven of them want 40 MB against a 32 MB budget.
        const COUNT: i64 = 11;
        let show = |state: &AppState, n: i64| {
            if !state.attachments.borrow().contains_key(&n) {
                let id = (base + n) as u64;
                let att = state.store.create_fixture(AttachmentId(id), 1281, 720).unwrap();
                assert_eq!(att.thumb, format!("{id}.cache.png"), "no downsample to measure");
                state.attachments.borrow_mut().insert(n, att);
            }
            assert!(
                state.image_for(n as i32).size().width > 0,
                "the fixture must really decode, or the cache is being \
                 measured on blanks"
            );
        };
        for n in 1..=COUNT {
            show(&state, n);
        }
        let cached = state.attachment_images.borrow().len();
        assert!(cached < COUNT as usize, "nothing was ever dropped: {cached} rasters");
        let spent = state.attachment_cache_bytes.get();
        assert!(spent <= MAX_ATTACHMENT_CACHE_BYTES, "budget blown by {spent}");
        // control: the survivors are full-size rasters, so the eviction above
        // emptied a real cache rather than a map of zero-weight blanks
        assert!(spent > MAX_ATTACHMENT_CACHE_BYTES / 2, "only {spent} spent");
        for n in (COUNT - 2)..=COUNT {
            assert!(
                state.attachment_images.borrow().contains_key(&n),
                "the newest raster {n} should still be decoded"
            );
        }

        // Re-showing the oldest survivor makes it the freshest thing here, so
        // the next round of evictions has to take 5, 6 and 7 and leave 4 —
        // first-in-first-out would drop 4, and scroll-back would re-decode.
        show(&state, 4);
        for n in 12..15 {
            show(&state, n);
        }
        assert!(state.attachment_images.borrow().contains_key(&4), "4 was just on screen");
        assert!(!state.attachment_images.borrow().contains_key(&5), "5 is the stalest");

        for n in 1..15 {
            let id = (base + n) as u64;
            std::fs::remove_file(state.store.dir().join(format!("{id}.png"))).ok();
            std::fs::remove_file(state.store.dir().join(format!("{id}.cache.png"))).ok();
        }
    }

    /// The plan is shared by the row builder and the seeder so neither can
    /// drift: `stride` is how far apart the picture rows sit, `pool` how many
    /// fixtures back them.
    #[test]
    fn a_pictures_plan_spaces_the_rows_and_caps_the_pool() {
        use super::{bench_picture_plan, BENCH_FIXTURE_POOL};
        assert_eq!(bench_picture_plan(0, 500), (0, 0), "a page with no rows");
        assert_eq!(bench_picture_plan(1000, 0), (0, 0), "--pictures 0 is off");
        // the two scenes the matrix runs
        assert_eq!(bench_picture_plan(10_000, 500), (20, BENCH_FIXTURE_POOL));
        assert_eq!(bench_picture_plan(10_000, 5_000), (2, BENCH_FIXTURE_POOL));
        // more pictures asked for than rows available cannot invent rows
        assert_eq!(bench_picture_plan(100, 1_000), (1, 100));
    }

    #[test]
    fn a_pictures_page_turns_every_stride_row_into_an_image() {
        use super::{bench_pictures, BENCH_FIXTURE_POOL, BLOCK_IMAGE, BLOCK_PARAGRAPH};
        use crate::core::{Block, BlockId, BlockKind, ColorKind, OrderKey, PageId};
        let mut blocks: Vec<Block> = (0..1000u64)
            .map(|i| Block {
                id: BlockId(i + 1),
                page: PageId(1),
                parent: None,
                order: OrderKey(0),
                kind: BlockKind::Paragraph,
                text: format!("row {i}"),
                checked: false,
                marks: Vec::new(),
                color: ColorKind::Default,
                background: ColorKind::Default,
                page_ref: None,
                folded: false,
                attachment: None,
                img_percent: 100,
                columns: 0,
                lang: Lang::Plain,
                db_ref: None,
                sync_ref: None,            })
            .collect();
        bench_pictures(&mut blocks, 250);

        let picture_rows: Vec<(usize, u64)> = blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.attachment.map(|a| (i, a.as_u64())))
            .collect();
        assert_eq!(picture_rows.len(), 250, "one row every four");
        assert!(
            blocks
                .iter()
                .all(|b| (b.kind == BlockKind::Image) == (b.attachment.is_some())),
            "an image row is the only row that carries an attachment"
        );
        assert_eq!(picture_rows[0], (3, 1), "the stride, not an off-by-one");
        // consecutive picture rows draw different fixtures — that is what a
        // photo page does and what the decode cache is sized for...
        assert_eq!(picture_rows[1], (7, 2));
        // ...and the pool wraps only after it is exhausted.
        assert_eq!(picture_rows[BENCH_FIXTURE_POOL].0, 3 + 4 * BENCH_FIXTURE_POOL);
        assert_eq!(picture_rows[BENCH_FIXTURE_POOL].1, 1, "the pool wrapped");
        assert_eq!(super::kind_to_int(blocks[0].kind), BLOCK_PARAGRAPH);
        assert_eq!(super::kind_to_int(blocks[3].kind), BLOCK_IMAGE);
        assert_eq!(blocks[3].text, "", "a picture row has no text to lay out");
    }

    /// The bench scene has to survive from its seed pass to its measured pass
    /// the way a real session survives a restart: the first `AppState::new`
    /// writes the pool and its rows, the second loads them. If the second one
    /// regenerated anything, the measured pass would be timing a write.
    #[test]
    fn the_pictures_scene_seeds_its_pool_once_and_then_only_loads_it() {
        use super::{AppState, HandleArgs, BLOCK_IMAGE};
        use crate::testing::ScratchDir;
        use slint::Model;

        let dir = ScratchDir::new("pictures");
        let db = dir.path().join("library.db");
        let args = HandleArgs { blocks: 60, auto_exit_secs: 0.0, bench_pages: 0, pictures: 20, marks: 0, code: 0 };

        let first = AppState::new(&args, Some(std::sync::Arc::new(
            crate::storage::SqliteRepository::open(&db).unwrap(),
        )));
        let seeded: Vec<String> = {
            let book = first.attachments.borrow();
            assert_eq!(book.len(), 20, "one fixture per picture row");
            (1..=20i64)
                .map(|k| book.get(&k).expect("fixture id").file.clone())
                .collect()
        };
        assert!(
            seeded.iter().all(|f| dir.path().join("attachments").join(f).is_file()),
            "the bytes are on disk, not just in the map"
        );
        let rows: Vec<i32> = (0..first.blocks.row_count())
            .filter_map(|i| first.blocks.row_data(i).map(|r| r.kind))
            .collect();
        assert_eq!(rows.iter().filter(|k| **k == BLOCK_IMAGE).count(), 20);

        // One fixture goes away between the passes. A second `AppState::new`
        // that re-seeded would silently write it back and the assertion below
        // would pass for the wrong reason, so the load path is what is being
        // checked here, not the count.
        std::fs::remove_file(dir.path().join("attachments").join(&seeded[0])).unwrap();

        let second = AppState::new(&args, Some(std::sync::Arc::new(
            crate::storage::SqliteRepository::open(&db).unwrap(),
        )));
        assert_eq!(second.attachments.borrow().len(), 20, "the rows loaded back");
        assert!(
            !dir.path().join("attachments").join(&seeded[0]).exists(),
            "the measured pass must not re-write the pool"
        );
    }

    /// A 3x2 grid between two paragraphs, in display order. Cell "A1" is bold.
    fn grid_scene() -> Vec<crate::core::Block> {
        use crate::core::{Block, BlockId, BlockKind, ColorKind, Mark, MarkKind, OrderKey, PageId};
        let mk = |id: u64, parent: Option<u64>, kind: BlockKind, text: &str| Block {
            id: BlockId(id),
            page: PageId(1),
            parent: parent.map(BlockId),
            order: OrderKey(0),
            kind,
            text: text.into(),
            checked: false,
            marks: Vec::new(),
            color: ColorKind::Default,
            background: ColorKind::Default,
            page_ref: None,
            folded: false,
            attachment: None,
            img_percent: 100,
            columns: if kind == BlockKind::Table { 3 } else { 0 },
            lang: Lang::Plain,
            db_ref: None,
            sync_ref: None,        };
        let cell = |id: u64, text: &str| mk(id, Some(8), BlockKind::TableCell, text);
        let mut blocks = vec![
            mk(1, None, BlockKind::Paragraph, "before"),
            mk(8, None, BlockKind::Table, ""),
            cell(2, "A0"),
            Block {
                marks: vec![Mark { start: 0, end: 2, kind: MarkKind::Bold, url: String::new(), date: None }],
                ..cell(3, "A1")
            },
            cell(4, "A2"),
            cell(5, "B0"),
            cell(6, "B1"),
            cell(7, "B2"),
            mk(9, None, BlockKind::Paragraph, "after"),
        ];
        // the vec *is* display order — that is what a page's block list is
        for (i, b) in blocks.iter_mut().enumerate() {
            b.order = OrderKey((i as u64 + 1) * 10);
        }
        blocks
    }

    fn cell_texts(row: &crate::BlockRow) -> Vec<String> {
        use slint::Model;
        (0..row.table_cells.row_count())
            .map(|i| row.table_cells.row_data(i).unwrap().text.to_string())
            .collect()
    }

    #[test]
    fn a_grid_costs_one_row_and_carries_its_cells() {
        use slint::Model;
        let blocks = grid_scene();
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        // ADR-0028: the cells paint inside the grid delegate, so they must not
        // also cost a row each — SPEC §三十七 counts hidden as really hidden.
        let ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![1, 8, 9]);
        let table = &rows[1];
        assert_eq!((table.kind, table.columns), (super::BLOCK_TABLE, 3));
        assert!(!table.can_fold, "a grid has no chevron: no fold reveals its cells");
        assert_eq!(cell_texts(table), ["A0", "A1", "A2", "B0", "B1", "B2"], "row-major");
        // §十 marks ride into the grid: the delegate reads runs like a line's
        let marked = table.table_cells.row_data(1).unwrap();
        assert_eq!(marked.runs.row_count(), 1);
        let run = marked.runs.row_data(0).unwrap();
        assert_eq!((run.text.as_str(), run.bold), ("A1", true));
        assert_eq!(
            table.table_cells.row_data(0).unwrap().runs.row_count(),
            0,
            "an unmarked cell keeps the wrapping Text"
        );
        assert!(rows.last().unwrap().tail);
    }

    #[test]
    fn a_ragged_grid_projects_whole_rows() {
        use slint::Model;
        let mut blocks = grid_scene();
        // stray cells (a v8 database touched by hand) must not become a
        // half-row: the delegate chunks by `columns` and indexes into the list
        blocks.retain(|b| b.id != crate::core::BlockId(6) && b.id != crate::core::BlockId(7));
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        assert_eq!(rows[1].table_cells.row_count(), 3, "four cells is one row of three");
        assert_eq!(cell_texts(&rows[1]), ["A0", "A1", "A2"]);
    }

    #[test]
    fn every_row_index_consumer_shares_the_one_visible_list() {
        let blocks = grid_scene();
        // a cell has no row, so no row can carry kind 17 and no row index can
        // land inside a grid — drop_index_for_row reads the same list
        let rows = super::project_blocks(&blocks, &FindHits::new(), &super::MentionTitles::empty());
        assert!(rows.iter().all(|r| r.kind != super::BLOCK_TABLE_CELL));
        assert_eq!(super::visible_block_indices(&blocks), vec![0, 1, 8]);
    }

    /// PNG bytes the way the clipboard hands them over: already encoded, never
    /// a file on disk.
    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([9, 99, 199, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn a_pasted_picture_becomes_an_empty_block_and_lands_below_a_written_one() {
        use super::{AppState, HandleArgs, BLOCK_IMAGE, BLOCK_PARAGRAPH};
        use crate::app::state::Change;
        use crate::core::{BlockId, Command};
        use crate::testing::ScratchDir;
        use slint::Model;

        let dir = ScratchDir::new("paste");
        // A real library in the scratch folder, so the attachment bytes this
        // test stores are the folder's to lose. With no database the store
        // falls back to a shared %TEMP% directory and the paste would litter
        // it.
        let repo = std::sync::Arc::new(
            crate::storage::SqliteRepository::open(&dir.path().join("library.db")).unwrap(),
        );
        let state =
            AppState::new(&HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 }, Some(repo));

        state.create_page(None);
        let empty = state.start_page().expect("an empty page takes a paragraph");
        let png = png_bytes(24, 18);

        assert!(state.paste_image(empty, &png), "the paste must land");
        let row = state.blocks.row_data(0).unwrap();
        assert_eq!(row.kind, BLOCK_IMAGE, "the empty block *is* the picture");
        assert_eq!(row.id, empty, "and not a new row");
        let att_id = state
            .doc
            .borrow()
            .block(BlockId(empty as u64))
            .and_then(|b| b.attachment)
            .expect("the block carries the attachment");
        let (stored, name, mime, size) = {
            let book = state.attachments.borrow();
            let att = book.get(&(att_id.as_u64() as i64)).expect("the attachment is known");
            (att.file.clone(), att.name.clone(), att.mime.clone(), att.bytes)
        };
        assert_eq!(name, "Pasted image");
        assert_eq!((size, mime.as_str()), (png.len() as i64, "image/png"));
        let on_disk = dir.path().join("attachments").join(&stored);
        assert!(on_disk.exists(), "the bytes went beside the library, not into it");

        // a block with words in it keeps them and gets the picture below
        let changes = state
            .exec_on_open_page(Command::InsertBlockAfter {
                id: BlockId(empty as u64),
                kind: crate::core::BlockKind::Paragraph,
                text: "written first".into(),
            })
            .expect("a paragraph below the picture");
        let written = changes
            .iter()
            .find_map(|c| match c {
                Change::BlockInserted(b) => Some(b.id.0 as i32),
                _ => None,
            })
            .unwrap();
        assert!(state.paste_image(written, &png));
        assert_eq!(state.blocks.row_count(), 3);
        assert_eq!(state.blocks.row_data(1).unwrap().kind, BLOCK_PARAGRAPH);
        assert_eq!(state.blocks.row_data(1).unwrap().text, "written first");
        assert_eq!(state.blocks.row_data(2).unwrap().kind, BLOCK_IMAGE);

        // one paste is one undo step, and it drops the reference only
        state.undo_open_page();
        assert_eq!(state.blocks.row_count(), 2);
        assert_eq!(state.blocks.row_data(0).unwrap().kind, BLOCK_IMAGE);
        assert!(on_disk.exists(), "undo never deletes bytes another block may still point at");
    }

    // --- reclaim (SPEC §三十七, ADR-0037) ---

    // The signatures here spell out their paths: the helpers below are used by
    // several tests and a `use` in a function body does not reach a signature.
    fn scratch_repo(
        dir: &crate::testing::ScratchDir,
    ) -> std::sync::Arc<crate::storage::SqliteRepository> {
        std::sync::Arc::new(
            crate::storage::SqliteRepository::open(&dir.path().join("library.db")).unwrap(),
        )
    }

    fn plain_args() -> super::HandleArgs {
        super::HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 }
    }

    /// A fresh session on a real database: one page with `n` pictures on it,
    /// oldest first, each with its row in the database and its bytes in the
    /// scratch folder. Per picture the caller gets back
    /// `(attachment id, file name, block id)`.
    fn session_with_pictures(
        dir: &crate::testing::ScratchDir,
        n: usize,
    ) -> (
        std::rc::Rc<super::AppState>,
        std::sync::Arc<crate::storage::SqliteRepository>,
        i32,
        Vec<(i64, String, i32)>,
    ) {
        use crate::core::BlockKind;

        let repo = scratch_repo(dir);
        let state = super::AppState::new(&plain_args(), Some(repo.clone()));
        let page = state.create_page(None);
        let mut anchor = state.start_page().expect("the page takes its first block");
        let mut pics = Vec::new();
        for _ in 0..n {
            let att = state
                .store
                .import_bytes(state.claim_attachment_id(), "sample", &png_bytes(24, 18))
                .unwrap();
            assert!(state.insert_attachment(anchor, att.clone(), BlockKind::Image));
            let block = {
                let doc = state.doc.borrow();
                let found = doc
                    .all_blocks()
                    .find(|b| b.attachment == Some(att.id))
                    .expect("the row carries the pointer")
                    .id;
                found.0 as i32
            };
            anchor = block;
            pics.push((att.id.as_u64() as i64, att.file.clone(), block));
        }
        (state, repo, page, pics)
    }

    #[test]
    fn a_reclaim_leaves_alone_everything_undo_can_still_bring_back() {
        use super::BLOCK_IMAGE;
        use crate::core::{BlockId, Command};
        use crate::testing::ScratchDir;
        use slint::Model;

        let dir = ScratchDir::new("reclaim-undo");
        let attach_dir = dir.path().join("attachments");
        let (state, _repo, _page, pics) = session_with_pictures(&dir, 2);

        // drop the second picture's row: reference gone, row and bytes stay
        state
            .exec_on_open_page(Command::DeleteBlock { id: BlockId(pics[1].2 as u64) })
            .expect("a picture block deletes");
        assert_eq!(
            state.reclaim_attachments().unwrap(),
            "no unused attachments to remove",
            "the undo step holds the only other pointer to it"
        );
        assert!(attach_dir.join(&pics[1].1).is_file(), "nothing was deleted");

        // the protection is not theoretical: Ctrl+Z puts the row back and the
        // picture still has bytes to show
        state.undo_open_page();
        let kinds: Vec<i32> = (0..state.blocks.row_count())
            .filter_map(|i| state.blocks.row_data(i).map(|r| r.kind))
            .collect();
        assert_eq!(kinds.iter().filter(|k| **k == BLOCK_IMAGE).count(), 2);
        assert_eq!(state.image_for(pics[1].0 as i32).size().width, 24);
    }

    #[test]
    fn a_restarted_session_reclaims_the_picture_its_predecessor_deleted() {
        use super::AppState;
        use crate::core::{BlockId, Command};
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("reclaim-restart");
        let attach_dir = dir.path().join("attachments");
        let (state, repo, _page, pics) = session_with_pictures(&dir, 2);
        state
            .exec_on_open_page(Command::DeleteBlock { id: BlockId(pics[1].2 as u64) })
            .expect("a picture block deletes");
        state.persistence_force_flush();
        drop(state);
        // control: the orphan is really in the database, or the sweep below
        // would be proving nothing
        assert_eq!(repo.load_attachments().unwrap().len(), 2);
        drop(repo);

        let repo = scratch_repo(&dir);
        let state = AppState::new(&plain_args(), Some(repo.clone()));
        assert_eq!(state.attachments.borrow().len(), 2, "a new session sees both rows");
        // the dead row is on no screen, but a decode of it costs the cache
        // weight, and the reclaim has to hand that back
        assert!(state.image_for(pics[1].0 as i32).size().width > 0);
        assert!(state.attachment_cache_bytes.get() > 0);

        let notice = state.reclaim_attachments().unwrap();
        assert!(notice.starts_with("1 unused attachment removed ("), "{notice}");
        assert!(state.attachments.borrow().contains_key(&pics[0].0), "the live row stays");
        assert!(!state.attachments.borrow().contains_key(&pics[1].0));
        assert!(attach_dir.join(&pics[0].1).is_file(), "and nothing else went");
        assert!(!attach_dir.join(&pics[1].1).exists(), "its bytes went with its row");
        assert_eq!(repo.load_attachments().unwrap().len(), 1, "the row left the database too");
        assert_eq!(state.attachment_cache_bytes.get(), 0, "the decode cache paid back");
        assert_eq!(
            state.image_for(pics[1].0 as i32).size().width,
            0,
            "a reclaimed id paints blank, not the bytes it used to name"
        );
    }

    /// The clipboard is the third pointer a block list cannot show. This needs
    /// a session that did not paste the picture itself — otherwise the undo
    /// stack is protecting it and the clipboard is never tested.
    #[test]
    fn a_copied_picture_survives_losing_its_page_to_a_reclaim() {
        use super::AppState;
        use crate::testing::ScratchDir;
        use slint::Model;

        let dir = ScratchDir::new("reclaim-clipboard");
        let (state, repo, page, pics) = session_with_pictures(&dir, 1);
        state.persistence_force_flush();
        assert_eq!(repo.load_attachments().unwrap().len(), 1, "control: the row is in the db");
        drop(state);
        drop(repo);

        let repo = scratch_repo(&dir);
        let state = AppState::new(&plain_args(), Some(repo));
        state.open_page(page);
        let rows: Vec<i32> = (0..state.blocks.row_count())
            .filter_map(|i| state.blocks.row_data(i).map(|r| r.id))
            .collect();
        assert!(rows.contains(&pics[0].2), "control: the page loaded with its picture row");
        state.copy_block(pics[0].2);
        state.create_page(None);
        state.delete_page(page);
        assert_eq!(
            state.reclaim_attachments().unwrap(),
            "no unused attachments to remove",
            "pasting the copied row is still one keystroke away"
        );
        let target = state.start_page().expect("the new page takes a block");
        assert!(state.paste_below(target), "the copied row lands");
        assert_eq!(state.image_for(pics[0].0 as i32).size().width, 24, "and its bytes were never touched");
    }

    #[test]
    fn a_deleted_pages_picture_is_reclaimable_without_a_restart() {
        use super::AppState;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("reclaim-page");
        let attach_dir = dir.path().join("attachments");
        let (state, repo, page, pics) = session_with_pictures(&dir, 1);
        state.persistence_force_flush();
        drop(state);
        drop(repo);

        let repo = scratch_repo(&dir);
        let state = AppState::new(&plain_args(), Some(repo.clone()));
        state.create_page(None); // deleting a page needs one left to be open
        // the bool is "was the open page among those removed", not "did it work"
        state.delete_page(page);
        let notice = state.reclaim_attachments().unwrap();
        assert!(notice.starts_with("1 unused attachment removed ("), "{notice}");
        assert!(!attach_dir.join(&pics[0].1).exists());
        assert_eq!(repo.load_attachments().unwrap().len(), 0, "the row went with the page");
    }

    /// A cover is a **page** pointing at a file, and until SPEC §三十八 the
    /// sweep asked only the blocks about who points at anything — which is the
    /// objection ADR-0046 raised against a picture in the icon slot. Two halves:
    /// bytes no block holds must survive because the page draws them, and once
    /// the page lets go, in a session whose undo stack no longer remembers
    /// them, they are as orphan as any other.
    #[test]
    fn a_pages_cover_keeps_its_bytes_through_a_reclaim() {
        use super::AppState;
        use crate::core::{BlockId, Command};
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("reclaim-cover");
        let attach_dir = dir.path().join("attachments");
        let (state, repo, page, pics) = session_with_pictures(&dir, 1);
        let cover = state
            .store
            .create_fixture(state.claim_attachment_id(), 640, 400)
            .expect("a cover fixture");
        let cover_file = cover.file.clone();
        state.set_page_cover_from(page, cover);
        state
            .exec_on_open_page(Command::DeleteBlock {
                id: BlockId(pics[0].2 as u64),
            })
            .expect("a picture block deletes");
        state.persistence_force_flush();

        // The restart is the point: the undo stack that still holds a vote for
        // the deleted block is gone, so what survives now survives on the page.
        drop(state);
        drop(repo);
        let repo = scratch_repo(&dir);
        let state = AppState::new(&plain_args(), Some(repo.clone()));
        let notice = state.reclaim_attachments().unwrap();
        assert!(
            notice.starts_with("1 unused attachment removed"),
            "{notice}"
        );
        assert!(!attach_dir.join(&pics[0].1).exists(), "the block's picture went");
        assert!(
            attach_dir.join(&cover_file).is_file(),
            "the cover's bytes are the page's, not nobody's"
        );
        assert_eq!(repo.load_attachments().unwrap().len(), 1);

        state.set_page_cover(page, None);
        let notice = state.reclaim_attachments().unwrap();
        assert!(
            notice.starts_with("1 unused attachment removed"),
            "{notice}"
        );
        assert!(
            !attach_dir.join(&cover_file).exists(),
            "the page let go, so the sweep could too"
        );
    }

    /// The half-orphan: a row whose bytes were deleted by hand. A missing file
    /// is not a failure for the sweep — clearing the row is the whole point.
    #[test]
    fn a_row_whose_bytes_are_already_gone_is_still_removed() {
        use super::AppState;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("reclaim-half-orphan");
        let attach_dir = dir.path().join("attachments");
        let (state, repo, page, pics) = session_with_pictures(&dir, 1);
        state.persistence_force_flush();
        std::fs::remove_file(attach_dir.join(&pics[0].1)).unwrap();
        drop(state);
        drop(repo);

        let repo = scratch_repo(&dir);
        let state = AppState::new(&plain_args(), Some(repo.clone()));
        state.create_page(None);
        state.delete_page(page);
        let notice = state.reclaim_attachments().unwrap();
        assert!(notice.starts_with("1 unused attachment removed ("), "{notice}");
        assert!(
            !notice.contains("could not be deleted"),
            "a file that was already gone is not a stuck file: {notice}"
        );
        assert_eq!(repo.load_attachments().unwrap().len(), 0);
    }

    #[test]
    fn a_session_without_a_file_has_nothing_to_reclaim() {
        use super::AppState;

        let state = AppState::new(&plain_args(), None);
        assert_eq!(
            state.reclaim_attachments().unwrap(),
            "running in memory — nothing to reclaim"
        );
        let repo = std::sync::Arc::new(crate::storage::SqliteRepository::in_memory().unwrap());
        let state = AppState::new(&plain_args(), Some(repo));
        assert_eq!(
            state.reclaim_attachments().unwrap(),
            "running in memory — nothing to reclaim"
        );
    }

    /// Reclaim runs on the UI thread, so its cost is how long a click freezes
    /// the window. Prints two arms in one sitting, so they share a disk cache:
    /// sweeping a library of 1 000 orphans, and the same call afterwards with
    /// nothing left to do (the floor — queue flush over an empty book).
    #[test]
    #[ignore = "prints a timing measurement"]
    fn a_reclaim_of_a_thousand_orphans_is_timed() {
        use crate::core::persistence::{Change, Repository};
        use crate::testing::ScratchDir;
        use std::time::Instant;

        const N: usize = 1000;
        let dir = ScratchDir::new("reclaim-timing");
        let repo = scratch_repo(&dir);
        let state = super::AppState::new(&plain_args(), Some(repo.clone()));
        // the shape of a library nobody ever reclaimed: a row and bytes for
        // every one of them, and no block pointing at any
        let setup = Instant::now();
        for _ in 0..N {
            let att = state
                .store
                .import_bytes(state.claim_attachment_id(), "orphan", &png_bytes(24, 18))
                .unwrap();
            repo.apply(&[Change::AttachmentAdded(att.clone())]).unwrap();
            state
                .attachments
                .borrow_mut()
                .insert(att.id.as_u64() as i64, att);
        }
        let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;

        // control: the sweep can only be fast if there really was work here
        let attach_dir = dir.path().join("attachments");
        let on_disk = std::fs::read_dir(&attach_dir).unwrap().count();
        assert_eq!(on_disk, N, "the folder has one file per orphan");
        assert_eq!(
            repo.load_attachments().unwrap().len(),
            N,
            "and the table has one row per file"
        );

        let swept = Instant::now();
        let report = state.reclaim_attachments().unwrap();
        let sweep_ms = swept.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            std::fs::read_dir(&attach_dir).unwrap().count(),
            0,
            "and the sweep took them all"
        );
        assert_eq!(repo.load_attachments().unwrap().len(), 0);

        let second = Instant::now();
        assert_eq!(
            state.reclaim_attachments().unwrap(),
            "no unused attachments to remove"
        );
        let scan_ms = second.elapsed().as_secs_f64() * 1000.0;

        println!(
            r#"{{"scene":"reclaim-timing","orphans":{N},"setup_ms":{setup_ms:.1},"sweep_ms":{sweep_ms:.1},"scan_ms":{scan_ms:.1},"report":"{report}"}}"#
        );
    }

    /// What the sidebar's 16px slot draws (SPEC §三十八). Two rules, and the
    /// second one is the reason this test exists: the first-character
    /// placeholder belongs to the tree, where it replaces a generic page
    /// glyph, and not to Favorites / Recent, where it would erase the star and
    /// the clock that say which section the row is in.
    #[test]
    fn the_sidebar_slot_takes_the_emoji_and_only_the_tree_takes_the_placeholder() {
        use super::AppState;
        use slint::Model as _;

        let state = AppState::new(&plain_args(), None);
        // A page can be in the sidebar twice — as a favorite or a recent, and
        // as its own tree row — and the two answer differently, so the lookup
        // is by both id and kind.
        let slot_of = |id: i32, kind: &str| -> Option<String> {
            let model = &state.sidebar;
            (0..model.row_count())
                .filter_map(|i| model.row_data(i))
                .find(|r| r.id == id && r.kind == kind)
                .map(|r| r.icon.to_string())
        };

        let leaf = (0..state.sidebar.row_count())
            .filter_map(|i| state.sidebar.row_data(i))
            .find(|r| r.kind == "page" && !r.has_children)
            .expect("the seed has a leaf page");
        let placeholder = leaf.label.chars().next().unwrap().to_string();
        assert_eq!(
            slot_of(leaf.id, "page"),
            Some(placeholder.clone()),
            "an iconless leaf shows its title's first character"
        );
        assert_eq!(
            (0..state.sidebar.row_count())
                .filter_map(|i| state.sidebar.row_data(i))
                .find(|r| r.kind == "favorite")
                .map(|r| r.icon.to_string()),
            Some(String::new()),
            "a favorite with no icon keeps its star"
        );

        // A fresh session opens on a page that is not in the tree at all, so
        // the write is driven through a page the tree does show: the leaf
        // found above.
        state.set_page_icon(leaf.id, "\u{1F680}");
        assert_eq!(
            slot_of(leaf.id, "page"),
            Some("\u{1F680}".into()),
            "the emoji beats the placeholder"
        );
        assert_eq!(
            slot_of(leaf.id, "favorite"),
            Some("\u{1F680}".into()),
            "and the shortcut to the same page carries it too"
        );
        state.set_page_icon(leaf.id, "");
        assert_eq!(slot_of(leaf.id, "page"), Some(placeholder), "clearing it falls back");
        assert_eq!(
            slot_of(leaf.id, "favorite"),
            Some(String::new()),
            "and a cleared favorite is a star again"
        );
    }

    /// What the page's blocks currently say, as one comparable value: the
    /// identity check the lock tests below run before and after a storm of
    /// refusals.
    fn words(state: &super::AppState) -> Vec<(i32, crate::core::BlockKind, String)> {
        let doc = state.doc.borrow();
        doc.page_blocks(super::core_page_id(state.open_page.get()))
            .iter()
            .map(|b| (b.id.0 as i32, b.kind, b.text.clone()))
            .collect()
    }

    /// Append one paragraph to the open page and hand back its id. Panics with
    /// the given reason when the page refuses it, so a fixture that never got
    /// off the ground cannot read as a passing refusal.
    fn add_line(
        state: &super::AppState,
        anchor: i32,
        text: &str,
    ) -> i32 {
        use crate::core::{BlockId, BlockKind, Change, Command};
        state
            .exec_on_open_page(Command::InsertBlockAfter {
                id: BlockId(anchor as u64),
                kind: BlockKind::Paragraph,
                text: text.into(),
            })
            .and_then(|chs| {
                chs.into_iter().find_map(|c| match c {
                    Change::BlockInserted(b) => Some(b.id.0 as i32),
                    _ => None,
                })
            })
            .expect("an unlocked page takes a block")
    }

    /// SPEC §三十八 "lock", the half a screenshot cannot show: every write
    /// entry point answers no, and the document is the same document
    /// afterwards. The controls matter as much as the refusals — a "no" proves
    /// nothing unless the same call says "yes" with the switch off, from the
    /// same fixture.
    #[test]
    fn a_locked_page_refuses_every_edit_and_leaves_the_document_as_it_was() {
        use super::AppState;
        use crate::core::{BlockId, BlockKind, Command};
        use slint::Model as _;

        let state = AppState::new(&plain_args(), None);
        let page = state.create_page(None);
        let host = state.start_page().expect("a new page takes its first block");
        let second = add_line(&state, host, "second");
        let link_host = add_line(&state, host, "");
        let page_host = add_line(&state, host, "");
        state
            .exec_editor(Command::ReplaceText {
                id: BlockId(host as u64),
                text: "kept".into(),
            })
            .expect("typing works before the lock");
        // a second page to name, created before the lock because creating one
        // navigates to it
        let other = state.create_page(None);
        state.open_page(page);

        // an inline sub-page, made before the lock because that is the only
        // way one gets here: duplicating its row is the write under test
        let sub = add_line(&state, host, "");
        assert!(
            state.create_page_block(sub).is_some(),
            "one row becomes a sub-page before the switch goes on"
        );
        let kids = state.workspace.borrow().children_of(Some(page)).len();

        let before = words(&state);
        assert_eq!(before.len(), 5, "the fixture has five rows");
        assert_eq!(state.blocks.row_count(), 5, "and the editor shows them");
        state
            .exec_on_open_page(Command::MoveBlockTo {
                id: BlockId(second as u64),
                index: 0,
            })
            .expect("a drag reorder works before the lock");
        let before = words(&state);
        assert_eq!(before[0].0, second, "and it landed");

        state.set_page_locked(page, true);
        assert!(state.page_locked(), "the switch is on the page on screen");

        assert!(
            state
                .exec_editor(Command::ReplaceText {
                    id: BlockId(host as u64),
                    text: "typed after the lock".into(),
                })
                .is_none(),
            "the editing input"
        );
        assert!(
            state
                .exec_editor(Command::SplitBlock {
                    id: BlockId(host as u64),
                    caret: 2,
                })
                .is_none(),
            "Enter"
        );
        assert!(
            state
                .exec_editor(Command::MergeBackward {
                    id: BlockId(second as u64),
                })
                .is_none(),
            "Backspace at column 0"
        );
        assert!(
            state
                .exec_on_open_page(Command::InsertBlockAfter {
                    id: BlockId(host as u64),
                    kind: BlockKind::Heading1,
                    text: "a heading".into(),
                })
                .is_none(),
            "the slash and + menus"
        );
        assert!(
            state
                .exec_on_open_page(Command::DeleteBlock {
                    id: BlockId(second as u64),
                })
                .is_none(),
            "⋮⋮ → Delete"
        );
        assert!(
            state
                .exec_on_open_page(Command::DuplicateBlock {
                    id: BlockId(second as u64),
                })
                .is_none(),
            "⋮⋮ → Duplicate"
        );
        assert!(
            state
                .exec_on_open_page(Command::SetBlockType {
                    id: BlockId(host as u64),
                    kind: BlockKind::Todo,
                })
                .is_none(),
            "Turn into"
        );
        assert!(
            state
                .exec_on_open_page(Command::ToggleMark {
                    id: BlockId(host as u64),
                    start: 0,
                    end: 2,
                    kind: crate::core::MarkKind::Bold,
                    url: String::new(),
                    date: None,
                })
                .is_none(),
            "a mark"
        );
        assert!(
            state
                .exec_on_open_page(Command::MoveBlockTo {
                    id: BlockId(second as u64),
                    index: 2,
                })
                .is_none(),
            "a drag landing"
        );
        assert!(
            state
                .exec_on_open_page(Command::ToggleTodoChecked {
                    id: BlockId(second as u64),
                })
                .is_none(),
            "a checkbox. The controller has its own gate for the same command, \
             because that is the one caller that reaches the core layer directly"
        );
        assert!(
            state
                .exec_all_on_open_page(vec![Command::ReplaceText {
                    id: BlockId(host as u64),
                    text: "one undo step".into(),
                }])
                .is_none(),
            "a multi-command edit (rich paste)"
        );
        state.copy_block(host);
        assert!(!state.paste_below(host), "a paste from the clipboard");
        assert!(
            state.undo_open_page().is_none(),
            "Ctrl+Z: a stack built before the lock does not walk the page back"
        );
        assert!(state.redo_open_page().is_none(), "Ctrl+Y");
        state.rename_page(page, "Renamed while locked");
        assert_eq!(
            state.workspace.borrow().title_of(page),
            Some("Untitled"),
            "the title is part of the document, so the lock covers it too"
        );
        assert!(
            !state.create_page_link_block(link_host, other),
            "a link card is a write"
        );
        assert!(
            state.create_page_block(page_host).is_none(),
            "an inline sub-page is a write"
        );
        // The one refusal a grep for `exec_editor` cannot find: duplicating a
        // Page block mints its child page through the tree, before the funnel
        // ever sees the block command.
        assert!(
            state.duplicate_page_block(sub).is_none(),
            "a Page block's duplicate is a write too"
        );
        assert_eq!(
            state.workspace.borrow().children_of(Some(page)).len(),
            kids,
            "and it did not leave a second child page behind"
        );

        // Every refusal above, and the document is the one it was. This is the
        // assertion the section's "不能静默吞输入" leans on: the input is
        // refused, not half-applied.
        assert_eq!(words(&state), before, "nothing was written");
        assert_eq!(state.blocks.row_count(), 5, "nothing was reprojected away");
        let line = state
            .db_notice
            .borrow()
            .last()
            .cloned()
            .expect("and every one of them said so");
        assert!(line.contains("locked"), "{line}");
        assert!(line.contains("Unlock page"), "{line} names the way out");

        // The switch off, and the same calls land — including the two whose
        // refusal is above, which is what proves those rows were untouched
        // rather than damaged.
        state.set_page_locked(page, false);
        assert!(!state.page_locked());
        assert!(state
            .exec_editor(Command::ReplaceText {
                id: BlockId(host as u64),
                text: "typed after the unlock".into(),
            })
            .is_some());
        assert_eq!(
            words(&state)
                .into_iter()
                .find(|(b, ..)| *b == host)
                .map(|(_, _, t)| t),
            Some("typed after the unlock".into()),
            "the row took what was typed"
        );
        assert!(state.create_page_link_block(link_host, other), "the row is still an empty paragraph");
        assert!(state.create_page_block(page_host).is_some(), "and so is this one");
        // `kids` above has grown by the page that call just made, so the
        // duplicate's arithmetic reads from here
        let grown = state.workspace.borrow().children_of(Some(page)).len();
        assert!(
            state.duplicate_page_block(sub).is_some(),
            "control: unlock it and the same duplicate lands"
        );
        assert_eq!(
            state.workspace.borrow().children_of(Some(page)).len(),
            grown + 1
        );
    }

    /// The other half of "不能静默吞输入": one gesture, one message. The drag
    /// hover asks `can_move_block_to` once per frame of the pointer's travel,
    /// so the check that hides the drop line stays quiet while the drop that
    /// follows it speaks — and speaks once, not sixty times.
    #[test]
    fn a_locked_page_refuses_once_per_gesture_and_still_folds() {
        use super::AppState;
        use crate::core::{BlockId, BlockKind, Command};

        let state = AppState::new(&plain_args(), None);
        let page = state.create_page(None);
        let host = state.start_page().expect("a new page takes its first block");
        let second = add_line(&state, host, "second");
        let other = state.create_page(None);
        state.open_page(page);
        let queued = || state.db_notice.borrow().len();
        let quiet = queued();

        // The hover check is the per-frame caller, so it answers without a
        // message. Control first: the same landing is valid with the lock off.
        assert!(
            state.can_move_block_to(second, 0),
            "before the lock the landing is a drop target"
        );
        assert_eq!(queued(), quiet, "and hovering says nothing");
        state.set_page_locked(page, true);
        assert!(
            !state.can_move_block_to(second, 0),
            "a locked page shows no drop line at all"
        );
        assert_eq!(queued(), quiet, "the hover itself stays silent");

        for _ in 0..30 {
            let _ = state.exec_editor(Command::ReplaceText {
                id: BlockId(host as u64),
                text: "typed".into(),
            });
        }
        assert_eq!(
            queued(),
            quiet + 1,
            "thirty frames of one gesture, one line on the bar"
        );

        // The one command a locked page still runs, and the reason it is the
        // exception: folding changes what is on screen, not what the document
        // says (§三十七 files it as persisted view state).
        assert!(
            state
                .exec_editor(Command::ToggleFold {
                    id: BlockId(host as u64),
                })
                .is_some(),
            "locking a page must not cost the user its outline"
        );
        assert!(
            state
                .exec_editor(Command::InsertBlockAfter {
                    id: BlockId(host as u64),
                    kind: BlockKind::Bullet,
                    text: "- ".into(),
                })
                .is_none(),
            "every other command still refuses"
        );

        // Moving a block *into* a locked page is a write to that page, and the
        // menu offering it has no disabled state to say so with.
        let block = {
            let doc = state.doc.borrow();
            doc.page_blocks(super::core_page_id(page))
                .first()
                .map(|b| b.id.0 as i32)
                .expect("the locked page has a block")
        };
        assert!(
            !state.move_block_to_page(block, other),
            "the source page is locked, so the command never runs"
        );
        // the same call with the destination locked and the source open
        state.set_page_locked(page, false);
        state.open_page(other);
        let moved = state.start_page().expect("the unlocked page takes a block");
        state.set_page_locked(page, true);
        assert!(
            !state.move_block_to_page(moved, page),
            "a locked page is not a drop destination either"
        );
        assert_eq!(
            state
                .doc
                .borrow()
                .block(BlockId(moved as u64))
                .expect("the block is still there")
                .page,
            super::core_page_id(other),
            "and it stayed where it was"
        );
        state.set_page_locked(page, false);
        assert!(
            state.move_block_to_page(moved, page),
            "control: unlock the destination and the same move lands"
        );
    }

    /// The touch-only row and the touch row height are one decision in two
    /// halves: the long-press menu stands in for the ⋮⋮/+ handle a finger can
    /// never hover (ANDROID_NOTES "What must be rebuilt"), so it must carry
    /// the insert door, and the desktop menu must keep the shape every
    /// baseline was shot with. A locked page drops it with the rest of the
    /// editing rows — the retain that keeps only the two take-only rows is
    /// the whole policy, and this is the assertion that it reaches the new row.
    #[test]
    fn the_touch_menu_offers_insert_below_and_the_desktop_menu_does_not() {
        use super::AppState;
        use slint::Model as _;

        let state = AppState::new(&plain_args(), None);
        let page = state.create_page(None);
        let host = state.start_page().expect("a new page takes its first block");

        state.fill_block_menu(host, false);
        let desktop: Vec<i32> = (0..state.block_menu.row_count())
            .map(|i| state.block_menu.row_data(i).unwrap().id)
            .collect();
        assert!(
            !desktop.contains(&AppState::INSERT_BELOW_ACTION),
            "the desktop menu is unchanged: no insert row without touch"
        );

        state.fill_block_menu(host, true);
        let touch: Vec<i32> = (0..state.block_menu.row_count())
            .map(|i| state.block_menu.row_data(i).unwrap().id)
            .collect();
        assert_eq!(
            touch.iter().filter(|a| **a == AppState::INSERT_BELOW_ACTION).count(),
            1,
            "the touch menu carries the insert door, exactly once"
        );

        state.set_page_locked(page, true);
        state.fill_block_menu(host, true);
        let locked: Vec<i32> = (0..state.block_menu.row_count())
            .map(|i| state.block_menu.row_data(i).unwrap().id)
            .collect();
        assert!(
            !locked.contains(&AppState::INSERT_BELOW_ACTION),
            "a locked page refuses edits, and inserting is one"
        );
    }

    // --- templates (SPEC §三十八 "模板") ---

    /// The storage shape's whole claim is that a template is a page nobody can
    /// reach, so the test is a list of the doors: the tree walk, the sidebar
    /// model, recents, the open-page setter, the search panel, and the two page
    /// counts that disagree on purpose. `page_count` is the control that proves
    /// the template is really there — an empty library would pass every
    /// "cannot see it" assertion below by itself.
    #[test]
    fn a_template_is_invisible_in_every_place_a_page_shows_up() {
        use super::AppState;
        use slint::Model as _;

        let state = AppState::new(&plain_args(), None);
        // A memory-only session still gets the mock workspace, so everything
        // below is a delta against what this one started with -- an absolute
        // count would be a test about the fixture rather than about templates.
        let before_pages = state.workspace.borrow().page_count();
        let before_visible = state.workspace.borrow().visible_page_count();
        let page = state.create_page(None);
        let host = state.start_page().expect("the page takes its first block");
        add_line(&state, host, "the words a template copies");
        let t = state.import_template("Zephyr body", "## Beta head\n\n- one\n");

        assert_eq!(
            state.workspace.borrow().page_count(),
            before_pages + 2,
            "the page, and the body saved behind it"
        );
        assert_eq!(
            state.workspace.borrow().visible_page_count(),
            before_visible + 1,
            "and only the page is one you can see"
        );
        assert!(state.workspace.borrow().contains(t), "a template *is* a page row");
        assert!(state.is_template(t));

        let ws = state.workspace.borrow();
        let tree = ws.dfs_order();
        assert!(tree.contains(&page), "control: the page is in the walk");
        assert!(!tree.contains(&t), "the tree walk never sees it");
        assert!(!ws.children_of(None).contains(&t), "it has no parent and no root slot");
        assert_eq!(
            ws.title_of(t),
            Some("Zephyr body"),
            "but it keeps the name the menu shows"
        );
        drop(ws);

        assert!(
            !(0..state.sidebar.row_count())
                .filter_map(|i| state.sidebar.row_data(i))
                .any(|r| r.id == t),
            "the sidebar has no row for it"
        );

        // Opening one is the strongest door, because the two things a template
        // must not touch are what `open_page` writes: `recents` and the
        // `current-page` meta row.
        let was = state.open_page.get();
        state.open_page(t);
        assert_eq!(state.open_page.get(), was, "a template has no door in");
        assert!(
            !state.workspace.borrow().recents_ids().contains(&t),
            "and refusing it kept it out of recents"
        );

        // The palette's blob scan walks the tree, so the same absence answers
        // it. Both names carry the same word, so "the page is a hit and the
        // body is not" is the scan saying *not in the tree* rather than merely
        // *nothing matched this query*.
        state.rename_page(page, "Zephyr agenda");
        state.set_search_query("zephyr");
        let hits: Vec<i32> = (0..state.search.row_count())
            .filter_map(|i| state.search.row_data(i))
            .map(|r| r.page_id)
            .collect();
        assert!(
            hits.contains(&page),
            "control: the page in the tree is findable by its own title"
        );
        assert!(!hits.contains(&t), "the body is not");
    }

    /// "模板的表示必须是「块序列的副本」", read as two things a copy has to get
    /// right: it carries the rows as they are — kinds, marks, order — and it
    /// shares no id with its source, because an id is how a row is found again
    /// and two rows answering to one id is the corruption this whole feature
    /// avoids by not inventing a format. The second half is the save policy:
    /// saving twice makes two templates, since overwriting would destroy a body
    /// that sits on nobody's undo stack.
    #[test]
    fn saving_a_page_copies_its_rows_and_saving_twice_overwrites_nothing() {
        use super::AppState;
        use crate::core::{BlockId, BlockKind, Command, MarkKind};

        let state = AppState::new(&plain_args(), None);
        let page = state.create_page(None);
        let host = state.start_page().expect("the page takes its first block");
        state
            .exec_editor(Command::ReplaceText {
                id: BlockId(host as u64),
                text: "the agenda".into(),
            })
            .expect("typing");
        state
            .exec_on_open_page(Command::SetBlockType {
                id: BlockId(host as u64),
                kind: BlockKind::Heading1,
            })
            .expect("a row becomes a heading");
        let second = add_line(&state, host, "do the thing");
        state
            .exec_on_open_page(Command::SetBlockType {
                id: BlockId(second as u64),
                kind: BlockKind::Todo,
            })
            .expect("a row becomes a todo");
        state
            .exec_editor(Command::ToggleMark {
                id: BlockId(second as u64),
                start: 0,
                end: 2,
                kind: MarkKind::Bold,
                url: String::new(),
                date: None,
            })
            .expect("and a word goes bold");

        let first = state.save_as_template(page);
        assert_eq!(
            state.workspace.borrow().title_of(first),
            state.workspace.borrow().title_of(page),
            "the menu names a template after the page it came from"
        );
        let src = state.doc.borrow().page_blocks(super::core_page_id(page)).to_vec();
        let copied = state
            .doc
            .borrow()
            .page_blocks(super::core_page_id(first))
            .to_vec();
        assert_eq!(copied.len(), src.len(), "every row of the page is in the body");
        assert_eq!(copied[0].kind, BlockKind::Heading1);
        assert_eq!(copied[1].kind, BlockKind::Todo);
        assert_eq!(copied[1].checked, src[1].checked);
        assert_eq!(copied[1].marks.len(), 1, "an inline mark rides along with its row");
        assert_eq!(copied[1].text, src[1].text);
        assert!(
            copied.iter().zip(&src).all(|(c, s)| c.id != s.id),
            "a copy sharing an id with its source is one row in two pages"
        );
        assert_eq!(
            state.template_list(),
            vec![(first, state.workspace.borrow().title_of(page).unwrap().to_string())],
            "one template, named after the page"
        );

        // Saving the same page again does not touch the first body.
        let again = state.save_as_template(page);
        assert_ne!(again, first);
        assert_eq!(state.template_list().len(), 2, "two templates, one name");
        let kept = state
            .doc
            .borrow()
            .page_blocks(super::core_page_id(first))
            .to_vec();
        assert_eq!(kept.len(), 2, "the older copy still has its rows");
        assert_eq!(
            kept.iter().map(|b| b.id).collect::<Vec<_>>(),
            copied.iter().map(|b| b.id).collect::<Vec<_>>(),
            "and the same rows — a save never rewrote it"
        );

        // Deleting one of them is the way out, so it had better be surgical.
        // (`delete_page` answers "was it the page on screen", not "did it
        // work" -- for a template that is always false, so the proof is the
        // row count and the surviving body.)
        assert!(state.workspace.borrow().contains(again));
        state.delete_page(again);
        assert!(
            !state.workspace.borrow().contains(again),
            "the picker's Delete row is not a lie"
        );
        assert_eq!(state.template_list().len(), 1);
        assert_eq!(
            state
                .doc
                .borrow()
                .page_blocks(super::core_page_id(first))
                .len(),
            2,
            "the survivor is untouched by its twin's deletion"
        );
        assert!(state.workspace.borrow().contains(page), "and the page it came from least of all");
    }

    /// Two anchor cases, one command. An empty line is *replaced* — the "+" row
    /// and a new page's first row are both empty, and a template arriving one
    /// row below the caret looks like it missed — while a line with words keeps
    /// them and takes the copy below. Both have to come back on a single
    /// Ctrl+Z, which is what `exec_all`'s one-batch-one-step rule buys.
    #[test]
    fn inserting_a_template_replaces_an_empty_line_and_undoes_as_one_step() {
        use super::AppState;
        use crate::core::{BlockId, Command};

        let state = AppState::new(&plain_args(), None);
        state.create_page(None);
        let t = state.import_template("Two lines", "one\n\ntwo\n");
        let host = state.start_page().expect("a new page takes its first row");

        let first = state
            .insert_template(Some(host), t)
            .expect("the empty line takes the copy");
        let rows = words(&state);
        assert_eq!(
            rows.iter().map(|(_, _, w)| w.as_str()).collect::<Vec<_>>(),
            ["one", "two"],
            "the line it replaced left no empty row behind"
        );
        assert_eq!(first, rows[0].0, "the id it hands back is the row the caret goes to");
        assert!(
            state.doc.borrow().block(BlockId(host as u64)).is_none(),
            "and that row is really gone from the document"
        );

        state.undo_open_page();
        let back = words(&state);
        assert_eq!(back.len(), 1, "one step undid the whole copy");
        assert_eq!(back[0].0, host, "the replaced line is back");
        assert!(back[0].2.is_empty(), "with nothing written on it");

        // The other case: the anchor keeps its words.
        state
            .exec_editor(Command::ReplaceText {
                id: BlockId(host as u64),
                text: "what I typed".into(),
            })
            .expect("typing");
        assert!(state.insert_template(Some(host), t).is_some());
        let rows = words(&state);
        assert_eq!(
            rows.iter().map(|(_, _, w)| w.as_str()).collect::<Vec<_>>(),
            ["what I typed", "one", "two"],
            "the copy arrived below a line that has content"
        );
        state.undo_open_page();
        assert_eq!(words(&state).len(), 1, "again in one step");

        // `None` anchor is the ⋯ menu's "append to this page".
        assert!(state.insert_template(None, t).is_some());
        let rows = words(&state);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].2, "two", "and it landed at the end");

        // A template is not consumed by being used.
        assert_eq!(
            state
                .doc
                .borrow()
                .page_blocks(super::core_page_id(t))
                .len(),
            2
        );
    }

    /// The two id encodings this feature adds to existing popups. A slash row
    /// for a template must *not* read as a block kind: `kind_from_int` folds an
    /// unknown number into Paragraph, which would quietly turn the user's line
    /// into an empty paragraph. And the ⋯ picker indexes into a list that can
    /// change under it, so an out-of-range or stale row has to answer `None`
    /// rather than "the first template".
    #[test]
    fn a_picker_row_names_a_template_and_never_a_block_kind() {
        use super::{
            AppState, MENU_TEMPLATE_DELETE, MENU_TEMPLATE_INSERT, MENU_TEMPLATE_PICK_BACK,
            TEMPLATE_PICK_BASE, TEMPLATE_SLASH_BASE,
        };
        use crate::core::BlockKind;
        use slint::Model as _;

        let state = AppState::new(&plain_args(), None);
        let meeting = state.import_template("Meeting notes", "## Agenda\n");
        let weekly = state.import_template("Weekly review", "## Shipped\n");
        assert_eq!(state.template_list().len(), 2);

        state.open_slash("meet");
        let rows: Vec<(i32, String, String)> = (0..state.slash.row_count())
            .filter_map(|i| state.slash.row_data(i))
            .map(|r| (r.id, r.label.to_string(), r.hint.to_string()))
            .collect();
        let picked = rows
            .iter()
            .position(|(id, _, _)| *id >= TEMPLATE_SLASH_BASE)
            .expect("one template row survived the filter");
        assert_eq!(rows[picked].1, "Meeting notes");
        assert_eq!(rows[picked].2, "Template", "a template has no breadcrumb to show");
        assert_eq!(
            state.slash_selected_template(picked as i32),
            Some((meeting, "Meeting notes".into())),
            "and the row names the page it copies from"
        );
        assert!(
            state.slash_selected_kind(picked as i32).is_none(),
            "the same row is not a block kind"
        );
        // The tail's ids index the *library*, not the rows currently visible.
        // A needle that matches only a later template must still name that one:
        // number the rows after filtering and this popup's single row becomes
        // "the first template", which inserts a body the user never read.
        state.open_slash("weekly");
        assert_eq!(state.slash.row_count(), 1, "and no block kind answers to it");
        assert_eq!(
            state.slash_selected_template(0),
            Some((weekly, "Weekly review".into())),
            "row 0 of a one-row popup is still the second template in the library"
        );
        // control: with a filter no template answers to, a real kind row still
        // works both ways -- it names a block kind and it names no template.
        // ("meet" above matches no kind label, so it proves the tail stands
        // alone but cannot host a kind row.)
        state.open_slash("tog");
        let kind_row = (0..state.slash.row_count())
            .filter_map(|i| state.slash.row_data(i).map(|r| (i, r)))
            .find(|(_, r)| r.label == "Toggle list")
            .expect("the filter keeps the kinds it matches");
        assert_eq!(
            state.slash_selected_kind(kind_row.0 as i32),
            Some(BlockKind::Toggle)
        );
        assert!(
            state.slash_selected_template(kind_row.0 as i32).is_none(),
            "and a kind row is not a template"
        );

        // No filter: the templates are the tail, in library order.
        state.open_slash("");
        let count = state.slash.row_count();
        assert_eq!(
            (0..count)
                .filter_map(|i| state.slash.row_data(i))
                .filter(|r| r.id >= TEMPLATE_SLASH_BASE)
                .map(|r| r.label.to_string())
                .collect::<Vec<_>>(),
            ["Meeting notes", "Weekly review"],
            "both are offered, oldest first, after every block kind"
        );
        // The "+" popup offers them too, as the same tail after its own items.
        state.open_slash_insert("");
        assert_eq!(
            (0..state.slash.row_count())
                .filter_map(|i| state.slash.row_data(i))
                .filter(|r| r.id >= TEMPLATE_SLASH_BASE)
                .map(|r| r.label.to_string())
                .collect::<Vec<_>>(),
            ["Meeting notes", "Weekly review"],
            "the + menu ends in the library as well"
        );
        state.open_slash("qqqqq");
        assert_eq!(state.slash.row_count(), 0, "an unmatched filter matches no template either");

        // ---- the ⋯ picker ----
        state.fill_template_pick(MENU_TEMPLATE_DELETE);
        assert_eq!(state.template_pick_action(), MENU_TEMPLATE_DELETE);
        let menu: Vec<(i32, String, bool)> = (0..state.menu.row_count())
            .filter_map(|i| state.menu.row_data(i))
            .map(|r| (r.id, r.label.to_string(), r.danger))
            .collect();
        assert_eq!(menu[0].0, MENU_TEMPLATE_PICK_BACK);
        assert_eq!(
            menu[1],
            (TEMPLATE_PICK_BASE, "Meeting notes".into(), true),
            "a delete picker draws its rows in the danger colour"
        );
        assert_eq!(menu[2].2, true);
        assert_eq!(
            state.template_picked(TEMPLATE_PICK_BASE + 1),
            Some((
                state.template_list()[1].0,
                "Weekly review".into()
            ))
        );
        assert!(state.template_picked(TEMPLATE_PICK_BASE + 9).is_none(), "a stale index is nothing");
        assert!(state.template_picked(MENU_TEMPLATE_INSERT).is_none(), "and so is a row from the menu above");

        // An emptied library: the picker has only its Back row, and every index
        // into it is out of range.
        for (id, _) in state.template_list() {
            state.delete_page(id);
        }
        state.fill_template_pick(MENU_TEMPLATE_INSERT);
        assert_eq!(state.menu.row_count(), 1);
        assert!(state.template_picked(TEMPLATE_PICK_BASE).is_none());
    }

    /// The library a first start writes (SPEC §三十八 "预置若干本地模板"), once per
    /// database. Three sessions on one file because the two guards fail in
    /// opposite directions: without the settings flag every start adds five more
    /// hidden pages, and without the name check a session that died halfway
    /// through the seed lands the library twice. A deletion has to survive the
    /// flag too, or the menu's Delete row is a lie — the built-ins would be
    /// furniture bolted to the floor.
    #[test]
    fn the_builtin_library_lands_once_and_a_deletion_sticks() {
        use super::AppState;
        use crate::core::template::PRESETS;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("template-seed");
        let repo = scratch_repo(&dir);
        let first = AppState::new(&plain_args(), Some(repo.clone()));
        assert_eq!(
            first
                .template_list()
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>(),
            PRESETS.iter().map(|p| p.name).collect::<Vec<_>>(),
            "a fresh library gets every preset, in the order the menu shows them"
        );
        assert_eq!(
            first.workspace.borrow().page_count(),
            first.workspace.borrow().visible_page_count() + PRESETS.len(),
            "pages in the database, none of them on screen"
        );
        for (id, name) in first.template_list() {
            assert!(
                !first
                    .doc
                    .borrow()
                    .page_blocks(super::core_page_id(id))
                    .is_empty(),
                "{name} arrived with a body, not as a blank page"
            );
        }
        first.persistence_force_flush();
        drop(first);

        let second = AppState::new(&plain_args(), Some(repo.clone()));
        assert_eq!(
            second.template_list().len(),
            PRESETS.len(),
            "a second start does not double the library"
        );
        // A restart is also the moment a hidden page could reattach itself: the
        // load path rebuilds `roots` and `children` out of the rows it read.
        let ids: Vec<i32> = second.template_list().iter().map(|(id, _)| *id).collect();
        let tree = second.workspace.borrow().dfs_order();
        assert!(
            ids.iter().all(|id| !tree.contains(id)),
            "and after the restart every one of them is still out of the tree"
        );
        for id in &ids {
            assert!(
                !second
                    .doc
                    .borrow()
                    .page_blocks(super::core_page_id(*id))
                    .is_empty(),
                "the rows a template is made of came back too"
            );
        }
        let meeting = ids[0];
        // `delete_page` answers "was the deleted page the one on screen", which
        // a template never is; the assertion is that the row is gone.
        second.delete_page(meeting);
        assert!(
            !second.workspace.borrow().contains(meeting),
            "and it went the moment it was asked"
        );
        second.persistence_force_flush();
        drop(second);
        drop(repo);

        let third = AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        assert_eq!(
            third.template_list().len(),
            PRESETS.len() - 1,
            "this library was seeded already, so nothing came back"
        );
        assert!(!third.is_template(meeting), "and the deleted one is not a page any more");
    }

    /// §三十八 sends a template's import and export through §二十六's Markdown
    /// channel, so the claim worth testing is that the channel is a *loop*: a
    /// body exported and re-imported reads as the same rows. The presets are
    /// included because they are written in that channel rather than stored in
    /// a second format, which is exactly what §三十八 forbids.
    #[test]
    fn a_template_round_trips_through_the_markdown_channel() {
        use super::AppState;
        use crate::core::template::PRESETS;

        let state = AppState::new(&plain_args(), None);
        let texts = |state: &AppState, id: i32| {
            state
                .doc
                .borrow()
                .page_blocks(super::core_page_id(id))
                .iter()
                .map(|b| b.text.clone())
                .collect::<Vec<_>>()
        };

        let page = state.create_page(None);
        assert!(
            state.template_markdown(page).is_none(),
            "Export template must not answer for the page it is not"
        );

        let t = state.import_template("Imported", "# Title\n\n> quoted\n\n- [x] done\n\n1. first\n");
        assert_eq!(
            texts(&state, t),
            ["Title", "quoted", "done", "first"],
            "the parser turned a file into rows, on a page nobody can open"
        );
        let md = state.template_markdown(t).expect("a template exports");
        assert!(
            md.contains("# Title") && md.contains("- [x] done") && md.contains("1. first"),
            "and the export says the same thing back: {md}"
        );
        let again = state.import_template("Imported again", &md);
        assert_eq!(texts(&state, again), texts(&state, t), "in one file, out one file");

        for preset in PRESETS {
            let id = state.import_template(preset.name, preset.markdown);
            let body = texts(&state, id);
            assert!(
                !body.iter().all(String::is_empty),
                "{} arrived empty",
                preset.name
            );
            let exported = state.template_markdown(id).expect("a preset exports");
            let back = state.import_template(&format!("{} again", preset.name), &exported);
            assert_eq!(
                texts(&state, back),
                body,
                "{} does not survive its own export:\n{exported}",
                preset.name
            );
        }
    }

    /// A template writes blocks, so the lock covers it like any other edit and
    /// says no out loud (§三十八 refuses a silent swallow). What the lock leaves
    /// alone matters as much: saving a locked page as a template, or starting a
    /// new page from one, take information out and put it somewhere unlocked —
    /// which is the same reasoning that keeps two read-only rows in the ⋮⋮ menu.
    #[test]
    fn a_locked_page_refuses_a_template_and_still_offers_to_save_one() {
        use super::AppState;

        let state = AppState::new(&plain_args(), None);
        let page = state.create_page(None);
        let host = state.start_page().expect("a new page takes its first row");
        add_line(&state, host, "kept words");
        let t = state.import_template("Body", "inserted\n");
        state.set_page_locked(page, true);
        assert!(state.page_locked());
        let queued = || state.db_notice.borrow().len();
        let quiet = queued();

        assert!(
            state.insert_template(Some(host), t).is_none(),
            "the copy at a row is refused"
        );
        assert!(
            state.insert_template(None, t).is_none(),
            "so is the append"
        );
        assert_eq!(words(&state).len(), 2, "and the page did not grow");
        assert!(queued() > quiet, "the refusal was said out loud");
        assert!(
            state.db_notice.borrow().last().unwrap().contains("locked"),
            "and it named the reason"
        );
        state.undo_open_page();
        assert_eq!(words(&state).len(), 2, "undo is behind the same door");

        let saved = state.save_as_template(page);
        assert_eq!(
            state
                .doc
                .borrow()
                .page_blocks(super::core_page_id(saved))
                .len(),
            2,
            "a locked page can still be copied *out*"
        );
        // The new page is a tree operation and it is not the locked one, so the
        // copy lands there — which is also why `insert_template` reads
        // `open_page` rather than the id the menu was opened on.
        let made = state.new_page_from_template(Some(page), t);
        assert_ne!(made, page);
        assert_eq!(state.open_page.get(), made, "and creating one navigates to it");
        assert_eq!(words(&state).len(), 1, "with its body, on a page that can be edited");
        assert_eq!(state.workspace.borrow().title_of(made), Some("Body"));
        state.open_page(page);
        assert_eq!(words(&state).len(), 2, "the locked page is exactly as it was");
    }

    /// "新建页面时选模板": the page it makes is an ordinary page — in the tree, in
    /// the sidebar, openable, on its own undo stack — while the body it was
    /// filled from stays hidden. This is the one place both roles sit side by
    /// side, so it is where the split is worth pinning.
    #[test]
    fn a_page_from_a_template_is_an_ordinary_page() {
        use super::AppState;
        use crate::core::BlockKind;
        use slint::Model as _;

        let state = AppState::new(&plain_args(), None);
        let parent = state.create_page(None);
        let t = state.import_template("Meeting notes", "## Agenda\n\n- [ ] action\n");
        let new = state.new_page_from_template(Some(parent), t);

        let ws = state.workspace.borrow();
        assert!(
            ws.children_of(Some(parent)).contains(&new),
            "it is a page of the tree"
        );
        assert!(!ws.is_template(new));
        assert_eq!(ws.title_of(new), Some("Meeting notes"), "named after its body");
        drop(ws);
        state.open_page(new);
        assert_eq!(state.open_page.get(), new, "which is to say: it opens");
        assert_eq!(
            words(&state).iter().map(|(_, k, _)| *k).collect::<Vec<_>>(),
            vec![BlockKind::Heading2, BlockKind::Todo],
            "with the template's rows still their own kinds"
        );
        let sidebar: Vec<i32> = (0..state.sidebar.row_count())
            .filter_map(|i| state.sidebar.row_data(i))
            .map(|r| r.id)
            .collect();
        assert!(sidebar.contains(&new), "the sidebar lists the page");
        assert!(!sidebar.contains(&t), "and never the template");

        state.undo_open_page();
        assert!(words(&state).is_empty(), "the copy was one step on this page's stack");
        assert_eq!(
            state
                .doc
                .borrow()
                .page_blocks(super::core_page_id(t))
                .len(),
            2,
            "and the library still has what it had"
        );
    }

    // --- version history (SPEC §三十八, ADR-0091) ---

    /// A session on a real library file, with one page holding `lines` as its
    /// paragraphs. A version *is* a file, so every test below needs the database
    /// a version can be copied out of — the mock session other tests use has no
    /// file at all, which is its own test further down.
    fn version_session(
        dir: &crate::testing::ScratchDir,
        lines: &[&str],
    ) -> (
        std::rc::Rc<super::AppState>,
        std::sync::Arc<crate::storage::SqliteRepository>,
        i32,
        std::path::PathBuf,
    ) {
        use crate::core::{BlockKind, Command};
        let path = dir.path().join("library.db");
        let repo = scratch_repo(dir);
        let state = super::AppState::new(&plain_args(), Some(repo.clone()));
        let page = state.create_page(None);
        for line in lines {
            state.exec_on_open_page(Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: (*line).into(),
            });
        }
        (state, repo, page, path)
    }

    /// The page's paragraphs, top to bottom, as the editor shows them.
    fn lines_of(state: &super::AppState) -> Vec<String> {
        state
            .doc
            .borrow()
            .page_blocks(super::core_page_id(state.open_page.get()))
            .iter()
            .map(|b| b.text.clone())
            .collect()
    }

    fn version_of(state: &super::AppState, page: i32, row: usize) -> (i64, String) {
        state
            .version_at(page, row as i32)
            .unwrap_or_else(|| panic!("row {row} of {:?}", state.page_versions(page)))
    }

    #[test]
    fn a_version_names_the_page_as_the_click_saw_it_not_as_the_last_flush_saw_it() {
        // The order inside `save_page_version` is the whole promise: `VACUUM
        // INTO` reads the file, so a version taken over a queue of unwritten
        // edits would be a picture of the page as of ten seconds ago.
        use crate::core::persistence::Repository as _;
        use crate::testing::ScratchDir;
        let dir = ScratchDir::new("version-flush-first");
        let (state, _repo, page, path) = version_session(&dir, &["first line"]);
        let created = {
            let label = state.save_page_version(page, "empty").unwrap();
            assert_eq!(label, "empty");
            let (created, _) = version_of(&state, page, 0);
            created
        };
        assert_eq!(
            crate::storage::versions::read(&path, page as i64, created)
                .unwrap()
                .1
                .len(),
            1,
            "control: the version holds the one line the file had"
        );

        // an edit the debounced writer has not been given a moment to reach
        state
            .exec_on_open_page(crate::core::Command::AppendBlock {
                kind: crate::core::BlockKind::Paragraph,
                text: "second line".into(),
            })
            .expect("a paragraph appends");
        assert_eq!(
            _repo
                .load()
                .unwrap()
                .blocks
                .iter()
                .filter(|b| b.page.0 as i64 == page as i64)
                .count(),
            1,
            "control: the edit really is still in memory"
        );

        let created = {
            state.save_page_version(page, "both").unwrap();
            version_of(&state, page, 0).0
        };
        let (_page_row, blocks) =
            crate::storage::versions::read(&path, page as i64, created).unwrap();
        assert_eq!(
            blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
            vec!["first line", "second line"],
            "the save flushed before it copied"
        );
    }

    #[test]
    fn a_version_with_no_name_is_called_by_which_one_it_is() {
        use crate::testing::ScratchDir;
        let dir = ScratchDir::new("version-label");
        let (state, _repo, page, _path) = version_session(&dir, &["one"]);
        assert_eq!(state.save_page_version(page, "").unwrap(), "Version 1");
        assert_eq!(state.save_page_version(page, "   ").unwrap(), "Version 2");
        // one line, trimmed, and cut where a person can still read it
        assert_eq!(
            state.save_page_version(page, "  spaced\nout  ").unwrap(),
            "spacedout"
        );
        let long = "x".repeat(90);
        assert_eq!(state.save_page_version(page, &long).unwrap().len(), 60);
        assert_eq!(state.page_versions(page).len(), 4);
        // and the newest is the first row the panel draws
        assert_eq!(version_of(&state, page, 0).1, "x".repeat(60));
    }

    #[test]
    fn the_oldest_versions_are_the_ones_that_go() {
        use crate::core::persistence::Repository;
        use crate::storage::versions::{self, MAX_PER_PAGE};
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-retention");
        let (state, repo, page, path) = version_session(&dir, &["one"]);
        state.persistence_force_flush();
        // Twenty versions already on file, so the next save is the one that has
        // to free room. Written through the database rather than the app
        // because the point is the *count*, and taking twenty in one session
        // would mean waiting out the clock.
        let first = versions::save(&repo, page as i64, 1_001).unwrap().unwrap();
        assert!(first.is_empty());
        let mut stale = Vec::new();
        for created in 1_001..1_001 + MAX_PER_PAGE as i64 {
            if created > 1_001 {
                std::fs::copy(
                    versions::path_for(&path, page as i64, 1_001),
                    versions::path_for(&path, page as i64, created),
                )
                .unwrap();
            }
            stale.push(created);
        }
        repo.apply(
            &stale
                .iter()
                .flat_map(|created| {
                    [
                        crate::core::Change::MetaSet {
                            key: versions::label_key(page as i64, *created),
                            value: format!("Version {created}"),
                        },
                        crate::core::Change::MetaSet {
                            key: versions::files_key(page as i64, *created),
                            value: String::new(),
                        },
                    ]
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        drop(state);

        // a session that loads that library mirrors all twenty
        let state = super::AppState::new(&plain_args(), Some(repo.clone()));
        assert_eq!(state.page_versions(page).len(), MAX_PER_PAGE);
        state.save_page_version(page, "the new one").unwrap();

        let list = state.page_versions(page);
        assert_eq!(list.len(), MAX_PER_PAGE, "the cap holds, not one over");
        assert_eq!(list[0].1, "the new one", "and the newest is never the one that goes");
        assert!(
            !versions::path_for(&path, page as i64, 1_001).exists(),
            "the oldest version's file went with it"
        );
        assert!(versions::path_for(&path, page as i64, 1_002).exists());
        state.persistence_force_flush();
        let meta = repo.load().unwrap().meta;
        assert!(
            !meta.contains_key(&versions::label_key(page as i64, 1_001)),
            "its index row went too, or the panel would offer a missing file"
        );
        assert!(!meta.contains_key(&versions::files_key(page as i64, 1_001)));
    }

    #[test]
    fn a_version_outlives_the_session_that_took_it() {
        use crate::storage::versions;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-restart");
        let (state, _repo, page, path) = version_session(&dir, &["written once"]);
        state.save_page_version(page, "before the rewrite").unwrap();
        let (created, label) = version_of(&state, page, 0);
        state.persistence_force_flush();
        drop(state);

        let repo = scratch_repo(&dir);
        let state = super::AppState::new(&plain_args(), Some(repo));
        assert_eq!(
            state.page_versions(page),
            vec![(created, label.clone())],
            "the index came back out of the database's own rows"
        );
        let blocks = versions::read(&path, page as i64, created).unwrap().1;
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "written once");
        // and the panel's row is the same row: an index of one is row 0
        assert_eq!(state.version_at(page, 0).unwrap().1, label);
        assert_eq!(state.version_at(page, 1), None, "a stale row resolves to nothing");
        assert_eq!(state.version_at(page, -1).unwrap().0, created);
    }

    #[test]
    fn comparing_a_version_says_what_changed_since_it_and_says_so_when_nothing_did() {
        use crate::core::diff::DiffMark;
        use crate::core::{BlockKind, Command};
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-diff");
        let (state, _repo, page, _path) = version_session(&dir, &["keep me", "change me"]);
        state.save_page_version(page, "before").unwrap();
        let (created, _) = version_of(&state, page, 0);
        assert_eq!(
            state.version_diff(page, created).unwrap().len(),
            0,
            "a version of what is on screen differs from nothing"
        );

        let second = {
            let doc = state.doc.borrow();
            doc.page_blocks(super::core_page_id(page))[1].id
        };
        state
            .exec_on_open_page(Command::ReplaceText {
                id: second,
                text: "changed".into(),
            })
            .unwrap();
        state
            .exec_on_open_page(Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: "new line".into(),
            })
            .unwrap();

        let lines = state.version_diff(page, created).unwrap();
        assert_eq!(
            lines
                .iter()
                .map(|l| (l.mark, l.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (DiffMark::Removed, "change me"),
                (DiffMark::Added, "changed"),
                (DiffMark::Added, "new line"),
            ],
            "the edited line is one pair, and the untouched head costs no row"
        );
        assert_eq!(
            super::version_diff_note(&lines),
            "2 lines arrived, 1 went. Restoring puts every one of them back."
        );
        assert!(
            state.version_diff(page, 999_999).is_err(),
            "a row that names no file says so instead of showing an empty page"
        );
    }

    #[test]
    fn restoring_a_version_is_one_undo_step_and_leaves_the_pages_name_alone() {
        use crate::core::{BlockKind, Command};
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-restore");
        let (state, _repo, page, _path) = version_session(&dir, &["as it was"]);
        state.save_page_version(page, "then").unwrap();
        let (created, _) = version_of(&state, page, 0);

        // what the restore has to replace: two rewritten lines and a subtree
        let first = state.doc.borrow().page_blocks(super::core_page_id(page))[0].id;
        state
            .exec_on_open_page(Command::ReplaceText { id: first, text: "rewritten".into() })
            .unwrap();
        let anchor = state
            .exec_on_open_page(Command::AppendBlock {
                kind: BlockKind::Heading1,
                text: "a heading".into(),
            })
            .unwrap();
        let heading = match &anchor[0] {
            crate::core::Change::BlockInserted(b) => b.id,
            other => panic!("expected the inserted heading, got {other:?}"),
        };
        state
            .exec_on_open_page(Command::InsertBlockAfter {
                id: heading,
                kind: BlockKind::Bullet,
                text: "its child".into(),
            })
            .unwrap();
        state.rename_page(page, "Renamed since");
        assert_eq!(
            lines_of(&state).len(),
            3,
            "control: two roots and the bullet that hangs off one of them"
        );

        let count = state.restore_version(page, created).unwrap();
        assert_eq!(count, 1, "one line came back");
        assert_eq!(lines_of(&state), vec!["as it was"]);
        assert_eq!(
            state.workspace.borrow().title_of(page).unwrap(),
            "Renamed since",
            "a version is of a page's content, not of its name"
        );

        // the whole replacement was ONE step: one Ctrl+Z, and the four rows the
        // restore swept away are back
        state.undo_open_page().expect("the restore is undoable");
        assert_eq!(
            lines_of(&state),
            vec!["rewritten", "a heading", "its child"],
            "undo puts back what the restore took, subtree included"
        );
    }

    #[test]
    fn a_locked_page_refuses_the_restore_it_was_asked_to_do() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-restore-locked");
        let (state, _repo, page, _path) = version_session(&dir, &["as it was"]);
        state.save_page_version(page, "then").unwrap();
        let (created, _) = version_of(&state, page, 0);
        let first = state.doc.borrow().page_blocks(super::core_page_id(page))[0].id;
        state
            .exec_on_open_page(crate::core::Command::ReplaceText {
                id: first,
                text: "edited while unlocked".into(),
            })
            .unwrap();

        state.set_page_locked(page, true);
        assert_eq!(
            state.restore_version(page, created),
            Err("this page is locked".into())
        );
        assert_eq!(
            lines_of(&state),
            vec!["edited while unlocked"],
            "and the page is exactly as it was before the refusal"
        );
        // ADR-0048: the lock line is the one that says what to do about it, so
        // the refusal has to leave it standing.
        state.set_page_locked(page, false);
        assert_eq!(state.restore_version(page, created).unwrap(), 1);
    }

    #[test]
    fn a_version_is_restored_only_into_the_page_it_was_taken_from() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-restore-other-page");
        let (state, _repo, first, _path) = version_session(&dir, &["page one"]);
        state.save_page_version(first, "of page one").unwrap();
        let (created, _) = version_of(&state, first, 0);
        let second = state.create_page(None);
        assert_eq!(state.open_page.get(), second, "creating navigates");

        assert_eq!(
            state.restore_version(first, created),
            Err("open that page before restoring its version".into()),
            "a row that went stale under a page switch restores nothing"
        );
        assert!(lines_of(&state).is_empty(), "and the open page is untouched");
    }

    #[test]
    fn deleting_a_page_forgets_the_versions_it_had() {
        use crate::core::persistence::Repository as _;
        use crate::storage::versions;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-page-delete");
        let (state, repo, first, path) = version_session(&dir, &["page one"]);
        let second = state.create_page(None);
        state.exec_on_open_page(crate::core::Command::AppendBlock {
            kind: crate::core::BlockKind::Paragraph,
            text: "page two's own line".into(),
        });
        state.save_page_version(first, "of page one").unwrap();
        state.save_page_version(second, "of page two").unwrap();
        state.persistence_force_flush();
        let (kept, gone) = (
            version_of(&state, first, 0).0,
            version_of(&state, second, 0).0,
        );
        assert!(versions::path_for(&path, second as i64, gone).exists());

        assert!(state.delete_page(second), "the page goes");
        assert!(
            !versions::path_for(&path, second as i64, gone).exists(),
            "and its version file with it — a page nobody can open has no reason to keep its past"
        );
        assert!(state.page_versions(second).is_empty());
        assert_eq!(state.page_versions(first).len(), 1, "another page's history stays");
        assert!(versions::path_for(&path, first as i64, kept).exists());

        state.persistence_force_flush();
        let meta = repo.load().unwrap().meta;
        assert!(!meta.contains_key(&versions::label_key(second as i64, gone)));
        assert!(!meta.contains_key(&versions::files_key(second as i64, gone)));
        assert!(meta.contains_key(&versions::label_key(first as i64, kept)));
    }

    #[test]
    fn a_version_pins_the_picture_its_page_no_longer_shows() {
        use crate::core::{BlockId, Command};
        use crate::storage::versions;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("version-reclaim");
        let path = dir.path().join("library.db");
        let (state, _repo, page, pics) = session_with_pictures(&dir, 1);
        state.save_page_version(page, "with the picture").unwrap();
        let created = version_of(&state, page, 0).0;
        let file = dir.path().join("attachments").join(&pics[0].1);
        state
            .exec_on_open_page(Command::DeleteBlock { id: BlockId(pics[0].2 as u64) })
            .expect("the picture's row goes");
        state.persistence_force_flush();
        drop(state);

        // A fresh session, so the undo stack is empty and the only pointer left
        // is the one the version mirrored. Without the restart this would be
        // proving §三十七's history pin instead of this one.
        let repo = scratch_repo(&dir);
        let state = super::AppState::new(&plain_args(), Some(repo));
        assert!(
            state.history.borrow().referenced_attachments().is_empty(),
            "control: nothing on this session's stack points at the picture"
        );
        assert_eq!(
            state.page_versions(page).len(),
            1,
            "control: and the version is still listed"
        );

        assert_eq!(
            state.reclaim_attachments().unwrap(),
            "no unused attachments to remove",
            "the version holds the pointer, so the bytes are not unused"
        );
        assert!(file.is_file(), "and nothing was deleted");

        state.delete_version(page, created).unwrap();
        assert!(!versions::path_for(&path, page as i64, created).exists());
        let notice = state.reclaim_attachments().unwrap();
        assert!(
            notice.starts_with("1 unused attachment removed ("),
            "with the version gone the picture is finally orphaned: {notice}"
        );
        assert!(!file.exists(), "and its bytes went with it");
    }

    #[test]
    fn a_session_with_no_database_file_has_nowhere_to_put_a_version() {
        // SPEC §三十八's one honest limit: a version is a file, so a session
        // that writes no file keeps no versions. The panel's empty state is the
        // user's answer to this; the error strings are the controller's.
        let state = super::AppState::new(&plain_args(), None);
        let page = state.open_page.get();
        assert!(page > 0, "the mock session has a page open");
        assert_eq!(
            state.save_page_version(page, "x").unwrap_err(),
            "this session has no database file, so a version has nowhere to live"
        );
        assert!(state.page_versions(page).is_empty());
        assert!(state.version_diff(page, 1).is_err());
        assert!(state.delete_version(page, 1).is_err());
        assert!(state
            .restore_version(page, 1)
            .unwrap_err()
            .contains("no database file"));
    }

    #[test]
    fn the_panels_rows_and_sentences_read_off_the_same_numbers() {
        use super::{version_heading, version_rows, versions_note};
        use crate::core::diff::{DiffLine, DiffMark};
        use crate::core::BlockKind;
        use slint::Model;

        let now = super::now_secs();
        let rows = version_rows(&[
            (now - 2 * 3_600, "Two hours".into()),
            (now - 40 * 60, "Forty minutes".into()),
        ]);
        assert_eq!(rows[0].index, 0, "a row is addressed by its place, not its time");
        assert_eq!(rows[0].label, "Two hours");
        assert_eq!(rows[0].when, "2 h ago");
        assert_eq!(rows[1].when, "40 min ago");
        assert_eq!(version_rows(&[]).len(), 0);

        assert_eq!(
            versions_note(1),
            format!("1 version of this page. Quire keeps the {} newest and lets the oldest go.", crate::storage::versions::MAX_PER_PAGE)
        );
        assert!(versions_note(3).starts_with("3 versions"));
        assert!(versions_note(0).starts_with("0 versions"));
        assert_eq!(version_heading("Draft", now - 90), "“Draft” · 1 min ago");

        // `fill_versions` is that projection pushed: a session with no window
        // attached still gets the model right, which is what the panel draws.
        let dir = crate::testing::ScratchDir::new("version-rows-model");
        let (state, _repo, page, _path) = version_session(&dir, &["one"]);
        state.save_page_version(page, "named").unwrap();
        state.fill_versions(page);
        assert_eq!(state.versions.row_count(), 1);
        assert_eq!(state.versions.row_data(0).unwrap().label, "named");
        state.fill_version_diff(
            &[DiffLine { mark: DiffMark::Added, kind: BlockKind::Todo, text: "x".into() }],
            "heading",
            "note",
        );
        assert_eq!(state.version_diff.row_count(), 1);
        let row = state.version_diff.row_data(0).unwrap();
        assert!(row.added);
        assert_eq!(row.kind, "To-do");
    }

    // ─── M14: the database's two computed kinds, through a real session ─────
    //
    // `core::database_relation` and `core::database_rollup` pin the rules and
    // the folds; the store's own tests pin the batched reads. Neither can reach
    // the layer that holds a catalog *and* a file at once, and that layer is
    // where the feature either works or does not: a pick that writes one side
    // of a pair, a rollup that reads the whole target table, a cell whose paint
    // never reaches the model the delegate draws. Every test below drives the
    // entry points a delegate calls and then reads back **the model the
    // delegate reads** — `db_windows`, not a second copy of the numbers.

    /// One paragraph at the end of the open page, by id.
    fn a_line(state: &super::AppState) -> i32 {
        use crate::core::{BlockKind, Command};
        state
            .exec_on_open_page(Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: String::new(),
            })
            .as_deref()
            .and_then(find_inserted_block_id)
            .expect("the line lands on the open page")
    }

    /// A fresh database on its own line. The catalog learns it through
    /// `db_absorb` on the way through `record`, so `db_ref_of` and
    /// `db_add_column` work the moment this returns.
    fn a_database(state: &super::AppState) -> i32 {
        let block = a_line(state);
        assert!(state.make_database(block), "the line becomes a database");
        block
    }

    /// One row at the end of `block`'s listing; its record id.
    fn a_row(state: &super::AppState, block: i32) -> i64 {
        let ord = state.db_next_row_ord(block);
        state.db_add_record(block, ord).expect("a row")
    }

    /// Name a row through the ordinary cell path. The title column is the one
    /// ADR-0063's `COALESCE` reads, so this is exactly the string a relation
    /// cell will paint.
    fn name_a_row(state: &super::AppState, block: i32, record: i64, title: &str) {
        let column = title_column(state, block);
        assert!(state.db_set_cell_text(block, record, column, title), "the row is named");
    }

    /// A database's title column, by id.
    fn title_column(state: &super::AppState, block: i32) -> i32 {
        let db = state.db_ref_of(block).expect("a database");
        state
            .databases
            .borrow()
            .properties_of(db)
            .find(|property| property.kind.is_title())
            .expect("a title column")
            .id
            .as_u64() as i32
    }

    /// The database id behind a block, as a config document stores it.
    fn database_of(state: &super::AppState, block: i32) -> i32 {
        state.db_ref_of(block).expect("a database").as_u64() as i32
    }

    /// The text the delegate draws at `(row, column)` of `block`'s window.
    fn drawn(state: &super::AppState, block: i32, row: usize, column: usize) -> String {
        use slint::Model as _;
        let windows = state.db_windows.borrow();
        let window = windows.get(&block).expect("a window was read");
        window
            .rows
            .row_data(row)
            .expect("the row is inside the window")
            .cells
            .row_data(column)
            .expect("the column is inside the row")
            .text
            .to_string()
    }

    /// Where a property sits among the columns the delegate draws.
    fn column_of(state: &super::AppState, block: i32, property: i32) -> usize {
        let windows = state.db_windows.borrow();
        windows
            .get(&block)
            .expect("a window was read")
            .columns
            .iter()
            .position(|column| column.property.as_u64() as i32 == property)
            .expect("the column is visible")
    }

    /// How many entries the delegate's model holds for `block` — the window's
    /// size, which is the number that must never be the table's.
    fn drawn_rows(state: &super::AppState, block: i32) -> usize {
        use slint::Model as _;
        state
            .db_windows
            .borrow()
            .get(&block)
            .map(|window| window.rows.row_count())
            .unwrap_or(0)
    }

    /// Two databases on one page — `tasks` with a relation column pointing at
    /// `people`, and a relation column in `people` for it to pair with — as the
    /// corner a user would build with three clicks each. Returns
    /// `(state, tasks, people, assignee, assigned)`.
    fn two_databases(
        dir: &crate::testing::ScratchDir,
    ) -> (std::rc::Rc<super::AppState>, i32, i32, i32, i32) {
        use crate::core::database::PropertyKind;

        let state = super::AppState::new(&plain_args(), Some(scratch_repo(dir)));
        state.create_page(None);
        let tasks = a_database(&state);
        let people = a_database(&state);
        let assignee = state
            .db_add_column(tasks, "Assignee", PropertyKind::Relation)
            .expect("a relation column");
        let assigned = state
            .db_add_column(people, "Assigned", PropertyKind::Relation)
            .expect("its back-pointer");
        (state, tasks, people, assignee, assigned)
    }

    /// A plain cell write reaches the model the delegate draws. This is the
    /// assumption every other test in this block rests on, so it is pinned
    /// first: a window that is not re-read after a write is a cell the user
    /// watches not change.
    #[test]
    fn a_cell_write_repaints_the_window_the_delegate_reads() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("cell-repaint");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let note = state
            .db_add_column(block, "Note", PropertyKind::Text)
            .expect("a text column");
        let row = a_row(&state, block);
        let at = column_of(&state, block, note);

        assert_eq!(drawn(&state, block, 0, at), "", "a new cell is empty");
        assert!(state.db_set_cell_text(block, row, note, "hello"));
        assert_eq!(
            drawn(&state, block, 0, at),
            "hello",
            "the committed value is what the delegate draws"
        );
        // And a step back moves it too: the cache holds *painted* rows, so every
        // recorded change — including the ones a Ctrl+Z makes — may move them.
        state.undo_open_page().expect("the edit is on the stack");
        assert_eq!(drawn(&state, block, 0, at), "", "an undo repaints the cell");
        state.redo_open_page().expect("and forward again");
        assert_eq!(drawn(&state, block, 0, at), "hello");
    }

    /// The pairing ADR-0088 is about, and the export of it: both configs land in
    /// **one** batch, so one Ctrl+Z un-declares the two-way relation whole.
    #[test]
    fn a_pairing_writes_both_configs_in_one_undo_step() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("relation-pair");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        let people_id = database_of(&state, people);
        let tasks_id = database_of(&state, tasks);

        assert_eq!(state.db_relation_config(assignee), (-1, -1), "nothing declared yet");
        state
            .db_relation_configure(tasks, assignee, people_id, assigned)
            .expect("the pairing is legal");
        assert_eq!(
            state.db_relation_config(assignee),
            (people_id, assigned),
            "the forward column learns its target and its other half"
        );
        assert_eq!(
            state.db_relation_config(assigned),
            (tasks_id, assignee),
            "and the back-pointer was told where to point in the same gesture"
        );

        state.undo_open_page().expect("the declaration is on the stack");
        assert_eq!(
            state.db_relation_config(assignee),
            (-1, -1),
            "half a pair is a back-pointer that never appears, so both go together"
        );
        assert_eq!(state.db_relation_config(assigned), (-1, -1));
    }

    /// The live-title rule (ADR-0088): the cell stores an id and paints what the
    /// target is called *now*, so a rename moves the cell with no write anywhere
    /// near the relation column.
    #[test]
    fn a_relation_cell_paints_the_targets_live_title() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("relation-live-title");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");

        let ada = a_row(&state, people);
        name_a_row(&state, people, ada, "Ada");
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");
        assert!(
            state.db_pick_relation(tasks, task, assignee, &[ada.to_string()]),
            "a configured relation takes a pick"
        );

        let at = column_of(&state, tasks, assignee);
        assert_eq!(drawn(&state, tasks, 0, at), "Ada");

        // Renaming the target writes `people` and nothing else — no write to the
        // relation column, no write to `tasks` — and the tasks cell follows,
        // because a database write hands the page's other database blocks the
        // chance to re-read (`db_refresh_peers`). That is the whole reason the
        // stored value is an id.
        name_a_row(&state, people, ada, "Ada Lovelace");
        assert_eq!(
            drawn(&state, tasks, 0, at),
            "Ada Lovelace",
            "the id never changed; only what it is called did"
        );
    }

    /// One pick, both sides, one undo (ADR-0088). The back-pointer is a real
    /// cell in the target row's column — not a projection — and it is the same
    /// `Entry` as the forward cell.
    #[test]
    fn one_pick_writes_the_back_pointer_and_one_undo_takes_both_back() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("relation-two-way");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");

        let ada = a_row(&state, people);
        name_a_row(&state, people, ada, "Ada");
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");

        // Nothing anywhere yet.
        let assigned_at = column_of(&state, people, assigned);
        assert_eq!(drawn(&state, people, 0, assigned_at), "");

        assert!(state.db_pick_relation(tasks, task, assignee, &[ada.to_string()]));
        assert_eq!(
            drawn(&state, people, 0, assigned_at),
            "Ship it",
            "the target row gained the source row in its back-pointer column"
        );

        // Dropping the target takes the back-pointer out again, in place.
        assert!(state.db_pick_relation(tasks, task, assignee, &[]));
        assert_eq!(
            drawn(&state, people, 0, assigned_at),
            "",
            "an empty pick un-points both halves"
        );
    }

    /// The refusal that makes a relation cycle unrepresentable (ADR-0088): a
    /// pairing has to be an involution with no fixed points.
    #[test]
    fn a_pairing_that_is_not_an_involution_is_refused_and_changes_nothing() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("relation-refusals");
        let (state, tasks, people, assignee, _assigned) = two_databases(&dir);
        let people_id = database_of(&state, people);
        let note = state
            .db_add_column(people, "Note", PropertyKind::Text)
            .expect("a text column");

        // Itself.
        assert_eq!(
            state.db_relation_configure(tasks, assignee, people_id, assignee).unwrap_err(),
            "a column cannot be its own back-pointer"
        );
        // Not a relation.
        assert_eq!(
            state.db_relation_configure(tasks, assignee, people_id, note).unwrap_err(),
            "the back-pointer must be a relation column"
        );
        // A relation that points at a third database is a coincidence of ids.
        let other = a_database(&state);
        let third = state
            .db_add_column(other, "Third", PropertyKind::Relation)
            .expect("a relation column");
        let _ = state.db_relation_configure(
            other,
            third,
            database_of(&state, people),
            -1,
        );
        assert_eq!(
            state.db_relation_configure(tasks, assignee, people_id, third).unwrap_err(),
            "that column does not point back at this database"
        );
        // None of it landed.
        assert_eq!(state.db_relation_config(assignee), (-1, -1));
        assert_eq!(state.db_relation_config(third), (people_id, -1));
    }

    /// An unconfigured relation is a column with nothing to point *at*: storing
    /// an id nothing can name is a fact the database could not show, so the
    /// write is refused rather than recorded.
    #[test]
    fn a_relation_without_a_target_refuses_every_pick() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("relation-unconfigured");
        let (state, tasks, people, assignee, _assigned) = two_databases(&dir);
        let ada = a_row(&state, people);
        name_a_row(&state, people, ada, "Ada");
        let task = a_row(&state, tasks);

        assert!(
            !state.db_pick_relation(tasks, task, assignee, &[ada.to_string()]),
            "no target, no pick"
        );
        let at = column_of(&state, tasks, assignee);
        assert_eq!(drawn(&state, tasks, 0, at), "", "and the cell stays empty");
        assert!(
            state.db_relation_candidates(assignee, "", 10).is_empty(),
            "the picker has no list to show either"
        );
    }

    /// The candidate list a picker fills itself from, capped where the caller
    /// says: the windowed read's red line is not undone by the back door.
    #[test]
    fn a_relation_picker_lists_the_target_rows_and_honours_its_cap() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("relation-candidates");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");

        for name in ["Ada", "Bob", "Cy"] {
            let row = a_row(&state, people);
            name_a_row(&state, people, row, name);
        }
        let all = state.db_relation_candidates(assignee, "", 10);
        assert_eq!(all.len(), 3, "three people in the target database");
        assert_eq!(all[0].1, "Ada", "the first row's title comes back with its id");
        assert_eq!(state.db_relation_candidates(assignee, "", 2).len(), 2, "the cap holds");
        let searched = state.db_relation_candidates(assignee, "cy", 10);
        assert_eq!(searched.len(), 1, "the needle is a filter, folded case");
        assert_eq!(searched[0].1, "Cy");
        assert!(searched[0].0 > 0, "and the row comes back as a record id");
    }

    /// A deleted target degrades instead of failing (ADR-0051's rule for the
    /// thing a relation actually points at), and the cell still counts it.
    #[test]
    fn a_deleted_target_degrades_to_the_word_and_the_cell_still_counts() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("relation-dangling");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");

        let ada = a_row(&state, people);
        name_a_row(&state, people, ada, "Ada");
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");
        assert!(state.db_pick_relation(tasks, task, assignee, &[ada.to_string()]));

        let at = column_of(&state, tasks, assignee);
        assert_eq!(drawn(&state, tasks, 0, at), "Ada");

        assert!(state.db_delete_record(people, ada), "the target row goes away");
        // The picker's stored value is untouched — deleting a row is not a
        // reason to rewrite everybody who mentioned it.
        assert_eq!(
            state.db_relation_config(assignee),
            (database_of(&state, people), assigned),
            "the relation's own config is not what a dangling target changes"
        );
        assert_eq!(
            drawn(&state, tasks, 0, at),
            crate::core::database_relation::DELETED_RECORD_LABEL,
            "the cell still holds the pick and says what it can no longer name"
        );

        // The rollup over the same relation counts the dangling target: the
        // stored fact is "the user picked this", and nobody un-picked it.
        let total = state
            .db_add_column(tasks, "Assigned count", PropertyKind::Rollup)
            .expect("a rollup column");
        state
            .db_rollup_configure(tasks, total, assignee, -1, 1)
            .expect("count needs no target column");
        assert_eq!(drawn(&state, tasks, 0, column_of(&state, tasks, total)), "1");
    }

    /// The whole of ADR-0089's contract in one place: a rollup reads **one
    /// column of the records the relation names**, folds it with the aggregate
    /// its config names, and paints the result — including the two folds that
    /// read no column at all.
    #[test]
    fn a_rollup_folds_its_six_aggregates_over_the_relation_it_names() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("rollup-six");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        let people_id = database_of(&state, people);
        state
            .db_relation_configure(tasks, assignee, people_id, assigned)
            .expect("the pairing is legal");
        let points = state
            .db_add_column(people, "Points", PropertyKind::Number)
            .expect("a number column");

        let mut targets = Vec::new();
        for (name, points_value) in [("Ada", 4), ("Bob", 6), ("Cy", 2)] {
            let row = a_row(&state, people);
            name_a_row(&state, people, row, name);
            assert!(state.db_set_cell_text(people, row, points, &points_value.to_string()));
            targets.push(row.to_string());
        }
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");
        assert!(state.db_pick_relation(tasks, task, assignee, &targets));

        let total = state
            .db_add_column(tasks, "Total", PropertyKind::Rollup)
            .expect("a rollup column");
        let at = column_of(&state, tasks, total);

        // `Aggregate::ALL`'s order, against 4, 6 and 2.
        for (index, aggregate, expected) in [
            (0, "none", ""),
            (1, "count", "3"),
            (2, "sum", "12"),
            (3, "average", "4"),
            (4, "min", "2"),
            (5, "max", "6"),
        ] {
            state
                .db_rollup_configure(tasks, total, assignee, points, index)
                .unwrap_or_else(|refusal| panic!("{aggregate}: {refusal}"));
            assert_eq!(drawn(&state, tasks, 0, at), expected, "the fold of `{aggregate}`");
        }

        // Clearing the pick empties the four reading folds but leaves `count`
        // with its one honest answer over nothing.
        assert!(state.db_pick_relation(tasks, task, assignee, &[]));
        state
            .db_rollup_configure(tasks, total, assignee, points, 1)
            .unwrap();
        assert_eq!(drawn(&state, tasks, 0, at), "0", "how many related rows: truthfully none");
        state
            .db_rollup_configure(tasks, total, assignee, points, 2)
            .unwrap();
        assert_eq!(drawn(&state, tasks, 0, at), "", "a sum over nothing is empty, never 0");
    }

    /// ADR-0089's refusals, including the one that makes a *cross-database*
    /// dependency cycle unrepresentable: a rollup may not aggregate another
    /// computed column of the related records.
    #[test]
    fn a_rollup_refuses_a_column_of_the_wrong_database_and_one_that_computes() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("rollup-refusals");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");
        let points = state
            .db_add_column(people, "Points", PropertyKind::Number)
            .expect("a number column");
        // A computed column *of the related database* — the only shape that
        // could close a loop.
        let theirs = state
            .db_add_column(people, "Their rollup", PropertyKind::Rollup)
            .expect("a rollup column");
        let total = state
            .db_add_column(tasks, "Total", PropertyKind::Rollup)
            .expect("a rollup column");

        // Not a column of the related database.
        assert_eq!(
            state
                .db_rollup_configure(tasks, total, assignee, title_column(&state, tasks), 2)
                .unwrap_err(),
            "that column is not a column of the related database"
        );
        // The one that makes a cycle unrepresentable.
        assert_eq!(
            state.db_rollup_configure(tasks, total, assignee, theirs, 2).unwrap_err(),
            "a rollup cannot aggregate another computed column"
        );
        // A relation column: every fold of it would be a type error at paint
        // time, so it is refused while the user is looking.
        assert_eq!(
            state.db_rollup_configure(tasks, total, assignee, assigned, 2).unwrap_err(),
            "a rollup cannot aggregate a relation column"
        );
        // A relation with no target has nothing to aggregate.
        let lonely = state
            .db_add_column(tasks, "Lonely", PropertyKind::Relation)
            .expect("a relation column");
        assert_eq!(
            state.db_rollup_configure(tasks, total, lonely, points, 2).unwrap_err(),
            "that relation column has no target database yet"
        );
        // None of it wrote a config: the column is still unconfigured.
        assert_eq!(state.db_rollup_config(total), (-1, -1, 0));
    }

    /// A fold that cannot fold paints the same word a formula's failure paints,
    /// rather than refusing the configuration: "sum of a text column" is a
    /// mistake the user should *see*, and a painted `Error` is a better answer
    /// than a surprise glued string (ADR-0089).
    #[test]
    fn a_fold_that_cannot_fold_paints_the_error_word() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("rollup-error");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");
        let note = state
            .db_add_column(people, "Note", PropertyKind::Text)
            .expect("a text column");

        let ada = a_row(&state, people);
        name_a_row(&state, people, ada, "Ada");
        assert!(state.db_set_cell_text(people, ada, note, "four"));
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");
        assert!(state.db_pick_relation(tasks, task, assignee, &[ada.to_string()]));

        let total = state
            .db_add_column(tasks, "Total", PropertyKind::Rollup)
            .expect("a rollup column");
        state
            .db_rollup_configure(tasks, total, assignee, note, 2)
            .expect("the configuration is legal: a text column *can* be named");
        assert_eq!(
            drawn(&state, tasks, 0, column_of(&state, tasks, total)),
            "Error",
            "summing text is a mistake the user should see, not a glued string"
        );
        // …and `min` over the same column is fine: `extreme` folds text.
        state
            .db_rollup_configure(tasks, total, assignee, note, 4)
            .expect("min of one text value is that value");
        assert_eq!(
            drawn(&state, tasks, 0, column_of(&state, tasks, total)),
            "four"
        );
    }

    /// The cost contract (ADR-0083/0089): one eval per **(drawn row × visible
    /// configured rollup column)**, and never a read whose size is the target
    /// table's. The counter is the only instrument the app has for this, and it
    /// is the number `REPORT_TRACK3`'s D8 section quotes.
    #[test]
    fn a_rollup_costs_one_eval_per_drawn_row_and_not_the_target_tables_size() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("rollup-cost");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");
        let points = state
            .db_add_column(people, "Points", PropertyKind::Number)
            .expect("a number column");

        // Ten target rows, three source rows pointing at all ten.
        let mut targets = Vec::new();
        for index in 0..10 {
            let row = a_row(&state, people);
            name_a_row(&state, people, row, &format!("P{index}"));
            assert!(state.db_set_cell_text(people, row, points, "1"));
            targets.push(row.to_string());
        }
        for index in 0..3 {
            let row = a_row(&state, tasks);
            name_a_row(&state, tasks, row, &format!("T{index}"));
            assert!(state.db_pick_relation(tasks, row, assignee, &targets));
        }
        let total = state
            .db_add_column(tasks, "Total", PropertyKind::Rollup)
            .expect("a rollup column");
        state
            .db_rollup_configure(tasks, total, assignee, points, 2)
            .expect("the fold is legal");

        // A fresh read of the three-row window: three rows × one rollup column.
        state.db_windows.borrow_mut().remove(&tasks);
        state.db_computed_evals.set(0);
        assert!(state.db_refresh(tasks));
        assert_eq!(
            state.db_computed_evals.get(),
            3,
            "three drawn rows, one configured rollup column: the ten targets are read \
             as values, never as rows"
        );
        assert_eq!(
            drawn(&state, tasks, 0, column_of(&state, tasks, total)),
            "10",
            "and the fold still saw all ten of them"
        );

        // A window that does not move is not re-read at all, so it costs
        // nothing — the same cache contract every other layout's read has.
        state.db_computed_evals.set(0);
        assert!(!state.db_refresh(tasks), "an unchanged window is not re-read");
        assert_eq!(state.db_computed_evals.get(), 0);
    }

    // ─── M14 · D8: the numbers relation and rollup owe (SPEC §三十九) ───────
    //
    // `#[ignore]` printing probes, the convention D0/D1/D4/D8 established (see
    // `docs/REPORT_TRACK3.md` §D8): they assert nothing about *time* — a dev box
    // is not a benchmark — and print the samples and one JSON line per probe for
    // `benchmarks/results/`. What they **do** assert is the *shape* the number
    // is supposed to have (the window stayed one row, the counter stayed at one
    // eval per drawn row, the fold still saw every target), because a
    // measurement of the wrong thing is worse than no measurement.

    /// min / median / max of a sample, in the sample's own unit.
    fn three(mut samples: Vec<f64>) -> (f64, f64, f64) {
        samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
        let n = samples.len();
        (samples[0], samples[n / 2], samples[n - 1])
    }

    /// The probe fixture: two databases on one page, a two-way relation between
    /// them, a `Points` number column in the target, and `rows` target records
    /// seeded **straight into SQL** in 500-row batches.
    ///
    /// The schema goes through the app's own write paths (so the page really
    /// carries the two blocks and the catalog really knows the columns), the
    /// rows do not: `db_add_record` re-reads a window per row, which is O(n²)
    /// here and would be timing the seed rather than the pick. Settling the
    /// write queue first is what keeps the two writers from interleaving.
    struct RelationBench {
        state: std::rc::Rc<super::AppState>,
        tasks: i32,
        people: i32,
        assignee: i32,
        assigned: i32,
        points: i32,
        task: i64,
        targets: Vec<String>,
    }

    fn relation_bench(name: &str, rows: usize) -> RelationBench {
        use crate::core::database::{PropertyId, PropertyKind, Record, RecordId};
        use crate::core::database::CellValue;
        use crate::core::persistence::Repository;
        use crate::core::{Change, OrderKey};
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new(name);
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");
        let points = state
            .db_add_column(people, "Points", PropertyKind::Number)
            .expect("a number column");
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");

        state.persistence_force_flush();
        let repo = state.db_repo().cloned().expect("a store");
        let people_id = state.db_ref_of(people).expect("a database");
        let title = PropertyId(title_column(&state, people) as u64);
        let points_id = PropertyId(points as u64);
        // A base well past the session's own records: this seed is deliberately
        // outside the app's watermark (ADR-0072), so nothing it does can collide.
        let base = 1_000_000u64;
        let mut targets = Vec::with_capacity(rows);
        const BATCH: usize = 500;
        for batch in 0..rows.div_ceil(BATCH) {
            let from = batch * BATCH;
            let to = ((batch + 1) * BATCH).min(rows);
            let mut changes = Vec::with_capacity((to - from) * 3);
            for index in from..to {
                let record = RecordId(base + index as u64);
                changes.push(Change::RecordCreated(Record::bare(
                    record,
                    people_id,
                    OrderKey(((index as u64) + 1) << 32),
                )));
                changes.push(Change::CellSet {
                    record,
                    property: title,
                    value: CellValue::Text(format!("P{index:05}")),
                });
                changes.push(Change::CellSet {
                    record,
                    property: points_id,
                    value: CellValue::Number(index as f64),
                });
            }
            repo.apply(&changes).expect("the seed lands");
            targets.extend((from..to).map(|index| (base + index as u64).to_string()));
        }
        RelationBench {
            state,
            tasks,
            people,
            assignee,
            assigned,
            points,
            task,
            targets,
        }
    }

    /// SPEC §三十九's relation, D8 (ADR-0088's open item): what a pick costs
    /// when the target database is **10 000 rows**, what the picker's own query
    /// costs over the same table, and what the forbidden shape would cost.
    #[test]
    #[ignore = "prints a measurement; run with --release --lib -- --ignored --nocapture"]
    fn a_relation_pick_on_a_ten_thousand_row_target_is_bounded() {
        use crate::core::database::{PropertyId, Property, RowRequest};
        use std::time::Instant;

        const ROUNDS: usize = 20;
        let bench = relation_bench("perf-relation-pick", 10_000);
        let state = &bench.state;

        // ── the picker: 20 rows out of 10 000, capped by the caller ─────────
        let mut picker = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let started = Instant::now();
            let rows = state.db_relation_candidates(bench.assignee, "", 20);
            picker.push(started.elapsed().as_secs_f64() * 1e6);
            assert_eq!(rows.len(), 20, "the cap is the caller's, not the table's");
        }
        let searched = state.db_relation_candidates(bench.assignee, "p00042", 20);
        assert_eq!(searched.len(), 1, "the needle finds one row in 10 000");
        assert_eq!(searched[0].1, "P00042");

        // ── the pick: 200 targets, alternating between two disjoint halves so
        // every round really writes (re-picking the same list is a no-op by
        // construction, and timing a no-op would flatter the number) ────────
        let half = 200;
        let left: Vec<String> = bench.targets[..half].to_vec();
        let right: Vec<String> = bench.targets[half..half * 2].to_vec();
        let mut pick = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let set = if round % 2 == 0 { &left } else { &right };
            let started = Instant::now();
            assert!(state.db_pick_relation(bench.tasks, bench.task, bench.assignee, set));
            pick.push(started.elapsed().as_secs_f64() * 1e6);
        }

        // One last pick of the *first* half, so the assertion below looks at a
        // row that is actually inside the drawn window (the 20 timed rounds end
        // on the second half, whose rows are 10 000-row-table rows 200+).
        assert!(state.db_pick_relation(bench.tasks, bench.task, bench.assignee, &left));

        // The shape the number has to have: one source row on screen, the
        // mirror written on the *target* rows, and the target window still 31
        // rows even though the table is 10 000.
        assert_eq!(drawn_rows(state, bench.tasks), 1);
        assert_eq!(drawn_rows(state, bench.people), 31, "the window, not the table");
        let assigned_at = column_of(state, bench.people, bench.assigned);
        assert_eq!(drawn(state, bench.people, 0, assigned_at), "Ship it");
        let assignee_at = column_of(state, bench.tasks, bench.assignee);
        let painted = drawn(state, bench.tasks, 0, assignee_at);
        assert_eq!(
            painted.split(", ").count(),
            200,
            "every picked target is painted, and no more"
        );
        assert!(painted.starts_with("P00000, "), "{painted}");

        // ── the forbidden control: the whole target table, as objects ───────
        let repo = state.db_repo().cloned().expect("a store");
        let people_id = state.db_ref_of(bench.people).expect("a database");
        let properties: Vec<Property> = {
            let catalog = state.databases.borrow();
            catalog.properties_of(people_id).cloned().collect()
        };
        let request = RowRequest::new(
            people_id,
            PropertyId(title_column(state, bench.people) as u64),
            &properties,
        );
        let started = Instant::now();
        let all = repo.unwindowed_rows(&request).expect("the control read");
        let control_ms = started.elapsed().as_secs_f64() * 1e3;
        assert_eq!(all.len(), 10_000);

        let (picker_min, picker_mid, picker_max) = three(picker);
        let (pick_min, pick_mid, pick_max) = three(pick);
        println!("relation pick probe: 10 000-row target, 2 databases on one page");
        println!("  picker (cap 20): {picker_min:.1} / {picker_mid:.1} / {picker_max:.1} µs (min/median/max, {ROUNDS} warm)");
        println!("  pick 200 targets end to end: {pick_min:.1} / {pick_mid:.1} / {pick_max:.1} µs  (flush + plan + write + window re-read + peer pass)");
        println!("  control: the whole target table = {control_ms:.1} ms and {} objects", all.len());
        println!(
            "  one pick against the control (higher = the pick is cheaper): {:.0}x",
            control_ms * 1e3 / pick_mid.max(1.0)
        );
        println!(
            "{{\"label\":\"m14-relation-pick\",\"date\":\"2026-09-22\",\
             \"harness\":\"cargo test --release --lib -- --ignored --nocapture\",\
             \"target_rows\":10000,\"picked\":200,\"rounds\":{ROUNDS},\
             \"picker_min_us\":{picker_min:.1},\"picker_median_us\":{picker_mid:.1},\"picker_max_us\":{picker_max:.1},\
             \"pick_min_us\":{pick_min:.1},\"pick_median_us\":{pick_mid:.1},\"pick_max_us\":{pick_max:.1},\
             \"control_all_rows_ms\":{control_ms:.1},\"control_rows\":{}}}",
            all.len()
        );
    }

    /// SPEC §三十九's rollup, D8 (ADR-0089): what a **window's** fold costs when
    /// the folded column is 10 000 rows wide, against folding all of them.
    #[test]
    #[ignore = "prints a measurement; run with --release --lib -- --ignored --nocapture"]
    fn a_rollup_folds_a_window_and_never_the_related_table() {
        use crate::core::database::{PropertyId, PropertyKind, RecordId};
        use crate::core::database_formula;
        use crate::core::database_rollup::{self, Aggregate};
        use std::time::Instant;

        const ROUNDS: usize = 20;
        let bench = relation_bench("perf-rollup", 10_000);
        let state = &bench.state;

        // One source row related to **every** target: the fan-out is the whole
        // table, which is the worst case a window can have.
        let all = bench.targets.clone();
        assert!(state.db_pick_relation(bench.tasks, bench.task, bench.assignee, &all));
        let total = state
            .db_add_column(bench.tasks, "Total", PropertyKind::Rollup)
            .expect("a rollup column");
        state
            .db_rollup_configure(bench.tasks, total, bench.assignee, bench.points, 2)
            .expect("the fold is legal");

        // A fresh read of the one-row source window, round after round.
        let mut window = Vec::with_capacity(ROUNDS);
        let mut evals = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            state.db_windows.borrow_mut().remove(&bench.tasks);
            state.db_computed_evals.set(0);
            let started = Instant::now();
            assert!(state.db_refresh(bench.tasks));
            window.push(started.elapsed().as_secs_f64() * 1e6);
            evals.push(state.db_computed_evals.get() as f64);
        }
        let expected: u64 = (0..10_000u64).sum();
        let painted = drawn(state, bench.tasks, 0, column_of(state, bench.tasks, total));
        assert_eq!(painted, expected.to_string(), "the fold really saw all 10 000");
        assert!(
            evals.iter().all(|n| *n == 1.0),
            "one drawn row × one configured rollup column, however wide the fan-out"
        );

        // The control: the same fold over every related record — the batched
        // read the *whole* relation would need, which is the path the window
        // does not take.
        let repo = state.db_repo().cloned().expect("a store");
        let ids: Vec<RecordId> = bench
            .targets
            .iter()
            .filter_map(|t| t.parse::<u64>().ok())
            .map(RecordId)
            .collect();
        let started = Instant::now();
        let values = repo
            .values_of(&ids, PropertyId(bench.points as u64))
            .expect("one batched read of 10 000 ids");
        let read_all_us = started.elapsed().as_secs_f64() * 1e6;
        let started = Instant::now();
        let folded: Vec<database_formula::Val> = ids
            .iter()
            .filter_map(|id| {
                values
                    .get(&id.as_u64())
                    .map(|value| database_formula::val_of(PropertyKind::Number, value))
            })
            .collect();
        let sum = database_rollup::fold(Aggregate::Sum, &folded, folded.len())
            .expect("a sum of numbers");
        let fold_all_us = started.elapsed().as_secs_f64() * 1e6;
        assert_eq!(database_rollup::paint(&sum), expected.to_string());
        assert_eq!(values.len(), 10_000);

        let (win_min, win_mid, win_max) = three(window);
        println!("rollup probe: one window row, fan-out 10 000, fold `sum`");
        println!("  window re-read (count + 1 row + relation + values + fold): {win_min:.1} / {win_mid:.1} / {win_max:.1} µs (min/median/max, {ROUNDS} warm)");
        println!("  computed cells evaluated per read: {} (1 row × 1 rollup column)", evals[0]);
        println!("  control (every related record): values read {read_all_us:.1} µs + fold {fold_all_us:.1} µs = {:.1} µs", read_all_us + fold_all_us);
        let control_us = read_all_us + fold_all_us;
        println!(
            "  the window against the control: {win_mid:.0} µs vs {control_us:.0} µs = {:.2}x — and one eval instead of {} folds.              The window is *not* cheaper here: the fan-out is the whole table, and a window pays for the relation's live              titles as well as for the values. The bound the cache promises is the fan-out, not the table size.",
            win_mid / control_us,
            ids.len()
        );
        println!(
            "{{\"label\":\"m14-rollup-window\",\"date\":\"2026-09-22\",\
             \"harness\":\"cargo test --release --lib -- --ignored --nocapture\",\
             \"target_rows\":10000,\"fan_out\":10000,\"aggregate\":\"sum\",\"rounds\":{ROUNDS},\
             \"window_read_min_us\":{win_min:.1},\"window_read_median_us\":{win_mid:.1},\"window_read_max_us\":{win_max:.1},\
             \"evals_per_read\":{},\"control_values_us\":{read_all_us:.1},\"control_fold_us\":{fold_all_us:.1}}}",
            evals[0]
        );
    }

    /// ADR-0090's price, D8: what the content stamp moved. Three numbers — a
    /// cached refresh, a committed cell end to end, and the peer pass on a page
    /// of one database (which is every scene D8 measured) against a page of two.
    #[test]
    #[ignore = "prints a measurement; run with --release --lib -- --ignored --nocapture"]
    fn the_content_stamp_costs_one_reread_after_a_change_and_nothing_when_cached() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;
        use std::time::Instant;

        const ROUNDS: usize = 50;
        let dir = ScratchDir::new("perf-stamp");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let note = state
            .db_add_column(block, "Note", PropertyKind::Text)
            .expect("a text column");
        let row = a_row(&state, block);
        for index in 0..40 {
            let extra = a_row(&state, block);
            name_a_row(&state, block, extra, &format!("row {index}"));
        }

        // ── cached: nothing recorded since the last read ────────────────────
        // (The last write already consumed the stamp, so this first call is the
        // cached path too — which is the point being measured.)
        let mut cached = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let started = Instant::now();
            assert!(!state.db_refresh(block), "an unchanged window is not re-read");
            cached.push(started.elapsed().as_secs_f64() * 1e6);
        }

        // ── a committed cell, end to end: the flush, the write, the re-read ──
        let mut commit = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let started = Instant::now();
            assert!(state.db_set_cell_text(block, row, note, &format!("n{round}")));
            commit.push(started.elapsed().as_secs_f64() * 1e6);
        }

        // ── the peer pass: free on the scenes D8 measured, priced on a page
        // with a second database ────────────────────────────────────────────
        // A structural edit elsewhere on the page records a change and touches
        // no database window — exactly the state the peer pass exists for.
        let mut lone = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let _ = state.exec_on_open_page(crate::core::Command::AppendBlock {
                kind: crate::core::BlockKind::Paragraph,
                text: String::new(),
            });
            let started = Instant::now();
            state.db_refresh_page(None);
            lone.push(started.elapsed().as_secs_f64() * 1e6);
        }
        let second = a_database(&state);
        // Settle both windows so the loop below measures the peer pass alone.
        let _ = state.db_refresh(block);
        let _ = state.db_refresh(second);
        let mut pair = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let _ = state.exec_on_open_page(crate::core::Command::AppendBlock {
                kind: crate::core::BlockKind::Paragraph,
                text: String::new(),
            });
            let started = Instant::now();
            state.db_refresh_page(None);
            pair.push(started.elapsed().as_secs_f64() * 1e6);
        }

        let (cached_min, cached_mid, cached_max) = three(cached);
        let (commit_min, commit_mid, commit_max) = three(commit);
        let (lone_min, lone_mid, lone_max) = three(lone);
        let (pair_min, pair_mid, pair_max) = three(pair);
        println!("content stamp probe: 41-row database");
        println!("  cached refresh (a scroll that changes nothing): {cached_min:.1} / {cached_mid:.1} / {cached_max:.1} µs");
        println!("  one committed cell end to end (flush + plan + write + re-read + peers): {commit_min:.1} / {commit_mid:.1} / {commit_max:.1} µs");
        println!("  peer pass, one database on the page (the D8 scenes): {lone_min:.1} / {lone_mid:.1} / {lone_max:.1} µs");
        println!("  peer pass, two databases on the page, after a write: {pair_min:.1} / {pair_mid:.1} / {pair_max:.1} µs");
        println!(
            "{{\"label\":\"m14-content-stamp\",\"date\":\"2026-09-22\",\
             \"harness\":\"cargo test --release --lib -- --ignored --nocapture\",\
             \"rows\":41,\"rounds\":{ROUNDS},\
             \"cached_min_us\":{cached_min:.1},\"cached_median_us\":{cached_mid:.1},\"cached_max_us\":{cached_max:.1},\
             \"commit_min_us\":{commit_min:.1},\"commit_median_us\":{commit_mid:.1},\"commit_max_us\":{commit_max:.1},\
             \"peers_lone_median_us\":{lone_mid:.1},\"peers_pair_median_us\":{pair_mid:.1}}}"
        );
    }

    // ─── D10 (SPEC §三十九 「属性」): the three editors a user can reach ───────
    //
    // D5's honest boundary was that the state layer was finished and the window
    // was not: a relation could be paired by a test and not by a click. These
    // drive the entry points the type menu, the record picker and the rollup
    // configurator call, and read back what those popups draw — their rows, their
    // ticks, their refusal captions — plus the models a pick repaints.
    //
    // The failure mode these are written against is a chooser that agrees with
    // itself and not with the save path, so several assertions compare a row's
    // `disabled`/`note` with the very sentence `check_pair`/`check_config` gives
    // the same write.

    /// The type menu as `(id, disabled, note)` — the three fields that decide
    /// whether a row can be clicked and what it says about itself.
    fn menu_of(rows: &[super::DbPickRow]) -> Vec<(i32, bool, String)> {
        rows.iter()
            .map(|r| (r.id, r.disabled, r.note.clone()))
            .collect()
    }

    /// The type menu's order *is* the number every database callback carries, so
    /// a row's index is compared against `PropertyKind::index()` rather than
    /// against a remembered literal — the same `kind` int a delegate sends.
    #[test]
    fn the_type_menu_lists_every_kind_in_the_order_the_callbacks_mean() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-menu-order");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let note = state
            .db_add_column(block, "Note", PropertyKind::Text)
            .expect("a text column");

        let rows = state.db_column_kinds(note);
        assert_eq!(rows.len(), PropertyKind::ALL.len(), "one row per kind");
        for (index, kind) in PropertyKind::ALL.iter().enumerate() {
            assert_eq!(rows[index].id, kind.index(), "{} at its own index", kind.label());
            assert_eq!(rows[index].name, kind.label());
        }
        assert_eq!(
            rows.iter().filter(|r| r.chosen).count(),
            1,
            "exactly one row is the answer the column already has"
        );
        assert!(rows.iter().any(|r| r.chosen && r.id == PropertyKind::Text.index()));
        // `title` is never a choice, and says its own reason; a plain column is
        // refused for nothing else.
        let only = menu_of(&rows)
            .into_iter()
            .filter(|(_, disabled, _)| *disabled)
            .collect::<Vec<_>>();
        assert_eq!(
            only,
            vec![(
                PropertyKind::Title.index(),
                true,
                "A database has one title column.".to_string()
            )],
            "one greyed row, and it is the kind a database cannot have twice"
        );

        // The creation list is the same menu asked a different question: nothing
        // is chosen, because there is no column yet.
        let fresh = state.db_column_kinds(-1);
        assert_eq!(fresh.len(), rows.len());
        assert!(fresh.iter().all(|r| !r.chosen), "a new column has no kind to mark");
        assert_eq!(
            menu_of(&fresh),
            menu_of(&rows),
            "creation and conversion offer the identical list, refusal included"
        );
    }

    /// The refusal that is not about the kind at all: a column the schema leans
    /// on cannot move, and the menu says so on **every** row rather than hiding
    /// a list the user has to discover is closed by clicking one.
    #[test]
    fn a_column_the_schema_depends_on_is_refused_on_every_row() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-menu-refusal");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        let people_id = database_of(&state, people);

        // A title column: rows are named by it, so it is not a movable column.
        let title = state.db_column_kinds(title_column(&state, tasks));
        assert!(title.iter().all(|r| r.disabled), "the whole menu is closed");
        assert!(title
            .iter()
            .all(|r| r.note.contains("title column is what its rows are named by")));

        // A paired relation: ADR-0088's involution is a fact about two columns,
        // and moving one of them under the other breaks it by a side door.
        state
            .db_relation_configure(tasks, assignee, people_id, assigned)
            .expect("the pairing is legal");
        let paired = state.db_column_kinds(assignee);
        assert!(paired.iter().all(|r| r.disabled));
        assert!(paired
            .iter()
            .all(|r| r.note.contains("clear its back-pointer before changing its type")));
        assert_eq!(
            state.db_column_kind_set(tasks, assignee, PropertyKind::Text.index()).unwrap_err(),
            "This relation is two-way — clear its back-pointer before changing its type.",
            "the menu's sentence and the save's refusal are the same words"
        );

        // Drop the back-pointer and the column is an ordinary one-way relation:
        // the menu opens again, with only `title` left refused.
        state
            .db_relation_configure(tasks, assignee, people_id, -1)
            .expect("one-way is legal");
        let freed = menu_of(&state.db_column_kinds(assignee));
        assert_eq!(freed.iter().filter(|(_, disabled, _)| *disabled).count(), 1);
        assert!(state.db_column_kind_set(tasks, assignee, PropertyKind::Number.index()).is_ok());
    }

    /// Creation through the same menu: a column named for the kind it was made
    /// as, and a name that is taken counting up instead of the click being
    /// dropped.
    #[test]
    fn a_new_column_from_the_menu_is_named_for_its_kind_and_a_collision_counts_up() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-column-add");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);

        let first = state.db_column_add(block, PropertyKind::Text.index()).expect("a column");
        let second = state.db_column_add(block, PropertyKind::Text.index()).expect("a second");
        assert_eq!(state.db_property_label(first), "Text");
        assert_eq!(state.db_property_label(second), "Text 2", "the tail counts up");
        let number = state.db_column_add(block, PropertyKind::Number.index()).expect("a number");
        assert_eq!(state.db_property_label(number), "Number");

        // A second title is not a thing a schema has a meaning for, and the menu
        // greyed that row for exactly this reason.
        assert!(state.db_column_add(block, PropertyKind::Title.index()).is_none());
        // The created column really is of the kind that was clicked: its own menu
        // row is the marked one.
        assert!(state
            .db_column_kinds(number)
            .iter()
            .any(|r| r.chosen && r.id == PropertyKind::Number.index()));
    }

    /// ADR-0062's rule, now reachable by a click: the type menu is the only
    /// producer of `PropertyKindSet`, and what it changes is what a cell
    /// **means** — never what it holds.
    #[test]
    fn moving_a_type_changes_what_a_cell_means_and_not_what_it_holds() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-kind-move");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let note = state
            .db_add_column(block, "Note", PropertyKind::Text)
            .expect("a text column");
        let row = a_row(&state, block);
        assert!(state.db_set_cell_text(block, row, note, "hello"));
        let at = column_of(&state, block, note);
        assert_eq!(drawn(&state, block, 0, at), "hello");
        let _ = state.take_db_notice();

        state
            .db_column_kind_set(block, note, PropertyKind::Number.index())
            .expect("a plain column may move");
        let notice = state.take_db_notice().expect("the move says what it did");
        assert!(
            notice.contains("\"Text\" is now \"Number\"") && notice.contains("did not change"),
            "{notice}"
        );
        // The bytes are untouched and the *read* is shaped by the kind: a number
        // column looks in the store's `num` slot and `hello` is in the text one,
        // so the cell shows nothing. That is what "the values did not change"
        // costs — the command's own doc says the undo puts the words back, and
        // only untouched bytes make that true.
        assert_eq!(state.db_cell_text(block, row, note).as_deref(), Some(""));
        assert_eq!(
            drawn(&state, block, 0, at),
            "",
            "the cell is empty through the new kind, not rewritten by it"
        );

        // One step back: the kind returns, and with it the meaning of the value.
        state.undo_open_page().expect("the move is one undo step");
        assert_eq!(state.db_property_kind(note), Some(PropertyKind::Text));
        assert_eq!(
            state.db_cell_text(block, row, note).as_deref(),
            Some("hello"),
            "undo has something to put back precisely because nothing was destroyed"
        );
        assert_eq!(drawn(&state, block, 0, at), "hello");
    }

    /// A move to a kind that computes has nothing to show until the column is
    /// defined, and the notice says that instead of the "values did not change"
    /// sentence — the one case where the two are not the same statement.
    #[test]
    fn a_move_into_a_computed_kind_says_the_cells_are_not_written_yet() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-kind-computed");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let note = state
            .db_add_column(block, "Note", PropertyKind::Text)
            .expect("a text column");
        let row = a_row(&state, block);
        assert!(state.db_set_cell_text(block, row, note, "hello"));
        let _ = state.take_db_notice();

        state
            .db_column_kind_set(block, note, PropertyKind::Formula.index())
            .expect("a text column may become a formula");
        let notice = state.take_db_notice().expect("the move says what it did");
        assert!(notice.contains("until it is defined"), "{notice}");
        assert!(!notice.contains("did not change"), "{notice}");

        // The same click on the kind a column already has writes nothing at all,
        // which is what keeps a menu that re-draws from costing an undo step.
        // (`take_db_notice` joins the whole queue without draining it, so "said
        // nothing" here is "said no *new* thing": the line is byte-identical.)
        let before = state.take_db_notice();
        let again = state.db_column_kind_set(block, note, PropertyKind::Formula.index());
        assert!(again.is_ok(), "not a refusal — just nothing to do");
        assert_eq!(
            state.take_db_notice(),
            before,
            "and it says nothing new, either"
        );
    }

    /// The relation editor's first question. Self-relations stay on the list —
    /// ADR-0088 refuses a column being *its own* back-pointer, not a database
    /// naming itself — and the note is what tells the two apart on screen.
    #[test]
    fn the_target_chooser_marks_this_database_and_leaves_a_self_relation_open() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-relation-targets");
        let (state, tasks, people, _assignee, _assigned) = two_databases(&dir);
        let tasks_id = database_of(&state, tasks);
        let people_id = database_of(&state, people);

        let rows = state.db_relation_databases(tasks, -1);
        assert!(rows.iter().all(|r| !r.disabled), "any database may be pointed at");
        assert_eq!(
            rows.iter().find(|r| r.id == tasks_id).expect("its own database is listed").note,
            "this database",
            "and the row says so, because a self-relation is an ordinary answer"
        );
        assert!(rows.iter().all(|r| !r.chosen), "the column has no target yet");

        let rows = state.db_relation_databases(tasks, people_id);
        assert!(rows.iter().any(|r| r.chosen && r.id == people_id));
        assert_eq!(
            state.db_database_name(-1),
            "",
            "the absence is an empty name, not a word for one"
        );
    }

    /// The relation editor's second question: only a target's *relation* columns
    /// can hold back-pointers, so the other kinds are absent, and among the
    /// relations a refused one is drawn greyed with the save path's own sentence.
    #[test]
    fn the_back_pointer_chooser_offers_no_then_the_relations_that_can_pair() {
        use crate::core::database::PropertyKind;
        use crate::core::database_relation;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-relation-mirrors");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        let people_id = database_of(&state, people);
        let tasks_id = database_of(&state, tasks);
        let note = state
            .db_add_column(people, "Note", PropertyKind::Text)
            .expect("a text column in the target");

        let rows = state.db_relation_mirrors(tasks, assignee, people_id);
        assert_eq!(rows[0].id, -1, "one-way is offered first");
        assert_eq!(rows[0].name, "No back-pointer");
        assert!(rows[0].chosen, "and it is the answer an unpaired column has");
        assert!(rows[0].note.contains("one-way"));
        assert!(
            rows.iter().all(|r| r.id != note),
            "a text column has nowhere to store a back-pointer, so it is not a candidate"
        );
        let candidate = rows.iter().find(|r| r.id == assigned).expect("the relation is listed");
        assert!(!candidate.disabled, "a free relation may be pointed back");

        // Pair it, and the same list now ticks the answer instead.
        state
            .db_relation_configure(tasks, assignee, people_id, assigned)
            .expect("the pairing is legal");
        let rows = state.db_relation_mirrors(tasks, assignee, people_id);
        assert!(rows.iter().any(|r| r.id == assigned && r.chosen));
        assert!(!rows[0].chosen, "and one-way is no longer the answer");

        // A relation that belongs to somebody *else* is drawn, greyed, refused:
        // `assigned` re-points at a second relation of this database, so pairing
        // it here would break the involution for that one (rule 4 of `check_pair`,
        // reached because rules 1–3 all pass).
        let reviewer = state
            .db_add_column(tasks, "Reviewer", PropertyKind::Relation)
            .expect("a second relation of this database");
        state
            .db_relation_configure(tasks, assignee, people_id, -1)
            .expect("un-pairing is legal, and it clears the other half too");
        assert_eq!(state.db_relation_config(assigned).1, -1, "one step un-does the pair");
        state
            .db_relation_configure(people, assigned, tasks_id, reviewer)
            .expect("the target's relation may point back at a different column here");
        let rows = state.db_relation_mirrors(tasks, assignee, people_id);
        let busy = rows
            .iter()
            .find(|r| r.id == assigned)
            .expect("listed, not hidden — hiding it is a missing column the user can see");
        assert!(busy.disabled);
        assert_eq!(
            busy.note,
            database_relation::PairRefusal::AlreadyPaired.message(),
            "the menu's grey-out and the save's refusal are one sentence"
        );
    }

    /// The picker itself: the target database's rows by title, this cell's own
    /// targets ticked, and one click adding or taking away through the same path
    /// the two-way write goes through.
    #[test]
    fn the_record_picker_ticks_what_the_cell_holds_and_one_click_moves_it() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-relation-picker");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");
        for name in ["Ada", "Bob", "Cy"] {
            let row = a_row(&state, people);
            name_a_row(&state, people, row, name);
        }
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");

        let at = column_of(&state, tasks, assignee);
        let mirror_at = column_of(&state, people, assigned);
        assert_eq!(drawn(&state, tasks, 0, at), "", "an empty cell has nothing ticked");

        let rows = state.db_relation_rows(assignee, task, "", 10);
        assert_eq!(rows.len(), 3, "the target database's rows");
        assert_eq!(rows[0].name, "Ada");
        assert!(rows.iter().all(|r| !r.chosen));

        assert!(state.db_relation_toggle(tasks, task, assignee, rows[0].id));
        assert_eq!(drawn(&state, tasks, 0, at), "Ada", "the click is a real write");
        assert_eq!(
            drawn(&state, people, 0, mirror_at),
            "Ship it",
            "and the back-pointer half moved with it"
        );
        let rows = state.db_relation_rows(assignee, task, "", 10);
        assert!(rows.iter().any(|r| r.chosen && r.name == "Ada"));
        assert!(rows.iter().all(|r| !r.chosen || r.name == "Ada"));

        // A second target, and the needle narrows the list without losing the
        // ticks — the popup re-pushes through the same read every time.
        assert!(state.db_relation_toggle(tasks, task, assignee, rows[1].id));
        assert_eq!(drawn(&state, tasks, 0, at), "Ada, Bob");
        let searched = state.db_relation_rows(assignee, task, "ad", 10);
        assert_eq!(searched.len(), 1, "the needle is a filter");
        assert_eq!(searched[0].name, "Ada");
        assert!(searched[0].chosen, "and the tick survives the narrowing");

        // The same gesture on a ticked row takes it away.
        assert!(state.db_relation_toggle(tasks, task, assignee, searched[0].id));
        assert_eq!(drawn(&state, tasks, 0, at), "Bob");
        assert_eq!(state.db_relation_held(assignee, task), vec![rows[1].id]);
    }

    /// A relation with no target database has nothing to list: the picker says so
    /// with an empty list rather than pointing at rows nothing can name.
    #[test]
    fn an_unconfigured_relation_lists_nothing_rather_than_the_wrong_rows() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-relation-empty");
        let (state, tasks, people, assignee, _assigned) = two_databases(&dir);
        let ada = a_row(&state, people);
        name_a_row(&state, people, ada, "Ada");
        let task = a_row(&state, tasks);

        assert!(state.db_relation_rows(assignee, task, "", 10).is_empty());
        assert!(state.db_relation_held(assignee, task).is_empty());
        assert!(
            !state.db_relation_toggle(tasks, task, assignee, ada as i32),
            "nothing to point at, so the click writes nothing"
        );
    }

    /// The rollup configurator's first two lists, each built *with* the gate that
    /// writes: an unconfigured relation is greyed with `NoTarget`'s own words, a
    /// column `check_config` refuses is drawn and greyed, and a live row carries
    /// its kind as its caption.
    #[test]
    fn the_rollup_choosers_are_gated_by_the_same_check_that_gates_the_write() {
        use crate::core::database::PropertyKind;
        use crate::core::database_rollup;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-rollup-choosers");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        let people_id = database_of(&state, people);
        let points = state
            .db_add_column(people, "Points", PropertyKind::Number)
            .expect("a number column in the target");
        let theirs = state
            .db_add_column(people, "Their rollup", PropertyKind::Rollup)
            .expect("a computed column in the target");

        // Panel 1: this database's relation columns — and one with no target has
        // nothing to fold through, so it is greyed with the save's refusal.
        let relations = state.db_rollup_relations(tasks, -1);
        assert_eq!(relations.len(), 1, "assignee is the only relation task rows have");
        assert_eq!(relations[0].id, assignee);
        assert!(relations[0].disabled);
        assert_eq!(
            relations[0].note,
            database_rollup::ConfigRefusal::NoTarget.message()
        );
        state
            .db_relation_configure(tasks, assignee, people_id, assigned)
            .expect("the pairing is legal");
        let relations = state.db_rollup_relations(tasks, assignee);
        assert!(!relations[0].disabled, "now it has a database to fold through");
        assert!(relations[0].chosen);

        // Panel 2: the target database's columns, each judged by `check_config`.
        let columns = state.db_rollup_columns(assignee, -1);
        assert_eq!(columns[0].id, -1, "`count` reads no column, so that is a choice");
        assert_eq!(columns[0].name, "No column");
        assert_eq!(columns[0].note, "count needs no column");
        let row_of = |property: i32| {
            columns
                .iter()
                .find(|r| r.id == property)
                .unwrap_or_else(|| panic!("{property} is listed"))
                .clone()
        };
        let number = row_of(points);
        assert!(!number.disabled);
        assert_eq!(number.note, "Number", "a live row says what kind it is");
        let relation = row_of(assigned);
        assert!(relation.disabled);
        assert_eq!(
            relation.note,
            database_rollup::ConfigRefusal::TargetIsRelation.message()
        );
        let computed = row_of(theirs);
        assert!(computed.disabled);
        assert_eq!(
            computed.note,
            database_rollup::ConfigRefusal::TargetIsComputed.message(),
            "the one shape that could close a cycle is refused while the user is looking"
        );

        // Panel 3: the six folds, in the order the editor offers them.
        let aggregates = state.db_rollup_aggregates(2);
        assert_eq!(aggregates.len(), 6);
        assert_eq!(aggregates[0].name, "None", "and `none` first, the state a new rollup is in");
        assert_eq!(aggregates[2].name, "Sum");
        assert!(aggregates.iter().all(|r| !r.disabled));
        assert!(aggregates.iter().filter(|r| r.chosen).count() == 1);
    }

    /// The three parts a rollup is, as the editor's header shows them: a column
    /// that reads as `Assignee · Points · Sum` is the same config
    /// `db_rollup_configure` wrote, and an unset slot reads "Not set" rather than
    /// an empty string the popup would have to invent a word for.
    #[test]
    fn a_rollup_editor_reads_its_three_parts_back_as_words() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("d10-rollup-labels");
        let (state, tasks, people, assignee, assigned) = two_databases(&dir);
        let points = state
            .db_add_column(people, "Points", PropertyKind::Number)
            .expect("a number column");
        let total = state
            .db_add_column(tasks, "Total", PropertyKind::Rollup)
            .expect("a rollup column");

        assert_eq!(
            state.db_rollup_labels(total),
            ("Not set".into(), "Not set".into(), "None".into()),
            "a column nobody has defined yet"
        );
        state
            .db_relation_configure(tasks, assignee, database_of(&state, people), assigned)
            .expect("the pairing is legal");
        state
            .db_rollup_configure(tasks, total, assignee, points, 2)
            .expect("a sum over the related points");
        assert_eq!(
            state.db_rollup_labels(total),
            ("Assignee".into(), "Points".into(), "Sum".into())
        );
        assert_eq!(state.db_rollup_config(total), (assignee, points, 2));

        // And the fold is real: the same three picks the editor makes are what
        // the projection paints.
        let task = a_row(&state, tasks);
        name_a_row(&state, tasks, task, "Ship it");
        for index in 0..2 {
            let row = a_row(&state, people);
            name_a_row(&state, people, row, &format!("P{index}"));
            assert!(state.db_set_cell_text(people, row, points, &(index as f64 * 10.0).to_string()));
            assert!(state.db_relation_toggle(tasks, task, assignee, row as i32));
        }
        assert_eq!(
            drawn(&state, tasks, 0, column_of(&state, tasks, total)),
            "10",
            "0 + 10 folded with `sum`"
        );
    }

    /// ADR-0087's view search, at the one layer that can see both the needle
    /// and the model the delegate draws: the needle has to *narrow* the window,
    /// and `db_view_search_set` has to say it did. A `false` here is the
    /// `database-search` scene painting the unsearched table — which is what it
    /// did, because a needle that matches every row leaves a window identical
    /// to the cached one, and `unchanged` is the refresh's own answer.
    #[test]
    fn a_view_needle_narrows_the_window_the_delegate_draws() {
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("view-search");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        for name in ["the first", "the second", "alpha"] {
            let row = a_row(&state, block);
            name_a_row(&state, block, row, name);
        }
        assert_eq!(drawn_rows(&state, block), 3, "three rows to start");

        assert!(
            state.db_view_search_set(block, "the"),
            "a needle that drops a row is a change"
        );
        assert_eq!(
            drawn_rows(&state, block),
            2,
            "the window holds the matching rows only"
        );
        assert_eq!(state.db_search_needle(block), "the");

        // …and clearing it puts all three back, through the same door.
        assert!(state.db_view_search_set(block, ""), "emptying the box changes");
        assert_eq!(drawn_rows(&state, block), 3, "no needle, no constraint");
    }

    /// The block row the delegate draws for `block`, freshly re-projected.
    fn block_row(state: &super::AppState, block: i32) -> crate::BlockRow {
        use slint::Model as _;
        state.reproject_blocks();
        (0..state.blocks.row_count())
            .filter_map(|at| state.blocks.row_data(at))
            .find(|row| row.id == block)
            .expect("the block is on the open page")
    }

    /// `db_columns` as the delegate reads it: `(property, permille)`.
    fn drawn_columns(row: &crate::BlockRow) -> Vec<(i32, i32)> {
        use slint::Model as _;
        (0..row.db_columns.row_count())
            .filter_map(|at| row.db_columns.row_data(at))
            .map(|column| (column.property, column.permille))
            .collect()
    }

    /// The layout the switcher's label says, and the number the delegate draws
    /// against, from the same projection. D3's seven layouts painted nothing
    /// for six slices because the predicates were written against the store's
    /// lowercase keys while `db-layout` carries the capitalized label — so the
    /// pair is pinned together, and the number is what a comparison must use.
    #[test]
    fn every_layout_arrives_as_its_own_index_alongside_its_label() {
        use crate::core::database::ViewLayout;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("layout-index");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        assert_eq!(
            block_row(&state, block).db_layout_index,
            ViewLayout::Table.index(),
            "a new database is a table"
        );
        for (at, layout) in ViewLayout::ALL.iter().enumerate() {
            assert!(
                state.db_add_view(block, at as i32),
                "{} creates",
                layout.label()
            );
            let row = block_row(&state, block);
            assert_eq!(row.db_layout_index, at as i32, "the switch is the number");
            assert_eq!(row.db_layout, layout.label(), "…and the word is still the word");
            assert!(row.db_layout_ok, "{} is drawn by this build", layout.label());
        }
    }

    /// An auto width is a **share**, worked out where the projection happens.
    /// The delegate cannot fold a list, and Slint gives an explicit `width`
    /// binding the last word over `horizontal-stretch`, so the `0` this used to
    /// hand over was a column of no width at all — the second reason a table
    /// body was blank for six slices.
    #[test]
    fn an_auto_column_width_is_a_share_of_the_grid_and_never_zero() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("auto-width");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let points = state
            .db_add_column(block, "Points", PropertyKind::Number)
            .expect("a number column");

        // Untouched, every column is auto, so the grid divides equally.
        let all = drawn_columns(&block_row(&state, block));
        assert!(all.len() > 1, "a new database has more than its title");
        let share = 1000 / all.len() as i32;
        assert!(
            all.iter().all(|(_, permille)| *permille == share),
            "an equal split of the grid, not a zero: {all:?}"
        );

        // One dragged column takes its own width and the rest divide what the
        // grid has left — and the dragged one keeps exactly what was set.
        assert!(state.db_set_column_width(block, points, 300));
        let after = drawn_columns(&block_row(&state, block));
        let left = 1000 - 300;
        let rest = left / (after.len() as i32 - 1);
        for (property, permille) in &after {
            let wanted = if *property == points { 300 } else { rest };
            assert_eq!(*permille, wanted, "column {property} of {after:?}");
        }
    }

    /// The round trip D4 promised and did not make: "add a rule" writes a
    /// clause with no value, the document stores that as `"value": null`, and
    /// the parser read a `null` as a value of the wrong shape — so the rule
    /// vanished on the way back out, the panel drew itself empty, and the next
    /// edit (`set_op`, `set_text`) had no clause at index 0 to edit.
    #[test]
    fn a_half_written_filter_rule_survives_the_document() {
        use crate::core::database::PropertyKind;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("filter-half-written");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let points = state
            .db_add_column(block, "Points", PropertyKind::Number)
            .expect("a number column");
        for (name, value) in [("big", "40"), ("small", "2")] {
            let row = a_row(&state, block);
            name_a_row(&state, block, row, name);
            assert!(state.db_set_cell_text(block, row, points, value));
        }

        assert!(state.db_filter_add_clause(block, points), "the rule is added");
        let (any, rows) = state.db_filter_panel(block);
        assert!(!any, "a new root is match-all");
        assert!(state.db_filter_editable(block), "and the panel may edit it");
        assert_eq!(rows.len(), 1, "…and it is still here to edit");
        assert!(!rows[0].has_value, "unfilled, exactly as written");
        assert_eq!(
            drawn_rows(&state, block),
            2,
            "a half-written rule hides nothing"
        );

        // The two edits that were unreachable while the clause was dropped.
        assert!(state.db_filter_set_op(block, 0, 3), "gt is a number's own");
        assert!(state.db_filter_set_text(block, 0, "5"));
        let (_, rows) = state.db_filter_panel(block);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].op_name, ">", "the number's own word for gt");
        assert!(rows[0].has_value, "the rule is filled in now");
        assert_eq!(
            drawn_rows(&state, block),
            1,
            "and the window answers to it"
        );
    }

    /// The other half of the same door: a comparison the column's kind cannot
    /// make is **refused** rather than written. The panel's menu is built from
    /// the same list, so this only fires on a row that went stale under an open
    /// menu — and what it prevents is a clause the parser would drop on
    /// read-back, which is a rule silently eaten.
    #[test]
    fn the_filter_refuses_a_comparison_its_column_cannot_make() {
        use crate::core::database::PropertyKind;
        use crate::core::database_view::FilterOp;
        use crate::testing::ScratchDir;

        let dir = ScratchDir::new("filter-op-refused");
        let state = super::AppState::new(&plain_args(), Some(scratch_repo(&dir)));
        state.create_page(None);
        let block = a_database(&state);
        let points = state
            .db_add_column(block, "Points", PropertyKind::Number)
            .expect("a number column");
        let row = a_row(&state, block);
        name_a_row(&state, block, row, "only");
        assert!(state.db_filter_add_clause(block, points));

        // `contains` is not a number's comparison — the control the refusal
        // rests on, asserted before the refusal itself so a future list that
        // admits it fails here rather than passing a vacuous test.
        assert!(!FilterOp::ops_for(PropertyKind::Number).contains(&FilterOp::Contains));
        assert!(
            !state.db_filter_set_op(block, 0, FilterOp::Contains.index()),
            "a number is not a substring"
        );
        let (_, rows) = state.db_filter_panel(block);
        assert_eq!(rows.len(), 1, "and the rule it was refused is still there");
        assert_eq!(rows[0].op_name, "is", "with the comparison it started with");

        // Out of the list entirely, and out of the clause list: both no-ops.
        let past_the_end = crate::core::database_view::FILTER_OPS.len() + 1;
        assert!(!state.db_filter_set_op(block, 0, past_the_end));
        assert!(!state.db_filter_set_op(block, 7, FilterOp::Gt.index()));
    }
}
