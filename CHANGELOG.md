# Changelog

## 0.1.0 — Windows RC (in progress)

First functional release: a local, single-file-database notes workspace.

### Editor
- Block editor: paragraphs, headings 1–3, bullet / numbered / to-do lists,
  quotes, code blocks, dividers
- One focused block owns the live text input; everything else renders
  lightweight (10 000-block pages stay flat in memory and rendering)
- Enter / Backspace merge & split semantics, empty list items leave the
  list on Enter or Backspace
- List nesting: Tab / Shift+Tab (depth 1), drag-free reordering via the
  block menu or Ctrl+Shift+↑/↓, Ctrl+D duplicates a block
- Drag the block handle (⋮⋮) to reorder: an accent line marks the landing
  spot and the drop commits one undo-able move; landings that would split a
  nested list away from its parent are rejected
- Markdown line-shortcuts: type "# ", "## ", "### ", "- ", "* ", "1. ",
  "[] ", "[x] ", "> ", "---", "```" or "$$ " at a block start to convert it as
  you type (one undo step)
- Callout block: a tinted rounded box with an emoji and text (slash menu
  "Callout"; turns into Text/Code/Divider like the other kinds)
- Block menu (⋮⋮) — Notion's set, minus the collab/AI items that stay out
  of v1: Turn into (Text / Callout / Code / Divider), Duplicate, Copy link
  to block (puts a quire://block anchor on the system clipboard), Move to
  (every other page; the whole subtree crosses in one undo step), Text
  color and Background color (live-picking palette with swatches), Move
  up/down, Copy block, Paste below, Delete. Clicking a quire://block or
  quire://page link jumps inside the app. The slash menu and Turn-into
  list only carry kinds without a symbol shortcut (ADR-0022)
- "+" handle opens Notion's insert menu: it creates the empty line below
  and shows the full block list (Text, Page, Link to page, To-do, Headings,
  Bulleted / Numbered, Quote, Divider, Callout, Code, Toggle list, Image, File,
  Table, Columns, Math, Table of contents, Embed, Table view, Synced block,
  Linked view) — picking a
  row converts the new line, clicking away or Escape keeps the empty line,
  typing filters the menu. The five remaining database rows (Board, Gallery,
  List, Calendar, Timeline) are still muted "later" placeholders: they name
  layouts, and a layout is a view of a database you already have — see
  Databases below
- Toggle list: a collapsible section. The chevron folds its whole subtree
  out of existence — the hidden blocks get no rows at all, so they cannot
  be tabbed into, dragged or renumbered — and the fold is a view setting,
  not content, so it survives restarts and never bumps the document.
  Turning a Toggle into another kind re-opens it rather than stranding its
  children; Markdown export degrades a toggle to a quote line
- Image block: pick a file from the insert menu, the slash menu or Turn
  into — or press Ctrl+V with a screenshot on the clipboard — and the
  picture lands in the page. The bytes go to an
  `attachments` folder beside the database (the app keeps only a
  reference), oversized pictures get a downscaled display copy while the
  original stays untouched, and the ⋮⋮ menu's "Image width" sets the row
  to 25 / 50 / 100 % of the column. Click a picture to view it full-width
  behind a scrim; clicking anywhere closes it. Undo removes the reference,
  never the file. The picker offers png, jpg, bmp and gif — the four
  formats the decoder is asked for; a gif becomes a still, its first frame
- File block: any file at all, from the same three doors as a picture. The
  row shows the name with its extension, the size, and two buttons — Open
  hands the bytes to whatever the system has registered for that type,
  Save-as copies them out under their original name. The file is streamed
  straight into the `attachments` folder and never read into the app, so a
  2 GB attachment costs the same as a 2 KB one until you press one of those
  buttons. Undo removes the reference, never the file
- Table block: a simple N×M grid, from the insert menu, the slash menu or
  Turn into. Tab moves through the cells and Shift+Tab back; tabbing past
  the last cell adds a row, so the table grows as far as the typing goes.
  Hovering the grid reveals its toolbar — Add row, Add column, Delete row,
  Delete column — where adding goes after the row or column the caret sits
  in and deleting takes it, and a table never shrinks below 1×1. Cells hold
  text with the same inline marks as anywhere else (Ctrl+B / Ctrl+I /
  Ctrl+E / Ctrl+Shift+X / Ctrl+M work inside a cell). Turning a line into a table
  keeps its words in the top-left cell, and turning a table back into text
  gives every cell back as its own paragraph; Markdown export writes a
  GitHub-flavoured table (import still reads those lines as text). This is
  a grid, not a database — schema, filters, sorts and views are a later
  milestone
- Columns block: a line becomes a side-by-side layout, from the insert menu, the
  slash menu or Turn into. Two boxes to start, three at most, and hovering the
  layout reveals its strip — Add column, Delete column — where a box is added at
  the right end with a line inside it and a deleted box's words move into the
  box before. The line's own words open the first box, so nothing is lost by
  converting a line; the layout itself holds no text of its own. Tab walks the
  lines inside the layout and stops at either end (you leave a box by clicking
  out), and an empty box says "Empty column" until you click it, which gives it
  its first line. Layouts are ordinary blocks all the way down — a box is a
  child block and so is every line in it — so undo covers every layout edit and
  no new storage is needed. Markdown export flattens a layout into page-level
  paragraphs, keeping the words and the reading order
- Math block: a formula on its own line, from the insert menu, the slash menu,
  Turn into, or by typing "$$ ". It stores what you type — a LaTeX subset as
  source — and paints the nearest Unicode reading of it: `\frac{a+b}{2}` becomes
  one-line `(a+b)/2`, `\sqrt{ab}` becomes `√(ab)`, `x^2_i` becomes `x²ᵢ` wherever
  the alphabet has the glyph. Nothing is thrown away: a command it does not know
  comes back as its own source, so the worst case reads "that did not render"
  rather than "that vanished". Click the block and the source is what you edit.
  Markdown writes and reads a `$$ … $$` fence, verbatim inside like a code fence
- Contents block: a page's own table of contents, from the insert menu, the slash
  menu or Turn into. The list is not stored — every line is read off the page each
  time the page is projected — so renaming a heading renames its line, deleting one
  removes it, and a heading a folded toggle hides leaves the list until the toggle
  opens again. Lines indent by heading level, an untitled heading reads "Untitled",
  and clicking a line puts the caret at the end of that heading. The block has no
  text of its own; converting a line into one keeps that line's words stored but
  unpainted, the way a divider does, so turning it back gives them back. Markdown
  writes and reads a single `<!-- quire:toc -->` marker line
- Embed block: a link shown as a card, from the insert menu, the slash menu or
  Turn into. The block stores the address and nothing else — the card's headline
  (YouTube, Figma, Google Maps, GitHub, or the host itself for a site the app has
  never heard of) and the address line under it are both read off that text as it
  paints, so there is no second copy of the link to go out of step. The arrow
  hands the address to your browser; nothing is fetched, no page is embedded and
  no favicon is downloaded, which is what keeps a card cheaper than the iframe it
  stands in for. Click the card and the address is what you edit — the headline
  changes as you type it, and an empty card says "Embed / No address yet" rather
  than showing a blank box. Markdown writes the bare address on its own line, so
  the file reads as a link in any other renderer; import turns a line back into a
  card only when it is one address and nothing else, so a sentence that happens to
  contain a url stays a sentence
- Fixed a marked line with room to spare painting its runs apart. A paragraph's
  inline marks render as side-by-side runs, and the row laid them out with
  Slint's default `alignment: stretch`, so any leftover width was divided among
  the runs as gaps — ~155 px between three of them on the first short formula
  line the sweep had ever shot. Every marked line in the then-44-scene baseline
  overflowed its frame instead, so nothing showed it before. The fix is on all
  three run rows — block, table cell, column line — and moved none of them
- A paragraph carrying inline marks now wraps like a plain one. This was the
  documented platform wall: Slint `Text` has no inline formatting, so marks paint
  as separate runs, a run was one whole stretch of text, and a layout cell cannot
  break — a marked paragraph ran off the edge and was clipped mid-word while the
  identical unmarked paragraph wrapped. `build_runs` now cuts every *unmarked*
  stretch to one word per run and the row lays the runs out with a wrapping
  `FlexboxLayout`, so the breaks land where the words do; marked stretches stay
  one cell, because an underline, a code box or a link's click target split at
  every space is worse than the bold phrase it still cannot break (ADR-0041).
  What it costs, measured rather than assumed: ≈2.8 ms per projection of a
  10 000-row page whose every tenth line is marked (and nothing on an unmarked
  page), 1.031× the control's memory on that same page, and ≈3 px of extra line
  spread on a marked line whose words were already fitting
- Fixed the page title sitting below where it is bound. The row's title band
  sets its height but used to leave `y` alone, and Slint centres such a child
  vertically in its parent — harmless while every first block was one line
  tall, visible as soon as the first block was a table, whose grid then
  painted over the title. The offset was ~34 px on a paragraph page and
  ~60 px on a table page; the band now states its `y`
- Page block: embeds a child page (insert menu "Page"). The row shows a
  page icon and the child's live title (renames propagate), clicking it
  opens the child, deleting the block deletes the child page, and
  duplicating copies the child so the two blocks never share a target.
  Exports as a `quire://page` link that re-imports clickable
- Link-to-page block (insert menu "Link to page"): points at any existing
  page through a filterable picker; the target is not owned — deleting
  the block, duplicating, or pasting it never touches the page
- Rich paste: pasting markdown with block structure (headings, lists,
  to-dos, quotes, code) splits it into real blocks with inline marks;
  plain text still pastes natively at the caret
- Ctrl+V pastes a screenshot: when the clipboard carries no text but holds a
  bitmap (`CF_DIBV5`/`CF_DIB` — what Snip-and-Sketch and PrintScreen write),
  it becomes an Image block stored like any other attachment. An empty block
  *becomes* the picture; a block with words in it gets the picture below, so a
  paste never leaves a stray empty line. One undo step. When a copy carries
  both text and a picture, the words win
- Every popup (page menus, ⋮⋮ menu, slash menu, command palette, search)
  dismisses on a click outside it and on Escape; UI state follows so
  nothing stays blocked behind an already-closed menu
- Modals — the delete confirm, Settings, the Add-link card — now dim the window
  behind them. Anchored popups deliberately still do not: a menu that asks you
  to pick a type for one line should not hide that line
- The "/" block menu now measures its own height when it picks where to sit, so
  a long list stays inside the window instead of running off the bottom edge as
  soon as another kind is added to it
- Fixed the handle (+/⋮⋮) being clickable while invisible on the block
  being edited: it now shows whenever the row or the buttons are hovered,
  editing or not
- Inline marks: bold (Ctrl+B), italic (Ctrl+I), inline code (Ctrl+E),
  strikethrough (Ctrl+Shift+X), links (Ctrl+L + dialog; click a link to
  open it — internal quire:// links navigate in-app), inline math (Ctrl+M over
  a selection: the same LaTeX subset as a math block, rendered inside the
  sentence; Markdown writes it as `$…$`, and a sentence that merely mentions
  prices — "costs $5 and $10" — stays prose)
- Toggle the sidebar with Ctrl+\ — it had been Ctrl+B, which is bold, so the
  same chord was labelled two different things in two different places
- A brand-new page can be written in: click the "This page is empty" panel, or
  press Enter in the title, and the page makes its own first paragraph, focused
  and one undo step away from gone (ADR-0033)
- Code blocks can be coloured: the block's own ⋮ → Language menu picks the
  language (Rust / Python / JavaScript / TypeScript / Markdown / JSON / Bash) and
  the block paints five token colours over its own text — keyword, comment,
  string, number, name. Only the language is stored; the colours are derived while
  drawing, so a keystroke never lexes and a block being edited shows plain text
  (ADR-0042). A Markdown fence's info string carries it both ways (`rs` in,
  `rust` out), a rich paste keeps it, and a language this build cannot lex is no
  colour rather than a broken block
- Per-page find bar (Ctrl+F) with hit counter and selection navigation. Every
  match is now painted, not just counted: a hit is one word-run cell with a pale
  fill and an amber border, so the box says where the text is without taking the
  block's own colour below what the palette already allows. A hit that lands in
  the middle of a word splits the word, one that lands inside bold or a formula
  tints the whole mark, and one inside a table cell or a column box rides on the
  row that draws it. Quote and callout hold their text in a `Text` of their own
  and now share that flexbox, so the counter and the page reconcile: on the swept
  page 16 hits are 14 boxes plus the 2 in the block the bar stepped into, which
  shows the editor's selection instead (ADR-0043)

- Undo/redo (Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z) — per page, command-based

### References
- Typing `@` in text opens the picker the slash menu already uses: its first row
  is a date, the rest are the workspace's pages. Applying one replaces the `@`
  and the filter words typed after it with a chip in a single undo step
- A mention stores the page's id and never its title, so renaming a page stays
  one write and no reference has to be re-printed: the chip, the backlink
  panel's group headers and Markdown export all read whatever that page is
  called right now. A page that is gone reads `(deleted page)` and greys out;
  clicking a live chip jumps to the page
- `@date` is a chip of its own, with a clock instead of a page mark: it stores
  the ISO day it was set to rather than "today's number", so it does not drift
  and only one spelling is ever written
- Backlinks: the bottom of every page lists the blocks that point at it, grouped
  by source page — five rows collapsed, fifty expanded, and one line that states
  the total so a page cited 200 times never pushes the text off screen. It is
  derived, not stored (no table, no column), and two indexes keep one page open
  at 78 µs instead of a full-library scan. Clicking a row jumps to the source
  block, opening its page first when it lives elsewhere
- Synced block: a second view of another block, from the insert menu or the
  slash menu. The copy holds no text of its own — it draws its source's line, and
  typing into either one writes that single content, undone by one Ctrl+Z.
  Deleting a mirror takes only its row; deleting the source leaves the mirrors
  visible, read-only and saying `(deleted source)`. A pair that would point at
  each other is refused at the moment you write it, not discovered while
  drawing. Markdown writes a mirror as its source's line, and import
  deliberately does not learn the syntax — a block id from another file would be
  a reference born broken

### Databases
- A database is one block with a tab strip: eight layouts over the same records
  — table, board, list, calendar, gallery, timeline, form, chart — where "+"
  adds a second view and each view keeps its own filters, sorts, grouping and
  layout. The insert menu's other five layout names stay muted on purpose,
  because a layout is a property of a view rather than a second kind of block:
  the way to a board is a tab, not a new block
- Seventeen property kinds: title, text, number, select, multi-select, status,
  date, checkbox, url, email, phone, files, created time, last edited time,
  formula, rollup, relation. The Columns popup shows and hides columns and, on
  its second panel, changes what a column is; created and last-edited stamps
  come from the record itself and are never typed. A cell with no value stores
  nothing at all, so "empty" means one thing in every screen
- Filter, sort and group are questions asked of the database, not a pass over
  rows already in memory: the operators offered depend on the column's type, a
  rule whose value you have not filled in constrains nothing, and a date sorts
  as text because its format is fixed width
- The search box is one needle over a record's title and text, and the header's
  "N rows" answers to it — the count comes from SQL before the window opens
- Formula: an expression sheet with a real lexer and interpreter (no script
  engine, no new dependency). `if / length / round / abs / min / max / text`,
  four budgets that cap tokens, depth, steps and result size so nothing can run
  away, and a cycle is refused when you save rather than hanging the draw. A
  formula's value is computed as the window is projected and never stored —
  which is also why sorting or filtering a formula column says no: that would
  mean evaluating the whole column first
- Rollup: three questions in order — through which relation, which column, what
  fold (count / sum / min / max / average, or none). Its chooser lists only what
  the save path would accept, so a column that cannot be folded reads greyed
  there rather than refusing afterwards
- Relation: a column points at another database, optionally declaring the
  back-pointer on the far side, and its cell is picked from a list with its own
  search box and a tick on what the cell already holds. One pick writes the cell
  and every mirror it implies as a single undo step, and the pairing rule is what
  makes a relation cycle unrepresentable instead of detected
- Linked view: a second block that draws a database which already exists, so one
  table can sit on two pages without the records being copied
- Each row's hover slot carries two actions: delete (record, its values and its
  page go together in one Ctrl+Z) and "T", which makes that row the database's
  template — every later "New row" arrives prefilled from it in one undo step,
  and a column the schema has since lost simply does not prefill
- Chart is the eighth layout and it plots aggregates rather than rows: one
  grouping query feeds bar / line / pie drawn with the primitives already in the
  app (no chart library), realizing zero rows however large the database is
- Ten thousand rows are never all present: the count is computed first, then a
  window, so only the rows on screen exist as objects (kilobytes rather than
  megabytes) and scrolling inside a window that has not changed recomputes
  nothing

### Workspace
- Page tree: create / rename in place / duplicate (nested lists survive) /
  delete with confirmation; favorites; recent pages; last page restored
  on startup
- Page title edits in place; word/char count in the editor footer
- Page look (top bar ⋯ → Style): a page picks its own typeface — Default, Serif
  or Monospace — and switches Full width and Small text. All three are stored on
  the page (schema v10: `pages.font`, `pages.layout`), never on a block: one
  derived token layer applies them to the document tier, so the sidebar, menus,
  palette and settings keep their own type. Small text shrinks body and headings
  by the same factor; full width drops the centred column for a left gutter.
  Not undoable, like Favorite — a look is a property, not an edit — and a
  duplicated page starts with its source's (ADR-0044)
- Page icon (top bar ⋯ → Set icon): a 96-emoji grid, twelve rows of eight, plus a
  None row to clear it. The page stores the emoji itself (schema v11:
  `pages.icon`), not the grid's index, so the catalogue can grow without
  rewriting anybody's page. An unset page shows its title's first character in the
  sidebar tree — the shortcuts keep their star and clock until a page really has
  an icon, and the page title shows nothing above itself rather than its own
  first letter at 46px. Like Style, setting one is not an undo step, and a
  duplicated page starts with its source's (ADR-0045). An icon is an emoji only —
  a picture belongs on the cover, not in a 16 px slot (ADR-0046)
- Page cover (top bar ⋯ → Set cover): a local picture behind the page title, with
  Change cover and Remove cover beside it. The page stores an attachment reference
  (schema v12: `pages.cover`, nullable), never a path, so the STORAGE reclaim can
  tell that the page still draws those bytes. A fixed dark veil sits between the
  picture and the title, which is what makes the title's contrast a bound rather
  than an opinion: white ink measures **6.19:1** over the worst picture a user can
  pick — a pure-white one — with room to spare over the 4.5:1 floor (which needs
  only α ≥ 0.535, and this veil is α 0.6196).
  `benchmarks/scripts/contrast_probe.ps1` reads that number off the
  rendered screenshot, self-checks its arithmetic, and ships with a scene built to
  fail so the gate is shown able to say no. The page's own emoji takes the same
  rule (it measured 1.04:1 before this was caught on pixels). Like Style and the
  icon, setting a cover is not an undo step, and a duplicated page starts with its
  source's (ADR-0047)
- Page lock (top bar ⋯ → Lock page): a read-only switch on the page itself
  (schema v13: `pages.locked`, `NOT NULL DEFAULT 0`, so "open" is a value and an
  old library upgrades with nothing locked). It closes the document — every
  block's content and the page's own title — while leaving the page's *look*
  (icon, cover, Style, favourite), the tree (move, delete, duplicate) and every
  read (navigation, search, folding) alone. Two gates, because one is not enough:
  the command funnel refuses the write, and the row's `editing` binding refuses
  the caret, so a stale edit cannot survive a lock/unlock/relock cycle. Nothing
  is swallowed silently — the notice bar says which switch to flip, the page says
  it in a pill above its title, the ⋯ row reads "Unlock page", and the ⋮⋮ menu
  keeps only its two read-only Copy rows. Undo is refused while a page is locked
  and its stack is kept, not dropped; a duplicated page starts **un**locked,
  which is the one place a lock departs from a look (ADR-0048)
- Template library (top bar ⋯ → Templates): a template is a page you cannot open —
  its rows are a body to copy from, and that is all it ever is (schema v14:
  `pages.template`; no second content format, so marks, colours, code languages,
  column layouts and page references ride along in a copy untouched). Six rows
  manage the whole feature: insert one into the page you are typing in (the slash
  menu and the "+" handle offer the library too, filtered by what you type), start
  a new page from one, save the current page as one, export and import through the
  same Markdown channel pages use, and delete one. An insert is one Ctrl+Z step, not
  eleven, and it fills the empty line it lands on rather than sitting below it. Five
  presets — Meeting notes, Weekly review, Project brief, Bug report, Long-form draft
  — land on a library's first start, and deleting one is permanent: they are imported
  once, not re-added at every launch. A template stays out of the sidebar, the page
  tree, the palette, search and the shared LAN workspace, which is a consequence of
  it never being attached to the tree rather than a list of places that remember to
  hide it (ADR-0049)
- Fixed: an **empty** list item used to vanish from a Markdown export, so a
  template's blank row — the line left open for somebody to fill in — did not
  survive its own round trip. An empty block now writes its marker bare (`-`,
  `1.`, `>`, `#`) and comes back as the same empty block, which also fixes
  ordinary pages whose drafts include an unfilled bullet (ADR-0049)
- Page version history (top bar ⋯ → Version history): one popup, two views — the
  versions this page has had, and what changed between one of them and what is on
  screen now. The three actions live on three surfaces: naming is the field at the
  bottom (always there, because "now" is worth a version), comparing is a row click,
  and restoring is a button that only exists *after* a comparison, because a restore
  replaces the page. A version is §二十五's snapshot mechanism pointed at one page:
  a `VACUUM INTO` copy in a `versions/` folder beside the database, narrowed to that
  page — so reading one back goes through the same loader the app opens the library
  with, and there is no second content format for marks, colours, code languages,
  tables or page references to fall out of. Which versions exist is two metadata
  rows each: the name the user typed, and the attachment files its rows point at,
  which is what stops the reclaim from freeing a picture a version still needs.
  Twenty per page, oldest going, the panel says the number out loud, and deleting a
  page forgets the versions it had. A restore is **one** Ctrl+Z step and rolls back a
  page's *content* only — the title, icon, cover and style the user chose since stay.
  The comparison is line-level and identifies a line by its block, so a moved or
  edited line reads as the pair it is; a stretch too large to align is reported as a
  rewrite rather than computed (ADR-0091)
- Slash menu ("/") for block types; command palette (Ctrl+K) with page
  jumping, plus Go Back / Go Forward (Alt+← / Alt+→) along the pages
  visited this session — a page deleted since drops out of the history
  instead of being opened
- Full-text search (Ctrl+P) — titles and content, Chinese included
  (FTS5 + segmentation)
- Context menus on pages and blocks; pages move: Move up / Move down
  reorders siblings, Move to reparents anywhere outside the moved
  subtree (the whole hierarchy travels, recorded and restored across
  restarts)

### Persistence & reliability
- SQLite (bundled, no server): pages, blocks, marks, colors, attachments,
  settings, metadata (schema v1–v14)
- Debounced batched writes; Ctrl+S forces a save; close saves too
- Rotating snapshots on every open (5 generations), restore-at-open when
  the main file is damaged, damaged file quarantined (`.corrupt`)
- Named page versions (ADR-0091) share that mechanism and not its lifecycle:
  one self-contained SQLite file per version in a `versions/` folder beside the
  database, twenty newest per page, pruned by hand or by the cap and never by age.
  A file the index no longer names is swept on the next save, so a save that died
  halfway leaves nothing invisible behind
- Startup integrity checks; schema migrations (v1–v14). The steps that only add a
  column share one helper and each guard on its own column's absence, so a
  half-migrated file — one somebody edited by hand, or restored to the middle of a
  sequence — converges instead of erroring on a duplicate column name
- Picture and file bytes live in an `attachments` folder beside the database
  and the row is only a reference — a reference whose file row is gone still
  loads the library and renders as a missing picture. A file is copied in
  without being read, so nothing about attaching one scales with its size
- Settings → STORAGE can **reclaim** the attachments nothing points at any
  more: the rows go first (`Change::AttachmentDeleted`, which no command plan
  emits), then the files beside them, and the notice bar reports how many and
  how many bytes. "Nothing points at" counts every page's blocks, every step
  still on any page's undo *or* redo stack, the copied block, and the pictures
  any stored version points at, so the
  100-step undo cap is how long a picture is protected, and the sweep reads
  only the rows this session loaded, never the folder. It runs on the UI
  thread and costs about half a second per thousand attachments
- An unclean end (panic, kill, native crash, power loss) is recognized on
  the next start: the notice bar says so and the fact is queryable in the
  metadata table (a panic additionally keeps its report)

### Desktop integration
- Frameless window with custom title bar, light + dark themes (persisted)
- The light theme's quietest text is measured rather than eyeballed: the third
  text tier (block handles, footer, sidebar, every hint row) went from 2.5–2.7:1
  to 4.1–4.4:1, and the two weakest block colours from 2.81 and 2.48 on their own
  tint to 3.61 and 3.56 — the band the dark theme has always sat in (ADR-0023)
- Window size remembered; last-opened page restored
- Settings: appearance, LAN sharing, the database folder (open it in
  Explorer, take a backup on demand, reclaim unused attachments)
- Markdown export/import (page level, inline marks round-trip)
- "Copy Page as Markdown" (command palette): the page through the
  exporter onto the clipboard, CJK-safe (Win32 FFI write path)
- An attached file opens in whatever the system has registered for its type
  (one `ShellExecuteW` call — no `windows` crate, no subprocess), and saves
  back out to any path picked in a Save-as dialog
- A link — inline, in a link block or on an embed card — leaves the app only
  when it is an address a browser understands: http, https or mailto. A local
  path, a network share, `file://` or a protocol the shell happens to have
  registered now does nothing, because a link's target is data that arrives
  from a file and the shell will *run* a path as readily as it opens a url
- Installer (Inno Setup): per-user, Start menu + desktop shortcuts,
  optional `.md` "Open with" association; `--open <path>` dispatch
- GPU rendering (FemtoVG default; Skia / wgpu builds selectable); idle
  CPU ≈ 0, a 10 000-block page costs ≈10 MB over the empty shell

### Android (M9.pre)
- The shell builds for `aarch64-linux-android` and `x86_64-linux-android`: the
  library is a `cdylib` whose entry is `android_main`, `slint` is declared once
  per target operating system, and `rfd` — which has no Android backend and
  fails to compile there — moved to the desktop side of that split (ADR-0095)
- The database and the attachments live in the Activity's private files
  directory (`internal_data_path()/Quire`), fed to the placement rules that
  already took a per-user path as a parameter; `quire-core` did not change for
  this. An Activity that answers no path says so in the notice bar instead of
  running an in-memory session that looks saved
- The seven file dialogs in the app go through `platform::Picker`. On Android the
  answer is `Unsupported` and the notice names the half that is missing ("cannot
  open…" vs "cannot save…"), so no button on a phone is silently dead. The
  Storage Access Framework door is M9.5
- One UI, not two: `UIState.touch-mode` (set once before the first layout) drops
  the title bar's window controls, turns the layout rail into an over-the-page
  drawer that starts shut, and mounts `MobileBar` — five thumb-reachable actions
  that are the desktop's own keyboard chords, so the bar adds no Rust surface
  (ADR-0096). `--touch` draws the same shape in a Windows window
- `scripts\android-build.ps1 -Task check|lib|apk` is the local entry: both ABIs,
  one flag (`--no-default-features`), and an APK through `cargo apk`. There is no
  CI for it, and nothing here has been run on a device — the milestone is a build
  result, not a behaviour claim (`docs/ANDROID_NOTES.md`)
- The first signed package is `target/release/apk/quire_shell.apk`: **24.29 MiB with
  both ABIs inside**, each one ≈12 MiB compressed and 31.3 MiB installed, against
  a 26.56 MiB desktop `quire.exe`. `strip` is worth 0.39 / 0.15 MiB of that, so
  there is no size lever left in it, and the cold release pass costs 17–23
  minutes per ABI (`docs/PERFORMANCE.md`, M9.pre section)
- `-Task apk` judges itself by the file it produces, not by cargo-apk's exit
  code: cargo-apk 0.10 reads the manifest itself and packages every artifact it
  finds there, so a `[[bin]]` target makes it panic (`Bin is not compatible with
  Cdylib`). `--lib` names the one target this platform can package. The script
  throws unless `quire_shell.apk` is on disk and newer than the run
- The library target is `quire_shell`, not `quire`. Making it a `cdylib` put a
  second product behind the binary's own name, and the two shared
  `target/<profile>/quire.pdb`: a `cargo build --lib` replaced the 413 MB symbol
  file `quire.exe` is documented against with the DLL's 187 MB. Cargo warns about
  that ("this may become a hard error", rust-lang/cargo#6313) and then does it
  anyway. Five consumers carry one `use quire_shell as quire;` line each instead
  of a path rewrite, and the same `[lib] name` is what the APK's
  `android.app.lib_name` and its `libquire_shell.so` are read from (ADR-0095)

### Android (M9.a · touch chrome)
- The editor's own chrome is reachable on a phone now (ADR-0097): a long-press
  on a block row opens the ⋮⋮ block menu through the same callback the desktop
  grip click uses, and the menu carries a touch-only **Insert below** row that
  runs the "+" handle's insert. The gesture is guarded three ways against the
  press that is really a scroll (a 12 px move, a stolen press arriving as
  `cancel`, and the editor's scroll y compared against press-down), and a fired
  long-press swallows the click the lifting finger produces
- The three row menus stand at the 44 dp touch minimum in touch mode —
  block menu 28 → 44 px, slash/insert menu 32 → 44 px, the page ⋯ menu
  30 → 44 px — and every Rust anchor that multiplies a row count multiplies the
  same number the popup draws. The desktop keeps its old numbers, and the sweep
  proves it: 131 shared scenes byte-identical against a control build of
  `dc2399f`, the one new file being `touch-menu.png`, the scene that renders
  the phone's menu
- Known shape: the database popups, the settings rows and the file block's
  26 px buttons are still desktop-sized (the rest of the 163-literal list), and
  drag-reorder is still handle-only — on a phone the menu's Move up/down is
  the reorder path

### Android (M9.b · the phone's own defaults)
- The Android build compiles the same `.slint` tree at a fixed scale factor of
  1.5 (`build.rs`, `CompilerConfiguration::with_scale_factor`). The activity
  backend derives its scale from the device's density bucket (dpi / 160), and
  the tablets this shell runs on report an mdpi-class bucket — so the
  desktop-sized layout landed at desktop *physical* sizes: 13 px body text and
  40 px bars on a 10" screen, every touch target half what a finger wants. 1.5
  lifts fonts, rows, bars and popups together; the 2000-px surface becomes a
  ~1333-px logical one and the 44 px touch rows render at ~66 physical px. The
  desktop's build script is untouched, so its scale stays the window system's
- Dark is the default theme on both platforms: a library with no stored `theme`
  row opens dark, and a stored "light" still wins (`dark_setting`). Every scene
  the sweep photographs without an explicit `set-dark` therefore renders dark —
  the sweep compares against a fresh control build of this commit, as always
- The thumb bar gains the "+" the phone never had: a sixth item that inserts an
  empty paragraph after the page's last block (or makes the page's first one
  through the empty state's front door) and opens the same insert menu the
  desktop's "+" handle opens, anchored above the bar. The dark-mode and bar
  changes ride the same `UIState` callbacks the keyboard chords use

### LAN sync (crate::sync)
- Two Quire installs on one network keep each other's workspace by exchanging
  whole snapshots and merging them three ways (aw-server-plus's model, adapted
  to Quire's integer ids and its change lists). The engine is
  `src/sync/`: UDP discovery on 46000, a dependency-free HTTP server on 5878
  (`/sync/info`, `/sync/snapshot` GET and POST, `/sync/attachment/<id>`,
  `/sync/pair`), a worker thread that runs one pull-merge-push cycle at a time,
  and a Slint `Timer` on the UI thread that answers every job that has to touch
  the session — the workspace is `Rc`-bound to that thread, so nothing else may
- The merge is against a **shadow** (what the two devices last agreed on, one
  settings row per peer): a row only the other side moved is taken, a row only
  this side moved is kept, a row both sides edited keeps this device's copy and
  says so in the log, and a row *neither* side had before — where both minted
  the same integer id, because ids are per-device `max + 1` — is renumbered
  against this session's own watermarks and kept alongside, cascading to the
  pages, blocks, attachments, databases, columns, views, records and cells it
  carries. First contact between two populated devices runs without a shadow
  and treats every differing row as a conflict: two workspaces meeting for the
  first time is one user decision, not one algorithm
- Pairing is trust-on-first-use: a device found on the wire (or typed in by
  hand as `ip[:port]`, for networks where the announcement cannot get through)
  is asked to pair with one POST, and both sides end up in each other's table.
  Sync then runs on demand (**Sync now**) or on a minute timer while the
  automatic switch is on; attachment bytes travel over the same protocol
  (`/sync/attachment/<id>`), pictures re-entering through the ordinary
  `import_bytes` path so the receiver builds its own preview
- What the snapshot deliberately does not carry: settings rows (device-local by
  policy — theme, window size and the peers table stay where they were set),
  named versions, and the `created`/`edited` stamps of database records, which
  the receiving store stamps for itself

### Build & test
- `just check`: `cargo check --all-targets`, the whole test suite, a release
  build. Visual regression and the RAM/CPU scenes run from
  `benchmarks/scripts/` (`sweep.ps1` compares against a manifest of hashes)
- The benchmark row writers no longer commit a machine path. ADR-0094 rewrote
  the 22 `benchmarks/results/*.jsonl` files that had one, and the next run would
  have written it straight back: `exe` and `db` are absolute paths at run time.
  `benchmarks/scripts/redact.ps1` now substitutes `<repo>`, `<temp>`,
  `<localappdata>` and `<user>` before those fields are JSON-escaped, in all four
  writers, and warns instead of staying quiet when an account name still reads as
  a whole path segment (ADR-0094)
- `benchmarks/scripts/stress_ladder.ps1` measures past the matrix's 10 000-block
  ceiling: 20 000 / 50 000 / 100 000 rows, a scroll at those sizes, a whole page
  of pictures or of coloured code, a 500-page sidebar. It waits for a *settled*
  idle rather than sampling two seconds after the window appears, publishes
  `settle_wait_ms` / `settled`, samples peak working set through the run, names
  which clock ended a process (`refused-to-exit` vs `harness-grace`), and stamps
  every row with the exe hash it ran against and the app's own `dump-state`
  identity. `-Only` throws on a filter that matched nothing
- `benchmarks/scripts/redact.ps1` now matches its prefix rules on either
  separator. A caller that hands it a forward-slash path — which is what a
  bash-style launch argument gives `stress_ladder.ps1` — used to fall through
  the `<temp>` rule and get caught only by the account-name fallback, so the
  row read `C:/Users/<user>/AppData/Local/…` and looked scrubbed while naming
  the machine. The 18 `exe` fields of
  `benchmarks/results/2026-09-23-stress-ladder-vg-r2.jsonl` are rewritten to
  `<temp>/…`
- A test that needs a folder — a database, a log family, an attachments
  directory — gets one from `quire::testing::ScratchDir`, which deletes it when
  the test ends. Each helper used to create a uniquely named `%TEMP%` directory
  and walk away from it: 3 639 of them had accumulated, and the naming that
  existed to stop a test reading a stale database was only load-bearing
  because nothing ever cleaned up
- The bench scripts delete the scratch database, its `.bak<N>` snapshots and
  their own report file when a run ends; the purge used to happen before the
  run and covered only the `.db`, so every label left its snapshot behind
- `quire-shot --hover x,y` parks the pointer without pressing a button, which
  is the only headless way to light UI that exists on hover alone (a table's
  edge toolbar, a row's ⋮ handle)
- `benchmarks/scripts/diffbbox.ps1 -OldDir A -NewDir B` prints, per scene, the
  bounding box of the pixels that moved between two sweeps. A re-sweep is
  judged from that table rather than from 42 pictures: when every box lands
  inside the band the change explains, one verdict covers the set
- The command palette's ids now resolve through `palette_action()` in Rust, and
  its dispatch is a `match` with no wildcard arm, so a new command that skips a
  variant is a compile error. A test walks the real command registry and asserts
  every row resolves to its own distinct action — the class of bug that once
  shipped with every palette row from id 9 up running one action, green tests
  and all (ADR-0034)
- `--pictures N` makes a bench scene of media: every `rows/N`-th row of the
  bench page becomes an image block, backed by a generated 1280×720 PNG from a
  pool of 200 files written when the library is first built. The fixtures are
  not re-encoded during the measured passes — a run that timed PNG encoding
  would not be measuring pictures — and the app prints its own decode cache
  figures (`--dump-state`, one JSON line on stderr) because a process memory
  counter cannot see a cache whose unit is one raster
- `--marks N` does the same for the other fixture scene D never had: it bolds the
  second word of every `rows/N`-th row, so a 10 000-block page can carry 1 000
  marked paragraphs and the RAM gate can see the runs channel at all. Without it
  the gate was blind — scene D has no marks, so a change to marked rendering
  measured 1.00× of nothing. `--dump-state` now also reports what it built
  (`gs+atlas-blocks=10024 marked=1000`), which lets each arm of an A/B prove its
  own fixture instead of taking the bench script's label on faith
- Scene F — continuous scroll — had never scrolled. Slint measures a list's
  content offset *negative* going down, and the harness timer was adding, so
  every frame wrote a value the clamp rounded straight back to zero: the scroll
  numbers recorded since M2 describe a repaint loop at the top of the page, not
  a scroll. `quire-shot --scroll-y` is now the control that catches this class
  of thing — two offsets that are real must produce two different PNGs — and
  `--scroll-step` sets how far a frame moves, so a flick and a wheel tick are
  separate measurements (ADR-0036)
- `benchmarks/scripts/scroll_ab.ps1` runs the scroll arms against two binaries in
  one sitting — the default build and a `--no-default-features --features
  skia-opengl` one in `target-skia/` — and fails the batch unless each binary's
  own first-paint line reports the renderer it is supposed to be. That
  self-identification is the whole point: the skia arm of the renderer verdict
  had been standing on the same broken scroll as the femtovg one, and a skia
  build that silently keeps the femtovg default would otherwise produce an A/B
  of a binary against itself (ADR-0036)
- The matrix's `-Only` filter now splits a comma list and throws when no scene
  matched. `powershell -File` passes `-Only A,B` to a `[string[]]` parameter as
  one string, so a multi-token filter selected nothing, printed nothing and exited
  0 — a run that measured nothing and said it finished. Two more shapes joined the
  matrix so every row the media batch published is reproducible from the script:
  the flick-sized scroll on a text page, and a short page of pictures
- A RAM gate now comes with its control. The previous four batches each reported
  a same-direction rise on both arms and recorded it as "session drift" without
  ever measuring what drift is, so the math slice built the commit before it
  (`2e9de99`) in a temporary worktree against the same target dir and ran the two
  exes alternately in one sitting: 1.016× on private bytes, with each binary
  first proving which build it is (md5, and the control tree has no math source
  file). The worktree goes once the number is in; the next kind that claims a
  per-row cost repeats the run against its own predecessor commit
- The contents block repeated that run against the commit before it (`cc7ccf0`) and
  came in at 1.007× on private bytes. It also bought the first whole-page projection
  number: the gate's scene has no contents block in it, so a green gate alone says
  nothing about a row that walks the page every time it is projected. Measured
  separately — 10 000 rows project in ≈38 ms with or without a 1 000-line list —
  and the delta is inside the noise of its own control
- The embed card is the third slice to run that comparison, and it lands in the
  same place: **1.005×** on private bytes, a 0.55 MB gap between the arms against
  the control arm's own 1.2 MB spread (rows
  `benchmarks/results/2026-09-21-m11-embed-ram.jsonl`). Three kinds in a row
  inside one megabyte says the gate has a *floor*, not that all three slices are
  free: a change whose whole per-row cost is a string function is below what this
  instrument resolves, and `docs/PERFORMANCE.md` now says that instead of
  publishing a ratio that only looks like a measurement
- The workspace is two crates now (ADR-0093): `crates/data` holds the model, the store
  and the windowless services — 31 455 lines that compile against nothing but `std`,
  `rusqlite` and `image` — and the root package keeps the Slint shell. That is the
  precondition for the Android port and for a Rust sync module, and it moved the gate
  command: **`cargo test` at a workspace root tests only the root package**, so the bare
  form here reported 134 passed and exit 0 while 431 data-layer tests sat unrun. `just
  check` says `--workspace` on all three lines now. Nothing else moved: 565 passed / 23
  ignored with the 589 test names proved identical to the run before, and 131 of 131
  sweep PNGs byte-identical between two shot binaries that differ by md5
- The same day that crate left this repository (ADR-0094): `crates/data/` is deleted,
  and `github.com/PT123123/quire-core` at a pinned rev is where the model and the store
  live now — 71 commits carried out with `git filter-repo`, of which 46 had a personal
  QQ address as author and committer and were rewritten to the GitHub noreply identity
  before the first push. The same pass then went over **this** repository, where 137 of
  174 commits and the tagger of `v0.1.0-rc1` carried the same address on a public remote
  — Track 3's handoff had asserted the metadata was clean because it had scanned the
  diff and never `%ae`, and that sentence has been corrected in place rather than
  quietly fixed. Every SHA quoted in these documents predates it. Landed as
  `origin/master` = `f475026` and `v0.1.0-rc1` = `1dbbde4`; the check that says the
  source did not move is that `cargo check` on the adopted head finished in **11 s**
  having recompiled nothing, and the shell's suite still reads 134 / 0 / 10 The rename is the only content change: 38 of 45 source files
  still hash byte-identical to the blobs this repository holds at `09ef5aa:crates/data/`,
  and the other 7 hash identical once the name is reversed. Two things follow that are
  worth knowing before quoting a green run. **The suite is no longer reachable from
  here** — a git dependency is not a workspace member, so `cargo test --workspace` runs
  the shell's 134 / 10 ignored and only *builds* the crate whose 431 tests gate it. And
  **cargo's own git cannot fetch a public repository**: it offers credentials the
  anonymous URL never asked for and takes a 401, so `.cargo/config.toml` now sets
  `net.git-fetch-with-cli = true` and a fresh clone builds without an SSH key

### Known limitations
- Switching directly from one open menu to another (e.g. ⋮⋮ on a different
  block while a menu is open) takes two clicks — the first click only
  dismisses the open popup (standard Slint popup semantics)
- A menu taller than the window (e.g. Move-to in a large workspace) is
  clamped to the space below its anchor and scrolls (the page menu) or is
  re-anchored when a submenu changes its row count (the ⋮⋮ block menu) — so
  nothing runs off the bottom any more. What is left is the difference
  between the two: a menu taller than the whole window scrolls only the
  page menu, because the ⋮⋮ menu is still a plain repetition, not a
  `ListView`. The Settings dialog no longer has that shape: it fits 1280x800
  with its shortcut list, ABOUT and Done on screen, and the sidebar chord is
  now one of the rows it lists
- Block colors are cosmetic: they do not survive a Markdown export/import
  round trip, and Callout blocks export as quotes
- Inline-mark paragraphs wrap between words now, and still clip in two shapes:
  a marked phrase longer than the line (a mark is one unbreakable cell), and a
  marked line that needs more lines than the same words unmarked — bold and mono
  are wider, and the row's height is measured from the plain text
  (Slint `Text` has no inline formatting yet)
- Pictures: replacing a stored file on disk in place needs a
  restart to show up. A page of pictures is now measured while it scrolls — the
  decode cache holds nine 1280×720 rasters and stops there by weight, and an
  on-screen picture costs the process roughly three times the raster — but the
  fixtures are generated gradients, so the disk footprint and any decode-time
  arm are optimistic against a real camera original. A clipboard picture other
  than a bitmap (a `file://` HTML image, an SVG) is not read — only
  `CF_DIB`/`CF_DIBV5`
- Attachments are reclaimed by hand, never on their own: Settings → STORAGE
  → Reclaim deletes the stored files no block, undo step or the copied block
  points at, and nothing runs it for you — a picture is protected for the
  100 undo steps of its page, and after that it waits for the click. And
  because the sweep reads only the rows this session loaded from the
  database, a file in the `attachments` folder whose row is already gone
  stays on disk: it is invisible to the count, and deleting it would mean
  listing the folder, which a session that failed to load its attachments
  would get terribly wrong
- A PDF attaches and opens, but shows no first-page thumbnail: it looks
  like any other file apart from its name. Deferred by explicit decision
  2026-09-20, and a version that was built but never committed was
  dropped on 2026-09-23. How it would be drawn is no longer an open
  question (four routes measured, the pure-Rust one chosen — docs/
  REPORT_TRACK4.md §T4.1); that it ships at all still is
- Tables are a grid, not a database: no per-column widths, no header
  formatting, no sorting. Inside a cell only Tab / Shift+Tab cross between
  cells — Enter does not split one, Backspace does not merge it with a
  neighbour, and the arrow keys will not step up or down a row. A cell takes
  bold / italic / code / strikethrough / formula and — since this slice, on a
  selection like the other four marks — a link (Ctrl+L). Converting a marked
  line into a table keeps its words
  and drops its marks. Hovering the grid adds its toolbar as a row, so
  content below a
  table shifts down by 22 px while the pointer is on it
- Math is a Unicode reading, not typesetting: `\frac{a+b}{2}` paints on one
  line as `(a+b)/2`, so there are no stacked fractions, no alignment, no
  equation numbers, and a superscript falls back to its plain characters
  wherever the alphabet has no glyph for it. Unknown commands come back as
  their own source. A Math mark and the other marks do not stack — a formula
  inside bold text keeps the formula and drops the bold on export
- A contents block lists the headings of the page it sits on, and nothing more: no
  collection across pages, no "show levels 1–2" setting, no compact or inline
  option, and a long heading is elided to one line rather than wrapped. Clicking a
  line moves the caret, not the scrollbar — a heading below the fold still needs a
  wheel turn first, which is the same limit a `quire://block/` anchor has
- An embed card says who a link belongs to, not what it is: no title, no preview,
  no favicon and nothing fetched, because the app has no WebView and makes no
  request it was not told to make (it does have an HTTP/1.0 client —
  `--pull <url>`, no TLS). This is **closed**, not pending: `bookmark` was
  withdrawn 2026-09-22 (ADR-0081), so no fetched title is coming. A site
  outside the 21 names it knows is labelled with its own
  host, and a Google url is read as its product from the subdomain or the first
  path segment — anything else behind google.com says "Google". And since this
  slice, a link whose target is not http, https or mailto opens nothing at all:
  `file://`, a local path and a network share are the shapes a document can carry
  that a shell would run rather than open
- Markdown reads tables as plain text: export writes GitHub-flavoured
  tables, and importing one back gives a paragraph per row. Deliberate and
  pinned by a test — the importer is line-at-a-time and a table needs
  lookahead
- A built-in template is only as rich as Markdown: the five presets carry
  headings, bullets, numbered items, a todo, a quote, a link, a contents block
  (the one `<!-- quire:toc -->` marker the channel knows) and one code block,
  because that is what the importer knows. A preset that asked for a callout, a
  table, a colour or a column layout would arrive as a paragraph, so the built-in
  library cannot show those shapes — a template you save from your own page can,
  since that one is a copy of real rows. And saving never overwrites: start from a
  template, edit it, save it back, and the library gains a second entry with the
  same name rather than updating the first, so pruning is a Delete you do by hand
- Chinese IME: the manual acceptance pass (docs/IME_CHECKLIST.md) is
  signed off — 2026-09-20
- Databases say what they will not do rather than guessing: a formula or rollup
  column cannot be sorted or filtered (that would mean evaluating every row of a
  10 000-row column, which is the red line the windowed read exists to keep), and
  a rollup cannot fold through another computed column, so its nesting depth is
  zero by construction
- A relation's picker lists a bounded number of the target's rows and its search
  box is the way past that bound — but nothing caps how many records one cell may
  *point at*. It is measured, not assumed: folding a window whose relation fans
  out to 10 000 rows costs 28.5 ms against 9.1 ms for reading those 10 000 values
  out and folding them in the app, so the upper bound is the fan-out rather than
  the table size. Whether to cap it is an open product decision
- Deleting a page from the sidebar is still not an undoable action (it predates
  the command registry), and a record owns its page in storage — so a record
  reached only through that page goes with it. Deleting the *record* is undoable
  and takes its page in the same step; that is the path this feature controls
