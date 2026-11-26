use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Key, Layout, Margin, Rect, RichText, Sense,
    Stroke, Vec2, pos2,
};

use crate::backend::Request;
use crate::innertube;
use crate::theme::{self, icon};

use super::cards::{draw_avatar, draw_thumbnail, icon_circle_button};
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
                    let right_reserve = if self.signed_in { 252.0 } else { 212.0 };
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

                    // Right side: Account Sign-in / Avatar pill + Notifications Bell
                    let mut bell_anchor: Option<(Rect, bool)> = None;
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

                            ui.add_space(4.0);
                            let (brect, bresp) =
                                ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
                            let b_hov = ui.ctx().animate_bool_with_time(
                                egui::Id::unique("topbar_notif_bell_hover"),
                                bresp.hovered() || self.notifications_open,
                                0.14,
                            );
                            if b_hov > 0.01 {
                                ui.painter().circle_filled(
                                    brect.center(),
                                    17.0,
                                    theme::CHIP.gamma_multiply(b_hov),
                                );
                            }
                            if bresp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            theme::draw_bell(
                                ui.painter(),
                                brect.center(),
                                18.0,
                                if self.notifications_open {
                                    Color32::WHITE
                                } else {
                                    theme::TEXT
                                },
                            );
                            if self.unread_notifications > 0 {
                                let badge_txt = if self.unread_notifications > 9 {
                                    "9+".to_owned()
                                } else {
                                    self.unread_notifications.to_string()
                                };
                                let badge_rect = Rect::from_min_size(
                                    pos2(brect.right() - 16.0, brect.top() + 3.0),
                                    Vec2::new(if self.unread_notifications > 9 { 18.0 } else { 15.0 }, 13.0),
                                );
                                ui.painter().rect_filled(badge_rect, 6.5, theme::YT_RED);
                                ui.painter().text(
                                    badge_rect.center(),
                                    Align2::CENTER_CENTER,
                                    badge_txt,
                                    theme::bold(8.5),
                                    Color32::WHITE,
                                );
                            }
                            let mut toggled = false;
                            if bresp.on_hover_text("Notifications").clicked() {
                                self.notifications_open = !self.notifications_open;
                                toggled = true;
                                if self.notifications_open {
                                    let stale = self
                                        .notifications_loaded_at
                                        .is_none_or(|t| t.elapsed() > Duration::from_secs(60));
                                    if (self.notifications.is_empty() || stale)
                                        && !self.notifications_loading
                                    {
                                        self.notifications_loading = true;
                                        let _ = self.backend.tx.send(Request::LoadNotifications);
                                    } else if self.unread_notifications > 0 {
                                        if let Some(p) = self.notifications_seen_params.clone() {
                                            let _ = self
                                                .backend
                                                .tx
                                                .send(Request::MarkNotificationsSeen { params: p });
                                        }
                                    }
                                }
                            }
                            bell_anchor = Some((brect, toggled));
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

                        // Always-on-Top pin toggle (handy for mini-player / compact windows)
                        ui.add_space(4.0);
                        let (pin_rect, pin_resp) =
                            ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                        let pin_hov = ui.ctx().animate_bool_with_time(
                            egui::Id::unique("topbar_pin_hover"),
                            pin_resp.hovered() || self.always_on_top,
                            0.14,
                        );
                        if pin_hov > 0.01 {
                            ui.painter().circle_filled(
                                pin_rect.center(),
                                16.0,
                                if self.always_on_top {
                                    theme::YT_RED.gamma_multiply(0.22 * pin_hov)
                                } else {
                                    theme::CHIP.gamma_multiply(pin_hov)
                                },
                            );
                        }
                        if pin_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        theme::draw_pin_icon(
                            ui.painter(),
                            pin_rect.center(),
                            15.5,
                            if self.always_on_top {
                                theme::YT_RED
                            } else {
                                theme::TEXT_DIM
                            },
                        );
                        let pin_tip = if self.always_on_top {
                            "Keep on Top: On (click to unpin)"
                        } else {
                            "Keep on Top"
                        };
                        if pin_resp.on_hover_text(pin_tip).clicked() {
                            let next = !self.always_on_top;
                            self.set_always_on_top(ui.ctx(), next);
                        }
                    });
                    if self.notifications_open {
                        if let Some((brect, toggled)) = bell_anchor {
                            self.draw_notifications_popup(ui.ctx(), brect, toggled);
                        }
                    }
                });
            });
    }

    fn draw_notifications_popup(
        &mut self,
        ctx: &egui::Context,
        bell_rect: Rect,
        just_toggled: bool,
    ) {
        let screen = ctx.content_rect();
        let popup_w = 390.0_f32.min((screen.width() - 24.0).max(260.0));
        let anchor = pos2(
            (bell_rect.right()).clamp(screen.left() + popup_w + 12.0, screen.right() - 12.0),
            bell_rect.bottom() + 8.0,
        );
        let mut open_video: Option<String> = None;
        let mut open_channel: Option<String> = None;
        let mut clicked_idx: Option<usize> = None;

        let area_resp = egui::Area::new(egui::Id::unique("topbar_notifications_popup"))
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(anchor)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(12)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 10],
                        blur: 28,
                        spread: 2,
                        color: Color32::from_black_alpha(190),
                    })
                    .inner_margin(Margin::symmetric(0, 10))
                    .show(ui, |ui| {
                        ui.set_width(popup_w);
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new("Notifications")
                                    .font(theme::semibold(15.0))
                                    .color(theme::TEXT),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add_space(10.0);
                                if icon_circle_button(ui, icon::X, 14.0, "Close") {
                                    self.notifications_open = false;
                                }
                                if ui
                                    .add_enabled(
                                        !self.notifications_loading,
                                        egui::Button::new(
                                            RichText::new(if self.notifications_loading {
                                                "Loading..."
                                            } else {
                                                "Refresh"
                                            })
                                            .font(theme::medium(12.0))
                                            .color(theme::YT_BLUE),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .corner_radius(14),
                                    )
                                    .clicked()
                                {
                                    self.notifications_loading = true;
                                    let _ = self.backend.tx.send(Request::LoadNotifications);
                                }
                            });
                        });
                        ui.add_space(6.0);
                        ui.painter().line_segment(
                            [
                                pos2(ui.min_rect().left(), ui.cursor().top()),
                                pos2(ui.min_rect().left() + popup_w, ui.cursor().top()),
                            ],
                            Stroke::new(1.0, theme::BORDER),
                        );
                        ui.add_space(4.0);

                        if self.notifications_loading && self.notifications.is_empty() {
                            ui.add_space(28.0);
                            ui.vertical_centered(|ui| {
                                ui.spinner();
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("Loading notifications...")
                                        .font(theme::regular(13.0))
                                        .color(theme::TEXT_DIM),
                                );
                            });
                            ui.add_space(28.0);
                        } else if self.notifications.is_empty() {
                            ui.add_space(28.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Your notifications live here")
                                        .font(theme::medium(14.0))
                                        .color(theme::TEXT),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(
                                        "Subscribe to channels to get notified of new uploads.",
                                    )
                                    .font(theme::regular(12.5))
                                    .color(theme::TEXT_DIM),
                                );
                            });
                            ui.add_space(28.0);
                        } else {
                            let max_h = (screen.height() - 120.0).clamp(220.0, 460.0);
                            egui::ScrollArea::vertical()
                                .id_salt("notifications_menu_scroll")
                                .max_height(max_h)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    for (idx, item) in self.notifications.iter().enumerate() {
                                        let row_w = popup_w - 8.0;
                                        let (row_rect, row_resp) = ui.allocate_exact_size(
                                            Vec2::new(row_w, 66.0),
                                            Sense::click(),
                                        );
                                        if row_resp.hovered() {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                            ui.painter().rect_filled(
                                                row_rect,
                                                8.0,
                                                theme::CHIP,
                                            );
                                        }
                                        let mut r_ui = ui.new_child(
                                            egui::UiBuilder::new()
                                                .max_rect(row_rect.shrink2(Vec2::new(10.0, 8.0))),
                                        );
                                        r_ui.horizontal(|ui| {
                                            let dot_x = ui.cursor().left() + 4.0;
                                            let dot_y = row_rect.center().y;
                                            if !item.read {
                                                ui.painter().circle_filled(
                                                    pos2(dot_x, dot_y),
                                                    3.0,
                                                    theme::YT_BLUE,
                                                );
                                            }
                                            ui.add_space(10.0);
                                            draw_avatar(
                                                ui,
                                                item.channel_avatar_url.as_deref(),
                                                "N",
                                                36.0,
                                            );
                                            ui.add_space(6.0);
                                            let thumb_reserve =
                                                if item.video_thumbnail_url.is_some() {
                                                    86.0
                                                } else {
                                                    4.0
                                                };
                                            let text_w =
                                                (ui.available_width() - thumb_reserve).max(110.0);
                                            ui.vertical(|ui| {
                                                ui.set_width(text_w);
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(&item.message)
                                                            .font(theme::regular(13.0))
                                                            .color(theme::TEXT),
                                                    )
                                                    .wrap(),
                                                );
                                                if !item.sent_time_text.is_empty() {
                                                    ui.label(
                                                        RichText::new(&item.sent_time_text)
                                                            .font(theme::regular(11.5))
                                                            .color(theme::TEXT_DIM),
                                                    );
                                                }
                                            });
                                            if let Some(thumb) =
                                                item.video_thumbnail_url.as_deref()
                                            {
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        draw_thumbnail(
                                                            ui,
                                                            thumb,
                                                            "",
                                                            Vec2::new(76.0, 44.0),
                                                            6.0,
                                                            None,
                                                        );
                                                    },
                                                );
                                            }
                                        });
                                        if row_resp.clicked() {
                                            clicked_idx = Some(idx);
                                            if let Some(vid) = &item.video_id {
                                                open_video = Some(vid.clone());
                                            } else if let Some(cid) = &item.channel_id {
                                                open_channel = Some(cid.clone());
                                            }
                                        }
                                    }
                                });
                        }
                    });
            });

        if let Some(idx) = clicked_idx {
            if let Some(n) = self.notifications.get_mut(idx) {
                n.read = true;
            }
            self.unread_notifications = self.unread_notifications.saturating_sub(1);
            self.notifications_open = false;
        }
        if let Some(vid) = open_video {
            self.open_video_id(&vid);
            return;
        }
        if let Some(cid) = open_channel {
            self.open_channel(cid, "Channel".to_owned());
            return;
        }

        let clicked_outside = !just_toggled
            && ctx.input(|i| i.pointer.any_released())
            && ctx.input(|i| i.pointer.interact_pos()).is_none_or(|p| {
                !area_resp.response.rect.contains(p) && !bell_rect.contains(p)
            });
        if clicked_outside {
            self.notifications_open = false;
        }
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
        let slide_y = (1.0 - eased) * 12.0;
        let alpha = (eased * 246.0).round() as u8;
        let screen = ctx.content_rect();
        let left_x = if self.fullscreen {
            24.0
        } else if self.sidebar_expanded {
            224.0
        } else {
            84.0
        };
        let pos = pos2(
            (screen.left() + left_x).min(screen.right() - 200.0),
            screen.bottom() - 22.0 + slide_y,
        );
        let max_text_w = (screen.right() - pos.x - 88.0).clamp(180.0, 440.0);

        let lower = msg.to_ascii_lowercase();
        let (badge_icon, accent) = if lower.contains("failed") || lower.contains("error") {
            (icon::X, theme::YT_RED)
        } else if lower.starts_with("queued")
            || lower.starts_with("added")
            || lower.starts_with("playing next")
        {
            (icon::LIST_MUSIC, theme::YT_BLUE)
        } else {
            (icon::CHECK, Color32::from_rgb(76, 195, 138))
        };

        egui::Area::new(egui::Id::unique("youtube_toast_snackbar"))
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(pos)
            .interactable(false)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(30, 31, 36, alpha))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(78, 80, 90, alpha),
                    ))
                    .corner_radius(10)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 1,
                        color: Color32::from_black_alpha((eased * 165.0) as u8),
                    })
                    .inner_margin(Margin::symmetric(14, 9))
                    .show(ui, |ui| {
                        let font = theme::medium(13.0);
                        let text_col = Color32::from_rgba_unmultiplied(244, 244, 246, alpha);
                        let measured_w = ui
                            .painter()
                            .layout_no_wrap(msg.clone(), font.clone(), text_col)
                            .size()
                            .x
                            .ceil()
                            .clamp(60.0, max_text_w);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            let (dot_r, _) =
                                ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                            ui.painter().circle_filled(
                                dot_r.center(),
                                10.0,
                                accent.gamma_multiply(eased * 0.20),
                            );
                            theme::draw_icon_centered(
                                ui.painter(),
                                dot_r.center(),
                                badge_icon,
                                12.0,
                                accent.gamma_multiply(eased),
                            );
                            ui.add_sized(
                                Vec2::new(measured_w, 20.0),
                                egui::Label::new(RichText::new(msg).font(font).color(text_col))
                                    .truncate(),
                            );
                        });
                    });
            });
    }
}
