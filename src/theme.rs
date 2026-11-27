use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke};

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

static BACKDROP_ACRYLIC: AtomicBool = AtomicBool::new(false);
static BACKDROP_AMBIENT: AtomicBool = AtomicBool::new(false);

pub fn set_backdrop_flags(acrylic: bool, ambient: bool) {
    BACKDROP_ACRYLIC.store(acrylic, Ordering::Relaxed);
    BACKDROP_AMBIENT.store(ambient, Ordering::Relaxed);
}

pub fn is_translucent() -> bool {
    BACKDROP_ACRYLIC.load(Ordering::Relaxed) || BACKDROP_AMBIENT.load(Ordering::Relaxed)
}

pub fn panel_bg() -> Color32 {
    if is_translucent() {
        Color32::TRANSPARENT
    } else {
        BG
    }
}

pub fn chrome_bg() -> Color32 {
    if BACKDROP_ACRYLIC.load(Ordering::Relaxed) {
        Color32::from_rgba_unmultiplied(16, 16, 20, 115)
    } else if BACKDROP_AMBIENT.load(Ordering::Relaxed) {
        Color32::from_rgba_unmultiplied(15, 15, 19, 130)
    } else {
        BG
    }
}

pub fn card_fill() -> Color32 {
    if is_translucent() {
        Color32::from_rgba_unmultiplied(255, 255, 255, 14)
    } else {
        SURFACE
    }
}

pub fn card_stroke() -> Color32 {
    if is_translucent() {
        Color32::from_rgba_unmultiplied(255, 255, 255, 26)
    } else {
        BORDER
    }
}

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

/// Draws a single Lucide icon glyph centered by its actual inked bounding box (`mesh_bounds`)
/// rather than typographic advance metrics, keeping glyphs dead-center inside circular buttons.
pub fn draw_icon_centered(
    painter: &egui::Painter,
    center: egui::Pos2,
    ico: char,
    size: f32,
    color: Color32,
) {
    let galley = painter.layout_no_wrap(ico.to_string(), icon_font(size), color);
    let bounds = if galley.mesh_bounds.is_positive() {
        galley.mesh_bounds
    } else {
        galley.rect
    };
    let pos = center - bounds.center().to_vec2();
    painter.galley(pos, galley, color);
}

/// Paints a network image without egui's oversized default spinner, showing a subtle
/// YouTube-style skeleton shimmer while pending.
pub fn paint_image_rounded(ui: &egui::Ui, url: &str, rect: egui::Rect, radius: u8) {
    paint_image_with_fit(ui, url, rect, radius, false);
}

/// Center-crops (`cover` fit) and paints a rounded network image so portrait cards
/// fill cleanly even when YouTube supplies 4:3 or 16:9 side-barred thumbnails.
pub fn paint_image_cover_rounded(ui: &egui::Ui, url: &str, rect: egui::Rect, radius: u8) {
    paint_image_with_fit(ui, url, rect, radius, true);
}

fn paint_image_with_fit(ui: &egui::Ui, url: &str, rect: egui::Rect, radius: u8, cover: bool) {
    let probe = egui::Image::new(url)
        .show_loading_spinner(false)
        .corner_radius(CornerRadius::same(radius))
        .fit_to_exact_size(rect.size());
    let poll = probe.load_for_size(ui.ctx(), rect.size());
    let tex_size = match &poll {
        Ok(egui::load::TexturePoll::Ready { texture }) => Some(texture.size),
        _ => None,
    };
    if tex_size.is_none() {
        let t = ui.input(|i| i.time) as f32;
        let pulse = 0.5 + 0.5 * (t * 3.2).sin();
        let shade = (32.0 + 11.0 * pulse).round() as u8;
        ui.painter().rect_filled(
            rect,
            radius as f32,
            Color32::from_rgb(shade, shade, shade + 4),
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
    let mut img = probe;
    if cover {
        if let Some(ts) = tex_size.filter(|s| s.x > 1.0 && s.y > 1.0) {
            let src_aspect = ts.x / ts.y;
            let dst_aspect = (rect.width() / rect.height().max(1.0)).max(0.05);
            if src_aspect > dst_aspect * 1.02 {
                let u_span = (dst_aspect / src_aspect).clamp(0.08, 1.0);
                let u0 = (1.0 - u_span) * 0.5;
                img = img.uv(egui::Rect::from_min_max(
                    egui::pos2(u0, 0.0),
                    egui::pos2(u0 + u_span, 1.0),
                ));
            } else if src_aspect < dst_aspect * 0.98 {
                let v_span = (src_aspect / dst_aspect).clamp(0.08, 1.0);
                let v0 = (1.0 - v_span) * 0.5;
                img = img.uv(egui::Rect::from_min_max(
                    egui::pos2(0.0, v0),
                    egui::pos2(1.0, v0 + v_span),
                ));
            }
        }
    }
    img.paint_at(ui, rect);
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

pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 * inv + b.r() as f32 * t).round() as u8,
        (a.g() as f32 * inv + b.g() as f32 * t).round() as u8,
        (a.b() as f32 * inv + b.b() as f32 * t).round() as u8,
        (a.a() as f32 * inv + b.a() as f32 * t).round() as u8,
    )
}

/// Crisp YouTube-style thumbs-up vector icon centered at `c`.
pub fn draw_thumbs_up(
    painter: &egui::Painter,
    c: egui::Pos2,
    s: f32,
    color: Color32,
    filled: bool,
) {
    let stroke = Stroke::new((s * 0.12).max(1.3), color);
    // Wrist cuff on the left
    let cuff = egui::Rect::from_min_max(
        egui::pos2(c.x - s * 0.44, c.y - s * 0.08),
        egui::pos2(c.x - s * 0.24, c.y + s * 0.36),
    );
    // Thumb + fist body
    let body = vec![
        egui::pos2(c.x - s * 0.22, c.y - s * 0.06),
        egui::pos2(c.x - s * 0.04, c.y - s * 0.40),
        egui::pos2(c.x + s * 0.08, c.y - s * 0.40),
        egui::pos2(c.x + s * 0.06, c.y - s * 0.14),
        egui::pos2(c.x + s * 0.36, c.y - s * 0.14),
        egui::pos2(c.x + s * 0.42, c.y - s * 0.04),
        egui::pos2(c.x + s * 0.34, c.y + s * 0.34),
        egui::pos2(c.x - s * 0.22, c.y + s * 0.34),
    ];
    if filled {
        painter.rect_filled(cuff, 1.5, color);
        painter.add(egui::Shape::convex_polygon(body, color, Stroke::NONE));
    } else {
        painter.rect_stroke(cuff, 1.5, stroke, egui::StrokeKind::Middle);
        painter.add(egui::Shape::closed_line(body, stroke));
    }
}

/// Crisp YouTube-style notification bell vector icon centered at `c`.
pub fn draw_bell(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new((s * 0.11).max(1.3), color);
    let dome = vec![
        egui::pos2(c.x - s * 0.30, c.y + s * 0.16),
        egui::pos2(c.x - s * 0.24, c.y - s * 0.18),
        egui::pos2(c.x - s * 0.08, c.y - s * 0.34),
        egui::pos2(c.x + s * 0.08, c.y - s * 0.34),
        egui::pos2(c.x + s * 0.24, c.y - s * 0.18),
        egui::pos2(c.x + s * 0.30, c.y + s * 0.16),
        egui::pos2(c.x + s * 0.38, c.y + s * 0.24),
        egui::pos2(c.x - s * 0.38, c.y + s * 0.24),
    ];
    painter.add(egui::Shape::closed_line(dome, stroke));
    painter.circle_filled(egui::pos2(c.x, c.y + s * 0.34), (s * 0.09).max(1.3), color);
}

/// Crisp YouTube-style curved share arrow centered at `c`.
pub fn draw_share_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new((s * 0.12).max(1.4), color);
    let pts = vec![
        egui::pos2(c.x - s * 0.36, c.y + s * 0.32),
        egui::pos2(c.x - s * 0.28, c.y + s * 0.02),
        egui::pos2(c.x - s * 0.04, c.y - s * 0.10),
        egui::pos2(c.x + s * 0.16, c.y - s * 0.10),
    ];
    painter.add(egui::Shape::line(pts, stroke));
    let head = vec![
        egui::pos2(c.x + s * 0.14, c.y - s * 0.30),
        egui::pos2(c.x + s * 0.40, c.y - s * 0.06),
        egui::pos2(c.x + s * 0.14, c.y + s * 0.18),
    ];
    painter.add(egui::Shape::convex_polygon(head, color, Stroke::NONE));
}

/// Crisp YouTube-style comment speech bubble centered at `c`.
pub fn draw_comment_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new((s * 0.11).max(1.4), color);
    let bubble = vec![
        egui::pos2(c.x - s * 0.40, c.y - s * 0.34),
        egui::pos2(c.x + s * 0.40, c.y - s * 0.34),
        egui::pos2(c.x + s * 0.40, c.y + s * 0.18),
        egui::pos2(c.x - s * 0.08, c.y + s * 0.18),
        egui::pos2(c.x - s * 0.36, c.y + s * 0.40),
        egui::pos2(c.x - s * 0.30, c.y + s * 0.18),
        egui::pos2(c.x - s * 0.40, c.y + s * 0.18),
    ];
    painter.add(egui::Shape::closed_line(bubble, stroke));
    painter.line_segment(
        [
            egui::pos2(c.x - s * 0.22, c.y - s * 0.14),
            egui::pos2(c.x + s * 0.22, c.y - s * 0.14),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(c.x - s * 0.22, c.y + s * 0.01),
            egui::pos2(c.x + s * 0.10, c.y + s * 0.01),
        ],
        stroke,
    );
}

/// YouTube-style Next track (`|>|`) icon centered at `c`.
pub fn draw_next_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let tri = vec![
        egui::pos2(c.x - s * 0.34, c.y - s * 0.34),
        egui::pos2(c.x + s * 0.14, c.y),
        egui::pos2(c.x - s * 0.34, c.y + s * 0.34),
    ];
    painter.add(egui::Shape::convex_polygon(tri, color, Stroke::NONE));
    let bar = egui::Rect::from_min_max(
        egui::pos2(c.x + s * 0.20, c.y - s * 0.33),
        egui::pos2(c.x + s * 0.35, c.y + s * 0.33),
    );
    painter.rect_filled(bar, 1.0, color);
}

/// Crisp Shuffle icon centered at `c`.
pub fn draw_shuffle_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new((s * 0.12).max(1.3), color);
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(c.x - s * 0.38, c.y - s * 0.24),
            egui::pos2(c.x - s * 0.10, c.y - s * 0.24),
            egui::pos2(c.x + s * 0.14, c.y + s * 0.24),
            egui::pos2(c.x + s * 0.36, c.y + s * 0.24),
        ],
        stroke,
    ));
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(c.x - s * 0.38, c.y + s * 0.24),
            egui::pos2(c.x - s * 0.10, c.y + s * 0.24),
            egui::pos2(c.x + s * 0.14, c.y - s * 0.24),
            egui::pos2(c.x + s * 0.36, c.y - s * 0.24),
        ],
        stroke,
    ));
    for dy in [-s * 0.24, s * 0.24] {
        let head = vec![
            egui::pos2(c.x + s * 0.26, c.y + dy - s * 0.14),
            egui::pos2(c.x + s * 0.44, c.y + dy),
            egui::pos2(c.x + s * 0.26, c.y + dy + s * 0.14),
        ];
        painter.add(egui::Shape::convex_polygon(head, color, Stroke::NONE));
    }
}

/// Crisp smiley face icon centered at `c` for the emote picker button.
pub fn draw_smile_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let r = s * 0.42;
    let stroke = Stroke::new((s * 0.10).max(1.2), color);
    painter.circle_stroke(c, r, stroke);
    painter.circle_filled(
        egui::pos2(c.x - r * 0.34, c.y - r * 0.22),
        (s * 0.07).max(1.1),
        color,
    );
    painter.circle_filled(
        egui::pos2(c.x + r * 0.34, c.y - r * 0.22),
        (s * 0.07).max(1.1),
        color,
    );
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(c.x - r * 0.42, c.y + r * 0.16),
            egui::pos2(c.x - r * 0.18, c.y + r * 0.38),
            egui::pos2(c.x + r * 0.18, c.y + r * 0.38),
            egui::pos2(c.x + r * 0.42, c.y + r * 0.16),
        ],
        stroke,
    ));
}

/// Crisp pin icon centered at `c` for pinned comments.
pub fn draw_pin_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new((s * 0.11).max(1.2), color);
    let head = vec![
        egui::pos2(c.x - s * 0.22, c.y - s * 0.36),
        egui::pos2(c.x + s * 0.22, c.y - s * 0.36),
        egui::pos2(c.x + s * 0.16, c.y - s * 0.04),
        egui::pos2(c.x + s * 0.32, c.y + s * 0.08),
        egui::pos2(c.x - s * 0.32, c.y + s * 0.08),
        egui::pos2(c.x - s * 0.16, c.y - s * 0.04),
    ];
    painter.add(egui::Shape::closed_line(head, stroke));
    painter.line_segment(
        [
            egui::pos2(c.x, c.y + s * 0.08),
            egui::pos2(c.x, c.y + s * 0.42),
        ],
        stroke,
    );
}

/// YouTube-style Theater Mode wide rectangle icon.
pub fn draw_theater_icon(
    painter: &egui::Painter,
    c: egui::Pos2,
    s: f32,
    color: Color32,
    active: bool,
) {
    let w = if active { s * 0.80 } else { s * 0.96 };
    let h = if active { s * 0.62 } else { s * 0.56 };
    let r = egui::Rect::from_center_size(c, egui::vec2(w, h));
    painter.rect_stroke(r, 2.0, Stroke::new(1.7, color), egui::StrokeKind::Middle);
}

/// YouTube-style Miniplayer (PiP) icon.
pub fn draw_miniplayer_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let outer = egui::Rect::from_center_size(c, egui::vec2(s * 0.94, s * 0.68));
    painter.rect_stroke(outer, 2.0, Stroke::new(1.5, color), egui::StrokeKind::Middle);
    let inner = egui::Rect::from_min_max(
        egui::pos2(outer.right() - s * 0.42, outer.bottom() - s * 0.32),
        egui::pos2(outer.right() - s * 0.10, outer.bottom() - s * 0.10),
    );
    painter.rect_filled(inner, 1.0, color);
}

/// YouTube-style Fullscreen corner brackets icon.
pub fn draw_fullscreen_icon(
    painter: &egui::Painter,
    c: egui::Pos2,
    s: f32,
    color: Color32,
    is_fullscreen: bool,
) {
    let stroke = Stroke::new(1.8, color);
    let outer = s * 0.39;
    if is_fullscreen {
        let inner = s * 0.14;
        let arm = outer - inner;
        for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let vertex = egui::pos2(c.x + sx * inner, c.y + sy * inner);
            let h_end = egui::pos2(vertex.x + sx * arm, vertex.y);
            let v_end = egui::pos2(vertex.x, vertex.y + sy * arm);
            painter.add(egui::Shape::line(vec![h_end, vertex, v_end], stroke));
        }
    } else {
        let arm = s * 0.22;
        for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let corner = egui::pos2(c.x + sx * outer, c.y + sy * outer);
            let h_end = egui::pos2(corner.x - sx * arm, corner.y);
            let v_end = egui::pos2(corner.x, corner.y - sy * arm);
            painter.add(egui::Shape::line(vec![h_end, corner, v_end], stroke));
        }
    }
}

/// Crisp refresh / reload circular arrow icon centered at `c`.
pub fn draw_refresh_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let r = s * 0.36;
    let stroke = Stroke::new((s * 0.12).max(1.4), color);
    let steps = 16;
    let start = -0.35_f32;
    let sweep = std::f32::consts::TAU * 0.76;
    let pts: Vec<egui::Pos2> = (0..=steps)
        .map(|k| {
            let a = start + sweep * (k as f32 / steps as f32);
            egui::pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts, stroke));
    let tip = egui::pos2(c.x + r * start.cos(), c.y + r * start.sin());
    let head = vec![
        egui::pos2(tip.x + s * 0.10, tip.y - s * 0.22),
        egui::pos2(tip.x + s * 0.16, tip.y + s * 0.08),
        egui::pos2(tip.x - s * 0.16, tip.y + s * 0.04),
    ];
    painter.add(egui::Shape::convex_polygon(head, color, Stroke::NONE));
}

/// Crisp YouTube Shorts badge outline + inner play glyph centered at `c`.
pub fn draw_shorts_icon(painter: &egui::Painter, c: egui::Pos2, s: f32, color: Color32) {
    let r = egui::Rect::from_center_size(c, egui::vec2(s * 0.66, s * 0.88));
    painter.rect_stroke(
        r,
        s * 0.16,
        Stroke::new((s * 0.10).max(1.4), color),
        egui::StrokeKind::Middle,
    );
    let tri = vec![
        egui::pos2(c.x - s * 0.10, c.y - s * 0.16),
        egui::pos2(c.x + s * 0.16, c.y),
        egui::pos2(c.x - s * 0.10, c.y + s * 0.16),
    ];
    painter.add(egui::Shape::convex_polygon(tri, color, Stroke::NONE));
}

/// Crisp anti-aliased crescent moon + sparkle star icon for Sleep Timer (Lucide MoonStar silhouette).
pub fn draw_sleep_timer_icon(
    ctx: &egui::Context,
    painter: &egui::Painter,
    center: egui::Pos2,
    size: f32,
    color: Color32,
) {
    let ppp = ctx.pixels_per_point();
    let px = (((size * ppp).round() as usize) + 1 & !1).clamp(16, 128);
    let key = egui::Id::unique(("sleep_moon_star_icon", px));
    let tex = ctx
        .data_mut(|d| d.get_temp::<egui::TextureHandle>(key))
        .unwrap_or_else(|| {
            let mut pixels = vec![Color32::TRANSPARENT; px * px];
            // Sample on a normalized 24x24 grid with 4x4 supersampling.
            let scale = 24.0 / px as f32;
            let in_shape = |u: f32, v: f32| -> bool {
                // Smooth solid crescent moon (outer disk minus offset upper-right cutout)
                let dx1 = u - 10.6;
                let dy1 = v - 13.0;
                let r1_sq = dx1 * dx1 + dy1 * dy1;
                let dx2 = u - 14.9;
                let dy2 = v - 9.0;
                let r2_sq = dx2 * dx2 + dy2 * dy2;
                let in_moon = r1_sq <= 7.8 * 7.8 && r2_sq >= 6.1 * 6.1;
                // Crisp 4-point concave sparkle star at upper-right (17.6, 6.2)
                let sx = (u - 17.6).abs();
                let sy = (v - 6.2).abs();
                let in_star = sx.sqrt() + sy.sqrt() <= 1.68;
                in_moon || in_star
            };
            for y in 0..px {
                for x in 0..px {
                    let mut hits = 0u32;
                    for sy in 0..4 {
                        for sx in 0..4 {
                            let u = (x as f32 + (sx as f32 + 0.5) * 0.25) * scale;
                            let v = (y as f32 + (sy as f32 + 0.5) * 0.25) * scale;
                            if in_shape(u, v) {
                                hits += 1;
                            }
                        }
                    }
                    if hits > 0 {
                        let a = ((hits as f32 / 16.0) * 255.0).round() as u8;
                        pixels[y * px + x] = Color32::from_white_alpha(a);
                    }
                }
            }
            let img = egui::ColorImage {
                size: [px, px],
                source_size: egui::Vec2::splat(px as f32),
                pixels,
            };
            let tex = ctx.load_texture(
                format!("sleep-moon-star-{px}"),
                img,
                egui::TextureOptions::LINEAR,
            );
            ctx.data_mut(|d| d.insert_temp(key, tex.clone()));
            tex
        });
    let rect = egui::Rect::from_center_size(center, egui::Vec2::splat(size));
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    painter.image(tex.id(), rect, uv, color);
}

/// Generate a crisp 64x64 YouTube-rs RGBA icon for the window titlebar & Windows taskbar.
pub fn generate_app_icon() -> egui::IconData {
    let size: u32 = 64;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    let inside_rounded_rect = |px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| -> bool {
        let cx = px.clamp(x0 + r, x1 - r);
        let cy = py.clamp(y0 + r, y1 - r);
        let dx = px - cx;
        let dy = py - cy;
        (dx * dx + dy * dy) <= r * r && px >= x0 && px <= x1 && py >= y0 && py <= y1
    };

    let inside_tri = |px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, cx: f32, cy: f32| -> bool {
        let s1 = (px - bx) * (ay - by) - (ax - bx) * (py - by);
        let s2 = (px - cx) * (by - cy) - (bx - cx) * (py - cy);
        let s3 = (px - ax) * (cy - ay) - (cx - ax) * (py - ay);
        let has_neg = s1 < 0.0 || s2 < 0.0 || s3 < 0.0;
        let has_pos = s1 > 0.0 || s2 > 0.0 || s3 > 0.0;
        !(has_neg && has_pos)
    };

    // 4x4 supersampling for clean anti-aliased edges
    for y in 0..size {
        for x in 0..size {
            let mut r_acc = 0.0f32;
            let mut g_acc = 0.0f32;
            let mut b_acc = 0.0f32;
            let mut a_acc = 0.0f32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f32 + (sx as f32 + 0.5) * 0.25;
                    let py = y as f32 + (sy as f32 + 0.5) * 0.25;
                    if inside_rounded_rect(px, py, 3.0, 10.0, 61.0, 54.0, 13.0) {
                        a_acc += 1.0;
                        if inside_tri(px, py, 25.5, 22.0, 44.0, 32.0, 25.5, 42.0) {
                            r_acc += 255.0;
                            g_acc += 255.0;
                            b_acc += 255.0;
                        } else {
                            r_acc += 255.0;
                            g_acc += 0.0;
                            b_acc += 51.0;
                        }
                    }
                }
            }
            let idx = ((y * size + x) * 4) as usize;
            if a_acc > 0.0 {
                rgba[idx] = (r_acc / a_acc).round() as u8;
                rgba[idx + 1] = (g_acc / a_acc).round() as u8;
                rgba[idx + 2] = (b_acc / a_acc).round() as u8;
                rgba[idx + 3] = ((a_acc / 16.0) * 255.0).round() as u8;
            }
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}
