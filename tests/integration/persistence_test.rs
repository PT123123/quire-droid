// End-to-end checks for the debounced persistence pipeline against a
// real SQLite file: typing-rate bursts coalesce, the quiet window gates
// writes, Ctrl+S / shutdown flushes land before the next "session" loads
// the file, and the periodic snapshot (M8 D10) reaches the `.bak<N>` family.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use quire::core::persistence::{Change, Repository};
use quire::core::types::{Block, BlockId, BlockKind, Lang, OrderKey, Page, PageFont, PageId};
use quire::services::persistence::{
    FakeClock, PersistenceService, DEFAULT_DEBOUNCE_MS, DEFAULT_SNAPSHOT_INTERVAL_MS,
};
use quire::storage::{backup, SqliteRepository};
use quire::testing::ScratchDir;
// The library target is `quire_shell`, so it never shares its PDB with the
// binaries — the alias keeps every path above reading as `quire::`
// (Cargo.toml's `[lib]` carries the reason).
use quire_shell as quire;

/// A database path, and the folder holding it. The folder comes back as a
/// guard that deletes itself, because a session leaves `.bak<N>` snapshots
/// beside the file — cleaning up the database alone would strand those, and
/// cleaning up nothing at all is what four hundred leftover `quire-persist-*`
/// directories in `%TEMP%` were the evidence of.
fn temp_db(name: &str) -> (ScratchDir, PathBuf) {
    let dir = ScratchDir::new(&format!("persist-{name}"));
    let path = dir.join("quire.db");
    (dir, path)
}

fn seeded(path: &Path) -> Arc<SqliteRepository> {
    let repo = Arc::new(SqliteRepository::open(path).unwrap());
    repo.apply(&[
        Change::PageCreated(Page {
            id: PageId(1),
            title: "Editor scratch".into(),
            parent: None,
            order: OrderKey::FIRST,
            favorite: false,
            expanded: true,
            font: PageFont::default(),
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: false,
        }),
        Change::BlockInserted(Block {
            id: BlockId(1),
            page: PageId(1),
            parent: None,
            order: OrderKey::FIRST,
            kind: BlockKind::Paragraph,
            text: String::new(),
            checked: false,
                marks: Vec::new(),
        color: quire::core::ColorKind::Default,
        background: quire::core::ColorKind::Default,
        page_ref: None,
        folded: false,
        attachment: None,
        img_percent: 100,
        columns: 0,
        lang: Lang::Plain,
        db_ref: None,
        sync_ref: None,        }),
    ])
    .unwrap();
    repo
}

#[test]
fn burst_then_quiet_writes_exactly_once() {
    let (_dir, path) = temp_db("burst");
    let repo = seeded(&path);
    let clock = Arc::new(FakeClock::new());
    let svc = PersistenceService::new(repo.clone(), clock.clone(), 300);

    // the M4 editor will send one batch per keystroke-equivalent command;
    // simulate 50 rapid text updates
    for i in 0..50 {
        clock.set(i * 5); // 5 ms apart: never quiet long enough
        svc.record(vec![Change::BlockTextSet {
            id: BlockId(1),
            text: format!("typed {} chars", i + 1),
        }]);
        assert_eq!(svc.flush_if_due().unwrap(), false);
    }
    let loaded = repo.load().unwrap();
    assert_eq!(loaded.blocks[0].text, "", "nothing may hit the db mid-burst");

    clock.set(1000); // user stops typing
    assert_eq!(svc.flush_if_due().unwrap(), true);
    let loaded = repo.load().unwrap();
    assert_eq!(loaded.blocks[0].text, "typed 50 chars");

    // quiet afterwards: no spinning writes without new changes
    clock.set(5000);
    assert_eq!(svc.flush_if_due().unwrap(), false);
}

#[test]
fn shutdown_flush_then_next_session_loads_it() {
    let (_dir, path) = temp_db("session");
    {
        let repo = seeded(&path);
        let clock = Arc::new(FakeClock::new());
        let svc = PersistenceService::new(repo.clone(), clock.clone(), 300);
        svc.record(vec![Change::BlockInserted(Block {
            id: BlockId(2),
            page: PageId(1),
            parent: Some(BlockId(1)),
            order: OrderKey::FIRST,
            kind: BlockKind::Todo,
            text: "未保存的中文草稿".into(),
            checked: true,
                marks: Vec::new(),
        color: quire::core::ColorKind::Default,
        background: quire::core::ColorKind::Default,
        page_ref: None,
        folded: false,
        attachment: None,
        img_percent: 100,
        columns: 0,
        lang: Lang::Plain,
        db_ref: None,
        sync_ref: None,        })]);
        clock.set(10); // nowhere near due — this is the Ctrl+S/quit path
        svc.force_flush().unwrap();
        assert!(!svc.has_pending());
    }
    let reopened = SqliteRepository::open(&path).unwrap();
    let state = reopened.load().unwrap();
    let todo = state.blocks.iter().find(|b| b.id == BlockId(2)).unwrap();
    assert_eq!(todo.text, "未保存的中文草稿");
    assert_eq!(todo.kind, BlockKind::Todo);
    assert!(todo.checked);
    assert_eq!(todo.parent, Some(BlockId(1)));
}

#[test]
fn deadline_guides_the_timer_and_multiple_bursts_stay_ordered() {
    let (_dir, path) = temp_db("deadline");
    let repo = seeded(&path);
    let clock = Arc::new(FakeClock::new());
    clock.set(1000);
    let svc = PersistenceService::new(repo.clone(), clock.clone(), 300);
    assert_eq!(svc.next_deadline_ms(), None);

    svc.record(vec![Change::PageTitleSet {
        id: PageId(1),
        title: "A".into(),
    }]);
    assert_eq!(svc.next_deadline_ms(), Some(1300));
    clock.set(1299);
    assert_eq!(svc.flush_if_due().unwrap(), false);
    clock.set(1300);
    assert_eq!(svc.flush_if_due().unwrap(), true);

    // two later bursts: last write per id must win, order inside a batch kept
    svc.record(vec![Change::PageTitleSet {
        id: PageId(1),
        title: "B".into(),
    }]);
    svc.record(vec![Change::PageTitleSet {
        id: PageId(1),
        title: "C".into(),
    }]);
    clock.set(1700);
    assert_eq!(svc.flush_if_due().unwrap(), true);
    let state = repo.load().unwrap();
    assert_eq!(state.pages[0].title, "C");
}

/// M8 D10: the periodic snapshot runs through the flush path the app already
/// ticks, against the real file, so a mid-session corruption costs the edits
/// made since the last tick instead of everything since startup.
#[test]
fn a_quiet_period_after_a_write_reaches_the_snapshot_family() {
    let (_dir, path) = temp_db("periodic-snapshot");
    let repo = seeded(&path); // this open wrote the startup snapshot
    let clock = Arc::new(FakeClock::new());
    let svc = PersistenceService::new(repo.clone(), clock.clone(), DEFAULT_DEBOUNCE_MS)
        .with_database_snapshots(&repo);

    // the edit lands, is written by the debounce, and the period then elapses
    svc.record(vec![Change::PageTitleSet {
        id: PageId(1),
        title: "backed up mid-session".into(),
    }]);
    clock.set(DEFAULT_DEBOUNCE_MS);
    assert!(svc.flush_if_due().unwrap());
    assert_eq!(
        1,
        snapshots_of(&path),
        "the period has not passed: `.bak1` is still the startup copy"
    );
    assert_eq!(None, title_at(&backup::slot(&path, 1)));

    clock.set(DEFAULT_SNAPSHOT_INTERVAL_MS + 1);
    assert!(!svc.flush_if_due().unwrap(), "nothing new to write");
    assert_eq!(
        Some("backed up mid-session".to_string()),
        title_at(&backup::slot(&path, 1)),
        "the tick that closed the period took the snapshot"
    );
    assert_eq!(2, snapshots_of(&path), "and pushed the startup copy down");

    // a further period with no write takes none: `pending` gates it
    clock.set(DEFAULT_SNAPSHOT_INTERVAL_MS * 2);
    assert!(!svc.flush_if_due().unwrap());
    assert_eq!(
        Some("backed up mid-session".to_string()),
        title_at(&backup::slot(&path, 1))
    );
    drop(svc);
    drop(repo);
}

/// How many generations the family at `path` currently holds.
fn snapshots_of(path: &Path) -> usize {
    (1..=backup::KEEP)
        .filter(|index| backup::slot(path, *index).exists())
        .count()
}

/// Read one page title out of a database file without opening it for write.
fn title_at(path: &Path) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    conn.query_row("SELECT title FROM pages WHERE id = 1", [], |r| {
        r.get::<_, String>(0)
    })
    .ok()
}

#[test]
fn settings_storage_row_reports_the_folder_and_snapshots_on_demand() {
    use quire::app::state::{AppState, HandleArgs};

    let (_dir, path) = temp_db("settings-storage-row");
    let repo = seeded(&path);
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, Some(repo.clone()));

    // the storage row shows the database's parent directory
    let dir = state.data_dir().expect("a file-backed session has a data dir");
    assert_eq!(
        std::path::Path::new(&dir).canonicalize().unwrap(),
        path.parent().unwrap().canonicalize().unwrap()
    );

    // "Back up now" must capture CURRENT state: a probe written after the
    // open-time snapshot may only reach .bak1 through the manual backup
    repo.apply(&[Change::SettingSet {
        key: "backup-probe".into(),
        value: "in".into(),
    }])
    .unwrap();
    assert!(state.backup_now().is_ok());

    let bak = std::path::PathBuf::from(format!("{}.bak1", path.display()));
    assert!(bak.exists(), "the on-demand snapshot landed beside the db");
    let probe = rusqlite::Connection::open_with_flags(
        bak,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
    .and_then(|c| {
        c.query_row(
            "SELECT value FROM settings WHERE key = 'backup-probe'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
    });
    assert_eq!(
        probe.as_deref(),
        Some("in"),
        "the probe row is inside the fresh .bak1"
    );
}
