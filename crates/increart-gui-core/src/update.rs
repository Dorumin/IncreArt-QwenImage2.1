//! The reducer: `update(state, msg) -> effects`. Pure: everything that
//! touches disk, servers or the engine is an [`Effect`] the runtime carries
//! out, reporting back with more [`Msg`]s.

use std::path::PathBuf;

use increart_core::config::{Config, RecipePreset, SdServerConfig, ServerConfig, ServerPreset};
use increart_core::engine::{Event, Stop};
use increart_core::launcher::device_name;
use increart_core::scheduler::Order;
use increart_core::{Id, IdeaLoc, ImageRef, Index, NewIdea, Tree, Verdict};

use increart_core::HistoryKey;
use increart_core::config::Resolution;
use increart_core::history::{HistoryEntry, HistoryKind};
use increart_core::model::{Recipe, SourceInfo};
use increart_core::model::{SweepParam, SweepTag};
use increart_core::prompts::{PromptPreset, PromptPresets};

use crate::state::*;
use crate::tile::{Tile, TileContent};

// A few messages carry a whole library index; they are rare, so the size
// doesn't matter.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    // ---- start-up: library
    SetLibraryPath(String),
    OpenLibrary,
    CreateLibrary,
    /// Opens `config.recent_libraries[i]`.
    OpenRecentLibrary(usize),
    /// Drops `config.recent_libraries[i]` from the list (the folder stays).
    ForgetLibrary(usize),
    /// Opens a library wizard on a folder (`None` closes it).
    LibraryWizard(Option<(LibraryWizard, String)>),
    LibraryOpened(Result<(PathBuf, Index), String>),
    /// The background check of the images' usage counts after opening:
    /// images done, of how many (done = total: finished).
    UsageCheck(usize, usize),
    // ---- switching libraries (the top bar's library menu)
    /// Switch to this library (asking first while runs are generating).
    SwitchLibrary(String),
    SwitchLibraryNow(String),
    /// The library screen, to pick another library; with a wizard open.
    OtherLibrary(Option<(LibraryWizard, String)>),
    OtherLibraryNow(Option<(LibraryWizard, String)>),
    /// Back to the open library from the library screen.
    CancelSwitch,
    // ---- start-up: servers
    /// Opens a preset (asking first when servers are already in use).
    OpenPreset(usize),
    /// Opens a preset without asking.
    OpenPresetNow(usize),
    /// Opens the preset in the editor.
    EditPreset(usize),
    /// Opens the editor on an empty preset.
    NewPreset,
    CancelEdit,
    AddStartServer,
    AddConnectServer,
    EditServer(usize, ServerConfig),
    RemoveServer(usize),
    SetPresetName(String),
    /// Saves the editor's preset (in place, renaming it if need be) and
    /// closes the editor.
    SavePreset,
    /// Save, then open it.
    SaveAndOpen,
    DeletePreset(usize),
    ListDevices,
    Devices(Result<Vec<String>, String>),
    /// Opens (lists the devices) or closes the server setup wizard.
    ServerWizard(bool),
    /// Wizard and editor: start a server on this device, or not.
    DraftDevice(String, bool),
    /// Wizard: save the draft as a preset (named after its devices) and open it.
    FinishServerWizard,
    /// Opens the editor's servers without saving them (asking first when
    /// servers are already in use).
    OpenDraft,
    /// Opens the draft's servers without asking.
    OpenDraftNow,
    /// (attempt, server, result)
    ServerOpened(u32, usize, Result<(), String>),
    /// Every server of the draft has answered or failed.
    ServersSettled(u32),
    /// The servers [`Effect::StopServers`] stopped are gone (the names of
    /// those the app had started; connected ones are only let go).
    ServersStopped(Vec<String>),
    /// A starting server's latest log line (attempt, server).
    ServerLog(u32, usize, String),
    /// Leave the dialog while servers are still opening; they join when up.
    ContinueInBackground,
    /// Stop opening (and the servers already started).
    CancelOpening,
    ContinueWithServers,
    ContinueWithoutServers,
    /// **Continue without servers**: asks first, then `ContinueWithoutServers`.
    AskContinueWithoutServers,
    /// Leaves a server dialog opened from the main window, keeping the
    /// servers in use.
    CloseServerDialog,
    /// Shows or hides the sd-server settings window.
    ServerSettings(bool),
    /// Shows or hides the server status window.
    ServerStatus(bool),
    /// The Settings window.
    Settings(bool),
    SetNotifications(increart_core::notify::Notifications),
    /// Settings → Test: plays a sound now, whatever the alerts' state.
    TestSound(increart_core::notify::Sound),
    /// A sound played (the player) or not; `test`: from Test.
    SoundPlayed(Result<String, String>, bool),
    /// No alert sounds for these seconds (`None`: until the app restarts).
    Snooze(Option<f64>),
    Unsnooze,
    /// The app's window gained or lost the focus.
    Focused(bool),
    /// Closes the "no server is connected" warning.
    CloseNoServerWarning,
    /// Back to the server dialog from the main window (**Servers…**).
    OpenServerDialog,
    // ---- navigation
    Tab(Tab),
    InboxTab(InboxTab),
    ProjectsTab(ProjectsTab),
    // ---- ideas
    Draft(IdeaDraft),
    /// Add the draft: to the inbox, straight to the library, or keep and promote.
    AddIdea(AddIdea),
    DropFiles(Vec<PathBuf>),
    InboxCursor(usize),
    /// Idea inbox Grid: the search, filters and order (the cursor starts over).
    InboxFilter(InboxFilter),
    /// Review the idea under the inbox cursor (keys / buttons).
    ReviewCurrentIdea(Verdict),
    ReviewIdea(Id, Verdict),
    /// Asks for the new project's name, then promotes these ideas.
    Promote(Vec<Id>),
    EditPromoteName(String),
    /// The Promote dialog's step: choosing where (true) or naming.
    PromoteChoose(bool),
    PromoteParent(PathBuf),
    /// Step 2's new folder (`None`: none).
    PromoteNewFolder(Option<String>),
    PromoteFilter(String),
    /// Collapses or expands a folder of the project tree.
    ToggleProjectFolder(PathBuf),
    /// Collapses (true) or expands every folder of the project tree.
    CollapseProjectFolders(bool),
    /// `true`: in the dialog's folder (Create in folder).
    ConfirmPromote,
    CancelPromote,
    Restore(Id, IdeaLoc),
    SetStar(Id, bool),
    DeleteDiscarded(Id),
    // ---- input library
    LibFolder(Option<PathBuf>),
    /// The idea in the Input library's side panel (`None` closes it).
    LibInfo(Option<Id>),
    LibSelect {
        id: Id,
        additive: bool,
    },
    LibClearSelection,
    // ---- trees
    NewFolder(Tree, PathBuf),
    NewProject(PathBuf),
    StartRename(Tree, PathBuf),
    EditRename(String),
    CommitRename,
    CancelRename,
    MoveNode(Tree, PathBuf, PathBuf),
    DeleteNode(Tree, PathBuf),
    // ---- projects
    SelectProject(Id),
    SelectProjectFolder(Option<PathBuf>),
    Generate(GenerateForm),
    SelectRecipe(usize),
    /// Adds images to the Generate form's list as references.
    AddRefs(Vec<ImageRef>),
    /// Opens (true) or closes Generate's From library… picker.
    RefPicker(bool),
    RefPickerSource(RefSource),
    RefPickerText(String),
    RefPickerRatings(Ratings),
    /// Chooses an image in the picker, or unchooses it.
    RefPickerToggle(ImageRef),
    /// Adds the chosen images as references and closes the picker.
    RefPickerAdd,
    /// Removes the list's `i`th image.
    RemoveImage(usize),
    /// Sets the list's `i`th image's roles (`<imageN>` reference, start).
    ImageRoles {
        index: usize,
        reference: bool,
        start: bool,
    },
    MoveImage {
        from: usize,
        to: usize,
    },
    /// Renumber the prompt's `<imageN>` tags as proposed (or leave it).
    RetagConfirm,
    RetagDismiss,
    QueueGenerate,
    /// Queue the Sweep mode's runs (`true`: a big one was confirmed).
    QueueSweep(bool),
    /// Jobs: how the queue shares the servers (saved in the config).
    JobOrder(Order),
    /// The sweep's "one at a time" question: `Some(true)` switches to
    /// spread evenly and queues, `Some(false)` queues as is, `None` doesn't.
    SweepOrderAnswer(Option<bool>),
    /// Stop a run: Finish & stop or Stop now.
    CancelRun(Id, Stop),
    /// Stop every active run.
    StopAll(Stop),
    // ---- review
    ReviewScope(Option<Id>),
    /// Review one sweep of a project (its contact sheet).
    ReviewSweep(Id, String),
    /// Review one run of a project.
    ReviewRun(Id, Id),
    /// Review's scope panel: every project's runs, or only the selected one's.
    ReviewAllRuns(bool),
    /// Review's scope panel: the tree's search text.
    ReviewSearch(String),
    /// The project tree lists the selected project's runs (kept in config).
    ProjectRuns(bool),
    /// Projects by name (A–Z) instead of newest first.
    ProjectsByName(bool),
    /// Selecting a project narrows the grid to its folder, when that holds
    /// others (**narrow to siblings**).
    NarrowToSiblings(bool),
    /// Shows or hides a right-side details panel (for this session).
    ShowInfo(InfoPanel, bool),
    /// The contact sheet's name filter.
    SheetFilter(String),
    /// The contact sheet's cell size; `save`: into the config (at the end
    /// of a drag).
    SheetZoom {
        size: u32,
        save: bool,
    },
    /// The interface zoom in percent; `save`: into the config (not while
    /// dragging).
    UiZoom {
        percent: u32,
        save: bool,
    },
    /// Ctrl + / Ctrl −: the interface zoom to the next step up or down.
    UiZoomStep(bool),
    SheetCell(usize, usize),
    /// Move the contact sheet's selection by (rows, columns).
    SheetMove(isize, isize),
    /// Rate the selected cell's image (whatever its verdict).
    SheetRate(Verdict),
    /// Rate a row's unreviewed images.
    SheetRow(usize, Verdict),
    /// This many more seeds for every style with a Good image.
    SheetMoreSeeds(u64),
    /// A run's prompt and settings into its project's Generate form.
    RunInGenerate(Id),
    /// Review every project under a folder (relative to `projects/`).
    ReviewScopeFolder(PathBuf),
    ReviewCursor(usize),
    ReviewNext(isize),
    ReviewCurrent(Verdict),
    /// Re-rate (or send back to Review: `None`) an output by its link.
    ReviewLink(PathBuf, Option<Verdict>),
    UndoReview,
    ReviewView(ReviewView),
    /// `Space`: grid ↔ single.
    ToggleReviewGrid,
    /// `C`: pin the current image as A and show the next as B (again: leave).
    Compare,
    /// Swap A and B.
    SwapCompare,
    /// Rate the pinned image (A).
    ReviewPinned(Verdict),
    // ---- output library
    OutputTab(OutputTab),
    OutputSource(OutputSource),
    DeleteOutput(PathBuf),
    /// Deletes the bad images the Grid shows.
    DeleteShownRejected,
    // ---- confirmation
    Ask(String, Box<Msg>),
    ConfirmYes,
    ConfirmNo,
    // ---- from the runtime
    Scanned(Index),
    OpDone(Result<OpOk, String>, Option<Index>),
    Engine(Event),
    /// The engine (re)started and queued these runs' pending seeds.
    RunsQueued(Vec<Id>),
    DismissMessage(usize),
    /// Clears the status line's messages.
    ClearMessages,
    /// Something went wrong that isn't about one operation.
    Problem(String),
    /// A note for the status line ("Copied …").
    Info(String),
    /// Files or folders changed outside the app (the library watch).
    LibraryChanged,
    /// Files in `inbox_drop/` have stopped changing: import them.
    DropReady,
    /// How the library is being watched (for the Import tab).
    WatchStatus(String),
    // ---- folder view
    FvGo(PathBuf),
    FvBack,
    FvForward,
    FvUp,
    FvRefresh,
    FvListed(PathBuf, Vec<FvEntry>),
    /// Folders whose tiles came on screen and need their previews.
    FvWantPreviews(Vec<String>),
    /// Exactly these selected (rubber-band selection).
    FvSelectSet(std::collections::BTreeSet<String>),
    /// Arrow keys: move the cursor `by` entries; `extend` (Shift) selects
    /// the range from the anchor.
    FvMoveCursor {
        by: i32,
        extend: bool,
    },
    /// Typed letters: jump to the first name starting with them.
    FvTypeAhead(String),
    /// New folder / project in the folder view's folder, renamed in place.
    FvNewFolder,
    FvNewProject,
    /// Drag and drop: move (or with Ctrl, copy) folders, projects or image
    /// links (paths relative to the library root) into the folder `dest`.
    DropInto {
        paths: Vec<PathBuf>,
        dest: PathBuf,
        copy: bool,
    },
    /// Previews for folders in a directory: (name, images).
    FvPreviews(PathBuf, Vec<(String, Vec<ImageRef>)>),
    /// Click (`Replace`), Ctrl-click (`Toggle`), Shift-click (`Range`).
    FvSelect(String, SelectHow),
    FvSelectAll,
    FvClearSelection,
    FvMode(FvMode),
    FvIconSize(f32),
    FvShowMeta(bool),
    FvPathEdit(Option<String>),
    /// Open an entry: a folder navigates, an image opens in Review / the
    /// Output library.
    FvOpen(String),
    FvCopy,
    FvCut,
    /// Paste the clipboard into the current folder (or into a subfolder).
    FvPaste(Option<String>),
    /// Delete the selection (asks first).
    FvDelete,
    /// F2 on the selection.
    FvRename,
    /// Delete these (library-relative) entries: the answer to FvDelete.
    FvDeleteConfirmed(Vec<PathBuf>),
    // ---- file pickers
    Pick(PickKind),
    Picked(PickKind, Vec<PathBuf>),
    RemoveDraftImage(usize),
    /// Files dropped on the Generate panel: imported as references.
    DropRefs(Vec<PathBuf>),
    SetSdServer(SdServerConfig),
    // ---- drafts
    /// The GUI's clock (seconds); drives autosave.
    Tick(f64),
    /// Save pending drafts now (quitting).
    FlushDrafts,
    DraftSaved(Result<(), String>),
    /// Reset the Generate form (and its draft).
    ClearGenerate,
    // ---- outputs as inputs
    UseImage(ImageRef, UseTarget),
    /// `R`: the image to the input library as a ref (an image-only idea).
    ToInputLibrary(ImageRef),
    /// `I`: a new idea in the Idea inbox from the image and its prompt.
    NewIdeaFromImage(ImageRef),
    /// The selected project's choice for images from outputs.
    /// Remove the img2img start image from the form.
    ClearInit,
    /// The top-left back / forward (`Alt+←/→` and the mouse's side
    /// buttons outside folder views).
    NavBack,
    NavForward,
    /// Projects tab: the tree's search text.
    ProjectSearch(String),
    /// Opens the name dialog for a plain copy of the project.
    DuplicateProject(PathBuf),
    EditDuplicateName(String),
    ConfirmDuplicate,
    CancelDuplicate,
    /// Add seeds to a run (continuing its seed numbers) and queue them.
    MoreSeeds(Id, u64),
    /// Select a project and open its Generate form.
    OpenInGenerate(Id),
    // ---- prompt presets
    /// Show / hide the Presets window.
    PresetsPanel(bool),
    /// What the window lists (Ticked: those ticked now, filled in here).
    PresetsShow(PresetsView),
    PresetsSearch(String),
    /// Use a preset: replace the prompt, or insert at this character of it.
    ApplyPreset {
        task: usize,
        preset: usize,
        insert_at: Option<usize>,
    },
    PresetFavourite(usize, usize),
    /// Tick / untick a preset for the sweep (source: Selected).
    SweepToggle(usize, usize),
    /// The same by key (a tick whose preset is gone).
    SweepToggleKey(increart_core::prompts::PresetKey),
    /// Ticks (`true`) or unticks these presets (task, preset) for the sweep
    /// (source: Selected).
    SweepTicks(Vec<(usize, usize)>, bool),
    /// Unticks every preset.
    SweepClearTicks,
    /// The Generate form has selected `prompt_select`.
    PromptSelected,
    /// **Edit presets…**, on this task and preset.
    OpenPresetEditor(usize, Option<usize>),
    EditPresetEditor(PresetsEditor),
    RestorePresetDefaults,
    SavePresetEditor,
    ClosePresetEditor,
    /// **Save as preset…** with the Generate prompt.
    OpenSavePreset,
    EditSavePreset(SavePresetDialog),
    ConfirmSavePreset,
    CancelSavePreset,
    // ---- prompt history
    OpenHistory(HistoryKey),
    HistoryLoaded(HistoryKey, Vec<HistoryEntry>),
    HistorySelect(usize),
    HistorySearch(String),
    HistoryCheckpointsOnly(bool),
    HistoryEveryEntry(bool),
    /// Restore the selected entry (with the negative prompt / title too).
    HistoryRestore(Restore),
    /// Settings changes only.
    HistorySettingsOnly(bool),
    /// The entries that led to a run (`None`: every entry).
    HistoryRunFilter(Option<Id>),
    /// Projects → a run: its project's history, at its queued checkpoint.
    RunHistory(Id),
    CloseHistory,
    // ---- Edit idea
    OpenIdea(Id),
    EditIdea(IdeaEdit),
    SaveIdea,
    CloseIdea,
    // ---- input library
    LibTab(LibTab),
    LibFilter(LibFilter),
    // ---- Import tab
    LoadImports,
    Imports(Vec<(Id, SourceInfo)>),
    ImportDropNow,
    // ---- resolutions
    OpenResolutions,
    /// The Scale dialog for the Generate size or a resolutions-editor row.
    OpenScale(ScaleTarget),
    EditScale(ScaleDialog),
    ApplyScale,
    CloseScale,
    /// The New project / Duplicate dialog's name, Create, Cancel.
    EditNewProjectName(String),
    ConfirmNewProject,
    CancelNewProject,
    /// Shows an image large (`None` closes it).
    Preview(Option<ImageRef>),
    /// The large preview's next (1) or previous (-1) image in
    /// [`AppState::preview_list`], selecting it there too.
    PreviewStep(isize),
    /// The large preview, opened from the contact sheet: the image in the
    /// same seed column `d` styles down (or up), skipping cells without one;
    /// the selected cell follows.
    PreviewRow(isize),
    /// Rates the image the large preview shows. In Review's queue (which
    /// it then leaves) the preview goes on to the next image, closing after
    /// the last; on the contact sheet it steps on; elsewhere it stays.
    PreviewRate(Verdict),
    /// The job details window for this run (`None` closes it).
    JobInfo(Option<Id>),
    EditResolutions(Vec<Resolution>),
    SaveResolutions,
    CloseResolutions,
    // ---- review / output library
    NeedRecipe(Id),
    RecipeLoaded(Id, Option<Recipe>),
    /// Idea panels: the input images' metadata, read once each.
    NeedInputMeta(Vec<Id>),
    InputMetaLoaded(Vec<(Id, InputMeta)>),
    /// An output image where it is: Review when unrated, else selected in
    /// the Output library under its project.
    GoToOutput(ImageRef),
    Checkerboard(bool),
    /// Jump to the newest unreviewed image.
    ReviewNewest,
    OutputFilter(OutputFilter),
    /// The Output library's search box: project names (`true`) or the
    /// images' titles and prompts; what's typed goes along.
    OutputSearchProjects(bool),
    OutputProjectSearch(String),
    /// The sources panel has shown the source Show outputs revealed.
    OutputRevealed,
    /// A Grid preset tab: its filters replace the current ones.
    OutputPreset(OutputPreset),
    /// Output library → Grid on this source, warning about active filters
    /// or an empty source (Show outputs on projects and project folders).
    ShowOutputs(OutputSource),
    ResetOutputFilters,
    DismissOutputNotice,
    OutputSelect(Option<ImageRef>),
    AddToCollection(PathBuf, Vec<ImageRef>),
    /// A new collection (opened for rename) holding these images.
    NewCollection(PathBuf, Vec<ImageRef>),
    SetCover(Id, Option<ImageRef>),
    /// Opens the title dialog for an output image (with its title), or
    /// closes it.
    TitleDialog(Option<ImageRef>),
    EditTitle(String),
    /// Saves the dialog's title (an empty one removes it).
    CommitTitle,
    // ---- picking and tiling (DESIGN.md, Tiling)
    /// Ctrl+click toggles, Shift+click picks the range from the last picked
    /// (or the cursor); `Replace` clears the picks (a plain click).
    PickImage(ImageRef, SelectHow),
    /// The rubber band's picks, or Ctrl+A's.
    PickSet(Vec<ImageRef>),
    RatePicked(Verdict),
    /// The Tile dialog, from the picks.
    OpenTile,
    CloseTile,
    TileSelect(Option<usize>),
    /// A tile to this (column, row).
    TileMove(usize, u32, u32),
    TileSpan(usize, u32, u32),
    TileCols(u32),
    TileReflow,
    TileRemove(usize),
    TileCaption(usize, String),
    /// A text block in an empty cell (column, row).
    TileAddText(u32, u32),
    TileText(usize, String),
    TileTextSize(usize, increart_core::config::TextSize),
    /// The title and subtitle above the grid.
    TileTitle(String, String),
    TileCaptions(CaptionFill),
    /// Every image tile's caption, a template (`AppState::caption_text`).
    TileCaptionFormat(String),
    TileSettings(increart_core::config::TileSettings),
    /// Compose and copy, save or add to the library.
    TileOut(TileTarget),
    TilesDone(TileTarget, Result<crate::tile::TileImage, String>),
    OpenExport(Vec<ImageRef>, String),
    EditExport(ExportDialog),
    RunExport,
    CloseExport,
    Exported(Result<usize, String>),
    // ---- file manager
    /// Open a folder (or a file's folder) in the system file manager; the
    /// path is relative to the library or absolute.
    ShowInFileManager(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddIdea {
    Inbox,
    Keep,
    Promote,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    SaveConfig(Config, PathBuf),
    /// Plays a sound with the configured player (on a thread); answers
    /// `SoundPlayed`.
    PlaySound {
        sound: increart_core::notify::Sound,
        volume: f32,
        player: String,
        test: bool,
    },
    OpenLibrary {
        path: PathBuf,
        create: bool,
    },
    ListDevices(String),
    OpenServers {
        servers: Vec<ServerConfig>,
        shared: SdServerConfig,
        attempt: u32,
    },
    /// Start the engine with the servers that came up, and queue every run
    /// with pending seeds.
    StartEngine {
        shared: SdServerConfig,
        order: Order,
    },
    /// Spread the servers over the queued runs, or finish them one by one.
    SetJobOrder(Order),
    /// Stop the engine and the servers the app started.
    StopServers,
    /// Stop the engine only, keeping the servers (switching libraries).
    StopEngine,
    SavePresets(PromptPresets, PathBuf),
    Lib(LibOp),
    QueueRun(Id),
    CancelRun(Id, Stop),
    Rescan,
    /// Show the system file picker (desktop portal, else zenity).
    Pick {
        kind: PickKind,
        start: Option<PathBuf>,
    },
    SaveDraft(DraftSave),
    LoadHistory(HistoryKey),
    /// Adds an entry (stamped now), or replaces the newest (a merged edit).
    RecordHistory {
        key: HistoryKey,
        entry: Box<HistoryEntry>,
        replace_last: bool,
    },
    LoadRecipe(Id),
    LoadInputMeta(Vec<Id>),
    LoadImports,
    Export {
        images: Vec<ImageRef>,
        dest: PathBuf,
        keep_metadata: bool,
        by_id: bool,
    },
    ShowInFileManager {
        path: PathBuf,
        command: String,
    },
    /// List a folder for the folder view (library-relative).
    ListDir(PathBuf),
    /// Watch this library for changes made outside the app (replacing any
    /// earlier watch); runs on its own thread.
    WatchLibrary(PathBuf),
    /// Checks every store image's usage counts against the runs, one image
    /// at a time, reporting [`Msg::UsageCheck`]; runs on its own thread
    /// (it reads every image: seconds on a cold disk).
    CheckUsage,
    /// Composes a tiled image, on its own thread; answers `TilesDone`
    /// (a file is written there too).
    ComposeTiles {
        root: PathBuf,
        layout: crate::tile::TileLayout,
        settings: increart_core::config::TileSettings,
        to: TileTarget,
    },
    /// A few images from inside each of `names` (folders in `dir`); runs
    /// on its own thread.
    FolderPreviews {
        root: PathBuf,
        dir: PathBuf,
        names: Vec<String>,
    },
}

/// Where a tiled image goes.
#[derive(Clone, Debug, PartialEq)]
pub enum TileTarget {
    Clipboard,
    File(PathBuf),
    /// A new idea at the top of the input library.
    Library,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectHow {
    Replace,
    Toggle,
    Range,
}

/// Autosaved forms (DESIGN.md, Drafts).
#[derive(Clone, Debug, PartialEq)]
pub enum DraftSave {
    Generate(Id, Box<increart_core::model::GenerateDraft>),
    Idea(increart_core::model::IdeaDraftFile),
    ClearIdea,
}

/// Library changes, carried out in order by the runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum LibOp {
    NewIdea(NewIdea),
    /// Keep a new idea and promote it to a project in `folder`.
    /// The idea, then a project named so in this folder.
    NewIdeaPromote(NewIdea, PathBuf, String),
    ImportFiles(Vec<PathBuf>),
    /// Import images into the input store (for the New idea draft).
    ImportImages(Vec<PathBuf>),
    /// Import images into the input store as Generate references.
    ImportRefs(Vec<PathBuf>),
    ReviewIdea(Id, Verdict),
    MoveIdea(Id, IdeaLoc),
    SetStar(Id, bool),
    DeleteDiscarded(Id),
    /// Keep the ideas (if still in the inbox) and make one project from them.
    /// The ideas into a new project named so in this folder.
    Promote(Vec<Id>, PathBuf, String),
    CreateFolder(Tree, PathBuf),
    CreateProject(PathBuf, String),
    Rename(Tree, PathBuf, String),
    MoveNode(Tree, PathBuf, PathBuf),
    DeleteNode(Tree, PathBuf),
    CreateRun {
        project: Id,
        form: GenerateForm,
        preset: RecipePreset,
        model_dir: Option<String>,
        queue: bool,
        /// Handed back with the run, for the history's queued checkpoint.
        queued: Box<Queued>,
    },
    /// A sweep: a run per form (each with its prompt and label), tagged.
    CreateSweep {
        project: Id,
        /// Each with its model preset.
        runs: Vec<(GenerateForm, SweepTag, RecipePreset)>,
        model_dir: Option<String>,
        queue: bool,
        queued: Box<Queued>,
    },
    ReviewOutput {
        link: PathBuf,
        verdict: Option<Verdict>,
        undo: bool,
    },
    /// Output links to the trash (the images stay until Clean up).
    DeleteOutputs(Vec<PathBuf>),
    /// A new project in `parent` named after `name`, or a copy of the
    /// project at `copy_of` (relative to `projects/`); then the Generate form
    /// `form` opens on it.
    PrepareProject {
        parent: PathBuf,
        name: String,
        copy_of: Option<PathBuf>,
        form: GenerateForm,
    },
    IdeaFromOutput {
        image: Id,
        with_prompt: bool,
        loc: IdeaLoc,
    },
    /// A tiled image as a new idea in the input library.
    IdeaFromTiles {
        image: crate::tile::TileImage,
        sources: Vec<ImageRef>,
    },
    /// A plain copy of a project next to it (setup and draft, no images),
    /// named.
    CopyProject(PathBuf, String),
    AddSeeds(Id, u64),
    /// Save an idea's title, text, tags and images (Edit idea).
    SaveIdea {
        id: Id,
        title: String,
        text: String,
        tags: Vec<String>,
        images: Vec<Id>,
    },
    ImportInboxDrop,
    AddToCollection(PathBuf, Vec<ImageRef>),
    NewCollection(PathBuf, Vec<ImageRef>),
    SetCover(Id, Option<ImageRef>),
    /// project, output image, title ("" removes it)
    SetTitle(Id, Id, String),
    CopyNode(Tree, PathBuf, PathBuf),
}

/// What a run or sweep was queued from: its history checkpoint once the
/// library has made the runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Queued {
    pub project: Id,
    pub form: Box<increart_core::model::GenerateDraft>,
}

/// How much of a history entry Restore puts back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restore {
    /// The whole form, where the entry has it (else the texts).
    All,
    /// The prompt (or idea text).
    Text,
    /// The prompt and negative (or idea text and title).
    TextAndExtra,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpOk {
    Done,
    Idea(Id),
    Tiled(Id),
    /// A new project from ideas: the Generate panel is seeded from them.
    Promoted {
        project: Id,
        prompt: String,
        refs: Vec<ImageRef>,
        idea: Option<Id>,
    },
    Created(Tree, PathBuf),
    /// A project copied by Duplicate (already named): select it.
    Duplicated(PathBuf),
    Run(Id, Box<Queued>),
    /// A sweep's runs (its name).
    Sweep(Vec<Id>, String, Box<Queued>),
    /// Images imported for the draft.
    Imported(Vec<Id>),
    /// Images imported as references for the Generate form.
    ImportedRefs(Vec<Id>),
    /// Seeds added to a run: queue it.
    SeedsAdded(Id, u64),
    /// A project made by `PrepareProject`: open `form` on it.
    ProjectReady {
        path: PathBuf,
        form: Box<GenerateForm>,
    },
    Reviewed {
        link: PathBuf,
        previous: Option<Verdict>,
        undo: bool,
    },
}

/// Opens a library from the library screen. Switching: the open one is
/// just returned to, else the engine stops first (keeping the servers).
fn open_library(state: &mut AppState, path: PathBuf, create: bool) -> Vec<Effect> {
    let mut fx = vec![];
    if let Some(sw) = &mut state.switching {
        if state.library.as_ref() == Some(&path) {
            return update(state, Msg::CancelSwitch);
        }
        if !sw.stopped && state.engine_running {
            sw.stopped = true;
            fx.push(Effect::StopEngine);
        }
    }
    fx.push(Effect::OpenLibrary { path, create });
    fx
}

/// Runs `then`, asking first while runs are generating (switching stops
/// them; they continue when their library is opened again).
fn ask_while_generating(state: &mut AppState, then: Msg) -> Vec<Effect> {
    let n = state.active_runs.len();
    if n == 0 {
        return update(state, then);
    }
    state.confirm = Some(Confirm {
        text: format!(
            "{n} run(s) are generating in this library. Switching stops them: the images being \
             made now are dropped, and everything left is generated when you open this library again."
        ),
        then: Box::new(then),
    });
    vec![]
}

/// A fresh state for another library, keeping what isn't the library's:
/// settings, servers, presets, messages.
fn fresh_for_library(state: &mut AppState) {
    let mut fresh = AppState::new(state.config.clone(), state.config_path.clone());
    std::mem::swap(state, &mut fresh);
    let old = fresh;
    state.servers = old.servers;
    state.engine_running = old.engine_running;
    state.open_attempt = old.open_attempt;
    state.background = old.background;
    state.no_server_warned = old.no_server_warned;
    state.checkerboard = old.checkerboard;
    state.review_all_runs = old.review_all_runs;
    state.prompt_presets = old.prompt_presets;
    state.presets_path = old.presets_path;
    state.busy = old.busy;
    state.saving = old.saving;
    state.now = old.now;
    state.messages = old.messages;
    // (inside `update`: its nesting goes on)
    state.nav.depth = old.nav.depth;
}

fn save_presets(state: &AppState) -> Vec<Effect> {
    state.presets_path.iter().map(|p| Effect::SavePresets(state.prompt_presets.clone(), p.clone())).collect()
}

/// Names given and unique (tasks; presets within a task), prompts given.
fn check_presets(list: &PromptPresets) -> Result<(), String> {
    let mut tasks = std::collections::HashSet::new();
    for t in &list.tasks {
        let name = t.name.trim();
        if name.is_empty() {
            return Err("A task has no name".into());
        }
        if !tasks.insert(name.to_lowercase()) {
            return Err(format!("Two tasks are called {name}"));
        }
        let mut names = std::collections::HashSet::new();
        for p in &t.presets {
            let n = p.name.trim();
            if n.is_empty() {
                return Err(format!("A preset in {name} has no name"));
            }
            if !names.insert(n.to_lowercase()) {
                return Err(format!("{name} has two presets called {n}"));
            }
            if p.prompt.trim().is_empty() {
                return Err(format!("{name} → {n} has no prompt"));
            }
        }
    }
    Ok(())
}

fn save_config(state: &AppState) -> Vec<Effect> {
    state.config_path.iter().map(|p| Effect::SaveConfig(state.config.clone(), p.clone())).collect()
}

fn server_dialog() -> ServerDialog {
    ServerDialog {
        editing: None,
        draft: ServerPreset { name: String::new(), servers: vec![] },
        devices: None,
        progress: vec![],
        opening: false,
        started: 0.0,
        wizard: false,
        from_main: false,
        settings: false,
        notice: None,
    }
}

/// The command line's `--servers`: opens the preset (going on to the main
/// window while it starts) or continues without servers. An unknown or
/// unusable preset leaves the servers screen up, saying why.
fn skip_servers(state: &mut AppState, choice: StartupServers) -> Vec<Effect> {
    let StartupServers::Preset(asked) = choice else { return update(state, Msg::ContinueWithoutServers) };
    let name = if asked.eq_ignore_ascii_case("last") { state.config.last_preset.clone() } else { Some(asked.clone()) };
    let found = name.and_then(|n| state.config.server_presets.iter().position(|p| p.name.eq_ignore_ascii_case(&n)));
    let problem = match found.map(|i| &state.config.server_presets[i]) {
        None if asked.eq_ignore_ascii_case("last") => Some("no preset opened yet".to_string()),
        None => Some("no preset by that name".into()),
        Some(p) if p.servers.is_empty() => Some("the preset has no servers".into()),
        Some(p) if p.servers.iter().any(|s| matches!(s, ServerConfig::Start { .. })) => state.sdserver_problem(),
        Some(_) => None,
    };
    if let (Some(e), Phase::Servers(d)) = (problem, &mut state.phase) {
        d.notice = Some(format!("--servers {asked}: {e}"));
        return vec![];
    }
    let mut fx = update(state, Msg::OpenPresetNow(found.unwrap_or_default()));
    fx.extend(update(state, Msg::ContinueInBackground));
    fx
}

/// Opening servers stops the ones in use: ask first when there are any.
fn ask_before_replacing(state: &mut AppState, what: &str, then: Msg) -> Vec<Effect> {
    if state.servers.is_empty() && !state.engine_running {
        return update(state, then);
    }
    let jobs = match state.active_runs.len() {
        0 => String::new(),
        n => format!(
            "\n\n{n} run(s) are generating: the images being made now are dropped and made again; \
             the rest of the queue goes on once the new servers are up."
        ),
    };
    update(state, Msg::Ask(format!("Stop the servers in use and open {what}?{jobs}"), Box::new(then)))
}

/// The editor's preset into `config.server_presets`: in place when editing
/// one (renaming it), else added. Closes the editor; false (with an error
/// message) when the name is missing or taken.
fn save_draft(state: &mut AppState) -> bool {
    let Phase::Servers(d) = &mut state.phase else { return false };
    let Some(editing) = d.editing else { return false };
    let name = d.draft.name.trim().to_string();
    let taken =
        state.config.server_presets.iter().enumerate().any(|(i, p)| p.name == name && editing != Editing::Preset(i));
    if name.is_empty() || taken {
        let e = if taken { format!("There is already a preset named {name}") } else { "Name the preset first".into() };
        state.messages_error(e);
        return false;
    }
    d.draft.name = name.clone();
    let preset = d.draft.clone();
    d.editing = None;
    match editing {
        Editing::Preset(i) if i < state.config.server_presets.len() => {
            let old = std::mem::replace(&mut state.config.server_presets[i], preset);
            if state.config.last_preset.as_ref() == Some(&old.name) {
                state.config.last_preset = Some(name);
            }
        }
        _ => state.config.server_presets.push(preset),
    }
    true
}

/// `--list-devices` for the editor and settings, unless listed already (the
/// settings' **List devices** asks again).
fn list_devices_once(state: &AppState) -> Vec<Effect> {
    match &state.phase {
        Phase::Servers(d) if d.devices.is_none() && !state.config.sdserver.binary.is_empty() => {
            vec![Effect::ListDevices(state.config.sdserver.binary.clone())]
        }
        _ => vec![],
    }
}

/// A `Start` server on `device`, on the next free port.
fn add_start_server(draft: &mut ServerPreset, device: &str) {
    let server = ServerConfig::Start {
        name: format!("gpu{}", draft.servers.len()),
        host: "127.0.0.1".into(),
        port: next_port(draft),
        device: device.into(),
        args: vec![],
    };
    draft.servers.push(server);
}

/// The wizard's preset name: its devices ("ROCm0 + ROCm1"), or "Running
/// server(s)" for connections only.
fn wizard_name(draft: &ServerPreset) -> String {
    let devices: Vec<&str> = draft
        .servers
        .iter()
        .filter_map(|s| match s {
            ServerConfig::Start { device, .. } => Some(device.as_str()),
            ServerConfig::Connect { .. } => None,
        })
        .collect();
    match (devices.len(), draft.servers.len()) {
        (0, 1) => "Running server".into(),
        (0, _) => "Running servers".into(),
        _ => devices.join(" + "),
    }
}

/// The next free port after the draft's `Start` servers (from 1234).
fn next_port(draft: &ServerPreset) -> u16 {
    draft
        .servers
        .iter()
        .filter_map(|s| match s {
            ServerConfig::Start { port, .. } => Some(*port + 1),
            _ => None,
        })
        .max()
        .unwrap_or(1234)
}

pub fn update(state: &mut AppState, msg: Msg) -> Vec<Effect> {
    state.nav.depth += 1;
    let fx = apply(state, msg);
    state.nav.depth -= 1;
    if state.nav.depth == 0 && !state.nav.restoring && state.phase == Phase::Main {
        let now = state.place();
        if state.nav.current.as_ref() != Some(&now) {
            if let Some(c) = state.nav.current.replace(now) {
                state.nav.back.push(c);
                if state.nav.back.len() > 100 {
                    state.nav.back.remove(0);
                }
            }
            state.nav.forward.clear();
        }
    }
    fx
}

/// Goes to a place from the history, through the messages that go there.
fn go_to(state: &mut AppState, p: Place) -> Vec<Effect> {
    let mut fx = update(state, Msg::Tab(p.tab));
    fx.extend(update(state, Msg::InboxTab(p.inbox_tab)));
    fx.extend(update(state, Msg::LibTab(p.lib_tab)));
    fx.extend(update(state, Msg::ProjectsTab(p.projects_tab)));
    fx.extend(update(state, Msg::OutputTab(p.output_tab)));
    if p.lib_folder != state.lib_folder {
        fx.extend(update(state, Msg::LibFolder(p.lib_folder)));
    }
    match p.project {
        Some(id) if state.project.as_ref() != Some(&id) && state.index.project(&id).is_some() => {
            fx.extend(update(state, Msg::SelectProject(id)));
        }
        None if state.project.is_some() || state.proj_folder != p.proj_folder => {
            fx.extend(update(state, Msg::SelectProjectFolder(p.proj_folder)));
        }
        _ => {}
    }
    match p.review_folder {
        Some(f) if state.review_folder.as_ref() != Some(&f) => fx.extend(update(state, Msg::ReviewScopeFolder(f))),
        None if state.review_folder.is_some() || state.review_scope != p.review_scope => {
            fx.extend(update(state, Msg::ReviewScope(p.review_scope)));
        }
        _ => {}
    }
    if p.output_source != state.output_source {
        fx.extend(update(state, Msg::OutputSource(p.output_source)));
    }
    fx
}

fn apply(state: &mut AppState, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::NavBack | Msg::NavForward => {
            let nav = &mut state.nav;
            let (from, to) =
                if msg == Msg::NavBack { (&mut nav.back, &mut nav.forward) } else { (&mut nav.forward, &mut nav.back) };
            let Some(p) = from.pop() else { return vec![] };
            if let Some(c) = nav.current.take() {
                to.push(c);
            }
            state.nav.restoring = true;
            let fx = go_to(state, p);
            state.nav.restoring = false;
            state.nav.current = Some(state.place());
            fx
        }
        Msg::ProjectSearch(q) => {
            state.project_search = q;
            vec![]
        }
        // ------------------------------------------------ library dialog
        Msg::SetLibraryPath(p) => {
            if let Phase::Library(d) = &mut state.phase {
                d.path = p;
            }
            vec![]
        }
        Msg::OpenLibrary | Msg::CreateLibrary => {
            let create = msg == Msg::CreateLibrary;
            let Phase::Library(d) = &mut state.phase else { return vec![] };
            if d.path.trim().is_empty() {
                d.error = Some("Enter a folder".into());
                return vec![];
            }
            d.busy = true;
            d.error = None;
            let path = PathBuf::from(d.path.trim());
            open_library(state, path, create)
        }
        Msg::OpenRecentLibrary(i) => {
            let (Phase::Library(d), Some(path)) = (&mut state.phase, state.config.recent_libraries.get(i)) else {
                return vec![];
            };
            if d.busy {
                return vec![];
            }
            d.path = path.clone();
            d.busy = true;
            d.error = None;
            let path = PathBuf::from(path);
            open_library(state, path, false)
        }
        Msg::ForgetLibrary(i) => {
            if i >= state.config.recent_libraries.len() {
                return vec![];
            }
            state.config.recent_libraries.remove(i);
            save_config(state)
        }
        Msg::LibraryWizard(w) => {
            if let Phase::Library(d) = &mut state.phase
                && !d.busy
            {
                d.error = None;
                d.wizard = w.map(|(kind, path)| {
                    d.path = path;
                    kind
                });
            }
            vec![]
        }
        Msg::LibraryOpened(Ok((root, index))) => {
            let switched = state.switching.take().is_some();
            if switched {
                fresh_for_library(state);
            }
            state.config.add_recent_library(root.to_string_lossy().into_owned());
            state.library = Some(root.clone());
            if let Some(d) = &index.idea_draft {
                state.draft = IdeaDraft::from_file(d);
            }
            state.index = index;
            state.rebuild_usage();
            state.inbox_tab = if state.unreviewed_ideas().is_empty() { InboxTab::NewIdea } else { InboxTab::Grid };
            let mut fx = save_config(state);
            fx.push(Effect::WatchLibrary(root));
            fx.push(Effect::CheckUsage);
            if switched {
                // the servers stay; the engine restarts on this library
                state.phase = Phase::Main;
                if state.engine_running {
                    fx.push(Effect::StartEngine {
                        shared: state.config.sdserver.clone(),
                        order: state.config.job_order,
                    });
                }
                return fx;
            }
            state.phase = Phase::Servers(server_dialog());
            if let Some(choice) = state.startup_servers.take() {
                fx.extend(skip_servers(state, choice));
            }
            fx
        }
        Msg::LibraryOpened(Err(e)) => {
            if let Phase::Library(d) = &mut state.phase {
                d.busy = false;
                d.error = Some(e);
            }
            vec![]
        }

        // ------------------------------------------------ switching libraries
        Msg::SwitchLibrary(path) => {
            if state.library.as_deref() == Some(std::path::Path::new(&path)) {
                return vec![];
            }
            ask_while_generating(state, Msg::SwitchLibraryNow(path))
        }
        Msg::SwitchLibraryNow(path) => {
            if state.phase != Phase::Main {
                return vec![];
            }
            let mut fx = update(state, Msg::FlushDrafts);
            state.switching = Some(LibrarySwitch::default());
            state.phase = Phase::Library(LibraryDialog { path: path.clone(), busy: true, ..Default::default() });
            fx.extend(open_library(state, PathBuf::from(path), false));
            fx
        }
        Msg::OtherLibrary(w) => ask_while_generating(state, Msg::OtherLibraryNow(w)),
        Msg::OtherLibraryNow(w) => {
            if state.phase != Phase::Main {
                return vec![];
            }
            let fx = update(state, Msg::FlushDrafts);
            state.switching = Some(LibrarySwitch::default());
            state.phase = Phase::Library(LibraryDialog::default());
            if w.is_some() {
                update(state, Msg::LibraryWizard(w));
            }
            fx
        }
        Msg::CancelSwitch => {
            let Some(sw) = state.switching.take() else { return vec![] };
            state.phase = Phase::Main;
            if sw.stopped && state.engine_running {
                return vec![Effect::StartEngine {
                    shared: state.config.sdserver.clone(),
                    order: state.config.job_order,
                }];
            }
            vec![]
        }

        // ------------------------------------------------ server dialog
        Msg::OpenPreset(i) => match state.config.server_presets.get(i) {
            Some(p) => {
                let what = p.name.clone();
                ask_before_replacing(state, &what, Msg::OpenPresetNow(i))
            }
            None => vec![],
        },
        Msg::OpenPresetNow(i) => {
            let (Phase::Servers(d), Some(p)) = (&mut state.phase, state.config.server_presets.get(i)) else {
                return vec![];
            };
            if d.opening {
                return vec![];
            }
            d.editing = None;
            d.draft = p.clone();
            update(state, Msg::OpenDraftNow)
        }
        Msg::EditPreset(i) => {
            if let (Phase::Servers(d), Some(p)) = (&mut state.phase, state.config.server_presets.get(i))
                && !d.opening
            {
                d.editing = Some(Editing::Preset(i));
                d.draft = p.clone();
                d.progress.clear();
                return list_devices_once(state);
            }
            vec![]
        }
        Msg::NewPreset => {
            if let Phase::Servers(d) = &mut state.phase
                && !d.opening
            {
                d.editing = Some(Editing::New);
                d.draft = ServerPreset { name: String::new(), servers: vec![] };
                d.progress.clear();
                return list_devices_once(state);
            }
            vec![]
        }
        Msg::CancelEdit => {
            if let Phase::Servers(d) = &mut state.phase {
                d.editing = None;
            }
            vec![]
        }
        Msg::AddStartServer | Msg::AddConnectServer => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            let n = d.draft.servers.len();
            if msg == Msg::AddStartServer {
                add_start_server(&mut d.draft, &format!("ROCm{n}"));
            } else {
                d.draft.servers.push(ServerConfig::Connect {
                    name: format!("server{n}"),
                    url: "http://127.0.0.1:1234".into(),
                    device: String::new(),
                });
            }
            d.progress.clear();
            vec![]
        }
        Msg::EditServer(i, s) => {
            if let Phase::Servers(d) = &mut state.phase
                && let Some(slot) = d.draft.servers.get_mut(i)
            {
                *slot = s;
                d.progress.clear();
            }
            vec![]
        }
        Msg::RemoveServer(i) => {
            if let Phase::Servers(d) = &mut state.phase
                && i < d.draft.servers.len()
            {
                d.draft.servers.remove(i);
                d.progress.clear();
            }
            vec![]
        }
        Msg::SetPresetName(n) => {
            if let Phase::Servers(d) = &mut state.phase {
                d.draft.name = n;
            }
            vec![]
        }
        Msg::SavePreset => {
            if !save_draft(state) {
                return vec![];
            }
            save_config(state)
        }
        Msg::SaveAndOpen => {
            if !save_draft(state) {
                return vec![];
            }
            let mut fx = save_config(state);
            fx.extend(update(state, Msg::OpenDraft));
            fx
        }
        Msg::DeletePreset(i) => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            if i >= state.config.server_presets.len() {
                return vec![];
            }
            let gone = state.config.server_presets.remove(i);
            if state.config.last_preset.as_ref() == Some(&gone.name) {
                state.config.last_preset = None;
            }
            d.editing = match d.editing {
                Some(Editing::Preset(e)) if e == i => None,
                Some(Editing::Preset(e)) if e > i => Some(Editing::Preset(e - 1)),
                e => e,
            };
            save_config(state)
        }
        Msg::ListDevices => vec![Effect::ListDevices(state.config.sdserver.binary.clone())],
        Msg::Devices(r) => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            // the wizard starts with every GPU ticked
            if d.wizard
                && d.draft.servers.is_empty()
                && let Ok(list) = &r
            {
                for l in list.iter().map(|l| device_name(l)).filter(|n| !n.eq_ignore_ascii_case("cpu")) {
                    add_start_server(&mut d.draft, l);
                }
            }
            d.devices = Some(r);
            vec![]
        }
        Msg::ServerWizard(open) => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            if d.opening {
                return vec![];
            }
            d.wizard = open;
            d.editing = None;
            d.progress.clear();
            d.draft = ServerPreset { name: String::new(), servers: vec![] };
            if !open {
                return vec![];
            }
            d.devices = None;
            update(state, Msg::ListDevices)
        }
        Msg::DraftDevice(device, on) => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            let has = |s: &ServerConfig| matches!(s, ServerConfig::Start { device: x, .. } if *x == device);
            if on && !d.draft.servers.iter().any(has) {
                add_start_server(&mut d.draft, &device);
            } else if !on {
                d.draft.servers.retain(|s| !has(s));
            }
            vec![]
        }
        Msg::FinishServerWizard => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            if !d.wizard || d.draft.servers.is_empty() {
                return vec![];
            }
            d.draft.name = wizard_name(&d.draft);
            d.editing = Some(Editing::New);
            d.wizard = false;
            update(state, Msg::SaveAndOpen)
        }
        Msg::OpenDraft => {
            let Phase::Servers(d) = &state.phase else { return vec![] };
            let what = if d.draft.name.trim().is_empty() { "these servers".into() } else { d.draft.name.clone() };
            ask_before_replacing(state, &what, Msg::OpenDraftNow)
        }
        Msg::OpenDraftNow => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            if d.draft.servers.is_empty() || d.opening {
                return vec![];
            }
            d.editing = None;
            d.opening = true;
            d.started = state.now;
            d.progress = vec![ServerProgress::Opening; d.draft.servers.len()];
            state.start_log = vec![String::new(); d.draft.servers.len()];
            state.open_attempt += 1;
            state.background = None;
            let mut fx = vec![stop_servers(state)];
            let Phase::Servers(d) = &state.phase else { unreachable!() };
            if !d.draft.name.is_empty() && state.config.preset(&d.draft.name).is_some() {
                state.config.last_preset = Some(d.draft.name.clone());
                fx.extend(save_config(state));
            }
            let Phase::Servers(d) = &state.phase else { unreachable!() };
            state.servers = d
                .draft
                .servers
                .iter()
                .map(|s| ServerStatus {
                    name: s.name().into(),
                    device: s.device().into(),
                    state: "opening".into(),
                    model: None,
                    idle_since: None,
                    worked: false,
                })
                .collect();
            fx.push(Effect::OpenServers {
                servers: d.draft.servers.clone(),
                shared: state.config.sdserver.clone(),
                attempt: state.open_attempt,
            });
            fx
        }
        Msg::ServerOpened(attempt, _, _) | Msg::ServersSettled(attempt) | Msg::ServerLog(attempt, _, _)
            if attempt != state.open_attempt =>
        {
            vec![]
        }
        Msg::ServersStopped(names) => {
            state.stopping.clear();
            if !names.is_empty() {
                state.messages_info(format!("Stopped sd-server {}: its GPU memory is free", names.join(", ")));
            }
            vec![]
        }
        Msg::ServerLog(_, i, line) => {
            let opening = match &state.phase {
                Phase::Servers(d) => d.progress.get(i),
                _ => state.background.as_ref().and_then(|p| p.get(i)),
            };
            if opening == Some(&ServerProgress::Opening) {
                if state.background.is_some()
                    && let Some(s) = state.servers.get_mut(i)
                {
                    s.state = format!("starting… {line}");
                }
                if let Some(l) = state.start_log.get_mut(i) {
                    *l = line;
                }
            }
            vec![]
        }
        Msg::ServerOpened(_, i, r) => {
            let progress = match &mut state.phase {
                Phase::Servers(d) if d.opening => Some(&mut d.progress),
                _ => state.background.as_mut(),
            };
            if let Some(p) = progress.and_then(|p| p.get_mut(i)) {
                *p = match &r {
                    Ok(()) => ServerProgress::Up,
                    Err(e) => ServerProgress::Failed(e.clone()),
                };
            }
            if state.background.is_some()
                && let Some(s) = state.servers.get_mut(i)
            {
                s.state = match &r {
                    Ok(()) => "up, connecting".into(),
                    Err(_) => "failed".into(),
                };
            }
            if let (Some(_), Err(e)) = (&state.background, r) {
                state.messages_error(e);
            }
            vec![]
        }
        Msg::ServersSettled(_) => {
            if let Some(progress) = state.background.take() {
                return start_with_up(state, &progress);
            }
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            d.opening = false;
            if d.progress.iter().all(|p| *p == ServerProgress::Up) {
                state.phase = Phase::Main;
                state.engine_running = true;
                state.servers.iter_mut().for_each(|s| (s.idle_since, s.worked) = (None, false));
                return vec![Effect::StartEngine {
                    shared: state.config.sdserver.clone(),
                    order: state.config.job_order,
                }];
            }
            vec![]
        }
        Msg::ContinueInBackground => {
            let Phase::Servers(d) = &state.phase else { return vec![] };
            if !d.opening {
                return vec![];
            }
            state.background = Some(d.progress.clone());
            for (s, p) in state.servers.iter_mut().zip(&d.progress) {
                s.state = match p {
                    ServerProgress::Opening => "starting…".into(),
                    ServerProgress::Up => "up, connecting".into(),
                    ServerProgress::Failed(_) => "failed".into(),
                };
            }
            state.phase = Phase::Main;
            vec![]
        }
        Msg::CancelOpening => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            if !d.opening {
                return vec![];
            }
            d.opening = false;
            d.progress.clear();
            state.open_attempt += 1;
            let fx = vec![stop_servers(state)];
            state.servers.clear();
            fx
        }
        Msg::ContinueWithServers => {
            let Phase::Servers(d) = &state.phase else { return vec![] };
            if d.opening || !d.progress.contains(&ServerProgress::Up) {
                return vec![];
            }
            let up: Vec<bool> = d.progress.iter().map(|p| *p == ServerProgress::Up).collect();
            state.servers = state.servers.iter().zip(up).filter(|(_, u)| *u).map(|(s, _)| s.clone()).collect();
            state.phase = Phase::Main;
            state.engine_running = true;
            state.servers.iter_mut().for_each(|s| (s.idle_since, s.worked) = (None, false));
            vec![Effect::StartEngine { shared: state.config.sdserver.clone(), order: state.config.job_order }]
        }
        Msg::ContinueWithoutServers => {
            state.phase = Phase::Main;
            let fx = vec![stop_servers(state)];
            state.servers.clear();
            fx
        }
        Msg::OpenServerDialog => {
            if state.phase == Phase::Main {
                state.server_status = false;
                state.phase = Phase::Servers(ServerDialog { from_main: true, ..server_dialog() });
            }
            vec![]
        }
        Msg::AskContinueWithoutServers => {
            let n = state.active_runs.len();
            let text = if state.servers.is_empty() {
                "Continue without servers? Browsing, review and the libraries work, but nothing generates \
                 until a server is connected (Servers… in the top bar)."
            } else {
                "Stop the servers in use and continue without? Nothing generates until a server is connected \
                 again; queued runs wait."
            };
            let text = match n {
                0 => text.to_string(),
                n => format!(
                    "{text}\n\n{n} run(s) are generating: the images being made now are dropped and made again later."
                ),
            };
            update(state, Msg::Ask(text, Box::new(Msg::ContinueWithoutServers)))
        }
        Msg::CloseServerDialog => {
            if matches!(&state.phase, Phase::Servers(d) if d.from_main && !d.opening) {
                state.phase = Phase::Main;
            }
            vec![]
        }
        Msg::ServerSettings(open) => {
            let Phase::Servers(d) = &mut state.phase else { return vec![] };
            d.settings = open && !d.opening;
            if d.settings { list_devices_once(state) } else { vec![] }
        }
        Msg::ServerStatus(open) => {
            state.server_status = open && state.phase == Phase::Main;
            vec![]
        }
        Msg::Settings(open) => {
            state.settings_open = open && state.phase == Phase::Main;
            vec![]
        }
        Msg::SetNotifications(n) => {
            if state.config.notifications == n {
                return vec![];
            }
            state.config.notifications = n;
            save_config(state)
        }
        Msg::TestSound(sound) => {
            state.alerts.test_result = None;
            let n = &state.config.notifications;
            vec![Effect::PlaySound { sound, volume: n.volume, player: n.player.clone(), test: true }]
        }
        Msg::SoundPlayed(r, test) => {
            if test {
                state.alerts.test_result = Some(r.clone());
            }
            match r {
                Ok(_) => state.alerts.sound_error = None,
                Err(e) => {
                    if !test && state.alerts.sound_error.as_ref() != Some(&e) {
                        state.messages_error(format!("Notification sound: {e}"));
                    }
                    state.alerts.sound_error = Some(e);
                }
            }
            vec![]
        }
        Msg::Snooze(secs) => {
            match secs {
                Some(s) => state.alerts.snooze_until = Some(state.now + s),
                None => state.alerts.muted = true,
            }
            vec![]
        }
        Msg::Unsnooze => {
            state.alerts.snooze_until = None;
            state.alerts.muted = false;
            vec![]
        }
        Msg::Focused(f) => {
            state.alerts.focused = f;
            if f {
                state.alerts.cleared_text = None;
            }
            vec![]
        }
        Msg::CloseNoServerWarning => {
            state.no_server_warning = false;
            vec![]
        }

        // ------------------------------------------------ navigation
        Msg::Tab(t) => {
            if t != state.tab {
                state.picked.clear();
            }
            if t == Tab::IdeaInbox && state.tab != Tab::IdeaInbox {
                state.inbox_tab = if state.unreviewed_ideas().is_empty() { InboxTab::NewIdea } else { InboxTab::Grid };
            }
            state.tab = t;
            fv_sync(state);
            relist(state)
        }
        Msg::InboxTab(t) => {
            state.inbox_tab = t;
            fv_sync(state);
            relist_if_stale(state)
        }
        Msg::ProjectsTab(t) => {
            state.projects_tab = t;
            fv_sync(state);
            if t == ProjectsTab::Folder && state.folder_view.listed.as_ref() != Some(&state.folder_view.path) {
                state.folder_view.listed = None;
                return relist(state);
            }
            vec![]
        }

        // ------------------------------------------------ ideas
        Msg::Draft(d) => {
            if d != state.draft {
                state.draft = d;
                state.idea_dirty = Some(state.now);
            }
            vec![]
        }
        Msg::AddIdea(how) => {
            if state.draft.is_empty() {
                state.messages_error("The idea is empty");
                return vec![];
            }
            if how == AddIdea::Promote {
                // the draft stays until the project is named
                let name = promote_name(state, [state.draft.title.as_str()]);
                state.promote = Some(PromoteDialog::new(vec![], promote_parent(state), name));
                return vec![];
            }
            add_idea(state, how, PathBuf::new(), String::new())
        }
        Msg::DropFiles(paths) => {
            if paths.is_empty() || state.library.is_none() {
                return vec![];
            }
            state.busy += 1;
            vec![Effect::Lib(LibOp::ImportFiles(paths))]
        }
        Msg::InboxCursor(i) => {
            state.inbox_cursor = i.min(state.inbox_queue().len().saturating_sub(1));
            vec![]
        }
        Msg::InboxFilter(f) => {
            state.inbox_filter = f;
            state.inbox_cursor = 0;
            vec![]
        }
        Msg::ReviewCurrentIdea(v) => {
            let Some(e) = state.inbox_queue().get(state.inbox_cursor).map(|e| e.idea.id.clone()) else {
                return vec![];
            };
            update(state, Msg::ReviewIdea(e, v))
        }
        Msg::ReviewIdea(id, v) => lib(state, LibOp::ReviewIdea(id, v)),
        Msg::Promote(ids) => {
            if ids.is_empty() {
                return vec![];
            }
            let titles: Vec<String> =
                ids.iter().filter_map(|i| state.index.idea(i)).map(|e| e.idea.title.clone()).collect();
            let name = promote_name(state, titles.iter().map(String::as_str));
            state.promote = Some(PromoteDialog::new(ids, promote_parent(state), name));
            vec![]
        }
        Msg::EditPromoteName(n) => {
            if let Some(d) = &mut state.promote {
                if d.own_folder() {
                    d.new_folder = Some(n.trim().to_string());
                }
                d.name = n;
            }
            vec![]
        }
        Msg::PromoteChoose(on) => {
            if let Some(d) = &mut state.promote {
                d.choosing = on;
            }
            vec![]
        }
        Msg::PromoteParent(p) => {
            if let Some(d) = &mut state.promote {
                d.parent = p;
            }
            vec![]
        }
        Msg::PromoteNewFolder(f) => {
            if let Some(d) = &mut state.promote {
                d.new_folder = f;
            }
            vec![]
        }
        Msg::PromoteFilter(f) => {
            if let Some(d) = &mut state.promote {
                d.filter = f;
            }
            vec![]
        }
        Msg::ConfirmPromote => {
            let Some(d) = state.promote.take() else { return vec![] };
            let name = d.name.trim().to_string();
            let (parent, _) = promote_folder(state, &d);
            if let Some(e) = promote_problem(state, &d) {
                state.messages_error(e);
                state.promote = Some(d);
                return vec![];
            }
            // the next one goes next to a project's own folder, not in it
            state.promote_last = Some(if d.own_folder() { d.parent.clone() } else { parent.clone() });
            if d.ideas.is_empty() {
                return add_idea(state, AddIdea::Promote, parent, name);
            }
            lib(state, LibOp::Promote(d.ideas, parent, name))
        }
        Msg::CancelPromote => {
            state.promote = None;
            vec![]
        }
        Msg::Restore(id, loc) => lib(state, LibOp::MoveIdea(id, loc)),
        Msg::SetStar(id, on) => lib(state, LibOp::SetStar(id, on)),
        Msg::DeleteDiscarded(id) => lib(state, LibOp::DeleteDiscarded(id)),

        // ------------------------------------------------ input library
        Msg::LibFolder(f) => {
            // on the Folder tab, the folder view goes there too
            let path = std::path::Path::new("input/library").join(f.clone().unwrap_or_default());
            state.lib_folder = f;
            state.lib_selected.clear();
            if state.tab == Tab::InputLibrary && state.lib_tab == LibTab::Folder {
                return fv_go(state, path, true);
            }
            vec![]
        }
        Msg::LibSelect { id, additive } => {
            if !additive {
                state.lib_selected.clear();
                state.lib_selected.insert(id);
            } else if !state.lib_selected.remove(&id) {
                state.lib_selected.insert(id);
            }
            vec![]
        }
        Msg::LibInfo(id) => {
            state.lib_info = id;
            vec![]
        }
        Msg::LibClearSelection => {
            state.lib_selected.clear();
            vec![]
        }

        // ------------------------------------------------ trees
        Msg::NewFolder(tree, parent) => lib(state, LibOp::CreateFolder(tree, parent)),
        Msg::NewProject(parent) => {
            // asks the name first; made on Create
            let mut name = "new project".to_string();
            for n in 2.. {
                if !state.project_name_taken(&parent.join("_"), &name) {
                    break;
                }
                name = format!("new project {n}");
            }
            state.renaming = Some(Renaming { tree: Tree::Projects, path: parent.join(&name), name, creating: true });
            vec![]
        }
        Msg::StartRename(tree, path) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            state.renaming = Some(Renaming { tree, path, name, creating: false });
            vec![]
        }
        Msg::EditRename(name) => {
            if let Some(r) = &mut state.renaming {
                r.name = name;
            }
            vec![]
        }
        Msg::CommitRename => match state.renaming.take() {
            Some(r) if r.creating => {
                let parent = r.path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
                lib(state, LibOp::CreateProject(parent, r.name.trim().to_string()))
            }
            Some(r) if r.path.file_name().is_some_and(|n| n.to_string_lossy() != r.name) => {
                lib(state, LibOp::Rename(r.tree, r.path, r.name))
            }
            _ => vec![],
        },
        Msg::CancelRename => {
            state.renaming = None;
            vec![]
        }
        Msg::MoveNode(tree, path, parent) => lib(state, LibOp::MoveNode(tree, path, parent)),
        Msg::DeleteNode(tree, path) => lib(state, LibOp::DeleteNode(tree, path)),

        // ------------------------------------------------ projects
        Msg::SelectProject(id) => {
            // narrowed to its folder when that holds others (and asked);
            // else the grid keeps showing what it did, if the project is in it
            let shown = state.project_scope();
            let keep = state.index.project(&id).and_then(|p| {
                let narrow = !state.config.project_click_keeps_scope && state.has_siblings(p);
                (!narrow && p.dir.starts_with(std::path::Path::new("projects").join(&shown))).then_some(shown)
            });
            let mut fx = vec![];
            if state.project.as_ref() != Some(&id) {
                fx = flush_generate(state);
                state.project = Some(id);
                load_generate(state);
            }
            state.proj_folder = None;
            state.proj_scope_kept = keep;
            fx
        }
        Msg::SelectProjectFolder(Some(f)) if state.projects_tab == ProjectsTab::Folder => {
            let mut fx = flush_generate(state);
            state.proj_folder = Some(f.clone());
            state.proj_scope_kept = None;
            state.project = None;
            fx.extend(fv_go(state, std::path::Path::new("projects").join(f), true));
            fx
        }
        Msg::SelectProjectFolder(f) => {
            let fx = flush_generate(state);
            state.proj_folder = f;
            state.proj_scope_kept = None;
            state.project = None;
            fx
        }
        Msg::Generate(f) => {
            if f != state.generate {
                state.generate = f;
                mark_generate(state);
            }
            vec![]
        }
        Msg::SelectRecipe(i) => {
            if let Some(p) = state.config.recipes.get(i).cloned() {
                state.generate.apply_preset(i, &p);
                mark_generate(state);
            }
            vec![]
        }
        Msg::RefPicker(true) => {
            let source = state.ref_picker_source.clone().unwrap_or_else(|| {
                let inputs =
                    state.index.ideas.iter().any(|e| matches!(e.loc, IdeaLoc::Library(_)) && !e.idea.images.is_empty());
                if inputs { RefSource::Inputs(None) } else { RefSource::Outputs(OutputSource::AllProjects) }
            });
            state.ref_picker = Some(RefPicker { source, text: String::new(), ratings: Ratings::KEPT, chosen: vec![] });
            vec![]
        }
        Msg::RefPicker(false) => {
            state.ref_picker = None;
            vec![]
        }
        Msg::RefPickerSource(s) => {
            if let Some(p) = &mut state.ref_picker {
                p.source = s.clone();
                state.ref_picker_source = Some(s);
            }
            vec![]
        }
        Msg::RefPickerText(t) => {
            if let Some(p) = &mut state.ref_picker {
                p.text = t;
            }
            vec![]
        }
        Msg::RefPickerRatings(r) => {
            if let Some(p) = &mut state.ref_picker {
                p.ratings = r;
            }
            vec![]
        }
        Msg::RefPickerToggle(img) => {
            let refs = state.generate.refs();
            if let Some(p) = &mut state.ref_picker
                && !refs.contains(&img)
            {
                match p.chosen.iter().position(|c| *c == img) {
                    Some(i) => {
                        p.chosen.remove(i);
                    }
                    None => p.chosen.push(img),
                }
            }
            vec![]
        }
        Msg::RefPickerAdd => {
            let chosen = state.ref_picker.take().map(|p| p.chosen).unwrap_or_default();
            update(state, Msg::AddRefs(chosen))
        }
        Msg::AddRefs(refs) => {
            let before = state.generate.refs();
            for r in refs {
                state.generate.add_reference(r);
            }
            images_changed(state, before)
        }
        Msg::RemoveImage(i) => {
            let before = state.generate.refs();
            if i < state.generate.images.len() {
                state.generate.images.remove(i);
            }
            images_changed(state, before)
        }
        Msg::ImageRoles { index, reference, start } => {
            let before = state.generate.refs();
            let images = &mut state.generate.images;
            if index < images.len() {
                if start {
                    // at most one start image
                    for (j, x) in images.iter_mut().enumerate() {
                        x.start = j == index;
                    }
                } else {
                    images[index].start = false;
                }
                images[index].reference = reference;
            }
            images_changed(state, before)
        }
        Msg::MoveImage { from, to } => {
            let before = state.generate.refs();
            let images = &mut state.generate.images;
            if from < images.len() && to < images.len() {
                let x = images.remove(from);
                images.insert(to, x);
            }
            images_changed(state, before)
        }
        Msg::RetagConfirm => {
            let Some(r) = state.retag.take() else { return vec![] };
            let mut fx = vec![];
            if let Some(p) = state.project.clone() {
                let (t, x) = (state.generate.prompt.clone(), state.generate.negative.clone());
                fx = record(state, HistoryKey::Project(p), HistoryKind::BeforeRetag, t, x);
            }
            state.generate.prompt = r.prompt;
            mark_generate(state);
            fx
        }
        Msg::RetagDismiss => {
            state.retag = None;
            vec![]
        }
        Msg::ClearGenerate => {
            let mut fx = vec![];
            if let Some(p) = state.project.clone() {
                let (t, x) = (state.generate.prompt.clone(), state.generate.negative.clone());
                fx = record(state, HistoryKey::Project(p), HistoryKind::BeforeClear, t, x);
            }
            let mut f = GenerateForm::new(&state.config);
            f.label = next_label(state);
            f.first_seed = state.next_seed();
            state.generate = f;
            mark_generate(state);
            fx
        }
        Msg::UseImage(image, target) => use_image(state, image, target),
        Msg::ToInputLibrary(image) => {
            lib(state, LibOp::IdeaFromOutput { image: image.id, with_prompt: false, loc: IdeaLoc::top_library() })
        }
        Msg::NewIdeaFromImage(image) => {
            lib(state, LibOp::IdeaFromOutput { image: image.id, with_prompt: true, loc: IdeaLoc::Unreviewed })
        }
        Msg::ClearInit => {
            // the image stays in the list (as a reference, or parked)
            for x in &mut state.generate.images {
                x.start = false;
            }
            mark_generate(state);
            vec![]
        }
        Msg::DuplicateProject(path) => {
            let base = path.file_name().unwrap_or_default().to_string_lossy();
            let name = std::iter::once(format!("{base} copy"))
                .chain((2..).map(|n| format!("{base} copy {n}")))
                .find(|n| !state.project_name_taken(&path, n))
                .unwrap_or_default();
            state.duplicate = Some(DuplicateDialog { path, name });
            vec![]
        }
        Msg::EditDuplicateName(n) => {
            if let Some(d) = &mut state.duplicate {
                d.name = n;
            }
            vec![]
        }
        Msg::ConfirmDuplicate => {
            let Some(d) = state.duplicate.take() else { return vec![] };
            let name = d.name.trim().to_string();
            let problem = increart_core::library::validate_name(&name).err().map(|e| e.to_string());
            if let Some(e) = problem
                .or_else(|| state.project_name_taken(&d.path, &name).then(|| format!("{name} already exists here")))
            {
                state.messages_error(e);
                state.duplicate = Some(d);
                return vec![];
            }
            lib(state, LibOp::CopyProject(d.path, name))
        }
        Msg::CancelDuplicate => {
            state.duplicate = None;
            vec![]
        }
        Msg::MoreSeeds(run, n) => {
            if n == 0 {
                return vec![];
            }
            lib(state, LibOp::AddSeeds(run, n))
        }
        // ------------------------------------------------ prompt presets
        Msg::PresetsPanel(open) => {
            let favourites = state.prompt_presets.tasks.iter().flat_map(|t| &t.presets).any(|p| p.favourite);
            let sw = &state.generate.sweep;
            let view = if sw.on && sw.vary == increart_core::prompts::SweepVary::Style && !sw.selected.is_empty() {
                PresetsView::Ticked(sw.selected.clone())
            } else if favourites {
                PresetsView::Favourites
            } else {
                PresetsView::Task(0)
            };
            state.presets_panel = open.then(|| PresetsPanel { view, search: String::new() });
            vec![]
        }
        Msg::PresetsShow(view) => {
            let view = match view {
                PresetsView::Ticked(_) => PresetsView::Ticked(state.generate.sweep.selected.clone()),
                v => v,
            };
            if let Some(p) = &mut state.presets_panel {
                p.view = view;
            }
            vec![]
        }
        Msg::PresetsSearch(q) => {
            if let Some(p) = &mut state.presets_panel {
                p.search = q;
            }
            vec![]
        }
        Msg::ApplyPreset { task, preset, insert_at } => {
            let Some(p) = state.prompt_presets.tasks.get(task).and_then(|t| t.presets.get(preset)).cloned() else {
                return vec![];
            };
            let mut fx = vec![];
            // the bytes of the prompt that are the preset's
            let at = match insert_at {
                Some(c) => {
                    let prompt = &mut state.generate.prompt;
                    let b = prompt.char_indices().nth(c).map_or(prompt.len(), |(i, _)| i);
                    let before = if b > 0 && !prompt[..b].ends_with(char::is_whitespace) { " " } else { "" };
                    let after =
                        if b < prompt.len() && !prompt[b..].starts_with(char::is_whitespace) { " " } else { "" };
                    prompt.insert_str(b, &format!("{before}{}{after}", p.prompt));
                    b + before.len()..b + before.len() + p.prompt.len()
                }
                None => {
                    let f = &state.generate;
                    if let Some(project) = state.project.clone()
                        && !f.prompt.trim().is_empty()
                        && f.prompt != p.prompt
                    {
                        let (t, x) = (f.prompt.clone(), f.negative.clone());
                        fx = record(state, HistoryKey::Project(project), HistoryKind::BeforePreset, t, x);
                    }
                    state.generate.prompt = p.prompt.clone();
                    if !p.negative.is_empty() {
                        state.generate.negative = p.negative.clone();
                    }
                    0..p.prompt.len()
                }
            };
            // its first {slot} selected for typing over, else the cursor after it
            let prompt = &state.generate.prompt;
            let sel = increart_core::prompts::slots(&prompt[at.clone()])
                .first()
                .map_or(at.end..at.end, |r| at.start + r.start..at.start + r.end);
            let chars = |b: usize| prompt[..b].chars().count();
            state.prompt_select = Some(chars(sel.start)..chars(sel.end));
            mark_generate(state);
            fx
        }
        Msg::PromptSelected => {
            state.prompt_select = None;
            vec![]
        }
        Msg::SweepToggle(t, p) => {
            let Some((task, preset)) = state.prompt_presets.tasks.get(t).and_then(|x| Some((x, x.presets.get(p)?)))
            else {
                return vec![];
            };
            let key = increart_core::prompts::PresetKey { task: task.name.clone(), name: preset.name.clone() };
            update(state, Msg::SweepToggleKey(key))
        }
        Msg::SweepToggleKey(key) => {
            let sw = &mut state.generate.sweep;
            match sw.selected.iter().position(|k| *k == key) {
                Some(i) => {
                    sw.selected.remove(i);
                }
                None => sw.selected.push(key),
            }
            sw.source = increart_core::prompts::SweepSource::Selected;
            mark_generate(state);
            vec![]
        }
        Msg::SweepTicks(presets, on) => {
            let keys = presets.into_iter().filter_map(|(t, p)| {
                let task = state.prompt_presets.tasks.get(t)?;
                Some(increart_core::prompts::PresetKey {
                    task: task.name.clone(),
                    name: task.presets.get(p)?.name.clone(),
                })
            });
            let keys: Vec<_> = keys.collect();
            let sw = &mut state.generate.sweep;
            if on {
                for k in keys {
                    if !sw.selected.contains(&k) {
                        sw.selected.push(k);
                    }
                }
            } else {
                sw.selected.retain(|k| !keys.contains(k));
            }
            sw.source = increart_core::prompts::SweepSource::Selected;
            mark_generate(state);
            vec![]
        }
        Msg::SweepClearTicks => {
            state.generate.sweep.selected.clear();
            mark_generate(state);
            vec![]
        }
        Msg::PresetFavourite(t, p) => {
            let Some(x) = state.prompt_presets.tasks.get_mut(t).and_then(|t| t.presets.get_mut(p)) else {
                return vec![];
            };
            x.favourite = !x.favourite;
            save_presets(state)
        }
        Msg::OpenPresetEditor(task, preset) => {
            let list = state.prompt_presets.clone();
            let task = task.min(list.tasks.len().saturating_sub(1));
            state.presets_editor = Some(PresetsEditor { list, task, preset, error: None });
            vec![]
        }
        Msg::EditPresetEditor(e) => {
            if state.presets_editor.is_some() {
                state.presets_editor = Some(e);
            }
            vec![]
        }
        Msg::RestorePresetDefaults => {
            if let Some(e) = &mut state.presets_editor {
                *e = PresetsEditor { list: increart_core::prompts::builtin(), task: 0, preset: None, error: None };
            }
            vec![]
        }
        Msg::SavePresetEditor => {
            let Some(e) = &mut state.presets_editor else { return vec![] };
            if let Err(err) = check_presets(&e.list) {
                e.error = Some(err);
                return vec![];
            }
            state.prompt_presets = state.presets_editor.take().unwrap().list.normalized();
            let n = state.prompt_presets.tasks.len();
            if let Some(p) = &mut state.presets_panel
                && p.view.task().is_some_and(|t| t >= n)
            {
                p.view = if n > 0 { PresetsView::Task(0) } else { PresetsView::Favourites };
            }
            save_presets(state)
        }
        Msg::ClosePresetEditor => {
            state.presets_editor = None;
            vec![]
        }
        Msg::OpenSavePreset => {
            let f = &state.generate;
            if f.prompt.trim().is_empty() {
                state.messages_error("The prompt is empty");
                return vec![];
            }
            let task = state.presets_panel.as_ref().and_then(|p| p.view.task()).unwrap_or(0);
            let preset = PromptPreset { prompt: f.prompt.clone(), negative: f.negative.clone(), ..Default::default() };
            state.save_preset = Some(SavePresetDialog { task, preset, error: None });
            vec![]
        }
        Msg::EditSavePreset(d) => {
            if state.save_preset.is_some() {
                state.save_preset = Some(d);
            }
            vec![]
        }
        Msg::ConfirmSavePreset => {
            let Some(d) = &mut state.save_preset else { return vec![] };
            let name = d.preset.name.trim().to_string();
            let Some(task) = state.prompt_presets.tasks.get(d.task) else {
                d.error = Some("Pick a task".into());
                return vec![];
            };
            if name.is_empty() {
                d.error = Some("Give it a name".into());
                return vec![];
            }
            if task.presets.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                d.error = Some(format!("{} already has a preset called {name}", task.name));
                return vec![];
            }
            let mut d = state.save_preset.take().unwrap();
            d.preset.name = name.clone();
            let task = &mut state.prompt_presets.tasks[d.task];
            task.presets.push(d.preset);
            let what = format!("Saved the preset {name} in {}", task.name);
            state.prompt_presets = state.prompt_presets.normalized();
            state.messages_info(what);
            save_presets(state)
        }
        Msg::CancelSavePreset => {
            state.save_preset = None;
            vec![]
        }
        Msg::OpenInGenerate(id) => {
            let fx = update(state, Msg::SelectProject(id));
            state.tab = Tab::Projects;
            state.projects_tab = ProjectsTab::Generate;
            fx
        }
        // ------------------------------------------------ prompt history
        Msg::OpenHistory(key) => {
            state.history_view = Some(HistoryView::new(key.clone()));
            vec![Effect::LoadHistory(key)]
        }
        Msg::HistoryLoaded(key, mut entries) => {
            let Some(mut v) = state.history_view.take() else { return vec![] };
            if v.key == key {
                entries.reverse();
                v.entries = entries;
                v.selected = 0;
                resolve_history_runs(state, &mut v);
                // Show in history: the run's checkpoint, among what led to it
                if let Some((run, _)) = v.run_filter.clone() {
                    v.run_filter = Some((run.clone(), run_before(state, &v, &run)));
                    if let Some(i) = (0..v.entries.len()).find(|i| v.runs_of(*i).contains(&run)) {
                        v.selected = i;
                    }
                }
            }
            state.history_view = Some(v);
            vec![]
        }
        Msg::HistorySettingsOnly(on) => {
            if let Some(v) = &mut state.history_view {
                v.settings_only = on;
            }
            vec![]
        }
        Msg::HistoryRunFilter(run) => {
            let Some(mut v) = state.history_view.take() else { return vec![] };
            v.run_filter = run.map(|r| (r.clone(), run_before(state, &v, &r)));
            state.history_view = Some(v);
            vec![]
        }
        Msg::RunHistory(run) => {
            let Some(p) = state.index.run(&run).map(|(p, _)| p.project.id.clone()) else { return vec![] };
            let fx = update(state, Msg::OpenHistory(HistoryKey::Project(p)));
            if let Some(v) = &mut state.history_view {
                v.run_filter = Some((run, None));
            }
            fx
        }
        Msg::HistorySelect(i) => {
            if let Some(v) = &mut state.history_view {
                v.selected = i;
            }
            vec![]
        }
        Msg::HistorySearch(q) => {
            if let Some(v) = &mut state.history_view {
                v.search = q;
            }
            vec![]
        }
        Msg::HistoryCheckpointsOnly(on) => {
            if let Some(v) = &mut state.history_view {
                v.checkpoints_only = on;
            }
            vec![]
        }
        Msg::HistoryEveryEntry(on) => {
            if let Some(v) = &mut state.history_view {
                v.every_entry = on;
            }
            vec![]
        }
        Msg::HistoryRestore(what) => {
            let Some(v) = &state.history_view else { return vec![] };
            let Some(e) = v.entries.get(v.selected).cloned() else { return vec![] };
            let key = v.key.clone();
            let with_extra = what != Restore::Text;
            let mut fx = vec![];
            match &key {
                HistoryKey::Project(p) if state.index.project(p).is_some() => {
                    // e.g. opened from a run in the Output library
                    if state.project.as_ref() != Some(p) || state.projects_tab != ProjectsTab::Generate {
                        fx.extend(update(state, Msg::OpenInGenerate(p.clone())));
                    }
                    let (t, x) = (state.generate.prompt.clone(), state.generate.negative.clone());
                    fx.extend(record(state, key.clone(), HistoryKind::BeforeRestore, t, x));
                    match (what, &e.form) {
                        (Restore::All, Some(d)) => restore_form(state, d),
                        _ => {
                            state.generate.prompt = e.text;
                            if with_extra {
                                state.generate.negative = e.extra;
                            }
                        }
                    }
                    mark_generate(state);
                }
                HistoryKey::NewIdea => {
                    let (t, x) = (state.draft.text.clone(), state.draft.title.clone());
                    fx.extend(record(state, key.clone(), HistoryKind::BeforeRestore, t, x));
                    state.draft.text = e.text;
                    if with_extra {
                        state.draft.title = e.extra;
                    }
                    state.idea_dirty = Some(state.now);
                }
                HistoryKey::Idea(id) if state.idea_edit.as_ref().is_some_and(|ed| &ed.id == id) => {
                    let ed = state.idea_edit.as_mut().unwrap();
                    let (t, x) = (ed.text.clone(), ed.title.clone());
                    ed.text = e.text;
                    if with_extra {
                        ed.title = e.extra;
                    }
                    fx.extend(record(state, key.clone(), HistoryKind::BeforeRestore, t, x));
                }
                _ => {
                    state.messages_error("Open what this history belongs to first");
                    return vec![];
                }
            }
            state.history_view = None;
            fx
        }
        Msg::CloseHistory => {
            state.history_view = None;
            vec![]
        }
        // ------------------------------------------------ Edit idea
        Msg::OpenIdea(id) => {
            let fx = flush_idea_edit(state);
            if let Some(i) = state.idea(&id).map(|e| e.idea.clone()) {
                state.idea_edit = Some(IdeaEdit {
                    id: id.clone(),
                    title: i.title.clone(),
                    text: i.text.clone(),
                    tags: i.tags.join(", "),
                    images: i.images.clone(),
                });
                // so the first pause records against what was there
                let (t, x) = (i.text.clone(), i.title.clone());
                state.history_last.entry(HistoryKey::Idea(id)).or_insert(LastEntry {
                    at: f64::NEG_INFINITY,
                    entry: HistoryEntry::text(Default::default(), HistoryKind::Saved, t, x),
                });
            }
            fx
        }
        Msg::EditIdea(ed) => {
            if state.idea_edit.as_ref() != Some(&ed) {
                state.idea_edit = Some(ed);
                state.idea_edit_dirty = Some(state.now);
            }
            vec![]
        }
        Msg::SaveIdea => {
            let Some(ed) = state.idea_edit.clone() else { return vec![] };
            state.idea_edit_dirty = None;
            let tags = IdeaDraft { tags: ed.tags.clone(), ..Default::default() }.tag_list();
            let mut fx =
                record(state, HistoryKey::Idea(ed.id.clone()), HistoryKind::Saved, ed.text.clone(), ed.title.clone());
            state.idea_edit = None;
            fx.extend(lib(
                state,
                LibOp::SaveIdea { id: ed.id, title: ed.title, text: ed.text, tags, images: ed.images },
            ));
            fx
        }
        Msg::CloseIdea => {
            let fx = flush_idea_edit(state);
            state.idea_edit = None;
            fx
        }
        // ------------------------------------------------ input library
        Msg::LibTab(t) => {
            state.lib_tab = t;
            fv_sync(state);
            relist_if_stale(state)
        }
        Msg::LibFilter(f) => {
            state.lib_filter = f;
            vec![]
        }
        // ------------------------------------------------ Import tab
        Msg::LoadImports => vec![Effect::LoadImports],
        Msg::Imports(list) => {
            state.imports = Some(list);
            vec![]
        }
        Msg::ImportDropNow => lib(state, LibOp::ImportInboxDrop),
        // ------------------------------------------------ resolutions
        Msg::OpenResolutions => {
            state.res_editor = Some(state.config.resolutions.clone());
            vec![]
        }
        Msg::EditResolutions(list) => {
            state.res_editor = Some(list);
            vec![]
        }
        Msg::SaveResolutions => {
            let Some(list) = state.res_editor.take() else { return vec![] };
            state.config.resolutions = list;
            for w in state.config.normalize() {
                state.messages_info(w);
            }
            save_config(state)
        }
        Msg::CloseResolutions => {
            state.res_editor = None;
            vec![]
        }
        Msg::OpenScale(target) => {
            let size = match target {
                ScaleTarget::Generate => Some((state.generate.width, state.generate.height)),
                ScaleTarget::Resolution(i) => {
                    state.res_editor.as_ref().and_then(|l| l.get(i)).map(|r| (r.width, r.height))
                }
            };
            state.scale = size.map(|(w, h)| ScaleDialog::new(target, w, h));
            vec![]
        }
        Msg::EditScale(d) => {
            state.scale = Some(d);
            vec![]
        }
        Msg::ApplyScale => {
            let Some(d) = state.scale.take() else { return vec![] };
            let (w, h) = d.result();
            match d.target {
                ScaleTarget::Generate => {
                    (state.generate.width, state.generate.height) = (w, h);
                    mark_generate(state);
                }
                ScaleTarget::Resolution(i) => {
                    if let Some(r) = state.res_editor.as_mut().and_then(|l| l.get_mut(i)) {
                        (r.width, r.height) = (w, h);
                    }
                }
            }
            vec![]
        }
        Msg::CloseScale => {
            state.scale = None;
            vec![]
        }
        Msg::EditNewProjectName(n) => {
            if let Some(d) = &mut state.new_project {
                d.name = n;
            }
            vec![]
        }
        Msg::ConfirmNewProject => {
            let Some(d) = state.new_project.take() else { return vec![] };
            let name = d.name.trim().to_string();
            if let Err(e) = increart_core::library::validate_name(&name) {
                state.messages_error(e.to_string());
                state.new_project = Some(d);
                return vec![];
            }
            prepare_project(state, d.image, d.target, name)
        }
        Msg::CancelNewProject => {
            state.new_project = None;
            vec![]
        }
        Msg::Preview(image) => {
            state.preview = image;
            vec![]
        }
        Msg::PreviewStep(d) => {
            let Some((i, n)) = state.preview_position() else { return vec![] };
            let j = (i as isize + d).clamp(0, n as isize - 1) as usize;
            let img = state.preview_list().and_then(|l| l.get(j).map(|o| o.image.clone()));
            if j == i || img.is_none() {
                return vec![];
            }
            state.preview = img.clone();
            // the selection follows, so closing leaves you on this image
            if state.tab == Tab::Review && state.review_view == ReviewView::Sheet {
                let at = state.sheet().and_then(|sh| {
                    sh.rows.iter().enumerate().find_map(|(r, row)| {
                        let c = row
                            .cells
                            .iter()
                            .position(|c| matches!(c, SheetCell::Image(o) if Some(&o.image) == img.as_ref()))?;
                        Some((r, c))
                    })
                });
                if let Some(at) = at {
                    state.sheet_cell = at;
                }
                vec![]
            } else if state.tab == Tab::Review {
                state.review_cursor = j;
                vec![]
            } else {
                update(state, Msg::OutputSelect(img))
            }
        }
        Msg::PreviewRow(d) => {
            if state.tab != Tab::Review || state.review_view != ReviewView::Sheet || d == 0 {
                return vec![];
            }
            let Some(sh) = state.sheet() else { return vec![] };
            let image = |r: usize, c: usize| match sh.rows.get(r)?.cells.get(c)? {
                SheetCell::Image(o) => Some(o.image.clone()),
                _ => None,
            };
            let at = sh.rows.iter().enumerate().find_map(|(r, row)| {
                let c = row
                    .cells
                    .iter()
                    .position(|c| matches!(c, SheetCell::Image(o) if Some(&o.image) == state.preview.as_ref()))?;
                Some((r, c))
            });
            let Some((r, c)) = at else { return vec![] };
            let mut to = r as isize + d.signum();
            while to >= 0 && (to as usize) < sh.rows.len() {
                if let Some(img) = image(to as usize, c) {
                    state.preview = Some(img);
                    state.sheet_cell = (to as usize, c);
                    break;
                }
                to += d.signum();
            }
            vec![]
        }
        Msg::PreviewRate(v) => {
            let Some((i, n)) = state.preview_position() else { return vec![] };
            let Some(list) = state.preview_list() else { return vec![] };
            let link = list[i].path.clone();
            let queued = state.tab == Tab::Review && matches!(state.review_view, ReviewView::Single | ReviewView::Grid);
            // the queue's next image once this one has left it
            let next = if i + 1 < n { Some(i + 1) } else { i.checked_sub(1) };
            let next = next.map(|j| list[j].image.clone());
            let fx = lib(state, LibOp::ReviewOutput { link, verdict: Some(v), undo: false });
            if queued {
                state.review_cursor = if i + 1 < n { i } else { i.saturating_sub(1) };
                state.preview = next;
            } else if state.tab == Tab::Review {
                update(state, Msg::PreviewStep(1));
            }
            fx
        }
        Msg::JobInfo(run) => {
            state.job_info = run.filter(|r| state.index.run(r).is_some());
            vec![]
        }
        // ------------------------------------------------ review / output library
        Msg::NeedRecipe(id) => {
            if state.recipes.contains_key(&id) {
                return vec![];
            }
            state.recipes.insert(id.clone(), None);
            vec![Effect::LoadRecipe(id)]
        }
        Msg::RecipeLoaded(id, r) => {
            state.recipes.insert(id, r);
            vec![]
        }
        Msg::NeedInputMeta(ids) => {
            let ids: Vec<Id> = ids.into_iter().filter(|id| !state.input_meta.contains_key(id)).collect();
            if ids.is_empty() {
                return vec![];
            }
            for id in &ids {
                state.input_meta.insert(id.clone(), None);
            }
            vec![Effect::LoadInputMeta(ids)]
        }
        Msg::InputMetaLoaded(list) => {
            for (id, m) in list {
                state.input_meta.insert(id, Some(m));
            }
            vec![]
        }
        Msg::GoToOutput(image) => {
            let Some(link) = state.output_link(&image).cloned() else { return vec![] };
            go_to_output(state, link)
        }
        Msg::Checkerboard(on) => {
            state.checkerboard = on;
            vec![]
        }
        Msg::ReviewNewest => {
            state.review_cursor = state.review_queue().len().saturating_sub(1);
            vec![]
        }
        Msg::OutputFilter(f) => {
            state.output_filter = f;
            vec![]
        }
        Msg::OutputSearchProjects(on) => {
            if on != state.output_search_projects {
                state.output_search_projects = on;
                if on {
                    state.output_project_search = std::mem::take(&mut state.output_filter.text);
                } else {
                    state.output_filter.text = std::mem::take(&mut state.output_project_search);
                }
            }
            vec![]
        }
        Msg::OutputProjectSearch(q) => {
            state.output_project_search = q;
            vec![]
        }
        Msg::OutputPreset(p) => {
            state.output_preset = p;
            state.output_filter = p.filter();
            vec![]
        }
        Msg::ShowOutputs(source) => {
            let mut fx = update(state, Msg::Tab(Tab::OutputLibrary));
            fx.extend(update(state, Msg::OutputTab(OutputTab::Grid)));
            fx.extend(update(state, Msg::OutputSource(source)));
            // its kept images: the Kept tab, the other filters kept (the notice offers a reset)
            state.output_preset = OutputPreset::Kept;
            state.output_filter.ratings = Ratings::KEPT;
            state.output_notice = true;
            // the panel shows it in "by project"; a project-name search that would hide it goes
            if !state.project_search_shows(&state.output_source) {
                state.output_project_search.clear();
            }
            state.output_reveal = true;
            state.output_reveals += 1;
            fx
        }
        Msg::OutputRevealed => {
            state.output_reveal = false;
            vec![]
        }
        Msg::ResetOutputFilters => {
            state.output_filter = state.output_preset.filter();
            vec![]
        }
        Msg::DismissOutputNotice => {
            state.output_notice = false;
            vec![]
        }
        Msg::OutputSelect(img) => {
            let fx = img.as_ref().map(|i| update(state, Msg::NeedRecipe(i.id.clone()))).unwrap_or_default();
            state.output_selected = img;
            fx
        }
        Msg::AddToCollection(folder, images) => lib(state, LibOp::AddToCollection(folder, images)),
        Msg::NewCollection(parent, images) => lib(state, LibOp::NewCollection(parent, images)),
        Msg::SetCover(p, img) => lib(state, LibOp::SetCover(p, img)),
        Msg::TitleDialog(Some(image)) => {
            let text = state.image_title(&image).unwrap_or_default().to_string();
            state.title_edit = Some(TitleEdit { image, text });
            vec![]
        }
        Msg::TitleDialog(None) => {
            state.title_edit = None;
            vec![]
        }
        Msg::EditTitle(t) => {
            if let Some(e) = &mut state.title_edit {
                e.text = t;
            }
            vec![]
        }
        Msg::CommitTitle => {
            let Some(e) = state.title_edit.take() else { return vec![] };
            let project =
                state.output_link(&e.image).and_then(|o| state.project_of_link(o)).map(|p| p.project.id.clone());
            match project {
                Some(p) => lib(state, LibOp::SetTitle(p, e.image.id, e.text)),
                None => {
                    state.messages_error("That image isn't in a project any more");
                    vec![]
                }
            }
        }
        Msg::PickImage(image, how) => {
            pick_image(state, image, how);
            vec![]
        }
        Msg::PickSet(images) => {
            state.picked = images;
            vec![]
        }
        Msg::RatePicked(v) => {
            let links: Vec<PathBuf> =
                state.picked.iter().filter_map(|i| state.output_link(i)).map(|o| o.path.clone()).collect();
            // rated images leave Review's queue (the sheet keeps them)
            if state.tab == Tab::Review && state.review_view != ReviewView::Sheet {
                state.picked.clear();
            }
            links
                .into_iter()
                .flat_map(|link| lib(state, LibOp::ReviewOutput { link, verdict: Some(v), undo: false }))
                .collect()
        }
        Msg::OpenTile => {
            if state.picked.is_empty() {
                return vec![];
            }
            let picked = state.picked.clone();
            let cells = state.sheet_cells();
            let on_sheet = state.tab == Tab::Review && state.review_view == ReviewView::Sheet;
            let layout = if on_sheet && picked.iter().all(|i| cells.contains_key(i)) {
                crate::tile::TileLayout::at(picked.into_iter().map(|i| (i.clone(), cells[&i].0, cells[&i].1)).collect())
            } else {
                crate::tile::TileLayout::square(picked)
            };
            state.tile = Some(TileDialog { layout, selected: None, working: 0 });
            vec![]
        }
        Msg::CloseTile => {
            state.tile = None;
            vec![]
        }
        Msg::TileSelect(i) => {
            if let Some(t) = &mut state.tile {
                t.selected = i.filter(|&i| i < t.layout.tiles.len());
            }
            vec![]
        }
        Msg::TileMove(i, c, r) => {
            if let Some(t) = &mut state.tile {
                t.layout.move_tile(i, c, r);
                t.selected = Some(i);
            }
            vec![]
        }
        Msg::TileSpan(i, w, h) => {
            if let Some(t) = &mut state.tile {
                t.layout.set_span(i, w, h);
            }
            vec![]
        }
        Msg::TileCols(n) => {
            if let Some(t) = &mut state.tile {
                t.layout.set_cols(n);
            }
            vec![]
        }
        Msg::TileReflow => {
            if let Some(t) = &mut state.tile {
                t.layout.reflow();
            }
            vec![]
        }
        Msg::TileRemove(i) => {
            if let Some(t) = &mut state.tile {
                t.layout.remove(i);
                t.selected = None;
                if t.layout.tiles.is_empty() {
                    state.tile = None;
                }
            }
            vec![]
        }
        Msg::TileCaption(i, text) => {
            if let Some(tile) = state.tile.as_mut().and_then(|t| t.layout.tiles.get_mut(i)) {
                tile.caption = text;
            }
            vec![]
        }
        Msg::TileAddText(c, r) => {
            if let Some(t) = &mut state.tile {
                t.selected = Some(t.layout.add_text(c, r, "Text".into()));
            }
            vec![]
        }
        Msg::TileText(i, text) => {
            if let Some(Tile { content: TileContent::Text { text: t, .. }, .. }) =
                state.tile.as_mut().and_then(|t| t.layout.tiles.get_mut(i))
            {
                *t = text;
            }
            vec![]
        }
        Msg::TileTextSize(i, px) => {
            if let Some(Tile { content: TileContent::Text { size, .. }, .. }) =
                state.tile.as_mut().and_then(|t| t.layout.tiles.get_mut(i))
            {
                *size = px;
            }
            vec![]
        }
        Msg::TileTitle(title, subtitle) => {
            if let Some(t) = &mut state.tile {
                (t.layout.title, t.layout.subtitle) = (title, subtitle);
            }
            vec![]
        }
        Msg::TileCaptions(fill) => {
            let Some(t) = &state.tile else { return vec![] };
            // text blocks keep none
            let captions: Vec<String> =
                t.layout.tiles.iter().map(|x| x.image().map(|i| state.caption(i, fill)).unwrap_or_default()).collect();
            if let Some(t) = &mut state.tile {
                for (x, c) in t.layout.tiles.iter_mut().zip(captions) {
                    x.caption = c;
                }
            }
            vec![]
        }
        Msg::TileCaptionFormat(template) => {
            if let Some(t) = &mut state.tile {
                for x in t.layout.tiles.iter_mut().filter(|x| x.image().is_some()) {
                    x.caption = template.clone();
                }
            }
            vec![]
        }
        Msg::TileSettings(s) => {
            state.config.tile = s;
            save_config(state)
        }
        Msg::TileOut(to) => {
            let (Some(t), Some(root)) = (&state.tile, state.library.clone()) else { return vec![] };
            // the captions' placeholders filled
            let mut layout = t.layout.clone();
            for (i, x) in layout.tiles.iter_mut().enumerate() {
                x.caption = state.caption_text(&x.caption, x.image(), i + 1);
            }
            if let Some(t) = &mut state.tile {
                t.working += 1;
            }
            state.busy += 1;
            vec![Effect::ComposeTiles { root, layout, settings: state.config.tile.clone(), to }]
        }
        Msg::TilesDone(to, r) => {
            state.busy = state.busy.saturating_sub(1);
            if let Some(t) = &mut state.tile {
                t.working = t.working.saturating_sub(1);
            }
            let image = match r {
                Ok(i) => i,
                Err(e) => {
                    state.messages_error(format!("Tile: {e}"));
                    return vec![];
                }
            };
            match to {
                TileTarget::Clipboard => {
                    state.clipboard_image = Some(image);
                    state.messages_info("Tiled image copied to the clipboard");
                    vec![]
                }
                TileTarget::File(path) => {
                    state.messages_info(format!("Tiled image saved to {}", path.display()));
                    vec![]
                }
                TileTarget::Library => {
                    let sources = state.tile.iter().flat_map(|t| t.layout.images()).cloned().collect();
                    lib(state, LibOp::IdeaFromTiles { image, sources })
                }
            }
        }
        Msg::OpenExport(images, what) => {
            let dest = state.export.as_ref().map(|e| e.dest.clone()).unwrap_or_default();
            state.export = Some(ExportDialog { images, what, dest, keep_metadata: true, by_id: false });
            vec![]
        }
        Msg::EditExport(e) => {
            state.export = Some(e);
            vec![]
        }
        Msg::RunExport => {
            let Some(e) = state.export.clone() else { return vec![] };
            if e.dest.trim().is_empty() {
                state.messages_error("Choose a destination folder");
                return vec![];
            }
            state.export = None;
            state.busy += 1;
            vec![Effect::Export {
                images: e.images,
                dest: e.dest.trim().into(),
                keep_metadata: e.keep_metadata,
                by_id: e.by_id,
            }]
        }
        Msg::CloseExport => {
            state.export = None;
            vec![]
        }
        Msg::Exported(r) => {
            state.busy = state.busy.saturating_sub(1);
            match r {
                Ok(n) => state.messages_info(format!("Exported {n} image(s)")),
                Err(e) => state.messages_error(format!("Export: {e}")),
            }
            vec![]
        }
        Msg::ShowInFileManager(path) => {
            let abs = match &state.library {
                Some(root) if path.is_relative() => root.join(&path),
                _ => path,
            };
            vec![Effect::ShowInFileManager { path: abs, command: state.config.file_manager.clone() }]
        }
        Msg::Tick(t) => {
            state.now = t;
            let mut fx = vec![];
            if state.generate_dirty.is_some_and(|since| t - since >= AUTOSAVE_DELAY) {
                fx.extend(flush_generate(state));
            }
            if state.idea_dirty.is_some_and(|since| t - since >= AUTOSAVE_DELAY) {
                fx.extend(flush_idea(state));
            }
            if state.idea_edit_dirty.is_some_and(|since| t - since >= AUTOSAVE_DELAY) {
                fx.extend(flush_idea_edit(state));
            }
            if t - state.alerts.checked >= 1.0 || t < state.alerts.checked {
                state.alerts.checked = t;
                fx.extend(check_alerts(state));
            }
            fx
        }
        Msg::FlushDrafts => {
            let mut fx = flush_generate(state);
            fx.extend(flush_idea(state));
            fx.extend(flush_idea_edit(state));
            fx
        }
        Msg::DraftSaved(Ok(())) => {
            state.saving = state.saving.saturating_sub(1);
            state.draft_saved = Some(state.now);
            vec![]
        }
        Msg::DraftSaved(Err(e)) => {
            state.saving = state.saving.saturating_sub(1);
            state.messages_error(format!("saving a draft: {e}"));
            vec![]
        }
        Msg::QueueGenerate if state.generate.sweep.on => update(state, Msg::QueueSweep(false)),
        Msg::QueueSweep(confirmed) => queue_sweep(state, confirmed),
        Msg::JobOrder(order) => {
            if state.config.job_order == order {
                return vec![];
            }
            state.config.job_order = order;
            let mut fx = save_config(state);
            fx.push(Effect::SetJobOrder(order));
            fx
        }
        Msg::SweepOrderAnswer(answer) => {
            let Some(big) = state.sweep_order_ask.take() else { return vec![] };
            let Some(spread) = answer else { return vec![] };
            let mut fx = if spread { update(state, Msg::JobOrder(Order::Spread)) } else { vec![] };
            state.sweep_order_asked = true;
            fx.extend(queue_sweep(state, big));
            state.sweep_order_asked = false;
            fx
        }
        Msg::QueueGenerate => {
            let Some(project) = state.project.clone() else {
                state.messages_error("Select a project first");
                return vec![];
            };
            let f = &state.generate;
            if f.prompt.trim().is_empty() {
                state.messages_error("The prompt is empty");
                return vec![];
            }
            if let Some(r) = increart_core::prompts::slots(&f.prompt).first().cloned() {
                state.messages_error(format!("Fill in {} in the prompt first", &f.prompt[r.clone()]));
                let chars = |b: usize| state.generate.prompt[..b].chars().count();
                state.prompt_select = Some(chars(r.start)..chars(r.end));
                return vec![];
            }
            if !f.width.is_multiple_of(32) || !f.height.is_multiple_of(32) || f.seeds == 0 {
                state.messages_error("Size must be a multiple of 32 and seeds at least 1");
                return vec![];
            }
            let Some(preset) = state.config.recipes.get(f.preset).cloned() else {
                state.messages_error("No model preset");
                return vec![];
            };
            // once per session: the run would only wait in the queue
            if state.servers.is_empty() && state.background.is_none() && !state.no_server_warned {
                state.no_server_warned = true;
                state.no_server_warning = true;
                return vec![];
            }
            let queued = Box::new(Queued { project: project.clone(), form: Box::new(f.to_draft(&state.config)) });
            let op = LibOp::CreateRun {
                project,
                form: f.clone(),
                preset,
                model_dir: state.config.sdserver.model_dirs.first().cloned(),
                queue: state.engine_running,
                queued,
            };
            if !state.engine_running {
                state.messages_info("Run saved; it will be generated once servers are connected");
            }
            lib(state, op)
        }
        Msg::CancelRun(id, how) => {
            if how == Stop::Finish {
                state.finishing.insert(id.clone());
            } else {
                state.finishing.remove(&id);
            }
            vec![Effect::CancelRun(id, how)]
        }
        Msg::StopAll(how) => {
            let runs: Vec<Id> = state.active_runs.iter().cloned().collect();
            runs.into_iter().flat_map(|id| update(state, Msg::CancelRun(id, how))).collect()
        }

        // ------------------------------------------------ review
        Msg::ReviewScopeFolder(f) => {
            leave_sweep(state);
            state.review_scope = None;
            state.review_folder = Some(f);
            state.review_cursor = 0;
            vec![]
        }
        Msg::ReviewScope(s) => {
            leave_sweep(state);
            state.review_folder = None;
            state.review_scope = s;
            state.review_cursor = 0;
            vec![]
        }
        Msg::ReviewSweep(project, name) => {
            // the image being looked at stays selected on the sheet
            let current = state.review_queue().get(state.review_cursor).map(|o| o.path.clone());
            if state.review_sweep.as_ref() != Some(&name) || state.review_scope.as_ref() != Some(&project) {
                state.sheet_filter.clear();
            }
            if state.review_sweep.as_ref() != Some(&name) || state.review_scope.as_ref() != Some(&project) {
                state.picked.clear();
            }
            state.review_folder = None;
            state.review_scope = Some(project);
            state.review_sweep = Some(name);
            state.review_run = None;
            state.review_cursor = 0;
            state.review_pinned = None;
            state.review_view = ReviewView::Sheet;
            state.sheet_cell = state
                .sheet()
                .and_then(|sh| {
                    sh.rows.iter().enumerate().find_map(|(r, row)| {
                        let c = row
                            .cells
                            .iter()
                            .position(|c| matches!(c, SheetCell::Image(o) if Some(&o.path) == current.as_ref()))?;
                        Some((r, c))
                    })
                })
                .unwrap_or((0, 0));
            vec![]
        }
        Msg::ReviewRun(project, run) => {
            leave_sweep(state);
            state.review_folder = None;
            state.review_scope = Some(project);
            state.review_run = Some(run);
            state.review_cursor = 0;
            vec![]
        }
        Msg::ReviewAllRuns(on) => {
            state.review_all_runs = on;
            vec![]
        }
        Msg::ReviewSearch(q) => {
            state.review_search = q;
            vec![]
        }
        Msg::ProjectRuns(on) => {
            state.config.hide_project_runs = !on;
            save_config(state)
        }
        Msg::ProjectsByName(on) => {
            state.config.projects_by_name = on;
            save_config(state)
        }
        Msg::NarrowToSiblings(on) => {
            state.config.project_click_keeps_scope = !on;
            save_config(state)
        }
        Msg::ShowInfo(panel, on) => {
            if on {
                state.info_hidden.remove(&panel);
            } else {
                state.info_hidden.insert(panel);
            }
            vec![]
        }
        Msg::ToggleProjectFolder(f) => {
            let collapse = !state.folder_collapsed(&f);
            set_collapsed(state, |c| {
                if collapse {
                    c.push(f);
                } else {
                    c.retain(|g| g != &f);
                }
            })
        }
        Msg::CollapseProjectFolders(collapse) => {
            let all = state.index.project_folders.clone();
            set_collapsed(state, |c| *c = if collapse { all } else { vec![] })
        }
        Msg::UiZoom { percent, save } => {
            let p = percent.clamp(UI_ZOOM_MIN, UI_ZOOM_MAX);
            state.config.ui_zoom = if p == 100 { 0 } else { p };
            if save { save_config(state) } else { vec![] }
        }
        Msg::UiZoomStep(up) => {
            let z = state.ui_zoom();
            let percent =
                if up { (z / UI_ZOOM_STEP + 1) * UI_ZOOM_STEP } else { (z - 1) / UI_ZOOM_STEP * UI_ZOOM_STEP };
            update(state, Msg::UiZoom { percent, save: true })
        }
        Msg::SheetZoom { size, save } => {
            state.config.sheet_cell = size.clamp(SHEET_CELL_MIN, SHEET_CELL_MAX);
            if save { save_config(state) } else { vec![] }
        }
        Msg::SheetFilter(f) => {
            state.sheet_filter = f;
            state.sheet_cell.0 = 0;
            vec![]
        }
        Msg::SheetCell(r, c) => {
            state.sheet_cell = (r, c);
            vec![]
        }
        Msg::SheetMove(dr, dc) => {
            if let Some(sh) = state.sheet() {
                let (rows, cols) = (sh.rows.len() as isize, sh.seeds.len() as isize);
                let (r, c) = state.sheet_cell;
                state.sheet_cell = (
                    (r as isize + dr).clamp(0, (rows - 1).max(0)) as usize,
                    (c as isize + dc).clamp(0, (cols - 1).max(0)) as usize,
                );
            }
            vec![]
        }
        Msg::SheetRate(v) => {
            let (r, c) = state.sheet_cell;
            let link = state.sheet().and_then(|sh| match sh.rows.get(r)?.cells.get(c)? {
                SheetCell::Image(o) => Some(o.path.clone()),
                _ => None,
            });
            let Some(link) = link else { return vec![] };
            // on to the next image in the row, as single review goes on
            let fx = lib(state, LibOp::ReviewOutput { link, verdict: Some(v), undo: false });
            update(state, Msg::SheetMove(0, 1));
            fx
        }
        Msg::SheetRow(r, v) => {
            let links: Vec<PathBuf> = state
                .sheet()
                .and_then(|sh| {
                    let row = sh.rows.get(r)?;
                    Some(
                        row.cells
                            .iter()
                            .filter_map(|c| match c {
                                SheetCell::Image(o) if o.verdict.is_none() => Some(o.path.clone()),
                                _ => None,
                            })
                            .collect(),
                    )
                })
                .unwrap_or_default();
            links
                .into_iter()
                .flat_map(|link| lib(state, LibOp::ReviewOutput { link, verdict: Some(v), undo: false }))
                .collect()
        }
        Msg::SheetMoreSeeds(n) => {
            let runs: Vec<Id> = state
                .sheet()
                .map(|sh| {
                    sh.rows
                        .iter()
                        .filter(|row| {
                            row.cells
                                .iter()
                                .any(|c| matches!(c, SheetCell::Image(o) if o.verdict == Some(Verdict::Good)))
                        })
                        .map(|row| row.run.id.clone())
                        .collect()
                })
                .unwrap_or_default();
            if runs.is_empty() {
                state.messages_error("No style has a Good image yet");
                return vec![];
            }
            runs.into_iter().flat_map(|r| update(state, Msg::MoreSeeds(r, n))).collect()
        }
        Msg::RunInGenerate(id) => {
            let Some((p, run)) = state.index.run(&id).map(|(p, r)| (p.project.id.clone(), r.clone())) else {
                return vec![];
            };
            let mut fx = update(state, Msg::OpenInGenerate(p.clone()));
            let f = &state.generate;
            if !f.prompt.trim().is_empty() {
                let (t, x) = (f.prompt.clone(), f.negative.clone());
                fx.extend(record(state, HistoryKey::Project(p), HistoryKind::BeforeRestore, t, x));
            }
            let mut f = GenerateForm::from_run(&run, &state.config);
            f.label = next_label(state);
            f.first_seed = state.next_seed();
            state.generate = f;
            mark_generate(state);
            fx
        }
        Msg::ReviewCursor(i) => {
            state.review_cursor = i.min(state.review_queue().len().saturating_sub(1));
            vec![]
        }
        Msg::ReviewNext(d) => {
            let n = state.review_queue().len();
            if n > 0 {
                state.review_cursor = (state.review_cursor as isize + d).clamp(0, n as isize - 1) as usize;
            }
            vec![]
        }
        Msg::ReviewCurrent(v) => {
            let Some(link) = state.review_queue().get(state.review_cursor).map(|o| o.path.clone()) else {
                return vec![];
            };
            lib(state, LibOp::ReviewOutput { link, verdict: Some(v), undo: false })
        }
        Msg::ReviewView(v) => {
            if v == ReviewView::Compare {
                return update(state, Msg::Compare);
            }
            if v == ReviewView::Sheet && state.review_sweep.is_none() {
                return vec![];
            }
            state.review_view = v;
            state.review_pinned = None;
            vec![]
        }
        Msg::ToggleReviewGrid => {
            let v = if state.review_view == ReviewView::Grid { ReviewView::Single } else { ReviewView::Grid };
            update(state, Msg::ReviewView(v))
        }
        Msg::Compare => {
            if state.review_view == ReviewView::Compare {
                state.review_view = ReviewView::Single;
                state.review_pinned = None;
                return vec![];
            }
            let queue = state.review_queue();
            let Some(cur) = queue.get(state.review_cursor).map(|o| o.image.clone()) else { return vec![] };
            let has_next = state.review_cursor + 1 < queue.len();
            state.review_pinned = Some(cur);
            state.review_view = ReviewView::Compare;
            if has_next {
                state.review_cursor += 1;
            }
            vec![]
        }
        Msg::SwapCompare => {
            let queue = state.review_queue();
            let Some(b) = queue.get(state.review_cursor).map(|o| o.image.clone()) else { return vec![] };
            let a_pos = state.review_pinned.as_ref().and_then(|a| queue.iter().position(|o| &o.image == a));
            if let Some(pos) = a_pos {
                state.review_cursor = pos;
                state.review_pinned = Some(b);
            }
            vec![]
        }
        Msg::ReviewPinned(v) => {
            let link = state.review_pinned.as_ref().and_then(|a| state.output_link(a)).map(|o| o.path.clone());
            match link {
                Some(link) => lib(state, LibOp::ReviewOutput { link, verdict: Some(v), undo: false }),
                None => vec![],
            }
        }
        Msg::OutputTab(t) => {
            state.output_tab = t;
            fv_sync(state);
            relist_if_stale(state)
        }
        Msg::OutputSource(s) => {
            // on the Folder tab, the folder view goes to the source too
            let path = source_path(state, &s);
            if s != state.output_source {
                state.picked.clear();
            }
            state.output_source = s;
            state.output_notice = false;
            if state.tab == Tab::OutputLibrary && state.output_tab == OutputTab::Folder {
                return fv_go(state, path, true);
            }
            vec![]
        }
        Msg::DeleteOutput(link) => lib(state, LibOp::DeleteOutputs(vec![link])),
        Msg::DeleteShownRejected => {
            let links: Vec<_> = state
                .output_source_images()
                .iter()
                .filter(|o| o.verdict == Some(Verdict::Bad))
                .map(|o| o.path.clone())
                .collect();
            if links.is_empty() {
                return vec![];
            }
            lib(state, LibOp::DeleteOutputs(links))
        }
        Msg::Ask(text, then) => {
            state.confirm = Some(Confirm { text, then });
            vec![]
        }
        Msg::ConfirmYes => match state.confirm.take() {
            Some(c) => update(state, *c.then),
            None => vec![],
        },
        Msg::ConfirmNo => {
            state.confirm = None;
            vec![]
        }
        Msg::ReviewLink(link, verdict) => lib(state, LibOp::ReviewOutput { link, verdict, undo: false }),
        Msg::UndoReview => match state.review_undo.pop() {
            Some((link, previous)) => {
                state.busy += 1;
                vec![Effect::Lib(LibOp::ReviewOutput { link, verdict: previous, undo: true })]
            }
            None => vec![],
        },

        // ------------------------------------------------ runtime reports
        Msg::Scanned(ix) => {
            set_index(state, ix);
            relist(state)
        }
        Msg::OpDone(result, ix) => {
            state.busy = state.busy.saturating_sub(1);
            if let Some(ix) = ix {
                set_index(state, ix);
            }
            let mut fx = relist(state);
            match result {
                Err(e) => state.messages_error(e),
                Ok(ok) => fx.extend(op_done(state, ok)),
            }
            fx
        }
        Msg::Engine(e) => engine_event(state, e),
        Msg::RunsQueued(runs) => {
            state.active_runs = runs.into_iter().collect();
            let active = &state.active_runs;
            state.finishing.retain(|r| active.contains(r));
            state.progress.clear();
            state.sampling_times.clear();
            vec![]
        }
        Msg::Pick(kind) => {
            let start = match kind {
                PickKind::LibraryFolder => match &state.phase {
                    Phase::Library(d) if !d.path.is_empty() => {
                        let p = PathBuf::from(&d.path);
                        Some(if p.is_dir() { p } else { p.parent().map(PathBuf::from).unwrap_or(p) })
                    }
                    _ => None,
                },
                PickKind::ModelDir => state.config.sdserver.model_dirs.first().map(PathBuf::from),
                PickKind::SdServerBinary => PathBuf::from(&state.config.sdserver.binary).parent().map(PathBuf::from),
                PickKind::ExportDir => state.export.as_ref().map(|e| PathBuf::from(&e.dest)),
                PickKind::ImportFiles | PickKind::IdeaImages | PickKind::GenerateRefs | PickKind::TileFile => None,
            };
            vec![Effect::Pick { kind, start }]
        }
        Msg::Picked(_, paths) if paths.is_empty() => vec![],
        Msg::Picked(kind, mut paths) => match kind {
            PickKind::LibraryFolder => {
                if let Phase::Library(d) = &mut state.phase {
                    d.path = paths.remove(0).to_string_lossy().into_owned();
                    d.error = None;
                }
                vec![]
            }
            PickKind::ImportFiles => update(state, Msg::DropFiles(paths)),
            PickKind::IdeaImages => lib(state, LibOp::ImportImages(paths)),
            PickKind::GenerateRefs => update(state, Msg::DropRefs(paths)),
            PickKind::TileFile => {
                let mut path = paths.remove(0);
                if path.extension().is_none() {
                    path.set_extension("png");
                }
                update(state, Msg::TileOut(TileTarget::File(path)))
            }
            PickKind::ExportDir => {
                if let Some(e) = &mut state.export {
                    e.dest = paths.remove(0).to_string_lossy().into_owned();
                }
                vec![]
            }
            PickKind::SdServerBinary => {
                let mut c = state.config.sdserver.clone();
                c.binary = paths.remove(0).to_string_lossy().into_owned();
                update(state, Msg::SetSdServer(c))
            }
            PickKind::ModelDir => {
                let mut c = state.config.sdserver.clone();
                let dir = paths.remove(0).to_string_lossy().into_owned();
                match c.model_dirs.first_mut() {
                    Some(first) => *first = dir,
                    None => c.model_dirs.push(dir),
                }
                update(state, Msg::SetSdServer(c))
            }
        },
        Msg::DropRefs(paths) => {
            if paths.is_empty() || state.project.is_none() {
                return vec![];
            }
            lib(state, LibOp::ImportRefs(paths))
        }
        Msg::RemoveDraftImage(i) => {
            if i < state.draft.images.len() {
                state.draft.images.remove(i);
                state.idea_dirty = Some(state.now);
            }
            vec![]
        }
        Msg::SetSdServer(c) => {
            if c == state.config.sdserver {
                return vec![];
            }
            state.config.sdserver = c;
            save_config(state)
        }
        // ------------------------------------------------ folder view
        Msg::FvGo(path) => fv_go(state, path, true),
        Msg::FvBack => match state.folder_view.back.pop() {
            Some(p) => {
                let cur = state.folder_view.path.clone();
                state.folder_view.forward.push(cur);
                fv_go(state, p, false)
            }
            None => vec![],
        },
        Msg::FvForward => match state.folder_view.forward.pop() {
            Some(p) => {
                let cur = state.folder_view.path.clone();
                state.folder_view.back.push(cur);
                fv_go(state, p, false)
            }
            None => vec![],
        },
        Msg::FvUp => {
            let fv = &state.folder_view;
            match fv.path.parent().filter(|p| p.starts_with(&fv.root)).map(PathBuf::from) {
                Some(p) => fv_go(state, p, true),
                None => vec![],
            }
        }
        Msg::FvRefresh => {
            let path = state.folder_view.path.clone();
            state.folder_view.listed = Some(path.clone());
            fv_list(state, path)
        }
        Msg::FvListed(path, mut entries) => {
            if path == std::path::Path::new("collections") && !entries.iter().any(|e| e.name == "smart") {
                let n = state.kept_outputs().len() as u64;
                let info = "smart folders (read-only)".to_string();
                entries.push(FvEntry { name: "smart".into(), kind: EntryKind::Smart, size: n, modified: None, info });
            }
            let smart = is_smart(&path);
            let fv = fv_for(state, &path);
            if fv.place == FvPlace::Inbox && path == std::path::Path::new("input") {
                entries.retain(|e| e.name != "library"); // the Input library tab's
            }
            if path == fv.path {
                // smart folders keep their own order (newest day first, …)
                if !smart {
                    entries.sort_by(|a, b| {
                        // smart/, then folders, then projects / runs / ideas, then files
                        let rank = |k: &EntryKind| match k {
                            EntryKind::Smart => 0,
                            EntryKind::Folder => 1,
                            k if k.is_dir() => 2,
                            _ => 3,
                        };
                        let key = |e: &FvEntry| (rank(&e.kind), e.name.to_lowercase());
                        key(a).cmp(&key(b))
                    });
                }
                fv.selected.retain(|n| entries.iter().any(|e| &e.name == n));
                fv.entries = entries;
                // Keep the old previews on a refresh until the new ones come.
                if fv.previews_for != path {
                    fv.previews.clear();
                    fv.previews_for = path.clone();
                }
                fv.previews_asked.clear();
                fv.listed = Some(path);
            }
            vec![]
        }
        Msg::DropInto { paths, dest, copy } => transfer(state, &paths, !copy, &dest),
        Msg::FvNewFolder | Msg::FvNewProject => {
            let fv = &state.folder_view;
            if is_smart(&fv.path) {
                state.messages_info("Smart folders are read-only: make a collection outside smart/");
                return vec![];
            }
            let Some(tree) = fv.tree() else {
                state.messages_info("The inbox has no folders of its own");
                return vec![];
            };
            let rel = fv.tree_rel(&fv.path);
            match msg {
                Msg::FvNewProject if tree == Tree::Projects => update(state, Msg::NewProject(rel)),
                _ => update(state, Msg::NewFolder(tree, rel)),
            }
        }
        Msg::FvWantPreviews(names) => {
            let fv = &mut state.folder_view;
            let mut names: Vec<String> = names.into_iter().filter(|n| fv.previews_asked.insert(n.clone())).collect();
            // the virtual smart folders: from the index
            let dir = fv.path.clone();
            let (smart, rest): (Vec<String>, Vec<String>) = names.drain(..).partition(|n| is_smart(&dir.join(n)));
            let found: Vec<(String, Vec<ImageRef>)> = smart
                .into_iter()
                .map(|n| {
                    let rel = dir.join(&n).strip_prefix(SMART_DIR).map(PathBuf::from).unwrap_or_default();
                    let imgs = state.smart_images(&rel).into_iter().take(3).map(|o| o.image.clone()).collect();
                    (n, imgs)
                })
                .collect();
            state.folder_view.previews.extend(found);
            let names = rest;
            let fv = &state.folder_view;
            match &state.library {
                Some(root) if !names.is_empty() => {
                    vec![Effect::FolderPreviews { root: root.clone(), dir: fv.path.clone(), names }]
                }
                _ => vec![],
            }
        }
        Msg::FvPreviews(dir, found) => {
            let fv = fv_for(state, &dir);
            if dir == fv.path {
                fv.previews.extend(found);
            }
            vec![]
        }
        Msg::FvSelectSet(names) => {
            state.folder_view.selected = names;
            vec![]
        }
        Msg::FvMoveCursor { by, extend } => {
            let fv = &mut state.folder_view;
            let names: Vec<String> = fv.shown().iter().map(|e| e.name.clone()).collect();
            if names.is_empty() {
                return vec![];
            }
            let at = fv.cursor.as_ref().or(fv.anchor.as_ref()).and_then(|c| names.iter().position(|n| n == c));
            let to = match at {
                Some(i) => (i as i64 + by as i64).clamp(0, names.len() as i64 - 1) as usize,
                None => 0, // nothing yet: the first entry
            };
            let name = names[to].clone();
            if extend {
                let a = fv.anchor.as_ref().and_then(|a| names.iter().position(|n| n == a)).unwrap_or(to);
                fv.selected = names[a.min(to)..=a.max(to)].iter().cloned().collect();
            } else {
                fv.selected = [name.clone()].into();
                fv.anchor = Some(name.clone());
            }
            fv.cursor = Some(name);
            vec![]
        }
        Msg::FvTypeAhead(text) => {
            let now = state.now;
            let fv = &mut state.folder_view;
            if now - fv.typeahead_at > 1.0 {
                fv.typeahead.clear();
            }
            fv.typeahead_at = now;
            fv.typeahead.push_str(&text.to_lowercase());
            let hit =
                fv.shown().iter().find(|e| e.label().to_lowercase().starts_with(&fv.typeahead)).map(|e| e.name.clone());
            if let Some(name) = hit {
                fv.selected = [name.clone()].into();
                fv.anchor = Some(name.clone());
                fv.cursor = Some(name);
            }
            vec![]
        }
        Msg::FvSelect(name, how) => {
            let fv = &mut state.folder_view;
            fv.cursor = Some(name.clone());
            match how {
                SelectHow::Replace => {
                    fv.selected = [name.clone()].into();
                    fv.anchor = Some(name);
                }
                SelectHow::Toggle => {
                    if !fv.selected.remove(&name) {
                        fv.selected.insert(name.clone());
                    }
                    fv.anchor = Some(name);
                }
                SelectHow::Range => {
                    let names: Vec<String> = fv.shown().iter().map(|e| e.name.clone()).collect();
                    let a = fv.anchor.as_ref().and_then(|a| names.iter().position(|n| n == a)).unwrap_or(0);
                    let b = names.iter().position(|n| n == &name).unwrap_or(a);
                    let (lo, hi) = (a.min(b), a.max(b));
                    fv.selected = names[lo..=hi].iter().cloned().collect();
                }
            }
            vec![]
        }
        Msg::FvSelectAll => {
            let fv = &mut state.folder_view;
            fv.selected = fv.shown().iter().map(|e| e.name.clone()).collect();
            vec![]
        }
        Msg::FvClearSelection => {
            state.folder_view.selected.clear();
            vec![]
        }
        Msg::FvMode(m) => {
            state.folder_view.mode = m;
            vec![]
        }
        Msg::FvIconSize(v) => {
            state.folder_view.icon_size = v.clamp(64.0, 256.0);
            vec![]
        }
        Msg::FvShowMeta(on) => {
            state.folder_view.show_meta = on;
            vec![]
        }
        Msg::FvPathEdit(t) => {
            state.folder_view.path_edit = t;
            vec![]
        }
        Msg::FvOpen(name) => fv_open(state, &name),
        Msg::FvCopy | Msg::FvCut => {
            let cut = msg == Msg::FvCut;
            let fv = &state.folder_view;
            let smart = is_smart(&fv.path);
            let mut paths = vec![];
            let mut skipped = false;
            for e in fv.shown().into_iter().filter(|e| fv.selected.contains(&e.name)) {
                let ok = match (fv.place, &e.kind) {
                    // smart images can be copied into a collection
                    _ if smart => !cut && matches!(e.kind, EntryKind::Image(_)),
                    (FvPlace::Collections, k) => matches!(k, EntryKind::Folder | EntryKind::Image(_)),
                    (FvPlace::InputLibrary, k) => matches!(k, EntryKind::Folder | EntryKind::Idea(_)),
                    (FvPlace::Inbox, _) => false,
                    (_, EntryKind::Folder | EntryKind::Project) => true,
                    (_, EntryKind::Image(_)) => cut,
                    _ => false,
                };
                if ok {
                    paths.push(fv.path.join(&e.name));
                } else {
                    skipped = true;
                }
            }
            if skipped {
                state.messages_info(match (fv.place, cut) {
                    _ if smart => "Smart folders are read-only: copy their images into a collection",
                    (FvPlace::Collections, _) => "Only collections and their images can be copied or cut",
                    (FvPlace::InputLibrary, _) => "Only folders and ideas can be copied or cut",
                    (FvPlace::Inbox, _) => "Inbox ideas are kept (into the library) or discarded from the Grid",
                    (_, true) => "Only folders, projects and image links can be cut",
                    (_, false) => {
                        "Only folders and projects can be copied (cut + paste moves image links between review folders)"
                    }
                });
            }
            if !paths.is_empty() {
                state.fv_clipboard = Some(FvClipboard { paths, cut });
            }
            vec![]
        }
        Msg::FvPaste(sub) => fv_paste(state, sub),
        Msg::FvDelete => {
            let fv = &state.folder_view;
            if is_smart(&fv.path) || fv.selected.iter().any(|n| is_smart(&fv.path.join(n))) {
                state.messages_info("Smart folders are read-only");
                return vec![];
            }
            let place = fv.place;
            if place == FvPlace::Inbox {
                state.messages_info("Inbox ideas are discarded from the Grid; files in inbox_drop/ are imported");
                return vec![];
            }
            let targets: Vec<PathBuf> = fv
                .shown()
                .into_iter()
                .filter(|e| {
                    fv.selected.contains(&e.name)
                        && match place {
                            FvPlace::InputLibrary => matches!(e.kind, EntryKind::Folder | EntryKind::Idea(_)),
                            _ => matches!(e.kind, EntryKind::Folder | EntryKind::Project | EntryKind::Image(_)),
                        }
                })
                .map(|e| fv.path.join(&e.name))
                .collect();
            if targets.is_empty() {
                return vec![];
            }
            let text = if place == FvPlace::InputLibrary {
                format!(
                    "Discard {} item(s)? Ideas go to Discarded (restore them from there); folders go to the trash.",
                    targets.len()
                )
            } else if place == FvPlace::Collections {
                format!(
                    "Remove {} item(s)? Collections go to the trash with their links; the images keep their rating and project.",
                    targets.len()
                )
            } else {
                format!(
                    "Delete {} item(s)? Folders and projects go to the trash with everything in them; image links go to the trash too, the images stay in the store until Clean up.",
                    targets.len()
                )
            };
            update(state, Msg::Ask(text, Box::new(Msg::FvDeleteConfirmed(targets))))
        }
        Msg::FvDeleteConfirmed(targets) => {
            let mut fx = vec![];
            for t in targets {
                let is_link = t.extension().is_some_and(|e| e == "png");
                if let Ok(rel) = t.strip_prefix("collections") {
                    fx.extend(update(state, Msg::DeleteNode(Tree::Collections, rel.to_path_buf())));
                } else if let Ok(rel) = t.strip_prefix("input/library") {
                    fx.extend(update(state, Msg::DeleteNode(Tree::InputLibrary, rel.to_path_buf())));
                } else if is_link {
                    fx.extend(update(state, Msg::DeleteOutput(t)));
                } else if let Ok(rel) = t.strip_prefix("projects") {
                    fx.extend(update(state, Msg::DeleteNode(Tree::Projects, rel.to_path_buf())));
                }
            }
            fx
        }
        Msg::FvRename => {
            let fv = &state.folder_view;
            let sel: Vec<_> = fv.shown().into_iter().filter(|e| fv.selected.contains(&e.name)).collect();
            match sel.as_slice() {
                [e] if is_smart(&fv.path.join(&e.name)) => {
                    state.messages_info("Smart folders are read-only");
                    vec![]
                }
                [e] if matches!(e.kind, EntryKind::Idea(_)) => {
                    state.messages_info("An idea is renamed by editing its title (Enter → Edit idea)");
                    vec![]
                }
                [e] if matches!(e.kind, EntryKind::Folder | EntryKind::Project) && fv.tree().is_some() => {
                    let (tree, rel) = (fv.tree().unwrap_or(Tree::Projects), fv.tree_rel(&fv.path.join(&e.name)));
                    update(state, Msg::StartRename(tree, rel))
                }
                [_] => {
                    state.messages_info(
                        "Only folders and projects can be renamed here (image link names carry run, revision and seed)",
                    );
                    vec![]
                }
                _ => vec![],
            }
        }
        Msg::Problem(e) => {
            state.messages_error(e);
            vec![]
        }
        Msg::Info(t) => {
            state.messages_info(t);
            vec![]
        }
        Msg::LibraryChanged => match state.library {
            Some(_) => vec![Effect::Rescan],
            None => vec![],
        },
        Msg::DropReady => match state.library {
            Some(_) => {
                state.messages_info("New files in inbox_drop/: importing them as ideas");
                lib(state, LibOp::ImportInboxDrop)
            }
            None => vec![],
        },
        Msg::WatchStatus(s) => {
            state.watch_status = s;
            vec![]
        }
        Msg::DismissMessage(i) => {
            if i < state.messages.len() {
                state.messages.remove(i);
            }
            vec![]
        }
        Msg::UsageCheck(done, total) => {
            state.usage_check = (done < total).then_some((done, total));
            vec![]
        }
        Msg::ClearMessages => {
            state.messages.clear();
            vec![]
        }
    }
}

fn lib(state: &mut AppState, op: LibOp) -> Vec<Effect> {
    state.busy += 1;
    vec![Effect::Lib(op)]
}

/// The New idea form's draft as an idea: into the inbox, the library, or
/// (Promote) the library and a new project `name` in `parent`.
fn add_idea(state: &mut AppState, how: AddIdea, parent: PathBuf, name: String) -> Vec<Effect> {
    let d = std::mem::take(&mut state.draft);
    state.idea_dirty = None;
    let text_for_history = (d.text.clone(), d.title.clone());
    let tags = d.tag_list();
    let loc = match how {
        AddIdea::Inbox => IdeaLoc::Unreviewed,
        AddIdea::Keep | AddIdea::Promote => IdeaLoc::top_library(),
    };
    let idea = NewIdea { title: d.title, text: d.text, tags, images: d.images, loc, ..NewIdea::text("", "") };
    state.busy += 1;
    let add = match how {
        AddIdea::Promote => Effect::Lib(LibOp::NewIdeaPromote(idea, parent, name)),
        _ => Effect::Lib(LibOp::NewIdea(idea)),
    };
    state.saving += 1;
    let mut fx = vec![add, Effect::SaveDraft(DraftSave::ClearIdea)];
    let (t, x) = (text_for_history.0, text_for_history.1);
    fx.extend(record(state, HistoryKey::NewIdea, HistoryKind::Saved, t, x));
    fx
}

/// Why the Promote dialog can't create the project (in its folder when
/// `in_folder`): a bad or taken name, or a folder name a project has.
pub fn promote_problem(state: &AppState, d: &PromoteDialog) -> Option<String> {
    let name = d.name.trim();
    if let Err(e) = increart_core::library::validate_name(name) {
        return Some(e.to_string());
    }
    let (at, exists) = promote_folder(state, d);
    if let Some(folder) = d.new_folder.as_deref().map(str::trim) {
        if let Err(e) = increart_core::library::validate_name(folder) {
            return Some(format!("new folder: {e}"));
        }
        if !exists && state.project_name_taken(&at, folder) {
            return Some(format!("a project is named {folder} there"));
        }
    }
    state.project_name_taken(&at.join("_"), name).then(|| format!("{name} already exists there"))
}

/// Where the project goes: the dialog's folder, or its new folder in it
/// (an existing one of that name, any case, is used); and whether that
/// folder exists.
pub fn promote_folder(state: &AppState, d: &PromoteDialog) -> (PathBuf, bool) {
    let Some(new) = &d.new_folder else {
        return (d.parent.clone(), true);
    };
    let at = d.parent.join(new.trim());
    let same = |f: &&PathBuf| f.to_string_lossy().to_lowercase() == at.to_string_lossy().to_lowercase();
    match state.index.project_folders.iter().find(same) {
        Some(f) => (f.clone(), true),
        None => (at, false),
    }
}

/// Changes this library's collapsed project folders (keeping only folders
/// that still exist) and saves the config.
fn set_collapsed(state: &mut AppState, change: impl FnOnce(&mut Vec<PathBuf>)) -> Vec<Effect> {
    let Some(key) = state.library.as_ref().map(|l| l.to_string_lossy().into_owned()) else { return vec![] };
    let folders = &state.index.project_folders;
    let mut c = state.config.collapsed_folders.remove(&key).unwrap_or_default();
    change(&mut c);
    c.retain(|f| folders.contains(f));
    c.sort();
    c.dedup();
    if !c.is_empty() {
        state.config.collapsed_folders.insert(key, c);
    }
    save_config(state)
}

/// The Promote dialog's first folder: where the last promote went, if it's
/// still there, else the top level.
fn promote_parent(state: &AppState) -> PathBuf {
    state
        .promote_last
        .clone()
        .filter(|p| p.as_os_str().is_empty() || state.index.project_folders.contains(p))
        .unwrap_or_default()
}

/// The Promote dialog's first guess: the first idea title that makes a
/// valid, free project name (numbered if taken), else empty.
fn promote_name<'a>(state: &AppState, titles: impl IntoIterator<Item = &'a str>) -> String {
    let at = promote_parent(state).join("_");
    titles
        .into_iter()
        .map(str::trim)
        .find(|t| !t.is_empty() && increart_core::library::validate_name(t).is_ok())
        .map(|t| {
            std::iter::once(t.to_string())
                .chain((2..).map(|n| format!("{t} {n}")))
                .find(|n| !state.project_name_taken(&at, n))
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Ctrl+click, Shift+click or a plain click on a pickable image.
fn pick_image(state: &mut AppState, image: ImageRef, how: SelectHow) {
    // picking starts from the image the cursor is on
    let start = |state: &mut AppState| {
        if state.picked.is_empty()
            && let Some(c) = state.current_image()
        {
            state.picked.push(c);
        }
    };
    match how {
        SelectHow::Replace => state.picked.clear(),
        SelectHow::Toggle => {
            start(state);
            match state.picked.iter().position(|i| i == &image) {
                Some(at) => {
                    state.picked.remove(at);
                }
                None => state.picked.push(image),
            }
        }
        SelectHow::Range => {
            start(state);
            let all = state.pickable();
            let to = all.iter().position(|i| i == &image);
            let from = state.picked.last().and_then(|a| all.iter().position(|i| i == a)).or(to);
            let (Some(from), Some(to)) = (from, to) else { return };
            let range: Vec<ImageRef> =
                if from <= to { all[from..=to].to_vec() } else { all[to..=from].iter().rev().cloned().collect() };
            for i in range {
                if !state.picked.contains(&i) {
                    state.picked.push(i);
                }
            }
        }
    }
}

/// Leaving a sweep (or run) scope: the sheet goes with it.
fn leave_sweep(state: &mut AppState) {
    state.review_sweep = None;
    state.review_run = None;
    state.picked.clear();
    state.sheet_filter.clear();
    if state.review_view == ReviewView::Sheet {
        state.review_view = ReviewView::Single;
    }
}

/// `r00`, `r01`…: one number per run, a sweep counting once, and past the
/// highest `rNN` already there (libraries from before started at `r01`).
fn next_label(state: &AppState) -> String {
    let n = state.project_entry().map_or(0, |p| {
        let sweeps: std::collections::HashSet<_> =
            p.runs.iter().filter_map(|r| r.sweep.as_ref()).map(|s| &s.name).collect();
        let count = p.runs.iter().filter(|r| r.sweep.is_none()).count() + sweeps.len();
        let number = |label: &str| {
            let digits: String = label.strip_prefix('r')?.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<usize>().ok()
        };
        let past = p.runs.iter().filter_map(|r| number(&r.label)).max().map_or(0, |m| m + 1);
        count.max(past)
    });
    format!("r{n:02}")
}

/// Images in a sweep before Queue asks first.
pub const BIG_SWEEP: u64 = 100;

/// The Sweep mode's runs, checked: (form, tag) per chosen preset that the
/// references fit, and how many chosen presets need more references.
pub fn sweep_runs(state: &AppState) -> Result<(Vec<(GenerateForm, SweepTag)>, usize), String> {
    use increart_core::prompts::{slot_names, sweep_presets, sweep_prompt};
    let f = &state.generate;
    if f.sweep.vary != increart_core::prompts::SweepVary::Style {
        return param_runs(f, &state.config.recipes).map(|r| (r, 0));
    }
    let refs = f.refs().len();
    let chosen = sweep_presets(&state.prompt_presets, &f.sweep);
    let needs = |p: &str| image_tags(p).into_iter().map(|(_, n)| n).max().unwrap_or(0);
    let (fit, unfit): (Vec<_>, Vec<_>) = chosen.into_iter().partition(|(_, p)| needs(&p.prompt) <= refs);
    if fit.is_empty() {
        return Err(if unfit.is_empty() {
            "No styles chosen for the sweep".into()
        } else {
            format!("None of the chosen presets fit {refs} reference image(s)")
        });
    }
    let texts = std::iter::once(f.prompt.as_str()).chain(fit.iter().map(|(_, p)| p.prompt.as_str()));
    let unfilled: Vec<String> = slot_names(texts)
        .into_iter()
        .filter(|n| f.sweep.slots.get(n).is_none_or(|v| v.trim().is_empty()))
        .map(|n| format!("{{{n}}}"))
        .collect();
    if !unfilled.is_empty() {
        return Err(format!("Fill in {} for the sweep first", unfilled.join(", ")));
    }
    let name = f.label.trim().to_string();
    let runs = fit
        .into_iter()
        .map(|(t, p)| {
            let mut run = f.clone();
            run.sweep = Default::default();
            run.prompt = sweep_prompt(&f.prompt, &p.prompt, &f.sweep.slots);
            if !p.negative.is_empty() {
                run.negative = p.negative.clone();
            }
            run.label = format!("{name} · {}", p.name);
            (run, SweepTag { name: name.clone(), task: t.name.clone(), preset: p.name.clone(), param: None })
        })
        .collect();
    Ok((runs, unfit.len()))
}

/// A setting sweep's runs: one per value, the setting changed and the rest
/// as the form has it (a model sweep takes each model preset's own steps,
/// CFG and flow shift).
pub fn param_runs(f: &GenerateForm, recipes: &[RecipePreset]) -> Result<Vec<(GenerateForm, SweepTag)>, String> {
    use increart_core::prompts::{PRESET_SLOT, SweepVary, fill_slots};
    let values = sweep_values(f, recipes)?;
    if f.prompt.contains(PRESET_SLOT) {
        return Err(format!("{PRESET_SLOT} in the prompt is for style sweeps: take it out, or vary the style"));
    }
    let prompt = fill_slots(f.prompt.clone(), &f.sweep.slots);
    let unfilled: Vec<String> =
        increart_core::prompts::slot_names([prompt.as_str()]).into_iter().map(|n| format!("{{{n}}}")).collect();
    if !unfilled.is_empty() {
        return Err(format!("Fill in {} (type over it) before queueing", unfilled.join(", ")));
    }
    let name = f.label.trim().to_string();
    let runs = values
        .into_iter()
        .map(|param| {
            let mut run = f.clone();
            run.sweep = Default::default();
            run.prompt = prompt.clone();
            let v: f32 = param.value.parse().unwrap_or_default();
            match f.sweep.vary {
                SweepVary::Steps => run.steps = v as u32,
                SweepVary::Cfg => run.cfg = v,
                SweepVary::FlowShift => run.flow_shift = Some(v),
                SweepVary::Strength => run.strength = v,
                SweepVary::Model => {
                    if let Some(i) = recipes.iter().position(|r| r.name == param.value) {
                        let p = &recipes[i];
                        (run.preset, run.steps, run.cfg, run.flow_shift) = (i, p.steps, p.cfg, p.flow_shift);
                    }
                }
                SweepVary::Style => {}
            }
            run.label = format!("{name} · {}", param.label());
            (run, SweepTag { name: name.clone(), task: String::new(), preset: String::new(), param: Some(param) })
        })
        .collect();
    Ok(runs)
}

/// A setting sweep's values, from what was typed (in order, without
/// repeats) or the ticked models, or what's wrong with them.
pub fn sweep_values(f: &GenerateForm, recipes: &[RecipePreset]) -> Result<Vec<SweepParam>, String> {
    use increart_core::prompts::SweepVary;
    let vary = f.sweep.vary;
    let param = |value: String| SweepParam { name: vary.key().into(), value };
    if vary == SweepVary::Model {
        let v: Vec<_> =
            recipes.iter().filter(|r| f.sweep.models.contains(&r.name)).map(|r| param(r.name.clone())).collect();
        return if v.is_empty() { Err("Tick the models to compare".into()) } else { Ok(v) };
    }
    if vary == SweepVary::Strength && f.init().is_none() {
        return Err("A strength sweep needs a start image".into());
    }
    let (lo, hi) = match vary {
        SweepVary::Steps => (1.0, 200.0),
        SweepVary::Cfg => (0.0, 30.0),
        SweepVary::FlowShift => (0.0, 20.0),
        _ => (0.0, 1.0),
    };
    let mut v: Vec<f32> = vec![];
    for w in f.sweep.values.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|w| !w.is_empty()) {
        let x: f32 = w.parse().map_err(|_| format!("“{w}” isn't a number"))?;
        if vary == SweepVary::Steps && x.fract() != 0.0 {
            return Err(format!("Steps are whole numbers: {w}"));
        }
        // CFG may be 0 … 30 and steps 1 … 200; the rest above 0
        let low_ok = if vary == SweepVary::Steps || vary == SweepVary::Cfg { x >= lo } else { x > lo };
        if !low_ok || x > hi {
            return Err(format!("{} goes from {lo} to {hi}: {w}", vary.name()));
        }
        v.push(x);
    }
    v.sort_by(f32::total_cmp);
    v.dedup();
    if v.is_empty() {
        return Err("Type the values to compare, e.g. 8, 12, 20".into());
    }
    Ok(v.into_iter().map(|x| param(fmt_value(x))).collect())
}

/// A setting value as typed: "2", "2.5", "0.45".
pub fn fmt_value(x: f32) -> String {
    format!("{}", (x * 1000.0).round() / 1000.0)
}

/// `from`, `from + step`, … up to `to` (at most 50), for the values field.
pub fn range_values(from: f32, to: f32, step: f32) -> String {
    if step <= 0.0 || to < from {
        return fmt_value(from);
    }
    let n = (((to - from) / step + 1e-4).floor() as usize + 1).min(50);
    (0..n).map(|i| fmt_value(from + step * i as f32)).collect::<Vec<_>>().join(", ")
}

/// A setting sweep's starting values, around the form's own.
pub fn sweep_defaults(vary: increart_core::prompts::SweepVary, f: &GenerateForm) -> String {
    use increart_core::prompts::SweepVary;
    let round = |x: f32, to: f32| (x / to).round() * to;
    let v: Vec<f32> = match vary {
        SweepVary::Steps => {
            let s = f.steps.max(2) as f32;
            [0.5, 0.75, 1.0, 1.5, 2.0].iter().map(|k| (s * k).round().max(1.0)).collect()
        }
        SweepVary::Cfg => {
            let c = f.cfg.max(1.0);
            [0.5, 0.75, 1.0, 1.5, 2.0].iter().map(|k| round(c * k, 0.5).max(1.0)).collect()
        }
        SweepVary::FlowShift => {
            let x = f.flow_shift.unwrap_or(3.0);
            [0.5, 2.0 / 3.0, 1.0, 4.0 / 3.0, 2.0].iter().map(|k| round(x * k, 0.1)).collect()
        }
        SweepVary::Strength => vec![0.3, 0.45, 0.6, 0.75, 0.9],
        SweepVary::Style | SweepVary::Model => vec![],
    };
    let mut v = v;
    v.dedup();
    v.into_iter().map(fmt_value).collect::<Vec<_>>().join(", ")
}

fn queue_sweep(state: &mut AppState, confirmed: bool) -> Vec<Effect> {
    let Some(project) = state.project.clone() else {
        state.messages_error("Select a project first");
        return vec![];
    };
    let f = &state.generate;
    if !f.width.is_multiple_of(32) || !f.height.is_multiple_of(32) || f.seeds == 0 {
        state.messages_error("Size must be a multiple of 32 and seeds at least 1");
        return vec![];
    }
    let name = f.label.trim();
    if name.is_empty() {
        state.messages_error("Give the sweep a label");
        return vec![];
    }
    let taken = state
        .project_entry()
        .is_some_and(|p| p.runs.iter().any(|r| r.sweep.as_ref().is_some_and(|s| s.name == name) || r.label == name));
    if taken {
        state.messages_error(format!("This project already has a run or sweep labelled {name}"));
        return vec![];
    }
    let (runs, _) = match sweep_runs(state) {
        Ok(r) => r,
        Err(e) => {
            state.messages_error(e);
            return vec![];
        }
    };
    // each run's own model preset (a model sweep has several)
    let Some(runs) = runs
        .into_iter()
        .map(|(f, t)| {
            let preset = state.config.recipes.get(f.preset).cloned()?;
            Some((f, t, preset))
        })
        .collect::<Option<Vec<_>>>()
    else {
        state.messages_error("No model preset");
        return vec![];
    };
    let images = runs.len() as u64 * state.generate.seeds;
    if images > BIG_SWEEP && !confirmed {
        state.confirm = Some(Confirm {
            text: format!("Queue {} runs × {} seeds = {images} images?", runs.len(), state.generate.seeds),
            then: Box::new(Msg::QueueSweep(true)),
        });
        return vec![];
    }
    // one run at a time would fill the contact sheet a style at a time
    if state.config.job_order == Order::Sequential && !state.sweep_order_asked {
        state.sweep_order_ask = Some(confirmed);
        return vec![];
    }
    if state.servers.is_empty() && state.background.is_none() && !state.no_server_warned {
        state.no_server_warned = true;
        state.no_server_warning = true;
        return vec![];
    }
    let queued = Box::new(Queued { project: project.clone(), form: Box::new(state.generate.to_draft(&state.config)) });
    let op = LibOp::CreateSweep {
        project,
        runs,
        model_dir: state.config.sdserver.model_dirs.first().cloned(),
        queue: state.engine_running,
        queued,
    };
    if !state.engine_running {
        state.messages_info("Sweep saved; it will be generated once servers are connected");
    }
    lib(state, op)
}

fn set_index(state: &mut AppState, ix: Index) {
    state.index = ix;
    state.rebuild_usage();
    state.folder_view.listed = None;
    for v in &mut state.parked_folder_views {
        v.listed = None;
    }
    state.inbox_cursor = state.inbox_cursor.min(state.inbox_queue().len().saturating_sub(1));
    state.review_cursor = state.review_cursor.min(state.review_queue().len().saturating_sub(1));
    if state.project.as_ref().is_some_and(|p| state.index.project(p).is_none()) {
        state.project = None;
    }
    state.lib_selected.retain(|id| state.index.idea(id).is_some());
}

/// The Review / Output library actions that put an output image into a
/// project's Generate form (DESIGN.md, Outputs as inputs).
fn use_image(state: &mut AppState, image: ImageRef, target: UseTarget) -> Vec<Effect> {
    let Some(link) = state.output_link(&image).cloned() else { return vec![] };
    let source = state.project_of_link(&link).cloned();
    let run = state.index.run(&link.name.run).map(|(_, r)| r.clone());
    match target {
        UseTarget::CurrentProject => {
            let Some(p) = state.project_entry().cloned() else {
                state.messages_error("Select a project first (Projects tab)");
                return vec![];
            };
            let before = state.generate.refs();
            state.generate.add_reference(image.clone());
            let tag =
                state.generate.images.iter().position(|i| i.image == image).and_then(|i| state.generate.tag_of(i));
            state.messages_info(format!(
                "Added to {}'s Generate form as <image{}>",
                p.name(),
                tag.map_or("?".into(), |t| t.to_string())
            ));
            images_changed(state, before)
        }
        UseTarget::Edit => {
            let Some(p) = source else { return vec![] };
            let mut fx = vec![];
            if state.project.as_ref() != Some(&p.project.id) {
                fx = flush_generate(state);
                state.project = Some(p.project.id.clone());
                state.proj_folder = None;
            }
            let mut f = GenerateForm::new(&state.config);
            f.label = next_label(state);
            f.first_seed = state.next_seed();
            if let Some(r) = &run {
                (f.width, f.height) = (r.width, r.height);
            }
            f.add_reference(image);
            f.prompt = "<image1> ".into();
            state.generate = f;
            mark_generate(state);
            state.tab = Tab::Projects;
            state.projects_tab = ProjectsTab::Generate;
            fx
        }
        UseTarget::NewProject | UseTarget::DuplicateProject => {
            let Some(p) = source else { return vec![] };
            let name = format!("{} from s{:04}", p.name(), link.name.seed);
            state.new_project = Some(NewProjectDialog { image, target, name });
            vec![]
        }
    }
}

/// Makes the project the New project / Duplicate dialog named.
fn prepare_project(state: &mut AppState, image: ImageRef, target: UseTarget, name: String) -> Vec<Effect> {
    let Some(link) = state.output_link(&image).cloned() else { return vec![] };
    let source = state.project_of_link(&link).cloned();
    let run = state.index.run(&link.name.run).map(|(_, r)| r.clone());
    let Some(p) = source else { return vec![] };
    let rel = p.dir.strip_prefix("projects").map(PathBuf::from).unwrap_or_default();
    let parent = rel.parent().map(PathBuf::from).unwrap_or_default();
    let (form, copy_of) = if target == UseTarget::NewProject {
        let mut f = GenerateForm::new(&state.config);
        if let Some(r) = &run {
            (f.prompt, f.width, f.height) = (r.prompt.clone(), r.width, r.height);
        }
        f.add_reference(image);
        (f, None)
    } else {
        let mut f = run
            .as_ref()
            .map(|r| GenerateForm::from_run(r, &state.config))
            .unwrap_or_else(|| GenerateForm::new(&state.config));
        // the run's images as they were, and this one as the next reference
        f.add_reference(image);
        (f, Some(rel))
    };
    lib(state, LibOp::PrepareProject { parent, name, copy_of, form })
}

/// After servers opened in the background settle: start the engine with the
/// ones that came up (the others were reported as they failed).
/// Stops the servers in use, showing them as stopping until they're gone
/// ([`Msg::ServersStopped`]).
fn stop_servers(state: &mut AppState) -> Effect {
    state.stopping =
        state.servers.iter().map(|s| if s.device.is_empty() { s.name.clone() } else { s.device.clone() }).collect();
    Effect::StopServers
}

fn start_with_up(state: &mut AppState, progress: &[ServerProgress]) -> Vec<Effect> {
    let up: Vec<bool> = progress.iter().map(|p| *p == ServerProgress::Up).collect();
    state.servers = state.servers.iter().zip(&up).filter(|(_, u)| **u).map(|(s, _)| s.clone()).collect();
    if state.servers.is_empty() {
        state.messages_error("No server came up: open Servers… to retry or change the preset");
        return vec![Effect::StopServers];
    }
    state.engine_running = true;
    state.servers.iter_mut().for_each(|s| (s.idle_since, s.worked) = (None, false));
    vec![Effect::StartEngine { shared: state.config.sdserver.clone(), order: state.config.job_order }]
}

/// After the Generate form's image list changed: saved soon, and if the
/// `<imageN>` numbers the prompt uses moved, a prompt edit to confirm.
fn images_changed(state: &mut AppState, before: Vec<ImageRef>) -> Vec<Effect> {
    mark_generate(state);
    let after = state.generate.refs();
    let mut map = std::collections::BTreeMap::new();
    for (_, n) in image_tags(&state.generate.prompt) {
        let Some(img) = before.get(n.wrapping_sub(1)) else { continue }; // a tag with no image before either
        let now = after.iter().position(|x| x == img).map(|i| i + 1);
        if now != Some(n) {
            map.insert(n, now);
        }
    }
    if !map.is_empty() {
        let prompt = retag(&state.generate.prompt, &map);
        state.retag = Some(Retag { changes: map.into_iter().collect(), prompt });
    }
    vec![]
}

/// Characters changed between two texts (word diff).
pub fn changed_chars(a: &str, b: &str) -> usize {
    use similar::{ChangeTag, TextDiff};
    TextDiff::from_words(a, b).iter_all_changes().filter(|c| c.tag() != ChangeTag::Equal).map(|c| c.value().len()).sum()
}

/// A history entry for `key` (DESIGN.md, Generate history). A project's
/// entry has the whole Generate form when it's the open project, and the
/// project's newest run.
fn record(state: &mut AppState, key: HistoryKey, kind: HistoryKind, text: String, extra: String) -> Vec<Effect> {
    let mut entry = HistoryEntry::text(Default::default(), kind, text, extra);
    if let HistoryKey::Project(p) = &key {
        if state.project.as_ref() == Some(p) {
            entry.form = Some(Box::new(state.generate.to_draft(&state.config)));
        }
        entry.after = newest_run(state, p, &[]);
    }
    record_entry(state, key, entry)
}

/// Puts a history entry's whole form back in Generate. The label and first
/// seed stay as they are (the next run's); images no longer in the library
/// and a model preset that's gone are left out, with a message.
fn restore_form(state: &mut AppState, d: &increart_core::model::GenerateDraft) {
    let mut f = GenerateForm::from_draft(d, &state.config);
    f.label = state.generate.label.clone();
    f.first_seed = state.generate.first_seed;
    let mut notes = vec![];
    if !state.config.recipes.iter().any(|r| r.name == d.preset) {
        f.preset = state.generate.preset;
        notes.push(format!("model preset {} no longer exists, kept the current one", d.preset));
    }
    if let Some(root) = &state.library {
        let before = f.images.len();
        f.images.retain(|i| root.join(i.image.rel_path()).is_file());
        let gone = before - f.images.len();
        if gone > 0 {
            notes.push(format!("{gone} image(s) no longer in the library were left out"));
        }
    }
    state.generate = f;
    if notes.is_empty() {
        state.messages_info("Restored the prompt and settings");
    } else {
        state.messages_error(format!("Restored; {}", notes.join("; ")));
    }
}

/// Each entry's runs and the run it follows: stored in newer entries; for
/// older ones, the runs labelled as the queued checkpoint says, and the
/// newest run made before the entry.
fn resolve_history_runs(state: &AppState, v: &mut HistoryView) {
    let runs: &[increart_core::model::Run] = match &v.key {
        HistoryKey::Project(p) => state.index.project(p).map_or(&[], |p| p.runs.as_slice()),
        _ => &[],
    };
    v.runs = v
        .entries
        .iter()
        .map(|e| match &e.kind {
            HistoryKind::Queued(_) if !e.runs.is_empty() => e.runs.clone(),
            HistoryKind::Queued(label) => runs
                .iter()
                .filter(|r| &r.label == label || r.sweep.as_ref().is_some_and(|s| &s.name == label))
                .map(|r| r.id.clone())
                .collect(),
            _ => vec![],
        })
        .collect();
    v.after = v
        .entries
        .iter()
        .zip(&v.runs)
        .map(|(e, mine)| {
            if e.form.is_some() {
                return e.after.clone();
            }
            runs.iter()
                .filter(|r| r.created <= e.time && !mine.contains(&r.id))
                .max_by_key(|r| (r.created, r.id.clone()))
                .map(|r| r.id.clone())
        })
        .collect();
}

/// The run before `run` for the history's "led to" filter: what its queued
/// checkpoint followed, else the project's run made just before it.
fn run_before(state: &AppState, v: &HistoryView, run: &Id) -> Option<Id> {
    if let Some(i) = (0..v.entries.len()).find(|i| v.runs_of(*i).contains(run)) {
        return v.after.get(i).cloned().flatten();
    }
    let (p, r) = state.index.run(run)?;
    p.runs.iter().filter(|x| x.created < r.created).max_by_key(|x| (x.created, x.id.clone())).map(|x| x.id.clone())
}

/// The queued checkpoint: the form a run (or a sweep's runs) was made from.
fn record_queued(state: &mut AppState, q: Queued, runs: Vec<Id>) -> Vec<Effect> {
    let after = newest_run(state, &q.project, &runs);
    let entry = HistoryEntry {
        time: Default::default(),
        kind: HistoryKind::Queued(q.form.label.clone()),
        text: q.form.prompt.clone(),
        extra: q.form.negative_prompt.clone(),
        form: Some(q.form),
        runs,
        after,
    };
    record_entry(state, HistoryKey::Project(q.project), entry)
}

/// A project's newest run (by creation), leaving out `except`.
fn newest_run(state: &AppState, project: &Id, except: &[Id]) -> Option<Id> {
    let p = state.index.project(project)?;
    p.runs.iter().filter(|r| !except.contains(&r.id)).max_by_key(|r| (r.created, r.id.clone())).map(|r| r.id.clone())
}

/// Records an entry: the same texts and settings again are skipped; a small
/// edit soon after an edit replaces it (a settings change alone is small);
/// anything else, and every checkpoint, is a new entry.
fn record_entry(state: &mut AppState, key: HistoryKey, entry: HistoryEntry) -> Vec<Effect> {
    if state.library.is_none() || (entry.text.trim().is_empty() && entry.extra.trim().is_empty()) {
        return vec![];
    }
    let edit = entry.kind == HistoryKind::Edit;
    let last = state.history_last.get(&key);
    if last.is_some_and(|l| l.entry.same_content(&entry) && (edit || l.entry.kind == entry.kind)) {
        return vec![];
    }
    let replace_last = edit
        && last.is_some_and(|l| {
            let (t, x) = (&l.entry.text, &l.entry.extra);
            l.entry.kind == HistoryKind::Edit
                && state.now - l.at < HISTORY_MERGE_SECS
                && changed_chars(&format!("{t}\n{x}"), &format!("{}\n{}", entry.text, entry.extra))
                    < HISTORY_MERGE_CHARS
        });
    let at = if replace_last { last.map_or(state.now, |l| l.at) } else { state.now };
    state.history_last.insert(key.clone(), LastEntry { at, entry: entry.clone() });
    vec![Effect::RecordHistory { key, entry: Box::new(entry), replace_last }]
}

/// Records the Edit idea text after a pause.
fn flush_idea_edit(state: &mut AppState) -> Vec<Effect> {
    if state.idea_edit_dirty.take().is_none() {
        return vec![];
    }
    let Some(ed) = state.idea_edit.clone() else { return vec![] };
    record(state, HistoryKey::Idea(ed.id), HistoryKind::Edit, ed.text, ed.title)
}

/// After the library changed: list the folder view's folder again if it
/// is showing.
fn relist(state: &mut AppState) -> Vec<Effect> {
    let showing = match state.tab {
        Tab::Projects => state.projects_tab == ProjectsTab::Folder,
        Tab::OutputLibrary => state.output_tab == OutputTab::Folder,
        Tab::InputLibrary => state.lib_tab == LibTab::Folder,
        Tab::IdeaInbox => state.inbox_tab == InboxTab::Folder,
        _ => false,
    };
    let fv = &mut state.folder_view;
    if showing && fv.listed.is_none() {
        let path = fv.path.clone();
        fv.listed = Some(path.clone());
        return fv_list(state, path);
    }
    vec![]
}

/// Lists the folder view again when its listing is out of date.
fn relist_if_stale(state: &mut AppState) -> Vec<Effect> {
    if state.folder_view.listed.as_ref() != Some(&state.folder_view.path) {
        state.folder_view.listed = None;
    }
    relist(state)
}

/// Makes `folder_view` the view of the tab showing, parking the one that
/// was.
fn fv_sync(state: &mut AppState) {
    let place = match state.tab {
        Tab::IdeaInbox => FvPlace::Inbox,
        Tab::InputLibrary => FvPlace::InputLibrary,
        Tab::OutputLibrary => FvPlace::Collections,
        _ => FvPlace::Projects,
    };
    fv_activate(state, place);
}

fn fv_activate(state: &mut AppState, place: FvPlace) {
    if state.folder_view.place != place
        && let Some(i) = state.parked_folder_views.iter().position(|v| v.place == place)
    {
        std::mem::swap(&mut state.folder_view, &mut state.parked_folder_views[i]);
    }
}

/// The place a path is in: the view with the longest root it's under
/// (`input/library/…` is the library's, not the inbox's).
fn place_of(path: &std::path::Path) -> Option<FvPlace> {
    FvPlace::ALL.into_iter().filter(|p| path.starts_with(p.root())).max_by_key(|p| p.root().len())
}

/// The folder view a path belongs to (a listing may arrive after a tab
/// switch).
fn fv_for<'a>(state: &'a mut AppState, path: &std::path::Path) -> &'a mut FolderView {
    match place_of(path) {
        Some(p) if p != state.folder_view.place => match state.parked_folder_views.iter_mut().find(|v| v.place == p) {
            Some(v) => v,
            None => &mut state.folder_view,
        },
        _ => &mut state.folder_view,
    }
}

/// Lists a folder: the virtual smart folders from the index, the rest from
/// disk.
fn fv_list(state: &mut AppState, path: PathBuf) -> Vec<Effect> {
    if is_smart(&path) {
        let rel = path.strip_prefix(SMART_DIR).map(PathBuf::from).unwrap_or_default();
        let entries = state.smart_listing(&rel);
        return update(state, Msg::FvListed(path, entries));
    }
    vec![Effect::ListDir(path)]
}

/// Where an Output library source is in its folder view.
fn source_path(state: &AppState, source: &OutputSource) -> PathBuf {
    let smart = PathBuf::from(SMART_DIR);
    match source {
        OutputSource::AllProjects => PathBuf::from("collections"),
        OutputSource::Collection(f) => PathBuf::from("collections").join(f),
        OutputSource::Smart(Smart::Day(d)) => smart.join("by date").join(d),
        OutputSource::Smart(Smart::Transparent) => smart.join("transparent"),
        OutputSource::ProjectFolder(f) => smart.join("by project").join(f),
        OutputSource::Run(id) => match state.index.run(id) {
            Some((p, _)) => source_path(state, &OutputSource::Project(p.project.id.clone())),
            None => smart.join("by project"),
        },
        OutputSource::Project(id) => {
            let dir = state.index.projects.iter().find(|p| &p.project.id == id).map(|p| &p.dir);
            let rel = dir.and_then(|d| d.strip_prefix("projects").ok());
            rel.map_or_else(|| smart.join("by project"), |r| smart.join("by project").join(r))
        }
        OutputSource::Smart(Smart::Idea(_)) => {
            let group = "by source idea";
            let names = state.smart_children(std::path::Path::new(group)).unwrap_or_default();
            let name = names
                .into_iter()
                .find(|n| state.smart_source(&std::path::Path::new(group).join(n)).as_ref() == Some(source));
            name.map_or_else(|| smart.join(group), |n| smart.join(group).join(n))
        }
    }
}

/// Navigates the folder view (within its root), keeping the selected project
/// in sync when it enters one.
fn fv_go(state: &mut AppState, path: PathBuf, push_history: bool) -> Vec<Effect> {
    let path = PathBuf::from(path.to_string_lossy().trim_end_matches('/'));
    if let Some(p) = place_of(&path) {
        fv_activate(state, p);
    }
    if !path.starts_with(&state.folder_view.root) {
        let root = state.folder_view.root.display().to_string();
        state.messages_error(format!("{} is outside {root}", path.display()));
        return vec![];
    }
    let fv = &mut state.folder_view;
    if push_history && path != fv.path {
        fv.back.push(fv.path.clone());
        fv.forward.clear();
    }
    fv.path = path.clone();
    fv.selected.clear();
    fv.anchor = None;
    fv.path_edit = None;
    fv.listed = Some(path.clone());
    let mut fx = fv_list(state, path.clone());
    let project = state.index.projects.iter().find(|p| path.starts_with(&p.dir)).map(|p| p.project.id.clone());
    if let Some(id) = project
        && state.project.as_ref() != Some(&id)
    {
        fx.extend(update(state, Msg::SelectProject(id)));
    }
    fx
}

/// An output image where it is: unrated in Review, else selected in the
/// Output library Grid under its project.
fn go_to_output(state: &mut AppState, link: increart_core::scan::OutputLink) -> Vec<Effect> {
    let project = state.project_of_link(&link).map(|p| p.project.id.clone());
    match link.verdict {
        None => {
            leave_sweep(state);
            state.review_folder = None;
            state.review_scope = project;
            state.review_view = ReviewView::Single;
            state.review_cursor = state.review_queue().iter().position(|o| o.path == link.path).unwrap_or(0);
            state.tab = Tab::Review;
            vec![]
        }
        Some(v) => {
            state.tab = Tab::OutputLibrary;
            state.output_tab = OutputTab::Grid;
            let preset = if v == Verdict::Bad { OutputPreset::Rejected } else { OutputPreset::Kept };
            if !state.output_filter.ratings.allows(Some(v)) {
                state.output_preset = preset;
                state.output_filter = preset.filter();
            }
            if let Some(p) = project {
                state.output_source = OutputSource::Project(p);
            }
            update(state, Msg::OutputSelect(Some(link.image.clone())))
        }
    }
}

/// Enter / double-click on a folder-view entry.
fn fv_open(state: &mut AppState, name: &str) -> Vec<Effect> {
    let fv = &state.folder_view;
    let Some(e) = fv.entries.iter().find(|e| e.name == name).cloned() else { return vec![] };
    let path = fv.path.join(name);
    match e.kind {
        k if k.is_dir() => fv_go(state, path, true),
        // a collection's or smart folder's image: the Output library Grid,
        // showing that collection / smart folder, with it selected
        // an idea's image: its Edit idea window
        EntryKind::Image(_) if matches!(state.folder_view.place, FvPlace::InputLibrary | FvPlace::Inbox) => {
            match state.folder_view.path.file_name().and_then(|n| n.to_string_lossy().parse::<Id>().ok()) {
                Some(id) => update(state, Msg::OpenIdea(id)),
                None => vec![],
            }
        }
        EntryKind::Image(img) if state.folder_view.place == FvPlace::Collections => {
            let dir = state.folder_view.path.clone();
            let source = if is_smart(&dir) {
                state.smart_source(dir.strip_prefix(SMART_DIR).unwrap_or(&dir))
            } else {
                dir.strip_prefix("collections").ok().map(|f| OutputSource::Collection(f.to_path_buf()))
            };
            state.output_tab = OutputTab::Grid;
            fv_sync(state);
            if let Some(s) = source {
                state.output_source = s;
            }
            update(state, Msg::OutputSelect(Some(img)))
        }
        EntryKind::Image(_) => {
            let Some(link) = state.index.projects.iter().flat_map(|p| &p.outputs).find(|o| o.path == path).cloned()
            else {
                return vec![];
            };
            go_to_output(state, link)
        }
        _ => update(state, Msg::ShowInFileManager(path)),
    }
}

/// Paste the clipboard into the current folder (or a subfolder of it).
fn fv_paste(state: &mut AppState, sub: Option<String>) -> Vec<Effect> {
    let Some(clip) = state.fv_clipboard.clone() else { return vec![] };
    let dest = match &sub {
        Some(d) => state.folder_view.path.join(d),
        None => state.folder_view.path.clone(),
    };
    let fx = transfer(state, &clip.paths, clip.cut, &dest);
    if clip.cut {
        state.fv_clipboard = None;
    }
    fx
}

/// Moves (`cut`) or copies folders, projects and image links (paths relative
/// to the library root) into `dest`, for paste and drag and drop.
fn transfer(state: &mut AppState, paths: &[PathBuf], cut: bool, dest: &std::path::Path) -> Vec<Effect> {
    let dest = dest.to_path_buf();
    let mut fx = vec![];
    for src in paths {
        if (cut && src.parent() == Some(dest.as_path())) || *src == dest {
            continue; // moving to where it already is, or dropped on itself
        }
        if is_smart(&dest) {
            state.messages_error("Smart folders are read-only");
            continue;
        }
        if let Ok(to) = dest.strip_prefix("input/library") {
            let Ok(from) = src.strip_prefix("input/library") else {
                state.messages_error("Only folders and ideas of the input library can go here");
                continue;
            };
            if dest.starts_with(src) {
                state.messages_error(format!("Can't put {} inside itself", src.display()));
                continue;
            }
            if state.index.ideas.iter().any(|i| dest.starts_with(&i.dir)) {
                state.messages_error("Folders and ideas can't go inside an idea");
                continue;
            }
            let (from, to) = (from.to_path_buf(), to.to_path_buf());
            let t = Tree::InputLibrary;
            fx.extend(lib(state, if cut { LibOp::MoveNode(t, from, to) } else { LibOp::CopyNode(t, from, to) }));
            continue;
        }
        if dest.starts_with("input") {
            state.messages_error("The inbox takes new ideas from inbox_drop/ (or the New idea form)");
            continue;
        }
        if let Ok(to) = dest.strip_prefix("collections") {
            let is_image = src.extension().is_some_and(|e| e == "png");
            if is_smart(src) || (is_image && !src.starts_with("collections")) {
                // an image from a smart folder or a project: added to the collection
                match state.image_at(src) {
                    Some(img) => fx.extend(lib(state, LibOp::AddToCollection(to.to_path_buf(), vec![img]))),
                    None => state.messages_error(format!("{} is not an image", src.display())),
                }
                continue;
            }
            let Ok(from) = src.strip_prefix("collections") else {
                state.messages_error("Only images can go into collections");
                continue;
            };
            if dest.starts_with(src) {
                state.messages_error(format!("Can't put {} inside itself", src.display()));
                continue;
            }
            let (from, to) = (from.to_path_buf(), to.to_path_buf());
            let op = if cut {
                LibOp::MoveNode(Tree::Collections, from, to)
            } else {
                LibOp::CopyNode(Tree::Collections, from, to)
            };
            fx.extend(lib(state, op));
            continue;
        }
        if dest.starts_with(src) {
            state.messages_error(format!("Can't put {} inside itself", src.display()));
            continue;
        }
        if state.index.projects.iter().any(|p| dest.starts_with(&p.dir)) && !src.extension().is_some_and(|e| e == "png")
        {
            state.messages_error("Folders and projects can't go inside a project");
            continue;
        }
        if src.extension().is_some_and(|e| e == "png") {
            // An image link: only between the review folders of its own project.
            let project_of = |p: &std::path::Path| {
                state.index.projects.iter().find(|x| p.starts_with(&x.dir)).map(|x| x.dir.clone())
            };
            let verdict = match dest
                .strip_prefix(project_of(&dest).unwrap_or_default())
                .ok()
                .map(|r| r.to_string_lossy().into_owned())
            {
                Some(r) if r == "unreviewed" => Some(None),
                Some(r) if r == "reviewed/good" => Some(Some(Verdict::Good)),
                Some(r) if r == "reviewed/neutral" => Some(Some(Verdict::Neutral)),
                Some(r) if r == "reviewed/bad" => Some(Some(Verdict::Bad)),
                _ => None,
            };
            match (verdict, project_of(src) == project_of(&dest)) {
                (Some(v), true) => fx.extend(update(state, Msg::ReviewLink(src.clone(), v))),
                _ => state.messages_error(
                    "Image links can only be moved between unreviewed/ and reviewed/*/ of their own project (use → input ref for others)",
                ),
            }
            continue;
        }
        let (Ok(from), Ok(to)) = (src.strip_prefix("projects"), dest.strip_prefix("projects")) else { continue };
        let op = if cut {
            LibOp::MoveNode(Tree::Projects, from.to_path_buf(), to.to_path_buf())
        } else {
            LibOp::CopyNode(Tree::Projects, from.to_path_buf(), to.to_path_buf())
        };
        fx.extend(lib(state, op));
    }
    fx
}

fn mark_generate(state: &mut AppState) {
    if state.project.is_some() {
        state.generate_dirty = Some(state.now);
    }
}

/// Saves the selected project's Generate draft now, if it was edited.
fn flush_generate(state: &mut AppState) -> Vec<Effect> {
    match (state.generate_dirty.take(), state.project.clone()) {
        (Some(_), Some(p)) => {
            let d = state.generate.to_draft(&state.config);
            if let Some(e) = state.index.projects.iter_mut().find(|e| e.project.id == p) {
                e.draft = Some(d.clone());
            }
            state.saving += 1;
            let mut fx = vec![Effect::SaveDraft(DraftSave::Generate(p.clone(), Box::new(d)))];
            let (t, x) = (state.generate.prompt.clone(), state.generate.negative.clone());
            fx.extend(record(state, HistoryKey::Project(p), HistoryKind::Edit, t, x));
            fx
        }
        _ => vec![],
    }
}

/// Saves the New idea draft now, if it was edited (an empty one is removed).
fn flush_idea(state: &mut AppState) -> Vec<Effect> {
    if state.idea_dirty.take().is_none() {
        return vec![];
    }
    let save = if state.draft.is_empty() && state.draft.tags.trim().is_empty() {
        DraftSave::ClearIdea
    } else {
        DraftSave::Idea(state.draft.to_file())
    };
    state.saving += 1;
    let mut fx = vec![Effect::SaveDraft(save)];
    let (t, x) = (state.draft.text.clone(), state.draft.title.clone());
    fx.extend(record(state, HistoryKey::NewIdea, HistoryKind::Edit, t, x));
    fx
}

/// The Generate form for the newly selected project: its saved draft, else a
/// fresh form. The label moves on if a run already has it; the first seed
/// continues after the project's seeds, unless the draft chose seed 0.
fn load_generate(state: &mut AppState) {
    let draft = state.project_entry().and_then(|p| p.draft.clone());
    let latest = state.project_entry().and_then(|p| state.latest_run(p)).cloned();
    let mut f = match (&draft, &latest) {
        (Some(d), _) => GenerateForm::from_draft(d, &state.config),
        // No draft (e.g. runs made before drafts existed): carry on from the
        // latest run's prompt, refs and settings.
        (None, Some(run)) => GenerateForm::from_run(run, &state.config),
        (None, None) => GenerateForm::new(&state.config),
    };
    let used = state.project_entry().is_some_and(|p| p.runs.iter().any(|r| r.label == f.label));
    if f.label.is_empty() || used {
        f.label = next_label(state);
    }
    if draft.is_none() || f.first_seed != 0 {
        f.first_seed = f.first_seed.max(state.next_seed());
    }
    state.generate = f;
    state.generate_dirty = None;
}

fn op_done(state: &mut AppState, ok: OpOk) -> Vec<Effect> {
    match ok {
        OpOk::Done | OpOk::Idea(_) => {}
        OpOk::Tiled(_) => state.messages_info("Tiled image added to the input library"),
        OpOk::ImportedRefs(ids) => {
            return update(state, Msg::AddRefs(ids.into_iter().map(ImageRef::input).collect()));
        }
        OpOk::Imported(ids) => {
            for id in ids {
                if !state.draft.images.contains(&id) {
                    state.draft.images.push(id);
                }
            }
            state.idea_dirty = Some(state.now);
        }
        OpOk::Promoted { project, prompt, refs, idea } => {
            let fx = flush_generate(state);
            state.project = Some(project);
            state.proj_folder = None;
            state.tab = Tab::Projects;
            state.projects_tab = ProjectsTab::Generate;
            let mut f = GenerateForm::new(&state.config);
            f.label = "r00".into();
            f.prompt = prompt;
            f.images = images_of(&refs, None);
            f.source_idea = idea;
            state.generate = f;
            mark_generate(state);
            return fx;
        }
        OpOk::Created(tree, path) => {
            let mut fx = vec![];
            if tree == Tree::Projects {
                let dir = std::path::Path::new("projects").join(&path);
                if let Some(id) = state.index.projects.iter().find(|p| p.dir == dir).map(|p| p.project.id.clone()) {
                    fx = flush_generate(state);
                    state.project = Some(id);
                    state.proj_folder = None;
                    load_generate(state);
                    // a project is named before it is made
                    return fx;
                }
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy().into();
            state.renaming = Some(Renaming { tree, name, path, creating: false });
            return fx;
        }
        OpOk::Duplicated(path) => {
            let dir = std::path::Path::new("projects").join(&path);
            let Some(id) = state.index.projects.iter().find(|p| p.dir == dir).map(|p| p.project.id.clone()) else {
                return vec![];
            };
            let fx = flush_generate(state);
            state.project = Some(id);
            state.proj_folder = None;
            load_generate(state);
            return fx;
        }
        OpOk::ProjectReady { path, form } => {
            let dir = std::path::Path::new("projects").join(&path);
            let Some(id) = state.index.projects.iter().find(|p| p.dir == dir).map(|p| p.project.id.clone()) else {
                return vec![];
            };
            let fx = flush_generate(state);
            state.project = Some(id);
            state.proj_folder = None;
            let mut f = form;
            f.label = next_label(state);
            f.first_seed = state.next_seed();
            state.generate = *f;
            mark_generate(state);
            state.tab = Tab::Projects;
            state.projects_tab = ProjectsTab::Generate;
            // named in the dialog already
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            state.messages_info(format!("Project {name} ready"));
            return fx;
        }
        OpOk::SeedsAdded(run, n) => {
            let label = state.index.run(&run).map(|(p, r)| format!("{} · {}", p.name(), r.label)).unwrap_or_default();
            if state.engine_running {
                state.active_runs.insert(run.clone());
                state.messages_info(format!("+{n} seeds queued for {label}"));
                return vec![Effect::QueueRun(run)];
            }
            state.messages_info(format!("+{n} seeds added to {label}; they run once servers are connected"));
        }
        OpOk::Sweep(ids, name, queued) => {
            if state.engine_running {
                state.active_runs.extend(ids.iter().cloned());
            }
            let fx = record_queued(state, *queued, ids.clone());
            state.generate.label = next_label(state);
            state.generate.first_seed = state.next_seed();
            mark_generate(state);
            state.messages_info(format!("Sweep {name} queued: {} runs", ids.len()));
            return fx;
        }
        OpOk::Run(id, queued) => {
            if state.engine_running {
                state.active_runs.insert(id.clone());
            }
            let fx = record_queued(state, *queued, vec![id]);
            state.generate.label = next_label(state);
            state.generate.first_seed = state.next_seed();
            mark_generate(state);
            state.messages_info("Run queued");
            return fx;
        }
        OpOk::Reviewed { link, previous, undo } => {
            if !undo {
                state.review_undo.push((link, previous));
            }
        }
    }
    vec![]
}

fn engine_event(state: &mut AppState, e: Event) -> Vec<Effect> {
    match e {
        Event::Server { server, state: s, model, idle } => {
            if let Some(st) = state.servers.get_mut(server) {
                st.model = model.map(|m| m.recipe);
                let was_idle = st.idle_since.is_some();
                st.idle_since = if idle { st.idle_since.or(Some(state.now)) } else { None };
                if s == "ready" && !idle {
                    st.worked = true;
                    state.alerts.cleared_text = None;
                } else if idle && !was_idle && std::mem::take(&mut st.worked) {
                    state.alerts.cleared.push(server);
                } else if s == "connecting" || s.starts_with("unreachable") {
                    st.worked = false;
                }
                st.state = s;
            }
            vec![]
        }
        Event::Progress { run, seed, step, total, .. } => {
            let now = state.now;
            state
                .sampling_times
                .entry((run.clone(), seed))
                .and_modify(|t| t.latest = (now, step))
                .or_insert_with(|| crate::timing::Sampling::new(now, step));
            state.progress.insert((run, seed), (step, total));
            vec![]
        }
        Event::Output { run, seed, .. } => {
            state.sampling_times.remove(&(run.clone(), seed));
            state.progress.remove(&(run, seed));
            vec![Effect::Rescan]
        }
        Event::SeedFailed { run, seed, error, .. } => {
            state.sampling_times.remove(&(run.clone(), seed));
            state.progress.remove(&(run.clone(), seed));
            state.messages_error(format!("run {run} seed {seed}: {error}"));
            vec![]
        }
        Event::RunDone { run } => {
            state.active_runs.remove(&run);
            state.finishing.remove(&run);
            state.progress.retain(|(r, _), _| r != &run);
            state.sampling_times.retain(|(r, _), _| r != &run);
            vec![Effect::Rescan]
        }
        Event::Problem(p) => {
            state.messages_error(p);
            vec![]
        }
    }
}

/// The notification alerts (DESIGN.md, Notifications), checked once a
/// second: each sounds when its condition first passes its threshold and
/// then every `repeat_minutes` while it holds, unless snoozed, muted or (if
/// so set) the window has the focus; a sound held back plays once that ends.
fn check_alerts(state: &mut AppState) -> Vec<Effect> {
    let n = state.config.notifications.clone();
    let now = state.now;
    let mut fx = vec![];

    // a GPU (server) ready with no job
    let limit = n.gpu_idle.threshold as f64 * 60.0;
    let idle: Vec<(usize, f64)> = state
        .servers
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.idle_since.map(|t| (i, now - t)))
        .filter(|(_, d)| *d >= limit)
        .collect();
    let holds = state.engine_running
        && !state.servers.is_empty()
        && if n.all_gpus { idle.len() == state.servers.len() } else { !idle.is_empty() };
    let text = holds.then(|| {
        let mins = |d: f64| (d / 60.0).floor() as u64;
        if n.all_gpus {
            let least = idle.iter().map(|(_, d)| *d).fold(f64::INFINITY, f64::min);
            format!("All GPUs idle for {} min", mins(least))
        } else {
            let (i, d) = idle.iter().copied().fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
            let s = &state.servers[i];
            let name = if s.device.is_empty() { &s.name } else { &s.device };
            format!("{name} idle for {} min", mins(d))
        }
    });
    fx.extend(alert(state, true, n.gpu_idle.on.then_some(text).flatten()));

    // a GPU just finished its last job: once, at once
    let cleared = std::mem::take(&mut state.alerts.cleared);
    if n.queue_cleared.on && state.engine_running && !cleared.is_empty() {
        let text = if n.queue_cleared_all {
            state.servers.iter().all(|s| !s.worked).then(|| "All GPUs finished their queues".to_string())
        } else {
            let names: Vec<&str> = cleared
                .iter()
                .filter_map(|&i| state.servers.get(i))
                .map(|s| if s.device.is_empty() { s.name.as_str() } else { s.device.as_str() })
                .collect();
            let its = if names.len() == 1 { "its queue" } else { "their queues" };
            Some(format!("{} finished {its}", names.join(", ")))
        };
        if let Some(text) = text {
            state.alerts.cleared_text = Some(text.clone());
            if !state.alerts.held(now, n.only_unfocused) {
                fx.extend(ring(state, n.queue_cleared.sound.clone(), text));
            }
        }
    }

    // the review queue
    let waiting = if state.library.is_some() { state.unreviewed_outputs() } else { 0 };
    let text = (waiting > n.review_backlog.threshold as usize).then(|| format!("{waiting} images waiting for review"));
    fx.extend(alert(state, false, n.review_backlog.on.then_some(text).flatten()));
    fx
}

/// One alert's step: `text` is what holds (`None`: nothing, which resets it).
fn alert(state: &mut AppState, idle: bool, text: Option<String>) -> Vec<Effect> {
    let n = &state.config.notifications;
    let rule = if idle { n.gpu_idle.clone() } else { n.review_backlog.clone() };
    let held = state.alerts.held(state.now, n.only_unfocused);
    let a = &mut state.alerts;
    let (played, shown) =
        if idle { (&mut a.idle_played, &mut a.idle_text) } else { (&mut a.backlog_played, &mut a.backlog_text) };
    *shown = text.clone();
    let Some(text) = text else {
        *played = None;
        return vec![];
    };
    let due = played.is_none_or(|t| state.now - t >= rule.repeat_minutes.max(1) as f64 * 60.0);
    if !due || held {
        return vec![];
    }
    *played = Some(state.now);
    ring(state, rule.sound, text)
}

/// Sound an alert now, with the taskbar flash and message if so set.
fn ring(state: &mut AppState, sound: increart_core::notify::Sound, text: String) -> Vec<Effect> {
    let n = &state.config.notifications;
    if n.flash_taskbar {
        state.alerts.attention = true;
    }
    let fx = vec![Effect::PlaySound { sound, volume: n.volume, player: n.player.clone(), test: false }];
    if n.show_message {
        state.messages_info(text);
    }
    fx
}
