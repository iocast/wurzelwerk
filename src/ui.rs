//! Colours and widget styles (light "Wurzelwerk" look).

use iced::widget::{button, container};
use iced::{Background, Border, Color, Theme};

pub const BOARD_BG: Color = Color {
    r: 0.969,
    g: 0.961,
    b: 0.941,
    a: 1.0,
};
pub const PANEL_BG: Color = Color {
    r: 0.949,
    g: 0.941,
    b: 0.918,
    a: 1.0,
};
pub const BORDER: Color = Color {
    r: 0.820,
    g: 0.800,
    b: 0.760,
    a: 1.0,
};
pub const TEXT: Color = Color {
    r: 0.130,
    g: 0.130,
    b: 0.130,
    a: 1.0,
};
pub const MUTED: Color = Color {
    r: 0.450,
    g: 0.440,
    b: 0.420,
    a: 1.0,
};
pub const DARK: Color = Color {
    r: 0.140,
    g: 0.140,
    b: 0.140,
    a: 1.0,
};
pub const EDGE: Color = Color {
    r: 0.250,
    g: 0.250,
    b: 0.250,
    a: 1.0,
};
pub const NODE_BORDER: Color = Color {
    r: 0.300,
    g: 0.300,
    b: 0.300,
    a: 1.0,
};
pub const GRID_DOT: Color = Color {
    r: 0.820,
    g: 0.800,
    b: 0.750,
    a: 1.0,
};
pub const HOVER: Color = Color {
    r: 0.925,
    g: 0.914,
    b: 0.886,
    a: 1.0,
};
pub const NOTE_MARK: Color = Color {
    r: 0.780,
    g: 0.520,
    b: 0.080,
    a: 1.0,
};
pub const FILE_MARK: Color = Color {
    r: 0.200,
    g: 0.400,
    b: 0.700,
    a: 1.0,
};
pub const DANGER: Color = Color {
    r: 0.690,
    g: 0.200,
    b: 0.180,
    a: 1.0,
};

pub fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_BG)),
        ..container::Style::default()
    }
}

pub fn divider(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BORDER)),
        ..container::Style::default()
    }
}

pub fn swatch(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: 2.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// Strip announcing an available update.
pub fn banner(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(255, 243, 205))),
        ..container::Style::default()
    }
}

/// Dimmed layer behind a modal dialog.
pub fn backdrop(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.35,
            ..Color::BLACK
        })),
        ..container::Style::default()
    }
}

pub fn dialog(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::WHITE)),
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: BORDER,
        },
        ..container::Style::default()
    }
}
fn hovered(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

pub fn tool(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if active {
            button::Style {
                background: Some(Background::Color(DARK)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 6.0.into(),
                    width: 1.0,
                    color: DARK,
                },
                ..button::Style::default()
            }
        } else {
            button::Style {
                background: Some(Background::Color(if hovered(status) {
                    HOVER
                } else {
                    Color::WHITE
                })),
                text_color: TEXT,
                border: Border {
                    radius: 6.0.into(),
                    width: 1.0,
                    color: BORDER,
                },
                ..button::Style::default()
            }
        }
    }
}

pub fn list_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: Some(Background::Color(if active {
            Color::WHITE
        } else if hovered(status) {
            HOVER
        } else {
            Color::TRANSPARENT
        })),
        text_color: TEXT,
        border: Border {
            radius: 5.0.into(),
            width: if active { 1.0 } else { 0.0 },
            color: BORDER,
        },
        ..button::Style::default()
    }
}

pub fn chip(color: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: Some(Background::Color(if hovered(status) {
            Color { a: 0.8, ..color }
        } else {
            color
        })),
        text_color: Color::WHITE,
        border: Border {
            radius: 11.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

pub fn card(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(if hovered(status) {
            HOVER
        } else {
            Color::WHITE
        })),
        text_color: TEXT,
        border: Border {
            radius: 5.0.into(),
            width: 1.0,
            color: BORDER,
        },
        ..button::Style::default()
    }
}

pub fn link(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: if hovered(status) { DARK } else { TEXT },
        ..button::Style::default()
    }
}

pub fn ghost(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: if hovered(status) {
            Some(Background::Color(HOVER))
        } else {
            None
        },
        text_color: MUTED,
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

pub fn drop_zone(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(if hovered(status) {
            HOVER
        } else {
            PANEL_BG
        })),
        text_color: MUTED,
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: BORDER,
        },
        ..button::Style::default()
    }
}

pub fn danger(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(if hovered(status) {
            Color { a: 0.12, ..DANGER }
        } else {
            Color::TRANSPARENT
        })),
        text_color: DANGER,
        border: Border {
            radius: 5.0.into(),
            width: 1.0,
            color: Color { a: 0.5, ..DANGER },
        },
        ..button::Style::default()
    }
}

/// Editor look matching the app: board-coloured page, panel-coloured toolbar.
pub fn editor_style() -> iced_markdown_editor::EditorStyle {
    use iced_markdown_editor::{EditorStyle, Palette};

    let rgb = |r: u8, g: u8, b: u8| Color::from_rgb8(r, g, b);
    let base = Palette::light();
    EditorStyle::light().with_palette(Palette {
        background: BOARD_BG,
        page: BOARD_BG,
        page_border: BORDER,
        page_shadow: Color::TRANSPARENT,
        ink: TEXT,
        heading: TEXT,
        quote: MUTED,
        quote_bar: BORDER,
        code: TEXT,
        code_background: PANEL_BG,
        inline_code_background: HOVER,
        link: FILE_MARK,
        marker: TEXT,
        rule: BORDER,
        caret: TEXT,
        toolbar_background: PANEL_BG,
        toolbar_border: BORDER,
        toolbar_text: TEXT,
        toolbar_hover: HOVER,
        toolbar_active: rgb(221, 214, 196),
        accent: DARK,
        muted: MUTED,
        ..base
    })
}
