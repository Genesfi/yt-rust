use egui::{
    Align, Align2, Color32, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense, Vec2, pos2,
};

use crate::backend::Request;
use crate::theme::{self, icon};

use super::{Page, YtApp};

impl YtApp {
    pub(super) fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
        let raw_t = ui.ctx().animate_bool_with_time(
            egui::Id::unique("sidebar_expand_anim"),
            self.sidebar_expanded,
            0.22,
        );
        let expand_t = theme::ease_out_cubic(raw_t);
        let width = 68.0 + (208.0 - 68.0) * expand_t;

        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(width)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                let inner_w = (width - 16.0).max(44.0);
                ui.set_width(inner_w);
                let clip = ui.clip_rect().intersect(ui.max_rect());
                ui.set_clip_rect(clip);

                ScrollArea::vertical()
                    .id_salt("sidebar_nav_scroll")
                    .scroll_bar_visibility(if expand_t > 0.5 {
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
                    } else {
                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
                    })
                    .max_height((ui.available_height() - 32.0).max(160.0))
                    .show(ui, |ui| {
                        // Top primary navigation
                        if sidebar_item(
                            ui,
                            icon::HOUSE,
                            "Home",
                            "Home",
                            self.page == Page::Home,
                            expand_t,
                        ) {
                            self.page = Page::Home;
                            if self.home_items.is_empty() && !self.home_loading {
                                self.home_loading = true;
                                let _ = self.backend.tx.send(Request::LoadHome {
                                    params: None,
                                    continuation: None,
                                });
                            }
                        }

                        // When collapsed, keep a quick mini-icon for Subscriptions feed
                        if expand_t <= 0.12
                            && sidebar_item(
                                ui,
                                icon::RADIO,
                                "Subscriptions",
                                "Subs",
                                self.page == Page::Subscriptions,
                                expand_t,
                            )
                        {
                            self.open_subscriptions_feed();
                        }

                        if sidebar_item(
                            ui,
                            icon::COMPASS,
                            "Trending",
                            "Trending",
                            self.page == Page::Trending,
                            expand_t,
                        ) {
                            self.page = Page::Trending;
                            if self.trending_items.is_empty() && !self.trending_loading {
                                self.trending_loading = true;
                                let _ = self.backend.tx.send(Request::LoadTrending);
                            }
                        }

                        if self.watch.is_some()
                            && sidebar_item(
                                ui,
                                icon::PLAY,
                                "Now Playing",
                                "Playing",
                                self.page == Page::Watch,
                                expand_t,
                            )
                        {
                            self.page = Page::Watch;
                        }

                        // Collapsible Subscriptions section (like YouTube's "Subscription >")
                        if expand_t > 0.12 {
                            let sec_alpha = ((expand_t - 0.12) / 0.88).clamp(0.0, 1.0);
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(6.0);

                            let (open_feed, toggle_subs) = sidebar_section_header(
                                ui,
                                "Subscriptions",
                                self.page == Page::Subscriptions,
                                self.subs_list_open,
                                sec_alpha,
                                "Open Subscriptions feed",
                                if self.subs_list_open {
                                    "Hide channels"
                                } else {
                                    "Show channels"
                                },
                            );
                            if open_feed {
                                self.open_subscriptions_feed();
                            }
                            if toggle_subs {
                                self.subs_list_open = !self.subs_list_open;
                            }

                            let subs_anim = ui.ctx().animate_bool_with_time(
                                egui::Id::unique("sidebar_subs_list_anim"),
                                self.subs_list_open,
                                0.20,
                            );
                            if subs_anim > 0.01 && self.signed_in && !self.guide_channels.is_empty()
                            {
                                let eased = theme::ease_out_cubic(subs_anim);
                                let list_alpha = sec_alpha * eased;
                                let limit = if self.subs_sidebar_expanded {
                                    self.guide_channels.len()
                                } else {
                                    7.min(self.guide_channels.len())
                                };
                                let has_more = self.guide_channels.len() > 7;
                                let full_h =
                                    limit as f32 * 38.0 + if has_more { 42.0 } else { 0.0 };
                                let cur_h = full_h * eased;
                                let (acc_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), cur_h),
                                    Sense::hover(),
                                );
                                let mut acc_ui =
                                    ui.new_child(egui::UiBuilder::new().max_rect(acc_rect));
                                let acc_clip = acc_ui.clip_rect().intersect(acc_rect);
                                acc_ui.set_clip_rect(acc_clip);
                                acc_ui.spacing_mut().item_spacing.y = 4.0;
                                acc_ui.add_space(2.0);

                                let chans: Vec<_> = self.guide_channels[..limit].to_vec();
                                for ch in chans {
                                    let active = self.page == Page::Channel
                                        && self.channel_id.as_deref()
                                            == Some(ch.browse_id.as_str());
                                    if sidebar_channel_row(
                                        &mut acc_ui,
                                        &ch.name,
                                        ch.avatar_url.as_deref(),
                                        ch.is_live,
                                        active,
                                        list_alpha,
                                    ) {
                                        self.open_channel(ch.browse_id, ch.name);
                                    }
                                }
                                if has_more {
                                    let toggle_lbl = if self.subs_sidebar_expanded {
                                        "Show less"
                                    } else {
                                        "Show more"
                                    };
                                    let toggle_ico = if self.subs_sidebar_expanded {
                                        icon::CHEVRON_UP
                                    } else {
                                        icon::CHEVRON_DOWN
                                    };
                                    if sidebar_item_alpha(
                                        &mut acc_ui,
                                        toggle_ico,
                                        toggle_lbl,
                                        "",
                                        false,
                                        expand_t,
                                        list_alpha,
                                    ) {
                                        self.subs_sidebar_expanded = !self.subs_sidebar_expanded;
                                    }
                                }
                            }

                            // Collapsible "You >" section (like YouTube's "Anda >")
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(6.0);

                            let (you_clicked, toggle_you) = sidebar_section_header(
                                ui,
                                "You",
                                false,
                                self.you_section_open,
                                sec_alpha,
                                "Toggle You menu",
                                if self.you_section_open {
                                    "Collapse You"
                                } else {
                                    "Expand You"
                                },
                            );
                            if you_clicked || toggle_you {
                                self.you_section_open = !self.you_section_open;
                            }

                            let you_anim = ui.ctx().animate_bool_with_time(
                                egui::Id::unique("sidebar_you_anim"),
                                self.you_section_open,
                                0.20,
                            );
                            let pl_anim = ui.ctx().animate_bool_with_time(
                                egui::Id::unique("sidebar_playlists_anim"),
                                self.playlists_section_open,
                                0.18,
                            );
                            if you_anim > 0.01 {
                                let you_eased = theme::ease_out_cubic(you_anim);
                                let you_alpha = sec_alpha * you_eased;
                                let has_playlists =
                                    self.signed_in && !self.guide_playlists.is_empty();
                                let pl_limit = if self.playlists_sidebar_expanded {
                                    self.guide_playlists.len()
                                } else {
                                    4.min(self.guide_playlists.len())
                                };
                                let pl_has_more = self.guide_playlists.len() > 4;
                                let pl_full_h = if has_playlists {
                                    pl_limit as f32 * 40.0 + if pl_has_more { 42.0 } else { 0.0 }
                                } else {
                                    0.0
                                };
                                let pl_cur_h = if pl_anim > 0.01 {
                                    pl_full_h * theme::ease_out_cubic(pl_anim)
                                } else {
                                    0.0
                                };
                                let base_rows = 3usize
                                    + usize::from(self.signed_in)
                                    + usize::from(has_playlists);
                                let full_h = base_rows as f32 * 42.0 + pl_cur_h + 2.0;
                                let cur_h = full_h * you_eased;

                                let (acc_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), cur_h),
                                    Sense::hover(),
                                );
                                let mut acc_ui =
                                    ui.new_child(egui::UiBuilder::new().max_rect(acc_rect));
                                let acc_clip = acc_ui.clip_rect().intersect(acc_rect);
                                acc_ui.set_clip_rect(acc_clip);
                                acc_ui.spacing_mut().item_spacing.y = 4.0;
                                acc_ui.add_space(2.0);

                                if sidebar_item_alpha(
                                    &mut acc_ui,
                                    icon::HISTORY,
                                    "History",
                                    "History",
                                    self.page == Page::History,
                                    expand_t,
                                    you_alpha,
                                ) {
                                    self.open_history_page();
                                }

                                if has_playlists {
                                    let pl_header_active = self.page == Page::Playlists
                                        || (self.page == Page::Playlist
                                            && self.active_playlist_id.as_deref() != Some("VLWL")
                                            && self.active_playlist_id.as_deref() != Some("VLLL"));
                                    let (open_pl_page, toggle_pl_list) = sidebar_expandable_item(
                                        &mut acc_ui,
                                        icon::LIST_MUSIC,
                                        "Playlists",
                                        pl_header_active,
                                        self.playlists_section_open,
                                        you_alpha,
                                    );
                                    if open_pl_page {
                                        self.page = Page::Playlists;
                                    }
                                    if toggle_pl_list {
                                        self.playlists_section_open = !self.playlists_section_open;
                                    }

                                    if pl_anim > 0.01 {
                                        let pl_eased = theme::ease_out_cubic(pl_anim);
                                        let pl_alpha = you_alpha * pl_eased;
                                        let (pl_rect, _) = acc_ui.allocate_exact_size(
                                            Vec2::new(acc_ui.available_width(), pl_cur_h),
                                            Sense::hover(),
                                        );
                                        let mut pl_ui = acc_ui
                                            .new_child(egui::UiBuilder::new().max_rect(pl_rect));
                                        let pl_clip = pl_ui.clip_rect().intersect(pl_rect);
                                        pl_ui.set_clip_rect(pl_clip);
                                        pl_ui.spacing_mut().item_spacing.y = 4.0;

                                        let pls: Vec<_> =
                                            self.guide_playlists[..pl_limit].to_vec();
                                        for pl in pls {
                                            let active = self.page == Page::Playlist
                                                && self.active_playlist_id.as_deref()
                                                    == Some(pl.browse_id.as_str());
                                            if sidebar_playlist_item(
                                                &mut pl_ui,
                                                &pl.title,
                                                &pl.subtitle,
                                                pl.thumbnail_url.as_deref(),
                                                active,
                                                pl_alpha,
                                            ) {
                                                self.open_playlist(pl.browse_id, pl.title);
                                            }
                                        }
                                        if pl_has_more {
                                            let toggle_lbl = if self.playlists_sidebar_expanded {
                                                "Show less"
                                            } else {
                                                "Show more"
                                            };
                                            let toggle_ico = if self.playlists_sidebar_expanded {
                                                icon::CHEVRON_UP
                                            } else {
                                                icon::CHEVRON_DOWN
                                            };
                                            if sidebar_item_alpha(
                                                &mut pl_ui,
                                                toggle_ico,
                                                toggle_lbl,
                                                "",
                                                false,
                                                expand_t,
                                                pl_alpha,
                                            ) {
                                                self.playlists_sidebar_expanded =
                                                    !self.playlists_sidebar_expanded;
                                            }
                                        }
                                    }
                                }

                                let wl_active = self.page == Page::Saved
                                    || (self.page == Page::Playlist
                                        && self.active_playlist_id.as_deref() == Some("VLWL"));
                                if sidebar_item_alpha(
                                    &mut acc_ui,
                                    icon::LIBRARY,
                                    "Watch Later",
                                    "Later",
                                    wl_active,
                                    expand_t,
                                    you_alpha,
                                ) {
                                    if self.signed_in {
                                        self.open_playlist(
                                            "VLWL".to_owned(),
                                            "Watch Later".to_owned(),
                                        );
                                    } else {
                                        self.page = Page::Saved;
                                    }
                                }

                                if self.signed_in {
                                    let liked_active = self.page == Page::Playlist
                                        && self.active_playlist_id.as_deref() == Some("VLLL");
                                    if sidebar_item_alpha(
                                        &mut acc_ui,
                                        icon::HEART,
                                        "Liked Videos",
                                        "Liked",
                                        liked_active,
                                        expand_t,
                                        you_alpha,
                                    ) {
                                        self.open_playlist(
                                            "VLLL".to_owned(),
                                            "Liked Videos".to_owned(),
                                        );
                                    }
                                }

                                let dl_label = if self.active_downloads.is_empty() {
                                    "Downloads".to_owned()
                                } else {
                                    format!("Downloads ({})", self.active_downloads.len())
                                };
                                if sidebar_item_alpha(
                                    &mut acc_ui,
                                    icon::DOWNLOAD,
                                    &dl_label,
                                    "Downloads",
                                    self.page == Page::Downloads,
                                    expand_t,
                                    you_alpha,
                                ) {
                                    self.page = Page::Downloads;
                                }
                            }
                        } else {
                            // Collapsed mini-sidebar essentials
                            if sidebar_item(
                                ui,
                                icon::HISTORY,
                                "History",
                                "History",
                                self.page == Page::History,
                                expand_t,
                            ) {
                                self.open_history_page();
                            }
                            if self.signed_in
                                && !self.guide_playlists.is_empty()
                                && sidebar_item(
                                    ui,
                                    icon::LIST_MUSIC,
                                    "Playlists",
                                    "Playlists",
                                    self.page == Page::Playlists,
                                    expand_t,
                                )
                            {
                                self.page = Page::Playlists;
                            }
                            let wl_active = self.page == Page::Saved
                                || (self.page == Page::Playlist
                                    && self.active_playlist_id.as_deref() == Some("VLWL"));
                            if sidebar_item(
                                ui,
                                icon::LIBRARY,
                                "Watch Later",
                                "Later",
                                wl_active,
                                expand_t,
                            ) {
                                if self.signed_in {
                                    self.open_playlist("VLWL".to_owned(), "Watch Later".to_owned());
                                } else {
                                    self.page = Page::Saved;
                                }
                            }
                            if sidebar_item(
                                ui,
                                icon::DOWNLOAD,
                                "Downloads",
                                "Saved",
                                self.page == Page::Downloads,
                                expand_t,
                            ) {
                                self.page = Page::Downloads;
                            }
                        }

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        if sidebar_item(
                            ui,
                            icon::SETTINGS,
                            "Settings",
                            "Settings",
                            self.page == Page::Settings,
                            expand_t,
                        ) {
                            self.page = Page::Settings;
                        }
                    });

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    if expand_t > 0.35 {
                        let a = ((expand_t - 0.35) / 0.65).clamp(0.0, 1.0);
                        if let Some(mpv) = &self.mpv {
                            ui.label(
                                RichText::new("● libmpv GPU ready")
                                    .font(theme::medium(11.5))
                                    .color(Color32::from_rgb(110, 215, 135).gamma_multiply(a)),
                            )
                            .on_hover_text(mpv.dll_path.display().to_string());
                        } else {
                            let r = ui.add(
                                egui::Button::new(
                                    RichText::new("● Install libmpv")
                                        .font(theme::medium(11.5))
                                        .color(Color32::from_rgb(255, 185, 85).gamma_multiply(a)),
                                )
                                .fill(Color32::from_rgb(38, 30, 18).gamma_multiply(a)),
                            );
                            if r.clicked() && !self.downloading_mpv {
                                self.downloading_mpv = true;
                                let _ = self.backend.tx.send(Request::DownloadLibmpv);
                            }
                        }
                    }
                });
            });
    }

    pub(super) fn open_subscriptions_feed(&mut self) {
        self.page = Page::Subscriptions;
        if self.signed_in && self.subs_items.is_empty() && !self.subs_loading {
            self.subs_loading = true;
            let _ = self
                .backend
                .tx
                .send(Request::LoadSubscriptions { continuation: None });
        }
    }

    pub(super) fn open_history_page(&mut self) {
        self.page = Page::History;
        if self.signed_in && self.sync_history && !self.remote_history_loading {
            self.remote_history_loading = true;
            let _ = self
                .backend
                .tx
                .send(Request::LoadHistory { continuation: None });
        }
    }
}

fn sidebar_section_header(
    ui: &mut egui::Ui,
    title: &str,
    active: bool,
    is_open: bool,
    alpha: f32,
    header_tip: &str,
    toggle_tip: &str,
) -> (bool, bool) {
    let w = ui.available_width();
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::hover());
    let main_rect = Rect::from_min_max(row_rect.min, pos2(row_rect.right() - 28.0, row_rect.bottom()));
    let chev_rect = Rect::from_min_max(pos2(row_rect.right() - 28.0, row_rect.top()), row_rect.max);

    let main_resp = ui.interact(main_rect, ui.scope_id().with((title, "hdr")), Sense::click());
    let chev_resp = ui.interact(chev_rect, ui.scope_id().with((title, "chev")), Sense::click());

    let main_hov = ui
        .ctx()
        .animate_bool_with_time(main_resp.id, main_resp.hovered(), 0.12);
    let chev_hov = ui
        .ctx()
        .animate_bool_with_time(chev_resp.id, chev_resp.hovered(), 0.12);

    if main_resp.hovered() || chev_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if active {
        ui.painter().rect_filled(main_rect, 8.0, theme::CHIP);
    } else if main_hov > 0.01 {
        ui.painter().rect_filled(
            main_rect,
            8.0,
            Color32::from_rgb(28, 28, 30).gamma_multiply(main_hov * alpha),
        );
    }
    if chev_hov > 0.01 {
        ui.painter().circle_filled(
            chev_rect.center(),
            12.0,
            Color32::from_white_alpha((24.0 * chev_hov * alpha) as u8),
        );
    }

    let text_col = if active { Color32::WHITE } else { theme::TEXT }.gamma_multiply(alpha);
    ui.painter().text(
        pos2(main_rect.left() + 10.0, main_rect.center().y),
        Align2::LEFT_CENTER,
        format!("{title}  ›"),
        theme::semibold(13.5),
        text_col,
    );

    let chev_ico = if is_open {
        icon::CHEVRON_DOWN
    } else {
        icon::CHEVRON_RIGHT
    };
    theme::draw_icon_centered(
        ui.painter(),
        chev_rect.center(),
        chev_ico,
        14.0,
        theme::TEXT_DIM.gamma_multiply(alpha),
    );

    let open_clicked = main_resp.on_hover_text(header_tip).clicked();
    let toggle_clicked = chev_resp.on_hover_text(toggle_tip).clicked();
    (open_clicked, toggle_clicked)
}

fn sidebar_expandable_item(
    ui: &mut egui::Ui,
    ico: char,
    label: &str,
    active: bool,
    is_open: bool,
    alpha: f32,
) -> (bool, bool) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 38.0), Sense::hover());
    let main_rect = Rect::from_min_max(rect.min, pos2(rect.right() - 28.0, rect.bottom()));
    let chev_rect = Rect::from_min_max(pos2(rect.right() - 28.0, rect.top()), rect.max);

    let main_resp = ui.interact(main_rect, ui.scope_id().with((label, "pl_main")), Sense::click());
    let chev_resp = ui.interact(chev_rect, ui.scope_id().with((label, "pl_chev")), Sense::click());

    let hover_t = ui
        .ctx()
        .animate_bool_with_time(main_resp.id, main_resp.hovered(), 0.12);
    let chev_hov = ui
        .ctx()
        .animate_bool_with_time(chev_resp.id, chev_resp.hovered(), 0.12);
    if main_resp.hovered() || chev_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let fill = if active {
        theme::CHIP.gamma_multiply(alpha)
    } else if hover_t > 0.01 {
        Color32::from_rgb(28, 28, 30).gamma_multiply(hover_t * alpha)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 10.0, fill);
    }
    if chev_hov > 0.01 {
        ui.painter().circle_filled(
            chev_rect.center(),
            12.0,
            Color32::from_white_alpha((24.0 * chev_hov * alpha) as u8),
        );
    }
    let col = if active { Color32::WHITE } else { theme::TEXT }.gamma_multiply(alpha);
    theme::draw_icon_centered(
        ui.painter(),
        pos2(rect.left() + 22.0, rect.center().y),
        ico,
        17.0,
        col,
    );
    ui.painter().text(
        pos2(rect.left() + 46.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        if active {
            theme::semibold(13.5)
        } else {
            theme::regular(13.5)
        },
        col,
    );
    let chev = if is_open {
        icon::CHEVRON_DOWN
    } else {
        icon::CHEVRON_RIGHT
    };
    theme::draw_icon_centered(
        ui.painter(),
        chev_rect.center(),
        chev,
        13.0,
        theme::TEXT_DIM.gamma_multiply(alpha),
    );
    (
        main_resp.on_hover_text("Open Playlists library").clicked(),
        chev_resp
            .on_hover_text(if is_open {
                "Collapse list"
            } else {
                "Expand list"
            })
            .clicked(),
    )
}

fn sidebar_playlist_item(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    thumbnail_url: Option<&str>,
    active: bool,
    alpha: f32,
) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let bg = if active {
        theme::CHIP.gamma_multiply(alpha)
    } else if hover_t > 0.01 {
        Color32::from_rgb(28, 28, 30).gamma_multiply(hover_t * alpha)
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 10.0, bg);
    }

    let text_col = if active { Color32::WHITE } else { theme::TEXT }.gamma_multiply(alpha);
    if let Some(thumb) = thumbnail_url.filter(|u| !u.is_empty()) {
        if alpha > 0.15 {
            let thumb_rect = Rect::from_min_size(
                pos2(rect.left() + 10.0, rect.center().y - 8.0),
                Vec2::new(24.0, 16.0),
            );
            theme::paint_image_rounded(ui, thumb, thumb_rect, 4);
            if alpha < 0.98 {
                ui.painter().rect_filled(
                    thumb_rect,
                    4.0,
                    theme::BG.gamma_multiply(1.0 - alpha),
                );
            }
        }
    } else {
        theme::draw_icon_centered(
            ui.painter(),
            pos2(rect.left() + 22.0, rect.center().y),
            icon::LIST_MUSIC,
            16.5,
            text_col,
        );
    }
    let max_text_w = (rect.width() - 52.0).max(20.0);
    let display = truncate_text(ui, title, theme::regular(13.0), max_text_w);
    ui.painter().text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        display,
        if active {
            theme::semibold(13.0)
        } else {
            theme::regular(13.0)
        },
        text_col,
    );
    let tip = if subtitle.is_empty() {
        title.to_owned()
    } else {
        format!("{title} · {subtitle}")
    };
    resp.on_hover_text(tip).clicked()
}

fn sidebar_channel_row(
    ui: &mut egui::Ui,
    name: &str,
    avatar_url: Option<&str>,
    is_live: bool,
    active: bool,
    alpha: f32,
) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::click());
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let fill = if active {
        theme::CHIP
    } else if hover_t > 0.01 {
        Color32::from_rgb(28, 28, 30).gamma_multiply(hover_t)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 9.0, fill);
    }
    let av_rect =
        Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(22.0));
    if let Some(u) = avatar_url {
        theme::paint_image_rounded(ui, u, av_rect, 11);
    } else {
        let initial = name.chars().next().unwrap_or('Y').to_ascii_uppercase();
        ui.painter().circle_filled(
            av_rect.center(),
            11.0,
            Color32::from_rgb(56, 56, 64).gamma_multiply(alpha),
        );
        ui.painter().text(
            av_rect.center(),
            Align2::CENTER_CENTER,
            initial.to_string(),
            theme::semibold(10.0),
            Color32::WHITE.gamma_multiply(alpha),
        );
    }
    let max_text_w = (rect.width() - 58.0).max(24.0);
    let display = truncate_text(ui, name, theme::regular(12.8), max_text_w);
    ui.painter().text(
        pos2(rect.left() + 40.0, rect.center().y),
        Align2::LEFT_CENTER,
        display,
        if active {
            theme::semibold(12.8)
        } else {
            theme::regular(12.8)
        },
        if active { Color32::WHITE } else { theme::TEXT }.gamma_multiply(alpha),
    );
    if is_live {
        theme::draw_icon_centered(
            ui.painter(),
            pos2(rect.right() - 12.0, rect.center().y),
            icon::RADIO,
            12.5,
            theme::YT_RED.gamma_multiply(alpha),
        );
    }
    resp.clicked()
}

fn sidebar_item(
    ui: &mut egui::Ui,
    ico: char,
    label: &str,
    short_label: &str,
    active: bool,
    expand_t: f32,
) -> bool {
    sidebar_item_alpha(ui, ico, label, short_label, active, expand_t, 1.0)
}

fn sidebar_item_alpha(
    ui: &mut egui::Ui,
    ico: char,
    label: &str,
    short_label: &str,
    active: bool,
    expand_t: f32,
    alpha: f32,
) -> bool {
    let w = ui.available_width();
    let h = 56.0 + (38.0 - 56.0) * expand_t;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let hover_t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let fill = if active {
        theme::CHIP.gamma_multiply(alpha)
    } else if hover_t > 0.01 {
        Color32::from_rgb(28, 28, 30).gamma_multiply(hover_t * alpha)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 10.0, fill);
    }

    let col = if active { Color32::WHITE } else { theme::TEXT }.gamma_multiply(alpha);

    // Smoothly interpolate icon position from mini centered-top to expanded left-aligned
    let mini_x = rect.center().x;
    let exp_x = rect.left() + 22.0;
    let icon_x = mini_x + (exp_x - mini_x) * expand_t;

    let has_short = !short_label.is_empty();
    let mini_y = if has_short {
        rect.center().y - 7.0
    } else {
        rect.center().y
    };
    let exp_y = rect.center().y;
    let icon_y = mini_y + (exp_y - mini_y) * expand_t;
    let icon_size = 18.5 + (17.0 - 18.5) * expand_t;

    theme::draw_icon_centered(ui.painter(), pos2(icon_x, icon_y), ico, icon_size, col);

    // Mini label underneath icon when collapsed
    if has_short && expand_t < 0.35 {
        let mini_alpha = (1.0 - expand_t / 0.35).clamp(0.0, 1.0);
        ui.painter().text(
            pos2(rect.center().x, rect.center().y + 13.0),
            Align2::CENTER_CENTER,
            short_label,
            theme::medium(9.8),
            col.gamma_multiply(mini_alpha),
        );
    }

    // Expanded label sliding and fading in smoothly
    if expand_t > 0.18 {
        let label_alpha = ((expand_t - 0.18) / 0.65).clamp(0.0, 1.0);
        ui.painter().text(
            pos2(rect.left() + 46.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            if active {
                theme::semibold(13.5)
            } else {
                theme::regular(13.5)
            },
            col.gamma_multiply(label_alpha),
        );
    }

    if expand_t < 0.4 {
        resp.on_hover_text(label).clicked()
    } else {
        resp.clicked()
    }
}

fn truncate_text(ui: &egui::Ui, text: &str, font: egui::FontId, max_w: f32) -> String {
    let mut display = text.to_owned();
    while display.chars().count() > 3
        && ui
            .painter()
            .layout_no_wrap(display.clone(), font.clone(), theme::TEXT)
            .size()
            .x
            > max_w
    {
        display.pop();
        if display.ends_with(' ') {
            display.pop();
        }
    }
    if display.len() < text.len() {
        display.push('…');
    }
    display
}
