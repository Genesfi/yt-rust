use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Rect, RichText,
    ScrollArea, Sense, Stroke, Vec2, ViewportCommand, pos2, vec2,
};

use crate::audio_viz::NUM_BANDS;
use crate::backend::Request;
use crate::model::{
    LiveChatKind, LiveChatRun, SpectrumTheme, SponsorCategory, VideoItem, WatchData,
};
use crate::mpv::{AMB_GRID_H, AMB_GRID_W, LiveAmbientSnapshot};
use crate::theme::{self, icon};

use super::cards::{
    GridAction, clamp_two_lines, draw_avatar, draw_members_badge, draw_thumbnail,
    draw_video_context_menu, icon_circle_button, join_meta,
};
use super::{
    HudIcon, Page, PlayerMenu, PlayerSlot, RightTab, SleepTimer, YtApp, format_duration,
};

impl YtApp {
    pub(super) fn draw_watch_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.fullscreen {
            let avail = ui.available_size();
            self.draw_video_surface(ui, ctx, avail, true, false);
            return;
        }

        self.draw_player_tab_bar(ui);

        let wide = ui.available_width() >= 920.0 && !self.theater_mode;
        if wide {
            let total_w = ui.available_width();
            let right_w = (total_w * 0.29).clamp(300.0, 390.0);
            let left_w = (total_w - right_w - 14.0).max(460.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left_w, ui.available_height()),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        let mut st = egui::style::ScrollStyle::floating();
                        st.bar_width = 5.0;
                        st.bar_inner_margin = 2.0;
                        st.bar_outer_margin = 1.0;
                        ui.spacing_mut().scroll = st;
                        let mut sa = ScrollArea::vertical().id_salt("watch_left_scroll");
                        if self.scroll_watch_to_top {
                            sa = sa.vertical_scroll_offset(0.0);
                            self.scroll_watch_to_top = false;
                            self.watch_scrolled_into_pip = false;
                        }
                        let content_w = (left_w - 14.0).max(420.0);
                        let mut last_video_h = 360.0_f32;
                        let scroll_out = sa.show(ui, |ui| {
                            ui.set_max_width(content_w);
                            let video_h =
                                (content_w * 9.0 / 16.0).min(ctx.content_rect().height() * 0.74);
                            last_video_h = video_h;
                            if self.watch_scrolled_into_pip && self.watch.is_some() {
                                if draw_pip_dock_placeholder(ui, Vec2::new(content_w, video_h)) {
                                    self.watch_scrolled_into_pip = false;
                                    self.scroll_watch_to_top = true;
                                }
                            } else {
                                self.draw_video_surface(
                                    ui,
                                    ctx,
                                    Vec2::new(content_w, video_h),
                                    false,
                                    false,
                                );
                            }
                            ui.add_space(12.0);
                            self.draw_watch_metadata(ui);
                        });
                        let sy = scroll_out.state.clamped_offset().y;
                        let max_scroll =
                            (scroll_out.content_size.y - scroll_out.inner_rect.height()).max(0.0);
                        let enter_sy = (last_video_h * 0.65).max(160.0);
                        let exit_sy = (last_video_h * 0.40).max(80.0);
                        if !self.watch_scrolled_into_pip
                            && self.watch.is_some()
                            && max_scroll > (last_video_h * 0.55).max(140.0)
                            && sy > enter_sy
                        {
                            self.watch_scrolled_into_pip = true;
                        } else if self.watch_scrolled_into_pip && sy <= exit_sy {
                            self.watch_scrolled_into_pip = false;
                        }
                    },
                );
                ui.add_space(10.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(right_w, ui.available_height()),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        self.draw_related_panel(ui, ctx);
                    },
                );
            });
        } else {
            let mut sa = ScrollArea::vertical().id_salt("watch_theater_scroll");
            if self.scroll_watch_to_top {
                sa = sa.vertical_scroll_offset(0.0);
                self.scroll_watch_to_top = false;
                self.watch_scrolled_into_pip = false;
            }
            let mut last_video_h = 360.0_f32;
            let scroll_out = sa.show(ui, |ui| {
                let content_w = (ui.available_width() - 16.0).max(320.0);
                ui.set_max_width(content_w);
                let video_h = if self.theater_mode {
                    (ctx.content_rect().height() * 0.76).max(340.0)
                } else {
                    (content_w * 9.0 / 16.0).min(580.0)
                };
                last_video_h = video_h;
                if self.watch_scrolled_into_pip && self.watch.is_some() {
                    if draw_pip_dock_placeholder(ui, Vec2::new(content_w, video_h)) {
                        self.watch_scrolled_into_pip = false;
                        self.scroll_watch_to_top = true;
                    }
                } else {
                    self.draw_video_surface(ui, ctx, Vec2::new(content_w, video_h), false, false);
                }
                ui.add_space(12.0);
                self.draw_watch_metadata(ui);
                ui.add_space(16.0);
                self.draw_related_panel(ui, ctx);
            });
            let sy = scroll_out.state.clamped_offset().y;
            let max_scroll =
                (scroll_out.content_size.y - scroll_out.inner_rect.height()).max(0.0);
            let enter_sy = (last_video_h * 0.65).max(160.0);
            let exit_sy = (last_video_h * 0.40).max(80.0);
            if !self.watch_scrolled_into_pip
                && self.watch.is_some()
                && max_scroll > (last_video_h * 0.55).max(140.0)
                && sy > enter_sy
            {
                self.watch_scrolled_into_pip = true;
            } else if self.watch_scrolled_into_pip && sy <= exit_sy {
                self.watch_scrolled_into_pip = false;
            }
        }
    }

    pub(super) fn draw_player_tab_bar(&mut self, ui: &mut egui::Ui) {
        let has_active = self.watch.is_some() || self.watch_loading;
        let has_bg = self.bg_player_tab.is_some();
        if !has_active && !has_bg {
            return;
        }
        let mut switch_tab = false;
        let mut add_second_tab = false;
        let mut close_idx: Option<usize> = None;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for tab_idx in 0..2 {
                let is_active = self.active_player_tab == tab_idx;
                if !is_active && self.bg_player_tab.is_none() {
                    continue;
                }
                let tab_title: String = if is_active {
                    self.watch
                        .as_ref()
                        .map(|w| w.title.clone())
                        .unwrap_or_else(|| {
                            if self.watch_loading {
                                "Loading...".to_owned()
                            } else {
                                "Empty (picks open here)".to_owned()
                            }
                        })
                } else if let Some(bg) = &self.bg_player_tab {
                    bg.watch
                        .as_ref()
                        .map(|w| w.title.clone())
                        .unwrap_or_else(|| {
                            if bg.watch_loading {
                                "Loading...".to_owned()
                            } else {
                                "Empty Tab".to_owned()
                            }
                        })
                } else {
                    continue;
                };
                let short_title: String = if tab_title.chars().count() > 26 {
                    let s: String = tab_title.chars().take(25).collect();
                    format!("{s}…")
                } else {
                    tab_title
                };
                let label_text = format!("Tab {}: {}", tab_idx + 1, short_title);

                let (fill, stroke, text_col) = if is_active {
                    (
                        theme::CHIP_HOVER,
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 45)),
                        Color32::WHITE,
                    )
                } else {
                    (
                        theme::SURFACE,
                        Stroke::new(1.0, theme::BORDER),
                        theme::TEXT_DIM,
                    )
                };

                Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(14)
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let btn = ui.add(
                                egui::Button::new(
                                    RichText::new(label_text)
                                        .font(if is_active {
                                            theme::semibold(12.0)
                                        } else {
                                            theme::medium(12.0)
                                        })
                                        .color(text_col),
                                )
                                .fill(Color32::TRANSPARENT)
                                .frame(false),
                            );
                            if btn.hovered() {
                                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                            }
                            if btn.clicked() && !is_active {
                                switch_tab = true;
                            }
                            if !is_active {
                                if let Some(bg) = &mut self.bg_player_tab {
                                    if bg.watch.is_some() {
                                        let p_ico = if bg.playback.paused {
                                            icon::PLAY
                                        } else {
                                            icon::PAUSE
                                        };
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(p_ico.to_string())
                                                        .font(theme::icon_font(12.0))
                                                        .color(theme::TEXT),
                                                )
                                                .fill(Color32::TRANSPARENT)
                                                .frame(false),
                                            )
                                            .on_hover_text("Play/Pause background tab")
                                            .clicked()
                                        {
                                            if let Some(m) = &bg.mpv {
                                                m.toggle_pause();
                                            }
                                        }
                                        let m_ico = if bg.muted {
                                            icon::VOLUME_X
                                        } else {
                                            icon::VOLUME_2
                                        };
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(m_ico.to_string())
                                                        .font(theme::icon_font(12.0))
                                                        .color(if bg.muted {
                                                            theme::YT_RED
                                                        } else {
                                                            theme::TEXT_DIM
                                                        }),
                                                )
                                                .fill(Color32::TRANSPARENT)
                                                .frame(false),
                                            )
                                            .on_hover_text("Mute/Unmute background tab")
                                            .clicked()
                                        {
                                            bg.muted = !bg.muted;
                                            if let Some(m) = &bg.mpv {
                                                m.set_mute(bg.muted);
                                            }
                                        }
                                    }
                                }
                            }
                            if self.bg_player_tab.is_some()
                                && ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(icon::X.to_string())
                                                .font(theme::icon_font(11.0))
                                                .color(theme::TEXT_DIM),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .frame(false),
                                    )
                                    .on_hover_text("Close tab")
                                    .clicked()
                            {
                                close_idx = Some(tab_idx);
                            }
                        });
                    });
            }

            if self.bg_player_tab.is_none() && has_active {
                let plus = ui.add(
                    egui::Button::new(
                        RichText::new("+ 2nd Tab")
                            .font(theme::medium(11.5))
                            .color(theme::TEXT_DIM),
                    )
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(14),
                );
                if plus.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if plus
                    .on_hover_text("Open a 2nd player tab (or middle-click any video card) to play two videos/podcasts simultaneously")
                    .clicked()
                {
                    add_second_tab = true;
                }
            }
        });
        ui.add_space(6.0);

        if let Some(ci) = close_idx {
            self.close_player_tab(ci == self.active_player_tab);
        } else if switch_tab {
            self.switch_player_tab();
        } else if add_second_tab {
            let mut slot = Box::new(PlayerSlot::new(1.0));
            slot.ensure_mpv(
                &self.backend.data_dir,
                &self.get_proc_address,
                ui.ctx(),
                self.volume,
                self.hwdec_mode,
                self.sub_font_size,
                self.sub_bg_opacity,
            );
            self.swap_active_with(&mut slot);
            self.bg_player_tab = Some(slot);
            self.active_player_tab = 1 - self.active_player_tab;
            self.watch = None;
            self.active_watch_id = None;
            self.watch_loading = false;
            self.toast("2nd player tab ready — pick any video to play here");
        }
    }

    pub(super) fn draw_shorts_player(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.shorts_slot.mpv.is_none() && self.mpv_error.is_none() {
            self.shorts_slot.ensure_mpv(
                &self.backend.data_dir,
                &self.get_proc_address,
                ctx,
                self.volume,
                self.hwdec_mode,
                self.sub_font_size,
                self.sub_bg_opacity,
            );
            if let Some(m) = &self.shorts_slot.mpv {
                m.set_cache_pause_wait(0.3);
            }
        }
        let resume_video_title = self
            .watch
            .as_ref()
            .map(|w| w.title.clone())
            .or_else(|| {
                self.bg_player_tab
                    .as_ref()
                    .and_then(|b| b.watch.as_ref().map(|w| w.title.clone()))
            });

        let mut slot = Box::new(PlayerSlot::new(1.0));
        std::mem::swap(&mut self.shorts_slot, &mut slot);
        self.swap_active_with(&mut slot);
        let action = self.draw_shorts_player_inner(ui, ctx, resume_video_title.as_deref());
        self.swap_active_with(&mut slot);
        std::mem::swap(&mut self.shorts_slot, &mut slot);

        match action {
            ShortsUiAction::None => {}
            ShortsUiAction::Step(delta) => self.step_short(delta),
            ShortsUiAction::ReturnToVideo => self.return_from_shorts_to_video(),
            ShortsUiAction::GridView => {
                if let Some(m) = &self.shorts_slot.mpv {
                    m.set_paused(true);
                }
                self.shorts_grid_view = true;
            }
            ShortsUiAction::OpenChannel(cid, name) => {
                if let Some(m) = &self.shorts_slot.mpv {
                    m.set_paused(true);
                }
                self.open_channel(cid, name);
            }
        }
    }

    fn draw_shorts_player_inner(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        resume_video_title: Option<&str>,
    ) -> ShortsUiAction {
        let page_rect = ui.available_rect_before_wrap();
        let full_rect = ctx.content_rect();
        let mut action = ShortsUiAction::None;

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if let Some(vtitle) = resume_video_title {
                    let short_v: String = if vtitle.chars().count() > 28 {
                        let s: String = vtitle.chars().take(27).collect();
                        format!("{s}…")
                    } else {
                        vtitle.to_owned()
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!(
                                    "{}  Back to video: {short_v}",
                                    icon::CHEVRON_LEFT
                                ))
                                .font(theme::medium(12.5))
                                .color(theme::TEXT),
                            )
                            .fill(theme::CHIP)
                            .corner_radius(16),
                        )
                        .on_hover_text("Return to your regular video player")
                        .clicked()
                    {
                        action = ShortsUiAction::ReturnToVideo;
                    }
                    ui.add_space(6.0);
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Grid View")
                                .font(theme::medium(12.0))
                                .color(theme::TEXT_DIM),
                        )
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .corner_radius(16),
                    )
                    .clicked()
                {
                    action = ShortsUiAction::GridView;
                }
                ui.add_space(10.0);
                let (irec, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                theme::draw_shorts_icon(ui.painter(), irec.center(), 16.0, theme::YT_RED);
                let pos_text = if self.shorts_queue.len() > 1 {
                    format!("Shorts  ·  {} / {}", self.shorts_idx + 1, self.shorts_queue.len())
                } else {
                    "Shorts".to_owned()
                };
                ui.label(
                    RichText::new(pos_text)
                        .font(theme::medium(13.0))
                        .color(theme::TEXT_DIM),
                );
            });
            ui.add_space(8.0);

            let body_rect = ui.available_rect_before_wrap();
            let rail_w = 58.0_f32;
            let rail_gap = 12.0_f32;
            let drawer_gap = 14.0_f32;
            let sh = (body_rect.height() - 12.0).clamp(360.0, 820.0);
            let max_sw_for_comments = if self.shorts_comments_open {
                (body_rect.width() - rail_w - rail_gap - drawer_gap - 280.0 - 24.0).max(240.0)
            } else {
                (body_rect.width() - rail_w - rail_gap - 24.0).max(240.0)
            };
            let sw = (sh * 9.0 / 16.0).min(max_sw_for_comments);
            let drawer_w = if self.shorts_comments_open {
                (body_rect.width() - sw - rail_w - rail_gap - drawer_gap - 62.0).clamp(260.0, 340.0)
            } else {
                0.0
            };

            // Lock video horizontally to the full window center so opening Sidebar or Comments never shifts it.
            let min_vid_left = body_rect.left() + 8.0;
            let max_vid_left = if self.shorts_comments_open {
                (body_rect.right() - drawer_w - drawer_gap - rail_w - rail_gap - sw - 54.0)
                    .max(min_vid_left)
            } else {
                (body_rect.right() - rail_w - rail_gap - sw - 16.0).max(min_vid_left)
            };
            let vid_left = (full_rect.center().x - sw * 0.5).clamp(min_vid_left, max_vid_left);
            let vid_top = body_rect.top() + ((body_rect.height() - sh) * 0.5).max(0.0);
            let base_vid_rect = Rect::from_min_size(pos2(vid_left, vid_top), Vec2::new(sw, sh));
            let travel = sh + 20.0;
            let anim_state = if let Some((dir, started)) = self.shorts_slide_anim {
                let t = (started.elapsed().as_secs_f32() / 0.28).clamp(0.0, 1.0);
                if t >= 1.0 {
                    self.shorts_slide_anim = None;
                    self.shorts_outgoing = None;
                    None
                } else {
                    ctx.request_repaint();
                    let ease = 1.0 - (1.0 - t).powi(3);
                    Some((dir, ease))
                }
            } else {
                None
            };

            let carousel_clip = Rect::from_min_max(
                pos2(base_vid_rect.left() - 2.0, body_rect.top()),
                pos2(base_vid_rect.right() + 2.0, body_rect.bottom()),
            );

            if let Some((dir, ease)) = anim_state {
                if let Some(out) = self.shorts_outgoing.clone() {
                    let out_offset_y = -(dir as f32) * ease * travel;
                    let out_rect = base_vid_rect.translate(vec2(0.0, out_offset_y));
                    if out_rect.intersects(carousel_clip) {
                        let mut out_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(out_rect)
                                .layout(Layout::top_down(Align::LEFT)),
                        );
                        out_ui.set_clip_rect(out_ui.clip_rect().intersect(carousel_clip));
                        out_ui.set_opacity((1.0 - ease * 0.25).clamp(0.0, 1.0));
                        draw_shorts_transition_card(&mut out_ui, out_rect, 14.0, &out);
                    }
                }
            }

            let slide_offset_y = anim_state
                .map(|(dir, ease)| (dir as f32) * (1.0 - ease) * travel)
                .unwrap_or(0.0);
            let vid_rect = base_vid_rect.translate(vec2(0.0, slide_offset_y));

            let mut vid_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(vid_rect)
                    .layout(Layout::top_down(Align::LEFT)),
            );
            vid_ui.set_clip_rect(vid_ui.clip_rect().intersect(carousel_clip));
            if let Some((_, ease)) = anim_state {
                vid_ui.set_opacity((0.75 + 0.25 * ease).clamp(0.0, 1.0));
            }
            if let Some((cid, cname)) =
                self.draw_video_surface_inner(&mut vid_ui, ctx, Vec2::new(sw, sh), false, true)
            {
                action = ShortsUiAction::OpenChannel(cid, cname);
            }
            if ui.rect_contains_pointer(base_vid_rect) {
                let wy = ctx.input(|i| i.smooth_scroll_delta.y);
                let cooldown_ok = self
                    .shorts_last_wheel_at
                    .is_none_or(|t| t.elapsed() >= Duration::from_millis(320));
                if cooldown_ok && wy > 24.0 {
                    self.shorts_last_wheel_at = Some(Instant::now());
                    action = ShortsUiAction::Step(-1);
                } else if cooldown_ok && wy < -24.0 {
                    self.shorts_last_wheel_at = Some(Instant::now());
                    action = ShortsUiAction::Step(1);
                }
            }

            let rail_rect = Rect::from_min_size(
                pos2(base_vid_rect.right() + rail_gap, vid_top),
                Vec2::new(rail_w, sh),
            );
            let mut rail_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rail_rect)
                    .layout(Layout::top_down(Align::Center)),
            );
            if let Some(w) = self.watch.clone() {
                rail_ui.spacing_mut().item_spacing.y = 0.0;
                let rail_total_h = 5.0 * 62.0 + 4.0 * 10.0 + 14.0;
                rail_ui.add_space((sh - rail_total_h).max(4.0));

                let like_lbl = if w.likes_text.is_empty() {
                    "Like"
                } else {
                    w.likes_text.as_str()
                };
                let liked = w.is_liked;
                if shorts_rail_button(
                    &mut rail_ui,
                    like_lbl,
                    if liked { "Unlike" } else { "Like" },
                    liked,
                    |painter, c, col| {
                        theme::draw_thumbs_up(painter, c, 18.0, col, liked);
                    },
                ) {
                    let _ = self.backend.tx.send(Request::SetLike {
                        video_id: w.id.clone(),
                        like: !liked,
                    });
                }
                rail_ui.add_space(10.0);

                let dislike_lbl = w
                    .dislikes_text
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Dislike");
                if shorts_rail_button(
                    &mut rail_ui,
                    dislike_lbl,
                    if w.is_disliked {
                        "Remove dislike"
                    } else {
                        "Dislike"
                    },
                    w.is_disliked,
                    |painter, c, col| {
                        theme::draw_icon_centered(
                            painter,
                            c,
                            icon::THUMBS_DOWN,
                            18.0,
                            col,
                        );
                    },
                ) {
                    let _ = self.backend.tx.send(Request::SetDislike {
                        video_id: w.id.clone(),
                        dislike: !w.is_disliked,
                    });
                }
                rail_ui.add_space(10.0);

                let c_label = if self.comments.is_empty() {
                    "Comments".to_owned()
                } else {
                    self.comments.len().to_string()
                };
                if shorts_rail_button(
                    &mut rail_ui,
                    &c_label,
                    "Comments & Replies",
                    self.shorts_comments_open,
                    |painter, c, col| {
                        theme::draw_comment_icon(painter, c, 18.0, col);
                    },
                ) {
                    self.shorts_comments_open = !self.shorts_comments_open;
                    if self.shorts_comments_open
                        && self.comments.is_empty()
                        && !self.comments_loading
                    {
                        if let Some(tok) = w.comments_token.clone() {
                            self.comments_loading = true;
                            let _ = self.backend.tx.send(Request::LoadComments {
                                video_id: w.id.clone(),
                                continuation: tok,
                                append: false,
                            });
                        }
                    }
                }
                rail_ui.add_space(10.0);

                if shorts_rail_button(
                    &mut rail_ui,
                    "Share",
                    "Copy Short URL",
                    false,
                    |painter, c, col| {
                        theme::draw_share_icon(painter, c, 18.0, col);
                    },
                ) {
                    ctx.copy_text(format!("https://www.youtube.com/shorts/{}", w.id));
                    self.toast("Copied Shorts URL");
                }
                rail_ui.add_space(10.0);

                if shorts_rail_button(
                    &mut rail_ui,
                    "Save",
                    "Download MP4",
                    false,
                    |painter, c, col| {
                        theme::draw_icon_centered(
                            painter,
                            c,
                            icon::DOWNLOAD,
                            18.0,
                            col,
                        );
                    },
                ) {
                    self.start_download(w.id.clone(), w.title.clone(), false);
                }
            }

            if self.shorts_comments_open {
                if let Some(w) = self.watch.clone() {
                    let drawer_rect = Rect::from_min_size(
                        pos2(rail_rect.right() + drawer_gap, vid_top),
                        Vec2::new(drawer_w, sh),
                    );
                    let mut dui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(drawer_rect)
                            .layout(Layout::top_down(Align::LEFT)),
                    );
                    Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .corner_radius(14)
                        .inner_margin(Margin::same(12))
                        .show(&mut dui, |ui| {
                            let inner_w = (drawer_w - 24.0).max(160.0);
                            let inner_h = (sh - 24.0).max(200.0);
                            ui.set_width(inner_w);
                            ui.set_height(inner_h);
                            ui.vertical(|ui| {
                                self.draw_comments_header(ui, &w, true);
                                ui.separator();
                                ui.add_space(4.0);
                                let has_composer =
                                    self.signed_in && self.comment_create_params.is_some();
                                let composer_h = if has_composer { 46.0 } else { 0.0 };
                                let list_h = (inner_h - 46.0 - composer_h).max(80.0);
                                ScrollArea::vertical()
                                    .id_salt("shorts_comments_drawer")
                                    .max_height(list_h)
                                    .show(ui, |ui| {
                                        ui.set_width(inner_w - 6.0);
                                        self.draw_comments_list(ui, &w);
                                    });
                                if has_composer {
                                    ui.separator();
                                    ui.add_space(4.0);
                                    self.draw_comment_composer(ui, &w);
                                }
                            });
                        });
                }
            }

            // Right-aligned Up/Down navigation arrows, visible smoothly on hover over the right edge
            let can_prev = self.shorts_idx > 0;
            let can_next = (!self.shorts_queue.is_empty()
                && self.shorts_idx + 1 < self.shorts_queue.len())
                || self
                    .watch
                    .as_ref()
                    .is_some_and(|w| !w.related.is_empty());
            if can_prev || can_next {
                let nav_hover_zone = Rect::from_min_max(
                    pos2((page_rect.right() - 120.0).max(page_rect.left()), body_rect.top()),
                    page_rect.max,
                );
                let nav_hovered = ui.rect_contains_pointer(nav_hover_zone);
                let nav_alpha = ctx.animate_bool_with_time(
                    egui::Id::unique("shorts_right_nav_hover"),
                    nav_hovered,
                    0.16,
                );
                if nav_alpha > 0.02 {
                    let btn_group_h = if can_prev && can_next { 100.0 } else { 44.0 };
                    let nav_rect = Rect::from_center_size(
                        pos2(page_rect.right() - 26.0, body_rect.center().y),
                        Vec2::new(48.0, btn_group_h),
                    );
                    let mut nui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(nav_rect)
                            .layout(Layout::top_down(Align::Center)),
                    );
                    nui.set_opacity(nav_alpha);
                    if can_prev
                        && shorts_nav_circle(&mut nui, icon::CHEVRON_UP, "Previous Short (Up)")
                    {
                        action = ShortsUiAction::Step(-1);
                    }
                    if can_prev && can_next {
                        nui.add_space(12.0);
                    }
                    if can_next
                        && shorts_nav_circle(&mut nui, icon::CHEVRON_DOWN, "Next Short (Down)")
                    {
                        action = ShortsUiAction::Step(1);
                    }
                }
            }

            ui.allocate_space(Vec2::new(body_rect.width(), body_rect.height().max(sh)));
        });

        action
    }

    fn draw_video_surface(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        size: Vec2,
        is_fullscreen: bool,
        is_shorts: bool,
    ) {
        let _ = self.draw_video_surface_inner(ui, ctx, size, is_fullscreen, is_shorts);
    }

    fn draw_video_surface_inner(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        size: Vec2,
        is_fullscreen: bool,
        is_shorts: bool,
    ) -> Option<(String, String)> {
        let mut open_channel: Option<(String, String)> = None;
        let is_sliding_short = is_shorts && self.shorts_slide_anim.is_some();
        let radius = if is_fullscreen || (!is_shorts && self.theater_mode) {
            0.0
        } else if is_shorts {
            14.0
        } else {
            12.0
        };
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let use_video_ambilight = self.ambient_mode == crate::model::AmbientMode::VideoAmbilight
            && self.ambient_glow
            && !is_fullscreen
            && !is_shorts
            && !is_sliding_short
            && !self.audio_only_mode
            && self.watch.is_some();
        if let Some(mpv) = &self.mpv {
            mpv.set_ambient_capture(use_video_ambilight);
        }
        if !is_fullscreen && self.ambient_glow && self.watch.is_some() && !is_sliding_short {
            let live_snap = if use_video_ambilight {
                self.mpv.as_ref().and_then(|m| m.live_ambient_snapshot())
            } else {
                None
            };
            if let Some(snap) = &live_snap {
                paint_video_ambilight_glow(
                    ui.painter(),
                    ctx,
                    rect,
                    snap,
                    self.ambient_opacity,
                    self.playback.paused,
                );
            } else {
                paint_smooth_ambient_glow(
                    ui.painter(),
                    ctx,
                    rect,
                    self.ambient_palette,
                    self.ambient_prev_palette,
                    self.ambient_changed_at,
                    self.ambient_opacity,
                    self.playback.paused,
                );
            }
        }
        ui.painter().rect_filled(rect, radius, Color32::BLACK);

        let mut audio_overlay_hit = false;
        if let Some(mpv) = &self.mpv {
            if is_shorts && (is_sliding_short || self.watch.is_none() || self.playback.video_h == 0)
            {
                if let Some(thumb) = self
                    .shorts_queue
                    .get(self.shorts_idx)
                    .map(|q| q.thumbnail_url.clone())
                    .filter(|u| !u.is_empty())
                {
                    theme::paint_image_cover_rounded(ui, &thumb, rect, radius.round() as u8);
                }
            }
            if self.watch.is_some() && !is_sliding_short && !self.audio_only_mode {
                ui.painter().add(mpv.paint_callback_rounded(rect, radius, radius));
            }
            if !is_shorts && self.audio_only_mode {
                if let Some(w) = self.watch.clone() {
                    if !w.id.starts_with("local:") {
                        let thumb_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", w.id);
                        theme::paint_image_cover_rounded(ui, &thumb_url, rect, radius.round() as u8);
                    }
                    ui.painter().rect_filled(
                        rect,
                        radius,
                        Color32::from_rgba_unmultiplied(8, 10, 14, 184),
                    );
                    if self.audio_spectrum_enabled {
                        let (bands, wave) = self.audio_spectrum.snapshot(self.playback.paused);
                        if !self.playback.paused
                            || bands.iter().any(|&b| b > 0.01)
                            || wave.iter().any(|&v| v > 0.01)
                        {
                            ctx.request_repaint_after(Duration::from_millis(16));
                        }
                        paint_audio_spectrum(
                            ui,
                            ctx,
                            rect,
                            radius,
                            &bands,
                            &wave,
                            self.audio_spectrum_theme,
                            self.ambient_palette,
                        );
                    }
                    // Compact floating top bar so center artwork + visualizer stay unobstructed
                    let bar_rect = Rect::from_min_max(
                        pos2(rect.left() + 12.0, rect.top() + 12.0),
                        pos2((rect.right() - 12.0).max(rect.left() + 200.0), rect.top() + 44.0),
                    );
                    if ctx
                        .input(|i| i.pointer.hover_pos())
                        .is_some_and(|p| bar_rect.contains(p))
                    {
                        audio_overlay_hit = true;
                    }
                    let mut top_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(bar_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    top_ui.set_clip_rect(top_ui.clip_rect().intersect(rect));
                    let codec_lbl = if self.playback.audio_codec.is_empty() {
                        "Audio Only".to_owned()
                    } else {
                        format!("Audio · {}", self.playback.audio_codec)
                    };
                    let info_pill = Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(16, 18, 24, 195))
                        .stroke(Stroke::new(1.0, Color32::from_white_alpha(26)))
                        .corner_radius(14)
                        .inner_margin(Margin::symmetric(10, 5))
                        .show(&mut top_ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                theme::draw_icon(ui, icon::HEADPHONES, 13.0, theme::YT_BLUE);
                                ui.label(
                                    RichText::new(codec_lbl)
                                        .font(theme::semibold(11.5))
                                        .color(Color32::WHITE),
                                );
                            });
                        });
                    if info_pill.response.hovered() {
                        top_ui.ctx().set_cursor_icon(CursorIcon::Default);
                    }

                    top_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let restore_btn = ui.add_sized(
                            vec2(92.0, 28.0),
                            egui::Button::new(
                                RichText::new("Video Mode")
                                    .font(theme::semibold(11.0))
                                    .color(Color32::from_rgb(15, 15, 15)),
                            )
                            .fill(Color32::from_rgb(241, 241, 241))
                            .corner_radius(14),
                        );
                        if restore_btn.hovered() {
                            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                        }
                        if restore_btn.clicked() {
                            self.toggle_audio_only_mode();
                            audio_overlay_hit = true;
                        }
                        if self.audio_spectrum_enabled
                            && popover_pill_button(
                                ui,
                                vec2(96.0, 28.0),
                                self.audio_spectrum_theme.label(),
                                false,
                                false,
                            )
                        {
                            self.audio_spectrum_theme = self.audio_spectrum_theme.next();
                            self.toast(format!(
                                "Spectrum theme: {}",
                                self.audio_spectrum_theme.label()
                            ));
                            audio_overlay_hit = true;
                        }
                        let spec_lbl = if self.audio_spectrum_enabled {
                            "Wave: On"
                        } else {
                            "Wave: Off"
                        };
                        if popover_pill_button(
                            ui,
                            vec2(76.0, 28.0),
                            spec_lbl,
                            self.audio_spectrum_enabled,
                            false,
                        ) {
                            self.audio_spectrum_enabled = !self.audio_spectrum_enabled;
                            audio_overlay_hit = true;
                        }
                    });
                }
            }
        } else if (self.watch_loading || crate::mpv::find_libmpv(&self.backend.data_dir).is_some())
            && self.mpv_error.is_none()
        {
            ctx.request_repaint();
            if is_shorts {
                if let Some(thumb) = self
                    .shorts_queue
                    .get(self.shorts_idx)
                    .map(|q| q.thumbnail_url.clone())
                    .filter(|u| !u.is_empty())
                {
                    theme::paint_image_cover_rounded(ui, &thumb, rect, radius.round() as u8);
                }
            }
            let label = if self.watch_loading {
                "Resolving stream..."
            } else {
                "Loading Shorts..."
            };
            draw_buffering_overlay(ui, ctx, rect, radius, label);
            if is_shorts && radius > 0.5 {
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    Stroke::new(1.0, Color32::from_white_alpha(22)),
                    egui::StrokeKind::Inside,
                );
            }
            return open_channel;
        } else {
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(24.0)), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        theme::draw_icon(ui, icon::DOWNLOAD, 34.0, theme::YT_BLUE);
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("libmpv-2.dll is required for hardware video playback")
                                .font(theme::semibold(16.0)),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "Place libmpv-2.dll beside yt-rust.exe or in {}",
                                self.backend.data_dir.display()
                            ))
                            .font(theme::regular(12.5))
                            .color(theme::TEXT_DIM),
                        );
                        ui.add_space(12.0);
                        if self.downloading_mpv {
                            ui.spinner();
                            ui.label("Downloading & extracting libmpv-2.dll...");
                        } else if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Download libmpv-2.dll automatically")
                                        .font(theme::semibold(13.0))
                                        .color(Color32::from_rgb(15, 15, 15)),
                                )
                                .fill(theme::YT_BLUE)
                                .corner_radius(18),
                            )
                            .clicked()
                        {
                            self.downloading_mpv = true;
                            let _ = self.backend.tx.send(Request::DownloadLibmpv);
                        }
                    });
                });
            });
            return open_channel;
        }

        // Track pointer movement to wake/auto-hide the YouTube-style overlay controls
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let pointer_in_video = pointer_pos.is_some_and(|p| rect.contains(p));
        if let Some(p) = pointer_pos {
            if rect.contains(p) && self.last_pointer_pos != Some(p) {
                self.last_pointer_pos = Some(p);
                self.last_pointer_move = Instant::now();
            }
        }
        let controls_zone = Rect::from_min_max(
            pos2(rect.left(), (rect.bottom() - 84.0).max(rect.top())),
            rect.max,
        );
        let pointer_in_controls = pointer_pos.is_some_and(|p| controls_zone.contains(p));

        let idle_elapsed = self.last_pointer_move.elapsed();
        let controls_should_show = self.playback.paused
            || self.seeking_pos.is_some()
            || self.player_menu != PlayerMenu::None
            || pointer_in_controls
            || (pointer_in_video && idle_elapsed < Duration::from_secs_f32(2.3));

        if controls_should_show && !self.playback.paused && self.player_menu == PlayerMenu::None {
            ctx.request_repaint_after(Duration::from_millis(350));
        }

        let overlay_alpha = ctx.animate_bool_with_time(
            egui::Id::unique("watch_controls_overlay_alpha"),
            controls_should_show,
            0.18,
        );

        // Buffering spinner (stop showing once mpv enters idle/aborted state)
        let is_initial_wait = self.watch.is_some()
            && !self.audio_only_mode
            && self.playback.video_h == 0
            && !self.playback.paused
            && !self.playback.idle;
        let shorts_grace = is_shorts
            && self
                .shorts_opened_at
                .is_some_and(|t| t.elapsed() < Duration::from_millis(550))
            && !self.playback.buffering;
        if (self.watch_loading || self.playback.buffering || is_initial_wait) && !is_sliding_short
        {
            ctx.request_repaint();
            if shorts_grace {
                draw_shorts_bottom_shimmer(ui, ctx, rect);
            } else {
                let label = if self.watch_loading {
                    "Resolving stream...".to_owned()
                } else if self.playback.cache_duration > 0.05 {
                    format!("Buffering · {:.1}s", self.playback.cache_duration)
                } else {
                    "Buffering...".to_owned()
                };
                draw_buffering_overlay(ui, ctx, rect, radius, &label);
            }
        }

        // Center HUD feedback (Play/Pause/Seek pulse)
        self.draw_hud_feedback(ui, ctx, rect);

        // Stats for Nerds floating card (top-left)
        if self.show_stats_for_nerds {
            self.draw_stats_for_nerds(ui, rect);
        }

        // Fullscreen top title scrim
        if is_fullscreen && overlay_alpha > 0.01 {
            if let Some(w) = &self.watch {
                let top_scrim = Rect::from_min_max(
                    rect.min,
                    pos2(rect.right(), (rect.top() + 86.0).min(rect.bottom())),
                );
                paint_vertical_gradient(
                    ui.painter(),
                    top_scrim,
                    Color32::from_black_alpha((195.0 * overlay_alpha).round() as u8),
                    Color32::TRANSPARENT,
                );
                ui.painter().text(
                    pos2(rect.left() + 24.0, rect.top() + 26.0),
                    Align2::LEFT_CENTER,
                    &w.title,
                    theme::semibold(16.0),
                    Color32::from_white_alpha((245.0 * overlay_alpha).round() as u8),
                );
                ui.painter().text(
                    pos2(rect.left() + 24.0, rect.top() + 48.0),
                    Align2::LEFT_CENTER,
                    &w.channel,
                    theme::regular(12.5),
                    Color32::from_white_alpha((185.0 * overlay_alpha).round() as u8),
                );
            }
        }

        // Overlaid bottom gradient + YouTube controls + Shorts bottom-left channel/title overlay
        let mut clicked_overlay = audio_overlay_hit;
        let mut toggled_menu = false;
        if is_shorts {
            let base_scrim_h = 148.0_f32.min(rect.height() * 0.45);
            let base_scrim = Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - base_scrim_h),
                rect.max,
            );
            paint_vertical_gradient_rounded(
                ui.painter(),
                base_scrim,
                Color32::TRANSPARENT,
                Color32::from_black_alpha(175),
                radius,
            );
            if let Some(w) = self.watch.clone() {
                let lift_y = overlay_alpha * 54.0;
                let info_rect = Rect::from_min_max(
                    pos2(rect.left() + 14.0, (rect.bottom() - 150.0 - lift_y).max(rect.top() + 12.0)),
                    pos2(rect.right() - 14.0, (rect.bottom() - 12.0 - lift_y).max(rect.top() + 36.0)),
                );
                if pointer_pos.is_some_and(|p| info_rect.contains(p)) {
                    clicked_overlay = true;
                }
                let mut iui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(info_rect)
                        .layout(Layout::bottom_up(Align::LEFT)),
                );
                iui.set_clip_rect(iui.clip_rect().intersect(rect));
                let title_display =
                    clamp_two_lines(&iui, &w.title, theme::semibold(13.5), info_rect.width());
                iui.add(
                    egui::Label::new(
                        RichText::new(title_display)
                            .font(theme::semibold(13.5))
                            .color(Color32::WHITE),
                    )
                    .wrap(),
                )
                .on_hover_text(&w.title);
                if w.is_members_only {
                    iui.add_space(4.0);
                    draw_members_badge(&mut iui);
                }
                iui.add_space(6.0);
                iui.allocate_ui_with_layout(
                    Vec2::new(info_rect.width(), 34.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let av_resp = ui
                            .horizontal(|ui| {
                                draw_avatar(ui, w.channel_avatar_url.as_deref(), &w.channel, 30.0);
                            })
                            .response
                            .interact(Sense::click());
                        if av_resp.hovered() {
                            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                        }
                        if av_resp.clicked() {
                            if let Some(cid) = w.channel_id.clone() {
                                open_channel = Some((cid, w.channel.clone()));
                            }
                            clicked_overlay = true;
                        }
                        let sub_reserve = if !w.is_subscribed {
                            104.0
                        } else if w.notifications.level == crate::model::NotificationLevel::Personalized {
                            134.0
                        } else {
                            96.0
                        };
                        let name_max_w = (info_rect.width() - 30.0 - 16.0 - sub_reserve).max(70.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(name_max_w, 34.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let ch_btn = ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(&w.channel)
                                                .font(theme::semibold(13.5))
                                                .color(Color32::WHITE),
                                        )
                                        .truncate()
                                        .sense(Sense::click()),
                                    )
                                    .on_hover_text(&w.channel);
                                if ch_btn.hovered() && w.channel_id.is_some() {
                                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                }
                                if ch_btn.clicked() {
                                    if let Some(cid) = w.channel_id.clone() {
                                        open_channel = Some((cid, w.channel.clone()));
                                    }
                                    clicked_overlay = true;
                                }
                            },
                        );
                        if let Some(cid) = w.channel_id.clone() {
                            super::cards::draw_subscribe_bell_button(
                                ui,
                                cid,
                                w.is_subscribed,
                                &w.notifications,
                                &self.backend.tx,
                            );
                        }
                    },
                );
            } else if let Some(q) = self.shorts_queue.get(self.shorts_idx).cloned() {
                let lift_y = overlay_alpha * 54.0;
                let info_rect = Rect::from_min_max(
                    pos2(rect.left() + 14.0, (rect.bottom() - 150.0 - lift_y).max(rect.top() + 12.0)),
                    pos2(rect.right() - 14.0, (rect.bottom() - 12.0 - lift_y).max(rect.top() + 36.0)),
                );
                let mut iui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(info_rect)
                        .layout(Layout::bottom_up(Align::LEFT)),
                );
                iui.set_clip_rect(iui.clip_rect().intersect(rect));
                let title_display =
                    clamp_two_lines(&iui, &q.title, theme::semibold(13.5), info_rect.width());
                iui.add(
                    egui::Label::new(
                        RichText::new(title_display)
                            .font(theme::semibold(13.5))
                            .color(Color32::WHITE),
                    )
                    .wrap(),
                );
                if !q.channel.is_empty() {
                    iui.add_space(6.0);
                    iui.horizontal(|ui| {
                        draw_avatar(ui, q.channel_avatar_url.as_deref(), &q.channel, 30.0);
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&q.channel)
                                    .font(theme::semibold(13.5))
                                    .color(Color32::WHITE),
                            )
                            .truncate(),
                        );
                    });
                }
            }
        }
        if overlay_alpha > 0.01 {
            let scrim_h = 104.0_f32.min(rect.height() * 0.55);
            let scrim_rect = Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - scrim_h),
                rect.max,
            );
            paint_vertical_gradient_rounded(
                ui.painter(),
                scrim_rect,
                Color32::TRANSPARENT,
                Color32::from_black_alpha((220.0 * overlay_alpha).round() as u8),
                radius,
            );
            let (interacted, toggled) =
                self.draw_overlaid_controls(ui, ctx, rect, overlay_alpha, is_shorts);
            clicked_overlay |= interacted;
            toggled_menu = toggled;
        }

        if is_shorts && radius > 0.5 {
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, Color32::from_white_alpha(22)),
                egui::StrokeKind::Inside,
            );
        }

        // Floating Settings / CC popovers above the bottom-right controls
        let popup_open = self.player_menu != PlayerMenu::None;
        let popup_hit = self.draw_player_popovers(ctx, rect, toggled_menu);

        // Video surface click (toggle pause) & double click (fullscreen), ignoring control hits
        if !pointer_in_controls && !clicked_overlay && !popup_hit {
            if !is_shorts && resp.double_clicked() {
                self.toggle_fullscreen(ctx);
                self.last_pointer_move = Instant::now();
            } else if resp.clicked() {
                if popup_open {
                    self.player_menu = PlayerMenu::None;
                } else if let Some(mpv) = &self.mpv {
                    mpv.toggle_pause();
                    let next_paused = !self.playback.paused;
                    self.trigger_hud(
                        if next_paused {
                            HudIcon::Pause
                        } else {
                            HudIcon::Play
                        },
                        if next_paused { "Paused" } else { "Playing" },
                    );
                }
                self.last_pointer_move = Instant::now();
            }
        }
        open_channel
    }

    fn draw_overlaid_controls(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        video_rect: Rect,
        alpha: f32,
        is_shorts: bool,
    ) -> (bool, bool) {
        let mut any_interacted = false;
        let mut toggled_menu = false;
        let dur = if self.playback.duration > 0.0 {
            self.playback.duration
        } else {
            self.watch.as_ref().map_or(0.0, |w| w.duration_secs)
        };
        let mut pos = self.seeking_pos.unwrap_or(self.playback.time_pos);
        let is_live = self.watch.as_ref().is_some_and(|w| w.is_live);
        let live_behind = (dur - pos).max(0.0);
        let at_live_edge =
            is_live && self.seeking_pos.is_none() && (dur <= 0.0 || live_behind <= 12.0);

        let pad_x = 14.0;
        let controls_h = 52.0;
        let controls_rect = Rect::from_min_max(
            pos2(
                video_rect.left() + pad_x,
                (video_rect.bottom() - controls_h - 4.0).max(video_rect.top()),
            ),
            pos2(video_rect.right() - pad_x, video_rect.bottom() - 4.0),
        );

        let mut cui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(controls_rect)
                .layout(Layout::top_down(Align::LEFT)),
        );
        cui.spacing_mut().item_spacing.y = 0.0;
        let cclip = cui.clip_rect().intersect(video_rect);
        cui.set_clip_rect(cclip);

        // 1. Slim interactive seekbar (animates thickness & knob on hover/drag)
        let bar_w = controls_rect.width();
        let (bar_rect, bar_resp) =
            cui.allocate_exact_size(Vec2::new(bar_w, 12.0), Sense::click_and_drag());
        let bar_hot = bar_resp.hovered() || bar_resp.dragged();
        if bar_hot {
            any_interacted = true;
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }
        let bar_hot_t = ctx.animate_bool_with_time(
            egui::Id::unique("seekbar_hot_anim"),
            bar_hot,
            0.12,
        );
        let h = 3.0 + 2.8 * bar_hot_t;
        let track = Rect::from_center_size(bar_rect.center(), Vec2::new(bar_rect.width(), h));

        cui.painter().rect_filled(
            track,
            h * 0.5,
            Color32::from_white_alpha((55.0 * alpha).round() as u8),
        );

        if dur > 0.0 || is_live {
            if !is_live && dur > 0.0 && self.playback.cache_duration > 0.0 {
                let buf_pos = self.playback.time_pos.max(0.0);
                let buf_frac =
                    ((buf_pos + self.playback.cache_duration) / dur).clamp(0.0, 1.0) as f32;
                let buf_rect = Rect::from_min_max(
                    track.min,
                    pos2(track.left() + track.width() * buf_frac, track.bottom()),
                );
                cui.painter().rect_filled(
                    buf_rect,
                    h * 0.5,
                    Color32::from_white_alpha((145.0 * alpha).round() as u8),
                );
            }

            if !is_live && dur > 0.0 {
                for seg in &self.sponsor_segments {
                    if !seg.fits(dur) {
                        continue;
                    }
                    let x0 =
                        track.left() + (seg.start / dur).clamp(0.0, 1.0) as f32 * track.width();
                    let x1 = track.left() + (seg.end / dur).clamp(0.0, 1.0) as f32 * track.width();
                    let seg_rect = Rect::from_min_max(
                        pos2(x0, track.top()),
                        pos2(x1.max(x0 + 2.0), track.bottom()),
                    );
                    let color = match seg.category {
                        SponsorCategory::Sponsor => Color32::from_rgb(0, 212, 0),
                        SponsorCategory::SelfPromo => Color32::from_rgb(255, 255, 0),
                        SponsorCategory::Interaction => Color32::from_rgb(204, 0, 255),
                        _ => Color32::from_rgb(0, 200, 230),
                    }
                    .gamma_multiply(alpha);
                    cui.painter().rect_filled(seg_rect, 1.5, color);
                }
            }

            let frac = if at_live_edge {
                1.0
            } else if dur > 0.0 {
                (pos / dur).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let fill = Rect::from_min_max(
                track.min,
                pos2(track.left() + track.width() * frac, track.bottom()),
            );
            cui.painter()
                .rect_filled(fill, h * 0.5, theme::YT_RED.gamma_multiply(alpha));

            // Subtle chapter dividers along the seekbar
            if !is_live && dur > 0.0 {
                if let Some(w) = &self.watch {
                    for ch in &w.chapters {
                        if ch.start_secs > 1.0 && ch.start_secs < dur {
                            let cx = track.left() + (ch.start_secs / dur) as f32 * track.width();
                            cui.painter().line_segment(
                                [pos2(cx, track.top()), pos2(cx, track.bottom())],
                                Stroke::new(2.0, Color32::from_black_alpha((200.0 * alpha) as u8)),
                            );
                        }
                    }
                }
            }

            // A-B Loop span & markers along seekbar
            if !is_live && dur > 0.0 && (self.ab_loop_a.is_some() || self.ab_loop_b.is_some()) {
                let loop_col = Color32::from_rgb(76, 217, 175).gamma_multiply(alpha);
                if let (Some(a), Some(b)) = (self.ab_loop_a, self.ab_loop_b) {
                    let x0 = track.left() + (a / dur).clamp(0.0, 1.0) as f32 * track.width();
                    let x1 = track.left() + (b / dur).clamp(0.0, 1.0) as f32 * track.width();
                    if x1 > x0 {
                        cui.painter().rect_filled(
                            Rect::from_min_max(pos2(x0, track.top()), pos2(x1, track.bottom())),
                            h * 0.5,
                            loop_col,
                        );
                    }
                }
                for (lbl, t_opt) in [("A", self.ab_loop_a), ("B", self.ab_loop_b)] {
                    if let Some(t) = t_opt {
                        let mx = track.left() + (t / dur).clamp(0.0, 1.0) as f32 * track.width();
                        cui.painter().line_segment(
                            [pos2(mx, track.top() - 4.0), pos2(mx, track.bottom() + 3.0)],
                            Stroke::new(2.0, loop_col),
                        );
                        cui.painter().text(
                            pos2(mx, track.top() - 10.0),
                            Align2::CENTER_CENTER,
                            lbl,
                            theme::bold(9.5),
                            loop_col,
                        );
                    }
                }
            }

            let knob_x = track.left() + track.width() * frac;
            let knob_r = 3.5 + 3.2 * bar_hot_t;
            cui.painter().circle_filled(
                pos2(knob_x, track.center().y),
                knob_r,
                theme::YT_RED.gamma_multiply(alpha),
            );

            if bar_hot && dur > 0.0 {
                if let Some(ptr) = ctx.input(|i| i.pointer.hover_pos()) {
                    let hf = ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
                    let h_secs = hf * dur;
                    let tip = if is_live {
                        let behind = (dur - h_secs).max(0.0);
                        if behind <= 5.0 {
                            "Live".to_owned()
                        } else {
                            format!("-{}", format_duration(behind))
                        }
                    } else {
                        let ch_title = self.watch.as_ref().and_then(|w| {
                            w.chapters
                                .iter()
                                .rfind(|c| c.start_secs <= h_secs)
                                .map(|c| c.title.as_str())
                        });
                        match ch_title {
                            Some(t) => format!("{} · {t}", format_duration(h_secs)),
                            None => format_duration(h_secs),
                        }
                    };
                    let galley = cui.painter().layout_no_wrap(
                        tip,
                        theme::medium(11.5),
                        Color32::WHITE,
                    );
                    let pad = vec2(8.0, 4.0);
                    let box_sz = galley.size() + pad * 2.0;
                    let tip_x = ptr.x.clamp(
                        track.left() + box_sz.x * 0.5,
                        (track.right() - box_sz.x * 0.5).max(track.left() + box_sz.x * 0.5),
                    );
                    let tip_rect = Rect::from_min_size(
                        pos2(tip_x - box_sz.x * 0.5, track.top() - box_sz.y - 10.0),
                        box_sz,
                    );
                    cui.painter()
                        .rect_filled(tip_rect, 6.0, Color32::from_black_alpha(225));
                    cui.painter()
                        .galley(tip_rect.min + pad, galley, Color32::WHITE);

                    if let Some(sb) = self.watch.as_ref().and_then(|w| w.storyboard.as_ref()) {
                        if let Some((sheet_url, uv)) = sb.frame_at(h_secs, dur) {
                            let prev_w = 160.0;
                            let prev_h = if sb.width > 0 && sb.height > 0 {
                                (prev_w * (sb.height as f32 / sb.width as f32)).clamp(60.0, 140.0)
                            } else {
                                90.0
                            };
                            let prev_sz = vec2(prev_w, prev_h);
                            let prev_x = ptr.x.clamp(
                                track.left() + prev_sz.x * 0.5,
                                (track.right() - prev_sz.x * 0.5)
                                    .max(track.left() + prev_sz.x * 0.5),
                            );
                            let prev_rect = Rect::from_min_size(
                                pos2(prev_x - prev_sz.x * 0.5, tip_rect.top() - prev_sz.y - 6.0),
                                prev_sz,
                            );
                            cui.painter()
                                .rect_filled(prev_rect.expand(2.0), 8.0, Color32::BLACK);
                            egui::Image::new(sheet_url)
                                .show_loading_spinner(false)
                                .uv(Rect::from_min_max(pos2(uv[0], uv[1]), pos2(uv[2], uv[3])))
                                .corner_radius(CornerRadius::same(6))
                                .fit_to_exact_size(prev_sz)
                                .paint_at(&cui, prev_rect);
                            cui.painter().rect_stroke(
                                prev_rect,
                                6.0,
                                Stroke::new(1.5, Color32::from_white_alpha(85)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                }
            }

            if bar_resp.dragged() || bar_resp.clicked() {
                if let Some(ptr) = bar_resp.interact_pointer_pos() {
                    let f = ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
                    pos = f * dur;
                    self.seeking_pos = Some(pos);
                    self.last_pointer_move = Instant::now();
                }
            }
            if bar_resp.drag_stopped() || bar_resp.clicked() {
                if let Some(target) = self.seeking_pos.take() {
                    let seek_to = if is_live && dur > 4.0 && target >= (dur - 4.0) {
                        (dur - 2.0).max(0.0)
                    } else if !is_live && dur > 2.0 && target >= (dur - 0.25) {
                        (dur - 0.25).max(0.0)
                    } else {
                        target
                    };
                    if let Some(mpv) = &self.mpv {
                        mpv.seek_abs(seek_to);
                        if self.playback.eof {
                            mpv.set_paused(false);
                        }
                    }
                }
            }
        }

        // 2. Clean YouTube control row
        cui.add_space(2.0);
        let row_w = cui.available_width();
        let (row_rect, _) = cui.allocate_exact_size(Vec2::new(row_w, 36.0), Sense::hover());
        let right_w = if is_shorts && !self.fullscreen {
            156.0_f32.min(row_w * 0.48)
        } else {
            340.0_f32.min(row_w * 0.56)
        };
        let left_w = (row_w - right_w - 6.0).max(80.0);

        let left_rect = Rect::from_min_max(row_rect.min, pos2(row_rect.left() + left_w, row_rect.max.y));
        let right_rect =
            Rect::from_min_max(pos2(row_rect.right() - right_w, row_rect.top()), row_rect.max);

        let white = Color32::from_white_alpha((255.0 * alpha).round() as u8);
        let dim = Color32::from_white_alpha((195.0 * alpha).round() as u8);

        // Left controls: Play/Pause, Next, Volume (with smooth expanding slider), Timestamp, Chapter
        let mut left_ui = cui.new_child(
            egui::UiBuilder::new()
                .max_rect(left_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let lclip = left_ui.clip_rect().intersect(left_rect);
        left_ui.set_clip_rect(lclip);
        left_ui.horizontal_centered(|ui| {
            let play_ico = if self.playback.paused {
                icon::PLAY
            } else {
                icon::PAUSE
            };
            if overlay_icon_btn(ui, play_ico, 18.0, white, "Play/Pause (Space / K)") {
                any_interacted = true;
                if let Some(mpv) = &self.mpv {
                    mpv.toggle_pause();
                    let next_paused = !self.playback.paused;
                    self.trigger_hud(
                        if next_paused {
                            HudIcon::Pause
                        } else {
                            HudIcon::Play
                        },
                        if next_paused { "Paused" } else { "Playing" },
                    );
                }
            }

            if !is_shorts {
                let next_item = self
                    .queue
                    .first()
                    .cloned()
                    .or_else(|| self.watch.as_ref().and_then(|w| w.related.first().cloned()));
                if let Some(next_vid) = next_item {
                    let (nrect, nresp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                    let nhov = ui
                        .ctx()
                        .animate_bool_with_time(nresp.id, nresp.hovered(), 0.12);
                    if nhov > 0.01 {
                        ui.painter().circle_filled(
                            nrect.center(),
                            16.0,
                            Color32::from_white_alpha((28.0 * nhov * alpha) as u8),
                        );
                    }
                    if nresp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    theme::draw_next_icon(ui.painter(), nrect.center(), 17.0, white);
                    if nresp
                        .on_hover_text(format!("Next: {}", next_vid.title))
                        .clicked()
                    {
                        any_interacted = true;
                        if !self.queue.is_empty() {
                            let v = self.queue.remove(0);
                            self.persist_queue();
                            self.open_video(v);
                        } else {
                            self.open_video(next_vid);
                        }
                    }
                }
            }

            // Volume icon + smooth hover-expanding slider (just like YouTube!)
            let vol_ico = if self.muted || self.volume < 0.5 {
                icon::VOLUME_X
            } else if self.volume < 50.0 {
                icon::VOLUME_1
            } else {
                icon::VOLUME_2
            };
            let vol_area_hovered = ui.rect_contains_pointer(Rect::from_min_size(
                ui.cursor().min,
                vec2(106.0, 34.0),
            ));
            let vol_expand_t = ui.ctx().animate_bool_with_time(
                egui::Id::unique("player_vol_expand"),
                vol_area_hovered,
                0.16,
            );
            if overlay_icon_btn(ui, vol_ico, 17.0, white, "Mute (M)") {
                any_interacted = true;
                self.muted = !self.muted;
                if let Some(mpv) = &self.mpv {
                    mpv.set_mute(self.muted);
                }
            }
            let slider_w = 58.0 * theme::ease_out_cubic(vol_expand_t);
            if slider_w > 2.0 {
                let (srect, sresp) =
                    ui.allocate_exact_size(Vec2::new(slider_w, 24.0), Sense::click_and_drag());
                let track_y = srect.center().y;
                let tr = Rect::from_min_max(
                    pos2(srect.left() + 4.0, track_y - 1.5),
                    pos2(srect.right() - 4.0, track_y + 1.5),
                );
                ui.painter().rect_filled(
                    tr,
                    1.5,
                    Color32::from_white_alpha((75.0 * alpha).round() as u8),
                );
                let vf = if self.muted {
                    0.0
                } else {
                    (self.volume / 100.0).clamp(0.0, 1.0) as f32
                };
                let vx = tr.left() + tr.width() * vf;
                ui.painter().rect_filled(
                    Rect::from_min_max(tr.min, pos2(vx, tr.max.y)),
                    1.5,
                    white,
                );
                ui.painter().circle_filled(pos2(vx, track_y), 5.0, white);
                if sresp.hovered() || sresp.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if sresp.dragged() || sresp.clicked() {
                    any_interacted = true;
                    if let Some(p) = sresp.interact_pointer_pos() {
                        let new_vol =
                            (((p.x - tr.left()) / tr.width().max(1.0)).clamp(0.0, 1.0) * 100.0)
                                as f64;
                        self.volume = new_vol;
                        self.muted = false;
                        if let Some(mpv) = &self.mpv {
                            mpv.set_mute(false);
                            mpv.set_volume(new_vol);
                        }
                    }
                }
            }

            ui.add_space(4.0);
            if is_live {
                let dot_col = if at_live_edge {
                    theme::YT_RED.gamma_multiply(alpha)
                } else {
                    dim
                };
                let live_label = if at_live_edge {
                    "Live".to_owned()
                } else {
                    format!("Live  ·  -{}", format_duration(live_behind))
                };
                let text_galley = ui.painter().layout_no_wrap(
                    live_label.clone(),
                    theme::semibold(12.2),
                    white,
                );
                let pill_w = 24.0 + text_galley.size().x;
                let (lrect, lresp) =
                    ui.allocate_exact_size(Vec2::new(pill_w, 24.0), Sense::click());
                if lresp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if lresp.hovered() || !at_live_edge {
                    ui.painter().rect_filled(
                        lrect,
                        12.0,
                        Color32::from_white_alpha((26.0 * alpha).round() as u8),
                    );
                }
                ui.painter()
                    .circle_filled(pos2(lrect.left() + 10.0, lrect.center().y), 3.6, dot_col);
                ui.painter().text(
                    pos2(lrect.left() + 18.0, lrect.center().y),
                    Align2::LEFT_CENTER,
                    live_label,
                    theme::semibold(12.2),
                    white,
                );
                if lresp
                    .on_hover_text(if at_live_edge {
                        "Watching live"
                    } else {
                        "Jump back to Live"
                    })
                    .clicked()
                    && dur > 2.0
                {
                    any_interacted = true;
                    if let Some(mpv) = &self.mpv {
                        mpv.seek_abs((dur - 2.0).max(0.0));
                    }
                }
            } else {
                ui.label(
                    RichText::new(format!(
                        "{} / {}",
                        format_duration(pos),
                        format_duration(dur)
                    ))
                    .font(theme::medium(12.2))
                    .color(white),
                );
            }

            if let Some(ch_title) = self.watch.as_ref().and_then(|w| {
                w.chapters
                    .iter()
                    .rfind(|c| c.start_secs <= pos)
                    .map(|c| c.title.clone())
            }) {
                let rem = ui.available_width();
                if rem > 60.0 {
                    let ch_resp = ui.add_sized(
                        [rem, 22.0],
                        egui::Label::new(
                            RichText::new(format!("• {ch_title}  ›"))
                                .font(theme::medium(12.0))
                                .color(dim),
                        )
                        .truncate()
                        .sense(Sense::click()),
                    );
                    if ch_resp.clicked() {
                        any_interacted = true;
                        self.show_chapters = !self.show_chapters;
                    }
                }
            }
        });

        // Right controls: Autoplay toggle, CC, Settings Gear, Theater, Fullscreen
        let mut right_ui = cui.new_child(
            egui::UiBuilder::new()
                .max_rect(right_rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let rclip = right_ui.clip_rect().intersect(right_rect);
        right_ui.set_clip_rect(rclip);
        right_ui.spacing_mut().item_spacing.x = 3.0;
        {
            let ui = &mut right_ui;

            // Fullscreen
            let (fs_rect, fs_resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
            paint_hover_circle(ui, fs_rect, &fs_resp, alpha);
            theme::draw_fullscreen_icon(ui.painter(), fs_rect.center(), 17.0, white, self.fullscreen);
            if fs_resp
                .on_hover_text(if self.fullscreen {
                    "Exit full screen (F)"
                } else {
                    "Full screen (F)"
                })
                .clicked()
            {
                any_interacted = true;
                self.toggle_fullscreen(ctx);
            }

            // Windows PiP (borderless always-on-top video-only window)
            if !is_shorts {
                let (pip_rect, pip_resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                paint_hover_circle(ui, pip_rect, &pip_resp, alpha);
                draw_win_pip_icon(ui.painter(), pip_rect.center(), 16.0, white);
                if pip_resp
                    .on_hover_text("Windows PiP (float video-only window)")
                    .clicked()
                {
                    any_interacted = true;
                    self.toggle_win_pip(ctx);
                }
            }

            // Screenshot (sends to DropBoard when running, otherwise copies to Windows clipboard)
            let (ss_rect, ss_resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
            paint_hover_circle(ui, ss_rect, &ss_resp, alpha);
            theme::draw_camera_icon(
                ui.painter(),
                ss_rect.center(),
                17.0,
                white,
                self.dropboard_online,
            );
            let ss_tip = if self.dropboard_online {
                format!(
                    "Screenshot -> DropBoard ({})",
                    self.shortcuts.screenshot.display()
                )
            } else {
                format!(
                    "Screenshot to Clipboard ({})",
                    self.shortcuts.screenshot.display()
                )
            };
            if ss_resp.on_hover_text(ss_tip).clicked() {
                any_interacted = true;
                self.capture_video_screenshot();
            }

            // Theater mode (hidden in fullscreen and Shorts mode)
            if !self.fullscreen && !is_shorts {
                let (th_rect, th_resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                paint_hover_circle(ui, th_rect, &th_resp, alpha);
                theme::draw_theater_icon(
                    ui.painter(),
                    th_rect.center(),
                    17.0,
                    white,
                    self.theater_mode,
                );
                if th_resp.on_hover_text("Theater mode (T)").clicked() {
                    any_interacted = true;
                    self.theater_mode = !self.theater_mode;
                }
            }

            // Sleep timer quick toggle button
            let sleep_active = self.sleep_timer != SleepTimer::Off;
            let (sl_rect, sl_resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
            paint_hover_circle(ui, sl_rect, &sl_resp, alpha);
            theme::draw_sleep_timer_icon(
                ui.ctx(),
                ui.painter(),
                sl_rect.center(),
                17.0,
                if sleep_active || self.player_menu == PlayerMenu::SleepTimer {
                    theme::YT_BLUE.gamma_multiply(alpha)
                } else {
                    white
                },
            );
            if sleep_active {
                let dot_r = Rect::from_min_size(
                    pos2(sl_rect.right() - 10.0, sl_rect.top() + 6.0),
                    vec2(6.0, 6.0),
                );
                ui.painter().circle_filled(
                    dot_r.center(),
                    3.0,
                    theme::YT_BLUE.gamma_multiply(alpha),
                );
            }
            let sleep_tip = match self.sleep_timer_remaining_label() {
                Some(rem) => format!("Sleep timer: {rem}"),
                None => "Sleep timer".to_owned(),
            };
            if sl_resp.on_hover_text(sleep_tip).clicked() {
                any_interacted = true;
                toggled_menu = true;
                self.player_menu = if self.player_menu == PlayerMenu::SleepTimer {
                    PlayerMenu::None
                } else {
                    PlayerMenu::SleepTimer
                };
            }

            // Settings Gear button (opens speed, quality, audio track, stats for nerds)
            let (gear_rect, gear_resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
            paint_hover_circle(ui, gear_rect, &gear_resp, alpha);
            theme::draw_icon_centered(
                ui.painter(),
                gear_rect.center(),
                icon::SETTINGS,
                17.0,
                if self.player_menu == PlayerMenu::Settings {
                    theme::YT_RED.gamma_multiply(alpha)
                } else {
                    white
                },
            );
            // Tiny HD badge on gear if quality >= 720p
            let is_hd = self
                .selected_format_idx
                .and_then(|i| self.watch.as_ref()?.video_formats.get(i))
                .is_some_and(|f| f.height >= 720)
                || (self.selected_format_idx.is_none() && self.playback.video_h >= 720);
            if is_hd {
                let hd_rect = Rect::from_min_size(
                    pos2(gear_rect.right() - 15.0, gear_rect.top() + 4.0),
                    vec2(14.0, 9.0),
                );
                ui.painter().rect_filled(
                    hd_rect,
                    2.0,
                    theme::YT_RED.gamma_multiply(alpha),
                );
                ui.painter().text(
                    hd_rect.center(),
                    Align2::CENTER_CENTER,
                    "HD",
                    theme::bold(6.5),
                    white,
                );
            }
            if gear_resp.on_hover_text("Settings (Quality, Speed, Audio, Stats)").clicked() {
                any_interacted = true;
                toggled_menu = true;
                self.player_menu = if self.player_menu == PlayerMenu::Settings {
                    PlayerMenu::None
                } else {
                    PlayerMenu::Settings
                };
            }

            // Captions (CC) button (hidden in compact Shorts bar unless active)
            let has_cc = !is_shorts
                && self
                    .watch
                    .as_ref()
                    .is_some_and(|w| !w.captions.is_empty());
            if has_cc {
                let (cc_rect, cc_resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                paint_hover_circle(ui, cc_rect, &cc_resp, alpha);
                let box_r = Rect::from_center_size(cc_rect.center(), vec2(22.0, 16.0));
                ui.painter().rect_stroke(
                    box_r,
                    3.0,
                    Stroke::new(1.5, white),
                    egui::StrokeKind::Middle,
                );
                ui.painter().text(
                    box_r.center(),
                    Align2::CENTER_CENTER,
                    "CC",
                    theme::bold(8.5),
                    white,
                );
                if self.selected_caption_idx.is_some() || self.secondary_caption_idx.is_some() {
                    ui.painter().line_segment(
                        [
                            pos2(box_r.left() + 2.0, box_r.bottom() + 2.5),
                            pos2(box_r.right() - 2.0, box_r.bottom() + 2.5),
                        ],
                        Stroke::new(2.0, theme::YT_RED.gamma_multiply(alpha)),
                    );
                }
                if cc_resp.on_hover_text("Subtitles/closed captions").clicked() {
                    any_interacted = true;
                    toggled_menu = true;
                    self.player_menu = if self.player_menu == PlayerMenu::Captions {
                        PlayerMenu::None
                    } else {
                        PlayerMenu::Captions
                    };
                }
            }

            if !is_shorts {
                // YouTube-style Autoplay pill switch
                let (ap_rect, ap_resp) =
                    ui.allocate_exact_size(Vec2::new(34.0, 24.0), Sense::click());
                let ap_t = ui.ctx().animate_bool_with_time(
                    egui::Id::unique("player_autoplay_switch"),
                    self.autoplay_next,
                    0.15,
                );
                if ap_resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let pill = Rect::from_center_size(ap_rect.center(), vec2(26.0, 12.0));
                ui.painter().rect_filled(
                    pill,
                    6.0,
                    Color32::from_white_alpha(((60.0 + 70.0 * ap_t) * alpha).round() as u8),
                );
                let knob_x = pill.left() + 6.0 + 14.0 * ap_t;
                let knob_c = pos2(knob_x, pill.center().y);
                ui.painter().circle_filled(knob_c, 7.0, white);
                let glyph_col = Color32::from_black_alpha((220.0 * alpha).round() as u8);
                if self.autoplay_next {
                    let tri = vec![
                        pos2(knob_c.x - 2.0, knob_c.y - 3.0),
                        pos2(knob_c.x + 3.0, knob_c.y),
                        pos2(knob_c.x - 2.0, knob_c.y + 3.0),
                    ];
                    ui.painter()
                        .add(egui::Shape::convex_polygon(tri, glyph_col, Stroke::NONE));
                } else {
                    for dx in [-1.8f32, 1.2] {
                        ui.painter().rect_filled(
                            Rect::from_min_size(pos2(knob_c.x + dx, knob_c.y - 2.8), vec2(1.4, 5.6)),
                            0.4,
                            glyph_col,
                        );
                    }
                }
                if ap_resp
                    .on_hover_text(if self.autoplay_next {
                        "Autoplay is on"
                    } else {
                        "Autoplay is off"
                    })
                    .clicked()
                {
                    any_interacted = true;
                    self.autoplay_next = !self.autoplay_next;
                }
            }
        }

        (any_interacted, toggled_menu)
    }

    fn draw_player_popovers(
        &mut self,
        ctx: &egui::Context,
        video_rect: Rect,
        just_toggled: bool,
    ) -> bool {
        if self.player_menu == PlayerMenu::None {
            return false;
        }
        let mut interacted = false;
        let menu_w = 352.0_f32.min((video_rect.width() - 24.0).max(260.0));
        let max_pop_h = (video_rect.height() - 96.0).clamp(200.0, 560.0);
        let anchor_bottom_right = pos2(
            (video_rect.right() - 14.0).max(video_rect.left() + menu_w + 8.0),
            (video_rect.bottom() - 68.0).max(video_rect.top() + 80.0),
        );

        let area_resp = egui::Area::new(egui::Id::unique("watch_player_popover"))
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(anchor_bottom_right)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(24, 24, 28, 246))
                    .stroke(Stroke::new(1.0, Color32::from_white_alpha(28)))
                    .corner_radius(12)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 2,
                        color: Color32::from_black_alpha(185),
                    })
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        let inner_w = menu_w - 24.0;
                        ui.set_width(inner_w);
                        match self.player_menu {
                            PlayerMenu::Settings => {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Player Settings")
                                            .font(theme::semibold(13.5))
                                            .color(Color32::WHITE),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if popover_close_btn(ui) {
                                            self.player_menu = PlayerMenu::None;
                                        }
                                    });
                                });
                                ui.add_space(6.0);

                                // Playback speed pills: single symmetric 7-pill row
                                ui.label(
                                    RichText::new("Playback speed")
                                        .font(theme::medium(11.5))
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add_space(5.0);
                                let speeds = [0.5_f64, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
                                let gap = 4.0_f32;
                                let pill_w = (inner_w - gap * (speeds.len() as f32 - 1.0))
                                    / speeds.len() as f32;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    for s in speeds {
                                        let active = (self.speed - s).abs() < 0.01;
                                        let label = if (s - 1.0).abs() < 0.01 {
                                            "1x".to_owned()
                                        } else if (s - 2.0).abs() < 0.01 {
                                            "2x".to_owned()
                                        } else {
                                            format!("{s}x")
                                        };
                                        if popover_pill_button(
                                            ui,
                                            vec2(pill_w, 26.0),
                                            &label,
                                            active,
                                            false,
                                        ) {
                                            self.speed = s;
                                            if let Some(mpv) = &self.mpv {
                                                mpv.set_speed(s);
                                            }
                                            interacted = true;
                                        }
                                    }
                                });

                                // Quality list
                                let mut picked_qual = None;
                                let mut picked_auto = false;
                                let mut picked_audio = None;
                                if let Some(w) = &self.watch {
                                    if !w.video_formats.is_empty() || w.hls_url.is_some() {
                                        ui.add_space(8.0);
                                        ui.separator();
                                        ui.add_space(6.0);
                                        ui.label(
                                            RichText::new("Quality")
                                                .font(theme::medium(11.5))
                                                .color(theme::TEXT_DIM),
                                        );
                                        ui.add_space(4.0);
                                        ScrollArea::vertical()
                                            .id_salt("popover_quality_scroll")
                                            .max_height(136.0)
                                            .show(ui, |ui| {
                                                if w.hls_url.is_some()
                                                    && popover_option_row(
                                                        ui,
                                                        "Auto",
                                                        self.selected_format_idx.is_none(),
                                                    )
                                                {
                                                    picked_auto = true;
                                                }
                                                for (i, f) in w.video_formats.iter().enumerate() {
                                                    let active = self.selected_format_idx == Some(i);
                                                    if popover_option_row(ui, &f.label(), active) {
                                                        picked_qual = Some(i);
                                                    }
                                                }
                                            });
                                    }
                                    if w.audio_tracks.len() > 1 {
                                        ui.add_space(8.0);
                                        ui.separator();
                                        ui.add_space(6.0);
                                        ui.label(
                                            RichText::new("Audio track")
                                                .font(theme::medium(11.5))
                                                .color(theme::TEXT_DIM),
                                        );
                                        ui.add_space(4.0);
                                        for (i, tr) in w.audio_tracks.iter().enumerate() {
                                            let active = self.selected_audio_idx == i;
                                            if popover_option_row(ui, &tr.label, active) {
                                                picked_audio = Some(i);
                                            }
                                        }
                                    }
                                }
                                if picked_auto {
                                    self.switch_quality_auto();
                                    self.player_menu = PlayerMenu::None;
                                    interacted = true;
                                }
                                if let Some(idx) = picked_qual {
                                    self.switch_quality(idx);
                                    self.player_menu = PlayerMenu::None;
                                    interacted = true;
                                }
                                if let Some(idx) = picked_audio {
                                    self.switch_audio_track(idx);
                                    self.player_menu = PlayerMenu::None;
                                    interacted = true;
                                }

                                ui.add_space(6.0);
                                ui.separator();
                                ui.add_space(5.0);
                                ui.label(
                                    RichText::new("Audio & Equalizer")
                                        .font(theme::medium(11.5))
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add_space(4.0);
                                if popover_toggle_row(
                                    ui,
                                    "Audio-only mode (low bandwidth)",
                                    self.audio_only_mode,
                                ) {
                                    self.toggle_audio_only_mode();
                                    interacted = true;
                                }
                                if popover_toggle_row(
                                    ui,
                                    "Loudness normalization",
                                    self.loudnorm_enabled,
                                ) {
                                    self.loudnorm_enabled = !self.loudnorm_enabled;
                                    self.apply_audio_filters_all();
                                    self.toast(if self.loudnorm_enabled {
                                        "Loudness normalization: On"
                                    } else {
                                        "Loudness normalization: Off"
                                    });
                                    interacted = true;
                                }
                                ui.add_space(5.0);
                                let eq_gap = 6.0_f32;
                                let eq_w = (inner_w - eq_gap) * 0.5;
                                for row in crate::model::EqPreset::ALL.chunks(2) {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = eq_gap;
                                        for &preset in row {
                                            let active = self.eq_preset == preset;
                                            if popover_pill_button(
                                                ui,
                                                vec2(eq_w, 27.0),
                                                preset.label(),
                                                active,
                                                false,
                                            ) {
                                                self.eq_preset = preset;
                                                self.apply_audio_filters_all();
                                                self.toast(format!("Equalizer: {}", preset.label()));
                                                interacted = true;
                                            }
                                        }
                                    });
                                    ui.add_space(4.0);
                                }

                                ui.add_space(3.0);
                                ui.separator();
                                ui.add_space(5.0);
                                let ab_desc = match (self.ab_loop_a, self.ab_loop_b) {
                                    (Some(a), Some(b)) => format!(
                                        "A-B Loop ({} -> {})",
                                        format_duration(a),
                                        format_duration(b)
                                    ),
                                    (Some(a), None) => format!("A-B Loop (A: {})", format_duration(a)),
                                    _ => format!("A-B Loop & Frame Step ({})", self.shortcuts.ab_loop.display()),
                                };
                                ui.label(
                                    RichText::new(ab_desc)
                                        .font(theme::medium(11.5))
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 5.0;
                                    if popover_pill_button(ui, vec2(56.0, 26.0), "Set A", false, false)
                                    {
                                        self.set_ab_loop_a(self.playback.time_pos);
                                        interacted = true;
                                    }
                                    if popover_pill_button(ui, vec2(56.0, 26.0), "Set B", false, false)
                                    {
                                        self.set_ab_loop_b(self.playback.time_pos);
                                        interacted = true;
                                    }
                                    if (self.ab_loop_a.is_some() || self.ab_loop_b.is_some())
                                        && popover_pill_button(
                                            ui,
                                            vec2(48.0, 26.0),
                                            "Clear",
                                            false,
                                            true,
                                        )
                                    {
                                        self.clear_ab_loop();
                                        interacted = true;
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui
                                            .add_sized(
                                                vec2(38.0, 26.0),
                                                egui::Button::new(RichText::new("+1f").font(theme::medium(11.0)))
                                                    .fill(theme::CHIP)
                                                    .corner_radius(7),
                                            )
                                            .on_hover_text(format!(
                                                "Next frame ({})",
                                                self.shortcuts.frame_forward.display()
                                            ))
                                            .clicked()
                                        {
                                            if let Some(m) = &self.mpv {
                                                m.frame_step();
                                                self.trigger_hud(HudIcon::SeekForward, "+1 frame");
                                            }
                                            interacted = true;
                                        }
                                        if ui
                                            .add_sized(
                                                vec2(38.0, 26.0),
                                                egui::Button::new(RichText::new("-1f").font(theme::medium(11.0)))
                                                    .fill(theme::CHIP)
                                                    .corner_radius(7),
                                            )
                                            .on_hover_text(format!(
                                                "Previous frame ({})",
                                                self.shortcuts.frame_back.display()
                                            ))
                                            .clicked()
                                        {
                                            if let Some(m) = &self.mpv {
                                                m.frame_back_step();
                                                self.trigger_hud(HudIcon::SeekBack, "-1 frame");
                                            }
                                            interacted = true;
                                        }
                                    });
                                });

                                ui.add_space(6.0);
                                ui.separator();
                                ui.add_space(4.0);
                                if popover_toggle_row(
                                    ui,
                                    "Ambient light behind player",
                                    self.ambient_glow,
                                ) {
                                    self.ambient_glow = !self.ambient_glow;
                                    interacted = true;
                                }
                                let stats_key = self.shortcuts.stats_for_nerds.display();
                                if popover_nav_row(
                                    ui,
                                    "Stats for Nerds",
                                    &stats_key,
                                    self.show_stats_for_nerds,
                                ) {
                                    self.show_stats_for_nerds = !self.show_stats_for_nerds;
                                    self.player_menu = PlayerMenu::None;
                                    interacted = true;
                                }
                            }
                            PlayerMenu::Captions => {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Dual Subtitles / Auto-Translate")
                                            .font(theme::semibold(13.5))
                                            .color(Color32::WHITE),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if popover_close_btn(ui) {
                                            self.player_menu = PlayerMenu::None;
                                        }
                                    });
                                });
                                ui.add_space(6.0);
                                ScrollArea::vertical()
                                    .id_salt("popover_cc_scroll_outer")
                                    .max_height(max_pop_h)
                                    .show(ui, |ui| {
                                        let mut subs_changed = false;
                                        if let Some(w) = self.watch.clone() {
                                            ui.label(
                                                RichText::new("Primary subtitle (bottom)")
                                                    .font(theme::medium(11.5))
                                                    .color(theme::TEXT_DIM),
                                            );
                                            ui.add_space(3.0);
                                            if popover_option_row(
                                                ui,
                                                "Off",
                                                self.selected_caption_idx.is_none(),
                                            ) {
                                                self.selected_caption_idx = None;
                                                subs_changed = true;
                                                interacted = true;
                                            }
                                            for (i, c) in w.captions.iter().enumerate() {
                                                let suffix = if c.is_auto { " (auto)" } else { "" };
                                                let lang = if c.language_code.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!(" [{}]", c.language_code.to_uppercase())
                                                };
                                                if popover_option_row(
                                                    ui,
                                                    &format!("{}{lang}{suffix}", c.name),
                                                    self.selected_caption_idx == Some(i),
                                                ) {
                                                    self.selected_caption_idx = Some(i);
                                                    subs_changed = true;
                                                    interacted = true;
                                                }
                                            }

                                            if self.selected_caption_idx.is_some() {
                                                ui.add_space(4.0);
                                                ui.label(
                                                    RichText::new("Primary auto-translate (tlang=)")
                                                        .font(theme::medium(11.0))
                                                        .color(theme::TEXT_DIM),
                                                );
                                                ui.add_space(3.0);
                                                let langs = build_tlang_options(&w.translation_languages);
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                                    let orig_active = self.selected_caption_tlang.is_none();
                                                    if tlang_chip(ui, "Original", orig_active) {
                                                        self.selected_caption_tlang = None;
                                                        subs_changed = true;
                                                        interacted = true;
                                                    }
                                                    for (code, label) in &langs {
                                                        let act = self.selected_caption_tlang.as_deref()
                                                            == Some(code.as_str());
                                                        if tlang_chip(ui, label, act) {
                                                            self.selected_caption_tlang = Some(code.clone());
                                                            subs_changed = true;
                                                            interacted = true;
                                                        }
                                                    }
                                                });
                                            }

                                            ui.add_space(8.0);
                                            ui.separator();
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new("Secondary subtitle (top / dual)")
                                                    .font(theme::medium(11.5))
                                                    .color(theme::TEXT_DIM),
                                            );
                                            ui.add_space(3.0);
                                            if popover_option_row(
                                                ui,
                                                "Off",
                                                self.secondary_caption_idx.is_none(),
                                            ) {
                                                self.secondary_caption_idx = None;
                                                subs_changed = true;
                                                interacted = true;
                                            }
                                            for (i, c) in w.captions.iter().enumerate() {
                                                let suffix = if c.is_auto { " (auto)" } else { "" };
                                                let lang = if c.language_code.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!(" [{}]", c.language_code.to_uppercase())
                                                };
                                                if popover_option_row(
                                                    ui,
                                                    &format!("{}{lang}{suffix}", c.name),
                                                    self.secondary_caption_idx == Some(i),
                                                ) {
                                                    self.secondary_caption_idx = Some(i);
                                                    subs_changed = true;
                                                    interacted = true;
                                                }
                                            }
                                            if self.secondary_caption_idx.is_some() {
                                                ui.add_space(4.0);
                                                ui.label(
                                                    RichText::new("Secondary auto-translate (tlang=)")
                                                        .font(theme::medium(11.0))
                                                        .color(theme::TEXT_DIM),
                                                );
                                                ui.add_space(3.0);
                                                let langs = build_tlang_options(&w.translation_languages);
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                                    let orig_active = self.secondary_caption_tlang.is_none();
                                                    if tlang_chip(ui, "Original", orig_active) {
                                                        self.secondary_caption_tlang = None;
                                                        subs_changed = true;
                                                        interacted = true;
                                                    }
                                                    for (code, label) in &langs {
                                                        let act = self.secondary_caption_tlang.as_deref()
                                                            == Some(code.as_str());
                                                        if tlang_chip(ui, label, act) {
                                                            self.secondary_caption_tlang = Some(code.clone());
                                                            subs_changed = true;
                                                            interacted = true;
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                        if subs_changed {
                                            self.apply_active_subtitles();
                                        }

                                        ui.add_space(8.0);
                                        ui.separator();
                                        ui.add_space(4.0);
                                        let mut style_changed = false;
                                        ui.label(
                                            RichText::new(format!("Primary size ({}px)", self.sub_font_size))
                                                .font(theme::medium(11.5))
                                                .color(theme::TEXT_DIM),
                                        );
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut self.sub_font_size, 24..=80)
                                                    .show_value(false),
                                            )
                                            .changed()
                                        {
                                            style_changed = true;
                                            interacted = true;
                                        }
                                        let mut pct = (self.sub_bg_opacity * 100.0).round() as u32;
                                        ui.label(
                                            RichText::new(format!("Primary backdrop ({pct}%)"))
                                                .font(theme::medium(11.5))
                                                .color(theme::TEXT_DIM),
                                        );
                                        if ui
                                            .add(egui::Slider::new(&mut pct, 0..=100).show_value(false))
                                            .changed()
                                        {
                                            self.sub_bg_opacity = pct as f32 / 100.0;
                                            style_changed = true;
                                            interacted = true;
                                        }
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                            for col in crate::model::SubtitleColorPreset::ALL {
                                                if tlang_chip(ui, col.label(), self.sub_color == col) {
                                                    self.sub_color = col;
                                                    style_changed = true;
                                                    interacted = true;
                                                }
                                            }
                                        });

                                        ui.add_space(6.0);
                                        ui.label(
                                            RichText::new(format!("Secondary size ({}px)", self.sec_sub_font_size))
                                                .font(theme::medium(11.5))
                                                .color(theme::TEXT_DIM),
                                        );
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut self.sec_sub_font_size, 20..=72)
                                                    .show_value(false),
                                            )
                                            .changed()
                                        {
                                            style_changed = true;
                                            interacted = true;
                                        }
                                        let mut sec_pct = (self.sec_sub_bg_opacity * 100.0).round() as u32;
                                        ui.label(
                                            RichText::new(format!("Secondary backdrop ({sec_pct}%)"))
                                                .font(theme::medium(11.5))
                                                .color(theme::TEXT_DIM),
                                        );
                                        if ui
                                            .add(egui::Slider::new(&mut sec_pct, 0..=100).show_value(false))
                                            .changed()
                                        {
                                            self.sec_sub_bg_opacity = sec_pct as f32 / 100.0;
                                            style_changed = true;
                                            interacted = true;
                                        }
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                            for col in crate::model::SubtitleColorPreset::ALL {
                                                if tlang_chip(ui, col.label(), self.sec_sub_color == col) {
                                                    self.sec_sub_color = col;
                                                    style_changed = true;
                                                    interacted = true;
                                                }
                                            }
                                        });
                                        if style_changed {
                                            self.apply_subtitle_style_all();
                                        }
                                    });
                            }
                            PlayerMenu::SleepTimer => {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Sleep timer")
                                            .font(theme::semibold(13.5))
                                            .color(Color32::WHITE),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if popover_close_btn(ui) {
                                            self.sleep_editing_seg = None;
                                            self.player_menu = PlayerMenu::None;
                                        }
                                    });
                                });
                                if let Some(rem) = self.sleep_timer_remaining_label() {
                                    ui.add_space(6.0);
                                    Frame::new()
                                        .fill(Color32::from_rgba_unmultiplied(62, 166, 255, 24))
                                        .stroke(Stroke::new(1.0, theme::YT_BLUE.gamma_multiply(0.45)))
                                        .corner_radius(8)
                                        .inner_margin(Margin::symmetric(10, 6))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let fade_tag = if self.sleep_fade_active {
                                                    " (fading)"
                                                } else {
                                                    ""
                                                };
                                                ui.label(
                                                    RichText::new(format!("Active · {rem}{fade_tag}"))
                                                        .font(theme::medium(12.0))
                                                        .color(theme::YT_BLUE),
                                                );
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        if ui
                                                            .add(
                                                                egui::Button::new(
                                                                    RichText::new("Turn off")
                                                                        .font(theme::medium(11.0))
                                                                        .color(Color32::WHITE),
                                                                )
                                                                .fill(Color32::from_white_alpha(26))
                                                                .corner_radius(6),
                                                            )
                                                            .clicked()
                                                        {
                                                            self.sleep_timer = SleepTimer::Off;
                                                            self.restore_sleep_volume();
                                                            self.toast("Sleep timer turned off");
                                                            interacted = true;
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                }

                                ui.add_space(8.0);
                                let preset_gap = 6.0_f32;
                                let preset_w = (inner_w - preset_gap * 3.0) / 4.0;
                                // 0 = Off, >0 = minutes, -1 = End of video
                                let rows: [[(&str, i32); 4]; 2] = [
                                    [("Off", 0), ("10 min", 10), ("15 min", 15), ("20 min", 20)],
                                    [("30 min", 30), ("45 min", 45), ("60 min", 60), ("End of video", -1)],
                                ];
                                for row in rows {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = preset_gap;
                                        for (lbl, mins) in row {
                                            let active = match mins {
                                                0 => self.sleep_timer == SleepTimer::Off,
                                                -1 => self.sleep_timer == SleepTimer::EndOfVideo,
                                                _ => false,
                                            };
                                            if popover_pill_button(
                                                ui,
                                                vec2(preset_w, 27.0),
                                                lbl,
                                                active,
                                                false,
                                            ) {
                                                self.sleep_editing_seg = None;
                                                self.restore_sleep_volume();
                                                match mins {
                                                    0 => {
                                                        self.sleep_timer = SleepTimer::Off;
                                                        self.toast("Sleep timer turned off");
                                                    }
                                                    -1 => {
                                                        self.sleep_timer = SleepTimer::EndOfVideo;
                                                        self.toast("Sleep timer: End of video");
                                                    }
                                                    m => {
                                                        let m_u = m as u32;
                                                        self.sleep_pick_h = m_u / 60;
                                                        self.sleep_pick_m = m_u % 60;
                                                        self.sleep_pick_s = 0;
                                                        self.sleep_timer = SleepTimer::Until(
                                                            Instant::now()
                                                                + Duration::from_secs(m as u64 * 60),
                                                        );
                                                        self.toast(format!("Sleep timer: {m} min"));
                                                    }
                                                }
                                                self.player_menu = PlayerMenu::None;
                                                interacted = true;
                                            }
                                        }
                                    });
                                    ui.add_space(5.0);
                                }

                                ui.add_space(2.0);
                                ui.separator();
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Custom timer")
                                            .font(theme::medium(11.5))
                                            .color(theme::TEXT_DIM),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new("Scroll, drag, or click to type")
                                                .font(theme::regular(10.5))
                                                .color(theme::TEXT_DIM.gamma_multiply(0.8)),
                                        );
                                    });
                                });
                                ui.add_space(6.0);

                                let seg_specs: [(&str, u32); 3] = [("hr", 23), ("min", 59), ("sec", 59)];
                                let mut seg_vals =
                                    [self.sleep_pick_h, self.sleep_pick_m, self.sleep_pick_s];
                                let mut nav_move: i32 = 0;
                                let mut confirm_enter = false;
                                if let Some(active_seg) = self.sleep_editing_seg {
                                    ctx.input(|i| {
                                        for ev in &i.events {
                                            if let egui::Event::Text(txt) = ev {
                                                for ch in txt.chars() {
                                                    if ch.is_ascii_digit() {
                                                        self.sleep_seg_buf.push(ch);
                                                        if self.sleep_seg_buf.len() > 2 {
                                                            let start = self.sleep_seg_buf.len() - 2;
                                                            self.sleep_seg_buf =
                                                                self.sleep_seg_buf[start..].to_owned();
                                                        }
                                                        if let Ok(v) = self.sleep_seg_buf.parse::<u32>() {
                                                            seg_vals[active_seg as usize] =
                                                                v.min(seg_specs[active_seg as usize].1);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if i.key_pressed(egui::Key::Backspace) {
                                            self.sleep_seg_buf.pop();
                                            seg_vals[active_seg as usize] =
                                                self.sleep_seg_buf.parse::<u32>().unwrap_or(0);
                                        }
                                        if i.key_pressed(egui::Key::ArrowUp) {
                                            let max_v = seg_specs[active_seg as usize].1;
                                            seg_vals[active_seg as usize] =
                                                step_timer_segment(seg_vals[active_seg as usize], 1, max_v);
                                        }
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            let max_v = seg_specs[active_seg as usize].1;
                                            seg_vals[active_seg as usize] =
                                                step_timer_segment(seg_vals[active_seg as usize], -1, max_v);
                                        }
                                        if i.key_pressed(egui::Key::Tab)
                                            || i.key_pressed(egui::Key::ArrowRight)
                                        {
                                            nav_move = 1;
                                        } else if i.key_pressed(egui::Key::ArrowLeft) {
                                            nav_move = -1;
                                        }
                                        if i.key_pressed(egui::Key::Enter) {
                                            confirm_enter = true;
                                        } else if i.key_pressed(egui::Key::Escape) {
                                            self.sleep_editing_seg = None;
                                        }
                                    });
                                    if nav_move != 0 {
                                        let next = (active_seg as i32 + nav_move).rem_euclid(3) as u8;
                                        self.sleep_editing_seg = Some(next);
                                        self.sleep_seg_buf.clear();
                                    }
                                    if confirm_enter {
                                        self.sleep_editing_seg = None;
                                        self.sleep_seg_buf.clear();
                                    }
                                }

                                // Draw HH : MM : SS columns centered symmetrically across full inner_w
                                let seg_box_w = 64.0_f32;
                                let colon_w = 22.0_f32;
                                let group_w = seg_box_w * 3.0 + colon_w * 2.0;
                                let side_pad = ((inner_w - group_w) * 0.5).floor().max(0.0);

                                // Up chevron row
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.add_space(side_pad);
                                    for idx in 0..3 {
                                        if idx > 0 {
                                            ui.add_space(colon_w);
                                        }
                                        if timer_arrow_button(ui, ("sleep_up", idx), seg_box_w, true) {
                                            self.sleep_editing_seg = None;
                                            seg_vals[idx] =
                                                step_timer_segment(seg_vals[idx], 1, seg_specs[idx].1);
                                            interacted = true;
                                        }
                                    }
                                    ui.add_space(side_pad);
                                });
                                ui.add_space(4.0);

                                // Framed digit readout with pill-drag + scroll + click-to-type
                                Frame::new()
                                    .fill(Color32::from_rgb(32, 32, 37))
                                    .stroke(Stroke::new(1.0, Color32::from_white_alpha(24)))
                                    .corner_radius(10)
                                    .inner_margin(Margin::symmetric(0, 6))
                                    .show(ui, |ui| {
                                        ui.allocate_ui_with_layout(
                                            vec2(inner_w, 46.0),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                ui.spacing_mut().item_spacing.x = 0.0;
                                                ui.add_space(side_pad);
                                                for idx in 0..3 {
                                                    if idx > 0 {
                                                        let (crect, _) = ui.allocate_exact_size(
                                                            vec2(colon_w, 46.0),
                                                            Sense::hover(),
                                                        );
                                                        ui.painter().text(
                                                            crect.center() - vec2(0.0, 2.0),
                                                            Align2::CENTER_CENTER,
                                                            ":",
                                                            egui::FontId::proportional(24.0),
                                                            theme::TEXT_DIM,
                                                        );
                                                    }
                                                    let changed = draw_timer_segment_box(
                                                        ui,
                                                        ctx,
                                                        idx as u8,
                                                        seg_box_w,
                                                        &mut seg_vals[idx],
                                                        seg_specs[idx].1,
                                                        &mut self.sleep_editing_seg,
                                                        &mut self.sleep_seg_buf,
                                                    );
                                                    if changed {
                                                        interacted = true;
                                                    }
                                                }
                                                ui.add_space(side_pad);
                                            },
                                        );
                                    });

                                ui.add_space(4.0);
                                // Down chevron row
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.add_space(side_pad);
                                    for idx in 0..3 {
                                        if idx > 0 {
                                            ui.add_space(colon_w);
                                        }
                                        if timer_arrow_button(ui, ("sleep_dn", idx), seg_box_w, false) {
                                            self.sleep_editing_seg = None;
                                            seg_vals[idx] =
                                                step_timer_segment(seg_vals[idx], -1, seg_specs[idx].1);
                                            interacted = true;
                                        }
                                    }
                                    ui.add_space(side_pad);
                                });
                                // Subtle unit labels (hr / min / sec)
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.add_space(side_pad);
                                    for (idx, (unit, _)) in seg_specs.iter().enumerate() {
                                        if idx > 0 {
                                            ui.add_space(colon_w);
                                        }
                                        let (lrect, _) =
                                            ui.allocate_exact_size(vec2(seg_box_w, 14.0), Sense::hover());
                                        ui.painter().text(
                                            lrect.center(),
                                            Align2::CENTER_CENTER,
                                            *unit,
                                            theme::regular(10.5),
                                            theme::TEXT_DIM.gamma_multiply(0.75),
                                        );
                                    }
                                    ui.add_space(side_pad);
                                });

                                self.sleep_pick_h = seg_vals[0];
                                self.sleep_pick_m = seg_vals[1];
                                self.sleep_pick_s = seg_vals[2];

                                ui.add_space(8.0);
                                let total_secs = segments_to_secs(
                                    self.sleep_pick_h,
                                    self.sleep_pick_m,
                                    self.sleep_pick_s,
                                );
                                let start_label = if total_secs == 0 {
                                    "Set a duration above".to_owned()
                                } else {
                                    format!("Start timer · {}", format_duration(total_secs as f64))
                                };
                                let (btn_rect, btn_resp) = ui.allocate_exact_size(
                                    vec2(inner_w, 34.0),
                                    if total_secs > 0 {
                                        Sense::click()
                                    } else {
                                        Sense::hover()
                                    },
                                );
                                if total_secs > 0 && btn_resp.hovered() {
                                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                }
                                let btn_fill = if total_secs == 0 {
                                    theme::CHIP
                                } else if btn_resp.hovered() {
                                    Color32::from_rgb(92, 182, 255)
                                } else {
                                    theme::YT_BLUE
                                };
                                ui.painter().rect_filled(btn_rect, 8.0, btn_fill);
                                ui.painter().text(
                                    btn_rect.center(),
                                    Align2::CENTER_CENTER,
                                    &start_label,
                                    theme::semibold(12.5),
                                    if total_secs > 0 {
                                        Color32::from_rgb(15, 15, 15)
                                    } else {
                                        theme::TEXT_DIM
                                    },
                                );
                                if btn_resp.clicked() && total_secs > 0 {
                                    self.sleep_editing_seg = None;
                                    self.restore_sleep_volume();
                                    self.sleep_timer = SleepTimer::Until(
                                        Instant::now() + Duration::from_secs(total_secs),
                                    );
                                    self.player_menu = PlayerMenu::None;
                                    self.toast(format!(
                                        "Sleep timer set for {}",
                                        format_duration(total_secs as f64)
                                    ));
                                    interacted = true;
                                }

                                ui.add_space(6.0);
                                ui.separator();
                                ui.add_space(4.0);
                                if popover_toggle_row(
                                    ui,
                                    "Fade audio gently (last 10s)",
                                    self.sleep_fade_enabled,
                                ) {
                                    self.sleep_fade_enabled = !self.sleep_fade_enabled;
                                    if !self.sleep_fade_enabled {
                                        self.restore_sleep_volume();
                                    }
                                    interacted = true;
                                }
                            }
                            PlayerMenu::None => {}
                        }
                    });
            });

        // Close popover cleanly when clicking outside (ignore drag releases whose press began inside)
        let started_inside = ctx
            .input(|i| i.pointer.press_origin())
            .is_some_and(|o| area_resp.response.rect.contains(o));
        let clicked_outside = !just_toggled
            && !started_inside
            && !egui::Popup::is_any_open(ctx)
            && ctx.input(|i| i.pointer.any_released())
            && ctx
                .input(|i| i.pointer.interact_pos())
                .is_none_or(|p| !area_resp.response.rect.contains(p));
        if clicked_outside {
            self.sleep_editing_seg = None;
            self.player_menu = PlayerMenu::None;
            interacted = true;
        }

        interacted || area_resp.response.hovered() || area_resp.response.clicked()
    }

    fn draw_stats_for_nerds(&mut self, ui: &mut egui::Ui, video_rect: Rect) {
        let box_rect = Rect::from_min_size(
            video_rect.min + vec2(14.0, 14.0),
            vec2(315.0_f32.min(video_rect.width() - 28.0), 178.0),
        );
        ui.painter().rect_filled(
            box_rect,
            10.0,
            Color32::from_rgba_unmultiplied(16, 16, 20, 218),
        );
        ui.painter().rect_stroke(
            box_rect,
            10.0,
            Stroke::new(1.0, Color32::from_white_alpha(32)),
            egui::StrokeKind::Inside,
        );
        let mut sui = ui.new_child(egui::UiBuilder::new().max_rect(box_rect.shrink(10.0)));
        sui.horizontal(|ui| {
            ui.label(
                RichText::new("Stats for Nerds")
                    .font(theme::semibold(12.5))
                    .color(Color32::WHITE),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button(icon::X.to_string()).clicked() {
                    self.show_stats_for_nerds = false;
                }
            });
        });
        sui.add_space(4.0);
        let vid_id = self
            .watch
            .as_ref()
            .map_or("-", |w| w.id.as_str());
        let fmt_lbl = self
            .selected_format_idx
            .and_then(|i| self.watch.as_ref()?.video_formats.get(i))
            .map(|f| f.label())
            .unwrap_or_else(|| "Auto".to_owned());
        let res_str = if self.playback.video_w > 0 {
            format!(
                "{}x{} @ {:.1} fps ({fmt_lbl})",
                self.playback.video_w, self.playback.video_h, self.playback.fps
            )
        } else {
            fmt_lbl
        };
        let codecs_str = match (
            self.playback.video_codec.is_empty(),
            self.playback.audio_codec.is_empty(),
        ) {
            (false, false) => format!(
                "{} / {}",
                self.playback.video_codec, self.playback.audio_codec
            ),
            (false, true) => self.playback.video_codec.clone(),
            _ => "resolving...".to_owned(),
        };
        let hwdec_str = if self.playback.hwdec.is_empty() {
            format!("{} (inactive)", self.hwdec_mode.mpv_value())
        } else {
            self.playback.hwdec.clone()
        };
        let rows = [
            ("Video ID", vid_id.to_owned()),
            ("Viewport", format!("{}x{}", video_rect.width().round() as i32, video_rect.height().round() as i32)),
            ("Stream", res_str),
            ("Codecs", codecs_str),
            ("Hardware Dec", hwdec_str),
            ("Buffer", format!("{:.1}s readahead", self.playback.cache_duration)),
            ("Dropped", format!("{} frames", self.playback.dropped_frames)),
            (
                "Source",
                if self.used_ytdlp {
                    "yt-dlp fallback".to_owned()
                } else {
                    "InnerTube + 4x Range Proxy".to_owned()
                },
            ),
        ];
        for (k, v) in rows {
            sui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{k}:"))
                        .font(theme::semibold(11.0))
                        .color(theme::TEXT_DIM),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(v)
                            .font(theme::regular(11.0))
                            .color(Color32::WHITE),
                    )
                    .truncate(),
                );
            });
        }
    }

    fn draw_hud_feedback(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rect: Rect) {
        let Some((hud_ico, msg, at)) = &self.hud_feedback else {
            return;
        };
        let elapsed = at.elapsed().as_secs_f32();
        if elapsed > 0.65 {
            self.hud_feedback = None;
            return;
        }
        ctx.request_repaint();
        let alpha = (1.0 - (elapsed / 0.65)).clamp(0.0, 1.0);
        let pill = Rect::from_center_size(rect.center(), vec2(128.0, 44.0));
        let fg = Color32::from_white_alpha((250.0 * alpha).round() as u8);
        ui.painter().rect_filled(
            pill,
            22.0,
            Color32::from_black_alpha((180.0 * alpha).round() as u8),
        );
        let ico_center = pos2(pill.left() + 26.0, pill.center().y);
        match hud_ico {
            HudIcon::Play => {
                let tri = vec![
                    pos2(ico_center.x - 4.0, ico_center.y - 6.0),
                    pos2(ico_center.x + 6.0, ico_center.y),
                    pos2(ico_center.x - 4.0, ico_center.y + 6.0),
                ];
                ui.painter()
                    .add(egui::Shape::convex_polygon(tri, fg, Stroke::NONE));
            }
            HudIcon::Pause => {
                for dx in [-4.5f32, 1.5] {
                    ui.painter().rect_filled(
                        Rect::from_min_size(pos2(ico_center.x + dx, ico_center.y - 6.0), vec2(3.0, 12.0)),
                        1.0,
                        fg,
                    );
                }
            }
            HudIcon::SeekBack => {
                for ox in [-5.0f32, 2.0] {
                    let tri = vec![
                        pos2(ico_center.x + ox + 4.0, ico_center.y - 5.0),
                        pos2(ico_center.x + ox - 2.0, ico_center.y),
                        pos2(ico_center.x + ox + 4.0, ico_center.y + 5.0),
                    ];
                    ui.painter()
                        .add(egui::Shape::convex_polygon(tri, fg, Stroke::NONE));
                }
            }
            HudIcon::SeekForward => {
                for ox in [-3.0f32, 4.0] {
                    let tri = vec![
                        pos2(ico_center.x + ox - 4.0, ico_center.y - 5.0),
                        pos2(ico_center.x + ox + 2.0, ico_center.y),
                        pos2(ico_center.x + ox - 4.0, ico_center.y + 5.0),
                    ];
                    ui.painter()
                        .add(egui::Shape::convex_polygon(tri, fg, Stroke::NONE));
                }
            }
            HudIcon::VolumeOn => {
                theme::draw_icon_centered(ui.painter(), ico_center, icon::VOLUME_2, 16.0, fg);
            }
            HudIcon::VolumeMuted => {
                theme::draw_icon_centered(ui.painter(), ico_center, icon::VOLUME_X, 16.0, fg);
            }
        }
        ui.painter().text(
            pos2(pill.left() + 44.0, pill.center().y),
            Align2::LEFT_CENTER,
            msg,
            theme::semibold(14.0),
            fg,
        );
    }

    pub(super) fn draw_watch_metadata(&mut self, ui: &mut egui::Ui) {
        let Some(w) = self.watch.clone() else {
            if self.watch_loading {
                ui.spinner();
            } else if let Some(bg) = &self.bg_player_tab {
                if let Some(bg_w) = &bg.watch {
                    Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .corner_radius(12)
                        .inner_margin(Margin::same(16))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Tab ready — pick a video to play here")
                                    .font(theme::semibold(16.0))
                                    .color(theme::TEXT),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!(
                                    "\"{}\"{}. Pick any video from Home, Search, or middle-click a card to play alongside it.",
                                    bg_w.title,
                                    if bg.playback.paused {
                                        " (paused in other tab)"
                                    } else {
                                        " is playing in the other tab"
                                    }
                                ))
                                .font(theme::regular(13.0))
                                .color(theme::TEXT_DIM),
                            );
                            ui.add_space(10.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Browse Home (keep this tab active)")
                                            .font(theme::semibold(12.5))
                                            .color(Color32::from_rgb(15, 15, 15)),
                                    )
                                    .fill(theme::YT_BLUE)
                                    .corner_radius(16),
                                )
                                .clicked()
                            {
                                self.page = Page::Home;
                            }
                        });
                }
            }
            return;
        };

        if w.is_members_only {
            draw_members_badge(ui);
            ui.add_space(6.0);
        }
        ui.label(
            RichText::new(&w.title)
                .font(theme::bold(20.0))
                .color(Color32::WHITE),
        );
        ui.add_space(10.0);

        let meta_w = ui.available_width();
        if meta_w >= 740.0 {
            ui.allocate_ui_with_layout(
                Vec2::new(meta_w, 42.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new((meta_w * 0.43).clamp(230.0, 380.0), 42.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            self.draw_channel_row(ui, &w);
                        },
                    );
                    self.draw_actions_row(ui, &w, true);
                },
            );
        } else {
            ui.allocate_ui_with_layout(
                Vec2::new(meta_w, 42.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    self.draw_channel_row(ui, &w);
                },
            );
            ui.add_space(6.0);
            self.draw_actions_row(ui, &w, false);
        }

        ui.add_space(10.0);

        Frame::new()
            .fill(theme::SURFACE)
            .corner_radius(12)
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    if !w.views_text.is_empty() {
                        ui.label(RichText::new(&w.views_text).font(theme::semibold(13.0)));
                    }
                    if !w.published_text.is_empty() {
                        ui.label(RichText::new(&w.published_text).font(theme::semibold(13.0)));
                    }
                    if self.used_ytdlp {
                        ui.label(
                            RichText::new("· via yt-dlp")
                                .font(theme::regular(12.0))
                                .color(theme::YT_BLUE),
                        );
                    }
                    if !self.sponsor_segments.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "· {} SponsorBlock segments",
                                self.sponsor_segments.len()
                            ))
                            .font(theme::regular(12.0))
                            .color(Color32::from_rgb(120, 220, 140)),
                        );
                    }
                });
                if !w.description.is_empty() {
                    ui.add_space(6.0);
                    let desc = if self.show_description || w.description.len() <= 240 {
                        w.description.as_str()
                    } else {
                        let end = w
                            .description
                            .char_indices()
                            .nth(240)
                            .map_or(w.description.len(), |(i, _)| i);
                        &w.description[..end]
                    };
                    let dur_limit = if self.playback.duration > 0.0 {
                        self.playback.duration
                    } else {
                        w.duration_secs
                    };
                    let max_seek = (!w.is_live && dur_limit > 0.0).then_some(dur_limit);
                    if let Some(seek_secs) = draw_text_with_timestamps(
                        ui,
                        desc,
                        13.0,
                        Color32::from_rgb(215, 215, 215),
                        max_seek,
                    ) {
                        if let Some(mpv) = &self.mpv {
                            mpv.seek_abs(seek_secs);
                            if self.playback.eof {
                                mpv.set_paused(false);
                            }
                            self.trigger_hud(HudIcon::SeekForward, format_duration(seek_secs));
                        }
                    }
                    if w.description.len() > 240 {
                        let btn = if self.show_description {
                            "Show less"
                        } else {
                            "...more"
                        };
                        if ui.link(RichText::new(btn).font(theme::semibold(12.5))).clicked() {
                            self.show_description = !self.show_description;
                        }
                    }
                }
            });

        if !w.chapters.is_empty() {
            ui.add_space(10.0);
            Frame::new()
                .fill(theme::SURFACE)
                .corner_radius(12)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Chapters ({})", w.chapters.len()))
                                .font(theme::semibold(14.0)),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let label = if self.show_chapters {
                                "Hide"
                            } else {
                                "Show all"
                            };
                            if ui.small_button(label).clicked() {
                                self.show_chapters = !self.show_chapters;
                            }
                        });
                    });
                    if self.show_chapters {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            for ch in &w.chapters {
                                let active = self.playback.time_pos >= ch.start_secs
                                    && w.chapters
                                        .iter()
                                        .find(|n| n.start_secs > ch.start_secs)
                                        .is_none_or(|n| self.playback.time_pos < n.start_secs);
                                let fill = if active {
                                    theme::YT_BLUE.gamma_multiply(0.25)
                                } else {
                                    theme::CHIP
                                };
                                let btn = egui::Button::new(
                                    RichText::new(format!(
                                        "{}  {}",
                                        format_duration(ch.start_secs),
                                        ch.title
                                    ))
                                    .font(theme::medium(12.0)),
                                )
                                .fill(fill)
                                .corner_radius(14);
                                if ui.add(btn).clicked() {
                                    if let Some(mpv) = &self.mpv {
                                        mpv.seek_abs(ch.start_secs);
                                    }
                                }
                            }
                        });
                    }
                });
        }

        // Comments section
        ui.add_space(14.0);
        self.draw_comments(ui, &w);
    }

    fn draw_comments(&mut self, ui: &mut egui::Ui, w: &crate::model::WatchData) {
        self.draw_comments_header(ui, w, false);
        ui.add_space(8.0);
        self.draw_comment_composer(ui, w);
        self.draw_comments_list(ui, w);
    }

    fn draw_comments_header(
        &mut self,
        ui: &mut egui::Ui,
        w: &crate::model::WatchData,
        show_close_shorts: bool,
    ) {
        let mut load_sort: Option<(usize, String)> = None;
        let compact = show_close_shorts;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.comments.is_empty() {
                    "Comments".to_owned()
                } else {
                    format!("Comments ({})", self.comments.len())
                })
                .font(theme::semibold(if compact { 15.0 } else { 16.0 })),
            );
            if !self.comment_sort_options.is_empty() {
                ui.add_space(8.0);
                for (idx, (label, tok)) in self.comment_sort_options.iter().enumerate() {
                    let active = idx == self.selected_comment_sort;
                    let (fill, col) = if active {
                        (Color32::from_rgb(241, 241, 241), Color32::from_rgb(15, 15, 15))
                    } else {
                        (theme::CHIP, theme::TEXT)
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(label)
                                    .font(theme::medium(if compact { 11.5 } else { 12.0 }))
                                    .color(col),
                            )
                            .fill(fill)
                            .corner_radius(8),
                        )
                        .clicked()
                        && !active
                    {
                        load_sort = Some((idx, tok.clone()));
                    }
                }
            }
            if self.comments_loading {
                ui.add_space(6.0);
                ui.spinner();
            }
            if show_close_shorts {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_circle_button(ui, icon::X, 15.0, "Close comments") {
                        self.shorts_comments_open = false;
                    }
                });
            }
        });
        if let Some((idx, tok)) = load_sort {
            self.selected_comment_sort = idx;
            self.comments.clear();
            self.comments_next_token = None;
            self.comments_loading = true;
            let _ = self.backend.tx.send(Request::LoadComments {
                video_id: w.id.clone(),
                continuation: tok,
                append: false,
            });
        }
    }

    fn draw_comment_composer(&mut self, ui: &mut egui::Ui, w: &crate::model::WatchData) {
        if !self.signed_in {
            return;
        }
        let Some(params) = self.comment_create_params.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            let input_w = (ui.available_width() - 92.0).max(120.0);
            let te = ui.add_sized(
                Vec2::new(input_w, 32.0),
                egui::TextEdit::singleline(&mut self.comment_draft).hint_text("Add a comment..."),
            );
            let submit = ui
                .add_sized(
                    Vec2::new(82.0, 32.0),
                    egui::Button::new(
                        RichText::new("Comment")
                            .font(theme::semibold(12.5))
                            .color(Color32::from_rgb(15, 15, 15)),
                    )
                    .fill(theme::YT_BLUE)
                    .corner_radius(16),
                )
                .clicked()
                || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if submit {
                let body = self.comment_draft.trim().to_owned();
                if !body.is_empty() {
                    let reload_tok = self
                        .comment_sort_options
                        .get(self.selected_comment_sort)
                        .map(|(_, t)| t.clone())
                        .or_else(|| w.comments_token.clone());
                    let _ = self.backend.tx.send(Request::PostComment {
                        video_id: w.id.clone(),
                        text: body,
                        params,
                        reload_continuation: reload_tok,
                    });
                    self.comment_draft.clear();
                }
            }
        });
        ui.add_space(6.0);
    }

    fn draw_comments_list(&mut self, ui: &mut egui::Ui, w: &crate::model::WatchData) {
        let dur_limit = if self.playback.duration > 0.0 {
            self.playback.duration
        } else {
            w.duration_secs
        };
        let max_seek = (!w.is_live && dur_limit > 0.0).then_some(dur_limit);
        let backend_tx = self.backend.tx.clone();
        let seek_req = draw_comments_thread_ui(
            ui,
            &w.id,
            self.signed_in,
            &mut self.comments,
            &mut self.comments_loading,
            &mut self.comments_next_token,
            None,
            &mut String::new(),
            None,
            &mut self.comment_reply_target_id,
            &mut self.comment_reply_target_author,
            &mut self.comment_reply_draft,
            &backend_tx,
            max_seek,
            "Comments are unavailable for this video.",
            w.comments_token.clone(),
            false,
        );
        if let Some(secs) = seek_req {
            if let Some(mpv) = &self.mpv {
                mpv.seek_abs(secs);
                if self.playback.eof {
                    mpv.set_paused(false);
                }
                self.trigger_hud(HudIcon::SeekForward, format_duration(secs));
            }
        }
    }

    pub(super) fn draw_post_comments_section(
        &mut self,
        ui: &mut egui::Ui,
        post_id: &str,
    ) {
        let backend_tx = self.backend.tx.clone();
        let create_params = self.post_comment_create_params.clone();
        let _ = draw_comments_thread_ui(
            ui,
            post_id,
            self.signed_in,
            &mut self.post_comments,
            &mut self.post_comments_loading,
            &mut self.post_comments_next_token,
            create_params.as_deref(),
            &mut self.post_comment_draft,
            None,
            &mut self.post_reply_target_id,
            &mut self.post_reply_target_author,
            &mut self.post_reply_draft,
            &backend_tx,
            None,
            "No comments yet.",
            None,
            true,
        );
    }

    fn draw_channel_row(&mut self, ui: &mut egui::Ui, w: &WatchData) {
        ui.horizontal_centered(|ui| {
            draw_avatar(ui, w.channel_avatar_url.as_deref(), &w.channel, 40.0);
            ui.add_space(10.0);
            let name_max_w = (ui.available_width() - 125.0).clamp(110.0, 240.0);
            ui.vertical(|ui| {
                ui.set_max_width(name_max_w);
                ui.spacing_mut().item_spacing.y = 1.0;
                let ch_resp = ui.add(
                    egui::Label::new(
                        RichText::new(&w.channel)
                            .font(theme::semibold(15.0))
                            .color(Color32::WHITE),
                    )
                    .truncate()
                    .sense(Sense::click()),
                );
                if ch_resp.hovered() && w.channel_id.is_some() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if ch_resp.clicked() {
                    if let Some(cid) = w.channel_id.clone() {
                        self.open_channel(cid, w.channel.clone());
                    }
                }
                if !w.subscribers_text.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&w.subscribers_text)
                                .font(theme::regular(12.0))
                                .color(theme::TEXT_DIM),
                        )
                        .truncate(),
                    );
                }
            });

            if let Some(cid) = w.channel_id.clone() {
                ui.add_space(8.0);
                super::cards::draw_subscribe_bell_button(
                    ui,
                    cid,
                    w.is_subscribed,
                    &w.notifications,
                    &self.backend.tx,
                );
            }
        });
    }

    fn draw_actions_row(&mut self, ui: &mut egui::Ui, w: &WatchData, rtl: bool) {
        let layout = if rtl {
            Layout::right_to_left(Align::Center)
        } else {
            Layout::left_to_right(Align::Center)
        };
        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 42.0), layout, |ui| {
            let watch_url = format!("https://www.youtube.com/watch?v={}", w.id);
            let is_saved = self.saved.iter().any(|x| x.id == w.id);

            let draw_more_menu = |ui: &mut egui::Ui, app: &mut Self| {
                let (mrect, mresp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                let mhov = ui
                    .ctx()
                    .animate_bool_with_time(mresp.id, mresp.hovered(), 0.12);
                if mresp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let fill = theme::lerp_color(theme::CHIP, theme::CHIP_HOVER, mhov);
                ui.painter().circle_filled(mrect.center(), 17.0, fill);
                theme::draw_icon_centered(
                    ui.painter(),
                    mrect.center(),
                    icon::ELLIPSIS_VERTICAL,
                    15.0,
                    theme::TEXT,
                );
                egui::Popup::menu(&mresp).show(|ui| {
                    if ui
                        .button(format!("{}  Open in Browser", icon::LINK))
                        .clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&watch_url));
                        ui.close();
                    }
                    if ui
                        .button(format!("{}  Download Audio (M4A)", icon::HEADPHONES))
                        .clicked()
                    {
                        app.start_download(w.id.clone(), w.title.clone(), true);
                        ui.close();
                    }
                    if ui
                        .button(if app.audio_only_mode {
                            "Restore Video Mode"
                        } else {
                            "Audio-Only Mode (Low Bandwidth)"
                        })
                        .clicked()
                    {
                        app.toggle_audio_only_mode();
                        ui.close();
                    }
                    if ui.button("Show Transcript").clicked() {
                        app.right_tab = RightTab::Transcript;
                        if app.transcript.is_empty() && !app.transcript_loading {
                            app.request_transcript(None);
                        }
                        ui.close();
                    }
                    if ui.button("Save Lossless PNG Snapshot").clicked() {
                        app.save_video_snapshot_file();
                        ui.close();
                    }
                    if ui
                        .button(format!("{}  Stats for Nerds (Ctrl+I)", icon::SETTINGS))
                        .clicked()
                    {
                        app.show_stats_for_nerds = !app.show_stats_for_nerds;
                        ui.close();
                    }
                });
            };

            let draw_save_pill = |ui: &mut egui::Ui, app: &mut Self| {
                if super::cards::pill_button(
                    ui,
                    if is_saved { icon::CHECK } else { icon::LIST_PLUS },
                    if is_saved { "Saved" } else { "Save" },
                )
                .clicked()
                {
                    app.open_playlist_picker(VideoItem {
                        id: w.id.clone(),
                        title: w.title.clone(),
                        channel: w.channel.clone(),
                        channel_id: w.channel_id.clone(),
                        channel_avatar_url: w.channel_avatar_url.clone(),
                        views_text: w.views_text.clone(),
                        published_text: w.published_text.clone(),
                        duration_text: format_duration(w.duration_secs),
                        thumbnail_url: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", w.id),
                        is_live: w.is_live,
                        is_new: false,
                        is_members_only: w.is_members_only,
                        watched_percent: None,
                        resume_secs: None,
                        playlist_browse_id: None,
                        mix_playlist_id: None,
                        playlist_set_video_id: None,
                    });
                }
            };

            let draw_download_pill = |ui: &mut egui::Ui, app: &mut Self| {
                if super::cards::pill_button(ui, icon::DOWNLOAD, "Download")
                    .on_hover_text(format!("Download MP4 ({})", app.preferred_quality.label()))
                    .clicked()
                {
                    app.start_download(w.id.clone(), w.title.clone(), false);
                }
            };

            let draw_share_pill = |ui: &mut egui::Ui, app: &mut Self| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(82.0, 34.0), Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                ui.painter().rect_filled(
                    rect,
                    17.0,
                    if resp.hovered() {
                        theme::CHIP_HOVER
                    } else {
                        theme::CHIP
                    },
                );
                theme::draw_share_icon(
                    ui.painter(),
                    pos2(rect.left() + 20.0, rect.center().y),
                    15.0,
                    theme::TEXT,
                );
                ui.painter().text(
                    pos2(rect.left() + 34.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    "Share",
                    theme::medium(12.5),
                    theme::TEXT,
                );
                if resp.on_hover_text("Copy video URL").clicked() {
                    ui.ctx().copy_text(watch_url.clone());
                    app.toast("Link copied to clipboard");
                }
            };

            let draw_like_dislike_pill = |ui: &mut egui::Ui, app: &mut Self| {
                let like_text = if w.likes_text.is_empty() {
                    if w.is_liked {
                        "Liked".to_owned()
                    } else {
                        "Like".to_owned()
                    }
                } else {
                    w.likes_text.clone()
                };
                let text_galley = ui.painter().layout_no_wrap(
                    like_text.clone(),
                    theme::medium(12.5),
                    theme::TEXT,
                );
                let like_w = (36.0 + text_galley.size().x + 12.0).max(74.0);
                let dislike_label = w.dislikes_text.as_deref().filter(|s| !s.is_empty());
                let dislike_galley = dislike_label.map(|t| {
                    ui.painter()
                        .layout_no_wrap(t.to_owned(), theme::medium(12.5), theme::TEXT)
                });
                let dislike_w = match &dislike_galley {
                    Some(g) => (34.0 + g.size().x + 12.0).max(58.0),
                    None => 42.0,
                };
                let total_w = like_w + dislike_w;

                let (pill_rect, _) =
                    ui.allocate_exact_size(Vec2::new(total_w, 34.0), Sense::hover());
                let like_rect = Rect::from_min_max(
                    pill_rect.min,
                    pos2(pill_rect.left() + like_w, pill_rect.bottom()),
                );
                let dislike_rect = Rect::from_min_max(
                    pos2(pill_rect.left() + like_w, pill_rect.top()),
                    pill_rect.max,
                );

                let like_resp =
                    ui.interact(like_rect, ui.scope_id().with("yt_like_seg"), Sense::click());
                let dislike_resp = ui.interact(
                    dislike_rect,
                    ui.scope_id().with("yt_dislike_seg"),
                    Sense::click(),
                );
                if like_resp.hovered() || dislike_resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }

                // Base pill background + individual hover segment highlights
                ui.painter().rect_filled(pill_rect, 17.0, theme::CHIP);
                if like_resp.hovered() || w.is_liked {
                    let col = if w.is_liked {
                        theme::YT_BLUE.gamma_multiply(0.22)
                    } else {
                        theme::CHIP_HOVER
                    };
                    ui.painter().rect_filled(
                        like_rect,
                        CornerRadius {
                            nw: 17,
                            sw: 17,
                            ne: 0,
                            se: 0,
                        },
                        col,
                    );
                }
                if dislike_resp.hovered() || w.is_disliked {
                    let col = if w.is_disliked {
                        theme::YT_RED.gamma_multiply(0.22)
                    } else {
                        theme::CHIP_HOVER
                    };
                    ui.painter().rect_filled(
                        dislike_rect,
                        CornerRadius {
                            nw: 0,
                            sw: 0,
                            ne: 17,
                            se: 17,
                        },
                        col,
                    );
                }

                // Vertical divider between Like and Dislike (matches YouTube!)
                ui.painter().line_segment(
                    [
                        pos2(like_rect.right(), pill_rect.top() + 7.0),
                        pos2(like_rect.right(), pill_rect.bottom() - 7.0),
                    ],
                    Stroke::new(1.0, Color32::from_white_alpha(36)),
                );

                let like_col = if w.is_liked { theme::YT_BLUE } else { theme::TEXT };
                theme::draw_thumbs_up(
                    ui.painter(),
                    pos2(like_rect.left() + 22.0, like_rect.center().y),
                    16.0,
                    like_col,
                    w.is_liked,
                );
                ui.painter().text(
                    pos2(like_rect.left() + 36.0, like_rect.center().y),
                    Align2::LEFT_CENTER,
                    like_text,
                    theme::medium(12.5),
                    like_col,
                );

                let dislike_col = if w.is_disliked {
                    theme::YT_RED
                } else {
                    theme::TEXT
                };
                if let Some(d_txt) = dislike_label {
                    theme::draw_icon_centered(
                        ui.painter(),
                        pos2(dislike_rect.left() + 18.0, dislike_rect.center().y),
                        icon::THUMBS_DOWN,
                        15.0,
                        dislike_col,
                    );
                    ui.painter().text(
                        pos2(dislike_rect.left() + 31.0, dislike_rect.center().y),
                        Align2::LEFT_CENTER,
                        d_txt,
                        theme::medium(12.5),
                        dislike_col,
                    );
                } else {
                    theme::draw_icon_centered(
                        ui.painter(),
                        dislike_rect.center(),
                        icon::THUMBS_DOWN,
                        15.0,
                        dislike_col,
                    );
                }

                if like_resp.clicked() {
                    let _ = app.backend.tx.send(Request::SetLike {
                        video_id: w.id.clone(),
                        like: !w.is_liked,
                    });
                }
                if dislike_resp.on_hover_text("I dislike this").clicked() {
                    let _ = app.backend.tx.send(Request::SetDislike {
                        video_id: w.id.clone(),
                        dislike: !w.is_disliked,
                    });
                }
            };

            if rtl {
                draw_more_menu(ui, self);
                draw_save_pill(ui, self);
                draw_download_pill(ui, self);
                draw_share_pill(ui, self);
                draw_like_dislike_pill(ui, self);
            } else {
                draw_like_dislike_pill(ui, self);
                draw_share_pill(ui, self);
                draw_download_pill(ui, self);
                draw_save_pill(ui, self);
                draw_more_menu(ui, self);
            }
        });
    }

    fn draw_related_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(w) = self.watch.as_ref() else {
            return;
        };
        let is_live = w.is_live;
        let has_chat = w.live_chat_token.is_some() || !self.live_chat.is_empty();
        let has_queue = !self.queue.is_empty();
        let has_transcript = !w.id.starts_with("local:")
            && (w.transcript_params.is_some() || !w.captions.is_empty());
        let mut load_transcript = false;
        if has_chat || has_queue || has_transcript {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(
                        self.right_tab == RightTab::UpNext,
                        RichText::new("Up Next").font(theme::semibold(14.0)),
                    )
                    .clicked()
                {
                    self.right_tab = RightTab::UpNext;
                }
                if has_queue
                    && ui
                        .selectable_label(
                            self.right_tab == RightTab::Queue,
                            RichText::new(format!("Queue ({})", self.queue.len()))
                                .font(theme::semibold(14.0)),
                        )
                        .clicked()
                {
                    self.right_tab = RightTab::Queue;
                }
                if has_chat
                    && ui
                        .selectable_label(
                            self.right_tab == RightTab::LiveChat,
                            RichText::new(if is_live { "Live Chat" } else { "Chat Replay" })
                                .font(theme::semibold(14.0)),
                        )
                        .clicked()
                {
                    self.right_tab = RightTab::LiveChat;
                }
                if has_transcript
                    && ui
                        .selectable_label(
                            self.right_tab == RightTab::Transcript,
                            RichText::new("Transcript").font(theme::semibold(14.0)),
                        )
                        .clicked()
                {
                    self.right_tab = RightTab::Transcript;
                    if self.transcript.is_empty() && !self.transcript_loading {
                        load_transcript = true;
                    }
                }
            });
            ui.add_space(8.0);
        } else {
            ui.label(RichText::new("Up Next").font(theme::semibold(15.5)));
            ui.add_space(8.0);
        }

        if load_transcript {
            self.request_transcript(None);
        }
        if has_transcript && self.right_tab == RightTab::Transcript {
            self.draw_transcript_panel(ui, ctx);
            return;
        }
        let Some(w) = self.watch.as_ref() else {
            return;
        };

        if has_queue && self.right_tab == RightTab::Queue {
            let mut clear_queue = false;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} queued", self.queue.len()))
                        .font(theme::semibold(12.5))
                        .color(theme::TEXT_DIM),
                );
                if self.queue.len() > 1 {
                    ui.label(
                        RichText::new("· Hold & drag to reorder")
                            .font(theme::regular(11.5))
                            .color(theme::TEXT_FAINT),
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (crect, cresp) =
                        ui.allocate_exact_size(Vec2::new(66.0, 26.0), Sense::click());
                    let chov = ui
                        .ctx()
                        .animate_bool_with_time(cresp.id, cresp.hovered(), 0.12);
                    if cresp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    ui.painter().rect_filled(
                        crect,
                        8.0,
                        theme::lerp_color(theme::CHIP, theme::CHIP_HOVER, chov),
                    );
                    theme::draw_icon_centered(
                        ui.painter(),
                        pos2(crect.left() + 15.0, crect.center().y),
                        icon::TRASH_2,
                        12.5,
                        theme::TEXT,
                    );
                    ui.painter().text(
                        pos2(crect.left() + 26.0, crect.center().y),
                        Align2::LEFT_CENTER,
                        "Clear",
                        theme::medium(12.0),
                        theme::TEXT,
                    );
                    if cresp.clicked() {
                        clear_queue = true;
                    }

                    if self.signed_in {
                        ui.add_space(6.0);
                        let (srect, sresp) =
                            ui.allocate_exact_size(Vec2::new(64.0, 26.0), Sense::click());
                        let shov = ui
                            .ctx()
                            .animate_bool_with_time(sresp.id, sresp.hovered(), 0.12);
                        if sresp.hovered() {
                            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                        }
                        let sfill = if self.save_queue_open {
                            theme::YT_BLUE.gamma_multiply(0.22)
                        } else {
                            theme::lerp_color(theme::CHIP, theme::CHIP_HOVER, shov)
                        };
                        let scol = if self.save_queue_open {
                            theme::YT_BLUE
                        } else {
                            theme::TEXT
                        };
                        ui.painter().rect_filled(srect, 8.0, sfill);
                        theme::draw_icon_centered(
                            ui.painter(),
                            pos2(srect.left() + 15.0, srect.center().y),
                            icon::LIST_PLUS,
                            13.0,
                            scol,
                        );
                        ui.painter().text(
                            pos2(srect.left() + 27.0, srect.center().y),
                            Align2::LEFT_CENTER,
                            "Save",
                            theme::medium(12.0),
                            scol,
                        );
                        if sresp.on_hover_text("Save queue as playlist").clicked() {
                            self.save_queue_open = !self.save_queue_open;
                            if self.save_queue_open && self.save_queue_title.is_empty() {
                                self.save_queue_title = "Saved Queue".to_owned();
                            }
                        }
                    }
                });
            });
            if clear_queue {
                self.queue.clear();
                self.active_mix_id = None;
                self.active_mix_title.clear();
                self.active_mix_continuation = None;
                self.active_mix_loading = false;
                self.queue_drag_idx = None;
                self.queue_drag_origin = None;
                self.queue_drag_active = false;
                self.save_queue_open = false;
                self.persist_queue();
                self.right_tab = RightTab::UpNext;
                return;
            }
            if self.active_mix_id.is_some() {
                ui.add_space(4.0);
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(36, 48, 78, 130))
                    .stroke(Stroke::new(1.0, theme::YT_BLUE.gamma_multiply(0.35)))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            theme::draw_icon(ui, icon::RADIO, 13.5, theme::YT_BLUE);
                            ui.add_space(4.0);
                            let lbl = if self.active_mix_title.is_empty() {
                                "Mix Radio · Auto-refill".to_owned()
                            } else {
                                format!("{} · Auto-refill", self.active_mix_title)
                            };
                            ui.add(
                                egui::Label::new(
                                    RichText::new(lbl)
                                        .font(theme::medium(11.5))
                                        .color(Color32::WHITE),
                                )
                                .truncate(),
                            );
                            if self.active_mix_loading {
                                ui.spinner();
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_circle_button(ui, icon::X, 11.5, "Stop Mix auto-refill") {
                                    self.active_mix_id = None;
                                    self.active_mix_continuation = None;
                                    self.active_mix_loading = false;
                                }
                            });
                        });
                    });
            }
            if self.save_queue_open && self.signed_in {
                ui.add_space(6.0);
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let input_w = (ui.available_width() - 92.0).max(90.0);
                            let te = ui.add_sized(
                                Vec2::new(input_w, 28.0),
                                egui::TextEdit::singleline(&mut self.save_queue_title)
                                    .hint_text("Playlist title..."),
                            );
                            let submit = ui
                                .add_sized(
                                    Vec2::new(56.0, 28.0),
                                    egui::Button::new(
                                        RichText::new("Save")
                                            .font(theme::semibold(12.0))
                                            .color(Color32::from_rgb(15, 15, 15)),
                                    )
                                    .fill(theme::YT_BLUE)
                                    .corner_radius(7),
                                )
                                .clicked()
                                || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                            if submit {
                                let title = self.save_queue_title.trim().to_owned();
                                if !title.is_empty() {
                                    let mut seen = std::collections::HashSet::new();
                                    let mut video_ids = Vec::new();
                                    for item in &self.queue {
                                        if !item.id.starts_with("local:")
                                            && seen.insert(item.id.clone())
                                        {
                                            video_ids.push(item.id.clone());
                                        }
                                    }
                                    if !video_ids.is_empty() {
                                        let _ = self.backend.tx.send(Request::CreatePlaylist {
                                            title,
                                            description: None,
                                            privacy: self.save_queue_privacy,
                                            video_ids,
                                        });
                                        self.save_queue_title.clear();
                                        self.save_queue_open = false;
                                    } else {
                                        self.toast("No YouTube videos in queue to save");
                                    }
                                }
                            }
                            if icon_circle_button(ui, icon::X, 13.0, "Cancel") {
                                self.save_queue_open = false;
                            }
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Visibility:")
                                    .font(theme::regular(11.0))
                                    .color(theme::TEXT_DIM),
                            );
                            for vis in crate::model::PlaylistPrivacy::ALL {
                                let selected = self.save_queue_privacy == vis;
                                if ui
                                    .selectable_label(
                                        selected,
                                        RichText::new(vis.label())
                                            .font(theme::semibold(11.0))
                                            .color(if selected {
                                                theme::YT_BLUE
                                            } else {
                                                theme::TEXT_DIM
                                            }),
                                    )
                                    .clicked()
                                {
                                    self.save_queue_privacy = vis;
                                }
                            }
                        });
                    });
            }
            ui.add_space(6.0);
            let q_items = self.queue.clone();
            let mut play_idx = None;
            let mut remove_idx = None;
            let mut move_top_idx = None;
            let mut queue_reordered = false;
            let mut row_rects: Vec<(usize, Rect)> = Vec::with_capacity(q_items.len());
            let (prim_pressed, prim_down, ptr_pos) = ui.ctx().input(|i| {
                (
                    i.pointer.primary_pressed(),
                    i.pointer.primary_down(),
                    i.pointer.interact_pos(),
                )
            });
            ScrollArea::vertical().id_salt("queue_scroll").show(ui, |ui| {
                for (qi, item) in q_items.iter().enumerate() {
                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 82.0),
                        Sense::click_and_drag(),
                    );
                    row_rects.push((qi, row_rect));

                    let inner = row_rect.shrink(6.0);
                    let num_w = 18.0;
                    let remove_w = 28.0;
                    let x_rect = Rect::from_center_size(
                        pos2(inner.right() - remove_w * 0.5, inner.center().y),
                        Vec2::splat(remove_w),
                    );
                    if prim_pressed
                        && ptr_pos.is_some_and(|p| row_rect.contains(p) && !x_rect.contains(p))
                    {
                        self.queue_drag_idx = Some(qi);
                        self.queue_drag_origin = ptr_pos.map(|p| (p, Instant::now()));
                        self.queue_drag_active = false;
                    }
                    let is_dragging = self.queue_drag_idx == Some(qi) && self.queue_drag_active;
                    let hover_t = ui
                        .ctx()
                        .animate_bool_with_time(row_resp.id, row_resp.hovered(), 0.12);
                    if is_dragging {
                        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                    } else if row_resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }

                    let row_fill = if is_dragging {
                        theme::CHIP
                    } else if hover_t > 0.01 {
                        theme::SURFACE.gamma_multiply(hover_t)
                    } else {
                        Color32::TRANSPARENT
                    };
                    if row_fill != Color32::TRANSPARENT {
                        ui.painter().rect_filled(row_rect, 10.0, row_fill);
                    }
                    if is_dragging {
                        ui.painter().rect_stroke(
                            row_rect,
                            10.0,
                            Stroke::new(1.5, theme::YT_BLUE),
                            egui::StrokeKind::Inside,
                        );
                    }

                    let thumb_w = 122.0;
                    let thumb_h = 68.0;
                    let idx_pos = pos2(inner.left() + num_w * 0.5, inner.center().y);
                    ui.painter().text(
                        idx_pos,
                        Align2::CENTER_CENTER,
                        format!("{}", qi + 1),
                        theme::semibold(11.5),
                        if qi == 0 {
                            theme::YT_BLUE
                        } else {
                            theme::TEXT_DIM
                        },
                    );

                    let thumb_rect = Rect::from_min_size(
                        pos2(inner.left() + num_w + 4.0, inner.center().y - thumb_h * 0.5),
                        Vec2::new(thumb_w, thumb_h),
                    );
                    let mut thumb_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(thumb_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    draw_thumbnail(
                        &mut thumb_ui,
                        &item.thumbnail_url,
                        &item.duration_text,
                        Vec2::new(thumb_w, thumb_h),
                        7.0,
                        item.watched_percent,
                    );

                    let x_resp = ui.interact(
                        x_rect,
                        ui.scope_id().with(("q_rm", qi, &item.id)),
                        Sense::click(),
                    );
                    let x_hov = ui
                        .ctx()
                        .animate_bool_with_time(x_resp.id, x_resp.hovered(), 0.12);
                    if x_resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if x_hov > 0.01 {
                        ui.painter().circle_filled(
                            x_rect.center(),
                            13.0,
                            Color32::from_white_alpha((26.0 * x_hov) as u8),
                        );
                    }
                    theme::draw_icon_centered(
                        ui.painter(),
                        x_rect.center(),
                        icon::X,
                        13.5,
                        if x_resp.hovered() {
                            Color32::WHITE
                        } else {
                            theme::TEXT_DIM
                        },
                    );
                    let x_hovered = x_resp.hovered();
                    if x_resp.on_hover_text("Remove from queue").clicked() {
                        remove_idx = Some(qi);
                    }

                    let text_rect = Rect::from_min_max(
                        pos2(thumb_rect.right() + 8.0, inner.top() + 8.0),
                        pos2(x_rect.left() - 6.0, inner.bottom() - 4.0),
                    );
                    let mut text_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(text_rect)
                            .layout(Layout::top_down(Align::LEFT)),
                    );
                    let tclip = text_ui.clip_rect().intersect(text_rect);
                    text_ui.set_clip_rect(tclip);
                    text_ui.spacing_mut().item_spacing.y = 4.0;
                    let clamped = clamp_two_lines(
                        &text_ui,
                        &item.title,
                        theme::medium(12.5),
                        (text_rect.width() - 4.0).max(40.0),
                    );
                    text_ui.add(
                        egui::Label::new(
                            RichText::new(clamped)
                                .font(theme::medium(12.5))
                                .color(theme::TEXT),
                        )
                        .wrap(),
                    );
                    text_ui.add(
                        egui::Label::new(
                            RichText::new(&item.channel)
                                .font(theme::regular(11.5))
                                .color(theme::TEXT_DIM),
                        )
                        .truncate(),
                    );

                    row_resp.context_menu(|ui| {
                        if ui.button("Play now").clicked() {
                            play_idx = Some(qi);
                            ui.close();
                        }
                        if qi > 0 && ui.button("Move to top").clicked() {
                            move_top_idx = Some(qi);
                            ui.close();
                        }
                        if ui.button("Remove from queue").clicked() {
                            remove_idx = Some(qi);
                            ui.close();
                        }
                    });
                    if row_resp.clicked() && !x_hovered && !self.queue_drag_active {
                        play_idx = Some(qi);
                    }
                    ui.add_space(4.0);
                }
            });

            // Live left-click hold & drag reordering across Queue items
            if prim_down {
                if let (Some(drag_i), Some(pos)) = (self.queue_drag_idx, ptr_pos) {
                    if !self.queue_drag_active {
                        if let Some((orig, started_at)) = self.queue_drag_origin {
                            if orig.distance(pos) >= 6.0
                                || started_at.elapsed() >= Duration::from_millis(180)
                            {
                                self.queue_drag_active = true;
                            } else {
                                ui.ctx().request_repaint_after(Duration::from_millis(30));
                            }
                        }
                    }
                    if self.queue_drag_active {
                        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                        if let Some(&(target_i, _)) = row_rects.iter().find(|&&(i, r)| {
                            i != drag_i && pos.y >= r.top() + 4.0 && pos.y <= r.bottom() - 4.0
                        }) {
                            if drag_i < self.queue.len() && target_i < self.queue.len() {
                                let moved = self.queue.remove(drag_i);
                                self.queue.insert(target_i, moved);
                                self.queue_drag_idx = Some(target_i);
                                queue_reordered = true;
                            }
                        }
                    }
                }
            } else if self.queue_drag_idx.is_some() || self.queue_drag_active {
                self.queue_drag_idx = None;
                self.queue_drag_origin = None;
                self.queue_drag_active = false;
            }

            if queue_reordered {
                self.persist_queue();
            }
            if let Some(i) = move_top_idx {
                if i < self.queue.len() {
                    let item = self.queue.remove(i);
                    self.queue.insert(0, item);
                    self.persist_queue();
                }
            }
            if let Some(i) = remove_idx {
                self.queue.remove(i);
                self.queue_drag_idx = None;
                self.queue_drag_origin = None;
                self.queue_drag_active = false;
                self.persist_queue();
                if self.queue.is_empty() {
                    self.right_tab = RightTab::UpNext;
                }
            } else if let Some(i) = play_idx {
                let next = self.queue.remove(i);
                self.queue_drag_idx = None;
                self.queue_drag_origin = None;
                self.queue_drag_active = false;
                self.persist_queue();
                self.maybe_refill_mix_queue();
                self.open_video(next);
            }
            return;
        }

        if has_chat && self.right_tab == RightTab::LiveChat {
            let can_send = !self.live_chat_is_replay
                && self.signed_in
                && self.live_chat_send_params.is_some();
            let picker_reserve = if can_send && self.live_chat_emote_picker_open {
                104.0
            } else {
                0.0
            };
            let input_reserve = if can_send { 54.0 + picker_reserve } else { 16.0 };
            Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(12)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt("live_chat_scroll")
                        .stick_to_bottom(true)
                        .max_height((ui.available_height() - input_reserve).max(160.0))
                        .show(ui, |ui| {
                            if self.live_chat.is_empty() {
                                let wait_msg = if self.live_chat_is_replay {
                                    "Waiting for chat replay messages..."
                                } else {
                                    "Waiting for live chat messages..."
                                };
                                ui.label(
                                    RichText::new(wait_msg)
                                        .font(theme::regular(12.5))
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            for m in &self.live_chat {
                                match &m.kind {
                                    LiveChatKind::SuperChat { amount, bg_color } => {
                                        let r = ((*bg_color >> 16) & 0xFF) as u8;
                                        let g = ((*bg_color >> 8) & 0xFF) as u8;
                                        let b = (*bg_color & 0xFF) as u8;
                                        let accent = Color32::from_rgb(r, g, b);
                                        let body_bg = Color32::from_rgba_unmultiplied(r, g, b, 55);
                                        Frame::new()
                                            .fill(body_bg)
                                            .stroke(Stroke::new(1.0, accent))
                                            .corner_radius(8)
                                            .inner_margin(Margin::symmetric(8, 6))
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.horizontal(|ui| {
                                                    draw_avatar(
                                                        ui,
                                                        m.avatar_url.as_deref(),
                                                        &m.author,
                                                        20.0,
                                                    );
                                                    ui.label(
                                                        RichText::new(&m.author)
                                                            .font(theme::semibold(12.0))
                                                            .color(Color32::WHITE),
                                                    );
                                                    ui.with_layout(
                                                        Layout::right_to_left(Align::Center),
                                                        |ui| {
                                                            Frame::new()
                                                                .fill(accent)
                                                                .corner_radius(6)
                                                                .inner_margin(Margin::symmetric(
                                                                    6, 2,
                                                                ))
                                                                .show(ui, |ui| {
                                                                    let lum = 0.299 * f32::from(r)
                                                                        + 0.587 * f32::from(g)
                                                                        + 0.114 * f32::from(b);
                                                                    let fg = if lum > 155.0 {
                                                                        Color32::BLACK
                                                                    } else {
                                                                        Color32::WHITE
                                                                    };
                                                                    ui.label(
                                                                        RichText::new(amount)
                                                                            .font(theme::bold(11.5))
                                                                            .color(fg),
                                                                    );
                                                                });
                                                        },
                                                    );
                                                });
                                                if !m.runs.is_empty() || !m.text.is_empty() {
                                                    ui.add_space(3.0);
                                                    draw_live_chat_message_runs(ui, m);
                                                }
                                            });
                                    }
                                    LiveChatKind::Membership { header } => {
                                        let green = Color32::from_rgb(15, 157, 88);
                                        Frame::new()
                                            .fill(Color32::from_rgba_unmultiplied(15, 157, 88, 38))
                                            .stroke(Stroke::new(1.0, green))
                                            .corner_radius(8)
                                            .inner_margin(Margin::symmetric(8, 6))
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.horizontal(|ui| {
                                                    draw_avatar(
                                                        ui,
                                                        m.avatar_url.as_deref(),
                                                        &m.author,
                                                        20.0,
                                                    );
                                                    theme::draw_icon(ui, icon::SPARKLES, 13.0, green);
                                                    ui.vertical(|ui| {
                                                        ui.label(
                                                            RichText::new(&m.author)
                                                                .font(theme::semibold(12.0))
                                                                .color(Color32::WHITE),
                                                        );
                                                        ui.label(
                                                            RichText::new(header)
                                                                .font(theme::medium(11.5))
                                                                .color(Color32::from_rgb(
                                                                    120, 230, 175,
                                                                )),
                                                        );
                                                    });
                                                });
                                                if !m.runs.is_empty() || !m.text.is_empty() {
                                                    ui.add_space(3.0);
                                                    draw_live_chat_message_runs(ui, m);
                                                }
                                            });
                                    }
                                    LiveChatKind::Normal => {
                                        ui.horizontal_top(|ui| {
                                            draw_avatar(
                                                ui,
                                                m.avatar_url.as_deref(),
                                                &m.author,
                                                20.0,
                                            );
                                            ui.add_space(4.0);
                                            ui.horizontal_wrapped(|ui| {
                                                ui.spacing_mut().item_spacing.x = 3.0;
                                                let author_col = if m.is_owner {
                                                    Color32::from_rgb(255, 214, 0)
                                                } else if m.is_mod {
                                                    theme::YT_BLUE
                                                } else {
                                                    theme::TEXT_DIM
                                                };
                                                ui.label(
                                                    RichText::new(format!("{}:", m.author))
                                                        .font(theme::semibold(12.0))
                                                        .color(author_col),
                                                );
                                                draw_live_chat_message_runs(ui, m);
                                            });
                                        });
                                    }
                                }
                                ui.add_space(5.0);
                            }
                        });
                    if can_send {
                        if let Some(params) = self.live_chat_send_params.clone() {
                            if self.live_chat_emote_picker_open && !self.live_chat_emotes.is_empty()
                            {
                                ui.add_space(6.0);
                                Frame::new()
                                    .fill(Color32::from_rgb(30, 30, 36))
                                    .stroke(Stroke::new(1.0, theme::BORDER))
                                    .corner_radius(8)
                                    .inner_margin(Margin::same(6))
                                    .show(ui, |ui| {
                                        ScrollArea::vertical()
                                            .id_salt("live_chat_emote_picker")
                                            .max_height(82.0)
                                            .show(ui, |ui| {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                                                    for em in &self.live_chat_emotes {
                                                        let (erect, eresp) = ui.allocate_exact_size(
                                                            vec2(26.0, 26.0),
                                                            Sense::click(),
                                                        );
                                                        if eresp.hovered() {
                                                            ui.ctx().set_cursor_icon(
                                                                CursorIcon::PointingHand,
                                                            );
                                                            ui.painter().rect_filled(
                                                                erect,
                                                                5.0,
                                                                theme::CHIP_HOVER,
                                                            );
                                                        }
                                                        if let Some(u) = &em.url {
                                                            egui::Image::new(u)
                                                                .show_loading_spinner(false)
                                                                .fit_to_exact_size(vec2(22.0, 22.0))
                                                                .paint_at(ui, erect.shrink(2.0));
                                                        } else {
                                                            ui.painter().text(
                                                                erect.center(),
                                                                Align2::CENTER_CENTER,
                                                                &em.shortcut,
                                                                theme::regular(10.0),
                                                                theme::TEXT,
                                                            );
                                                        }
                                                        if eresp
                                                            .on_hover_text(&em.shortcut)
                                                            .clicked()
                                                        {
                                                            if !self.live_chat_draft.is_empty()
                                                                && !self
                                                                    .live_chat_draft
                                                                    .ends_with(' ')
                                                            {
                                                                self.live_chat_draft.push(' ');
                                                            }
                                                            self.live_chat_draft
                                                                .push_str(&em.shortcut);
                                                            self.live_chat_draft.push(' ');
                                                        }
                                                    }
                                                });
                                            });
                                    });
                            }
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let has_emotes = !self.live_chat_emotes.is_empty();
                                if has_emotes {
                                    let (srect, sresp) =
                                        ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                                    if sresp.hovered() {
                                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                    }
                                    if self.live_chat_emote_picker_open || sresp.hovered() {
                                        ui.painter().circle_filled(
                                            srect.center(),
                                            13.0,
                                            theme::CHIP_HOVER,
                                        );
                                    }
                                    theme::draw_smile_icon(
                                        ui.painter(),
                                        srect.center(),
                                        16.0,
                                        if self.live_chat_emote_picker_open {
                                            theme::YT_BLUE
                                        } else {
                                            theme::TEXT_DIM
                                        },
                                    );
                                    if sresp.on_hover_text("Channel emotes").clicked() {
                                        self.live_chat_emote_picker_open =
                                            !self.live_chat_emote_picker_open;
                                    }
                                }
                                let reserve_w = if has_emotes { 98.0 } else { 64.0 };
                                let input_w = (ui.available_width() - reserve_w).max(90.0);
                                let te = ui.add_sized(
                                    Vec2::new(input_w, 28.0),
                                    egui::TextEdit::singleline(&mut self.live_chat_draft)
                                        .hint_text("Say something..."),
                                );
                                let send = ui
                                    .add_sized(
                                        Vec2::new(56.0, 28.0),
                                        egui::Button::new(
                                            RichText::new("Send")
                                                .font(theme::semibold(12.0))
                                                .color(Color32::from_rgb(15, 15, 15)),
                                        )
                                        .fill(theme::YT_BLUE)
                                        .corner_radius(14),
                                    )
                                    .clicked()
                                    || (te.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if send {
                                    let msg = self.live_chat_draft.trim().to_owned();
                                    if !msg.is_empty() {
                                        let _ = self.backend.tx.send(Request::SendLiveChat {
                                            video_id: w.id.clone(),
                                            params,
                                            message: msg,
                                            emotes: self.live_chat_emotes.clone(),
                                        });
                                        self.live_chat_draft.clear();
                                        self.live_chat_emote_picker_open = false;
                                    }
                                }
                            });
                        }
                    }
                });
            return;
        }

        let chips = build_up_next_chips(w);
        let mut clicked_chip: Option<(usize, crate::model::UpNextFilterMode, String, Option<String>)> =
            None;
        if !chips.is_empty() {
            let pan_id = ui.make_persistent_id("up_next_chips_pan");
            let (mut scroll_x, mut dragging, mut drag_dist) = ui
                .data(|d| d.get_temp::<(f32, bool, f32)>(pan_id))
                .unwrap_or((0.0, false, 0.0));
            let drag_blocked = drag_dist >= 5.0;

            let bar_out = ui.scope(|ui| {
                ScrollArea::horizontal()
                    .id_salt("up_next_chips_scroll")
                    .horizontal_scroll_offset(scroll_x)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for (idx, (mode, title, cont)) in chips.iter().enumerate() {
                                let selected = if self.up_next_active_chip < chips.len() {
                                    idx == self.up_next_active_chip
                                } else {
                                    *mode == self.up_next_mode
                                };
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
                                        .font(theme::medium(12.0))
                                        .color(text_col),
                                )
                                .fill(fill)
                                .corner_radius(8)
                                .min_size(vec2(0.0, 28.0));
                                let resp = ui.add(btn);
                                if resp.hovered() && !drag_blocked {
                                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                }
                                if resp.clicked() && !drag_blocked {
                                    clicked_chip =
                                        Some((idx, *mode, title.clone(), cont.clone()));
                                }
                            }
                        });
                    })
            });
            let scroll_out = bar_out.inner;
            let bar_rect = bar_out.response.rect;
            let max_scroll =
                (scroll_out.content_size.x - scroll_out.inner_rect.width()).max(0.0);
            scroll_x = scroll_out.state.clamped_offset().x.clamp(0.0, max_scroll);

            let hovered_bar = ui.rect_contains_pointer(bar_rect);
            let (primary_down, primary_pressed, delta_x, wheel_y) = ui.ctx().input(|i| {
                (
                    i.pointer.primary_down(),
                    i.pointer.primary_pressed(),
                    i.pointer.delta().x,
                    i.smooth_scroll_delta.y,
                )
            });
            if hovered_bar && primary_pressed {
                dragging = true;
                drag_dist = 0.0;
            }
            if dragging && primary_down {
                drag_dist += delta_x.abs();
                scroll_x = (scroll_x - delta_x).clamp(0.0, max_scroll);
                if drag_dist >= 5.0 {
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                }
            } else if !primary_down {
                dragging = false;
                drag_dist = 0.0;
            }
            if hovered_bar && wheel_y.abs() > 0.1 {
                scroll_x = (scroll_x - wheel_y * 1.25).clamp(0.0, max_scroll);
            }
            ui.data_mut(|d| d.insert_temp(pan_id, (scroll_x, dragging, drag_dist)));
            ui.add_space(6.0);
        }
        if self.up_next_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Updating recommendations...")
                        .font(theme::regular(11.5))
                        .color(theme::TEXT_DIM),
                );
            });
            ui.add_space(4.0);
        }

        let related = w.related.clone();
        let mut right_action: Option<GridAction> = None;
        ScrollArea::vertical().id_salt("related_scroll").show(ui, |ui| {
            for item in related {
                let row_w = ui.available_width();
                let (row_rect, row_resp) =
                    ui.allocate_exact_size(Vec2::new(row_w, 94.0), Sense::click());
                let hover_t = ui
                    .ctx()
                    .animate_bool_with_time(row_resp.id, row_resp.hovered(), 0.12);
                if row_resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if hover_t > 0.01 {
                    ui.painter().rect_filled(
                        row_rect,
                        8.0,
                        theme::SURFACE.gamma_multiply(hover_t),
                    );
                }
                let inner = row_rect.shrink(4.0);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                let rclip = child.clip_rect().intersect(inner);
                child.set_clip_rect(rclip);
                child.horizontal(|ui| {
                    let thumb_size = Vec2::new(152.0, 86.0);
                    draw_thumbnail(
                        ui,
                        &item.thumbnail_url,
                        &item.duration_text,
                        thumb_size,
                        8.0,
                        item.watched_percent,
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        let text_w = (inner.width() - 162.0).max(60.0);
                        ui.set_max_width(text_w);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let clamped_title =
                            clamp_two_lines(ui, &item.title, theme::medium(12.8), text_w - 6.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(clamped_title)
                                    .font(theme::medium(12.8))
                                    .color(theme::TEXT),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.channel)
                                    .font(theme::regular(11.5))
                                    .color(theme::TEXT_DIM),
                            )
                            .truncate(),
                        );
                        let meta = join_meta(&item.views_text, &item.published_text);
                        if !meta.is_empty() {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(meta)
                                        .font(theme::regular(11.5))
                                        .color(theme::TEXT_DIM),
                                )
                                .truncate(),
                            );
                        }
                    });
                });
                row_resp.context_menu(|ui| {
                    draw_video_context_menu(ui, &item, &mut right_action, true);
                });
                let row_hover_resp = if item.title.chars().count() > 28 {
                    row_resp.on_hover_text(&item.title)
                } else {
                    row_resp
                };
                if row_hover_resp.middle_clicked() && right_action.is_none() {
                    right_action = Some(GridAction::OpenInSecondTab(item));
                } else if row_hover_resp.clicked() && right_action.is_none() {
                    right_action = Some(GridAction::Open(item));
                }
                ui.add_space(4.0);
            }
        });
        if let Some((idx, mode, title, cont)) = clicked_chip {
            self.select_up_next_chip(idx, mode, title, cont);
        }
        if let Some(act) = right_action {
            self.handle_grid_action(ctx, act, |_| {});
        }
    }

    fn draw_transcript_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(w) = self.watch.clone() else {
            return;
        };
        Frame::new()
            .fill(theme::SURFACE)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(12)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let input_w = (ui.available_width() - 82.0).max(90.0);
                    ui.add_sized(
                        Vec2::new(input_w, 28.0),
                        egui::TextEdit::singleline(&mut self.transcript_query)
                            .hint_text("Search transcript..."),
                    );
                    if !self.transcript_query.is_empty() && icon_circle_button(ui, icon::X, 12.0, "Clear") {
                        self.transcript_query.clear();
                    }
                    let follow_btn = egui::Button::new(
                        RichText::new("Follow")
                            .font(theme::medium(11.5))
                            .color(if self.transcript_auto_scroll {
                                Color32::from_rgb(15, 15, 15)
                            } else {
                                theme::TEXT
                            }),
                    )
                    .fill(if self.transcript_auto_scroll {
                        theme::YT_BLUE
                    } else {
                        theme::CHIP
                    })
                    .corner_radius(8);
                    if ui
                        .add(follow_btn)
                        .on_hover_text("Auto-scroll transcript with current playback time")
                        .clicked()
                    {
                        self.transcript_auto_scroll = !self.transcript_auto_scroll;
                        if self.transcript_auto_scroll {
                            self.transcript_last_active_idx = None;
                        }
                    }
                });

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(5.0, 4.0);
                    if w.captions.len() > 1 {
                        let cur_name = w
                            .captions
                            .get(self.transcript_caption_idx)
                            .map(|c| c.name.as_str())
                            .unwrap_or("Track");
                        let mut switch_cap: Option<usize> = None;
                        egui::ComboBox::from_id_salt("transcript_lang_pick")
                            .width(120.0)
                            .selected_text(cur_name)
                            .show_ui(ui, |ui| {
                                for (idx, c) in w.captions.iter().enumerate() {
                                    if ui
                                        .selectable_label(
                                            self.transcript_caption_idx == idx,
                                            &c.name,
                                        )
                                        .clicked()
                                    {
                                        switch_cap = Some(idx);
                                    }
                                }
                            });
                        if let Some(idx) = switch_cap {
                            self.request_transcript(Some(idx));
                        }
                    }
                    if !self.transcript.is_empty() {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Copy").font(theme::medium(11.0)),
                                )
                                .fill(theme::CHIP)
                                .corner_radius(7),
                            )
                            .clicked()
                        {
                            ctx.copy_text(crate::innertube::format_transcript_txt(
                                &self.transcript,
                            ));
                            self.toast("Copied transcript to clipboard");
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Export .txt").font(theme::medium(11.0)),
                                )
                                .fill(theme::CHIP)
                                .corner_radius(7),
                            )
                            .clicked()
                        {
                            self.export_transcript_file(&w.title, false);
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Export .srt").font(theme::medium(11.0)),
                                )
                                .fill(theme::CHIP)
                                .corner_radius(7),
                            )
                            .clicked()
                        {
                            self.export_transcript_file(&w.title, true);
                        }
                    } else if !self.transcript_loading
                        && ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Reload").font(theme::medium(11.0)),
                                )
                                .fill(theme::CHIP)
                                .corner_radius(7),
                            )
                            .clicked()
                    {
                        self.request_transcript(None);
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                if self.transcript_loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new("Loading transcript...")
                                .font(theme::regular(12.5))
                                .color(theme::TEXT_DIM),
                        );
                    });
                    return;
                }
                if let Some(err) = &self.transcript_error {
                    ui.label(
                        RichText::new(err)
                            .font(theme::regular(12.0))
                            .color(theme::TEXT_DIM),
                    );
                    return;
                }
                if self.transcript.is_empty() {
                    ui.label(
                        RichText::new("No transcript available for this video.")
                            .font(theme::regular(12.5))
                            .color(theme::TEXT_DIM),
                    );
                    return;
                }

                let q = self.transcript_query.trim().to_ascii_lowercase();
                let pos = self.playback.time_pos.max(0.0);
                let active_idx = self
                    .transcript
                    .iter()
                    .rposition(|c| pos >= c.start_secs && pos <= c.end_secs + 0.5);
                let should_scroll = self.transcript_auto_scroll
                    && q.is_empty()
                    && active_idx.is_some()
                    && active_idx != self.transcript_last_active_idx;
                if should_scroll {
                    self.transcript_last_active_idx = active_idx;
                }

                let mut seek_to: Option<f64> = None;
                let max_h = (ui.available_height() - 8.0).clamp(240.0, 560.0);
                ScrollArea::vertical()
                    .id_salt("watch_transcript_scroll")
                    .max_height(max_h)
                    .show(ui, |ui| {
                        for (idx, cue) in self.transcript.iter().enumerate() {
                            if !q.is_empty() && !cue.text.to_ascii_lowercase().contains(&q) {
                                continue;
                            }
                            let is_active = active_idx == Some(idx);
                            let row_fill = if is_active {
                                Color32::from_rgba_unmultiplied(62, 166, 255, 34)
                            } else {
                                Color32::TRANSPARENT
                            };
                            let frame_resp = Frame::new()
                                .fill(row_fill)
                                .corner_radius(7)
                                .inner_margin(Margin::symmetric(8, 5))
                                .show(ui, |ui| {
                                    ui.set_width((ui.available_width() - 6.0).max(80.0));
                                    ui.horizontal_top(|ui| {
                                        let ts_col = if is_active {
                                            theme::YT_BLUE
                                        } else {
                                            theme::TEXT_DIM
                                        };
                                        ui.label(
                                            RichText::new(format_duration(cue.start_secs))
                                                .font(theme::semibold(11.5))
                                                .color(ts_col),
                                        );
                                        ui.add_space(4.0);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&cue.text)
                                                    .font(if is_active {
                                                        theme::semibold(12.5)
                                                    } else {
                                                        theme::regular(12.5)
                                                    })
                                                    .color(if is_active {
                                                        Color32::WHITE
                                                    } else {
                                                        theme::TEXT
                                                    }),
                                            )
                                            .wrap(),
                                        );
                                    });
                                })
                                .response
                                .interact(Sense::click());
                            if frame_resp.hovered() {
                                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                            }
                            if frame_resp.clicked() {
                                seek_to = Some(cue.start_secs);
                            }
                            if should_scroll && is_active {
                                frame_resp.scroll_to_me(Some(Align::Center));
                            }
                            ui.add_space(2.0);
                        }
                    });
                if let Some(secs) = seek_to {
                    if let Some(mpv) = &self.mpv {
                        mpv.seek_abs(secs);
                        if self.playback.eof {
                            mpv.set_paused(false);
                        }
                        self.trigger_hud(HudIcon::SeekForward, format_duration(secs));
                    }
                }
            });
    }

    fn export_transcript_file(&mut self, title: &str, as_srt: bool) {
        let dir = self.effective_download_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.toast(format!("Export failed: {e}"));
            return;
        }
        let clean = super::sanitize_filename(title);
        let ext = if as_srt { "srt" } else { "txt" };
        let path = dir.join(format!("{clean}.{ext}"));
        let content = if as_srt {
            crate::innertube::format_transcript_srt(&self.transcript)
        } else {
            crate::innertube::format_transcript_txt(&self.transcript)
        };
        match std::fs::write(&path, content) {
            Ok(()) => self.toast(format!(
                "Exported {}",
                path.file_name().and_then(|s| s.to_str()).unwrap_or(ext)
            )),
            Err(e) => self.toast(format!("Export failed: {e}")),
        }
    }

    pub(super) fn draw_mini_player(&mut self, ctx: &egui::Context) {
        if self.watch.is_some() {
            self.draw_mini_player_inner(ctx, false);
            return;
        }
        if self
            .bg_player_tab
            .as_ref()
            .is_some_and(|b| b.watch.is_some())
        {
            if let Some(mut bg) = self.bg_player_tab.take() {
                self.swap_active_with(&mut bg);
                let action = self.draw_mini_player_inner(ctx, true);
                self.swap_active_with(&mut bg);
                self.bg_player_tab = Some(bg);
                match action {
                    MiniPlayerAction::Expand => {
                        self.switch_player_tab();
                        self.page = Page::Watch;
                        self.watch_scrolled_into_pip = false;
                        self.scroll_watch_to_top = true;
                    }
                    MiniPlayerAction::Close => {
                        self.close_player_tab(false);
                    }
                    MiniPlayerAction::PopOutWinPip => {
                        self.switch_player_tab();
                        self.enter_win_pip(ctx);
                    }
                    MiniPlayerAction::None => {}
                }
            }
        }
    }

    fn draw_mini_player_inner(&mut self, ctx: &egui::Context, previewing_bg: bool) -> MiniPlayerAction {
        let Some(w) = self.watch.as_ref() else {
            return MiniPlayerAction::None;
        };
        let mut mini_action = MiniPlayerAction::None;
        let title = w.title.clone();
        let channel = w.channel.clone();
        let cur_id = w.id.clone();
        let cur_thumb = format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", w.id);
        let dur = self.playback.duration.max(w.duration_secs).max(0.0);
        let mut pos = self.seeking_pos.unwrap_or(self.playback.time_pos).max(0.0);
        let is_live = w.is_live;

        let screen = ctx.content_rect();
        let default_x = (screen.right() - 360.0).max(20.0);
        let default_y = (screen.bottom() - 280.0).max(60.0);

        let mut snap_pos_this_frame: Option<egui::Pos2> = None;
        if let Some(target) = self.pip_snap_target {
            let cur = self.pip_pos.unwrap_or(pos2(default_x, default_y));
            let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.05);
            let k = 1.0 - (-18.0 * dt).exp();
            let next = pos2(
                cur.x + (target.x - cur.x) * k,
                cur.y + (target.y - cur.y) * k,
            );
            if next.distance(target) < 0.75 {
                self.pip_pos = Some(target);
                snap_pos_this_frame = Some(target);
                self.pip_snap_target = None;
            } else {
                self.pip_pos = Some(next);
                snap_pos_this_frame = Some(next);
                ctx.request_repaint();
            }
        }

        let mut area = egui::Area::new(egui::Id::unique("mini_player"))
            .default_pos(pos2(default_x, default_y))
            .movable(true)
            .constrain(true)
            .order(egui::Order::Foreground);
        if let Some(p) = snap_pos_this_frame {
            area = area.current_pos(p);
        }

        let has_queue = !self.queue.is_empty();
        if !has_queue {
            self.pip_queue_open = false;
        }
        let visible_q_rows = 1 + self.queue.iter().filter(|q| q.id != cur_id).count();
        let queue_scroll_h = ((visible_q_rows as f32) * 49.0 + 4.0).clamp(54.0, 210.0);
        let mut play_queue_idx: Option<usize> = None;
        let mut remove_queue_idx: Option<usize> = None;
        let mut clear_queue_now = false;
        let mut play_next_now = false;
        let mut toggle_queue_open = false;

        let area_resp = area
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(12)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 22,
                        spread: 2,
                        color: Color32::from_black_alpha(170),
                    })
                    .inner_margin(Margin::same(1))
                    .show(ui, |ui| {
                        let pip_w = 340.0;
                        let video_size = vec2(pip_w, 191.0);
                        ui.set_width(pip_w);

                        let (vrect, vresp) = ui.allocate_exact_size(video_size, Sense::click());
                        ui.painter().rect_filled(vrect, 11.0, Color32::BLACK);
                        if let Some(mpv) = &self.mpv {
                            ui.painter().add(mpv.paint_callback_rounded(vrect, 11.0, 0.0));
                        }

                        let video_hovered =
                            vresp.hovered() || ui.rect_contains_pointer(vrect);
                        let hover_alpha = ctx.animate_bool_with_time(
                            egui::Id::unique("mini_pip_video_hover"),
                            video_hovered,
                            0.14,
                        );
                        if hover_alpha > 0.01 {
                            ui.painter().rect_filled(
                                vrect,
                                11.0,
                                Color32::from_black_alpha((95.0 * hover_alpha).round() as u8),
                            );
                        }

                        // Hover overlay controls on top of the mini-player video (Expand, Close X, Play/Pause/Next, Timestamp)
                        let mut overlay_clicked = false;
                        if hover_alpha > 0.02 || self.playback.paused {
                            let ctrl_alpha = if hover_alpha > 0.02 { hover_alpha } else { 0.88 };
                            let pill_bg = Color32::from_black_alpha((165.0 * ctrl_alpha).round() as u8);
                            let icon_fg = Color32::from_white_alpha((245.0 * ctrl_alpha).round() as u8);

                            // Top-left Expand (only on hover) + optional Tab badge
                            let mut left_badge_x = vrect.left() + 8.0;
                            if hover_alpha > 0.02 {
                                let exp_rect = Rect::from_min_size(
                                    pos2(vrect.left() + 8.0, vrect.top() + 8.0),
                                    Vec2::splat(30.0),
                                );
                                let exp_resp = ui
                                    .interact(
                                        exp_rect,
                                        ui.scope_id().with("mini_pip_hover_expand"),
                                        Sense::click(),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand);
                                let bg = if exp_resp.hovered() {
                                    Color32::from_black_alpha((215.0 * hover_alpha).round() as u8)
                                } else {
                                    pill_bg
                                };
                                ui.painter().circle_filled(exp_rect.center(), 15.0, bg);
                                draw_expand_icon(
                                    ui.painter(),
                                    exp_rect.center(),
                                    14.0,
                                    Color32::from_white_alpha((245.0 * hover_alpha).round() as u8),
                                );
                                if exp_resp.on_hover_text("Expand to full player").clicked() {
                                    overlay_clicked = true;
                                    if previewing_bg {
                                        mini_action = MiniPlayerAction::Expand;
                                    } else {
                                        self.page = Page::Watch;
                                        self.watch_scrolled_into_pip = false;
                                        self.scroll_watch_to_top = true;
                                    }
                                }
                                left_badge_x = exp_rect.right() + 6.0;
                            }

                            if self.bg_player_tab.is_some() || previewing_bg {
                                let tab_num = if previewing_bg {
                                    (1 - self.active_player_tab) + 1
                                } else {
                                    self.active_player_tab + 1
                                };
                                let chip_r = Rect::from_min_size(
                                    pos2(left_badge_x, vrect.top() + 13.0),
                                    vec2(44.0, 20.0),
                                );
                                ui.painter()
                                    .rect_filled(chip_r, 6.0, Color32::from_black_alpha(180));
                                ui.painter().text(
                                    chip_r.center(),
                                    Align2::CENTER_CENTER,
                                    format!("Tab {tab_num}"),
                                    theme::semibold(10.0),
                                    Color32::WHITE,
                                );
                            }

                            // Top-right Close (X) on hover
                            if hover_alpha > 0.02 {
                                let close_rect = Rect::from_min_size(
                                    pos2(vrect.right() - 38.0, vrect.top() + 8.0),
                                    Vec2::splat(30.0),
                                );
                                let close_resp = ui
                                    .interact(
                                        close_rect,
                                        ui.scope_id().with("mini_pip_hover_close"),
                                        Sense::click(),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand);
                                let bg = if close_resp.hovered() {
                                    Color32::from_black_alpha((215.0 * hover_alpha).round() as u8)
                                } else {
                                    pill_bg
                                };
                                ui.painter().circle_filled(close_rect.center(), 15.0, bg);
                                theme::draw_icon_centered(
                                    ui.painter(),
                                    close_rect.center(),
                                    icon::X,
                                    14.0,
                                    Color32::from_white_alpha((245.0 * hover_alpha).round() as u8),
                                );
                                if close_resp.on_hover_text("Close player").clicked() {
                                    overlay_clicked = true;
                                    if previewing_bg {
                                        mini_action = MiniPlayerAction::Close;
                                    } else if self.bg_player_tab.is_some() {
                                        self.close_player_tab(true);
                                    } else {
                                        self.backend.cursor.stop();
                                        if let Some(m) = &self.mpv {
                                            m.stop();
                                        }
                                        self.watch = None;
                                        self.watch_scrolled_into_pip = false;
                                        self.clear_last_watched();
                                    }
                                }
                            }

                            // Center Play/Pause (+ Next when queue has items)
                            let play_rect =
                                Rect::from_center_size(vrect.center(), Vec2::splat(42.0));
                            let play_resp = ui
                                .interact(
                                    play_rect,
                                    ui.scope_id().with("mini_pip_hover_play"),
                                    Sense::click(),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand);
                            let play_bg = if play_resp.hovered() {
                                Color32::from_black_alpha((215.0 * ctrl_alpha).round() as u8)
                            } else {
                                pill_bg
                            };
                            ui.painter().circle_filled(play_rect.center(), 21.0, play_bg);
                            let play_ico = if self.playback.paused {
                                icon::PLAY
                            } else {
                                icon::PAUSE
                            };
                            theme::draw_icon_centered(
                                ui.painter(),
                                play_rect.center(),
                                play_ico,
                                18.0,
                                icon_fg,
                            );
                            if play_resp
                                .on_hover_text(if self.playback.paused { "Play" } else { "Pause" })
                                .clicked()
                            {
                                overlay_clicked = true;
                                if let Some(m) = &self.mpv {
                                    m.toggle_pause();
                                }
                            }

                            if has_queue && hover_alpha > 0.02 {
                                let next_rect = Rect::from_center_size(
                                    pos2(play_rect.right() + 26.0, vrect.center().y),
                                    Vec2::splat(32.0),
                                );
                                let next_resp = ui
                                    .interact(
                                        next_rect,
                                        ui.scope_id().with("mini_pip_hover_next"),
                                        Sense::click(),
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand);
                                let bg = if next_resp.hovered() {
                                    Color32::from_black_alpha((215.0 * hover_alpha).round() as u8)
                                } else {
                                    pill_bg
                                };
                                ui.painter().circle_filled(next_rect.center(), 16.0, bg);
                                theme::draw_next_icon(
                                    ui.painter(),
                                    next_rect.center(),
                                    14.0,
                                    Color32::from_white_alpha((245.0 * hover_alpha).round() as u8),
                                );
                                if next_resp.on_hover_text("Play next in queue").clicked() {
                                    overlay_clicked = true;
                                    play_next_now = true;
                                }
                            }

                            // Bottom-left timestamp badge on hover
                            if hover_alpha > 0.02 {
                                let ts_text = if is_live {
                                    "LIVE".to_owned()
                                } else if dur > 0.5 {
                                    format!("{} / {}", format_duration(pos), format_duration(dur))
                                } else {
                                    format_duration(pos)
                                };
                                let galley = ui.painter().layout_no_wrap(
                                    ts_text,
                                    theme::medium(11.0),
                                    Color32::from_white_alpha((245.0 * hover_alpha).round() as u8),
                                );
                                let pad = vec2(6.0, 2.5);
                                let bsz = galley.size() + pad * 2.0;
                                let brect = Rect::from_min_size(
                                    pos2(vrect.left() + 8.0, vrect.bottom() - bsz.y - 8.0),
                                    bsz,
                                );
                                ui.painter().rect_filled(
                                    brect,
                                    4.0,
                                    if is_live {
                                        theme::YT_RED.gamma_multiply(hover_alpha)
                                    } else {
                                        Color32::from_black_alpha((175.0 * hover_alpha).round() as u8)
                                    },
                                );
                                ui.painter().galley(
                                    brect.min + pad,
                                    galley,
                                    Color32::from_white_alpha((245.0 * hover_alpha).round() as u8),
                                );
                            }
                        } else if self.bg_player_tab.is_some() || previewing_bg {
                            let tab_num = if previewing_bg {
                                (1 - self.active_player_tab) + 1
                            } else {
                                self.active_player_tab + 1
                            };
                            let chip_r = Rect::from_min_size(
                                pos2(vrect.left() + 8.0, vrect.top() + 8.0),
                                vec2(44.0, 20.0),
                            );
                            ui.painter()
                                .rect_filled(chip_r, 6.0, Color32::from_black_alpha(180));
                            ui.painter().text(
                                chip_r.center(),
                                Align2::CENTER_CENTER,
                                format!("Tab {tab_num}"),
                                theme::semibold(10.0),
                                Color32::WHITE,
                            );
                        }

                        if vresp.clicked() && !overlay_clicked {
                            if previewing_bg {
                                mini_action = MiniPlayerAction::Expand;
                            } else {
                                self.page = Page::Watch;
                                self.watch_scrolled_into_pip = false;
                                self.scroll_watch_to_top = true;
                            }
                        }

                        // Interactive seek/progress bar directly under the PiP video
                        if !is_live && dur > 0.5 {
                            let (bar_rect, bar_resp) = ui.allocate_exact_size(
                                Vec2::new(pip_w, 12.0),
                                Sense::click_and_drag(),
                            );
                            let hot = bar_resp.hovered() || bar_resp.dragged();
                            let bar_h = if hot { 5.0 } else { 3.5 };
                            let track = Rect::from_min_max(
                                pos2(bar_rect.left(), bar_rect.center().y - bar_h * 0.5),
                                pos2(bar_rect.right(), bar_rect.center().y + bar_h * 0.5),
                            );
                            ui.painter()
                                .rect_filled(track, 0.0, Color32::from_white_alpha(45));
                            let cache_frac =
                                ((self.playback.time_pos + self.playback.cache_duration) / dur)
                                    .clamp(0.0, 1.0) as f32;
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    track.min,
                                    Vec2::new(track.width() * cache_frac, bar_h),
                                ),
                                0.0,
                                Color32::from_white_alpha(80),
                            );
                            if bar_resp.dragged() || bar_resp.clicked() {
                                if let Some(ptr) = bar_resp.interact_pointer_pos() {
                                    let f = ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0)
                                        as f64;
                                    pos = f * dur;
                                    self.seeking_pos = Some(pos);
                                }
                            }
                            if bar_resp.drag_stopped() || bar_resp.clicked() {
                                if let Some(target) = self.seeking_pos.take() {
                                    if let Some(mpv) = &self.mpv {
                                        mpv.seek_abs(target);
                                    }
                                }
                            }
                            let frac = (pos / dur).clamp(0.0, 1.0) as f32;
                            let fill_x = track.left() + track.width() * frac;
                            ui.painter().rect_filled(
                                Rect::from_min_max(track.min, pos2(fill_x, track.max.y)),
                                0.0,
                                theme::YT_RED,
                            );
                            if hot {
                                ui.painter().circle_filled(
                                    pos2(fill_x, track.center().y),
                                    5.5,
                                    theme::YT_RED,
                                );
                                if let Some(hpos) = bar_resp.hover_pos() {
                                    let hf = ((hpos.x - track.left()) / track.width())
                                        .clamp(0.0, 1.0)
                                        as f64;
                                    bar_resp.on_hover_text(format_duration(hf * dur));
                                }
                            }
                        } else if is_live {
                            let (bar_rect, _) =
                                ui.allocate_exact_size(Vec2::new(pip_w, 4.0), Sense::hover());
                            ui.painter().rect_filled(bar_rect, 0.0, theme::YT_RED);
                        }

                        // Compact bottom metadata bar: Title + Queue/Channel info + Windows PiP + Queue toggle
                        Frame::new()
                            .inner_margin(Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                let inner_w = pip_w - 24.0;
                                let btns_w = if has_queue { 72.0 } else { 36.0 };
                                let text_w = (inner_w - btns_w - 8.0).max(120.0);
                                let sub_line = if has_queue {
                                    format!("Queue · 1 / {}", self.queue.len() + 1)
                                } else if is_live {
                                    format!("LIVE · {channel}")
                                } else if dur > 0.5 {
                                    format!(
                                        "{} / {} · {channel}",
                                        format_duration(pos),
                                        format_duration(dur)
                                    )
                                } else {
                                    channel.clone()
                                };

                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(text_w, 34.0),
                                        Layout::top_down(Align::LEFT),
                                        |ui| {
                                            let tclip = ui.clip_rect().intersect(ui.max_rect());
                                            ui.set_clip_rect(tclip);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&title)
                                                        .font(theme::semibold(12.5))
                                                        .color(theme::TEXT),
                                                )
                                                .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(sub_line)
                                                        .font(theme::regular(11.5))
                                                        .color(theme::TEXT_DIM),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        if has_queue {
                                            let chevron = if self.pip_queue_open {
                                                icon::CHEVRON_DOWN
                                            } else {
                                                icon::CHEVRON_UP
                                            };
                                            let tip = if self.pip_queue_open {
                                                "Hide queue"
                                            } else {
                                                "Show queue"
                                            };
                                            if icon_circle_button(ui, chevron, 16.0, tip) {
                                                toggle_queue_open = true;
                                            }
                                        }
                                        let (wprect, wpresp) = ui.allocate_exact_size(
                                            Vec2::splat(34.0),
                                            Sense::click(),
                                        );
                                        paint_hover_circle(ui, wprect, &wpresp, 1.0);
                                        draw_win_pip_icon(
                                            ui.painter(),
                                            wprect.center(),
                                            15.0,
                                            Color32::WHITE,
                                        );
                                        if wpresp
                                            .on_hover_text("Windows PiP (float over Windows)")
                                            .clicked()
                                        {
                                            mini_action = MiniPlayerAction::PopOutWinPip;
                                        }
                                    });
                                });
                            });

                        // Expandable Queue panel directly inside the mini-player
                        if has_queue && self.pip_queue_open {
                            ui.separator();
                            Frame::new()
                                .inner_margin(Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "Up next ({})",
                                                self.queue.len()
                                            ))
                                            .font(theme::semibold(11.5))
                                            .color(theme::TEXT_DIM),
                                        );
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            RichText::new("Clear")
                                                                .font(theme::medium(11.0))
                                                                .color(theme::TEXT_DIM),
                                                        )
                                                        .fill(theme::CHIP)
                                                        .corner_radius(10),
                                                    )
                                                    .clicked()
                                                {
                                                    clear_queue_now = true;
                                                }
                                                if self.signed_in
                                                    && ui
                                                        .add(
                                                            egui::Button::new(
                                                                RichText::new(format!(
                                                                    "{}  Save",
                                                                    icon::LIST_PLUS
                                                                ))
                                                                .font(theme::medium(11.0))
                                                                .color(theme::TEXT),
                                                            )
                                                            .fill(theme::CHIP)
                                                            .corner_radius(10),
                                                        )
                                                        .clicked()
                                                {
                                                    self.right_tab = RightTab::Queue;
                                                    self.save_queue_open = true;
                                                    if self.save_queue_title.is_empty() {
                                                        self.save_queue_title =
                                                            "Saved Queue".to_owned();
                                                    }
                                                    if previewing_bg {
                                                        mini_action = MiniPlayerAction::Expand;
                                                    } else {
                                                        self.page = Page::Watch;
                                                        self.watch_scrolled_into_pip = false;
                                                        self.scroll_watch_to_top = true;
                                                    }
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(4.0);
                                    ScrollArea::vertical()
                                        .id_salt("mini_pip_queue_scroll")
                                        .min_scrolled_height(queue_scroll_h)
                                        .max_height(queue_scroll_h)
                                        .show(ui, |ui| {
                                            // Active playing row
                                            let row_w = ui.available_width();
                                            let (act_rect, _) = ui.allocate_exact_size(
                                                Vec2::new(row_w, 46.0),
                                                Sense::hover(),
                                            );
                                            ui.painter().rect_filled(
                                                act_rect,
                                                6.0,
                                                Color32::from_rgba_unmultiplied(64, 52, 42, 160),
                                            );
                                            let mut aui = ui.new_child(
                                                egui::UiBuilder::new().max_rect(act_rect.shrink(4.0)),
                                            );
                                            aui.horizontal_centered(|ui| {
                                                ui.spacing_mut().item_spacing.x = 6.0;
                                                let (icon_slot, _) = ui.allocate_exact_size(
                                                    Vec2::new(14.0, 38.0),
                                                    Sense::hover(),
                                                );
                                                theme::draw_icon_centered(
                                                    ui.painter(),
                                                    icon_slot.center(),
                                                    icon::PLAY,
                                                    10.0,
                                                    theme::TEXT,
                                                );
                                                draw_thumbnail(
                                                    ui,
                                                    &cur_thumb,
                                                    &if is_live {
                                                        "LIVE".to_owned()
                                                    } else if dur > 0.5 {
                                                        format_duration(dur)
                                                    } else {
                                                        String::new()
                                                    },
                                                    Vec2::new(68.0, 38.0),
                                                    5.0,
                                                    None,
                                                );
                                                ui.allocate_ui_with_layout(
                                                    Vec2::new((row_w - 104.0).max(80.0), 34.0),
                                                    Layout::top_down(Align::LEFT),
                                                    |ui| {
                                                        ui.spacing_mut().item_spacing.y = 2.0;
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(&title)
                                                                    .font(theme::semibold(11.5))
                                                                    .color(Color32::WHITE),
                                                            )
                                                            .truncate(),
                                                        );
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(&channel)
                                                                    .font(theme::regular(10.5))
                                                                    .color(theme::TEXT_DIM),
                                                            )
                                                            .truncate(),
                                                        );
                                                    },
                                                );
                                            });
                                            ui.add_space(3.0);

                                            for (qi, qitem) in self.queue.iter().enumerate() {
                                                if qitem.id == cur_id {
                                                    continue;
                                                }
                                                let (qrect, qresp) = ui.allocate_exact_size(
                                                    Vec2::new(row_w, 46.0),
                                                    Sense::click(),
                                                );
                                                let hovered = qresp.hovered()
                                                    || ui.rect_contains_pointer(qrect);
                                                if hovered {
                                                    ui.ctx()
                                                        .set_cursor_icon(CursorIcon::PointingHand);
                                                    ui.painter().rect_filled(
                                                        qrect,
                                                        6.0,
                                                        Color32::from_white_alpha(16),
                                                    );
                                                }
                                                let mut qui = ui.new_child(
                                                    egui::UiBuilder::new()
                                                        .max_rect(qrect.shrink(4.0)),
                                                );
                                                let mut rm_clicked = false;
                                                qui.horizontal_centered(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    ui.add_space(14.0);
                                                    draw_thumbnail(
                                                        ui,
                                                        &qitem.thumbnail_url,
                                                        &qitem.duration_text,
                                                        Vec2::new(68.0, 38.0),
                                                        5.0,
                                                        qitem.watched_percent,
                                                    );
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new((row_w - 134.0).max(70.0), 34.0),
                                                        Layout::top_down(Align::LEFT),
                                                        |ui| {
                                                            ui.spacing_mut().item_spacing.y = 2.0;
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(&qitem.title)
                                                                        .font(theme::medium(11.5))
                                                                        .color(theme::TEXT),
                                                                )
                                                                .truncate(),
                                                            );
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(&qitem.channel)
                                                                        .font(theme::regular(10.5))
                                                                        .color(theme::TEXT_DIM),
                                                                )
                                                                .truncate(),
                                                            );
                                                        },
                                                    );
                                                    if hovered {
                                                        ui.with_layout(
                                                            Layout::right_to_left(Align::Center),
                                                            |ui| {
                                                                if icon_circle_button(
                                                                    ui,
                                                                    icon::X,
                                                                    12.0,
                                                                    "Remove from queue",
                                                                ) {
                                                                    rm_clicked = true;
                                                                }
                                                            },
                                                        );
                                                    }
                                                });
                                                if rm_clicked {
                                                    remove_queue_idx = Some(qi);
                                                } else if qresp.clicked() {
                                                    play_queue_idx = Some(qi);
                                                }
                                                ui.add_space(2.0);
                                            }
                                        });
                                });
                        }
                    });
            });

        let snap_after_queue_close =
            |pre_pos: &mut Option<egui::Pos2>,
             pip_pos: &mut Option<egui::Pos2>,
             snap_target: &mut Option<egui::Pos2>,
             r: egui::Rect,
             drawer_total_h: f32| {
                let margin = 18.0;
                let collapsed_h = (r.height() - drawer_total_h).clamp(240.0, 270.0);
                let bottom_y = (screen.bottom() - collapsed_h - margin).max(screen.top() + 70.0);
                let target_y = if let Some(prev) = pre_pos.take() {
                    if prev.y >= screen.center().y - collapsed_h * 0.5 {
                        bottom_y
                    } else {
                        prev.y.clamp(screen.top() + 70.0, bottom_y)
                    }
                } else if r.top() <= screen.top() + 95.0 && r.bottom() <= screen.bottom() - 120.0 {
                    // User explicitly parked the mini-player near the top edge: keep it anchored top.
                    r.top()
                } else {
                    bottom_y
                };
                if (r.top() - target_y).abs() > 1.0 {
                    *pip_pos = Some(r.min);
                    *snap_target = Some(pos2(r.left(), target_y));
                }
            };

        let drawer_total_h = 46.0 + queue_scroll_h;
        if toggle_queue_open {
            self.pip_queue_open = !self.pip_queue_open;
            let r = area_resp.response.rect;
            let margin = 18.0;
            if self.pip_queue_open {
                let est_h = r.height() + drawer_total_h;
                if r.center().y >= screen.center().y || r.top() + est_h + margin > screen.bottom() {
                    self.pip_pre_queue_pos = Some(r.min);
                    let target_y = (screen.bottom() - est_h - margin).max(screen.top() + 70.0);
                    self.pip_pos = Some(r.min);
                    self.pip_snap_target = Some(pos2(r.left(), target_y));
                } else {
                    self.pip_pre_queue_pos = None;
                }
            } else {
                snap_after_queue_close(
                    &mut self.pip_pre_queue_pos,
                    &mut self.pip_pos,
                    &mut self.pip_snap_target,
                    r,
                    drawer_total_h,
                );
            }
        } else if self.pip_queue_open
            && self.pip_snap_target.is_none()
            && !area_resp.response.dragged()
            && area_resp.response.rect.bottom() + 10.0 > screen.bottom()
        {
            let r = area_resp.response.rect;
            let margin = 18.0;
            let target_y = (screen.bottom() - r.height() - margin).max(screen.top() + 70.0);
            if (r.top() - target_y).abs() > 2.0 {
                self.pip_pos = Some(r.min);
                self.pip_snap_target = Some(pos2(r.left(), target_y));
            }
        }
        if clear_queue_now {
            let was_open = self.pip_queue_open;
            self.queue.clear();
            self.active_mix_id = None;
            self.active_mix_title.clear();
            self.active_mix_continuation = None;
            self.active_mix_loading = false;
            self.pip_queue_open = false;
            self.persist_queue();
            if was_open {
                snap_after_queue_close(
                    &mut self.pip_pre_queue_pos,
                    &mut self.pip_pos,
                    &mut self.pip_snap_target,
                    area_resp.response.rect,
                    drawer_total_h,
                );
            }
        } else if let Some(ri) = remove_queue_idx {
            if ri < self.queue.len() {
                let was_open = self.pip_queue_open;
                self.queue.remove(ri);
                if self.queue.is_empty() {
                    self.pip_queue_open = false;
                    if was_open {
                        snap_after_queue_close(
                            &mut self.pip_pre_queue_pos,
                            &mut self.pip_pos,
                            &mut self.pip_snap_target,
                            area_resp.response.rect,
                            drawer_total_h,
                        );
                    }
                }
                self.persist_queue();
            }
        } else if let Some(pi) = play_queue_idx {
            if pi < self.queue.len() {
                let next = self.queue.remove(pi);
                self.persist_queue();
                self.maybe_refill_mix_queue();
                if previewing_bg {
                    self.switch_player_tab();
                }
                self.open_video(next);
            }
        } else if play_next_now && !self.queue.is_empty() {
            if previewing_bg {
                self.switch_player_tab();
            }
            self.play_next_video();
        }

        if mini_action == MiniPlayerAction::PopOutWinPip && !previewing_bg {
            self.enter_win_pip(ctx);
            mini_action = MiniPlayerAction::None;
        }

        // Snap PiP gently to nearest screen corner when released near edges
        if area_resp.response.dragged() {
            self.pip_snap_target = None;
            self.pip_pre_queue_pos = None;
            self.pip_pos = Some(area_resp.response.rect.min);
        } else if area_resp.response.drag_stopped() {
            let r = area_resp.response.rect;
            self.pip_pos = Some(r.min);
            let margin = 18.0;
            let snap_x = if r.center().x < screen.center().x {
                screen.left() + margin
            } else {
                (screen.right() - r.width() - margin).max(screen.left() + margin)
            };
            let snap_y = if r.center().y < screen.center().y {
                screen.top() + 70.0
            } else {
                (screen.bottom() - r.height() - margin).max(screen.top() + 70.0)
            };
            if (r.left() - snap_x).abs() < 90.0 || (r.top() - snap_y).abs() < 90.0 {
                self.pip_snap_target = Some(pos2(snap_x, snap_y));
            }
        }
        mini_action
    }

    pub(super) fn draw_win_pip_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(w) = self.watch.as_ref() else {
            return;
        };
        let dur = self.playback.duration.max(w.duration_secs).max(0.0);
        let mut pos = self.seeking_pos.unwrap_or(self.playback.time_pos).max(0.0);
        let is_live = w.is_live;

        let full_rect = ui.available_rect_before_wrap();
        let (vrect, vresp) = ui.allocate_exact_size(full_rect.size(), Sense::click_and_drag());
        ui.painter().rect_filled(vrect, 0.0, Color32::BLACK);
        if let Some(mpv) = &self.mpv {
            ui.painter().add(mpv.paint_callback(vrect));
        }

        // Subtle 1px dark border so the borderless floating window stands out cleanly over other apps
        ui.painter().rect_stroke(
            vrect.shrink(0.5),
            0.0,
            Stroke::new(1.0, Color32::from_white_alpha(38)),
            egui::StrokeKind::Inside,
        );

        let hovered = ui.rect_contains_pointer(vrect) || self.player_menu == PlayerMenu::SleepTimer;
        let overlay_alpha = ui.ctx().animate_bool_with_time(
            egui::Id::unique("win_pip_overlay_fade"),
            hovered || self.seeking_pos.is_some(),
            0.14,
        );

        let mut exit_pip = false;
        let mut close_video = false;
        let mut any_control_hit = false;

        if overlay_alpha > 0.01 {
            let white = Color32::from_white_alpha((245.0 * overlay_alpha).round() as u8);
            let bg_pill = Color32::from_black_alpha((165.0 * overlay_alpha).round() as u8);

            // Top gradient & top-right minimal action pills (Exit PiP / Sleep timer / Close)
            let top_grad = Rect::from_min_max(
                vrect.left_top(),
                pos2(vrect.right(), (vrect.top() + 52.0).min(vrect.bottom())),
            );
            paint_vertical_gradient(
                ui.painter(),
                top_grad,
                Color32::from_black_alpha((170.0 * overlay_alpha).round() as u8),
                Color32::TRANSPARENT,
            );

            let top_bar = Rect::from_min_max(
                pos2(vrect.left() + 8.0, vrect.top() + 8.0),
                pos2(vrect.right() - 8.0, vrect.top() + 38.0),
            );
            let mut top_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(top_bar)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            top_ui.spacing_mut().item_spacing.x = 6.0;

            let (x_rect, x_resp) = top_ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
            top_ui.painter().circle_filled(x_rect.center(), 13.0, bg_pill);
            theme::draw_icon_centered(top_ui.painter(), x_rect.center(), icon::X, 13.0, white);
            if x_resp.hovered() {
                top_ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if x_resp.on_hover_text("Close player").clicked() {
                any_control_hit = true;
                close_video = true;
            }

            let (back_rect, back_resp) =
                top_ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
            top_ui.painter().circle_filled(back_rect.center(), 13.0, bg_pill);
            draw_expand_icon(top_ui.painter(), back_rect.center(), 13.5, white);
            if back_resp.hovered() {
                top_ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if back_resp
                .on_hover_text("Back to full window (Esc)")
                .clicked()
            {
                any_control_hit = true;
                exit_pip = true;
            }

            if let Some(rem) = self.sleep_timer_remaining_label() {
                let txt = format!("Sleep: {rem}");
                let (srect, sresp) =
                    top_ui.allocate_exact_size(Vec2::new(88.0, 24.0), Sense::click());
                top_ui.painter().rect_filled(
                    srect,
                    12.0,
                    Color32::from_rgba_unmultiplied(
                        24,
                        28,
                        36,
                        (200.0 * overlay_alpha).round() as u8,
                    ),
                );
                top_ui.painter().text(
                    srect.center(),
                    Align2::CENTER_CENTER,
                    txt,
                    theme::medium(11.0),
                    theme::YT_BLUE.gamma_multiply(overlay_alpha),
                );
                if sresp
                    .on_hover_text("Click to turn off sleep timer")
                    .clicked()
                {
                    any_control_hit = true;
                    self.sleep_timer = SleepTimer::Off;
                    self.toast("Sleep timer turned off");
                }
            }

            // Bottom gradient & minimal playback bar (seek bar + Play/Pause + Mute + Time + Resize grip)
            let bot_h = 54.0_f32.min(vrect.height() * 0.45);
            let bot_grad = Rect::from_min_max(
                pos2(vrect.left(), vrect.bottom() - bot_h - 14.0),
                vrect.right_bottom(),
            );
            paint_vertical_gradient(
                ui.painter(),
                bot_grad,
                Color32::TRANSPARENT,
                Color32::from_black_alpha((205.0 * overlay_alpha).round() as u8),
            );

            // Resize grip in bottom-right corner
            let grip_r = Rect::from_min_max(
                pos2(vrect.right() - 18.0, vrect.bottom() - 18.0),
                vrect.right_bottom(),
            );
            let grip_resp = ui.interact(
                grip_r,
                egui::Id::unique("win_pip_resize_grip"),
                Sense::drag(),
            );
            if grip_resp.hovered() || grip_resp.dragged() {
                ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
            }
            for off in [5.0_f32, 9.5] {
                ui.painter().line_segment(
                    [
                        pos2(vrect.right() - off - 2.0, vrect.bottom() - 3.0),
                        pos2(vrect.right() - 3.0, vrect.bottom() - off - 2.0),
                    ],
                    Stroke::new(1.3, Color32::from_white_alpha((150.0 * overlay_alpha).round() as u8)),
                );
            }
            if grip_resp.drag_started() {
                any_control_hit = true;
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(
                    egui::ResizeDirection::SouthEast,
                ));
            }

            // Seek bar
            if !is_live && dur > 0.5 {
                let bar_rect = Rect::from_min_max(
                    pos2(vrect.left() + 10.0, vrect.bottom() - 44.0),
                    pos2(vrect.right() - 10.0, vrect.bottom() - 30.0),
                );
                let bar_resp =
                    ui.interact(bar_rect, egui::Id::unique("win_pip_seek_bar"), Sense::click_and_drag());
                let hot = bar_resp.hovered() || bar_resp.dragged();
                let track_h = if hot { 4.5 } else { 3.0 };
                let track = Rect::from_min_max(
                    pos2(bar_rect.left(), bar_rect.center().y - track_h * 0.5),
                    pos2(bar_rect.right(), bar_rect.center().y + track_h * 0.5),
                );
                ui.painter().rect_filled(
                    track,
                    1.5,
                    Color32::from_white_alpha((55.0 * overlay_alpha).round() as u8),
                );
                if self.playback.cache_duration > 0.0 {
                    let cache_frac = ((self.playback.time_pos.max(0.0)
                        + self.playback.cache_duration)
                        / dur)
                        .clamp(0.0, 1.0) as f32;
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            track.min,
                            Vec2::new(track.width() * cache_frac, track_h),
                        ),
                        1.5,
                        Color32::from_white_alpha((145.0 * overlay_alpha).round() as u8),
                    );
                }
                if bar_resp.dragged() || bar_resp.clicked() {
                    any_control_hit = true;
                    if let Some(ptr) = bar_resp.interact_pointer_pos() {
                        let f = ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
                        pos = f * dur;
                        self.seeking_pos = Some(pos);
                    }
                }
                if bar_resp.drag_stopped() || bar_resp.clicked() {
                    any_control_hit = true;
                    if let Some(target) = self.seeking_pos.take() {
                        if let Some(mpv) = &self.mpv {
                            mpv.seek_abs(target);
                        }
                    }
                }
                let frac = (pos / dur).clamp(0.0, 1.0) as f32;
                let fill_x = track.left() + track.width() * frac;
                ui.painter().rect_filled(
                    Rect::from_min_max(track.min, pos2(fill_x, track.max.y)),
                    1.5,
                    theme::YT_RED.gamma_multiply(overlay_alpha),
                );
                if hot {
                    ui.painter().circle_filled(
                        pos2(fill_x, track.center().y),
                        5.5,
                        theme::YT_RED.gamma_multiply(overlay_alpha),
                    );
                }
            }

            // Minimal bottom controls row
            let ctrl_rect = Rect::from_min_max(
                pos2(vrect.left() + 8.0, vrect.bottom() - 32.0),
                pos2(vrect.right() - 22.0, vrect.bottom() - 4.0),
            );
            let mut cui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(ctrl_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            cui.spacing_mut().item_spacing.x = 4.0;
            let (prect, presp) = cui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
            paint_hover_circle(&cui, prect, &presp, overlay_alpha);
            theme::draw_icon_centered(
                cui.painter(),
                prect.center(),
                if self.playback.paused {
                    icon::PLAY
                } else {
                    icon::PAUSE
                },
                14.0,
                white,
            );
            if presp.clicked() {
                any_control_hit = true;
                self.toggle_play_pause();
            }

            let (mrect, mresp) = cui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
            paint_hover_circle(&cui, mrect, &mresp, overlay_alpha);
            theme::draw_icon_centered(
                cui.painter(),
                mrect.center(),
                if self.muted || self.volume <= 0.1 {
                    icon::VOLUME_X
                } else if self.volume < 50.0 {
                    icon::VOLUME_1
                } else {
                    icon::VOLUME_2
                },
                14.0,
                white,
            );
            if mresp.clicked() {
                any_control_hit = true;
                self.muted = !self.muted;
                if let Some(m) = &self.mpv {
                    m.set_mute(self.muted);
                }
            }

            let time_txt = if is_live {
                "LIVE".to_owned()
            } else if dur > 0.5 {
                format!("{} / {}", format_duration(pos), format_duration(dur))
            } else {
                String::new()
            };
            if !time_txt.is_empty() {
                cui.label(
                    RichText::new(time_txt)
                        .font(theme::medium(11.5))
                        .color(if is_live {
                            theme::YT_RED.gamma_multiply(overlay_alpha)
                        } else {
                            white
                        }),
                );
            }
        }

        // Video surface interactions: drag to move window, click to play/pause, double-click to return
        if !any_control_hit {
            if vresp.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            } else if vresp.double_clicked() {
                exit_pip = true;
            } else if vresp.clicked() {
                self.toggle_play_pause();
            }
        }
        if vresp.hovered() {
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 1.0 {
                self.volume = (self.volume + f64::from(scroll) * 0.12).clamp(0.0, 100.0);
                if let Some(m) = &self.mpv {
                    m.set_volume(self.volume);
                }
            }
        }

        if close_video {
            self.exit_win_pip(ctx);
            if self.bg_player_tab.is_some() {
                self.close_player_tab(true);
            } else {
                self.backend.cursor.stop();
                if let Some(m) = &self.mpv {
                    m.stop();
                }
                self.watch = None;
                self.page = self.win_pip_saved_page;
                self.clear_last_watched();
            }
        } else if exit_pip {
            self.exit_win_pip(ctx);
        }
    }
}

#[derive(PartialEq, Eq)]
enum MiniPlayerAction {
    None,
    Expand,
    Close,
    PopOutWinPip,
}

fn overlay_icon_btn(
    ui: &mut egui::Ui,
    ico: char,
    size: f32,
    color: Color32,
    tooltip: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    let alpha = color.a() as f32 / 255.0;
    paint_hover_circle(ui, rect, &resp, alpha);
    theme::draw_icon_centered(ui.painter(), rect.center(), ico, size, color);
    resp.on_hover_text(tooltip).clicked()
}

fn paint_hover_circle(ui: &egui::Ui, rect: Rect, resp: &egui::Response, alpha: f32) {
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if hover_t > 0.01 {
        ui.painter().circle_filled(
            rect.center(),
            16.0,
            Color32::from_white_alpha((30.0 * hover_t * alpha).round() as u8),
        );
    }
}

fn paint_vertical_gradient_rounded(
    painter: &egui::Painter,
    rect: Rect,
    top_col: Color32,
    bottom_col: Color32,
    bottom_radius: f32,
) {
    let r = bottom_radius.min(rect.width() * 0.5).min(rect.height());
    if r <= 0.5 {
        paint_vertical_gradient(painter, rect, top_col, bottom_col);
        return;
    }
    let split_y = rect.bottom() - r;
    let frac = ((split_y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
    let lerp_col = |t: f32| -> Color32 {
        Color32::from_rgba_premultiplied(
            ((top_col.r() as f32) + ((bottom_col.r() as f32) - (top_col.r() as f32)) * t).round()
                as u8,
            ((top_col.g() as f32) + ((bottom_col.g() as f32) - (top_col.g() as f32)) * t).round()
                as u8,
            ((top_col.b() as f32) + ((bottom_col.b() as f32) - (top_col.b() as f32)) * t).round()
                as u8,
            ((top_col.a() as f32) + ((bottom_col.a() as f32) - (top_col.a() as f32)) * t).round()
                as u8,
        )
    };
    let mid_col = lerp_col(frac);
    let top_band = Rect::from_min_max(rect.min, pos2(rect.right(), split_y));
    paint_vertical_gradient(painter, top_band, top_col, mid_col);

    // Bottom rounded band using horizontal strips so alpha gradient & rounded corners both match cleanly
    let strips = 8usize;
    for s in 0..strips {
        let t0 = s as f32 / strips as f32;
        let t1 = (s + 1) as f32 / strips as f32;
        let y0 = split_y + r * t0;
        let y1 = split_y + r * t1;
        let dy = ((y0 + y1) * 0.5) - split_y;
        let x_span = (r * r - dy * dy).max(0.0).sqrt();
        let inset = r - x_span;
        let band = Rect::from_min_max(pos2(rect.left() + inset, y0), pos2(rect.right() - inset, y1));
        let col0 = lerp_col(frac + (1.0 - frac) * t0);
        let col1 = lerp_col(frac + (1.0 - frac) * t1);
        paint_vertical_gradient(painter, band, col0, col1);
    }
}

fn paint_vertical_gradient(
    painter: &egui::Painter,
    rect: Rect,
    top_col: Color32,
    bottom_col: Color32,
) {
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top_col);
    mesh.colored_vertex(rect.right_top(), top_col);
    mesh.colored_vertex(rect.right_bottom(), bottom_col);
    mesh.colored_vertex(rect.left_bottom(), bottom_col);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

fn build_up_next_chips(
    w: &WatchData,
) -> Vec<(crate::model::UpNextFilterMode, String, Option<String>)> {
    use crate::model::UpNextFilterMode;

    let find_tok = |target: UpNextFilterMode| -> Option<(String, Option<String>)> {
        w.related_chips
            .iter()
            .find(|c| UpNextFilterMode::classify_chip(&c.title, &w.channel) == Some(target))
            .map(|c| (c.title.clone(), c.continuation.clone()))
    };

    let mut out = Vec::new();
    let (all_title, all_tok) = find_tok(UpNextFilterMode::All)
        .unwrap_or_else(|| ("All".to_owned(), None));
    out.push((UpNextFilterMode::All, all_title, all_tok));

    if !w.channel.trim().is_empty() && w.channel != "Unknown" {
        let (ch_title, ch_tok) = find_tok(UpNextFilterMode::FromChannel)
            .unwrap_or_else(|| (format!("From {}", w.channel), None));
        out.push((UpNextFilterMode::FromChannel, ch_title, ch_tok));
    }

    let (rel_title, rel_tok) = find_tok(UpNextFilterMode::Related)
        .unwrap_or_else(|| ("Related".to_owned(), None));
    out.push((UpNextFilterMode::Related, rel_title, rel_tok));

    let (fy_title, fy_tok) = find_tok(UpNextFilterMode::ForYou)
        .unwrap_or_else(|| ("For you".to_owned(), None));
    out.push((UpNextFilterMode::ForYou, fy_title, fy_tok));

    for c in &w.related_chips {
        if UpNextFilterMode::classify_chip(&c.title, &w.channel).is_none()
            && c.continuation.is_some()
        {
            out.push((
                UpNextFilterMode::Related,
                c.title.clone(),
                c.continuation.clone(),
            ));
        }
    }
    out
}

fn draw_shorts_transition_card(
    ui: &mut egui::Ui,
    rect: Rect,
    radius: f32,
    out: &super::ShortsOutgoing,
) {
    ui.painter().rect_filled(rect, radius, Color32::BLACK);
    if !out.thumbnail_url.is_empty() {
        theme::paint_image_cover_rounded(ui, &out.thumbnail_url, rect, radius.round() as u8);
    }
    let scrim_h = 148.0_f32.min(rect.height() * 0.45);
    let scrim = Rect::from_min_max(pos2(rect.left(), rect.bottom() - scrim_h), rect.max);
    paint_vertical_gradient_rounded(
        ui.painter(),
        scrim,
        Color32::TRANSPARENT,
        Color32::from_black_alpha(180),
        radius,
    );
    let text_painter = ui.painter().with_clip_rect(rect.shrink2(vec2(14.0, 8.0)));
    if !out.channel.is_empty() {
        text_painter.text(
            pos2(rect.left() + 14.0, (rect.bottom() - 44.0).max(rect.top() + 16.0)),
            Align2::LEFT_BOTTOM,
            &out.channel,
            theme::medium(12.5),
            Color32::from_white_alpha(210),
        );
    }
    if !out.title.is_empty() {
        text_painter.text(
            pos2(rect.left() + 14.0, (rect.bottom() - 18.0).max(rect.top() + 34.0)),
            Align2::LEFT_BOTTOM,
            &out.title,
            theme::semibold(13.5),
            Color32::WHITE,
        );
    }
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, Color32::from_white_alpha(22)),
        egui::StrokeKind::Inside,
    );
}

fn draw_shorts_bottom_shimmer(ui: &mut egui::Ui, ctx: &egui::Context, rect: Rect) {
    let t = (ctx.input(|i| i.time) as f32 * 1.8) % 1.0;
    let bar_y = rect.bottom() - 3.0;
    let bar_w = rect.width() * 0.36;
    let x0 = rect.left() - bar_w + (rect.width() + bar_w) * t;
    let bar = Rect::from_min_max(
        pos2(x0.max(rect.left()), bar_y),
        pos2((x0 + bar_w).min(rect.right()), rect.bottom()),
    );
    if bar.width() > 1.0 {
        ui.painter().rect_filled(bar, 1.5, theme::YT_RED);
    }
}

fn draw_buffering_overlay(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    rect: Rect,
    radius: f32,
    label: &str,
) {
    ui.painter()
        .rect_filled(rect, radius, Color32::from_black_alpha(110));

    let c = rect.center();
    let pill = Rect::from_center_size(c, vec2(190.0, 74.0));
    ui.painter()
        .rect_filled(pill, 14.0, Color32::from_black_alpha(205));
    ui.painter().rect_stroke(
        pill,
        14.0,
        Stroke::new(1.0, Color32::from_white_alpha(28)),
        egui::StrokeKind::Inside,
    );

    let t = ctx.input(|i| i.time) as f32;
    let spin_center = pos2(c.x, c.y - 11.0);
    let r = 14.0;
    let start_angle = t * 5.2;
    let sweep = std::f32::consts::PI * 1.35;

    ui.painter()
        .circle_stroke(spin_center, r, Stroke::new(3.0, Color32::from_white_alpha(35)));

    let steps = 20;
    let pts: Vec<egui::Pos2> = (0..=steps)
        .map(|k| {
            let a = start_angle + sweep * (k as f32 / steps as f32);
            pos2(spin_center.x + r * a.cos(), spin_center.y + r * a.sin())
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(pts, Stroke::new(3.0, Color32::WHITE)));

    ui.painter().text(
        pos2(c.x, c.y + 18.0),
        Align2::CENTER_CENTER,
        label,
        theme::medium(12.5),
        Color32::from_rgb(230, 230, 230),
    );
}

fn popover_close_btn(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if hover_t > 0.01 {
        ui.painter().circle_filled(
            rect.center(),
            12.0,
            Color32::from_white_alpha((28.0 * hover_t) as u8),
        );
    }
    theme::draw_icon_centered(
        ui.painter(),
        rect.center(),
        icon::X,
        13.0,
        if resp.hovered() {
            Color32::WHITE
        } else {
            theme::TEXT_DIM
        },
    );
    resp.clicked()
}

fn popover_option_row(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let fill = if selected {
        theme::YT_BLUE.gamma_multiply(0.16)
    } else if hover_t > 0.01 {
        Color32::from_white_alpha((20.0 * hover_t) as u8)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 7.0, fill);
    }
    if selected {
        theme::draw_icon_centered(
            ui.painter(),
            pos2(rect.left() + 14.0, rect.center().y),
            icon::CHECK,
            13.0,
            theme::YT_BLUE,
        );
    }
    let text_col = if selected {
        Color32::WHITE
    } else {
        theme::TEXT
    };
    ui.painter().text(
        pos2(rect.left() + 28.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        if selected {
            theme::semibold(12.5)
        } else {
            theme::medium(12.5)
        },
        text_col,
    );
    resp.clicked()
}

fn popover_toggle_row(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if hover_t > 0.01 {
        ui.painter().rect_filled(
            rect,
            7.0,
            Color32::from_white_alpha((18.0 * hover_t) as u8),
        );
    }
    ui.painter().text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::medium(12.0),
        if on { Color32::WHITE } else { theme::TEXT },
    );

    let on_t = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("sw"), on, 0.14);
    let sw_rect = Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(32.0, 18.0));
    let track_col = if on {
        theme::YT_BLUE
    } else {
        Color32::from_white_alpha(38)
    };
    ui.painter().rect_filled(sw_rect, 9.0, track_col);
    let knob_x = sw_rect.left() + 9.0 + on_t * 14.0;
    ui.painter()
        .circle_filled(pos2(knob_x, sw_rect.center().y), 6.5, Color32::WHITE);

    resp.clicked()
}

fn popover_nav_row(ui: &mut egui::Ui, label: &str, badge: &str, active: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if hover_t > 0.01 {
        ui.painter().rect_filled(
            rect,
            7.0,
            Color32::from_white_alpha((18.0 * hover_t) as u8),
        );
    }
    ui.painter().text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::medium(12.0),
        theme::TEXT,
    );
    let badge_col = if active { theme::YT_BLUE } else { theme::TEXT_DIM };
    let chevron_x = rect.right() - 10.0;
    ui.painter().text(
        pos2(chevron_x - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        badge,
        theme::medium(11.5),
        badge_col,
    );
    let stroke = Stroke::new(1.4, theme::TEXT_DIM);
    let cy = rect.center().y;
    ui.painter()
        .line_segment([pos2(chevron_x - 3.0, cy - 3.5), pos2(chevron_x + 1.0, cy)], stroke);
    ui.painter()
        .line_segment([pos2(chevron_x + 1.0, cy), pos2(chevron_x - 3.0, cy + 3.5)], stroke);
    resp.clicked()
}

fn step_timer_segment(val: u32, delta: i32, max_val: u32) -> u32 {
    let modulo = max_val as i32 + 1;
    (val as i32 + delta).rem_euclid(modulo) as u32
}

fn segments_to_secs(h: u32, m: u32, s: u32) -> u64 {
    h as u64 * 3600 + m as u64 * 60 + s as u64
}

fn timer_arrow_button(ui: &mut egui::Ui, id_salt: (&str, usize), width: f32, up: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        ui.painter()
            .rect_filled(rect, 6.0, Color32::from_white_alpha(20));
    }
    let col = if resp.hovered() {
        Color32::WHITE
    } else {
        theme::TEXT_DIM
    };
    let c = rect.center();
    let dy = if up { -2.5 } else { 2.5 };
    let stroke = Stroke::new(1.6, col);
    ui.painter().line_segment([c + vec2(-5.5, -dy), c + vec2(0.0, dy)], stroke);
    ui.painter().line_segment([c + vec2(0.0, dy), c + vec2(5.5, -dy)], stroke);
    let _ = id_salt;
    resp.clicked()
}

#[allow(clippy::too_many_arguments)]
fn draw_timer_segment_box(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    seg_idx: u8,
    width: f32,
    value: &mut u32,
    max_val: u32,
    editing_seg: &mut Option<u8>,
    seg_buf: &mut String,
) -> bool {
    let id = ui.make_persistent_id(("sleep_seg_box", seg_idx));
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 46.0), Sense::click_and_drag());
    let editing = *editing_seg == Some(seg_idx);
    let mut changed = false;

    if resp.dragged() {
        *editing_seg = None;
        ctx.set_cursor_icon(CursorIcon::ResizeVertical);
        let accum_id = id.with("drag_y");
        let dy = -resp.drag_delta().y;
        let step_px = 10.0_f32;
        let mut accum = ui.data(|d| d.get_temp::<f32>(accum_id)).unwrap_or(0.0) + dy;
        let steps = (accum / step_px).trunc() as i32;
        if steps != 0 {
            accum -= steps as f32 * step_px;
            *value = step_timer_segment(*value, steps, max_val);
            changed = true;
        }
        ui.data_mut(|d| d.insert_temp(accum_id, accum));
    } else {
        ui.data_mut(|d| d.insert_temp(id.with("drag_y"), 0.0_f32));
        if resp.hovered() {
            ctx.set_cursor_icon(CursorIcon::ResizeVertical);
        }
    }

    if resp.hovered() {
        let wheel_y = ctx.input(|i| i.smooth_scroll_delta.y);
        if wheel_y.abs() > 0.01 {
            *editing_seg = None;
            let s_id = id.with("wheel_y");
            let mut accum = ui.data(|d| d.get_temp::<f32>(s_id)).unwrap_or(0.0) + wheel_y;
            if accum.abs() >= 18.0 {
                let step = accum.signum() as i32;
                accum = 0.0;
                *value = step_timer_segment(*value, step, max_val);
                changed = true;
            }
            ui.data_mut(|d| d.insert_temp(s_id, accum));
        }
    }

    if resp.clicked() && !resp.drag_stopped() {
        if editing {
            *editing_seg = None;
            seg_buf.clear();
        } else {
            *editing_seg = Some(seg_idx);
            seg_buf.clear();
        }
        changed = true;
    }

    let bg = if editing {
        Color32::from_rgb(46, 48, 58)
    } else if resp.hovered() || resp.dragged() {
        Color32::from_rgb(42, 42, 49)
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 7.0, bg);
    }
    if editing {
        ui.painter()
            .rect_stroke(rect, 7.0, Stroke::new(1.5, theme::YT_BLUE), egui::StrokeKind::Inside);
    }

    let text_col = if editing || resp.hovered() || resp.dragged() {
        Color32::WHITE
    } else {
        Color32::from_rgb(205, 207, 214)
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("{:02}", *value),
        egui::FontId::proportional(25.0),
        text_col,
    );
    changed
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let r = (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t).round() as u8;
    let g = (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t).round() as u8;
    let bl = (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t).round() as u8;
    Color32::from_rgb(r, g, bl)
}

#[allow(clippy::too_many_arguments)]
fn paint_smooth_ambient_glow(
    painter: &egui::Painter,
    ctx: &egui::Context,
    rect: Rect,
    palette: [[u8; 3]; 4],
    prev_palette: [[u8; 3]; 4],
    changed_at: Option<Instant>,
    opacity_setting: f32,
    paused: bool,
) {
    let fade_t = changed_at
        .map(|t| (t.elapsed().as_secs_f32() / 1.4).clamp(0.0, 1.0))
        .unwrap_or(1.0);
    let animated = !paused;
    if animated || fade_t < 1.0 {
        ctx.request_repaint_after(Duration::from_millis(50));
    }
    let time = if animated {
        ctx.input(|i| i.time) as f32
    } else {
        0.0
    };
    let strength = (opacity_setting.clamp(0.05, 0.70) * 1.65).clamp(0.24, 1.0);
    let base_alpha = 112.0 * strength;

    let anchors = [
        pos2(rect.left() + rect.width() * 0.24, rect.top() + rect.height() * 0.26),
        pos2(rect.left() + rect.width() * 0.76, rect.top() + rect.height() * 0.24),
        pos2(rect.left() + rect.width() * 0.26, rect.top() + rect.height() * 0.76),
        pos2(rect.left() + rect.width() * 0.74, rect.top() + rect.height() * 0.74),
    ];
    let phases = [0.0_f32, 1.55, 3.1, 4.65];
    let rx_base = (rect.width() * 0.48).clamp(180.0, 480.0);
    let ry_base = (rect.height() * 0.62).clamp(150.0, 380.0);

    let mut mesh = egui::Mesh::default();
    const RINGS: [f32; 4] = [0.28, 0.54, 0.78, 1.0];
    const SEGS: usize = 28;

    for i in 0..4 {
        let col = lerp_rgb(prev_palette[i], palette[i], fade_t);
        let ph = phases[i];
        let drift_x = (time * 0.42 + ph).sin() * rect.width() * 0.045;
        let drift_y = (time * 0.31 + ph * 1.3).cos() * rect.height() * 0.045;
        let pulse = 1.0 + 0.065 * (time * 0.55 + ph * 0.8).sin();
        let alpha_pulse = 0.88 + 0.12 * (time * 0.62 + ph).cos();

        let center = anchors[i] + vec2(drift_x, drift_y);
        let rx = rx_base * pulse;
        let ry = ry_base * (2.0 - pulse * 0.96);
        let peak_a = (base_alpha * alpha_pulse).clamp(12.0, 128.0);

        let base_idx = mesh.vertices.len() as u32;
        mesh.colored_vertex(
            center,
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), peak_a.round() as u8),
        );

        for (ring_i, &t) in RINGS.iter().enumerate() {
            // Cosine-squared falloff gives C^1 smoothness with zero visible ring edges
            let w = 0.5 + 0.5 * (t * std::f32::consts::PI).cos();
            let a = if ring_i + 1 == RINGS.len() {
                0
            } else {
                (peak_a * w * w).round() as u8
            };
            let ring_col = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a);
            for s in 0..SEGS {
                let ang = (s as f32 / SEGS as f32) * std::f32::consts::TAU;
                let pos = pos2(center.x + ang.cos() * rx * t, center.y + ang.sin() * ry * t);
                mesh.colored_vertex(pos, ring_col);
            }
        }

        for s in 0..SEGS {
            let next = (s + 1) % SEGS;
            mesh.add_triangle(base_idx, base_idx + 1 + s as u32, base_idx + 1 + next as u32);
        }
        for ring_i in 0..(RINGS.len() - 1) {
            let r0 = base_idx + 1 + (ring_i * SEGS) as u32;
            let r1 = r0 + SEGS as u32;
            for s in 0..SEGS {
                let next = (s + 1) % SEGS;
                let a = r0 + s as u32;
                let b = r0 + next as u32;
                let c = r1 + s as u32;
                let d = r1 + next as u32;
                mesh.add_triangle(a, c, d);
                mesh.add_triangle(a, d, b);
            }
        }
    }

    let scroll_clip = painter.clip_rect();
    let glow_clip = Rect::from_min_max(
        pos2(scroll_clip.left() - 140.0, scroll_clip.top() - 18.0),
        pos2(scroll_clip.right() + 140.0, scroll_clip.bottom() + 140.0),
    )
    .intersect(ctx.content_rect());
    painter
        .with_clip_rect(glow_clip)
        .add(egui::Shape::mesh(mesh));
}

fn popover_pill_button(
    ui: &mut egui::Ui,
    size: Vec2,
    label: &str,
    active: bool,
    danger: bool,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let fill = if active {
        Color32::from_rgb(241, 241, 241)
    } else if resp.hovered() {
        Color32::from_rgb(56, 56, 64)
    } else {
        theme::CHIP
    };
    ui.painter().rect_filled(rect, 7.0, fill);
    let col = if active {
        Color32::from_rgb(15, 15, 15)
    } else if danger {
        theme::YT_RED
    } else {
        theme::TEXT
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        theme::medium(11.0),
        col,
    );
    resp.clicked()
}

fn sample_perimeter_ambilight(grid: &[[u8; 3]; AMB_GRID_W * AMB_GRID_H], angle: f32) -> Color32 {
    // Project ray from unit square center at `angle` onto the nearest video border and sample 2-deep
    let (s, c) = angle.sin_cos();
    let scale = 1.0 / c.abs().max(s.abs()).max(1e-4);
    let u = (0.5 + 0.45 * c * scale).clamp(0.0, 1.0);
    let v = (0.5 + 0.45 * s * scale).clamp(0.0, 1.0);
    let fx = u * (AMB_GRID_W - 1) as f32;
    let fy = v * (AMB_GRID_H - 1) as f32;
    let x0 = (fx.floor() as usize).min(AMB_GRID_W - 1);
    let x1 = (x0 + 1).min(AMB_GRID_W - 1);
    let y0 = (fy.floor() as usize).min(AMB_GRID_H - 1);
    let y1 = (y0 + 1).min(AMB_GRID_H - 1);
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;

    let c00 = grid[y0 * AMB_GRID_W + x0];
    let c10 = grid[y0 * AMB_GRID_W + x1];
    let c01 = grid[y1 * AMB_GRID_W + x0];
    let c11 = grid[y1 * AMB_GRID_W + x1];
    let mut out = [0u8; 3];
    for ch in 0..3 {
        let top = c00[ch] as f32 * (1.0 - tx) + c10[ch] as f32 * tx;
        let bot = c01[ch] as f32 * (1.0 - tx) + c11[ch] as f32 * tx;
        out[ch] = (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8;
    }
    Color32::from_rgb(out[0], out[1], out[2])
}

/// Real-time YouTube-Ambilight perimeter bloom driven directly by live decoded video frames.
fn paint_video_ambilight_glow(
    painter: &egui::Painter,
    ctx: &egui::Context,
    rect: Rect,
    snap: &LiveAmbientSnapshot,
    opacity_setting: f32,
    paused: bool,
) {
    if !paused {
        ctx.request_repaint_after(Duration::from_millis(33));
    }
    let strength = (opacity_setting.clamp(0.05, 0.70) * 1.60).clamp(0.24, 0.92);
    let peak_alpha = 112.0 * strength;
    let spread_x = (rect.width() * 0.28).clamp(130.0, 250.0);
    let spread_y = (rect.height() * 0.34).clamp(110.0, 220.0);
    let hw = rect.width() * 0.5;
    let hh = rect.height() * 0.5;
    let center = rect.center();

    const SEGS: usize = 48;
    const RINGS: [f32; 5] = [0.0, 0.24, 0.52, 0.78, 1.0];
    let mut mesh = egui::Mesh::default();

    for (ri, &t) in RINGS.iter().enumerate() {
        let w = 0.5 + 0.5 * (t * std::f32::consts::PI).cos();
        let ring_a = if ri + 1 == RINGS.len() {
            0
        } else {
            (peak_alpha * w * w).round().clamp(0.0, 165.0) as u8
        };
        let rx = (hw - 4.0) + (spread_x + 4.0) * t;
        let ry = (hh - 4.0) + (spread_y + 4.0) * t;
        for s in 0..SEGS {
            let ang = (s as f32 / SEGS as f32) * std::f32::consts::TAU;
            let (sa, ca) = ang.sin_cos();
            // Superellipse n=4 blends cleanly around rounded player corners
            let norm = (ca.abs().powi(4) + sa.abs().powi(4)).powf(-0.25);
            let pos = pos2(center.x + ca * norm * rx, center.y + sa * norm * ry);
            let edge = sample_perimeter_ambilight(&snap.grid, ang);
            let col = Color32::from_rgba_unmultiplied(edge.r(), edge.g(), edge.b(), ring_a);
            mesh.colored_vertex(pos, col);
        }
    }

    for ri in 0..(RINGS.len() - 1) {
        let r0 = (ri * SEGS) as u32;
        let r1 = ((ri + 1) * SEGS) as u32;
        for s in 0..SEGS {
            let next = (s + 1) % SEGS;
            let a = r0 + s as u32;
            let b = r0 + next as u32;
            let c = r1 + s as u32;
            let d = r1 + next as u32;
            mesh.add_triangle(a, c, d);
            mesh.add_triangle(a, d, b);
        }
    }

    let scroll_clip = painter.clip_rect();
    let glow_clip = Rect::from_min_max(
        pos2(scroll_clip.left() - 180.0, scroll_clip.top() - 28.0),
        pos2(scroll_clip.right() + 180.0, scroll_clip.bottom() + 180.0),
    )
    .intersect(ctx.content_rect());
    painter
        .with_clip_rect(glow_clip)
        .add(egui::Shape::mesh(mesh));
}

fn lerp_color32(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t).round() as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t).round() as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t).round() as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t).round() as u8,
    )
}

#[allow(clippy::too_many_arguments)]
fn paint_audio_spectrum(
    ui: &egui::Ui,
    ctx: &egui::Context,
    rect: Rect,
    radius: f32,
    bands: &[f32; NUM_BANDS],
    wave: &[f32; NUM_BANDS],
    theme_mode: SpectrumTheme,
    palette: [[u8; 3]; 4],
) {
    let p = ui.painter().with_clip_rect(rect.shrink(radius.min(4.0) * 0.25));
    let time = ctx.input(|i| i.time) as f32;
    let col_a = Color32::from_rgb(
        palette[0][0].max(62),
        palette[0][1].max(145),
        palette[0][2].max(220),
    );
    let col_b = Color32::from_rgb(
        palette[1][0].max(185),
        palette[1][1].max(72),
        palette[1][2].max(175),
    );

    match theme_mode {
        SpectrumTheme::WaveBars => {
            // Symmetrical sleek wave bars centered vertically across the canvas (ref: screenshot 4)
            let pad_x = 32.0_f32;
            let inner_w = (rect.width() - pad_x * 2.0).max(100.0);
            let step_w = inner_w / NUM_BANDS as f32;
            let bar_w = (step_w * 0.56).clamp(2.5, 6.0);
            let half_max = rect.height() * 0.38;
            let cy = rect.center().y + 6.0;

            p.line_segment(
                [pos2(rect.left() + pad_x, cy), pos2(rect.right() - pad_x, cy)],
                Stroke::new(1.0, Color32::from_white_alpha(32)),
            );
            for i in 0..NUM_BANDS {
                let b_mir = if i < NUM_BANDS / 2 {
                    bands[NUM_BANDS / 2 - 1 - i]
                } else {
                    bands[i - NUM_BANDS / 2]
                };
                let amp = (0.55 * wave[i] + 0.45 * b_mir).clamp(0.0, 1.0);
                let half_h = (amp * half_max).max(3.0);
                let x = rect.left() + pad_x + i as f32 * step_w + (step_w - bar_w) * 0.5;
                let bar_rect = Rect::from_min_max(pos2(x, cy - half_h), pos2(x + bar_w, cy + half_h));
                p.rect_filled(
                    bar_rect.expand2(vec2(1.5, 2.0)),
                    bar_w,
                    Color32::from_white_alpha(20),
                );
                p.rect_filled(
                    bar_rect,
                    bar_w * 0.5,
                    Color32::from_rgba_unmultiplied(245, 248, 255, 232),
                );
            }
        }
        SpectrumTheme::SynthHorizon => {
            // Luminous center glow + mirror horizon frequency spikes + water reflection (ref: screenshot 5)
            let pad_x = 24.0_f32;
            let inner_w = (rect.width() - pad_x * 2.0).max(100.0);
            let horizon_y = rect.top() + rect.height() * 0.56;
            let up_max = rect.height() * 0.38;
            let down_max = rect.height() * 0.28;
            let step_w = inner_w / NUM_BANDS as f32;
            let bar_w = (step_w * 0.62).clamp(2.0, 5.0);

            // Soft horizon center glow
            let energy: f32 = bands.iter().take(16).sum::<f32>() / 16.0;
            let halo_r = (rect.height() * (0.28 + energy * 0.14)).clamp(70.0, 210.0);
            p.circle_filled(
                pos2(rect.center().x, horizon_y),
                halo_r,
                col_b.gamma_multiply(0.14 + energy * 0.14),
            );
            p.line_segment(
                [
                    pos2(rect.left() + pad_x, horizon_y),
                    pos2(rect.right() - pad_x, horizon_y),
                ],
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 220, 245, 110)),
            );

            for i in 0..NUM_BANDS {
                // Mirror low frequencies smoothly on both sides / outer flanks + detail across center
                let edge_dist = ((i as f32 / (NUM_BANDS - 1) as f32) - 0.5).abs() * 2.0;
                let idx = ((edge_dist * (NUM_BANDS - 1) as f32).round() as usize).min(NUM_BANDS - 1);
                let amp = (bands[idx] * 0.72 + wave[i] * 0.28).clamp(0.0, 1.0);
                let uh = (amp * up_max).max(1.5);
                let dh = (amp * down_max).max(1.0);
                let x = rect.left() + pad_x + i as f32 * step_w + (step_w - bar_w) * 0.5;
                let tint = lerp_color32(Color32::from_rgb(255, 242, 252), col_b, 0.22);

                let top_rect =
                    Rect::from_min_max(pos2(x, horizon_y - uh), pos2(x + bar_w, horizon_y));
                p.rect_filled(top_rect.expand2(vec2(1.5, 2.5)), 2.0, col_b.gamma_multiply(0.24));
                p.rect_filled(top_rect, 1.5, tint.gamma_multiply(0.94));

                let ref_rect =
                    Rect::from_min_max(pos2(x, horizon_y + 1.0), pos2(x + bar_w, horizon_y + dh));
                p.rect_filled(ref_rect, 1.5, col_b.gamma_multiply(0.32));
            }
        }
        SpectrumTheme::NeonBars => {
            let pad_x = 24.0_f32;
            let inner_w = (rect.width() - pad_x * 2.0).max(80.0);
            let step_w = inner_w / NUM_BANDS as f32;
            let bar_w = (step_w * 0.68).max(2.5);
            let max_h = rect.height() * 0.56;
            let base_y = rect.bottom() - 22.0;

            let peak_id = ui.make_persistent_id("spec_neon_peaks");
            let mut peaks = ui
                .data(|d| d.get_temp::<[f32; NUM_BANDS]>(peak_id))
                .unwrap_or([0.0; NUM_BANDS]);

            for i in 0..NUM_BANDS {
                let v = bands[i].clamp(0.0, 1.0);
                peaks[i] = (peaks[i] - 0.012).max(v).clamp(0.0, 1.0);
                let bh = (v * max_h).max(3.0);
                let x = rect.left() + pad_x + i as f32 * step_w + (step_w - bar_w) * 0.5;
                let frac = i as f32 / (NUM_BANDS - 1) as f32;
                let c = lerp_color32(col_a, col_b, frac);

                let bar_rect = Rect::from_min_max(pos2(x, base_y - bh), pos2(x + bar_w, base_y));
                let glow_rect = bar_rect.expand2(vec2(2.0, 3.0));
                p.rect_filled(glow_rect, 3.0, c.gamma_multiply(0.18));
                p.rect_filled(bar_rect, (bar_w * 0.45).min(4.0), c.gamma_multiply(0.90));

                let cap_y = base_y - peaks[i] * max_h - 3.0;
                p.rect_filled(
                    Rect::from_min_max(pos2(x, cap_y - 2.0), pos2(x + bar_w, cap_y + 1.0)),
                    1.5,
                    Color32::from_rgba_unmultiplied(245, 248, 255, 205),
                );
            }
            ui.data_mut(|d| d.insert_temp(peak_id, peaks));
        }
        SpectrumTheme::RadialHalo => {
            let center = rect.center() + vec2(0.0, 8.0);
            let base_r = (rect.height().min(rect.width()) * 0.24).clamp(58.0, 130.0);
            let max_len = (rect.height().min(rect.width()) * 0.22).clamp(48.0, 125.0);
            let bass = (bands[0] + bands[1] + bands[2] + bands[3]) * 0.25;
            p.circle_filled(
                center,
                base_r * (0.94 + bass * 0.10),
                col_a.gamma_multiply(0.10 + bass * 0.12),
            );
            p.circle_stroke(
                center,
                base_r * (0.96 + bass * 0.08),
                Stroke::new(2.0, col_a.gamma_multiply(0.55)),
            );
            let count = NUM_BANDS;
            for i in 0..count {
                let ang = (i as f32 / count as f32) * std::f32::consts::TAU + time * 0.20;
                let (sa, ca) = ang.sin_cos();
                let amp = bands[i].clamp(0.0, 1.0);
                let r0 = base_r + 2.0;
                let r1 = r0 + 2.0 + amp * max_len;
                let frac = i as f32 / (NUM_BANDS - 1) as f32;
                let c = lerp_color32(col_a, col_b, frac).gamma_multiply(0.92);
                p.line_segment(
                    [
                        pos2(center.x + ca * r0, center.y + sa * r0),
                        pos2(center.x + ca * r1, center.y + sa * r1),
                    ],
                    Stroke::new(3.0, c),
                );
            }
        }
        SpectrumTheme::AuroraRibbon => {
            let layers = [
                (col_a.gamma_multiply(0.36), 0.56, 0.0_f32),
                (lerp_color32(col_a, col_b, 0.5).gamma_multiply(0.34), 0.44, 1.4),
                (col_b.gamma_multiply(0.32), 0.34, 2.8),
            ];
            let base_y = rect.bottom() - 8.0;
            for (fill_col, scale_h, phase) in layers {
                let mut mesh = egui::Mesh::default();
                let top_clear = Color32::from_rgba_unmultiplied(fill_col.r(), fill_col.g(), fill_col.b(), 165);
                let bot_col = Color32::from_rgba_unmultiplied(fill_col.r(), fill_col.g(), fill_col.b(), 12);
                let pts = 72usize;
                for i in 0..=pts {
                    let u = i as f32 / pts as f32;
                    let bf = u * (NUM_BANDS - 1) as f32;
                    let b0 = bf.floor() as usize;
                    let b1 = (b0 + 1).min(NUM_BANDS - 1);
                    let frac = bf - b0 as f32;
                    let amp = (bands[b0] * 0.65 + wave[b0] * 0.35) * (1.0 - frac)
                        + (bands[b1] * 0.65 + wave[b1] * 0.35) * frac;
                    let wave_mod = 0.05 * (u * 8.0 + time * 1.4 + phase).sin();
                    let h = ((amp + wave_mod).max(0.04) * rect.height() * scale_h).min(rect.height() * 0.74);
                    let x = rect.left() + u * rect.width();
                    mesh.colored_vertex(pos2(x, base_y - h), top_clear);
                    mesh.colored_vertex(pos2(x, base_y), bot_col);
                }
                for i in 0..pts {
                    let a = (i * 2) as u32;
                    mesh.add_triangle(a, a + 1, a + 3);
                    mesh.add_triangle(a, a + 3, a + 2);
                }
                p.add(egui::Shape::mesh(mesh));
            }
        }
        SpectrumTheme::DotMatrix => {
            let pad_x = 26.0_f32;
            let inner_w = (rect.width() - pad_x * 2.0).max(80.0);
            let cols = 40usize;
            let rows = 18usize;
            let step_x = inner_w / cols as f32;
            let cell_w = (step_x * 0.66).max(3.0);
            let max_h = rect.height() * 0.58;
            let step_y = max_h / rows as f32;
            let cell_h = (step_y * 0.66).max(2.5);
            let base_y = rect.bottom() - 20.0;

            for c_idx in 0..cols {
                let band_idx = (c_idx * NUM_BANDS) / cols;
                let lit_rows = (bands[band_idx].clamp(0.0, 1.0) * rows as f32).round() as usize;
                let x = rect.left() + pad_x + c_idx as f32 * step_x + (step_x - cell_w) * 0.5;
                for r_idx in 0..rows {
                    let y1 = base_y - r_idx as f32 * step_y;
                    let cell_rect = Rect::from_min_max(pos2(x, y1 - cell_h), pos2(x + cell_w, y1));
                    if r_idx < lit_rows {
                        let lvl = r_idx as f32 / (rows - 1) as f32;
                        let col = if lvl > 0.78 {
                            theme::YT_RED
                        } else if lvl > 0.48 {
                            col_b
                        } else {
                            theme::YT_BLUE
                        };
                        p.rect_filled(cell_rect, 2.0, col.gamma_multiply(0.92));
                    } else {
                        p.rect_filled(cell_rect, 2.0, Color32::from_white_alpha(12));
                    }
                }
            }
        }
    }
}

enum ShortsUiAction {
    None,
    Step(isize),
    ReturnToVideo,
    GridView,
    OpenChannel(String, String),
}

fn draw_pip_dock_placeholder(ui: &mut egui::Ui, size: Vec2) -> bool {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, 12.0, Color32::from_rgb(18, 18, 22));
    ui.painter().rect_stroke(
        rect,
        12.0,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    let mut dock_clicked = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                theme::draw_miniplayer_icon(
                    ui.painter(),
                    pos2(rect.center().x, rect.center().y - 24.0),
                    28.0,
                    theme::TEXT_DIM,
                );
                ui.add_space(28.0);
                ui.label(
                    RichText::new("Playing in Mini-player")
                        .font(theme::semibold(14.0))
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(8.0);
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Back to player")
                                .font(theme::semibold(12.5))
                                .color(Color32::from_rgb(15, 15, 15)),
                        )
                        .fill(Color32::from_rgb(241, 241, 241))
                        .corner_radius(16),
                    )
                    .clicked()
                {
                    dock_clicked = true;
                }
            });
        });
    });
    dock_clicked
}

fn draw_live_chat_message_runs(ui: &mut egui::Ui, m: &crate::model::LiveChatMessage) {
    if m.runs.is_empty() {
        if !m.text.is_empty() {
            ui.label(
                RichText::new(&m.text)
                    .font(theme::regular(12.5))
                    .color(theme::TEXT),
            );
        }
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for run in &m.runs {
            match run {
                LiveChatRun::Text(t) => {
                    if !t.is_empty() {
                        ui.label(
                            RichText::new(t)
                                .font(theme::regular(12.5))
                                .color(theme::TEXT),
                        );
                    }
                }
                LiveChatRun::Emoji { alt, url } => {
                    if let Some(u) = url {
                        let img = egui::Image::new(u)
                            .show_loading_spinner(false)
                            .fit_to_exact_size(vec2(17.0, 17.0));
                        let eresp = ui.add(img);
                        if !alt.is_empty() {
                            eresp.on_hover_text(alt);
                        }
                    } else if !alt.is_empty() {
                        ui.label(
                            RichText::new(alt)
                                .font(theme::regular(12.5))
                                .color(theme::TEXT),
                        );
                    }
                }
            }
        }
    });
}

fn shorts_rail_button(
    ui: &mut egui::Ui,
    label: &str,
    tooltip: &str,
    active: bool,
    paint_icon: impl FnOnce(&egui::Painter, egui::Pos2, Color32),
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(60.0, 62.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let circle_center = pos2(rect.center().x, rect.top() + 21.0);
    let fill = if active {
        theme::YT_BLUE.gamma_multiply(0.24)
    } else if resp.hovered() {
        theme::CHIP_HOVER
    } else {
        theme::CHIP
    };
    ui.painter().circle_filled(circle_center, 21.0, fill);
    let col = if active { theme::YT_BLUE } else { theme::TEXT };
    paint_icon(ui.painter(), circle_center, col);
    ui.painter().text(
        pos2(rect.center().x, rect.top() + 52.0),
        Align2::CENTER_CENTER,
        label,
        theme::medium(11.0),
        if active { theme::YT_BLUE } else { theme::TEXT_DIM },
    );
    resp.on_hover_text(tooltip).clicked()
}

/// Diagonal outward arrows icon (`⤢`) for expanding from PiP back to the full Watch player.
fn draw_expand_icon(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    let half = size * 0.42;
    let arm = size * 0.28;
    let gap = size * 0.12;
    let stroke = Stroke::new(1.6, color);

    // Top-right outward arrow
    let tr = pos2(center.x + half, center.y - half);
    painter.line_segment([pos2(center.x + gap, center.y - gap), tr], stroke);
    painter.line_segment([tr, pos2(tr.x - arm, tr.y)], stroke);
    painter.line_segment([tr, pos2(tr.x, tr.y + arm)], stroke);

    // Bottom-left outward arrow
    let bl = pos2(center.x - half, center.y + half);
    painter.line_segment([pos2(center.x - gap, center.y + gap), bl], stroke);
    painter.line_segment([bl, pos2(bl.x + arm, bl.y)], stroke);
    painter.line_segment([bl, pos2(bl.x, bl.y - arm)], stroke);
}

/// Pop-out window frame with diagonal external arrow (`↗`) for borderless Windows PiP.
fn draw_win_pip_icon(painter: &egui::Painter, center: egui::Pos2, size: f32, color: Color32) {
    let half = size * 0.42;
    let stroke = Stroke::new(1.5, color);
    let x0 = center.x - half;
    let y0 = center.y - half * 0.78;
    let x1 = center.x + half * 0.78;
    let y1 = center.y + half;
    let r = 2.0_f32;

    // Open-top-right rounded window frame (top-left -> left -> bottom -> right-mid)
    let pts = vec![
        pos2(center.x + half * 0.05, y0),
        pos2(x0 + r, y0),
        pos2(x0, y0 + r),
        pos2(x0, y1 - r),
        pos2(x0 + r, y1),
        pos2(x1 - r, y1),
        pos2(x1, y1 - r),
        pos2(x1, center.y - half * 0.05),
    ];
    painter.add(egui::Shape::line(pts, stroke));

    // Pop-out diagonal arrow pointing out to the top-right corner
    let tip = pos2(center.x + half, center.y - half);
    let stem = pos2(center.x - half * 0.05, center.y + half * 0.05);
    let head = size * 0.28;
    painter.line_segment([stem, tip], stroke);
    painter.line_segment([tip, pos2(tip.x - head, tip.y)], stroke);
    painter.line_segment([tip, pos2(tip.x, tip.y + head)], stroke);
}

fn shorts_nav_circle(ui: &mut egui::Ui, ico: char, tooltip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let fill = if resp.hovered() {
        theme::CHIP_HOVER
    } else {
        theme::CHIP
    };
    ui.painter().circle_filled(rect.center(), 22.0, fill);
    theme::draw_icon_centered(ui.painter(), rect.center(), ico, 19.0, theme::TEXT);
    resp.on_hover_text(tooltip).clicked()
}

fn is_ts_left_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '(' | '[' | '{' | '-' | '/' | '•' | '|' | '–' | '—' | '"' | '\'' | '@'
        )
}

fn is_ts_right_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            ')' | ']' | '}' | ',' | '.' | '!' | '?' | ';' | ':' | '-' | '/' | '–' | '—' | '"' | '\''
        )
}

fn parse_timestamp_token(tok: &str) -> Option<f64> {
    let parts: Vec<&str> = tok.split(':').collect();
    if parts.len() != 2 && parts.len() != 3 {
        return None;
    }
    if parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    if parts.len() == 2 {
        if parts[0].len() > 3 || parts[1].len() != 2 {
            return None;
        }
        let m: u64 = parts[0].parse().ok()?;
        let s: u64 = parts[1].parse().ok()?;
        (s < 60).then_some((m * 60 + s) as f64)
    } else {
        if parts[0].len() > 2 || parts[1].len() != 2 || parts[2].len() != 2 {
            return None;
        }
        let h: u64 = parts[0].parse().ok()?;
        let m: u64 = parts[1].parse().ok()?;
        let s: u64 = parts[2].parse().ok()?;
        (m < 60 && s < 60).then_some((h * 3600 + m * 60 + s) as f64)
    }
}

/// Finds clickable timestamp byte ranges `(start, end, secs)` inside comment/description text.
fn extract_timestamp_spans(text: &str, max_secs: Option<f64>) -> Vec<(usize, usize, f64)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
            i += ch_len;
            continue;
        }
        let left_ok = i == 0
            || text[..i]
                .chars()
                .next_back()
                .is_some_and(is_ts_left_boundary);
        let start = i;
        let mut end = i;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b':') {
            end += 1;
        }
        // Trim trailing ':' (e.g. "3:45: intro") so the timestamp token itself stays clean.
        while end > start && bytes[end - 1] == b':' {
            end -= 1;
        }
        let right_ok = end == bytes.len()
            || text[end..]
                .chars()
                .next()
                .is_some_and(is_ts_right_boundary);
        if left_ok && right_ok && end > start {
            if let Some(secs) = parse_timestamp_token(&text[start..end]) {
                let in_range = max_secs.is_none_or(|m| secs <= m + 1.0);
                if in_range {
                    out.push((start, end, secs));
                }
            }
        }
        i = end.max(i + 1);
    }
    out
}

fn draw_text_with_timestamps(
    ui: &mut egui::Ui,
    text: &str,
    font_size: f32,
    base_color: Color32,
    max_secs: Option<f64>,
) -> Option<f64> {
    let spans = extract_timestamp_spans(text, max_secs);
    if spans.is_empty() {
        ui.label(
            RichText::new(text)
                .font(theme::regular(font_size))
                .color(base_color),
        );
        return None;
    }

    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width().max(60.0);
    let normal_fmt = egui::TextFormat {
        font_id: theme::regular(font_size),
        color: base_color,
        ..Default::default()
    };
    let link_fmt = egui::TextFormat {
        font_id: theme::medium(font_size),
        color: theme::YT_BLUE,
        ..Default::default()
    };

    let mut cursor_byte = 0usize;
    let mut char_pos = 0usize;
    let mut char_spans: Vec<(usize, usize, f64)> = Vec::with_capacity(spans.len());

    for (s, e, secs) in spans {
        if s > cursor_byte {
            let plain = &text[cursor_byte..s];
            job.append(plain, 0.0, normal_fmt.clone());
            char_pos += plain.chars().count();
        }
        let ts_str = &text[s..e];
        let ts_chars = ts_str.chars().count();
        job.append(ts_str, 0.0, link_fmt.clone());
        char_spans.push((char_pos, char_pos + ts_chars, secs));
        char_pos += ts_chars;
        cursor_byte = e;
    }
    if cursor_byte < text.len() {
        job.append(&text[cursor_byte..], 0.0, normal_fmt);
    }

    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());

    let hovered_ts = resp
        .hover_pos()
        .or_else(|| {
            if resp.clicked() {
                resp.interact_pointer_pos()
            } else {
                None
            }
        })
        .and_then(|ptr| {
            char_spans.iter().find_map(|&(cs, ce, secs)| {
                let r0 = galley.pos_from_cursor(egui::text::CCursor::new(cs));
                let r1 = galley.pos_from_cursor(egui::text::CCursor::new(ce));
                if (r0.top() - r1.top()).abs() < 2.0 {
                    let hit = Rect::from_min_max(
                        pos2(rect.left() + r0.left() - 2.0, rect.top() + r0.top()),
                        pos2(rect.left() + r1.left() + 2.0, rect.top() + r0.bottom()),
                    );
                    hit.contains(ptr).then_some((hit, secs))
                } else {
                    None
                }
            })
        });

    ui.painter().galley(rect.min, galley, base_color);

    if let Some((hit_rect, secs)) = hovered_ts {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        ui.painter().line_segment(
            [
                pos2(hit_rect.left() + 2.0, hit_rect.bottom() - 1.5),
                pos2(hit_rect.right() - 2.0, hit_rect.bottom() - 1.5),
            ],
            Stroke::new(1.0, theme::YT_BLUE),
        );
        let tip = format!("Jump to {}", format_duration(secs));
        let clicked = resp.on_hover_text(tip).clicked();
        if clicked {
            return Some(secs);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn draw_comments_thread_ui(
    ui: &mut egui::Ui,
    owner_id: &str,
    signed_in: bool,
    comments: &mut Vec<crate::model::CommentItem>,
    comments_loading: &mut bool,
    next_token: &mut Option<String>,
    create_params: Option<&str>,
    comment_draft: &mut String,
    reload_tok: Option<String>,
    reply_target_id: &mut Option<String>,
    reply_target_author: &mut String,
    reply_draft: &mut String,
    backend_tx: &crossbeam_channel::Sender<Request>,
    max_seek: Option<f64>,
    empty_msg: &str,
    allow_load_btn: Option<String>,
    is_post: bool,
) -> Option<f64> {
    let mut expand_replies: Option<(String, String)> = None;
    let mut seek_requested: Option<f64> = None;

    if signed_in {
        if let Some(params) = create_params {
            ui.horizontal(|ui| {
                let input_w = (ui.available_width() - 92.0).max(120.0);
                let te = ui.add_sized(
                    Vec2::new(input_w, 30.0),
                    egui::TextEdit::singleline(comment_draft).hint_text("Add a comment..."),
                );
                let submit = ui
                    .add_sized(
                        Vec2::new(82.0, 30.0),
                        egui::Button::new(
                            RichText::new("Comment")
                                .font(theme::semibold(12.5))
                                .color(Color32::from_rgb(15, 15, 15)),
                        )
                        .fill(theme::YT_BLUE)
                        .corner_radius(15),
                    )
                    .clicked()
                    || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if submit {
                    let body = comment_draft.trim().to_owned();
                    if !body.is_empty() {
                        let _ = backend_tx.send(Request::PostComment {
                            video_id: owner_id.to_owned(),
                            text: body,
                            params: params.to_owned(),
                            reload_continuation: reload_tok,
                        });
                        comment_draft.clear();
                    }
                }
            });
            ui.add_space(6.0);
        }
    }

    if *comments_loading && comments.is_empty() {
        ui.spinner();
        return None;
    }

    if comments.is_empty() {
        if let Some(tok) = allow_load_btn {
            if ui.button("Load comments").clicked() {
                *comments_loading = true;
                let _ = backend_tx.send(Request::LoadComments {
                    video_id: owner_id.to_owned(),
                    continuation: tok,
                    append: false,
                });
            }
        } else {
            ui.label(
                RichText::new(empty_msg)
                    .font(theme::regular(13.0))
                    .color(theme::TEXT_DIM),
            );
        }
        return None;
    }

    let mut open_reply_for: Option<(String, String, Option<String>)> = None;
    let mut submit_reply: Option<(String, String, String, Option<String>)> = None;
    let mut cancel_reply = false;
    let mut toggle_like_comment: Option<(String, String, bool)> = None;
    let active_reply_id = reply_target_id.clone();

    let draw_reply_box = |ui: &mut egui::Ui,
                          draft: &mut String,
                          target_author: &str,
                          parent_cid: &str,
                          rp: String,
                          replies_tok: Option<String>,
                          submit_reply: &mut Option<(String, String, String, Option<String>)>,
                          cancel_reply: &mut bool| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let rw = (ui.available_width() - 140.0).max(130.0);
            let te = ui.add_sized(
                Vec2::new(rw, 28.0),
                egui::TextEdit::singleline(draft)
                    .hint_text(format!("Replying to {target_author}...")),
            );
            let send_clicked = ui
                .add_sized(
                    Vec2::new(60.0, 28.0),
                    egui::Button::new(
                        RichText::new("Reply")
                            .font(theme::semibold(12.0))
                            .color(Color32::from_rgb(15, 15, 15)),
                    )
                    .fill(theme::YT_BLUE)
                    .corner_radius(14),
                )
                .clicked()
                || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if send_clicked {
                let rtxt = draft.trim().to_owned();
                if !rtxt.is_empty() {
                    *submit_reply = Some((parent_cid.to_owned(), rtxt, rp, replies_tok));
                }
            }
            if ui
                .add_sized(
                    Vec2::new(60.0, 28.0),
                    egui::Button::new(
                        RichText::new("Cancel")
                            .font(theme::medium(12.0))
                            .color(theme::TEXT_DIM),
                    )
                    .fill(theme::CHIP)
                    .corner_radius(14),
                )
                .clicked()
            {
                *cancel_reply = true;
            }
        });
    };

    for c in comments.iter() {
        ui.horizontal_top(|ui| {
            draw_avatar(ui, c.avatar_url.as_deref(), &c.author, 32.0);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                if c.is_pinned {
                    ui.horizontal(|ui| {
                        let (prect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        theme::draw_pin_icon(
                            ui.painter(),
                            prect.center(),
                            11.0,
                            theme::TEXT_DIM,
                        );
                        ui.label(
                            RichText::new("Pinned")
                                .font(theme::medium(11.0))
                                .color(theme::TEXT_DIM),
                        );
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&c.author).font(theme::semibold(12.5)));
                    if !c.published.is_empty() {
                        ui.label(
                            RichText::new(&c.published)
                                .font(theme::regular(11.5))
                                .color(theme::TEXT_DIM),
                        );
                    }
                });
                if let Some(secs) =
                    draw_text_with_timestamps(ui, &c.text, 13.0, theme::TEXT, max_seek)
                {
                    seek_requested = Some(secs);
                }
                ui.horizontal(|ui| {
                    let can_like = signed_in
                        && if c.is_liked {
                            c.unlike_action_params.is_some()
                        } else {
                            c.like_action_params.is_some()
                        };
                    let like_col = if c.is_liked {
                        theme::YT_BLUE
                    } else {
                        theme::TEXT_DIM
                    };
                    let (lrect, lresp) = ui.allocate_exact_size(
                        vec2(22.0, 22.0),
                        if can_like { Sense::click() } else { Sense::hover() },
                    );
                    if can_like && lresp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    theme::draw_thumbs_up(
                        ui.painter(),
                        lrect.center(),
                        13.0,
                        like_col,
                        c.is_liked,
                    );
                    if can_like && lresp.on_hover_text("Like comment").clicked() {
                        if let Some(act) = if c.is_liked {
                            c.unlike_action_params.clone()
                        } else {
                            c.like_action_params.clone()
                        } {
                            toggle_like_comment = Some((c.id.clone(), act, !c.is_liked));
                        }
                    }
                    if !c.likes.is_empty() {
                        ui.label(
                            RichText::new(&c.likes)
                                .font(theme::regular(11.5))
                                .color(like_col),
                        );
                    }
                    if c.has_creator_heart {
                        ui.add_space(4.0);
                        let (hrect, hresp) =
                            ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        theme::draw_icon_centered(
                            ui.painter(),
                            hrect.center(),
                            icon::HEART,
                            12.5,
                            theme::YT_RED,
                        );
                        hresp.on_hover_text("Hearted by creator");
                    }
                    if signed_in && c.reply_params.is_some() {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Reply")
                                        .font(theme::semibold(11.5))
                                        .color(theme::TEXT_DIM),
                                )
                                .fill(Color32::TRANSPARENT),
                            )
                            .clicked()
                        {
                            open_reply_for = Some((c.id.clone(), c.author.clone(), None));
                        }
                    }
                });
                if active_reply_id.as_deref() == Some(c.id.as_str()) {
                    if let Some(rp) = c.reply_params.clone() {
                        draw_reply_box(
                            ui,
                            reply_draft,
                            reply_target_author,
                            &c.id,
                            rp,
                            c.replies_token.clone(),
                            &mut submit_reply,
                            &mut cancel_reply,
                        );
                    }
                }
                if !c.replies.is_empty() {
                    ui.add_space(6.0);
                    for r in &c.replies {
                        ui.horizontal_top(|ui| {
                            draw_avatar(ui, r.avatar_url.as_deref(), &r.author, 24.0);
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(&r.author).font(theme::semibold(12.0)),
                                    );
                                    if !r.published.is_empty() {
                                        ui.label(
                                            RichText::new(&r.published)
                                                .font(theme::regular(11.0))
                                                .color(theme::TEXT_DIM),
                                        );
                                    }
                                });
                                if let Some(secs) = draw_text_with_timestamps(
                                    ui,
                                    &r.text,
                                    12.5,
                                    theme::TEXT,
                                    max_seek,
                                ) {
                                    seek_requested = Some(secs);
                                }
                                ui.horizontal(|ui| {
                                    let can_like = signed_in
                                        && if r.is_liked {
                                            r.unlike_action_params.is_some()
                                        } else {
                                            r.like_action_params.is_some()
                                        };
                                    let like_col = if r.is_liked {
                                        theme::YT_BLUE
                                    } else {
                                        theme::TEXT_DIM
                                    };
                                    let (lrect, lresp) = ui.allocate_exact_size(
                                        vec2(20.0, 20.0),
                                        if can_like {
                                            Sense::click()
                                        } else {
                                            Sense::hover()
                                        },
                                    );
                                    if can_like && lresp.hovered() {
                                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                    }
                                    theme::draw_thumbs_up(
                                        ui.painter(),
                                        lrect.center(),
                                        12.0,
                                        like_col,
                                        r.is_liked,
                                    );
                                    if can_like && lresp.on_hover_text("Like reply").clicked() {
                                        if let Some(act) = if r.is_liked {
                                            r.unlike_action_params.clone()
                                        } else {
                                            r.like_action_params.clone()
                                        } {
                                            toggle_like_comment =
                                                Some((r.id.clone(), act, !r.is_liked));
                                        }
                                    }
                                    if !r.likes.is_empty() {
                                        ui.label(
                                            RichText::new(&r.likes)
                                                .font(theme::regular(11.0))
                                                .color(like_col),
                                        );
                                    }
                                    if r.has_creator_heart {
                                        let (hrect, hresp) = ui.allocate_exact_size(
                                            vec2(16.0, 16.0),
                                            Sense::hover(),
                                        );
                                        theme::draw_icon_centered(
                                            ui.painter(),
                                            hrect.center(),
                                            icon::HEART,
                                            11.5,
                                            theme::YT_RED,
                                        );
                                        hresp.on_hover_text("Hearted by creator");
                                    }
                                    let r_params =
                                        r.reply_params.as_ref().or(c.reply_params.as_ref());
                                    if signed_in && r_params.is_some() {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("Reply")
                                                        .font(theme::semibold(11.0))
                                                        .color(theme::TEXT_DIM),
                                                )
                                                .fill(Color32::TRANSPARENT),
                                            )
                                            .clicked()
                                        {
                                            open_reply_for = Some((
                                                r.id.clone(),
                                                r.author.clone(),
                                                Some(format!("@{} ", r.author)),
                                            ));
                                        }
                                    }
                                });
                                if active_reply_id.as_deref() == Some(r.id.as_str()) {
                                    if let Some(rp) =
                                        r.reply_params.clone().or_else(|| c.reply_params.clone())
                                    {
                                        draw_reply_box(
                                            ui,
                                            reply_draft,
                                            reply_target_author,
                                            &c.id,
                                            rp,
                                            c.replies_token.clone(),
                                            &mut submit_reply,
                                            &mut cancel_reply,
                                        );
                                    }
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                }
                if c.replies_loading {
                    ui.spinner();
                } else if let Some(rtok) = c.replies_token.clone() {
                    let label = if c.replies.is_empty() && !c.reply_count_text.is_empty() {
                        c.reply_count_text.clone()
                    } else if c.replies.is_empty() {
                        "View replies".to_owned()
                    } else {
                        "Load more replies".to_owned()
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("{}  {label}", icon::CHEVRON_DOWN))
                                    .font(theme::semibold(12.0))
                                    .color(theme::YT_BLUE),
                            )
                            .fill(Color32::TRANSPARENT),
                        )
                        .clicked()
                    {
                        expand_replies = Some((c.id.clone(), rtok));
                    }
                }
            });
        });
        ui.add_space(10.0);
    }

    if let Some((cid, act, next_liked)) = toggle_like_comment {
        for c in comments.iter_mut() {
            if c.id == cid {
                c.is_liked = next_liked;
            }
            for r in &mut c.replies {
                if r.id == cid {
                    r.is_liked = next_liked;
                }
            }
        }
        let _ = backend_tx.send(Request::PerformCommentAction {
            video_id: owner_id.to_owned(),
            comment_id: cid,
            action_params: act,
            liked: next_liked,
        });
    }
    if let Some((cid, author, prefill)) = open_reply_for {
        *reply_target_id = Some(cid);
        *reply_target_author = author;
        *reply_draft = prefill.unwrap_or_default();
    } else if cancel_reply {
        *reply_target_id = None;
        reply_target_author.clear();
        reply_draft.clear();
    } else if let Some((cid, rtxt, rp, rtok)) = submit_reply {
        let _ = backend_tx.send(Request::PostCommentReply {
            video_id: owner_id.to_owned(),
            comment_id: cid,
            text: rtxt,
            params: rp,
            replies_continuation: rtok,
        });
        *reply_target_id = None;
        reply_target_author.clear();
        reply_draft.clear();
    }
    if let Some((cid, rtok)) = expand_replies {
        if let Some(c) = comments.iter_mut().find(|c| c.id == cid) {
            c.replies_loading = true;
        }
        let _ = backend_tx.send(Request::LoadCommentReplies {
            video_id: owner_id.to_owned(),
            comment_id: cid,
            continuation: rtok,
        });
    }
    if !*comments_loading {
        if let Some(tok) = next_token.clone() {
            if super::cards::pill_button(ui, icon::CHEVRON_DOWN, "Load more comments").clicked() {
                *comments_loading = true;
                if is_post {
                    let _ = backend_tx.send(Request::LoadPostComments {
                        post_id: owner_id.to_owned(),
                        browse_id: None,
                        params: None,
                        continuation: Some(tok),
                        append: true,
                    });
                } else {
                    let _ = backend_tx.send(Request::LoadComments {
                        video_id: owner_id.to_owned(),
                        continuation: tok,
                        append: true,
                    });
                }
            }
        }
    }
    seek_requested
}

#[cfg(test)]
mod watch_timestamp_tests {
    use super::*;

    #[test]
    fn extracts_valid_timestamps_and_ignores_invalid_tokens() {
        let txt = "0:15 intro (3:45) and 1:02:30-1:05:00, skip 12:75, 1:2, abc1:23, 1:2:3:4.";
        let spans = extract_timestamp_spans(txt, None);
        let got: Vec<(&str, f64)> = spans
            .iter()
            .map(|&(s, e, sec)| (&txt[s..e], sec))
            .collect();
        assert_eq!(
            got,
            vec![
                ("0:15", 15.0),
                ("3:45", 225.0),
                ("1:02:30", 3750.0),
                ("1:05:00", 3900.0),
            ]
        );
    }

    #[test]
    fn filters_timestamps_beyond_video_duration() {
        let txt = "0:30 early, 15:00 way past end";
        let spans = extract_timestamp_spans(txt, Some(300.0));
        assert_eq!(spans.len(), 1);
        assert_eq!(&txt[spans[0].0..spans[0].1], "0:30");
    }

    #[test]
    fn steps_and_wraps_sleep_timer_segments() {
        assert_eq!(step_timer_segment(0, -1, 59), 59);
        assert_eq!(step_timer_segment(59, 1, 59), 0);
        assert_eq!(step_timer_segment(23, 2, 23), 1);
        assert_eq!(step_timer_segment(1, -3, 23), 22);
        assert_eq!(segments_to_secs(1, 10, 5), 4205);
    }
}

fn tlang_chip(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let resp = ui.add(
        egui::Button::new(
            RichText::new(label)
                .font(theme::medium(10.5))
                .color(if active {
                    Color32::from_rgb(15, 15, 15)
                } else {
                    theme::TEXT
                }),
        )
        .min_size(vec2(0.0, 22.0))
        .fill(if active {
            Color32::from_rgb(241, 241, 241)
        } else {
            theme::CHIP
        })
        .corner_radius(6),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn build_tlang_options(yt_langs: &[(String, String)]) -> Vec<(String, String)> {
    const COMMON: &[(&str, &str)] = &[
        ("en", "EN"),
        ("id", "ID"),
        ("ja", "JA"),
        ("ko", "KO"),
        ("es", "ES"),
        ("fr", "FR"),
        ("de", "DE"),
        ("pt", "PT"),
        ("zh-Hans", "ZH"),
        ("ar", "AR"),
        ("hi", "HI"),
        ("ru", "RU"),
    ];
    let mut out = Vec::new();
    for (code, short) in COMMON {
        out.push(((*code).to_owned(), (*short).to_owned()));
    }
    if yt_langs.is_empty() {
        return out;
    }
    // Keep common quick codes while ensuring only valid/known codes or extra top languages are reachable
    for (code, name) in yt_langs.iter().take(6) {
        if !out.iter().any(|(c, _)| c == code) {
            let short = name.chars().take(10).collect::<String>();
            out.push((code.clone(), short));
        }
    }
    out
}
