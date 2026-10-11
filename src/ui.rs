//! Design system: colour palettes (light / dark), fonts and widget styles.
//!
//! All colours come from [`pal`], which follows the appearance chosen with [`set_dark`],
//! so the panels, the board canvas and the markdown editor always match.

use std::sync::atomic::{AtomicBool, Ordering};

use iced::widget::{button, container, pick_list, scrollable, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, font};

/// Font with reliable coverage of the symbols used as icons (bundled by the editor).
pub const ICON: Font = Font::with_name("DejaVu Sans");

pub const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};

pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

/// Corner radii.
pub const R_SM: f32 = 6.0;
pub const R_MD: f32 = 8.0;
pub const R_LG: f32 = 12.0;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Board canvas.
    pub board: Color,
    /// Toolbar and status bar.
    pub chrome: Color,
    /// Side panels and dialogs.
    pub surface: Color,
    /// Cards, inputs and anything lifted above a surface.
    pub raised: Color,
    pub border: Color,
    pub border_strong: Color,
    pub text: Color,
    pub muted: Color,
    pub subtle: Color,
    pub hover: Color,
    pub pressed: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_soft: Color,
    pub on_accent: Color,
    pub danger: Color,
    pub danger_soft: Color,
    pub warning_soft: Color,
    pub success: Color,
    pub shadow: Color,
    // board
    pub grid_dot: Color,
    pub edge: Color,
    pub node: Color,
    pub node_border: Color,
    pub node_shadow: Color,
    pub selection: Color,
    pub note_mark: Color,
    pub file_mark: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color {
        a,
        ..rgb(r, g, b)
    }
}

pub const LIGHT: Palette = Palette {
    dark: false,
    board: rgb(246, 245, 241),
    chrome: rgb(251, 250, 247),
    surface: rgb(255, 255, 255),
    raised: rgb(255, 255, 255),
    border: rgb(229, 226, 218),
    border_strong: rgb(205, 201, 190),
    text: rgb(31, 35, 40),
    muted: rgb(107, 104, 96),
    subtle: rgb(150, 146, 137),
    hover: rgb(243, 241, 236),
    pressed: rgb(234, 231, 223),
    accent: rgb(46, 125, 91),
    accent_hover: rgb(37, 106, 76),
    accent_soft: rgb(229, 241, 234),
    on_accent: rgb(255, 255, 255),
    danger: rgb(190, 54, 46),
    danger_soft: rgb(252, 236, 234),
    warning_soft: rgb(255, 244, 214),
    success: rgb(46, 160, 98),
    shadow: rgba(40, 32, 16, 0.10),
    grid_dot: rgb(219, 215, 205),
    edge: rgb(88, 92, 99),
    node: rgb(255, 255, 255),
    node_border: rgb(214, 210, 200),
    node_shadow: rgba(40, 32, 16, 0.08),
    selection: rgb(46, 125, 91),
    note_mark: rgb(199, 133, 20),
    file_mark: rgb(51, 102, 179),
};

pub const DARK: Palette = Palette {
    dark: true,
    board: rgb(22, 24, 27),
    chrome: rgb(27, 29, 33),
    surface: rgb(31, 34, 38),
    raised: rgb(39, 43, 48),
    border: rgb(52, 56, 62),
    border_strong: rgb(72, 77, 85),
    text: rgb(232, 230, 225),
    muted: rgb(160, 156, 148),
    subtle: rgb(118, 115, 108),
    hover: rgb(45, 49, 55),
    pressed: rgb(54, 59, 66),
    accent: rgb(84, 184, 138),
    accent_hover: rgb(104, 201, 156),
    accent_soft: rgb(33, 58, 47),
    on_accent: rgb(14, 30, 22),
    danger: rgb(236, 107, 96),
    danger_soft: rgb(66, 36, 34),
    warning_soft: rgb(70, 58, 28),
    success: rgb(84, 184, 138),
    shadow: rgba(0, 0, 0, 0.40),
    grid_dot: rgb(48, 52, 58),
    edge: rgb(150, 156, 165),
    node: rgb(39, 43, 48),
    node_border: rgb(66, 71, 79),
    node_shadow: rgba(0, 0, 0, 0.35),
    selection: rgb(84, 184, 138),
    note_mark: rgb(232, 172, 64),
    file_mark: rgb(110, 160, 235),
};

static DARK_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_dark(dark: bool) {
    DARK_MODE.store(dark, Ordering::Relaxed);
}

/// The active palette.
pub fn pal() -> &'static Palette {
    if DARK_MODE.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

/// iced theme built from the active palette (inputs, pick lists, checkboxes, scrollbars).
pub fn theme() -> Theme {
    let p = pal();
    Theme::custom(
        if p.dark {
            "Wurzelwerk Dark"
        } else {
            "Wurzelwerk Light"
        },
        iced::theme::Palette {
            background: p.surface,
            text: p.text,
            primary: p.accent,
            success: p.success,
            warning: rgb(214, 158, 46),
            danger: p.danger,
        },
    )
}

/// `color` at `alpha`.
pub fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

fn hovered(status: button::Status) -> bool {
    matches!(status, button::Status::Hovered | button::Status::Pressed)
}

fn border(radius: f32, width: f32, color: Color) -> Border {
    Border {
        radius: radius.into(),
        width,
        color,
    }
}

fn soft_shadow(blur: f32, y: f32) -> Shadow {
    Shadow {
        color: pal().shadow,
        offset: Vector::new(0.0, y),
        blur_radius: blur,
    }
}

// ---------------------------------------------------------------------------
// containers
// ---------------------------------------------------------------------------

pub fn chrome(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(pal().chrome)),
        ..container::Style::default()
    }
}

pub fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(pal().surface)),
        ..container::Style::default()
    }
}

pub fn divider(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(pal().border)),
        ..container::Style::default()
    }
}

/// Grouped content inside a panel.
pub fn section_card(_theme: &Theme) -> container::Style {
    let p = pal();
    container::Style {
        background: Some(Background::Color(p.raised)),
        border: border(R_LG, 1.0, p.border),
        ..container::Style::default()
    }
}

/// Floating element above the board (zoom controls, hints).
pub fn floating(_theme: &Theme) -> container::Style {
    let p = pal();
    container::Style {
        background: Some(Background::Color(p.raised)),
        border: border(R_MD + 2.0, 1.0, p.border),
        shadow: soft_shadow(14.0, 4.0),
        ..container::Style::default()
    }
}

/// Short message shown at the bottom of the board.
pub fn toast(_theme: &Theme) -> container::Style {
    let p = pal();
    let (bg, fg) = if p.dark {
        (rgb(232, 230, 225), rgb(31, 34, 38))
    } else {
        (rgb(38, 42, 47), rgb(246, 245, 241))
    };
    container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(fg),
        border: border(R_MD + 2.0, 0.0, Color::TRANSPARENT),
        shadow: soft_shadow(18.0, 6.0),
        ..container::Style::default()
    }
}

/// Text colour used on [`toast`].
pub fn toast_text() -> Color {
    if pal().dark {
        rgb(31, 34, 38)
    } else {
        rgb(246, 245, 241)
    }
}

/// Tooltip bubble.
pub fn tooltip(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(toast_text()),
        ..toast(_theme)
    }
}

pub fn swatch(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(color)),
        border: border(100.0, 0.0, Color::TRANSPARENT),
        ..container::Style::default()
    }
}

/// Round avatar with initials, tinted with `color`.
pub fn avatar(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| {
        let p = pal();
        container::Style {
            background: Some(Background::Color(alpha(color, if p.dark { 0.30 } else { 0.16 }))),
            text_color: Some(if p.dark { p.text } else { mix(color, Color::BLACK, 0.35) }),
            border: border(100.0, 0.0, Color::TRANSPARENT),
            ..container::Style::default()
        }
    }
}

/// Small count pill.
pub fn badge(_theme: &Theme) -> container::Style {
    let p = pal();
    container::Style {
        background: Some(Background::Color(p.hover)),
        text_color: Some(p.muted),
        border: border(100.0, 0.0, Color::TRANSPARENT),
        ..container::Style::default()
    }
}

/// Keyboard key in shortcut lists.
pub fn kbd(_theme: &Theme) -> container::Style {
    let p = pal();
    container::Style {
        background: Some(Background::Color(p.chrome)),
        text_color: Some(p.muted),
        border: border(4.0, 1.0, p.border_strong),
        ..container::Style::default()
    }
}

/// Strip announcing an available update.
pub fn banner(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(pal().warning_soft)),
        ..container::Style::default()
    }
}

/// Dimmed layer behind a modal dialog.
pub fn backdrop(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: if pal().dark { 0.55 } else { 0.32 },
            ..Color::BLACK
        })),
        ..container::Style::default()
    }
}

pub fn dialog(_theme: &Theme) -> container::Style {
    let p = pal();
    container::Style {
        background: Some(Background::Color(p.surface)),
        border: border(R_LG + 2.0, 1.0, p.border),
        shadow: Shadow {
            color: alpha(Color::BLACK, if p.dark { 0.5 } else { 0.18 }),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 40.0,
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------------------
// buttons
// ---------------------------------------------------------------------------

fn flat(bg: Option<Color>, fg: Color, b: Border) -> button::Style {
    button::Style {
        background: bg.map(Background::Color),
        text_color: fg,
        border: b,
        ..button::Style::default()
    }
}

/// Segment of the tool switcher.
pub fn segment(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let p = pal();
        if active {
            button::Style {
                shadow: soft_shadow(4.0, 1.0),
                ..flat(Some(p.raised), p.accent, border(R_SM, 1.0, p.border))
            }
        } else {
            flat(
                hovered(status).then_some(p.pressed),
                if hovered(status) { p.text } else { p.muted },
                border(R_SM, 0.0, Color::TRANSPARENT),
            )
        }
    }
}

/// Tool switcher background.
pub fn segments(_theme: &Theme) -> container::Style {
    let p = pal();
    container::Style {
        background: Some(Background::Color(p.hover)),
        border: border(R_MD, 1.0, p.border),
        ..container::Style::default()
    }
}

/// Filled call to action.
pub fn primary(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    let bg = match status {
        button::Status::Disabled => alpha(p.accent, 0.5),
        s if hovered(s) => p.accent_hover,
        _ => p.accent,
    };
    button::Style {
        shadow: soft_shadow(3.0, 1.0),
        ..flat(Some(bg), p.on_accent, border(R_MD, 0.0, Color::TRANSPARENT))
    }
}

/// Outlined secondary action.
pub fn secondary(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    let fg = if status == button::Status::Disabled {
        p.subtle
    } else {
        p.text
    };
    flat(
        Some(if hovered(status) { p.hover } else { p.raised }),
        fg,
        border(R_MD, 1.0, if hovered(status) { p.border_strong } else { p.border }),
    )
}

/// Borderless icon button (toolbar, list row actions).
pub fn icon(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    flat(
        hovered(status).then_some(p.hover),
        if hovered(status) { p.text } else { p.muted },
        border(R_SM, 0.0, Color::TRANSPARENT),
    )
}

pub fn list_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let p = pal();
        let bg = if active {
            Some(p.accent_soft)
        } else if hovered(status) {
            Some(p.hover)
        } else {
            None
        };
        flat(bg, p.text, border(R_SM, 0.0, Color::TRANSPARENT))
    }
}

/// Section header that collapses / expands.
pub fn header(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    flat(
        hovered(status).then_some(p.hover),
        p.muted,
        border(R_SM, 0.0, Color::TRANSPARENT),
    )
}

pub fn chip(color: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let p = pal();
        let a = if hovered(status) { 0.30 } else { 0.18 };
        flat(
            Some(alpha(color, if p.dark { a + 0.1 } else { a })),
            if p.dark { p.text } else { mix(color, Color::BLACK, 0.4) },
            border(100.0, 1.0, alpha(color, 0.45)),
        )
    }
}

pub fn card(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    flat(
        Some(if hovered(status) { p.hover } else { p.raised }),
        p.text,
        border(R_MD, 1.0, if hovered(status) { p.border_strong } else { p.border }),
    )
}

pub fn ghost(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    flat(
        hovered(status).then_some(p.hover),
        if hovered(status) { p.text } else { p.muted },
        border(R_SM, 0.0, Color::TRANSPARENT),
    )
}

/// "+ New …" rows at the end of a list.
pub fn add_row(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    flat(
        hovered(status).then_some(p.accent_soft),
        p.accent,
        border(R_SM, 0.0, Color::TRANSPARENT),
    )
}

pub fn drop_zone(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    flat(
        Some(if hovered(status) { p.accent_soft } else { p.chrome }),
        if hovered(status) { p.accent } else { p.muted },
        border(
            R_MD + 2.0,
            1.0,
            if hovered(status) { p.accent } else { p.border_strong },
        ),
    )
}

pub fn danger(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    flat(
        Some(if hovered(status) { p.danger_soft } else { p.raised }),
        p.danger,
        border(R_MD, 1.0, alpha(p.danger, 0.45)),
    )
}

/// Filled destructive action (confirmation dialog).
pub fn danger_solid(_theme: &Theme, status: button::Status) -> button::Style {
    let p = pal();
    let bg = if hovered(status) {
        mix(p.danger, Color::BLACK, 0.12)
    } else {
        p.danger
    };
    flat(Some(bg), Color::WHITE, border(R_MD, 0.0, Color::TRANSPARENT))
}

// ---------------------------------------------------------------------------
// inputs
// ---------------------------------------------------------------------------

fn input_with(radius: f32) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_theme, status| {
        let p = pal();
        let edge = match status {
            text_input::Status::Focused { .. } => p.accent,
            text_input::Status::Hovered => p.border_strong,
            _ => p.border,
        };
        text_input::Style {
            background: Background::Color(if matches!(status, text_input::Status::Disabled) {
                p.hover
            } else {
                p.raised
            }),
            border: border(radius, if matches!(status, text_input::Status::Focused { .. }) { 1.5 } else { 1.0 }, edge),
            icon: p.muted,
            placeholder: p.subtle,
            value: p.text,
            selection: alpha(p.accent, 0.28),
        }
    }
}

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    input_with(R_MD)(theme, status)
}

/// Pill-shaped search field.
pub fn search(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = input_with(100.0)(theme, status);
    if !matches!(status, text_input::Status::Focused { .. }) {
        style.background = Background::Color(pal().hover);
        style.border.color = Color::TRANSPARENT;
    }
    style
}

/// Large borderless title field (person / clan name).
pub fn title_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = input_with(R_MD)(theme, status);
    if matches!(status, text_input::Status::Active) {
        style.border.color = Color::TRANSPARENT;
        style.background = Background::Color(Color::TRANSPARENT);
    }
    style
}

pub fn pick(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let p = pal();
    let hot = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. });
    pick_list::Style {
        text_color: p.text,
        placeholder_color: p.muted,
        handle_color: p.muted,
        background: Background::Color(if hot { p.hover } else { p.raised }),
        border: border(R_MD, 1.0, if hot { p.border_strong } else { p.border }),
    }
}

/// Dashed-looking "+ clan" picker shown among the clan chips.
pub fn pick_chip(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let p = pal();
    let hot = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. });
    pick_list::Style {
        text_color: p.accent,
        placeholder_color: p.accent,
        handle_color: p.accent,
        background: Background::Color(if hot { p.accent_soft } else { Color::TRANSPARENT }),
        border: border(100.0, 1.0, alpha(p.accent, 0.5)),
    }
}

/// Thin, unobtrusive scrollbars.
pub fn scroll(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let p = pal();
    let mut style = scrollable::default(theme, status);
    let (v_hot, h_hot) = match status {
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered: v,
            is_horizontal_scrollbar_hovered: h,
            ..
        } => (v, h),
        scrollable::Status::Dragged {
            is_vertical_scrollbar_dragged: v,
            is_horizontal_scrollbar_dragged: h,
            ..
        } => (v, h),
        scrollable::Status::Active { .. } => (false, false),
    };
    for (rail, hot) in [
        (&mut style.vertical_rail, v_hot),
        (&mut style.horizontal_rail, h_hot),
    ] {
        rail.background = None;
        rail.border = Border::default();
        rail.scroller.border = border(100.0, 0.0, Color::TRANSPARENT);
        rail.scroller.background =
            Background::Color(if hot { p.border_strong } else { p.border });
    }
    style
}

/// Linear blend from `a` to `b`.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// Editor look matching the app: board-coloured surround, surface-coloured page.
pub fn editor_style() -> iced_markdown_editor::EditorStyle {
    use iced_markdown_editor::{EditorStyle, Palette as EditorPalette};

    let p = pal();
    let style = if p.dark {
        EditorStyle::dark()
    } else {
        EditorStyle::light()
    };
    style.with_palette(EditorPalette {
        background: p.board,
        page: p.surface,
        page_border: p.border,
        page_shadow: Color::TRANSPARENT,
        ink: p.text,
        heading: p.text,
        quote: p.muted,
        quote_bar: p.border_strong,
        code: p.text,
        code_background: p.chrome,
        inline_code_background: p.hover,
        link: p.file_mark,
        marker: p.text,
        rule: p.border,
        selection: alpha(p.accent, 0.28),
        caret: p.accent,
        toolbar_background: p.chrome,
        toolbar_border: p.border,
        toolbar_text: p.text,
        toolbar_hover: p.hover,
        toolbar_active: p.accent_soft,
        accent: p.accent,
        muted: p.muted,
    })
}
