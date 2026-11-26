use egui::{
    Align, Align2, Color32, CursorIcon, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense,
    Stroke, Vec2, pos2, vec2,
};

use crate::backend::Request;
use crate::model::{PlaylistOption, VideoItem};
use crate::theme::{self, icon};

use super::YtApp;

pub(super) enum GridAction {
    Open(VideoItem),
    OpenChannel { channel_id: String, name: String },
    PlayNext(VideoItem),
    AddQueue(VideoItem),
    ToggleWatchLater(VideoItem),
    OpenPlaylistPicker(VideoItem),
    Download { item: VideoItem, audio_only: bool },
    CopyUrl(String),
    RemoveFromHistory(VideoItem),
    LoadMore,
}

impl YtApp {
    pub(super) fn draw_playlist_picker_modal(&mut self, ctx: &egui::Context) {
        let Some(video) = self.playlist_modal_video.clone() else {
            return;
        };
        let mut open = true;
        let mut close_now = false;
        let win_resp = egui::Window::new("Save video to...")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(370.0, 410.0))
            .frame(
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(14)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 20,
                        spread: 2,
                        color: Color32::from_black_alpha(160),
                    })
                    .inner_margin(Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(&video.title)
                            .font(theme::medium(12.5))
                            .color(theme::TEXT_DIM),
                    )
                    .truncate(),
                );
                ui.add_space(10.0);

                let in_wl = self.saved.iter().any(|x| x.id == video.id);
                let (wl_rect, wl_resp) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 36.0),
                    Sense::click(),
                );
                if wl_resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let wl_fill = if in_wl {
                    theme::YT_BLUE.gamma_multiply(0.18)
                } else if wl_resp.hovered() {
                    theme::CHIP_HOVER
                } else {
                    theme::CHIP
                };
                ui.painter().rect_filled(wl_rect, 9.0, wl_fill);
                let wl_ico = if in_wl { icon::CHECK } else { icon::LIBRARY };
                let wl_col = if in_wl { theme::YT_BLUE } else { theme::TEXT };
                theme::draw_icon_centered(
                    ui.painter(),
                    pos2(wl_rect.left() + 20.0, wl_rect.center().y),
                    wl_ico,
                    15.0,
                    wl_col,
                );
                ui.painter().text(
                    pos2(wl_rect.left() + 38.0, wl_rect.center().y),
                    Align2::LEFT_CENTER,
                    if in_wl {
                        "Watch Later (Saved)"
                    } else {
                        "Watch Later"
                    },
                    theme::medium(13.0),
                    theme::TEXT,
                );
                if wl_resp.clicked() {
                    if self.signed_in {
                        let _ = self.backend.tx.send(Request::SetWatchLater {
                            video_id: video.id.clone(),
                            add: !in_wl,
                        });
                    }
                    if in_wl {
                        self.saved.retain(|x| x.id != video.id);
                        self.toast("Removed from Watch Later");
                    } else {
                        self.saved.insert(0, video.clone());
                        self.toast("Saved to Watch Later");
                    }
                }

                ui.add_space(8.0);
                if self.signed_in {
                    ui.add_sized(
                        Vec2::new(ui.available_width(), 28.0),
                        egui::TextEdit::singleline(&mut self.playlist_filter)
                            .hint_text("Search playlists..."),
                    );
                    ui.add_space(6.0);
                    if self.playlist_options_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("Loading your YouTube playlists...")
                                    .font(theme::regular(12.5))
                                    .color(theme::TEXT_DIM),
                            );
                        });
                        ui.add_space(6.0);
                    }
                    let q_filter = self.playlist_filter.trim().to_ascii_lowercase();
                    ScrollArea::vertical()
                        .id_salt("playlist_picker_list")
                        .max_height(195.0)
                        .show(ui, |ui| {
                            let non_wl: Vec<(usize, PlaylistOption)> = self
                                .playlist_options
                                .iter()
                                .enumerate()
                                .filter(|(_, p)| {
                                    p.id != "WL"
                                        && (q_filter.is_empty()
                                            || p.title.to_ascii_lowercase().contains(&q_filter))
                                })
                                .map(|(i, p)| (i, p.clone()))
                                .collect();
                            if non_wl.is_empty() && !self.playlist_options_loading {
                                ui.label(
                                    RichText::new("No matching playlists.")
                                        .font(theme::regular(12.5))
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            for (idx, opt) in non_wl {
                                let row_w = ui.available_width();
                                let (rect, resp) = ui.allocate_exact_size(
                                    Vec2::new(row_w, 38.0),
                                    Sense::click(),
                                );
                                let fill = if opt.is_added {
                                    theme::YT_BLUE.gamma_multiply(0.18)
                                } else if resp.hovered() {
                                    theme::CHIP_HOVER
                                } else {
                                    theme::CHIP
                                };
                                ui.painter().rect_filled(rect, 8.0, fill);

                                let thumb_rect = Rect::from_min_size(
                                    pos2(rect.left() + 6.0, rect.center().y - 13.0),
                                    Vec2::new(46.0, 26.0),
                                );
                                ui.painter().rect_filled(
                                    thumb_rect,
                                    5.0,
                                    Color32::from_rgb(38, 38, 44),
                                );
                                if let Some(u) = &opt.thumbnail_url {
                                    theme::paint_image_rounded(ui, u, thumb_rect, 5);
                                } else {
                                    theme::draw_icon_centered(
                                        ui.painter(),
                                        thumb_rect.center(),
                                        icon::LIST_MUSIC,
                                        13.0,
                                        theme::TEXT_DIM,
                                    );
                                }

                                let ico = if opt.is_added {
                                    icon::CHECK
                                } else {
                                    icon::LIST_PLUS
                                };
                                theme::draw_icon_centered(
                                    ui.painter(),
                                    pos2(rect.right() - 18.0, rect.center().y),
                                    ico,
                                    15.0,
                                    if opt.is_added {
                                        theme::YT_BLUE
                                    } else {
                                        theme::TEXT_DIM
                                    },
                                );

                                let title_rect = Rect::from_min_max(
                                    pos2(thumb_rect.right() + 10.0, rect.top() + 4.0),
                                    pos2(rect.right() - 34.0, rect.bottom() - 4.0),
                                );
                                let mut t_ui =
                                    ui.new_child(egui::UiBuilder::new().max_rect(title_rect));
                                let clip = t_ui.clip_rect().intersect(title_rect);
                                t_ui.set_clip_rect(clip);
                                t_ui.with_layout(
                                    Layout::left_to_right(Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&opt.title)
                                                    .font(theme::medium(13.0))
                                                    .color(theme::TEXT),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );

                                if resp.clicked() {
                                    let new_added = !opt.is_added;
                                    self.playlist_options[idx].is_added = new_added;
                                    let _ = self.backend.tx.send(Request::EditPlaylist {
                                        playlist_id: opt.id,
                                        playlist_title: opt.title,
                                        video_id: video.id.clone(),
                                        add: new_added,
                                    });
                                }
                                ui.add_space(4.0);
                            }
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Create new playlist")
                            .font(theme::semibold(12.5))
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let resp = ui.add_sized(
                            Vec2::new((ui.available_width() - 78.0).max(120.0), 32.0),
                            egui::TextEdit::singleline(&mut self.new_playlist_title)
                                .hint_text("Playlist title"),
                        );
                        let submit = ui
                            .add_sized(
                                Vec2::new(68.0, 32.0),
                                egui::Button::new(
                                    RichText::new("Create")
                                        .font(theme::semibold(12.5))
                                        .color(Color32::from_rgb(15, 15, 15)),
                                )
                                .fill(theme::YT_BLUE)
                                .corner_radius(8),
                            )
                            .clicked()
                            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if submit {
                            let title = self.new_playlist_title.trim().to_owned();
                            if !title.is_empty() {
                                let _ = self.backend.tx.send(Request::CreatePlaylist {
                                    title,
                                    video_ids: vec![video.id.clone()],
                                });
                                self.new_playlist_title.clear();
                                close_now = true;
                            }
                        }
                    });
                } else {
                    ui.label(
                        RichText::new("Sign in with Google to save to your YouTube playlists.")
                            .font(theme::regular(12.5))
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        let clicked_outside = self
            .playlist_modal_opened_at
            .is_some_and(|t| t.elapsed() > std::time::Duration::from_millis(120))
            && ctx.input(|i| i.pointer.any_pressed())
            && win_resp.as_ref().is_some_and(|r| {
                ctx.input(|i| i.pointer.interact_pos())
                    .is_some_and(|p| !r.response.rect.contains(p))
            });
        if !open || close_now || clicked_outside {
            self.playlist_modal_video = None;
            self.playlist_modal_opened_at = None;
        }
    }
}

pub(super) fn draw_video_grid(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    loading: bool,
    has_more: bool,
) -> Option<GridAction> {
    draw_video_grid_opts(ui, heading, items, loading, has_more, false)
}

pub(super) fn draw_video_grid_opts(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    loading: bool,
    has_more: bool,
    show_remove_history: bool,
) -> Option<GridAction> {
    let mut action = None;
    if !heading.is_empty() || loading {
        ui.horizontal(|ui| {
            if !heading.is_empty() {
                ui.label(RichText::new(heading).font(theme::bold(20.0)));
            }
            if loading {
                ui.add_space(8.0);
                ui.spinner();
            }
        });
        ui.add_space(10.0);
    }

    if items.is_empty() && !loading {
        ui.label(
            RichText::new("No videos found.")
                .font(theme::regular(14.0))
                .color(theme::TEXT_DIM),
        );
        return None;
    }

    let scroll_out = ScrollArea::vertical().id_salt(heading).show(ui, |ui| {
        let avail = (ui.available_width() - 8.0).max(200.0);
        let cols = ((avail / 305.0).floor() as usize).clamp(1, 5);
        let spacing = 16.0;
        let card_w = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).max(180.0);
        let thumb_h = card_w * 9.0 / 16.0;

        for row in items.chunks(cols) {
            ui.horizontal_top(|ui| {
                for (col_idx, item) in row.iter().enumerate() {
                    let card_h = thumb_h + 92.0;
                    let (card_rect, card_resp) =
                        ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                    let hovered = card_resp.hovered() || ui.rect_contains_pointer(card_rect);
                    let hover_t = ui.ctx().animate_bool_with_time(
                        egui::Id::unique(("card_hover", &item.id)),
                        hovered,
                        0.15,
                    );
                    if hovered {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if hover_t > 0.01 {
                        // Smooth YouTube-style ambient card glow
                        ui.painter().rect_filled(
                            card_rect.expand(6.0),
                            14.0,
                            Color32::from_rgba_unmultiplied(40, 32, 29, (215.0 * hover_t) as u8),
                        );
                    }

                    let mut card_ui = ui.new_child(egui::UiBuilder::new().max_rect(card_rect));
                    let cclip = card_ui.clip_rect().intersect(card_rect);
                    card_ui.set_clip_rect(cclip);
                    let mut child_clicked_override = false;
                    card_ui.vertical(|ui| {
                        draw_thumbnail_with_hover(
                            ui,
                            &item.thumbnail_url,
                            &item.duration_text,
                            Vec2::new(card_w, thumb_h),
                            12.0,
                            item.watched_percent,
                            hover_t,
                        );
                        ui.add_space(10.0);
                        ui.horizontal_top(|ui| {
                            let avatar_r = ui
                                .horizontal(|ui| {
                                    draw_avatar(
                                        ui,
                                        item.channel_avatar_url.as_deref(),
                                        &item.channel,
                                        34.0,
                                    );
                                })
                                .response;
                            if item.channel_id.is_some()
                                && avatar_r.interact(Sense::click()).clicked()
                            {
                                if let Some(cid) = item.channel_id.clone() {
                                    action = Some(GridAction::OpenChannel {
                                        channel_id: cid,
                                        name: item.channel.clone(),
                                    });
                                    child_clicked_override = true;
                                }
                            }
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((card_w - 72.0).max(90.0));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&item.title)
                                            .font(theme::semibold(13.8))
                                            .color(theme::TEXT),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(2.0);
                                let ch_lbl = ui.add(
                                    egui::Label::new(
                                        RichText::new(&item.channel)
                                            .font(theme::regular(12.2))
                                            .color(theme::TEXT_DIM),
                                    )
                                    .truncate()
                                    .sense(Sense::click()),
                                );
                                if ch_lbl.hovered() && item.channel_id.is_some() {
                                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                }
                                if ch_lbl.clicked() {
                                    if let Some(cid) = item.channel_id.clone() {
                                        action = Some(GridAction::OpenChannel {
                                            channel_id: cid,
                                            name: item.channel.clone(),
                                        });
                                        child_clicked_override = true;
                                    }
                                }
                                let meta = join_meta(&item.views_text, &item.published_text);
                                if !meta.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(meta)
                                                .font(theme::regular(12.0))
                                                .color(theme::TEXT_DIM),
                                        )
                                        .truncate(),
                                    );
                                }
                            });

                            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                                let menu_id = ui.scope_id().with(&item.id);
                                ui.push_id(menu_id, |ui| {
                                    let (mrect, mresp) = ui.allocate_exact_size(
                                        Vec2::splat(28.0),
                                        Sense::click(),
                                    );
                                    let mhov = ui.ctx().animate_bool_with_time(
                                        mresp.id,
                                        mresp.hovered(),
                                        0.12,
                                    );
                                    if mresp.hovered() {
                                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                    }
                                    if mhov > 0.01 {
                                        ui.painter().circle_filled(
                                            mrect.center(),
                                            14.0,
                                            Color32::from_white_alpha((24.0 * mhov) as u8),
                                        );
                                    }
                                    let icon_col = theme::lerp_color(
                                        theme::TEXT_FAINT,
                                        Color32::WHITE,
                                        hover_t.max(mhov).clamp(0.0, 1.0),
                                    );
                                    theme::draw_icon_centered(
                                        ui.painter(),
                                        mrect.center(),
                                        icon::ELLIPSIS_VERTICAL,
                                        15.0,
                                        icon_col,
                                    );
                                    egui::Popup::menu(&mresp).show(|ui| {
                                        draw_video_context_menu(ui, item, &mut action, false);
                                        if show_remove_history
                                            && ui
                                                .button(format!(
                                                    "{}  Remove from history",
                                                    icon::TRASH_2
                                                ))
                                                .clicked()
                                        {
                                            action = Some(GridAction::RemoveFromHistory(item.clone()));
                                            ui.close();
                                        }
                                    });
                                    if mresp.hovered() || mresp.clicked() {
                                        child_clicked_override = true;
                                    }
                                });
                            });
                        });
                    });

                    card_resp.context_menu(|ui| {
                        draw_video_context_menu(ui, item, &mut action, false);
                        if show_remove_history
                            && ui
                                .button(format!("{}  Remove from history", icon::TRASH_2))
                                .clicked()
                        {
                            action = Some(GridAction::RemoveFromHistory(item.clone()));
                            ui.close();
                        }
                    });

                    let card_hover_resp = if item.title.chars().count() > 46 {
                        card_resp.on_hover_text(&item.title)
                    } else {
                        card_resp
                    };

                    if card_hover_resp.clicked() && !child_clicked_override && action.is_none() {
                        action = Some(GridAction::Open(item.clone()));
                    }
                    if col_idx + 1 < row.len() {
                        ui.add_space(spacing);
                    }
                }
            });
            ui.add_space(18.0);
        }

        if has_more && !loading {
            ui.add_space(8.0);
            if ui
                .add(
                    egui::Button::new(RichText::new("Load more").font(theme::medium(13.0)))
                        .fill(theme::CHIP)
                        .corner_radius(18),
                )
                .clicked()
            {
                action = Some(GridAction::LoadMore);
            }
        }
    });

    if has_more && !loading && action.is_none() {
        let remaining = scroll_out.content_size.y
            - (scroll_out.state.clamped_offset().y + scroll_out.inner_rect.height());
        if remaining < 450.0 {
            action = Some(GridAction::LoadMore);
        }
    }

    action
}

pub(super) fn draw_video_context_menu(
    ui: &mut egui::Ui,
    item: &VideoItem,
    action: &mut Option<GridAction>,
    show_play_next: bool,
) {
    if show_play_next
        && ui
            .button(format!("{}  Play Next", icon::PLAY))
            .clicked()
    {
        *action = Some(GridAction::PlayNext(item.clone()));
        ui.close();
    }
    if ui
        .button(format!("{}  Add to Queue", icon::LIST_PLUS))
        .clicked()
    {
        *action = Some(GridAction::AddQueue(item.clone()));
        ui.close();
    }
    if ui
        .button(format!("{}  Save to Watch Later", icon::LIBRARY))
        .clicked()
    {
        *action = Some(GridAction::ToggleWatchLater(item.clone()));
        ui.close();
    }
    if ui
        .button(format!("{}  Save to playlist...", icon::LIST_MUSIC))
        .clicked()
    {
        *action = Some(GridAction::OpenPlaylistPicker(item.clone()));
        ui.close();
    }
    if let Some(cid) = item.channel_id.clone() {
        if ui
            .button(format!("{}  Go to channel", icon::USER))
            .clicked()
        {
            *action = Some(GridAction::OpenChannel {
                channel_id: cid,
                name: item.channel.clone(),
            });
            ui.close();
        }
    }
    ui.separator();
    if ui
        .button(format!("{}  Download MP4", icon::DOWNLOAD))
        .clicked()
    {
        *action = Some(GridAction::Download {
            item: item.clone(),
            audio_only: false,
        });
        ui.close();
    }
    if ui
        .button(format!("{}  Download Audio", icon::HEADPHONES))
        .clicked()
    {
        *action = Some(GridAction::Download {
            item: item.clone(),
            audio_only: true,
        });
        ui.close();
    }
    if ui.button(format!("{}  Copy URL", icon::COPY)).clicked() {
        *action = Some(GridAction::CopyUrl(item.id.clone()));
        ui.close();
    }
}

pub(super) fn pill_button(ui: &mut egui::Ui, ico: char, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(format!("{ico}  {label}")).font(theme::medium(12.5)))
            .min_size(Vec2::new(0.0, 34.0))
            .fill(theme::CHIP)
            .corner_radius(17),
    )
}

pub(super) fn icon_circle_button(
    ui: &mut egui::Ui,
    ico: char,
    size: f32,
    tooltip: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
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
            Color32::from_white_alpha((26.0 * hover_t) as u8),
        );
    }
    theme::draw_icon_centered(ui.painter(), rect.center(), ico, size, Color32::WHITE);
    resp.on_hover_text(tooltip).clicked()
}

pub(super) fn settings_card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(12)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).font(theme::semibold(15.5)));
            ui.add_space(10.0);
            add(ui);
        });
}

pub(super) fn draw_avatar(ui: &mut egui::Ui, url: Option<&str>, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if let Some(u) = url {
        theme::paint_image_rounded(ui, u, rect, (size * 0.5).round() as u8);
    } else {
        let initial = name.chars().next().unwrap_or('Y').to_ascii_uppercase();
        ui.painter()
            .circle_filled(rect.center(), size * 0.5, Color32::from_rgb(56, 56, 64));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            initial.to_string(),
            theme::semibold(size * 0.44),
            Color32::WHITE,
        );
    }
}

pub(super) fn draw_thumbnail(
    ui: &mut egui::Ui,
    url: &str,
    duration: &str,
    size: Vec2,
    radius: f32,
    watched_percent: Option<f32>,
) {
    draw_thumbnail_with_hover(ui, url, duration, size, radius, watched_percent, 0.0);
}

fn draw_thumbnail_with_hover(
    ui: &mut egui::Ui,
    url: &str,
    duration: &str,
    size: Vec2,
    radius: f32,
    watched_percent: Option<f32>,
    hover_t: f32,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

    // Slightly flatten radius on hover just like YouTube
    let cur_radius = (radius - 4.0 * hover_t).max(4.0);
    theme::paint_image_rounded(ui, url, rect, cur_radius.round() as u8);

    if hover_t > 0.02 {
        let badge_alpha = (hover_t * 210.0).round() as u8;
        let play_pill = Rect::from_center_size(rect.center(), vec2(44.0, 32.0));
        ui.painter().rect_filled(
            play_pill,
            16.0,
            Color32::from_black_alpha((hover_t * 175.0).round() as u8),
        );
        theme::draw_icon_centered(
            ui.painter(),
            play_pill.center(),
            icon::PLAY,
            16.0,
            Color32::from_white_alpha(badge_alpha),
        );
    }

    if let Some(pct) = watched_percent.filter(|&p| p > 0.01) {
        let bar_h = 4.0;
        let track = Rect::from_min_max(
            pos2(rect.left(), rect.bottom() - bar_h),
            pos2(rect.right(), rect.bottom()),
        );
        ui.painter()
            .rect_filled(track, 0.0, Color32::from_white_alpha(75));
        let fill_w = (size.x * pct.clamp(0.0, 1.0)).max(3.0);
        let fill = Rect::from_min_size(track.min, Vec2::new(fill_w, bar_h));
        ui.painter().rect_filled(fill, 0.0, Color32::from_rgb(255, 0, 0));
    }

    if !duration.is_empty() {
        let is_live = duration.eq_ignore_ascii_case("LIVE");
        let bg = if is_live {
            theme::YT_RED
        } else {
            Color32::from_black_alpha(210)
        };
        let galley = ui.painter().layout_no_wrap(
            duration.to_owned(),
            theme::semibold(11.0),
            Color32::WHITE,
        );
        let pad = vec2(6.0, 2.5);
        let badge_size = galley.size() + pad * 2.0;
        let badge_rect =
            Rect::from_min_size(rect.max - badge_size - vec2(7.0, 7.0), badge_size);
        ui.painter().rect_filled(badge_rect, 4.0, bg);
        ui.painter()
            .galley(badge_rect.min + pad, galley, Color32::WHITE);
    }
}

pub(super) fn join_meta(views: &str, published: &str) -> String {
    match (views.is_empty(), published.is_empty()) {
        (false, false) => format!("{views} · {published}"),
        (false, true) => views.to_owned(),
        (true, false) => published.to_owned(),
        (true, true) => String::new(),
    }
}
