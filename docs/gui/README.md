# IncreArt GUI screenshots

The real app (`crates/increart-gui`, binary `increart`) on a copy of the
sample library with no servers connected. Taken by the app's screenshot tour,
which drives the screens through the gui-core reducer, on a private Xvfb
display:

```sh
scripts/headless.sh env INCREART_CONFIG=$PWD/tmp/gui-tour/config.rsn \
    INCREART_TOUR=$PWD/docs/gui INCREART_TOUR_LIBRARY=$PWD/tmp/gui-tour/library \
    target/release/increart
```

| | |
| --- | --- |
| [01-library-dialog](01-library-dialog.png) | Start-up: recent libraries (last used in the accent colour), Create a new library… / Open an existing library… |
| [02-server-dialog](02-server-dialog.png) | Start-up: server presets (a click opens one; ✎ edit, 🗑 delete), Open the last used, sd-server settings… |
| [03-idea-inbox-grid](03-idea-inbox-grid.png) | Idea inbox grid with keys panel |
| [04-idea-inbox-new-idea](04-idea-inbox-new-idea.png) | New idea form |
| [05-input-library](05-input-library.png) | Input library: folder tree, selection actions, cards |
| [06-projects-grid](06-projects-grid.png) | Projects: tree and project tiles (cover, counts) |
| [07-projects-generate](07-projects-generate.png) | Generate: the form, and the Images pane on the right (each image a `<imageN>` reference, the start image, both, or parked) |
| [08-projects-jobs](08-projects-jobs.png) | Jobs: servers and running runs |
| [09-review](09-review.png) | Review: scope, image, recipe, verdicts, filmstrip |
| [10-output-library](10-output-library.png) | Output library: all projects, Kept tab (the Grid's preset tabs: Kept · Good · Rejected · Unreviewed · All) |
| [11-jobs-popover](11-jobs-popover.png) | Status bar jobs indicator and its popover (engine state simulated by the tour) |
| [12-review-grid](12-review-grid.png) | Review → Grid |
| [13-review-compare](13-review-compare.png) | Review → A/B compare: A pinned with its own buttons, B next |
| [14-output-library-rejected](14-output-library-rejected.png) | Output library → Grid → Rejected tab: the source's bad images, with Delete these N rejected… |
| [15-generate-img2img](15-generate-img2img.png) | Generate in a project set to img2img: start image with strength, refs counter |
| [16-servers-opening](16-servers-opening.png) | Server dialog while opening: locked and dimmed, progress, Continue in background / Cancel |
| [17-prompt-history](17-prompt-history.png) | Generate history: entries newest first with the settings changed and the runs they led to, word diff and settings against the current form, Restore all / prompt only / Copy |
| [18-edit-idea](18-edit-idea.png) | Edit idea window (title, tags, text with history, images, variants) |
| [19-idea-inbox-import](19-idea-inbox-import.png) | Idea inbox → Import: waiting files, Import now, recent imports with conversions |
| [20-input-library-filters](20-input-library-filters.png) | Input library: filters (starred only), Grid · Folder · Discarded |
| [21-resolutions](21-resolutions.png) | Resolutions editor (aspect, MP, time estimate, row actions) |
| [22-output-library-lineage](22-output-library-lineage.png) | Output library: sources (collections, smart folders), filters, lineage and recipe of an edited image |
| [23-export](23-export.png) | Export dialog |
| [24-projects-grid-tile-actions](24-projects-grid-tile-actions.png) | Projects grid: the selected tile shows Review / Generate (scene 25 clicks Generate and checks the result) |
| [26-projects-folder](26-projects-folder.png) | Projects → Folder: folder view of `projects/` (two-tone icons; folders show images from inside them, projects their cover with a rocket badge; back / forward / up, path bar, Icons · Details, metadata toggle, refresh, open in file manager) |
| [27-projects-folder-images](27-projects-folder-images.png) | Folder view inside a project's `unreviewed/`: image links with thumbnails (double-click opens Review) |
| [28-projects-folder-details](28-projects-folder-details.png) | Folder view, Details mode: name, kind, info, size, modified |
| [29-projects-folder-project](29-projects-folder-project.png) | Folder view inside a project: review folders and `runs/` with their images behind the icons |
| [30-projects-folder-runs](30-projects-folder-runs.png) | Folder view of `runs/`: each run shows its own outputs behind a film-strip icon |
| [38-scale-dialog](38-scale-dialog.png) | Scale dialog (Generate → Scale…, or a row of the resolutions editor): to megapixels, a width or a height, keeping the aspect; multiples of 32 |
| [39-output-library-folder](39-output-library-folder.png) | Output library → Folder: collections, with the virtual read-only `smart/` folder first |
| [40-output-library-smart-folder](40-output-library-smart-folder.png) | Output library → Folder: `smart › by project` (copy images from here into a collection) |
| [42-input-library-folder](42-input-library-folder.png) | Input library → Folder: folders and ideas (purple, titled; their images behind) |
| [43-idea-inbox-folder](43-idea-inbox-folder.png) | Idea inbox → Folder: `input/unreviewed/`, one folder per idea |
| [44-projects-folder-compact](44-projects-folder-compact.png) | Folder view, Compact mode: small thumbnails / icons and names in columns |
| [45-reference-preview](45-reference-preview.png) | Generate: double-click a reference (or the start image) to view it large |
| [47-new-project-name](47-new-project-name.png) | Review → D / N: the new project's name is asked first |
| [49-output-library-show-outputs](49-output-library-show-outputs.png) | Show outputs on a project with a filter on: the banner says how many it hides, with Reset filters |
| [52-server-dialog-edit](52-server-dialog-edit.png) | The preset editor modal: name, server rows, the listed devices to add, Save / Save and open / Open without saving / Cancel |
| [53-library-wizard](53-library-wizard.png) | Create a new library: home folder, current folder or elsewhere |
| [55-library-open-wizard](55-library-open-wizard.png) | Open an existing library: the folder, checked for library.rsn |
| [54-server-wizard](54-server-wizard.png) | Server setup wizard (no presets): sd-server, the listed GPUs ticked, or connect to a running server |
| [56-server-status](56-server-status.png) | Top bar Servers…: the server status window (here without servers) |
| [57-no-server-warning](57-no-server-warning.png) | First Queue of a session with no server: Set up servers… / Queue anyway / Cancel |
| [58-server-dialog-from-main](58-server-dialog-from-main.png) | The server dialog reopened from the main window: Close goes back |
| [59-continue-without-servers](59-continue-without-servers.png) | Continue without servers asks first |
| [60-server-settings](60-server-settings.png) | sd-server settings modal: binary, models, leave running, devices |
| [61-duplicate-project](61-duplicate-project.png) | A project's right-click Duplicate: name the copy first ("stickers copy") |
| [63-projects-search](63-projects-search.png) | Projects side panel: New project / New folder on top, search ("fox": matching projects and the folders holding them) |
| [62-rename](62-rename.png) | Rename modal (folders, projects, collections; also right after New folder / New project) |
| [64-prompt-presets](64-prompt-presets.png) | Generate → Presets…: tasks on the left, Modify style's presets under their group headings; the prompt replaced by Solid colour background, its `{colour}` selected and flagged; the library menu at the top right |
| [65-prompt-presets-search](65-prompt-presets-search.png) | Presets search ("clay") across every task |
| [66-preset-editor](66-preset-editor.png) | Edit presets…: tasks, the task's presets, the selected preset's fields (Claymation) |
| [67-save-preset](67-save-preset.png) | Save as preset…: the current prompt as a new preset |
| [68-switch-library](68-switch-library.png) | Switching libraries from the top bar: the library screen with Back to the open one |
| [69-style-sweep](69-style-sweep.png) | Generate in Style sweep mode: the template with `{preset}`, 8 random styles from Modify style, the summary and the chosen names |
| [70-contact-sheet](70-contact-sheet.png) | Review → a sweep's Contact sheet: a row per style with its counts and row buttons, a column per seed, `…` for an image still to come |
| [71-job-info](71-job-info.png) | Projects → Jobs, a run clicked: its details with progress, prompt, the input images labelled `<image1>` / start image, and the settings |
| [72-review-sweep-button](72-review-sweep-button.png) | Review → Single on a sweep's image: **Sweep r02 contact sheet** in the top bar |
| [73-promote](73-promote.png) | Promote into project: the new project's name, always asked (the first idea title as the guess) |
