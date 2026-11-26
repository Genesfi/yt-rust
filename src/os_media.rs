//! OS media integration (Windows SMTC & hardware media keys via `souvlaki`).

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crossbeam_channel::{Receiver, unbounded};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use souvlaki::{
        MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition,
        PlatformConfig,
    };

    /// How far Windows's SMTC timeline clock may drift before we re-sync it.
    const DRIFT: Duration = Duration::from_secs(2);

    struct SentPlayback {
        playing: bool,
        pos: Duration,
        at: Instant,
    }

    impl SentPlayback {
        fn estimated_pos(&self, now: Instant) -> Duration {
            if self.playing {
                self.pos + now.saturating_duration_since(self.at)
            } else {
                self.pos
            }
        }
    }

    #[derive(PartialEq, Eq)]
    struct SentTrack {
        video_id: String,
        title: String,
        channel: String,
        cover_url: Option<String>,
        duration_secs: u64,
    }

    pub struct OsMedia {
        controls: MediaControls,
        rx: Receiver<MediaControlEvent>,
        track: Option<SentTrack>,
        playback: Option<SentPlayback>,
        resend_playback: bool,
    }

    fn extract_hwnd(cc: &eframe::CreationContext<'_>) -> Option<*mut c_void> {
        cc.window_handle().ok().and_then(|h| match h.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get() as *mut c_void),
            _ => None,
        })
    }

    impl OsMedia {
        pub fn new(
            cc: &eframe::CreationContext<'_>,
            wake: Arc<dyn Fn() + Send + Sync>,
        ) -> Option<Self> {
            let hwnd = extract_hwnd(cc)?;
            let config = PlatformConfig {
                display_name: "yt-rust",
                dbus_name: "yt_rust",
                hwnd: Some(hwnd),
            };
            let mut controls = match MediaControls::new(config) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("SMTC init unavailable: {e}");
                    return None;
                }
            };
            let (tx, rx) = unbounded();
            if let Err(e) = controls.attach(move |ev| {
                let _ = tx.send(ev);
                wake();
            }) {
                log::warn!("SMTC attach failed: {e}");
                return None;
            }
            Some(Self {
                controls,
                rx,
                track: None,
                playback: None,
                resend_playback: false,
            })
        }

        pub fn try_recv(&self) -> Option<MediaControlEvent> {
            self.rx.try_recv().ok()
        }

        pub fn set_track(
            &mut self,
            video_id: &str,
            title: &str,
            channel: &str,
            cover_url: Option<&str>,
            duration_secs: f64,
        ) {
            let rounded_dur = if duration_secs > 0.0 && duration_secs.is_finite() {
                duration_secs.round() as u64
            } else {
                0
            };
            let next = SentTrack {
                video_id: video_id.to_owned(),
                title: title.to_owned(),
                channel: channel.to_owned(),
                cover_url: cover_url.map(str::to_owned),
                duration_secs: rounded_dur,
            };
            if self.track.as_ref() == Some(&next) {
                return;
            }
            let duration = (rounded_dur > 0).then(|| Duration::from_secs(rounded_dur));
            if let Err(e) = self.controls.set_metadata(MediaMetadata {
                title: Some(&next.title),
                artist: Some(&next.channel),
                album: None,
                cover_url: next.cover_url.as_deref(),
                duration,
            }) {
                log::debug!("SMTC set_metadata failed: {e}");
            }
            self.track = Some(next);
            // Windows SMTC resets timeline position on metadata update; re-push next sync.
            self.resend_playback = true;
        }

        pub fn sync_duration_if_unset(&mut self, duration_secs: f64) {
            let Some(cur) = self.track.as_ref() else {
                return;
            };
            if cur.duration_secs == 0 && duration_secs > 1.0 && duration_secs.is_finite() {
                let id = cur.video_id.clone();
                let title = cur.title.clone();
                let channel = cur.channel.clone();
                let cover = cur.cover_url.clone();
                self.set_track(&id, &title, &channel, cover.as_deref(), duration_secs);
            }
        }

        pub fn sync_playback(&mut self, active: bool, paused: bool, pos_secs: f64) {
            if !active {
                if self.playback.take().is_some() {
                    let _ = self.controls.set_playback(MediaPlayback::Stopped);
                }
                self.track = None;
                self.resend_playback = false;
                return;
            }
            let now = Instant::now();
            let playing = !paused;
            let pos = if pos_secs.is_finite() && pos_secs > 0.0 {
                Duration::from_secs_f64(pos_secs)
            } else {
                Duration::ZERO
            };
            let needs_send = self.resend_playback
                || self.playback.as_ref().is_none_or(|s| {
                    let slack = if s.playing { DRIFT } else { Duration::ZERO };
                    s.playing != playing || pos.abs_diff(s.estimated_pos(now)) > slack
                });
            if !needs_send {
                return;
            }
            self.resend_playback = false;
            self.playback = Some(SentPlayback {
                playing,
                pos,
                at: now,
            });
            let progress = Some(MediaPosition(pos));
            let state = if playing {
                MediaPlayback::Playing { progress }
            } else {
                MediaPlayback::Paused { progress }
            };
            let _ = self.controls.set_playback(state);
        }
    }

    pub fn seek_target(ev: &MediaControlEvent, pos_secs: f64, dur_secs: f64) -> Option<f64> {
        let step = 10.0;
        let to = match *ev {
            MediaControlEvent::SetPosition(MediaPosition(at)) => at.as_secs_f64(),
            MediaControlEvent::Seek(souvlaki::SeekDirection::Forward) => pos_secs + step,
            MediaControlEvent::Seek(souvlaki::SeekDirection::Backward) => {
                (pos_secs - step).max(0.0)
            }
            MediaControlEvent::SeekBy(souvlaki::SeekDirection::Forward, d) => {
                pos_secs + d.as_secs_f64()
            }
            MediaControlEvent::SeekBy(souvlaki::SeekDirection::Backward, d) => {
                (pos_secs - d.as_secs_f64()).max(0.0)
            }
            _ => return None,
        };
        Some(if dur_secs > 0.0 {
            to.clamp(0.0, dur_secs)
        } else {
            to.max(0.0)
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use souvlaki::SeekDirection;

        #[test]
        fn seek_target_clamps_within_duration() {
            assert_eq!(
                seek_target(&MediaControlEvent::SetPosition(MediaPosition(Duration::from_secs(42))), 10.0, 200.0),
                Some(42.0)
            );
            assert_eq!(
                seek_target(&MediaControlEvent::SetPosition(MediaPosition(Duration::from_secs(500))), 10.0, 200.0),
                Some(200.0)
            );
            assert_eq!(
                seek_target(&MediaControlEvent::Seek(SeekDirection::Forward), 100.0, 200.0),
                Some(110.0)
            );
            assert_eq!(
                seek_target(&MediaControlEvent::Seek(SeekDirection::Backward), 4.0, 200.0),
                Some(0.0)
            );
            assert_eq!(
                seek_target(&MediaControlEvent::Next, 10.0, 200.0),
                None
            );
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::sync::Arc;

    pub struct OsMedia;

    impl OsMedia {
        pub fn new(
            _cc: &eframe::CreationContext<'_>,
            _wake: Arc<dyn Fn() + Send + Sync>,
        ) -> Option<Self> {
            None
        }

        pub fn set_track(
            &mut self,
            _video_id: &str,
            _title: &str,
            _channel: &str,
            _cover_url: Option<&str>,
            _duration_secs: f64,
        ) {
        }

        pub fn sync_duration_if_unset(&mut self, _duration_secs: f64) {}

        pub fn sync_playback(&mut self, _active: bool, _paused: bool, _pos_secs: f64) {}
    }
}

pub use imp::OsMedia;
#[cfg(windows)]
pub use imp::seek_target;
