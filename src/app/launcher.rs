// The start-up sequence, shared by every entry point (M9.pre).
//
// A desktop `main` and an Android `android_main` differ in three facts — which
// thread owns the event loop, where the library is allowed to live, and whether
// the screen is touched — and in nothing else. Those three are the arguments
// here; everything under them (args, logging, the database, the window, the
// bench harness switches) is one path both platforms run, which is the whole
// reason the model, the store and the services were split into their own crate
// first (ADR-0094).

use crate::app::controller;
use crate::app::state::{AppState, HandleArgs};
use crate::services::logging;
use crate::AppWindow;
use slint::{ComponentHandle, Timer};

pub struct LaunchArgs {
    pub blocks: usize,
    pub auto_exit_secs: f64,
    /// Scene G: extra flat pages to switch between (--page-switch N).
    pub bench_pages: usize,
    /// Scene D/F with media: image rows on the bench page (--pictures N).
    pub pictures: usize,
    /// Scene D with inline marks: rows of the bench page carrying a bold mark
    /// (--marks N). Without it the bench page has no marks at all, and the
    /// runs channel a marked line is drawn from stays invisible to the gate.
    pub marks: usize,
    /// Scene D with highlighted code: code rows on the bench page
    /// (--code N), each carrying a language so its layers are drawn.
    pub code: usize,
    /// Scene F: programmatic continuous scroll (--scroll).
    pub scroll: bool,
    /// Scene F: how far one frame advances (--scroll-step, default 8 px). A
    /// page of tall image rows needs a wheel-flick-sized step to cross a
    /// picture at all, so the harness can ask.
    pub scroll_step: f32,
    /// Headless state setup for screenshots (--scene <name>).
    pub scene: Option<String>,
    /// Database file (--db <path>; default appdata/quire.db).
    pub db: Option<std::path::PathBuf>,
    /// Keep the library beside the working directory (--portable) instead of
    /// in the per-user profile (--db still wins, see storage::data_location).
    pub portable: bool,
    /// Markdown file to import and open at startup (--open <path>; also the
    /// bare positional, which is what the .md file association passes).
    pub open: Option<std::path::PathBuf>,
    /// Serve the workspace read-only on the LAN (--share [port], default
    /// 5877).
    pub share: Option<u16>,
    /// Pull a framed workspace from another Quire and import it
    /// (--pull http://host:5877).
    pub pull: Option<String>,
    /// Debug: print loaded page/block counts to stderr (--dump-state).
    pub dump_state: bool,
    /// A2: measure the first painted frame and print one JSON line to stderr
    /// (--measure-startup). Normal runs never set this: no timer, no thread,
    /// no extra frame.
    pub measure_startup: bool,
    /// Draw the window the way a phone does (--touch): no window controls, no
    /// layout rail, a drawer instead of a sidebar, and the thumb bar along the
    /// bottom. Desktop-only — Android sets this itself and reads no arguments.
    pub touch: bool,
}

fn parse_launch_args() -> LaunchArgs {
    let argv: Vec<String> = std::env::args().collect();
    let mut a = LaunchArgs {
        blocks: 0,
        auto_exit_secs: 0.0,
        bench_pages: 0,
        pictures: 0,
        marks: 0,
        code: 0,
        scroll: false,
        scroll_step: 8.0,
        scene: None,
        db: None,
        portable: false,
        dump_state: false,
        open: None,
        share: None,
        pull: None,
        measure_startup: false,
        touch: false,
    };
    let mut i = 1;
    while i < argv.len() {
        match (argv[i].as_str(), argv.get(i + 1)) {
            ("--blocks", Some(v)) => {
                a.blocks = v.parse().unwrap_or(0);
                i += 1;
            }
            ("--auto-exit", Some(v)) => {
                a.auto_exit_secs = v.parse().unwrap_or(0.0);
                i += 1;
            }
            ("--page-switch", Some(v)) => {
                a.bench_pages = v.parse().unwrap_or(0);
                i += 1;
            }
            ("--pictures", Some(v)) => {
                a.pictures = v.parse().unwrap_or(0);
                i += 1;
            }
            ("--marks", Some(v)) => {
                a.marks = v.parse().unwrap_or(0);
                i += 1;
            }
            ("--code", Some(v)) => {
                a.code = v.parse().unwrap_or(0);
                i += 1;
            }
            ("--scroll", _) => {
                a.scroll = true;
            }
            ("--scroll-step", Some(v)) => {
                a.scroll_step = v.parse().unwrap_or(8.0f32).max(1.0);
                i += 1;
            }
            ("--scene", Some(v)) => {
                a.scene = Some(v.clone());
                i += 1;
            }
            ("--db", Some(v)) => {
                a.db = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            ("--portable", _) => {
                a.portable = true;
            }
            ("--dump-state", _) => {
                a.dump_state = true;
            }
            ("--open", Some(v)) => {
                a.open = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            ("--share", _) => {
                a.share = Some(crate::services::lan_server::DEFAULT_PORT);
            }
            ("--share-port", Some(v)) => {
                if let Ok(p) = v.parse() {
                    a.share = Some(p);
                }
                i += 1;
            }
            ("--pull", Some(v)) => {
                a.pull = Some(v.clone());
                i += 1;
            }
            ("--measure-startup", _) => {
                a.measure_startup = true;
            }
            ("--touch", _) => {
                a.touch = true;
            }
            (positional, _) if !positional.starts_with('-') => {
                if a.open.is_none() {
                    a.open = Some(std::path::PathBuf::from(positional));
                }
            }
            _ => {}
        }
        i += 1;
    }
    a
}

/// Keep a timer alive for the whole run (dropping a Timer cancels it).
fn leak_timer(t: Timer) {
    std::mem::forget(t);
}

/// The desktop entry: `src/main.rs` calls this and nothing else.
pub fn desktop_main() -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    // The UI event loop runs on its own thread with a generous stack:
    // Slint 1.18 evaluates the initial property/layout bindings of the
    // component tree recursively on the C stack, and Quire's shell sits
    // just above the 1 MB Windows default in debug builds (popups add
    // more when they open). 8 MB is the standard Linux default and costs
    // nothing but address-space reservation. See DECISIONS.md ADR-0009.
    let child = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
        .spawn(move || run(start, false)).expect("spawn UI thread");
    child.join().map_err(|_| "UI thread panicked".to_string())??;
    Ok(())
}

/// A2 follow-up · phase stamps (--measure-startup only).
///
/// `first_paint_ms` says how long the start took, not where. This records the
/// wall time of each step of `real_main` so the window-up → first-paint gap
/// gets decomposed instead of guessed at. One `Instant::now()` per mark, and
/// normal runs pass `None` and never build it.
#[derive(Clone)]
struct PhaseLog {
    start: std::time::Instant,
    last: std::rc::Rc<std::cell::RefCell<std::time::Instant>>,
    rows: std::rc::Rc<std::cell::RefCell<Vec<(&'static str, f64, f64)>>>,
}

impl PhaseLog {
    fn new(start: std::time::Instant) -> Self {
        Self {
            start,
            last: std::rc::Rc::new(std::cell::RefCell::new(start)),
            rows: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }

    fn mark(&self, name: &'static str) {
        let now = std::time::Instant::now();
        let delta = {
            let mut last = self.last.borrow_mut();
            let d = now.duration_since(*last).as_secs_f64() * 1000.0;
            *last = now;
            d
        };
        let at = now.duration_since(self.start).as_secs_f64() * 1000.0;
        self.rows.borrow_mut().push((name, delta, at));
    }

    fn json(&self) -> String {
        let rows = self.rows.borrow();
        let parts: Vec<String> = rows
            .iter()
            .map(|(n, d, t)| format!("{{\"phase\":\"{n}\",\"ms\":{d:.1},\"at_ms\":{t:.1}}}"))
            .collect();
        format!("[{}]", parts.join(","))
    }
}

/// A2 · first-paint measurement (--measure-startup).
///
/// Slint 1.18 exposes `Window::set_rendering_notifier`; femtovg and skia
/// support it and fire `AfterRendering` once per drawn frame, so the first
/// firing is the first painted frame (its exact boundary: after the scene is
/// rendered and the GPU commands submitted, immediately before presentation —
/// see `i-slint-renderer-femtovg` draw(); a sub-millisecond underestimate of
/// true on-screen time). Measured caveat, 2026-09-20: skia only notifies on a
/// surface whose `with_graphics_api` is real, so `--features skia-opengl` does
/// fire and the default `--features skia` build (wgpu/softbuffer candidates)
/// stays silent while still returning `Ok(())` — never read a silent skia run
/// as "no frame was drawn". The software renderer has no notifier; there the
/// proxy is the first timer to run inside the event loop, which lands before
/// any frame is drawn (so it under-reports paint, over-reports readiness —
/// flagged `"confidence":"low"` and documented in PERFORMANCE.md).
///
/// The window is still hidden at install time, so no frame can be missed.
/// Normal runs never call this — no timer, no thread, no extra frame.
fn install_startup_measurement(
    ui: &AppWindow,
    start: std::time::Instant,
    phases: Option<PhaseLog>,
) {
    // The stamps after this point are still written by `real_main`, which keeps
    // running until `ui.run()` — PhaseLog is Rc inside, so the read here sees
    // every mark made before the first frame.
    let paint_json = |phases: &Option<PhaseLog>| {
        phases
            .as_ref()
            .map(|p| format!(",\"phases\":{}", p.json()))
            .unwrap_or_default()
    };
    let reported = std::rc::Rc::new(std::cell::Cell::new(false));
    let reported2 = reported.clone();
    let phases2 = phases.clone();
    let result = ui.window().set_rendering_notifier(move |s, _api| {
        if matches!(s, slint::RenderingState::AfterRendering) && !reported2.get() {
            reported2.set(true);
            eprintln!(
                "{{\"event\":\"first_paint\",\"renderer\":\"{}\",\"first_paint_ms\":{:.1},\"method\":\"AfterRendering\"{}}}",
                crate::app::controller::renderer_id(),
                start.elapsed().as_secs_f64() * 1000.0,
                paint_json(&phases2)
            );
        }
    });
    if result.is_err() {
        eprintln!("quire: [measure] set_rendering_notifier failed: {:?}", result.err());
        // No notifier on this backend: the 1 ms single-shot runs as the first
        // event-loop callback, before any drawn frame exists.
        let t = Timer::default();
        let phases3 = phases.clone();
        t.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(1),
            move || {
                if !reported.get() {
                    reported.set(true);
                    eprintln!(
                        "{{\"event\":\"first_paint\",\"renderer\":\"{}\",\"first_paint_ms\":{:.1},\"method\":\"event_loop_proxy\",\"confidence\":\"low\"{}}}",
                        crate::app::controller::renderer_id(),
                        start.elapsed().as_secs_f64() * 1000.0,
                        paint_json(&phases3)
                    );
                }
            },
        );
        leak_timer(t);
    }
}

/// Where the session's files go, once the placement rules have answered.
///
/// The database and the log share a folder by policy (M8 D9), and `decide` is
/// pure, so this reads the same placement the database open uses instead of
/// asking the OS a second time — which is exactly the question Android cannot
/// answer from the environment.
fn data_folder(placement: &crate::storage::data_location::Placement) -> std::path::PathBuf {
    match placement.path().parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    }
}

pub fn run(start: std::time::Instant, touch_mode: bool) -> Result<(), String> {
    // The argument parser reads only strings — no I/O to fail — so running it
    // before logging costs no coverage: `logging::init` installs the panic
    // hook as its first act, and needs the flags to know which directory the
    // log belongs in (M8_FEEDBACK #13).
    let launch = parse_launch_args();
    // The entry point decides the shape — `android_main` says touch, `main` says
    // desktop — and this flag lets a desktop run ask for the other one. It is
    // not a supported mode (no phone, no 44 dp audit); it is how the touch
    // layout got looked at at all before there is hardware.
    let touch_mode = touch_mode || launch.touch;
    let phases = launch
        .measure_startup
        .then(|| PhaseLog::new(start));
    let mark = {
        let phases = phases.clone();
        move |name: &'static str| {
            if let Some(p) = &phases {
                p.mark(name);
            }
        }
    };
    mark("args_parsed");
    let location = crate::storage::data_location::LaunchOptions {
        db_override: launch.db.clone(),
        portable: launch.portable,
    };
    // M9.pre: one resolve of the placement rules, from the platform's own answer
    // about where a per-user library may live. Desktop hands in `%APPDATA%` the
    // way it always did; Android hands in the app's private files directory,
    // which no environment variable names — and a session with no answer at all
    // is the silent in-memory run the notes warned about, so `platform::data_dir`
    // is the one place allowed to say `None`.
    let requested = launch
        .db
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("appdata/quire.db"));
    let per_user = crate::platform::data_dir();
    let placement =
        crate::storage::data_location::decide(&location, &requested, per_user.as_deref());
    logging::init_at(&data_folder(&placement)); // the rotating log + panic hook (SPEC §二十五, M8 D9)
    mark("logging_init");

    // Persistence (M3): open (or create) the database. A failure to open
    // means the session runs in memory only — never fall back to writing
    // over a database we could not read.
    let mut recovered: Option<std::path::PathBuf> = None;
    let mut moved_from: Option<std::path::PathBuf> = None;
    let repo: Option<std::sync::Arc<crate::storage::SqliteRepository>> = {
        // D12: the placement rules (M8 FEEDBACK #13) also carry a legacy
        // `appdata/` library across. What comes back is the file to open plus,
        // once per install, the folder it was moved out of, which the open
        // reports back so the notice bar can say it. The decision itself was
        // already made above — this is its move half, on the same inputs.
        let moved = crate::storage::data_location::migration(
            &location,
            &requested,
            per_user.as_deref(),
        );
        mark("data_location");
        match crate::storage::SqliteRepository::open_at(&moved.path, moved.from) {
            Ok((r, report)) => {
                if let Some(from) = &report.recovered_from {
                    recovered = Some(from.clone());
                }
                moved_from = report.migrated_from.clone();
                report.log();
                Some(std::sync::Arc::new(r))
            }
            Err(e) => {
                eprintln!("quire: database unavailable ({}); running in memory", e);
                None
            }
        }
    };
    mark("repo_open");
    let args = HandleArgs {
        blocks: launch.blocks,
        auto_exit_secs: launch.auto_exit_secs,
        bench_pages: launch.bench_pages,
        pictures: launch.pictures,
        marks: launch.marks,
        code: launch.code,
    };
    let ui = AppWindow::new().map_err(|e| e.to_string())?;
    mark("appwindow_new");
    // A phone has no pointer, and the chrome that assumes one is the whole
    // difference between the two shells. The flag is set before the first layout
    // pass so nothing animates from the desktop shape into the touch one; the
    // components that read it are listed in docs/ANDROID_NOTES.md (M9.pre).
    ui.global::<crate::UIState>().set_touch_mode(touch_mode);
    if launch.measure_startup {
        install_startup_measurement(&ui, start, phases);
    }
    // Window::set_icon does not exist in Slint 1.18 (M8_FEEDBACK #4/#5 note):
    // the taskbar/explorer icon comes from the exe's embedded resource (D8),
    // and the frameless window shows no title bar — nothing user-visible is
    // missing. Revisit on Slint upgrade.
    let repo_for_lan = repo.clone();
    let state = AppState::new(&args, repo);
    mark("state_new");
    // startup notices (restore-from-backup, the D12 library move, and the
    // previous session's abort record — `db_notice` is a queue, #9)
    let mut notices: Vec<String> = Vec::new();
    if let Some(from) = &recovered {
        notices.push(format!(
            "the database was damaged — restored from a backup ({})",
            from.display()
        ));
    }
    if moved_from.is_some() {
        notices.push("the library moved to your user profile".into());
    }
    if !notices.is_empty() {
        state.set_db_notice(format!("{}.", notices.join("; ")));
    }
    // restore the remembered window size after the state is built (slint
    // still counts it as pre-first-paint). Only where a window has a size the
    // app gets to choose: on Android the Activity owns the surface, and a
    // remembered 1280x800 is a desktop window fighting a phone.
    if !touch_mode {
        if let Some((w, h)) = state.window_size_setting() {
            ui.window()
                .set_size(slint::PhysicalSize::new(w as u32, h as u32));
        }
    }
    controller::bind(&ui, &state);
    controller::wire(&ui, &state);
    mark("bind_wire");

    // .md file association: double-clicking a markdown file lands here
    if let Some(path) = launch.open.clone() {
        let g = ui.global::<crate::UIState>();
        controller::import_from_path(&g, &state, &path);
    }
    mark("import_open");

    // LAN share: serve the committed workspace read-only on its own thread.
    // Enabled via --share or the persisted settings toggle (lan.share).
    let share_port = launch.share.or_else(|| {
        if state.setting_flag("lan.share") {
            Some(crate::services::lan_server::DEFAULT_PORT)
        } else {
            None
        }
    });
    if let Some(port) = share_port {
        match &repo_for_lan {
            Some(repo) => {
                let server =
                    crate::services::lan_server::LanServer::new(repo.clone(), port);
                std::thread::spawn(move || match server.bind() {
                    Ok(listener) => server.serve(listener),
                    Err(e) => eprintln!("quire: lan bind failed: {e}"),
                });
            }
            None => eprintln!("quire: --share needs a database; sharing disabled"),
        }
    }

    // LAN pull: import every page from a peer's share endpoint
    if let Some(url) = &launch.pull {
        match crate::services::lan_client::pull_workspace(url) {
            Ok(pages) => {
                let g = ui.global::<crate::UIState>();
                let count = controller::import_lan_pages(&g, &state, url, pages);
                eprintln!("quire: imported {count} pages from {url}");
            }
            Err(e) => eprintln!("quire: pull failed: {e}"),
        }
    }
    mark("lan_setup");

    if launch.dump_state {
        let pages = state.workspace.borrow().page_count();
        let (blocks, marked, coloured) = {
            let d = state.doc.borrow();
            let mut n = 0;
            let mut marked = 0;
            let mut coloured = 0;
            for pid in [102, 105] {
                let rows = d.page_blocks(crate::core::PageId(pid));
                n += rows.len();
                marked += rows.iter().filter(|b| !b.marks.is_empty()).count();
                coloured += rows.iter().filter(|b| b.lang != crate::core::Lang::Plain).count();
            }
            (n, marked, coloured)
        };
        // `coloured` is the highlight arm's identity: a binary without the
        // language field has no such count to print.
        eprintln!("dump-state: pages={pages} gs+atlas-blocks={blocks} marked={marked} coloured={coloured}");
    }

    if args.auto_exit_secs > 0.0 {
        let t = Timer::default();
        // The bench harness reads the decode cache's high-water mark off
        // stderr here; a normal run has no reason to ask for it.
        let state_w = std::rc::Rc::downgrade(&state);
        let report_cache = launch.dump_state;
        t.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_secs_f64(args.auto_exit_secs),
            move || {
                if report_cache {
                    if let Some(state) = state_w.upgrade() {
                        eprint!("{}", state.attachment_cache_report());
                    }
                }
                let _ = slint::quit_event_loop();
            },
        );
        leak_timer(t);
    }

    if launch.scroll {
        // Scene F: programmatic continuous scroll. The editor's viewport-y is
        // two-way bound to UIState.editor-scroll-y, so advancing the property
        // drives the same repaint path a mouse wheel would — and Slint's
        // viewport-y is *negative* the way down, so the step subtracts.
        //
        // A ListView only knows the height of the rows it has realized, so the
        // clamp grows a frame behind the scroll and the position stalls for a
        // tick or two without the bottom being near. One stalled tick used to
        // mean "wrap to top"; now a wrap needs the position to sit still for a
        // whole second of ticks.
        let ui_w = ui.as_weak();
        let state_w = std::rc::Rc::downgrade(&state);
        let step = launch.scroll_step;
        let stalls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let t = Timer::default();
        t.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(16),
            move || {
                let (Some(ui), Some(state)) = (ui_w.upgrade(), state_w.upgrade()) else {
                    return;
                };
                let g = ui.global::<crate::UIState>();
                let cur = g.get_editor_scroll_y();
                // A ListView that has not measured its rows yet reports a
                // shorter content height, so the clamp moves under the scroll
                // and the read-back differs from what was set for a tick. Only
                // a position that was actually *put* there counts as stuck.
                let stuck = cur == state.last_scroll_y();
                state.set_last_scroll_y(cur);
                if stuck && cur < 0.0 {
                    stalls.set(stalls.get() + 1);
                    if stalls.get() < 60 {
                        return; // the ListView is still catching up
                    }
                    // bottom reached and held: wrap to top
                    stalls.set(0);
                    g.set_editor_scroll_y(0.0);
                } else {
                    stalls.set(0);
                    g.set_editor_scroll_y(cur - step);
                }
            },
        );
        leak_timer(t);
    }

    if launch.bench_pages > 0 {
        // Scene G: cycle through the bench pages, exercising model swap +
        // delegate rebuild at a fixed rate.
        let ids: Vec<i32> = state
            .workspace
            .borrow()
            .dfs_order()
            .into_iter()
            .filter(|id| *id >= crate::app::workspace::BENCH_ID_BASE)
            .collect();
        let ui_w = ui.as_weak();
        let state_w = std::rc::Rc::downgrade(&state);
        let cursor = std::cell::Cell::new(0usize);
        let t = Timer::default();
        t.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(120),
            move || {
                let (Some(ui), Some(state)) = (ui_w.upgrade(), state_w.upgrade()) else {
                    return;
                };
                let mut c = cursor.get();
                controller::bench_switch_next(&ui, &state, &ids, &mut c);
                cursor.set(c);
            },
        );
        leak_timer(t);
    }

    if let Some(scene) = launch.scene {
        let ui_w = ui.as_weak();
        let state_w = std::rc::Rc::downgrade(&state);
        let name = scene.clone();
        let t = Timer::default();
        t.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(600),
            move || {
                if let (Some(ui), Some(state)) = (ui_w.upgrade(), state_w.upgrade()) {
                    controller::apply_scene(&ui, &state, &name);
                }
            },
        );
        leak_timer(t);
    }

    mark("pre_event_loop");
    ui.run().map_err(|e| e.to_string())?;
    // remember the window size, then flush dirty state on close (SPEC §十九)
    let size = ui.window().size();
    state.record_window_size(size.width as f64, size.height as f64);
    state.persistence_force_flush();
    // The clean-exit record (M8_FEEDBACK #10, ADR-0018): the next start reads
    // its absence — with no panic report — as "killed, crashed natively, or
    // lost power". Normal exit path only, after the final flush, so a session
    // that dies any other way still counts as unclean.
    logging::note(logging::END_RECORD);
    Ok(())
}


