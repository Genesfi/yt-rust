use egui::{
    Align, Align2, Color32, CursorIcon, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense,
    Stroke, Vec2, pos2, vec2,
};

use crate::backend::Request;
use crate::model::{ChannelNotificationParams, NotificationLevel, PlaylistOption, VideoItem};
use crate::mpv::MpvRenderHandle;
use crate::theme::{self, icon};

use super::{YtApp, format_duration};

#[derive(Clone, Default)]
pub(super) struct HoverPreviewInput {
    pub enabled: bool,
    pub active_id: Option<String>,
    pub loading_id: Option<String>,
    pub muted: bool,
    pub time_pos: f64,
    pub duration: f64,
    pub has_frame: bool,
    pub render_handle: Option<MpvRenderHandle>,
}

#[derive(Clone, Default)]
pub(super) struct HoverPreviewOutput {
    pub hovered_id: Option<String>,
    pub toggle_mute: bool,
}

pub(super) fn hover_input_id() -> egui::Id {
    egui::Id::unique("hover_preview_bridge_input")
}

pub(super) fn hover_output_id() -> egui::Id {
    egui::Id::unique("hover_preview_bridge_output")
}

pub(super) enum GridAction {
    Open(VideoItem),
    OpenInSecondTab(VideoItem),
    OpenChannel { channel_id: String, name: String },
    PlayNext(VideoItem),
    AddQueue(VideoItem),
    ToggleWatchLater(VideoItem),
    OpenPlaylistPicker(VideoItem),
    Download { item: VideoItem, audio_only: bool },
    CopyUrl(String),
    RemoveFromHistory(VideoItem),
    RemoveFromPlaylist(VideoItem),
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
    draw_video_grid_with_flags(
        ui,
        heading,
        items,
        loading,
        has_more,
        show_remove_history,
        false,
    )
}

pub(super) fn draw_video_grid_with_flags(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    loading: bool,
    has_more: bool,
    show_remove_history: bool,
    show_remove_playlist: bool,
) -> Option<GridAction> {
    draw_feed_grid_inner(
        ui,
        heading,
        items,
        None,
        loading,
        has_more,
        show_remove_history,
        show_remove_playlist,
        false,
    )
}

/// Renders a mixed feed (like Home) that separates `Short` entries into a dedicated
/// horizontal portrait Shorts shelf after the first row of regular 16:9 videos.
pub(super) fn draw_home_mixed_grid(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    prioritized_shorts: &[VideoItem],
    loading: bool,
    has_more: bool,
) -> Option<GridAction> {
    draw_feed_grid_inner(
        ui,
        heading,
        items,
        Some(prioritized_shorts),
        loading,
        has_more,
        false,
        false,
        true,
    )
}

pub(super) fn draw_history_feed(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    filter: &mut super::HistoryFilter,
    loading: bool,
    has_more: bool,
) -> Option<GridAction> {
    let mut action = None;
    let (shorts, videos): (Vec<&VideoItem>, Vec<&VideoItem>) =
        items.iter().partition(|it| it.is_short());

    ui.horizontal(|ui| {
        ui.label(RichText::new(heading).font(theme::bold(20.0)));
        if loading {
            ui.add_space(8.0);
            ui.spinner();
        }
    });
    ui.add_space(10.0);

    // Separate filter chips (All | Videos | Shorts) just like YouTube Watch History
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let opts = [
            (super::HistoryFilter::All, format!("All ({})", items.len())),
            (super::HistoryFilter::Videos, format!("Videos ({})", videos.len())),
            (super::HistoryFilter::Shorts, format!("Shorts ({})", shorts.len())),
        ];
        for (opt, lbl) in opts {
            let active = *filter == opt;
            let fill = if active {
                Color32::from_rgb(241, 241, 241)
            } else if theme::is_translucent() {
                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
            } else {
                theme::CHIP
            };
            let text_col = if active {
                Color32::from_rgb(15, 15, 15)
            } else {
                theme::TEXT
            };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(lbl).font(theme::medium(13.0)).color(text_col),
                    )
                    .fill(fill)
                    .corner_radius(16)
                    .min_size(Vec2::new(62.0, 30.0)),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                *filter = opt;
            }
        }
    });
    ui.add_space(12.0);

    if items.is_empty() && !loading {
        ui.label(
            RichText::new("No watch history yet.")
                .font(theme::regular(14.0))
                .color(theme::TEXT_DIM),
        );
        return None;
    }

    let scroll_out = ScrollArea::vertical()
        .id_salt(("history_feed_scroll", heading))
        .show(ui, |ui| {
            let avail = (ui.available_width() - 8.0).max(200.0);
            match *filter {
                super::HistoryFilter::All => {
                    if !shorts.is_empty() {
                        draw_shorts_shelf(ui, "history_shorts", &shorts, &mut action);
                        ui.add_space(16.0);
                        if !videos.is_empty() {
                            ui.label(
                                RichText::new("Videos")
                                    .font(theme::bold(17.5))
                                    .color(theme::TEXT),
                            );
                            ui.add_space(10.0);
                        }
                    }
                    if !videos.is_empty() {
                        draw_history_video_rows(ui, avail, &videos, &mut action);
                    }
                }
                super::HistoryFilter::Videos => {
                    if videos.is_empty() && !loading {
                        ui.label(
                            RichText::new("No regular videos in watch history.")
                                .font(theme::regular(14.0))
                                .color(theme::TEXT_DIM),
                        );
                    } else {
                        draw_history_video_rows(ui, avail, &videos, &mut action);
                    }
                }
                super::HistoryFilter::Shorts => {
                    if shorts.is_empty() && !loading {
                        ui.label(
                            RichText::new("No Shorts in watch history.")
                                .font(theme::regular(14.0))
                                .color(theme::TEXT_DIM),
                        );
                    } else {
                        let cols = ((avail / 188.0).floor() as usize).clamp(2, 7);
                        let spacing = 14.0;
                        let card_w = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32)
                            .clamp(150.0, 210.0);
                        let thumb_h = (card_w * 16.0 / 9.0).round();
                        for row in shorts.chunks(cols) {
                            ui.horizontal_top(|ui| {
                                for (idx, item) in row.iter().enumerate() {
                                    draw_short_portrait_card(
                                        ui,
                                        item,
                                        card_w,
                                        thumb_h,
                                        false,
                                        &mut action,
                                    );
                                    if idx + 1 < row.len() {
                                        ui.add_space(spacing);
                                    }
                                }
                            });
                            ui.add_space(16.0);
                        }
                    }
                }
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

fn draw_history_video_rows(
    ui: &mut egui::Ui,
    avail: f32,
    videos: &[&VideoItem],
    action: &mut Option<GridAction>,
) {
    let cols = ((avail / 305.0).floor() as usize).clamp(1, 5);
    let spacing = 16.0;
    let card_w = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).max(180.0);
    let thumb_h = card_w * 9.0 / 16.0;
    for row in videos.chunks(cols) {
        draw_video_card_row(
            ui,
            row,
            card_w,
            thumb_h,
            spacing,
            true,
            false,
            action,
        );
        ui.add_space(18.0);
    }
}

pub(super) fn draw_shorts_portrait_grid(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    loading: bool,
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
            RichText::new("No Shorts found.")
                .font(theme::regular(14.0))
                .color(theme::TEXT_DIM),
        );
        return None;
    }
    ScrollArea::vertical()
        .id_salt(("shorts_grid", heading))
        .show(ui, |ui| {
            let avail = (ui.available_width() - 8.0).max(200.0);
            let cols = ((avail / 188.0).floor() as usize).clamp(2, 7);
            let spacing = 14.0;
            let card_w = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).clamp(150.0, 210.0);
            let thumb_h = (card_w * 16.0 / 9.0).round();
            for row in items.chunks(cols) {
                ui.horizontal_top(|ui| {
                    for (idx, item) in row.iter().enumerate() {
                        draw_short_portrait_card(ui, item, card_w, thumb_h, false, &mut action);
                        if idx + 1 < row.len() {
                            ui.add_space(spacing);
                        }
                    }
                });
                ui.add_space(16.0);
            }
        });
    action
}

pub(super) fn draw_video_grid_body(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    loading: bool,
    has_more: bool,
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

    draw_feed_grid_body_inner(
        ui,
        heading,
        items,
        None,
        loading,
        has_more,
        false,
        false,
        false,
        &mut action,
    );
    action
}

#[allow(clippy::too_many_arguments)]
fn draw_feed_grid_body_inner(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    prioritized_shorts: Option<&[VideoItem]>,
    loading: bool,
    has_more: bool,
    show_remove_history: bool,
    show_remove_playlist: bool,
    split_shorts_shelf: bool,
    action: &mut Option<GridAction>,
) {
    let avail = (ui.available_width() - 8.0).max(200.0);
    let cols = ((avail / 305.0).floor() as usize).clamp(1, 5);
    let spacing = 16.0;
    let card_w = ((avail - spacing * (cols as f32 - 1.0)) / cols as f32).max(180.0);
    let thumb_h = card_w * 9.0 / 16.0;

    let (regulars, fallback_shorts): (Vec<&VideoItem>, Vec<&VideoItem>) = if split_shorts_shelf {
        items.iter().partition(|it| !it.is_short())
    } else {
        (items.iter().collect(), Vec::new())
    };
    let shorts: Vec<&VideoItem> = match prioritized_shorts {
        Some(ov) if !ov.is_empty() => ov.iter().collect(),
        _ => fallback_shorts,
    };

    let split_at = if !shorts.is_empty() {
        cols.min(regulars.len())
    } else {
        regulars.len()
    };

    for row in regulars[..split_at].chunks(cols) {
        draw_video_card_row(
            ui,
            row,
            card_w,
            thumb_h,
            spacing,
            show_remove_history,
            show_remove_playlist,
            action,
        );
        ui.add_space(18.0);
    }

    if !shorts.is_empty() {
        draw_shorts_shelf(ui, heading, &shorts, action);
        ui.add_space(20.0);
    }

    for row in regulars[split_at..].chunks(cols) {
        draw_video_card_row(
            ui,
            row,
            card_w,
            thumb_h,
            spacing,
            show_remove_history,
            show_remove_playlist,
            action,
        );
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
            *action = Some(GridAction::LoadMore);
        }
    }
}

fn draw_feed_grid_inner(
    ui: &mut egui::Ui,
    heading: &str,
    items: &[VideoItem],
    prioritized_shorts: Option<&[VideoItem]>,
    loading: bool,
    has_more: bool,
    show_remove_history: bool,
    show_remove_playlist: bool,
    split_shorts_shelf: bool,
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
        draw_feed_grid_body_inner(
            ui,
            heading,
            items,
            prioritized_shorts,
            loading,
            has_more,
            show_remove_history,
            show_remove_playlist,
            split_shorts_shelf,
            &mut action,
        );
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

#[derive(Clone, Default)]
struct ShortsShelfDrag {
    scroll_x: f32,
    max_scroll: f32,
    visible_w: f32,
    has_layout: bool,
    dragging: bool,
    drag_dist: f32,
    suppress_click: bool,
}

fn draw_shorts_shelf(
    ui: &mut egui::Ui,
    id_salt: &str,
    shorts: &[&VideoItem],
    action: &mut Option<GridAction>,
) {
    let drag_id = egui::Id::unique(("shorts_shelf_drag", id_salt));
    let mut drag_state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<ShortsShelfDrag>(drag_id))
        .unwrap_or_default();
    let mut step_left = false;
    let mut step_right = false;

    ui.add_space(4.0);
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 34.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            let (irec, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
            theme::draw_shorts_icon(ui.painter(), irec.center(), 18.0, theme::YT_RED);
            ui.add_space(4.0);
            ui.label(
                RichText::new("Shorts")
                    .font(theme::bold(19.0))
                    .color(theme::TEXT),
            );
            if let Some(first) = shorts.first() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("{}  Play Shorts", icon::PLAY))
                                    .font(theme::medium(12.5))
                                    .color(theme::TEXT),
                            )
                            .min_size(Vec2::new(0.0, 32.0))
                            .fill(theme::CHIP)
                            .corner_radius(16),
                        )
                        .clicked()
                    {
                        *action = Some(GridAction::Open((*first).clone()));
                    }
                    ui.add_space(6.0);
                    if icon_circle_button(ui, icon::CHEVRON_RIGHT, 15.0, "Scroll Shorts right") {
                        step_right = true;
                    }
                    ui.add_space(2.0);
                    if icon_circle_button(ui, icon::CHEVRON_LEFT, 15.0, "Scroll Shorts left") {
                        step_left = true;
                    }
                });
            }
        },
    );
    ui.add_space(10.0);

    let card_w = 172.0;
    let thumb_h = 304.0;
    let step_px = if drag_state.visible_w > 0.0 {
        (drag_state.visible_w * 0.75).max(368.0)
    } else {
        368.0
    };
    if step_left {
        drag_state.scroll_x = (drag_state.scroll_x - step_px).max(0.0);
        ui.ctx().request_repaint();
    }
    if step_right {
        drag_state.scroll_x = if drag_state.has_layout {
            (drag_state.scroll_x + step_px).min(drag_state.max_scroll)
        } else {
            drag_state.scroll_x + step_px
        };
        ui.ctx().request_repaint();
    }

    let drag_blocked = drag_state.suppress_click || (drag_state.dragging && drag_state.drag_dist >= 6.0);
    let mut sa = ScrollArea::horizontal().id_salt(("home_shorts_shelf", id_salt));
    if drag_state.dragging || step_left || step_right {
        sa = sa.horizontal_scroll_offset(drag_state.scroll_x);
    }
    let shelf_out = ui.scope(|ui| {
        sa.show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for (idx, item) in shorts.iter().enumerate() {
                    draw_short_portrait_card(ui, item, card_w, thumb_h, drag_blocked, action);
                    if idx + 1 < shorts.len() {
                        ui.add_space(12.0);
                    }
                }
            });
        })
    });
    let scroll_out = shelf_out.inner;
    let shelf_rect = shelf_out.response.rect;
    let max_scroll = (scroll_out.content_size.x - scroll_out.inner_rect.width()).max(0.0);
    drag_state.max_scroll = max_scroll;
    drag_state.visible_w = scroll_out.inner_rect.width();
    drag_state.has_layout = true;

    let hovered_shelf = ui.rect_contains_pointer(shelf_rect);
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

    if !drag_state.dragging {
        drag_state.scroll_x = scroll_out.state.clamped_offset().x;
    }

    if hovered_shelf && primary_pressed {
        drag_state.dragging = true;
        drag_state.drag_dist = 0.0;
        drag_state.suppress_click = false;
    }
    if drag_state.dragging && primary_down {
        drag_state.drag_dist += delta_x.abs();
        drag_state.scroll_x =
            (scroll_out.state.clamped_offset().x - delta_x).clamp(0.0, max_scroll);
        if drag_state.drag_dist >= 6.0 {
            drag_state.suppress_click = true;
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
        if delta_x.abs() > 0.1 {
            ui.ctx().request_repaint();
        }
    } else if !primary_down {
        drag_state.dragging = false;
        drag_state.drag_dist = 0.0;
        drag_state.suppress_click = false;
    }

    if hovered_shelf && secondary_down {
        drag_state.scroll_x =
            (scroll_out.state.clamped_offset().x - delta_x).clamp(0.0, max_scroll);
    }
    if hovered_shelf && wheel_y.abs() > 0.1 && drag_state.dragging {
        drag_state.scroll_x = (drag_state.scroll_x - wheel_y).clamp(0.0, max_scroll);
    }

    ui.ctx().data_mut(|d| d.insert_temp(drag_id, drag_state));
}

fn draw_short_portrait_card(
    ui: &mut egui::Ui,
    item: &VideoItem,
    card_w: f32,
    thumb_h: f32,
    drag_blocked: bool,
    action: &mut Option<GridAction>,
) {
    let card_h = thumb_h + 76.0;
    let (card_rect, card_resp) =
        ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
    let hovered = card_resp.hovered() || ui.rect_contains_pointer(card_rect);
    let hover_t = ui.ctx().animate_bool_with_time(
        egui::Id::unique(("short_card_hover", &item.id)),
        hovered,
        0.15,
    );
    if hovered && !drag_blocked {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if hover_t > 0.01 {
        ui.painter().rect_filled(
            card_rect.expand(4.0),
            14.0,
            Color32::from_rgba_unmultiplied(40, 32, 29, (205.0 * hover_t) as u8),
        );
    }

    let mut card_ui = ui.new_child(egui::UiBuilder::new().max_rect(card_rect));
    let cclip = card_ui.clip_rect().intersect(card_rect);
    card_ui.set_clip_rect(cclip);
    let mut child_clicked_override = false;
    card_ui.vertical(|ui| {
        let (thumb_rect, _) =
            ui.allocate_exact_size(Vec2::new(card_w, thumb_h), Sense::hover());
        ui.painter().rect_filled(thumb_rect, 12.0, Color32::BLACK);
        theme::paint_image_cover_rounded(ui, &item.thumbnail_url, thumb_rect, 12);

        // Top-left badges ("New", "Members only") matching YouTube Shorts shelf
        let mut badge_y = thumb_rect.top() + 8.0;
        if item.is_new {
            let galley = ui.painter().layout_no_wrap(
                "New".to_owned(),
                theme::semibold(10.5),
                Color32::WHITE,
            );
            let pad = vec2(7.0, 2.5);
            let bsz = galley.size() + pad * 2.0;
            let brect = Rect::from_min_size(pos2(thumb_rect.left() + 8.0, badge_y), bsz);
            ui.painter()
                .rect_filled(brect, 5.0, Color32::from_black_alpha(195));
            ui.painter()
                .galley(brect.min + pad, galley, Color32::WHITE);
            badge_y += bsz.y + 5.0;
        }
        if item.is_members_only {
            let text_col = Color32::from_rgb(72, 232, 148);
            let galley = ui.painter().layout_no_wrap(
                "Members only".to_owned(),
                theme::semibold(10.0),
                text_col,
            );
            let pad = vec2(6.5, 2.5);
            let bsz = galley.size() + pad * 2.0;
            let brect = Rect::from_min_size(pos2(thumb_rect.left() + 8.0, badge_y), bsz);
            ui.painter()
                .rect_filled(brect, 5.0, Color32::from_rgba_unmultiplied(12, 46, 30, 225));
            ui.painter()
                .galley(brect.min + pad, galley, text_col);
        }

        if hover_t > 0.02 {
            ui.painter().circle_filled(
                thumb_rect.center(),
                20.0,
                Color32::from_black_alpha((150.0 * hover_t) as u8),
            );
            theme::draw_icon_centered(
                ui.painter(),
                thumb_rect.center(),
                icon::PLAY,
                16.0,
                Color32::from_white_alpha((235.0 * hover_t) as u8),
            );
        }

        ui.add_space(7.0);
        ui.horizontal_top(|ui| {
            let text_w = (card_w - 30.0).max(90.0);
            ui.vertical(|ui| {
                ui.set_max_width(text_w);
                ui.spacing_mut().item_spacing.y = 3.0;
                let clamped = clamp_two_lines(ui, &item.title, theme::semibold(13.0), text_w - 2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(clamped)
                            .font(theme::semibold(13.0))
                            .color(theme::TEXT),
                    )
                    .wrap(),
                );
                let sub = match (item.channel.is_empty(), item.views_text.is_empty()) {
                    (false, false) => format!("{} · {}", item.channel, item.views_text),
                    (false, true) => item.channel.clone(),
                    (true, false) => item.views_text.clone(),
                    (true, true) => String::new(),
                };
                if !sub.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(sub)
                                .font(theme::regular(11.5))
                                .color(theme::TEXT_DIM),
                        )
                        .truncate(),
                    );
                }
            });
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                let menu_id = ui.scope_id().with(("short_menu", &item.id));
                ui.push_id(menu_id, |ui| {
                    let (mrect, mresp) =
                        ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                    let mhov =
                        ui.ctx()
                            .animate_bool_with_time(mresp.id, mresp.hovered(), 0.12);
                    if mresp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if mhov > 0.01 {
                        ui.painter().circle_filled(
                            mrect.center(),
                            13.0,
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
                        14.0,
                        icon_col,
                    );
                    egui::Popup::menu(&mresp).show(|ui| {
                        draw_video_context_menu(ui, item, action, false);
                    });
                    if mresp.hovered() || mresp.clicked() {
                        child_clicked_override = true;
                    }
                });
            });
        });
    });

    card_resp.context_menu(|ui| {
        draw_video_context_menu(ui, item, action, false);
    });
    let card_hover_resp = if item.title.chars().count() > 38 {
        card_resp.on_hover_text(&item.title)
    } else {
        card_resp
    };
    if card_hover_resp.clicked() && !child_clicked_override && !drag_blocked && action.is_none()
    {
        *action = Some(GridAction::Open(item.clone()));
    }
}

fn draw_video_card_row(
    ui: &mut egui::Ui,
    row: &[&VideoItem],
    card_w: f32,
    thumb_h: f32,
    spacing: f32,
    show_remove_history: bool,
    show_remove_playlist: bool,
    action: &mut Option<GridAction>,
) {
    let hover_in = ui
        .ctx()
        .data_mut(|d| d.get_temp::<HoverPreviewInput>(hover_input_id()))
        .unwrap_or_default();
    let any_popup_open = egui::Popup::is_any_open(ui.ctx());

    ui.horizontal_top(|ui| {
        for (col_idx, item) in row.iter().enumerate() {
            let item = *item;
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

            let can_preview = hover_in.enabled
                && !item.is_short()
                && item.playlist_browse_id.is_none()
                && !any_popup_open;
            if can_preview && hovered {
                ui.ctx().data_mut(|d| {
                    let mut out = d
                        .get_temp::<HoverPreviewOutput>(hover_output_id())
                        .unwrap_or_default();
                    out.hovered_id = Some(item.id.clone());
                    d.insert_temp(hover_output_id(), out);
                });
            }

            let is_preview_active =
                can_preview && hover_in.active_id.as_deref() == Some(item.id.as_str());
            let is_preview_loading =
                can_preview && hover_in.loading_id.as_deref() == Some(item.id.as_str());

            let mut card_ui = ui.new_child(egui::UiBuilder::new().max_rect(card_rect));
            let cclip = card_ui.clip_rect().intersect(card_rect);
            card_ui.set_clip_rect(cclip);
            let mut child_clicked_override = false;
            card_ui.vertical(|ui| {
                let thumb_rect = draw_thumbnail_with_hover(
                    ui,
                    &item.thumbnail_url,
                    &item.duration_text,
                    Vec2::new(card_w, thumb_h),
                    12.0,
                    item.watched_percent,
                    hover_t,
                    if is_preview_active {
                        Some(&hover_in)
                    } else {
                        None
                    },
                    is_preview_loading,
                );
                if is_preview_active && hover_in.has_frame {
                    let mute_rect = Rect::from_min_size(
                        pos2(thumb_rect.right() - 36.0, thumb_rect.top() + 8.0),
                        Vec2::splat(28.0),
                    );
                    let mute_resp = ui
                        .interact(
                            mute_rect,
                            ui.scope_id().with(("hover_mute", &item.id)),
                            Sense::click(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    let bg_alpha = if mute_resp.hovered() { 220 } else { 175 };
                    ui.painter()
                        .circle_filled(mute_rect.center(), 14.0, Color32::from_black_alpha(bg_alpha));
                    theme::draw_icon_centered(
                        ui.painter(),
                        mute_rect.center(),
                        if hover_in.muted {
                            icon::VOLUME_X
                        } else {
                            icon::VOLUME_2
                        },
                        14.0,
                        Color32::WHITE,
                    );
                    let mute_tip = if hover_in.muted {
                        "Unmute preview"
                    } else {
                        "Mute preview"
                    };
                    let mute_resp = mute_resp.on_hover_text(mute_tip);
                    if mute_resp.hovered() || mute_resp.clicked() {
                        child_clicked_override = true;
                    }
                    if mute_resp.clicked() {
                        ui.ctx().data_mut(|d| {
                            let mut out = d
                                .get_temp::<HoverPreviewOutput>(hover_output_id())
                                .unwrap_or_default();
                            out.toggle_mute = true;
                            d.insert_temp(hover_output_id(), out);
                        });
                    }
                }
                if item.is_members_only {
                    let green = Color32::from_rgb(72, 232, 148);
                    let galley = ui.painter().layout_no_wrap(
                        "Members only".to_owned(),
                        theme::semibold(10.0),
                        green,
                    );
                    let pad = vec2(6.5, 2.0);
                    let bsz = galley.size() + pad * 2.0;
                    let has_bar = (is_preview_active && hover_in.has_frame)
                        || item.watched_percent.is_some_and(|p| p > 0.01);
                    let bottom_inset = if has_bar { 9.0 } else { 7.0 };
                    let brect = Rect::from_min_size(
                        pos2(thumb_rect.left() + 7.0, thumb_rect.bottom() - bsz.y - bottom_inset),
                        bsz,
                    );
                    ui.painter().rect_filled(
                        brect,
                        4.0,
                        Color32::from_rgba_unmultiplied(12, 46, 30, 225),
                    );
                    ui.painter().galley(brect.min + pad, galley, green);
                }
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
                            *action = Some(GridAction::OpenChannel {
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
                                *action = Some(GridAction::OpenChannel {
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
                                draw_video_context_menu(ui, item, action, false);
                                if show_remove_history
                                    && ui
                                        .button(format!(
                                            "{}  Remove from history",
                                            icon::TRASH_2
                                        ))
                                        .clicked()
                                {
                                    *action = Some(GridAction::RemoveFromHistory(item.clone()));
                                    ui.close();
                                }
                                if show_remove_playlist
                                    && ui
                                        .button(format!(
                                            "{}  Remove from playlist",
                                            icon::TRASH_2
                                        ))
                                        .clicked()
                                {
                                    *action = Some(GridAction::RemoveFromPlaylist(item.clone()));
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
                draw_video_context_menu(ui, item, action, false);
                if show_remove_history
                    && ui
                        .button(format!("{}  Remove from history", icon::TRASH_2))
                        .clicked()
                {
                    *action = Some(GridAction::RemoveFromHistory(item.clone()));
                    ui.close();
                }
                if show_remove_playlist
                    && ui
                        .button(format!("{}  Remove from playlist", icon::TRASH_2))
                        .clicked()
                {
                    *action = Some(GridAction::RemoveFromPlaylist(item.clone()));
                    ui.close();
                }
            });

            let card_hover_resp = if item.title.chars().count() > 46 {
                card_resp.on_hover_text(&item.title)
            } else {
                card_resp
            };

            if card_hover_resp.middle_clicked()
                && !child_clicked_override
                && action.is_none()
                && !item.is_short()
                && item.playlist_browse_id.is_none()
            {
                *action = Some(GridAction::OpenInSecondTab(item.clone()));
            } else if card_hover_resp.clicked() && !child_clicked_override && action.is_none() {
                *action = Some(GridAction::Open(item.clone()));
            }
            if col_idx + 1 < row.len() {
                ui.add_space(spacing);
            }
        }
    });
}

pub(super) fn draw_video_context_menu(
    ui: &mut egui::Ui,
    item: &VideoItem,
    action: &mut Option<GridAction>,
    show_play_next: bool,
) {
    if !item.is_short()
        && item.playlist_browse_id.is_none()
        && ui
            .button(format!("{}  Open in 2nd Player Tab", icon::PLAY))
            .clicked()
    {
        *action = Some(GridAction::OpenInSecondTab(item.clone()));
        ui.close();
    }
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
    draw_thumbnail_with_hover(ui, url, duration, size, radius, watched_percent, 0.0, None, false);
}

#[allow(clippy::too_many_arguments)]
fn draw_thumbnail_with_hover(
    ui: &mut egui::Ui,
    url: &str,
    duration: &str,
    size: Vec2,
    radius: f32,
    watched_percent: Option<f32>,
    hover_t: f32,
    preview: Option<&HoverPreviewInput>,
    preview_loading: bool,
) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

    // Slightly flatten radius on hover just like YouTube
    let cur_radius = (radius - 4.0 * hover_t).max(4.0);
    let playing_frame = preview.filter(|p| p.has_frame && p.render_handle.is_some());

    if let Some(p) = playing_frame {
        ui.painter().rect_filled(rect, cur_radius, Color32::BLACK);
        if let Some(handle) = &p.render_handle {
            ui.painter().add(handle.paint_callback(rect));
        }
    } else {
        theme::paint_image_rounded(ui, url, rect, cur_radius.round() as u8);
        if preview_loading {
            let pill = Rect::from_min_size(pos2(rect.right() - 34.0, rect.top() + 8.0), Vec2::splat(26.0));
            ui.painter()
                .circle_filled(pill.center(), 13.0, Color32::from_black_alpha(180));
            let mut spin_ui = ui.new_child(egui::UiBuilder::new().max_rect(pill.shrink(5.0)));
            spin_ui.add(egui::Spinner::new().size(14.0).color(Color32::WHITE));
        } else if hover_t > 0.02 {
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
    }

    let active_pct = if let Some(p) = playing_frame {
        (p.duration > 0.0).then_some((p.time_pos / p.duration).clamp(0.0, 1.0) as f32)
    } else {
        watched_percent.filter(|&p| p > 0.01)
    };

    if let Some(pct) = active_pct {
        let bar_h = if playing_frame.is_some() { 3.5 } else { 4.0 };
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

    let badge_text = if let Some(p) = playing_frame {
        let is_live = duration.eq_ignore_ascii_case("LIVE");
        if is_live {
            duration.to_owned()
        } else if p.duration > 0.0 {
            format_duration((p.duration - p.time_pos).max(0.0))
        } else {
            duration.to_owned()
        }
    } else {
        duration.to_owned()
    };

    if !badge_text.is_empty() {
        let is_live = badge_text.eq_ignore_ascii_case("LIVE");
        let bg = if is_live {
            theme::YT_RED
        } else {
            Color32::from_black_alpha(210)
        };
        let compact = size.y <= 54.0;
        let font = theme::semibold(if compact { 8.8 } else { 11.0 });
        let pad = if compact { vec2(3.5, 1.0) } else { vec2(6.0, 2.5) };
        let offset = if compact { vec2(3.0, 3.0) } else { vec2(7.0, 7.0) };
        let corner = if compact { 3.0 } else { 4.0 };
        let galley = ui.painter().layout_no_wrap(
            badge_text,
            font,
            Color32::WHITE,
        );
        let badge_size = galley.size() + pad * 2.0;
        let badge_rect =
            Rect::from_min_size(rect.max - badge_size - offset, badge_size);
        ui.painter().rect_filled(badge_rect, corner, bg);
        ui.painter()
            .galley(badge_rect.min + pad, galley, Color32::WHITE);
    }

    rect
}

pub(super) fn draw_members_badge(ui: &mut egui::Ui) {
    let green = Color32::from_rgb(45, 212, 128);
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(15, 157, 88, 38))
        .corner_radius(4)
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                theme::draw_icon(ui, icon::SPARKLES, 10.5, green);
                ui.label(
                    RichText::new("Members only")
                        .font(theme::semibold(10.5))
                        .color(green),
                );
            });
        });
}

pub(super) fn clamp_two_lines(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    max_w: f32,
) -> String {
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

pub(super) fn join_meta(views: &str, published: &str) -> String {
    match (views.is_empty(), published.is_empty()) {
        (false, false) => format!("{views} · {published}"),
        (false, true) => views.to_owned(),
        (true, false) => published.to_owned(),
        (true, true) => String::new(),
    }
}

pub(super) fn draw_subscribe_bell_button(
    ui: &mut egui::Ui,
    channel_id: String,
    is_subscribed: bool,
    notifications: &ChannelNotificationParams,
    tx: &crossbeam_channel::Sender<Request>,
) {
    if !is_subscribed {
        let (srect, sresp) = ui.allocate_exact_size(Vec2::new(96.0, 34.0), Sense::click());
        if sresp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let fill = if sresp.hovered() {
            Color32::from_rgb(217, 217, 217)
        } else {
            Color32::from_rgb(241, 241, 241)
        };
        ui.painter().rect_filled(srect, 17.0, fill);
        ui.painter().text(
            srect.center(),
            Align2::CENTER_CENTER,
            "Subscribe",
            theme::semibold(13.0),
            Color32::from_rgb(15, 15, 15),
        );
        if sresp.clicked() {
            let _ = tx.send(Request::SetSubscribe {
                channel_id,
                subscribe: true,
            });
        }
        return;
    }

    let label = match notifications.level {
        NotificationLevel::All => "All",
        NotificationLevel::Personalized => "Subscribed",
        NotificationLevel::None => "Off",
    };
    let pill_w = match notifications.level {
        NotificationLevel::All => 88.0,
        NotificationLevel::Personalized => 126.0,
        NotificationLevel::None => 88.0,
    };
    let (srect, sresp) = ui.allocate_exact_size(Vec2::new(pill_w, 34.0), Sense::click());
    if sresp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let fill = if sresp.hovered() {
        theme::CHIP_HOVER
    } else {
        theme::CHIP
    };
    ui.painter().rect_filled(srect, 17.0, fill);
    let bell_col = match notifications.level {
        NotificationLevel::All => Color32::WHITE,
        NotificationLevel::Personalized => theme::TEXT,
        NotificationLevel::None => theme::TEXT_DIM,
    };
    theme::draw_bell(
        ui.painter(),
        pos2(srect.left() + 18.0, srect.center().y),
        14.0,
        bell_col,
    );
    if notifications.level == NotificationLevel::None {
        ui.painter().line_segment(
            [
                pos2(srect.left() + 12.0, srect.center().y + 6.0),
                pos2(srect.left() + 24.0, srect.center().y - 6.0),
            ],
            Stroke::new(1.5, bell_col),
        );
    }
    ui.painter().text(
        pos2(srect.left() + 32.0, srect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::semibold(13.0),
        theme::TEXT,
    );
    theme::draw_icon_centered(
        ui.painter(),
        pos2(srect.right() - 14.0, srect.center().y),
        icon::CHEVRON_DOWN,
        12.0,
        theme::TEXT_DIM,
    );

    egui::Popup::menu(&sresp).show(|ui| {
        ui.set_min_width(170.0);
        if notifications.has_controls() {
            for lvl in [
                NotificationLevel::All,
                NotificationLevel::Personalized,
                NotificationLevel::None,
            ] {
                let mark = if notifications.level == lvl {
                    icon::CHECK
                } else {
                    ' '
                };
                let p_opt = notifications.params_for(lvl);
                if ui
                    .add_enabled(
                        p_opt.is_some(),
                        egui::Button::new(
                            RichText::new(format!("{mark}  {}", lvl.label()))
                                .font(theme::regular(13.0)),
                        ),
                    )
                    .clicked()
                {
                    if let Some(params) = p_opt {
                        let _ = tx.send(Request::SetChannelNotifications {
                            channel_id: channel_id.clone(),
                            level: lvl,
                            params: params.to_owned(),
                        });
                    }
                    ui.close();
                }
            }
            ui.separator();
        }
        if ui
            .button(
                RichText::new(format!("{}  Unsubscribe", icon::X))
                    .font(theme::regular(13.0))
                    .color(theme::TEXT),
            )
            .clicked()
        {
            let _ = tx.send(Request::SetSubscribe {
                channel_id: channel_id.clone(),
                subscribe: false,
            });
            ui.close();
        }
    });
}
