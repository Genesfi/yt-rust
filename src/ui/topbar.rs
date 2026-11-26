use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Key, Layout, Margin, Rect, RichText, Sense,
    Stroke, Vec2, pos2,
};

use crate::backend::Request;
use crate::innertube;
use crate::theme::{self, icon};

use super::cards::{draw_avatar, icon_circle_button};
use super::{Page, YtApp};

impl YtApp {
    pub(super) fn draw_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar")
            .default_size(58.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    // Hamburger menu button with smooth hover circle
                    let (menu_rect, menu_resp) =
                        ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
                    let menu_hover = ui.ctx().animate_bool_with_time(
                        egui::Id::unique("topbar_menu_hover"),
                        menu_resp.hovered(),
                        0.14,
                    );
                    if menu_hover > 0.01 {
                        ui.painter().circle_filled(
                            menu_rect.center(),
                            17.0,
                            theme::CHIP.gamma_multiply(menu_hover),
                        );
                    }
                    if menu_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    for dy in [-5.0f32, 0.0, 5.0] {
                        let y = menu_rect.center().y + dy;
                        ui.painter().line_segment(
                            [
                                pos2(menu_rect.center().x - 8.5, y),
                                pos2(menu_rect.center().x + 8.5, y),
                            ],
                            Stroke::new(1.6, theme::TEXT),
                        );
                    }
                    if menu_resp.clicked() {
                        self.sidebar_expanded = !self.sidebar_expanded;
                    }

                    ui.add_space(6.0);

                    if (!self.nav_back.is_empty() || self.page == Page::Watch)
                        && icon_circle_button(ui, icon::CHEVRON_LEFT, 18.0, "Back (Mouse Back / Alt+Left)")
                    {
                        self.nav_go_back();
                    }
                    if !self.nav_forward.is_empty()
                        && icon_circle_button(
                            ui,
                            icon::CHEVRON_RIGHT,
                            18.0,
                            "Forward (Mouse Forward / Alt+Right)",
                        )
                    {
                        self.nav_go_forward();
                    }
                    if !self.nav_back.is_empty()
                        || !self.nav_forward.is_empty()
                        || self.page == Page::Watch
                    {
                        ui.add_space(4.0);
                    }

                    // YouTube brand badge
                    let (logo_rect, logo_resp) =
                        ui.allocate_exact_size(Vec2::new(126.0, 28.0), Sense::click());
                    if logo_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let badge = Rect::from_min_size(
                        pos2(logo_rect.left(), logo_rect.center().y - 10.0),
                        Vec2::new(28.0, 20.0),
                    );
                    theme::draw_youtube_logo(ui.painter(), badge);
                    ui.painter().text(
                        pos2(badge.right() + 7.0, logo_rect.center().y),
                        Align2::LEFT_CENTER,
                        "YouTube",
                        theme::bold(17.5),
                        Color32::WHITE,
                    );
                    ui.painter().text(
                        pos2(badge.right() + 85.0, logo_rect.center().y - 5.0),
                        Align2::LEFT_CENTER,
                        "rs",
                        theme::semibold(10.0),
                        theme::TEXT_DIM,
                    );
                    if logo_resp.clicked() {
                        self.page = Page::Home;
                    }

                    // Centered pill Search bar
                    let total_avail = ui.available_width();
                    let right_reserve = 175.0;
                    let search_w = (total_avail - right_reserve).clamp(180.0, 560.0);
                    let left_pad = ((total_avail - right_reserve - search_w) * 0.5).max(8.0);
                    ui.add_space(left_pad);

                    let pill_h = 38.0;
                    let (pill_rect, _) =
                        ui.allocate_exact_size(Vec2::new(search_w, pill_h), Sense::hover());

                    let search_id = ui.scope_id().with("search_input_box");
                    let search_focused = ui.ctx().memory(|m| m.has_focus(search_id));
                    let focus_t = ui.ctx().animate_bool_with_time(
                        egui::Id::unique("search_focus_anim"),
                        search_focused,
                        0.16,
                    );

                    ui.painter().rect_filled(
                        pill_rect,
                        CornerRadius::same(19),
                        Color32::from_rgb(18, 18, 18),
                    );
                    let border_col = theme::lerp_color(
                        theme::BORDER,
                        Color32::from_rgb(28, 98, 185),
                        focus_t,
                    );
                    ui.painter().rect_stroke(
                        pill_rect,
                        CornerRadius::same(19),
                        Stroke::new(1.0 + 0.4 * focus_t, border_col),
                        egui::StrokeKind::Inside,
                    );

                    // Search button cap on right of pill
                    let btn_w = 56.0;
                    let btn_rect = Rect::from_min_max(
                        pos2(pill_rect.right() - btn_w, pill_rect.top()),
                        pill_rect.max,
                    );
                    let btn_resp =
                        ui.interact(btn_rect, ui.scope_id().with("search_btn"), Sense::click());
                    let btn_hover_t = ui.ctx().animate_bool_with_time(
                        egui::Id::unique("search_btn_hover"),
                        btn_resp.hovered(),
                        0.12,
                    );
                    if btn_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let btn_fill = theme::lerp_color(
                        Color32::from_rgb(34, 34, 34),
                        theme::CHIP_HOVER,
                        btn_hover_t,
                    );
                    ui.painter().rect_filled(
                        btn_rect,
                        CornerRadius {
                            nw: 0,
                            sw: 0,
                            ne: 19,
                            se: 19,
                        },
                        btn_fill,
                    );
                    ui.painter().line_segment(
                        [btn_rect.left_top(), btn_rect.left_bottom()],
                        Stroke::new(1.0, theme::BORDER),
                    );
                    theme::draw_icon_centered(
                        ui.painter(),
                        btn_rect.center(),
                        icon::SEARCH,
                        16.0,
                        theme::TEXT,
                    );
                    if btn_resp.clicked() {
                        self.submit_search();
                    }

                    // Input vertically centered inside left portion of pill
                    let clear_w = if self.search_input.is_empty() { 0.0 } else { 28.0 };
                    let input_rect = Rect::from_min_max(
                        pos2(pill_rect.left() + 16.0, pill_rect.center().y - 10.0),
                        pos2(btn_rect.left() - clear_w - 4.0, pill_rect.center().y + 10.0),
                    );
                    let mut input_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(input_rect));
                    let edit_resp = input_ui.add_sized(
                        input_rect.size(),
                        egui::TextEdit::singleline(&mut self.search_input)
                            .id(search_id)
                            .frame(Frame::NONE)
                            .font(theme::regular(14.0))
                            .hint_text(
                                RichText::new("Search or paste a YouTube URL")
                                    .color(theme::TEXT_FAINT),
                            ),
                    );
                    if edit_resp.gained_focus() && !self.search_input.trim().is_empty() {
                        self.search_popup_open = true;
                    }
                    if edit_resp.changed() {
                        let q = self.search_input.trim().to_owned();
                        if q.is_empty() || innertube::extract_video_id(&q).is_some() {
                            self.search_suggestions.clear();
                            self.search_popup_open = false;
                            self.last_suggest_edit = None;
                        } else {
                            self.search_popup_open = true;
                            self.last_suggest_edit = Some(Instant::now());
                        }
                    }
                    if let Some(edited_at) = self.last_suggest_edit {
                        if edited_at.elapsed() >= Duration::from_millis(180) {
                            self.last_suggest_edit = None;
                            let q = self.search_input.trim().to_owned();
                            if !q.is_empty() && q != self.last_suggest_query {
                                self.last_suggest_query = q.clone();
                                let _ = self.backend.tx.send(Request::SearchSuggest { query: q });
                            }
                        } else {
                            ui.ctx().request_repaint_after(Duration::from_millis(60));
                        }
                    }
                    if edit_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.search_popup_open = false;
                        self.submit_search();
                    }
                    if self.search_popup_open
                        && !self.search_suggestions.is_empty()
                        && !edit_resp.has_focus()
                        && ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        self.search_popup_open = false;
                    }
                    if !self.search_input.is_empty() {
                        let clear_rect = Rect::from_min_max(
                            pos2(btn_rect.left() - clear_w, pill_rect.top()),
                            pos2(btn_rect.left(), pill_rect.bottom()),
                        );
                        let clr = ui.interact(
                            clear_rect,
                            ui.scope_id().with("clr_search"),
                            Sense::click(),
                        );
                        theme::draw_icon_centered(
                            ui.painter(),
                            clear_rect.center(),
                            icon::X,
                            14.0,
                            if clr.hovered() {
                                Color32::WHITE
                            } else {
                                theme::TEXT_DIM
                            },
                        );
                        if clr.clicked() {
                            self.search_input.clear();
                            self.search_suggestions.clear();
                            self.search_popup_open = false;
                        }
                    }

                    if self.search_popup_open && !self.search_suggestions.is_empty() {
                        let mut picked_suggestion = None;
                        let popup_pos = pos2(pill_rect.left() + 6.0, pill_rect.bottom() + 6.0);
                        let popup_w = (pill_rect.width() - 12.0).max(200.0);
                        let area_resp = egui::Area::new(egui::Id::unique("search_suggest_popup"))
                            .fixed_pos(popup_pos)
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                Frame::new()
                                    .fill(theme::SURFACE)
                                    .stroke(Stroke::new(1.0, theme::BORDER))
                                    .corner_radius(14)
                                    .shadow(egui::epaint::Shadow {
                                        offset: [0, 8],
                                        blur: 20,
                                        spread: 2,
                                        color: Color32::from_black_alpha(165),
                                    })
                                    .inner_margin(Margin::symmetric(6, 6))
                                    .show(ui, |ui| {
                                        ui.set_width(popup_w - 12.0);
                                        for s in &self.search_suggestions {
                                            let row = ui.add_sized(
                                                Vec2::new(popup_w - 16.0, 32.0),
                                                egui::Button::new(
                                                    RichText::new(format!("{}  {s}", icon::SEARCH))
                                                        .font(theme::regular(13.0))
                                                        .color(theme::TEXT),
                                                )
                                                .fill(Color32::TRANSPARENT)
                                                .corner_radius(8),
                                            );
                                            if row.clicked() {
                                                picked_suggestion = Some(s.clone());
                                            }
                                        }
                                    });
                            });
                        if let Some(s) = picked_suggestion {
                            self.search_input = s;
                            self.search_popup_open = false;
                            self.submit_search();
                        } else if ui.ctx().input(|i| i.pointer.any_pressed())
                            && ui.ctx().input(|i| i.pointer.interact_pos()).is_some_and(|p| {
                                !pill_rect.contains(p) && !area_resp.response.rect.contains(p)
                            })
                        {
                            self.search_popup_open = false;
                        }
                    }

                    // Right side: Account Sign-in / Avatar pill
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.signed_in {
                            let label = self
                                .account_name
                                .as_deref()
                                .unwrap_or("Account")
                                .to_owned();
                            let avatar = self.account_avatar.clone();
                            ui.horizontal(|ui| {
                                draw_avatar(ui, avatar.as_deref(), &label, 26.0);
                                ui.menu_button(
                                    RichText::new(&label).font(theme::medium(13.0)),
                                    |ui| {
                                        if ui
                                            .button(format!("{}  Settings", icon::SETTINGS))
                                            .clicked()
                                        {
                                            ui.close();
                                            self.page = Page::Settings;
                                        }
                                        if ui
                                            .button(format!("{}  Sign out", icon::LOG_OUT))
                                            .clicked()
                                        {
                                            ui.close();
                                            let _ = self.backend.tx.send(Request::SignOut);
                                        }
                                    },
                                );
                            });
                        } else {
                            let btn_text = if self.signing_in {
                                "Signing in...".to_owned()
                            } else {
                                format!("{}  Sign in", icon::CIRCLE_USER)
                            };
                            let sign_btn = egui::Button::new(
                                RichText::new(btn_text)
                                    .font(theme::medium(13.0))
                                    .color(theme::YT_BLUE),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, theme::BORDER))
                            .corner_radius(CornerRadius::same(18));
                            if ui.add_enabled(!self.signing_in, sign_btn).clicked() {
                                self.start_sign_in();
                            }
                        }
                    });
                });
            });
    }

    pub(super) fn draw_toast_snackbar(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let active = self.status_toast.as_ref().is_some_and(|(_, at)| {
            now.saturating_duration_since(*at) < Duration::from_secs(3)
        });
        let anim_t = ctx.animate_bool_with_time(
            egui::Id::unique("snackbar_toast_anim"),
            active,
            0.18,
        );
        if !active && anim_t <= 0.01 {
            self.status_toast = None;
            return;
        }
        let Some((msg, _)) = &self.status_toast else {
            return;
        };
        if active {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        let eased = theme::ease_out_cubic(anim_t);
        let slide_y = (1.0 - eased) * 14.0;
        let alpha = (eased * 245.0).round() as u8;
        let screen = ctx.content_rect();
        let pos = pos2(screen.left() + 24.0, screen.bottom() - 56.0 + slide_y);

        egui::Area::new(egui::Id::unique("youtube_toast_snackbar"))
            .fixed_pos(pos)
            .interactable(false)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(33, 33, 36, alpha))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(72, 72, 78, alpha),
                    ))
                    .corner_radius(10)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 1,
                        color: Color32::from_black_alpha((eased * 150.0) as u8),
                    })
                    .inner_margin(Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(msg)
                                .font(theme::medium(13.0))
                                .color(Color32::from_rgba_unmultiplied(241, 241, 241, alpha)),
                        );
                    });
            });
    }
}
