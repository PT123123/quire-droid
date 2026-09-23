// quire-typing — scene E: programmatic typing benchmark (SPEC §二十二, M4/M7
// acceptance). Not a test: it runs the real app on the real event loop and
// types into it, so every number covers the whole path one keystroke takes —
// Slint property binding, the 300 ms edit debounce, the command layer and
// undo history, the model row update, the repaint, the 600 ms persistence
// flush and the FTS5 index write behind it (ADR-0014).
//
// Build: cargo build --release --bin quire-typing
// Usage: quire-typing --blocks 1000 --rate 30 --duration 8 [--search-every 200]
//
// CPU% and memory are sampled by benchmarks/scripts/bench.ps1 -Typing; this
// binary reports single-threaded latencies on stdout as one JSON line.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quire::app::controller;
use quire::app::state::{AppState, HandleArgs};
use quire::services::search_service::SearchService;
use quire::storage::search_index::SearchRequest;
use quire::storage::SqliteRepository;
use quire::AppWindow;
// The library target is `quire_shell`, so it never shares its PDB with the
// binaries — the alias keeps every path above reading as `quire::`
// (Cargo.toml's `[lib]` carries the reason).
use quire_shell as quire;
use slint::{ComponentHandle, Model, Timer, TimerMode};

struct Args {
    /// Blocks on the page typed into (scene B/C/D content sizes).
    blocks: usize,
    /// Keystrokes per second.
    rate: f64,
    /// Seconds of typing, then exit.
    duration: f64,
    /// Run one full-text query every N keystrokes and time it (0 = off).
    search_every: usize,
    /// Rows of the page that carry a bold mark (0 = none, as ever).
    marks: usize,
    code: usize,
    /// Database file to type against (default: in-memory).
    db: Option<std::path::PathBuf>,
    /// Print one line per keystroke to stderr.
    verbose: bool,
}

fn parse_str(argv: &[String], key: &str) -> Option<String> {
    argv.iter()
        .position(|a| a == key)
        .and_then(|i| argv.get(i + 1))
        .cloned()
}

impl Args {
    fn parse(argv: &[String]) -> Self {
        let get = |key: &str, default: f64| -> f64 {
            parse_str(argv, key)
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        Args {
            blocks: get("--blocks", 1000.0) as usize,
            rate: get("--rate", 30.0).max(0.1),
            duration: get("--duration", 8.0),
            search_every: get("--search-every", 0.0) as usize,
            marks: get("--marks", 0.0) as usize,
            code: get("--code", 0.0) as usize,
            db: parse_str(argv, "--db").map(std::path::PathBuf::from),
            verbose: argv.iter().any(|a| a == "--verbose"),
        }
    }
}

/// Driver state, living on the UI thread only.
struct Driver {
    /// Row indices into the open page's model.
    rows: Vec<usize>,
    rng: u64,
    /// Growth text, Chinese on purpose: every write is re-segmented by the
    /// search index, so this is the worst case.
    corpus: Vec<char>,
    keystrokes: usize,
    /// Handler duration per keystroke, microseconds.
    samples: Vec<u64>,
    searches: Vec<u64>,
    /// First and last keystroke, so the rate ignores window setup and the
    /// flush pad.
    first: Option<Instant>,
    last: Instant,
    verbose: bool,
}

impl Driver {
    fn pick(&mut self) -> usize {
        // xorshift64: deterministic between runs, no dependency
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rows[(self.rng as usize) % self.rows.len()]
    }

    fn finish(&self, rate: f64, blocks: usize, search_every: usize) {
        // the active window spans keystrokes only
        let active = match self.first {
            Some(first) => (self.last - first).as_secs_f64(),
            None => 0.0,
        };
        let json = |samples: &[u64], label: &str| -> String {
            if samples.is_empty() {
                return format!("\"{label}\": null");
            }
            let mut sorted = samples.to_vec();
            sorted.sort_unstable();
            let at = |q: f64| sorted[((sorted.len() as f64 * q) as usize).min(sorted.len() - 1)];
            format!(
                "\"{label}\": {{\"median_us\": {}, \"p95_us\": {}, \"max_us\": {}, \"n\": {}}}",
                at(0.5),
                at(0.95),
                sorted[sorted.len() - 1],
                sorted.len()
            )
        };
        println!(
            "{{\"scene\": \"E\", \"blocks\": {blocks}, \"requested_rate\": {rate}, \
             \"achieved_rate\": {}, \"active_s\": {active:.3}, \"keystrokes\": {}, \
             \"search_every\": {search_every}, {}, {}}}",
            round(self.keystrokes as f64 / active.max(1e-6)),
            self.keystrokes,
            json(&self.samples, "handler"),
            json(&self.searches, "search"),
        );
        if self.verbose {
            eprintln!("typed {} keystrokes into {blocks} rows", self.keystrokes);
        }
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn main() {
    // Same rationale as the GUI binary: Slint's initial binding/layout
    // evaluation is deep (ADR-0009).
    let child = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(run)
        .expect("spawn UI thread");
    match child.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("quire-typing: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("quire-typing: UI thread panicked");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().collect();
    let args = Args::parse(&argv);

    // The repository stays a concrete Arc so the search service can be built
    // from it; the app gets the trait object its pipeline expects
    // (ADR-0014 consequences).
    let repo = Arc::new(match args.db.as_deref() {
        Some(path) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            SqliteRepository::open(path).map_err(|e| format!("open {path:?}: {e}"))?
        }
        None => SqliteRepository::in_memory().map_err(|e| e.to_string())?,
    });
    let service = Rc::new(SearchService::new(repo.clone()));

    let ui = AppWindow::new().map_err(|e| e.to_string())?;
    let handle = HandleArgs {
        blocks: args.blocks,
        auto_exit_secs: 0.0,
        bench_pages: 0,
        pictures: 0,
        marks: args.marks,
        code: args.code,
    };
    let state = AppState::new(&handle, Some(repo.clone()));
    controller::bind(&ui, &state);
    controller::wire(&ui, &state);

    let rows: Vec<usize> = (0..state.blocks.row_count())
        .filter(|i| state.blocks.row_data(*i).is_some_and(|r| r.id > 0))
        .collect();
    let on_page = rows.len();
    if rows.is_empty() {
        return Err(format!(
            "the opened page holds no editable blocks (asked for {})",
            args.blocks
        ));
    }

    let driver = Rc::new(RefCell::new(Driver {
        rows,
        rng: 0x2545_F491_4F6C_DD1D,
        corpus: "the quick brown 中文 fox jumps over 字体回退 lazy dog 行高设计 ".chars().collect(),
        keystrokes: 0,
        samples: Vec::new(),
        searches: Vec::new(),
        first: None,
        last: Instant::now(),
        verbose: args.verbose,
    }));

    // one keystroke: write the live editing text and fire the same callback
    // the editor fires per character, so the controller's debounce decides
    // when the command (and the save) lands
    {
        let ui_w = ui.as_weak();
        let state = state.clone();
        let driver = driver.clone();
        let service = service.clone();
        let search_every = args.search_every;
        let t: &'static Timer = Box::leak(Box::new(Timer::default()));
        t.start(
            TimerMode::Repeated,
            Duration::from_secs_f64(1.0 / args.rate),
            move || {
                let Some(ui) = ui_w.upgrade() else { return };
                let g = ui.global::<quire::UIState>();
                let started = Instant::now();

                let (row_index, block_id, next_text) = {
                    let mut d = driver.borrow_mut();
                    let i = d.pick();
                    let row = match state.blocks.row_data(i) {
                        Some(row) => row,
                        None => return,
                    };
                    let ch = d.corpus[d.keystrokes % d.corpus.len()];
                    let mut text = row.text.to_string();
                    text.push(ch);
                    (i, row.id, text)
                };
                g.set_editing_id(block_id);
                g.set_editing_text(next_text.as_str().into());
                g.set_pending_caret(next_text.chars().count() as i32);
                g.invoke_editing_changed(0.0f32, 0.0f32, 0.0f32);

                let mut d = driver.borrow_mut();
                d.first.get_or_insert(started);
                d.last = started;
                d.keystrokes += 1;
                d.samples.push(started.elapsed().as_micros() as u64);
                if d.verbose {
                    eprintln!("{} -> row {row_index} id {block_id} len {}", d.keystrokes, next_text.chars().count());
                }
                let count = d.keystrokes;
                drop(d);

                if search_every > 0 && count % search_every == 0 {
                    let started = Instant::now();
                    let hits = service
                        .search(&SearchRequest::new("中文"))
                        .map(|hits| hits.len())
                        .unwrap_or(usize::MAX);
                    let micros = started.elapsed().as_micros() as u64;
                    driver.borrow_mut().searches.push(micros);
                    if args.verbose {
                        eprintln!("  search #{count}: {hits} hits in {micros} µs");
                    }
                }
            },
        );
    }

    // stop after the sampling window and print the report from the UI thread
    {
        let driver = driver.clone();
        let state = state.clone();
        let rate = args.rate;
        let search_every = args.search_every;
        let t: &'static Timer = Box::leak(Box::new(Timer::default()));
        t.start(
            TimerMode::SingleShot,
            Duration::from_secs_f64(args.duration) + Duration::from_millis(1000),
            move || {
                // the final burst has flushed by now; write it for real
                state.persistence_force_flush();
                driver.borrow().finish(rate, on_page, search_every);
                let _ = slint::quit_event_loop();
            },
        );
    }

    ui.run().map_err(|e| e.to_string())?;
    Ok(())
}
