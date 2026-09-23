// quire-shot — headless visual regression tool.
//
// Renders the real AppWindow through Slint's software renderer into an
// offscreen buffer and writes a BMP (PNG encoding would need an extra
// dependency; the capture script converts to PNG). No window is created,
// so shots are deterministic and immune to foreground/z-order issues —
// see docs/UI_ARCHITECTURE.md "Visual regression".
//
// Build: cargo build --features software --bin quire-shot
// Usage: quire-shot --out shot.bmp [--scene menu] [--w 1280] [--h 800]
//        [--click x,y] [--key escape]
//
// --click/--key dispatch a real input event through the window after the
// first render (and after the scene overlay), then re-render and print a
// probe line with the open/closed state of every popup. This is what
// verifies the close-on-click-outside behavior headlessly.

use quire::app::controller;
use quire::app::state::{AppState, HandleArgs};
use quire::{AppWindow, UIState};
// The library target is `quire_shell`, so it never shares its PDB with the
// binaries — the alias keeps every path above reading as `quire::`
// (Cargo.toml's `[lib]` carries the reason).
use quire_shell as quire;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, PhysicalSize, Rgb8Pixel, SharedPixelBuffer, SharedString};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
}

struct HeadlessPlatform;

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
        Ok(window)
    }
}

/// Minimal 24-bit BMP writer (bottom-up rows, 4-byte padding).
fn write_bmp(path: &str, buffer: &SharedPixelBuffer<Rgb8Pixel>) -> Result<(), String> {
    let w = buffer.width() as usize;
    let h = buffer.height() as usize;
    let stride = (w * 3 + 3) & !3;
    let data_size = (stride * h + 54) as u32;
    let mut out = Vec::with_capacity(data_size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&24u16.to_le_bytes()); // bpp
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((stride * h) as u32).to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes()); // 72 dpi
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    let src = buffer.as_bytes();
    for y in (0..h).rev() {
        let row = &src[y * w * 3..(y + 1) * w * 3];
        for px in row.chunks_exact(3) {
            out.push(px[2]);
            out.push(px[1]);
            out.push(px[0]);
        }
        out.resize(out.len() + (stride - w * 3), 0);
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}

fn render(w: u32, h: u32) -> bool {
    WINDOW.with(|slot| -> bool {
        let window = slot.borrow().clone().expect("window adapter");
        window.draw_if_needed(|renderer| {
            let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(w, h);
            let stride = buffer.width() as usize;
            renderer.render(buffer.make_mut_slice(), stride);
            BUFFER.with(|b| *b.borrow_mut() = Some(buffer));
        })
    })
}

fn parse(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Print the open/closed state of every popup (the --click/--key probe).
fn probe(ui: &AppWindow, label: &str) {
    let g = ui.global::<UIState>();
    println!(
        "{label} menu-open={} slash-open={} slash-insert={} block-menu-id={} palette-open={} search-open={}",
        g.get_menu_open(),
        g.get_slash_open(),
        g.get_slash_insert(),
        g.get_block_menu_open_id(),
        g.get_palette_open(),
        g.get_search_open(),
    );
}

/// Deliver a synthetic click (press + release) at logical coordinates —
/// the same route a real mouse takes through the engine's event dispatch.
fn dispatch_click(window: &slint::Window, x: f32, y: f32) {
    let pos = LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Left });
    window.dispatch_event(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Left });
}

/// Park the pointer at logical coordinates without pressing anything: the
/// engine's enter events fire on the move, which is the only way a headless
/// shot can light up hover-only UI (a table's edge toolbar, a row's handle).
fn dispatch_hover(window: &slint::Window, x: f32, y: f32) {
    window.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x, y) });
}

/// Parse an "x,y" argument value.
fn parse_xy(spec: &str) -> (f32, f32) {
    spec.split_once(',')
        .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)))
        .unwrap_or_else(|| panic!("expected x,y, got {spec}"))
}

/// Print the block projection as `id:kind` pairs (the --probe-blocks flag):
/// lets a scripted run assert what a popup action did to the document.
fn probe_blocks(ui: &AppWindow) {
    let g = ui.global::<UIState>();
    let kinds: Vec<String> = (0..g.get_blocks().row_count())
        .filter_map(|i| g.get_blocks().row_data(i))
        .map(|b| format!("{}:{}", b.id, b.kind))
        .collect();
    println!("blocks: {}", kinds.join(" "));
}

fn main() -> Result<(), String> {
    // Same rationale as the GUI binary: Slint's initial binding/layout
    // evaluation is deep; give it headroom (ADR-0009).
    let child = std::thread::Builder::new().stack_size(8 * 1024 * 1024)
        .spawn(|| run()).expect("spawn render thread");
    child.join().unwrap_or_else(|_| Err("render thread panicked".into()))
}

fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().collect();
    let out = parse(&argv, "--out").unwrap_or_else(|| "quire-shot.bmp".into());
    let scene = parse(&argv, "--scene");
    let w: u32 = parse(&argv, "--w").and_then(|v| v.parse().ok()).unwrap_or(1280);
    let h: u32 = parse(&argv, "--h").and_then(|v| v.parse().ok()).unwrap_or(800);

    slint::platform::set_platform(Box::new(HeadlessPlatform))
        .map_err(|e| format!("set_platform: {e:?}"))?;

    let ui = AppWindow::new().map_err(|e| e.to_string())?;
    ui.window().set_size(PhysicalSize::new(w, h));

    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    // The backlink scenes read `marks` and `blocks.page_ref` out of a database,
    // because the panel *is* a query rather than a fixture — so those scenes
    // get a library and every other scene keeps the repo-less setup it has
    // always had. An empty in-memory library still falls back to the demo
    // session (`AppState::new` keeps the mock pages when nothing was
    // persisted), so the scenes around the panel are the same ones as before:
    // switching this on is meant to change the pixels of exactly the new
    // scenes, and a sweep with a baseline says whether it did.
    // SPEC §三十九: the database scenes join them for the same reason — the
    // table draws *records*, and records live in SQL (ADR-0067); a repo-less
    // session has none, so `database-table` would photograph an empty grid.
    let needs_db = scene
        .as_deref()
        .map(|s| s.contains("backlinks") || s.contains("database"))
        .unwrap_or(false);
    let repo = if needs_db {
        Some(std::sync::Arc::new(
            quire::storage::SqliteRepository::in_memory().map_err(|e| e.to_string())?,
        ))
    } else {
        None
    };
    let state = AppState::new(&args, repo);
    controller::bind(&ui, &state);
    controller::wire(&ui, &state);
    // `renderer_name()` is a compile-time guess about which renderer *feature*
    // was asked for, and this process did not ask: `HeadlessPlatform` above is
    // Slint's software rasterizer whatever else is compiled in — a
    // `--features software` build keeps the `femtovg` default on, which is why
    // every swept PNG used to caption itself "FemtoVG · GL" over a picture no
    // GPU drew. The sweep is evidence about a renderer, so it says which one ran.
    ui.global::<UIState>()
        .set_renderer_name("Software · headless".into());
    if let Some(scene) = &scene {
        controller::apply_scene(&ui, &state, scene);
    }
    // --scroll-y drives the same viewport mirror scene F uses. It exists
    // because a scroll that moves nothing still burns CPU, and the bench
    // harness shipped with that bug: a run at 0 and a run at ±2000 have to
    // give different pixels before either number means anything.
    if let Some(spec) = parse(&argv, "--scroll-y") {
        let v: f32 = spec.parse().unwrap_or(0.0);
        ui.global::<UIState>().set_editor_scroll_y(v);
    }
    ui.show().map_err(|e| e.to_string())?;

    let drawn = render(w, h);
    if !drawn {
        return Err("window never requested a redraw".into());
    }


    if let Some(scene) = &scene {
        controller::apply_scene_overlay(&ui, &state, scene);
    }

    // --click/--key: drive a real input event, then re-render so the BMP
    // shows the post-input state; the probe lines document the popup flags
    // before and after the input (the "before" one proves the scene really
    // opened the popup the "after" one claims to have dismissed).
    let mut interacted = false;
    if let Some(spec) = parse(&argv, "--hover") {
        let (x, y) = parse_xy(&spec);
        probe(&ui, "before-input:");
        dispatch_hover(ui.window(), x, y);
        interacted = true;
    }
    if let Some(spec) = parse(&argv, "--click") {
        let (x, y) = parse_xy(&spec);
        probe(&ui, "before-input:");
        dispatch_click(ui.window(), x, y);
        interacted = true;
    }
    if let Some(key) = parse(&argv, "--key") {
        let text: SharedString = match key.as_str() {
            "escape" => slint::platform::Key::Escape.into(),
            "return" => slint::platform::Key::Return.into(),
            other => other.into(),
        };
        probe(&ui, "before-input:");
        ui.window().dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        ui.window().dispatch_event(WindowEvent::KeyReleased { text });
        interacted = true;
    }
    if interacted {
        probe(&ui, "after-input: ");
    }
    if argv.iter().any(|a| a == "--probe-blocks") {
        probe_blocks(&ui);
    }
    BUFFER.with(|b| *b.borrow_mut() = None);
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().clone() {
            // the overlay flags mark popup state, not the window's redraw
            // flag — request the second pass explicitly
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(w, h);
                let stride = buffer.width() as usize;
                renderer.render(buffer.make_mut_slice(), stride);
                BUFFER.with(|b| *b.borrow_mut() = Some(buffer));
            });
        }
    });

    let buffer = BUFFER.with(|b| b.borrow_mut().take());
    match buffer {
        Some(buffer) => {
            if let Some(parent) = std::path::Path::new(&out).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            write_bmp(&out, &buffer)?;
            println!("wrote {out}");
            Ok(())
        }
        None => Err("render produced no buffer".into()),
    }
}

thread_local! {
    static BUFFER: RefCell<Option<SharedPixelBuffer<Rgb8Pixel>>> = const { RefCell::new(None) };
}
