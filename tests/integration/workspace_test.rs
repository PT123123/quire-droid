// Integration tests: exercise the workspace model and the state projection
// through the public crate API, the way a future `core/` consumer would.

use quire::app::state::{core_page_id, AppState, HandleArgs, BLOCK_PARAGRAPH, PAGE_GETTING_STARTED};
use quire::app::workspace::Workspace;
// The library target is `quire_shell`, so it never shares its PDB with the
// binaries — the alias keeps every path here reading as `quire::`
// (Cargo.toml's `[lib]` carries the reason).
use quire_shell as quire;
use slint::Model;

#[test]
fn tree_operations_survive_a_full_lifecycle() {
    let mut ws = Workspace::sample();
    let page = ws.create(Some(102), "Child");
    ws.rename(page, "Renamed Child");
    assert_eq!(ws.title_of(page), Some("Renamed Child"));

    // Getting Started + its 2 sample children + the new child
    let copy = ws.duplicate(102).expect("duplicate root");
    assert_eq!(ws.subtree_size(copy), 4);

    let removed = ws.delete(copy);
    assert_eq!(removed.len(), 4);
    assert!(!ws.contains(copy));
    assert!(ws.contains(page)); // original untouched
}

#[test]
fn recents_cap_and_order() {
    let mut ws = Workspace::sample();
    ws.mark_opened(112);
    assert_eq!(ws.recents()[0].0, 112);
    // only real pages land in recents; opening 6 more pushes 112 out
    for id in [103, 104, 106, 107, 108, 109] {
        ws.mark_opened(id);
    }
    assert_eq!(ws.recents().len(), quire::app::workspace::MAX_RECENTS);
    assert_eq!(ws.recents()[0].0, 109);
    assert!(ws.recents().iter().all(|(id, _)| *id != 112));
}

#[test]
fn search_finds_unopened_page_content() {
    // content blobs are filled at construction, so search works before a
    // page is ever opened
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let hits = state.workspace.borrow().search("字体回退");
    assert!(
        hits.iter().any(|h| h.id == 112 && h.snippet.contains("字体回退")),
        "chinese fixture page should match by content"
    );
}

#[test]
fn sidebar_projection_shape() {
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let rows = state.build_sidebar_rows();
    // sections in order, y offsets strictly increasing, no duplicate ids
    let kinds: Vec<&str> = rows.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds[0], "header");
    assert!(rows.iter().any(|r| r.kind == "favorite"));
    assert!(rows.iter().any(|r| r.kind == "recent"));
    assert!(rows.iter().any(|r| r.kind == "new-page"));
    for w in rows.windows(2) {
        assert!(w[0].y < w[1].y, "y offsets must increase");
    }
    // the open page (Getting Started) is selected in the tree
    assert!(
        rows.iter()
            .any(|r| r.kind == "page" && r.selected && r.id == 102)
    );
}

#[test]
fn create_then_open_page_lands_empty() {
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let id = state.create_page(None);
    assert_eq!(state.open_page.get(), id);
    assert_eq!(state.blocks.row_count(), 0, "new page shows the empty state");
    assert_eq!(state.workspace.borrow().title_of(id), Some("无标题"));
    // ...and the empty state is not a dead end: it can make its own first
    // row, which is the only insert in the app with nothing to anchor on
    let started = state.start_page().expect("the empty page takes a paragraph");
    assert_eq!(state.blocks.row_count(), 1);
    let row = state.blocks.row_data(0).unwrap();
    assert_eq!(row.id, started);
    assert_eq!(row.kind, BLOCK_PARAGRAPH);
    assert!(row.text.is_empty());
}

#[test]
fn duplicated_nested_list_keeps_parents_in_the_copy() {
    // page with a bullet + a nested child (parent pointer inside the page)
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let page = state.create_page(None);
    let root = quire::core::BlockId(9_000_000_001);
    let child = quire::core::BlockId(9_000_000_002);
    state.doc.borrow_mut().set_page_blocks(
        quire::core::PageId(page as u32 as u64),
        vec![
            quire::core::Block {
                id: root,
                page: quire::core::PageId(page as u32 as u64),
                parent: None,
                order: quire::core::OrderKey(10),
                kind: quire::core::BlockKind::Bullet,
                text: "parent item".into(),
                checked: false,
                marks: Vec::new(),
        color: quire::core::ColorKind::Default,
        background: quire::core::ColorKind::Default,
        page_ref: None,
        folded: false,
        attachment: None,
        img_percent: 100,
        columns: 0,
        lang: quire::core::Lang::Plain,
        db_ref: None,
        sync_ref: None,            },
            quire::core::Block {
                id: child,
                page: quire::core::PageId(page as u32 as u64),
                parent: Some(root),
                order: quire::core::OrderKey(11),
                kind: quire::core::BlockKind::Bullet,
                text: "child item".into(),
                checked: false,
                marks: Vec::new(),
        color: quire::core::ColorKind::Default,
        background: quire::core::ColorKind::Default,
        page_ref: None,
        folded: false,
        attachment: None,
        img_percent: 100,
        columns: 0,
        lang: quire::core::Lang::Plain,
        db_ref: None,
        sync_ref: None,            },
        ],
    );

    let copy = state.duplicate_page(page).expect("duplicate");
    let cpid = quire::core::PageId(copy as u32 as u64);
    let blocks = state.doc.borrow().page_blocks(cpid).to_vec();
    assert_eq!(blocks.len(), 2, "copy carries both blocks");
    // the child in the COPY points at the copied root, not the original
    let copied_child = blocks.iter().find(|b| b.text == "child item").unwrap();
    let copied_root = blocks.iter().find(|b| b.text == "parent item").unwrap();
    assert_ne!(copied_child.parent, Some(root), "stale parent pointer");
    assert_eq!(copied_child.parent, Some(copied_root.id));
    // depth projection agrees (both render, child indented)
    let rows = quire::app::state::project_blocks(
        &blocks,
        &Default::default(),
        &quire::app::state::MentionTitles::empty(),
    );
    assert_eq!(rows[0].depth, 0);
    assert_eq!(rows[1].depth, 1);
}

#[test]
fn a_page_look_is_set_duplicated_and_left_alone_by_its_copy() {
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let page = state.open_page.get();
    state.set_page_font(page, quire::core::PageFont::Serif);
    state.toggle_page_full_width(page);
    state.toggle_page_small_text(page);
    assert_eq!(
        state.workspace.borrow().page_style(page),
        Some((quire::core::PageFont::Serif, true, true)),
        "the three switches are three facts about the page"
    );

    let copy = state.duplicate_page(page).expect("duplicate");
    assert_eq!(
        state.workspace.borrow().page_style(copy),
        Some((quire::core::PageFont::Serif, true, true)),
        "a duplicate is a copy of the page, and its look is part of it"
    );

    // and the two pages own their look separately from here on
    state.toggle_page_full_width(page);
    assert_eq!(
        state.workspace.borrow().page_style(page),
        Some((quire::core::PageFont::Serif, false, true))
    );
    assert_eq!(
        state.workspace.borrow().page_style(copy),
        Some((quire::core::PageFont::Serif, true, true)),
        "flipping the original must not reach into the copy"
    );
}

#[test]
fn delete_open_page_resets_selection() {
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let id = state.create_page(None);
    assert!(state.delete_page(id), "deleting the open page is reported");
    assert_eq!(state.open_page.get(), 0, "selection resets to no-page");
    // the controller picks the fallback; simulate that path
    let fallback = state.workspace.borrow().first_root();
    state.open_page(fallback.expect("sample keeps a root"));
    assert_eq!(state.open_page.get(), fallback.unwrap());
}

#[test]
fn move_page_reparents_refuses_cycles_and_swaps_siblings() {
    use quire::app::state::PAGE_ATLAS;

    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);

    // reparent: Atlas moves under Getting Started, appended last
    assert!(state.move_page(PAGE_ATLAS, Some(PAGE_GETTING_STARTED)));
    {
        let ws = state.workspace.borrow();
        let kids = ws.children_of(Some(PAGE_GETTING_STARTED));
        assert!(kids.contains(&PAGE_ATLAS), "Atlas is a child now");
        assert!(!ws.children_of(None).contains(&PAGE_ATLAS), "no longer a root");
        assert_eq!(
            ws.get(PAGE_ATLAS).unwrap().parent,
            Some(PAGE_GETTING_STARTED)
        );
    }

    // a page cannot move into its own subtree (Getting Started under Atlas)
    assert!(!state.move_page(PAGE_GETTING_STARTED, Some(PAGE_ATLAS)));

    // sibling swap: Atlas lands one slot up among the children
    let kids = state.workspace.borrow().children_of(Some(PAGE_GETTING_STARTED));
    let count = kids.len();
    let before_neighbor = kids[count - 2];
    assert!(state.move_page_by(PAGE_ATLAS, -1));
    let kids = state.workspace.borrow().children_of(Some(PAGE_GETTING_STARTED));
    assert_eq!(kids.len(), count);
    assert_eq!(kids[count - 2], PAGE_ATLAS, "swapped one slot up");
    assert_eq!(kids[count - 1], before_neighbor);

    // the first child cannot move up further
    let first = kids[0];
    assert!(!state.move_page_by(first, -1));

    // move back to the top level: the page is a root again
    assert!(state.move_page(PAGE_ATLAS, None));
    assert!(state.workspace.borrow().children_of(None).contains(&PAGE_ATLAS));
}

#[test]
fn duplicate_reserves_its_id_range() {
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let copy = state
        .duplicate_page(PAGE_GETTING_STARTED)
        .expect("the sample page duplicates");
    let copied_ids: Vec<u64> = {
        let doc = state.doc.borrow();
        doc.page_blocks(core_page_id(copy))
            .iter()
            .map(|b| b.id.as_u64())
            .collect()
    };
    assert!(!copied_ids.is_empty(), "the sample page carries blocks");
    // the whole copied range sits strictly below the next allocation — ids
    // minted without reserving collided with it (M8_FEEDBACK #2)
    let next = state.doc.borrow().next_id_value();
    assert!(
        copied_ids.iter().all(|&id| id < next),
        "next allocation {} would collide with a copied id",
        next
    );
    let fresh = state.doc.borrow_mut().alloc_block_id();
    assert!(!copied_ids.contains(&fresh.as_u64()));
}

#[test]
fn the_page_picker_lists_pages_and_the_link_conversion_guards_its_inputs() {
    use quire::app::state::PAGE_ATLAS;
    use slint::Model;

    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);

    // the picker lists every page in tree order; typing filters by title
    state.open_slash_pick("");
    let all = state.slash_model().row_count();
    assert!(all >= 5, "the sample workspace carries pages");
    state.open_slash_pick("getting");
    assert_eq!(state.slash_model().row_count(), 1);
    let picked = state.slash_selected_page(0).expect("a filtered row is a page");
    assert_eq!(
        state.workspace.borrow().title_of(picked),
        Some("Getting Started")
    );

    // the link conversion guards: only an EMPTY PARAGRAPH converts, and the
    // target must exist. The first block of Getting Started is text → refuse
    let first = {
        let doc = state.doc.borrow();
        doc.page_blocks(quire::app::state::core_page_id(PAGE_GETTING_STARTED))[0].id.0 as i32
    };
    assert!(!state.create_page_link_block(first, PAGE_ATLAS));

    // the real flow: the "+" handle first inserts the empty line, the
    // picker then converts it. An empty paragraph converts; the row reads
    // the target's live title on reproject
    state.open_page(PAGE_ATLAS);
    let tail = {
        let doc = state.doc.borrow();
        doc.page_blocks(quire::app::state::core_page_id(PAGE_ATLAS))
            .last()
            .unwrap()
            .id
            .0 as i32
    };
    let changes = state.exec_on_open_page(quire::core::Command::InsertBlockAfter {
        id: quire::core::BlockId(tail as u64),
        kind: quire::core::BlockKind::Paragraph,
        text: String::new(),
    });
    let fresh = changes
        .as_deref()
        .and_then(|cs| {
            cs.iter().find_map(|c| match c {
                quire::core::Change::BlockInserted(b) => Some(b.id.0 as i32),
                _ => None,
            })
        })
        .expect("the insert lands");
    assert!(state.create_page_link_block(fresh, PAGE_GETTING_STARTED));
    let doc = state.doc.borrow();
    let b = doc
        .block(quire::core::BlockId(fresh as u64))
        .expect("the converted block exists");
    assert_eq!(b.kind, quire::core::BlockKind::Link);
    assert_eq!(
        b.page_ref,
        Some(quire::core::PageId(PAGE_GETTING_STARTED as u64))
    );
}

#[test]
fn page_tree_drag_moves_pages_and_refuses_illegal_lands() {
    use quire::app::state::PAGE_ATLAS;
    use slint::Model;

    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let rows = state.sidebar_model().row_count() as i32;

    let header_row = (0..rows)
        .find(|&i| state.page_drop_target(i) == Some(None))
        .expect("the Workspace header targets the top level");
    let atlas_row = (0..rows)
        .find(|&i| state.page_drop_target(i) == Some(Some(PAGE_ATLAS)))
        .expect("Atlas has a page row");

    // drop Getting Started onto Atlas: it nests as Atlas's child
    assert!(state.page_dropped(PAGE_GETTING_STARTED, atlas_row));
    assert_eq!(
        state.workspace.borrow().get(PAGE_GETTING_STARTED).unwrap().parent,
        Some(PAGE_ATLAS)
    );

    // dropping the parent onto its own child is a cycle: refused, tree intact
    let gs_row = (0..rows)
        .find(|&i| state.page_drop_target(i) == Some(Some(PAGE_GETTING_STARTED)))
        .expect("Getting Started has a row after nesting");
    assert!(!state.page_dropped(PAGE_ATLAS, gs_row));
    assert_eq!(
        state.workspace.borrow().get(PAGE_GETTING_STARTED).unwrap().parent,
        Some(PAGE_ATLAS),
        "the refused drop left the tree alone"
    );

    // non-target rows (favorites, recents, new-page) reject drops
    let non_target = (0..rows)
        .find(|&i| state.page_drop_target(i).is_none())
        .expect("favorites/recents/new-page rows are not targets");
    assert!(!state.page_dropped(PAGE_ATLAS, non_target));

    // back to the top level via the Workspace header
    assert!(state.page_dropped(PAGE_GETTING_STARTED, header_row));
    assert_eq!(
        state.workspace.borrow().get(PAGE_GETTING_STARTED).unwrap().parent,
        None
    );
}

#[test]
fn duplicate_appends_consistently_when_the_gap_is_exhausted() {
    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);

    // two fresh roots get consecutive order keys, so duplicating the first
    // finds no gap and falls back to appending at the end of the run
    let a = state.create_page(None);
    let b = state.create_page(None);
    let copy = state.duplicate_page(a).expect("duplicates");

    // the key ordering puts the copy last...
    assert!(state.page_order_of(copy) > state.page_order_of(a));
    assert!(state.page_order_of(copy) > state.page_order_of(b));
    // ...and the session's children vec agrees with the keys — the old
    // drift had the vec showing the copy adjacent while the restart
    // projection (key order) sorted it last
    let kids = state.workspace.borrow().children_of(None);
    let keys_in_vec: Vec<quire::core::OrderKey> =
        kids.iter().map(|k| state.page_order_of(*k)).collect();
    let mut sorted = keys_in_vec.clone();
    sorted.sort();
    assert_eq!(keys_in_vec, sorted, "session order == restart order");
    assert_eq!(*kids.last().unwrap(), copy, "the copy is the run's last");
}

/// SPEC §三十七: a folded subtree costs real rows, so an editor row index
/// stops being a model index. The drag landing is the one consumer that
/// counts model positions, and a forgotten translation is a rejected drop,
/// not a subtle corruption — which is why both halves are asserted here.
#[test]
fn a_folded_section_shifts_drag_landings_but_not_row_numbers() {
    use quire::app::state::{project_blocks, MentionTitles};
    use quire::core::{Block, BlockId, BlockKind, ColorKind, OrderKey};

    let args = HandleArgs { blocks: 0, auto_exit_secs: 0.0, bench_pages: 0, pictures: 0, marks: 0, code: 0 };
    let state = AppState::new(&args, None);
    let page = state.create_page(None);
    let pid = core_page_id(page);
    let mk = |id: u64, parent: Option<u64>, kind: BlockKind, folded: bool| Block {
        id: BlockId(id),
        page: pid,
        parent: parent.map(BlockId),
        order: OrderKey(id * 10),
        kind,
        text: format!("b{id}"),
        checked: false,
        marks: Vec::new(),
        color: ColorKind::Default,
        background: ColorKind::Default,
        page_ref: None,
        folded,
        attachment: None,
        img_percent: 100,
        columns: 0,
        lang: quire::core::Lang::Plain,
        db_ref: None,
        sync_ref: None,    };
    state.doc.borrow_mut().set_page_blocks(
        pid,
        vec![
            mk(1, None, BlockKind::Toggle, true),
            mk(2, Some(1), BlockKind::Paragraph, false),
            mk(3, None, BlockKind::Paragraph, false),
            mk(4, None, BlockKind::Paragraph, false),
        ],
    );

    let rows = project_blocks(
        state.doc.borrow().page_blocks(pid),
        &Default::default(),
        &MentionTitles::empty(),
    );
    assert_eq!(
        rows.iter().map(|r| r.id).collect::<Vec<i32>>(),
        vec![1, 3, 4],
        "block 2 sits inside the fold and has no row"
    );

    assert_eq!(state.drop_index_for_row(0, false), Some(0));
    assert_eq!(state.drop_index_for_row(1, false), Some(2));
    assert_eq!(state.drop_index_for_row(2, true), Some(4));
    assert_eq!(state.drop_index_for_row(3, false), None, "there is no 4th row");
    // dropping right below a folded parent is its first child's slot, which
    // the subtree rule already refused when the section was open
    assert_eq!(state.drop_index_for_row(0, true), Some(1));
    assert!(!state.can_move_block_to(4, 1));

    // dragging b4 above the row that shows b3: row 1, model 2
    assert!(state.can_move_block_to(4, state.drop_index_for_row(1, false).unwrap()));
}
