# POC1 — IncreArt GUI prototype

Status: **complete** (October 2026). Throwaway prototype; the decisions it
settled are written into [DESIGN.md](DESIGN.md), which stays the spec. This
document records what POC1 is, how to run it, what it looks like, every design
choice made while building it (with the alternatives and the reasons), and how
it was tested.

## Contents

1. [What POC1 is](#1-what-poc1-is)
2. [Running it](#2-running-it)
3. [How the prototype is built](#3-how-the-prototype-is-built)
4. [Tour of the GUI](#4-tour-of-the-gui)
5. [Design decisions](#5-design-decisions)
6. [Testing](#6-testing)
7. [Known limitations and mock shortcuts](#7-known-limitations-and-mock-shortcuts)
8. [Open questions and next steps](#8-open-questions-and-next-steps)
9. [File map](#9-file-map)

---

## 1. What POC1 is

A clickable prototype of the whole IncreArt GUI, built to decide the screen
structure, the workflows and the on-disk model before writing the real
`increart-core` / `increart-gui-core` / `increart-gui` crates.

**It proves:**

- the screen structure: five top-level tabs following the pipeline
  (ideas → kept ideas → generation → review → kept outputs), and the sub-tabs
  of each;
- the two review queues (ideas, outputs) with the same keys and mechanics;
- folder trees for projects, the input library and output collections, with
  file-manager style operations (rename, copy, cut/paste, delete, drag and
  drop, shortcuts) and a Thunar-like folder view;
- the review flow from queueing a run to reviewing its images as they arrive,
  including A/B compare;
- the library model: content stores, links, IDs, collections and smart
  folders, export.

**It does not:** talk to sd-server or a PE server, read or write the library
on disk, or persist anything. All data is mock data created at startup; a
mock backend simulates generation and prompt rewriting. (A separate
generator, `scripts/make-sample-library.py` at the time, writes a real on-disk
sample of the library format; see [6.5](#65-on-disk-sample-library).)

## 2. Running it

The prototype lives in `tmp/gui-prototype/` (single crate, Rust 2024 edition,
built with rustc 1.98).

```sh
cd tmp/gui-prototype
cargo build --release
./target/release/increart-prototype                  # interactive, on your desktop
```

Automated runs never use the desktop display. They go through
`scripts/headless.sh`, which starts a private Xvfb display (see
[6.1](#61-headless-runs-scriptsheadlesssh)):

```sh
scripts/headless.sh env INCREART_SHOTS=/some/dir \
    tmp/gui-prototype/target/release/increart-prototype
```

With `INCREART_SHOTS` set, the app walks through every scripted scene, saves a
PNG per scene into that directory and exits (about 3 s for all 43 scenes).
That walkthrough is a regression run: scenes build on each other's state.

For documentation there is a separate tour, `INCREART_TOUR=<dir>`: 38 scenes,
each starting from a reset UI state, paced at ~60 fps so menus and windows
have finished fading in (about 35 s). It produced the screenshots in
[poc1/](poc1/README.md):

```sh
scripts/headless.sh env INCREART_TOUR=docs/poc1 \
    tmp/gui-prototype/target/release/increart-prototype
```

> `tmp/` is git-ignored. If POC1 should be kept, move `tmp/gui-prototype/` to
> a tracked location (e.g. `prototypes/poc1/`); see [8](#8-open-questions-and-next-steps).

Dependencies: `eframe` 0.37 (egui 0.37), `egui-phosphor` 0.14 (icons; see
[5.9](#59-visual-language)), `image` 0.25 with only the PNG codec.

## 3. How the prototype is built

- **One file**, `src/main.rs` (~6 000 lines), deliberately: it is throwaway,
  and a single file was fastest to reshape. The real app splits into the three
  crates described in DESIGN.md (a pure `update(state, msg) -> effects`
  reducer, tested headlessly, plus a thin egui view); POC1 does not.
- **One `App` struct** holds all state: mock ideas, projects (with runs),
  output items, folder trees, collections, the New-idea draft, rewrite jobs,
  import records, selection and view state per screen, undo stack, toast.
- **Immediate mode**: every frame redraws from state; widgets return
  responses, actions are collected into small enums (`TreeAct`, `FvAct`,
  `ReviewAction`, `LibAction`, `IdeaAction`) and applied after drawing, which
  keeps the borrow checker happy and gives one code path for buttons, menus
  and keys.
- **Shared components**, written once and used on several screens:
  - the **folder tree** (`tree_panel` / `tree_ui` / `tree_act`), generic over
    a `Side`: `Proj` (projects), `Lib` (input library), `Coll` (output
    collections). A leaf is a project, an idea, or a collection link
    respectively;
  - the **folder view** (`ui_folder_view` and the `fv_*` functions), a
    Thunar-like navigator over a tree's on-disk layout, with four independent
    instances: Projects, Input library, Idea inbox, Output library;
  - grids of cards/thumbnails with selection, right-click menus and drag
    payloads.
- **Mock backend**, advanced every frame by real elapsed time:
  - *generation*: two "GPUs" at 1.5 s per seed each, shared round-robin
    between all unfinished runs; every finished seed becomes an unreviewed
    item with its real seed number and the run's settings;
  - *PE server*: starts on demand (~2 s), ~2 s per rewrite, stops after 10 s
    idle.
- **Mock data**: 11 ideas (6 in the inbox, 4 kept, 1 discarded), 3 projects in
  folders (`clients/acme/fox-cafe`, `experiments/glass-fish`, `stickers`),
  ~50 output images (procedural textures; the first fox-cafe image is a real
  sd-server render), collections "Portfolio/animals" and
  "Client picks/acme", a few import records, two files waiting in the drop
  folder.
- **Screenshot harness** (`INCREART_SHOTS`, `drive_shots`,
  `raw_input_hook`): scripted scenes with synthetic mouse, keyboard,
  clipboard and file-drop input; see [6.2](#62-screenshot-walkthrough-with-synthetic-input).

## 4. Tour of the GUI

Screenshots of every screen and feature are in [4.9](#49-screenshots)
(38 images in `docs/poc1/`, indexed in [poc1/README.md](poc1/README.md)).

### 4.1 Top bar

**Idea inbox (N) · Input library (N) · Projects (N) ✦ · Review (N) · Output library (N)**

- Counts: unreviewed ideas, kept ideas, projects, unreviewed images, kept
  images. ✦ on Projects while anything is generating.
- Right side: `ROCm0` / `ROCm1` server status (generating / idle) — click to
  open Projects → Jobs; `PE` status (stopped / starting / running) — tooltip
  shows the rewrite queue, click opens the Idea inbox.
- Bottom status bar: toasts (every action reports what it did, with
  "Ctrl+Z to undo" where it applies).

### 4.2 Idea inbox — input review

Tabs: **New idea · Grid · Folder · Import · Discarded**. Opening the inbox shows
New idea when there is nothing to review, Grid otherwise. Files dragged from
the desktop onto any inbox tab are imported as new ideas (drop highlight
while dragging).

- **Grid**: idea cards (title, first images, text, variant count, rewrite
  state), oldest first. Keys: `1`/`G` keep ★, `2`/`N` keep, `3`/`B` discard,
  `P`/`Space` promote to a project, `W` rewrite with PE, `Enter`/`T` or
  double-click edit, `←`/`→` move. Right-click: the same plus copy text and
  "Accept all ready rewrites".
- **Edit idea** window: title, tags, text, variants, images; and, when a PE
  rewrite exists, its progress or the original-vs-rewritten diff (added words
  highlighted) with **Accept** (adds a variant, original kept) / **Reject**.
- **New idea** (`Ctrl+N` from anywhere): title, text, tags, images, Rewrite
  with PE (use as text / keep as variant / discard). **Add to inbox**, **Add &
  keep** (into a chosen library folder), **Add & promote**. The draft survives
  tab switches.
- **Folder**: file-manager view of the inbox only — `inbox_drop/` (waiting
  files, `imported/` originals), `unreviewed/` (one folder per idea),
  `discarded/` — with a Places list. Moving an idea between `unreviewed/` and
  `discarded/` discards / restores it.
- **Import**: drop-folder watcher status, waiting files with Import now,
  recent imports with the conversions applied and duplicates detected,
  import settings (strip GPS, delete imported originals after N days, clear
  now).
- **Discarded**: discarded ideas, newest first; restore to inbox or library,
  delete permanently (asks), empty all.

### 4.3 Input library — kept ideas

Tabs: **Grid · Folder · Discarded**. Full-height left panel: filters (text /
tag, starred, has images), selection actions (promote selected → one project
(`Space`), rewrite selected), the library folder tree.

- **Grid**: idea cards of the selected folder (and subfolders); click selects,
  drag cards onto tree folders to file them (a selected card drags the whole
  selection). Right-click: star/unstar, promote, add images as refs to the
  current project, rewrite, edit, copy text, back to inbox, discard,
  accept ready rewrites.
- **Folder**: the folder view over the library tree; an idea is a folder
  holding its image links.
- **Discarded**: the same view as in the inbox.

### 4.4 Projects — making images

Tabs: **Grid · Folder · Generate · Jobs**. Left panel: the project folder tree
(new folder / project, rename, copy, cut/paste, delete, drag and drop, `F2`,
`Del`, `Ctrl+C/X/V`).

- **Grid**: flat — one tile per project under the selected folder (no folder
  tiles). Cover = the project's chosen cover, else its best image (good >
  neutral > unreviewed > bad, most recent among equals). Unreviewed count as a
  pill on the cover, good / neutral / bad / unreviewed counts, ✦ while
  generating, folder path. Double-click a project to review it.
- **Folder**: Thunar-like navigator over the on-disk layout: folders,
  projects, and inside a project `runs/`, `unreviewed/`,
  `reviewed/{good,neutral,bad}/` with image links. Moving an image between
  review folders is a review.
- **Generate**: prompt + Rewrite (PE), negative, recipe, size (editable
  resolution list + custom W × H, with aspect / megapixels / time estimate),
  steps / CFG / shift, seed count and **first seed** (default 0, Continue =
  one past the project's highest), Transparent toggle (shows the exact prompt
  sent), reference strip, edit-from slot, Queue run.
- **Resolutions** window: per-row name, W × H (snapped to multiples of 32),
  aspect, megapixels, estimated time; set default, duplicate, swap W/H,
  reorder, delete; add, reset to built-ins.
- **Jobs**: two server cards with the seed being generated and a live
  preview; runs table (path, run, recipe, progress, cancel).

### 4.5 Review — output review

Top-level tab with its own scope (left panel: all projects / a folder / a
project, with unreviewed counts), independent of the Projects tab.

- **Single**: big image, name (`r01_s0003`), path, position; buttons prev /
  good / neutral / bad / next / pin / edit run / → input ref / → new idea;
  right-click menu with the same plus Add to collection.
- **Grid**: thumbnails; click selects, double-click opens single, right-click
  menu.
- **A/B compare** (`C` or the A/B button): pins the current image as A and
  shows the next as B; both can be rated, A/B can be swapped; keys act on B.
- "N generating — new images join the end of the queue", **newest ›** jump,
  and an empty state that says when images are still coming.
- Keys: `1`/`G`, `2`/`N`, `3`/`B`, `←`/`→`, `Space` (grid), `C`, `E` (new edit
  run), `R` (→ input library as ref), `I` (→ new idea), `Ctrl+Z`.

### 4.6 Output library — finished work

Tabs: **Grid · Folder · Rejected**. Full-height left panel: filters (prompt
text, score good+neutral / good only, has refs, has alpha) and **sources**:
**All kept images** (default), **Collections** (editable tree), **Smart
folders** (by date, by rating, by source idea, by project, transparent).

- **Grid**: the source's images; selecting one shows the lineage panel
  (source idea → parent ← image → children) and the recipe. Drag thumbnails
  onto a collection. Right-click: add to collection (or new collection), use
  as project cover, export, change rating, back to review, edit run, → input
  ref, → new idea, show project, copy path. **Export these N…**.
- **Folder**: folder view over the collections tree with a `smart/` folder at
  the top; images can be moved / copied between collections, removed,
  exported, set as cover.
- **Rejected**: images reviewed as bad, across all projects; re-score, back to
  review, delete permanently (asks), delete all.
- **Export** dialog: destination, keep metadata or not, names by
  run/revision/seed or by image ID.

### 4.7 Folder view (all four instances)

Toolbar: back / forward / up, path bar (clickable segments; `Ctrl+L` turns it
into an editable path), metadata-file toggle (`Ctrl+H`), Icons (size slider) /
Details (name, kind, info, size, modified) / Compact. Directories are plain
folder icons (projects blue, ideas purple); images are thumbnails. Selection:
click, `Ctrl`-click, `Shift`-click, rubber band, `Ctrl+A`, arrows, type-ahead.
Open: `Enter` / double-click. Right-click on an entry or on empty space;
Properties window; status bar with counts, selection size and path.

### 4.8 Keyboard map

| Where | Keys |
| --- | --- |
| Anywhere | `Ctrl+Z` undo the last review/discard/restore · `Ctrl+N` new idea |
| Idea inbox → Grid | `1`/`G` `2`/`N` `3`/`B` · `P`/`Space` promote · `W` rewrite · `Enter`/`T` edit · `←` `→` |
| Input library → Grid | `Space` promote the selection |
| Trees (side panels) | `F2` rename · `Del` delete · `Ctrl+C` / `Ctrl+X` / `Ctrl+V` · drag to move |
| Folder views | `Enter` open · `Backspace` / `Alt+↑` up · `Alt+←` / `Alt+→` back / forward · `Ctrl+A` · `Ctrl+H` · `Ctrl+L` · `F2` · `Del` · `Ctrl+C/X/V` · arrows · type-ahead |
| Review | `1`/`G` `2`/`N` `3`/`B` · `←` `→` · `Space` grid · `C` compare · `E` edit run · `R` → ref · `I` → idea |

Key handling is skipped while a text field has focus. `Ctrl+C/X/V` arrive
from egui as clipboard events, not key presses, so they never trigger
Review's plain `C`.

### 4.9 Screenshots

Generated by the documentation tour (`INCREART_TOUR`, see [2](#2-running-it)); each starts from a reset UI state. Mock data, 1400 × 900. The index with the same captions is [poc1/README.md](poc1/README.md).

#### Idea inbox

**Grid: idea cards and the keys panel**

![Grid: idea cards and the keys panel](poc1/01-idea-inbox-grid.png)

**Right-click menu on an idea**

![Right-click menu on an idea](poc1/02-idea-inbox-card-menu.png)

**Edit idea window**

![Edit idea window](poc1/03-idea-inbox-edit-idea.png)

**Finished PE rewrite: diff with Accept / Reject; cards show rewrite states**

![Finished PE rewrite: diff with Accept / Reject; cards show rewrite states](poc1/04-idea-inbox-pe-rewrite.png)

**New idea form with a PE suggestion and the three add buttons**

![New idea form with a PE suggestion and the three add buttons](poc1/05-idea-inbox-new-idea.png)

**Folder view of `unreviewed/` with the Places list**

![Folder view of `unreviewed/` with the Places list](poc1/06-idea-inbox-folder.png)

**Folder view of `inbox_drop/`: waiting files and `imported/`**

![Folder view of `inbox_drop/`: waiting files and `imported/`](poc1/07-idea-inbox-drop-folder.png)

**Import: watcher, waiting files, recent imports, settings**

![Import: watcher, waiting files, recent imports, settings](poc1/08-idea-inbox-import.png)

**Discarded ideas**

![Discarded ideas](poc1/09-idea-inbox-discarded.png)

**Files dragged from the desktop: drop highlight**

![Files dragged from the desktop: drop highlight](poc1/10-idea-inbox-desktop-drop.png)

#### Input library

**Grid with a multi-selection; filters and library tree**

![Grid with a multi-selection; filters and library tree](poc1/11-input-library-grid.png)

**Right-click menu on a kept idea**

![Right-click menu on a kept idea](poc1/12-input-library-card-menu.png)

**Dragging two selected cards onto a folder**

![Dragging two selected cards onto a folder](poc1/13-input-library-drag-to-folder.png)

**Folder view: ideas as folders**

![Folder view: ideas as folders](poc1/14-input-library-folder.png)

**Discarded (same view as the inbox's)**

![Discarded (same view as the inbox's)](poc1/15-input-library-discarded.png)

#### Projects

**Flat grid: covers, unreviewed pills, counts, paths**

![Flat grid: covers, unreviewed pills, counts, paths](poc1/16-projects-grid.png)

**Right-click menu on a folder in the project tree**

![Right-click menu on a folder in the project tree](poc1/17-projects-tree-menu.png)

**Dragging a project onto a folder**

![Dragging a project onto a folder](poc1/18-projects-tree-drag.png)

**Delete confirmation with contents and pending seeds**

![Delete confirmation with contents and pending seeds](poc1/19-projects-delete-confirm.png)

**Folder view, top level**

![Folder view, top level](poc1/20-projects-folder.png)

**Inside a project: `runs/`, `unreviewed/`, `reviewed/`**

![Inside a project: `runs/`, `unreviewed/`, `reviewed/`](poc1/21-projects-folder-project.png)

**Details mode of `unreviewed/`**

![Details mode of `unreviewed/`](poc1/22-projects-folder-details.png)

**Generate panel (Transparent on, prompt sent shown)**

![Generate panel (Transparent on, prompt sent shown)](poc1/23-projects-generate.png)

**Generate's PE rewrite window**

![Generate's PE rewrite window](poc1/24-projects-generate-pe-rewrite.png)

**Resolutions editor**

![Resolutions editor](poc1/25-projects-resolutions.png)

**Jobs: server cards and runs table**

![Jobs: server cards and runs table](poc1/26-projects-jobs.png)

#### Review

**Single view with scope panel, buttons and recipe**

![Single view with scope panel, buttons and recipe](poc1/27-review-single.png)

**Grid view**

![Grid view](poc1/28-review-grid.png)

**Right-click menu, including Add to collection**

![Right-click menu, including Add to collection](poc1/29-review-grid-menu.png)

**A/B compare: A pinned, B next; A's own verdict buttons and swap**

![A/B compare: A pinned, B next; A's own verdict buttons and swap](poc1/30-review-ab-compare.png)

#### Output library

**All kept images; lineage panel for the selected image**

![All kept images; lineage panel for the selected image](poc1/31-output-library-grid.png)

**Right-click menu: collections, cover, export, rating, hand-offs**

![Right-click menu: collections, cover, export, rating, hand-offs](poc1/32-output-library-menu.png)

**Grid of one collection with Export these N…**

![Grid of one collection with Export these N…](poc1/33-output-library-collection.png)

**Dragging a thumbnail onto a collection**

![Dragging a thumbnail onto a collection](poc1/34-output-library-drag-to-collection.png)

**Folder view: collections and the `smart/` folder**

![Folder view: collections and the `smart/` folder](poc1/35-output-library-folder.png)

**Smart folder "by rating / good"**

![Smart folder "by rating / good"](poc1/36-output-library-smart-folder.png)

**Rejected images**

![Rejected images](poc1/37-output-library-rejected.png)

**Export dialog**

![Export dialog](poc1/38-output-library-export.png)

## 5. Design decisions

Each entry: what was decided, what else was considered, and why. Most are
mirrored in DESIGN.md (section in brackets).

### 5.1 Overall structure

- **Five top-level tabs in pipeline order**: Idea inbox · Input library ·
  Projects · Review · Output library. The two review queues (Idea inbox,
  Review) are both top-level and symmetric. *[GUI]*
- **Review became its own top-level tab** (it started as a Projects sub-tab).
  Reasons: symmetry with the Idea inbox; it is the busiest screen during
  batches; it is cross-project by nature; Projects is then about making
  images. Cost: switching top-level tabs between Generate and Review —
  mitigated by `E` (edit run → Generate) and double-click in Projects → Grid
  (→ Review). *[GUI]*
- **Review has its own scope**, not the Projects tab's selection, so browsing
  projects never changes the review queue. *[GUI]*
- **Jobs stays a Projects sub-tab**; the server status in the top bar is
  clickable instead of giving Jobs a top-level tab (a glance-at screen).
- **Counts in tab labels** for every top-level tab (unreviewed for the two
  queues, kept items for the libraries, project count for Projects).
- **Consistent sub-tab names**: Grid · Folder in all three tree-based screens;
  Discarded (ideas) and Rejected (outputs) for the "bad" pile.
- **Left panels run the full window height**; sub-tab bars sit beside them,
  over the content area only.

### 5.2 Idea inbox

- **Renamed "Inbox" → "Idea inbox"**, to pair with the output side.
- **Grid only**: the original single-idea view was removed. Editing moved to
  an **Edit idea** window (Enter / T / double-click), so nothing was lost.
- **Right-click menus on every card** (here and in the libraries) mirror the
  keys.
- **`Space` promotes** (same as `P`) — "projectize the idea". In the input
  library, `Space` promotes the selection.
- **Tabs New idea · Grid · Folder · Import · Discarded**. New idea is leftmost
  and is the default only when there are no ideas to review.
- **New idea form**: three equal buttons (Add to inbox / Add & keep / Add &
  promote). The question "which is the main one?" was left open.
- **Import tab** makes the drop-folder pipeline visible: what is waiting, what
  was converted on import, duplicates, and the two open import questions as
  settings.
- **Discarded in both the Idea inbox and the Input library** (same view).
- **Folder tab, inbox-only**: rooted at `inbox_drop/`, `unreviewed/`,
  `discarded/` — not the whole `input/` tree. Its location/history are its
  own. No subfolders in `unreviewed/` (the inbox is a queue; filing happens
  in the library). Moving an idea folder between `unreviewed/` and
  `discarded/` is the review.
- **Dragging files from the desktop imports them**, on any inbox tab.
- **PE rewrites are inline, not a tab.** A "Rewrites" tab was built and then
  removed: a whole tab for an occasional action separated results from their
  idea. Now the card shows the state, the Edit idea window shows the diff with
  Accept / Reject, menus offer "Accept all ready". Rewriting stays
  asynchronous (the PE server may run on CPU) and never blocks; results are
  only added as variants when accepted, the original is always kept.
  *[Prompt rewriting]*

### 5.3 Folder trees

- **Projects, the input library and output collections are folder trees.**
  Project and library trees are real directories on disk; collections are
  directories of links. *[Project folders, Input library folders, Image stores]*
- **Identity is never the path**: projects, ideas and runs are referred to by
  ID, so renaming or moving folders breaks nothing. *[Project folders]*
- **Tree operations**: new folder / project / idea, inline rename (Enter or
  click away commits, Escape cancels; invalid or clashing names refused),
  copy / cut / paste, drag and drop (blue outline, red when refused), delete
  (asks; to the system trash). *[Project folders]*
- **Copy semantics**: copying a project copies its setup (sources, run
  definitions) but not its images; copying an idea duplicates it with shared
  images; copying a collection link adds the image to another collection.
- **Folder names** must be unique among siblings (case-insensitive) because
  they are directory names; idea titles may repeat (idea folders are IDs).
- **Shortcuts `F2`, `Del`, `Ctrl+C/X/V`** were checked against Review's keys:
  no clash, because egui reports `Ctrl+C/X/V` as clipboard events. Copy/Cut
  also put the item's path on the system clipboard, because egui only reports
  `Ctrl+V` when the clipboard is non-empty.

### 5.4 Folder views (Thunar-like)

- **Purpose**: a file-manager view of the on-disk layout, because the design
  keeps review state *as* folder location — moving a file from
  `unreviewed/` to `reviewed/good/` is a review.
- **Plain folder icons** for directories (no cover thumbnails); only image
  files show thumbnails. Covers belong to the Grid tabs.
- **Projects only** as a root for Projects → Folder (not the whole library);
  the same component is **duplicated for the other screens** with their own
  roots and state: Input library, Idea inbox (inbox-only), Output library
  (collections + smart folders).
- **Metadata files hidden by default** (`Ctrl+H`), like hidden files.
- **Rules for image files** in a project: move between review folders of the
  same project (= review, undoable); no rename (the name carries run,
  revision, seed); no copy; delete removes the link only; moving into another
  project is refused (use → input ref).
- **Double-click an image**: unreviewed → Review single view; reviewed →
  Output library with lineage.
- **"Open in system file manager"** is offered (mocked in POC1).
- **Dual pane (`F3`)**: deferred.

### 5.5 Projects screens

- **Projects → Grid is flat**: every project under the selected folder as a
  tile, no folder tiles — the Folder tab does the navigating.
- **Cover** = chosen cover ("Use as project cover") else best-rated, most
  recent. **Unreviewed count** as a pill on the cover.
- **Resolution list** is user-editable, shared by base and Turbo, saved in
  `config.ron`, with a one-off custom size and "Save to list". Sizes snap to
  multiples of 32 (sd.cpp rounds output up to 32 and rejects unaligned refs),
  which turned the original 1328² preset into 1344². The time estimate is a
  rough scale of one measured value (34 s at 1024², 20 steps). *[Config file]*
- **Seeds start at 0 for every run** by default and count up, so runs line up
  seed for seed for A/B comparison; "Continue" offers fresh seeds; adding seeds
  to a run continues after its highest seed. *[Seeds]*

### 5.6 Review

- **Mouse parity with the keys**: verdict / navigation / hand-off buttons and
  right-click menus everywhere.
- **A/B compare in one step**: pin current as A, show next as B; pinning
  always advances; A can be rated and swapped. (The first version required
  pin → move → compare and otherwise showed the same image twice.)
- **New images join the end** of the oldest-first queue; a "newest" button and
  arrival toasts make them findable.
- **`R` / `I`** create real input-library entries (an image-only reference
  idea, or an inbox idea with the run's prompt).

### 5.7 Output library

- **It is for finished work; Projects is for making and triaging.** Its two
  views were differentiated accordingly:
  - **Collections**: user-made folders of kept images, independent of
    projects; one image can be in several. Links on disk under
    `collections/`.
  - **Smart folders**: virtual and read-only — by date, rating, source idea,
    project, transparency.
- **No project tree in the Output library.** It was tried (first linked to
  Projects → Folder, then a library-shaped project view), then removed:
  browsing by project is the Projects tab's job and the "by project" smart
  folder covers kept images per project.
- **Sources** (All kept images / Collections / Smart folders) drive both Grid
  and Folder.
- **Rejected** replaces a "bad" score filter, like Discarded on the input side.
- **Export…** everywhere images are shown; **Use as project cover**.
- **Real filters** for has refs / has alpha (they started as placeholder
  text).

### 5.8 Storage, identity and files

- **`config.ron`** in `$XDG_CONFIG_HOME/increart/` (the request said
  `config.rsn`; RON was assumed, matching the spec). *[Config file]*
  Since POC1 the spec has switched every file to **RSN** (`config.rsn`,
  `idea.rsn`, …; see File format: RSN). The prototype and its screenshots
  still show `.ron` names.
- **Two content stores**, `input_images/` and `output_images/`, every image a
  PNG, never moved; idea and project folders hold **relative links** into them.
  This also made every image reference stable. *[Image stores, Links]*
- **Link kind per library** (`library.ron`): symlinks (Linux/macOS), else hard
  links (Windows without Developer Mode; NTFS, one volume), else copies
  (FAT/exFAT). Each store PNG carries `increart:id` so hard links and copies
  can be identified. *[Links]*
- **Inputs are re-encoded as PNG** on import (lossy decoders differ across
  versions), with the original's details in an `increart:source` JSON chunk
  and EXIF kept in `eXIf`; **outputs are PNG only** (`output_format: "png"`),
  chunks added without re-encoding pixels. *[Image stores]*
- **Pixel hash** (`increart-pixels-v1` over the decoded pixels, not file
  bytes) is the content fingerprint for duplicates and verification, so
  metadata edits don't change it. *[Pixel hash]*
- **IDs `YYYY.MM.DD-HH.MM-NNNN`**, UTC, counter from 0000 per kind within the
  minute, for ideas, projects, runs and images (replacing ULIDs and
  hash-named files). Allocation by the single writer holding `library.lock`,
  plus exclusive create for store files. Known wart: the same string can be an
  idea, a run and an image (separate counters were chosen). *[IDs]*
- **Output link names `<run-id>_r<rev>_s<seed>.png`**, seed zero-padded to at
  least 4 digits, so they don't read like image IDs. *[Filesystem layout]*
- **Inbox has no image storage of its own**; drop-folder files move to
  `inbox_drop/imported/` after import. *[Ideas]*
- **Clean up library** is a manual command that trashes unreferenced store
  images; deleting projects/ideas/collections only removes links.

### 5.9 Visual language

- **Phosphor icons** via the `egui-phosphor` crate. It targets egui 0.36 while
  POC1 uses 0.37, so only its font bytes and icon constants are used and the
  font is registered directly (cost: an unused egui 0.36 is compiled).
- Icons that the default egui fonts lack (`🗀`, `⧉`) rendered as boxes, which
  prompted the switch to Phosphor.
- egui's dark theme; colours carry meaning consistently: green good, grey
  neutral, red bad, blue selection/links, purple PE/ideas, amber warnings.

### 5.10 Generation and scheduling

- **Round-robin across unfinished runs** instead of strict oldest-run-first,
  so a newly queued run starts producing images immediately. At real speed
  (~34 s/seed) a 256-seed batch would otherwise block new runs for over an
  hour. Affinity (conditioning cache reuse) only breaks ties. *[Scheduling]*

## 6. Testing

### 6.1 Headless runs (`scripts/headless.sh`)

Rule: any GUI run that needs no human input runs on a private display, never
the user's desktop (earlier screenshot runs opened windows and injected
pointer input on the desktop).

- `scripts/headless.sh <command>` starts Xvfb on display `:93`, waits for its
  socket, runs the command with `DISPLAY=:93`, and stops Xvfb on exit.
- Xvfb listens only on the abstract socket (`-nolisten unix`), so nothing is
  written to the host's shared `/tmp/.X11-unix`.
- The dev container has no root (all capabilities dropped) and no Xvfb. The
  script uses the system Xvfb if present; otherwise it fetches Debian's xvfb
  packages, unpacks them to `tmp/xvfb-root`, and starts Xvfb inside a user +
  mount namespace with the unpacked binaries overlaid on `/usr/bin` (Xvfb
  runs `/usr/bin/xkbcomp` by absolute path).
- `.devcontainer/Dockerfile` is read-only from inside the container; adding
  `xvfb` to its apt list makes the fallback unnecessary.
- Rendering runs on Mesa's software GL; the "No DRI3 support" messages from
  Vulkan are harmless.

### 6.2 Screenshot walkthrough with synthetic input

`INCREART_SHOTS=<dir>` runs 43 scenes of 20 frames each: phase 0 sets the
scene up, phase 10 takes the screenshot, and `raw_input_hook` injects input
in the phases between — pointer moves and button presses (drags), key
presses (with modifiers), clipboard events (`Cut`, `Paste`), hovered and
dropped files (a small `TestDrop` type serves real files). Row, card and
button rectangles from the previous frame are recorded so the input can aim
at real widgets. Scenes run in sequence on shared state, so later screenshots
show leftovers of earlier scenes (e.g. a rename field left open).

| Scenes | What they show / exercise |
| --- | --- |
| 1, 19, 24–29, 36–39 | Idea inbox: grid, `Space` promote, New idea with PE suggestion, Import, Discarded, rewrite state + Edit window diff, `Ctrl+N`, empty-inbox default tab, inbox Folder, keyboard discard, desktop file drop |
| 3, 15, 16, 28, 34 | Input library: grid, card drag into a folder, folder `F2`, Discarded, Folder view |
| 4, 5, 11 | Generate panel, PE rewrite window, Resolutions editor |
| 6, 17 | Jobs; clicking **Queue run** then fast-forwarding the mock backend |
| 12, 13, 14 | Delete confirmation, tree drag-and-drop, tree `Ctrl+X` / `Ctrl+V` / `F2` |
| 22, 23, 30–33 | Projects Grid (top level, folder), Folder view root / project / Details, keyboard review by cut + paste |
| 7, 8, 9, 18, 21 | Review single / grid / compare, key `1`, one-step compare |
| 10, 20, 35, 40–44 | Output library grid, output menu, Rejected, collections folder, smart folder, collection grid, drag thumbnail onto a collection, Export dialog |

### 6.3 Behaviour checks

For behaviour (not just looks), scenes temporarily printed state before and
after the injected input; the prints were removed once the check passed.
Results recorded during POC1:

| Check | Result |
| --- | --- |
| Drag a project onto a folder | moved into the target folder |
| Tree `Ctrl+X`, select folder, `Ctrl+V`, `F2` | moved; rename field opened |
| Drag two selected library cards onto a folder | both moved |
| Queue run (real button click) + 60 s mock time | seeds 0–3 created; review queue 7 → 11 |
| Key `1` in Review | queue 11 → 10; good count +1 |
| Space on an inbox idea | new project created, Generate opened |
| Mock backend while idle (no input, 10 s) | 13.1 seeds ≈ the expected 1.33/s |
| Round-robin over 3 s with 3 unfinished runs | each run +1.33 seeds, new run started at once |
| Compare via keys (`C`, `2`) | A pinned, B advanced, B rated |
| `Ctrl+N` from Projects | Idea inbox → New idea |
| Empty inbox, then open Idea inbox | lands on New idea (startup with 6 ideas lands on Grid) |
| Folder view: cut image, paste into `reviewed/good` | good 2 → 3, review queue −1 |
| Inbox Folder: cut idea, paste into `discarded/` | inbox 6 → 5, discarded 1 → 2 |
| Desktop drop of a PNG and a `.txt` | 2 inbox ideas; PNG decoded for real; text from the file |
| Output Folder (when project-shaped) at a project | only `good`, `neutral`; Projects view unaffected |
| Drag a grid thumbnail onto "Client picks" | collection 2 → 3 images |

### 6.4 Bugs found by testing

- **Generation stalled while idle**: egui's `stable_dt` is a prediction
  (~1/60 s) after an idle sleep, so the mock backend ran ~6× slow unless the
  mouse moved. Fixed by measuring real elapsed time; the per-frame cap was
  removed too (time spent hidden still counts).
- **"New runs never reach Review"**: strict oldest-first scheduling put new
  runs behind ~30 older seeds → round-robin (5.10).
- **`Ctrl+N` ignored synthetic input**: the check read egui's global modifier
  state; `consume_key` matches the modifier on the key event.
- **A/B compare showed the same image twice** → one-step compare (5.6).
- **Lineage panel showed a stale image** outside the current scope/tab →
  shown only for visible images.
- **Overlay/menus**: a single-view right-click menu would have triggered
  "next" on open; a grid background hit area would have swallowed tile
  clicks; both caught in review before running.
- **Sample generator**: `hash()` of strings is randomised per Python process
  (non-deterministic output) → seeds from sha256; a `FromOutput` reference
  took single characters of a tuple (path `"d"`), caught by the verifier.

### 6.5 On-disk sample library

(Both scripts have since been ported to Rust, as test support in
`crates/increart-core/tests/common/`: `sample.rs` and `verify.rs`, run by
hand with `cargo run -p increart-core --example sample_library`.)

`scripts/make-sample-library.py` writes `tmp/sample-library/`: the full
library layout with the same content as the mock data — `library.rsn`,
`config/config.rsn`, both image stores (procedural 1024² PNGs with
`increart:id`, `increart:pixels`, `increart:recipe` / `increart:source`,
sd-server-style `parameters`), idea / project / run folders with relative
symlinks, `inbox_drop/` with `imported/`, empty caches. Deterministic
(fixed RNG seed), standard library only, ~30 s.

`scripts/verify-sample-library.py` checks it independently (its own PNG
decoder, all five row filters): store names = `increart:id`,
`increart:pixels` = the real pixel hash, required chunks present, every link
relative and pointing into a store, every `ImageRef` and idea image
resolving, IDs well formed and unique. Last result: 19 idea/project/run IDs,
34 store images, 34 links, 22 references, no problems. A regenerate-and-diff
check confirmed byte-identical output.

### 6.6 Not tested

- Most right-click menu items and buttons were checked visually, not clicked
  (the tests above cover the main flows).
- A real drag from the desktop (only synthetic drops).
- Anything against a real sd-server or PE server; anything persisting to
  disk.
- Windows (links, paths) — reasoned about, not run.

## 7. Known limitations and mock shortcuts

- Nothing is saved; restarting resets everything.
- Images are procedural textures (plus one real render); dropped JPEG/WebP
  files get a mock picture (only PNG is decoded).
- Export, "open in system file manager" and "clear imported originals" only
  report what they would do.
- Run names in link names are the mock's short labels (`r01_s0003`), not run
  IDs.
- `F2` on a folder/project in a folder view renames in the side tree; on an
  idea it opens the Edit idea window.
- Deleted projects and permanently deleted ideas stay in memory (flagged), so
  indices stay valid; the real app works by ID.
- Undo covers review, discard and restore actions; tree deletes and moves are
  not undoable.
- The single-file structure and the `fv_inbox` / `coll_view` context flags are
  prototype shortcuts, not a design for the real crates.

## 8. Open questions and next steps

Open questions (the EXIF, `imported/` and Settings ones are also in
DESIGN.md → Open questions; the rest came up during POC1):

- New idea: which add button is the primary one?
- Input library: the "promoted: any ▾" filter is still placeholder text.
- EXIF / GPS on import: strip, keep, or a setting.
- `inbox_drop/imported/`: auto-delete after N days, or leave it to the user.
- A Settings screen (resolutions are only reachable from Generate).
- Output library section headers vs. "All …" rows (merging them was discussed;
  partly moot now that the project tree is gone).
- Separate per-kind ID counters produce identical strings across kinds; a
  shared counter would make every ID unique in the library.

Next steps:

1. Move `tmp/gui-prototype/` (and the screenshots) to a tracked location if
   POC1 should be kept for reference.
2. Add `xvfb` to `.devcontainer/Dockerfile`.
3. Start the real crates: `increart-core` (library on disk: stores, links,
   IDs, pixel hash, RSN/JSON chunks — the sample library and verifier are a
   ready test fixture), then the `increart-gui-core` reducer with headless
   tests for the flows listed in 6.3, then the egui view.
4. Connect to sd-server (job lifecycle, round-robin scheduler, model
   switching) and a PE server.

## 9. File map

| Path | What |
| --- | --- |
| `docs/DESIGN.md` | the spec, updated with every POC1 decision |
| `docs/POC1.md` | this document |
| `docs/poc1/` | screenshots of every screen and feature, with an index (`README.md`) |
| `tmp/gui-prototype/src/main.rs` | the prototype (git-ignored location) |
| `tmp/gui-prototype/Cargo.toml` | its dependencies |
| `scripts/headless.sh` | run any GUI command on a private Xvfb display |
| `crates/increart-core/tests/common/sample.rs` | write the on-disk sample library (was `scripts/make-sample-library.py`) |
| `crates/increart-core/tests/common/verify.rs` | independently verify a sample library (was `scripts/verify-sample-library.py`) |
| `tmp/sample-library/` | the generated sample (git-ignored) |
| `tmp/xvfb-root/` | unpacked Xvfb fallback (git-ignored) |
