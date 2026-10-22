// SPDX-License-Identifier: GPL-3.0-only

//! The L6 environment: an off-screen KWin on a private D-Bus bus, isolated XDG
//! folders, and the harness client (TEST §2.1). Nothing touches the user's
//! session: the bus has no service activation, so no real portal answers, and
//! the runtime folder is private, so no audio server is found either.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// A generated fixture (`cargo xtask fixtures`).
pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name);
    assert!(
        path.exists(),
        "{} is missing: run `cargo xtask fixtures`",
        path.display()
    );
    path.canonicalize().unwrap().to_string_lossy().into_owned()
}

/// Polls `f` until it returns `Some`, or panics after `timeout`.
pub fn until<T>(timeout: Duration, what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Waits for a child to exit.
pub fn exit_status(child: &mut Child, timeout: Duration) -> ExitStatus {
    until(timeout, "process exit", || child.try_wait().unwrap())
}

/// The internet sockets (TCP, UDP or raw, IPv4 or IPv6) that process `pid`
/// holds, as the kernel's tables list them.
pub fn internet_sockets(pid: u32) -> Vec<String> {
    let inodes: Vec<String> = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .map(|dir| {
            dir.flatten()
                .filter_map(|fd| {
                    let link = std::fs::read_link(fd.path()).ok()?;
                    let link = link.to_string_lossy();
                    Some(
                        link.strip_prefix("socket:[")?
                            .strip_suffix(']')?
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let mut found = Vec::new();
    for table in ["tcp", "tcp6", "udp", "udp6", "raw", "raw6"] {
        let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/net/{table}")) else {
            continue;
        };
        for line in text.lines().skip(1) {
            // the tenth column is the socket's inode
            if line
                .split_whitespace()
                .nth(9)
                .is_some_and(|inode| inodes.iter().any(|i| i == inode))
            {
                found.push(format!("{table}: {line}"));
            }
        }
    }
    found
}

/// An off-screen desktop for one test.
pub struct Desktop {
    dir: tempfile::TempDir,
    bus: Child,
    kwin: Child,
    bus_address: String,
    launches: usize,
    /// More variables for the apps launched from now on.
    env: Vec<(String, String)>,
}

const WAYLAND: &str = "wayland-e2e";

impl Desktop {
    pub fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        for sub in ["config", "state", "data", "cache", "run"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        let run = dir.path().join("run");
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o700)).unwrap();
        // No audio server is reachable here: mpv's audio output is null. Probing
        // the others also races in mpv's PipeWire output, which can hang the core
        // when a second file is loaded while it is still failing to connect.
        std::fs::create_dir_all(dir.path().join("config/nikomiru")).unwrap();
        std::fs::write(
            dir.path().join("config/nikomiru/config.toml"),
            "[player]\nmpv_options = { ao = \"null\" }\n",
        )
        .unwrap();

        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                run.join("bus").display()
            ),
        )
        .unwrap();
        let mut bus = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon");
        let mut bus_address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut bus_address)
            .unwrap();
        let bus_address = bus_address.trim().to_string();

        let log = std::fs::File::create(dir.path().join("kwin.log")).unwrap();
        let kwin = Command::new("kwin_wayland")
            .args([
                "--virtual",
                "--width",
                "1280",
                "--height",
                "720",
                "--scale",
                "1",
            ])
            .args(["--socket", WAYLAND])
            .args([
                "--no-lockscreen",
                "--no-global-shortcuts",
                "--no-kactivities",
            ])
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("XDG_RUNTIME_DIR", &run)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus_address)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("kwin_wayland");
        let socket = run.join(WAYLAND);
        until(Duration::from_secs(20), "KWin's Wayland socket", || {
            socket.exists().then_some(())
        });
        Self {
            dir,
            bus,
            kwin,
            bus_address,
            launches: 0,
            env: Vec::new(),
        }
    }

    /// Gives the apps launched from now on the variable `key`.
    pub fn set_env(&mut self, key: &str, value: &str) {
        self.env.push((key.to_string(), value.to_string()));
    }

    /// The compositor's socket, for the test's own Wayland clients.
    pub fn wayland_socket(&self) -> PathBuf {
        self.path("run").join(WAYLAND)
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    /// A file of the app's data folder (`$XDG_DATA_HOME/nikomiru`).
    pub fn data_file(&self, name: &str) -> PathBuf {
        self.path("data").join("nikomiru").join(name)
    }

    /// The registered dictionaries of this desktop's profile, read while the
    /// app is not running.
    pub fn registered_dictionaries(&self) -> Vec<String> {
        use std::sync::Arc;
        let data = self.path("data").join("nikomiru");
        let library =
            Arc::new(nikomiru_library::Library::open(&data.join("library.redb")).unwrap());
        let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(nikomiru_core::SystemClock::new());
        let manager =
            nikomiru_library::DictionaryManager::open(library, data.join("dicts"), clock, "ja")
                .unwrap();
        manager
            .list()
            .unwrap()
            .into_iter()
            .map(|(_, r)| r.title)
            .collect()
    }

    /// A Yomitan dictionary of `entries` made-up terms, in banks of 10 000,
    /// written in this desktop's folder: long enough to import that a test
    /// can act while it runs.
    pub fn large_dictionary(&self, entries: usize) -> String {
        use std::io::Write as _;
        let path = self.path("large.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("index.json", options).unwrap();
        zip.write_all(br#"{"title":"Nikomiru large","format":3,"revision":"1"}"#)
            .unwrap();
        for bank in 0..entries.div_ceil(10_000) {
            let rows: Vec<String> = (bank * 10_000..((bank + 1) * 10_000).min(entries))
                .map(|i| {
                    format!(
                        r#"["語{i}","ご{i}","n","",0,["word {i}", "a longer gloss so that the bank weighs what a real one does"],{i},""]"#
                    )
                })
                .collect();
            zip.start_file(format!("term_bank_{}.json", bank + 1), options)
                .unwrap();
            zip.write_all(format!("[{}]", rows.join(",")).as_bytes())
                .unwrap();
        }
        zip.finish().unwrap();
        path.to_string_lossy().into_owned()
    }

    /// Imports dictionaries (zips of `target/fixtures/dicts`) into this
    /// desktop's profile, before the app starts.
    pub fn import_dictionaries(&self, names: &[&str]) {
        use std::sync::Arc;
        let data = self.path("data").join("nikomiru");
        let library =
            Arc::new(nikomiru_library::Library::open(&data.join("library.redb")).unwrap());
        let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(nikomiru_core::SystemClock::new());
        let manager =
            nikomiru_library::DictionaryManager::open(library, data.join("dicts"), clock, "ja")
                .unwrap();
        for name in names {
            let zip = fixture(&format!("dicts/{name}.zip"));
            manager
                .import(Path::new(&zip), None, &nikomiru_core::Cancel::new(), |_| {})
                .unwrap();
        }
    }

    /// `nikomiru` (the harness build) with `args`, in this desktop and with its
    /// folders.
    pub fn app(&mut self, args: &[&str]) -> Command {
        self.command(Path::new(env!("CARGO_BIN_EXE_nikomiru")), args)
    }

    /// Another build of `nikomiru` (the release one) in this desktop.
    pub fn command(&mut self, binary: &Path, args: &[&str]) -> Command {
        self.launches += 1;
        let log = std::fs::File::create(self.path(&format!("app-{}.log", self.launches))).unwrap();
        let mut cmd = Command::new(binary);
        cmd.args(args)
            .env_clear()
            .env("HOME", self.dir.path())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("LANG", "C.UTF-8")
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_CACHE_HOME", self.path("cache"))
            .env("XDG_RUNTIME_DIR", self.path("run"))
            .env("WAYLAND_DISPLAY", WAYLAND)
            .env("QT_QPA_PLATFORM", "wayland")
            .env("QT_FORCE_STDERR_LOGGING", "1")
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus_address)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        cmd
    }

    /// Starts `nikomiru --test-harness` and connects to it.
    pub fn launch(&mut self) -> (Child, Harness) {
        self.launch_with(&[])
    }

    /// Writes `config.toml` in this desktop's settings folder.
    pub fn write_config(&self, text: &str) {
        let dir = self.path("config/nikomiru");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), text).unwrap();
    }

    /// Like [`Desktop::launch`], with more arguments.
    pub fn launch_with(&mut self, args: &[&str]) -> (Child, Harness) {
        let socket = self.path(&format!("harness-{}.sock", self.launches + 1));
        let listener = UnixListener::bind(&socket).unwrap();
        let socket_arg = socket.to_string_lossy().into_owned();
        let mut all = vec!["--test-harness", socket_arg.as_str()];
        all.extend_from_slice(args);
        let mut child = self.app(&all).spawn().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stream = until(Duration::from_secs(30), "the harness connection", || {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("nikomiru exited at startup: {status}\n{}", self.log());
            }
            listener.accept().ok().map(|(s, _)| s)
        });
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        let logs = vec![
            self.path(&format!("app-{}.log", self.launches)),
            self.path("state/nikomiru/logs"),
        ];
        (child, Harness::new(stream, logs))
    }

    /// The application logs of this desktop, for failure messages.
    pub fn log(&self) -> String {
        let mut out = String::new();
        for i in 1..=self.launches {
            if let Ok(text) = std::fs::read_to_string(self.path(&format!("app-{i}.log"))) {
                out.push_str(&format!("--- launch {i}\n{text}"));
            }
        }
        if let Ok(entries) = std::fs::read_dir(self.path("state/nikomiru/logs")) {
            for e in entries.flatten() {
                if let Ok(text) = std::fs::read_to_string(e.path()) {
                    out.push_str(&format!("--- {}\n{text}", e.path().display()));
                }
            }
        }
        out
    }
}

impl Desktop {
    /// The apps a failed test left running on this desktop: the `nikomiru`
    /// processes whose environment holds this desktop's runtime folder.
    fn leftover_apps(&self) -> Vec<u32> {
        let needle = format!("XDG_RUNTIME_DIR={}", self.path("run").display());
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
            .filter(|pid| {
                let proc = PathBuf::from(format!("/proc/{pid}"));
                let is_app = std::fs::read_link(proc.join("exe"))
                    .is_ok_and(|exe| exe.file_name().is_some_and(|n| n == "nikomiru"));
                is_app
                    && std::fs::read(proc.join("environ"))
                        .is_ok_and(|env| env.split(|b| *b == 0).any(|var| var == needle.as_bytes()))
            })
            .collect()
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // a failed test may leave its app running: it must not outlive the desktop
        for pid in self.leftover_apps() {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }
        for child in [&mut self.kwin, &mut self.bus] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The client side of the harness protocol.
pub struct Harness {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    /// The app's output and its log folder, shown when a command fails.
    logs: Vec<PathBuf>,
}

impl Harness {
    fn new(stream: UnixStream, logs: Vec<PathBuf>) -> Self {
        Self {
            writer: stream.try_clone().unwrap(),
            reader: BufReader::new(stream),
            logs,
        }
    }

    /// The end of the app's logs, for failure messages.
    fn log_tail(&self) -> String {
        let mut text = String::new();
        for path in &self.logs {
            let files: Vec<PathBuf> = if path.is_dir() {
                std::fs::read_dir(path)
                    .map(|r| r.flatten().map(|e| e.path()).collect())
                    .unwrap_or_default()
            } else {
                vec![path.clone()]
            };
            for f in files {
                if let Ok(t) = std::fs::read_to_string(&f) {
                    let lines: Vec<&str> = t.lines().collect();
                    let tail = &lines[lines.len().saturating_sub(60)..];
                    text.push_str(&format!("--- {}\n{}\n", f.display(), tail.join("\n")));
                }
            }
        }
        text
    }

    /// Sends a command and returns its result, or the harness's error.
    pub fn try_cmd(&mut self, command: Value) -> Result<Value, String> {
        writeln!(self.writer, "{command}").unwrap();
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        let answer: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("bad answer {line:?}: {e}\n{}", self.log_tail()));
        if answer["ok"] == true {
            Ok(answer["result"].clone())
        } else {
            Err(answer["error"].to_string())
        }
    }

    pub fn cmd(&mut self, command: Value) -> Value {
        let shown = command.to_string();
        self.try_cmd(command)
            .unwrap_or_else(|e| panic!("{shown}: {e}\n{}", self.log_tail()))
    }

    pub fn open(&mut self, path: &str) {
        self.cmd(json!({ "cmd": "open", "path": path }));
    }

    pub fn wait(&mut self, event: &str) -> Value {
        self.cmd(json!({ "cmd": "wait", "event": event, "timeout_ms": 20_000 }))
    }

    pub fn drop_files(&mut self, paths: &[&str]) {
        self.cmd(json!({ "cmd": "drop", "paths": paths }));
    }

    pub fn key(&mut self, code: &str) {
        self.cmd(json!({ "cmd": "key", "code": code, "mods": [] }));
    }

    pub fn state(&mut self) -> Value {
        self.cmd(json!({ "cmd": "state" }))
    }

    /// Polls the state until `test` accepts it.
    pub fn state_until(&mut self, what: &str, test: impl Fn(&Value) -> bool) -> Value {
        until(Duration::from_secs(10), what, || {
            let s = self.state();
            test(&s).then_some(s)
        })
    }

    pub fn quit(&mut self) {
        self.cmd(json!({ "cmd": "quit" }));
    }
}
