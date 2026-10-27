# POC1 screenshots

One screenshot per screen and feature of the POC1 GUI prototype (see
[../POC1.md](../POC1.md)). All are 1400 × 900, mock data, dark theme.

They are produced by the prototype's documentation tour, on a private Xvfb
display:

```sh
scripts/headless.sh env INCREART_TOUR=docs/poc1 \
    tmp/gui-prototype/target/release/increart-prototype
```

Each scene starts from a reset UI state; menus, drags and the drop highlight
are produced by injected input (drags are held mid-way and released over the
status bar, so nothing is actually moved). The mock backend keeps generating
while the tour runs, so counts differ slightly between shots.

## Idea inbox

| | |
| --- | --- |
| [01-idea-inbox-grid](01-idea-inbox-grid.png) | Grid: idea cards, keys panel |
| [02-idea-inbox-card-menu](02-idea-inbox-card-menu.png) | Right-click menu on an idea |
| [03-idea-inbox-edit-idea](03-idea-inbox-edit-idea.png) | Edit idea window |
| [04-idea-inbox-pe-rewrite](04-idea-inbox-pe-rewrite.png) | Finished PE rewrite: diff with Accept / Reject; cards show rewrite states |
| [05-idea-inbox-new-idea](05-idea-inbox-new-idea.png) | New idea form with a PE suggestion and the three add buttons |
| [06-idea-inbox-folder](06-idea-inbox-folder.png) | Folder view of `unreviewed/`, Places list |
| [07-idea-inbox-drop-folder](07-idea-inbox-drop-folder.png) | Folder view of `inbox_drop/`: waiting files and `imported/` |
| [08-idea-inbox-import](08-idea-inbox-import.png) | Import: watcher, waiting files, recent imports, settings |
| [09-idea-inbox-discarded](09-idea-inbox-discarded.png) | Discarded ideas |
| [10-idea-inbox-desktop-drop](10-idea-inbox-desktop-drop.png) | Files dragged from the desktop: drop highlight |

## Input library

| | |
| --- | --- |
| [11-input-library-grid](11-input-library-grid.png) | Grid with a multi-selection; filters and library tree |
| [12-input-library-card-menu](12-input-library-card-menu.png) | Right-click menu on a kept idea |
| [13-input-library-drag-to-folder](13-input-library-drag-to-folder.png) | Dragging two selected cards onto a folder |
| [14-input-library-folder](14-input-library-folder.png) | Folder view: ideas as folders |
| [15-input-library-discarded](15-input-library-discarded.png) | Discarded (same view as the inbox's) |

## Projects

| | |
| --- | --- |
| [16-projects-grid](16-projects-grid.png) | Flat grid: covers, unreviewed pills, counts, paths |
| [17-projects-tree-menu](17-projects-tree-menu.png) | Right-click menu on a folder in the project tree |
| [18-projects-tree-drag](18-projects-tree-drag.png) | Dragging a project onto a folder |
| [19-projects-delete-confirm](19-projects-delete-confirm.png) | Delete confirmation with contents and pending seeds |
| [20-projects-folder](20-projects-folder.png) | Folder view, top level |
| [21-projects-folder-project](21-projects-folder-project.png) | Inside a project: `runs/`, `unreviewed/`, `reviewed/` |
| [22-projects-folder-details](22-projects-folder-details.png) | Details mode of `unreviewed/` |
| [23-projects-generate](23-projects-generate.png) | Generate panel (Transparent on, prompt sent shown) |
| [24-projects-generate-pe-rewrite](24-projects-generate-pe-rewrite.png) | Generate's PE rewrite window |
| [25-projects-resolutions](25-projects-resolutions.png) | Resolutions editor |
| [26-projects-jobs](26-projects-jobs.png) | Jobs: server cards and runs table |

## Review

| | |
| --- | --- |
| [27-review-single](27-review-single.png) | Single view with scope panel, buttons and recipe |
| [28-review-grid](28-review-grid.png) | Grid view |
| [29-review-grid-menu](29-review-grid-menu.png) | Right-click menu, including Add to collection |
| [30-review-ab-compare](30-review-ab-compare.png) | A/B compare: A pinned, B next; A's own verdict buttons and swap |

## Output library

| | |
| --- | --- |
| [31-output-library-grid](31-output-library-grid.png) | All kept images; lineage panel for an edited image |
| [32-output-library-menu](32-output-library-menu.png) | Right-click menu: collections, cover, export, rating, hand-offs |
| [33-output-library-collection](33-output-library-collection.png) | Grid of one collection with Export these N… |
| [34-output-library-drag-to-collection](34-output-library-drag-to-collection.png) | Dragging a thumbnail onto a collection |
| [35-output-library-folder](35-output-library-folder.png) | Folder view: collections and the `smart/` folder |
| [36-output-library-smart-folder](36-output-library-smart-folder.png) | Smart folder "by rating / good" |
| [37-output-library-rejected](37-output-library-rejected.png) | Rejected images |
| [38-output-library-export](38-output-library-export.png) | Export dialog |
