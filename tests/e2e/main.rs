// SPDX-License-Identifier: GPL-3.0-only

//! L6: the real `nikomiru` binary, built with the `test-harness` feature and
//! driven through the GUI test harness in an off-screen KWin (TEST §2).
//! `cargo xtask ci --full` runs them; by hand:
//! `cargo test -p nikomiru-app --features test-harness --test e2e -- --include-ignored`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod drag;
mod support;

use std::time::Duration;

use support::{Desktop, exit_status, fixture};

const NEEDS: &str = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`";

fn time_pos(state: &serde_json::Value) -> f64 {
    state["time_pos"].as_f64().unwrap_or(-1.0)
}

// T-E2E-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn open_play_seek_close_and_reopen_resumes() {
    let _ = NEEDS;
    let video = fixture("av_long.mkv");
    let mut desk = Desktop::start();

    let (mut app, mut h) = desk.launch();
    h.open(&video);
    h.wait("file-loaded");
    h.wait("first-frame");
    let playing = h.state_until("playback", |s| s["paused"] == false && time_pos(s) > 0.0);
    assert_eq!(playing["title"], "av_long.mkv");

    // seek forward from the keyboard: ArrowRight = +3 s by default (PLY-001)
    let before = time_pos(&h.state());
    h.key("ArrowRight");
    h.state_until("the seek", |s| time_pos(s) >= before + 2.5);

    // pause from the keyboard, then close the window
    h.key("Space");
    let paused = h.state_until("the pause", |s| s["paused"] == true);
    let saved = time_pos(&paused);
    assert!(saved > 3.0, "position {saved}");
    h.quit();
    let status = exit_status(&mut app, Duration::from_secs(15));
    assert!(status.success(), "{status}\n{}", desk.log());

    // reopen: the file resumes from the saved position (LIB-006, PLY-007)
    let (mut app, mut h) = desk.launch();
    h.open(&video);
    h.wait("file-loaded");
    let resumed = h.state_until("the resume position", |s| time_pos(s) > 0.0);
    let at = time_pos(&resumed);
    assert!(
        (saved - 0.5..saved + 2.0).contains(&at),
        "saved at {saved}, resumed at {at}\n{}",
        desk.log()
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-APP-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_second_launch_opens_its_file_in_the_first() {
    let video = fixture("av_basic.mkv");
    let mut desk = Desktop::start();
    let (mut first, mut h) = desk.launch();
    assert_eq!(h.state()["loaded"], false);

    let mut second = desk.app(&[&video]).spawn().unwrap();
    let status = exit_status(&mut second, Duration::from_secs(15));
    assert!(status.success(), "second launch: {status}\n{}", desk.log());

    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "av_basic.mkv");
    h.quit();
    assert!(exit_status(&mut first, Duration::from_secs(15)).success());
}

// T-LIB-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn files_open_from_the_command_line_and_by_drag_and_drop() {
    let basic = fixture("av_basic.mkv");
    let weird = fixture("-weird: name & spaces ü 日本.mkv");
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch_with(&["--", &basic]);
    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "av_basic.mkv");

    // a drop arrives as percent-encoded file:// URLs
    h.drop_files(&[&weird]);
    h.wait("file-loaded");
    assert_eq!(h.state()["title"], "-weird: name & spaces ü 日本.mkv");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PRV-002
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_logs_never_hold_an_api_key() {
    const SENTINEL: &str = "SENTINEL-4f2a9c";
    let mut desk = Desktop::start();
    // a key in its place and one in the wrong field, which makes the file invalid
    desk.write_config(&format!(
        "[anki]\napi_key = \"{SENTINEL}-key\"\n[player]\nseek_seconds = \"{SENTINEL}-misplaced\"\n"
    ));
    let (mut app, mut h) = desk.launch_with(&["--debug"]);
    let error = h.state()["error"].as_str().unwrap_or_default().to_string();
    assert!(
        error.contains("config.toml"),
        "the invalid settings are reported: {error:?}"
    );
    assert!(!error.contains(SENTINEL), "{error}");
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let logs = desk.log();
    assert!(logs.contains("Nikomiru"), "the logs were written");
    assert!(
        !logs.contains(SENTINEL),
        "a secret reached the logs:\n{logs}"
    );
}

// T-SEC-004
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon, `cargo xtask fixtures` and the release build"]
fn the_release_build_starts_and_quits() {
    // `cargo xtask ci --full` builds it just before; by hand:
    // `cargo build --release -p nikomiru-app`
    let release =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/nikomiru");
    assert!(
        release.exists(),
        "{} is missing: cargo build --release -p nikomiru-app",
        release.display()
    );
    let mut desk = Desktop::start();
    let mut app = desk
        .command(&release, &[&fixture("av_basic.mkv")])
        .spawn()
        .unwrap();
    support::until(
        Duration::from_secs(20),
        "the release build loads a file",
        || {
            if let Some(status) = app.try_wait().unwrap() {
                panic!("the release build exited: {status}\n{}", desk.log());
            }
            desk.log().contains("file loaded").then_some(())
        },
    );
    let kill = std::process::Command::new("kill")
        .args(["-TERM", &app.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    let status = exit_status(&mut app, Duration::from_secs(15));
    assert!(status.success(), "{status}\n{}", desk.log());
}

fn subs(state: &serde_json::Value) -> &serde_json::Value {
    &state["subtitles"]
}

fn visible_text(state: &serde_json::Value) -> Vec<String> {
    subs(state)["visible"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str().map(str::to_string))
        .collect()
}

// T-SUB-009
// T-SUB-012
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn embedded_subtitles_are_drawn_clickable_and_navigable() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    let shown = h.wait("cue-shown");
    assert_eq!(shown["index"], 0);
    let s = h.state();
    assert_eq!(visible_text(&s), ["食べました"]);
    // the Japanese SRT track, extracted and drawn by Nikomiru (SUB-007, SUB-008)
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .starts_with("track:"),
        "{s}"
    );
    assert_eq!(subs(&s)["cues"], 3);

    // a click on the second grapheme selects it and pauses (byte offset 3: べ)
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 1 }));
    let s = h.state_until("the pause after the click", |s| s["paused"] == true);
    assert_eq!(subs(&s)["selected"], serde_json::json!([0, 3]));
    // no dictionary: the popup says so; Esc closes it and playback resumes
    let s = h.state_until("the empty popup", |s| s["popups"][0]["empty"] == true);
    assert_eq!(s["popups"][0]["query"], "べました", "{s}");
    h.key("Escape");
    h.state_until("the popup closed and playback resumed", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty) && s["paused"] == false
    });

    // Ctrl+→ goes 180 ms before the next cue; paused, so that the position
    // stays there long enough to be read
    h.key("Space");
    h.state_until("paused", |s| s["paused"] == true);
    h.cmd(serde_json::json!({ "cmd": "key", "code": "ArrowRight", "mods": ["Ctrl"] }));
    let s = h.state_until("the jump to the next cue", |s| {
        (time_pos(s) - 4.82).abs() < 0.05
    });
    assert!(visible_text(&s).is_empty(), "{s}");
    // F enters fullscreen, Esc leaves it (M1 checklist, item 10)
    h.key("KeyF");
    h.state_until("fullscreen", |s| s["fullscreen"] == true);
    h.key("Escape");
    h.state_until("windowed again", |s| s["fullscreen"] == false);
    // V turns the subtitles off and on again, keeping the track
    h.key("KeyV");
    h.state_until("subtitles off", |s| subs(s)["disabled"] == true);
    h.key("KeyV");
    let s = h.state_until("subtitles on", |s| subs(s)["disabled"] == false);
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .starts_with("track:"),
        "{s}"
    );
    // X moves the subtitles 100 ms later
    h.key("KeyX");
    let s = h.state_until("the offset", |s| subs(s)["delay_ms"] == 100);
    assert!(s["toast"].as_str().unwrap_or("").contains("+100"), "{s}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-006
// T-SUB-008
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_subtitle_file_next_to_the_video_wins_over_embedded_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("Ep01.mkv");
    std::fs::copy(fixture("av_basic.mkv"), &video).unwrap();
    std::fs::write(
        dir.path().join("Ep01.ja.srt"),
        "1\n00:00:01,000 --> 00:00:03,000\n<i>外部</i>の字幕ファイル\n",
    )
    .unwrap();
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&video.to_string_lossy());
    h.wait("file-loaded");
    h.wait("cue-shown");
    let s = h.state();
    assert_eq!(visible_text(&s), ["外部の字幕ファイル"]);
    assert!(
        subs(&s)["source"]
            .as_str()
            .unwrap_or("")
            .ends_with("Ep01.ja.srt"),
        "{s}"
    );
    assert_eq!(subs(&s)["encoding"], "UTF-8");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PERF-07
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn perf_cues_appear_within_50_ms_of_their_start() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    for start in [2.0, 5.0, 8.0] {
        let shown = h.wait("cue-shown");
        let at = shown["time_pos"].as_f64().unwrap();
        assert!(
            (0.0..=0.050).contains(&(at - start)),
            "cue at {start} shown at {at}"
        );
    }
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-POP-001, T-POP-002, T-POP-004 with real dictionaries
// T-PERF-06
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_click_on_a_subtitle_word_opens_the_dictionary_popup() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    // 食べました: the lookup finds 食べる, the popup is drawn, the video pauses
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    let rendered = match h.try_cmd(
        serde_json::json!({ "cmd": "wait", "event": "popup-rendered", "timeout_ms": 20000 }),
    ) {
        Ok(v) => v,
        Err(e) => panic!("{e}\nstate: {}\nlog:\n{}", h.state(), desk.log()),
    };
    assert_eq!(rendered["query"], "食べました");
    assert_eq!(rendered["headwords"][0], "食べる", "{rendered}");
    let s = h.state_until("paused for the lookup", |s| s["paused"] == true);
    assert_eq!(s["paused_for_lookup"], true);
    // the matched text is highlighted: the whole 食べました (15 bytes)
    assert_eq!(subs(&s)["highlight"], serde_json::json!([0, 0, 15]), "{s}");
    // Esc closes it and playback resumes (POP-004)
    h.key("Escape");
    h.state_until("resumed", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty) && s["paused"] == false
    });

    // T-PERF-06: from the click to the drawn popup, p95 ≤ 100 ms
    let mut times = Vec::new();
    for i in 0..30 {
        let started = std::time::Instant::now();
        h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": i % 3 }));
        h.wait("popup-rendered");
        times.push(started.elapsed());
    }
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    eprintln!(
        "T-PERF-06 click → popup: median {:?}, p95 {p95:?}",
        times[times.len() / 2]
    );
    assert!(p95 <= Duration::from_millis(100), "p95 {p95:?}");
    // nothing went wrong in the interface while drawing them
    let log = desk.log();
    let bad: Vec<&str> = log
        .lines()
        .filter(|l| (l.contains("WARN") || l.contains("ERROR")) && l.contains("nikomiru_ui"))
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SEC-001, T-SEC-006
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_hostile_dictionary_entry_shows_only_what_the_allowlist_lets_through() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    // a spy proxy: anything that fetched an address of the entry through the
    // usual proxy variables would connect here
    let spy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    spy.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", spy.local_addr().unwrap());
    for var in [
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
    ] {
        desk.set_env(var, &proxy);
    }
    let (mut app, mut h) = desk.launch();
    // 危険: a <script>, an onclick, a remote image and a javascript: link
    h.cmd(serde_json::json!({ "cmd": "lookup", "text": "危険" }));
    let rendered = h.wait("popup-rendered");
    assert_eq!(
        rendered["headwords"],
        serde_json::json!(["危険"]),
        "{rendered}"
    );
    let content = h.cmd(serde_json::json!({ "cmd": "popup_content", "level": 0 }));
    let section = &content["entries"][0]["sections"][0];
    assert_eq!(section["dictionary"], "Nikomiru mini-ja", "{content}");
    assert_eq!(
        section["glossaries"][0]["blocks"],
        serde_json::json!([{ "k": "flow", "runs": [
            { "k": "text", "t": "danger", "s": { "color": "#ff0000" } },
            { "k": "link", "t": "click", "target": { "kind": "lookup", "to": "click" }, "s": {} }
        ] }]),
        "only the span's text and colour and the link's text survive"
    );
    let text = content.to_string();
    for bad in [
        "alert",
        "steal",
        "evil.example",
        "javascript",
        "script",
        "url(",
    ] {
        assert!(!text.contains(bad), "{bad} in {text}");
    }
    // nothing reached for the network
    let accepted = spy.accept();
    assert!(
        accepted
            .as_ref()
            .is_err_and(|e| e.kind() == std::io::ErrorKind::WouldBlock),
        "the spy proxy was reached: {accepted:?}"
    );
    let sockets = support::internet_sockets(app.id());
    assert!(sockets.is_empty(), "{sockets:#?}");
    assert!(
        !support::internet_sockets(std::process::id()).is_empty(),
        "the check sees the spy's own socket"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-DIC-001, T-DIC-032 from the page
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_dropped_dictionary_is_imported_and_used() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    // a zip dropped on the window goes to the dictionaries page
    h.drop_files(&[&fixture("dicts/mini-ja.zip")]);
    h.wait("dictionary-imported");
    let s = h.state();
    let rows = &s["dictionaries"]["rows"];
    assert_eq!(rows[0]["name"], "Nikomiru mini-ja", "{s}");
    assert_eq!(rows[0]["terms"], 76);
    assert_eq!(s["dictionaries"]["import"]["state"], "idle");
    // the same zip again: refused, nothing changes
    h.drop_files(&[&fixture("dicts/mini-ja.zip")]);
    let s = h.state_until("the refusal", |s| {
        s["toast"]
            .as_str()
            .unwrap_or("")
            .contains("Nikomiru mini-ja")
    });
    assert_eq!(s["dictionaries"]["rows"].as_array().map(Vec::len), Some(1));
    // Esc closes the page (it covers the popups); the popup uses the dictionary
    h.key("Escape");
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    let rendered = h.wait("popup-rendered");
    assert_eq!(rendered["headwords"][0], "食べる", "{rendered}");
    h.key("Escape");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());

    // T-REL-001: the exit closed the library (the lookup worker holds it
    // too): the next start needs no repair and still has the dictionary
    let (mut app, mut h) = desk.launch();
    let s = h.state();
    assert!(
        !desk.data_file("library.redb.bak-repair").exists(),
        "the library was left open at exit"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    assert_eq!(
        s["dictionaries"]["rows"].as_array().map(Vec::len),
        Some(0),
        "the page is read when it opens"
    );
}

/// The style of the first text run of `content` that reads `text`.
fn style_of<'a>(content: &'a serde_json::Value, text: &str) -> Option<&'a serde_json::Value> {
    match content {
        serde_json::Value::Object(o) if o.get("t").and_then(|t| t.as_str()) == Some(text) => {
            o.get("s")
        }
        serde_json::Value::Object(o) => o.values().find_map(|v| style_of(v, text)),
        serde_json::Value::Array(a) => a.iter().find_map(|v| style_of(v, text)),
        _ => None,
    }
}

// T-DIC-035 with a real lookup
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_users_css_saved_on_the_page_styles_the_popup() {
    // 表 in mini-ja has a note, grey in the dictionary's styles.css
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("Ep01.mkv");
    std::fs::copy(fixture("av_basic.mkv"), &video).unwrap();
    std::fs::write(
        dir.path().join("Ep01.ja.srt"),
        "1\n00:00:00,500 --> 00:00:09,500\n表\n",
    )
    .unwrap();
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    // paused on the cue, so that it stays
    let open = |desk: &mut Desktop| {
        let (app, mut h) = desk.launch();
        h.open(&video.to_string_lossy());
        h.wait("file-loaded");
        h.state_until("the cue", |s| visible_text(s) == ["表"]);
        h.key("Space");
        h.state_until("paused", |s| s["paused"] == true);
        (app, h)
    };
    let (mut app, mut h) = open(&mut desk);
    let note = |h: &mut support::Harness| {
        h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
        h.wait("popup-rendered");
        let content = h.cmd(serde_json::json!({ "cmd": "popup_content", "level": 0 }));
        let style = style_of(&content, "a table").cloned();
        h.key("Escape");
        h.state_until("the popup closed", |s| {
            s["popups"].as_array().is_some_and(Vec::is_empty)
        });
        style.unwrap_or_else(|| panic!("no note in {content}"))
    };
    let style = note(&mut h);
    assert_eq!(style["color"], "#808080", "{style}");
    assert_eq!(style["italic"], true, "{style}");

    // the page, opened by a drop of the installed zip (refused as the same
    // revision); then its editor's Save
    h.drop_files(&[&fixture("dicts/mini-ja.zip")]);
    let s = h.state_until("the page's list", |s| {
        s["dictionaries"]["rows"]
            .as_array()
            .is_some_and(|r| !r.is_empty())
    });
    let id = s["dictionaries"]["rows"][0]["id"].clone();
    let css = r#"span[data-sc-class="note"] { color: #ff0000; position: fixed }"#;
    h.cmd(serde_json::json!({ "cmd": "dict_css", "id": id, "css": css }));
    let s = h.state();
    assert_eq!(s["dictionaries"]["rows"][0]["userCss"], css, "{s}");
    assert!(
        s["toast"]
            .as_str()
            .unwrap_or("")
            .contains("Nikomiru mini-ja"),
        "{s}"
    );
    // Esc closes the page, which covers the popups
    h.key("Escape");

    // the user's rule comes after the dictionary's and wins; the rest of the
    // style stays, and what the subset refuses does nothing
    let style = note(&mut h);
    assert_eq!(style["color"], "#ff0000", "{style}");
    assert_eq!(style["italic"], true, "{style}");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());

    // kept for the next start
    let (mut app, mut h) = open(&mut desk);
    assert_eq!(note(&mut h)["color"], "#ff0000");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-POP-014, T-POP-015
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn the_popup_keyboard_replay_and_go_to_sentence() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    h.wait("cue-shown");
    let cue_start = time_pos(&h.state());
    // 食べました: 食べる, 食べ物?, 食… several entries
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 0, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    let s = h.state();
    let entries = s["popups"][0]["entries"].as_u64().unwrap_or(0);
    assert!(entries >= 2, "{s}");
    h.key("Tab");
    h.state_until("the second entry", |s| s["popups"][0]["entry"] == 1);
    h.cmd(serde_json::json!({ "cmd": "key", "code": "Tab", "mods": ["Shift"] }));
    h.state_until("back to the first", |s| s["popups"][0]["entry"] == 0);

    // R with the popup open: the cue again, pausing at its end (SUB-013)
    h.key("KeyR");
    let s = h.state_until("replaying", |s| s["paused"] == false);
    assert_eq!(
        s["popups"].as_array().map(Vec::len),
        Some(1),
        "the popup stays"
    );
    // the cue is 2.0–2.5 s: mpv stops on its last frame
    let s = h.state_until("paused at the end of the cue", |s| {
        s["paused"] == true && time_pos(s) >= cue_start + 0.3
    });
    assert!(time_pos(&s) <= 2.5, "stopped at the end of the cue: {s}");
    // closing resumes and the stop is gone: playback goes past the cue
    h.key("Escape");
    h.state_until("past the end of the cue", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty) && time_pos(s) > 5.2
    });
    // "go to the sentence": the popups close and playback is back at the cue
    h.cmd(serde_json::json!({ "cmd": "click_grapheme", "layer": "main", "cue_index": 1, "grapheme_index": 0 }));
    h.wait("popup-rendered");
    h.cmd(serde_json::json!({ "cmd": "popup_button", "button": "goto" }));
    h.state_until("back at the second cue, playing", |s| {
        s["popups"].as_array().is_some_and(Vec::is_empty)
            && s["paused"] == false
            && (4.9..5.6).contains(&time_pos(s))
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-LIB-001, T-DIC-001 on Wayland, where winit takes no drops: the app's own
// data device, with a drag from another client as a file manager makes it
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn files_dragged_from_another_client_open_and_import() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("file-loaded");
    let mut source = drag::DragSource::connect(&desk.wayland_socket());
    // a video: it opens, and is copied (the source offers a move too)
    let video = fixture("-weird: name & spaces ü 日本.mkv");
    let outcome = source.drag(&[&video], (640.0, 400.0));
    assert_eq!(
        outcome.accepted.as_deref(),
        Some("text/uri-list"),
        "{outcome:?}"
    );
    assert!(
        outcome.sent && outcome.finished && !outcome.cancelled,
        "{outcome:?}"
    );
    h.wait("file-loaded");
    let s = h.state();
    assert!(s["title"].as_str().unwrap_or("").contains("weird"), "{s}");
    assert!(std::path::Path::new(&video).exists(), "copied, not moved");
    // a dictionary: imported from the page
    let outcome = source.drag(&[&fixture("dicts/mini-ja.zip")], (640.0, 400.0));
    assert!(outcome.finished, "{outcome:?}");
    h.wait("dictionary-imported");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-REL-001, T-DIC-010: quitting during an import cancels it; the library is
// closed, nothing is registered and nothing is left to repair or clean up
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn quitting_during_an_import_leaves_nothing_behind() {
    let mut desk = Desktop::start();
    let zip = desk.large_dictionary(300_000);
    let (mut app, mut h) = desk.launch();
    h.drop_files(&[&zip]);
    h.state_until("the import running", |s| {
        s["dictionaries"]["import"]["state"] == "running"
            && s["dictionaries"]["import"]["done"].as_u64().unwrap_or(0) > 0
    });
    let quitting = std::time::Instant::now();
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let took = quitting.elapsed();
    eprintln!("quit during the import: {took:?}");
    assert!(took < Duration::from_secs(5), "the exit waited {took:?}");
    assert!(
        !desk.data_file("library.redb.bak-repair").exists(),
        "the library was left open"
    );
    // the next start: still nothing to repair, nothing registered, no
    // staging folder left
    let (mut app, mut h) = desk.launch();
    h.state();
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    assert!(!desk.data_file("library.redb.bak-repair").exists());
    assert!(
        desk.registered_dictionaries().is_empty(),
        "{:?}",
        desk.registered_dictionaries()
    );
    let left: Vec<_> = std::fs::read_dir(desk.data_file("dicts"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
}
