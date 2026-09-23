// Pure page-tree workspace model behind the M2 shell navigation. No Slint
// types here on purpose: this module is the seed of `core/`'s real model
// (M3 swaps the mock content for SQLite rows behind the same operations),
// so it must stay compilable and testable without a window.

use std::collections::HashMap;

use crate::services::search_service::snap_to_words;

/// How many pages the Recent section keeps.
pub const MAX_RECENTS: usize = 6;

/// Pages with an id >= this are benchmark fixtures, never listed in the
/// command palette.
pub const BENCH_ID_BASE: i32 = 1000;

#[derive(Clone)]
pub struct Page {
    pub id: i32,
    pub title: String,
    pub parent: Option<i32>,
    pub children: Vec<i32>,
    pub favorite: bool,
    pub expanded: bool,
    /// Page appearance (SPEC §三十八), mirrored from the core page so the
    /// style menu can show what is on and the editor can be told what to
    /// draw without a second lookup.
    pub font: crate::core::PageFont,
    pub full_width: bool,
    pub small_text: bool,
    /// The page's own emoji (SPEC §三十八 "图标与封面"); empty means unset and
    /// the sidebar shows the title's first character.
    pub icon: String,
    /// The page's cover (same section): the attachment it draws behind the
    /// title, mirrored from the core page so the editor can be told what to
    /// draw — and so the reclaim can ask, per page, who still points at a file.
    pub cover: Option<crate::core::AttachmentId>,
    /// The page's read-only switch (SPEC §三十八 "lock"), mirrored from the
    /// core page so the editor can be told not to open an input, and so the
    /// one funnel that refuses commands can ask the tree instead of the
    /// document — the lock is a fact about the page, not about its blocks.
    pub locked: bool,
    /// This page is a template (SPEC §三十八 "模板"), and the flag has one
    /// meaning here: the page is **not attached to the tree**. Everything that
    /// lists pages walks `roots` / `children`, so an unattached page is already
    /// invisible to the sidebar, the blob search, the palette, the slash page
    /// picker and the Move-to walks without a single `if template` in any of
    /// them. It stays in `pages`, because the library has to be able to name it
    /// and its blocks have to be reachable to copy them.
    pub template: bool,
    /// Title + block text blob, filled by the app layer; the search source.
    pub search_text: String,
}

/// One visible row of the workspace tree (expanded nodes only, pre-flattened).
pub struct TreeRow {
    pub id: i32,
    pub label: String,
    pub depth: i32,
    pub expanded: bool,
    pub has_children: bool,
}

pub struct SearchHit {
    pub id: i32,
    pub title: String,
    pub breadcrumb: String,
    /// Text around the first content match; empty for title/recents hits.
    pub snippet: String,
}

pub struct Workspace {
    pages: HashMap<i32, Page>,
    roots: Vec<i32>,
    next_id: i32,
    recents: Vec<i32>, // most recent first
}

impl Workspace {
    /// The sample tree M1's mock sidebar imitated. Ids are stable so tests
    /// and scenes can refer to them.
    pub fn sample() -> Self {
        let mut ws = Workspace {
            pages: HashMap::new(),
            roots: Vec::new(),
            next_id: 100,
            recents: Vec::new(),
        };
        ws.create(None, "Weekly Review"); // 100
        ws.create(None, "Design Ideas"); // 101
        let getting_started = ws.create(None, "Getting Started"); // 102
        ws.create(Some(getting_started), "Keyboard Shortcuts"); // 103
        ws.create(Some(getting_started), "Import from Markdown"); // 104
        let atlas = ws.create(None, "Project Atlas"); // 105
        let research = ws.create(Some(atlas), "Research Notes"); // 106
        ws.create(Some(research), "Sources"); // 107
        ws.create(Some(atlas), "Meeting Notes"); // 108
        ws.create(Some(atlas), "Architecture"); // 109
        let reading = ws.create(None, "Reading List"); // 110
        ws.create(Some(reading), "Papers to Read"); // 111
        ws.create(None, "写作与中文测试"); // 112
        ws.create(None, "Scratchpad"); // 113
        ws.toggle_favorite(100);
        ws.toggle_favorite(101);
        ws.pages.get_mut(&110).unwrap().expanded = false;
        ws.pages.get_mut(&102).unwrap().expanded = true;
        ws.pages.get_mut(&105).unwrap().expanded = true;
        ws.mark_opened(108);
        ws.mark_opened(105);
        ws
    }

    /// Sample tree plus `n` flat benchmark pages (scene G).
    pub fn with_bench_pages(n: usize) -> Self {
        let mut ws = Workspace::sample();
        for i in 0..n {
            let title = format!("Bench {:03}", i);
            let id = ws.create(None, &title);
            let _ = id;
        }
        ws
    }

    fn new_page(&mut self, title: &str) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        self.pages.insert(
            id,
            Page {
                id,
                title: title.to_string(),
                parent: None,
                children: Vec::new(),
                favorite: false,
                expanded: false,
                font: crate::core::PageFont::default(),
                full_width: false,
                small_text: false,
                icon: String::new(),
                cover: None,
                locked: false,
                // A page created through the tree is a page. The template flag
                // is only ever set by `create_template`, which never attaches
                // what it makes -- so no caller can forget this line and end up
                // with a page that is both in the tree and a body to copy from.
                template: false,
                search_text: String::new(),
            },
        );
        id
    }

    fn attach(&mut self, id: i32, parent: Option<i32>, after: Option<i32>) {
        self.pages.get_mut(&id).expect("page exists").parent = parent;
        let list: &mut Vec<i32> = match parent {
            Some(p) => &mut self.pages.get_mut(&p).expect("parent exists").children,
            None => &mut self.roots,
        };
        match after {
            Some(prev) => {
                let pos = list
                    .iter()
                    .position(|&x| x == prev)
                    .map_or(list.len(), |i| i + 1);
                list.insert(pos, id);
            }
            None => list.push(id),
        }
    }

    /// Insert a page row that arrived with its id already decided — the sync
    /// merge renumbers remote rows against this session's watermark, so the
    /// id the tree stores has to be the id the document and the store see.
    /// Appends after the parent's (or the roots') last child; every other
    /// field is the row's own. Returns the id it was given.
    pub fn insert_persisted(&mut self, p: crate::core::Page) -> i32 {
        let id = p.id.0 as i32;
        self.next_id = self.next_id.max(id + 1);
        let node = Page {
            id,
            title: p.title.clone(),
            parent: p.parent.map(|v| v.0 as i32),
            children: Vec::new(),
            favorite: p.favorite,
            expanded: p.expanded,
            font: p.font,
            full_width: p.full_width,
            small_text: p.small_text,
            icon: p.icon.clone(),
            cover: p.cover,
            locked: p.locked,
            template: p.template,
            search_text: String::new(),
        };
        self.pages.insert(id, node);
        if !p.template {
            self.attach(id, p.parent.map(|v| v.0 as i32), None);
        }
        id
    }

    /// Set the favorite star to an exact value (the sync apply knows the
    /// merged fact; `toggle_favorite` only knows the click).
    pub fn set_favorite(&mut self, id: i32, v: bool) {
        if let Some(node) = self.pages.get_mut(&id) {
            node.favorite = v;
        }
    }

    /// Set the tree's expanded flag to an exact value (same reason).
    pub fn set_expanded(&mut self, id: i32, v: bool) {
        if let Some(node) = self.pages.get_mut(&id) {
            node.expanded = v;
        }
    }

    /// Reserve the next page id without creating anything — the sync apply
    /// renumbers an incoming row before the tree sees it.
    pub fn reserve_page_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Every page row, templates included, in no particular order — the sync
    /// export's read of the tree. Order keys live in `app::state`'s own map,
    /// so the caller pairs the rows with that map.
    pub fn page_rows(&self) -> Vec<Page> {
        self.pages.values().cloned().collect()
    }

    /// Create a page under `parent` (root when None) and return its id.
    pub fn create(&mut self, parent: Option<i32>, title: &str) -> i32 {
        let id = self.new_page(title);
        if let Some(p) = parent {
            self.pages.get_mut(&p).expect("parent exists").expanded = true;
        }
        self.attach(id, parent, None);
        id
    }

    /// Create a **template** (SPEC §三十八 "模板"): a page whose blocks are a
    /// body to copy from. The one thing that makes it a template is that it is
    /// never `attach`ed -- no parent, no slot in `roots` -- so every walk that
    /// lists pages skips it for free. It stays in `pages`, because the library
    /// has to be able to name it and its body has to be reachable to copy it.
    /// The caller owns the body: `app::state` fills it with `BlockInserted`s,
    /// exactly the way a page's own blocks are recorded.
    pub fn create_template(&mut self, title: &str) -> i32 {
        let id = self.new_page(title);
        self.pages.get_mut(&id).expect("page exists").template = true;
        id
    }

    /// The template library as the menus see it: `(id, name)`, oldest first.
    /// Id order means the built-ins seeded on a library's first start lead the
    /// list and anything saved later follows it; nothing sorts by title,
    /// because two templates called "Meeting notes" are the user's business
    /// rather than a reason to reshuffle their menu.
    pub fn templates(&self) -> Vec<(i32, String)> {
        let mut out: Vec<(i32, String)> = self
            .pages
            .values()
            .filter(|p| p.template)
            .map(|p| (p.id, p.title.clone()))
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }

    /// Is this id a template? False for a missing page, which is the same
    /// answer as "not a page at all" and is why the delete path can ask it.
    pub fn is_template(&self, id: i32) -> bool {
        self.pages.get(&id).map_or(false, |p| p.template)
    }

    pub fn rename(&mut self, id: i32, title: &str) {
        if let Some(p) = self.pages.get_mut(&id) {
            p.title = title.to_string();
        }
    }

    /// Would moving `id` under `new_parent` (root when `None`) be legal?
    /// False for a missing page, the page itself, or any of its own
    /// descendants (a cycle). Read-only: the drag hover path calls this per
    /// frame of the gesture.
    pub fn can_move_page(&self, id: i32, new_parent: Option<i32>) -> bool {
        if !self.pages.contains_key(&id) || new_parent == Some(id) {
            return false;
        }
        let mut cursor = new_parent;
        while let Some(c) = cursor {
            if c == id {
                return false;
            }
            cursor = self.pages.get(&c).and_then(|p| p.parent);
        }
        true
    }

    /// Move `id` (its whole subtree travels with it) under `new_parent`
    /// (root when `None`), appended after `after` (the end when `None`).
    /// Refused — without touching anything — when the move is illegal (see
    /// `can_move_page`). The target parent is expanded so the moved page is
    /// visible on arrival.
    pub fn move_page(&mut self, id: i32, new_parent: Option<i32>, after: Option<i32>) -> bool {
        if !self.can_move_page(id, new_parent) {
            return false;
        }
        let old_parent = self.pages.get(&id).expect("page exists").parent;
        match old_parent {
            Some(p) => self
                .pages
                .get_mut(&p)
                .expect("parent exists")
                .children
                .retain(|&c| c != id),
            None => self.roots.retain(|&c| c != id),
        }
        self.attach(id, new_parent, after);
        if let Some(p) = new_parent {
            self.pages.get_mut(&p).expect("parent exists").expanded = true;
        }
        true
    }

    /// Swap `id` with the sibling `delta` slots away (±1). Refused at the
    /// run's edges or when the page is missing.
    pub fn swap_with_neighbor(&mut self, id: i32, delta: i32) -> bool {
        let parent = match self.pages.get(&id) {
            Some(p) => p.parent,
            None => return false,
        };
        let list: &mut Vec<i32> = match parent {
            Some(p) => &mut self.pages.get_mut(&p).expect("parent exists").children,
            None => &mut self.roots,
        };
        let Some(idx) = list.iter().position(|&c| c == id) else {
            return false;
        };
        let target = idx as isize + delta as isize;
        if target < 0 || target as usize >= list.len() {
            return false;
        }
        list.swap(idx, target as usize);
        true
    }

    /// Deep-copy the subtree; the copy lands directly after the original and
    /// is titled "Copy of …". Returns the new root id.
    pub fn duplicate(&mut self, id: i32) -> Option<i32> {
        let (parent, title, style) = {
            let src = self.pages.get(&id)?;
            (
                src.parent,
                format!("Copy of {}", src.title),
                (
                    src.font,
                    src.full_width,
                    src.small_text,
                    src.icon.clone(),
                    src.cover,
                ),
            )
        };
        let new_root = self.new_page(&title);
        // The copy is a copy of the page, and its look is part of the page
        // (SPEC §三十八) — the persisted side records the same, and a session
        // that disagreed with what a restart would show is the defect this
        // project has been bitten by before.
        {
            let dst = self.pages.get_mut(&new_root).expect("just created");
            dst.font = style.0;
            dst.full_width = style.1;
            dst.small_text = style.2;
            dst.icon = style.3;
            // The cover travels with it: a copy that pointed at the same
            // attachment as its source is two referencers, not one, and the
            // reclaim has to be told that before it sweeps the bytes.
            dst.cover = style.4;
        }
        // The lock deliberately does *not* travel. Font, icon and cover are
        // parts of what the page looks like, so a copy that dropped them would
        // open differently from how it looked; the lock is a gate on editing,
        // and duplicating a locked page is what a user does to edit it without
        // touching the original. `new_page` hands the copy an unlocked row.
        self.attach(new_root, parent, Some(id));
        self.copy_children(id, new_root);
        Some(new_root)
    }

    fn copy_children(&mut self, src: i32, dst: i32) {
        let kids = self.pages.get(&src).expect("src exists").children.clone();
        for k in kids {
            let (title, blob) = {
                let p = self.pages.get(&k).expect("child exists");
                (p.title.clone(), p.search_text.clone())
            };
            let new_id = self.new_page(&title);
            self.pages.get_mut(&new_id).unwrap().search_text = blob;
            self.attach(new_id, Some(dst), None);
            self.copy_children(k, new_id);
        }
    }

    /// Remove the subtree; returns the removed ids (root first).
    pub fn delete(&mut self, id: i32) -> Vec<i32> {
        if !self.pages.contains_key(&id) {
            return Vec::new();
        }
        let parent = self.pages.get(&id).unwrap().parent;
        match parent {
            Some(p) => self.pages.get_mut(&p).unwrap().children.retain(|&c| c != id),
            None => self.roots.retain(|&r| r != id),
        }
        let mut removed = Vec::new();
        self.remove_recursive(id, &mut removed);
        self.recents.retain(|r| !removed.contains(r));
        removed
    }

    fn remove_recursive(&mut self, id: i32, out: &mut Vec<i32>) {
        let kids = self.pages.get(&id).expect("page exists").children.clone();
        for k in kids {
            self.remove_recursive(k, out);
        }
        self.pages.remove(&id);
        out.push(id);
    }

    /// Number of pages in the subtree rooted at `id` (inclusive).
    pub fn subtree_size(&self, id: i32) -> usize {
        let mut n = 0;
        self.count_recursive(id, &mut n);
        n
    }

    fn count_recursive(&self, id: i32, n: &mut usize) {
        if let Some(p) = self.pages.get(&id) {
            *n += 1;
            for &c in &p.children {
                self.count_recursive(c, n);
            }
        }
    }

    pub fn toggle_expanded(&mut self, id: i32) {
        if let Some(p) = self.pages.get_mut(&id) {
            p.expanded = !p.expanded;
        }
    }

    pub fn toggle_favorite(&mut self, id: i32) {
        if let Some(p) = self.pages.get_mut(&id) {
            p.favorite = !p.favorite;
        }
    }

    /// The page's typeface. Returns what it is now, which is what the caller
    /// persists and then pushes to the editor.
    pub fn set_font(&mut self, id: i32, font: crate::core::PageFont) -> crate::core::PageFont {
        match self.pages.get_mut(&id) {
            Some(p) => {
                p.font = font;
                p.font
            }
            None => crate::core::PageFont::default(),
        }
    }

    /// The two layout switches, written as the pair the one column holds and
    /// read back so the caller persists exactly what changed.
    pub fn set_layout(&mut self, id: i32, full_width: bool, small_text: bool) -> (bool, bool) {
        match self.pages.get_mut(&id) {
            Some(p) => {
                p.full_width = full_width;
                p.small_text = small_text;
                (p.full_width, p.small_text)
            }
            None => (false, false),
        }
    }

    /// What the editor's three page-type inputs need, in one lookup.
    pub fn page_style(&self, id: i32) -> Option<(crate::core::PageFont, bool, bool)> {
        self.pages
            .get(&id)
            .map(|p| (p.font, p.full_width, p.small_text))
    }

    /// The page's own emoji, written and read back so the caller persists what
    /// actually changed. Empty clears it, which is what "no icon" means here —
    /// the sidebar then shows the title's first character.
    pub fn set_icon(&mut self, id: i32, icon: &str) -> String {
        match self.pages.get_mut(&id) {
            Some(p) => {
                p.icon = icon.to_string();
                p.icon.clone()
            }
            None => String::new(),
        }
    }

    /// What the page stores, `""` for none. The caller decides what an empty
    /// slot shows: the sidebar substitutes the title's first character, the
    /// editor shows nothing above a title that has no icon — repeating the
    /// title's own first character at 60px is an echo, not a placeholder.
    pub fn icon_of(&self, id: i32) -> String {
        self.pages.get(&id).map(|p| p.icon.clone()).unwrap_or_default()
    }

    /// The page's cover, as the attachment id it points at (`None` = no band).
    /// Set and read back like the icon, so the caller persists what moved;
    /// `None` clears. The workspace hands out an id, never a path — decoding is
    /// the attachment store's job and so is deciding whether the bytes live on.
    pub fn set_cover(
        &mut self,
        id: i32,
        cover: Option<crate::core::AttachmentId>,
    ) -> Option<crate::core::AttachmentId> {
        match self.pages.get_mut(&id) {
            Some(p) => {
                p.cover = cover;
                p.cover
            }
            None => None,
        }
    }

    pub fn cover_of(&self, id: i32) -> Option<crate::core::AttachmentId> {
        self.pages.get(&id).and_then(|p| p.cover)
    }

    /// Every attachment some page in this tree uses as its cover. §三十七's
    /// reclaim asked only the blocks who pointed at a file, and a page could
    /// legitimately answer "nobody" while drawing that file behind its title.
    pub fn cover_ids(&self) -> Vec<i64> {
        self.pages
            .values()
            .filter_map(|p| p.cover)
            .map(|a| a.as_u64() as i64)
            .collect()
    }

    /// Set the page's read-only switch and answer with what the page now says
    /// (`None` for an unknown page, like every other setter here).
    pub fn set_locked(&mut self, id: i32, locked: bool) -> Option<bool> {
        self.pages.get_mut(&id).map(|p| {
            p.locked = locked;
            p.locked
        })
    }

    pub fn locked_of(&self, id: i32) -> bool {
        self.pages.get(&id).map_or(false, |p| p.locked)
    }

    pub fn mark_opened(&mut self, id: i32) {
        if !self.pages.contains_key(&id) {
            return;
        }
        self.recents.retain(|&r| r != id);
        self.recents.insert(0, id);
        self.recents.truncate(MAX_RECENTS);
    }

    /// Expand every ancestor of `id` so it is visible in the tree.
    pub fn expand_ancestors(&mut self, id: i32) {
        let mut cur = self.pages.get(&id).and_then(|p| p.parent);
        while let Some(pid) = cur {
            if let Some(p) = self.pages.get_mut(&pid) {
                p.expanded = true;
                cur = p.parent;
            } else {
                break;
            }
        }
    }

    /// Restore persisted recents (M3 integration); unknown ids are dropped
    /// lazily by the accessor.
    pub fn set_recents(&mut self, ids: Vec<i32>) {
        self.recents = ids;
    }

    pub fn recents_ids(&self) -> Vec<i32> {
        self.recents.clone()
    }

    pub fn set_search_text(&mut self, id: i32, blob: String) {
        if let Some(p) = self.pages.get_mut(&id) {
            p.search_text = blob;
        }
    }

    pub fn get(&self, id: i32) -> Option<&Page> {
        self.pages.get(&id)
    }

    pub fn contains(&self, id: i32) -> bool {
        self.pages.contains_key(&id)
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Pages a person can see, which is `page_count` minus the library (SPEC
    /// §三十八). The one caller that matters is the sidebar's "is this workspace
    /// empty" flag: five built-in templates arrive on a library's first start,
    /// and an empty-looking workspace that says it is not empty is a hint that
    /// never comes back. `page_count` stays the raw count for the benches, which
    /// report what the file holds.
    pub fn visible_page_count(&self) -> usize {
        self.pages.values().filter(|p| !p.template).count()
    }

    pub fn title_of(&self, id: i32) -> Option<&str> {
        self.pages.get(&id).map(|p| p.title.as_str())
    }

    /// Ancestors of `id`, root first, inclusive of `id` itself.
    pub fn ancestors(&self, id: i32) -> Vec<(i32, String)> {
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(cid) = cur {
            match self.pages.get(&cid) {
                Some(p) => {
                    chain.push((p.id, p.title.clone()));
                    cur = p.parent;
                }
                None => break,
            }
        }
        chain.reverse();
        chain
    }

    /// Breadcrumb of ancestors above `id` (empty for roots), "A / B" style.
    pub fn breadcrumb(&self, id: i32) -> String {
        let chain = self.ancestors(id);
        chain
            .iter()
            .take(chain.len().saturating_sub(1))
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// First root page, used as the fallback landing page.
    pub fn first_root(&self) -> Option<i32> {
        self.roots.first().copied()
    }

    /// Favorites in tree order.
    pub fn favorites(&self) -> Vec<(i32, String)> {
        let mut out = Vec::new();
        self.collect_if(self.roots.clone(), &mut |p| {
            p.favorite.then(|| (p.id, p.title.clone()))
        }, &mut out);
        out
    }

    pub fn recents(&self) -> Vec<(i32, String)> {
        self.recents
            .iter()
            .filter_map(|id| self.pages.get(id).map(|p| (p.id, p.title.clone())))
            .collect()
    }

    fn collect_if(
        &self,
        ids: Vec<i32>,
        f: &mut dyn FnMut(&Page) -> Option<(i32, String)>,
        out: &mut Vec<(i32, String)>,
    ) {
        for id in ids {
            if let Some(p) = self.pages.get(&id) {
                if let Some(hit) = f(p) {
                    out.push(hit);
                }
                let kids = p.children.clone();
                self.collect_if(kids, f, out);
            }
        }
    }

    /// Visible tree rows, depth-first, honoring `expanded` flags.
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        self.tree_recursive(self.roots.clone(), 0, &mut rows);
        rows
    }

    fn tree_recursive(&self, ids: Vec<i32>, depth: i32, rows: &mut Vec<TreeRow>) {
        for id in ids {
            if let Some(p) = self.pages.get(&id) {
                let has_children = !p.children.is_empty();
                rows.push(TreeRow {
                    id: p.id,
                    label: p.title.clone(),
                    depth,
                    expanded: p.expanded,
                    has_children,
                });
                if p.expanded {
                    self.tree_recursive(p.children.clone(), depth + 1, rows);
                }
            }
        }
    }

    /// All page ids in depth-first display order.
    pub fn dfs_order(&self) -> Vec<i32> {
        let mut ids = Vec::new();
        self.dfs_recursive(self.roots.clone(), &mut ids);
        ids
    }

    /// Ordered children of `parent` (roots when `None`).
    pub fn children_of(&self, parent: Option<i32>) -> Vec<i32> {
        match parent {
            Some(p) => self.pages.get(&p).map(|n| n.children.clone()).unwrap_or_default(),
            None => self.roots.clone(),
        }
    }

    /// Rebuild the workspace from persisted page rows (M3 integration).
    /// Sibling order comes from the persisted order keys; ids are the
    /// persisted u64s truncated to the workspace's i32 space (single-app
    /// scope: ids stay well below 2^31). Recents restart empty.
    pub fn from_persisted(pages: &[crate::core::Page]) -> Self {
        let mut ws = Workspace {
            pages: HashMap::new(),
            roots: Vec::new(),
            next_id: 1,
            recents: Vec::new(),
        };
        let mut max_id = 0i32;
        for p in pages {
            let id = p.id.0 as i32;
            max_id = max_id.max(id);
            ws.pages.insert(
                id,
                Page {
                    id,
                    title: p.title.clone(),
                    parent: p.parent.map(|v| v.0 as i32),
                    children: Vec::new(),
                    favorite: p.favorite,
                    expanded: p.expanded,
                    font: p.font,
                    full_width: p.full_width,
                    small_text: p.small_text,
                    icon: p.icon.clone(),
                    cover: p.cover,
                    locked: p.locked,
                    template: p.template,
                    search_text: String::new(),
                },
            );
        }
        // assemble roots/children ordered by key. A template is skipped here and
        // nowhere else: `roots` is the only door every page listing walks, so
        // leaving a template out of it is what keeps a restart from putting the
        // library back into the sidebar. (Its `parent` is NULL for the same
        // reason -- see `create_template` -- so the only way it could reattach
        // is by being a root.)
        let mut by_parent: std::collections::BTreeMap<Option<i32>, Vec<(crate::core::OrderKey, i32)>> =
            std::collections::BTreeMap::new();
        for p in pages {
            if p.template {
                continue;
            }
            let key = (p.parent.map(|v| v.0 as i32), p.order);
            by_parent
                .entry(key.0)
                .or_default()
                .push((p.order, p.id.0 as i32));
        }
        for (parent, mut kids) in by_parent {
            kids.sort();
            let ids: Vec<i32> = kids.into_iter().map(|(_, id)| id).collect();
            match parent {
                Some(p) => {
                    if let Some(node) = ws.pages.get_mut(&p) {
                        node.children = ids;
                    }
                }
                None => ws.roots = ids,
            }
        }
        ws.next_id = max_id + 1;
        ws
    }

    /// Expose the persisted-tree shape for seeding (id, title, parent,
    /// favorite, expanded, font, full width, small text, icon) — used by the
    /// app layer when recording the initial workspace into storage. Every
    /// appearance field is here even though nothing sets one before seeding:
    /// a row this list drops is a page property a restart would lose.
    pub fn page_seed_rows(
        &self,
    ) -> Vec<(i32, String, Option<i32>, bool, bool, crate::core::PageFont, bool, bool, String)>
    {
        self.dfs_order()
            .iter()
            .filter_map(|id| {
                self.pages.get(id).map(|p| {
                    (
                        p.id,
                        p.title.clone(),
                        p.parent,
                        p.favorite,
                        p.expanded,
                        p.font,
                        p.full_width,
                        p.small_text,
                        p.icon.clone(),
                    )
                })
            })
            .collect()
    }

    fn dfs_recursive(&self, list: Vec<i32>, out: &mut Vec<i32>) {
        for id in list {
            if let Some(p) = self.pages.get(&id) {
                out.push(id);
                let kids = p.children.clone();
                self.dfs_recursive(kids, out);
            }
        }
    }

    /// Search pages by title substring first, then by content substring.
    /// An empty query returns the recent pages ("jump back in" list).
    /// Result count is capped at 20.
    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        let q = query.trim();
        if q.is_empty() {
            return self
                .recents()
                .into_iter()
                .take(20)
                .map(|(id, title)| SearchHit {
                    id,
                    title,
                    breadcrumb: self.breadcrumb(id),
                    snippet: String::new(),
                })
                .collect();
        }
        let needle: Vec<char> = q.chars().collect();
        let mut title_hits = Vec::new();
        let mut content_hits = Vec::new();
        for id in self.dfs_order() {
            let p = &self.pages[&id];
            if find_ci(&p.title.chars().collect::<Vec<_>>(), &needle).is_some() {
                title_hits.push(SearchHit {
                    id,
                    title: p.title.clone(),
                    breadcrumb: self.breadcrumb(id),
                    snippet: String::new(),
                });
            } else if let Some(pos) = find_ci(&p.search_text.chars().collect::<Vec<_>>(), &needle) {
                content_hits.push(SearchHit {
                    id,
                    title: p.title.clone(),
                    breadcrumb: self.breadcrumb(id),
                    snippet: extract_snippet(&p.search_text, pos, needle.len(), 48),
                });
            }
            if title_hits.len() + content_hits.len() >= 20 {
                break;
            }
        }
        title_hits.extend(content_hits);
        title_hits
    }
}

fn lower_ascii(c: char) -> char {
    c.to_ascii_lowercase()
}

/// Case-insensitive substring search over char slices (ASCII folding only;
/// CJK has no case). Returns the char position of the first match.
fn find_ci(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    if needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| {
        w.iter()
            .zip(needle)
            .all(|(a, b)| lower_ascii(*a) == lower_ascii(*b))
    })
}

/// ±`pad` chars around a match, on char boundaries, with ellipses. The edges
/// are snapped to whole words by the same rule the FTS snippet uses.
fn extract_snippet(text: &str, pos: usize, needle_len: usize, pad: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = pos.saturating_sub(pad);
    let end = (pos + needle_len + pad).min(chars.len());
    let (s, e) = snap_to_words(&chars, start, end, pos, pos + needle_len);
    let window = chars[s..e].iter().collect::<String>();
    let window = window.trim();
    let mut s = String::new();
    if start > 0 {
        s.push('…');
    }
    s.push_str(window);
    if end < chars.len() {
        s.push('…');
    }
    s.replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::sample()
    }

    #[test]
    fn sample_tree_shape() {
        let w = ws();
        assert_eq!(w.page_count(), 14);
        assert_eq!(w.title_of(102), Some("Getting Started"));
        let rows = w.tree_rows();
        // expanded roots and their children are visible
        assert!(rows.iter().any(|r| r.id == 102 && r.depth == 0 && r.expanded));
        assert!(rows.iter().any(|r| r.id == 103 && r.depth == 1));
        // Reading List starts collapsed
        let rl = rows.iter().find(|r| r.id == 110).unwrap();
        assert!(!rl.expanded && rl.has_children);
        // collapsed child not present
        assert!(!rows.iter().any(|r| r.id == 111));
    }

    #[test]
    fn create_rename_delete_roundtrip() {
        let mut w = ws();
        let id = w.create(Some(105), "New Kid");
        assert_eq!(w.title_of(id), Some("New Kid"));
        // parent was auto-expanded, child visible
        assert!(w.tree_rows().iter().any(|r| r.id == id && r.depth == 1));
        w.rename(id, "Renamed");
        assert_eq!(w.title_of(id), Some("Renamed"));
        let removed = w.delete(105); // Atlas + 4 descendants + the new kid
        assert_eq!(removed.len(), 6);
        assert!(removed.contains(&id));
        assert!(!w.contains(105));
        assert!(w.tree_rows().iter().all(|r| r.id != id));
        // delete also cleans recents
        assert!(w.recents().iter().all(|(id, _)| *id != 105));
    }

    #[test]
    fn duplicate_copies_subtree_with_fresh_ids() {
        let mut w = ws();
        let n_before = w.page_count();
        let copy = w.duplicate(105).unwrap();
        assert_eq!(w.title_of(copy), Some("Copy of Project Atlas"));
        assert_eq!(w.page_count(), n_before + 5);
        assert_eq!(w.subtree_size(copy), 5);
        // original untouched, copy lands after the whole original subtree
        // (both are expanded, so the DFS list interleaves children)
        let roots = w.tree_rows();
        let pos_orig = roots.iter().position(|r| r.id == 105).unwrap();
        assert_eq!(roots[pos_orig + w.subtree_size(105)].id, copy);
    }

    #[test]
    fn page_style_is_three_switches_and_nothing_else() {
        use crate::core::PageFont;
        let mut w = ws();
        // a page nobody touched
        assert_eq!(
            w.page_style(105),
            Some((PageFont::Default, false, false)),
            "the sample tree stores no look"
        );
        assert_eq!(w.set_font(105, PageFont::Serif), PageFont::Serif);
        assert_eq!(w.set_layout(105, true, false), (true, false));
        assert_eq!(
            w.page_style(105),
            Some((PageFont::Serif, true, false)),
            "the two switches and the font are three separate facts"
        );
        // an id that is not a page answers with the defaults and changes nothing
        assert_eq!(w.set_font(9999, PageFont::Mono), PageFont::Default);
        assert_eq!(w.set_layout(9999, true, true), (false, false));
        assert_eq!(w.page_style(9999), None);
    }

    #[test]
    fn a_page_icon_is_set_cleared_and_carried_by_a_copy() {
        let mut w = ws();
        assert_eq!(w.icon_of(105), "", "the sample tree has no icons");
        assert_eq!(w.set_icon(105, "\u{1F680}"), "\u{1F680}");
        assert_eq!(w.icon_of(105), "\u{1F680}");
        // the look and the icon are separate facts: one does not imply another
        assert_eq!(
            w.page_style(105),
            Some((crate::core::PageFont::Default, false, false))
        );
        let copy = w.duplicate(105).expect("duplicate");
        assert_eq!(
            w.icon_of(copy),
            "\u{1F680}",
            "a copy that lost its icon would disagree with the PageCreated written for it"
        );
        // clearing is a write, not a delete
        assert_eq!(w.set_icon(105, ""), "");
        assert_eq!(w.icon_of(105), "");
        assert_eq!(w.icon_of(copy), "\u{1F680}", "and the copy keeps its own");
        assert_eq!(w.set_icon(9999, "?"), "", "an unknown page stores nothing");
        assert_eq!(w.icon_of(9999), "");
    }

    #[test]
    fn a_page_cover_points_at_an_attachment_and_survives_a_copy() {
        let mut w = ws();
        assert_eq!(w.cover_of(105), None, "the sample tree has no covers");
        assert_eq!(
            w.cover_ids(),
            // serde_json's PartialEq<i64> for Value makes the bare `Vec::new()`
            // ambiguous from the sync module's dependency edge
            Vec::<i64>::new(),
            "so the reclaim has nothing to hear from the tree yet"
        );
        let seven = Some(crate::core::AttachmentId(7));
        assert_eq!(w.set_cover(105, seven), seven);
        assert_eq!(w.cover_of(105), seven);
        assert_eq!(
            w.cover_ids(),
            vec![7],
            "the sweep asks the whole tree, not the open page"
        );
        let copy = w.duplicate(105).expect("duplicate");
        assert_eq!(
            w.cover_of(copy),
            seven,
            "a copy is a second referencer, pointing at the same bytes"
        );
        assert_eq!(w.cover_ids(), vec![7, 7], "and the sweep hears it twice");
        // clearing one page must not free what the other still draws
        assert_eq!(w.set_cover(105, None), None);
        assert_eq!(w.cover_ids(), vec![7]);
        assert_eq!(w.set_cover(9999, None), None, "an unknown page stores nothing");
    }

    #[test]
    fn a_locked_page_stays_a_page_and_a_copy_of_it_is_open() {
        let mut w = ws();
        assert!(!w.locked_of(105), "the sample tree locks nothing");
        assert_eq!(w.set_locked(105, true), Some(true));
        assert!(w.locked_of(105));
        assert_eq!(
            w.title_of(105),
            Some("Project Atlas"),
            "locking is one flag, not a wall around the rest of the row"
        );
        assert_eq!(w.set_locked(9999, true), None, "an unknown page stores nothing");
        // The copy is where the user goes to edit: a look travels with a
        // duplicate, a gate does not.
        let copy = w.duplicate(105).expect("duplicate");
        assert!(!w.locked_of(copy), "and the duplicate is unlocked");
        assert!(w.locked_of(105), "the source keeps its own state");
        assert_eq!(w.set_locked(105, false), Some(false), "and unlocks");
    }

    #[test]
    fn favorites_and_recents() {
        let mut w = ws();
        let favs = w.favorites();
        assert_eq!(favs.len(), 2);
        assert!(favs.iter().all(|(id, _)| *id == 100 || *id == 101));
        w.toggle_favorite(112);
        assert_eq!(w.favorites().len(), 3);
        // recents: most recent first, capped
        w.mark_opened(113);
        assert_eq!(w.recents()[0].0, 113);
        for i in 200..200 + MAX_RECENTS as i32 + 3 {
            w.mark_opened(i); // unknown ids ignored
        }
        w.mark_opened(103);
        w.mark_opened(104);
        w.mark_opened(106);
        w.mark_opened(107);
        w.mark_opened(108);
        w.mark_opened(109);
        assert_eq!(w.recents().len(), MAX_RECENTS);
        assert_eq!(w.recents()[0].0, 109);
        // 113 was pushed out by the cap
        assert!(w.recents()[1..].iter().all(|(id, _)| *id != 113));
    }

    #[test]
    fn expand_ancestors_makes_leaf_visible() {
        let mut w = ws();
        w.expand_ancestors(107); // Sources
        let rows = w.tree_rows();
        assert!(rows.iter().any(|r| r.id == 107 && r.depth == 2));
    }

    #[test]
    fn search_title_then_content() {
        let mut w = ws();
        w.set_search_text(112, "写作与中文测试\n中文段落用于验证字体回退与行高".into());
        w.set_search_text(113, "Scratchpad\nnothing useful here".into());
        // title hit
        let hits = w.search("atlas");
        assert!(hits.iter().any(|h| h.id == 105 && h.snippet.is_empty()));
        // content hit produces a snippet
        let hits = w.search("字体回退");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 112);
        assert!(hits[0].snippet.contains("字体回退"));
        // empty query = recents
        assert!(!w.search("").is_empty());
        // no hits
        assert!(w.search("zzzz-not-there").is_empty());
    }

    #[test]
    fn local_search_snippet_breaks_on_words() {
        let mut w = ws();
        // The repo-less path is what the A4 `search-notes` shot renders, and
        // there the ±48-char window opened inside "quiet": the row read
        // "…uiet home for thinking". Varying token lengths make some of these
        // windows cut mid-token and some not; all must come back whole.
        let text: String = (0..120).map(|i| format!("w{i} ")).collect();
        w.set_search_text(113, text.clone());
        for term in 20..40 {
            let needle = format!("w{term}");
            let hits = w.search(&needle);
            assert_eq!(hits.len(), 1, "term {needle} unmatched");
            let s = &hits[0].snippet;
            assert!(s.contains(&needle), "term lost: {s}");
            let body = s.trim_matches('…');
            let at = text
                .find(body)
                .unwrap_or_else(|| panic!("not a source window: {s}"));
            let head_ok = at == 0 || text.as_bytes()[at - 1] == b' ';
            let tail = at + body.len();
            let tail_ok = tail == text.len() || text.as_bytes()[tail] == b' ';
            assert!(head_ok && tail_ok, "split word in {s:?} (term {needle})");
        }
    }

    #[test]
    fn breadcrumb_walks_parents() {
        let w = ws();
        assert_eq!(w.breadcrumb(107), "Project Atlas / Research Notes");
        assert_eq!(w.breadcrumb(105), "");
    }
}
