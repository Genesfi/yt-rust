use egui::{
    Align, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Rect, RichText, ScrollArea,
    Sense, Stroke, Vec2, pos2,
};

use crate::backend::Request;
use crate::model::{
    AmbientMode, ChannelInfo, HwdecMode, PoTokenMode, PreferredQuality, VideoItem, WindowMaterial,
};
use crate::theme::{self, icon};
use crate::ytdlp;

use super::cards::{
    draw_avatar, draw_thumbnail, draw_video_grid, draw_video_grid_with_flags, icon_circle_button,
};
use super::{GUEST_TOPICS, Page, SettingsTab, SubsFilter, YtApp, format_duration};

impl YtApp {
    pub(super) fn draw_home_page(&mut self, ui: &mut egui::Ui) {
        // Filter chips bar with smooth animated < / > scrolling, wheel scroll, and drag panning
        let drag_blocked_click = self.chips_drag_dist >= 6.0;
        let mut clicked_remote: Option<usize> = None;
        let mut clicked_guest: Option<(usize, Option<&'static str>)> = None;

        // Smoothly ease chips_scroll_x toward chips_scroll_target
        let diff = self.chips_scroll_target - self.chips_scroll_x;
        if diff.abs() > 0.5 && !self.chips_dragging {
            let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
            let alpha = 1.0 - (-16.0 * dt).exp();
            self.chips_scroll_x += diff * alpha;
            ui.ctx().request_repaint();
        } else if !self.chips_dragging {
            self.chips_scroll_x = self.chips_scroll_target;
        }

        ui.horizontal(|ui| {
            let left_clicked = icon_circle_button(ui, icon::CHEVRON_LEFT, 15.0, "Scroll left");
            let reload_reserve = 92.0;
            let avail_bar_w = (ui.available_width() - 40.0 - reload_reserve).max(140.0);
            let bar_out = ui.allocate_ui_with_layout(
                Vec2::new(avail_bar_w, 34.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    let bar_clip = ui.clip_rect().intersect(ui.max_rect());
                    ui.set_clip_rect(bar_clip);
                    ScrollArea::horizontal()
                        .id_salt("home_chips")
                        .horizontal_scroll_offset(self.chips_scroll_x)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            if !self.home_chips.is_empty() {
                                for (idx, chip) in self.home_chips.iter().enumerate() {
                                    let fill = if chip.selected {
                                        Color32::from_rgb(241, 241, 241)
                                    } else {
                                        theme::CHIP
                                    };
                                    let text_col = if chip.selected {
                                        Color32::from_rgb(15, 15, 15)
                                    } else {
                                        theme::TEXT
                                    };
                                    let btn = egui::Button::new(
                                        RichText::new(&chip.title)
                                            .font(theme::medium(13.0))
                                            .color(text_col),
                                    )
                                    .fill(fill)
                                    .corner_radius(CornerRadius::same(8));
                                    if ui.add(btn).clicked() && !drag_blocked_click {
                                        clicked_remote = Some(idx);
                                    }
                                }
                            } else {
                                for (idx, &(title, query)) in GUEST_TOPICS.iter().enumerate() {
                                    let selected = self.selected_topic_chip == idx;
                                    let fill = if selected {
                                        Color32::from_rgb(241, 241, 241)
                                    } else {
                                        theme::CHIP
                                    };
                                    let text_col = if selected {
                                        Color32::from_rgb(15, 15, 15)
                                    } else {
                                        theme::TEXT
                                    };
                                    let btn = egui::Button::new(
                                        RichText::new(title)
                                            .font(theme::medium(13.0))
                                            .color(text_col),
                                    )
                                    .fill(fill)
                                    .corner_radius(CornerRadius::same(8));
                                    if ui.add(btn).clicked() && !drag_blocked_click {
                                        clicked_guest = Some((idx, query));
                                    }
                                }
                            }
                        })
                },
            );
            let scroll_out = bar_out.inner;
            let bar_rect = bar_out.response.rect;
            let max_scroll =
                (scroll_out.content_size.x - scroll_out.inner_rect.width()).max(0.0);
            let mut next_target = self.chips_scroll_target.clamp(0.0, max_scroll);
            if left_clicked {
                next_target = (next_target - 240.0).clamp(0.0, max_scroll);
            }

            let hovered_bar = ui.rect_contains_pointer(bar_rect);
            let (primary_down, primary_pressed, secondary_down, delta_x, wheel_y) =
                ui.ctx().input(|i| {
                    (
                        i.pointer.primary_down(),
                        i.pointer.primary_pressed(),
                        i.pointer.secondary_down(),
                        i.pointer.delta().x,
                        i.smooth_scroll_delta.y,
                    )
                });
            if hovered_bar && primary_pressed {
                self.chips_dragging = true;
                self.chips_drag_dist = 0.0;
            }
            if self.chips_dragging && primary_down {
                self.chips_drag_dist += delta_x.abs();
                let cur = (scroll_out.state.clamped_offset().x - delta_x).clamp(0.0, max_scroll);
                self.chips_scroll_x = cur;
                next_target = cur;
                if self.chips_drag_dist >= 6.0 {
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                }
            } else if !primary_down {
                self.chips_dragging = false;
                self.chips_drag_dist = 0.0;
            }
            if hovered_bar {
                if secondary_down {
                    let cur =
                        (scroll_out.state.clamped_offset().x - delta_x).clamp(0.0, max_scroll);
                    self.chips_scroll_x = cur;
                    next_target = cur;
                }
                if wheel_y.abs() > 0.1 {
                    next_target = (next_target - wheel_y * 1.4).clamp(0.0, max_scroll);
                }
            }

            let right_clicked =
                icon_circle_button(ui, icon::CHEVRON_RIGHT, 15.0, "Scroll right (or drag)");
            if right_clicked {
                next_target = (next_target + 240.0).clamp(0.0, max_scroll);
            }
            self.chips_scroll_target = next_target.clamp(0.0, max_scroll);

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (rrect, rresp) =
                    ui.allocate_exact_size(Vec2::new(82.0, 30.0), Sense::click());
                let rhov = ui
                    .ctx()
                    .animate_bool_with_time(rresp.id, rresp.hovered(), 0.12);
                if rresp.hovered() && !self.home_loading {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                ui.painter().rect_filled(
                    rrect,
                    15.0,
                    theme::lerp_color(theme::CHIP, theme::CHIP_HOVER, rhov),
                );
                if self.home_loading {
                    ui.painter().text(
                        rrect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Loading...",
                        theme::medium(11.5),
                        theme::TEXT_DIM,
                    );
                } else {
                    theme::draw_refresh_icon(
                        ui.painter(),
                        pos2(rrect.left() + 17.0, rrect.center().y),
                        13.5,
                        theme::TEXT,
                    );
                    ui.painter().text(
                        pos2(rrect.left() + 30.0, rrect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "Reload",
                        theme::medium(12.0),
                        theme::TEXT,
                    );
                }
                if !self.home_loading
                    && rresp.on_hover_text("Reload Home recommendations (F5)").clicked()
                {
                    self.reload_home_feed();
                }
            });
        });

        if let Some(idx) = clicked_remote {
            let (params, continuation) = if let Some(chip) = self.home_chips.get(idx) {
                (chip.params.clone(), chip.continuation.clone())
            } else {
                (None, None)
            };
            for (i, c) in self.home_chips.iter_mut().enumerate() {
                c.selected = i == idx;
            }
            self.home_items.clear();
            self.home_continuation = None;
            self.home_loading = true;
            let _ = self.backend.tx.send(Request::LoadHome {
                params,
                continuation,
                append: false,
            });
        } else if let Some((idx, query)) = clicked_guest {
            self.selected_topic_chip = idx;
            match query {
                None => {
                    self.home_loading = true;
                    let _ = self.backend.tx.send(Request::LoadHome {
                        params: None,
                        continuation: None,
                        append: false,
                    });
                }
                Some(q) => {
                    let _ = self.backend.tx.send(Request::Search {
                        query: q.to_owned(),
                        continuation: None,
                    });
                    self.active_query = q.to_owned();
                    self.page = Page::Search;
                    self.search_items.clear();
                    self.search_loading = true;
                    self.home_loading = false;
                }
            }
        }
        ui.add_space(10.0);

        if let Some(lw) = self.last_watched.clone() {
            let matches_primary = self.watch.as_ref().is_some_and(|w| w.id == lw.id);
            let matches_second = lw.second_tab.as_ref().is_none_or(|b| {
                self.bg_player_tab
                    .as_ref()
                    .and_then(|s| s.watch.as_ref())
                    .is_some_and(|w| w.id == b.id)
                    || self.watch.as_ref().is_some_and(|w| w.id == b.id)
            });
            let currently_playing_same = matches_primary && matches_second;
            if !currently_playing_same && (lw.position_secs > 2.0 || lw.second_tab.is_some()) {
                let mut resume_now = false;
                let mut dismiss_now = false;
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(12)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let pct = if lw.duration_secs > 10.0 {
                                Some((lw.position_secs / lw.duration_secs).clamp(0.02, 1.0) as f32)
                            } else {
                                None
                            };
                            if !lw.thumbnail_url.is_empty() {
                                draw_thumbnail(
                                    ui,
                                    &lw.thumbnail_url,
                                    "",
                                    Vec2::new(88.0, 50.0),
                                    7.0,
                                    pct,
                                );
                                ui.add_space(10.0);
                            } else {
                                theme::draw_icon(ui, icon::PLAY, 22.0, theme::YT_BLUE);
                                ui.add_space(8.0);
                            }
                            ui.vertical(|ui| {
                                ui.set_max_width((ui.available_width() - 185.0).max(120.0));
                                let time_str = if lw.duration_secs > 1.0 {
                                    format!(
                                        "Continue watching · {} / {}",
                                        format_duration(lw.position_secs),
                                        format_duration(lw.duration_secs)
                                    )
                                } else {
                                    format!(
                                        "Continue watching · {}",
                                        format_duration(lw.position_secs)
                                    )
                                };
                                ui.label(
                                    RichText::new(time_str)
                                        .font(theme::semibold(11.5))
                                        .color(theme::YT_BLUE),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&lw.title)
                                            .font(theme::semibold(13.5))
                                            .color(Color32::WHITE),
                                    )
                                    .truncate(),
                                );
                                let sub_line = if let Some(bg) = &lw.second_tab {
                                    if bg.position_secs > 2.0 {
                                        format!(
                                            "{}  ·  + Tab 2: {} ({})",
                                            lw.channel,
                                            bg.title,
                                            format_duration(bg.position_secs)
                                        )
                                    } else {
                                        format!("{}  ·  + Tab 2: {}", lw.channel, bg.title)
                                    }
                                } else {
                                    lw.channel.clone()
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(sub_line)
                                            .font(theme::regular(12.0))
                                            .color(theme::TEXT_DIM),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_circle_button(ui, icon::X, 14.0, "Dismiss") {
                                    dismiss_now = true;
                                }
                                let btn_label = if lw.second_tab.is_some() {
                                    format!("{}  Resume 2 tabs", icon::PLAY)
                                } else {
                                    format!("{}  Resume", icon::PLAY)
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(btn_label)
                                                .font(theme::semibold(12.5))
                                                .color(Color32::from_rgb(15, 15, 15)),
                                        )
                                        .fill(Color32::from_rgb(241, 241, 241))
                                        .corner_radius(18),
                                    )
                                    .clicked()
                                {
                                    resume_now = true;
                                }
                            });
                        });
                    });
                ui.add_space(10.0);
                if resume_now {
                    let ctx = ui.ctx().clone();
                    self.resume_last_watched(&ctx);
                } else if dismiss_now {
                    self.clear_last_watched();
                }
            }
        }

        if !self.signed_in && self.home_is_fallback {
            Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(12)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::draw_icon(ui, icon::SPARKLES, 16.0, theme::YT_BLUE);
                        ui.label(
                            RichText::new(
                                "Showing Trending & Explore. Sign in with Google for your personalized YouTube Home feed & Subscriptions.",
                            )
                            .font(theme::regular(13.0))
                            .color(theme::TEXT_DIM),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Sign in")
                                            .font(theme::semibold(12.5))
                                            .color(Color32::from_rgb(15, 15, 15)),
                                    )
                                    .fill(theme::YT_BLUE)
                                    .corner_radius(16),
                                )
                                .clicked()
                            {
                                self.start_sign_in();
                            }
                        });
                    });
                });
            ui.add_space(12.0);
        }

        let items = self.home_items.clone();
        let home_shorts = self.prioritized_home_shorts();
        let loading = self.home_loading;
        let cont = self.home_continuation.clone();
        let title = if self.signed_in {
            ""
        } else {
            "Recommended & Trending"
        };
        let ctx = ui.ctx().clone();
        if let Some(act) = super::cards::draw_home_mixed_grid(
            ui,
            title,
            &items,
            &home_shorts,
            loading,
            cont.is_some(),
        ) {
            self.handle_grid_action(&ctx, act, |app| {
                app.home_loading = true;
                let _ = app.backend.tx.send(Request::LoadHome {
                    params: None,
                    continuation: cont,
                    append: true,
                });
            });
        }
    }

    pub(super) fn draw_shorts_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.shorts_grid_view
            && (self.shorts_slot.watch.is_some()
                || self.shorts_slot.watch_loading
                || self.shorts_slot.active_watch_id.is_some())
        {
            self.draw_shorts_player(ui, &ctx);
            return;
        }
        if self.shorts_loading && self.shorts_items.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Opening Shorts Reel...")
                            .font(theme::medium(14.0))
                            .color(theme::TEXT_DIM),
                    );
                });
            });
            return;
        }
        ui.horizontal(|ui| {
            let (irec, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
            theme::draw_shorts_icon(ui.painter(), irec.center(), 18.0, theme::YT_RED);
            ui.add_space(4.0);
            ui.label(RichText::new("Shorts").font(theme::bold(20.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.shorts_loading,
                        egui::Button::new(
                            RichText::new(if self.shorts_loading {
                                "Refreshing..."
                            } else {
                                "Refresh"
                            })
                            .font(theme::medium(12.5))
                            .color(theme::TEXT),
                        )
                        .fill(theme::CHIP)
                        .corner_radius(16),
                    )
                    .clicked()
                {
                    self.shorts_loading = true;
                    let _ = self.backend.tx.send(Request::LoadShorts);
                }
                if let Some(first) = self.shorts_items.first().cloned() {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("{}  Play Shorts", icon::PLAY))
                                    .font(theme::semibold(12.5))
                                    .color(Color32::from_rgb(15, 15, 15)),
                            )
                            .fill(Color32::from_rgb(241, 241, 241))
                            .corner_radius(16),
                        )
                        .clicked()
                    {
                        self.shorts_grid_view = false;
                        let list = self.shorts_items.clone();
                        self.open_short(first, list);
                    }
                }
            });
        });
        ui.add_space(10.0);
        let items = self.shorts_items.clone();
        let loading = self.shorts_loading;
        if let Some(act) = super::cards::draw_shorts_portrait_grid(ui, "", &items, loading) {
            self.handle_grid_action(&ctx, act, |_| {});
        }
    }

    pub(super) fn draw_subscriptions_page(&mut self, ui: &mut egui::Ui) {
        if !self.signed_in {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    theme::draw_icon(ui, icon::RADIO, 42.0, theme::TEXT_DIM);
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Don't miss new videos")
                            .font(theme::bold(20.0))
                            .color(Color32::WHITE),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Sign in to see updates from your favorite YouTube channels")
                            .font(theme::regular(13.5))
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(12.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("{}  Sign in with Google", icon::CIRCLE_USER))
                                    .font(theme::semibold(13.5))
                                    .color(theme::YT_BLUE),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, theme::BORDER))
                            .corner_radius(18),
                        )
                        .clicked()
                    {
                        self.start_sign_in();
                    }
                });
            });
            return;
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Subscriptions")
                    .font(theme::bold(20.0))
                    .color(Color32::WHITE),
            );
            ui.add_space(16.0);
            let live_count = self.subs_items.iter().filter(|it| it.is_live).count();
            let filters = [
                (SubsFilter::All, "All".to_owned()),
                (SubsFilter::Unwatched, "Unwatched".to_owned()),
                (
                    SubsFilter::Live,
                    if live_count > 0 {
                        format!("Live now ({live_count})")
                    } else {
                        "Live now".to_owned()
                    },
                ),
            ];
            for (kind, label) in filters {
                let selected = self.subs_filter == kind;
                let (fill, text_col) = if selected {
                    (Color32::from_rgb(241, 241, 241), Color32::from_rgb(15, 15, 15))
                } else {
                    (theme::CHIP, theme::TEXT)
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(label)
                                .font(theme::medium(13.0))
                                .color(text_col),
                        )
                        .fill(fill)
                        .corner_radius(CornerRadius::same(8)),
                    )
                    .clicked()
                {
                    self.subs_filter = kind;
                }
            }
        });
        ui.add_space(10.0);

        let items: Vec<VideoItem> = self
            .subs_items
            .iter()
            .filter(|it| match self.subs_filter {
                SubsFilter::All => true,
                SubsFilter::Unwatched => !it.watched_percent.is_some_and(|p| p >= 0.85),
                SubsFilter::Live => it.is_live,
            })
            .cloned()
            .collect();
        let loading = self.subs_loading;
        let cont = self.subs_continuation.clone();
        if items.is_empty() && !self.subs_items.is_empty() && !loading {
            Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(12)
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let empty_msg = match self.subs_filter {
                            SubsFilter::Live => "No live streams in loaded subscriptions.",
                            SubsFilter::Unwatched => "All loaded subscription videos have been watched.",
                            SubsFilter::All => "No subscription videos loaded yet.",
                        };
                        ui.label(
                            RichText::new(empty_msg)
                                .font(theme::regular(13.5))
                                .color(theme::TEXT_DIM),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Show all").clicked() {
                                self.subs_filter = SubsFilter::All;
                            }
                        });
                    });
                });
            ui.add_space(10.0);
        }
        let ctx = ui.ctx().clone();
        if let Some(act) = draw_video_grid(ui, "", &items, loading, cont.is_some()) {
            self.handle_grid_action(&ctx, act, |app| {
                app.subs_loading = true;
                let _ = app.backend.tx.send(Request::LoadSubscriptions {
                    continuation: cont,
                });
            });
        }
    }

    pub(super) fn draw_channel_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let info = self.channel_info.clone().unwrap_or_else(|| ChannelInfo {
            id: self.channel_id.clone().unwrap_or_default(),
            name: self.channel_name.clone(),
            ..Default::default()
        });

        if let Some(banner) = &info.banner_url {
            let bw = ui.available_width();
            let bh = (bw * 0.145).clamp(72.0, 140.0);
            let (brect, _) = ui.allocate_exact_size(Vec2::new(bw, bh), Sense::hover());
            theme::paint_image_rounded(ui, banner, brect, 12);
            ui.add_space(12.0);
        }

        ui.horizontal(|ui| {
            draw_avatar(ui, info.avatar_url.as_deref(), &info.name, 72.0);
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&info.name)
                        .font(theme::bold(22.0))
                        .color(Color32::WHITE),
                );
                let sub_line = match (info.handle.is_empty(), info.subscribers_text.is_empty()) {
                    (false, false) => format!("{} · {}", info.handle, info.subscribers_text),
                    (false, true) => info.handle.clone(),
                    (true, false) => info.subscribers_text.clone(),
                    (true, true) => String::new(),
                };
                if !sub_line.is_empty() {
                    ui.label(
                        RichText::new(sub_line)
                            .font(theme::regular(13.0))
                            .color(theme::TEXT_DIM),
                    );
                }
                ui.add_space(4.0);
                if let Some(cid) = self.channel_id.clone() {
                    ui.horizontal(|ui| {
                        super::cards::draw_subscribe_bell_button(
                            ui,
                            cid,
                            info.is_subscribed,
                            &info.notifications,
                            &self.backend.tx,
                        );
                        if self
                            .channel_items
                            .iter()
                            .any(|it| it.playlist_browse_id.is_none())
                        {
                            let items_snap = self.channel_items.clone();
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(format!("{}  Play All", icon::PLAY))
                                            .font(theme::medium(12.5))
                                            .color(theme::TEXT),
                                    )
                                    .fill(theme::CHIP)
                                    .corner_radius(18),
                                )
                                .clicked()
                            {
                                self.play_all(&items_snap, false, None);
                            }
                            let (sh_rect, sh_resp) = ui.allocate_exact_size(
                                Vec2::new(88.0, 30.0),
                                Sense::click(),
                            );
                            if sh_resp.hovered() {
                                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                            }
                            ui.painter().rect_filled(
                                sh_rect,
                                15.0,
                                if sh_resp.hovered() {
                                    theme::CHIP_HOVER
                                } else {
                                    theme::CHIP
                                },
                            );
                            theme::draw_shuffle_icon(
                                ui.painter(),
                                pos2(sh_rect.left() + 17.0, sh_rect.center().y),
                                14.0,
                                theme::TEXT,
                            );
                            ui.painter().text(
                                pos2(sh_rect.left() + 30.0, sh_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                "Shuffle",
                                theme::medium(12.5),
                                theme::TEXT,
                            );
                            if sh_resp.clicked() {
                                self.play_all(&items_snap, true, None);
                            }
                        }
                    });
                }
            });
        });

        ui.add_space(10.0);
        let tabs = if !info.tabs.is_empty() {
            info.tabs
        } else {
            vec![
                crate::model::ChannelTab {
                    title: "Home".to_owned(),
                    params: Some("EghmZWF0dXJlZPIGBAoCMgA%3D".to_owned()),
                    selected: false,
                },
                crate::model::ChannelTab {
                    title: "Videos".to_owned(),
                    params: Some("EgZ2aWRlb3PyBgQKAjoA".to_owned()),
                    selected: self.channel_tab_params.is_none(),
                },
                crate::model::ChannelTab {
                    title: "Shorts".to_owned(),
                    params: Some("EgZzaG9ydHPyBgUKA5oBAA%3D%3D".to_owned()),
                    selected: false,
                },
                crate::model::ChannelTab {
                    title: "Live".to_owned(),
                    params: Some("EgdzdHJlYW1z8gYECgJ6AA%3D%3D".to_owned()),
                    selected: false,
                },
                crate::model::ChannelTab {
                    title: "Playlists".to_owned(),
                    params: Some("EglwbGF5bGlzdHPyBgQKAkIA".to_owned()),
                    selected: false,
                },
            ]
        };
        let active_tab_title = tabs
            .iter()
            .find(|t| match (&self.channel_tab_params, &t.params) {
                (Some(cur), Some(p)) => cur == p,
                (None, _) => t.selected,
                _ => false,
            })
            .map(|t| t.title.clone())
            .unwrap_or_default();
        ui.horizontal(|ui| {
            for tab in &tabs {
                let is_active = self.channel_search_active.is_empty()
                    && match (&self.channel_tab_params, &tab.params) {
                        (Some(cur), Some(p)) => cur == p,
                        (None, _) => tab.selected,
                        _ => false,
                    };
                let resp = ui.add(
                    egui::Button::new(
                        RichText::new(&tab.title)
                            .font(if is_active {
                                theme::semibold(14.0)
                            } else {
                                theme::medium(14.0)
                            })
                            .color(if is_active {
                                Color32::WHITE
                            } else {
                                theme::TEXT_DIM
                            }),
                    )
                    .fill(Color32::TRANSPARENT),
                );
                if is_active {
                    let r = resp.rect;
                    ui.painter().line_segment(
                        [
                            pos2(r.left() + 4.0, r.bottom()),
                            pos2(r.right() - 4.0, r.bottom()),
                        ],
                        Stroke::new(2.0, Color32::WHITE),
                    );
                }
                if resp.clicked() && !is_active {
                    if let Some(cid) = self.channel_id.clone() {
                        self.channel_search_active.clear();
                        self.channel_tab_params = tab.params.clone();
                        self.channel_items.clear();
                        self.channel_continuation = None;
                        self.channel_loading = true;
                        let _ = self.backend.tx.send(Request::LoadChannel {
                            channel_id: cid,
                            params: tab.params.clone(),
                            continuation: None,
                        });
                    }
                }
                ui.add_space(6.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.channel_search_active.is_empty()
                    && icon_circle_button(ui, icon::X, 13.0, "Clear channel search")
                {
                    self.channel_search_input.clear();
                    self.channel_search_active.clear();
                    if let Some(cid) = self.channel_id.clone() {
                        self.channel_items.clear();
                        self.channel_continuation = None;
                        self.channel_loading = true;
                        let _ = self.backend.tx.send(Request::LoadChannel {
                            channel_id: cid,
                            params: self.channel_tab_params.clone(),
                            continuation: None,
                        });
                    }
                }
                let search_clicked = icon_circle_button(ui, icon::SEARCH, 14.0, "Search channel");
                let te = ui.add_sized(
                    Vec2::new(190.0_f32.min(ui.available_width().max(90.0)), 28.0),
                    egui::TextEdit::singleline(&mut self.channel_search_input)
                        .hint_text("Search channel..."),
                );
                if search_clicked || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    let q = self.channel_search_input.trim().to_owned();
                    if !q.is_empty() {
                        if let Some(cid) = self.channel_id.clone() {
                            self.channel_search_active = q.clone();
                            self.channel_items.clear();
                            self.channel_continuation = None;
                            self.channel_loading = true;
                            let _ = self.backend.tx.send(Request::SearchChannel {
                                channel_id: cid,
                                query: q,
                                continuation: None,
                            });
                        }
                    }
                }
            });
        });
        ui.separator();
        ui.add_space(8.0);

        let items = self.channel_items.clone();
        let loading = self.channel_loading;
        let cont = self.channel_continuation.clone();
        let tab_params = self.channel_tab_params.clone();
        let search_active = self.channel_search_active.clone();
        let heading = if search_active.is_empty() {
            String::new()
        } else {
            format!("Results in channel for \"{search_active}\"")
        };
        let is_shorts_tab = search_active.is_empty() && active_tab_title.eq_ignore_ascii_case("Shorts");
        if let Some(act) = draw_video_grid(ui, &heading, &items, loading, cont.is_some()) {
            if is_shorts_tab {
                if let super::cards::GridAction::Open(ref it) = act {
                    if it.playlist_browse_id.is_none() {
                        self.open_short(it.clone(), items);
                        return;
                    }
                }
            }
            self.handle_grid_action(ctx, act, |app| {
                if let Some(cid) = app.channel_id.clone() {
                    app.channel_loading = true;
                    if search_active.is_empty() {
                        let _ = app.backend.tx.send(Request::LoadChannel {
                            channel_id: cid,
                            params: tab_params,
                            continuation: cont,
                        });
                    } else {
                        let _ = app.backend.tx.send(Request::SearchChannel {
                            channel_id: cid,
                            query: search_active,
                            continuation: cont,
                        });
                    }
                }
            });
        }
    }

    pub(super) fn draw_settings_page(&mut self, ui: &mut egui::Ui) {
        let total_w = ui.available_width();
        let wide = total_w >= 760.0;

        if wide {
            let rail_w = 224.0_f32;
            let total_h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(rail_w, total_h),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        self.draw_settings_nav(ui, true);
                    },
                );
                ui.add_space(24.0);
                let content_w = (ui.available_width() - 8.0).max(320.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(content_w, total_h),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        ScrollArea::vertical()
                            .id_salt("settings_content_scroll")
                            .show(ui, |ui| {
                                ui.set_max_width(720.0);
                                self.draw_settings_groups(ui);
                                ui.add_space(28.0);
                            });
                    },
                );
            });
        } else {
            self.draw_settings_nav(ui, false);
            ui.add_space(12.0);
            ScrollArea::vertical()
                .id_salt("settings_content_scroll_narrow")
                .show(ui, |ui| {
                    ui.set_max_width(720.0);
                    self.draw_settings_groups(ui);
                    ui.add_space(24.0);
                });
        }
    }

    fn draw_settings_nav(&mut self, ui: &mut egui::Ui, vertical: bool) {
        ui.add_space(4.0);
        ui.label(
            RichText::new("Settings")
                .font(theme::bold(28.0))
                .color(Color32::WHITE),
        );
        ui.add_space(12.0);

        // Search input pill
        let search_h = 36.0;
        let search_w = if vertical {
            ui.available_width()
        } else {
            ui.available_width().min(360.0)
        };
        let (srect, sresp) = ui.allocate_exact_size(Vec2::new(search_w, search_h), Sense::click());
        let search_fill = if theme::is_translucent() {
            Color32::from_rgba_unmultiplied(255, 255, 255, 18)
        } else {
            theme::SURFACE
        };
        ui.painter()
            .rect_filled(srect, search_h * 0.5, search_fill);
        ui.painter().rect_stroke(
            srect,
            search_h * 0.5,
            Stroke::new(1.0, theme::card_stroke()),
            egui::StrokeKind::Middle,
        );
        theme::draw_icon_centered(
            ui.painter(),
            pos2(srect.left() + 17.0, srect.center().y),
            icon::SEARCH,
            14.0,
            theme::TEXT_DIM,
        );
        let clearable = !self.settings_query.is_empty();
        let inner = Rect::from_min_max(
            pos2(srect.left() + 33.0, srect.center().y - 10.0),
            pos2(
                srect.right() - if clearable { 32.0 } else { 12.0 },
                srect.center().y + 10.0,
            ),
        );
        let mut input_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let edit = input_ui.add_sized(
            inner.size(),
            egui::TextEdit::singleline(&mut self.settings_query)
                .hint_text(
                    RichText::new("Search settings")
                        .font(theme::regular(13.0))
                        .color(theme::TEXT_FAINT),
                )
                .font(theme::regular(13.0))
                .frame(Frame::NONE)
                .align(egui::Align2::LEFT_CENTER)
                .desired_width(inner.width()),
        );
        if clearable {
            let x_rect = Rect::from_center_size(
                pos2(srect.right() - 17.0, srect.center().y),
                Vec2::splat(24.0),
            );
            let x_resp = ui
                .interact(x_rect, egui::Id::unique("settings_search_clear"), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            if x_resp.hovered() {
                ui.painter().circle_filled(
                    x_rect.center(),
                    11.0,
                    Color32::from_white_alpha(20),
                );
            }
            theme::draw_icon_centered(
                ui.painter(),
                x_rect.center(),
                icon::X,
                13.0,
                theme::TEXT_DIM,
            );
            if x_resp.clicked() {
                self.settings_query.clear();
                edit.request_focus();
            }
        }
        if sresp.clicked() {
            edit.request_focus();
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_query.clear();
        }

        // Per-group vs all-groups search toggle
        if !self.settings_query.trim().is_empty() {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (in_group, lbl) in [
                    (false, "All groups"),
                    (true, self.settings_tab.label()),
                ] {
                    let active = self.settings_search_in_group == in_group;
                    let fill = if active {
                        theme::YT_BLUE.gamma_multiply(0.24)
                    } else if theme::is_translucent() {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                    } else {
                        theme::CHIP
                    };
                    let col = if active { Color32::WHITE } else { theme::TEXT_DIM };
                    let btn = egui::Button::new(
                        RichText::new(lbl).font(theme::medium(11.5)).color(col),
                    )
                    .fill(fill)
                    .stroke(Stroke::new(
                        1.0,
                        if active {
                            theme::YT_BLUE.gamma_multiply(0.65)
                        } else {
                            theme::card_stroke()
                        },
                    ))
                    .corner_radius(12);
                    if ui.add(btn).clicked() {
                        self.settings_search_in_group = in_group;
                    }
                }
            });
        }

        ui.add_space(12.0);
        let filtering = !self.settings_query.trim().is_empty();
        if vertical {
            for tab in SettingsTab::ALL {
                let selected = (!filtering || self.settings_search_in_group) && self.settings_tab == tab;
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
                let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                let hover_t = ui.ctx().animate_bool_with_time(
                    egui::Id::unique(("settings_nav_h", tab as usize)),
                    resp.hovered(),
                    0.14,
                );
                let sel_t = ui.ctx().animate_bool_with_time(
                    egui::Id::unique(("settings_nav_s", tab as usize)),
                    selected,
                    0.16,
                );
                if hover_t + sel_t > 0.01 {
                    let alpha = (14.0 * hover_t + 26.0 * sel_t).round() as u8;
                    ui.painter()
                        .rect_filled(rect, 10.0, Color32::from_white_alpha(alpha));
                }
                let text_col = if selected || resp.hovered() {
                    Color32::WHITE
                } else {
                    theme::TEXT_DIM
                };
                ui.painter().text(
                    pos2(rect.left() + 14.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    tab.label(),
                    if selected {
                        theme::semibold(13.5)
                    } else {
                        theme::medium(13.5)
                    },
                    text_col,
                );
                if resp.clicked() {
                    self.settings_tab = tab;
                    if filtering && !self.settings_search_in_group {
                        self.settings_query.clear();
                    }
                }
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for tab in SettingsTab::ALL {
                    let selected = (!filtering || self.settings_search_in_group) && self.settings_tab == tab;
                    let fill = if selected {
                        Color32::from_white_alpha(32)
                    } else if theme::is_translucent() {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 12)
                    } else {
                        theme::CHIP
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(tab.label())
                                    .font(theme::medium(12.5))
                                    .color(if selected {
                                        Color32::WHITE
                                    } else {
                                        theme::TEXT_DIM
                                    }),
                            )
                            .fill(fill)
                            .corner_radius(14),
                        )
                        .clicked()
                    {
                        self.settings_tab = tab;
                        if filtering && !self.settings_search_in_group {
                            self.settings_query.clear();
                        }
                    }
                }
            });
        }
    }

    fn draw_settings_groups(&mut self, ui: &mut egui::Ui) {
        let raw_query = self.settings_query.trim().to_ascii_lowercase();
        let words: Vec<String> = raw_query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect();
        let filtering = !words.is_empty();

        let tabs_to_render: Vec<SettingsTab> = if !filtering || self.settings_search_in_group {
            vec![self.settings_tab]
        } else {
            SettingsTab::ALL.to_vec()
        };

        let mut shown_groups = 0usize;
        for tab in tabs_to_render {
            let group_name = tab.label();
            let rendered = match tab {
                SettingsTab::Account => self.draw_settings_account_group(ui, group_name, &words),
                SettingsTab::Playback => self.draw_settings_playback_group(ui, group_name, &words),
                SettingsTab::Appearance => self.draw_settings_appearance_group(ui, group_name, &words),
                SettingsTab::Engine => self.draw_settings_engine_group(ui, group_name, &words),
                SettingsTab::Connections => self.draw_settings_connections_group(ui, group_name, &words),
                SettingsTab::Shortcuts => self.draw_settings_shortcuts_group(ui, group_name, &words),
            };
            if rendered {
                shown_groups += 1;
                ui.add_space(18.0);
            }
        }

        if filtering && shown_groups == 0 {
            settings_group_frame(ui, "No matching settings", None, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "No settings found matching \"{}\"{}.",
                            self.settings_query.trim(),
                            if self.settings_search_in_group {
                                format!(" in {}", self.settings_tab.label())
                            } else {
                                String::new()
                            }
                        ))
                        .font(theme::regular(13.0))
                        .color(theme::TEXT_DIM),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Clear search").clicked() {
                            self.settings_query.clear();
                        }
                    });
                });
            });
        }
    }

    fn draw_settings_account_group(
        &mut self,
        ui: &mut egui::Ui,
        group: &str,
        words: &[String],
    ) -> bool {
        let mut any = false;
        let show_auth = settings_match(
            words,
            group,
            "Google account sign in sign out YouTube feed subscriptions",
        );
        let show_sync =
            self.signed_in && settings_match(words, group, "Sync watch history playback progress");
        if !show_auth && !show_sync {
            return false;
        }
        settings_group_frame(ui, group, None, |ui| {
            if show_auth {
                any = true;
                if self.signed_in {
                    let name = self
                        .account_name
                        .as_deref()
                        .unwrap_or("Google Account")
                        .to_owned();
                    settings_row(
                        ui,
                        &name,
                        Some("Signed in with Google. Home feed, subscriptions, playlists and likes come from this account."),
                        |ui| {
                            if settings_pill_btn(ui, "Sign out", Some(icon::LOG_OUT)).clicked() {
                                let _ = self.backend.tx.send(Request::SignOut);
                            }
                        },
                    );
                } else {
                    settings_row(
                        ui,
                        "Not signed in",
                        Some("Sign in with Google to access your Home feed, Subscriptions and synced history."),
                        |ui| {
                            if settings_pill_btn(ui, "Sign in", Some(icon::CIRCLE_USER)).clicked() {
                                self.start_sign_in();
                            }
                        },
                    );
                }
            }
            if show_sync {
                if any {
                    settings_divider(ui);
                }
                any = true;
                settings_row(
                    ui,
                    "Sync watch history",
                    Some("Report playback progress and watch history back to YouTube."),
                    |ui| {
                        settings_toggle(ui, &mut self.sync_history, "sync_history");
                    },
                );
            }
        });
        any
    }

    fn draw_settings_playback_group(
        &mut self,
        ui: &mut egui::Ui,
        group: &str,
        words: &[String],
    ) -> bool {
        let m_quality = settings_match(words, group, "Preferred max quality resolution 4k 1080p 720p");
        let m_autoplay = settings_match(words, group, "Autoplay next Up Next video");
        let m_resume = settings_match(words, group, "Auto-resume last watched video tabs startup");
        let m_pot = settings_match(
            words,
            group,
            "BotGuard PO Token auto on off yt-dlp blocked 403 webview",
        );
        if !m_quality && !m_autoplay && !m_resume && !m_pot {
            return false;
        }
        settings_group_frame(ui, group, None, |ui| {
            let mut count = 0usize;
            if m_quality {
                count += 1;
                settings_row(
                    ui,
                    "Preferred max quality",
                    Some("Target resolution chosen when loading or recovering streams."),
                    |ui| {
                        egui::ComboBox::from_id_salt("settings_quality")
                            .selected_text(self.preferred_quality.label())
                            .show_ui(ui, |ui| {
                                for q in PreferredQuality::ALL {
                                    ui.selectable_value(&mut self.preferred_quality, q, q.label());
                                }
                            });
                    },
                );
            }
            if m_autoplay {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(
                    ui,
                    "Autoplay next video",
                    Some("Automatically play the next queued or Up Next video when playback ends."),
                    |ui| {
                        settings_toggle(ui, &mut self.autoplay_next, "autoplay_next");
                    },
                );
            }
            if m_resume {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(
                    ui,
                    "Auto-resume on startup",
                    Some("Reopen your last watched tab(s) and timestamps automatically when yt-rust starts."),
                    |ui| {
                        settings_toggle(
                            ui,
                            &mut self.auto_resume_on_start,
                            "auto_resume_on_start",
                        );
                    },
                );
            }
            if m_pot {
                if count > 0 {
                    settings_divider(ui);
                }
                let prev_pot = self.po_token_mode;
                settings_row(
                    ui,
                    "BotGuard PO Token",
                    Some("Mint Proof-of-Origin tokens on demand via hidden WebView2 when yt-dlp fallback hits bot checks or 403."),
                    |ui| {
                        egui::ComboBox::from_id_salt("settings_potoken_mode")
                            .width(190.0)
                            .selected_text(self.po_token_mode.label())
                            .show_ui(ui, |ui| {
                                for mode in PoTokenMode::ALL {
                                    ui.selectable_value(
                                        &mut self.po_token_mode,
                                        mode,
                                        mode.label(),
                                    );
                                }
                            });
                    },
                );
                if self.po_token_mode != prev_pot {
                    let _ = self
                        .backend
                        .tx
                        .send(Request::SetPoTokenMode(self.po_token_mode));
                    self.toast(format!("BotGuard PO Token: {}", self.po_token_mode.label()));
                }
            }
        });
        true
    }

    fn draw_settings_appearance_group(
        &mut self,
        ui: &mut egui::Ui,
        group: &str,
        words: &[String],
    ) -> bool {
        let backdrop_supported = crate::win_shell::windows_backdrop_supported();
        let m_mat = backdrop_supported
            && settings_match(words, group, "Window material theme acrylic mica solid dark glass");
        let m_amb = settings_match(
            words,
            group,
            "Ambient mode background video blur soft tiled repeat wallpaper",
        );
        let m_amb_op = self.ambient_mode != AmbientMode::Off
            && settings_match(words, group, "Ambient background strength opacity intensity");
        let m_glow = settings_match(
            words,
            group,
            "Ambient cinema glow behind video player watch",
        );
        let m_sub_sz = settings_match(words, group, "Caption font size subtitles");
        let m_sub_op = settings_match(words, group, "Caption background opacity subtitles");
        let m_aot = settings_match(words, group, "Always on Top window pin");
        let m_close_tray = settings_match(words, group, "Close button system tray");
        let m_min_tray = settings_match(words, group, "Minimize button system tray");

        if !m_mat
            && !m_amb
            && !m_amb_op
            && !m_glow
            && !m_sub_sz
            && !m_sub_op
            && !m_aot
            && !m_close_tray
            && !m_min_tray
        {
            return false;
        }

        settings_group_frame(ui, group, None, |ui| {
            let mut count = 0usize;
            if m_mat {
                count += 1;
                let prev = self.window_material;
                settings_row(
                    ui,
                    "Window material",
                    Some("Windows 11 translucent backdrop style (Acrylic / Mica) applied live."),
                    |ui| {
                        egui::ComboBox::from_id_salt("settings_window_material")
                            .width(155.0)
                            .selected_text(self.window_material.label())
                            .show_ui(ui, |ui| {
                                for m in WindowMaterial::ALL {
                                    ui.selectable_value(&mut self.window_material, m, m.label());
                                }
                            });
                    },
                );
                if self.window_material != prev {
                    self.toast(format!("Theme material: {}", self.window_material.label()));
                }
            }
            if m_amb {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(
                    ui,
                    "Ambient background",
                    Some("Softly blurred or tiled video artwork behind pages, similar to ytm-rs."),
                    |ui| {
                        egui::ComboBox::from_id_salt("settings_ambient_mode")
                            .width(155.0)
                            .selected_text(self.ambient_mode.label())
                            .show_ui(ui, |ui| {
                                for mode in AmbientMode::ALL {
                                    ui.selectable_value(&mut self.ambient_mode, mode, mode.label());
                                }
                            });
                    },
                );
            }
            if m_amb_op {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(
                    ui,
                    "Ambient intensity",
                    Some("How subtly the blurred artwork blends into the app backdrop."),
                    |ui| {
                        let mut pct = (self.ambient_opacity * 100.0).round() as u32;
                        ui.label(
                            RichText::new(format!("{pct}%"))
                                .font(theme::medium(12.5))
                                .color(theme::TEXT_DIM),
                        );
                        if ui
                            .add(egui::Slider::new(&mut pct, 5..=70).show_value(false))
                            .changed()
                        {
                            self.ambient_opacity = pct as f32 / 100.0;
                        }
                    },
                );
            }
            if m_glow {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(
                    ui,
                    "Player ambient cinema glow",
                    Some("Soft glowing halo behind the active video surface on the Watch page."),
                    |ui| {
                        settings_toggle(ui, &mut self.ambient_glow, "ambient_glow");
                    },
                );
            }
            let mut sub_changed = false;
            if m_sub_sz {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(ui, "Caption font size", None, |ui| {
                    ui.label(
                        RichText::new(format!("{} px", self.sub_font_size))
                            .font(theme::medium(12.5))
                            .color(theme::TEXT_DIM),
                    );
                    if ui
                        .add(egui::Slider::new(&mut self.sub_font_size, 24..=80).show_value(false))
                        .changed()
                    {
                        sub_changed = true;
                    }
                });
            }
            if m_sub_op {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(ui, "Caption background opacity", None, |ui| {
                    let mut pct = (self.sub_bg_opacity * 100.0).round() as u32;
                    ui.label(
                        RichText::new(format!("{pct}%"))
                            .font(theme::medium(12.5))
                            .color(theme::TEXT_DIM),
                    );
                    if ui
                        .add(egui::Slider::new(&mut pct, 0..=100).show_value(false))
                        .changed()
                    {
                        self.sub_bg_opacity = pct as f32 / 100.0;
                        sub_changed = true;
                    }
                });
            }
            if sub_changed {
                if let Some(mpv) = &self.mpv {
                    mpv.set_subtitle_style(self.sub_font_size, self.sub_bg_opacity);
                }
            }
            if m_aot {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                let mut aot = self.always_on_top;
                let ctx = ui.ctx().clone();
                settings_row(
                    ui,
                    "Keep window Always on Top",
                    Some("Pin the main yt-rust window above all other windows."),
                    |ui| {
                        if settings_toggle(ui, &mut aot, "always_on_top") {
                            self.set_always_on_top(&ctx, aot);
                        }
                    },
                );
            }
            if m_close_tray {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                settings_row(
                    ui,
                    "Close to system tray",
                    Some("Close button hides window to tray (use tray menu -> Quit to exit)."),
                    |ui| {
                        settings_toggle(ui, &mut self.close_to_tray, "close_to_tray");
                    },
                );
            }
            if m_min_tray {
                if count > 0 {
                    settings_divider(ui);
                }
                settings_row(
                    ui,
                    "Minimize to system tray",
                    Some("Minimize button sends window to the Windows notification area."),
                    |ui| {
                        settings_toggle(ui, &mut self.minimize_to_tray, "minimize_to_tray");
                    },
                );
            }
        });
        true
    }

    fn draw_settings_engine_group(
        &mut self,
        ui: &mut egui::Ui,
        group: &str,
        words: &[String],
    ) -> bool {
        let m_dl = settings_match(words, group, "Download folder offline directory open reset");
        let m_hw = settings_match(
            words,
            group,
            "Hardware decoding hwdec libmpv video engine auto-copy software",
        );
        let m_mpv = settings_match(words, group, "libmpv-2.dll status download video engine");
        if !m_dl && !m_hw && !m_mpv {
            return false;
        }
        settings_group_frame(ui, group, None, |ui| {
            let mut count = 0usize;
            if m_dl {
                count += 1;
                let default_dir = ytdlp::default_downloads_dir(&self.backend.data_dir);
                let active_dir = self.effective_download_dir();
                ui.label(
                    RichText::new("Download folder")
                        .font(theme::medium(14.0))
                        .color(Color32::WHITE),
                );
                ui.label(
                    RichText::new("Leave empty to use the default app downloads directory.")
                        .font(theme::regular(12.0))
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let edit_w = (ui.available_width() - 210.0).max(160.0);
                    ui.add_sized(
                        Vec2::new(edit_w, 30.0),
                        egui::TextEdit::singleline(&mut self.download_dir)
                            .hint_text(default_dir.display().to_string()),
                    );
                    if !self.download_dir.trim().is_empty() && ui.button("Reset").clicked() {
                        self.download_dir.clear();
                    }
                    if settings_pill_btn(ui, "Open folder", Some(icon::FOLDER_OPEN)).clicked() {
                        let _ = std::fs::create_dir_all(&active_dir);
                        #[cfg(windows)]
                        let _ = std::process::Command::new("explorer").arg(&active_dir).spawn();
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(&active_dir).spawn();
                        #[cfg(all(unix, not(target_os = "macos")))]
                        let _ = std::process::Command::new("xdg-open").arg(&active_dir).spawn();
                    }
                });
            }
            if m_hw {
                if count > 0 {
                    settings_divider(ui);
                }
                count += 1;
                let prev = self.hwdec_mode;
                settings_row(
                    ui,
                    "Hardware decoding (hwdec)",
                    Some("Use GPU decoding when supported or fall back to CPU stability."),
                    |ui| {
                        egui::ComboBox::from_id_salt("settings_hwdec")
                            .width(260.0)
                            .selected_text(self.hwdec_mode.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.hwdec_mode,
                                    HwdecMode::AutoCopy,
                                    HwdecMode::AutoCopy.label(),
                                );
                                ui.selectable_value(
                                    &mut self.hwdec_mode,
                                    HwdecMode::Software,
                                    HwdecMode::Software.label(),
                                );
                            });
                    },
                );
                if self.hwdec_mode != prev {
                    if let Some(mpv) = &self.mpv {
                        mpv.set_hwdec(self.hwdec_mode);
                    }
                    self.toast(format!("hwdec: {}", self.hwdec_mode.mpv_value()));
                }
            }
            if m_mpv {
                if count > 0 {
                    settings_divider(ui);
                }
                let status_desc = if let Some(mpv) = &self.mpv {
                    format!("Active: {}", mpv.dll_path.display())
                } else {
                    self.mpv_error
                        .as_deref()
                        .unwrap_or("libmpv-2.dll is not installed yet.")
                        .to_owned()
                };
                settings_row(ui, "libmpv Engine", Some(&status_desc), |ui| {
                    if self.mpv.is_some() {
                        ui.colored_label(Color32::from_rgb(110, 215, 135), "Ready");
                    } else if self.downloading_mpv {
                        ui.spinner();
                    } else if settings_pill_btn(ui, "Download libmpv-2.dll", Some(icon::DOWNLOAD))
                        .clicked()
                    {
                        self.downloading_mpv = true;
                        let _ = self.backend.tx.send(Request::DownloadLibmpv);
                    }
                });
            }
        });
        true
    }

    fn draw_settings_connections_group(
        &mut self,
        ui: &mut egui::Ui,
        group: &str,
        words: &[String],
    ) -> bool {
        let m_sb = settings_match(
            words,
            group,
            "SponsorBlock auto-skip sponsor intro outro interaction",
        );
        let m_da = settings_match(
            words,
            group,
            "DeArrow clickbait titles thumbnails branding",
        );
        let m_ryd = settings_match(words, group, "Return YouTube Dislike counts RYD");
        if !m_sb && !m_da && !m_ryd {
            return false;
        }
        settings_group_frame(
            ui,
            group,
            Some("Optional privacy-friendly third-party enhancements."),
            |ui| {
                let mut count = 0usize;
                if m_sb {
                    count += 1;
                    settings_row(
                        ui,
                        "Auto-skip SponsorBlock segments",
                        Some("Skip sponsored segments, intros, outros, and reminders automatically."),
                        |ui| {
                            settings_toggle(ui, &mut self.sponsorblock_enabled, "sb_enabled");
                        },
                    );
                    if self.sponsorblock_enabled {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                            for &cat in crate::model::SponsorCategory::ALL {
                                let mut on = self.sponsorblock_categories.contains(&cat);
                                if ui.checkbox(&mut on, cat.label()).changed() {
                                    if on {
                                        self.sponsorblock_categories.insert(cat);
                                    } else {
                                        self.sponsorblock_categories.remove(&cat);
                                    }
                                }
                            }
                        });
                    }
                }
                if m_da {
                    if count > 0 {
                        settings_divider(ui);
                    }
                    count += 1;
                    let mut changed = false;
                    settings_row(
                        ui,
                        "DeArrow",
                        Some("Replace clickbait video titles & thumbnails via crowd-sourced branding."),
                        |ui| {
                            changed = settings_toggle(ui, &mut self.dearrow_enabled, "dearrow");
                        },
                    );
                    if changed && self.dearrow_enabled {
                        if let Some(w) = &self.watch {
                            if !w.id.starts_with("local:") {
                                let _ = self.backend.tx.send(Request::LoadCommunityMeta {
                                    video_id: w.id.clone(),
                                    dearrow: true,
                                    ryd: false,
                                });
                            }
                        }
                    }
                }
                if m_ryd {
                    if count > 0 {
                        settings_divider(ui);
                    }
                    let mut changed = false;
                    settings_row(
                        ui,
                        "Return YouTube Dislike",
                        Some("Show estimated dislike counts beside the Dislike button."),
                        |ui| {
                            changed = settings_toggle(ui, &mut self.ryd_enabled, "ryd");
                        },
                    );
                    if changed {
                        if self.ryd_enabled {
                            if let Some(w) = &self.watch {
                                if !w.id.starts_with("local:") {
                                    let _ = self.backend.tx.send(Request::LoadCommunityMeta {
                                        video_id: w.id.clone(),
                                        dearrow: false,
                                        ryd: true,
                                    });
                                }
                            }
                        } else if let Some(w) = &mut self.watch {
                            w.dislikes_text = None;
                        }
                    }
                }
            },
        );
        true
    }

    fn draw_settings_shortcuts_group(
        &mut self,
        ui: &mut egui::Ui,
        group: &str,
        words: &[String],
    ) -> bool {
        let slots: Vec<crate::model::ShortcutSlot> = crate::model::ShortcutSlot::ALL
            .iter()
            .copied()
            .filter(|slot| {
                let hay = format!(
                    "{} {}",
                    slot.label(),
                    self.shortcuts.get(*slot).display()
                );
                settings_match(words, group, &hay)
            })
            .collect();
        if slots.is_empty() {
            return false;
        }
        settings_group_frame(
            ui,
            group,
            Some("Click any shortcut pill and press a key combination (Esc cancels)."),
            |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if settings_pill_btn(ui, "Reset defaults", None).clicked() {
                            self.shortcuts = crate::model::ShortcutConfig::default();
                            self.rebinding_shortcut = None;
                            self.toast("Keyboard shortcuts reset to defaults");
                        }
                    });
                });
                ui.add_space(6.0);
                for (idx, slot) in slots.into_iter().enumerate() {
                    if idx > 0 {
                        settings_divider(ui);
                    }
                    let listening = self.rebinding_shortcut == Some(slot);
                    settings_row(ui, slot.label(), None, |ui| {
                        let label = if listening {
                            "Press key...".to_owned()
                        } else {
                            self.shortcuts.get(slot).display()
                        };
                        let btn = egui::Button::new(
                            RichText::new(label)
                                .font(theme::medium(12.5))
                                .color(if listening {
                                    Color32::WHITE
                                } else {
                                    theme::TEXT
                                }),
                        )
                        .min_size(Vec2::new(112.0, 28.0))
                        .fill(if listening {
                            theme::YT_BLUE
                        } else if theme::is_translucent() {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 22)
                        } else {
                            theme::CHIP
                        })
                        .corner_radius(8);
                        if ui.add(btn).clicked() {
                            self.rebinding_shortcut = if listening { None } else { Some(slot) };
                        }
                    });
                }
            },
        );
        true
    }

    pub(super) fn draw_downloads_page(&mut self, ui: &mut egui::Ui) {
        let dir = self.effective_download_dir();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Downloads").font(theme::bold(20.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("{}  Open folder", icon::FOLDER_OPEN))
                                .font(theme::medium(12.5)),
                        )
                        .fill(theme::CHIP)
                        .corner_radius(18),
                    )
                    .clicked()
                {
                    let _ = std::fs::create_dir_all(&dir);
                    #[cfg(windows)]
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&dir).spawn();
                    #[cfg(all(unix, not(target_os = "macos")))]
                    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                }
            });
        });
        ui.label(
            RichText::new(dir.display().to_string())
                .font(theme::regular(12.0))
                .color(theme::TEXT_DIM),
        );
        ui.add_space(12.0);

        if !self.active_downloads.is_empty() {
            ui.label(
                RichText::new(format!("Active Downloads ({})", self.active_downloads.len()))
                    .font(theme::semibold(15.0))
                    .color(Color32::WHITE),
            );
            ui.add_space(6.0);
            for dl in &self.active_downloads {
                let kind = if dl.audio_only { "M4A Audio" } else { "MP4 Video" };
                let ico = if dl.audio_only {
                    icon::HEADPHONES
                } else {
                    icon::DOWNLOAD
                };
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            theme::draw_icon(ui, ico, 18.0, theme::YT_BLUE);
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((ui.available_width() - 95.0).max(140.0));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&dl.title)
                                            .font(theme::medium(13.5))
                                            .color(Color32::WHITE),
                                    )
                                    .truncate(),
                                );
                                let mut meta = vec![kind.to_owned(), dl.status.clone()];
                                if !dl.speed.is_empty() {
                                    meta.push(dl.speed.clone());
                                }
                                if !dl.total_size.is_empty() {
                                    meta.push(dl.total_size.clone());
                                }
                                ui.label(
                                    RichText::new(meta.join(" · "))
                                        .font(theme::regular(11.5))
                                        .color(theme::TEXT_DIM),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{:.0}%", dl.percent * 100.0))
                                        .font(theme::semibold(13.0))
                                        .color(theme::YT_BLUE),
                                );
                            });
                        });
                        ui.add_space(6.0);
                        let (bar_rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 5.0),
                            Sense::hover(),
                        );
                        ui.painter().rect_filled(bar_rect, 2.5, theme::CHIP);
                        let fill_w = (bar_rect.width() * dl.percent.clamp(0.02, 1.0)).max(6.0);
                        let fill_rect = Rect::from_min_size(
                            bar_rect.min,
                            Vec2::new(fill_w, bar_rect.height()),
                        );
                        ui.painter().rect_filled(fill_rect, 2.5, theme::YT_BLUE);
                    });
                ui.add_space(6.0);
            }
            ui.add_space(8.0);
        }

        let mut entries: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if !p.is_file() {
                    continue;
                }
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(ext.as_str(), "mp4" | "m4a" | "webm" | "mkv" | "mp3" | "opus") {
                    if let Ok(meta) = ent.metadata() {
                        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                        entries.push((p, meta.len(), mtime));
                    }
                }
            }
        }
        entries.sort_by(|a, b| b.2.cmp(&a.2));

        if entries.is_empty() && self.active_downloads.is_empty() {
            Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(12)
                .inner_margin(Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        theme::draw_icon(ui, icon::DOWNLOAD, 30.0, theme::TEXT_DIM);
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("No offline downloads yet")
                                .font(theme::semibold(15.0))
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new("Download any video or audio to watch offline directly in yt-rust.")
                                .font(theme::regular(12.5))
                                .color(theme::TEXT_DIM),
                        );
                    });
                });
            return;
        }

        let mut play_path: Option<std::path::PathBuf> = None;
        let mut delete_path: Option<std::path::PathBuf> = None;
        ScrollArea::vertical().id_salt("downloads_scroll").show(ui, |ui| {
            for (path, size_bytes, _) in &entries {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Media");
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_uppercase();
                let mb = *size_bytes as f64 / (1024.0 * 1024.0);
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let ico = if ext == "M4A" || ext == "MP3" || ext == "OPUS" {
                                icon::HEADPHONES
                            } else {
                                icon::PLAY
                            };
                            theme::draw_icon(ui, ico, 18.0, theme::YT_BLUE);
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((ui.available_width() - 165.0).max(120.0));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(name)
                                            .font(theme::medium(13.5))
                                            .color(Color32::WHITE),
                                    )
                                    .truncate(),
                                );
                                ui.label(
                                    RichText::new(format!("{ext} · {mb:.1} MB"))
                                        .font(theme::regular(11.5))
                                        .color(theme::TEXT_DIM),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(icon::TRASH_2.to_string())
                                                .font(theme::icon_font(14.0))
                                                .color(theme::TEXT_DIM),
                                        )
                                        .fill(theme::CHIP)
                                        .corner_radius(16),
                                    )
                                    .on_hover_text("Delete file")
                                    .clicked()
                                {
                                    delete_path = Some(path.clone());
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(format!("{}  Play", icon::PLAY))
                                                .font(theme::semibold(12.5))
                                                .color(Color32::from_rgb(15, 15, 15)),
                                        )
                                        .fill(Color32::from_rgb(241, 241, 241))
                                        .corner_radius(16),
                                    )
                                    .clicked()
                                {
                                    play_path = Some(path.clone());
                                }
                            });
                        });
                    });
                ui.add_space(6.0);
            }
        });

        if let Some(p) = play_path {
            self.open_local_file(&p);
        } else if let Some(p) = delete_path {
            match std::fs::remove_file(&p) {
                Ok(()) => self.toast("Deleted downloaded file"),
                Err(e) => self.last_error = Some(format!("Failed to delete file: {e}")),
            }
        }
    }

    pub(super) fn draw_playlists_library_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Playlists").font(theme::bold(22.0)));
            ui.add_space(8.0);
            if !self.guide_playlists.is_empty() {
                ui.label(
                    RichText::new(format!("({})", self.guide_playlists.len()))
                        .font(theme::medium(14.0))
                        .color(theme::TEXT_DIM),
                );
            }
        });
        ui.add_space(10.0);

        // Quick Library shortcuts (Watch Later & Liked Videos)
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(format!("{}  Watch Later", icon::LIBRARY))
                            .font(theme::semibold(13.0)),
                    )
                    .fill(theme::CHIP)
                    .corner_radius(18),
                )
                .clicked()
            {
                if self.signed_in {
                    self.open_playlist("VLWL".to_owned(), "Watch Later".to_owned());
                } else {
                    self.page = Page::Saved;
                }
            }
            if self.signed_in
                && ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("{}  Liked Videos", icon::HEART))
                                .font(theme::semibold(13.0)),
                        )
                        .fill(theme::CHIP)
                        .corner_radius(18),
                    )
                    .clicked()
            {
                self.open_playlist("VLLL".to_owned(), "Liked Videos".to_owned());
            }
        });
        ui.add_space(14.0);

        if !self.signed_in {
            ui.label(
                RichText::new("Sign in with Google to see your YouTube playlists.")
                    .font(theme::regular(13.5))
                    .color(theme::TEXT_DIM),
            );
            return;
        }
        if self.guide_playlists.is_empty() {
            ui.label(
                RichText::new("No playlists found yet.")
                    .font(theme::regular(13.5))
                    .color(theme::TEXT_DIM),
            );
            return;
        }

        let playlists = self.guide_playlists.clone();
        let mut open_pl: Option<(String, String)> = None;
        ScrollArea::vertical()
            .id_salt("playlists_library_scroll")
            .show(ui, |ui| {
                let avail = (ui.available_width() - 8.0).max(200.0);
                let cols = ((avail / 270.0).floor() as usize).clamp(1, 5);
                let spacing = 16.0;
                let card_w = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).max(170.0);
                let thumb_h = card_w * 9.0 / 16.0;

                for row in playlists.chunks(cols) {
                    ui.horizontal_top(|ui| {
                        for (col_idx, pl) in row.iter().enumerate() {
                            let card_h = thumb_h + 62.0;
                            let (card_rect, card_resp) =
                                ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                            let hovered =
                                card_resp.hovered() || ui.rect_contains_pointer(card_rect);
                            let hover_t = ui.ctx().animate_bool_with_time(
                                egui::Id::unique(("pl_card_hover", &pl.browse_id)),
                                hovered,
                                0.14,
                            );
                            if hovered {
                                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                            }
                            if hover_t > 0.01 {
                                ui.painter().rect_filled(
                                    card_rect.expand(5.0),
                                    13.0,
                                    Color32::from_rgba_unmultiplied(
                                        40,
                                        34,
                                        32,
                                        (210.0 * hover_t) as u8,
                                    ),
                                );
                            }

                            let mut card_ui =
                                ui.new_child(egui::UiBuilder::new().max_rect(card_rect));
                            let cclip = card_ui.clip_rect().intersect(card_rect);
                            card_ui.set_clip_rect(cclip);
                            card_ui.vertical(|ui| {
                                let (trect, _) = ui.allocate_exact_size(
                                    Vec2::new(card_w, thumb_h),
                                    Sense::hover(),
                                );
                                // Subtle stacked-deck accent bar above playlist card
                                let stack_bar = Rect::from_min_max(
                                    pos2(trect.left() + 10.0, trect.top() - 4.0),
                                    pos2(trect.right() - 10.0, trect.top()),
                                );
                                ui.painter().rect_filled(
                                    stack_bar,
                                    3.0,
                                    Color32::from_rgb(56, 56, 62),
                                );
                                if let Some(u) = &pl.thumbnail_url {
                                    theme::paint_image_rounded(ui, u, trect, 12);
                                } else {
                                    ui.painter().rect_filled(
                                        trect,
                                        12.0,
                                        Color32::from_rgb(32, 32, 38),
                                    );
                                    theme::draw_icon_centered(
                                        ui.painter(),
                                        trect.center(),
                                        icon::LIST_MUSIC,
                                        30.0,
                                        theme::TEXT_DIM,
                                    );
                                }
                                let badge_sz = egui::vec2(72.0, 22.0);
                                let badge = Rect::from_min_size(
                                    trect.max - badge_sz - egui::vec2(7.0, 7.0),
                                    badge_sz,
                                );
                                ui.painter().rect_filled(
                                    badge,
                                    5.0,
                                    Color32::from_black_alpha(205),
                                );
                                theme::draw_icon_centered(
                                    ui.painter(),
                                    pos2(badge.left() + 13.0, badge.center().y),
                                    icon::LIST_MUSIC,
                                    11.0,
                                    Color32::WHITE,
                                );
                                ui.painter().text(
                                    pos2(badge.left() + 23.0, badge.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "Playlist",
                                    theme::medium(11.0),
                                    Color32::WHITE,
                                );

                                ui.add_space(8.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&pl.title)
                                            .font(theme::semibold(13.8))
                                            .color(Color32::WHITE),
                                    )
                                    .truncate(),
                                );
                                if !pl.subtitle.is_empty() {
                                    ui.label(
                                        RichText::new(&pl.subtitle)
                                            .font(theme::regular(12.0))
                                            .color(theme::TEXT_DIM),
                                    );
                                }
                            });
                            if card_resp.clicked() {
                                open_pl = Some((pl.browse_id.clone(), pl.title.clone()));
                            }
                            if col_idx + 1 < row.len() {
                                ui.add_space(spacing);
                            }
                        }
                    });
                    ui.add_space(16.0);
                }
            });

        if let Some((bid, title)) = open_pl {
            self.open_playlist(bid, title);
        }
    }

    pub(super) fn draw_playlist_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let heading = self.active_playlist_title.clone();
        let items = self.playlist_items.clone();
        let loading = self.playlist_loading;
        let cont = self.playlist_continuation.clone();
        let editable = self.playlist_is_editable && self.signed_in;
        let has_videos = items.iter().any(|it| it.playlist_browse_id.is_none());

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&heading).font(theme::bold(20.0)));
            if !items.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "{} video{}",
                        items.len(),
                        if items.len() == 1 { "" } else { "s" }
                    ))
                    .font(theme::regular(13.0))
                    .color(theme::TEXT_DIM),
                );
            }
            if loading {
                ui.spinner();
            }
            if has_videos {
                ui.add_space(10.0);
                let pl_cont = self
                    .active_playlist_id
                    .clone()
                    .zip(cont.clone());
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("{}  Play All", icon::PLAY))
                                .font(theme::semibold(13.0))
                                .color(Color32::from_rgb(15, 15, 15)),
                        )
                        .fill(Color32::from_rgb(241, 241, 241))
                        .corner_radius(18),
                    )
                    .clicked()
                {
                    self.play_all(&items, false, pl_cont.clone());
                }
                let (sh_rect, sh_resp) =
                    ui.allocate_exact_size(Vec2::new(92.0, 32.0), Sense::click());
                if sh_resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                ui.painter().rect_filled(
                    sh_rect,
                    16.0,
                    if sh_resp.hovered() {
                        theme::CHIP_HOVER
                    } else {
                        theme::CHIP
                    },
                );
                theme::draw_shuffle_icon(
                    ui.painter(),
                    pos2(sh_rect.left() + 18.0, sh_rect.center().y),
                    14.0,
                    theme::TEXT,
                );
                ui.painter().text(
                    pos2(sh_rect.left() + 31.0, sh_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "Shuffle",
                    theme::semibold(13.0),
                    theme::TEXT,
                );
                if sh_resp.clicked() {
                    self.play_all(&items, true, pl_cont);
                }
            }
            if editable && !items.is_empty() {
                let manage_fill = if self.playlist_manage_mode {
                    theme::YT_BLUE.gamma_multiply(0.24)
                } else {
                    theme::CHIP
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!(
                                "{}  {}",
                                icon::LIST_MUSIC,
                                if self.playlist_manage_mode {
                                    "Done editing"
                                } else {
                                    "Reorder / Edit"
                                }
                            ))
                            .font(theme::medium(12.5))
                            .color(if self.playlist_manage_mode {
                                theme::YT_BLUE
                            } else {
                                theme::TEXT
                            }),
                        )
                        .fill(manage_fill)
                        .corner_radius(18),
                    )
                    .clicked()
                {
                    self.playlist_manage_mode = !self.playlist_manage_mode;
                }
            }
        });
        ui.add_space(10.0);

        if self.playlist_manage_mode && editable {
            let mut move_pair: Option<(usize, usize)> = None;
            let mut remove_item: Option<VideoItem> = None;
            let mut open_item: Option<VideoItem> = None;
            let mut load_more_clicked = false;
            let mut row_rects: Vec<(usize, Rect)> = Vec::new();
            let ptr_down = ui.input(|i| i.pointer.primary_down());
            let ptr_pos = ui.input(|i| i.pointer.interact_pos());

            if !ptr_down {
                self.playlist_drag_idx = None;
                self.playlist_drag_origin = None;
                self.playlist_drag_active = false;
            }

            ScrollArea::vertical()
                .id_salt("playlist_manage_scroll")
                .show(ui, |ui| {
                    let row_w = ui.available_width().min(860.0);
                    for (idx, it) in items.iter().enumerate() {
                        let is_dragged =
                            self.playlist_drag_active && self.playlist_drag_idx == Some(idx);
                        let frame_resp = Frame::new()
                            .fill(if is_dragged {
                                theme::CHIP_HOVER
                            } else {
                                theme::SURFACE
                            })
                            .stroke(Stroke::new(
                                1.0,
                                if is_dragged {
                                    theme::YT_BLUE
                                } else {
                                    theme::BORDER
                                },
                            ))
                            .corner_radius(10)
                            .inner_margin(Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.set_width(row_w);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("{:>2}", idx + 1))
                                            .font(theme::medium(12.5))
                                            .color(theme::TEXT_DIM),
                                    );
                                    let thumb_resp = ui
                                        .horizontal(|ui| {
                                            draw_thumbnail(
                                                ui,
                                                &it.thumbnail_url,
                                                &it.duration_text,
                                                Vec2::new(112.0, 63.0),
                                                7.0,
                                                it.watched_percent,
                                            );
                                        })
                                        .response
                                        .interact(Sense::click());
                                    if thumb_resp.clicked() {
                                        open_item = Some(it.clone());
                                    }
                                    ui.add_space(8.0);
                                    ui.vertical(|ui| {
                                        ui.set_max_width((row_w - 280.0).max(140.0));
                                        let title_resp = ui
                                            .add(
                                                egui::Label::new(
                                                    RichText::new(&it.title)
                                                        .font(theme::semibold(13.5))
                                                        .color(Color32::WHITE),
                                                )
                                                .truncate(),
                                            )
                                            .interact(Sense::click());
                                        if title_resp.clicked() {
                                            open_item = Some(it.clone());
                                        }
                                        if !it.channel.is_empty() {
                                            ui.label(
                                                RichText::new(&it.channel)
                                                    .font(theme::regular(12.0))
                                                    .color(theme::TEXT_DIM),
                                            );
                                        }
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if icon_circle_button(
                                            ui,
                                            icon::TRASH_2,
                                            15.0,
                                            "Remove from playlist",
                                        ) {
                                            remove_item = Some(it.clone());
                                        }
                                        if it.playlist_set_video_id.is_some() {
                                            if idx + 1 < items.len()
                                                && icon_circle_button(
                                                    ui,
                                                    icon::CHEVRON_DOWN,
                                                    15.0,
                                                    "Move down",
                                                )
                                            {
                                                move_pair = Some((idx, idx + 1));
                                            }
                                            if idx > 0
                                                && icon_circle_button(
                                                    ui,
                                                    icon::CHEVRON_UP,
                                                    15.0,
                                                    "Move up",
                                                )
                                            {
                                                move_pair = Some((idx, idx - 1));
                                            }
                                        }
                                    });
                                });
                            });
                        let rect = frame_resp.response.rect;
                        row_rects.push((idx, rect));
                        if it.playlist_set_video_id.is_some()
                            && frame_resp.response.hovered()
                            && ui.input(|i| i.pointer.primary_pressed())
                        {
                            self.playlist_drag_idx = Some(idx);
                            self.playlist_drag_origin =
                                ptr_pos.map(|p| (p, std::time::Instant::now()));
                            self.playlist_drag_active = false;
                        }
                        ui.add_space(6.0);
                    }
                    if cont.is_some() && !loading {
                        ui.add_space(6.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Load more").font(theme::medium(13.0)),
                                )
                                .fill(theme::CHIP)
                                .corner_radius(18),
                            )
                            .clicked()
                        {
                            load_more_clicked = true;
                        }
                    }
                });

            if let (Some(from), Some(p)) = (self.playlist_drag_idx, ptr_pos) {
                if !self.playlist_drag_active {
                    if let Some((orig, started)) = self.playlist_drag_origin {
                        if started.elapsed() >= std::time::Duration::from_millis(180)
                            || (p - orig).length() > 8.0
                        {
                            self.playlist_drag_active = true;
                        }
                    }
                }
                if self.playlist_drag_active {
                    ctx.set_cursor_icon(CursorIcon::Grabbing);
                    if let Some((target_idx, _)) =
                        row_rects.iter().find(|(_, r)| r.contains(p))
                    {
                        if *target_idx != from {
                            move_pair = Some((from, *target_idx));
                            self.playlist_drag_idx = Some(*target_idx);
                        }
                    }
                }
            }

            if let Some((from, to)) = move_pair {
                self.move_playlist_item(from, to);
            }
            if let Some(it) = remove_item {
                self.remove_playlist_item(&it);
            }
            if let Some(it) = open_item {
                self.open_video(it);
            }
            if load_more_clicked {
                if let Some(bid) = self.active_playlist_id.clone() {
                    self.playlist_loading = true;
                    let _ = self.backend.tx.send(Request::LoadPlaylist {
                        browse_id: bid,
                        continuation: cont,
                    });
                }
            }
            return;
        }

        if let Some(act) = draw_video_grid_with_flags(
            ui,
            "",
            &items,
            loading,
            cont.is_some(),
            false,
            editable,
        ) {
            self.handle_grid_action(ctx, act, |app| {
                if let Some(bid) = app.active_playlist_id.clone() {
                    app.playlist_loading = true;
                    let _ = app.backend.tx.send(Request::LoadPlaylist {
                        browse_id: bid,
                        continuation: cont,
                    });
                }
            });
        }
    }
}

fn settings_match(words: &[String], group: &str, text: &str) -> bool {
    if words.is_empty() {
        return true;
    }
    let hay = format!("{} {}", group.to_ascii_lowercase(), text.to_ascii_lowercase());
    words.iter().all(|w| hay.contains(w.as_str()))
}

fn settings_group_frame(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: Option<&str>,
    add: impl FnOnce(&mut egui::Ui),
) {
    ui.label(
        RichText::new(title)
            .font(theme::semibold(15.5))
            .color(Color32::WHITE),
    );
    if let Some(sub) = subtitle {
        ui.label(
            RichText::new(sub)
                .font(theme::regular(12.5))
                .color(theme::TEXT_DIM),
        );
    }
    ui.add_space(10.0);
    Frame::new()
        .fill(theme::card_fill())
        .stroke(Stroke::new(1.0, theme::card_stroke()))
        .corner_radius(14)
        .inner_margin(Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn settings_divider(ui: &mut egui::Ui) {
    ui.add_space(10.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    let line_col = if theme::is_translucent() {
        Color32::from_white_alpha(22)
    } else {
        theme::BORDER
    };
    ui.painter().rect_filled(rect, 0.0, line_col);
    ui.add_space(10.0);
}

fn settings_row(
    ui: &mut egui::Ui,
    title: &str,
    desc: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    let total_w = ui.available_width();
    let label_w = (total_w - 230.0).max(160.0);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 0.0),
            Layout::top_down(Align::LEFT),
            |ui| {
                ui.label(
                    RichText::new(title)
                        .font(theme::medium(14.0))
                        .color(Color32::WHITE),
                );
                if let Some(d) = desc {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(d)
                            .font(theme::regular(12.0))
                            .color(theme::TEXT_DIM),
                    );
                }
            },
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            control(ui);
        });
    });
}

fn settings_toggle(ui: &mut egui::Ui, value: &mut bool, id_salt: &str) -> bool {
    let desired = Vec2::new(40.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let clicked = resp.clicked();
    if clicked {
        *value = !*value;
    }
    let t = ui.ctx().animate_bool_with_time(
        egui::Id::unique(("settings_switch", id_salt)),
        *value,
        0.16,
    );
    let off_col = if theme::is_translucent() {
        Color32::from_white_alpha(38)
    } else {
        theme::CHIP_HOVER
    };
    let on_col = theme::YT_BLUE;
    let fill = Color32::from_rgb(
        (off_col.r() as f32 + (on_col.r() as f32 - off_col.r() as f32) * t).round() as u8,
        (off_col.g() as f32 + (on_col.g() as f32 - off_col.g() as f32) * t).round() as u8,
        (off_col.b() as f32 + (on_col.b() as f32 - off_col.b() as f32) * t).round() as u8,
    );
    ui.painter().rect_filled(rect, 11.0, fill);
    let knob_x = rect.left() + 11.0 + t * (rect.width() - 22.0);
    ui.painter()
        .circle_filled(pos2(knob_x, rect.center().y), 8.0, Color32::WHITE);
    clicked
}

fn settings_pill_btn(ui: &mut egui::Ui, label: &str, ico: Option<char>) -> egui::Response {
    let text = match ico {
        Some(c) => format!("{c}  {label}"),
        None => label.to_owned(),
    };
    let fill = if theme::is_translucent() {
        Color32::from_rgba_unmultiplied(255, 255, 255, 20)
    } else {
        theme::CHIP
    };
    ui.add(
        egui::Button::new(RichText::new(text).font(theme::medium(12.5)).color(Color32::WHITE))
            .fill(fill)
            .stroke(Stroke::new(1.0, theme::card_stroke()))
            .corner_radius(16)
            .min_size(Vec2::new(86.0, 30.0)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

#[cfg(test)]
mod settings_tests {
    use super::settings_match;

    #[test]
    fn settings_match_filters_by_group_and_row() {
        let q = vec!["acrylic".to_owned()];
        assert!(settings_match(&q, "Appearance", "Window material Acrylic Mica"));
        assert!(!settings_match(&q, "Playback", "Preferred max quality"));

        let q_group = vec!["playback".to_owned(), "token".to_owned()];
        assert!(settings_match(&q_group, "Playback", "BotGuard PO Token"));
        assert!(!settings_match(&q_group, "Appearance", "Ambient background"));
    }
}
