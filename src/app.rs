//! Application state, update and view (toolbar · clans/people · board · inspector · status).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced_markdown_editor::{
    Action, EditorState, Event as EditorEvent, Paper, ZoomSettings, markdown_editor,
};
use iced::widget::{
    Column, Row, Text, button, canvas, column, container, opaque, operation, pick_list, row,
    scrollable, space, stack, text, text_input, tooltip,
};
use iced::{
    Color, Element, Event, Length, Padding, Point, Rectangle, Size, Subscription, Task, Vector,
    alignment, event, keyboard, window,
};

use crate::board::{Board, Snapshot, World};
use crate::camera::Camera;
use crate::files;
use crate::i18n::{t, tf};
use crate::notes::{self, Book, NoteKey};
use crate::layout;
use crate::routing::Routes;
use crate::shell::SettingsPage;
use crate::timeline::RangeSlider;
use crate::model::{
    CLAN_BOX_H, CLAN_BOX_W, Clan, Data, Era, FileRec, Id, LINK_PRESETS, Link, LinkLabelPosition, LinkStyle, NODE_H, NODE_W,
    PALETTE, Person, link_style, new_id, year_num,
};
use crate::spatial::{self, Grid, Hull};
use crate::store::{CLANS, Cmd, Db, FILES, LINKS, PEOPLE, RELATIONS};
use crate::ui;

const SIDEBAR_W: f32 = 256.0;
const INSPECTOR_W: f32 = 330.0;
const TOOLBAR_H: f32 = 54.0;
const STATUS_H: f32 = 28.0;
/// Quiet time after zooming before the scaled board is re-tessellated sharply.
const ZOOM_SETTLE: Duration = Duration::from_millis(160);
const MAX_PEOPLE_LIST: usize = 150;
const MAX_LINES_LIST: usize = 100;
/// Notices (toasts) disappear after this long.
const NOTICE_TTL: Duration = Duration::from_secs(6);
const SEARCH_ID: &str = "people-search";
/// Playing the timeline advances one year per step.
const TIMELINE_STEP: Duration = Duration::from_millis(90);

// ---------------------------------------------------------------------------
// config / boot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Config {
    /// Project folder: holds `wurzelwerk.rdb`, `person/` and `notes/`.
    pub project: PathBuf,
    /// `--demo <clans>x<people per clan>` - only applied to an empty project.
    pub demo: Option<(usize, usize)>,
}

/// Everything `boot` needs. Clone-able so the boot fn stays `Fn`.
#[derive(Debug, Clone)]
pub struct Init {
    pub cfg: Config,
    pub db: Db,
    pub data: Data,
}

// ---------------------------------------------------------------------------
// state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    AddPerson,
    Connect,
    /// Click on the board to place a new (empty) clan.
    Clan,
    /// Click people to add them to / remove them from the active clan.
    Members,
}

/// Collapsible sidebar sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Clans,
    People,
    Books,
}

/// Deletion waiting for confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Person(Id),
    Clan(Id),
    /// The book page open in the editor.
    Page,
}

#[derive(Debug, Clone)]
pub struct PendingLink {
    pub from: Id,
    pub to: Id,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClanChoice {
    pub id: Id,
    pub name: String,
}

impl fmt::Display for ClanChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// A markdown note (person, clan or book page) open in the editor.
pub struct OpenNote {
    pub key: NoteKey,
    pub editor: EditorState,
    /// Editable page name (book pages only).
    pub name: String,
    /// Editable book name (book pages only).
    pub book_name: String,
    /// Unsaved changes (flushed on tick / close / switch).
    pub dirty: bool,
}

pub struct App {
    pub cfg: Config,
    pub db: Db,
    pub data: Data,
    pub grid: Grid,
    pub hulls: HashMap<Id, Hull>,
    /// Routed lines, kept in sync with `data` after every update.
    pub routes: Routes,
    pub clan_counts: HashMap<Id, usize>,
    pub camera: Camera,
    pub canvas_size: Size,
    pub cache: canvas::Cache,
    /// Camera and size `cache` was tessellated for (set while drawing).
    pub snapshot: Cell<Option<Snapshot>>,
    /// Text of the snapshot, in its space; drawn per frame (see `board::World`).
    pub snapshot_text: RefCell<Vec<canvas::Text>>,
    /// Last zoom; the board is re-tessellated at the exact zoom once it settles.
    zoomed_at: Option<Instant>,
    pub tool: Tool,
    pub selected: Option<Id>,
    pub selected_link: Option<Id>,
    pub selected_link_vertex: Option<usize>,
    pub active_clan: Option<Id>,
    pub show_clan: bool,
    pub search: String,
    pub pending_link: Option<PendingLink>,
    /// Person with unsaved text edits (flushed on submit / selection change / tick).
    pub dirty: Option<Id>,
    pub notice: Option<String>,
    /// When the current notice appeared (it hides itself after [`NOTICE_TTL`]).
    pub notice_at: Option<Instant>,
    pub show_sidebar: bool,
    pub collapsed: HashSet<Section>,
    pub confirm: Option<Confirm>,
    /// Last known window size (the board gets what the panels leave).
    pub window: Size,
    /// Markdown editor shown in place of the board.
    pub note: Option<OpenNote>,
    /// Global documentation books found in `notes/books`.
    pub books: Vec<Book>,
    /// Person / clan ids that have a non-empty markdown note (board indicator).
    pub with_notes: HashSet<Id>,
    /// Person ids that have attached files (board indicator).
    pub with_files: HashSet<Id>,
    /// Timeline range `(from, to)`: the board shows the people alive at some point in it
    /// and fades those who died by `to`. `None` = timeline closed, everyone shown.
    pub years: Option<(i32, i32)>,
    /// The timeline runs forward on its own.
    pub playing: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    // toolbar
    Tool(Tool),
    AutoLayout,
    FitToWindow,
    Search(String),
    SearchSubmit,
    FocusSearch,
    ZoomStep(f32),
    ZoomReset,
    ToggleSidebar,
    // timeline
    ToggleTimeline,
    /// New timeline range (from, to).
    SetYears(i32, i32),
    TogglePlay,
    TimelineStep,
    /// Handled by the shell (light / dark), not by the workspace.
    ToggleTheme,
    /// Single-key board shortcut; ignored while a note is open or a dialog is shown.
    Shortcut(Box<Message>),
    DismissNotice,
    // sidebar
    SelectPerson(Option<Id>),
    FocusPerson(Id),
    SelectClan(Id),
    /// Opens a clan in the inspector (board click; never toggles).
    ShowClan(Id),
    DeselectClan,
    NewClan,
    AddClanAt(Point),
    ClanMoved(Id, [f32; 2]),
    ToggleSection(Section),
    AddPersonCentre,
    // clan inspector
    RenameClan(String),
    SetClanColor(usize),
    FitClan,
    DeleteClan,
    // board
    ZoomAt(Point, f32),
    PanBy(Vector),
    NodeDragged(Id, Point),
    NodeDropped(Id),
    AddPersonAt(Point),
    ToggleClan(Id),
    ConnectRequested(Id, Id),
    SelectLink(Id),
    SelectLinkVertex(Id, usize, Vec<[f32; 2]>),
    DeleteLinkVertex(Id, usize),
    ResetLinkRoute(Id),
    LinkRouteDragged(Id, Vec<[f32; 2]>, Option<LinkLabelPosition>, u8, u8),
    LinkLabelDragged(Id, LinkLabelPosition),
    LinkDropped(Id),
    ClanLabelDragged(Id, [f32; 2]),
    ClanLabelDropped(Id),
    // line naming
    LinkLabel(String),
    LinkPreset(String),
    ConfirmLink,
    CancelLink,
    DeleteLink(Id),
    EditLinkLabel(String),
    SetLinkLabel(String),
    /// Keeps the selected line's name as a relation (Enter in its name field).
    RememberLinkLabel,
    // relations (workspace settings)
    RenameRelation(Id, &'static str, String),
    DeleteRelation(Id),
    // confirmation
    AskDelete(Confirm),
    ConfirmDelete,
    CancelDelete,
    // person inspector
    EditName(String),
    EditBorn(String),
    EditDied(String),
    CommitPerson,
    DeletePerson,
    AddToClan(ClanChoice),
    RemoveFromClan(Id),
    // files
    ChooseFiles,
    FilesChosen(Vec<PathBuf>),
    FileDropped(PathBuf),
    FilesImported(Result<Vec<FileRec>, String>),
    OpenFile(Id),
    DeleteFile(Id),
    // notes & books
    OpenNote(NoteKey),
    CloseNote,
    NoteEditor(Action),
    NoteName(String),
    RenameNote,
    BookName(String),
    RenameBook,
    DeleteNote,
    NewBook,
    NewPage(String),
    /// Handled by the shell (settings dialog), not by the workspace.
    OpenSettings(SettingsPage),
    // runtime
    WindowResized(Size),
    Escape,
    DeleteKey,
    EnterKey,
    Tick,
    ZoomSettled,
}

impl App {
    pub fn boot(init: Init) -> (Self, Task<Message>) {
        let Init { cfg, db, mut data } = init;

        // one-time move from the old `files/<id>` and `notes/people/<id>.md` layout
        for rec in files::migrate_legacy(&cfg.project, &mut data.files) {
            db.send(Cmd::put(FILES, &rec.id, &rec, false));
        }
        notes::migrate_legacy(&cfg.project);
        // workspaces from before relations existed: their own line names become relations
        if data.relations.is_empty() {
            let labels: Vec<String> = data.links.iter().map(|l| l.label.clone()).collect();
            for label in labels {
                if let Some(r) = data.remember_relation(&label) {
                    db.send(Cmd::put(RELATIONS, &r.id, r, true));
                }
            }
        }

        if data.people.is_empty()
            && let Some((clans, per_clan)) = cfg.demo
        {
            let (c, p, l) = crate::demo::generate(clans, per_clan);
            data = Data::from_parts(p, c, l, Vec::new());
            layout::auto_layout(&mut data);
            let mut cmds =
                Vec::with_capacity(data.people.len() + data.links.len() + data.clans.len());
            cmds.extend(data.clans.iter().map(|c| Cmd::put(CLANS, &c.id, c, true)));
            cmds.extend(data.people.iter().map(|p| Cmd::put(PEOPLE, &p.id, p, true)));
            cmds.extend(data.links.iter().map(|l| Cmd::put(LINKS, &l.id, l, true)));
            db.send(Cmd::Batch(cmds));
            for (key, markdown) in crate::demo::notes(&data) {
                let _ = notes::save(&key.path(&cfg.project), &markdown);
            }
        }

        let mut app = Self {
            cfg,
            db,
            data,
            grid: Grid::default(),
            hulls: HashMap::new(),
            routes: Routes::default(),
            clan_counts: HashMap::new(),
            camera: Camera::default(),
            canvas_size: Size::ZERO,
            cache: canvas::Cache::new(),
            snapshot: Cell::new(None),
            snapshot_text: RefCell::new(Vec::new()),
            zoomed_at: None,
            tool: Tool::Select,
            selected: None,
            selected_link: None,
            selected_link_vertex: None,
            active_clan: None,
            show_clan: false,
            search: String::new(),
            pending_link: None,
            dirty: None,
            notice: None,
            notice_at: None,
            show_sidebar: true,
            collapsed: HashSet::new(),
            confirm: None,
            window: Size::new(1440.0, 900.0),
            note: None,
            books: Vec::new(),
            with_notes: HashSet::new(),
            with_files: HashSet::new(),
            years: None,
            playing: false,
        };
        app.books = notes::scan_books(&app.cfg.project);
        app.with_notes.extend(notes::noted_people(&app.cfg.project));
        app.with_notes
            .extend(notes::noted_ids(&app.cfg.project, "clans"));
        app.refresh_file_marks();
        app.relayout();
        app.rebuild_all();
        app.fit_all();
        (app, Task::none())
    }

    /// Folder name of the open workspace.
    pub fn workspace_name(&self) -> String {
        self.cfg
            .project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.cfg.project.display().to_string())
    }

    /// Writes pending edits; called before the workspace is closed or replaced.
    pub fn shutdown(&mut self) {
        self.flush_dirty();
        self.flush_note();
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let events = event::listen_with(|ev, status, _window| match ev {
            Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
            Event::Window(window::Event::Resized(size)) => Some(Message::WindowResized(size)),
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if status == event::Status::Ignored =>
            {
                shortcut(&key, modifiers)
            }
            _ => None,
        });

        let mut subs = vec![events];
        if self.db.pending() > 0
            || self.dirty.is_some()
            || self.notice.is_some()
            || self.note.as_ref().is_some_and(|n| n.dirty)
        {
            subs.push(iced::time::every(Duration::from_millis(400)).map(|_| Message::Tick));
        }
        if self.zoomed_at.is_some() {
            subs.push(iced::time::every(ZOOM_SETTLE / 2).map(|_| Message::ZoomSettled));
        }
        if self.playing {
            subs.push(iced::time::every(TIMELINE_STEP).map(|_| Message::TimelineStep));
        }
        if let Some(note) = &self.note {
            subs.push(note.editor.subscription().map(Message::NoteEditor));
        }
        Subscription::batch(subs)
    }

    // -----------------------------------------------------------------------
    // queries used by the board
    // -----------------------------------------------------------------------

    pub fn selected_person(&self) -> Option<&Person> {
        self.selected.as_deref().and_then(|id| self.data.person(id))
    }

    pub fn hit_test(&self, world: Point) -> Option<usize> {
        let probe = Rectangle {
            x: world.x,
            y: world.y,
            width: 1.0,
            height: 1.0,
        };
        let mut hit = None;
        self.grid.query(probe, |i| {
            let p = &self.data.people[i as usize];
            if self.shown(p)
                && world.x >= p.x
                && world.x <= p.x + NODE_W
                && world.y >= p.y
                && world.y <= p.y + NODE_H
            {
                hit = Some(i as usize);
            }
        });
        hit
    }

    /// On the board: alive at some point in the timeline range (always, with the timeline closed).
    pub fn shown(&self, p: &Person) -> bool {
        self.years.is_none_or(|(from, to)| p.lived_within(from, to))
    }

    /// Died by the end of the timeline range: drawn faded.
    pub fn faded(&self, p: &Person) -> bool {
        self.years.is_some_and(|(_, to)| p.era(to) == Era::Dead)
    }

    /// A line is drawn while both of its people are shown, and a line whose name holds a
    /// year ("married 1789") once the range reaches that year.
    pub fn link_shown(&self, link: &Link) -> bool {
        let Some((_, to)) = self.years else {
            return true;
        };
        year_num(&link.label).is_none_or(|since| to >= since)
            && [&link.from, &link.to]
                .into_iter()
                .all(|id| self.data.person(id).is_some_and(|p| self.shown(p)))
    }

    /// First and last year mentioned in any birth or death date.
    pub fn year_span(&self) -> Option<(i32, i32)> {
        self.data
            .people
            .iter()
            .flat_map(|p| [year_num(&p.born), year_num(&p.died)])
            .flatten()
            .fold(None, |span, y| match span {
                None => Some((y, y)),
                Some((first, last)) => Some((y.min(first), y.max(last))),
            })
    }

    // -----------------------------------------------------------------------
    // derived data
    // -----------------------------------------------------------------------

    fn rebuild_all(&mut self) {
        self.grid = Grid::build(&self.data.people);
        self.rebuild_hulls();
        self.routes.sync(&self.data);
        self.recount();
    }

    /// Hulls of all clans around the people shown in the timeline range.
    fn rebuild_hulls(&mut self) {
        self.hulls = spatial::all_hulls(
            self.data.people.iter().filter(|p| self.shown(p)),
            &self.data.clans,
        );
    }

    /// Moves the timeline range (`from`, `to`); drops a selection that is no longer shown.
    fn set_years(&mut self, years: Option<(i32, i32)>) {
        self.years = years.map(|(from, to)| (from.min(to), from.max(to)));
        self.rebuild_hulls();
        if self.selected_person().is_some_and(|p| !self.shown(p)) {
            self.flush_dirty();
            self.selected = None;
        }
        if let Some(id) = &self.selected_link
            && self
                .data
                .links
                .iter()
                .find(|link| link.id == *id)
                .is_some_and(|link| !self.link_shown(link))
        {
            self.selected_link = None;
            self.selected_link_vertex = None;
        }
    }

    fn recount(&mut self) {
        self.clan_counts.clear();
        for p in &self.data.people {
            for c in &p.clans {
                *self.clan_counts.entry(c.clone()).or_default() += 1;
            }
        }
    }

    fn rebuild_hulls_for(&mut self, clans: &[Id]) {
        for c in clans {
            match spatial::clan_hull(self.data.people.iter().filter(|p| self.shown(p)), c) {
                Some(h) => {
                    self.hulls.insert(c.clone(), h);
                }
                None => {
                    self.hulls.remove(c);
                }
            }
        }
    }

    fn world_bounds(&self) -> Option<Rectangle> {
        let mut it = self.data.people.iter();
        let first = it.next()?;
        let (mut x0, mut y0, mut x1, mut y1) =
            (first.x, first.y, first.x + NODE_W, first.y + NODE_H);
        for p in it {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x + NODE_W);
            y1 = y1.max(p.y + NODE_H);
        }
        Some(Rectangle {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    /// Clan without members whose box contains `world`.
    pub fn empty_clan_at(&self, world: Point) -> Option<Id> {
        self.data
            .clans
            .iter()
            .rev()
            .filter(|c| self.clan_counts.get(&c.id).copied().unwrap_or(0) == 0)
            .find(|c| c.box_rect().is_some_and(|r| r.contains(world)))
            .map(|c| c.id.clone())
    }

    /// Board size = window minus toolbar, status bar and the visible panels.
    fn relayout(&mut self) {
        let side = if self.show_sidebar { SIDEBAR_W + 1.0 } else { 0.0 };
        self.canvas_size = Size::new(
            (self.window.width - side - INSPECTOR_W - 1.0).max(100.0),
            (self.window.height - TOOLBAR_H - STATUS_H - 2.0).max(100.0),
        );
    }

    /// Re-applies the palette to the open editor after a light / dark switch.
    pub fn restyle(&mut self) {
        if let Some(note) = &mut self.note {
            note.editor.set_style(ui::editor_style());
        }
        self.cache.clear();
    }

    fn fit_all(&mut self) {
        if let Some(bounds) = self.world_bounds() {
            self.camera.fit(bounds, self.canvas_size);
        }
    }

    fn focus(&mut self, id: &str) {
        if let Some(p) = self.data.person(id) {
            let center = Point::new(p.x + NODE_W / 2.0, p.y + NODE_H / 2.0);
            self.camera.zoom = self.camera.zoom.max(0.9);
            self.camera.center_on(center, self.canvas_size);
        }
    }

    // -----------------------------------------------------------------------
    // persistence helpers
    // -----------------------------------------------------------------------

    fn save_person(&self, id: &str) {
        if let Some(p) = self.data.person(id) {
            self.db.send(Cmd::put(PEOPLE, &p.id, p, false));
        }
    }

    fn save_clan(&self, id: &str) {
        if let Some(c) = self.data.clan(id) {
            self.db.send(Cmd::put(CLANS, &c.id, c, false));
        }
    }

    fn remember_relation(&mut self, label: &str) {
        if let Some(r) = self.data.remember_relation(label) {
            self.db.send(Cmd::put(RELATIONS, &r.id, r, true));
        }
    }

    /// Suggested line names (in the active language), then the workspace's own ones.
    fn relation_names(&self) -> impl Iterator<Item = &str> {
        LINK_PRESETS
            .iter()
            .map(|&id| t(id))
            .chain(self.data.relations.iter().map(|r| r.name()))
            .filter(|name| !name.is_empty())
    }

    fn save_link(&self, id: &str) {
        if let Some(link) = self.data.links.iter().find(|link| link.id == id) {
            self.db.send(Cmd::put(LINKS, &link.id, link, false));
        }
    }

    fn flush_dirty(&mut self) {
        if let Some(id) = self.dirty.take() {
            self.save_person(&id);
            if let Some(p) = self.data.person(&id) {
                notes::refresh_preamble(&self.cfg.project, p);
            }
        }
    }

    // -----------------------------------------------------------------------
    // update
    // -----------------------------------------------------------------------

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let redraw = !matches!(
            message,
            Message::Tick
                | Message::ZoomSettled
                | Message::Search(_)
                | Message::DismissNotice
                | Message::FocusSearch
        );
        // the cached board is only moved and scaled on the GPU for these (see `board::World`)
        let camera_only = matches!(
            message,
            Message::ZoomAt(..)
                | Message::ZoomStep(_)
                | Message::ZoomReset
                | Message::PanBy(_)
                | Message::FitToWindow
        );
        if matches!(
            message,
            Message::ZoomAt(..) | Message::ZoomStep(_) | Message::ZoomReset | Message::FitToWindow
        ) {
            self.zoomed_at = Some(Instant::now());
        }
        // camera-only messages never move cards or change lines
        let geometry = redraw
            && !matches!(
                message,
                Message::ZoomAt(..)
                    | Message::ZoomStep(_)
                    | Message::ZoomReset
                    | Message::PanBy(_)
                    | Message::FitToWindow
                    | Message::WindowResized(_)
                    | Message::ToggleSidebar
                    | Message::ToggleTimeline
                    | Message::SetYears(..)
                    | Message::TogglePlay
                    | Message::TimelineStep
            );
        let notice = self.notice.clone();
        let task = self.handle(message);
        if self.notice.is_some() && self.notice != notice {
            self.notice_at = Some(Instant::now());
        }
        if geometry {
            // incremental: only lines near moved cards or with edited inputs are re-routed
            self.routes.sync(&self.data);
        }
        if redraw && !camera_only {
            self.cache.clear();
        }
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tool(tool) => {
                self.flush_dirty();
                self.close_note();
                self.tool = tool;
                self.pending_link = None;
                self.notice = (tool == Tool::Members && self.active_clan.is_none()).then(|| {
                    t("notice-select-clan").into()
                });
            }
            Message::AutoLayout => {
                self.flush_dirty();
                layout::auto_layout(&mut self.data);
                self.selected_link_vertex = None;
                let mut cmds: Vec<Cmd> = self
                    .data
                    .people
                    .iter()
                    .map(|p| Cmd::put(PEOPLE, &p.id, p, false))
                    .collect();
                for link in &mut self.data.links {
                    if !link.route.is_empty()
                        || link.start_port.is_some()
                        || link.end_port.is_some()
                    {
                        link.route.clear();
                        link.start_port = None;
                        link.end_port = None;
                        cmds.push(Cmd::put(LINKS, &link.id, link, false));
                    }
                }
                self.db.send(Cmd::Batch(cmds));
                self.rebuild_all();
                self.fit_all();
            }
            Message::FitToWindow => self.fit_all(),
            Message::ZoomStep(factor) => {
                let centre = Point::new(self.canvas_size.width / 2.0, self.canvas_size.height / 2.0);
                self.camera.zoom_at(centre, factor);
            }
            Message::ZoomReset => {
                let centre = Point::new(self.canvas_size.width / 2.0, self.canvas_size.height / 2.0);
                self.camera.zoom_at(centre, 1.0 / self.camera.zoom);
            }
            Message::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                self.relayout();
            }
            Message::ToggleTimeline => {
                self.playing = false;
                if self.years.is_some() {
                    self.set_years(None);
                } else if let Some(span) = self.year_span() {
                    // opens on the full tree; play or drag the ends to watch it grow
                    self.set_years(Some(span));
                } else {
                    self.notice = Some(t("notice-no-dates").into());
                }
            }
            Message::SetYears(from, to) => {
                self.playing = false;
                self.set_years(Some((from, to)));
            }
            Message::TogglePlay => {
                if let (Some((from, to)), Some((_, last))) = (self.years, self.year_span()) {
                    self.playing = !self.playing;
                    // played to the end: start again from the left end of the range
                    if self.playing && to >= last {
                        self.set_years(Some((from, from)));
                    }
                }
            }
            Message::TimelineStep => match (self.years, self.year_span()) {
                (Some((from, to)), Some((_, last))) if self.playing => {
                    let next = (to + 1).min(last);
                    self.playing = next < last;
                    self.set_years(Some((from, next)));
                }
                _ => self.playing = false,
            },
            Message::ToggleTheme => {}
            Message::FocusSearch => return operation::focus(SEARCH_ID),
            Message::Shortcut(msg) => {
                if self.note.is_none() && self.confirm.is_none() && self.pending_link.is_none() {
                    return self.handle(*msg);
                }
            }
            Message::DismissNotice => {
                self.notice = None;
                self.notice_at = None;
            }
            Message::ToggleSection(section) => {
                if !self.collapsed.remove(&section) {
                    self.collapsed.insert(section);
                }
            }
            Message::AddPersonCentre => {
                self.close_note();
                let centre = Point::new(self.canvas_size.width / 2.0, self.canvas_size.height / 2.0);
                let world = self.camera.to_world(centre);
                return self.handle(Message::AddPersonAt(world));
            }
            Message::Search(query) => self.search = query,
            Message::SearchSubmit => {
                let q = self.search.trim().to_lowercase();
                if let Some(id) = self
                    .data
                    .people
                    .iter()
                    .find(|p| !q.is_empty() && p.name.to_lowercase().contains(&q))
                    .map(|p| p.id.clone())
                {
                    self.flush_dirty();
                    self.selected = Some(id.clone());
                    self.selected_link = None;
                    self.show_clan = false;
                    self.focus(&id);
                }
            }

            Message::SelectPerson(id) => {
                if id != self.selected {
                    self.flush_dirty();
                    self.close_note();
                }
                self.selected = id;
                self.selected_link = None;
                self.selected_link_vertex = None;
                self.show_clan = false;
            }
            Message::SelectLink(id) => {
                self.flush_dirty();
                self.close_note();
                self.selected = None;
                self.selected_link = Some(id);
                self.selected_link_vertex = None;
                self.show_clan = false;
            }
            Message::SelectLinkVertex(id, vertex, route) => {
                self.flush_dirty();
                self.selected = None;
                self.selected_link = Some(id.clone());
                self.selected_link_vertex = Some(vertex);
                self.show_clan = false;
                if let Some(link) = self.data.links.iter_mut().find(|link| link.id == id) {
                    link.route = route;
                }
            }
            Message::FocusPerson(id) => {
                self.flush_dirty();
                self.close_note();
                self.selected = Some(id.clone());
                self.selected_link = None;
                self.show_clan = false;
                self.focus(&id);
            }
            Message::SelectClan(id) if self.active_clan.as_ref() == Some(&id) => {
                // clicking the active clan again deselects it
                return self.handle(Message::DeselectClan);
            }
            Message::DeselectClan => {
                self.active_clan = None;
                self.show_clan = false;
                if self.tool == Tool::Members {
                    self.tool = Tool::Select;
                }
            }
            Message::ShowClan(id) => {
                self.flush_dirty();
                self.close_note();
                self.active_clan = Some(id);
                self.show_clan = true;
                self.selected = None;
                self.selected_link = None;
            }
            Message::SelectClan(id) => {
                self.flush_dirty();
                self.close_note();
                self.active_clan = Some(id);
                self.show_clan = true;
                self.selected = None;
                self.selected_link = None;
                self.notice = None;
            }
            Message::NewClan => {
                let centre = Point::new(self.canvas_size.width / 2.0, self.canvas_size.height / 2.0);
                return self.handle(Message::AddClanAt(self.camera.to_world(centre)));
            }
            Message::ClanMoved(id, position) => {
                if let Some(clan) = self.data.clan_mut(&id) {
                    clan.position = Some(position);
                }
            }
            Message::AddClanAt(world) => {
                self.flush_dirty();
                self.close_note();
                let n = self.data.clans.len();
                let clan = Clan {
                    id: new_id(),
                    name: tf("default-clan-name", &[("number", (n + 1).into())]),
                    color: PALETTE[n % PALETTE.len()],
                    label_offset: [0.0, 0.0],
                    position: Some([world.x - CLAN_BOX_W / 2.0, world.y - CLAN_BOX_H / 2.0]),
                };
                if self.tool == Tool::Clan {
                    self.tool = Tool::Select;
                }
                self.db.send(Cmd::put(CLANS, &clan.id, &clan, true));
                self.active_clan = Some(clan.id.clone());
                self.show_clan = true;
                self.selected = None;
                self.selected_link = None;
                self.data.clans.push(clan);
                self.data.reindex_clans();
            }

            Message::RenameClan(name) => {
                if let Some(id) = self.active_clan.clone()
                    && let Some(c) = self.data.clan_mut(&id)
                {
                    c.name = name;
                    self.save_clan(&id);
                }
            }
            Message::SetClanColor(i) => {
                if let Some(id) = self.active_clan.clone()
                    && let Some(c) = self.data.clan_mut(&id)
                    && let Some(color) = PALETTE.get(i)
                {
                    c.color = *color;
                    self.save_clan(&id);
                }
            }
            Message::FitClan => {
                let bbox = self
                    .active_clan
                    .as_ref()
                    .and_then(|id| self.hulls.get(id))
                    .map(|h| h.bbox);
                if let Some(bbox) = bbox {
                    self.camera.fit(bbox, self.canvas_size);
                }
            }
            Message::DeleteClan => {
                if let Some(id) = self.active_clan.take() {
                    let mut cmds = vec![Cmd::del(CLANS, &id)];
                    for p in self
                        .data
                        .people
                        .iter_mut()
                        .filter(|p| p.clans.contains(&id))
                    {
                        p.clans.retain(|c| *c != id);
                        cmds.push(Cmd::put(PEOPLE, &p.id, &*p, false));
                    }
                    self.db.send(Cmd::Batch(cmds));
                    self.forget_note(NoteKey::Clan(id.clone()));
                    self.data.clans.retain(|c| c.id != id);
                    self.data.reindex_clans();
                    self.hulls.remove(&id);
                    self.recount();
                    self.show_clan = false;
                    if self.tool == Tool::Members {
                        self.tool = Tool::Select;
                    }
                }
            }

            Message::ZoomAt(anchor, factor) => self.camera.zoom_at(anchor, factor),
            Message::PanBy(delta) => self.camera.pan_screen(delta),
            Message::NodeDragged(id, to) => {
                if let Some(&i) = self.data.person_idx.get(&id) {
                    let p = &mut self.data.people[i];
                    let from = Point::new(p.x, p.y);
                    p.x = to.x;
                    p.y = to.y;
                    let clans = p.clans.clone();
                    self.grid.move_item(i as u32, from, to);
                    self.rebuild_hulls_for(&clans);
                }
            }
            Message::NodeDropped(id) => {
                self.save_person(&id);
                for link in self
                    .data
                    .links
                    .iter()
                    .filter(|link| link.from == id || link.to == id)
                {
                    self.db.send(Cmd::put(LINKS, &link.id, link, false));
                }
            }
            Message::LinkRouteDragged(id, route, label_position, start_port, end_port) => {
                if let Some(link) = self.data.links.iter_mut().find(|link| link.id == id) {
                    link.route = route;
                    link.start_port = Some(start_port);
                    link.end_port = Some(end_port);
                    if label_position.is_some() {
                        link.label_position = label_position;
                    }
                }
            }
            Message::LinkLabelDragged(id, position) => {
                if let Some(link) = self.data.links.iter_mut().find(|link| link.id == id) {
                    link.label_position = Some(position);
                }
            }
            Message::LinkDropped(id) => self.save_link(&id),
            Message::DeleteLinkVertex(id, vertex) => {
                if let Some(link) = self.data.links.iter_mut().find(|link| link.id == id)
                    && vertex < link.route.len()
                {
                    link.route.remove(vertex);
                    if link.route.is_empty() {
                        link.start_port = None;
                        link.end_port = None;
                    }
                    link.label_position = None;
                    self.selected_link_vertex = None;
                    self.save_link(&id);
                }
            }
            Message::ResetLinkRoute(id) => {
                if let Some(link) = self.data.links.iter_mut().find(|link| link.id == id) {
                    link.route.clear();
                    link.start_port = None;
                    link.end_port = None;
                    link.label_position = None;
                    self.selected_link_vertex = None;
                    self.save_link(&id);
                }
            }
            Message::ClanLabelDragged(id, offset) => {
                if let Some(clan) = self.data.clan_mut(&id) {
                    clan.label_offset = offset;
                }
            }
            Message::ClanLabelDropped(id) => self.save_clan(&id),
            Message::AddPersonAt(world) => {
                self.flush_dirty();
                if let Some(id) = self.empty_clan_at(world) {
                    self.active_clan = Some(id);
                }
                let person = Person {
                    id: new_id(),
                    name: t("default-person-name").into(),
                    born: String::new(),
                    died: String::new(),
                    x: world.x - NODE_W / 2.0,
                    y: world.y - NODE_H / 2.0,
                    clans: self.active_clan.iter().cloned().collect(),
                };
                self.db.send(Cmd::put(PEOPLE, &person.id, &person, true));
                let id = person.id.clone();
                let clans = person.clans.clone();
                let i = self.data.push_person(person);
                self.grid.insert(
                    i as u32,
                    Point::new(world.x - NODE_W / 2.0, world.y - NODE_H / 2.0),
                );
                self.rebuild_hulls_for(&clans);
                self.recount();
                self.selected = Some(id);
                self.show_clan = false;
                self.tool = Tool::Select;
            }
            Message::ToggleClan(person_id) => {
                if let Some(clan_id) = self.active_clan.clone()
                    && let Some(p) = self.data.person_mut(&person_id)
                {
                    if let Some(pos) = p.clans.iter().position(|c| *c == clan_id) {
                        p.clans.remove(pos);
                    } else {
                        p.clans.push(clan_id.clone());
                    }
                    self.save_person(&person_id);
                    self.rebuild_hulls_for(&[clan_id]);
                    self.recount();
                }
            }
            Message::ConnectRequested(from, to) => {
                self.flush_dirty();
                self.pending_link = Some(PendingLink {
                    from,
                    to,
                    label: String::new(),
                });
            }

            Message::LinkLabel(label) => {
                if let Some(pl) = &mut self.pending_link {
                    pl.label = label;
                }
            }
            Message::LinkPreset(label) => {
                if let Some(pl) = &mut self.pending_link {
                    pl.label = label;
                }
                return Task::done(Message::ConfirmLink);
            }
            Message::ConfirmLink => {
                if let Some(pl) = self.pending_link.take() {
                    let label = pl.label.trim();
                    let link = Link {
                        id: new_id(),
                        from: pl.from,
                        to: pl.to,
                        label: if label.is_empty() {
                            "related to".into()
                        } else {
                            label.to_string()
                        },
                        route: Vec::new(),
                        label_position: None,
                        start_port: None,
                        end_port: None,
                    };
                    self.db.send(Cmd::put(LINKS, &link.id, &link, true));
                    self.remember_relation(&link.label);
                    self.data.links.push(link);
                    if self.tool == Tool::Connect {
                        self.tool = Tool::Select;
                    }
                }
            }
            Message::CancelLink => self.pending_link = None,
            Message::EditLinkLabel(label) => {
                if let Some(id) = self.selected_link.clone()
                    && let Some(link) = self.data.links.iter_mut().find(|link| link.id == id)
                {
                    link.label = label;
                    self.save_link(&id);
                }
            }
            Message::SetLinkLabel(label) => return self.handle(Message::EditLinkLabel(label)),
            Message::RememberLinkLabel => {
                if let Some(label) = self
                    .selected_link
                    .as_ref()
                    .and_then(|id| self.data.links.iter().find(|l| &l.id == id))
                    .map(|l| l.label.clone())
                {
                    self.remember_relation(&label);
                }
            }
            Message::RenameRelation(id, locale, name) => {
                for link in self.data.rename_relation(&id, locale, name) {
                    self.save_link(&link);
                }
                if let Some(r) = self.data.relations.iter().find(|r| r.id == id) {
                    self.db.send(Cmd::put(RELATIONS, &r.id, r, false));
                }
            }
            Message::DeleteRelation(id) => {
                self.data.relations.retain(|r| r.id != id);
                self.db.send(Cmd::del(RELATIONS, &id));
            }
            Message::AskDelete(confirm) => self.confirm = Some(confirm),
            Message::CancelDelete => self.confirm = None,
            Message::ConfirmDelete => match self.confirm.take() {
                Some(Confirm::Person(id)) => {
                    self.selected = Some(id);
                    return self.handle(Message::DeletePerson);
                }
                Some(Confirm::Clan(id)) => {
                    self.active_clan = Some(id);
                    return self.handle(Message::DeleteClan);
                }
                Some(Confirm::Page) => return self.handle(Message::DeleteNote),
                None => {}
            },
            Message::EnterKey => {
                if self.confirm.is_some() {
                    return self.handle(Message::ConfirmDelete);
                }
            }
            Message::DeleteLink(id) => {
                self.data.links.retain(|l| l.id != id);
                self.db.send(Cmd::del(LINKS, &id));
                if self.selected_link.as_deref() == Some(id.as_str()) {
                    self.selected_link = None;
                    self.selected_link_vertex = None;
                }
            }

            Message::EditName(v) => self.edit_selected(|p| p.name = v),
            Message::EditBorn(v) => self.edit_selected(|p| p.born = v),
            Message::EditDied(v) => self.edit_selected(|p| p.died = v),
            Message::CommitPerson => self.flush_dirty(),
            Message::DeletePerson => {
                if let Some(id) = self.selected.take() {
                    self.delete_person(&id);
                }
            }
            Message::DeleteKey if self.confirm.is_some() || self.note.is_some() => {}
            Message::DeleteKey => {
                if let (Some(id), Some(vertex)) =
                    (self.selected_link.clone(), self.selected_link_vertex.take())
                {
                    if let Some(link) = self.data.links.iter_mut().find(|link| link.id == id)
                        && vertex < link.route.len()
                    {
                        link.route.remove(vertex);
                        if link.route.is_empty() {
                            link.start_port = None;
                            link.end_port = None;
                        }
                        link.label_position = None;
                        self.save_link(&id);
                    }
                } else if let Some(id) = self.selected_link.take() {
                    self.data.links.retain(|link| link.id != id);
                    self.db.send(Cmd::del(LINKS, &id));
                } else if let Some(id) = self.selected.clone() {
                    self.confirm = Some(Confirm::Person(id));
                }
            }
            Message::AddToClan(choice) => {
                if let Some(id) = self.selected.clone()
                    && let Some(p) = self.data.person_mut(&id)
                    && !p.clans.contains(&choice.id)
                {
                    p.clans.push(choice.id.clone());
                    self.save_person(&id);
                    self.rebuild_hulls_for(&[choice.id]);
                    self.recount();
                }
            }
            Message::RemoveFromClan(clan_id) => {
                if let Some(id) = self.selected.clone()
                    && let Some(p) = self.data.person_mut(&id)
                {
                    p.clans.retain(|c| *c != clan_id);
                    self.save_person(&id);
                    self.rebuild_hulls_for(&[clan_id]);
                    self.recount();
                }
            }

            Message::ChooseFiles => {
                if self.selected.is_some() {
                    return Task::perform(
                        async {
                            rfd::AsyncFileDialog::new()
                                .set_title(t("person-attach-files"))
                                .pick_files()
                                .await
                                .map(|handles| {
                                    handles
                                        .iter()
                                        .map(|h| h.path().to_path_buf())
                                        .collect::<Vec<PathBuf>>()
                                })
                                .unwrap_or_default()
                        },
                        Message::FilesChosen,
                    );
                }
                self.notice = Some(t("notice-select-person").into());
            }
            Message::FileDropped(path) => return self.import_files(vec![path]),
            Message::FilesChosen(paths) => return self.import_files(paths),
            Message::FilesImported(result) => match result {
                Ok(recs) => {
                    for rec in recs {
                        self.db.send(Cmd::put(FILES, &rec.id, &rec, true));
                        self.data.files.push(rec);
                    }
                    self.refresh_file_marks();
                }
                Err(e) => self.notice = Some(e),
            },
            Message::OpenFile(id) => {
                if let Some(rec) = self.data.files.iter().find(|f| f.id == id)
                    && let Err(e) = files::open(&files::absolute(&self.cfg.project, rec))
                {
                    self.notice = Some(e);
                }
            }
            Message::DeleteFile(id) => {
                if let Some(pos) = self.data.files.iter().position(|f| f.id == id) {
                    let rec = self.data.files.remove(pos);
                    let _ = std::fs::remove_file(files::absolute(&self.cfg.project, &rec));
                    self.db.send(Cmd::del(FILES, &rec.id));
                    self.refresh_file_marks();
                }
            }

            Message::WindowResized(size) => {
                self.window = size;
                self.relayout();
            }
            Message::Escape => {
                if self.confirm.take().is_some() {
                } else if self.pending_link.take().is_none() {
                    if self.tool != Tool::Select {
                        self.tool = Tool::Select;
                    } else if self.selected.is_some()
                        || self.selected_link.is_some()
                        || self.show_clan
                    {
                        self.flush_dirty();
                        self.selected = None;
                        self.selected_link = None;
                        self.show_clan = false;
                    } else {
                        // nothing left to deselect: new people go to no clan
                        self.active_clan = None;
                    }
                }
            }
            Message::Tick => {
                self.flush_dirty();
                self.flush_note();
                if self.notice_at.is_some_and(|at| at.elapsed() >= NOTICE_TTL) {
                    self.notice = None;
                    self.notice_at = None;
                }
            }
            Message::ZoomSettled => {
                if self.zoomed_at.is_some_and(|at| at.elapsed() >= ZOOM_SETTLE) {
                    self.zoomed_at = None;
                    if self.snapshot.get().is_some_and(|s| s.scaled(self.camera)) {
                        self.cache.clear();
                    }
                }
            }
            Message::OpenSettings(_) => {}

            Message::OpenNote(key) => self.open_note(key),
            Message::CloseNote => self.close_note(),
            Message::NoteEditor(action) => {
                if let Some(note) = &mut self.note {
                    let output = note.editor.update(action);
                    if output.events.contains(&EditorEvent::Changed) {
                        note.dirty = true;
                    }
                    return output.task.map(Message::NoteEditor);
                }
            }
            Message::NoteName(name) => {
                if let Some(note) = &mut self.note {
                    note.name = name;
                }
            }
            Message::RenameNote => {
                self.flush_note();
                if let Some(note) = &mut self.note {
                    match notes::rename_page(&self.cfg.project, &note.key, &note.name) {
                        Ok(key) => {
                            if let NoteKey::Page { page, .. } = &key {
                                note.name = page.clone();
                            }
                            note.key = key;
                            self.notice = None;
                        }
                        Err(e) => self.notice = Some(e),
                    }
                    self.books = notes::scan_books(&self.cfg.project);
                }
            }
            Message::BookName(name) => {
                if let Some(note) = &mut self.note {
                    note.book_name = name;
                }
            }
            Message::RenameBook => {
                self.flush_note();
                if let Some(note) = &mut self.note
                    && let NoteKey::Page { book, page } = &note.key
                {
                    match notes::rename_book(&self.cfg.project, book, &note.book_name) {
                        Ok(new_book) => {
                            note.book_name = new_book.clone();
                            note.key = NoteKey::Page {
                                book: new_book,
                                page: page.clone(),
                            };
                            self.notice = None;
                        }
                        Err(e) => self.notice = Some(e),
                    }
                    self.books = notes::scan_books(&self.cfg.project);
                }
            }
            Message::DeleteNote => {
                if let Some(note) = self.note.take() {
                    notes::delete(&self.cfg.project, &note.key);
                    self.books = notes::scan_books(&self.cfg.project);
                }
            }
            Message::NewBook => {
                self.close_note();
                match notes::create_book(&self.cfg.project) {
                    Ok(key) => {
                        self.books = notes::scan_books(&self.cfg.project);
                        self.open_note(key);
                    }
                    Err(e) => self.notice = Some(e),
                }
            }
            Message::NewPage(book) => {
                self.close_note();
                match notes::create_page(&self.cfg.project, &book) {
                    Ok(key) => {
                        self.books = notes::scan_books(&self.cfg.project);
                        self.open_note(key);
                    }
                    Err(e) => self.notice = Some(e),
                }
            }
        }
        Task::none()
    }

    fn flush_note(&mut self) {
        if let Some(note) = &mut self.note
            && note.dirty
        {
            let path = note.key.path(&self.cfg.project);
            let markdown = note.editor.markdown();
            let result = match &note.key {
                NoteKey::Page { .. } => notes::save(&path, &markdown),
                // empty person / clan notes leave no file behind
                key if markdown.trim().is_empty() => {
                    notes::delete(&self.cfg.project, key);
                    Ok(())
                }
                NoteKey::Person(id) => match self.data.person(id) {
                    Some(p) => notes::save(&path, &notes::with_preamble(p, &markdown)),
                    None => notes::save(&path, &markdown),
                },
                NoteKey::Clan(_) => notes::save(&path, &markdown),
            };
            match result {
                Ok(()) => {
                    note.dirty = false;
                    let id = match &note.key {
                        NoteKey::Person(id) | NoteKey::Clan(id) => Some(id.clone()),
                        NoteKey::Page { .. } => None,
                    };
                    if let Some(id) = id {
                        let changed = if markdown.trim().is_empty() {
                            self.with_notes.remove(&id)
                        } else {
                            self.with_notes.insert(id)
                        };
                        if changed {
                            self.cache.clear();
                        }
                    }
                }
                Err(e) => self.notice = Some(e),
            }
        }
    }

    fn refresh_file_marks(&mut self) {
        self.with_files = self.data.files.iter().map(|f| f.person.clone()).collect();
    }

    fn close_note(&mut self) {
        self.flush_note();
        self.note = None;
    }

    fn open_note(&mut self, key: NoteKey) {
        self.close_note();
        let text = notes::load(&self.cfg.project, &key);
        let (book_name, name) = match &key {
            NoteKey::Page { book, page } => (book.clone(), page.clone()),
            _ => (String::new(), String::new()),
        };
        self.note = Some(OpenNote {
            editor: EditorState::with_markdown(&text)
                .paper(Paper::fill())
                .style(ui::editor_style())
                .zoom_settings(ZoomSettings::disabled())
                .tab_width(2),
            key,
            name,
            book_name,
            dirty: false,
        });
        self.selected_link = None;
        self.pending_link = None;
    }

    fn note_title(&self, key: &NoteKey) -> String {
        match key {
            NoteKey::Person(id) => self
                .data
                .person(id)
                .map_or_else(|| t("default-person").into(), |p| p.name.clone()),
            NoteKey::Clan(id) => self
                .data
                .clan(id)
                .map_or_else(|| t("clan").into(), |c| c.name.clone()),
            NoteKey::Page { book, .. } => book.clone(),
        }
    }

    /// Drops the note of a deleted person / clan (and the open editor if it shows it).
    fn forget_note(&mut self, key: NoteKey) {
        if self.note.as_ref().is_some_and(|n| n.key == key) {
            self.note = None;
        }
        notes::delete(&self.cfg.project, &key);
        if let NoteKey::Person(id) | NoteKey::Clan(id) = &key {
            self.with_notes.remove(id);
        }
    }

    fn edit_selected(&mut self, edit: impl FnOnce(&mut Person)) {
        if let Some(id) = self.selected.clone()
            && let Some(p) = self.data.person_mut(&id)
        {
            edit(p);
            let clans = p.clans.clone();
            self.dirty = Some(id);
            if self.years.is_some() {
                // new dates can move the person in or out of the timeline range
                self.rebuild_hulls_for(&clans);
            }
        }
    }

    fn delete_person(&mut self, id: &str) {
        if self.dirty.as_deref() == Some(id) {
            self.dirty = None;
        }
        let Some(person) = self.data.remove_person(id) else {
            return;
        };
        if self.selected_link.as_ref().is_some_and(|link_id| {
            self.data
                .links
                .iter()
                .any(|link| link.id == *link_id && (link.from == id || link.to == id))
        }) {
            self.selected_link = None;
        }
        let mut cmds = vec![Cmd::del(PEOPLE, id)];

        self.data.links.retain(|l| {
            let keep = l.from != id && l.to != id;
            if !keep {
                cmds.push(Cmd::del(LINKS, &l.id));
            }
            keep
        });

        let project = self.cfg.project.clone();
        self.data.files.retain(|f| {
            let keep = f.person != id;
            if !keep {
                let _ = std::fs::remove_file(files::absolute(&project, f));
                cmds.push(Cmd::del(FILES, &f.id));
            }
            keep
        });
        let _ = std::fs::remove_dir(files::person_dir(&project, id));
        self.forget_note(NoteKey::Person(id.to_owned()));
        self.refresh_file_marks();

        self.db.send(Cmd::Batch(cmds));
        if self
            .pending_link
            .as_ref()
            .is_some_and(|pl| pl.from == id || pl.to == id)
        {
            self.pending_link = None;
        }
        // indices changed (swap_remove) -> rebuild grid; hulls only for affected clans
        self.grid = Grid::build(&self.data.people);
        self.rebuild_hulls_for(&person.clans);
        self.recount();
    }

    fn import_files(&mut self, paths: Vec<PathBuf>) -> Task<Message> {
        let Some(person) = self.selected.clone() else {
            self.notice = Some(t("notice-drop-files").into());
            return Task::none();
        };
        if paths.is_empty() {
            return Task::none();
        }
        let project = self.cfg.project.clone();
        Task::perform(
            async move { files::import(&project, &person, &paths) },
            Message::FilesImported,
        )
    }

    // -----------------------------------------------------------------------
    // view
    // -----------------------------------------------------------------------

    pub fn view(&self) -> Element<'_, Message> {
        let mut body = Row::new().height(Length::Fill);
        if self.show_sidebar {
            body = body.push(self.view_sidebar()).push(vline());
        }
        let body = body
            .push(self.view_centre())
            .push(vline())
            .push(self.view_inspector());

        let page = column![
            self.view_toolbar(),
            hline(),
            body,
            hline(),
            self.view_status()
        ];

        match &self.confirm {
            Some(confirm) => stack![page, opaque(self.view_confirm(confirm))].into(),
            None => page.into(),
        }
    }

    /// Board (or the open note) with floating overlays: tool hint, zoom, toast, empty state.
    fn view_centre(&self) -> Element<'_, Message> {
        if let Some(note) = &self.note {
            return self.view_note(note);
        }

        let mut layers = stack![
            World { app: self },
            canvas(Board { app: self })
                .width(Length::Fill)
                .height(Length::Fill)
        ];

        if self.data.people.is_empty() && self.data.clans.is_empty() {
            layers = layers.push(container(self.view_empty_board()).center(Length::Fill));
        }
        if let Some(hint) = self.tool_hint() {
            layers = layers.push(
                container(
                    container(
                        row![
                            text(hint).size(13),
                            kbd("Esc"),
                        ]
                        .spacing(10)
                        .align_y(alignment::Vertical::Center),
                    )
                    .padding([7, 14])
                    .style(ui::tooltip),
                )
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .padding(14),
            );
        }
        layers = layers.push(
            container(self.view_zoom())
                .align_right(Length::Fill)
                .align_bottom(Length::Fill)
                .padding(14),
        );
        if let Some(years) = self.years {
            layers = layers.push(
                container(self.view_timeline(years))
                    .align_left(Length::Fill)
                    .align_bottom(Length::Fill)
                    .padding(14),
            );
        }
        if let Some(notice) = &self.notice {
            layers = layers.push(
                container(
                    container(
                        row![
                            text(notice.as_str())
                                .size(13)
                                .color(ui::toast_text()),
                            button(icon("✕").size(12).color(ui::toast_text()))
                                .padding([2, 6])
                                .style(ui::ghost)
                                .on_press(Message::DismissNotice),
                        ]
                        .spacing(12)
                        .align_y(alignment::Vertical::Center),
                    )
                    .max_width(560)
                    .padding([8, 10])
                    .style(ui::toast),
                )
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_bottom(Length::Fill)
                .padding([18, 80]),
            );
        }
        layers.width(Length::Fill).height(Length::Fill).into()
    }

    fn tool_hint(&self) -> Option<String> {
        if self.pending_link.is_some() {
            return Some(t("hint-name-line").into());
        }
        let clan = self
            .active_clan
            .as_deref()
            .and_then(|id| self.data.clan(id))
            .map(|c| c.name.as_str());
        match (self.tool, clan) {
            (Tool::Select, _) => None,
            (Tool::AddPerson, Some(clan)) => Some(tf(
                "hint-add-person-in-clan",
                &[("clan", clan.into())],
            )),
            (Tool::AddPerson, None) => {
                Some(t("hint-add-person").into())
            }
            (Tool::Connect, _) => {
                Some(t("hint-connect").into())
            }
            (Tool::Clan, _) => Some(t("hint-add-clan").into()),
            (Tool::Members, Some(clan)) => Some(tf(
                "hint-members",
                &[("clan", clan.into())],
            )),
            (Tool::Members, None) => Some(t("hint-pick-clan").into()),
        }
    }

    fn view_zoom(&self) -> Element<'_, Message> {
        let step = |glyph: &'static str, msg: Message, label: &'static str| {
            tip(
                button(icon(glyph).size(15))
                    .padding([4, 10])
                    .style(ui::icon)
                    .on_press(msg),
                label,
            )
        };
        container(
            row![
                step("−", Message::ZoomStep(1.0 / 1.25), t("zoom-out-tooltip")),
                tip(
                    button(
                        text(format!("{:.0}%", self.camera.zoom * 100.0))
                            .size(12)
                            .width(Length::Fixed(44.0))
                            .align_x(alignment::Horizontal::Center),
                    )
                    .padding([5, 2])
                    .style(ui::icon)
                    .on_press(Message::ZoomReset),
                    t("zoom-reset-tooltip"),
                ),
                step("+", Message::ZoomStep(1.25), t("zoom-in-tooltip")),
                vrule(18.0),
                step("⊡", Message::FitToWindow, t("tool-fit-to-window-tooltip")),
            ]
            .spacing(2)
            .align_y(alignment::Vertical::Center),
        )
        .padding(4)
        .style(ui::floating)
        .into()
    }

    /// Range slider (from – to) with play / pause and the number of people alive at `to`.
    fn view_timeline(&self, (from, to): (i32, i32)) -> Element<'_, Message> {
        let (first, last) = self.year_span().unwrap_or((from, to));
        let living = self
            .data
            .people
            .iter()
            .filter(|p| p.era(to) == Era::Living)
            .count();
        let play = tip_key(
            button(icon(if self.playing { "❚❚" } else { "▶" }).size(13))
                .padding([5, 10])
                .style(ui::icon)
                .on_press(Message::TogglePlay),
            if self.playing {
                t("timeline-pause")
            } else {
                t("timeline-play")
            },
            "Space",
        );
        // leaves room for the zoom controls on the right
        let width = (self.canvas_size.width - 260.0).clamp(380.0, 800.0);
        let range = if from == to {
            to.to_string()
        } else {
            format!("{from} – {to}")
        };
        container(
            row![
                play,
                text(range)
                    .size(16)
                    .font(ui::BOLD)
                    .width(Length::Fixed(112.0)),
                text(first.to_string()).size(11).color(ui::pal().subtle),
                canvas(RangeSlider {
                    first,
                    last,
                    from: from.clamp(first, last),
                    to: to.clamp(first, last),
                })
                .width(Length::Fill)
                .height(Length::Fixed(24.0)),
                text(last.to_string()).size(11).color(ui::pal().subtle),
                vrule(18.0),
                text(tf("timeline-living", &[("count", living.into())]))
                    .size(12)
                    .color(ui::pal().muted),
                tip_key(
                    button(icon("✕").size(12))
                        .padding([4, 8])
                        .style(ui::icon)
                        .on_press(Message::ToggleTimeline),
                    t("timeline-close"),
                    "T",
                ),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center),
        )
        .width(Length::Fixed(width))
        .padding([4, 8])
        .style(ui::floating)
        .into()
    }

    fn view_empty_board(&self) -> Element<'_, Message> {
        container(
            column![
                text("✦").font(ui::ICON).size(30).color(ui::pal().accent),
                text(t("empty-board-title")).size(20).font(ui::BOLD),
                text(t(
                    "empty-board-body"
                ))
                .size(13)
                .color(ui::pal().muted)
                .align_x(alignment::Horizontal::Center),
                row![
                    button(icon_label("✚", t("empty-board-add-person")))
                        .padding([8, 16])
                        .style(ui::primary)
                        .on_press(Message::AddPersonCentre),
                    button(icon_label("◎", t("sidebar-new-clan")))
                        .padding([8, 16])
                        .style(ui::secondary)
                        .on_press(Message::NewClan),
                ]
                .spacing(10),
            ]
            .spacing(12)
            .align_x(alignment::Horizontal::Center),
        )
        .width(Length::Fixed(400.0))
        .padding(28)
        .style(ui::floating)
        .into()
    }

    fn view_note<'a>(&'a self, note: &'a OpenNote) -> Element<'a, Message> {
        let mut header = row![
            button(icon_label("←", t("notes-back")))
                .padding([6, 12])
                .style(ui::secondary)
                .on_press(Message::CloseNote),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center);

        if matches!(note.key, NoteKey::Page { .. }) {
            header = header
                .push(
                    text_input(t("notes-book-name"), &note.book_name)
                        .on_input(Message::BookName)
                        .on_submit(Message::RenameBook)
                        .size(14)
                        .padding([6, 10])
                        .style(ui::input)
                        .width(Length::Fixed(200.0)),
                )
                .push(text("/").size(16).color(ui::pal().subtle))
                .push(
                    text_input(t("notes-page-name"), &note.name)
                        .on_input(Message::NoteName)
                        .on_submit(Message::RenameNote)
                        .size(14)
                        .padding([6, 10])
                        .style(ui::input)
                        .width(Length::Fixed(260.0)),
                )
                .push(space::horizontal())
                .push(
                    button(icon_label("✕", t("notes-delete-page")))
                        .padding([6, 12])
                        .style(ui::danger)
                        .on_press(Message::AskDelete(Confirm::Page)),
                );
        } else {
            header = header
                .push(icon("✎").size(15).color(ui::pal().note_mark))
                .push(
                    text(tf("notes-title", &[("title", self.note_title(&note.key).into())]))
                        .size(15)
                        .font(ui::BOLD),
                );
        }
        let saved = if note.dirty { t("notes-saving") } else { t("notes-saved") };
        header = header.push(space::horizontal()).push(
            text(saved).size(12).color(ui::pal().subtle),
        );

        column![
            container(header.padding([10, 14]))
                .width(Length::Fill)
                .style(ui::chrome),
            hline(),
            markdown_editor(&note.editor)
                .status_bar(false)
                .view()
                .map(Message::NoteEditor),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// Small note / file indicators for list rows.
    fn marks(&self, id: &str, files: bool) -> Element<'_, Message> {
        let mut r = Row::new().spacing(5).align_y(alignment::Vertical::Center);
        if self.with_notes.contains(id) {
            r = r.push(icon("✎").size(11).color(ui::pal().note_mark));
        }
        if files && self.with_files.contains(id) {
            r = r.push(icon("▣").size(11).color(ui::pal().file_mark));
        }
        r.into()
    }

    fn view_toolbar(&self) -> Element<'_, Message> {
        let segment = |glyph: &'static str, label: &'static str, key: &'static str, tool: Tool| {
            tip_key(
                button(icon_label(glyph, label))
                    .padding([5, 11])
                    .style(ui::segment(self.tool == tool))
                    .on_press(Message::Tool(tool)),
                label,
                key,
            )
        };
        let tools = container(
            row![
                segment("↖", t("tool-select"), "V", Tool::Select),
                segment("◎", t("clan"), "G", Tool::Clan),
                segment("✚", t("default-person"), "P", Tool::AddPerson),
                segment("⟶", t("tool-connect"), "C", Tool::Connect),
            ]
            .spacing(2),
        )
        .padding(3)
        .style(ui::segments);

        let actions = row![
            tip(
                button(icon_label("▦", t("tool-auto-layout")))
                    .padding([6, 11])
                    .style(ui::icon)
                    .on_press(Message::AutoLayout),
                t("tool-auto-layout-tooltip"),
            ),
            tip_key(
                button(icon_label("⊡", t("tool-fit")))
                    .padding([6, 11])
                    .style(ui::icon)
                    .on_press(Message::FitToWindow),
                t("tool-fit-to-window"),
                "F",
            ),
            tip_key(
                button(icon_label("◷", t("tool-timeline")))
                    .padding([6, 11])
                    .style(ui::segment(self.years.is_some()))
                    .on_press(Message::ToggleTimeline),
                t("tool-timeline-tooltip"),
                "T",
            ),
        ]
        .spacing(2);

        let sidebar = tip_key(
            button(icon("◧").size(16))
                .padding([5, 9])
                .style(ui::icon)
                .on_press(Message::ToggleSidebar),
            if self.show_sidebar {
                t("tool-hide-sidebar")
            } else {
                t("tool-show-sidebar")
            },
            "Ctrl+B",
        );
        let workspace = tip(
            button(
                row![
                    icon("▤").size(14).color(ui::pal().accent),
                    text(self.workspace_name()).size(14).font(ui::SEMIBOLD),
                    icon("▾").size(11).color(ui::pal().muted),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            )
            .padding([6, 10])
            .style(ui::icon)
            .on_press(Message::OpenSettings(SettingsPage::Workspace)),
            t("tool-switch-workspaces"),
        );

        let search = text_input(t("tool-search-placeholder"), &self.search)
            .id(SEARCH_ID)
            .on_input(Message::Search)
            .on_submit(Message::SearchSubmit)
            .size(13)
            .padding([7, 14])
            .style(ui::search)
            .width(Length::Fixed(240.0));

        let theme = tip(
            button(icon(if ui::pal().dark { "☀" } else { "☾" }).size(15))
                .padding([5, 9])
                .style(ui::icon)
                .on_press(Message::ToggleTheme),
            if ui::pal().dark {
                t("tool-light-mode")
            } else {
                t("tool-dark-mode")
            },
        );
        let settings = tip_key(
            button(icon("⚙").size(16))
                .padding([4, 9])
                .style(ui::icon)
                .on_press(Message::OpenSettings(SettingsPage::General)),
            t("tool-settings"),
            "Ctrl+,",
        );

        container(
            row![
                sidebar,
                workspace,
                vrule(22.0),
                tools,
                actions,
                space::horizontal(),
                search,
                theme,
                settings,
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .padding([0, 10]),
        )
        .width(Length::Fill)
        .height(Length::Fixed(TOOLBAR_H))
        .align_y(alignment::Vertical::Center)
        .style(ui::chrome)
        .into()
    }

    /// Collapsible sidebar section header with a count and an optional add button.
    fn section_header(
        &self,
        section: Section,
        label: &'static str,
        count: usize,
        add: Option<(Message, &'static str)>,
    ) -> Element<'_, Message> {
        let open = !self.collapsed.contains(&section);
        let mut header = row![
            button(
                row![
                    icon(if open { "▾" } else { "▸" })
                        .size(11)
                        .color(ui::pal().subtle)
                        .width(Length::Fixed(12.0)),
                    caption(label),
                    badge(count),
                ]
                .spacing(6)
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .padding([5, 6])
            .style(ui::header)
            .on_press(Message::ToggleSection(section)),
        ]
        .spacing(2)
        .align_y(alignment::Vertical::Center);
        if let Some((msg, label)) = add {
            header = header.push(tip(
                button(icon("+").size(15))
                    .padding([1, 8])
                    .style(ui::icon)
                    .on_press(msg),
                label,
            ));
        }
        header.into()
    }

    /// Scrollable list of a sidebar section (nothing when collapsed).
    /// Scrollable list of a sidebar section (nothing when collapsed); `max` caps short
    /// sections at their content height, without a cap the section takes the free space.
    fn section_body<'a>(
        &self,
        section: Section,
        list: Column<'a, Message>,
        max: Option<f32>,
    ) -> Element<'a, Message> {
        if self.collapsed.contains(&section) {
            return Column::new().into();
        }
        let list = scrollable(list.padding(Padding::ZERO.right(8))).style(ui::scroll);
        match max {
            Some(max) => container(list.height(Length::Shrink)).max_height(max).into(),
            None => list.height(Length::Fill).into(),
        }
    }

    fn view_sidebar(&self) -> Element<'_, Message> {
        // clans
        let mut clans = Column::new().spacing(1);
        for clan in &self.data.clans {
            let count = self.clan_counts.get(&clan.id).copied().unwrap_or(0);
            let active = self.active_clan.as_deref() == Some(clan.id.as_str());
            let content = row![
                container(text(""))
                    .width(Length::Fixed(10.0))
                    .height(Length::Fixed(10.0))
                    .style(ui::swatch(clan.color())),
                text(clan.name.as_str())
                    .size(13)
                    .width(Length::Fill)
                    .wrapping(text::Wrapping::None),
                self.marks(&clan.id, false),
                text(count.to_string()).size(12).color(ui::pal().subtle),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center);
            let row = button(content)
                .width(Length::Fill)
                .padding([6, 10])
                .style(ui::list_item(active))
                .on_press(Message::SelectClan(clan.id.clone()));
            clans = clans.push(if active {
                Element::from(
                    row![
                        row,
                        tip(
                            button(icon("✕").size(11))
                                .padding([4, 7])
                                .style(ui::ghost)
                                .on_press(Message::DeselectClan),
                            t("sidebar-deselect-clan"),
                        ),
                    ]
                    .spacing(2)
                    .align_y(alignment::Vertical::Center),
                )
            } else {
                row.into()
            });
        }
        if self.data.clans.is_empty() {
            clans = clans.push(empty_hint(t("sidebar-no-clans")));
        }
        clans = clans.push(add_row(t("sidebar-new-clan"), Message::NewClan));

        // people
        let q = self.search.trim().to_lowercase();
        let mut people = Column::new().spacing(1);
        let mut total = 0usize;
        for p in &self.data.people {
            if !q.is_empty() && !p.name.to_lowercase().contains(&q) {
                continue;
            }
            total += 1;
            if total <= MAX_PEOPLE_LIST {
                let active = self.selected.as_deref() == Some(p.id.as_str());
                let years = p.years();
                let mut name = Column::new().push(
                    text(p.name.as_str())
                        .size(13)
                        .wrapping(text::Wrapping::None),
                );
                if !years.is_empty() {
                    name = name.push(text(years).size(11).color(ui::pal().subtle));
                }
                people = people.push(
                    button(
                        row![
                            avatar(&p.name, self.person_color(p), 26.0),
                            name.width(Length::Fill),
                            self.marks(&p.id, true),
                        ]
                        .spacing(10)
                        .align_y(alignment::Vertical::Center),
                    )
                    .width(Length::Fill)
                    .padding([5, 8])
                    .style(ui::list_item(active))
                    .on_press(Message::FocusPerson(p.id.clone())),
                );
            }
        }
        if total > MAX_PEOPLE_LIST {
            people = people.push(empty_hint(&tf(
                "sidebar-more-use-search",
                &[("count", (total - MAX_PEOPLE_LIST).into())],
            )));
        } else if total == 0 && !q.is_empty() {
            people = people.push(empty_hint(t("sidebar-no-match")));
        } else if total == 0 {
            people = people.push(empty_hint(t("sidebar-no-people")));
        }

        // books
        let mut books = Column::new().spacing(1);
        for book in &self.books {
            books = books.push(
                row![
                    icon("▤").size(12).color(ui::pal().muted),
                    text(book.name.as_str())
                        .size(13)
                        .font(ui::SEMIBOLD)
                        .width(Length::Fill),
                    tip(
                        button(icon("+").size(13))
                            .padding([0, 7])
                            .style(ui::icon)
                            .on_press(Message::NewPage(book.name.clone())),
                        t("sidebar-new-page"),
                    ),
                ]
                .spacing(8)
                .padding([4, 10])
                .align_y(alignment::Vertical::Center),
            );
            for page in &book.pages {
                let key = NoteKey::Page {
                    book: book.name.clone(),
                    page: page.clone(),
                };
                let active = self.note.as_ref().is_some_and(|n| n.key == key);
                books = books.push(
                    button(
                        row![
                            icon("✎").size(11).color(ui::pal().subtle),
                            text(page.as_str()).size(13),
                        ]
                        .spacing(8)
                        .align_y(alignment::Vertical::Center),
                    )
                    .width(Length::Fill)
                    .padding(Padding::from([4, 10]).left(30))
                    .style(ui::list_item(active))
                    .on_press(Message::OpenNote(key)),
                );
            }
        }
        books = books.push(add_row(t("sidebar-new-book"), Message::NewBook));

        let col = column![
            self.section_header(
                Section::Clans,
                t("sidebar-clans"),
                self.data.clans.len(),
                Some((Message::NewClan, t("sidebar-new-clan"))),
            ),
            self.section_body(Section::Clans, clans, Some(220.0)),
            self.section_header(
                Section::People,
                t("sidebar-people"),
                self.data.people.len(),
                Some((Message::AddPersonCentre, t("empty-board-add-person"))),
            ),
            self.section_body(Section::People, people, None),
            self.section_header(
                Section::Books,
                t("sidebar-books"),
                self.books.len(),
                Some((Message::NewBook, t("sidebar-new-book"))),
            ),
            self.section_body(Section::Books, books, Some(200.0)),
        ]
        .spacing(4);

        container(col)
            .padding([10, 8])
            .width(Length::Fixed(SIDEBAR_W))
            .height(Length::Fill)
            .style(ui::panel)
            .into()
    }

    /// Colour shown for a person: their first clan, else neutral.
    fn person_color(&self, p: &Person) -> Color {
        p.clans
            .first()
            .and_then(|c| self.data.clan(c))
            .map_or(ui::pal().subtle, |c| c.color())
    }

    fn view_inspector(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = if let Some(pl) = &self.pending_link {
            self.view_pending_link(pl)
        } else if let Some(link) = self
            .selected_link
            .as_deref()
            .and_then(|id| self.data.links.iter().find(|link| link.id == id))
        {
            self.view_link(link)
        } else if let Some(p) = self.selected_person() {
            self.view_person(p)
        } else if self.show_clan
            && let Some(c) = self
                .active_clan
                .as_deref()
                .and_then(|id| self.data.clan(id))
        {
            self.view_clan(c)
        } else {
            self.view_guide()
        };

        container(
            scrollable(container(content).padding(Padding::from(16).right(20)))
                .style(ui::scroll)
                .height(Length::Fill),
        )
        .width(Length::Fixed(INSPECTOR_W))
        .height(Length::Fill)
        .style(ui::panel)
        .into()
    }

    /// Inspector with nothing selected: quick start and keyboard shortcuts.
    fn view_guide(&self) -> Element<'_, Message> {
        let step = |glyph: &'static str, title: &'static str, body: &'static str| {
            row![
                container(icon(glyph).size(14).color(ui::pal().accent))
                    .center(Length::Fixed(30.0))
                    .style(ui::avatar(ui::pal().accent)),
                column![
                    text(title).size(13).font(ui::SEMIBOLD),
                    text(body).size(12).color(ui::pal().muted),
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .spacing(12)
        };
        let shortcut = |keys: &'static str, label: &'static str| {
            row![
                text(label).size(12).color(ui::pal().muted).width(Length::Fill),
                kbd(keys),
            ]
            .align_y(alignment::Vertical::Center)
        };

        column![
            column![
                text(t("inspector-nothing-selected")).size(17).font(ui::BOLD),
                text(t("inspector-nothing-selected-body"))
                    .size(13)
                    .color(ui::pal().muted),
            ]
            .spacing(4),
            card(
                t("inspector-getting-started"),
                column![
                    step(
                        "✚",
                        t("help-add-people"),
                        t("help-add-people-body"),
                    ),
                    step(
                        "⟶",
                        t("help-connect"),
                        t("help-connect-body"),
                    ),
                    step(
                        "◎",
                        t("help-clans"),
                        t("help-clans-body"),
                    ),
                    step(
                        "↖",
                        t("help-lines"),
                        t("help-lines-body"),
                    ),
                ]
                .spacing(14),
            ),
            card(
                t("inspector-keyboard"),
                column![
                    shortcut("V", t("tool-select")),
                    shortcut("G", t("sidebar-add-clan")),
                    shortcut("P", t("empty-board-add-person")),
                    shortcut("C", t("tool-connect")),
                    shortcut("F", t("tool-fit-to-window")),
                    shortcut("+ / −", t("key-zoom")),
                    shortcut("Ctrl+F", t("key-search")),
                    shortcut("Del", t("key-delete")),
                    shortcut("Esc", t("key-cancel")),
                    shortcut(t("key-wheel"), t("key-wheel-zoom")),
                    shortcut(t("key-drag"), t("key-drag-pan")),
                ]
                .spacing(7),
            ),
        ]
        .spacing(16)
        .into()
    }

    fn view_pending_link(&self, pl: &PendingLink) -> Element<'_, Message> {
        let presets = Row::with_children(self.relation_names().map(|label| {
            button(text(label).size(12))
                .padding([4, 10])
                .style(ui::chip(ui::pal().accent))
                .on_press(Message::LinkPreset(label.to_owned()))
                .into()
        }))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);

        column![
            column![
                caption(t("line-name-title")),
                self.link_ends(&pl.from, &pl.to),
            ]
            .spacing(10),
            card(
                t("line-relationship"),
                column![
                    text_input(t("line-name-placeholder"), &pl.label)
                        .on_input(Message::LinkLabel)
                        .on_submit(Message::ConfirmLink)
                        .padding([8, 10])
                        .size(13)
                        .style(ui::input),
                    hint(t("line-common")),
                    presets,
                ]
                .spacing(10),
            ),
            row![
                button(icon_label("✓", t("line-create")))
                    .style(ui::primary)
                    .padding([7, 14])
                    .on_press(Message::ConfirmLink),
                button(text(t("cancel")).size(13))
                    .style(ui::secondary)
                    .padding([7, 14])
                    .on_press(Message::CancelLink),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .into()
    }

    /// `[avatar] name → [avatar] name` for a line.
    fn link_ends(&self, from: &str, to: &str) -> Element<'_, Message> {
        let end = |id: &str| -> Element<'_, Message> {
            match self.data.person(id) {
                Some(p) => button(
                    row![
                        avatar(&p.name, self.person_color(p), 24.0),
                        text(p.name.clone())
                            .size(13)
                            .font(ui::SEMIBOLD)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    ]
                    .spacing(8)
                    .align_y(alignment::Vertical::Center),
                )
                .padding([6, 8])
                .width(Length::Fill)
                .style(ui::card)
                .on_press(Message::FocusPerson(p.id.clone()))
                .into(),
                None => text(t("line-missing-person")).size(13).color(ui::pal().danger).into(),
            }
        };
        column![
            end(from),
            container(icon("↓").size(14).color(ui::pal().subtle)).padding([0, 16]),
            end(to),
        ]
        .spacing(4)
        .into()
    }

    fn view_link(&self, link: &Link) -> Element<'_, Message> {
        let shown = self.data.link_label(&link.label);
        let presets = Row::with_children(self.relation_names().map(|label| {
            let active = shown == label;
            button(text(label).size(12))
                .padding([4, 10])
                .style(if active {
                    ui::chip(ui::pal().accent)
                } else {
                    ui::chip(ui::pal().subtle)
                })
                .on_press(Message::SetLinkLabel(label.to_owned()))
                .into()
        }))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);

        let mut route = Column::new()
            .spacing(10)
            .push(hint(t("line-route-help")));
        if let Some(vertex) = self.selected_link_vertex {
            route = route
                .push(hint(t("line-corner-selected")))
                .push(
                    button(icon_label("✕", t("line-remove-corner")))
                        .padding([6, 12])
                        .style(ui::secondary)
                        .on_press(Message::DeleteLinkVertex(link.id.clone(), vertex)),
                );
        } else {
            route = route.push(hint(t("line-corner-help")));
        }
        if !link.route.is_empty() {
            route = route.push(
                button(icon_label("⟲", t("line-reset-route")))
                    .padding([6, 12])
                    .style(ui::secondary)
                    .on_press(Message::ResetLinkRoute(link.id.clone())),
            );
        }

        column![
            column![
                caption(t("line-selected")),
                self.link_ends(&link.from, &link.to),
            ]
            .spacing(10),
            card(
                t("line-relationship"),
                column![
                    text_input(t("line-name-placeholder"), &shown)
                        .on_input(Message::EditLinkLabel)
                        .on_submit(Message::RememberLinkLabel)
                        .padding([8, 10])
                        .size(13)
                        .style(ui::input),
                    presets,
                ]
                .spacing(10),
            ),
            card(t("line-route"), route),
            danger_zone(
                t("line-delete-help"),
                t("line-remove"),
                Message::DeleteLink(link.id.clone()),
            ),
        ]
        .spacing(16)
        .into()
    }

    fn view_person<'a>(&'a self, p: &'a Person) -> Element<'a, Message> {
        // header
        let years = p.years();
        let header = row![
            avatar(&p.name, self.person_color(p), 48.0),
            column![
                text_input(t("person-name"), &p.name)
                    .on_input(Message::EditName)
                    .on_submit(Message::CommitPerson)
                    .font(ui::BOLD)
                    .size(18)
                    .padding([4, 6])
                    .style(ui::title_input),
                text(if years.is_empty() {
                    t("person-dates-unknown").to_string()
                } else {
                    years
                })
                .size(12)
                .color(ui::pal().muted),
            ]
            .spacing(0)
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(alignment::Vertical::Center);

        let details = row![
            labelled(
                t("person-born"),
                text_input(t("person-born-placeholder"), &p.born)
                    .on_input(Message::EditBorn)
                    .on_submit(Message::CommitPerson)
                    .size(13)
                    .padding([7, 10])
                    .style(ui::input),
            ),
            labelled(
                t("person-died"),
                text_input(t("person-died-placeholder"), &p.died)
                    .on_input(Message::EditDied)
                    .on_submit(Message::CommitPerson)
                    .size(13)
                    .padding([7, 10])
                    .style(ui::input),
            ),
        ]
        .spacing(10);

        // clans
        let mut chips: Vec<Element<'a, Message>> = p
            .clans
            .iter()
            .filter_map(|cid| self.data.clan(cid))
            .map(|c| {
                tip(
                    button(
                        row![
                            container(text(""))
                                .width(Length::Fixed(8.0))
                                .height(Length::Fixed(8.0))
                                .style(ui::swatch(c.color())),
                            text(c.name.as_str()).size(12),
                            icon("✕").size(10),
                        ]
                        .spacing(6)
                        .align_y(alignment::Vertical::Center),
                    )
                    .padding([4, 10])
                    .style(ui::chip(c.color()))
                    .on_press(Message::RemoveFromClan(c.id.clone())),
                    t("person-remove-from-clan"),
                )
            })
            .collect();
        let choices: Vec<ClanChoice> = self
            .data
            .clans
            .iter()
            .filter(|c| !p.clans.contains(&c.id))
            .map(|c| ClanChoice {
                id: c.id.clone(),
                name: c.name.clone(),
            })
            .collect();
        if !choices.is_empty() {
            chips.push(
                pick_list(choices, None::<ClanChoice>, Message::AddToClan)
                    .placeholder(t("person-add-clan"))
                    .text_size(12)
                    .padding([4, 10])
                    .style(ui::pick_chip)
                    .into(),
            );
        } else if p.clans.is_empty() {
            chips.push(hint(t("person-no-clans")).into());
        }

        // lines
        let mut lines = Column::new().spacing(2);
        let mut line_count = 0usize;
        for l in self
            .data
            .links
            .iter()
            .filter(|l| l.from == p.id || l.to == p.id)
        {
            line_count += 1;
            if line_count > MAX_LINES_LIST {
                continue;
            }
            let undirected = link_style(&l.label) == LinkStyle::Undirected;
            let (arrow, other) = if l.from == p.id {
                (if undirected { "↔" } else { "→" }, &l.to)
            } else {
                (if undirected { "↔" } else { "←" }, &l.from)
            };
            let other_person = self.data.person(other);
            let other_name = other_person.map_or("?", |o| o.name.as_str());
            let color = other_person.map_or(ui::pal().subtle, |o| self.person_color(o));
            lines = lines.push(
                row![
                    button(
                        row![
                            avatar(other_name, color, 24.0),
                            column![
                                text(other_name).size(13).wrapping(text::Wrapping::None),
                                row![
                                    icon(arrow).size(11).color(ui::pal().subtle),
                                    text(self.data.link_label(&l.label).into_owned())
                                        .size(11)
                                        .color(ui::pal().muted),
                                ]
                                .spacing(4),
                            ]
                            .width(Length::Fill),
                        ]
                        .spacing(10)
                        .align_y(alignment::Vertical::Center),
                    )
                    .width(Length::Fill)
                    .padding([4, 6])
                    .style(ui::list_item(false))
                    .on_press(Message::FocusPerson(other.clone())),
                    tip(
                        button(icon("⚙").size(12))
                            .padding([3, 6])
                            .style(ui::ghost)
                            .on_press(Message::SelectLink(l.id.clone())),
                        t("person-edit-line"),
                    ),
                    tip(
                        button(icon("✕").size(11))
                            .padding([3, 6])
                            .style(ui::ghost)
                            .on_press(Message::DeleteLink(l.id.clone())),
                        t("line-remove"),
                    ),
                ]
                .spacing(2)
                .align_y(alignment::Vertical::Center),
            );
        }
        if line_count > MAX_LINES_LIST {
            lines = lines.push(empty_hint(&tf(
                "list-more",
                &[("count", (line_count - MAX_LINES_LIST).into())],
            )));
        }
        if line_count == 0 {
            lines = lines.push(hint(t("person-lines-help")));
        }

        // files
        let person_files: Vec<&FileRec> = self
            .data
            .files
            .iter()
            .filter(|f| f.person == p.id)
            .collect();
        let mut file_list = Column::new().spacing(6);
        for f in &person_files {
            file_list = file_list.push(
                row![
                    button(
                        row![
                            icon("▣").size(13).color(ui::pal().file_mark),
                            text(f.name.as_str()).size(13).width(Length::Fill),
                            text(files::human_size(f.size)).size(11).color(ui::pal().subtle),
                        ]
                        .spacing(8)
                        .align_y(alignment::Vertical::Center),
                    )
                    .width(Length::Fill)
                    .padding([7, 10])
                    .style(ui::card)
                    .on_press(Message::OpenFile(f.id.clone())),
                    tip(
                        button(icon("✕").size(11))
                            .padding([3, 6])
                            .style(ui::ghost)
                            .on_press(Message::DeleteFile(f.id.clone())),
                        t("person-delete-file"),
                    ),
                ]
                .spacing(4)
                .align_y(alignment::Vertical::Center),
            );
        }
        let drop_zone = button(
            column![
                icon("⇧").size(18),
                text(t("person-drop-files")).size(12),
            ]
            .spacing(4)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center),
        )
        .width(Length::Fill)
        .padding(14)
        .style(ui::drop_zone)
        .on_press(Message::ChooseFiles);

        let has_notes = self.with_notes.contains(&p.id);
        let notes = button(
            row![
                icon("✎").size(14).color(ui::pal().note_mark),
                column![
                    text(if has_notes {
                        t("person-open-notes")
                    } else {
                        t("person-write-notes")
                    })
                    .size(13)
                    .font(ui::SEMIBOLD),
                    text(t("person-notes-hint"))
                        .size(11)
                        .color(ui::pal().muted),
                ]
                .width(Length::Fill),
                icon("→").size(13).color(ui::pal().subtle),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .padding([8, 12])
        .style(ui::card)
        .on_press(Message::OpenNote(NoteKey::Person(p.id.clone())));

        column![
            header,
            card(t("person-details"), details),
            card(t("sidebar-clans"), Row::with_children(chips).spacing(6).wrap().vertical_spacing(6)),
            card(
                tf("person-lines", &[("count", line_count.into())]),
                column![
                    lines,
                    hint(t("person-lines-route-help")),
                ]
                .spacing(8),
            ),
            notes,
            card(
                tf("person-files", &[("count", person_files.len().into())]),
                column![file_list, drop_zone].spacing(8),
            ),
            danger_zone(
                t("person-delete-help"),
                t("person-delete"),
                Message::AskDelete(Confirm::Person(p.id.clone())),
            ),
        ]
        .spacing(14)
        .into()
    }

    fn view_clan<'a>(&'a self, c: &'a Clan) -> Element<'a, Message> {
        let count = self.clan_counts.get(&c.id).copied().unwrap_or(0);

        let header = row![
            container(icon("◎").size(20).color(Color::WHITE))
                .center(Length::Fixed(48.0))
                .style(ui::swatch(c.color())),
            column![
                text_input(t("clan-name"), &c.name)
                    .on_input(Message::RenameClan)
                    .font(ui::BOLD)
                    .size(18)
                    .padding([4, 6])
                    .style(ui::title_input),
                text(tf("clan-member-count", &[("count", count.into())])).size(12).color(ui::pal().muted),
            ]
            .width(Length::Fill),
        ]
        .spacing(12)
        .align_y(alignment::Vertical::Center);

        let colours = Row::with_children(PALETTE.iter().enumerate().map(|(i, rgb)| {
            let color = Color::from_rgb(rgb[0], rgb[1], rgb[2]);
            let current = c.color == *rgb;
            button(
                container(if current {
                    icon("✓").size(12).color(Color::WHITE)
                } else {
                    icon("")
                })
                .center(Length::Fixed(18.0))
                .style(ui::swatch(color)),
            )
            .padding(2)
            .style(ui::list_item(current))
            .on_press(Message::SetClanColor(i))
            .into()
        }))
        .spacing(2)
        .wrap()
        .vertical_spacing(4);

        let mut members = Column::new().spacing(1);
        let mut shown = 0usize;
        for p in self.data.people.iter().filter(|p| p.clans.contains(&c.id)) {
            shown += 1;
            if shown > MAX_LINES_LIST {
                continue;
            }
            members = members.push(
                button(
                    row![
                        avatar(&p.name, c.color(), 22.0),
                        text(p.name.as_str()).size(13).width(Length::Fill),
                        text(p.years()).size(11).color(ui::pal().subtle),
                    ]
                    .spacing(10)
                    .align_y(alignment::Vertical::Center),
                )
                .width(Length::Fill)
                .padding([4, 6])
                .style(ui::list_item(false))
                .on_press(Message::FocusPerson(p.id.clone())),
            );
        }
        if shown > MAX_LINES_LIST {
            members = members.push(empty_hint(&tf("list-more", &[("count", (shown - MAX_LINES_LIST).into())])));
        }
        if shown == 0 {
            members = members.push(hint(t("clan-no-members")));
        }

        let clan_tool = self.tool == Tool::Members;
        column![
            header,
            card(t("clan-colour"), colours),
            row![
                button(icon_label("◎", if clan_tool { t("clan-done-adding") } else { t("clan-edit-members") }))
                    .padding([7, 12])
                    .width(Length::Fill)
                    .style(if clan_tool { ui::primary } else { ui::secondary })
                    .on_press(Message::Tool(if clan_tool { Tool::Select } else { Tool::Members })),
                tip(
                    button(icon("⊡").size(14))
                        .padding([7, 11])
                        .style(ui::secondary)
                        .on_press(Message::FitClan),
                    t("clan-fit"),
                ),
            ]
            .spacing(8),
            hint(t("clan-members-help")),
            button(
                row![
                    icon("✎").size(14).color(ui::pal().note_mark),
                    text(if self.with_notes.contains(&c.id) {
                        t("person-open-notes")
                    } else {
                        t("person-write-notes")
                    })
                    .size(13)
                    .font(ui::SEMIBOLD)
                    .width(Length::Fill),
                    icon("→").size(13).color(ui::pal().subtle),
                ]
                .spacing(10)
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .padding([9, 12])
            .style(ui::card)
            .on_press(Message::OpenNote(NoteKey::Clan(c.id.clone()))),
            card(tf("clan-members", &[("count", count.into())]), members),
            danger_zone(
                t("clan-delete-help"),
                t("clan-delete"),
                Message::AskDelete(Confirm::Clan(c.id.clone())),
            ),
        ]
        .spacing(14)
        .into()
    }

    fn view_confirm(&self, confirm: &Confirm) -> Element<'_, Message> {
        let (title, body) = match confirm {
            Confirm::Person(id) => (
                tf(
                    "confirm-delete-title",
                    &[("name", self.data.person(id).map_or("", |p| p.name.as_str()).into())],
                ),
                t("confirm-delete-person"),
            ),
            Confirm::Clan(id) => (
                tf(
                    "confirm-delete-title",
                    &[("name", self.data.clan(id).map_or("", |c| c.name.as_str()).into())],
                ),
                t("confirm-delete-clan"),
            ),
            Confirm::Page => (
                tf(
                    "confirm-delete-title",
                    &[("name", self.note.as_ref().map_or("", |n| n.name.as_str()).into())],
                ),
                t("confirm-delete-page"),
            ),
        };
        let dialog = container(
            column![
                row![
                    container(icon("⚠").size(18).color(ui::pal().danger))
                        .center(Length::Fixed(38.0))
                        .style(ui::avatar(ui::pal().danger)),
                    column![
                        text(title).size(16).font(ui::BOLD),
                        text(body).size(13).color(ui::pal().muted),
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                ]
                .spacing(14),
                row![
                    space::horizontal(),
                    button(text(t("cancel")).size(13))
                        .padding([7, 16])
                        .style(ui::secondary)
                        .on_press(Message::CancelDelete),
                    button(text(t("confirm-delete")).size(13))
                        .padding([7, 16])
                        .style(ui::danger_solid)
                        .on_press(Message::ConfirmDelete),
                ]
                .spacing(8),
            ]
            .spacing(22),
        )
        .width(Length::Fixed(440.0))
        .padding(22)
        .style(ui::dialog);

        container(dialog)
            .center(Length::Fill)
            .style(ui::backdrop)
            .into()
    }

    fn view_status(&self) -> Element<'_, Message> {
        let pending = self.db.pending();
        let (dot, save) = if let Some(e) = self.db.last_error() {
            (ui::pal().danger, tf("status-save-error", &[("error", e.to_string().into())]))
        } else if pending > 0 || self.dirty.is_some() {
            (ui::pal().note_mark, tf("status-saving", &[("pending", pending.into())]))
        } else {
            (ui::pal().success, t("status-saved").into())
        };
        let counts = tf(
            "status-counts",
            &[
                ("people", self.data.people.len().into()),
                ("clans", self.data.clans.len().into()),
                ("lines", self.data.links.len().into()),
            ],
        );

        container(
            row![
                container(text(""))
                    .width(Length::Fixed(7.0))
                    .height(Length::Fixed(7.0))
                    .style(ui::swatch(dot)),
                text(save).size(12).color(ui::pal().muted),
                text("·").size(12).color(ui::pal().subtle),
                text(counts).size(12).color(ui::pal().muted).width(Length::Fill),
                text(format!("{:.0}%", self.camera.zoom * 100.0))
                    .size(12)
                    .color(ui::pal().muted),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .padding([0, 12]),
        )
        .width(Length::Fill)
        .height(Length::Fixed(STATUS_H))
        .align_y(alignment::Vertical::Center)
        .style(ui::chrome)
        .into()
    }
}

// ---------------------------------------------------------------------------
// small view helpers
// ---------------------------------------------------------------------------

/// Small bold uppercase heading (settings pages, inspector cards).
pub fn section<'a>(label: impl ToString) -> Text<'a> {
    caption(label)
}

fn caption<'a>(label: impl ToString) -> Text<'a> {
    text(label.to_string())
        .size(11)
        .color(ui::pal().muted)
        .font(ui::BOLD)
}

pub fn hint<'a>(label: &'a str) -> Text<'a> {
    text(label).size(12).color(ui::pal().muted)
}

fn empty_hint<'a, M: 'a>(label: &str) -> Element<'a, M> {
    container(text(label.to_string()).size(12).color(ui::pal().subtle))
        .padding([6, 10])
        .into()
}

/// Symbol from the icon font.
pub fn icon<'a>(glyph: &'a str) -> Text<'a> {
    text(glyph).font(ui::ICON).size(14)
}

/// `glyph label` content for buttons.
pub fn icon_label<'a, M: 'a>(glyph: &'a str, label: &'a str) -> Element<'a, M> {
    row![icon(glyph).size(13), text(label).size(13)]
        .spacing(7)
        .align_y(alignment::Vertical::Center)
        .into()
}

/// Hover tooltip.
pub fn tip<'a, M: 'a>(content: impl Into<Element<'a, M>>, label: &'a str) -> Element<'a, M> {
    tooltip(
        content,
        container(text(label).size(12).color(ui::toast_text()))
            .padding([4, 8])
            .style(ui::tooltip),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

/// Hover tooltip with a keyboard shortcut.
fn tip_key<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    label: &'a str,
    key: &'a str,
) -> Element<'a, M> {
    tooltip(
        content,
        container(
            row![
                text(label).size(12).color(ui::toast_text()),
                text(key).size(11).color(ui::alpha(ui::toast_text(), 0.6)),
            ]
            .spacing(8),
        )
        .padding([4, 8])
        .style(ui::tooltip),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

fn kbd<'a, M: 'a>(keys: &'a str) -> Element<'a, M> {
    container(text(keys).size(11))
        .padding([1, 6])
        .style(ui::kbd)
        .into()
}

fn badge<'a, M: 'a>(count: usize) -> Element<'a, M> {
    container(text(count.to_string()).size(10))
        .padding([1, 6])
        .style(ui::badge)
        .into()
}

/// Round avatar with up to two initials.
fn avatar<'a, M: 'a>(name: &str, color: Color, size: f32) -> Element<'a, M> {
    let initials: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    container(text(initials).size(size * 0.38).font(ui::SEMIBOLD))
        .center(Length::Fixed(size))
        .style(ui::avatar(color))
        .into()
}

/// Inspector card with a caption.
fn card<'a, M: 'a>(title: impl ToString, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(column![caption(title), content.into()].spacing(10))
        .width(Length::Fill)
        .padding(14)
        .style(ui::section_card)
        .into()
}

/// Field with a small label above it.
fn labelled<'a, M: 'a>(label: &'a str, field: impl Into<Element<'a, M>>) -> Element<'a, M> {
    column![text(label).size(11).color(ui::pal().muted), field.into()]
        .spacing(4)
        .width(Length::Fill)
        .into()
}

/// "+ New …" row at the end of a sidebar list.
fn add_row<'a, M: Clone + 'a>(label: &'a str, msg: M) -> Element<'a, M> {
    button(icon_label("+", label))
        .width(Length::Fill)
        .padding([5, 10])
        .style(ui::add_row)
        .on_press(msg)
        .into()
}

/// Explanation plus a destructive button at the bottom of the inspector.
fn danger_zone<'a, M: Clone + 'a>(explain: &'a str, label: &'a str, msg: M) -> Element<'a, M> {
    column![
        hline(),
        hint(explain),
        button(icon_label("✕", label))
            .padding([7, 14])
            .style(ui::danger)
            .on_press(msg),
    ]
    .spacing(10)
    .into()
}

fn vrule<'a, M: 'a>(height: f32) -> Element<'a, M> {
    container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(height))
        .style(ui::divider)
        .into()
}

pub fn hline<'a, M: 'a>() -> Element<'a, M> {
    container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(ui::divider)
        .into()
}

pub fn vline<'a, M: 'a>() -> Element<'a, M> {
    container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(ui::divider)
        .into()
}

/// Maps a key press that no widget handled to a workspace message.
fn shortcut(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::Key;
    use keyboard::key::Named;

    match key.as_ref() {
        Key::Named(Named::Escape) => Some(Message::Escape),
        Key::Named(Named::Delete) => Some(Message::DeleteKey),
        Key::Named(Named::Enter) => Some(Message::EnterKey),
        Key::Named(Named::Space) => Some(Message::Shortcut(Box::new(Message::TogglePlay))),
        Key::Character(c) if modifiers.command() => match c {
            "f" => Some(Message::FocusSearch),
            "b" => Some(Message::ToggleSidebar),
            "," => Some(Message::OpenSettings(SettingsPage::General)),
            _ => None,
        },
        Key::Character(c) if !modifiers.alt() => {
            let msg = match c {
                "v" => Message::Tool(Tool::Select),
                "g" => Message::Tool(Tool::Clan),
                "p" => Message::Tool(Tool::AddPerson),
                "c" => Message::Tool(Tool::Connect),
                "f" => Message::FitToWindow,
                "t" => Message::ToggleTimeline,
                "+" | "=" => Message::ZoomStep(1.25),
                "-" => Message::ZoomStep(1.0 / 1.25),
                "0" => Message::ZoomReset,
                _ => return None,
            };
            Some(Message::Shortcut(Box::new(msg)))
        }
        _ => None,
    }
}
