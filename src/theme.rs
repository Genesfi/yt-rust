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
    let img = egui::Image::new(url)
        .show_loading_spinner(false)
        .corner_radius(CornerRadius::same(radius))
        .fit_to_exact_size(rect.size());
    let ready = img
        .load_for_size(ui.ctx(), rect.size())
        .is_ok_and(|p| matches!(p, egui::load::TexturePoll::Ready { .. }));
    if !ready {
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
    let half = s * 0.38;
    let arm = s * 0.20;
    for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let corner = egui::pos2(c.x + sx * half, c.y + sy * half);
        if is_fullscreen {
            let p_mid = egui::pos2(c.x + sx * (half - arm), c.y + sy * (half - arm));
            painter.line_segment([p_mid, egui::pos2(p_mid.x - sx * arm, p_mid.y)], stroke);
            painter.line_segment([p_mid, egui::pos2(p_mid.x, p_mid.y - sy * arm)], stroke);
        } else {
            painter.line_segment([corner, egui::pos2(corner.x - sx * arm, corner.y)], stroke);
            painter.line_segment([corner, egui::pos2(corner.x, corner.y - sy * arm)], stroke);
        }
    }
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
