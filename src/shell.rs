//! Application shell: global settings, workspace switching, settings dialog, update check.
//!
//! The shell owns the open workspace (an [`App`], if any) and everything that outlives
//! it. With no workspace it shows the welcome screen that asks for a location.

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::widget::{
    Column, Row, button, checkbox, column, container, image, opaque, pick_list, row, scrollable,
    space, stack, text, text_input,
};
use iced::{Element, Event, Length, Padding, Size, Subscription, Task, Theme, alignment, event, keyboard, theme, window};

use crate::app::{App, Config, Init, Message, hint, hline, icon, icon_label, section, tip, vline};
use crate::demo;
use crate::files;
use crate::i18n::{self, Lang, t, tf};
use crate::settings::{self, Appearance, Settings};
use crate::store::{self, Db};
use crate::ui;
use crate::update::{self, Check, Downloaded, Release};

const LOGO: &[u8] = include_bytes!("../assets/icon-256.png");
const DIALOG_W: f32 = 820.0;
const DIALOG_H: f32 = 560.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    General,
    Workspace,
    /// Translations of the workspace's own line names; only listed when there are some.
    Relations,
    Updates,
    About,
}

impl SettingsPage {
    const ALL: [SettingsPage; 5] = [
        Self::General,
        Self::Workspace,
        Self::Relations,
        Self::Updates,
        Self::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => t("settings-general"),
            Self::Workspace => t("settings-workspace"),
            Self::Relations => t("settings-relations-page"),
            Self::Updates => t("settings-updates"),
            Self::About => t("settings-about"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::General => "⚙",
            Self::Workspace => "▤",
            Self::Relations => "↔",
            Self::Updates => "⟳",
            Self::About => "✦",
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
    /// Fetching the installer in the background.
    Downloading(Release),
    /// Installer verified; installs on [`ShellMessage::InstallUpdate`].
    Ready(Release, Downloaded),
    /// Already translated message.
    Failed(String),
}

/// A new version and the button that gets it ([`Shell::update_offer`]).
struct UpdateOffer {
    text: String,
    action: &'static str,
    message: ShellMessage,
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
    /// Opens the example workspace, generating it on first use.
    OpenExample,
    // settings dialog
    OpenSettings(SettingsPage),
    CloseSettings,
    SetLanguage(Lang),
    SetAppearance(Appearance),
    SystemTheme(theme::Mode),
    SetCheckUpdates(bool),
    // updates
    CheckUpdates,
    UpdateChecked(Result<Check, String>),
    UpdateDownloaded(Result<Downloaded, String>),
    InstallUpdate,
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
    /// Operating system prefers a dark look (used by [`Appearance::System`]).
    system_dark: bool,
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
            system_dark: false,
        };
        shell.apply_appearance();

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

        let mut tasks = vec![iced::system::theme().map(ShellMessage::SystemTheme)];
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
        ui::theme()
    }

    fn dark(&self) -> bool {
        match self.settings.appearance {
            Appearance::System => self.system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }

    /// Switches the palette and redraws everything that caches colours.
    fn apply_appearance(&mut self) {
        ui::set_dark(self.dark());
        if let Some(app) = &mut self.app {
            app.restyle();
        }
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
        let mut subs = vec![
            resize,
            iced::system::theme_changes().map(ShellMessage::SystemTheme),
        ];
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
            ShellMessage::App(Message::ToggleTheme) => {
                self.settings.appearance = if self.dark() {
                    Appearance::Light
                } else {
                    Appearance::Dark
                };
                self.apply_appearance();
                self.save_settings();
            }
            ShellMessage::App(msg) => {
                if let Message::WindowResized(size) = msg {
                    self.window = Some(size);
                }
                // the dialog is modal: keep keyboard shortcuts away from the board
                if self.dialog.is_some()
                    && matches!(
                        msg,
                        Message::DeleteKey
                            | Message::Escape
                            | Message::EnterKey
                            | Message::Shortcut(_)
                            | Message::FocusSearch
                            | Message::ToggleSidebar
                    )
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
                            .set_title(t("welcome-choose-folder"));
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
                    self.error = Some(t("welcome-enter-location").into());
                } else {
                    let path = absolute(PathBuf::from(input));
                    return self.open_workspace(path, true, None);
                }
            }
            ShellMessage::OpenWorkspace(path) => return self.open_workspace(path, true, None),
            ShellMessage::OpenExample => {
                return self.open_workspace(
                    settings::example_workspace(),
                    true,
                    Some(demo::EXAMPLE),
                );
            }

            ShellMessage::OpenSettings(page) => self.dialog = Some(page),
            ShellMessage::CloseSettings => self.dialog = None,
            ShellMessage::SetLanguage(lang) => {
                self.settings.language = lang;
                i18n::set(lang);
                self.save_settings();
                if let Some(app) = &mut self.app {
                    // line labels change width; text drawn on the canvas is cached
                    app.routes.relabel(&app.data);
                    app.cache.clear();
                }
            }
            ShellMessage::SetAppearance(appearance) => {
                self.settings.appearance = appearance;
                self.apply_appearance();
                self.save_settings();
            }
            ShellMessage::SystemTheme(mode) => {
                self.system_dark = mode == theme::Mode::Dark;
                self.apply_appearance();
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
                    Ok(Check::Available(release)) if release.package.is_some() => {
                        let package = release.package.clone().expect("checked above");
                        self.update = UpdateState::Downloading(release);
                        return Task::perform(
                            update::download_async(package),
                            ShellMessage::UpdateDownloaded,
                        );
                    }
                    Ok(check) => UpdateState::Done(check),
                    Err(e) => UpdateState::Failed(tf("update-check-failed", &[("error", e.to_string().into())])),
                };
            }
            ShellMessage::UpdateDownloaded(result) => {
                if let UpdateState::Downloading(release) = &self.update {
                    self.update = match result {
                        Ok(downloaded) => UpdateState::Ready(release.clone(), downloaded),
                        Err(e) => UpdateState::Failed(tf("update-failed", &[("error", e.to_string().into())])),
                    };
                }
            }
            ShellMessage::InstallUpdate => {
                if let UpdateState::Ready(_, downloaded) = &self.update {
                    if let Some(app) = &mut self.app {
                        app.shutdown();
                        app.db.sync();
                    }
                    // returns only on failure; success restarts the app
                    let Err(e) = update::install(downloaded.clone());
                    self.update = UpdateState::Failed(tf("update-failed", &[("error", e.to_string().into())]));
                }
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
            self.error = Some(tf("error-save-settings", &[("error", e.to_string().into())]));
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
        app.restyle();

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
        if !self.update_dismissed
            && let Some(offer) = self.update_offer()
        {
            page = page.push(self.view_update_banner(offer)).push(hline());
        }
        let page = page.push(container(main).height(Length::Fill));

        match self.dialog {
            Some(dialog) => stack![page, opaque(self.view_dialog(dialog))].into(),
            None => page.into(),
        }
    }

    /// What the user can do about a new version: a downloaded update to install, or (for builds
    /// that cannot update themselves) the release page to open.
    fn update_offer(&self) -> Option<UpdateOffer> {
        match &self.update {
            UpdateState::Ready(release, _) => Some(UpdateOffer {
                text: tf("update-ready", &[("version", release.version.to_string().into())]),
                action: t("update-restart"),
                message: ShellMessage::InstallUpdate,
            }),
            UpdateState::Done(Check::Available(release)) => Some(UpdateOffer {
                text: tf("update-available", &[("version", release.version.to_string().into())]),
                action: t("update-download"),
                message: ShellMessage::OpenUrl(release.url.clone()),
            }),
            _ => None,
        }
    }

    fn view_update_banner(&self, offer: UpdateOffer) -> Element<'_, ShellMessage> {
        container(
            row![
                icon("✦").size(14).color(ui::pal().accent),
                text(offer.text).size(13).width(Length::Fill),
                button(text(offer.action).size(12))
                    .padding([4, 12])
                    .style(ui::primary)
                    .on_press(offer.message),
                button(text(t("update-dismiss")).size(12))
                    .padding([4, 12])
                    .style(ui::secondary)
                    .on_press(ShellMessage::DismissUpdate),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center)
            .padding([6, 14]),
        )
        .width(Length::Fill)
        .style(ui::banner)
        .into()
    }

    fn view_welcome(&self) -> Element<'_, ShellMessage> {
        let mut form = Column::new()
            .spacing(10)
            .width(Length::Fill)
            .push(section(t("welcome-location")))
            .push(self.path_row(t("welcome-create")))
            .push(hint(t(
                "welcome-location-help",
            )));
        if let Some(e) = &self.error {
            form = form.push(text(e.as_str()).size(13).color(ui::pal().danger));
        }

        let mut card = Column::new()
            .spacing(16)
            .align_x(alignment::Horizontal::Center)
            .push(image(self.logo.clone()).width(96).height(96))
            .push(
                column![
                    text(t("welcome-title")).size(28).font(ui::BOLD),
                    text(t(
                        "welcome-body",
                    ))
                    .size(14)
                    .color(ui::pal().muted)
                    .align_x(alignment::Horizontal::Center),
                ]
                .spacing(8)
                .align_x(alignment::Horizontal::Center),
            )
            .push(container(form).padding(18).style(ui::section_card))
            .push(example_row());

        if !self.settings.recent.is_empty() {
            let mut recent = Column::new().spacing(4).width(Length::Fill);
            recent = recent.push(section(t("welcome-recent")));
            for path in &self.settings.recent {
                recent = recent.push(recent_row(path));
            }
            card = card.push(recent);
        }

        let top = row![
            space::horizontal(),
            tip(
                button(icon(if ui::pal().dark { "☀" } else { "☾" }).size(15))
                    .padding([5, 9])
                    .style(ui::icon)
                    .on_press(ShellMessage::SetAppearance(if ui::pal().dark {
                        Appearance::Light
                    } else {
                        Appearance::Dark
                    })),
                if ui::pal().dark {
                    t("tool-light-mode")
                } else {
                    t("tool-dark-mode")
                },
            ),
            button(icon_label("⚙", t("tool-settings")))
                .padding([6, 12])
                .style(ui::secondary)
                .on_press(ShellMessage::OpenSettings(SettingsPage::General)),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .padding(14);

        container(
            column![
                top,
                container(
                    scrollable(
                        container(card.width(Length::Fixed(560.0)))
                            .width(Length::Fill)
                            .align_x(alignment::Horizontal::Center)
                            .padding([0, 24]),
                    )
                    .style(ui::scroll),
                )
                .center(Length::Fill),
            ],
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme| container::Style {
            background: Some(ui::pal().board.into()),
            ..ui::panel(theme)
        })
        .into()
    }

    /// `[ path input ] [Browse…] [action]` shared by the welcome screen and the workspace page.
    fn path_row(&self, action: &'static str) -> Element<'_, ShellMessage> {
        row![
            text_input(t("welcome-folder-path"), &self.path_input)
                .on_input(ShellMessage::PathInput)
                .on_submit(ShellMessage::SubmitPath)
                .size(13)
                .padding([8, 10])
                .style(ui::input),
            button(text(t("settings-browse")).size(13))
                .padding([8, 12])
                .style(ui::secondary)
                .on_press(ShellMessage::Browse),
            button(text(action).size(13))
                .padding([8, 14])
                .style(ui::primary)
                .on_press(ShellMessage::SubmitPath),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .into()
    }

    fn view_dialog(&self, current: SettingsPage) -> Element<'_, ShellMessage> {
        let mut nav = Column::new()
            .spacing(2)
            .width(Length::Fixed(200.0))
            .push(
                container(text(t("tool-settings")).size(18).font(ui::BOLD))
                    .padding([6, 10])
                    .height(Length::Fixed(44.0)),
            );
        let has_relations = self.app.as_ref().is_some_and(|a| !a.data.relations.is_empty());
        for page in SettingsPage::ALL {
            // the relations page stays listed while open, so removing the last one doesn't yank it away
            if page == SettingsPage::Relations && !has_relations && current != page {
                continue;
            }
            let active = page == current;
            nav = nav.push(
                button(
                    row![
                        icon(page.icon()).size(14).color(if active {
                            ui::pal().accent
                        } else {
                            ui::pal().muted
                        }),
                        text(page.label()).size(14),
                    ]
                    .spacing(10)
                    .align_y(alignment::Vertical::Center),
                )
                .width(Length::Fill)
                .padding([8, 12])
                .style(ui::list_item(active))
                .on_press(ShellMessage::OpenSettings(page)),
            );
        }

        let content: Element<'_, ShellMessage> = match current {
            SettingsPage::General => self.page_general(),
            SettingsPage::Workspace => self.page_workspace(),
            SettingsPage::Relations => self.page_relations(),
            SettingsPage::Updates => self.page_updates(),
            SettingsPage::About => self.page_about(),
        };

        let header = row![
            text(current.label()).size(18).font(ui::BOLD).width(Length::Fill),
            tip(
                button(icon("✕").size(14))
                    .padding([5, 9])
                    .style(ui::icon)
                    .on_press(ShellMessage::CloseSettings),
                t("settings-close"),
            ),
        ]
        .align_y(alignment::Vertical::Center)
        .height(Length::Fixed(44.0));

        let dialog = container(
            row![
                container(nav)
                    .padding(12)
                    .style(ui::chrome)
                    .height(Length::Fill),
                vline(),
                column![
                    container(header).padding(Padding::from([10, 24]).right(14)),
                    scrollable(container(content).padding(Padding::from([8, 24]).bottom(24)).width(Length::Fill))
                        .style(ui::scroll)
                        .width(Length::Fill)
                        .height(Length::Fill),
                ]
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .height(Length::Fill),
        )
        .width(Length::Fixed(DIALOG_W))
        .height(Length::Fixed(DIALOG_H))
        .clip(true)
        .style(ui::dialog);

        container(dialog)
            .center(Length::Fill)
            .style(ui::backdrop)
            .into()
    }

    fn page_general(&self) -> Element<'_, ShellMessage> {
        let appearance = Row::with_children(Appearance::ALL.into_iter().map(|a| {
            let glyph = match a {
                Appearance::System => "◐",
                Appearance::Light => "☀",
                Appearance::Dark => "☾",
            };
            button(
                column![icon(glyph).size(20), text(a.to_string()).size(13)]
                    .spacing(6)
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Center),
            )
            .width(Length::Fixed(120.0))
            .padding([12, 10])
            .style(ui::segment(self.settings.appearance == a))
            .on_press(ShellMessage::SetAppearance(a))
            .into()
        }))
        .spacing(4);

        column![
            setting(
                t("settings-appearance"),
                t("settings-appearance-help"),
                container(appearance).padding(4).style(ui::segments),
            ),
            setting(
                t("settings-language"),
                t("settings-language-help"),
                pick_list(Lang::ALL, Some(self.settings.language), ShellMessage::SetLanguage)
                    .text_size(13)
                    .padding([7, 12])
                    .style(ui::pick)
                    .width(Length::Fixed(220.0)),
            ),
        ]
        .spacing(26)
        .into()
    }

    fn page_workspace(&self) -> Element<'_, ShellMessage> {
        let mut page = Column::new().spacing(26);

        if let Some(app) = &self.app {
            page = page.push(setting(
                t("settings-current-workspace"),
                t("settings-current-workspace-help"),
                container(
                    row![
                        icon("▤").size(18).color(ui::pal().accent),
                        column![
                            text(app.workspace_name()).size(14).font(ui::SEMIBOLD),
                            text(app.cfg.project.display().to_string())
                                .size(12)
                                .color(ui::pal().muted),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        button(text(t("settings-open-folder")).size(13))
                            .padding([6, 12])
                            .style(ui::secondary)
                            .on_press(ShellMessage::OpenPath(app.cfg.project.clone())),
                    ]
                    .spacing(12)
                    .align_y(alignment::Vertical::Center),
                )
                .padding(14)
                .style(ui::section_card),
            ));
        }

        let mut switch = Column::new().spacing(8).push(self.path_row(t("settings-open")));
        if let Some(e) = &self.error {
            switch = switch.push(text(e.as_str()).size(13).color(ui::pal().danger));
        }
        page = page.push(setting(
            t("settings-switch-workspace"),
            t("settings-switch-workspace-help"),
            switch,
        ));

        let current = self.app.as_ref().map(|a| &a.cfg.project);
        let mut recent = Column::new().spacing(4);
        let mut any = false;
        for path in self.settings.recent.iter().filter(|p| Some(*p) != current) {
            any = true;
            recent = recent.push(recent_row(path));
        }
        if !any {
            recent = recent.push(hint(t("settings-no-recent")));
        }
        page.push(setting(
            t("welcome-recent"),
            t("settings-recent-help"),
            recent,
        ))
        .push(setting(
            t("example-title"),
            t("example-help"),
            example_row(),
        ))
        .into()
    }

    fn page_relations(&self) -> Element<'_, ShellMessage> {
        let editor: Element<'_, ShellMessage> = match &self.app {
            Some(app) => relations_editor(app),
            None => hint(t("settings-no-relations")).into(),
        };
        setting(t("settings-relations"), t("settings-relations-help"), editor)
    }

    fn page_updates(&self) -> Element<'_, ShellMessage> {
        let status: Element<'_, ShellMessage> = match &self.update {
            UpdateState::Idle => text("").into(),
            UpdateState::Checking => text(t("settings-checking")).size(13).color(ui::pal().muted).into(),
            UpdateState::Done(Check::UpToDate) => row![
                icon("✓").size(13).color(ui::pal().success),
                text(t("settings-up-to-date")).size(13),
            ]
            .spacing(6)
            .into(),
            UpdateState::Done(Check::Available(_)) | UpdateState::Ready(..) => {
                let offer = self.update_offer().expect("new version on offer");
                row![
                    text(offer.text).size(13).width(Length::Fill),
                    button(text(offer.action).size(13))
                        .padding([6, 12])
                        .style(ui::primary)
                        .on_press(offer.message),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center)
                .into()
            }
            UpdateState::Downloading(release) => {
                text(tf("update-downloading", &[("version", release.version.to_string().into())]))
                    .size(13)
                    .color(ui::pal().muted)
                    .into()
            }
            UpdateState::Failed(e) => text(e).size(13).color(ui::pal().danger).into(),
        };

        let mut check = button(icon_label("⟳", t("settings-check-now")))
            .padding([7, 14])
            .style(ui::secondary);
        if !matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading(_) | UpdateState::Ready(..)
        ) {
            check = check.on_press(ShellMessage::CheckUpdates);
        }

        column![
            setting(
                t("settings-version"),
                tf("settings-current-version", &[("version", update::VERSION.into())]),
                row![check, container(status).width(Length::Fill)]
                    .spacing(12)
                    .align_y(alignment::Vertical::Center),
            ),
            setting(
                t("settings-automatic-check"),
                t("settings-automatic-check-help"),
                checkbox(self.settings.check_updates)
                    .label(t("settings-check-on-start"))
                    .on_toggle(ShellMessage::SetCheckUpdates)
                    .text_size(13),
            ),
        ]
        .spacing(26)
        .into()
    }

    fn page_about(&self) -> Element<'_, ShellMessage> {
        let title = row![
            image(self.logo.clone()).width(80).height(80),
            column![
                text("Wurzelwerk").size(26).font(ui::BOLD),
                text(tf("about-version", &[("version", update::VERSION.into())])).size(14),
                text(t("about-tagline")).size(13).color(ui::pal().muted),
            ]
            .spacing(4),
        ]
        .spacing(18)
        .align_y(alignment::Vertical::Center);

        let settings_file = settings::settings_path();
        let folder = settings::config_dir();

        column![
            title,
            setting(
                t("about-settings-file"),
                t("about-settings-file-help"),
                column![
                    text(settings_file.display().to_string()).size(13),
                    button(text(t("settings-open-folder")).size(13))
                        .padding([6, 12])
                        .style(ui::secondary)
                        .on_press(ShellMessage::OpenPath(folder)),
                ]
                .spacing(8),
            ),
            setting(t("about-licence"), "MIT OR Apache-2.0", Row::new()),
            setting(
                t("about-source-code"),
                update::REPO_URL,
                button(icon_label("↗", t("about-open-github")))
                    .padding([6, 12])
                    .style(ui::secondary)
                    .on_press(ShellMessage::OpenUrl(update::REPO_URL.into())),
            ),
        ]
        .spacing(24)
        .into()
    }
}

/// Settings block: caption, explanation and the control.
fn setting<'a>(
    title: &'a str,
    explain: impl ToString,
    control: impl Into<Element<'a, ShellMessage>>,
) -> Element<'a, ShellMessage> {
    column![
        column![
            section(title),
            text(explain.to_string()).size(12).color(ui::pal().muted)
        ]
        .spacing(4),
        control.into()
    ]
    .spacing(10)
    .into()
}

/// The workspace's own line names, one row each: the name in every language and a remove button.
fn relations_editor(app: &App) -> Element<'_, ShellMessage> {
    if app.data.relations.is_empty() {
        return hint(t("settings-no-relations")).into();
    }
    let header = Row::with_children(Lang::LANGUAGES.map(|lang| {
        text(lang.to_string())
            .size(11)
            .font(ui::SEMIBOLD)
            .color(ui::pal().muted)
            .width(Length::Fill)
            .into()
    }))
    .push(space().width(30))
    .spacing(8);
    let rows = app.data.relations.iter().map(|r| {
        Row::with_children(Lang::LANGUAGES.map(|lang| {
            let (id, code) = (r.id.clone(), lang.code());
            text_input(r.name(), r.names.get(code).map_or("", String::as_str))
                .on_input(move |name| {
                    ShellMessage::App(Message::RenameRelation(id.clone(), code, name))
                })
                .padding([6, 10])
                .size(13)
                .style(ui::input)
                .width(Length::Fill)
                .into()
        }))
        .push(tip(
            button(icon("✕").size(12))
                .padding([6, 8])
                .style(ui::secondary)
                .on_press(ShellMessage::App(Message::DeleteRelation(r.id.clone()))),
            t("settings-remove-relation"),
        ))
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .into()
    });
    Column::with_children(std::iter::once(header.into()).chain(rows))
        .spacing(6)
        .into()
}

/// `[✦ Open example workspace]  <where it lives>`
fn example_row<'a>() -> Element<'a, ShellMessage> {
    row![
        button(icon_label("✦", t("example-open")))
            .padding([7, 14])
            .style(ui::secondary)
            .on_press(ShellMessage::OpenExample),
        text(settings::example_workspace().display().to_string())
            .size(12)
            .color(ui::pal().muted),
    ]
    .spacing(10)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// Clickable recent-workspace row: folder name over the full path.
fn recent_row<'a>(path: &Path) -> Element<'a, ShellMessage> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    button(
        row![
            icon("▤").size(14).color(ui::pal().muted),
            column![
                text(name).size(13).font(ui::SEMIBOLD),
                text(path.display().to_string()).size(11).color(ui::pal().muted),
            ]
            .spacing(1)
            .width(Length::Fill),
            icon("→").size(13).color(ui::pal().subtle),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding([7, 12])
    .style(ui::card)
    .on_press(ShellMessage::OpenWorkspace(path.to_path_buf()))
    .into()
}

/// Opens the database of a workspace folder and boots the board on it.
fn open_app(cfg: Config) -> Result<(App, Task<Message>), String> {
    std::fs::create_dir_all(&cfg.project)
        .map_err(|e| tf(
            "error-create-folder",
            &[
                ("path", cfg.project.display().to_string().into()),
                ("error", e.to_string().into()),
            ],
        ))?;
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
                    "error-open-database",
                    &[
                        ("path", cfg.project.display().to_string().into()),
                        ("error", e.to_string().into()),
                    ],
                ));
            }
        }
    };
    Ok(App::boot(Init { cfg, db, data }))
}
