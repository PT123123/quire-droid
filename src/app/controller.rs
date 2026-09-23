// UI callbacks land here. The controller owns *when* something happens;
// state.rs owns *what* the data looks like; workspace.rs owns the tree
// itself. Nothing here touches DB or disk.
//
// 1.18 global-handle pattern: `ui.global::<UIState>()` borrows the handle, so
// 'static callbacks capture a Weak and upgrade() it at fire time.

use crate::app::state::{
    core_page_id, kind_from_int, palette_action, AppState, PaletteAction, PAGE_GETTING_STARTED,
    ROW_NEW_PAGE,
};
use crate::core::database::PropertyKind;
use crate::core::{BlockId, Change, Command, Lang};
use crate::{AppWindow, UIState};
use slint::{ComponentHandle, Global, Model};
use std::rc::Rc;

/// Y offset of the first tree row inside the window: title bar (40) +
/// workspace header (36) + search row (28) + settings row (28) + spacer (8).
pub const TREE_TOP_PX: f32 = 140.0;

/// plain-text marker identifying the app's own drag payload:
/// "slint-notion/block:<id>". Foreign drops (files etc.) don't carry it and
/// are rejected by the row DropAreas.
const BLOCK_DRAG_MIME: &str = "slint-notion/block:";
const PAGE_DRAG_MIME: &str = "slint-notion/page:";

fn block_drag_id(data: &slint::DataTransfer) -> Option<i32> {
    data.plain_text().ok()?.strip_prefix(BLOCK_DRAG_MIME)?.parse().ok()
}

fn page_drag_id(data: &slint::DataTransfer) -> Option<i32> {
    data.plain_text().ok()?.strip_prefix(PAGE_DRAG_MIME)?.parse().ok()
}

/// The extensions the attachment decoder can actually read. Named once because
/// the picture picker and the cover picker promise the same list (SPEC §三十七).
const PICTURE_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif"];

/// How far the document starts from the window's left edge: the rail's width
/// while it stands beside the page. On a touch screen the same list is a drawer
/// *over* the page (M9.pre), so the page never moves — and with the drawer open
/// `sidebar-open` is true while the width the editor lost is still none.
fn content_edge_offset(g: &UIState<'_>) -> f32 {
    if g.get_sidebar_open() && !g.get_touch_mode() {
        260.0
    } else {
        0.0
    }
}

/// Run a file dialog and turn its answer into "the path, or nothing happened".
///
/// Every picker in the app comes through here, which is what lets the Android
/// build have no file dialog at all without a single silent button: the platform
/// answers `Unsupported` and the notice bar says so (see `src/platform/picker.rs`).
fn ask(g: &UIState<'_>, picker: crate::platform::picker::Picker) -> Option<std::path::PathBuf> {
    match picker.pick() {
        crate::platform::picker::Chosen::Path(path) => Some(path),
        crate::platform::picker::Chosen::Cancelled => None,
        crate::platform::picker::Chosen::Unsupported(why) => {
            g.set_db_notice(format!("{why}.").into());
            None
        }
    }
}

// ─── LAN sync (crate::sync) ─────────────────────────────────────────────────
//
// The engine (`crate::services::sync::engine`) owns the sockets and the worker thread;
// this is its UI half: the pump that answers its jobs on this thread (the
// session is `Rc`, so nothing else may touch it), the settings dialog's rows,
// and the five callbacks that dialog fires.

/// Start the engine and the pump. Called once from `wire`.
fn start_sync(ui: &AppWindow, state: &Rc<AppState>) {
    let me = state.sync_self_info();
    let (job_tx, job_rx) = std::sync::mpsc::channel::<crate::services::sync::engine::Job>();
    let cmd_tx = crate::services::sync::engine::Engine::start(me, job_tx);

    // the dialog's own switch and rows, pushed once at wire time
    {
        let (auto, _) = state.sync_config();
        ui.global::<UIState>().set_sync_auto(auto);
        refresh_sync_ui(&ui.global::<UIState>(), state);
    }

    {
        let gw = ui.global::<UIState>().as_weak();
        let s = state.clone();
        let cmd = cmd_tx.clone();
        ui.global::<UIState>().on_sync_now(move |id| {
            let g = gw.upgrade().unwrap();
            if let Some(peer) = s.sync_peers().into_iter().find(|p| p.id == id.as_str()) {
                g.set_sync_status(format!("正在与 {} 同步…", peer.name).into());
                let _ = cmd.send(crate::services::sync::engine::Cmd::SyncWith(peer));
            }
        });
    }
    {
        let gw = ui.global::<UIState>().as_weak();
        let s = state.clone();
        let cmd = cmd_tx.clone();
        ui.global::<UIState>().on_sync_pair(move |id| {
            let g = gw.upgrade().unwrap();
            if let Some(peer) = s.sync_peers().into_iter().find(|p| p.id == id.as_str()) {
                g.set_sync_status(format!("正在请求 {} 配对…", peer.name).into());
                let _ = cmd.send(crate::services::sync::engine::Cmd::PairWith(peer));
            }
        });
    }
    {
        let gw = ui.global::<UIState>().as_weak();
        let s = state.clone();
        ui.global::<UIState>().on_sync_forget(move |id| {
            let g = gw.upgrade().unwrap();
            s.sync_forget_peer(&id);
            g.set_sync_status("已忘记设备。".into());
            refresh_sync_ui(&g, &s);
        });
    }
    {
        let gw = ui.global::<UIState>().as_weak();
        let s = state.clone();
        ui.global::<UIState>().on_sync_auto_toggled(move |on| {
            let g = gw.upgrade().unwrap();
            s.sync_set_auto(on);
            g.set_sync_status(
                if on {
                    "自动同步已开启 — 已配对设备每分钟同步一次。"
                } else {
                    "自动同步已关闭 — 请手动「立即同步」。"
                }
                .into(),
            );
        });
    }
    {
        let gw = ui.global::<UIState>().as_weak();
        let cmd = cmd_tx.clone();
        ui.global::<UIState>().on_sync_add(move |text| {
            let g = gw.upgrade().unwrap();
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }
            let (ip, port) = match text.rsplit_once(':') {
                Some((host, p)) => match p.parse::<u16>() {
                    Ok(port) => (host.to_string(), port),
                    Err(_) => (text.clone(), crate::services::sync::SYNC_PORT),
                },
                None => (text.clone(), crate::services::sync::SYNC_PORT),
            };
            g.set_sync_status(format!("正在查找 {ip}:{port} 上的 Quire…").into());
            let _ = cmd.send(crate::services::sync::engine::Cmd::ProbeAdd { ip, port });
        });
    }

    // the pump: drain the engine's jobs on this thread, then decide whether
    // the periodic round is due. A `Box::leak`ed Timer is what keeps it
    // armed for the whole run (dropping a Timer cancels it) — the same
    // discipline the flush timer follows.
    let ui_w = ui.as_weak();
    let s = state.clone();
    let cmd = cmd_tx.clone();
    let last_auto = std::cell::Cell::new(std::time::Instant::now());
    let t: &'static slint::Timer = Box::leak(Box::new(slint::Timer::default()));
    t.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        move || {
            let Some(ui) = ui_w.upgrade() else { return };
            let g = ui.global::<UIState>();
            let mut answered = false;
            while let Ok(job) = job_rx.try_recv() {
                handle_sync_job(&g, &s, job);
                answered = true;
            }
            if answered {
                refresh_sync_ui(&g, &s);
            }
            let (auto, interval) = s.sync_config();
            if auto && last_auto.get().elapsed().as_secs() >= interval {
                last_auto.set(std::time::Instant::now());
                for peer in s.sync_peers().into_iter().filter(|p| p.paired) {
                    let _ = cmd.send(crate::services::sync::engine::Cmd::SyncWith(peer));
                }
            }
        },
    );
}

/// One engine job, answered on the UI thread.
fn handle_sync_job(g: &UIState<'_>, state: &Rc<AppState>, job: crate::services::sync::engine::Job) {
    use crate::services::sync::engine::Job;
    match job {
        Job::ExportSnapshot { reply } => {
            let _ = reply.send(state.sync_export());
        }
        Job::ListLocalAttachments { reply } => {
            let ids: Vec<u64> = state.attachments.borrow().keys().map(|k| *k as u64).collect();
            let _ = reply.send(ids);
        }
        Job::AttachmentBytes { id, reply } => {
            let bytes = {
                let atts = state.attachments.borrow();
                atts.get(&(id as i64))
                    .and_then(|att| std::fs::read(state.store.display_path(att)).ok())
            };
            if let Some(bytes) = bytes {
                let _ = reply.send(bytes);
            }
        }
        Job::ApplyRemote {
            peer,
            snapshot,
            bytes,
            reply,
        } => {
            // An *inbound push* arrives without a kind (the server has only
            // the snapshot to name the sender by, see `sync::server`), and it
            // is refused unless the device is in the peers table as paired —
            // otherwise anything on the network could write into this
            // library. The pull half always carries a full record from the
            // table, which only holds paired peers the user can see.
            let inbound_push = peer.kind.is_empty();
            let known = state
                .sync_peers()
                .iter()
                .any(|p| p.id == peer.id && p.paired);
            if inbound_push && !known {
                let message = format!(
                    "{} 推送了快照，但未配对 — 已拒绝",
                    if peer.name.is_empty() { "未知设备" } else { &peer.name }
                );
                state.sync_log_push(&peer.id, false, &message);
                g.set_sync_status(message.clone().into());
                let _ = reply.send(Err(message));
                return;
            }
            let result = state.sync_apply_remote(&snapshot, &bytes, &peer);
            match &result {
                Ok(_) => {
                    // the workspace moved under the open page: redraw what the
                    // window shows before anything else looks at it
                    open(g, state, state.open_page.get());
                    state.reproject_blocks();
                    state.db_refresh_page(None);
                    state.sync_log_push(&peer.name, true, "已合并对端的更改");
                    g.set_sync_status(format!("已与 {} 同步。", peer.name).into());
                }
                Err(e) => {
                    state.sync_log_push(&peer.name, false, e);
                    g.set_sync_status(format!("同步失败：{e}").into());
                }
            }
            // whoever reached us is trusted from here on — that is the
            // pairing handshake's receiving half
            state.sync_note_device(
                &peer.id,
                &peer.name,
                &peer.kind,
                &peer.ip,
                peer.port,
                Some(true),
            );
            let _ = reply.send(result);
        }
        Job::InboundPair { device, ip, reply } => {
            state.sync_note_device(
                &device.id,
                &device.name,
                &device.kind,
                &ip,
                device.port,
                Some(true),
            );
            state.sync_log_push(&device.name, true, "已配对");
            g.set_sync_status(format!("{} 请求配对，现已信任。", device.name).into());
            let _ = reply.send(true);
        }
        Job::Discovered { device, ip } => {
            state.sync_note_device(
                &device.id,
                &device.name,
                &device.kind,
                &ip,
                device.port,
                None,
            );
        }
        Job::Paired { device, ip } => {
            state.sync_note_device(
                &device.id,
                &device.name,
                &device.kind,
                &ip,
                device.port,
                Some(true),
            );
            state.sync_log_push(&device.name, true, "已配对");
            g.set_sync_status(format!("已与 {} 配对。", device.name).into());
        }
        Job::SyncDone {
            peer_id,
            ok,
            message,
        } => {
            state.sync_note_synced(&peer_id, ok);
            state.sync_log_push(&peer_id, ok, &message);
            if ok {
                g.set_sync_status(message.into());
            } else {
                g.set_sync_status(message.clone().into());
                // a failed round is worth the notice bar: the user asked for
                // a sync (or left auto on) and nothing moved
                g.set_db_notice(format!("同步：{message}").into());
            }
        }
        Job::AutoTick => {}
    }
}

/// The dialog's rows, rebuilt from the peers table and the log.
fn refresh_sync_ui(g: &UIState<'_>, state: &AppState) {
    let now = crate::services::sync::engine::now_unix();
    let rows: Vec<crate::SyncRow> = state
        .sync_peers()
        .into_iter()
        .map(|p| {
            // the announcement cadence is 4 s; a peer heard inside ~15 s is
            // on the network right now
            let online = now.saturating_sub(p.last_seen) < 15;
            let status = if !p.paired {
                "已发现 · 未配对".to_string()
            } else if p.last_sync.is_empty() {
                "已配对 · 从未同步".to_string()
            } else {
                format!("已配对 · 已同步 {}", p.last_sync)
            };
            crate::SyncRow {
                id: p.id.into(),
                label: format!(
                    "{} ({})",
                    if p.name.is_empty() { "设备" } else { &p.name },
                    if p.kind.is_empty() { "未知" } else { &p.kind }
                )
                .into(),
                status: status.into(),
                ip: p.ip.into(),
                paired: p.paired,
                online,
            }
        })
        .collect();
    g.set_sync_rows(slint::ModelRc::new(slint::VecModel::from(rows)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::BlockKind;

    #[test]
    fn block_drag_payload_roundtrip() {
        let mut data = slint::DataTransfer::default();
        data.set_plain_text(format!("{BLOCK_DRAG_MIME}42").into());
        assert_eq!(block_drag_id(&data), Some(42));
        // foreign payloads (files, external text) never parse to a block
        let mut foreign = slint::DataTransfer::default();
        foreign.set_plain_text("some pasted text".into());
        assert_eq!(block_drag_id(&foreign), None);
        assert_eq!(block_drag_id(&slint::DataTransfer::default()), None);
    }

    #[test]
    fn markdown_shortcuts_convert_and_strip() {
        let conv = |t: &str| markdown_convert(t, Some(BlockKind::Paragraph)).map(|(k, r, _)| (k, r));
        assert_eq!(conv("# "), Some((BlockKind::Heading1, "".to_string())));
        assert_eq!(conv("## Big"), Some((BlockKind::Heading2, "Big".into())));
        assert_eq!(conv("### Small"), Some((BlockKind::Heading3, "Small".into())));
        assert_eq!(conv("- item"), Some((BlockKind::Bullet, "item".into())));
        assert_eq!(conv("* item"), Some((BlockKind::Bullet, "item".into())));
        assert_eq!(conv("1. first"), Some((BlockKind::Numbered, "first".into())));
        assert_eq!(conv("12. x"), Some((BlockKind::Numbered, "x".into())));
        assert_eq!(conv("[] buy"), Some((BlockKind::Todo, "buy".into())));
        assert_eq!(conv("[ ] buy"), Some((BlockKind::Todo, "buy".into())));
        assert_eq!(conv("> note"), Some((BlockKind::Quote, "note".into())));
        assert_eq!(conv("---"), Some((BlockKind::Divider, "".into())));
        assert_eq!(conv("```"), Some((BlockKind::Code, "".into())));
        // non-triggers
        assert_eq!(conv("#no-space"), None);
        assert_eq!(conv("then # "), None);
        assert_eq!(conv("a. x"), None);
        assert_eq!(conv(". x"), None);
        assert_eq!(conv("-"), None);
        assert_eq!(conv("----"), None);
        assert_eq!(conv(""), None);
    }

    #[test]
    fn markdown_shortcut_checked_todo_and_code_exemption() {
        // "[x] " lands checked
        let (kind, rest, checked) = markdown_convert("[x] done", Some(BlockKind::Paragraph)).unwrap();
        assert_eq!(kind, BlockKind::Todo);
        assert_eq!(rest, "done");
        assert_eq!(checked, Some(true));
        // an already-Todo block typing "[x] ": the fn still reports the
        // intent, the caller skips the toggle for same-kind blocks
        let (_, _, checked) = markdown_convert("[x] done", Some(BlockKind::Todo)).unwrap();
        assert_eq!(checked, Some(true));
        // code and divider text is exempt: "# " is legitimate content there
        assert_eq!(markdown_convert("# comment", Some(BlockKind::Code)), None);
        assert_eq!(markdown_convert("---", Some(BlockKind::Divider)), None);
    }
}

pub fn bind(ui: &AppWindow, state: &Rc<AppState>) {
    let g = ui.global::<UIState>();
    state.set_ui(ui.global::<UIState>().as_weak());
    g.set_sidebar_rows(state.sidebar_model());
    g.set_blocks(state.blocks_model());
    g.set_backlinks(state.backlinks_model());
    g.set_commands(state.commands_model());
    g.set_search_rows(state.search_model());
    g.set_menu_rows(state.menu_model());
    g.set_slash_items(state.slash_model());
    g.set_block_menu_rows(state.block_menu_model());
    g.set_version_rows(state.versions_model());
    g.set_version_diff_rows(state.version_diff_model());
    let (title, crumb) = state.open_page_info(state.open_page.get());
    g.set_page_title(title.into());
    g.set_page_breadcrumb(crumb.into());
    g.set_renderer_name(renderer_name().into());
    g.set_dark(state.dark_setting());
    g.set_lan_sharing(state.setting_flag("lan.share"));
    // The rail's memory is a desktop half-open window; a drawer that remembered
    // itself open would cover the page it is supposed to slide over.
    g.set_sidebar_open(!g.get_touch_mode() && !state.setting_flag("sidebar.closed"));
    // settings storage row (M8): the database folder, hidden for a
    // memory-only session
    g.set_data_dir(state.data_dir().unwrap_or_default().into());
    g.set_storage_available(state.data_dir().is_some());
    state.update_page_stats();
    if let Some(notice) = state.take_db_notice() {
        g.set_db_notice(notice.into());
    }
    if let Some(notice) = state.take_db_notice() {
        g.set_db_notice(notice.into());
    }
}

/// Select a find hit: route through the hit block's editing input, which
/// recreates the delegate with a pending range selection.
fn apply_find_hit(g: &UIState<'_>, s: &Rc<AppState>, bid: i32, start: usize, end: usize) {
    let text = {
        let d = s.doc.borrow();
        d.block(BlockId(bid as u64))
            .map(|b| b.text.clone())
            .unwrap_or_default()
    };
    g.set_editing_text(text.into());
    g.set_pending_sel_start(start as i32);
    g.set_pending_sel_end(end as i32);
    g.set_editing_id(-1);
    g.set_editing_id(bid);
    g.set_find_target_block(bid);
    let gen = g.get_find_sel_gen() + 1;
    g.set_find_sel_gen(gen);
}

/// Machine-readable renderer id for the A2 measurement JSON
/// ("femtovg"/"skia"/...). The display name lives in `renderer_name`.
pub fn renderer_id() -> &'static str {
    #[cfg(feature = "femtovg")]
    return "femtovg";
    #[cfg(all(not(feature = "femtovg"), feature = "femtovg-wgpu"))]
    return "femtovg-wgpu";
    #[cfg(all(
        not(feature = "femtovg"),
        not(feature = "femtovg-wgpu"),
        any(feature = "skia", feature = "skia-opengl")
    ))]
    return "skia";
    #[cfg(all(
        not(feature = "femtovg"),
        not(feature = "femtovg-wgpu"),
        not(any(feature = "skia", feature = "skia-opengl")),
        feature = "software"
    ))]
    return "software";
    #[cfg(all(
        not(feature = "femtovg"),
        not(feature = "femtovg-wgpu"),
        not(any(feature = "skia", feature = "skia-opengl")),
        not(feature = "software")
    ))]
    return "unknown";
}

fn renderer_name() -> &'static str {
    if cfg!(feature = "femtovg") {
        "FemtoVG · GL"
    } else if cfg!(feature = "femtovg-wgpu") {
        "FemtoVG · wgpu"
    } else if cfg!(feature = "skia") {
        "Skia"
    } else if cfg!(feature = "skia-opengl") {
        "Skia · GL"
    } else if cfg!(feature = "software") {
        "Software"
    } else {
        "unknown"
    }
}

pub fn wire(ui: &AppWindow, state: &Rc<AppState>) {
    let gw = ui.global::<UIState>().as_weak();
    state.set_ui(gw.clone());

    // ---- shell ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_toggle_sidebar(move || {
            let g = gw.upgrade().unwrap();
            let open = !g.get_sidebar_open();
            g.set_sidebar_open(open);
            s.record_setting("sidebar.closed", if open { "0" } else { "1" });
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_toggle_theme(move || {
            let g = gw.upgrade().unwrap();
            let dark = !g.get_dark();
            g.set_dark(dark);
            s.set_dark(dark);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_set_dark(move |dark| {
            let g = gw.upgrade().unwrap();
            g.set_dark(dark);
            s.set_dark(dark);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_move_block(move |delta| {
            let g = gw.upgrade().unwrap();
            let cur = g.get_editing_id();
            if cur <= 0 {
                return;
            }
            s.exec_on_open_page(Command::MoveBlock {
                id: BlockId(cur as u64),
                delta,
            });
            refresh_focused_text(&g, &s);
        });
    }

    // ---- grip-handle drag-reorder (Slint DragArea/DropArea) ----
    {
        ui.global::<UIState>().on_block_drag_payload(|id| {
            let mut data = slint::DataTransfer::default();
            data.set_plain_text(format!("{BLOCK_DRAG_MIME}{id}").into());
            data
        });
    }
    // ---- page-tree drag-move (SPEC §八): drop ONTO a page to nest, onto
    // the Workspace header for the top level ----
    {
        ui.global::<UIState>().on_page_drag_payload(|id| {
            let mut data = slint::DataTransfer::default();
            data.set_plain_text(format!("{PAGE_DRAG_MIME}{id}").into());
            data
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_page_drag_hover(move |data, index| {
            let Some(id) = page_drag_id(&data) else { return false };
            s.page_drop_target_valid(id, index)
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_page_dropped(move |data, index| {
            let Some(id) = page_drag_id(&data) else { return };
            s.page_dropped(id, index);
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_block_drag_hover(move |data, index, below| {
            let Some(id) = block_drag_id(&data) else { return false };
            // No drop line on a locked page: the gesture says for itself that
            // there is nowhere to put the block. The notice is idempotent, so
            // saying it once per drag frame is one message on screen.
            if s.page_locked() {
                s.note_locked();
                return false;
            }
            // the delegate reports a row index; the command counts model
            // positions, and a folded subtree makes those differ
            let Some(target) = s.drop_index_for_row(index, below) else { return false };
            s.can_move_block_to(id, target)
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_block_dropped(move |data, index, below| {
            let Some(id) = block_drag_id(&data) else { return };
            let Some(target) = s.drop_index_for_row(index, below) else { return };
            let _ = s.exec_on_open_page(Command::MoveBlockTo {
                id: BlockId(id as u64),
                index: target,
            });
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_settings_open_requested(move || {
            let g = gw.upgrade().unwrap();
            // the sync section reads the peers table: rebuild its rows on
            // every open, so a device found while the dialog was shut is
            // there when the user looks
            refresh_sync_ui(&g, &s);
            g.set_settings_open(true);
        });
    }

    {
        let gw = gw.clone();
        ui.global::<UIState>().on_settings_close_requested(move || {
            let g = gw.upgrade().unwrap();
            g.set_settings_open(false);
        });
    }

    // LAN sync: the engine's threads and the pump that answers them here
    start_sync(ui, state);

    // ---- page tree ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_node_clicked(move |id| {
            let g = gw.upgrade().unwrap();
            if id == ROW_NEW_PAGE {
                let new_id = s.create_page(None);
                open(&g, &s, new_id);
                g.set_renaming_id(new_id);
                return;
            }
            // favorites/recents/page rows all resolve to a real page
            open(&g, &s, id);
        });
    }

    {
        let s = state.clone();
        ui.global::<UIState>().on_node_expand_toggled(move |id| {
            s.toggle_expanded(id);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_node_context(move |id| {
            let g = gw.upgrade().unwrap();
            s.fill_menu(id);
            g.set_menu_node_id(id);
            // anchor at the row's right edge; MenuRow y comes from the
            // sidebar projection, adjusted by the tree scroll position.
            let row_y = s.sidebar_row_y(id) as f32;
            let y = (TREE_TOP_PX + g.get_tree_viewport_y() + row_y - 4.0)
                .max(48.0)
                .min(g.get_window_h() - 190.0);
            g.set_menu_y(y);
            g.set_menu_x(240.0);
            g.set_menu_open(true);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_menu_action(move |action| {
            let g = gw.upgrade().unwrap();
            let id = g.get_menu_node_id();
            // submenu navigation swaps the rows and keeps the popup open;
            // the taller Move-to list re-anchors so it stays on the window
            if action == crate::app::state::MENU_MOVE_TO
                || action == crate::app::state::MENU_PAGE_STYLE
            {
                if action == crate::app::state::MENU_MOVE_TO {
                    s.fill_page_menu_move_to(id);
                } else {
                    s.fill_page_menu_style(id);
                }
                reanchor_page_menu(&g);
                return;
            }
            if action == crate::app::state::MENU_BACK {
                s.fill_menu(id);
                return;
            }
            // ---- Templates (SPEC §三十八) ----
            // Two of these swap the popup's rows instead of closing it, for the
            // same reason Move-to does: the list they install is taller than
            // the menu it replaces, so the anchor has to be re-measured or the
            // bottom rows land off-window. The third group *is* the row that
            // was clicked in the picker, and it closes.
            //
            // What is deliberately *not* here is a fourth door: the ⋮⋮ block
            // menu does not offer templates. That menu is about the block it
            // was opened on, and a template is not a property of a block — the
            // slash popup already inserts at that exact row, so the extra door
            // would only add a picker to maintain.
            if action == crate::app::state::MENU_PAGE_TEMPLATES {
                s.fill_template_menu();
                reanchor_page_menu(&g);
                return;
            }
            if action == crate::app::state::MENU_TEMPLATE_PICK_BACK {
                s.fill_template_menu();
                reanchor_page_menu(&g);
                return;
            }
            if matches!(
                action,
                crate::app::state::MENU_TEMPLATE_INSERT
                    | crate::app::state::MENU_TEMPLATE_NEW_PAGE
                    | crate::app::state::MENU_TEMPLATE_EXPORT
                    | crate::app::state::MENU_TEMPLATE_DELETE
            ) {
                // The picker has nothing to show and the row would be a popup
                // with one Back button. Saving and importing are the two rows
                // that answer for an empty library instead.
                if s.template_list().is_empty() {
                    g.set_menu_open(false);
                    g.set_db_notice("还没有模板 — 先把此页面保存为模板。".into());
                    return;
                }
                s.fill_template_pick(action);
                reanchor_page_menu(&g);
                return;
            }
            g.set_menu_open(false);
            g.set_menu_node_id(-1);
            match action {
                crate::app::state::MENU_NEW_SUBPAGE => {
                    let new_id = s.create_page(Some(id));
                    open(&g, &s, new_id);
                    g.set_renaming_id(new_id);
                }
                crate::app::state::MENU_RENAME => {
                    g.set_renaming_id(id);
                }
                crate::app::state::MENU_DUPLICATE => {
                    if let Some(new_id) = s.duplicate_page(id) {
                        open(&g, &s, new_id);
                    }
                }
                crate::app::state::MENU_MOVE_UP => {
                    if id > 0 {
                        s.move_page_by(id, -1);
                    }
                }
                crate::app::state::MENU_MOVE_DOWN => {
                    if id > 0 {
                        s.move_page_by(id, 1);
                    }
                }
                crate::app::state::PAGE_MOVE_TO_ROOT => {
                    if id > 0 {
                        s.move_page(id, None);
                    }
                }
                a if (crate::app::state::PAGE_MOVE_TO_BASE
                    ..crate::app::state::PAGE_MOVE_TO_BASE + 1_000_000)
                    .contains(&a) =>
                {
                    if id > 0 {
                        s.move_page(id, Some(a - crate::app::state::PAGE_MOVE_TO_BASE));
                    }
                }
                crate::app::state::MENU_FAVORITE => s.toggle_favorite(id),
                a if (crate::app::state::PAGE_FONT_BASE
                    ..crate::app::state::PAGE_FONT_BASE
                        + crate::core::PageFont::ALL.len() as i32)
                    .contains(&a) =>
                {
                    if id > 0 {
                        let font = crate::core::PageFont::ALL[(a
                            - crate::app::state::PAGE_FONT_BASE)
                            as usize];
                        s.set_page_font(id, font);
                    }
                }
                crate::app::state::MENU_PAGE_FULL_WIDTH => {
                    if id > 0 {
                        s.toggle_page_full_width(id);
                    }
                }
                crate::app::state::MENU_PAGE_SMALL_TEXT => {
                    if id > 0 {
                        s.toggle_page_small_text(id);
                    }
                }
                // The emoji grid replaces this menu rather than nesting under
                // it: it is the taller of the two, and it is anchored at the
                // same place, so keeping the menu open would put one popup
                // behind the other.
                crate::app::state::MENU_PAGE_ICON => {
                    if id > 0 {
                        s.fill_icon_picker(id);
                        // the grid is 8 cells wide and 13 rows tall; a short
                        // window clips it, and the popup scrolls the rest
                        g.set_icon_picker_x(
                            g.get_menu_x()
                                .min((g.get_window_w() - 256.0).max(8.0)),
                        );
                        g.set_icon_picker_y(
                            g.get_menu_y()
                                .max(48.0)
                                .min((g.get_window_h() - 160.0).max(48.0)),
                        );
                        g.set_icon_picker_open(true);
                    }
                }
                crate::app::state::MENU_PAGE_COVER => {
                    if id > 0 {
                        pick_cover(&g, &s, id);
                    }
                }
                crate::app::state::MENU_PAGE_COVER_REMOVE => {
                    if id > 0 {
                        s.set_page_cover(id, None);
                    }
                }
                crate::app::state::MENU_PAGE_LOCK => {
                    if id > 0 {
                        // The keystroke that is still in flight commits first:
                        // what the user typed before the door closed stands,
                        // and nothing after it is swallowed by the switch
                        // itself. Then the live input and the title's editor
                        // are both taken away, so the row cannot disagree with
                        // the page it sits on.
                        flush_pending_edit(&g, &s);
                        g.set_editing_id(-1);
                        g.set_title_editing(false);
                        let now = !s.workspace.borrow().locked_of(id);
                        s.set_page_locked(id, now);
                    }
                }
                crate::app::state::MENU_PAGE_VERSIONS => {
                    if id > 0 {
                        open_version_panel(&g, &s, id);
                    }
                }
                crate::app::state::MENU_TEMPLATE_SAVE => {
                    if id > 0 {
                        // The name is the page's title, so the notice is what
                        // tells the user a *copy* was made rather than the page
                        // being marked somehow — and where to find it.
                        let name = s
                            .workspace
                            .borrow()
                            .title_of(id)
                            .unwrap_or("未命名模板")
                            .to_string();
                        s.save_as_template(id);
                        // `…`, not `⋯` — see `AppState::note_locked`'s line.
                        g.set_db_notice(
                            format!("已将“{name}”保存为模板 — … → 模板。").into(),
                        );
                    }
                }
                crate::app::state::MENU_TEMPLATE_IMPORT => {
                    import_template_dialog(&g, &s);
                }
                a if (crate::app::state::TEMPLATE_PICK_BASE
                    ..crate::app::state::TEMPLATE_PICK_BASE + 1_000_000)
                    .contains(&a) =>
                {
                    let Some((template, name)) = s.template_picked(a) else {
                        return;
                    };
                    // The picker doesn't say what it was opened *for* — the row
                    // id only names the template — so `template_pick` carries
                    // the action across the row swap.
                    match s.template_pick_action() {
                        crate::app::state::MENU_TEMPLATE_INSERT => {
                            // The ⋯ menu can be opened on a sidebar row that is
                            // not the page on screen, and a template writes
                            // *blocks*, which only ever land on the open page.
                            // Saying so beats filling a page three rows away
                            // from the one the user clicked.
                            if id != s.open_page.get() {
                                g.set_db_notice(
                                    "请先打开该页面 — 模板会插入到当前显示的页面。"
                                        .into(),
                                );
                            } else if let Some(first) = s.insert_template(None, template) {
                                // The caret follows the copy. Without this the
                                // page grew and the selection stayed wherever
                                // it was, which reads as "nothing happened" on
                                // a long page.
                                focus_block(&g, &s, first, i32::MAX);
                            } else if !s.page_locked() {
                                // A locked page already put its own line up
                                // (ADR-0048), and overwriting it with a vaguer
                                // one would hide the real reason.
                                g.set_db_notice(
                                    format!("“{name}”没有可插入的块。").into(),
                                );
                            }
                        }
                        crate::app::state::MENU_TEMPLATE_NEW_PAGE => {
                            // A tree operation, so it follows the menu's page
                            // rather than the screen: the subpage button two
                            // rows above does the same thing with no template.
                            let parent = if id > 0 { Some(id) } else { None };
                            let new_id = s.new_page_from_template(parent, template);
                            open(&g, &s, new_id);
                        }
                        crate::app::state::MENU_TEMPLATE_EXPORT => {
                            export_template_markdown(&g, &s, template, &name);
                        }
                        crate::app::state::MENU_TEMPLATE_DELETE => {
                            // No confirm dialog, unlike Delete page: this is the
                            // second deliberate click in a row the user chose
                            // from a list of their own names, and the row is
                            // drawn in the danger colour to say so. What it must
                            // not be is silent, so the line names the loss.
                            s.delete_page(template);
                            g.set_db_notice(format!("已删除“{name}”模板。").into());
                        }
                        _ => {}
                    }
                }
                crate::app::state::MENU_DELETE => {
                    let (_title, message) = s.delete_dialog_text(id);
                    g.set_dialog_title("删除页面？".into());
                    g.set_dialog_message(message.into());
                    g.set_dialog_open(true);
                }
                _ => {}
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_icon_picked(move |glyph| {
            let g = gw.upgrade().unwrap();
            let id = g.get_icon_picker_page();
            // the pick closes the grid either way; "" is its "None" cell, and
            // picking it on a page with no icon is a write of the empty string
            // the storage layer already treats as "unset"
            g.set_icon_picker_open(false);
            g.set_icon_picker_page(-1);
            if id > 0 {
                s.set_page_icon(id, glyph.as_str());
            }
        });
    }

    // ---- version history (SPEC §三十八, ADR-0091) ----
    //
    // Six callbacks for one panel, and the panel never closes on its own: a
    // save, a delete or a restore all leave the user looking at the list they
    // are working through, because the work is "find the version I want" and
    // throwing the panel away at each step would make them reopen it and
    // re-find it. The notice bar carries what happened instead.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_version_save(move || {
            let g = gw.upgrade().unwrap();
            let page = g.get_versions_page();
            let label = g.get_version_name();
            match s.save_page_version(page, label.as_str()) {
                Ok(name) => {
                    // The field empties either way: the name is spent on the
                    // version now, and a copy left in it would be saved twice.
                    g.set_version_name("".into());
                    g.set_versions_showing_diff(false);
                    g.set_versions_selected(-1);
                    s.fill_versions(page);
                    g.set_db_notice(
                        format!("已保存“{name}” — 现在可以在此比较或恢复。").into(),
                    );
                }
                Err(e) => g.set_db_notice(format!("版本未能保存：{e}").into()),
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_version_view(move |row| {
            let g = gw.upgrade().unwrap();
            let page = g.get_versions_page();
            let Some((created, label)) = s.version_at(page, row) else {
                return;
            };
            let lines = match s.version_diff(page, created) {
                Ok(lines) => lines,
                Err(e) => {
                    g.set_db_notice(format!("无法读取该版本：{e}").into());
                    return;
                }
            };
            s.fill_version_diff(
                &lines,
                &crate::app::state::version_heading(&label, created),
                &crate::app::state::version_diff_note(&lines),
            );
            g.set_versions_selected(row);
            g.set_versions_showing_diff(true);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_version_restore(move |row| {
            let g = gw.upgrade().unwrap();
            let page = g.get_versions_page();
            let Some((created, _label)) = s.version_at(page, row) else {
                return;
            };
            match s.restore_version(page, created) {
                Ok(count) => {
                    // Back to the list, whose rows the restore just made
                    // truthful: the comparison the user was looking at is now
                    // of the page with itself.
                    g.set_versions_showing_diff(false);
                    g.set_versions_selected(-1);
                    s.fill_versions(page);
                    g.set_db_notice(format!(
                        "已从该版本恢复 {count} 行 — Ctrl+Z 可以恢复它所替换的内容。"
                    ).into());
                }
                // A locked page put its own line up inside the refusal, and a
                // vaguer one here would overwrite the only wording that says
                // what to do about it (ADR-0048).
                Err(e) if !s.page_locked() => {
                    g.set_db_notice(format!("版本未能恢复：{e}").into())
                }
                Err(_) => {}
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_version_delete(move |row| {
            let g = gw.upgrade().unwrap();
            let page = g.get_versions_page();
            let Some((created, label)) = s.version_at(page, row) else {
                return;
            };
            if let Err(e) = s.delete_version(page, created) {
                g.set_db_notice(format!("版本未能删除：{e}").into());
                return;
            }
            // No confirm dialog, for the reason the template delete has: this is
            // the second deliberate click on a row the user named themselves,
            // drawn in the danger colour to say what it does. What it must not
            // be is silent, so the line names the version that is gone — and a
            // comparison of the row just deleted has nothing left to compare.
            g.set_versions_showing_diff(false);
            g.set_versions_selected(-1);
            s.fill_versions(page);
            g.set_db_notice(format!("已删除版本“{label}”。页面本身未变。").into());
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_version_back(move || {
            let g = gw.upgrade().unwrap();
            let page = g.get_versions_page();
            g.set_versions_showing_diff(false);
            g.set_versions_selected(-1);
            s.fill_versions(page);
        });
    }

    {
        let gw = gw.clone();
        ui.global::<UIState>().on_versions_close(move || {
            let g = gw.upgrade().unwrap();
            g.set_versions_open(false);
            g.set_versions_page(-1);
            g.set_versions_selected(-1);
            g.set_versions_showing_diff(false);
            g.set_version_name("".into());
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_page_menu_requested(move || {
            let g = gw.upgrade().unwrap();
            let id = g.get_sidebar_selected_id();
            if !s.workspace.borrow().contains(id) {
                return;
            }
            s.fill_menu(id);
            g.set_menu_node_id(id);
            // under the ⋯ button, top-right of the window
            g.set_menu_x(g.get_window_w() - 224.0);
            g.set_menu_y(44.0);
            g.set_menu_open(true);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_page_create_requested(move || {
            let g = gw.upgrade().unwrap();
            let new_id = s.create_page(None);
            open(&g, &s, new_id);
            g.set_renaming_id(new_id);
        });
    }

    // ---- rename ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_rename_committed(move |id, text| {
            let g = gw.upgrade().unwrap();
            g.set_renaming_id(-1);
            s.rename_page(id, &text);
            if s.open_page.get() == id {
                let (title, crumb) = s.open_page_info(id);
                g.set_page_title(title.into());
                g.set_page_breadcrumb(crumb.into());
            }
        });
    }

    {
        let gw = gw.clone();
        ui.global::<UIState>().on_rename_cancelled(move || {
            let g = gw.upgrade().unwrap();
            g.set_renaming_id(-1);
        });
    }

    // ---- delete dialog ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_dialog_confirmed(move || {
            let g = gw.upgrade().unwrap();
            g.set_dialog_open(false);
            if let Some(id) = s.pending_delete.take() {
                let was_open = s.delete_page(id);
                if was_open {
                    let fallback = s.workspace.borrow().first_root();
                    match fallback {
                        Some(fid) => open(&g, &s, fid),
                        None => {
                            g.set_page_title("Quire".into());
                            g.set_page_breadcrumb("".into());
                            s.blocks.set_vec(Vec::new());
                            g.set_sidebar_selected_id(0);
                        }
                    }
                }
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_dialog_cancelled(move || {
            let g = gw.upgrade().unwrap();
            g.set_dialog_open(false);
            s.pending_delete.set(None);
        });
    }

    // ---- command palette ----
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_palette_open_requested(move || {
            let g = gw.upgrade().unwrap();
            g.set_palette_focus(0);
            g.set_command_selected_id(-1);
            g.set_palette_open(true);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_palette_query_changed(move || {
            let g = gw.upgrade().unwrap();
            let q = g.get_palette_query().to_string();
            s.set_query(&q);
            g.set_palette_focus(0);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_command_invoked(move || {
            let g = gw.upgrade().unwrap();
            let mut id = g.get_command_selected_id();
            if id < 0 {
                // keyboard Enter: resolve by focus index into the filtered model
                let focus = g.get_palette_focus() as usize;
                id = s.commands.iter().nth(focus).map_or(-1, |c| c.id);
            }
            g.set_palette_open(false);
            g.set_palette_query("".into());
            s.set_query("");
            g.set_palette_focus(0);
            // One id -> action mapping in state.rs, matched here with no
            // wildcard arm: this dispatch used to be `match id` over literals
            // plus constants, and a constant whose import was missing turns
            // into a catch-all *binding* rather than a pattern — that shadowed
            // every command with id >= 9 for one round (`aaa3763`). An enum
            // path cannot bind, and a variant with no arm here no longer
            // compiles.
            match palette_action(id) {
                PaletteAction::NewPage => {
                    let new_id = s.create_page(None);
                    open(&g, &s, new_id);
                    g.set_renaming_id(new_id);
                }
                PaletteAction::SearchPages => {
                    g.set_search_open(true);
                }
                PaletteAction::ToggleSidebar => g.set_sidebar_open(!g.get_sidebar_open()),
                PaletteAction::ToggleTheme => g.set_dark(!g.get_dark()),
                PaletteAction::Settings => g.set_settings_open(true),
                PaletteAction::RenamePage => {
                    g.set_renaming_id(g.get_sidebar_selected_id());
                }
                PaletteAction::DuplicatePage => {
                    let cur = s.open_page.get();
                    if let Some(new_id) = s.duplicate_page(cur) {
                        open(&g, &s, new_id);
                    }
                }
                PaletteAction::DeletePage => {
                    let cur = s.open_page.get();
                    if s.workspace.borrow().contains(cur) {
                        let (_t, message) = s.delete_dialog_text(cur);
                        g.set_dialog_title("删除页面？".into());
                        g.set_dialog_message(message.into());
                        g.set_dialog_open(true);
                    }
                }
                PaletteAction::ExportMarkdown => export_current_page(&g, &s),
                PaletteAction::ImportMarkdown => import_markdown_dialog(&g, &s),
                PaletteAction::CopyMarkdown => copy_current_page_markdown(&g, &s),
                PaletteAction::NavigateBack => navigate(&g, &s, false),
                PaletteAction::NavigateForward => navigate(&g, &s, true),
                PaletteAction::OpenPage(page) => open(&g, &s, page),
                PaletteAction::None => {}
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_palette_move(move |delta| {
            let g = gw.upgrade().unwrap();
            let count = s.commands.row_count() as i32;
            if count == 0 {
                g.set_palette_focus(0);
                return;
            }
            let next = (g.get_palette_focus() + delta).clamp(0, count - 1);
            g.set_palette_focus(next);
        });
    }

    // ---- search panel ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_search_open_requested(move || {
            let g = gw.upgrade().unwrap();
            g.set_search_query("".into());
            s.set_search_query("");
            g.set_search_focus(0);
            g.set_search_open(true);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_search_query_changed(move || {
            let g = gw.upgrade().unwrap();
            let q = g.get_search_query().to_string();
            s.set_search_query(&q);
            g.set_search_focus(0);
            poll_arm(&gw, &s);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_search_move(move |delta| {
            let g = gw.upgrade().unwrap();
            let count = s.search.row_count() as i32;
            if count == 0 {
                g.set_search_focus(0);
                return;
            }
            let next = (g.get_search_focus() + delta).clamp(0, count - 1);
            g.set_search_focus(next);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_search_open_hit(move |page, block| {
            let g = gw.upgrade().unwrap();
            if page <= 0 || block <= 0 {
                return;
            }
            g.set_search_open(false);
            g.set_search_query("".into());
            open(&g, &s, page);
            // open the hit block in edit mode with its text selected — the
            // same mechanism the find bar uses for its navigation
            let (text, len) = {
                let d = s.doc.borrow();
                d.block(crate::core::BlockId(block as u64))
                    .map(|b| (b.text.clone(), b.text.len()))
                    .unwrap_or_default()
            };
            if len > 0 {
                g.set_editing_text(text.into());
                g.set_pending_sel_start(0);
                g.set_pending_sel_end(len as i32);
                g.set_editing_id(-1);
                g.set_editing_id(block);
                let gen = g.get_find_sel_gen() + 1;
                g.set_find_sel_gen(gen);
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_search_invoked(move || {
            let g = gw.upgrade().unwrap();
            let id = search_target(&g, &s);
            g.set_search_open(false);
            g.set_search_query("".into());
            open(&g, &s, id);
        });
    }

    // ---- blocks ----
    {
        let s = state.clone();
        ui.global::<UIState>().on_todo_toggled(move |id| {
            if id <= 0 {
                return;
            }
            // A check box is an edit, and this one path calls the command
            // layer directly so it can update a single row — so the lock is
            // asked here as well as at the funnel.
            if s.page_locked() {
                s.note_locked();
                return;
            }
            // route through the command layer; targeted row update only
            let changes = crate::core::command::exec(
                &mut s.doc.borrow_mut(),
                &mut s.history.borrow_mut(),
                core_page_id(s.open_page.get()),
                Command::ToggleTodoChecked {
                    id: BlockId(id as u64),
                },
            );
            if changes.is_some() {
                let checked = s
                    .doc
                    .borrow()
                    .block(BlockId(id as u64))
                    .map(|b| b.checked)
                    .unwrap_or(false);
                let mut i = 0;
                while let Some(mut row) = s.blocks.row_data(i) {
                    if row.id == id {
                        row.checked = checked;
                        s.blocks.set_row_data(i, row);
                        return;
                    }
                    i += 1;
                }
            }
        });
    }

    // toggle block (SPEC §三十七): the chevron hides/shows the subtree. The
    // rows themselves come and go, so this takes the full reproject that
    // `exec_on_open_page` does — unlike the todo patch above. The pending
    // typing commits first: the block losing its row must not eat it.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_toggle_fold(move |id| {
            if id <= 0 {
                return;
            }
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            s.exec_on_open_page(Command::ToggleFold {
                id: BlockId(id as u64),
            });
        });
    }

    // picture block (SPEC §三十七 批次 A): the row asks for its raster by
    // attachments id. Both callbacks run during layout of a realized row, so
    // a page with five hundred pictures decodes the handful on screen.
    {
        let s = state.clone();
        ui.global::<UIState>().on_image_for(move |id| s.image_for(id));
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_image_aspect(move |id| s.image_aspect(id));
    }

    // file block (SPEC §三十七 批次 A): the size label is a lookup, the two
    // buttons hand the stored bytes to the system.
    {
        let s = state.clone();
        ui.global::<UIState>()
            .on_attachment_size(move |id| s.attachment_size(id).into());
    }
    // math block (SPEC §三十七 批次 C): no state to consult — the source is in
    // the row, and the conversion is a function of it.
    ui.global::<UIState>()
        .on_math_render(move |src| crate::core::math::to_unicode(&src).into());
    // embed card (SPEC §三十七 批次 C): the same shape as the math row — the
    // address is the block's own text, and both lines the card shows are
    // functions of it. Nothing here looks anything up.
    ui.global::<UIState>()
        .on_embed_label(move |url| crate::core::embed::describe(&url).into());
    ui.global::<UIState>()
        .on_embed_url(move |url| crate::core::embed::with_scheme(&url).into());
    // code highlight (SPEC §三十七 批次 C): the same shape as the two above —
    // one pure function of what the row already holds, no state to consult. The
    // row asks for each colour of its block separately; the lexing is done once
    // per call by Slint's own cache of a pure callback.
    ui.global::<UIState>().on_code_layer(move |text, lang, frame, advance, kind| {
        crate::core::highlight::layer(
            &text,
            Lang::try_from_str(&lang).unwrap_or(Lang::Plain),
            frame,
            advance,
            crate::core::highlight::Kind::from_int(kind),
        )
        .into()
    });
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_attachment_opened(move |id| {
            let g = gw.upgrade().unwrap();
            attachment_action(&g, &s, id, false);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_attachment_saved(move |id| {
            let g = gw.upgrade().unwrap();
            attachment_action(&g, &s, id, true);
        });
    }

    // ---- block editing (M4) ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_block_activate(move |id| {
            let g = gw.upgrade().unwrap();
            if id <= 0 {
                return;
            }
            flush_pending_edit(&g, &s);
            // a Page block is open-only: activating it opens the child page
            // (the row is never editable, so this is its whole interaction)
            if let Some(child) = s.block_page_ref(id) {
                open(&g, &s, child);
                return;
            }
            // A locked page answers the click and stops there (SPEC §三十八).
            // The row above this one still navigates, because opening the page
            // a link names is reading, not editing.
            if s.page_locked() {
                s.note_locked();
                return;
            }
            // SPEC §四十 / ADR-0052: clicking a mirror puts the caret *in the
            // source*, which is the whole of "edit either copy and both change".
            // The row's own delegate then lights up too, because it asks
            // `editing-id == content-id` — one block being edited, two rows in
            // agreement about whose words they are showing.
            //
            // An unresolvable mirror (`content_of` returning `-1`) falls
            // through to itself, and `set_editing_id(-1)` there is a no-op that
            // leaves the placeholder untouched: there is nothing to type into.
            let target = s.content_of(id);
            if target <= 0 {
                return;
            }
            let (text, len) = {
                let d = s.doc.borrow();
                d.block(BlockId(target as u64))
                    .map(|b| (b.text.clone(), b.text.len()))
                    .unwrap_or_default()
            };
            g.set_editing_text(text.into());
            g.set_pending_caret(len as i32);
            g.set_editing_id(target);
        });
    }

    // toc (SPEC §三十七 批次 C): a contents line names a block on this page,
    // so the click walks the same path a `quire://block/` link walks — minus
    // the url parse and the page open, since the list is built from the page
    // that is already showing.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_toc_jump(move |id| {
            let g = gw.upgrade().unwrap();
            if id <= 0 {
                return;
            }
            let len = {
                let d = s.doc.borrow();
                d.block(BlockId(id as u64))
                    .map(|b| b.text.len() as i32)
                    .unwrap_or(0)
            };
            flush_pending_edit(&g, &s);
            // -1 first: recreate the delegate so the input takes over
            g.set_editing_id(-1);
            focus_block(&g, &s, id, len);
        });
    }

    // backlink panel (SPEC §四十): a row names the block that mentioned this
    // page, and the row's click is the same jump a `quire://block/` link in
    // the text makes — including when the reference lives on *another* page,
    // which is the common case and the reason the panel cannot just move the
    // caret where it stands.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_backlink_jump(move |id| {
            let g = gw.upgrade().unwrap();
            if id <= 0 {
                return;
            }
            jump_to_block(&g, &s, id as u64);
        });
    }

    {
        let s = state.clone();
        ui.global::<UIState>().on_backlink_toggle(move || {
            // the window is chosen from the flag, so the fold *is* the read:
            // nothing else has to be re-derived for the panel to change size
            s.toggle_backlinks();
        });
    }

    // rich paste (SPEC §二十七): the clipboard's markdown structure lands
    // as blocks. The clipboard read is direct Win32 FFI (microseconds — a
    // Get-Clipboard subprocess measured 7-10 s on the dev desktop); a false
    // return lets the key fall through to the native plain paste.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_rich_paste(move |id| -> bool {
            let g = gw.upgrade().unwrap();
            if id <= 0 {
                return false;
            }
            flush_pending_edit(&g, &s);
            let clip = crate::platform::read_clipboard();
            let Some(text) = clip else {
                // No text on the clipboard at all, so the other thing worth
                // pasting gets its turn: a screenshot. When a copy carries
                // both, the words win — a rich app's text must not be turned
                // into a picture of itself.
                return match crate::platform::read_clipboard_image() {
                    None => false,
                    Some(png) => {
                        if s.paste_image(id, &png) {
                            g.set_editing_id(-1);
                            true
                        } else {
                            g.set_db_notice("无法保存剪贴板中的图片。".into());
                            false
                        }
                    }
                };
            };
            let Some(parsed) =
                crate::services::import_service::parse_if_block_structure(&text)
            else {
                return false;
            };
            if s.paste_block_structure(id, &parsed) {
                // the row's content changed under the input; end editing so
                // the rendered row (and its marks) take over
                g.set_editing_id(-1);
                true
            } else {
                false
            }
        });
    }

    // persistence: arm the flush timer hook; every recorded batch restarts
    // it, so the write lands once the user has been quiet for 600 ms
    {
        let t: &'static slint::Timer = Box::leak(Box::new(slint::Timer::default()));
        let sw = std::rc::Rc::downgrade(&state);
        state.install_flush_hook(Box::new(move || {
            let sw = sw.clone();
            t.start(
                slint::TimerMode::SingleShot,
                std::time::Duration::from_millis(600),
                move || {
                    if let Some(s) = sw.upgrade() {
                        s.persistence_force_flush();
                    }
                },
            );
        }));
    }

    {
        let s = state.clone();
        ui.global::<UIState>().on_save_requested(move || {
            s.persistence_force_flush();
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_nav_back_requested(move || {
            let g = gw.upgrade().unwrap();
            navigate(&g, &s, false);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_nav_forward_requested(move || {
            let g = gw.upgrade().unwrap();
            navigate(&g, &s, true);
        });
    }

    // settings storage row (M8): open the database folder / snapshot now
    {
        let s = state.clone();
        ui.global::<UIState>().on_open_data_folder(move || {
            if let Some(dir) = s.data_dir() {
                crate::platform::open_folder(&dir);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_backup_now(move || {
            let g = gw.upgrade().unwrap();
            let text = match s.backup_now() {
                Ok(msg) => msg,
                Err(e) => format!("备份失败：{e}"),
            };
            g.set_db_notice(text.into());
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_reclaim_attachments(move || {
            let g = gw.upgrade().unwrap();
            let text = match s.reclaim_attachments() {
                Ok(msg) => msg,
                Err(e) => format!("清理失败：{e}"),
            };
            g.set_db_notice(text.into());
        });
    }

    // debounce the typing commit: each keystroke restarts the timer; the
    // Timer must outlive this scope (leaked, like the bench timers). The table
    // cells arm the same timer — they skip everything above it, because a cell
    // is not a line of prose: no markdown shortcut converts in a grid, and the
    // slash menu has no room to open in one.
    let t: &'static slint::Timer = Box::leak(Box::new(slint::Timer::default()));
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_editing_changed(move |row_y, row_h, content_x| {
                let gw = gw.clone();
                let s = s.clone();
                if let Some(g) = gw.upgrade() {
                    let text = g.get_editing_text().to_string();
                    // markdown line-shortcuts convert before anything else
                    // looks at the text (slash filter included); the block
                    // kind gates which triggers are eligible
                    let editing = g.get_editing_id();
                    if editing > 0 {
                        let current = s.block_kind(editing);
                        if let Some((kind, cleaned, checked)) =
                            markdown_convert(&text, current)
                        {
                            let bid = BlockId(editing as u64);
                            let mut cmds = vec![
                                Command::ReplaceText { id: bid, text: cleaned.clone() },
                                Command::SetBlockType { id: bid, kind },
                            ];
                            // "[x] " lands checked — unless it already is one
                            if checked == Some(true) && current != Some(crate::core::BlockKind::Todo) {
                                cmds.push(Command::ToggleTodoChecked { id: bid });
                            }
                            let _ = s.exec_all_on_open_page(cmds);
                            g.set_slash_open(false);
                            g.set_editing_text(cleaned.into());
                            g.set_pending_caret(0);
                            g.set_editing_id(editing);
                            return;
                        }
                    }
                    // "/" at block start opens the slash menu with the rest of
                    // the line as the filter (SPEC §十五); anchored below the
                    // editing block. The "+"-handle insert menu filters on
                    // the whole line instead and keeps its original anchor;
                    // the page picker (Link to page) filters the same way.
                    // "@" opens the mention picker wherever it is typed (SPEC
                    // §四十) — mid-line included, which is why it is checked by
                    // scanning the text rather than by looking at the head.
                    let pick_mode = g.get_slash_pick_page() && g.get_slash_open();
                    let linkdb_mode = g.get_slash_pick_linkdb() && g.get_slash_open();
                    let insert_mode = g.get_slash_insert() && g.get_slash_open();
                    let mention_mode = g.get_slash_pick_mention() && g.get_slash_open();
                    let mention_at = crate::core::trigger_at(&text);
                    if mention_mode && mention_at.is_some() {
                        // still typing the filter: refilter, keep the anchor
                        let at = mention_at.unwrap();
                        s.open_slash_mention(&text[at + 1..]);
                        g.set_slash_filter(text[at + 1..].into());
                        g.set_slash_focus(0);
                    } else if pick_mode {
                        s.open_slash_pick(&text);
                        g.set_slash_filter(text.into());
                        g.set_slash_focus(0);
                    } else if linkdb_mode {
                        // D7 (ADR-0085): still picking a database to link —
                        // refilter the picker's rows, keep the anchor
                        s.open_slash_links(&text);
                        g.set_slash_filter(text.into());
                        g.set_slash_focus(0);
                    } else if insert_mode {
                        s.open_slash_insert(&text);
                        g.set_slash_filter(text.into());
                        g.set_slash_focus(0);
                    } else if let Some(at) = mention_at {
                        // the menu opens where the text column starts, so it
                        // never runs off the left edge on a narrow window
                        s.open_slash_mention(&text[at + 1..]);
                        g.set_slash_filter(text[at + 1..].into());
                        g.set_slash_focus(0);
                        open_slash_at(&g, row_y, row_h, content_x);
                        g.set_slash_pick_mention(true);
                        g.set_slash_insert(false);
                        g.set_slash_pick_page(false);
                    } else if let Some(filter) = text.strip_prefix('/') {
                        s.open_slash(filter);
                        g.set_slash_filter(filter.into());
                        g.set_slash_focus(0);
                        open_slash_at(&g, row_y as f32, row_h as f32, content_x as f32);
                        g.set_slash_pick_mention(false);
                    } else {
                        g.set_slash_open(false);
                        g.set_slash_pick_mention(false);
                    }
                }
                debounce_arm(t, &gw, &s);
            });
    }

    // ---- table grid (SPEC §三十七 批次 B) ----
    // The cells are child blocks the projection hides, so everything here
    // goes through `editing-id`: the row model carries them, Rust reads the
    // focused cell back out of the UI state, and the grid's own text never
    // touches `editing-changed` (a cell has no line shortcuts to run).
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_table_cell_move(move |cell, delta| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            // Tab off the last cell appends a row first; Shift-Tab off the
            // front stays put, which is what `None` means
            if let Some(next) = s.table_step(cell, delta) {
                focus_block(&g, &s, next, i32::MAX);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_cell_changed(move || {
            debounce_arm(t, &gw, &s);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_table_row_added(move |table| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            s.table_add_row(table, g.get_editing_id());
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_table_column_added(move |table| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            s.table_add_column(table, g.get_editing_id());
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_table_row_removed(move |table| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            let cell = g.get_editing_id();
            let at = s.table_cell_index(cell);
            if s.table_delete_row(table, cell) {
                table_refocus(&g, &s, table, at);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_table_column_removed(move |table| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            let cell = g.get_editing_id();
            let at = s.table_cell_index(cell);
            if s.table_delete_column(table, cell) {
                table_refocus(&g, &s, table, at);
            }
        });
    }

    // ---- columns layout (SPEC §三十七 批次 B) ----
    // The boxes and their blocks are child blocks the projection hides, so —
    // exactly as for a grid — everything here goes through `editing-id`, and
    // the layout's row never sees `editing-changed` for them.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_column_item_move(move |item, delta| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            // either end of the layout stops: a box is left by clicking out,
            // not by tabbing past the edge
            if let Some(next) = s.column_step(item, delta) {
                focus_block(&g, &s, next, i32::MAX);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_column_added(move |layout| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            if s.column_add(layout) {
                columns_refocus(&g, &s);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_column_removed(move |layout| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            // the last box's blocks move into the one before it, so the
            // focused block survives its own box going away
            if s.column_remove(layout) {
                columns_refocus(&g, &s);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_column_fill(move |column| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            // the empty box just got a line, and the caret should be on it —
            // otherwise the click looks like it did nothing
            if let Some(id) = s.column_fill(column) {
                focus_block(&g, &s, id, i32::MAX);
            }
        });
    }

    // ---- database views (SPEC §三十九) ----
    //
    // A database is the one block whose rows are not blocks, so none of these
    // callbacks goes through `editing-id`: a cell is named by
    // (block, record, property) and the write paths (`AppState::db_*`) parse,
    // plan and re-read the window. Two rules hold across the whole section:
    //
    // * **the "one live TextInput" discipline still holds** — a cell's editor
    //   exists only while `editing-id < 0`, and opening a cell closes the
    //   block editor (and the other way round, in `focus_block`'s caller),
    //   so a page can never have two inputs whatever order the two were set;
    // * **a structural change re-fills that one row.** A window read mutates
    //   the rows model in place (that is why scrolling a 10 000-row table is
    //   one query), but the *shape* the delegate lays out — row count, window
    //   start, columns, tabs — rides on the `BlockRow`, which only
    //   `db_fill_row` rebuilds. `db_refill_row` is the one-row version of the
    //   page projection, and it is what keeps the block as tall as the table.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_cell_activated(move |block, record, property| {
                let g = gw.upgrade().unwrap();
                // a block editor's pending keystrokes land first: closing it
                // below would otherwise drop everything typed in the last
                // debounce window
                flush_pending_edit(&g, &s);
                // the *stored* value, not the painted one: a number with a
                // format paints (`50%`) differently from what an editor must
                // accept (`0.5`)
                let text = s
                    .db_cell_text(block, record as i64, property)
                    .unwrap_or_default();
                g.set_editing_id(-1);
                g.set_editing_text(text.into());
                g.set_db_editing_block(block);
                g.set_db_editing_record(record);
                g.set_db_editing_property(property);
                // a click is also the way out of an open option list
                g.set_db_picking_record(-1);
                g.set_db_picking_property(-1);
            });
    }
    {
        // the debounced commit while typing: the same 300 ms timer the prose
        // rows use, with the database's own ids read back instead of
        // `editing-id`.
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_cell_changed(move || {
            db_debounce_arm(t, &gw, &s);
        });
    }
    {
        // the live cell is going away (Enter / Escape / Tab): commit **now**.
        // Leaving it to the debounce would lose the text typed in the last
        // 300 ms, because closing resets the two ids that commit reads back.
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_cell_closed(move || {
            let g = gw.upgrade().unwrap();
            db_commit_cell(&g, &s);
            g.set_db_editing_block(-1);
            g.set_db_editing_record(-1);
            g.set_db_editing_property(-1);
            // the next editor is created with the focus and gets its text from
            // Rust; an empty mirror here is what keeps a stale value from
            // flashing up first
            g.set_editing_text("".into());
            g.set_db_picking_record(-1);
            g.set_db_picking_property(-1);
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_cell_toggled(move |block, record, property, checked| {
                // the inverse of what the cell shows, which is the stored flag
                // (the painted word is ADR-0065's `Yes`/`No`); the rows model is
                // re-read in place, so no `db_refill_row` is needed — a toggle
                // changes no row's existence
                s.db_toggle_checkbox(block, record as i64, property, checked);
            });
    }
    {
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_option_picked(move |block, record, property, option, current| {
                // `option` is the stored id (ADR-0061 stores ids, not labels);
                // picking the one a cell already holds clears it
                s.db_pick_option(block, record as i64, property, &option, &current);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_row_added(move |block| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            // the key is asked of the store (a `MAX(ord)` on an index): the app
            // deliberately does not hold the rows to find the last one (ADR-0067)
            let ord = s.db_next_row_ord(block);
            if s.db_add_record(block, ord).is_some() {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_row_removed(move |block, record| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            if s.db_delete_record(block, record as i64) {
                // the row menu's delete is ADR-0063's plan ([values, record,
                // page?]) and one Ctrl+Z, so nothing here touches the history
                db_refill_row(&s, block);
            }
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_column_resized(move |block, property, permille| {
                // applied on release, never per move: the write is one
                // view-document edit and one undo step (ADR-0064's `widths`)
                if s.db_set_column_width(block, property, permille) {
                    db_refill_row(&s, block);
                }
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_columns_opened(move |block, x, y| {
                let g = gw.upgrade().unwrap();
                db_push_columns(&g, &s, block);
                // `absolute-position` is window-relative, so the anchor needs
                // no conversion — only a clamp: a database near the bottom of
                // the window must not open its popup off screen.
                let rows = g.get_db_columns_rows().row_count() as f32;
                let popup_h = rows * 30.0 + 8.0;
                let y = y.clamp(48.0, (g.get_window_h() - popup_h - 8.0).max(48.0));
                let x = x.clamp(8.0, (g.get_window_w() - 232.0).max(8.0));
                g.set_db_columns_x(x);
                g.set_db_columns_y(y);
                g.set_db_columns_block(block);
                g.set_db_columns_open(true);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_column_toggled(move |block, property| {
            let g = gw.upgrade().unwrap();
            if s.db_toggle_column(block, property) {
                // the view shows different columns now: the row is re-filled
                // (header and cells) and the popup's own rows are rebuilt in
                // place, because it stays open across the click
                db_refill_row(&s, block);
                db_push_columns(&g, &s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_columns_closed(move || {
            let g = gw.upgrade().unwrap();
            g.set_db_columns_open(false);
            // The type menu is panel *1 of this same popup*, so Escape or a
            // click outside has to send the popup home, not just hide it:
            // left set, the next summons opened straight back onto a kind list
            // for a column that is no longer selected.
            g.set_db_columns_panel(0);
            g.set_db_columns_property(-1);
            g.set_db_columns_title("".into());
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_db_view_picked(move |block, view| {
            // session state, not a column (ADR-0073): a different view has
            // different columns, so the row is re-read and re-filled
            if s.db_pick_view(block, view) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_viewport(move |block, top_in_view| {
            let g = gw.upgrade().unwrap();
            // the editor's height is the viewport every database on the page is
            // windowed against; the delegate reports it once per resize
            s.editor_viewport_h.set(g.get_editor_viewport_h());
            // a scroll costs the arithmetic and, only when the window moved past
            // its overscan, one query (`db_watch` is that gate). The rows model
            // is mutated in place; the block row is re-filled only when the
            // window really moved, because that is when `db-row-start` changed.
            if s.db_watch(block, top_in_view) {
                db_refill_row(&s, block);
            }
        });
    }

    // ---- database rules (SPEC §三十九 「操作」, D4) ----
    //
    // The same two rules the cell callbacks carry, with one addition: the
    // panel stays open across its own edits, so every accepted edit re-pushes
    // the panel's rows (`db_push_filter`) the way the columns popup re-pushes
    // its toggles. An edit that Rust refuses (a value of the wrong shape, a
    // tree the panel cannot represent) writes nothing, and the panel shows
    // the rules that are still in force.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_opened(move |block, x, y| {
            let g = gw.upgrade().unwrap();
            // the panel's value boxes are inputs: the block editor must not
            // also be up
            flush_pending_edit(&g, &s);
            g.set_editing_id(-1);
            db_push_filter(&g, &s, block);
            g.set_db_filter_panel(0);
            g.set_db_filter_block(block);
            // anchored under the button (window coordinates, the columns
            // popup's convention) and clamped to the rules state's height —
            // the tallest the panel gets
            let rows = s.db_filter_panel(block).1.len() as f32;
            let popup_h = 40.0 + rows * 76.0 + 36.0 + 8.0;
            let y = y.clamp(48.0, (g.get_window_h() - popup_h - 8.0).max(48.0));
            let x = x.clamp(8.0, (g.get_window_w() - 348.0 - 8.0).max(8.0));
            g.set_db_filter_x(x);
            g.set_db_filter_y(y);
            g.set_db_filter_open(true);
        });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_filter_closed(move || {
            let g = gw.upgrade().unwrap();
            g.set_db_filter_open(false);
        });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_filter_panel_set(move |panel| {
            let g = gw.upgrade().unwrap();
            g.set_db_filter_panel(panel);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_match_set(move |block, any| {
            let g = gw.upgrade().unwrap();
            if s.db_filter_set_match(block, any) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_filter_add_rule(move |_block| {
            let g = gw.upgrade().unwrap();
            // the chooser's list is already pushed; the panel just turns a page
            g.set_db_filter_panel(1);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_column_picked(move |block, property| {
            let g = gw.upgrade().unwrap();
            if s.db_filter_add_clause(block, property) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
                g.set_db_filter_panel(0);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_row_removed(move |block, index| {
            let g = gw.upgrade().unwrap();
            if s.db_filter_remove_clause(block, index as usize) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_op_menu(move |block, index| {
            let g = gw.upgrade().unwrap();
            db_push_filter_ops(&g, &s, block, index as usize);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_op_picked(move |block, index, op| {
            let g = gw.upgrade().unwrap();
            // switching the comparison resets the value (the old shape is not
            // the new one's), so the row comes back unfilled
            if s.db_filter_set_op(block, index as usize, op as usize) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
                g.set_db_filter_panel(0);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_text_set(move |block, index, text| {
            let g = gw.upgrade().unwrap();
            // a refused value (not a number, not a stored date) writes nothing
            // and the panel keeps the text for the user to fix
            if s.db_filter_set_text(block, index as usize, &text) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_flag_set(move |block, index, checked| {
            let g = gw.upgrade().unwrap();
            if s.db_filter_set_flag(block, index as usize, checked) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_option_menu(move |block, index| {
            let g = gw.upgrade().unwrap();
            db_push_filter_options(&g, &s, block, index as usize);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_option_picked(move |block, index, option| {
            let g = gw.upgrade().unwrap();
            // `is`: picking the held option clears the rule; `any of`: the
            // pick toggles membership — either way the panel goes home
            if s.db_filter_toggle_option(block, index as usize, &option) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
                g.set_db_filter_panel(0);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_not_toggled(move |block, index| {
            let g = gw.upgrade().unwrap();
            if s.db_filter_toggle_not(block, index as usize) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_filter_cleared(move |block| {
            let g = gw.upgrade().unwrap();
            if s.db_filter_clear(block) {
                db_refill_row(&s, block);
                db_push_filter(&g, &s, block);
            }
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_db_sort_cycled(move |block, property| {
            // the header cycles none → asc → desc → none; a kind with no
            // order refuses in Rust and the header stays as it was
            if s.db_sort_cycle(block, property) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_group_opened(move |block, x, y| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            db_push_group(&g, &s, block);
            g.set_db_group_block(block);
            let rows = s.db_group_choices(block).len() as f32 + 1.0;
            let popup_h = rows * 30.0 + 8.0;
            let y = y.clamp(48.0, (g.get_window_h() - popup_h - 8.0).max(48.0));
            let x = x.clamp(8.0, (g.get_window_w() - 224.0 - 8.0).max(8.0));
            g.set_db_group_x(x);
            g.set_db_group_y(y);
            g.set_db_group_open(true);
        });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_group_closed(move || {
            let g = gw.upgrade().unwrap();
            g.set_db_group_open(false);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_group_picked(move |block, property| {
            let g = gw.upgrade().unwrap();
            if s.db_group_pick(block, property) {
                db_refill_row(&s, block);
            }
            // the pick is the whole interaction: the picker closes whether it
            // changed anything or not (a refusal said its own word)
            g.set_db_group_open(false);
        });
    }

    // ---- database view family (SPEC §三十九 「视图」, D5) ----
    //
    // The switcher's `+` (one callback per picked layout, `ViewLayout::ALL`'s
    // order), the card click (open the record's page, minting it when the
    // record is bare — ADR-0063's lazy page finally getting its trigger), the
    // calendar's ‹ ›, the gallery's reported shape, and the form's three
    // callbacks. The same two rules the D4 bindings carry: every accepted edit
    // re-fills the one block row (the layout's own payload rides on it), and a
    // write Rust refused changed nothing on screen.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_view_added(move |block, layout| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            // `db_add_view` also switches to the new view — creating and not
            // looking would be half a gesture — so one refill carries the
            // switch's own re-read
            if s.db_add_view(block, layout) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_open_record(move |block, record| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            if let Some(page) = s.db_open_record(block, record as i64) {
                // the record may have just gained its page: the block row's
                // own shape (board cards' Open targets) follows on the refill,
                // and the navigation below is the sidebar's usual dance
                db_refill_row(&s, block);
                open(&g, &s, page);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_cal_month(move |block, delta| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            if s.db_cal_shift(block, delta) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_db_gallery_shaped(move |block, per_row| {
            // the grid was resized: the card slice is rows × per_row, so a new
            // shape is a re-read (and a no-op when the clamp keeps the number)
            if s.db_gallery_set_per_row(block, per_row) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_db_form_text(move |block, property, text| {
            // the draft only: no refill (the live input must not be rebuilt
            // under the keystroke), no SQL (nothing exists until Submit)
            s.db_form_set_text(block, property, &text);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_form_submitted(move |block| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            // a refused submit said its own word (`db-notice`); a successful
            // one cleared the draft, and the refill is what blanks the fields
            if s.db_form_submit(block) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_db_form_cleared(move |block| {
            s.db_form_clear(block);
            db_refill_row(&s, block);
        });
    }
    // ─── D6 (SPEC §三十九 「需计算」): the formula editor ──────────────────────
    // A formula cell's click lands here (DatabaseCell's `is-formula` arm): the
    // editor opens pre-filled with the *stored* expression, and the preview
    // evaluates the draft on the row the user clicked. One accepted save is one
    // `SetDatabaseFormula` change and one Ctrl+Z; a refused save says why in the
    // notice line and keeps the old expression.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_formula_opened(move |block, record, property| {
                let g = gw.upgrade().unwrap();
                // a cell editor's pending keystrokes land first: the same rule
                // `db-cell-activated` follows, for the same reason
                flush_pending_edit(&g, &s);
                db_show_formula(&g, &s, block, record, property);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_formula_text_changed(move |text| {
                let g = gw.upgrade().unwrap();
                // the ids are read back from the mirror: the popup is the one
                // editor open, and it opened them
                let property = g.get_db_formula_property();
                let record = g.get_db_formula_record();
                let (preview, error) = s.db_formula_preview(record as i64, property, &text);
                g.set_db_formula_preview(preview.into());
                g.set_db_formula_error(error.into());
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_formula_accepted(move || {
            let g = gw.upgrade().unwrap();
            let block = g.get_db_formula_block();
            let property = g.get_db_formula_property();
            let text = g.get_db_formula_text().to_string();
            // close first, then commit: the same order `db-cell-closed` uses, so
            // a refused save's notice is on screen rather than behind a popup
            g.set_db_formula_open(false);
            g.set_db_formula_block(-1);
            g.set_db_formula_property(-1);
            g.set_db_formula_record(-1);
            if s.db_formula_accept(block, property, &text) {
                // the column's every cell may now paint differently
                db_refill_row(&s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_formula_closed(move || {
            let g = gw.upgrade().unwrap();
            // Escape is the keyboard's way out, and the window flag is Rust's to
            // clear — the same shape D10's two popups close with. Left unset,
            // the sheet stayed up over its own reset ids.
            g.set_db_formula_open(false);
            g.set_db_formula_block(-1);
            g.set_db_formula_property(-1);
            g.set_db_formula_record(-1);
        });
    }
    // ─── D10 (SPEC §三十九 「属性」): the column type menu ────────────────────
    //
    // The menu is the door D5 left unbuilt. Every kind after `text` could
    // already be stored (`AddDatabaseProperty` takes one, `PropertyKindSet`
    // writes one) and projected (a relation points, a rollup folds, a formula
    // computes); none of them could be *chosen*, so the whole of ADR-0088 and
    // ADR-0089 was reachable from a test and not from a window.
    //
    // Creation and conversion are the same panel because a schema has one set
    // of kinds and two menus would be two lists agreeing about it — and the
    // same one refusal (`kind_move_refusal`, in the state layer) gates both.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_kind_opened(move |block, property| {
                let g = gw.upgrade().unwrap();
                flush_pending_edit(&g, &s);
                db_push_kinds(&g, &s, block, property);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_kind_back(move || {
            let g = gw.upgrade().unwrap();
            let block = g.get_db_columns_block();
            if block >= 0 {
                db_show_columns(&g, &s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_kind_picked(move |block, kind| {
            let g = gw.upgrade().unwrap();
            let property = g.get_db_columns_property();
            if property < 0 {
                // Creation. A formula column goes straight into its editor,
                // the way D6's one creation row left its user: a column that
                // computes has nothing to show until one line of it exists.
                let Some(created) = s.db_column_add(block, kind) else {
                    g.set_db_notice("无法添加该列。".into());
                    return;
                };
                db_show_columns(&g, &s, block);
                db_refill_row(&s, block);
                if kind == PropertyKind::Formula.index() {
                    g.set_db_columns_open(false);
                    db_show_formula(&g, &s, block, -1, created);
                }
                return;
            }
            if let Err(why) = s.db_column_kind_set(block, property, kind) {
                g.set_db_notice(why.into());
                return;
            }
            // The kind is what the header, the cells and every editor of that
            // column read, so the row's shape goes back through the delegate —
            // and the sentence about what happened to the stored values is the
            // state layer's to say, which it does through the notice line.
            db_show_columns(&g, &s, block);
            db_refill_row(&s, block);
            if kind == PropertyKind::Formula.index() {
                // …and the sheet is the one popup: the menu it was opened from
                // goes away rather than sitting behind it.
                g.set_db_columns_open(false);
                db_show_formula(&g, &s, block, -1, property);
            }
        });
    }

    // ─── D10 (ADR-0088): the relation picker ─────────────────────────────────
    //
    // One popup, three states, and the write is always the *whole* list plus
    // the back-pointers it implies — so a pick is one Ctrl+Z whether it added
    // the third target or cleared the last. Every refusal here is
    // `check_pair`'s or the state layer's, said in the notice line rather than
    // by a silent no-op: a click that did nothing has to be a click the user
    // can hear.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_relation_opened(move |block, record, property| {
                let g = gw.upgrade().unwrap();
                flush_pending_edit(&g, &s);
                // the cell editor and the option list are the two live
                // surfaces a click can otherwise leave up behind this popup
                g.set_db_editing_block(-1);
                g.set_db_editing_record(-1);
                g.set_db_editing_property(-1);
                g.set_db_picking_record(-1);
                g.set_db_picking_property(-1);
                g.set_db_formula_open(false);
                g.set_db_rollup_open(false);
                g.set_db_relation_block(block);
                g.set_db_relation_record(record);
                g.set_db_relation_property(property);
                g.set_db_relation_name(s.db_property_label(property).into());
                g.set_db_relation_text("".into());
                let target = s.db_relation_config(property).0;
                g.set_db_relation_target(s.db_database_name(target).into());
                // An unconfigured column opens on its configuration: a picker
                // over a database nobody has named would be a text field above
                // an empty box.
                g.set_db_relation_open(true);
                db_push_relation(&g, &s, if target < 0 { 1 } else { 0 });
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_relation_panel_set(move |panel| {
            let g = gw.upgrade().unwrap();
            db_push_relation(&g, &s, panel);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_relation_text_set(move |_needle| {
            let g = gw.upgrade().unwrap();
            // the text itself is the binding's job; this is the narrowed list
            db_push_relation(&g, &s, 0);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_relation_database_picked(move |target| {
                let g = gw.upgrade().unwrap();
                let block = g.get_db_relation_block();
                let property = g.get_db_relation_property();
                // Re-pointing a relation drops its back-pointer with the
                // database it pointed at: a mirror of the old target is not a
                // mirror of the new one, and ADR-0088's involution is the
                // property that would quietly break.
                if let Err(why) = s.db_relation_configure(block, property, target, -1) {
                    g.set_db_notice(why.into());
                    return;
                }
                g.set_db_relation_target(s.db_database_name(target).into());
                db_push_relation(&g, &s, 2);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_relation_mirror_picked(move |mirror| {
                let g = gw.upgrade().unwrap();
                let block = g.get_db_relation_block();
                let property = g.get_db_relation_property();
                let target = s.db_relation_config(property).0;
                if let Err(why) = s.db_relation_configure(block, property, target, mirror) {
                    g.set_db_notice(why.into());
                    return;
                }
                // Home to the picker: the column is configured now, and which
                // rows it holds is the question the user was on their way to.
                db_push_relation(&g, &s, 0);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_relation_toggled(move |record| {
            let g = gw.upgrade().unwrap();
            let block = g.get_db_relation_block();
            let property = g.get_db_relation_property();
            let row = g.get_db_relation_record();
            if !s.db_relation_toggle(block, row as i64, property, record) {
                g.set_db_notice("无法写入该链接。".into());
                return;
            }
            // The cell's own list changed, so its painted text did — one row,
            // not one page (`db_refill_row`'s reasoning), and the tick list is
            // re-read from storage rather than toggled in place here.
            db_push_relation(&g, &s, 0);
            db_refill_row(&s, block);
        });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_relation_closed(move || {
            let g = gw.upgrade().unwrap();
            // the callback is the *keyboard's* way out (Enter/Escape in the
            // needle); the window flag is Rust's to clear, as
            // `db-formula-accepted`'s handler does. Without it the popup stays
            // up with its ids reset, and the next click writes to nothing.
            g.set_db_relation_open(false);
            g.set_db_relation_block(-1);
            g.set_db_relation_record(-1);
            g.set_db_relation_property(-1);
            g.set_db_relation_panel(0);
            g.set_db_relation_text("".into());
        });
    }

    // ─── D10 (ADR-0089): the rollup configurator ─────────────────────────────
    //
    // Three names, one config, one write per pick — and the user lands back on
    // the sentence each time, because a fold that half-exists is a fact about
    // the column the header should be able to show. The choosers list what
    // `check_config` accepts, so a pick here refuses on the way in rather than
    // surprising the next projection.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_rollup_opened(move |block, property| {
                let g = gw.upgrade().unwrap();
                flush_pending_edit(&g, &s);
                g.set_db_editing_block(-1);
                g.set_db_editing_record(-1);
                g.set_db_editing_property(-1);
                g.set_db_picking_record(-1);
                g.set_db_picking_property(-1);
                g.set_db_formula_open(false);
                g.set_db_relation_open(false);
                g.set_db_rollup_block(block);
                g.set_db_rollup_property(property);
                g.set_db_rollup_name(s.db_property_label(property).into());
                g.set_db_rollup_open(true);
                db_push_rollup(&g, &s, 0);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_rollup_panel_set(move |panel| {
            let g = gw.upgrade().unwrap();
            db_push_rollup(&g, &s, panel);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_rollup_relation_picked(move |relation| {
                let g = gw.upgrade().unwrap();
                let block = g.get_db_rollup_block();
                let property = g.get_db_rollup_property();
                let (_was_relation, _was_column, aggregate) = s.db_rollup_config(property);
                // The second name goes with the first: it belonged to a
                // different database's columns, and a fold whose two names
                // disagree about which database is ADR-0089's cycle waiting to
                // happen. Straight on to the column chooser, so the sentence
                // is never left half-spoken on screen.
                if let Err(why) =
                    s.db_rollup_configure(block, property, relation, -1, aggregate)
                {
                    g.set_db_notice(why.into());
                    return;
                }
                db_push_rollup(&g, &s, 2);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_rollup_column_picked(move |column| {
                let g = gw.upgrade().unwrap();
                let block = g.get_db_rollup_block();
                let property = g.get_db_rollup_property();
                let (relation, _was_column, aggregate) = s.db_rollup_config(property);
                if let Err(why) = s.db_rollup_configure(block, property, relation, column, aggregate)
                {
                    g.set_db_notice(why.into());
                    return;
                }
                db_push_rollup(&g, &s, 0);
                // the fold's answer is a cell's text now, so the row repaints
                db_refill_row(&s, block);
            });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_db_rollup_aggregate_picked(move |aggregate| {
                let g = gw.upgrade().unwrap();
                let block = g.get_db_rollup_block();
                let property = g.get_db_rollup_property();
                let (relation, column, _was_aggregate) = s.db_rollup_config(property);
                if let Err(why) =
                    s.db_rollup_configure(block, property, relation, column, aggregate)
                {
                    g.set_db_notice(why.into());
                    return;
                }
                db_push_rollup(&g, &s, 0);
                db_refill_row(&s, block);
            });
    }
    {
        let gw = gw.clone();
        ui.global::<UIState>().on_db_rollup_closed(move || {
            let g = gw.upgrade().unwrap();
            g.set_db_rollup_open(false);
            g.set_db_rollup_block(-1);
            g.set_db_rollup_property(-1);
            g.set_db_rollup_panel(0);
        });
    }

    // ---- database advanced features (SPEC §三十九 「操作」, D7) ----
    //
    // The search box, the chart's shape buttons and the row's save-as-template
    // affordance. The shape of every block here is the one the rules callbacks
    // established in D4: Rust validates and writes (one change, one undo step
    // where a write happens), the rows model is re-read, and the block row is
    // re-filled because a search changes the count the header shows.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_search_opened(move |block| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            g.set_editing_id(-1);
            // the box opens with the needle it is still filtering by — a
            // search that was never closed by the caller re-opens as itself
            g.set_db_search_block(block);
            g.set_db_search_text(s.db_search_needle(block).into());
            g.set_db_search_open(true);
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_search_text_set(move |block, text| {
            let g = gw.upgrade().unwrap();
            // the needle is a read's input, not a document write: no change,
            // no undo step — but the count in the header moves with it, so
            // the row is re-filled whenever the request changed
            g.set_db_search_text(text.clone().into());
            if s.db_view_search_set(block, &text) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_search_closed(move |block| {
            let g = gw.upgrade().unwrap();
            g.set_db_search_open(false);
            g.set_db_search_text("".into());
            // closing the box clears the search: an invisible predicate on a
            // visible table is exactly the kind of state the header's count
            // could not explain
            if s.db_view_search_set(block, "") {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let s = state.clone();
        ui.global::<UIState>().on_db_chart_kind(move |block, kind| {
            if s.db_chart_kind_set(block, kind) {
                db_refill_row(&s, block);
            }
        });
    }
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_db_template_saved(move |block, record| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            if s.db_template_from_row(block, record as i64) {
                db_refill_row(&s, block);
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_enter_at_caret(move |caret| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            let cur = g.get_editing_id();
            if cur <= 0 {
                return;
            }
            let changes = s.exec_on_open_page(Command::SplitBlock {
                id: BlockId(cur as u64),
                caret: caret.max(0) as usize,
            });
            let new_id = changes.as_deref().and_then(find_inserted_id);
            if let Some(nid) = new_id {
                focus_block(&g, &s, nid, 0);
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_backspace_at_start(move || {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            let cur = g.get_editing_id();
            if cur <= 0 {
                return;
            }
            let page = core_page_id(s.open_page.get());
            // capture the previous block so the caret can land at the seam
            let prev = {
                let d = s.doc.borrow();
                let blocks = d.page_blocks(page);
                blocks
                    .iter()
                    .position(|b| b.id.0 as i32 == cur)
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| blocks.get(i))
                    .map(|b| (b.id.0 as i32, b.text.len() as i32))
            };
            let changes = s.exec_on_open_page(Command::MergeBackward {
                id: BlockId(cur as u64),
            });
            if changes.is_some() {
                match prev {
                    Some((pid, plen)) => focus_block(&g, &s, pid, plen),
                    None => g.set_editing_id(-1), // first block deleted
                }
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_focus_move(move |delta| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            let cur = g.get_editing_id();
            if cur <= 0 {
                return;
            }
            let page = core_page_id(s.open_page.get());
            let target = {
                let d = s.doc.borrow();
                let blocks = d.page_blocks(page);
                blocks
                    .iter()
                    .position(|b| b.id.0 as i32 == cur)
                    .map(|i| i as i32 + delta)
                    .and_then(|t| {
                        if t < 0 {
                            None
                        } else {
                            blocks
                                .get(t as usize)
                                .map(|b| (b.id.0 as i32, b.text.len() as i32, delta < 0))
                        }
                    })
            };
            if let Some((tid, tlen, up)) = target {
                focus_block(&g, &s, tid, if up { tlen } else { 0 });
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_cancel_editing(move || {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            g.set_editing_id(-1);
        });
    }

    // ---- slash menu keyboard + apply ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_slash_move(move |delta| {
            let g = gw.upgrade().unwrap();
            let count = s.slash_focus_count();
            if count == 0 {
                return;
            }
            let next = s.slash_next_focus(g.get_slash_focus(), delta);
            g.set_slash_focus(next);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_slash_apply_selected(move || {
            let g = gw.upgrade().unwrap();
            let focus = g.get_slash_focus();
            // mention mode: the focused row is the date item or a page, and the
            // atom replaces the "@filter" the writer typed (SPEC §四十). The
            // trigger is recomputed from the live text rather than remembered:
            // the caret can be moved mid-line while the menu is open, and the
            // offset that matters is the one on screen at the moment of the
            // pick.
            if g.get_slash_pick_mention() {
                let id = g.get_editing_id();
                // the pending keystrokes land first: the atom replaces the
                // text the *document* holds, and a cell's own commit is still
                // on the debounce timer when the pointer reaches the menu
                flush_pending_edit(&g, &s);
                let text = g.get_editing_text().to_string();
                if id > 0 {
                    if let (Some(at), Some((row, label))) = (
                        crate::core::trigger_at(&text),
                        s.slash_selected_mention(focus),
                    ) {
                        let page = (row != crate::app::state::MENTION_DATE_ROW).then_some(row);
                        s.apply_mention(id, at, page, &label);
                    }
                }
                g.set_slash_open(false);
                g.set_slash_pick_mention(false);
                refresh_focused_text(&g, &s);
                return;
            }
            // synced-picker mode (SPEC §四十, ADR-0052): the focused row is a
            // **block** anywhere in the library, and the line has already been
            // turned into a mirror, so this pick writes one thing — the pointer.
            // Nothing here touches the row's text: a mirror holding words would
            // have two owners of one sentence, and `set_sync_source` is the only
            // place allowed to say so.
            if g.get_slash_pick_synced() {
                let id = g.get_editing_id();
                if id > 0 {
                    if let Some(source) = s.slash_selected_block(focus) {
                        s.set_sync_source(id, Some(source));
                    }
                }
                g.set_slash_open(false);
                g.set_slash_pick_synced(false);
                return;
            }
            // the database picker (D7, ADR-0085): the focused row IS a
            // database — convert the empty line into a linked database that
            // draws it. `db_make_linked` refuses a dead id, so a picker row
            // whose entity died while the menu was open costs nothing.
            if g.get_slash_pick_linkdb() {
                let id = g.get_editing_id();
                if id > 0 {
                    if let Some(db) = s.slash_selected_link(focus) {
                        s.db_make_linked(id, db);
                    }
                }
                g.set_slash_open(false);
                g.set_slash_pick_linkdb(false);
                g.set_editing_id(-1);
                return;
            }
            // page-picker mode: the focused row IS a page — convert the empty
            // line into a Link-to-page block pointing at it
            if g.get_slash_pick_page() {
                let id = g.get_editing_id();
                if let Some(page) = s.slash_selected_page(focus) {
                    if id > 0 {
                        s.create_page_link_block(id, page);
                    }
                }
                g.set_slash_open(false);
                g.set_slash_pick_page(false);
                g.set_editing_id(-1);
                return;
            }
            // A template row: this line becomes the copy. Both doors mean the
            // same thing at this point — "/" has nothing left to keep once the
            // filter is stripped, so its empty line is replaced, while the "+"
            // menu's line keeps whatever it holds and the copy lands under it.
            // That split is `insert_template`'s replace-an-empty-anchor rule
            // reading the row's own emptiness, so no mode test picks where the
            // blocks go; the mode only decides whether to clear the text first.
            if let Some((template, name)) = s.slash_selected_template(focus) {
                let insert_mode = g.get_slash_insert();
                let id = g.get_editing_id();
                g.set_slash_open(false);
                if id <= 0 {
                    g.set_slash_insert(false);
                    return;
                }
                if !insert_mode {
                    let _ = s.exec_on_open_page(Command::ReplaceText {
                        id: BlockId(id as u64),
                        text: String::new(),
                    });
                    g.set_editing_text("".into());
                    g.set_pending_caret(0);
                }
                g.set_slash_insert(false);
                match s.insert_template(Some(id), template) {
                    Some(first) => focus_block(&g, &s, first, i32::MAX),
                    None => {
                        // Nothing landed, so the row that asked for it is still
                        // the row on screen — except in the replace case, where
                        // it is gone, and an editing-id pointing at a deleted
                        // block is a stale caret rather than an empty line.
                        g.set_editing_id(-1);
                        if !s.page_locked() {
                            g.set_db_notice(
                                format!("“{name}”没有可插入的块。").into(),
                            );
                        }
                    }
                }
                return;
            }
            // D7 (ADR-0085): the "Linked view" row is not a kind — its id is
            // the picker's (`LINKED_VIEW_ROW`), so it is handled before any
            // kind mapping would drop it. The line gives up its words now and
            // the popup switches to the database picker; the pick that follows
            // does the converting, exactly the Link row's two-step shape.
            if s.slash_selected_link(focus) == Some(crate::app::state::LINKED_VIEW_ROW) {
                let id = g.get_editing_id();
                if id > 0 {
                    let _ = s.exec_on_open_page(Command::ReplaceText {
                        id: BlockId(id as u64),
                        text: String::new(),
                    });
                    g.set_editing_text("".into());
                    g.set_pending_caret(0);
                    s.open_slash_links("");
                    g.set_slash_filter("".into());
                    g.set_slash_focus(0);
                    g.set_slash_pick_linkdb(true);
                }
                return;
            }
            let kind = match s.slash_selected_kind(focus) {
                Some(k) => k,
                None => return,
            };
            let text = g.get_editing_text().to_string();
            let insert_mode = g.get_slash_insert();
            // insert mode: the "+" menu discards the typed filter text;
            // slash mode: strip "/filter" (the menu only triggers on the
            // block's first token; a space closes the menu before this point)
            let filter_len = g.get_slash_filter().len();
            let cleaned = if insert_mode {
                String::new()
            } else if text.len() >= 1 + filter_len && text.is_char_boundary(1 + filter_len) {
                text[1 + filter_len..].to_string()
            } else {
                String::new()
            };
            let id = g.get_editing_id();
            if id <= 0 {
                return;
            }
            // An attachment is not a text style: it needs a file before the
            // block can exist, so the menu closes and the picker decides. "/"
            // means "this line becomes the picture" (the line is empty once
            // the /filter is stripped); the "+" menu means "a picture appears
            // here", so the line it was opened on keeps whatever it holds.
            if matches!(
                kind,
                crate::core::BlockKind::Image | crate::core::BlockKind::File
            ) {
                g.set_slash_open(false);
                g.set_slash_insert(false);
                g.set_editing_id(-1);
                if insert_mode {
                    pick_attachment(&g, &s, id, false, kind);
                } else {
                    let _ = s.exec_on_open_page(Command::ReplaceText {
                        id: BlockId(id as u64),
                        text: cleaned.clone(),
                    });
                    pick_attachment(&g, &s, id, true, kind);
                }
                return;
            }
            // Page is insert-menu-only: applying it creates the child page
            // and converts the (empty insert-mode) row; no text edit involved
            if kind == crate::core::BlockKind::Page {
                s.create_page_block(id);
                g.set_slash_open(false);
                g.set_slash_insert(false);
                return;
            }
            // "Synced block" switches the popup to the block picker (SPEC §四十,
            // ADR-0052). The line gives up its words and takes the mirror's kind
            // in **one** batch — two calls would leave two Ctrl+Z the writer
            // never asked for — and the pick that follows decides which block
            // this row is a second view of. Walking away is allowed: it leaves
            // the documented placeholder rather than a half-made mirror.
            if kind == crate::core::BlockKind::Synced {
                s.exec_all_on_open_page(vec![
                    Command::ReplaceText {
                        id: BlockId(id as u64),
                        text: String::new(),
                    },
                    Command::SetBlockType {
                        id: BlockId(id as u64),
                        kind,
                    },
                ]);
                g.set_editing_text("".into());
                g.set_pending_caret(0);
                s.open_slash_block("");
                g.set_slash_filter("".into());
                g.set_slash_focus(0);
                g.set_slash_pick_synced(true);
                return;
            }
            // Link to page switches the popup to the page picker: the typed
            // filter is discarded, the anchor stays, and the next pick
            // converts the line
            if kind == crate::core::BlockKind::Link {
                let _ = s.exec_on_open_page(Command::ReplaceText {
                    id: BlockId(id as u64),
                    text: String::new(),
                });
                g.set_editing_text("".into());
                g.set_pending_caret(0);
                s.open_slash_pick("");
                g.set_slash_filter("".into());
                g.set_slash_focus(0);
                g.set_slash_pick_page(true);
                return;
            }
            // SPEC §三十九: "Table view" turns the line into a database — the
            // block, the entity, its title column and its first view in one
            // batch (`make_database`; the line's own words go with the same
            // undo). `SetBlockType` refuses a Database kind by design: the
            // entity is not the plan layer's to allocate. A database draws no
            // line input of its own, so the caret leaves the row entirely.
            if kind == crate::core::BlockKind::Database {
                s.make_database(id);
                g.set_slash_open(false);
                g.set_slash_insert(false);
                g.set_editing_text("".into());
                g.set_editing_id(-1);
                return;
            }
            let _ = s.exec_on_open_page(Command::ReplaceText {
                id: BlockId(id as u64),
                text: cleaned.clone(),
            });
            let changes = s.exec_on_open_page(Command::SetBlockType {
                id: BlockId(id as u64),
                kind,
            });
            g.set_slash_open(false);
            g.set_slash_insert(false);
            // a grid and a layout draw no row input of their own, so the caret
            // cannot stay on the row that just became one: it goes into the
            // first cell or the first line, which is where the words moved
            let target = match kind {
                crate::core::BlockKind::Table => {
                    changes.as_deref().and_then(find_inserted_cell_id)
                }
                crate::core::BlockKind::Columns => {
                    changes.as_deref().and_then(find_inserted_line_id)
                }
                _ => None,
            };
            match target {
                Some(target) => focus_block(&g, &s, target, i32::MAX),
                None => {
                    g.set_editing_text(cleaned.clone().into());
                    g.set_pending_caret(cleaned.len() as i32);
                    g.set_editing_id(id);
                }
            }
        });
    }

    {
        let gw = gw.clone();
        ui.global::<UIState>().on_slash_close(move || {
            let g = gw.upgrade().unwrap();
            g.set_slash_open(false);
            // every mode flag goes with it: a space or an Escape is the writer
            // saying "that was the character", so the next "/" or "@" has to
            // open its own menu rather than reopen the one just dismissed
            g.set_slash_pick_mention(false);
            g.set_slash_pick_page(false);
            g.set_slash_pick_synced(false);
            g.set_slash_pick_linkdb(false);
        });
    }

    // SPEC §四十: a table cell's edited-text report. The cell fires
    // `cell-changed` (the debounced commit) and nothing else, because a grid
    // is not a line of prose — but "@" has to reach a cell all the same, so
    // the cell passes its geometry and the trigger is noticed here.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_mention_typed(move |text, row_y, row_h, content_x| {
                let g = gw.upgrade().unwrap();
                if g.get_slash_pick_mention() && g.get_slash_open() {
                    return; // already open on this cell; its own edits refilter
                }
                let id = g.get_editing_id();
                let Some(at) = crate::core::trigger_at(&text) else {
                    return;
                };
                if id <= 0 || s.block_kind(id).is_none() {
                    return;
                }
                let filter = &text[at + 1..];
                s.open_slash_mention(filter);
                g.set_slash_filter(filter.into());
                g.set_slash_focus(0);
                // the grid's own row is the anchor; the row-height argument is
                // the cell's, so the popup clears the cell it was typed in
                open_slash_at(&g, row_y, row_h, content_x);
                g.set_slash_pick_mention(true);
                g.set_slash_insert(false);
                g.set_slash_pick_page(false);
            });
    }

    // ---- block handle menu (+/⋮⋮) and clipboard ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_block_menu_opened(move |id, handle_y, content_x| {
                let g = gw.upgrade().unwrap();
                s.fill_block_menu(id, g.get_touch_mode());
                // anchor beside the handle: window y = top bar + list-layout
                // y (pre-scroll) - scroll; x aligns with the text column.
                // The height follows the row count so the tall root menu
                // (and its submenus) never anchor below the window. The row
                // height is the popup's own (touch rows stand at 44 dp).
                let row_h = if g.get_touch_mode() { 44.0 } else { 28.0 };
                let scroll = g.get_editor_scroll_y();
                let edge = content_edge_offset(&g);
                let menu_h = g.get_block_menu_rows().row_count() as f32 * row_h + 16.0;
                let y = (40.0 + handle_y as f32 - scroll + 2.0)
                    .clamp(48.0, (g.get_window_h() - menu_h).max(48.0));
                g.set_block_menu_x(edge + content_x as f32 + 2.0);
                g.set_block_menu_y(y);
                g.set_block_menu_open_id(id);
            });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_block_menu_action(move |action| {
            let g = gw.upgrade().unwrap();
            let id = g.get_block_menu_open_id();
            if id <= 0 {
                return;
            }
            // A submenu keeps the popup open but swaps its rows, and the swap
            // can be far taller than the one the anchor was computed for: the
            // root menu is nine rows and Move-to is one row per page, so in a
            // workspace of any size the list ran off the window bottom. The
            // open path above already clamps; this is the same clamp applied
            // to the *new* row count, with the current y as the base so a list
            // that still fits does not move at all. The page menu's submenu
            // branch has done exactly this since 2026-09-20 (the A4 sweep's
            // D3 fix) — the block menu was the one that was missed, and it is
            // the one the Known-limitations entry was reporting.
            let reanchor = || {
                let row_h = if g.get_touch_mode() { 44.0 } else { 28.0 };
                let menu_h = g.get_block_menu_rows().row_count() as f32 * row_h + 16.0;
                let y = g
                    .get_block_menu_y()
                    .clamp(48.0, (g.get_window_h() - menu_h - 8.0).max(48.0));
                g.set_block_menu_y(y);
            };
            if action == 7 {
                s.fill_block_menu_turn_into(id);
                reanchor();
                return;
            }
            if action == 8 {
                s.fill_block_menu(id, g.get_touch_mode());
                reanchor();
                return;
            }
            if action == 10 {
                s.fill_block_menu_move_to();
                reanchor();
                return;
            }
            if action == 11 {
                s.fill_block_menu_colors(id, false);
                reanchor();
                return;
            }
            if action == 12 {
                s.fill_block_menu_colors(id, true);
                reanchor();
                return;
            }
            if action == 13 {
                s.fill_block_menu_image_width(id);
                reanchor();
                return;
            }
            if action == 14 {
                s.fill_block_menu_code_lang(id);
                reanchor();
                return;
            }
            // color picks stay open (Notion-style live preview); everything
            // else closes first
            let color_pick = (AppState::COLOR_TEXT_BASE..AppState::COLOR_BG_BASE + 100)
                .contains(&action);
            let width_pick = (AppState::IMAGE_WIDTH_BASE
                ..AppState::IMAGE_WIDTH_BASE + 200)
                .contains(&action);
            let lang_pick = (AppState::CODE_LANG_BASE
                ..AppState::CODE_LANG_BASE + Lang::ALL.len() as i32)
                .contains(&action);
            if !(color_pick || width_pick || lang_pick) {
                g.set_block_menu_open_id(-1);
            }
            if width_pick {
                s.set_image_width(id, action - AppState::IMAGE_WIDTH_BASE);
                // refill so the current-tier check follows the pick
                s.fill_block_menu_image_width(id);
                return;
            }
            if lang_pick {
                let lang = Lang::ALL[(action - AppState::CODE_LANG_BASE) as usize];
                s.set_code_lang(id, lang);
                s.fill_block_menu_code_lang(id);
                return;
            }
            if action == 9 {
                crate::platform::copy_to_clipboard(&s.block_link(id));
                return;
            }
            if (AppState::MOVE_TO_BASE..AppState::COLOR_TEXT_BASE).contains(&action) {
                let page = action - AppState::MOVE_TO_BASE;
                if s.move_block_to_page(id, page) && g.get_editing_id() == id {
                    g.set_editing_id(-1);
                }
                return;
            }
            if (AppState::COLOR_TEXT_BASE..AppState::COLOR_BG_BASE).contains(&action) {
                s.set_block_color_slot(id, false, action - AppState::COLOR_TEXT_BASE);
                // refill so the current-pick check follows the pick
                s.fill_block_menu_colors(id, false);
                return;
            }
            if (AppState::COLOR_BG_BASE..AppState::COLOR_BG_BASE + 100).contains(&action) {
                s.set_block_color_slot(id, true, action - AppState::COLOR_BG_BASE);
                s.fill_block_menu_colors(id, true);
                return;
            }
            match action {
                1 => {
                    let _ = s.exec_on_open_page(Command::MoveBlock {
                        id: BlockId(id as u64),
                        delta: -1,
                    });
                }
                2 => {
                    let _ = s.exec_on_open_page(Command::MoveBlock {
                        id: BlockId(id as u64),
                        delta: 1,
                    });
                }
                3 => {
                    // a Page block duplicates its child page too, so the copy
                    // never shares a target with the original
                    if s
                        .duplicate_page_block(id)
                        .is_none()
                    {
                        let _ = s.exec_on_open_page(Command::DuplicateBlock {
                            id: BlockId(id as u64),
                        });
                    }
                }
                4 => s.copy_block(id),
                5 => {
                    s.paste_below(id);
                }
                // Touch-only (the row exists only there): the desktop "+" is a
                // handle on the row, the phone's is this row in the long-press
                // menu. Same shape as `on_block_plus` — one empty paragraph,
                // caret in it, the insert menu on top — anchored where the
                // menu just was, because the row geometry left with the
                // delegate that reported it.
                15 => {
                    let changes = s.exec_on_open_page(Command::InsertBlockAfter {
                        id: BlockId(id as u64),
                        kind: crate::core::BlockKind::Paragraph,
                        text: String::new(),
                    });
                    if let Some(nid) = changes.as_deref().and_then(find_inserted_id) {
                        focus_block(&g, &s, nid, 0);
                        s.open_slash_insert("");
                        let row_h = if g.get_touch_mode() { 44.0 } else { 32.0 };
                        let count = g.get_slash_items().row_count() as f32;
                        let menu_h = count * row_h + 8.0;
                        let bm_row_h = if g.get_touch_mode() { 44.0 } else { 28.0 };
                        let bm_h = g.get_block_menu_rows().row_count() as f32 * bm_row_h + 16.0;
                        let y = (g.get_block_menu_y() + bm_h + 4.0)
                            .clamp(48.0, (g.get_window_h() - menu_h - 8.0).max(48.0));
                        g.set_slash_x(g.get_block_menu_x());
                        g.set_slash_y(y);
                        g.set_slash_filter("".into());
                        g.set_slash_focus(0);
                        g.set_slash_insert(true);
                        g.set_slash_open(true);
                    }
                }
                6 => {
                    // deleting a Page block takes its child page with it; a
                    // Link block's target is unowned and survives
                    let kind = s.block_kind_of(id);
                    let page_ref = s.block_page_ref(id);
                    // and the child only goes *with* the block: a refused
                    // delete (SPEC §三十八) must not orphan the page behind it
                    if s
                        .exec_on_open_page(Command::DeleteBlock {
                            id: BlockId(id as u64),
                        })
                        .is_some()
                    {
                        if kind == Some(crate::core::BlockKind::Page) {
                            if let Some(child) = page_ref {
                                s.delete_page(child);
                            }
                        }
                        if g.get_editing_id() == id {
                            g.set_editing_id(-1);
                        }
                    }
                }
                a if (100..200).contains(&a) => {
                    // leaving Page/Link via Turn-into drops the reference; a
                    // Page's child page survives in the tree, unowned
                    let old_kind = s.block_kind_of(id);
                    let new_kind = kind_from_int(a - 100);
                    // A mirror is not a style either (SPEC §四十, ADR-0052):
                    // the line gives up its words in the same batch that makes
                    // it a mirror — one Ctrl+Z for one decision — because a
                    // mirror still holding words would have two owners of one
                    // sentence. Then it hands itself to the block picker,
                    // anchored where the menu already was.
                    if new_kind == crate::core::BlockKind::Synced {
                        let _ = s.exec_all_on_open_page(vec![
                            Command::ReplaceText {
                                id: BlockId(id as u64),
                                text: String::new(),
                            },
                            Command::SetBlockType {
                                id: BlockId(id as u64),
                                kind: new_kind,
                            },
                        ]);
                        s.open_slash_block("");
                        g.set_slash_x(g.get_block_menu_x());
                        g.set_slash_y(g.get_block_menu_y());
                        g.set_slash_filter("".into());
                        g.set_slash_focus(0);
                        g.set_slash_pick_synced(true);
                        g.set_slash_open(true);
                        // the picker's "apply" reads `editing-id`, exactly as
                        // it does when the "/" path opened the same list
                        g.set_editing_id(id);
                        return;
                    }
                    // SPEC §三十九: Turn-into reaches the same one-batch path
                    // the "/" and "+" menus do. `SetBlockType` refuses a
                    // Database kind — the entity, its title column and its
                    // first view are not the plan layer's to allocate — so
                    // `make_database` is the only door, and the line's words
                    // leave with it (one Ctrl+Z restores the line whole).
                    if new_kind == crate::core::BlockKind::Database {
                        s.make_database(id);
                        if g.get_editing_id() == id {
                            g.set_editing_text("".into());
                            g.set_editing_id(-1);
                        }
                        return;
                    }
                    // A picture is not a style: it takes a file first, and the
                    // block only converts once the user has chosen one.
                    if matches!(
                        new_kind,
                        crate::core::BlockKind::Image | crate::core::BlockKind::File
                    ) {
                        pick_attachment(&g, &s, id, true, new_kind);
                        return;
                    }
                    // the reference goes only when the kind actually changed:
                    // a refused Turn into (SPEC §三十八) would otherwise strip
                    // the pointer out of a row that still says it is a Page
                    if s.exec_on_open_page(Command::SetBlockType {
                        id: BlockId(id as u64),
                        kind: new_kind,
                    })
                    .is_some()
                        && matches!(
                            old_kind,
                            Some(crate::core::BlockKind::Page)
                                | Some(crate::core::BlockKind::Link)
                        )
                    {
                        s.clear_block_ref(id);
                    }
                }
                _ => {}
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_block_plus(move |id, row_bottom, content_x| {
            let g = gw.upgrade().unwrap();
            if id <= 0 {
                return;
            }
            let changes = s.exec_on_open_page(Command::InsertBlockAfter {
                id: BlockId(id as u64),
                kind: crate::core::BlockKind::Paragraph,
                text: String::new(),
            });
            if let Some(nid) = changes.as_deref().and_then(find_inserted_id) {
                focus_block(&g, &s, nid, 0);
                // Notion's "+": the new empty line gains the insert menu.
                // Picking a row types the block; clicking away (or Escape)
                // keeps the empty paragraph, exactly like Notion.
                s.open_slash_insert("");
                let row_h = if g.get_touch_mode() { 44.0 } else { 32.0 };
                let count = g.get_slash_items().row_count() as f32;
                let menu_h = count * row_h + 8.0;
                let scroll = g.get_editor_scroll_y();
                let edge = content_edge_offset(&g);
                let y = (40.0 + row_bottom as f32 - scroll + 4.0)
                    .clamp(48.0, (g.get_window_h() - menu_h - 8.0).max(48.0));
                g.set_slash_x(edge + content_x as f32);
                g.set_slash_y(y);
                g.set_slash_filter("".into());
                g.set_slash_focus(0);
                g.set_slash_insert(true);
                g.set_slash_open(true);
            }
        });
    }

    // The touch bar's "+": the desktop "+" handle's insert affordance, moved
    // to the one row a thumb always reaches. It inserts an empty paragraph
    // after the page's last block — or makes the page's first one through the
    // empty state's own door — and opens the insert menu over the new row.
    // The menu anchors above the bar, clamped like every popup anchor.
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_insert_at_end_requested(move || {
                let g = gw.upgrade().unwrap();
                if s.page_locked() {
                    s.note_locked();
                    return;
                }
                let page = core_page_id(s.open_page.get());
                let open_menu = |g: &UIState<'_>| {
                    s.open_slash_insert("");
                    let row_h = 44.0_f32;
                    let count = g.get_slash_items().row_count() as f32;
                    let menu_h = count * row_h + 8.0;
                    let edge = content_edge_offset(&g);
                    let bar = if g.get_touch_mode() { 56.0 } else { 0.0 };
                    let y = (g.get_window_h() - bar - menu_h - 8.0).max(48.0);
                    g.set_slash_x(edge + 16.0);
                    g.set_slash_y(y);
                    g.set_slash_filter("".into());
                    g.set_slash_focus(0);
                    g.set_slash_insert(true);
                    g.set_slash_open(true);
                };
                let last = {
                    let d = s.doc.borrow();
                    d.page_blocks(page).last().map(|b| b.id.0 as i32)
                };
                match last {
                    Some(id) => {
                        let changes = s.exec_on_open_page(Command::InsertBlockAfter {
                            id: BlockId(id as u64),
                            kind: crate::core::BlockKind::Paragraph,
                            text: String::new(),
                        });
                        if let Some(nid) = changes.as_deref().and_then(find_inserted_id) {
                            focus_block(&g, &s, nid, 0);
                            open_menu(&g);
                        }
                    }
                    None => {
                        // the empty state makes the first paragraph and puts
                        // the caret in it; the menu then opens over that row
                        g.invoke_empty_page_started();
                        let made = {
                            let d = s.doc.borrow();
                            d.page_blocks(page).last().map(|b| b.id.0 as i32)
                        };
                        if let Some(nid) = made {
                            focus_block(&g, &s, nid, 0);
                        }
                        open_menu(&g);
                    }
                }
            });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>()
            .on_toggle_mark(move |kind, start, end| {
                let g = gw.upgrade().unwrap();
                flush_pending_edit(&g, &s);
                let cur = g.get_editing_id();
                if cur <= 0 {
                    return;
                }
                let kind = match kind {
                    0 => Some(crate::core::MarkKind::Bold),
                    1 => Some(crate::core::MarkKind::Italic),
                    2 => Some(crate::core::MarkKind::Strike),
                    3 => Some(crate::core::MarkKind::Code),
                    4 => Some(crate::core::MarkKind::Math),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let _ = s.exec_on_open_page(Command::ToggleMark {
                        id: BlockId(cur as u64),
                        start: start.min(end).max(0) as usize,
                        end: end.max(start).max(0) as usize,
                        kind,
                        url: String::new(),
                        date: None,
                    });
                }
            });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_open_link(move |url| {
            let g = gw.upgrade().unwrap();
            let url = url.to_string();
            if url.is_empty() {
                return;
            }
            // internal anchors first: a block link jumps to the block (its
            // page opens when it isn't the current one); a page link opens
            // the page. Unknown quire URLs are swallowed, not shelled out.
            if let Some(rest) = url.strip_prefix("quire://block/") {
                if let Ok(bid) = rest.parse::<u64>() {
                    jump_to_block(&g, &s, bid);
                }
                return;
            }
            if let Some(rest) = url.strip_prefix("quire://page/") {
                if let Ok(pid) = rest.parse::<i32>() {
                    open(&g, &s, pid);
                }
                return;
            }
            // Windows shell open; cfg-gated so other targets simply no-op.
            // Only an address a browser understands gets this far: a link's
            // target is data that arrives from a file, and `start` will run a
            // path as readily as it opens a url — so a local path, a share or
            // an unknown protocol is the document's own business, not the
            // command line's. (SPEC §三十七 批次 C, ADR-0040; the embed card
            // is what made this worth fixing, since it puts a button on it.)
            if !crate::core::embed::is_openable(&url) {
                return;
            }
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", &url])
                    .spawn();
            }
            #[cfg(not(target_os = "windows"))]
            {
                eprintln!("quire: open {url} (not supported on this platform yet)");
            }
        });
    }

    {
        let gw = gw.clone();
        ui.global::<UIState>().on_close_db_notice(move || {
            let g = gw.upgrade().unwrap();
            g.set_db_notice("".into());
        });
    }

    // ---- in-page find (Ctrl+F) ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_find_start(move || {
            let g = gw.upgrade().unwrap();
            let term = g.get_find_term().to_string();
            s.find_start(&term);
            g.set_find_label(s.find_label().into());
            // jump to the first hit right away: a "0 / N" position selects
            // nothing, which read as a broken find in the A4 sweep
            if s.find_label().starts_with("0 / ") {
                flush_pending_edit(&g, &s);
                if let Some((bid, start, end)) = s.find_step(true) {
                    apply_find_hit(&g, &s, bid, start, end);
                }
                g.set_find_label(s.find_label().into());
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_find_step(move |delta| {
            let g = gw.upgrade().unwrap();
            if !g.get_find_open() {
                return;
            }
            flush_pending_edit(&g, &s);
            if let Some((bid, start, end)) = s.find_step(delta < 0) {
                apply_find_hit(&g, &s, bid, start, end);
            } else {
                g.set_find_label(s.find_label().into());
            }
        });
    }

    {
        let s = state.clone();
        ui.global::<UIState>().on_find_close(move || {
            s.find_close();
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_duplicate_block(move |id| {
            let g = gw.upgrade().unwrap();
            if id <= 0 {
                return;
            }
            flush_pending_edit(&g, &s);
            if let Some(nid) = s
                .exec_on_open_page(Command::DuplicateBlock { id: BlockId(id as u64) })
                .as_deref()
                .and_then(find_inserted_id)
            {
                focus_block(&g, &s, nid, 0);
            }
        });
    }

    // ---- list nesting (Tab / Shift+Tab) ----
    {
        let gw = gw.clone();
        let s = state.clone();
        let nest = move |g: &UIState<'_>, s: &Rc<AppState>, id: i32, indent: bool| {
            flush_pending_edit(g, s);
            let cmd = if indent {
                Command::IndentList { id: BlockId(id as u64) }
            } else {
                Command::OutdentList { id: BlockId(id as u64) }
            };
            if s.exec_on_open_page(cmd).is_some() {
                refresh_focused_text(g, s);
            }
        };
        ui.global::<UIState>().on_indent_list(move |id| {
            let g = gw.upgrade().unwrap();
            nest(&g, &s, id, true);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_outdent_list(move |id| {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            if s.exec_on_open_page(Command::OutdentList { id: BlockId(id as u64) }).is_some() {
                refresh_focused_text(&g, &s);
            }
        });
    }

    {
        let s = state.clone();
        ui.global::<UIState>().on_toggle_lan_sharing(move || {
            let on = !s.setting_flag("lan.share");
            s.record_setting("lan.share", if on { "1" } else { "0" });
        });
    }

    // ---- page title in-place editing ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_title_commit(move |text| {
            let g = gw.upgrade().unwrap();
            let title = text.trim().to_string();
            let cur = s.open_page.get();
            g.set_title_editing(false);
            if cur <= 0 {
                return;
            }
            if title.is_empty() {
                // empty title reverts to the stored one
                let t = s.workspace.borrow().title_of(cur).unwrap_or("").to_string();
                g.set_page_title(t.into());
                return;
            }
            s.rename_page(cur, &title);
            g.set_page_title(title.into());
            // Enter from the title of an empty page lands the caret in the
            // body — which on such a page means making it exist first.
            if s.blocks.row_count() == 0 {
                if let Some(id) = s.start_page() {
                    focus_block(&g, &s, id, 0);
                }
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_empty_page_started(move || {
            let g = gw.upgrade().unwrap();
            if let Some(id) = s.start_page() {
                focus_block(&g, &s, id, 0);
            }
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_title_cancel(move || {
            let g = gw.upgrade().unwrap();
            g.set_title_editing(false);
            let cur = s.open_page.get();
            let t = s.workspace.borrow().title_of(cur).unwrap_or("").to_string();
            g.set_page_title(t.into());
        });
    }

    // A locked page answers the attempt (SPEC §三十八: 不能静默吞输入). The
    // .slint side knows which clicks those are; the wording lives here, with
    // every other refusal.
    {
        let s = state.clone();
        ui.global::<UIState>().on_lock_nudge(move || {
            s.note_locked();
        });
    }

    // ---- link dialog (M6) ----
    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_link_apply(move || {
            let g = gw.upgrade().unwrap();
            let mut url = g.get_link_url().to_string();
            g.set_link_open(false);
            let cur = g.get_editing_id();
            if cur <= 0 {
                return;
            }
            // bare domains get an https scheme so the browser opens them
            let trimmed = url.trim();
            if !trimmed.is_empty()
                && !trimmed.contains("://")
                && !trimmed.starts_with("mailto:")
            {
                url = format!("https://{trimmed}");
            }
            let _ = s.exec_on_open_page(Command::ToggleMark {
                id: BlockId(cur as u64),
                start: g.get_link_start().max(0) as usize,
                end: g.get_link_end().max(0) as usize,
                kind: crate::core::MarkKind::Link,
                url,
                date: None,
            });
            refresh_focused_text(&g, &s);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_link_remove(move || {
            let g = gw.upgrade().unwrap();
            g.set_link_open(false);
            let cur = g.get_editing_id();
            if cur <= 0 {
                return;
            }
            // a covering Link mark is toggled off by the same range
            let _ = s.exec_on_open_page(Command::ToggleMark {
                id: BlockId(cur as u64),
                start: g.get_link_start().max(0) as usize,
                end: g.get_link_end().max(0) as usize,
                kind: crate::core::MarkKind::Link,
                url: String::new(),
                date: None,
            });
            refresh_focused_text(&g, &s);
        });
    }

    {
        let gw = gw.clone();
        ui.global::<UIState>().on_link_cancel(move || {
            let g = gw.upgrade().unwrap();
            g.set_link_open(false);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_undo_requested(move || {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            s.undo_open_page();
            refresh_focused_text(&g, &s);
        });
    }

    {
        let gw = gw.clone();
        let s = state.clone();
        ui.global::<UIState>().on_redo_requested(move || {
            let g = gw.upgrade().unwrap();
            flush_pending_edit(&g, &s);
            s.redo_open_page();
            refresh_focused_text(&g, &s);
        });
    }
}

/// Arm the one-shot poll timer for an in-flight async search. Each poll
/// that comes back empty re-arms; a landed result stops the cycle.
fn poll_arm(gw: &slint::Weak<UIState<'static>>, s: &Rc<AppState>) {
    if !s.search_in_flight() {
        return;
    }
    let t: &'static slint::Timer = Box::leak(Box::new(slint::Timer::default()));
    let gw = gw.clone();
    let s = s.clone();
    t.start(
        slint::TimerMode::SingleShot,
        std::time::Duration::from_millis(30),
        move || {
            let g = gw.upgrade().unwrap();
            if let Some(rows) = s.poll_search() {
                s.search.set_vec(rows);
                g.set_search_focus(0);
            } else if s.search_in_flight() {
                poll_arm(&gw, &s);
            }
        },
    );
}

/// Restart the 300 ms typing-commit timer. A prose row arms it after its
/// markdown and slash work; a table cell arms it alone, since a cell is a grid
/// slot rather than a line.
fn debounce_arm(
    t: &'static slint::Timer,
    gw: &slint::Weak<UIState<'static>>,
    s: &Rc<AppState>,
) {
    let gw = gw.clone();
    let s = s.clone();
    t.start(
        slint::TimerMode::SingleShot,
        std::time::Duration::from_millis(300),
        move || {
            if let Some(g) = gw.upgrade() {
                flush_pending_edit(&g, &s);
            }
        },
    );
}

/// The database cell's debounced commit (SPEC §三十九). The same 300 ms rhythm
/// the prose rows use, but the ids it reads back are the cell's —
/// (block, record, property), never `editing-id`, which is `-1` while a cell
/// is live by the one-input discipline. A parse the column's kind refuses
/// (`db_set_cell_text` returns `false`) keeps the editor open with the typed
/// text: the cell's stored value is untouched, and closing the editor is what
/// repaints it. A commit that lands re-reads the window in place.
fn db_debounce_arm(
    t: &'static slint::Timer,
    gw: &slint::Weak<UIState<'static>>,
    s: &Rc<AppState>,
) {
    let gw = gw.clone();
    let s = s.clone();
    t.start(
        slint::TimerMode::SingleShot,
        std::time::Duration::from_millis(300),
        move || {
            if let Some(g) = gw.upgrade() {
                let record = g.get_db_editing_record();
                let property = g.get_db_editing_property();
                // not a live cell, or the block editor took the keyboard back:
                // neither state belongs to this commit
                if record < 0 || property < 0 || g.get_editing_id() >= 0 {
                    return;
                }
                let block = g.get_db_editing_block();
                if block < 0 {
                    return;
                }
                let text = g.get_editing_text().to_string();
                let _ = s.db_set_cell_text(block, record as i64, property, &text);
            }
        },
    );
}

/// Commit the live database cell synchronously — the `db-cell-closed` path
/// (Enter / Escape / Tab). The debounce cannot be trusted here: closing resets
/// `db-editing-record`/`-property`, and the debounced commit reads those back,
/// so a text typed in the last 300 ms would be silently dropped. A refused
/// parse closes anyway: the editor going away is what repaints the stored
/// value, which is the honest answer to "that did not take".
fn db_commit_cell(g: &UIState<'_>, state: &Rc<AppState>) {
    let record = g.get_db_editing_record();
    let property = g.get_db_editing_property();
    if record < 0 || property < 0 || g.get_db_editing_block() < 0 {
        return;
    }
    let block = g.get_db_editing_block();
    let text = g.get_editing_text().to_string();
    let _ = state.db_set_cell_text(block, record as i64, property, &text);
}

/// Re-fill one projected block row's §三十九 fields in place — the one-row
/// version of the page projection's `db_fill_row` pass. A window read mutates
/// the rows model in place, but the *shape* the delegate lays out (row count,
/// window start, columns, tabs) rides on the `BlockRow`, and every structural
/// database change (row added or removed, column toggled or resized, view
/// picked) moves that shape. Walking the model here keeps the cost one row
/// instead of one page — the same reasoning the projection's targeted patch
/// (`set_row_data` on the todo toggle) already follows.
fn db_refill_row(state: &Rc<AppState>, block: i32) {
    let mut i = 0;
    while let Some(mut row) = state.blocks.row_data(i) {
        if row.id == block {
            state.db_fill_row(&mut row);
            state.blocks.set_row_data(i, row);
            return;
        }
        i += 1;
    }
}

/// (Re)build the columns popup's rows from the view's own document. Two
/// callers, one truth: opening the popup, and the moment after a toggle — the
/// popup stays open across the click, so its rows are stale the instant the
/// toggle lands, and the same builder is what un-stales them.
fn db_push_columns(g: &UIState<'_>, state: &Rc<AppState>, block: i32) {
    let rows: Vec<crate::DbColumnToggle> = state
        .db_column_toggles(block)
        .into_iter()
        .map(|t| crate::DbColumnToggle {
            property: t.property,
            name: t.name.into(),
            kind: t.kind.into(),
            visible: t.visible,
            locked: t.locked,
        })
        .collect();
    g.set_db_columns_rows(Rc::new(slint::VecModel::from(rows)).into());
}

/// Push the filter panel's whole state (D4): the rule rows, the root's
/// flavour, and the column chooser's list. Called on open and after every
/// accepted edit, so the panel a user is looking at is always the document's
/// current rules — the model is derived, never a second copy of the filter.
fn db_push_filter(g: &UIState<'_>, state: &Rc<AppState>, block: i32) {
    let (any, rows) = state.db_filter_panel(block);
    g.set_db_filter_match_any(any);
    let rows: Vec<crate::DbFilterRow> = rows
        .into_iter()
        .map(|r| crate::DbFilterRow {
            property: r.property,
            name: r.name.into(),
            kind: r.kind,
            op: r.op,
            op_name: r.op_name.into(),
            value: r.value.into(),
            has_value: r.has_value,
            invert: r.invert,
        })
        .collect();
    g.set_db_filter_rows(Rc::new(slint::VecModel::from(rows)).into());
    // The chooser lists every property of the database (the columns popup's
    // own model shape, so a name and a type word are already what it draws).
    let choices: Vec<crate::DbColumnToggle> = state
        .db_column_toggles(block)
        .into_iter()
        .map(|t| crate::DbColumnToggle {
            property: t.property,
            name: t.name.into(),
            kind: t.kind.into(),
            visible: t.visible,
            locked: t.locked,
        })
        .collect();
    g.set_db_filter_columns(Rc::new(slint::VecModel::from(choices)).into());
}

/// Push the comparison chooser for one clause: the comparisons its column's
/// kind has — the same list the parser accepts, so nothing the menu offers can
/// be refused later. The panel switches to state 2.
fn db_push_filter_ops(g: &UIState<'_>, state: &Rc<AppState>, block: i32, index: usize) {
    let rows = state.db_filter_panel(block).1;
    let Some(row) = rows.get(index) else {
        return;
    };
    let ops: Vec<crate::DbFilterOp> = state
        .db_ops_for_kind(row.kind)
        .into_iter()
        .map(|(op, name)| crate::DbFilterOp {
            op,
            name: name.into(),
        })
        .collect();
    g.set_db_filter_ops(Rc::new(slint::VecModel::from(ops)).into());
    g.set_db_filter_edit_row(index as i32);
    g.set_db_filter_panel(2);
}

/// Push the value chooser for one clause: the column's options (an id and the
/// name the config gives it). The panel switches to state 3.
fn db_push_filter_options(g: &UIState<'_>, state: &Rc<AppState>, block: i32, index: usize) {
    let rows = state.db_filter_panel(block).1;
    let Some(row) = rows.get(index) else {
        return;
    };
    let options: Vec<crate::DbOption> = state
        .db_property_options(row.property)
        .into_iter()
        .map(|(id, name, color)| crate::DbOption {
            id: id.into(),
            name: name.into(),
            color: color.into(),
        })
        .collect();
    g.set_db_filter_options(Rc::new(slint::VecModel::from(options)).into());
    g.set_db_filter_edit_row(index as i32);
    g.set_db_filter_panel(3);
}

/// Push the group picker's rows (D4): the option-bounded columns, plus the
/// active one so a row can mark itself.
fn db_push_group(g: &UIState<'_>, state: &Rc<AppState>, block: i32) {
    let rows: Vec<crate::DbColumnToggle> = state
        .db_group_choices(block)
        .into_iter()
        .map(|t| crate::DbColumnToggle {
            property: t.property,
            name: t.name.into(),
            kind: t.kind.into(),
            visible: t.visible,
            locked: t.locked,
        })
        .collect();
    g.set_db_group_rows(Rc::new(slint::VecModel::from(rows)).into());
    g.set_db_group_current(state.db_group_current(block));
}

/// Open the formula editor for one column (D6), with the draft, the live
/// answer and the error line all seeded from the **stored** expression
/// evaluated on `record` — the row whose cell was clicked, or -1 for an empty
/// row when the column has just been created and has nothing to compute on.
fn db_show_formula(
    g: &UIState<'_>,
    state: &Rc<AppState>,
    block: i32,
    record: i32,
    property: i32,
) {
    let text = state.db_formula_current(property);
    let (preview, error) = state.db_formula_preview(record as i64, property, &text);
    g.set_db_formula_block(block);
    g.set_db_formula_property(property);
    g.set_db_formula_record(record);
    g.set_db_formula_name(state.db_property_label(property).into());
    g.set_db_formula_text(text.into());
    g.set_db_formula_preview(preview.into());
    g.set_db_formula_error(error.into());
    g.set_db_formula_open(true);
}

/// How many rows the relation picker lists at once. The cap is the feature's
/// shape rather than a performance tweak — `records_named` is capped for the
/// reason `database::window` exists — and the needle is what a user past the
/// cap has: they narrow the list, they do not scroll a thousand titles.
const DB_RELATION_PICK_LIMIT: usize = 10;

/// One chooser list as the UI reads it (D10). The type menu, the relation
/// picker and the rollup editor all draw `DbPickRow`s, and Rust has already
/// decided every field of one — including whether the row can be picked and
/// what sentence says why not — so this is a copy, not a conversion.
fn db_pick_model(rows: Vec<crate::app::state::DbPickRow>) -> slint::ModelRc<crate::DbPickRow> {
    let rows: Vec<crate::DbPickRow> = rows
        .into_iter()
        .map(|r| crate::DbPickRow {
            id: r.id,
            name: r.name.into(),
            chosen: r.chosen,
            disabled: r.disabled,
            note: r.note.into(),
        })
        .collect();
    Rc::new(slint::VecModel::from(rows)).into()
}

/// Open the Columns popup's second panel: the type menu. `property < 0` is the
/// **creation** list — what a new column can start as — which is why the panel
/// is the same one for both questions: a schema has one set of kinds, and a
/// menu per question would be two menus agreeing about it.
fn db_push_kinds(g: &UIState<'_>, state: &Rc<AppState>, block: i32, property: i32) {
    // The delegate's own block is the authority for which panel is being
    // filled, so it travels with the call rather than being read back.
    g.set_db_columns_block(block);
    g.set_db_kind_rows(db_pick_model(state.db_column_kinds(property)));
    g.set_db_columns_property(property);
    let title = if property < 0 {
        String::new()
    } else {
        state.db_property_label(property)
    };
    g.set_db_columns_title(title.into());
    g.set_db_columns_panel(1);
}

/// Home from the type menu to the visibility list, with the rows re-pushed:
/// the popup's own content is stale the instant the panel changes, and a kind
/// that moved while it was open is exactly what the list has to say now.
fn db_show_columns(g: &UIState<'_>, state: &Rc<AppState>, block: i32) {
    db_push_columns(g, state, block);
    g.set_db_columns_property(-1);
    g.set_db_columns_title("".into());
    g.set_db_columns_panel(0);
}

/// Push one panel of the relation editor (D10, ADR-0088). The three lists are
/// one function because they are one question in three tenses — what does this
/// point at, what points back at it, what does this row hold — and because
/// every one of them is read back out of the **column's config**, not out of
/// what the user last clicked: a pick that was refused leaves the popup
/// showing what the column still is.
fn db_push_relation(g: &UIState<'_>, state: &Rc<AppState>, panel: i32) {
    let block = g.get_db_relation_block();
    let property = g.get_db_relation_property();
    let target = state.db_relation_config(property).0;
    let rows = match panel {
        1 => state.db_relation_databases(block, target),
        2 => state.db_relation_mirrors(block, property, target),
        _ => state.db_relation_rows(
            property,
            g.get_db_relation_record() as i64,
            &g.get_db_relation_text(),
            DB_RELATION_PICK_LIMIT,
        ),
    };
    g.set_db_relation_rows(db_pick_model(rows));
    g.set_db_relation_panel(panel);
}

/// Push one panel of the rollup editor (D10, ADR-0089), and with it the three
/// summary lines — the sentence the choosers edit, always from the current
/// config rather than from the click that opened a panel.
fn db_push_rollup(g: &UIState<'_>, state: &Rc<AppState>, panel: i32) {
    let block = g.get_db_rollup_block();
    let property = g.get_db_rollup_property();
    let (relation, column, aggregate) = state.db_rollup_config(property);
    let rows = match panel {
        1 => state.db_rollup_relations(block, relation),
        2 => state.db_rollup_columns(relation, column),
        3 => state.db_rollup_aggregates(aggregate),
        _ => Vec::new(),
    };
    g.set_db_rollup_rows(db_pick_model(rows));
    let (rel, col, agg) = state.db_rollup_labels(property);
    g.set_db_rollup_relation(rel.into());
    g.set_db_rollup_column(col.into());
    g.set_db_rollup_aggregate(agg.into());
    g.set_db_rollup_panel(panel);
}

/// Where the caret belongs after a row or column delete: `at` was the focused
/// cell's row-major slot, so the same slot of the smaller grid is the cell
/// nearest to it. `None` means the caret was never in this grid — the delete
/// took a row from under someone else, so it keeps its own focus.
fn table_refocus(g: &UIState<'_>, s: &Rc<AppState>, table: i32, at: Option<usize>) {
    match at.and_then(|at| s.table_cell_at(table, at)) {
        Some(cell) => focus_block(g, s, cell, i32::MAX),
        None => refresh_focused_text(g, s),
    }
}

/// Adding or removing a box rebuilds the layout's row, and the item's input
/// is created with the focus — so it would come back with the caret at the
/// start of the line. Hand it back where the reader left it.
fn columns_refocus(g: &UIState<'_>, s: &Rc<AppState>) {
    let id = g.get_editing_id();
    if id > 0 && s.is_column_item(id) {
        focus_block(g, s, id, i32::MAX);
    }
}

/// Commit the live editing text as a `ReplaceText` command (no-op when the
/// text is unchanged). Called by the debounce timer and before every
/// structural operation so undo history stays consistent.
fn flush_pending_edit(g: &UIState<'_>, state: &Rc<AppState>) {
    let editing = g.get_editing_id();
    if editing <= 0 {
        return;
    }
    let id = BlockId(editing as u64);
    let text = g.get_editing_text().to_string();
    let applied = state.exec_editor(Command::ReplaceText {
        id,
        text: text.clone(),
    });
    if applied.is_some() {
        if state.is_table_cell(editing) {
            // a cell has no row of its own: its text rides on the table's row
            state.sync_cell_text(editing, &text);
            return;
        }
        if state.is_column_item(editing) {
            // ... and so does a block inside a columns box
            state.sync_column_text(editing, &text);
            return;
        }
        // targeted row sync; no delegate rebuild
        let mut i = 0;
        while let Some(mut row) = state.blocks.row_data(i) {
            if row.id == editing {
                row.text = text.into();
                state.blocks.set_row_data(i, row);
                return;
            }
            i += 1;
        }
    }
}

/// Anchor and open the slash popup under the row being edited, clamped to the
/// window. It lives in one place because two callers now open it (the "/" and
/// the "@" triggers) and the fit-the-window rule is the one that was learned
/// the hard way: the list is as tall as its rows, so the reserve is read from
/// the *current* model — a hardcoded reserve is what pushed the last entries
/// off the bottom once before (ADR-0032's fix), and the mention picker's row
/// count changes with every keystroke of its filter.
fn open_slash_at(g: &UIState<'_>, row_y: f32, row_h: f32, content_x: f32) {
    let item_h = if g.get_touch_mode() { 44.0 } else { 32.0 };
    let menu_h = g.get_slash_items().row_count() as f32 * item_h + 8.0;
    let scroll = g.get_editor_scroll_y();
    let edge = content_edge_offset(&g);
    let y = (40.0 + row_y - scroll + row_h + 4.0)
        .clamp(48.0, (g.get_window_h() - menu_h - 8.0).max(48.0));
    g.set_slash_x(edge + content_x);
    g.set_slash_y(y);
    g.set_slash_open(true);
}

/// Jump to a block anywhere in the library: open the page it lives on, then
/// put the caret in it.
///
/// One rule for every internal anchor, because they are the same jump. A
/// `quire://block/` link in the text, a line of a `Toc` block, and a row of the
/// backlink panel all name a block id and nothing else — SPEC §四十 says the
/// panel reuses M8's anchor path, and the way not to have two of them is to
/// have the one. A block whose page is not in the workspace (deleted under a
/// reference, a library edited by hand) is a no-op, not a crash.
///
/// **Known limit, shared with every anchor in the app:** this moves the caret
/// and the selection, but it does not scroll the viewport to the row. Slint
/// 1.18's plain `ListView` has no `bring-into-view`, so a block outside the
/// window is focused out of sight — see `docs/REPORT_TRACK2.md`.
fn jump_to_block(g: &UIState<'_>, state: &Rc<AppState>, id: u64) {
    let (page, text_len) = {
        let d = state.doc.borrow();
        d.block(BlockId(id))
            .map(|b| (b.page.0 as i32, b.text.len() as i32))
            .unwrap_or((0, 0))
    };
    if page <= 0 || !state.workspace.borrow().contains(page) {
        return;
    }
    flush_pending_edit(g, state);
    open(g, state, page);
    // -1 first: recreate the delegate so the input takes over
    g.set_editing_id(-1);
    focus_block(g, state, id as u32 as i32, text_len);
}

/// Move the live editor onto another block with the caret at `caret` bytes.
fn focus_block(g: &UIState<'_>, state: &Rc<AppState>, id: i32, caret: i32) {
    let (text, len) = {
        let d = state.doc.borrow();
        d.block(BlockId(id as u64))
            .map(|b| (b.text.clone(), b.text.len()))
            .unwrap_or_default()
    };
    g.set_editing_text(text.into());
    g.set_pending_caret(caret.clamp(0, len as i32));
    g.set_editing_id(id);
}

/// After undo/redo: keep the editor on its block if it still exists.
fn refresh_focused_text(g: &UIState<'_>, state: &Rc<AppState>) {
    let cur = g.get_editing_id();
    if cur <= 0 {
        return;
    }
    let text = {
        let d = state.doc.borrow();
        d.block(BlockId(cur as u64)).map(|b| b.text.clone())
    };
    match text {
        Some(t) => {
            g.set_editing_text(t.into());
            g.set_pending_caret(-1);
        }
        None => g.set_editing_id(-1),
    }
}

/// Import every (title, markdown) pair from a LAN pull as new pages.
/// Returns the number of pages imported.
pub fn import_lan_pages(
    g: &UIState<'_>,
    state: &Rc<AppState>,
    url: &str,
    pages: Vec<(String, String)>,
) -> usize {
    let mut imported = 0;
    for (title, md) in pages {
        let safe_title = if title.trim().is_empty() {
            "已导入".to_string()
        } else {
            title.trim().to_string()
        };
        let new_id = state.create_page(None);
        let core_page = crate::core::Page {
            id: crate::core::PageId(new_id as u32 as u64),
            title: safe_title.clone(),
            parent: None,
            order: state.page_order_of(new_id),
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
        let changes = {
            let mut doc = state.doc.borrow_mut();
            let mut alloc = || doc.alloc_block_id();
            crate::services::import_service::import_markdown(&md, &core_page, &mut alloc)
        };
        let rest = changes.into_iter().skip(1).collect::<Vec<_>>();
        {
            let mut d = state.doc.borrow_mut();
            d.apply(&rest);
        }
        state.record(rest);
        state.rename_page(new_id, &safe_title);
        imported += 1;
    }
    if imported > 0 {
        g.set_db_notice(format!("已从 {url} 导入 {imported} 个页面").into());
        open(g, state, state.open_page.get());
        state.reproject_blocks();
    }
    imported
}

/// One picture round-trip: the native file dialog, then store the bytes beside
/// the database and put the block in the page. `convert` picks between the two
/// image commands — this block becomes the picture, or a picture block appears
/// below it.
///
/// The dialog blocks, deliberately, exactly like the import/export buttons do:
/// it runs on the click that opened it, before the editor takes another event,
/// so there is no half-inserted state to reconcile. A cancelled dialog changes
/// nothing; a failed import says why in the notice bar.
fn pick_attachment(
    g: &UIState<'_>,
    s: &Rc<AppState>,
    anchor: i32,
    convert: bool,
    kind: crate::core::BlockKind,
) {
    let pictures = kind == crate::core::BlockKind::Image;
    let picker = crate::platform::picker::Picker::open().title(if pictures {
        "插入图片"
    } else {
        "附加文件"
    });
    // The picture picker constrains the choice, because the decoder reads four
    // formats — png, jpeg, bmp, gif — and nothing else. The file one must not:
    // naming any type is the whole point of the kind.
    let picker = if pictures {
        picker.filter("图片", PICTURE_EXTS)
    } else {
        picker.filter("所有文件", &["*"])
    };
    let Some(path) = ask(g, picker) else {
        return;
    };
    let id = s.claim_attachment_id();
    let imported = if pictures {
        s.store.import_file(id, &path)
    } else {
        s.store.import_any_file(id, &path)
    };
    let placed = match imported {
        Ok(att) => {
            if convert {
                s.set_block_attachment(anchor, att, kind)
            } else {
                s.insert_attachment(anchor, att, kind)
            }
        }
        Err(e) => {
            g.set_db_notice(e.to_string().into());
            return;
        }
    };
    if !placed {
        g.set_db_notice(
            if pictures {
                "该图片无法放入页面。"
            } else {
                "该文件无法放入页面。"
            }
            .into(),
        );
    }
}

/// The cover picker (SPEC §三十八 "图标与封面"). Same picture filter as an image
/// row — the decoder reads four formats and nothing else — and the same import,
/// because a cover is not a second kind of file: it is one more thing that
/// points at the attachment table.
fn pick_cover(g: &UIState<'_>, s: &Rc<AppState>, page: i32) {
    let Some(path) = ask(
        g,
        crate::platform::picker::Picker::open()
            .title("选择封面")
            .filter("图片", PICTURE_EXTS),
    ) else {
        return;
    };
    let id = s.claim_attachment_id();
    match s.store.import_file(id, &path) {
        Ok(att) => s.set_page_cover_from(page, att),
        Err(e) => g.set_db_notice(e.to_string().into()),
    }
}

/// Hand an attachment's stored bytes to whatever the system opens this type
/// with, or copy them out to a path the user picks. Both read the `attachments`
/// row rather than the block, so the name and the extension agree (SPEC
/// §三十七 批次 A).
fn attachment_action(g: &UIState<'_>, s: &Rc<AppState>, id: i32, save: bool) {
    let Some(att) = s.attachment_row(id) else {
        g.set_db_notice("该附件属于另一个库。".into());
        return;
    };
    let label = att.name.clone();
    let path = s.store.stored_path(&att);
    if save {
        let Some(target) = ask(
            g,
            crate::platform::picker::Picker::save()
                .title("另存附件为")
                .name(&s.store.save_name(&att)),
        ) else {
            return;
        };
        let notice = match s.store.export_to(&att, &target) {
            Ok(()) => format!("已保存 {label}。"),
            Err(e) => e.to_string(),
        };
        g.set_db_notice(notice.into());
        return;
    }
    if !path.is_file() {
        g.set_db_notice(format!("{label}：存储的文件已丢失。").into());
        return;
    }
    if !crate::platform::open_with_default(&path) {
        g.set_db_notice(format!("没有程序可以打开 {label}。").into());
    }
}

/// Export the open page's blocks to a .md file via the native save dialog.
/// "Copy Page as Markdown" (palette): the page through the exporter onto
/// the clipboard. The FFI write path is mandatory here — a markdown page
/// routinely carries CJK, which clip.exe's OEM stdin garbles (ADR-0025).
fn copy_current_page_markdown(g: &UIState<'_>, s: &Rc<AppState>) {
    let page = s.open_page.get();
    let md = {
        let d = s.doc.borrow();
        // the live titles go with it: a mention chip shows the target page's
        // *current* name, and the file the writer copies has to agree with the
        // screen or a rename would be silently undone in the export
        let ws = s.workspace.borrow();
        crate::services::export_service::export_page_full(
            d.page_blocks(core_page_id(page)),
            &|id| ws.title_of(id.as_u64() as i32).map(str::to_string),
            // ADR-0065: the caller that can read the database pre-renders the
            // table (current view → GFM); a clipboard copy is that caller.
            &|id| s.db_markdown_table(id.as_u64() as i32),
            // §四十 / ADR-0052 §7: a mirror leaves as the sentence it was a
            // second view of. `sync_target` is the same walk the row uses, so
            // the file agrees with the screen instead of flattening to a blank.
            &|id| {
                crate::app::state::sync_target_for_export(&d, id)
            },
        )
    };
    let notice = if md.trim().is_empty() {
        "此页面还没有可复制的内容。".to_string()
    } else if crate::platform::copy_to_clipboard(&md) {
        "已复制页面为 Markdown。".to_string()
    } else {
        "无法访问剪贴板。".to_string()
    };
    g.set_db_notice(notice.into());
}

/// Re-measure the page menu's popup after its rows were swapped for a taller
/// submenu (Move-to, Style, Templates). Every 28 px row counts, and the anchor
/// is the top of the popup, so a list that outgrows the space below the ⋯
/// button has to move up or its last rows are unreachable.
fn reanchor_page_menu(g: &UIState<'_>) {
    // 30 px rows + 8 px padding is what `ContextMenu` itself draws and clamps
    // against; this used to say 28 + 16, which under-estimates by
    // `2 * rows - 8` — 40 px on a 24-row Move-to list, so the popup was told it
    // had more room than it has and had to shrink and scroll for no reason.
    // The two numbers are now the same number. (Touch rows stand at 44 dp —
    // the same number the popup clamps against there.)
    let row_h = if g.get_touch_mode() { 44.0 } else { 30.0 };
    let menu_h = g.get_menu_rows().row_count() as f32 * row_h + 8.0;
    let y = g
        .get_menu_y()
        .clamp(48.0, (g.get_window_h() - menu_h - 8.0).max(48.0));
    g.set_menu_y(y);
}

/// ⋯ → Version history, and the same three lines a sweep scene needs to reach
/// the panel's list view.
///
/// The panel is centred rather than anchored where the menu was: it is 460px
/// wide and holds an input, and the page menu opens from the top-right ⋯ as
/// often as from a tree row, so an anchored panel would sit half off the window
/// for one of the two. The page it belongs to travels on `versions-page`,
/// because the panel outlives the menu that opened it and a restore has to know
/// which page to replace.
fn open_version_panel(g: &UIState<'_>, state: &Rc<AppState>, page: i32) {
    state.fill_versions(page);
    g.set_versions_page(page);
    let title = state
        .workspace
        .borrow()
        .title_of(page)
        .unwrap_or("无标题")
        .to_string();
    g.set_versions_title(title.into());
    g.set_versions_showing_diff(false);
    g.set_versions_selected(-1);
    g.set_version_name("".into());
    g.set_versions_open(true);
}

/// Write a template out through §二十六's channel: the same `export_page` the
/// page export calls, on the template's block sequence. `g` is only here so the
/// failure is something the user sees rather than a line in a console nobody
/// has open.
fn export_template_markdown(
    g: &UIState<'_>,
    state: &Rc<AppState>,
    template: i32,
    name: &str,
) {
    let Some(md) = state.template_markdown(template) else {
        g.set_db_notice("该行已不再是模板。".into());
        return;
    };
    if let Some(path) = ask(
        g,
        crate::platform::picker::Picker::save()
            .filter("Markdown", &["md"])
            .name(&format!("{name}.md")),
    ) {
        match std::fs::write(&path, md) {
            Ok(()) => g.set_db_notice(format!("已导出“{name}”到 {}", path.display()).into()),
            Err(e) => g.set_db_notice(format!("导出失败：{e}").into()),
        }
    }
}

/// Pull a .md file into the library as a template. Separate from
/// `import_markdown_dialog` on purpose: that one makes a *page* and opens it,
/// and a template must not be openable — sharing the function would mean one of
/// the two doors is wrong.
fn import_template_dialog(g: &UIState<'_>, state: &Rc<AppState>) {
    let Some(path) = ask(
        g,
        crate::platform::picker::Picker::open().filter("Markdown", &["md"]),
    ) else {
        return;
    };
    let Ok(src) = std::fs::read_to_string(&path) else {
        g.set_db_notice(format!("无法读取 {}", path.display()).into());
        return;
    };
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "导入的模板".into());
    let id = state.import_template(&name, &src);
    // The count is the fact worth reporting: a Markdown file that parses to no
    // blocks lands as an empty template, and the menu would offer it forever.
    let blocks = {
        let d = state.doc.borrow();
        d.page_blocks(core_page_id(id)).len()
    };
    if blocks == 0 {
        g.set_db_notice(format!("已导入“{name}”为空模板。").into());
    } else {
        g.set_db_notice(
            format!("已导入“{name}”为模板（{blocks} 个块）。").into(),
        );
    }
}

fn export_current_page(g: &UIState<'_>, state: &Rc<AppState>) {
    let page = state.open_page.get();
    let title = state
        .workspace
        .borrow()
        .title_of(page)
        .unwrap_or("page")
        .to_string();
    let md = {
        let d = state.doc.borrow();
        let ws = state.workspace.borrow();
        crate::services::export_service::export_page_full(
            d.page_blocks(core_page_id(page)),
            &|id| ws.title_of(id.as_u64() as i32).map(str::to_string),
            // ADR-0065: the caller that can read the database pre-renders the
            // table (current view → GFM); the .md export is that caller.
            &|id| state.db_markdown_table(id.as_u64() as i32),
            &|id| {
                crate::app::state::sync_target_for_export(&d, id)
            },
        )
    };
    if let Some(path) = ask(
        g,
        crate::platform::picker::Picker::save()
            .filter("Markdown", &["md"])
            .name(&format!("{}.md", title)),
    ) {
        match std::fs::write(&path, md) {
            Ok(()) => eprintln!("quire: exported {}", path.display()),
            Err(e) => eprintln!("quire: export failed: {e}"),
        }
    }
}

/// Import a .md file as a new page via the native open dialog.
fn import_markdown_dialog(g: &UIState<'_>, state: &Rc<AppState>) {
    if let Some(path) = ask(g, crate::platform::picker::Picker::open().filter("Markdown", &["md"]))
    {
        import_from_path(g, state, &path);
    }
}

/// Import `path` as a new page, record it, and open it. Shared by the
/// palette command and the .md file-association dispatch (--open).
pub fn import_from_path(g: &UIState<'_>, state: &Rc<AppState>, path: &std::path::Path) {
    let Ok(src) = std::fs::read_to_string(path) else {
        eprintln!("quire: import failed: cannot read {}", path.display());
        return;
    };
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "已导入".into());

    let new_id = state.create_page(None);
    g.set_db_notice(format!("已导入 {} 为新页面", title).into());
    let core_page = crate::core::Page {
        id: crate::core::PageId(new_id as u32 as u64),
        title: title.clone(),
        parent: None,
        order: state.page_order_of(new_id),
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
    let changes = {
        let mut doc = state.doc.borrow_mut();
        let mut alloc = || doc.alloc_block_id();
        crate::services::import_service::import_markdown(&src, &core_page, &mut alloc)
    };
    // the service's PageCreated replaces create_page's "无标题" record
    let rest = changes.into_iter().skip(1).collect::<Vec<_>>();
    {
        let mut d = state.doc.borrow_mut();
        d.apply(&rest);
    }
    state.record(rest);
    state.rename_page(new_id, &title);
    open(g, state, new_id);
}

fn find_inserted_id(changes: &[Change]) -> Option<i32> {
    changes.iter().find_map(|c| match c {
        Change::BlockInserted(b) => Some(b.id.0 as i32),
        _ => None,
    })
}

/// Insert a fresh paragraph right after the open page's first line of prose.
///
/// The top of the page is where a chip has to be to appear in an 800 px shot of
/// it: `AppendBlock` would put it under the fold of every demo page long enough
/// to be worth photographing, and a scene whose subject is off-screen is a
/// scene that reviews clean and proves nothing.
fn insert_near_top(state: &Rc<AppState>, text: &str) -> Option<i32> {
    use crate::core::BlockKind;

    let after = {
        let d = state.doc.borrow();
        d.page_blocks(core_page_id(state.open_page.get()))
            .iter()
            .find(|b| b.kind == BlockKind::Paragraph && !b.text.is_empty())
            .map(|b| b.id.0 as i32)
    }?;
    state
        .exec_on_open_page(Command::InsertBlockAfter {
            id: BlockId(after as u64),
            kind: BlockKind::Paragraph,
            text: text.to_string(),
        })
        .as_deref()
        .and_then(find_inserted_id)
}

/// Seed a page holding a mirror of a block **on another page**, and open it —
/// the synced-block scenes (SPEC §四十, ADR-0052).
///
/// Two pages, on purpose: a mirror of a block on the same page would not show
/// the thing these shots are for, which is that the words come from somewhere
/// else. The source is a real block that really holds the sentence; the mirror
/// is a real `Synced` block whose pointer went through `set_sync_source`, so
/// the cycle check ran before anything was drawn.
///
/// `drop_source` then deletes that source, which is the second thing a reader of
/// these shots needs to see: the row **degrades** rather than disappearing, and
/// the page still opens.
fn seed_synced_page(state: &Rc<AppState>, drop_source: bool) -> Option<i32> {
    let source_page = state.create_page(None);
    state.rename_page(source_page, "Source page");
    let source = state
        .exec_on_open_page(Command::AppendBlock {
            kind: crate::core::BlockKind::Paragraph,
            text: "The words live here, and nowhere else.".to_string(),
        })
        .as_deref()
        .and_then(find_inserted_id)?;

    let home = state.create_page(None);
    state.rename_page(home, "Mirror page");
    let mirror = state
        .exec_on_open_page(Command::AppendBlock {
            kind: crate::core::BlockKind::Paragraph,
            text: String::new(),
        })
        .as_deref()
        .and_then(find_inserted_id)?;
    // the two steps the "/" path takes, in one batch each: the line becomes a
    // mirror, then the pick writes the pointer
    state.exec_on_open_page(Command::SetBlockType {
        id: BlockId(mirror as u64),
        kind: crate::core::BlockKind::Synced,
    })?;
    if !state.set_sync_source(mirror, Some(source)) {
        return None;
    }

    if drop_source {
        // the source is on another page, and `exec_on_open_page` plans against
        // the one that is showing — so the delete has to happen there
        state.open_page(source_page);
        state.exec_on_open_page(Command::DeleteBlock {
            id: BlockId(source as u64),
        })?;
    }
    state.open_page(home);
    Some(home)
}

/// Seed a page that other pages point at, and open it — the backlink panel's
/// scenes (SPEC §四十).
///
/// **The rows the panel draws come from the same query the app runs**, not from
/// a fixture: a mention's address in `marks` and a block-level reference in
/// `blocks.page_ref`, both served by migration 16's indexes. What a shot of
/// this scene shows is therefore what the reference layer answered.
///
/// The three pages are ones the demo session already has, and the opening goes
/// through the real `open()` — so the shot is a page with its own content and a
/// panel under it, not a page grown for the camera. `refs` is how many
/// references to make across two source pages: above `BACKLINK_WINDOW` is what
/// makes the folded line say something.
fn seed_referenced_page(g: &UIState<'_>, state: &Rc<AppState>, refs: usize) -> Option<i32> {
    use crate::core::BlockKind;

    let find = |title: &str| -> i32 {
        let ws = state.workspace.borrow();
        ws.dfs_order()
            .into_iter()
            .find(|id| ws.title_of(*id) == Some(title))
            // A scene that cannot find its pages must not render a plausible
            // shot of the wrong thing: a panic here leaves no BMP behind and
            // the sweep reports RENDER-FAIL, which is the failure it is.
            .unwrap_or_else(|| panic!("the demo session has no page called {title:?}"))
    };
    let target = find("Project Atlas");
    let sources = [find("Meeting Notes"), find("Reading List")];

    for (source, share) in sources
        .iter()
        .zip([refs.div_ceil(2), refs / 2])
    {
        open(g, state, *source);
        for i in 0..share {
            let text = format!("See @ for detail {i}");
            let id = state
                .exec_on_open_page(Command::AppendBlock {
                    kind: BlockKind::Paragraph,
                    text,
                })
                .as_deref()
                .and_then(find_inserted_id)?;
            // The same two steps the `@` picker takes: the trigger's `@` sits
            // at byte 4, and the atom replaces it and everything after it.
            if !state.apply_mention(id, 4, Some(target), "Project Atlas") {
                return None;
            }
        }
    }

    // The panel is a *read*, and a read sees what has been written — this is
    // the debounce being stepped over, which is the same step any scene that
    // opens a page out of the database has to take.
    state.persistence_force_flush();
    open(g, state, target);
    Some(target)
}

/// Seed the database-table scene (SPEC §三十九): one table on the demo page
/// with its title column and three more kinds the inline editors carry —
/// number, checkbox, date — and five rows of fixed values, because a shot has
/// to look the same tomorrow (the date is a literal, exactly as the `@date`
/// scene's is). Every step goes through the write paths the UI uses
/// (`make_database` → `db_add_column` → `db_add_record` → `db_set_cell_text`),
/// so the scene is a fixture of the *pipeline* and not of the model: if the
/// command layer cannot build a database, the scene must not pretend one.
///
/// A select/status column is deliberately absent: D3 has no option editor
/// (D5's), so a fresh select column has an empty option list and the shot
/// would be about "No options yet" rather than about the table.
fn seed_database_table(state: &Rc<AppState>) -> Option<i32> {
    use crate::core::database::{CellValue, PropertyKind};
    use crate::core::BlockKind;

    // the line the database grows from: an empty paragraph after the first
    // line of prose (the same anchor every near-top seed uses)
    let after = {
        let d = state.doc.borrow();
        d.page_blocks(core_page_id(state.open_page.get()))
            .iter()
            .find(|b| b.kind == BlockKind::Paragraph && !b.text.is_empty())
            .map(|b| b.id.0 as i32)
    }?;
    let id = state
        .exec_on_open_page(Command::InsertBlockAfter {
            id: BlockId(after as u64),
            kind: BlockKind::Paragraph,
            text: String::new(),
        })
        .as_deref()
        .and_then(find_inserted_id)?;
    if !state.make_database(id) {
        return None;
    }
    let points = state.db_add_column(id, "Points", PropertyKind::Number);
    let done = state.db_add_column(id, "Done", PropertyKind::Checkbox);
    let due = state.db_add_column(id, "Due", PropertyKind::Date);
    let (Some(points), Some(done), Some(due)) = (points, done, due) else {
        return None;
    };
    // The title column's id: the one `MakeDatabase` created first and locked
    // (ADR-0061), which the popup lists as non-toggleable.
    let Some(title) = state.db_column_toggles(id).iter().find(|t| t.locked).map(|t| t.property)
    else {
        return None;
    };
    // (name, points, done, due) — the values are literals, not `now()`/`today()`:
    // a sweep that runs tomorrow must photograph the same table.
    let rows: [(&str, &str, bool, &str); 5] = [
        ("Spec the window", "31", true, "2026-09-22"),
        ("Sort in SQL", "10", false, "2026-09-23"),
        ("Write the switcher", "7", true, "2026-09-25"),
        ("Measure a scroll", "39", false, "2026-10-01"),
        ("Light the menus", "3", true, "2026-10-04"),
    ];
    for (name, pts, checked, day) in rows {
        let Some(record) = state.db_add_record(id, state.db_next_row_ord(id)) else {
            continue;
        };
        state.db_set_cell_text(id, record, title, name);
        state.db_set_cell_text(id, record, points, pts);
        state.db_set_cell_text(id, record, due, day);
        // the box carries a typed value, not a parse: the cell's own toggle
        // path is `Flag(!shown)`, and the scene uses the same door
        state.db_set_cell_value(id, record, done, CellValue::Flag(checked));
    }
    // the last write's window is current, but the block row's *shape* (row
    // count) is only rebuilt by `db_fill_row` — the same call every structural
    // change in the wire section follows its write with
    db_refill_row(state, id);
    Some(id)
}

/// Seed the database-filter scene (SPEC §三十九 「操作」, D4): the table seed plus
/// one number rule — Points > 5 — written through the **same helpers the filter
/// panel drives**, so the shot shows the real write path's result, not a
/// hand-built fixture: 4 of the 5 rows, the header chip reading "Filter 1",
/// and the rules persisted in the view's document (`{"and":[{…,"op":"gt",
/// "value":5}]}`). The values are literals: a sweep that runs tomorrow must
/// photograph the same table.
fn seed_database_filter(state: &Rc<AppState>) -> Option<i32> {
    let id = seed_database_table(state)?;
    let points = state
        .db_column_toggles(id)
        .into_iter()
        .find(|t| t.name == "Points")
        .map(|t| t.property)?;
    let a = state.db_filter_add_clause(id, points);
    // `gt` is FILTER_OPS[3] (contains, eq, ne, gt, …), and the value goes
    // through the kind's own validation — the same door a user's keystroke
    // would.
    let b = state.db_filter_set_op(id, 0, 3);
    let c = state.db_filter_set_text(id, 0, "5");
    if !(a && b && c) {
        return None;
    }
    db_refill_row(state, id);
    Some(id)
}

/// Seed the database-formula scene (SPEC §三十九 「需计算」, D6): the table seed
/// plus two computed columns, created and defined **through the same write
/// paths the UI drives** — `db_add_column` for the column and
/// `db_formula_accept` for the expression, which is the formula editor's own
/// door *including its save-time checks* (the parse, the column names, the
/// dependency cycle). Nothing here hand-builds a formula the editor could not
/// have accepted.
///
/// The two expressions cover the two shapes a formula column most often has,
/// and their values are literals over the seed's literal cells (a sweep
/// tomorrow photographs the same table):
///
/// * `Double` = `[Points] * 2` — arithmetic over a number column: 62, 20, 14,
///   78, 6.
/// * `Label` = `if([Done], "done", "open")` — a boolean column's value through
///   `if`, a number-free shape that shows the condition arm: done, open, done,
///   open, done.
///
/// `db_refill_row` follows, because a new column changes every row's cells and
/// the block's own shape (the same call `db_add_column` makes and every
/// structural seed ends with).
fn seed_database_formula(state: &Rc<AppState>) -> Option<i32> {
    use crate::core::database::PropertyKind;

    let id = seed_database_table(state)?;
    let double = state.db_add_column(id, "Double", PropertyKind::Formula)?;
    if !state.db_formula_accept(id, double, "[Points] * 2") {
        return None;
    }
    let label = state.db_add_column(id, "Label", PropertyKind::Formula)?;
    if !state.db_formula_accept(id, label, "if([Done], \"done\", \"open\")") {
        return None;
    }
    db_refill_row(state, id);
    Some(id)
}

/// Seed one of D5's view-family scenes: the table seed plus a second view of
/// the layout the scene names, added **through the same path the switcher's
/// `+` drives** (`db_add_view`, which also switches to the new view), plus the
/// layout's own rule through the same helpers the UI drives:
///
/// * board — groups by the seed's `Done` checkbox (D4's `db_group_pick`, the
///   same key the Group picker writes). A checkbox rather than a select: an
///   option editor is still D5's undone half, so a seeded select column would
///   have an empty option list and the shot would be about that.
/// * calendar — the month is **pinned** (`db_calendar_month_set`, 2026-09), not
///   defaulted to now: a sweep that runs tomorrow must photograph the same
///   grid. The records carry literal dates (`2026-09-22` …), so the month's
///   counts and the day peeks answer to the same query the view runs.
/// * timeline / gallery / list / form — the layout itself, no extra rule: the
///   date column resolves by the schema's own order (`Due` is the seed's only
///   date), the gallery reports its own shape on first layout, and the form is
///   its field list.
///
/// The values are literals throughout: a sweep that runs tomorrow must
/// photograph the same view.
fn seed_database_view(state: &Rc<AppState>, layout: crate::core::database::ViewLayout) -> Option<i32> {
    let id = seed_database_table(state)?;
    let index = crate::core::database::ViewLayout::ALL
        .iter()
        .position(|l| *l == layout)? as i32;
    if !state.db_add_view(id, index) {
        return None;
    }
    // board **and** chart: both layouts read the view's group rule, and the
    // seed's `Done` checkbox is the one option-bounded column (an option
    // editor is still D5's undone half, so a seeded select would show an
    // empty option list). The chart's plot is the same `GROUP BY` the board's
    // columns are — two unchecked, three checked.
    if matches!(
        layout,
        crate::core::database::ViewLayout::Board | crate::core::database::ViewLayout::Chart
    ) {
        let done = state
            .db_column_toggles(id)
            .into_iter()
            .find(|t| t.name == "Done")
            .map(|t| t.property)?;
        if !state.db_group_pick(id, done) {
            return None;
        }
    }
    if layout == crate::core::database::ViewLayout::Calendar {
        state.db_calendar_month_set(id, 2026, 9);
    }
    db_refill_row(state, id);
    Some(id)
}

/// Seed the database-search scene (SPEC §三十九 「操作」, D7, ADR-0087): the
/// table seed with a live search over the view — needle `the`, which two of
/// the five titles match ("Spec the window", "Write the switcher"), written
/// through `db_view_search_set`, the same helper the header's search box
/// drives. The scene arm opens the box itself (the open state is session
/// state in the UI), so the shot pins both halves: the narrowed rows and the
/// needle that narrowed them.
fn seed_database_search(state: &Rc<AppState>) -> Option<i32> {
    let id = seed_database_table(state)?;
    if !state.db_view_search_set(id, "the") {
        return None;
    }
    db_refill_row(state, id);
    Some(id)
}

/// Seed the database-linked scene (SPEC §三十九 「操作」, D7, ADR-0085): the
/// table seed, then a second block that **links** it — a paragraph converted
/// through `db_make_linked`, the same door the database picker drives. The
/// link draws the source's data and its views (nothing is copied); the shot
/// shows two views of one entity stacked on one page.
fn seed_database_linked(state: &Rc<AppState>) -> Option<i32> {
    use crate::core::BlockKind;

    let id = seed_database_table(state)?;
    let db = state.db_ref_of(id)?;
    let after = {
        let d = state.doc.borrow();
        d.page_blocks(core_page_id(state.open_page.get()))
            .iter()
            .find(|b| b.kind == BlockKind::Paragraph && !b.text.is_empty())
            .map(|b| b.id.0 as i32)
    }?;
    let block = state
        .exec_on_open_page(Command::InsertBlockAfter {
            id: BlockId(after as u64),
            kind: BlockKind::Paragraph,
            text: String::new(),
        })
        .as_deref()
        .and_then(find_inserted_id)?;
    if !state.db_make_linked(block, db.as_u64() as i32) {
        return None;
    }
    db_refill_row(state, block);
    Some(id)
}

/// The pair D10's two popups need on screen at once: the table seed, a second
/// database with three named rows, and the two relation columns ADR-0088
/// declares together.
struct DbPair {
    tasks: i32,
    people: i32,
    assignee: i32,
    people_rows: Vec<i64>,
}

/// …and the way a user would build it: `db_add_column` for each relation
/// column and one `db_relation_configure` for the pair, so no shot photographs
/// a schema the type menu could not have made.
///
/// The target database is appended **below the fold**. The two popups this feeds
/// are centered sheets, so what the picture needs is that the pair exists and
/// what it points at; where the second table's own rows paint is not the thing
/// being measured.
fn seed_database_pair(state: &Rc<AppState>) -> Option<DbPair> {
    use crate::core::database::PropertyKind;
    use crate::core::BlockKind;

    let tasks = seed_database_table(state)?;
    let people = {
        let id = state
            .exec_on_open_page(Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: String::new(),
            })
            .as_deref()
            .and_then(find_inserted_id)?;
        if state.make_database(id) {
            id
        } else {
            return None;
        }
    };
    let assigned = state.db_add_column(people, "Assigned", PropertyKind::Relation)?;
    let assignee = state.db_add_column(tasks, "Assignee", PropertyKind::Relation)?;
    let target = state.db_ref_of(people).map(|db| db.as_u64() as i32)?;
    state
        .db_relation_configure(tasks, assignee, target, assigned)
        .ok()?;
    let title = state
        .db_column_toggles(people)
        .into_iter()
        .find(|t| t.locked)
        .map(|t| t.property)?;
    let mut people_rows = Vec::new();
    for name in ["Ada Lovelace", "Grace Hopper", "Alan Turing"] {
        let row = state.db_add_record(people, state.db_next_row_ord(people))?;
        state.db_set_cell_text(people, row, title, name);
        people_rows.push(row);
    }
    Some(DbPair {
        tasks,
        people,
        assignee,
        people_rows,
    })
}

/// One column of a seeded database, by name.
fn column_named(state: &Rc<AppState>, block: i32, name: &str) -> Option<i32> {
    state
        .db_column_toggles(block)
        .into_iter()
        .find(|t| !name.is_empty() && t.name == name)
        .map(|t| t.property)
}

/// Seed the `database-kinds` scene (SPEC §三十九 「属性」, D10): the table seed
/// with the type menu open **on a column**, which is the half of the menu that
/// carries the gates — a conversion the stored values cannot survive is drawn
/// greyed with the sentence that says why, not hidden from the list.
fn seed_database_kinds(state: &Rc<AppState>, g: &UIState<'_>) -> Option<i32> {
    let id = seed_database_table(state)?;
    db_push_kinds(g, state, id, column_named(state, id, "Points")?);
    // The click anchors the menu to the header's own button; a headless scene
    // has no click, so it is pinned inside the grid at the popup's own width.
    g.set_db_columns_x(520.0);
    g.set_db_columns_y(300.0);
    Some(id)
}

/// Seed the `database-relation` scene (ADR-0088): the pair, and a task row
/// already holding two of the three people — the picker photographed on its way
/// back from a half-done pick, which is the one state a write-path test cannot
/// show.
fn seed_database_relation(state: &Rc<AppState>, g: &UIState<'_>) -> Option<i32> {
    let pair = seed_database_pair(state)?;
    let title = state
        .db_column_toggles(pair.tasks)
        .into_iter()
        .find(|t| t.locked)
        .map(|t| t.property)?;
    let task = state.db_add_record(pair.tasks, state.db_next_row_ord(pair.tasks))?;
    state.db_set_cell_text(pair.tasks, task, title, "Pair the columns");
    for held in &pair.people_rows[..2] {
        state.db_relation_toggle(pair.tasks, task, pair.assignee, *held as i32);
    }
    // The ids first, then the list — the order the cell's own click runs them,
    // because `db_push_relation` reads the popup's state rather than a copy.
    g.set_db_relation_block(pair.tasks);
    g.set_db_relation_record(task as i32);
    g.set_db_relation_property(pair.assignee);
    g.set_db_relation_name(state.db_property_label(pair.assignee).into());
    // The popup's `target` is a *database* id, not the block that holds it —
    // the first cut of this seed passed the block and the header answered with
    // `db_database_name`'s dangling-name fallback, which the shot then
    // photographed as a placeholder sentence about a missing database.
    let people_db = state.db_ref_of(pair.people).map(|db| db.as_u64() as i32)?;
    g.set_db_relation_target(state.db_database_name(people_db).into());
    g.set_db_relation_text("".into());
    db_push_relation(g, state, 0);
    db_refill_row(state, pair.tasks);
    Some(pair.tasks)
}

/// Seed the `database-rollup` scene (ADR-0089): the pair plus a `Total` fold
/// over it, then the editor on its **second** chooser — the state where the
/// three summary lines are half-spoken, which is what the configurator looks
/// like while it is being filled in.
///
/// The folded column belongs to the *target* database, because `check_config`
/// requires it (`NotAColumnOfTheTarget`): a rollup over the near side's own
/// column is not a fold of anything, and the first cut of this seed asked for
/// exactly that, so the configure was refused and the popup never opened.
fn seed_database_rollup(state: &Rc<AppState>, g: &UIState<'_>) -> Option<i32> {
    use crate::core::database::PropertyKind;

    let pair = seed_database_pair(state)?;
    let hours = state.db_add_column(pair.people, "Hours", PropertyKind::Number)?;
    for (row, value) in pair.people_rows.iter().zip(["8", "6", "4"]) {
        state.db_set_cell_text(pair.people, *row, hours, value);
    }
    // One task that actually links to two of them, so the fold has a sum to
    // answer and the shot shows the configured column doing its job.
    let title = state
        .db_column_toggles(pair.tasks)
        .into_iter()
        .find(|t| t.locked)
        .map(|t| t.property)?;
    let task = state.db_add_record(pair.tasks, state.db_next_row_ord(pair.tasks))?;
    state.db_set_cell_text(pair.tasks, task, title, "Fold the hours");
    for held in &pair.people_rows[..2] {
        state.db_relation_toggle(pair.tasks, task, pair.assignee, *held as i32);
    }
    let total = state.db_add_column(pair.tasks, "Total", PropertyKind::Rollup)?;
    // `2` is `sum` in `Aggregate::ALL`, the same int the editor's own list
    // hands back; the fold then answers over the two people the row links to.
    state
        .db_rollup_configure(pair.tasks, total, pair.assignee, hours, 2)
        .ok()?;
    g.set_db_rollup_block(pair.tasks);
    g.set_db_rollup_property(total);
    g.set_db_rollup_name(state.db_property_label(total).into());
    db_push_rollup(g, state, 1);
    db_refill_row(state, pair.tasks);
    Some(pair.tasks)
}

/// Seed the database-template scene (SPEC §三十九 「操作」, D7, ADR-0086): the
/// table seed, then — all through the real write paths — one row filled and
/// saved as the template (`db_template_from_row`, the row affordance's own
/// door), then one `db_add_record` whose prefill comes from the template in
/// the creation batch. The shot shows three facts at once: the source row
/// ("Template row"), the new row it prefilled (same title, 50, checked), and
/// the five rows the template never touched.
fn seed_database_template(state: &Rc<AppState>) -> Option<i32> {
    use crate::core::database::CellValue;

    let id = seed_database_table(state)?;
    let points = state
        .db_column_toggles(id)
        .iter()
        .find(|t| t.name == "Points")
        .map(|t| t.property)?;
    let done = state
        .db_column_toggles(id)
        .iter()
        .find(|t| t.name == "Done")
        .map(|t| t.property)?;
    let Some(title) = state.db_column_toggles(id).iter().find(|t| t.locked).map(|t| t.property)
    else {
        return None;
    };
    let Some(source) = state.db_add_record(id, state.db_next_row_ord(id)) else {
        return None;
    };
    state.db_set_cell_text(id, source, title, "Template row");
    state.db_set_cell_text(id, source, points, "50");
    state.db_set_cell_value(id, source, done, CellValue::Flag(true));
    if !state.db_template_from_row(id, source) {
        return None;
    }
    if state.db_add_record(id, state.db_next_row_ord(id)).is_none() {
        return None;
    }
    db_refill_row(state, id);
    Some(id)
}

/// The first cell a grid-building command created — the cell a table's caret
/// starts in. `apply` inserts cells in row-major order, so this is the
/// top-left one, which is where the converted line's words went.
fn find_inserted_cell_id(changes: &[Change]) -> Option<i32> {
    changes.iter().find_map(|c| match c {
        Change::BlockInserted(b) if b.kind == crate::core::BlockKind::TableCell => {
            Some(b.id.0 as i32)
        }
        _ => None,
    })
}

/// The first line a layout-building command created — where a columns caret
/// starts, since that is where the converted line's words moved. The command
/// inserts a box and its first line as a pair, so this is the first box.
fn find_inserted_line_id(changes: &[Change]) -> Option<i32> {
    changes.iter().find_map(|c| match c {
        Change::BlockInserted(b) if b.kind == crate::core::BlockKind::Paragraph => {
            Some(b.id.0 as i32)
        }
        _ => None,
    })
}

/// Markdown line-shortcuts (ADR-0022): typing a trigger prefix + space (or
/// the exact token) converts the block being edited, with the trigger
/// stripped. The slash menu and Turn-into list omit every kind reachable
/// this way, so the symbol is the only path to them. Returns
/// `(kind, remaining text, set-checked)`. Code and divider blocks never
/// convert — their text legitimately starts with these characters.
fn markdown_convert(
    text: &str,
    current: Option<crate::core::BlockKind>,
) -> Option<(crate::core::BlockKind, String, Option<bool>)> {
    use crate::core::BlockKind;
    if matches!(
        current,
        Some(BlockKind::Code) | Some(BlockKind::Divider) | Some(BlockKind::Math)
    ) {
        // a formula is source: `> x` inside `\begin{...}` is content, not a
        // quote marker
        return None;
    }
    let conv = |kind, rest: &str| Some((kind, rest.to_string(), None));
    if let Some(rest) = text.strip_prefix("### ") {
        conv(BlockKind::Heading3, rest)
    } else if let Some(rest) = text.strip_prefix("## ") {
        conv(BlockKind::Heading2, rest)
    } else if let Some(rest) = text.strip_prefix("# ") {
        conv(BlockKind::Heading1, rest)
    } else if let Some(rest) = text.strip_prefix("- ").or_else(|| text.strip_prefix("* ")) {
        conv(BlockKind::Bullet, rest)
    } else if let Some(rest) = text.strip_prefix("[x] ") {
        Some((BlockKind::Todo, rest.to_string(), Some(true)))
    } else if let Some(rest) = text.strip_prefix("[] ").or_else(|| text.strip_prefix("[ ] ")) {
        conv(BlockKind::Todo, rest)
    } else if let Some(rest) = text.strip_prefix("> ") {
        conv(BlockKind::Quote, rest)
    } else if let Some(rest) = numbered_prefix(text) {
        conv(BlockKind::Numbered, rest)
    } else if text == "---" {
        conv(BlockKind::Divider, "")
    } else if text == "```" {
        conv(BlockKind::Code, "")
    } else if let Some(rest) = text.strip_prefix("$$ ") {
        conv(BlockKind::Math, rest)
    } else if text == "$$" {
        conv(BlockKind::Math, "")
    } else {
        None
    }
}

/// `12. rest` → `rest`; digits + ". " only.
fn numbered_prefix(text: &str) -> Option<&str> {
    let dot = text.find(". ")?;
    if dot == 0 || !text[..dot].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.get(dot + 2..)
}

/// Open a page, sync the top bar, and highlight it in the tree.
fn open(g: &UIState<'_>, state: &Rc<AppState>, id: i32) {
    if !state.workspace.borrow().contains(id) {
        return;
    }
    // the typing flush is debounced 300 ms and resolves against the open
    // page, so commit it before the page under it changes
    flush_pending_edit(g, state);
    state.nav_record(state.open_page.get(), id);
    state.open_page(id);
    show_open_page(g, state);
}

/// Go Back / Go Forward (SPEC §十六): move along the session's page history.
/// `nav_step` already parked the page being left onto the opposite stack, so
/// this must not record again.
fn navigate(g: &UIState<'_>, state: &Rc<AppState>, forward: bool) {
    flush_pending_edit(g, state);
    let Some(id) = state.nav_step(forward) else {
        return;
    };
    state.open_page(id);
    show_open_page(g, state);
}

/// Paint the shell for whatever `state.open_page` now holds.
fn show_open_page(g: &UIState<'_>, state: &Rc<AppState>) {
    let id = state.open_page.get();
    let (title, crumb) = state.open_page_info(id);
    g.set_page_title(title.into());
    g.set_page_breadcrumb(crumb.into());
    g.set_sidebar_selected_id(id);
    // a picture from the page you just left must not keep covering the one
    // you arrived at
    g.set_preview_attachment(0);
}

fn search_target(g: &UIState<'_>, state: &Rc<AppState>) -> i32 {
    let mut id = g.get_search_selected_id();
    if id < 0 {
        let focus = g.get_search_focus() as usize;
        id = state.search.iter().nth(focus).map_or(-1, |r| r.page_id);
    }
    if id < 0 {
        id = PAGE_GETTING_STARTED;
    }
    id
}

/// Benchmark scene G: switch to the next bench page (called from a timer).
pub fn bench_switch_next(
    ui: &AppWindow,
    state: &Rc<AppState>,
    bench_ids: &[i32],
    cursor: &mut usize,
) {
    if bench_ids.is_empty() {
        return;
    }
    let id = bench_ids[*cursor % bench_ids.len()];
    *cursor = (*cursor + 1) % bench_ids.len();
    let g = ui.global::<UIState>();
    open(&g, state, id);
}

// Scenes for headless visual captures (--scene <name>).
/// The built-in library, in the session a scene draws. A headless shot has no
/// database, and `AppState::seed_builtin_templates` refuses to seed without one
/// (its "already done" flag is a settings row), so the same five bodies land
/// through `import_template` — the channel the menu's Import row uses, which is
/// the point: a preset is Markdown in the ordinary rows, not a resource file.
fn seed_template_library(state: &Rc<AppState>) {
    for preset in crate::core::template::PRESETS {
        state.import_template(preset.name, preset.markdown);
    }
}

/// The version panel's list, drawn from rows that do not exist on disk. A
/// version *is* a database file and a headless session has no database, so a
/// sweep scene cannot show a real one; what it shows is the same projection of
/// the same row shape — `version_rows` and `versions_note`, the two functions
/// `fill_versions` itself calls — over a list chosen to exercise the panel: a
/// name, an auto-label, a name long enough to be elided, and ages spanning
/// minutes to weeks so `age_text` is read at both ends. That the files exist
/// behind such rows is what the storage tests prove. Returns the list so the
/// comparison scene can name the same version the same way.
fn seed_versions(state: &Rc<AppState>, g: &UIState<'_>, page: i32) -> Vec<(i64, String)> {
    let now = crate::app::state::now_secs();
    let minute = 60;
    let versions = vec![
        (now - 26 * minute, "Shipped the recap".to_string()),
        (now - 3 * 60 * minute, "Version 2".to_string()),
        (
            now - 28 * 60 * minute,
            "Before the restructure, when this was still one long section".to_string(),
        ),
        (now - 5 * 24 * 60 * minute, "Weekly review".to_string()),
        (now - 21 * 24 * 60 * minute, "First draft".to_string()),
    ];
    let rows = crate::app::state::version_rows(&versions);
    let note = crate::app::state::versions_note(rows.len());
    state.versions.set_vec(rows);
    g.set_versions_page(page);
    g.set_versions_title(
        state
            .workspace
            .borrow()
            .title_of(page)
            .unwrap_or("无标题")
            .to_string()
            .into(),
    );
    g.set_versions_note(note.into());
    g.set_versions_showing_diff(false);
    g.set_versions_selected(-1);
    g.set_version_name("".into());
    g.set_versions_open(true);
    versions
}

pub fn apply_scene(ui: &AppWindow, state: &Rc<AppState>, scene: &str) {
    let g = ui.global::<UIState>();
    match scene {
        "dark" => g.set_dark(true),
        "palette" | "search" | "search-notes" | "menu" | "dialog" | "settings" => {
            apply_scene_overlay(ui, state, scene)
        }
        "slash" => {
            // base: focus the first paragraph (overlay opens the menu)
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| (b.id.0 as i32, b.text.len() as i32, b.text.clone()))
            };
            if let Some((id, len, text)) = target {
                g.set_editing_text(text.into());
                g.set_pending_caret(len);
                g.set_editing_id(id);
            }
        }
        "block-menu" => {}
        "link" => apply_scene_overlay(ui, state, "link-dlg"),
        "dark-slash" => {
            g.set_dark(true);
            apply_scene(ui, state, "slash");
        }
        "dark-find" => {
            g.set_dark(true);
            apply_scene(ui, state, "find");
        }
        "dark-marks" => {
            g.set_dark(true);
            apply_scene(ui, state, "marks");
        }
        "dark-code-hl" => {
            g.set_dark(true);
            apply_scene(ui, state, "code-hl");
        }
        "dark-link" => {
            g.set_dark(true);
            apply_scene_overlay(ui, state, "link-dlg");
        }
        "dark-block-menu" => {
            g.set_dark(true);
            apply_scene_overlay(ui, state, "block-menu");
        }
        "dark-block-colors" => {
            g.set_dark(true);
            apply_scene(ui, state, "block-colors");
        }
        "dark-synced" => {
            g.set_dark(true);
            apply_scene(ui, state, "synced");
        }
        "dark-synced-source-gone" => {
            g.set_dark(true);
            apply_scene(ui, state, "synced-source-gone");
        }
        "dark-move-to-tall" => {
            g.set_dark(true);
            apply_scene_overlay(ui, state, "move-to-tall");
        }
        "dark-title-edit" => {
            g.set_dark(true);
            apply_scene(ui, state, "title-edit");
        }
        "dark-mention" => {
            g.set_dark(true);
            apply_scene(ui, state, "mention");
        }
        "dark-date" => {
            g.set_dark(true);
            apply_scene(ui, state, "date");
        }
        "dark-backlinks" => {
            g.set_dark(true);
            apply_scene(ui, state, "backlinks");
        }
        "dark-dangling" => {
            g.set_dark(true);
            apply_scene(ui, state, "dangling");
        }
        "dark-database-table" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-table");
        }
        "dark-database-filter" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-filter");
        }
        "dark-database-formula" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-formula");
        }
        "dark-database-board" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-board");
        }
        "dark-database-list" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-list");
        }
        "dark-database-calendar" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-calendar");
        }
        "dark-database-gallery" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-gallery");
        }
        "dark-database-timeline" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-timeline");
        }
        "dark-database-form" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-form");
        }
        "dark-database-chart" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-chart");
        }
        "dark-database-search" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-search");
        }
        "dark-database-linked" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-linked");
        }
        "dark-database-template" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-template");
        }
        // D10's three popups in the dark theme: `accent-text` is a different
        // brush there, and the tick that went unseen in a light shot was drawn
        // from that brush, so the dark picture is the other half of the check.
        "dark-database-kinds" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-kinds");
        }
        "dark-database-relation" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-relation");
        }
        "dark-database-rollup" => {
            g.set_dark(true);
            apply_scene(ui, state, "database-rollup");
        }
        "title-edit" => {
            g.set_page_title("Renaming in place…".into());
            g.set_title_editing(true);
        }
        "recovered" => {
            // mirror the real string from main.rs (the library lives in the
            // per-user profile since ADR-0020; a sweep shot that invents copy
            // gets judged as if users read it)
            g.set_db_notice(
                "数据库已损坏 — 已从备份恢复（C:\\Users\\you\\AppData\\Roaming\\Quire\\quire.db.bak1）。".into(),
            );
        }

        "page-block" => {
            // seed an embedded child page: an empty line after the first
            // block turns into a Page block (PageCreated + ref + kind in one
            // batch), then the child gets a real title for the shot
            let first = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .first()
                    .map(|b| b.id.0 as i32)
            };
            let Some(first) = first else { return };
            let changes = state.exec_on_open_page(Command::InsertBlockAfter {
                id: BlockId(first as u64),
                kind: crate::core::BlockKind::Paragraph,
                text: String::new(),
            });
            if let Some(nid) = changes.as_deref().and_then(find_inserted_id) {
                if let Some(child) = state.create_page_block(nid) {
                    state.rename_page(child, "Project Atlas");
                }
            }
        }

        "link-block" => {
            // seed a Link-to-page block pointing at an existing page: the
            // unowned reference renders like a page block with a link icon
            let first = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .first()
                    .map(|b| b.id.0 as i32)
            };
            let target = state
                .workspace
                .borrow()
                .dfs_order()
                .into_iter()
                .find(|id| *id != state.open_page.get());
            let (Some(first), Some(target)) = (first, target) else {
                return;
            };
            let changes = state.exec_on_open_page(Command::InsertBlockAfter {
                id: BlockId(first as u64),
                kind: crate::core::BlockKind::Paragraph,
                text: String::new(),
            });
            if let Some(nid) = changes.as_deref().and_then(find_inserted_id) {
                state.create_page_link_block(nid, target);
            }
        }

        "rename" => {
            g.set_renaming_id(108);
        }

        // SPEC §四十: the two inline atoms, on a line of the demo page so the
        // chip is shown among real prose. A mention stores the page's *id* —
        // the chip reads the workspace, which is why a rename follows with
        // nothing rewritten — and a date stores its own ISO text, because a
        // date has no page to be renamed. The date is a literal and not
        // `today_iso()`: a shot has to look the same tomorrow.
        "mention" | "date" => {
            let home = state.open_page.get();
            let target = state.create_page(None);
            state.rename_page(target, "Project Atlas");
            state.open_page(home);
            let is_mention = scene == "mention";
            // byte 4 is the `@` in both lines — the picker's own trigger
            let line = insert_near_top(
                state,
                if is_mention {
                    "See @ for the numbers"
                } else {
                    "Due @ in the calendar"
                },
            );
            if let Some(id) = line {
                if is_mention {
                    state.apply_mention(id, 4, Some(target), "Project Atlas");
                } else {
                    state.apply_mention(id, 4, None, "2026-09-22");
                }
            }
        }

        // SPEC §四十: a reference whose target is gone. Deleting the page does
        // *not* rewrite the mention — that is the whole consequence of storing
        // an id — so the chip has to say so itself rather than go blank or
        // keep naming a page that is not there.
        "dangling" => {
            let home = state.open_page.get();
            let target = state.create_page(None);
            state.rename_page(target, "Doomed page");
            state.open_page(home);
            if let Some(id) = insert_near_top(state, "See @ while it lasted") {
                state.apply_mention(id, 4, Some(target), "Doomed page");
                state.delete_page(target);
            }
        }

        // SPEC §四十: the panel. Three shapes, because the panel's whole
        // design is about how much of the answer it draws: fewer references
        // than the window (no fold control at all), more (a folded window and
        // a count), and the same page unfolded.
        "backlinks" | "backlinks-open" => {
            if seed_referenced_page(&g, state, 12).is_some() && scene == "backlinks-open" {
                state.toggle_backlinks();
            }
        }
        "backlinks-small" => {
            seed_referenced_page(&g, state, 3);
        }

        // SPEC §三十九: the table view. A real database through the real write
        // paths — nothing here hand-builds a fixture the UI could not make.
        "database-table" => {
            seed_database_table(state);
        }

        // SPEC §三十九 「操作」(D4): the same table with one rule compiled into
        // SQL — Points > 5 keeps 4 of the 5 rows. The shot shows the red
        // line's visible half: the count and the rows answer to the rules,
        // the chip says "Filter 1", and nothing filtered happens anywhere but
        // in the statement.
        "database-filter" => {
            seed_database_filter(state);
        }

        // SPEC §三十九 「需计算」(D6): the same table with two computed columns,
        // added and defined through the real write paths (`db_add_column` →
        // `db_formula_accept`, the editor's own door — save-time checks
        // included). The formulas are literals over literal cells, so a sweep
        // tomorrow photographs the same values: `Double` is `[Points] * 2`
        // (arithmetic), `Label` is an `if` over the checkbox with text branches
        // (booleans, concatenation-adjacent), and both cover the two most
        // common shapes a formula column has. The *editor* itself is not in
        // this shot — the scene pins the table with its computed values.
        "database-formula" => {
            seed_database_formula(state);
        }

        // SPEC §三十九 「视图」(D5): the family. Each scene is the table seed
        // plus a second view of the named layout, added and switched to through
        // the switcher's own write path — so every shot is the real read
        // pipeline's answer (group counts, month counts, per-day peeks, card
        // slices), never a hand-built fixture.
        "database-board" => {
            seed_database_view(state, crate::core::database::ViewLayout::Board);
        }
        "database-list" => {
            seed_database_view(state, crate::core::database::ViewLayout::List);
        }
        "database-calendar" => {
            seed_database_view(state, crate::core::database::ViewLayout::Calendar);
        }
        "database-gallery" => {
            seed_database_view(state, crate::core::database::ViewLayout::Gallery);
        }
        "database-timeline" => {
            seed_database_view(state, crate::core::database::ViewLayout::Timeline);
        }
        "database-form" => {
            seed_database_view(state, crate::core::database::ViewLayout::Form);
        }

        // SPEC §三十九 (D7): the eighth layout. The plot is the group list —
        // the same `GROUP BY` the board's columns are — so the seed is the
        // board's: the table seed plus a chart view grouped by `Done` (two
        // unchecked, three checked), drawn in the default bar shape.
        "database-chart" => {
            seed_database_view(state, crate::core::database::ViewLayout::Chart);
        }

        // SPEC §三十九 「操作」 (D7, ADR-0087): the view's live search. The
        // needle is set through the state helper the box drives; the box
        // itself is opened here because its open state is session state in
        // the UI — the shot pins the narrowed rows, the count that says so,
        // and the needle that narrowed them.
        "database-search" => {
            if let Some(id) = seed_database_search(state) {
                g.set_db_search_block(id);
                g.set_db_search_text("the".into());
                g.set_db_search_open(true);
            }
        }

        // SPEC §三十九 「操作」 (D7, ADR-0085): a linked database. The second
        // block draws the first block's entity through `db_ref` — nothing is
        // copied — so the shot is two views of one database on one page.
        "database-linked" => {
            seed_database_linked(state);
        }

        // SPEC §三十九 「操作」 (D7, ADR-0086): the record template. The
        // source row was saved as the template through the row affordance's
        // own write path, and the last row is a `db_add_record` whose prefill
        // came from the template in the creation batch.
        "database-template" => {
            seed_database_template(state);
        }

        // SPEC §三十九 「属性」 (D10): the three doors the state layer had and
        // the window did not. Each seeds through the same helpers the click
        // drives and stops one step short of *showing* — the popup's own open
        // bit belongs to `apply_scene_overlay`, because a delegate-owned
        // `PopupWindow` only fires its `changed` handler on a false → true
        // transition, and a flag already true at the first render never moves.
        "database-kinds" => {
            seed_database_kinds(state, &g);
        }
        "database-relation" => {
            seed_database_relation(state, &g);
        }
        "database-rollup" => {
            seed_database_rollup(state, &g);
        }

        // SPEC §四十 / ADR-0052: a mirror, and a mirror whose source is gone.
        // Both are the *projection* of a stored pointer, not a fixture: what
        // the shot shows is what `sync_target` answered for those bytes.
        "synced" => {
            seed_synced_page(state, false);
        }
        "synced-source-gone" => {
            seed_synced_page(state, true);
        }

        "empty" => open(&g, state, 113),
        "nest" => {
            // indent the second bullet under the first (visual test)
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .filter(|b| b.kind == crate::core::BlockKind::Bullet)
                    .nth(1)
                    .map(|b| b.id)
            };
            if let Some(id) = target {
                let _ = state.exec_on_open_page(Command::IndentList { id });
            }
        }
        // SPEC §三十七: a collapsible section. `toggle` is the open shape
        // (chevron down, child visible) and `toggle-fold` closes it, so the
        // two captures must differ by exactly the child's row.
        "toggle" | "toggle-fold" => {
            let page = core_page_id(state.open_page.get());
            let bullets = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .filter(|b| b.kind == crate::core::BlockKind::Bullet)
                    .take(2)
                    .map(|b| b.id)
                    .collect::<Vec<_>>()
            };
            if bullets.len() == 2 {
                let (parent, child) = (bullets[0], bullets[1]);
                let _ = state.exec_on_open_page(Command::IndentList { id: child });
                let _ = state.exec_on_open_page(Command::SetBlockType {
                    id: parent,
                    kind: crate::core::BlockKind::Toggle,
                });
                if scene == "toggle-fold" {
                    let _ = state.exec_on_open_page(Command::ToggleFold { id: parent });
                }
            }
        }
        // SPEC §三十七 批次 A: a picture sitting in the page. `image-half` is
        // the same block at the 50 % tier, so the pair shows the width setting
        // really does drive the row geometry. The fixture is generated rather
        // than picked — the native file dialog is the one part of this feature
        // a headless scene cannot reach.
        "image" | "image-half" => {
            let page = core_page_id(state.open_page.get());
            let after = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id.0 as i32)
            };
            let aid = state.claim_attachment_id();
            let (Some(after), Some(att)) = (after, state.store.create_fixture(aid, 640, 400))
            else {
                return;
            };
            if !state.insert_attachment(after, att, crate::core::BlockKind::Image)
                || scene != "image-half"
            {
                return;
            }
            let pic = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Image)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = pic {
                state.set_image_width(id, 50);
            }
        }
        // The other half of 批次 A: bytes the editor names but never opens. The
        // payload has to be a fixed length or the size label — the one number
        // this row paints — would differ between sweeps.
        "file" => {
            let page = core_page_id(state.open_page.get());
            let after = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id.0 as i32)
            };
            let aid = state.claim_attachment_id();
            let payload = vec![0xA5u8; 1_842_000];
            let (Some(after), Some(att)) = (
                after,
                state
                    .store
                    .create_file_fixture(aid, "quarterly-report.pdf", &payload),
            ) else {
                return;
            };
            let _ = state.insert_attachment(after, att, crate::core::BlockKind::File);
        }
        // SPEC §三十七 批次 B: a grid sitting in the page. `table-edit` is the
        // same table with the caret in one cell, so the pair proves both the
        // resting shape and that a cell can take the editor at all — the
        // second is the one a static model could fake. `table-marks` is the
        // third question: a marked cell, which is drawn by a different row.
        "table" | "table-edit" | "table-marks" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(line) = target else { return };
            // built the way the menu builds one: the line's own words move into
            // the top-left cell, which is where a scene like this starts
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id: line,
                kind: crate::core::BlockKind::Table,
            });
            let cells = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .filter(|b| b.kind == crate::core::BlockKind::TableCell)
                    .map(|b| b.id)
                    .collect::<Vec<_>>()
            };
            for (cell, word) in cells.iter().zip(["North", "South", "East", "West", "Up"]) {
                let _ = state.exec_editor(Command::ReplaceText {
                    id: *cell,
                    text: (*word).into(),
                });
            }
            if scene == "table-marks" {
                // one cell whose words cannot fit its column: a marked cell is
                // drawn by the run row, and whether that row wraps is a
                // different question from whether a paragraph's does
                // (ADR-0041)
                let Some(cell) = cells.first().copied() else { return };
                let text = "Revenue grew because the team shipped the second half of the plan.";
                let _ = state.exec_editor(Command::ReplaceText {
                    id: cell,
                    text: text.into(),
                });
                let marks = vec![crate::core::Mark {
                    start: text.find("the team").expect("needle present"),
                    end: text.find("the team").expect("needle present") + "the team".len(),
                    kind: crate::core::MarkKind::Bold,
                    url: String::new(),
                    date: None,
                }];
                state.doc.borrow_mut().apply(&[crate::core::Change::BlockMarksSet {
                    id: cell,
                    marks,
                }]);
            }
            state.reproject_blocks();
            if scene == "table-edit" {
                if let Some(cell) = cells.get(3) {
                    focus_block(&g, state, cell.as_u64() as i32, i32::MAX);
                }
            }
        }
        // SPEC §三十七 批次 B: a layout tiling the page. `columns-3` is the same
        // layout after the strip's "Add column", so the pair proves the boxes
        // really re-tile — a static model could show a count that never moved
        // the widths. `columns-marks` puts an over-long marked line in the
        // first box, which is the third place runs are drawn.
        "columns" | "columns-3" | "columns-marks" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(line) = target else { return };
            // built the way the menu builds one, so the words move into the
            // first box and the scene shows a layout with content in it
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id: line,
                kind: crate::core::BlockKind::Columns,
            });
            if scene == "columns-3" {
                let layout = {
                    let d = state.doc.borrow();
                    d.page_blocks(page)
                        .iter()
                        .find(|b| b.kind == crate::core::BlockKind::Columns)
                        .map(|b| b.id.0 as i32)
                };
                if let Some(id) = layout {
                    state.column_add(id);
                }
            }
            let boxes = {
                let d = state.doc.borrow();
                let mut boxes = d
                    .page_blocks(page)
                    .iter()
                    .filter(|b| b.kind == crate::core::BlockKind::Column)
                    .map(|b| (b.order, b.id))
                    .collect::<Vec<_>>();
                // left to right, whatever order the page slice happens to hold
                boxes.sort();
                boxes.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
            };
            for (n, bx) in boxes.iter().enumerate() {
                // one word per box, so an uneven render is visible at a glance
                let first = {
                    let d = state.doc.borrow();
                    d.page_blocks(page)
                        .iter()
                        .filter(|b| b.parent == Some(*bx))
                        .min_by_key(|b| b.order)
                        .map(|b| b.id)
                };
                if let Some(id) = first {
                    let _ = state.exec_editor(Command::ReplaceText {
                        id,
                        text: format!("Column {}", n + 1).into(),
                    });
                }
            }
            if scene == "columns-marks" {
                // the third run consumer: a line inside a box, at half the page
                // width, with more words than fit (ADR-0041)
                let first_line = {
                    let d = state.doc.borrow();
                    d.page_blocks(page)
                        .iter()
                        .filter(|b| b.parent == boxes.first().copied())
                        .min_by_key(|b| b.order)
                        .map(|b| b.id)
                };
                let Some(id) = first_line else { return };
                let text = "Two boxes, and a sentence that has to break inside one of them.";
                let _ = state.exec_editor(Command::ReplaceText { id, text: text.into() });
                let marks = vec![crate::core::Mark {
                    start: text.find("has to break").expect("needle present"),
                    end: text.find("has to break").expect("needle present") + "has to break".len(),
                    kind: crate::core::MarkKind::Bold,
                    url: String::new(),
                    date: None,
                }];
                state.doc.borrow_mut().apply(&[crate::core::Change::BlockMarksSet {
                    id,
                    marks,
                }]);
            }
            state.reproject_blocks();
        }
        // SPEC §三十七 批次 C: a formula, built the way the slash menu builds
        // one and then filled with source that touches every shape the
        // renderer knows — fraction, radical, relation, script, spacing escape.
        "math" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(id) = target else { return };
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id,
                kind: crate::core::BlockKind::Math,
            });
            let _ = state.exec_editor(Command::ReplaceText {
                id,
                text: r"\frac{a+b}{2} \leq \sqrt{ab} \ne 0 \quad \int_0^1 x^2 \,dx".into(),
            });
            state.reproject_blocks();
        }
        // …and the same renderer inside a sentence: the mark holds the source,
        // the run shows the glyphs, and the words around it are untouched.
        "math-inline" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(id) = target else { return };
            let text = r"The rest energy is E = mc^2 for any mass.";
            let source = r"E = mc^2";
            let _ = state.exec_editor(Command::ReplaceText {
                id,
                text: text.into(),
            });
            if let Some(start) = text.find(source) {
                let marks = vec![crate::core::Mark {
                    start,
                    end: start + source.len(),
                    kind: crate::core::MarkKind::Math,
                    url: String::new(),
                    date: None,
                }];
                state
                    .doc
                    .borrow_mut()
                    .apply(&[crate::core::Change::BlockMarksSet { id, marks }]);
            }
            state.reproject_blocks();
        }
        // SPEC §三十七 批次 C: the page's own contents. Built the way the
        // insert menu builds one — the intro line becomes the block, so the
        // list sits above every heading it lists, and the headings are the
        // fixture's own (three H2s and an H3, so the indent has two steps).
        "toc" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(id) = target else { return };
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id,
                kind: crate::core::BlockKind::Toc,
            });
            state.reproject_blocks();
        }
        // SPEC §三十七 批次 C: a link as a card. The intro line becomes the
        // block and carries an address with a `www.`, a query and a fragment, so
        // both lines the card paints are derived from something shaped like a
        // real link — and the headline is a provider the card knows.
        "embed" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(id) = target else { return };
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id,
                kind: crate::core::BlockKind::Embed,
            });
            let _ = state.exec_editor(Command::ReplaceText {
                id,
                text: "https://www.youtube.com/watch?v=dQw4w9WgXcQ#t=1s".into(),
            });
            state.reproject_blocks();
        }
        // …and the card before anything is typed into it. This is the state
        // every embed starts in, and the only one with no address to lay out,
        // so it is where a card that collapses to nothing would show up.
        "embed-empty" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(id) = target else { return };
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id,
                kind: crate::core::BlockKind::Embed,
            });
            let _ = state.exec_editor(Command::ReplaceText { id, text: "".into() });
            state.reproject_blocks();
        }
        // SPEC §三十七 批次 C: the same block the memory gate scrolls, painted.
        // The fixture carries a comment, a string, numbers, a name, CJK inside
        // the comment, and one line long enough to need a hard break, so every
        // rule the layer model has is on the screen at once.
        "code-hl" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            let Some(id) = target else { return };
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id,
                kind: crate::core::BlockKind::Code,
            });
            let _ = state.exec_editor(Command::ReplaceText {
                id,
                text: crate::app::state::bench_code_source(),
            });
            let _ = state.exec_on_open_page(Command::SetCodeLang {
                id,
                lang: crate::core::Lang::Rust,
            });
            state.reproject_blocks();
        }
        "find" => {
            g.set_find_open(true);
            g.set_find_term("the".into());
            state.find_start("the");
            g.set_find_label(state.find_label().into());
            // jump to the first hit like the live path does, so the scene
            // shows a selection, not a bare "0 / N"
            if let Some((bid, start, end)) = state.find_step(true) {
                apply_find_hit(&g, state, bid, start, end);
                g.set_find_label(state.find_label().into());
            }
        }
        // A4 D7's other two shapes: a match with no row of its own. Each arm
        // turns one paragraph into a container the way the menu does, so the
        // hit has to ride on the row that paints it (ADR-0028) — and each
        // searches a word that only exists inside that container, because a
        // swept scene that paints nothing either way would pass.
        "find-grid" | "find-cols" => {
            let grid = scene == "find-grid";
            apply_scene(ui, state, if grid { "table" } else { "columns" });
            let term = if grid { "North" } else { "Column" };
            g.set_find_open(true);
            g.set_find_term(term.into());
            state.find_start(term);
            g.set_find_label(state.find_label().into());
        }
        // The fourth surface: a callout keeps its text in a tinted box of its
        // own, offset past the emoji, so the runs path needs its frame to mark
        // a match there. The arm turns the page's first paragraph into one and
        // does not step — the *selected* hit belongs to the block being
        // edited, and an editing block answers with a real text selection
        // instead of a cell, which is the right answer and the wrong demo.
        "find-callout" => {
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| {
                        b.kind == crate::core::BlockKind::Paragraph && b.text.contains("Atlas")
                    })
                    .map(|b| b.id)
            };
            let Some(id) = target else { return };
            let _ = state.exec_on_open_page(Command::SetBlockType {
                id,
                kind: crate::core::BlockKind::Callout,
            });
            g.set_find_open(true);
            g.set_find_term("the".into());
            state.find_start("the");
            g.set_find_label(state.find_label().into());
        }
        // SPEC §三十八's page look, one switch per scene so a moved pixel says
        // which token moved it. Each arm goes through the same state methods
        // the Style menu does — including the write to `pages` — because a
        // scene that set the UI global by hand would prove nothing about the
        // column the look is stored in.
        "style-serif" | "style-mono" | "style-small" | "style-full" | "style-tight" => {
            let page = state.open_page.get();
            match scene {
                "style-serif" => state.set_page_font(page, crate::core::PageFont::Serif),
                "style-mono" => state.set_page_font(page, crate::core::PageFont::Mono),
                "style-small" => state.toggle_page_small_text(page),
                "style-full" => state.toggle_page_full_width(page),
                // both at once: the tier shrinks AND widens, so a line that
                // used to wrap must say where it stopped
                _ => {
                    state.set_page_font(page, crate::core::PageFont::Serif);
                    state.toggle_page_small_text(page);
                    state.toggle_page_full_width(page);
                }
            }
        }
        "dark-style-serif" => {
            g.set_dark(true);
            apply_scene(ui, state, "style-serif");
        }
        // The page's emoji in the two places it draws: above its own title and
        // in its sidebar row. Set through the state method the picker calls,
        // so the shot proves the write as well as the pixels. The sidebar slot
        // is the interesting one: it carries the emoji into the dark.
        "page-icon" => {
            state.set_page_icon(state.open_page.get(), "\u{1f680}");
        }
        "dark-page-icon" => {
            g.set_dark(true);
            apply_scene(ui, state, "page-icon");
        }
        // The page's cover (SPEC §三十八, ADR-0047), written through the state
        // method the picker calls. `page-cover-white` is the arithmetic control:
        // the brightest picture a user can pick is a white one, so that scene
        // holds the worst contrast the scrim ever has to carry — a pass there is
        // a pass everywhere, and a failure there is not a taste argument.
        // `page-cover-icon` is the band's other shape, with the hero tall.
        "page-cover" | "page-cover-white" | "page-cover-icon" => {
            let aid = state.claim_attachment_id();
            let fixture = if scene == "page-cover-white" {
                state.store.create_solid_fixture(aid, 1280, 400, [255, 255, 255])
            } else {
                state.store.create_fixture(aid, 1280, 400)
            };
            let Some(att) = fixture else { return };
            state.set_page_cover_from(state.open_page.get(), att);
            if scene == "page-cover-icon" {
                state.set_page_icon(state.open_page.get(), "\u{1f680}");
            }
        }
        "dark-page-cover" => {
            g.set_dark(true);
            apply_scene(ui, state, "page-cover");
        }
        "dark-page-cover-white" => {
            g.set_dark(true);
            apply_scene(ui, state, "page-cover-white");
        }
        // The read-only switch (SPEC §三十八, ADR-0048), driven through the same
        // state method the ⋯ menu calls so the shot proves the write too. A
        // refusal is a *lack* of pixels, so what these scenes pin is the other
        // half of the promise — the state has to be visible: the pill over the
        // page, the ⋯ row that says "Unlock page", and the ⋮⋮ menu left with
        // only its two read-only rows. The refusals themselves are what
        // app::state's tests cover.
        "page-lock" => {
            state.set_page_locked(state.open_page.get(), true);
        }
        "page-lock-menu" => {
            let page = state.open_page.get();
            state.set_page_locked(page, true);
            state.fill_menu(page);
            g.set_menu_node_id(page);
            g.set_menu_y(TREE_TOP_PX + state.sidebar_row_y(page) as f32 - 4.0);
            g.set_menu_x(240.0);
            g.set_menu_open(true);
        }
        "page-lock-block-menu" => {
            let page = state.open_page.get();
            state.set_page_locked(page, true);
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(page))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = target {
                state.fill_block_menu(id, false);
                g.set_block_menu_x(320.0);
                g.set_block_menu_y(300.0);
                g.set_block_menu_open_id(id);
            }
        }
        "dark-page-lock" => {
            g.set_dark(true);
            apply_scene(ui, state, "page-lock");
        }
        // The three surfaces SPEC §三十八 "模板" adds (ADR-0049). A headless
        // session has no database, and `seed_builtin_templates` refuses to write
        // one without it on purpose, so the scene lands the same five bodies
        // through the channel the Import row uses — which is the feature's own
        // claim: a preset is Markdown, not a resource.
        "page-templates" => {
            seed_template_library(state);
            let page = state.open_page.get();
            state.fill_template_menu();
            g.set_menu_node_id(page);
            g.set_menu_y(TREE_TOP_PX + state.sidebar_row_y(page) as f32 - 4.0);
            g.set_menu_x(240.0);
            reanchor_page_menu(&g);
            g.set_menu_open(true);
        }
        // The Delete picker rather than the Insert one: the list of names is the
        // same, and this is the only place the library draws in the danger
        // colour, so the shot is the one that can be wrong in a new way.
        "page-template-pick" => {
            seed_template_library(state);
            let page = state.open_page.get();
            state.fill_template_pick(crate::app::state::MENU_TEMPLATE_DELETE);
            g.set_menu_node_id(page);
            g.set_menu_y(TREE_TOP_PX + state.sidebar_row_y(page) as f32 - 4.0);
            g.set_menu_x(240.0);
            reanchor_page_menu(&g);
            g.set_menu_open(true);
        }
        // A template row inside the "/" popup, with the block kinds filtered
        // away: the one picture of "it reads as a row of this menu, and its
        // hint says Template because a body has no breadcrumb".
        "slash-template" => {
            seed_template_library(state);
            state.open_slash("weekly");
            g.set_slash_focus(0);
            g.set_slash_x(340.0);
            g.set_slash_y(260.0);
            g.set_slash_open(true);
        }
        "dark-page-templates" => {
            g.set_dark(true);
            apply_scene(ui, state, "page-templates");
        }
        // Version history (SPEC §三十八, ADR-0091): the list, then one of its
        // rows opened into a comparison.
        "page-versions" => {
            seed_versions(state, &g, state.open_page.get());
        }
        "page-versions-diff" => {
            let page = state.open_page.get();
            let versions = seed_versions(state, &g, page);
            let (created, label) = &versions[1];
            let lines = {
                let doc = state.doc.borrow();
                let after = doc.page_blocks(core_page_id(page));
                // The same page as it stood three hours ago: one line reworded
                // in place, one line written since the snapshot (so it is in
                // `after` only), one line the page has since lost (in `before`
                // only). Built from the fixture's own blocks rather than
                // invented text so the panel's widest rows carry what this app
                // really draws, and put through the real comparator.
                let mut before: Vec<crate::core::Block> = after.to_vec();
                if let Some(edited) = before.iter_mut().find(|b| {
                    b.kind == crate::core::BlockKind::Paragraph
                        && b.parent.is_none()
                        && !b.text.is_empty()
                }) {
                    edited.text =
                        "Shipped the recap, and split the long section under it in two.".into();
                }
                if let Some(i) = before.iter().position(|b| {
                    b.kind == crate::core::BlockKind::Todo && b.parent.is_none()
                }) {
                    before.remove(i);
                }
                let lost = before
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Quote && b.parent.is_none())
                    .cloned()
                    .map(|mut b| {
                        b.id = crate::core::BlockId(9_999_997);
                        b.text = "Nothing else to add.".into();
                        b
                    });
                if let Some(lost) = lost {
                    before.insert(before.len() / 2, lost);
                }
                crate::core::diff::compare(&before, after)
            };
            g.set_versions_selected(1);
            g.set_versions_showing_diff(true);
            state.fill_version_diff(
                &lines,
                &crate::app::state::version_heading(label, *created),
                &crate::app::state::version_diff_note(&lines),
            );
        }
        "dark-page-versions" => {
            g.set_dark(true);
            apply_scene(ui, state, "page-versions");
        }
        "marks" => {
            // seed inline marks on the first paragraph (visual test only,
            // applied directly like an editor toggle would). Offsets are
            // derived from the words themselves — the A4 sweep caught the
            // old hardcoded bytes drifting off the words they demoed.
            let page = core_page_id(state.open_page.get());
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| (b.id, b.text.clone()))
            };
            if let Some((id, text)) = target {
                let span = |needle: &str, kind: crate::core::MarkKind| crate::core::Mark {
                    start: text.find(needle).unwrap_or(0),
                    end: text
                        .find(needle)
                        .map(|s| s + needle.len())
                        .unwrap_or(0),
                    kind,
                    url: if kind == crate::core::MarkKind::Link {
                        "https://example.com".into()
                    } else {
                        String::new()
                    },
                    date: None,
                };
                let marks = vec![
                    span("home", crate::core::MarkKind::Bold),
                    span("for thinking.", crate::core::MarkKind::Bold),
                    span("collects notes", crate::core::MarkKind::Strike),
                    span("Quire itself", crate::core::MarkKind::Code),
                    span("plans, and references", crate::core::MarkKind::Link),
                ];
                state
                    .doc
                    .borrow_mut()
                    .apply(&[crate::core::Change::BlockMarksSet { id, marks }]);
                state.reproject_blocks();
            }
        }
        "marks-wrap" => {
            // A marked paragraph long enough to need several lines (ADR-0041):
            // `marks` is one line deep, and the wall this scene watches is the
            // one below the first — where the runs break, and whether the row
            // is tall enough to show all of it.
            let page = core_page_id(state.open_page.get());
            let text = "The landing page is a fixture and not a promise: every line of it is painted by the same delegate that paints a ten thousand line page, so what wraps here wraps there. A bold phrase in the middle of a sentence, an italic word, a code span like cargo build --release, and a link all arrive as runs, and a run is one cell of a layout that breaks between cells.";
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
            };
            if let Some(id) = target {
                // a needle that misses must not silently mark the head of the
                // line — that is how `marks` once demoed the wrong words
                let span = |needle: &str, kind: crate::core::MarkKind| crate::core::Mark {
                    start: text.find(needle).expect("needle present"),
                    end: text.find(needle).expect("needle present") + needle.len(),
                    kind,
                    url: if kind == crate::core::MarkKind::Link {
                        "https://example.com".into()
                    } else {
                        String::new()
                    },
                    date: None,
                };
                let marks = vec![
                    span("and not a promise", crate::core::MarkKind::Bold),
                    span("delegate", crate::core::MarkKind::Italic),
                    span("cargo build --release", crate::core::MarkKind::Code),
                    span("breaks between cells", crate::core::MarkKind::Link),
                ];
                state.doc.borrow_mut().apply(&[
                    crate::core::Change::BlockTextSet {
                        id,
                        text: text.into(),
                    },
                    crate::core::Change::BlockMarksSet { id, marks },
                ]);
                state.reproject_blocks();
            }
        }
        "edit" => {
            // focus the first paragraph of the landing page
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| (b.id.0 as i32, b.text.len() as i32, b.text.clone()))
            };
            if let Some((id, len, text)) = target {
                g.set_editing_text(text.into());
                g.set_pending_caret(len);
                g.set_editing_id(id);
            }
        }
        "block-colors" => {
            // color a few blocks + add a callout near the top (visual test
            // only; applied directly like the menu would)
            let page = core_page_id(state.open_page.get());
            let mut colors: Vec<(BlockId, crate::core::ColorKind, crate::core::ColorKind)> =
                {
                    let d = state.doc.borrow();
                    let blocks = d.page_blocks(page);
                    let mut v = Vec::new();
                    if let Some(h) = blocks
                        .iter()
                        .find(|b| matches!(b.kind, crate::core::BlockKind::Heading1 | crate::core::BlockKind::Heading2 | crate::core::BlockKind::Heading3))
                    {
                        v.push((h.id, crate::core::ColorKind::Blue, crate::core::ColorKind::Default));
                    }
                    if let Some(p) = blocks
                        .iter()
                        .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    {
                        v.push((p.id, crate::core::ColorKind::Red, crate::core::ColorKind::Default));
                    }
                    if let Some(q) = blocks.iter().find(|b| b.kind == crate::core::BlockKind::Quote) {
                        v.push((q.id, crate::core::ColorKind::Green, crate::core::ColorKind::Yellow));
                    }
                    if let Some(t) = blocks.iter().find(|b| b.kind == crate::core::BlockKind::Todo) {
                        v.push((t.id, crate::core::ColorKind::Default, crate::core::ColorKind::Blue));
                    }
                    // Orange and yellow text on their own tint used to be in
                    // this scene only as a menu dot, which is exactly the pair
                    // the palette's contrast slice moved -- a hex ratio needs a
                    // row of glyphs to be worth reading.
                    let taken: Vec<BlockId> = v.iter().map(|(id, ..)| *id).collect();
                    let mut rest = blocks
                        .iter()
                        .filter(|b| !b.text.is_empty() && !taken.contains(&b.id));
                    for c in [
                        crate::core::ColorKind::Orange,
                        crate::core::ColorKind::Yellow,
                    ] {
                        if let Some(b) = rest.by_ref().next() {
                            v.push((b.id, c, c));
                        }
                    }
                    v
                };
            let doc_changes: Vec<crate::core::Change> = colors
                .drain(..)
                .map(|(id, c, bg)| crate::core::Change::BlockColorSet { id, color: c, background: bg })
                .collect();
            state.doc.borrow_mut().apply(&doc_changes);
            // the callout goes right under the first paragraph so it is on
            // screen in a headless capture
            let anchor = {
                let d = state.doc.borrow();
                d.page_blocks(page)
                    .iter()
                    .find(|b| b.kind == crate::core::BlockKind::Paragraph && !b.text.is_empty())
                    .map(|b| b.id)
                    .or_else(|| d.page_blocks(page).last().map(|b| b.id))
                    .unwrap_or(BlockId(1))
            };
            let _ = state.exec_on_open_page(Command::InsertBlockAfter {
                id: anchor,
                kind: crate::core::BlockKind::Callout,
                text: "Callouts stand out — an emoji, a tinted box, and text.".into(),
            });
            state.reproject_blocks();
        }
        _ => {}
    }
}

/// Popup-opening half of a scene: called AFTER the first render pass so the
/// delegate-owned popups (slash, block menu) see a false -> true transition
/// and their `changed` handlers fire.
pub fn apply_scene_overlay(ui: &AppWindow, state: &Rc<AppState>, scene: &str) {
    let g = ui.global::<UIState>();
    match scene {
        "palette" => g.set_palette_open(true),
        // The Navigate rows sit below the palette's visible fold when the query
        // is empty, so a scene that filters to them is the only way to see them.
        "palette-nav" => {
            state.set_query("go");
            g.set_palette_query("go".into());
            g.set_palette_open(true);
        }
        "search" => {
            g.set_search_open(true);
        }
        "search-notes" => {
            g.set_search_query("notes".into());
            // headless: blocking query (the GUI path polls async instead)
            state.set_search_rows_sync("notes");
            g.set_search_open(true);
        }
        "menu" => {
            state.fill_menu(106);
            g.set_menu_node_id(106);
            let row_y = state.sidebar_row_y(106) as f32;
            g.set_menu_y(TREE_TOP_PX + row_y - 4.0);
            g.set_menu_x(240.0);
            g.set_menu_open(true);
        }
        "page-move-to" => {
            // the sidebar menu's Move-to submenu for page 106: Back, Top
            // level, then every legal target (106's own subtree is skipped)
            state.fill_page_menu_move_to(106);
            g.set_menu_node_id(106);
            let row_y = state.sidebar_row_y(106) as f32;
            g.set_menu_y(TREE_TOP_PX + row_y - 4.0);
            g.set_menu_x(240.0);
            g.set_menu_open(true);
        }
        "page-style" => {
            // The Style submenu itself, with the checks on the row the page
            // stores — the only picture of the menu's other half.
            state.set_page_font(106, crate::core::PageFont::Serif);
            state.toggle_page_small_text(106);
            state.fill_page_menu_style(106);
            g.set_menu_node_id(106);
            let row_y = state.sidebar_row_y(106) as f32;
            g.set_menu_y(TREE_TOP_PX + row_y - 4.0);
            g.set_menu_x(240.0);
            g.set_menu_open(true);
        }
        "page-icon" => {
            // The page's emoji in the two places it draws: above its own
            // title and in its sidebar row. Set through the state method the
            // picker calls, so the shot proves the write as well as pixels.
            let page = state.open_page.get();
            state.set_page_icon(page, "\u{1f680}");
        }
        "icon-picker" => {
            let page = state.open_page.get();
            state.fill_icon_picker(page);
            g.set_icon_picker_x(240.);
            g.set_icon_picker_y(TREE_TOP_PX + state.sidebar_row_y(page) as f32);
            g.set_icon_picker_open(true);
        }
        "dialog" => {
            let (_title, message) = state.delete_dialog_text(105);
            g.set_dialog_title("删除页面？".into());
            g.set_dialog_message(message.into());
            g.set_dialog_open(true);
        }
        "settings" => g.set_settings_open(true),
        "dark-slash" => apply_scene_overlay(ui, state, "slash"),
        "slash" => {
            state.open_slash("");
            g.set_slash_focus(0);
            // headless estimate: landing-page paragraph position
            g.set_slash_x(340.0);
            g.set_slash_y(260.0);
            g.set_slash_open(true);
        }
        "block-menu" => {
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = target {
                state.fill_block_menu(id, false);
                g.set_block_menu_x(320.0);
                g.set_block_menu_y(300.0);
                g.set_block_menu_open_id(id);
            }
        }
        // The phone's shape: touch mode mounts the thumb bar and the drawer
        // rule (a closed sidebar — the startup state `launcher::run` gives a
        // real device), and the long-press menu's rows stand at the 44 dp
        // minimum with the touch-only "Insert below" row on them. The
        // long-press itself cannot be shot headlessly; this scene is the
        // menu it produces.
        "touch-menu" => {
            g.set_touch_mode(true);
            g.set_sidebar_open(false);
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = target {
                state.fill_block_menu(id, true);
                g.set_block_menu_x(320.0);
                g.set_block_menu_y(300.0);
                g.set_block_menu_open_id(id);
            }
        }
        // the "+"-handle insert menu, anchored below an empty new line
        "plus" => {
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = target {
                g.invoke_block_plus(id, 280.0, 340.0);
            }
        }
        "move-to" => {
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = target {
                state.fill_block_menu_move_to();
                g.set_block_menu_x(320.0);
                g.set_block_menu_y(120.0);
                g.set_block_menu_open_id(id);
            }
        }
        // the ⋮⋮ menu's Move-to submenu in a workspace big enough that the
        // list is taller than the space below its anchor: the shot is taken
        // after the action callback re-anchors (the `reanchor` closure in
        // `on_block_menu_action`), so the popup's bottom edge must sit on the
        // window — that edge is the defect this scene exists to prove. Pages
        // are added until the rows clear 22 (≈ 632 px of menu against 200 px
        // of room below a y = 600 anchor), so the reading does not depend on
        // how many pages the base document happens to carry.
        "move-to-tall" => {
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            let Some(id) = target else { return };
            loop {
                state.fill_block_menu_move_to();
                if g.get_block_menu_rows().row_count() >= 22 {
                    break;
                }
                state.create_page(None);
            }
            state.fill_block_menu(id, false);
            g.set_block_menu_x(320.0);
            g.set_block_menu_y(600.0);
            g.set_block_menu_open_id(id);
            g.invoke_block_menu_action(10);
        }
        "text-color" => {
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = target {
                state.fill_block_menu_colors(id, false);
                g.set_block_menu_x(320.0);
                g.set_block_menu_y(200.0);
                g.set_block_menu_open_id(id);
            }
        }
        "bg-color" => {
            let target = {
                let d = state.doc.borrow();
                d.page_blocks(core_page_id(state.open_page.get()))
                    .get(4)
                    .map(|b| b.id.0 as i32)
            };
            if let Some(id) = target {
                state.fill_block_menu_colors(id, true);
                g.set_block_menu_x(320.0);
                g.set_block_menu_y(120.0);
                g.set_block_menu_open_id(id);
            }
        }
        "link-dlg" => {
            g.set_link_start(0);
            g.set_link_end(20);
            g.set_link_url("https://github.com/slint-ui/slint".into());
            g.set_link_open(true);
        }

        // SPEC §三十九 「属性」 (D10): the three popups' open bits, one render
        // pass after their content landed in `apply_scene`. Each is guarded by
        // the id its seed wrote — a scene whose seed refused leaves the popup
        // shut rather than showing an empty one.
        "database-kinds" => {
            if g.get_db_columns_property() >= 0 {
                g.set_db_columns_open(true);
            }
        }
        "database-relation" => {
            if g.get_db_relation_property() >= 0 {
                g.set_db_relation_open(true);
            }
        }
        "database-rollup" => {
            if g.get_db_rollup_property() >= 0 {
                g.set_db_rollup_open(true);
            }
        }
        "dark-database-kinds" => apply_scene_overlay(ui, state, "database-kinds"),
        "dark-database-relation" => apply_scene_overlay(ui, state, "database-relation"),
        "dark-database-rollup" => apply_scene_overlay(ui, state, "database-rollup"),

        _ => {}
    }
}
