// SPDX-License-Identifier: GPL-3.0-only

//! The playback session behind the player page, apart from its drawing: it opens files,
//! filters and reduces player events into a [`ViewState`], saves the position
//! (LIB-006) and turns shortcut actions into player commands.

use std::path::{Path, PathBuf};

use nikomiru_core::{Clock, Config};
use nikomiru_library::progress::ProgressSaver;
use nikomiru_library::{Library, VideoRecord, Writer};
use nikomiru_player::control::{self, HoldToSpeed, LoadToken, Stamped};
use nikomiru_player::player::{EndFile, Track};
use nikomiru_player::{LoadRequest, PlayerEvent, PlayerHandle, options};

use crate::controls::ControlsVisibility;
pub use crate::dictionaries::DictionaryService;
use crate::dictionaries::{DictionaryPage, Effect};
use crate::hover::Hover;
use crate::i18n::Translator;
use crate::popup::{Command, Origin, Popups, Rect, Reply};
use crate::screensaver::ScreensaverPolicy;
use crate::shortcuts::{Action, Bindings, Context, Modifiers};
use crate::subtitles::{Loaded, Obscure, Request, Source, Subtitles};
use nikomiru_subs::select::{self, Choice, EmbeddedTrack};
use nikomiru_subs::timeline::Jump;

/// What the view must do after an action: the window's business, not the
/// session's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEffect {
    /// Nothing for the UI.
    None,
    /// Toggle fullscreen.
    ToggleFullscreen,
    /// Back: leave fullscreen (later: close popups, go to the library).
    Back,
    /// Show the file dialog.
    OpenDialog,
    /// Show or hide the subtitle list (SUB-015).
    ToggleSubList,
    /// Open the subtitle list with the search field focused.
    SubSearch,
}

/// Milestones of a load, for the GUI test harness (TEST §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// The file is open.
    FileLoaded,
    /// The first position after the load: the video is moving.
    FirstFrame,
    /// The file could not be played: the message id of the error.
    Error(&'static str),
    /// A cue became visible (T-PERF-07): its index and the position.
    CueShown {
        /// Index of the cue.
        index: usize,
        /// Position in milliseconds when it appeared.
        time_pos_ms: i64,
    },
    /// A dictionary import finished well.
    DictionaryImported,
    /// A popup was drawn with its content (T-PERF-06).
    PopupRendered {
        /// Its level (0 = from the subtitles).
        level: usize,
        /// The query.
        query: String,
        /// Headwords shown, in order.
        headwords: Vec<String>,
    },
}

/// State shown by the player page.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// A file is loaded.
    pub loaded: bool,
    /// File name, for the title.
    pub title: String,
    /// Position in seconds.
    pub time_pos: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// Paused.
    pub paused: bool,
    /// Volume in percent.
    pub volume: f64,
    /// Muted.
    pub muted: bool,
    /// Speed.
    pub speed: f64,
    /// Audio tracks.
    pub audio_tracks: Vec<Track>,
    /// Selected audio track.
    pub aid: Option<i64>,
    /// Number of chapters.
    pub chapters: usize,
    /// Error to show, already translated (PLY-012, ERR-001).
    pub error: Option<String>,
    /// Display size of the video (anamorphic pixels applied), 0 if unknown.
    pub video_size: (i64, i64),
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            loaded: false,
            title: String::new(),
            time_pos: 0.0,
            duration: 0.0,
            paused: true,
            volume: 100.0,
            muted: false,
            speed: 1.0,
            audio_tracks: Vec::new(),
            aid: None,
            chapters: 0,
            error: None,
            video_size: (0, 0),
        }
    }
}

/// Keeps the screen on (PLY-013); the application talks to the desktop.
pub trait IdleInhibitor: Send {
    /// Inhibits the screensaver, or releases the inhibition.
    fn set(&self, inhibit: bool);
}

/// Loads subtitles away from the UI thread: decoding (SUB-005) and the
/// extraction of embedded tracks (SUB-007). `done` may run on any thread.
pub trait SubtitleSource: Send {
    /// Loads `request` and calls `done` with the result or a translated error.
    fn load(&self, request: Request, done: Box<dyn FnOnce(Result<Loaded, String>) + Send>);
}

/// Everything the session needs from the application.
pub struct Services {
    /// The player.
    pub player: PlayerHandle,
    /// The configuration.
    pub config: Config,
    /// The library, if it could be opened.
    pub library: Option<std::sync::Arc<Library>>,
    /// The interface strings.
    pub translator: Translator,
    /// Time source.
    pub clock: Box<dyn Clock>,
    /// Startup problems, already translated (invalid settings, read-only
    /// library…): shown until the first file opens (ERR-001).
    pub notices: Vec<String>,
    /// The screensaver inhibition.
    pub inhibitor: Box<dyn IdleInhibitor>,
    /// The subtitle loader.
    pub subtitle_source: Box<dyn SubtitleSource>,
    /// The dictionaries.
    pub dictionaries: Box<dyn DictionaryService>,
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Services")
            .field("player", &self.player)
            .finish()
    }
}

struct Current {
    uid: Option<String>,
    path: PathBuf,
}

/// The playback session.
pub struct Session {
    services: Services,
    bindings: Bindings,
    token: LoadToken,
    saver: ProgressSaver,
    hold: HoldToSpeed,
    controls: ControlsVisibility,
    screensaver: ScreensaverPolicy,
    inhibited: bool,
    renderer_ready: bool,
    pending: Option<PathBuf>,
    current: Option<Current>,
    /// What the page shows.
    pub view: ViewState,
    /// Toasts waiting to be shown (translated).
    pub toasts: Vec<String>,
    /// Load milestones not yet taken by the UI layer.
    pub events: Vec<UiEvent>,
    first_frame: bool,
    /// The subtitles of the current file.
    pub subs: Subtitles,
    /// The subtitle sources of the current file, for the menu.
    sub_sources: Vec<Source>,
    /// Languages of embedded tracks, for the menu labels.
    sub_tracks: Vec<Track>,
    sub_generation: u64,
    sub_loads: Vec<(u64, Request)>,
    sub_auto_done: bool,
    /// Changes whenever the cue list changes, so the UI rebuilds it.
    pub sub_list_version: u64,
    /// The dictionary popups.
    pub popups: Popups,
    lookups: Vec<(u64, String)>,
    /// The dictionaries page.
    pub dict_page: DictionaryPage,
    dict_imports: Vec<DictImport>,
    /// Library writes, off the UI thread (LIB-006 saves every second).
    writer: Option<Writer>,
    /// Lookup on hover (POP-003).
    hover: Hover,
    /// The pointer is over a popup.
    popup_hovered: bool,
    /// A cue is being replayed from the popup: mpv stops at its end (SUB-013).
    replaying: bool,
}

/// An import to start, taken by the UI layer.
#[derive(Debug)]
pub struct DictImport {
    /// The zip.
    pub zip: PathBuf,
    /// The dictionary it updates.
    pub replace: Option<String>,
    /// Matches its progress and its end.
    pub generation: u64,
    /// Stops it.
    pub cancel: nikomiru_core::Cancel,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("view", &self.view).finish()
    }
}

/// `file:///path%20x` or a plain path → path.
pub fn url_to_path(url: &str) -> PathBuf {
    let Some(rest) = url.strip_prefix("file://") else {
        return PathBuf::from(url);
    };
    // bytes, not `&str` slices: a `%` before a multibyte character must not panic
    let hex = |b: u8| {
        char::from(b)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(high << 4 | low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(out))
}

impl Drop for Session {
    /// Every way out (window closed, quit, signal) saves the position (LIB-006).
    fn drop(&mut self) {
        self.flush_position();
        if self.inhibited {
            self.services.inhibitor.set(false);
        }
    }
}

impl Session {
    /// A new session.
    pub fn new(services: Services) -> Self {
        let bindings = Bindings::with_overrides(&services.config.shortcuts)
            .unwrap_or_else(|_| Bindings::defaults());
        let view = ViewState {
            error: (!services.notices.is_empty()).then(|| services.notices.join("\n")),
            ..ViewState::default()
        };
        let services_obscure = services.config.subtitles.obscure.clone();
        let popup_size = (
            services.config.popup.max_width,
            services.config.popup.max_height,
        );
        let writer = services
            .library
            .as_ref()
            .filter(|l| !l.read_only())
            .map(|l| Writer::start(std::sync::Arc::clone(l)));
        Self {
            services,
            bindings,
            token: LoadToken::default(),
            saver: ProgressSaver::default(),
            hold: HoldToSpeed::default(),
            controls: ControlsVisibility::default(),
            screensaver: ScreensaverPolicy::default(),
            inhibited: false,
            renderer_ready: false,
            pending: None,
            current: None,
            view,
            toasts: Vec::new(),
            events: Vec::new(),
            first_frame: false,
            subs: Subtitles::with_mode(Obscure::from_config(&services_obscure)),
            sub_sources: Vec::new(),
            sub_tracks: Vec::new(),
            sub_generation: 0,
            sub_loads: Vec::new(),
            sub_auto_done: false,
            sub_list_version: 0,
            popups: Popups::new(popup_size),
            lookups: Vec::new(),
            dict_page: DictionaryPage::default(),
            dict_imports: Vec::new(),
            writer,
            hover: Hover::default(),
            popup_hovered: false,
            replaying: false,
        }
    }

    fn tr(&self, id: &str, args: &[(&str, String)]) -> String {
        self.services.translator.tr(id, args)
    }

    /// The player, for the GUI test harness's `mpv_get`.
    #[cfg(feature = "test-harness")]
    pub fn player(&self) -> &nikomiru_player::PlayerHandle {
        &self.services.player
    }

    /// The interface strings.
    pub fn translator(&self) -> &Translator {
        &self.services.translator
    }

    /// The video item has a render context: a pending file can be loaded now.
    pub fn renderer_ready(&mut self) {
        self.renderer_ready = true;
        if let Some(path) = self.pending.take() {
            self.open(&path);
        }
    }

    /// Opens a file (LIB-001). Waits for the renderer if needed (§5.2.2).
    pub fn open(&mut self, path: &Path) {
        if !self.renderer_ready {
            self.pending = Some(path.to_path_buf());
            return;
        }
        self.flush_position();
        self.end_replay();
        // the reads below must see the positions still queued (the same file
        // reopened at once)
        if let Some(w) = &self.writer {
            w.wait();
        }
        self.view.error = None;
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let meta = std::fs::metadata(&path).ok();
        let library = self.services.library.as_ref().filter(|l| !l.read_only());
        let mut record = VideoRecord::default();
        let uid = library.and_then(|lib| {
            let uid = lib
                .uid_for_path(&path)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()?;
            record = lib
                .video(&uid)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()
                .flatten()
                .unwrap_or_else(|| VideoRecord {
                    path: path.to_string_lossy().into_owned(),
                    ..VideoRecord::default()
                });
            if let Some(m) = &meta {
                record.size = m.len();
                record.mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs() as i64);
            }
            Some(uid)
        });
        if let (Some(uid), Some(w)) = (&uid, &self.writer) {
            let (uid, record) = (uid.clone(), record.clone());
            w.run(move |lib| {
                if let Err(e) = lib.put_video(&uid, &record) {
                    tracing::warn!("library: {e}");
                }
            });
        }
        let (options, report) = options::load_options(&self.services.config.player);
        // Names only: a raw option's value may hold a token (PRV-002).
        for line in report {
            match line {
                options::Line::Apply { .. } => {}
                options::Line::Ignore { line, name, reason } => {
                    tracing::warn!("mpv option {name} (line {line}) ignored: {reason:?}");
                }
                options::Line::Invalid { line, error } => {
                    tracing::warn!("mpv options, line {line}: {error}");
                }
            }
        }
        let request = LoadRequest {
            start: (record.last_position_ms > 0).then(|| record.last_position_ms as f64 / 1000.0),
            options,
            aid: record.aid,
            volume: record.volume,
            speed: record.speed,
        };
        self.saver.reset();
        self.first_frame = false;
        self.reset_subtitles();
        self.view.loaded = false;
        self.view.title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        tracing::info!(
            "opening {} (resume at {:?} s)",
            path.display(),
            request.start
        );
        // the options mpv rejects are logged by the player's command thread
        match self.services.player.load(&path.to_string_lossy(), request) {
            Ok(token) => self.token = token,
            Err(e) => {
                tracing::error!("load failed: {e}");
                self.view.error = Some(self.services.translator.error(&e));
            }
        }
        self.current = Some(Current { uid, path });
    }

    /// A player event; returns whether the view changed.
    pub fn on_event(&mut self, event: Stamped<PlayerEvent>) -> bool {
        let Some(event) = event.current(self.token) else {
            return false;
        };
        let now = self.services.clock.monotonic();
        match event {
            PlayerEvent::FileLoaded => {
                tracing::info!("file loaded");
                self.events.push(UiEvent::FileLoaded);
                self.view.loaded = true;
                self.saver.ready();
            }
            PlayerEvent::TimePos(t) => {
                if self.view.loaded && !self.first_frame {
                    self.first_frame = true;
                    self.events.push(UiEvent::FirstFrame);
                }
                self.view.time_pos = t;
                let pos_ms = (t * 1000.0).round() as i64;
                for index in self.subs.update(pos_ms) {
                    self.events.push(UiEvent::CueShown {
                        index,
                        time_pos_ms: pos_ms,
                    });
                }
                if let Some(ms) = self.saver.position((t * 1000.0) as u64, now) {
                    self.save_position(ms);
                }
            }
            PlayerEvent::Duration(d) => self.view.duration = d,
            PlayerEvent::Pause(p) => {
                self.view.paused = p;
                self.controls.set_playing(!p, now);
            }
            PlayerEvent::Speed(s) => self.view.speed = s,
            PlayerEvent::Volume(v) => self.view.volume = v,
            PlayerEvent::Mute(m) => self.view.muted = m,
            PlayerEvent::Tracks(t) => {
                self.sub_tracks = t.iter().filter(|t| t.kind == "sub").cloned().collect();
                self.view.audio_tracks = t.into_iter().filter(|t| t.kind == "audio").collect();
                self.discover_subtitles();
            }
            PlayerEvent::VideoSize(w, h) => self.view.video_size = (w, h),
            PlayerEvent::Aid(a) => self.view.aid = a,
            PlayerEvent::Chapters(c) => self.view.chapters = c.len(),
            PlayerEvent::EndFile(EndFile::Error(e)) => {
                tracing::warn!("playback error: {e}");
                let file = self.view.title.clone();
                self.events
                    .push(UiEvent::Error(nikomiru_core::Localized::message_id(&e)));
                let reason = self.services.translator.error(&e);
                self.view.loaded = false;
                self.view.error =
                    Some(self.tr("error-file", &[("file", file), ("reason", reason)]));
            }
            _ => return false,
        }
        self.update_screensaver();
        true
    }

    /// Periodic update (from the UI's timer): the screensaver policy, and a
    /// hover popup the pointer left (POP-003).
    pub fn tick(&mut self) {
        self.update_screensaver();
        let now = self.services.clock.monotonic();
        if self
            .hover
            .expired(now, self.services.config.lookup.resume_on_leave)
        {
            self.close_all_popups();
        }
    }

    /// The end set by a replay from the popup no longer applies: the user
    /// took control, a popup closed or another file opened.
    fn end_replay(&mut self) {
        if std::mem::take(&mut self.replaying) {
            let _ = self.services.player.stop_at(None);
        }
    }

    /// "Replay sentence" (POP-015, SUB-013): the popup's cue from its start,
    /// pausing at its end.
    pub fn popup_replay(&mut self) {
        let Some(cue) = self.popups.root_cue() else {
            return;
        };
        let t = self.subs.timeline();
        let Some(c) = t.cues().get(cue) else {
            return;
        };
        let (start, end) = (t.start_pos(cue), c.end_ms + t.delay_ms);
        let p = &self.services.player;
        let _ = p.stop_at(Some(end as f64 / 1000.0));
        let _ = p.seek_absolute(start as f64 / 1000.0);
        let _ = p.set_pause(false);
        self.replaying = true;
    }

    /// "Go to sentence" (POP-015): the popups close and playback goes to the
    /// start of the cue.
    pub fn popup_goto(&mut self) {
        let Some(cue) = self
            .popups
            .root_cue()
            .filter(|&c| c < self.subs.timeline().cues().len())
        else {
            return;
        };
        let start = self.subs.timeline().start_pos(cue);
        self.close_all_popups();
        let _ = self.services.player.seek_absolute(start as f64 / 1000.0);
    }

    /// Tab, Shift+Tab, Alt+wheel in the popup (POP-014).
    pub fn popup_entry(&mut self, delta: i64) {
        self.popups.move_entry(delta);
    }

    fn update_screensaver(&mut self) {
        let now = self.services.clock.monotonic();
        let playing = self.view.loaded && !self.view.paused;
        self.screensaver.set_playing(playing, now);
        let want = self.screensaver.inhibit(now);
        if want != self.inhibited {
            self.services.inhibitor.set(want);
            self.inhibited = want;
        }
    }

    fn save_position(&self, ms: u64) {
        let (Some(w), Some(Current { uid: Some(uid), .. })) =
            (self.writer.as_ref(), self.current.as_ref())
        else {
            return;
        };
        let at = self
            .services
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let uid = uid.clone();
        w.run(move |lib| {
            if let Err(e) = lib.save_position(&uid, ms, at) {
                tracing::warn!("saving the position: {e}");
            }
        });
    }

    /// Exit or file change: write the last position and the per-file settings.
    pub fn flush_position(&mut self) {
        if let Some(ms) = self.saver.flush() {
            self.save_position(ms);
        }
        let (Some(w), Some(Current { uid: Some(uid), .. })) =
            (self.writer.as_ref(), self.current.as_ref())
        else {
            return;
        };
        let (uid, aid, volume, speed, duration) = (
            uid.clone(),
            self.view.aid,
            self.view.volume,
            self.view.speed,
            self.view.duration,
        );
        w.run(move |lib| {
            if let Ok(Some(mut rec)) = lib.video(&uid) {
                rec.aid = aid;
                rec.volume = Some(volume);
                rec.speed = Some(speed);
                if duration > 0.0 {
                    rec.duration_ms = Some((duration * 1000.0) as u64);
                }
                let _ = lib.put_video(&uid, &rec);
            }
        });
    }

    /// The mouse moved over the window (PLY-004).
    pub fn mouse_moved(&mut self) {
        self.controls.mouse_moved(self.services.clock.monotonic());
    }

    /// Whether the control bar and the cursor are visible now (PLY-004).
    pub fn controls_visible(&self) -> bool {
        !self.view.loaded || self.controls.visible(self.services.clock.monotonic())
    }

    /// A key event: the physical key as an evdev code (KEY-002) and the
    /// modifiers. `text_focus`: a text field has the keyboard (KEY-004). Returns
    /// the effect and whether a binding took the key.
    pub fn key(
        &mut self,
        evdev: u32,
        mods: Modifiers,
        pressed: bool,
        auto_repeat: bool,
        text_focus: bool,
    ) -> (UiEffect, bool) {
        let Some(chord) = crate::shortcuts::chord(evdev, mods) else {
            if pressed {
                tracing::debug!("key: evdev code {evdev} has no name");
            }
            return (UiEffect::None, false);
        };
        // a popup's keys first, then the player's (KEY-003)
        let action = if self.popups.is_open() {
            self.bindings
                .action(Context::Popup, &chord, text_focus)
                .or_else(|| self.bindings.action(Context::Player, &chord, text_focus))
        } else {
            self.bindings.action(Context::Player, &chord, text_focus)
        };
        if pressed && !auto_repeat {
            tracing::debug!("key {chord} → {}", action.map_or("nothing", Action::name));
        }
        let Some(action) = action else {
            return (UiEffect::None, false);
        };
        if !pressed {
            if action == Action::HoldSpeed {
                self.release_hold();
            }
            return (UiEffect::None, true);
        }
        if action == Action::HoldSpeed {
            if let Some(speed) = self.hold.key_down(self.view.speed, auto_repeat) {
                let _ = self.services.player.set_speed(speed);
            }
            return (UiEffect::None, true);
        }
        (self.action(action), true)
    }

    /// Ends hold-to-speed (PLY-002): on the key's release, or when the window
    /// loses the keyboard and the release will never arrive.
    pub fn release_hold(&mut self) {
        if let Some(speed) = self.hold.key_up() {
            let _ = self.services.player.set_speed(speed);
        }
    }

    /// Runs an action (from a key or a button).
    pub fn action(&mut self, action: Action) -> UiEffect {
        let p = &self.services.player;
        let step = control::seek_step(self.services.config.player.seek_seconds);
        let result = match action {
            Action::PlayPause => {
                self.popups.playback_changed_by_user();
                self.end_replay();
                self.services.player.toggle_pause()
            }
            Action::PopupNext | Action::PopupPrev => {
                self.popup_entry(if action == Action::PopupNext { 1 } else { -1 });
                Ok(())
            }
            Action::ReplayCue if self.popups.is_open() => {
                self.popup_replay();
                Ok(())
            }
            Action::SeekBack => p.seek_relative(-step),
            Action::SeekForward => p.seek_relative(step),
            Action::VolumeUp | Action::VolumeDown => {
                let dir = if action == Action::VolumeUp { 1 } else { -1 };
                let v = control::step_volume(self.view.volume, dir);
                self.toasts
                    .push(self.tr("toast-volume", &[("value", format!("{v:.0}"))]));
                p.set_volume(v)
            }
            Action::Mute => {
                let key = if self.view.muted {
                    "toast-unmuted"
                } else {
                    "toast-muted"
                };
                self.toasts.push(self.tr(key, &[]));
                p.set_mute(!self.view.muted)
            }
            Action::SpeedUp | Action::SpeedDown | Action::SpeedReset => {
                let s = match action {
                    Action::SpeedUp => control::step_speed(self.view.speed, 1),
                    Action::SpeedDown => control::step_speed(self.view.speed, -1),
                    _ => 1.0,
                };
                self.toasts
                    .push(self.tr("toast-speed", &[("value", format!("{s:.2}"))]));
                p.set_speed(s)
            }
            Action::FramePrev => p.frame_step(false),
            Action::FrameNext => p.frame_step(true),
            Action::ChapterPrev => p.chapter(-1),
            Action::ChapterNext => p.chapter(1),
            Action::SubPrev | Action::SubNext => {
                let pos = (self.view.time_pos * 1000.0) as i64;
                let secs = i64::from(self.services.config.player.seek_seconds.max(1));
                let t = self.subs.timeline();
                let jump = if action == Action::SubPrev {
                    t.previous(pos, true, secs)
                } else {
                    t.next(pos, secs)
                };
                match jump {
                    Jump::To(ms) => p.seek_absolute(ms as f64 / 1000.0),
                    Jump::Seek(s) => p.seek_relative(s as f64),
                }
            }
            Action::ReplayCue | Action::ReplayPrevCue => {
                let pos = (self.view.time_pos * 1000.0) as i64;
                let t = self.subs.timeline();
                let target = if action == Action::ReplayCue {
                    t.replay(pos).map(|(start, _)| start)
                } else {
                    match t.previous(pos, false, 0) {
                        Jump::To(ms) => Some(ms),
                        Jump::Seek(_) => None,
                    }
                };
                match target {
                    Some(ms) => p.seek_absolute(ms as f64 / 1000.0),
                    None => Ok(()),
                }
            }
            Action::SubDelayMinus | Action::SubDelayPlus => {
                let steps = if action == Action::SubDelayPlus {
                    1
                } else {
                    -1
                };
                let d = self.subs.timeline_mut().nudge(steps);
                self.after_delay_change(d);
                Ok(())
            }
            Action::SubAlignPrev | Action::SubAlignNext => {
                let pos = (self.view.time_pos * 1000.0) as i64;
                if let Some(d) = self
                    .subs
                    .timeline_mut()
                    .align(pos, action == Action::SubAlignNext)
                {
                    self.after_delay_change(d);
                }
                Ok(())
            }
            Action::SubBlur | Action::SubHide => {
                let wanted = if action == Action::SubBlur {
                    Obscure::Blur
                } else {
                    Obscure::Hidden
                };
                self.subs.mode = if self.subs.mode == wanted {
                    Obscure::None
                } else {
                    wanted
                };
                let key = match self.subs.mode {
                    Obscure::None => "toast-sub-visible",
                    Obscure::Blur => "toast-sub-blurred",
                    Obscure::Hidden => "toast-sub-hidden",
                };
                self.toasts.push(self.tr(key, &[]));
                Ok(())
            }
            Action::SubToggle => {
                self.subs.disabled = !self.subs.disabled;
                let key = if self.subs.disabled {
                    "toast-sub-off"
                } else {
                    "toast-sub-on"
                };
                self.toasts.push(self.tr(key, &[]));
                Ok(())
            }
            Action::SubList => return UiEffect::ToggleSubList,
            Action::SubSearch => return UiEffect::SubSearch,
            Action::Fullscreen => return UiEffect::ToggleFullscreen,
            Action::Back if self.popups.is_open() => {
                self.close_popup();
                return UiEffect::None;
            }
            Action::Back => return UiEffect::Back,
            Action::OpenFile => return UiEffect::OpenDialog,
            Action::HoldSpeed => Ok(()),
        };
        if let Err(e) = result {
            // a command mpv rejects is not fatal (e.g. a chapter past the last)
            tracing::warn!("player command {} failed: {e}", action.name());
        }
        UiEffect::None
    }

    /// Absolute seek from the progress bar.
    pub fn seek_to(&self, seconds: f64) {
        let _ = self.services.player.seek_absolute(seconds);
    }

    /// Volume from the slider.
    pub fn set_volume(&self, volume: f64) {
        let _ = self.services.player.set_volume(volume);
    }

    /// Audio track from the menu.
    pub fn select_audio(&self, id: i64) {
        let _ = self.services.player.set_aid((id > 0).then_some(id));
    }

    /// Current file, if any.
    pub fn current_path(&self) -> Option<&Path> {
        self.current.as_ref().map(|c| c.path.as_path())
    }
}

/// Subtitles (SUB-005…SUB-016).
impl Session {
    /// The cues were replaced or removed: the list starts over, and the popups
    /// close, since they looked up words of the cues that were (and replay or
    /// go to them).
    fn cues_changed(&mut self) {
        self.sub_list_version += 1;
        if self.popups.is_open() {
            self.close_all_popups();
        }
    }

    fn reset_subtitles(&mut self) {
        self.subs.clear(None);
        self.sub_sources.clear();
        self.sub_tracks.clear();
        self.sub_generation += 1;
        self.sub_loads.clear();
        self.sub_auto_done = false;
        self.cues_changed();
    }

    /// The subtitle sources of the current file: external files next to it
    /// (SUB-006) and the embedded tracks; the first time, the automatic choice
    /// (SUB-008).
    fn discover_subtitles(&mut self) {
        let Some(video) = self.current.as_ref().map(|c| c.path.clone()) else {
            return;
        };
        let entries: Vec<PathBuf> = video
            .parent()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        let externals = select::external_files(&video, &entries);
        let mut sources: Vec<Source> = externals
            .iter()
            .map(|e| Source::File(e.path.clone()))
            .collect();
        // a file the user opened stays in the menu
        if let Some(Source::File(p)) = &self.subs.source
            && !sources.contains(&Source::File(p.clone()))
        {
            sources.push(Source::File(p.clone()));
        }
        let mut embedded = Vec::new();
        for t in &self.sub_tracks {
            if t.external {
                continue;
            }
            let codec = t.codec.clone().unwrap_or_default();
            let text = nikomiru_media::extract::is_text_codec(&codec);
            sources.push(match (text, t.ff_index) {
                (true, Some(ff_index)) => Source::Embedded {
                    id: t.id,
                    ff_index,
                    codec,
                },
                _ => Source::Bitmap(t.id),
            });
            embedded.push(EmbeddedTrack {
                id: t.id,
                language: t.lang.clone(),
                title: t.title.clone(),
                text,
            });
        }
        self.sub_sources = sources;
        if self.sub_auto_done || !self.view.loaded {
            return;
        }
        self.sub_auto_done = true;
        let preferred = self.services.config.subtitles.preferred_languages.clone();
        let choice = preferred
            .iter()
            .find_map(|lang| select::choose(None, &externals, &embedded, lang));
        let source = match choice {
            Some(Choice::External(p)) => Some(Source::File(p)),
            Some(Choice::Embedded(id)) => self
                .sub_sources
                .iter()
                .find(
                    |s| matches!(s, Source::Embedded { id: i, .. } | Source::Bitmap(i) if *i == id),
                )
                .cloned(),
            None => None,
        };
        if let Some(source) = source {
            self.use_subtitles(source);
        }
    }

    /// Shows `source`: mpv draws graphic tracks; text ones are loaded in the
    /// background and drawn by the overlay.
    fn use_subtitles(&mut self, source: Source) {
        self.sub_generation += 1;
        self.sub_loads.clear();
        match &source {
            Source::Bitmap(id) => {
                let _ = self.services.player.show_bitmap_subtitles(Some(*id));
                self.subs.clear(Some(source));
                self.cues_changed();
            }
            Source::File(_) | Source::Embedded { .. } => {
                let _ = self.services.player.show_bitmap_subtitles(None);
                let Some(video) = self.current.as_ref().map(|c| c.path.clone()) else {
                    return;
                };
                self.sub_loads.push((
                    self.sub_generation,
                    Request {
                        video,
                        source,
                        encoding: self.subs.forced_encoding.clone(),
                        duration_ms: (self.view.duration * 1000.0) as i64,
                    },
                ));
            }
        }
    }

    /// Loads waiting to be started by the UI layer, with their generation.
    pub fn take_subtitle_loads(&mut self) -> Vec<(u64, Request)> {
        std::mem::take(&mut self.sub_loads)
    }

    /// Starts `request` on the loader; `done` gets the result.
    pub fn start_subtitle_load(
        &self,
        request: Request,
        done: Box<dyn FnOnce(Result<Loaded, String>) + Send>,
    ) {
        self.services.subtitle_source.load(request, done);
    }

    /// A load finished. A result for an older request is dropped.
    pub fn subtitles_loaded(
        &mut self,
        generation: u64,
        source: Source,
        result: Result<Loaded, String>,
    ) {
        if generation != self.sub_generation {
            return;
        }
        match result {
            Ok(loaded) => {
                let name = source_name(&source);
                let encoding = loaded.encoding.as_deref().map(|e| self.encoding_label(e));
                self.subs.set_track(source, loaded);
                self.subs.update((self.view.time_pos * 1000.0) as i64);
                self.cues_changed();
                let text = match encoding {
                    Some(e) => self.tr(
                        "toast-sub-loaded-encoding",
                        &[("name", name), ("encoding", e)],
                    ),
                    None => self.tr("toast-sub-loaded", &[("name", name)]),
                };
                self.toasts.push(text);
            }
            Err(reason) => {
                tracing::warn!("subtitles: {reason}");
                self.subs.clear(None);
                self.cues_changed();
                self.toasts
                    .push(self.tr("error-subtitles", &[("reason", reason)]));
            }
        }
    }

    /// Menu choice: a source key, or `"off"`.
    pub fn select_subtitles(&mut self, key: &str) {
        if key == "off" {
            self.sub_generation += 1;
            let _ = self.services.player.show_bitmap_subtitles(None);
            self.subs.clear(None);
            self.cues_changed();
            return;
        }
        if let Some(source) = self.sub_sources.iter().find(|s| s.key() == key).cloned() {
            self.use_subtitles(source);
        }
    }

    /// A subtitle file chosen by the user.
    pub fn open_subtitle_file(&mut self, path: &Path) {
        let source = Source::File(path.to_path_buf());
        if !self.sub_sources.contains(&source) {
            self.sub_sources.push(source.clone());
        }
        self.use_subtitles(source);
    }

    /// Forces an encoding (`""` = automatic) and reloads the file (SUB-005).
    pub fn force_encoding(&mut self, encoding: &str) {
        self.subs.forced_encoding = (!encoding.is_empty()).then(|| encoding.to_string());
        if let Some(source @ Source::File(_)) = self.subs.source.clone() {
            self.use_subtitles(source);
        }
    }

    /// The subtitle menu: `{key, kind, name, lang, title, codec}`.
    pub fn subtitle_menu(&self) -> serde_json::Value {
        let items = self.sub_sources.iter().map(|s| {
            let track = |id: i64| self.sub_tracks.iter().find(|t| t.id == id);
            match s {
                Source::File(p) => serde_json::json!({
                    "key": s.key(), "kind": "file", "name": source_name(s),
                    "path": p.display().to_string(),
                }),
                Source::Embedded { id, codec, .. } => serde_json::json!({
                    "key": s.key(), "kind": "text", "name": format!("#{id}"),
                    "lang": track(*id).and_then(|t| t.lang.clone()),
                    "title": track(*id).and_then(|t| t.title.clone()),
                    "codec": codec,
                }),
                Source::Bitmap(id) => serde_json::json!({
                    "key": s.key(), "kind": "bitmap", "name": format!("#{id}"),
                    "lang": track(*id).and_then(|t| t.lang.clone()),
                    "title": track(*id).and_then(|t| t.title.clone()),
                    "codec": track(*id).and_then(|t| t.codec.clone()),
                }),
            }
        });
        serde_json::Value::Array(items.collect())
    }

    /// A readable name for an encoding ("Japanese — Shift_JIS (CP932)"), or
    /// the encoding itself when there is no translation for it.
    pub fn encoding_label(&self, encoding: &str) -> String {
        let id = format!("encoding-{}", encoding.to_ascii_lowercase());
        self.services
            .translator
            .try_tr(&id, &[])
            .unwrap_or_else(|_| encoding.to_string())
    }

    fn after_delay_change(&mut self, delay_ms: i64) {
        self.subs.update((self.view.time_pos * 1000.0) as i64);
        self.sub_list_version += 1;
        self.toasts
            .push(self.tr("toast-sub-delay", &[("value", format!("{delay_ms:+}"))]));
    }

    /// A click on a grapheme of the overlay or of the list (POP-001): the
    /// text from there (LKP-001…LKP-003) is looked up in a popup next to
    /// `anchor`; playback pauses if it was going.
    pub fn select_grapheme(&mut self, cue: usize, offset: usize, anchor: Rect) {
        // a click: the popup does not close when the pointer leaves
        self.hover.reset();
        self.look_up_grapheme(cue, offset, anchor);
    }

    /// The pointer moved over a grapheme (POP-003): with Shift held, or
    /// always if so configured, a movement of 8 px looks it up.
    #[allow(clippy::too_many_arguments)]
    pub fn grapheme_hovered(
        &mut self,
        cue: usize,
        offset: usize,
        x: f64,
        y: f64,
        shift: bool,
        anchor: Rect,
    ) {
        let armed = shift || self.services.config.lookup.hover_without_modifier;
        if self.hover.moved(x, y, armed, (cue, offset)) {
            self.look_up_grapheme(cue, offset, anchor);
        }
    }

    /// The pointer entered or left the subtitles.
    pub fn subtitle_hover(&mut self, inside: bool) {
        self.subs.hovered = inside;
        let now = self.services.clock.monotonic();
        self.controls.set_over_subtitles(inside, now);
        self.hover.inside(inside || self.popup_hovered, now);
    }

    /// The pointer entered or left a popup.
    pub fn popup_hover(&mut self, inside: bool) {
        self.popup_hovered = inside;
        let now = self.services.clock.monotonic();
        self.hover.inside(inside || self.subs.hovered, now);
    }

    fn look_up_grapheme(&mut self, cue: usize, offset: usize, anchor: Rect) {
        let Some(text) = self.subs.cue(cue).map(|c| c.plain_text.clone()) else {
            return;
        };
        if offset >= text.len() {
            return;
        }
        let range = nikomiru_text::scan::query(&text, offset, nikomiru_text::scan::Context::Cue);
        let query = nikomiru_text::scan::normalize(&text[range.clone()]);
        // a space or a delimiter: nothing to look up, and nothing changes
        if query.is_empty() {
            return;
        }
        self.subs.selection = Some((cue, offset));
        self.subs.highlight = None;
        // where the normalized query starts in the cue
        let start = range.start + text[range].find(query.as_str()).unwrap_or(0);
        let playing = self.view.loaded && !self.view.paused;
        let commands =
            self.popups
                .open_from_subtitle(playing, query, Origin::Subtitle { cue, start }, anchor);
        self.run_popup_commands(commands);
    }

    /// A click in the text of a list row, on the character at index `pos`.
    pub fn select_in_list(&mut self, cue: usize, pos: usize, anchor: Rect) {
        if let Some(c) = self.subs.cue(cue) {
            let offset = crate::subtitles::char_to_byte(&c.plain_text, pos);
            self.select_grapheme(cue, offset, anchor);
        }
    }

    /// Looks `text` up in a root popup, as a click on a subtitle word does but
    /// with no cue behind it: the GUI test harness's `lookup` (TEST §2.1).
    #[cfg(feature = "test-harness")]
    pub fn look_up_text(&mut self, text: &str) {
        let query = nikomiru_text::scan::normalize(text);
        if query.is_empty() {
            return;
        }
        self.hover.reset();
        let playing = self.view.loaded && !self.view.paused;
        let commands =
            self.popups
                .open_from_subtitle(playing, query, Origin::Glossary, Rect::default());
        self.run_popup_commands(commands);
    }

    fn run_popup_commands(&mut self, commands: Vec<Command>) {
        for c in commands {
            match c {
                Command::Pause => {
                    let _ = self.services.player.set_pause(true);
                }
                Command::Resume => {
                    self.end_replay();
                    let _ = self.services.player.set_pause(false);
                }
                Command::Lookup { generation, text } => self.lookups.push((generation, text)),
            }
        }
        if !self.popups.is_open() {
            self.hover.reset();
            self.end_replay();
        }
        // the bar stays while a popup is open (PLY-004)
        self.controls.set_popup_open(self.popups.is_open());
        self.sync_highlight();
    }

    fn sync_highlight(&mut self) {
        self.subs.highlight = self.popups.highlight().and_then(|(cue, start, chars)| {
            let text = &self.subs.cue(cue)?.plain_text;
            let end = text
                .get(start..)?
                .char_indices()
                .nth(chars)
                .map_or(text.len(), |(i, _)| start + i);
            Some((cue, start..end))
        });
        if !self.popups.is_open() {
            self.subs.selection = None;
        }
    }

    /// A link or a selection in popup `level` (POP-011, POP-012).
    pub fn popup_lookup(&mut self, level: usize, text: &str, anchor: Rect) {
        let query = nikomiru_text::scan::normalize(text);
        if query.is_empty() {
            return;
        }
        let commands = self.popups.open_nested(level, query, anchor);
        self.run_popup_commands(commands);
    }

    /// A popup's border was dragged to `width` × `height` (POP-006): the next
    /// popups take that size until the app closes. `config.toml` keeps the
    /// configured one: the app does not write it yet.
    pub fn popup_resized(&mut self, width: f64, height: f64) {
        self.popups.set_size((width, height));
    }

    /// A click inside popup `level`: the popups above it close.
    pub fn popup_focus(&mut self, level: usize) {
        if self.popups.stack().len() > level + 1 {
            self.popups.focus(level);
        }
    }

    /// Esc or Back: the last popup closes (POP-012).
    pub fn close_popup(&mut self) {
        let commands = self.popups.close_last();
        self.run_popup_commands(commands);
    }

    /// A click outside the popups: they all close.
    pub fn close_all_popups(&mut self) {
        if self.popups.is_open() {
            let commands = self.popups.close_all();
            self.run_popup_commands(commands);
        }
    }

    /// Popup `level` is on screen with its content (T-PERF-06).
    pub fn popup_rendered(&mut self, level: usize) {
        let Some(p) = self.popups.stack().get(level) else {
            return;
        };
        let headwords = p
            .reply
            .as_ref()
            .map(|r| {
                r.content
                    .entries
                    .iter()
                    .map(|e| e.expression.clone())
                    .collect()
            })
            .unwrap_or_default();
        self.events.push(UiEvent::PopupRendered {
            level,
            query: p.query.clone(),
            headwords,
        });
    }

    /// The lookups to start, taken by the UI layer.
    pub fn take_lookups(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.lookups)
    }

    /// Starts a lookup on the dictionary service.
    pub fn start_lookup(&self, text: String, done: Box<dyn FnOnce(Result<Reply, String>) + Send>) {
        self.services.dictionaries.lookup(text, done);
    }

    fn dict_effects(&mut self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Message(id, args) => {
                    let message = self.tr(id, &args);
                    self.toasts.push(message);
                }
                Effect::Error(message) => self.toasts.push(message),
                Effect::Start {
                    zip,
                    replace,
                    generation,
                    cancel,
                } => self.dict_imports.push(DictImport {
                    zip,
                    replace,
                    generation,
                    cancel,
                }),
            }
        }
    }

    /// Reads the dictionaries list again (the page opens).
    pub fn dict_refresh(&mut self) {
        let effects = self.dict_page.refresh(self.services.dictionaries.as_ref());
        self.dict_effects(effects);
    }

    /// A zip chosen in the dialog or dropped on the window.
    pub fn dict_choose(&mut self, zip: PathBuf) {
        let effects = self
            .dict_page
            .choose(self.services.dictionaries.as_ref(), zip);
        self.dict_effects(effects);
    }

    /// The answer to the update offer (DIC-016).
    pub fn dict_confirm(&mut self, update: bool) {
        let effects = self.dict_page.confirm(update);
        self.dict_effects(effects);
    }

    /// Stops the running import.
    pub fn dict_cancel(&mut self) {
        self.dict_page.cancel();
    }

    /// The imports to start, taken by the UI layer.
    pub fn take_dict_imports(&mut self) -> Vec<DictImport> {
        std::mem::take(&mut self.dict_imports)
    }

    /// Starts an import on the dictionary service.
    pub fn start_dict_import(
        &self,
        import: DictImport,
        progress: Box<dyn Fn(nikomiru_dict::import::Progress) + Send>,
        done: Box<dyn FnOnce(Result<String, String>) + Send>,
    ) {
        self.services.dictionaries.import(
            import.zip,
            import.replace,
            import.cancel,
            progress,
            done,
        );
    }

    /// Progress of an import.
    pub fn dict_progress(&mut self, generation: u64, progress: nikomiru_dict::import::Progress) {
        self.dict_page.progress(generation, progress);
    }

    /// An import ended.
    pub fn dict_finished(&mut self, generation: u64, result: Result<String, String>) {
        let ok = result.is_ok();
        let effects =
            self.dict_page
                .finished(self.services.dictionaries.as_ref(), generation, result);
        if ok {
            self.events.push(UiEvent::DictionaryImported);
        }
        self.dict_effects(effects);
    }

    /// Moves a dictionary up or down in its group (DIC-031).
    pub fn dict_move(&mut self, id: &str, delta: i32) {
        let effects = self
            .dict_page
            .move_by(self.services.dictionaries.as_ref(), id, delta);
        self.dict_effects(effects);
    }

    /// Moves a dictionary to a gap of its group, where it was dropped (DIC-031).
    pub fn dict_move_to(&mut self, id: &str, to: usize) {
        let effects = self
            .dict_page
            .move_to(self.services.dictionaries.as_ref(), id, to);
        self.dict_effects(effects);
    }

    /// Enables or disables a dictionary (DIC-032).
    pub fn dict_set_enabled(&mut self, id: &str, enabled: bool) {
        let effects = self
            .dict_page
            .set_enabled(self.services.dictionaries.as_ref(), id, enabled);
        self.dict_effects(effects);
    }

    /// Deletes a dictionary (DIC-033).
    pub fn dict_delete(&mut self, id: &str) {
        let effects = self
            .dict_page
            .delete(self.services.dictionaries.as_ref(), id);
        self.dict_effects(effects);
    }

    /// Renames a dictionary (DIC-033).
    pub fn dict_rename(&mut self, id: &str, name: &str) {
        let effects = self
            .dict_page
            .rename(self.services.dictionaries.as_ref(), id, name);
        self.dict_effects(effects);
    }

    /// Sets a dictionary's block state (DIC-034).
    pub fn dict_set_collapse(&mut self, id: &str, state: &str) {
        let effects = self
            .dict_page
            .set_collapse(self.services.dictionaries.as_ref(), id, state);
        self.dict_effects(effects);
    }

    /// Saves the user's CSS of a dictionary (DIC-035).
    pub fn dict_set_user_css(&mut self, id: &str, css: &str) {
        let effects = self
            .dict_page
            .set_user_css(self.services.dictionaries.as_ref(), id, css);
        self.dict_effects(effects);
    }

    /// A lookup answered.
    pub fn lookup_done(&mut self, generation: u64, result: Result<Reply, String>) {
        let reply = match result {
            Ok(reply) => reply,
            Err(message) => {
                self.toasts.push(message);
                Reply {
                    empty: true,
                    ..Reply::default()
                }
            }
        };
        if self.popups.deliver(generation, reply) {
            self.sync_highlight();
        }
    }

    /// A click on the time of a list row: playback goes to the cue.
    pub fn seek_to_cue(&self, cue: usize) {
        if cue < self.subs.timeline().cues().len() {
            let ms = self.subs.timeline().start_pos(cue);
            let _ = self.services.player.seek_absolute(ms as f64 / 1000.0);
        }
    }

    /// The obscuring that applies now (SUB-014).
    pub fn obscure_now(&self) -> Obscure {
        crate::subtitles::obscured(
            self.subs.mode,
            self.view.loaded && !self.view.paused,
            self.popups.is_open(),
            self.subs.hovered,
        )
    }

    /// The configured subtitle font size (1080p reference) and whether ASS
    /// styles are respected.
    pub fn subtitle_style(&self) -> (f64, bool) {
        let c = &self.services.config.subtitles;
        (c.font_size, c.respect_ass_styles)
    }
}

/// A short name for a subtitle source.
fn source_name(source: &Source) -> String {
    match source {
        Source::File(p) => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        Source::Embedded { id, .. } | Source::Bitmap(id) => format!("#{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            url_to_path("file:///v/My%20Show/ep%2001.mkv"),
            PathBuf::from("/v/My Show/ep 01.mkv")
        );
        assert_eq!(
            url_to_path("file:///v/%E6%97%A5%E6%9C%AC.mkv"),
            PathBuf::from("/v/日本.mkv")
        );
        assert_eq!(
            url_to_path("/plain/path.mkv"),
            PathBuf::from("/plain/path.mkv")
        );
        assert_eq!(
            url_to_path("file:///bad%zz.mkv"),
            PathBuf::from("/bad%zz.mkv")
        );
        // a `%` and one hex digit before a multibyte character (ERR-002)
        assert_eq!(
            url_to_path("file:///x%a日.mkv"),
            PathBuf::from("/x%a日.mkv")
        );
    }
}
