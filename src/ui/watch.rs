use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Rect, RichText,
    ScrollArea, Sense, Stroke, Vec2, pos2, vec2,
};

use crate::backend::Request;
use crate::model::{SponsorCategory, VideoItem, WatchData};
use crate::theme::{self, icon};

use super::cards::{
    GridAction, draw_avatar, draw_thumbnail, draw_video_context_menu, icon_circle_button, join_meta,
};
use super::{HudIcon, Page, PlayerMenu, RightTab, YtApp, format_duration};

impl YtApp {
    pub(super) fn draw_watch_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.fullscreen {
            let avail = ui.available_size();
            self.draw_video_surface(ui, ctx, avail, true);
            return;
        }

        let wide = ui.available_width() >= 920.0 && !self.theater_mode;
        if wide {
            let total_w = ui.available_width();
            let right_w = (total_w * 0.29).clamp(300.0, 390.0);
            let left_w = (total_w - right_w - 18.0).max(460.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left_w, ui.available_height()),
                    Layout::top_down(Align::LEFT),
                    |ui| {
                        let mut sa = ScrollArea::vertical().id_salt("watch_left_scroll");
                        if self.scroll_watch_to_top {
                            sa = sa.vertical_scroll_offset(0.0);
                            self.scroll_watch_to_top = false;
                            self.watch_scrolled_into_pip = false;
                        }
                        let scroll_out = sa.show(ui, |ui| {
                            let video_h =
                                (left_w * 9.0 / 16.0).min(ctx.content_rect().height() * 0.74);
                            if self.watch_scrolled_into_pip && self.watch.is_some() {
                                if draw_pip_dock_placeholder(ui, Vec2::new(left_w, video_h)) {
                                    self.watch_scrolled_into_pip = false;
                                    self.scroll_watch_to_top = true;
                                }
                            } else {
                                self.draw_video_surface(ui, ctx, Vec2::new(left_w, video_h), false);
                            }
                            ui.add_space(12.0);
                            self.draw_watch_metadata(ui);
                        });
                        let sy = scroll_out.state.clamped_offset().y;
                        if !self.watch_scrolled_into_pip && sy > 28.0 && self.watch.is_some() {
                            self.watch_scrolled_into_pip = true;
                        } else if self.watch_scrolled_into_pip && sy <= 12.0 {
                            self.watch_scrolled_into_pip = false;
                        }
                    },
                );
                ui.add_space(16.0);
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
            let scroll_out = sa.show(ui, |ui| {
                let w = ui.available_width();
                let video_h = if self.theater_mode {
                    (ctx.content_rect().height() * 0.76).max(340.0)
                } else {
                    (w * 9.0 / 16.0).min(580.0)
                };
                if self.watch_scrolled_into_pip && self.watch.is_some() {
                    if draw_pip_dock_placeholder(ui, Vec2::new(w, video_h)) {
                        self.watch_scrolled_into_pip = false;
                        self.scroll_watch_to_top = true;
                    }
                } else {
                    self.draw_video_surface(ui, ctx, Vec2::new(w, video_h), false);
                }
                ui.add_space(12.0);
                self.draw_watch_metadata(ui);
                ui.add_space(16.0);
                self.draw_related_panel(ui, ctx);
            });
            let sy = scroll_out.state.clamped_offset().y;
            if !self.watch_scrolled_into_pip && sy > 28.0 && self.watch.is_some() {
                self.watch_scrolled_into_pip = true;
            } else if self.watch_scrolled_into_pip && sy <= 12.0 {
                self.watch_scrolled_into_pip = false;
            }
        }
    }

    fn draw_video_surface(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        size: Vec2,
        is_fullscreen: bool,
    ) {
        let radius = if is_fullscreen || self.theater_mode {
            0.0
        } else {
            12.0
        };
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        ui.painter().rect_filled(rect, radius, Color32::BLACK);

        if let Some(mpv) = &self.mpv {
            if self.watch.is_some() {
                ui.painter().add(mpv.paint_callback(rect));
            }
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
            return;
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

        // Buffering spinner
        let is_initial_wait =
            self.watch.is_some() && self.playback.video_h == 0 && !self.playback.paused;
        if self.watch_loading || self.playback.buffering || is_initial_wait {
            ctx.request_repaint();
            let label = if self.watch_loading {
                "Resolving stream...".to_owned()
            } else if self.playback.cache_duration > 0.05 {
                format!("Buffering · {:.1}s", self.playback.cache_duration)
            } else {
                "Buffering...".to_owned()
            };
            draw_buffering_overlay(ui, ctx, rect, radius, &label);
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

        // Overlaid bottom gradient + YouTube controls
        let mut clicked_overlay = false;
        let mut toggled_menu = false;
        if overlay_alpha > 0.01 {
            let scrim_h = 104.0_f32.min(rect.height() * 0.55);
            let scrim_rect = Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - scrim_h),
                rect.max,
            );
            paint_vertical_gradient(
                ui.painter(),
                scrim_rect,
                Color32::TRANSPARENT,
                Color32::from_black_alpha((220.0 * overlay_alpha).round() as u8),
            );
            let (interacted, toggled) = self.draw_overlaid_controls(ui, ctx, rect, overlay_alpha);
            clicked_overlay = interacted;
            toggled_menu = toggled;
        }

        // Floating Settings / CC popovers above the bottom-right controls
        let popup_open = self.player_menu != PlayerMenu::None;
        let popup_hit = self.draw_player_popovers(ctx, rect, toggled_menu);

        // Video surface click (toggle pause) & double click (fullscreen), ignoring control hits
        if !pointer_in_controls && !clicked_overlay && !popup_hit {
            if resp.double_clicked() {
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
    }

    fn draw_overlaid_controls(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        video_rect: Rect,
        alpha: f32,
    ) -> (bool, bool) {
        let mut any_interacted = false;
        let mut toggled_menu = false;
        let dur = self
            .playback
            .duration
            .max(self.watch.as_ref().map_or(0.0, |w| w.duration_secs));
        let mut pos = self.seeking_pos.unwrap_or(self.playback.time_pos);

        let pad_x = 14.0;
        let controls_h = 62.0;
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
        let cclip = cui.clip_rect().intersect(video_rect);
        cui.set_clip_rect(cclip);

        // 1. Slim interactive seekbar (animates thickness & knob on hover/drag)
        let bar_w = controls_rect.width();
        let (bar_rect, bar_resp) =
            cui.allocate_exact_size(Vec2::new(bar_w, 16.0), Sense::click_and_drag());
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
            Color32::from_white_alpha((70.0 * alpha).round() as u8),
        );

        if dur > 0.0 {
            if self.playback.cache_duration > 0.0 {
                let buf_frac = ((pos + self.playback.cache_duration) / dur).clamp(0.0, 1.0) as f32;
                let buf_rect = Rect::from_min_max(
                    track.min,
                    pos2(track.left() + track.width() * buf_frac, track.bottom()),
                );
                cui.painter().rect_filled(
                    buf_rect,
                    h * 0.5,
                    Color32::from_white_alpha((120.0 * alpha).round() as u8),
                );
            }

            for seg in &self.sponsor_segments {
                if !seg.fits(dur) {
                    continue;
                }
                let x0 = track.left() + (seg.start / dur).clamp(0.0, 1.0) as f32 * track.width();
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

            let frac = (pos / dur).clamp(0.0, 1.0) as f32;
            let fill = Rect::from_min_max(
                track.min,
                pos2(track.left() + track.width() * frac, track.bottom()),
            );
            cui.painter()
                .rect_filled(fill, h * 0.5, theme::YT_RED.gamma_multiply(alpha));

            // Subtle chapter dividers along the seekbar
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

            let knob_x = track.left() + track.width() * frac;
            let knob_r = 3.5 + 3.2 * bar_hot_t;
            cui.painter().circle_filled(
                pos2(knob_x, track.center().y),
                knob_r,
                theme::YT_RED.gamma_multiply(alpha),
            );

            if bar_hot {
                if let Some(ptr) = ctx.input(|i| i.pointer.hover_pos()) {
                    let hf = ((ptr.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
                    let h_secs = hf * dur;
                    let ch_title = self.watch.as_ref().and_then(|w| {
                        w.chapters
                            .iter()
                            .rfind(|c| c.start_secs <= h_secs)
                            .map(|c| c.title.as_str())
                    });
                    let tip = match ch_title {
                        Some(t) => format!("{} · {t}", format_duration(h_secs)),
                        None => format_duration(h_secs),
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
                    if let Some(mpv) = &self.mpv {
                        mpv.seek_abs(target);
                    }
                }
            }
        }

        // 2. Clean YouTube control row
        cui.add_space(4.0);
        let row_w = cui.available_width();
        let (row_rect, _) = cui.allocate_exact_size(Vec2::new(row_w, 38.0), Sense::hover());
        let right_w = 240.0_f32.min(row_w * 0.45);
        let left_w = (row_w - right_w - 6.0).max(160.0);

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
                        self.open_video(v);
                    } else {
                        self.open_video(next_vid);
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
            ui.label(
                RichText::new(format!(
                    "{} / {}",
                    format_duration(pos),
                    format_duration(dur)
                ))
                .font(theme::medium(12.2))
                .color(white),
            );

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

            // Theater mode (hidden in fullscreen)
            if !self.fullscreen {
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
                .is_some_and(|f| f.height >= 720);
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

            // Captions (CC) button
            let has_cc = self
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
                if self.selected_caption_idx.is_some() {
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
        let menu_w = 282.0_f32.min((video_rect.width() - 24.0).max(210.0));
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

                                // Playback speed pills
                                ui.label(
                                    RichText::new("Playback speed")
                                        .font(theme::medium(11.5))
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add_space(5.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
                                    for s in [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0] {
                                        let active = (self.speed - s).abs() < 0.01;
                                        let btn = egui::Button::new(
                                            RichText::new(format!("{s}x"))
                                                .font(theme::medium(11.5))
                                                .color(if active {
                                                    Color32::from_rgb(15, 15, 15)
                                                } else {
                                                    theme::TEXT
                                                }),
                                        )
                                        .min_size(vec2(38.0, 26.0))
                                        .fill(if active {
                                            Color32::from_rgb(241, 241, 241)
                                        } else {
                                            theme::CHIP
                                        })
                                        .corner_radius(7);
                                        if ui.add(btn).clicked() {
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
                                let mut picked_audio = None;
                                if let Some(w) = &self.watch {
                                    if !w.video_formats.is_empty() {
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
                                            .max_height(145.0)
                                            .show(ui, |ui| {
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
                                ui.add_space(4.0);
                                if popover_option_row(
                                    ui,
                                    "Stats for Nerds (Ctrl+I)",
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
                                        RichText::new("Subtitles / CC")
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
                                let mut picked_cap: Option<Option<usize>> = None;
                                if let Some(w) = &self.watch {
                                    ScrollArea::vertical()
                                        .id_salt("popover_cc_scroll")
                                        .max_height(190.0)
                                        .show(ui, |ui| {
                                            if popover_option_row(
                                                ui,
                                                "Off",
                                                self.selected_caption_idx.is_none(),
                                            ) {
                                                picked_cap = Some(None);
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
                                                    picked_cap = Some(Some(i));
                                                }
                                            }
                                        });
                                    if let Some(choice) = picked_cap {
                                        self.selected_caption_idx = choice;
                                        let url = choice
                                            .and_then(|i| w.captions.get(i))
                                            .map(|c| c.vtt_url.as_str());
                                        if let Some(mpv) = &self.mpv {
                                            mpv.set_subtitle(url);
                                        }
                                        self.player_menu = PlayerMenu::None;
                                        interacted = true;
                                    }
                                }
                            }
                            PlayerMenu::None => {}
                        }
                    });
            });

        // Close popover cleanly when clicking anywhere outside it
        let clicked_outside = !just_toggled
            && ctx.input(|i| i.pointer.any_pressed())
            && ctx
                .input(|i| i.pointer.interact_pos())
                .is_none_or(|p| !area_resp.response.rect.contains(p));
        if clicked_outside {
            self.player_menu = PlayerMenu::None;
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
            }
            return;
        };

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
                    ui.label(
                        RichText::new(desc)
                            .font(theme::regular(13.0))
                            .color(Color32::from_rgb(215, 215, 215)),
                    );
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
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.comments.is_empty() {
                    "Comments".to_owned()
                } else {
                    format!("Comments ({})", self.comments.len())
                })
                .font(theme::semibold(16.0)),
            );
            if self.comments_loading {
                ui.add_space(8.0);
                ui.spinner();
            }
        });
        ui.add_space(8.0);

        if self.comments.is_empty() && !self.comments_loading {
            if let Some(tok) = w.comments_token.clone() {
                if ui.button("Load comments").clicked() {
                    self.comments_loading = true;
                    let _ = self.backend.tx.send(Request::LoadComments {
                        video_id: w.id.clone(),
                        continuation: tok,
                        append: false,
                    });
                }
            } else {
                ui.label(
                    RichText::new("Comments are unavailable for this video.")
                        .font(theme::regular(13.0))
                        .color(theme::TEXT_DIM),
                );
            }
        } else {
            for c in &self.comments {
                ui.horizontal_top(|ui| {
                    draw_avatar(ui, c.avatar_url.as_deref(), &c.author, 34.0);
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
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
                        ui.label(
                            RichText::new(&c.text)
                                .font(theme::regular(13.0))
                                .color(theme::TEXT),
                        );
                        if !c.likes.is_empty() {
                            ui.label(
                                RichText::new(format!("{} likes", c.likes))
                                    .font(theme::regular(11.5))
                                    .color(theme::TEXT_DIM),
                            );
                        }
                    });
                });
                ui.add_space(10.0);
            }
            if !self.comments_loading {
                if let Some(tok) = self.comments_next_token.clone() {
                    if super::cards::pill_button(ui, icon::CHEVRON_DOWN, "Load more comments")
                        .clicked()
                    {
                        self.comments_loading = true;
                        let _ = self.backend.tx.send(Request::LoadComments {
                            video_id: w.id.clone(),
                            continuation: tok,
                            append: true,
                        });
                    }
                }
            }
        }
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
                let (sub_w, sub_fill, sub_text_col) = if w.is_subscribed {
                    (114.0, theme::CHIP, theme::TEXT)
                } else {
                    (
                        94.0,
                        Color32::from_rgb(241, 241, 241),
                        Color32::from_rgb(15, 15, 15),
                    )
                };
                let (srect, sresp) =
                    ui.allocate_exact_size(Vec2::new(sub_w, 34.0), Sense::click());
                if sresp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let fill = if sresp.hovered() {
                    if w.is_subscribed {
                        theme::CHIP_HOVER
                    } else {
                        Color32::from_rgb(217, 217, 217)
                    }
                } else {
                    sub_fill
                };
                ui.painter().rect_filled(srect, 17.0, fill);
                if w.is_subscribed {
                    theme::draw_bell(
                        ui.painter(),
                        pos2(srect.left() + 18.0, srect.center().y),
                        14.0,
                        sub_text_col,
                    );
                    ui.painter().text(
                        pos2(srect.left() + 32.0, srect.center().y),
                        Align2::LEFT_CENTER,
                        "Subscribed",
                        theme::semibold(13.0),
                        sub_text_col,
                    );
                } else {
                    ui.painter().text(
                        srect.center(),
                        Align2::CENTER_CENTER,
                        "Subscribe",
                        theme::semibold(13.0),
                        sub_text_col,
                    );
                }
                if sresp.clicked() {
                    let _ = self.backend.tx.send(Request::SetSubscribe {
                        channel_id: cid,
                        subscribe: !w.is_subscribed,
                    });
                }
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
            let output_dir = self.custom_download_dir();

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
                        app.toast(format!("Downloading audio: {}", w.title));
                        let _ = app.backend.tx.send(Request::DownloadVideo {
                            video_id: w.id.clone(),
                            title: w.title.clone(),
                            audio_only: true,
                            max_height: app.preferred_quality.max_height(),
                            output_dir: output_dir.clone(),
                        });
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
                        watched_percent: None,
                        resume_secs: None,
                        playlist_browse_id: None,
                    });
                }
            };

            let draw_download_pill = |ui: &mut egui::Ui, app: &mut Self| {
                if super::cards::pill_button(ui, icon::DOWNLOAD, "Download")
                    .on_hover_text(format!("Download MP4 ({})", app.preferred_quality.label()))
                    .clicked()
                {
                    app.toast(format!("Downloading video: {}", w.title));
                    let _ = app.backend.tx.send(Request::DownloadVideo {
                        video_id: w.id.clone(),
                        title: w.title.clone(),
                        audio_only: false,
                        max_height: app.preferred_quality.max_height(),
                        output_dir: output_dir.clone(),
                    });
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
                let dislike_w = 42.0;
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
                theme::draw_icon_centered(
                    ui.painter(),
                    dislike_rect.center(),
                    icon::THUMBS_DOWN,
                    15.0,
                    dislike_col,
                );

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
        let has_chat = w.live_chat_token.is_some() || !self.live_chat.is_empty();
        let has_queue = !self.queue.is_empty();
        if has_chat || has_queue {
            ui.horizontal(|ui| {
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
                            RichText::new("Live Chat").font(theme::semibold(14.0)),
                        )
                        .clicked()
                {
                    self.right_tab = RightTab::LiveChat;
                }
            });
            ui.add_space(8.0);
        } else {
            ui.label(RichText::new("Up Next").font(theme::semibold(15.5)));
            ui.add_space(8.0);
        }

        if has_queue && self.right_tab == RightTab::Queue {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} queued", self.queue.len()))
                        .font(theme::regular(12.0))
                        .color(theme::TEXT_DIM),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button(format!("{} Clear", icon::TRASH_2))
                        .clicked()
                    {
                        self.queue.clear();
                        self.right_tab = RightTab::UpNext;
                    }
                });
            });
            ui.add_space(6.0);
            let q_items = self.queue.clone();
            let mut play_idx = None;
            let mut remove_idx = None;
            ScrollArea::vertical().id_salt("queue_scroll").show(ui, |ui| {
                for (qi, item) in q_items.iter().enumerate() {
                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 82.0),
                        Sense::click(),
                    );
                    if row_resp.hovered() {
                        ui.painter().rect_filled(row_rect, 8.0, theme::SURFACE);
                    }
                    let inner = row_rect.shrink(4.0);
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                    child.horizontal(|ui| {
                        let thumb_size = Vec2::new(132.0, 74.0);
                        draw_thumbnail(
                            ui,
                            &item.thumbnail_url,
                            &item.duration_text,
                            thumb_size,
                            8.0,
                            item.watched_percent,
                        );
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.set_max_width((inner.width() - 168.0).max(60.0));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&item.title)
                                        .font(theme::medium(12.5))
                                        .color(theme::TEXT),
                                )
                                .truncate(),
                            );
                            ui.label(
                                RichText::new(&item.channel)
                                    .font(theme::regular(11.5))
                                    .color(theme::TEXT_DIM),
                            );
                        });
                        if ui.small_button(icon::X.to_string()).clicked() {
                            remove_idx = Some(qi);
                        }
                    });
                    if row_resp.clicked() {
                        play_idx = Some(qi);
                    }
                    ui.add_space(4.0);
                }
            });
            if let Some(i) = remove_idx {
                self.queue.remove(i);
                if self.queue.is_empty() {
                    self.right_tab = RightTab::UpNext;
                }
            } else if let Some(i) = play_idx {
                let next = self.queue.remove(i);
                self.open_video(next);
            }
            return;
        }

        if has_chat && self.right_tab == RightTab::LiveChat {
            Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(12)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_salt("live_chat_scroll")
                        .stick_to_bottom(true)
                        .max_height((ui.available_height() - 16.0).max(240.0))
                        .show(ui, |ui| {
                            if self.live_chat.is_empty() {
                                ui.label(
                                    RichText::new("Waiting for live chat messages...")
                                        .font(theme::regular(12.5))
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            for m in &self.live_chat {
                                ui.horizontal_wrapped(|ui| {
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
                                    ui.label(
                                        RichText::new(&m.text)
                                            .font(theme::regular(12.5))
                                            .color(theme::TEXT),
                                    );
                                });
                                ui.add_space(4.0);
                            }
                        });
                });
            return;
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
                if row_hover_resp.clicked() && right_action.is_none() {
                    right_action = Some(GridAction::Open(item));
                }
                ui.add_space(4.0);
            }
        });
        if let Some(act) = right_action {
            self.handle_grid_action(ctx, act, |_| {});
        }
    }

    pub(super) fn draw_mini_player(&mut self, ctx: &egui::Context) {
        let Some(w) = self.watch.as_ref() else {
            return;
        };
        let title = w.title.clone();
        let channel = w.channel.clone();
        let dur = self.playback.duration.max(w.duration_secs).max(0.0);
        let mut pos = self.seeking_pos.unwrap_or(self.playback.time_pos).max(0.0);
        let is_live = w.is_live;

        let screen = ctx.content_rect();
        let default_x = (screen.right() - 360.0).max(20.0);
        let default_y = (screen.bottom() - 280.0).max(60.0);

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
                self.pip_snap_target = None;
            } else {
                self.pip_pos = Some(next);
                ctx.request_repaint();
            }
        }

        let mut area = egui::Area::new(egui::Id::unique("mini_player"))
            .default_pos(pos2(default_x, default_y))
            .movable(true)
            .constrain(true)
            .order(egui::Order::Foreground);
        if self.pip_snap_target.is_some() {
            if let Some(p) = self.pip_pos {
                area = area.current_pos(p);
            }
        }

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
                            ui.painter().add(mpv.paint_callback(vrect));
                        }
                        // Round top corners cleanly over the OpenGL blit using corner masks matching Frame fill
                        paint_rounded_top_corners_mask(ui.painter(), vrect, 11.0, theme::SURFACE);

                        if vresp.clicked() {
                            self.page = Page::Watch;
                            self.watch_scrolled_into_pip = false;
                            self.scroll_watch_to_top = true;
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

                        // Compact bottom bar with strictly bounded text column + evenly spaced buttons
                        Frame::new()
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                let inner_w = pip_w - 20.0;
                                let btns_w = 112.0;
                                let text_w = (inner_w - btns_w - 8.0).max(110.0);
                                let sub_line = if is_live {
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
                                                        .font(theme::semibold(12.0))
                                                        .color(theme::TEXT),
                                                )
                                                .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(sub_line)
                                                        .font(theme::regular(11.0))
                                                        .color(theme::TEXT_DIM),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        if icon_circle_button(ui, icon::X, 15.0, "Close player") {
                                            self.backend.cursor.stop();
                                            if let Some(m) = &self.mpv {
                                                m.stop();
                                            }
                                            self.watch = None;
                                            self.watch_scrolled_into_pip = false;
                                            self.clear_last_watched();
                                        }
                                        let (mrect, mresp) = ui.allocate_exact_size(
                                            Vec2::splat(34.0),
                                            Sense::click(),
                                        );
                                        paint_hover_circle(ui, mrect, &mresp, 1.0);
                                        theme::draw_miniplayer_icon(
                                            ui.painter(),
                                            mrect.center(),
                                            16.0,
                                            Color32::WHITE,
                                        );
                                        if mresp.on_hover_text("Expand").clicked() {
                                            self.page = Page::Watch;
                                            self.watch_scrolled_into_pip = false;
                                            self.scroll_watch_to_top = true;
                                        }
                                        let play_ico = if self.playback.paused {
                                            icon::PLAY
                                        } else {
                                            icon::PAUSE
                                        };
                                        if icon_circle_button(ui, play_ico, 15.0, "Play/Pause") {
                                            if let Some(m) = &self.mpv {
                                                m.toggle_pause();
                                            }
                                        }
                                    });
                                });
                            });
                    });
            });

        // Snap PiP gently to nearest screen corner when released near edges
        if area_resp.response.dragged() {
            self.pip_snap_target = None;
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
    }
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

fn paint_rounded_top_corners_mask(
    painter: &egui::Painter,
    rect: Rect,
    r: f32,
    bg: Color32,
) {
    let steps = 8;
    for is_right in [false, true] {
        let mut pts = Vec::with_capacity(steps + 2);
        let corner = if is_right {
            rect.right_top()
        } else {
            rect.left_top()
        };
        let cx = if is_right {
            rect.right() - r
        } else {
            rect.left() + r
        };
        let cy = rect.top() + r;
        pts.push(corner);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let ang = if is_right {
                -std::f32::consts::FRAC_PI_2 * (1.0 - t)
            } else {
                std::f32::consts::PI + std::f32::consts::FRAC_PI_2 * t
            };
            pts.push(pos2(cx + r * ang.cos(), cy + r * ang.sin()));
        }
        painter.add(egui::Shape::convex_polygon(pts, bg, Stroke::NONE));
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

fn clamp_two_lines(ui: &egui::Ui, text: &str, font: egui::FontId, max_w: f32) -> String {
    let galley = ui
        .painter()
        .layout(text.to_owned(), font.clone(), theme::TEXT, max_w);
    if galley.rows.len() <= 2 {
        return text.to_owned();
    }
    let two_line_chars: usize = galley.rows[..2]
        .iter()
        .map(|r| r.glyphs.len())
        .sum();
    let mut out: String = text.chars().take(two_line_chars.saturating_sub(2)).collect();
    while !out.is_empty() {
        let cand = format!("{out}…");
        let g = ui.painter().layout(cand, font.clone(), theme::TEXT, max_w);
        if g.rows.len() <= 2 {
            break;
        }
        out.pop();
    }
    if out.is_empty() {
        "…".to_owned()
    } else {
        format!("{out}…")
    }
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
