use std::sync::{Arc, LazyLock};

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke};

// YouTube Dark Palette
pub const BG: Color32 = Color32::from_rgb(15, 15, 15);
pub const SURFACE: Color32 = Color32::from_rgb(24, 24, 24);
pub const CHIP: Color32 = Color32::from_rgb(39, 39, 39);
pub const CHIP_HOVER: Color32 = Color32::from_rgb(56, 56, 56);
pub const BORDER: Color32 = Color32::from_rgb(48, 48, 48);
pub const TEXT: Color32 = Color32::from_rgb(241, 241, 241);
pub const TEXT_DIM: Color32 = Color32::from_rgb(170, 170, 170);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(113, 113, 113);
pub const YT_RED: Color32 = Color32::from_rgb(255, 0, 51);
pub const YT_BLUE: Color32 = Color32::from_rgb(62, 166, 255);

static MEDIUM: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("medium".into()));
static SEMIBOLD: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("semibold".into()));
static BOLD: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("bold".into()));
static ICONS: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("icons".into()));

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, MEDIUM.clone())
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, SEMIBOLD.clone())
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, BOLD.clone())
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, ICONS.clone())
}

/// Lucide vector icon codepoints (matching assets/fonts/lucide.ttf).
pub mod icon {
    pub const HOUSE: char = '\u{e0f5}';
    pub const COMPASS: char = '\u{e09b}';
    pub const LIBRARY: char = '\u{e100}';
    pub const SEARCH: char = '\u{e151}';
    pub const CHEVRON_LEFT: char = '\u{e06e}';
    pub const CHEVRON_RIGHT: char = '\u{e06f}';
    pub const CHEVRON_UP: char = '\u{e070}';
    pub const CHEVRON_DOWN: char = '\u{e06d}';
    pub const PLAY: char = '\u{e13c}';
    pub const PAUSE: char = '\u{e12e}';
    pub const SKIP_BACK: char = '\u{e15f}';
    pub const SKIP_FORWARD: char = '\u{e160}';
    pub const VOLUME_2: char = '\u{e1ab}';
    pub const VOLUME_1: char = '\u{e1aa}';
    pub const VOLUME_X: char = '\u{e1ac}';
    pub const X: char = '\u{e1b2}';
    pub const HISTORY: char = '\u{e1f5}';
    pub const USER: char = '\u{e19f}';
    pub const CIRCLE_USER: char = '\u{e461}';
    pub const SETTINGS: char = '\u{e154}';
    pub const LOG_OUT: char = '\u{e10e}';
    pub const CHECK: char = '\u{e06c}';
    pub const EXPAND: char = '\u{e21a}';
    pub const SHRINK: char = '\u{e220}';
    pub const MAXIMIZE_2: char = '\u{e113}';
    pub const COPY: char = '\u{e09e}';
    pub const LINK: char = '\u{e102}';
    pub const DOWNLOAD: char = '\u{e0b2}';
    pub const LIST_PLUS: char = '\u{e23f}';
    pub const LIST_MUSIC: char = '\u{e2e0}';
    pub const RADIO: char = '\u{e142}';
    pub const SPARKLES: char = '\u{e412}';
    pub const ELLIPSIS_VERTICAL: char = '\u{e0b7}';
    pub const HEART: char = '\u{e0f2}';
    pub const THUMBS_DOWN: char = '\u{e189}';
    pub const USERS: char = '\u{e1a4}';
    pub const HEADPHONES: char = '\u{e0f1}';
    pub const TRASH_2: char = '\u{e18e}';
    pub const FOLDER_OPEN: char = '\u{e247}';
}

const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
const INTER_EXTRABOLD: &[u8] = include_bytes!("../assets/fonts/Inter-ExtraBold.ttf");
const LUCIDE: &[u8] = include_bytes!("../assets/fonts/lucide.ttf");

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter_regular".into(),
        Arc::new(FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "inter_medium".into(),
        Arc::new(FontData::from_static(INTER_MEDIUM)),
    );
    fonts.font_data.insert(
        "inter_semibold".into(),
        Arc::new(FontData::from_static(INTER_SEMIBOLD)),
    );
    fonts.font_data.insert(
        "inter_bold".into(),
        Arc::new(FontData::from_static(INTER_EXTRABOLD)),
    );
    fonts
        .font_data
        .insert("lucide".into(), Arc::new(FontData::from_static(LUCIDE)));

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter_regular".into());
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("lucide".into());
    fonts
        .families
        .insert(MEDIUM.clone(), vec!["inter_medium".into(), "inter_regular".into(), "lucide".into()]);
    fonts
        .families
        .insert(SEMIBOLD.clone(), vec!["inter_semibold".into(), "inter_regular".into(), "lucide".into()]);
    fonts
        .families
        .insert(BOLD.clone(), vec!["inter_bold".into(), "inter_semibold".into(), "lucide".into()]);
    fonts
        .families
        .insert(ICONS.clone(), vec!["lucide".into()]);

    ctx.set_fonts(fonts);

    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = Color32::from_rgb(18, 18, 18);
    style.visuals.faint_bg_color = Color32::from_rgb(28, 28, 28);
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = YT_BLUE.gamma_multiply(0.3);
    style.visuals.selection.stroke = Stroke::new(1.0, YT_BLUE);

    style.visuals.widgets.noninteractive.bg_fill = SURFACE;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    style.visuals.widgets.inactive.bg_fill = CHIP;
    style.visuals.widgets.inactive.weak_bg_fill = CHIP;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

    style.visuals.widgets.hovered.bg_fill = CHIP_HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = CHIP_HOVER;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(8);

    style.visuals.widgets.active.bg_fill = Color32::from_rgb(72, 72, 72);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(72, 72, 72);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(8);

    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.scroll.bar_width = 7.0;
    style.spacing.scroll.floating = true;

    ctx.set_global_style(style);
}

/// Draw a YouTube-style rounded red play badge at `center`.
pub fn draw_youtube_logo(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 5.0, YT_RED);
    let c = rect.center();
    let s = rect.height() * 0.28;
    let pts = vec![
        egui::pos2(c.x - s * 0.65, c.y - s),
        egui::pos2(c.x + s * 1.1, c.y),
        egui::pos2(c.x - s * 0.65, c.y + s),
    ];
    painter.add(egui::Shape::convex_polygon(pts, Color32::WHITE, Stroke::NONE));
}

pub fn draw_icon(ui: &mut egui::Ui, ch: char, size: f32, color: Color32) {
    ui.label(
        egui::RichText::new(ch.to_string())
            .font(icon_font(size))
            .color(color),
    );
}

pub fn pill_frame(fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::symmetric(12, 6))
}
