// SPDX-License-Identifier: GPL-3.0-only

//! The Nikomiru binary: the event loop and its window (egui, D-13), single
//! instance and the wiring between the crates.

#![deny(unsafe_code)]

use std::process::ExitCode;
use std::sync::Arc;

use nikomiru_app::inhibit::PortalInhibitor;
use nikomiru_app::shell::{ProxyWaker, Shell, UserEvent};
use nikomiru_app::{cli, instance, linux, logging};
use nikomiru_core::{Config, ConfigError, Paths, SystemClock};
use nikomiru_library::{DictionaryManager, Library};
use nikomiru_player::{Player, VideoOutput};
use nikomiru_ui::dictionaries::{DictionaryService, Done};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::popup::Reply;
use nikomiru_ui::{Inbound, Services, Waker};
use winit::event_loop::EventLoop;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut imports = Vec::new();
    let options = match cli::parse(std::env::args_os().skip(1), &cwd) {
        Ok(cli::Command::Run(o)) => o,
        Ok(cli::Command::ImportDictionaries(zips)) => {
            imports = zips;
            cli::Options::default()
        }
        Ok(cli::Command::Help) => {
            print!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(cli::Command::Version) => {
            println!("nikomiru {VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("nikomiru: {e}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    let token = linux::take_activation_token();

    let debug = options.debug || std::env::var("NIKOMIRU_LOG").is_ok_and(|v| v == "debug");
    let paths = Paths::from_env();
    let logs = paths.as_ref().ok().map(Paths::logs_dir);
    let _log = logging::init(logs.as_deref(), debug);
    let paths = match paths {
        Ok(p) => p,
        Err(e) => return fatal(&e),
    };
    tracing::info!("Nikomiru {VERSION}");
    if !imports.is_empty() {
        return import_dictionaries(&paths, &imports);
    }

    // APP-001
    let request = instance::Request {
        open: options.targets.clone(),
    };
    let server = match paths.socket_path() {
        None => {
            tracing::warn!("XDG_RUNTIME_DIR is not set: no single-instance check");
            None
        }
        Some(socket) => match instance::acquire(&socket, &request) {
            Ok(instance::Role::Forwarded) => {
                tracing::info!("targets handed to the running instance");
                return ExitCode::SUCCESS;
            }
            Ok(instance::Role::Primary(server)) => Some(server),
            Err(e @ instance::InstanceError::NoAnswer(_)) => return fatal(&e),
            Err(e) => {
                tracing::warn!("single-instance check unavailable: {e}");
                None
            }
        },
    };
    run(&paths, &options, server, token)
}

/// The first termination signal closes the window like the user would, so the
/// position is saved (LIB-006); a second one exits at once.
fn watch_termination_signals(waker: Arc<dyn Waker>) {
    let mut signals = match linux::catch_termination_signals() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("termination signals: {e}");
            return;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            let mut asked = false;
            while let Some(signal) = signals.wait() {
                if asked {
                    tracing::warn!("signal {signal} again: exiting now");
                    std::process::exit(128 + signal);
                }
                tracing::info!("signal {signal}: quitting");
                waker.wake(Inbound::Quit);
                asked = true;
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("signal thread: {e}");
    }
}

/// EXT-001: finds ffmpeg and ffprobe and logs what they can do, on a thread so
/// that a slow executable does not delay the window. Nothing in M1 uses them
/// yet; from M2 the result goes to the features that need it.
fn check_ffmpeg(setting: &str) {
    let search = nikomiru_media::tools::Search::from_env(setting);
    let spawned = std::thread::Builder::new()
        .name("ffmpeg-probe".into())
        .spawn(move || match nikomiru_media::tools::probe(&search) {
            Ok(tools) => {
                tracing::info!(
                    "ffmpeg {} at {} ({:?})",
                    tools.version,
                    tools.ffmpeg.display(),
                    tools.origin
                );
                for m in tools.missing_encoders() {
                    tracing::warn!(
                        "ffmpeg has no {} encoder: the {} format is disabled",
                        m.encoder,
                        m.format
                    );
                }
            }
            Err(e) => tracing::warn!("{e}: subtitle extraction and card media are disabled"),
        });
    if let Err(e) = spawned {
        tracing::warn!("ffmpeg probe thread: {e}");
    }
}

/// Logs a startup error, prints it and fails.
fn fatal(e: &dyn std::fmt::Display) -> ExitCode {
    tracing::error!("{e}");
    eprintln!("nikomiru: {e}");
    ExitCode::FAILURE
}

/// The settings, or the defaults and the error to show (ERR-001).
fn load_config(paths: &Paths) -> (Config, Option<ConfigError>) {
    match Config::load(&paths.config_file()) {
        Ok(config) => {
            let unknown = config.unknown_keys();
            if !unknown.is_empty() {
                tracing::info!("settings this version does not know: {unknown:?}");
            }
            (config, None)
        }
        Err(e) => {
            tracing::warn!("{e}");
            (Config::default(), Some(e))
        }
    }
}

/// `--import-dictionary`: imports into the registry and the dictionaries
/// folder, then exits (the running app must be closed: the library is locked).
fn import_dictionaries(paths: &Paths, zips: &[std::path::PathBuf]) -> ExitCode {
    let (config, _) = load_config(paths);
    let translator =
        Translator::from_setting(&config.general.ui_language, |k| std::env::var(k).ok());
    let library = match Library::open(&paths.library_db()) {
        Ok(l) => Arc::new(l),
        Err(e) => {
            eprintln!("nikomiru: {}", translator.error(&e));
            eprintln!("nikomiru: close Nikomiru before importing from the command line");
            return ExitCode::FAILURE;
        }
    };
    let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(SystemClock::new());
    let manager = match DictionaryManager::open(
        library,
        paths.dicts_dir(),
        clock,
        &config.general.study_language,
    ) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("nikomiru: {}", translator.error(&e));
            return ExitCode::FAILURE;
        }
    };
    let mut failed = false;
    for zip in zips {
        let name = zip.display();
        let replace = match manager.check(zip) {
            Ok(nikomiru_library::Check::Update { id, installed }) => {
                eprintln!("{name}: updating revision {installed}");
                Some(id)
            }
            Ok(_) => None,
            Err(e) => {
                eprintln!("{name}: {}", translator.error(&e));
                failed = true;
                continue;
            }
        };
        let cancel = nikomiru_core::Cancel::new();
        let result = manager.import(zip, replace.as_deref(), &cancel, |p| {
            eprint!(
                "\r{name}: {}/{} files, {} entries",
                p.files_done, p.files_total, p.entries
            );
        });
        eprintln!();
        match result {
            Ok(id) => {
                let info = manager
                    .list()
                    .ok()
                    .and_then(|l| l.into_iter().find(|(i, _)| *i == id));
                if let Some((_, r)) = info {
                    eprintln!(
                        "{name}: {} (revision {}) imported: {} terms",
                        r.title, r.revision, r.counts.terms
                    );
                }
            }
            Err(e) => {
                eprintln!("{name}: {}", translator.error(&e));
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Without a library there are no dictionaries: popups are empty and the
/// page says why.
struct NoDictionaries(String);

impl DictionaryService for NoDictionaries {
    fn lookup(&self, _text: String, done: Done<Reply>) {
        done(Ok(Reply {
            empty: true,
            ..Reply::default()
        }));
    }
    fn list(&self) -> Result<Vec<(String, nikomiru_library::DictionaryRecord)>, String> {
        Ok(Vec::new())
    }
    fn check(&self, _zip: &std::path::Path) -> Result<nikomiru_library::Check, String> {
        Err(self.0.clone())
    }
    fn import(
        &self,
        _zip: std::path::PathBuf,
        _replace: Option<String>,
        _cancel: nikomiru_core::Cancel,
        _progress: Box<dyn Fn(nikomiru_dict::import::Progress) + Send>,
        done: Done<String>,
    ) {
        done(Err(self.0.clone()));
    }
    fn set_enabled(&self, _id: &str, _enabled: bool) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn reorder(&self, _ids: &[String]) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn delete(&self, _id: &str) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn rename(&self, _id: &str, _name: Option<String>) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn set_collapse(&self, _id: &str, _collapse: nikomiru_library::Collapse) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn set_user_css(&self, _id: &str, _css: &str) -> Result<(), String> {
        Err(self.0.clone())
    }
}

fn run(
    paths: &Paths,
    options: &cli::Options,
    server: Option<instance::Server>,
    token: Option<String>,
) -> ExitCode {
    // The event loop first: the player, the instance server, the signal thread
    // and the harness all send to it; on X11 winit sets LC_CTYPE here.
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(e) => return fatal(&format!("window system: {e}")),
    };
    linux::force_c_numeric_locale();
    let proxy = event_loop.create_proxy();
    let waker: Arc<dyn Waker> = Arc::new(ProxyWaker::new(proxy.clone()));

    let (config, config_notice) = load_config(paths);
    check_ffmpeg(&config.paths.ffmpeg);
    let translator =
        Translator::from_setting(&config.general.ui_language, |k| std::env::var(k).ok());
    let mut notices = Vec::new();
    if let Some(e) = config_notice {
        notices.push(translator.error(&e));
    }
    let library = match Library::open(&paths.library_db()) {
        Ok(library) => {
            if library.read_only() {
                tracing::warn!("library schema {} is newer: read-only", library.schema());
                notices.push(translator.tr("error-library-read-only", &[]));
            }
            Some(Arc::new(library))
        }
        Err(e) => {
            tracing::error!("{e}");
            notices.push(translator.error(&e));
            None
        }
    };
    // the dictionaries need the registry in the library (DIC-021)
    let dictionaries = library.as_ref().and_then(|library| {
        let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(SystemClock::new());
        match DictionaryManager::open(
            Arc::clone(library),
            paths.dicts_dir(),
            clock,
            &config.general.study_language,
        ) {
            Ok(m) => Some(Arc::new(m)),
            Err(e) => {
                tracing::error!("{e}");
                notices.push(translator.error(&e));
                None
            }
        }
    });

    let sink = waker.clone();
    let player = match Player::new(VideoOutput::RenderApi, move |e| {
        sink.wake(Inbound::Player(e))
    }) {
        Ok(player) => player,
        Err(e) => {
            tracing::error!("{e}");
            eprintln!("nikomiru: {}", translator.error(&e));
            return ExitCode::FAILURE;
        }
    };
    let inhibitor = PortalInhibitor::start(translator.tr("inhibit-reason", &[]));
    let subtitle_source = nikomiru_app::subtitles::Loader::start(
        Translator::from_setting(&config.general.ui_language, |k| std::env::var(k).ok()),
        config.paths.ffmpeg.clone(),
        paths.cache_dir.join("subtitles"),
    );
    let config_language = config.general.ui_language.clone();
    let max_results = config.lookup.max_results;
    let no_dictionaries = translator.tr("error-dict-unavailable", &[]);
    let services = Services {
        player: player.handle(),
        config,
        library,
        translator,
        clock: Box::new(SystemClock::new()),
        notices,
        inhibitor: Box::new(inhibitor),
        subtitle_source: Box::new(subtitle_source),
        dictionaries: match dictionaries {
            Some(manager) => Box::new(nikomiru_app::dictionaries::Dictionaries::start(
                manager,
                Translator::from_setting(&config_language, |k| std::env::var(k).ok()),
                max_results,
            )),
            None => Box::new(NoDictionaries(no_dictionaries)),
        },
    };
    let instance = server.map(|server| {
        let waker = waker.clone();
        server.serve(move |request| {
            if let Some(target) = request.open.into_iter().next() {
                waker.wake(Inbound::Open(target));
            }
            waker.wake(Inbound::Activate);
        })
    });
    if let Some(target) = options.targets.first() {
        waker.wake(Inbound::Open(target.clone()));
    }
    watch_termination_signals(waker.clone());
    #[cfg(feature = "test-harness")]
    if let Some(socket) = &options.harness {
        nikomiru_app::harness::start(socket);
    }

    let mut shell = Shell::new(&player, services, proxy, waker, token);
    let code = match event_loop.run_app(&mut shell) {
        Ok(()) => shell.code,
        Err(e) => {
            tracing::error!("event loop: {e}");
            1
        }
    };
    // Shutdown order (SPECIFICATION §5.2.2 #7): mpv's render context, the window
    // and the session went when the loop ended (`Shell::exiting`); the services of
    // a window that never opened go with the shell; then the mpv core.
    drop(shell);
    player.shutdown();
    drop(instance);
    tracing::info!("exit {code}");
    ExitCode::from(code)
}
