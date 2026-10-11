//! Application shell: global settings, workspace switching, settings dialog, update check.
//!
//! The shell owns the open workspace (an [`App`], if any) and everything that outlives
//! it. With no workspace it shows the welcome screen that asks for a location.

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::widget::{
    Column, Row, button, checkbox, column, container, image, opaque, pick_list, row, scrollable,
    stack, text, text_input,
};
use iced::{Element, Event, Font, Length, Size, Subscription, Task, Theme, alignment, event, font, keyboard, window};

use crate::app::{App, Config, Init, Message, hint, hline, section, vline};
use crate::files;
use crate::i18n::{self, Lang, t, tf};
use crate::settings::{self, Settings};
use crate::store::{self, Db};
use crate::ui;
use crate::update::{self, Check};

const LOGO: &[u8] = include_bytes!("../assets/icon-256.png");
const DIALOG_W: f32 = 780.0;
const DIALOG_H: f32 = 520.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    General,
    Workspace,
    Updates,
    About,
}

impl SettingsPage {
    const ALL: [SettingsPage; 4] = [Self::General, Self::Workspace, Self::Updates, Self::About];

    fn label(self) -> &'static str {
        match self {
            Self::General => t("General"),
            Self::Workspace => t("Workspace"),
            Self::Updates => t("Updates"),
            Self::About => t("About"),
        }
    }
}

/// Command line: `wurzelwerk [WORKSPACE_DIR] [--demo 40x250]` (env `WURZELWERK_PROJECT`).
#[derive(Debug, Clone, Default)]
pub struct Cli {
    pub workspace: Option<PathBuf>,
    pub demo: Option<(usize, usize)>,
}

impl Cli {
    pub fn parse() -> Self {
        let mut workspace = std::env::var_os("WURZELWERK_PROJECT")
            .or_else(|| std::env::var_os("SKETCHBOOK_PROJECT"))
            .map(PathBuf::from);
        let mut demo = None;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--demo" {
                demo = args.next().and_then(|v| {
                    let (c, p) = v.split_once(['x', 'X'])?;
                    Some((c.trim().parse().ok()?, p.trim().parse().ok()?))
                });
            } else {
                workspace = Some(PathBuf::from(arg));
            }
        }
        Self {
            workspace: workspace.map(absolute),
            demo,
        }
    }
}

fn absolute(path: PathBuf) -> PathBuf {
    std::path::absolute(&path).unwrap_or(path)
}

/// Project folders created before the workspace setting existed live next to the working directory.
fn legacy_workspace() -> Option<PathBuf> {
    ["wurzelwerk-project", "sketchbook-project"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_dir())
        .map(absolute)
}

#[derive(Debug, Clone)]
enum UpdateState {
    Idle,
    Checking,
    Done(Check),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum ShellMessage {
    App(Message),
    Resized(Size),
    Escape,
    // workspace
    PathInput(String),
    Browse,
    Browsed(Option<PathBuf>),
    SubmitPath,
    OpenWorkspace(PathBuf),
    // settings dialog
    OpenSettings(SettingsPage),
    CloseSettings,
    SetLanguage(Lang),
    SetCheckUpdates(bool),
    // updates
    CheckUpdates,
    UpdateChecked(Result<Check, String>),
    DismissUpdate,
    OpenPath(PathBuf),
    OpenUrl(String),
}

pub struct Shell {
    settings: Settings,
    app: Option<App>,
    /// Location typed / picked for the next workspace.
    path_input: String,
    /// Last problem opening a workspace or saving the settings.
    error: Option<String>,
    dialog: Option<SettingsPage>,
    update: UpdateState,
    update_dismissed: bool,
    window: Option<Size>,
    logo: image::Handle,
}

impl Shell {
    pub fn boot(cli: Cli) -> (Self, Task<ShellMessage>) {
        let settings = Settings::load();
        i18n::set(settings.language);

        let mut shell = Self {
            path_input: settings::default_workspace().display().to_string(),
            settings,
            app: None,
            error: None,
            dialog: None,
            update: UpdateState::Idle,
            update_dismissed: false,
            window: None,
            logo: image::Handle::from_bytes(LOGO),
        };

        let saved = shell
            .settings
            .workspace
            .clone()
            .filter(|p| p.is_dir());
        let (start, persist) = match (cli.workspace, saved) {
            (Some(cli), _) => (Some(cli), false),
            (None, Some(saved)) => (Some(saved), false),
            (None, None) => (legacy_workspace(), true),
        };

        let mut tasks = Vec::new();
        if let Some(path) = start {
            tasks.push(shell.open_workspace(path, persist, cli.demo));
        }
        if shell.settings.check_updates {
            tasks.push(shell.start_update_check());
        }
        (shell, Task::batch(tasks))
    }

    pub fn title(&self) -> String {
        match &self.app {
            Some(app) => format!("{} — Wurzelwerk", app.workspace_name()),
            None => "Wurzelwerk".into(),
        }
    }

    pub fn theme(&self) -> Theme {
        Theme::Light
    }

    pub fn subscription(&self) -> Subscription<ShellMessage> {
        let resize = event::listen_with(|ev, status, _window| match ev {
            Event::Window(window::Event::Resized(size)) => Some(ShellMessage::Resized(size)),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) if status == event::Status::Ignored => Some(ShellMessage::Escape),
            _ => None,
        });
        // the workspace subscribes to Escape itself
        let mut subs = vec![resize];
        if let Some(app) = &self.app {
            subs.push(app.subscription().map(ShellMessage::App));
        }
        Subscription::batch(subs)
    }

    // -----------------------------------------------------------------------
    // update
    // -----------------------------------------------------------------------

    pub fn update(&mut self, message: ShellMessage) -> Task<ShellMessage> {
        match message {
            ShellMessage::App(Message::OpenSettings(page)) => self.dialog = Some(page),
            ShellMessage::App(Message::Escape) if self.dialog.is_some() => self.dialog = None,
            ShellMessage::App(msg) => {
                if let Message::WindowResized(size) = msg {
                    self.window = Some(size);
                }
                // the dialog is modal: keep keyboard shortcuts away from the board
                if self.dialog.is_some()
                    && matches!(msg, Message::DeleteKey | Message::Escape)
                {
                    return Task::none();
                }
                if let Some(app) = &mut self.app {
                    return app.update(msg).map(ShellMessage::App);
                }
            }
            ShellMessage::Resized(size) => self.window = Some(size),
            ShellMessage::Escape => {
                if self.app.is_none() {
                    self.dialog = None;
                }
            }

            ShellMessage::PathInput(path) => self.path_input = path,
            ShellMessage::Browse => {
                let start = PathBuf::from(&self.path_input);
                return Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new()
                            .set_title(t("Choose workspace folder"));
                        if let Some(dir) = Some(start).filter(|p| p.is_dir()) {
                            dialog = dialog.set_directory(dir);
                        }
                        dialog
                            .pick_folder()
                            .await
                            .map(|h| h.path().to_path_buf())
                    },
                    ShellMessage::Browsed,
                );
            }
            ShellMessage::Browsed(path) => {
                if let Some(path) = path {
                    self.path_input = path.display().to_string();
                }
            }
            ShellMessage::SubmitPath => {
                let input = self.path_input.trim();
                if input.is_empty() {
                    self.error = Some(t("Please enter a location.").into());
                } else {
                    let path = absolute(PathBuf::from(input));
                    return self.open_workspace(path, true, None);
                }
            }
            ShellMessage::OpenWorkspace(path) => return self.open_workspace(path, true, None),

            ShellMessage::OpenSettings(page) => self.dialog = Some(page),
            ShellMessage::CloseSettings => self.dialog = None,
            ShellMessage::SetLanguage(lang) => {
                self.settings.language = lang;
                i18n::set(lang);
                self.save_settings();
                if let Some(app) = &mut self.app {
                    // text drawn on the canvas is cached
                    app.cache.clear();
                }
            }
            ShellMessage::SetCheckUpdates(on) => {
                self.settings.check_updates = on;
                self.save_settings();
            }

            ShellMessage::CheckUpdates => {
                self.update_dismissed = false;
                return self.start_update_check();
            }
            ShellMessage::UpdateChecked(result) => {
                self.update = match result {
                    Ok(check) => UpdateState::Done(check),
                    Err(e) => UpdateState::Failed(e),
                };
            }
            ShellMessage::DismissUpdate => self.update_dismissed = true,
            ShellMessage::OpenPath(path) => {
                if let Err(e) = files::open(&path) {
                    self.error = Some(e);
                }
            }
            ShellMessage::OpenUrl(url) => {
                // the URL comes from the GitHub API; only follow links to the project
                if url.starts_with("https://github.com/")
                    && let Err(e) = files::open(Path::new(&url))
                {
                    self.error = Some(e);
                }
            }
        }
        Task::none()
    }

    fn start_update_check(&mut self) -> Task<ShellMessage> {
        self.update = UpdateState::Checking;
        Task::perform(update::check_async(), ShellMessage::UpdateChecked)
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.error = Some(tf("Could not save settings: {}", &[&e]));
        }
    }

    /// Opens (creating if needed) the workspace folder and makes it current.
    /// The old workspace stays open when the new one cannot be opened.
    fn open_workspace(
        &mut self,
        path: PathBuf,
        persist: bool,
        demo: Option<(usize, usize)>,
    ) -> Task<ShellMessage> {
        if self.app.as_ref().is_some_and(|app| app.cfg.project == path) {
            self.dialog = None;
            return Task::none();
        }

        let (mut app, task) = match open_app(Config { project: path.clone(), demo }) {
            Ok(opened) => opened,
            Err(e) => {
                self.error = Some(e);
                return Task::none();
            }
        };
        if let Some(mut old) = self.app.take() {
            old.shutdown();
        }
        if let Some(size) = self.window {
            let _ = app.update(Message::WindowResized(size));
        }

        self.app = Some(app);
        self.error = None;
        self.dialog = None;
        self.path_input = path.display().to_string();
        self.settings.remember(&path);
        if persist {
            self.settings.workspace = Some(path);
        }
        self.save_settings();
        task.map(ShellMessage::App)
    }

    // -----------------------------------------------------------------------
    // view
    // -----------------------------------------------------------------------

    pub fn view(&self) -> Element<'_, ShellMessage> {
        let main: Element<'_, ShellMessage> = match &self.app {
            Some(app) => app.view().map(ShellMessage::App),
            None => self.view_welcome(),
        };

        let mut page = Column::new();
        if let Some(release) = self.available_update() {
            page = page.push(self.view_update_banner(release)).push(hline());
        }
        let page = page.push(container(main).height(Length::Fill));

        match self.dialog {
            Some(dialog) => stack![page, opaque(self.view_dialog(dialog))].into(),
            None => page.into(),
        }
    }

    fn available_update(&self) -> Option<&update::Release> {
        match &self.update {
            UpdateState::Done(Check::Available(release)) if !self.update_dismissed => {
                Some(release)
            }
            _ => None,
        }
    }

    fn view_update_banner(&self, release: &update::Release) -> Element<'_, ShellMessage> {
        container(
            row![
                text(tf("Version {} is available.", &[&release.version]))
                    .size(13)
                    .width(Length::Fill),
                button(text(t("Download")).size(12))
                    .padding([3, 10])
                    .style(ui::tool(true))
                    .on_press(ShellMessage::OpenUrl(release.url.clone())),
                button(text(t("Dismiss")).size(12))
                    .padding([3, 10])
                    .style(ui::tool(false))
                    .on_press(ShellMessage::DismissUpdate),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .padding([6, 12]),
        )
        .width(Length::Fill)
        .style(ui::banner)
        .into()
    }

    fn view_welcome(&self) -> Element<'_, ShellMessage> {
        let mut body = Column::new()
            .spacing(14)
            .width(Length::Fixed(520.0))
            .align_x(alignment::Horizontal::Center)
            .push(image(self.logo.clone()).width(112).height(112))
            .push(text(t("Welcome to Wurzelwerk")).size(26).font(bold()))
            .push(
                text(t(
                    "Create a workspace to get started. A workspace is a folder that holds your families, clans, notes and files.",
                ))
                .size(14)
                .color(ui::MUTED)
                .align_x(alignment::Horizontal::Center),
            )
            .push(
                column![
                    section(t("Workspace location")),
                    self.path_row(t("Create workspace")),
                ]
                .spacing(6)
                .width(Length::Fill),
            );
        if let Some(e) = &self.error {
            body = body.push(text(e.as_str()).size(13).color(ui::DANGER));
        }

        column![
            container(
                button(text(t("Settings")).size(13))
                    .padding([6, 12])
                    .style(ui::tool(false))
                    .on_press(ShellMessage::OpenSettings(SettingsPage::General))
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right)
            .padding(12),
            container(body).center(Length::Fill),
        ]
        .into()
    }

    /// `[ path input ] [Browse…] [action]` shared by the welcome screen and the workspace page.
    fn path_row(&self, action: &'static str) -> Element<'_, ShellMessage> {
        row![
            text_input("", &self.path_input)
                .on_input(ShellMessage::PathInput)
                .on_submit(ShellMessage::SubmitPath)
                .size(13)
                .padding(7),
            button(text(t("Browse…")).size(13))
                .padding([7, 12])
                .style(ui::tool(false))
                .on_press(ShellMessage::Browse),
            button(text(action).size(13))
                .padding([7, 12])
                .style(ui::tool(true))
                .on_press(ShellMessage::SubmitPath),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .into()
    }

    fn view_dialog(&self, current: SettingsPage) -> Element<'_, ShellMessage> {
        let mut nav = Column::new().spacing(2).width(Length::Fixed(180.0));
        for page in SettingsPage::ALL {
            nav = nav.push(
                button(text(page.label()).size(14))
                    .width(Length::Fill)
                    .padding([7, 12])
                    .style(ui::list_item(page == current))
                    .on_press(ShellMessage::OpenSettings(page)),
            );
        }

        let content: Element<'_, ShellMessage> = match current {
            SettingsPage::General => self.page_general(),
            SettingsPage::Workspace => self.page_workspace(),
            SettingsPage::Updates => self.page_updates(),
            SettingsPage::About => self.page_about(),
        };

        let header = row![
            text(t("Settings")).size(16).font(bold()).width(Length::Fill),
            button(text(t("Close")).size(13))
                .padding([4, 12])
                .style(ui::tool(false))
                .on_press(ShellMessage::CloseSettings),
        ]
        .align_y(alignment::Vertical::Center)
        .padding([10, 16]);

        let dialog = container(
            column![
                header,
                hline(),
                row![
                    container(nav).padding(10).style(ui::panel).height(Length::Fill),
                    vline(),
                    scrollable(container(content).padding(24).width(Length::Fill))
                        .width(Length::Fill)
                        .height(Length::Fill),
                ]
                .height(Length::Fill),
            ],
        )
        .width(Length::Fixed(DIALOG_W))
        .height(Length::Fixed(DIALOG_H))
        .style(ui::dialog);

        container(dialog)
            .center(Length::Fill)
            .style(ui::backdrop)
            .into()
    }

    fn page_general(&self) -> Element<'_, ShellMessage> {
        column![
            section(t("Language")),
            hint(t("Language of the user interface.")),
            pick_list(Lang::ALL, Some(self.settings.language), ShellMessage::SetLanguage)
                .text_size(13)
                .padding([6, 10]),
        ]
        .spacing(10)
        .into()
    }

    fn page_workspace(&self) -> Element<'_, ShellMessage> {
        let mut page = Column::new().spacing(10);

        if let Some(app) = &self.app {
            page = page
                .push(section(t("Current workspace")))
                .push(text(app.cfg.project.display().to_string()).size(13))
                .push(
                    button(text(t("Open folder")).size(13))
                        .padding([6, 12])
                        .style(ui::tool(false))
                        .on_press(ShellMessage::OpenPath(app.cfg.project.clone())),
                );
        }

        page = page
            .push(section(t("Switch workspace")))
            .push(hint(t(
                "Pick a folder to open it as workspace. A new or empty folder becomes a new workspace.",
            )))
            .push(self.path_row(t("Open")));
        if let Some(e) = &self.error {
            page = page.push(text(e.as_str()).size(13).color(ui::DANGER));
        }

        let current = self.app.as_ref().map(|a| &a.cfg.project);
        let mut recent = Column::new().spacing(2);
        let mut any = false;
        for path in self.settings.recent.iter().filter(|p| Some(*p) != current) {
            any = true;
            recent = recent.push(
                button(text(path.display().to_string()).size(13))
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(ui::card)
                    .on_press(ShellMessage::OpenWorkspace(path.clone())),
            );
        }
        if !any {
            recent = recent.push(hint(t("No other workspaces yet.")));
        }
        page.push(section(t("Recent workspaces"))).push(recent).into()
    }

    fn page_updates(&self) -> Element<'_, ShellMessage> {
        let status: Element<'_, ShellMessage> = match &self.update {
            UpdateState::Idle => text("").into(),
            UpdateState::Checking => text(t("Checking…")).size(13).into(),
            UpdateState::Done(Check::UpToDate) => text(t("You are up to date.")).size(13).into(),
            UpdateState::Done(Check::Available(release)) => row![
                text(tf("Version {} is available.", &[&release.version]))
                    .size(13)
                    .width(Length::Fill),
                button(text(t("Download")).size(13))
                    .padding([6, 12])
                    .style(ui::tool(true))
                    .on_press(ShellMessage::OpenUrl(release.url.clone())),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .into(),
            UpdateState::Failed(e) => text(tf("Update check failed: {}", &[e]))
                .size(13)
                .color(ui::DANGER)
                .into(),
        };

        let mut check = button(text(t("Check now")).size(13))
            .padding([6, 12])
            .style(ui::tool(false));
        if !matches!(self.update, UpdateState::Checking) {
            check = check.on_press(ShellMessage::CheckUpdates);
        }

        column![
            section(t("Updates")),
            text(tf("Current version: {}", &[&update::VERSION])).size(13),
            checkbox(self.settings.check_updates)
                .label(t("Check for updates on start"))
                .on_toggle(ShellMessage::SetCheckUpdates)
                .text_size(13),
            row![check, container(status).width(Length::Fill)]
                .spacing(12)
                .align_y(alignment::Vertical::Center),
        ]
        .spacing(12)
        .into()
    }

    fn page_about(&self) -> Element<'_, ShellMessage> {
        let link = |label: &'static str, url: &'static str| {
            button(text(label).size(13))
                .padding([6, 12])
                .style(ui::tool(false))
                .on_press(ShellMessage::OpenUrl(url.into()))
        };

        let title = row![
            image(self.logo.clone()).width(88).height(88),
            column![
                text("Wurzelwerk").size(26).font(bold()),
                text(tf("Version {}", &[&update::VERSION])).size(14),
                text(t("Clans & families board")).size(13).color(ui::MUTED),
            ]
            .spacing(4),
        ]
        .spacing(18)
        .align_y(alignment::Vertical::Center);

        let settings_file = settings::settings_path();
        let folder = settings::config_dir();

        column![
            title,
            section(t("Settings file")),
            hint(t("Global settings are stored here:")),
            text(settings_file.display().to_string()).size(13),
            button(text(t("Open folder")).size(13))
                .padding([6, 12])
                .style(ui::tool(false))
                .on_press(ShellMessage::OpenPath(folder)),
            section(t("Licence")),
            text("MIT OR Apache-2.0").size(13),
            section(t("Source code")),
            Row::new().push(link(update::REPO_URL, update::REPO_URL)),
        ]
        .spacing(10)
        .into()
    }
}

fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::DEFAULT
    }
}

/// Opens the database of a workspace folder and boots the board on it.
fn open_app(cfg: Config) -> Result<(App, Task<Message>), String> {
    std::fs::create_dir_all(&cfg.project)
        .map_err(|e| tf("cannot create {}: {}", &[&cfg.project.display(), &e]))?;
    store::migrate_legacy_db(&cfg.project);

    // a workspace that was closed a moment ago may still be flushing its file
    let path = cfg.project.join(store::DB_FILE);
    let mut attempt = 0;
    let (db, data) = loop {
        match Db::open(path.clone()) {
            Ok(opened) => break opened,
            Err(_) if attempt < 3 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => {
                return Err(tf(
                    "cannot open the database in {}: {}",
                    &[&cfg.project.display(), &e],
                ));
            }
        }
    };
    Ok(App::boot(Init { cfg, db, data }))
}
