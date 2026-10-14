//! Wurzelwerk - family / clan board.
//!
//! ```text
//! cargo run --release -- [WORKSPACE_DIR] [--demo 40x250]
//! ```
//! The workspace (a folder with the embedded RedDB file `wurzelwerk.rdb` and the per-person
//! folders) is normally remembered in the global settings (`<config dir>/wurzelwerk/settings.json`).
//! `WORKSPACE_DIR` or env `WURZELWERK_PROJECT` overrides it for one run.

// release builds start without a console window on Windows (debug builds keep it for logs)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod board;
mod camera;
mod demo;
mod files;
mod i18n;
mod layout;
mod model;
mod notes;
mod routing;
mod settings;
mod shell;
mod spatial;
mod store;
mod timeline;
mod ui;
mod update;

use shell::{Cli, Shell};

const ICON: &[u8] = include_bytes!("../assets/icon-256.png");

fn main() -> iced::Result {
    let cli = Cli::parse();

    let mut app = iced::application(move || Shell::boot(cli.clone()), Shell::update, Shell::view)
        .title(Shell::title)
        .subscription(Shell::subscription)
        .theme(Shell::theme)
        .window(iced::window::Settings {
            size: iced::Size::new(1440.0, 900.0),
            icon: iced::window::icon::from_file_data(ICON, None).ok(),
            ..iced::window::Settings::default()
        })
        .antialiasing(true);
    for font in iced_markdown_editor::FONTS {
        app = app.font(font);
    }
    app.run()
}