# Nikomiru — Specification

| Field | Value |
|---|---|
| Status | v0.7 — decisions D-1…D-12 approved on 2026-10-19 and D-13 on 2026-10-20 (§10.1); D-13 replaces Qt 6 Quick with egui; D-11 renames the project to Nikomiru |
| Date | 2026-10-20 |
| Reference | Fushi, commit `ac8034e8bbc25e877e6d419334a38951bc477105` (2026-10-18), <https://github.com/hajisensai/Fushi> |
| Related documents | [ROADMAP.md](ROADMAP.md) · [TEST.md](TEST.md) |

## 0. Conventions

- **MUST / MUST NOT**: mandatory requirement. **SHOULD**: recommended; an exception needs a written justification in the PR. **MAY**: optional.
- Every requirement has a stable identifier (`PLY-012`). IDs are never reused: a removed requirement stays in the table, struck through, with the reason.
- Tests that verify a requirement use the same prefix (`T-PLY-012`, see [TEST.md](TEST.md)).
- "Fushi:" followed by a path names the Fushi file to consult during porting. Paths start from the root of the Fushi repository.
- The **milestone** of each requirement (M0…M8, or "post-1.0") is defined in [ROADMAP.md](ROADMAP.md).
- **Language (D-9)**: documents, code comments, commit messages and developer-tool messages are written in **English**. The user interface is localized through Fluent (SET-004), with Italian and English at 1.0.

---

## 1. Purpose

### 1.1 What Nikomiru is

Nikomiru is a desktop video player for **learning languages by watching anime and series** (*immersion learning*). While you watch, you can click a word in the subtitles: Nikomiru looks it up in the dictionaries you imported (Yomitan format) and, with one click, creates an Anki card with the word, the sentence, a screenshot and the audio of the line.

Nikomiru is a **Rust rewrite of a subset of Fushi**. Fushi is a GPL-3.0 Flutter/Dart app that covers EPUB reading, manga, audiobooks, video, visual novels and much more. Nikomiru starts from its "video + dictionary + Anki" core and reimplements it with:

- **UI**: [egui](https://github.com/emilk/egui) (immediate mode, all Rust) on winit and glutin (OpenGL through EGL), with accessibility through AccessKit;
- **playback**: libmpv through [libmpv2](https://crates.io/crates/libmpv2) 6.0 (core, commands, events) and its OpenGL render API, which draws the video into the window under the interface;
- **licenses**: only GNU (GPL/LGPL), Apache-2.0 and MIT, with the unavoidable exceptions documented in §3.

**Direction (D-12).** Until 1.0 the focus is video with clickable subtitles: a click on a word, in the overlay or in the subtitle list, shows its dictionary explanation, and a second click makes the Anki card. After 1.0 Nikomiru will grow to books (EPUB), manga and online resources. That is why lookup, dictionaries and mining live in crates that know nothing about video (`nikomiru-text`, `nikomiru-dict`, `nikomiru-anki`, §5.1): a book or a manga page must be able to reuse them as they are.

### 1.2 Goals

1. Play local video files with mpv quality: hardware decoding, all codecs and containers supported by mpv.
2. Show text subtitles in an overlay above the video that is **clickable character by character**.
3. Look up words in Yomitan dictionaries with deinflection, frequencies and pitch accent, with quality at least equal to Fushi.
4. Create Anki cards via AnkiConnect (ready-to-use Lapis note) with sentence, screenshot, sentence audio and a link back to the scene.
5. Manage a library of series and episodes, with progress and watch statistics.
6. Work offline. The network is used only for features the user asks for (metadata, online subtitles, word audio). No telemetry.

### 1.3 Non-goals (out of scope until a new decision)

| Fushi feature | Decision | Reason |
|---|---|---|
| EPUB reader, manga + OCR, PDF, audiobooks (Sasayaki), online resources | **planned after 1.0** (D-12) | Each one is as large as the whole MVP; until 1.0 the focus is video. The text, dictionary and Anki crates stay independent of video so they can be reused |
| Text and voice hooks for galgames (Windows) | excluded | Windows only; depends on LunaHook and MinHook |
| Torrent, Mihon/Aniyomi extensions, LNReader, download center | excluded | Out of scope; Kotlin/APK runtime |
| AI/LLM, ASR (subtitle generation), TTS, ONNX models | excluded | Some models have CC BY-NC licenses; huge size |
| Google Lens OCR, AACS Blu-ray decryption | excluded | Legal and terms-of-service risks |
| Cloud sync, Interconnect, iroh P2P, headless server, CLI | post-1.0 (WebDAV backup/sync only) | Complexity; not needed for the MVP |
| Built-in SRS (FSRS) | post-1.0 | `fsrs` is BSD-3-Clause: it would need an exception (§3) |
| Leaderboard, in-app feedback, usage telemetry | excluded | Privacy; no service to run |
| Android, iOS | excluded | Mobile builds with libmpv are not a goal |
| Windows, macOS | post-1.0 | Linux first; the architecture must not rule them out (§5.2.4) |

### 1.4 Platforms

- **Primary**: Linux x86_64, both **Wayland** and **X11**, with OpenGL drivers with EGL installed (Mesa or the proprietary NVIDIA driver), the Wayland and X11 client libraries and libxkbcommon, which winit loads at run time, and fontconfig. Fractional scaling on Wayland comes through winit. On X11 the OpenGL context is EGL too (§5.2.2).
- **libmpv**: API ≥ 2.2 (mpv ≥ 0.37). Development reference: mpv 0.41 (API 2.5).
- **FFmpeg**: ≥ 6.0 as a system executable (`ffmpeg`, `ffprobe`). Development reference: 9.0.
- Secondary (post-1.0): Windows 10+, macOS 13+ on Apple Silicon.

### 1.5 Languages

- Primary study language: **Japanese**. All algorithms MUST still be data-driven (Yomitan transform tables and per-language preprocessors), so that other languages can be added without rewriting code.
- UI languages: **Italian** and **English** at 1.0. Japanese SHOULD follow.

---

## 2. Relationship with Fushi

### 2.1 Fushi in numbers (at the reference commit)

- App: about 1,860 Dart files, about 900k lines in `fushi/lib`; separate packages (`fushi_engine` alone: about 182k lines).
- Tests: about 4,270 `*_test.dart` files, about 1M lines. About 930 are "guard tests" that check architectural rules by reading the source.
- Drift/SQLite database: 98 tables, schema v118.
- Native dictionary engine in C++23 (`native/fushidicts`, a fork of hoshidicts).
- 17 UI languages, about 8,400 keys.

**Consequence**: Nikomiru is not a 1:1 port. It is a **selective, requirement-driven** port based on this document. Fushi is the reference implementation for behaviors, algorithms, edge cases and known bugs.

### 2.2 Porting rules

1. Before implementing a requirement that cites a Fushi file, read that file and its tests. Every cited Fushi bug (`BUG-xxxx`) becomes a regression test in Nikomiru.
2. Code translated from Fushi, Yomitan, hoshidicts, Lapis or Tsubasa is a **GPL-3.0 derivative work**. The Rust module that contains it MUST have a header comment that names the project and the source files, with the commit.
3. Port **behaviors, not structures**. Do not replicate workarounds tied to Flutter, Dart or Windows (isolates, FFI, Windows IME).
4. Where Fushi has known gaps (see §6.6.8), Nikomiru MAY improve, but MUST document the difference here and cover it with a test.

### 2.3 Module map

| Fushi area | Reference | Nikomiru crate | Milestone |
|---|---|---|---|
| Video player, mpv config | `fushi/lib/src/media/video/video_player_controller.dart`, `video_mpv_config.dart` | `nikomiru-player` | M1 |
| Subtitles (parser, markup, overlay) | `packages/fushi_audio/lib/src/parsers/`, `fushi/lib/src/media/video/video_subtitle_overlay.dart` | `nikomiru-subs`, `nikomiru-ui` (egui) | M2 |
| Dictionary engine | `native/fushidicts/fushidicts_src/` | `nikomiru-dict` | M3 |
| Preprocessors and kana | `native/fushidicts/fushidicts_src/text_processor/` | `nikomiru-text` | M3 |
| Lookup popup | `fushi/assets/popup/popup.js`, `selection.js` | `nikomiru-ui` (egui) | M3 |
| Anki / Lapis / composer | `packages/fushi_anki/lib/src/` | `nikomiru-anki` | M4 |
| Video mining (ffmpeg) | `fushi/lib/src/mining/immersion_mining_engine.dart`, `packages/fushi_engine/lib/utils/misc/desktop_audio_clipper.dart` | `nikomiru-media` | M4 |
| Library, file name parser | `packages/fushi_engine/lib/media/video/video_library_import.dart`, `video_filename_parser.dart` | `nikomiru-library` | M5 |
| Word audio | `fushi/lib/src/utils/misc/word_audio_resolver.dart`, `local_audio_db.dart` | `nikomiru-dict` (`audio` module) | M5 |
| Statistics | `fushi/lib/src/media/video/video_watch_tracker.dart`, `fushi/lib/src/stats/` | `nikomiru-library` (`stats` module) | M6 |
| Shortcuts | `fushi/lib/src/shortcuts/shortcut_defaults.dart` | `nikomiru-ui` | M1–M6 |
| Online subtitles, metadata | `packages/fushi_engine/lib/media/video/jimaku_client.dart`, `metadata/` | `nikomiru-online` | M7 |

---

## 3. Licenses

### 3.1 Nikomiru license

Nikomiru is distributed under **GPL-3.0-only** (`LICENSE` file).

- This is mandatory: Nikomiru derives from Fushi (GPL-3.0), the Yomitan tables (GPL-3.0), hoshidicts (GPL-3.0) and the Lapis templates (GPL-3.0).
- "only" is used instead of "or-later" because Fushi does not explicitly grant later versions. A move to "or-later" is possible only if all derived sources allow it.
- Every source file MUST start with the SPDX header `// SPDX-License-Identifier: GPL-3.0-only`.

### 3.2 Rust dependency policy

**Class A — always allowed** (direct and transitive dependencies):

| SPDX license | Notes |
|---|---|
| `MIT` | |
| `Apache-2.0`, `Apache-2.0 WITH LLVM-exception` | Compatible with GPL-3.0 |
| `GPL-2.0-or-later`, `GPL-3.0-only`, `GPL-3.0-or-later` | `GPL-2.0-only` is **not** allowed: it is incompatible with GPL-3.0 |
| `LGPL-2.1-only`, `LGPL-2.1-or-later`, `LGPL-3.0-only`, `LGPL-3.0-or-later` | E.g. `libmpv2` (LGPL-2.1) |

Expressions with `OR` (e.g. `MIT OR Apache-2.0`, `Unlicense OR MIT`, `MIT OR Zlib OR Apache-2.0`) are allowed if at least one branch is class A: Nikomiru picks that branch.

**Class B — unavoidable exceptions, allowed only for the listed crates.**

A real check with `cargo deny` on the whole §4 stack (D-13: egui 0.37 on winit and glutin, `libmpv2` 6.0.0, `reqwest` with native-tls) shows a few unavoidable crates outside class A: the Unicode-3.0 ICU family (`url` → `idna`, needed by reqwest, and the direct `icu_normalizer`), `unicode-ident` (every procedural macro), `foldhash` (Zlib, the default hasher of `hashbrown` in egui's text stack and in AccessKit) and `libloading` (ISC: glutin and winit open the system's graphics and windowing libraries at run time, [ADR 0006](adr/0006-toolkit-comparison.md)). All of them are permissive licenses compatible with GPL-3.0. The longer list forced by Dioxus desktop (D-1, D-7, D-8) went with D-10; cxx and cxx-qt went with D-13.

| License | Crate (verified version) | Required by |
|---|---|---|
| `Unicode-3.0` | `icu_*` 2.3, `litemap`, `tinystr`, `writeable`, `yoke`, `yoke-derive`, `zerofrom`, `zerofrom-derive`, `zerotrie`, `zerovec`, `zerovec-derive`, `potential_utf`; `unicode-ident` (`(MIT OR Apache-2.0) AND Unicode-3.0`) | `url` → `idna` (reqwest); `icu_normalizer` (direct, §4); `proc-macro2` (every procedural macro: serde_derive, thiserror, zbus through AccessKit) |
| `Zlib` | `foldhash` | `hashbrown`'s default hasher (fontique, AccessKit, epaint's `vello_cpu`) and `glifo` (epaint) |
| `ISC` | `libloading` | `glutin` and the libraries winit loads at run time (libEGL, libwayland, Xlib): egui's window and OpenGL context (D-13) |

Rules for class B:

1. The exceptions apply **only** to the listed crates, through `[[licenses.exceptions]]` in `deny.toml`. A new crate with one of these licenses makes CI fail.
2. Every new exception requires: a justification ("what requires it and why it cannot be avoided"), a row in this table and explicit approval from the maintainer.
3. No **direct** dependency of Nikomiru may be class B, except `icu_normalizer` (see §4): it is already in the graph through `idna`, so it adds no new license.
4. On every update of egui, winit, glutin or reqwest we MUST try to shrink class B.

**Forbidden** (CI fails):

- All licenses not listed above.
- In particular: `AGPL-*` (Anki rslib), `CC-BY-NC-*`, `CC0-1.0` (e.g. `notify`; forbidden with no exception, D-8 is obsolete), `BSL-1.0` (e.g. `xxhash-rust`), `GPL-2.0-only`, `OpenSSL`, `CECILL-*`, and any crate without a declared license.

### 3.3 System libraries and external programs

They are loaded at runtime or invoked as processes. They are not distributed with Nikomiru, except in the post-1.0 packages.

| Component | License | Use |
|---|---|---|
| libmpv (mpv) | GPL-2.0-or-later (or LGPL-2.1-or-later, depending on the build) | Playback |
| FFmpeg / FFprobe (executables) | LGPL-2.1-or-later or GPL-2.0-or-later | Subtitle extraction, audio clips, images |
| libwayland-client, libxkbcommon, libX11 and its extensions (Xcursor, Xi, Xrandr) | MIT / X11 | Window and input (winit, glutin), loaded at run time |
| fontconfig | MIT-style | Finding the system fonts (fontique) |
| glibc (`iconv`) | LGPL-2.1-or-later | Subtitle encoding conversion (§6.4) |
| OpenSSL 3 | Apache-2.0 | TLS through `native-tls` |
| Mesa / libEGL / libGL | MIT | OpenGL context for egui and for mpv's rendering |

### 3.4 Data and resources

| Resource | License | Rule |
|---|---|---|
| Yomitan transform tables (JSON generated from Fushi `fushi/assets/transforms/`) | GPL-3.0 | Copied into `assets/transforms/` with a `PROVENANCE.md` file (Yomitan version and Fushi commit) |
| Lapis note templates (donkuri/lapis) | GPL-3.0 | Copied into `assets/anki/lapis/` with a pinned version |
| Dictionaries (JMdict, JMnedict, KANJIDIC, frequency dictionaries, etc.) | Various (e.g. CC BY-SA 4.0) | **NOT** distributed. The user imports them. Tests use hand-written dictionaries (TEST §4.3) |
| Fonts | — | **NO** fonts are distributed: system fonts are used (e.g. Noto CJK). An OFL font is not in the allowed list |
| Icons and logo | GPL-3.0-only | Original works of the project; no Creative Commons license, for consistency with the policy |

### 3.5 How the policy is enforced

- `deny.toml` ([cargo-deny](https://github.com/EmbarkStudios/cargo-deny), MIT/Apache) encodes §3.2: `[licenses] allow = [class A]` and `[[licenses.exceptions]]` for class B. The same checks also cover `bans`, `advisories` and `sources`.
- `cargo deny check` is a **mandatory CI gate** (T-LIC-001).
- `cargo about` (MIT/Apache) generates `THIRD_PARTY_LICENSES.html`. The file is included in every package and shown in the "About" screen (attribution requirement of MIT/BSD/ISC/Zlib).
- Development tools (cargo-deny, cargo-about, cargo-nextest) are not distributed, but they MUST still comply with class A. **No `just`** (CC0): tasks are written as `cargo xtask` in Rust.

### 3.6 Alternatives chosen to stay within the policy

| Need | Rejected choice (license) | Adopted choice |
|---|---|---|
| Local database | SQLite (public domain: not in the list) | `redb` (MIT OR Apache-2.0) |
| Glossary compression | zstd C (BSD-3 / GPL-2.0-only) | `lz4_flex` (MIT) |
| Fast hash | `xxhash-rust` (BSL-1.0) | `twox-hash` (MIT) if needed; otherwise the `redb` hasher |
| Subtitle encoding detection | `encoding_rs` (`… AND BSD-3-Clause`) | glibc `iconv` via FFI (LGPL) + own heuristics |
| TLS | `rustls` + `ring` (ISC) | `native-tls` → OpenSSL (Apache-2.0) |
| Subtitle parsers | `subparse` (MPL-2.0) | Own parsers (ported from Fushi) |
| Anime file name parser | `anitomy` (MPL-2.0) | Own parser (port of `video_filename_parser.dart`) |
| File system watching | `notify` (CC0-1.0) | Manual or periodic rescan; inotify via `libc` if ever needed |
| Task runner | `just` (CC0-1.0) | `cargo xtask` |
| Unicode normalization | `unicode-normalization` (adds an AND Unicode-3.0) | `icu_normalizer` (already in the graph) |
| XDG paths | `directories` (pulls `option-ext`, MPL-2.0) | Own implementation in `nikomiru-core` (XDG Base Directory variables and `user-dirs.dirs`) |

---

## 4. Technology stack

Versions verified on crates.io on 2026-10-19. The lockfile (`Cargo.lock`) is committed.

| Area | Crate | Version | License |
|---|---|---|---|
| Toolchain | Rust stable, edition 2024, pinned in `rust-toolchain.toml` | ≥ 1.95: egui 0.37 (also let chains, `File::try_lock`) (development: 1.99) | — |
| UI | `egui` (no default fonts), `egui-winit` (AccessKit, clipboard; no `links`), `egui_glow`, `winit` (no Adwaita decorations), `glutin` and `glutin-winit` (EGL first), `glow`, `raw-window-handle` | 0.37 / 0.30 / 0.32 / 0.5 / 0.17 / 0.6 | MIT OR Apache-2.0; class B: `libloading` (§3.2) |
| Fonts | `fontique` (system fonts through fontconfig, §3.4), `egui_system_font_provider` | 0.11 / 0.37 | Apache-2.0 OR MIT |
| Video | `libmpv2` (feature `render`) for the core, `libmpv2-sys` for the render context (§5.2) | 6.0.0 / 4.0.1 | LGPL-2.1 |
| Dictionary images | `image` (png, jpeg, gif, webp, bmp), decoded on the lookup thread (POP-010) | 0.25 | MIT OR Apache-2.0 |
| Wayland drag and drop | `wayland-client`, `wayland-backend` (already in the graph through winit): a data device on winit's connection, which has none | 0.31 / 0.3 | MIT |
| Async | `tokio` | 1.x | MIT |
| Storage | `redb` | 4.x | MIT OR Apache-2.0 |
| Serialization | `serde`, `serde_json`, `toml`, `postcard` | 1.x | MIT OR Apache-2.0 |
| Compression | `lz4_flex` | 0.14 | MIT |
| ZIP | `zip` (`default-features = false`, `deflate-flate2`; `flate2` with `miniz_oxide` backend) | 9.x | MIT |
| HTTP | `reqwest` (`default-features = false`, `json`, `native-tls`) | 0.13 | MIT OR Apache-2.0 |
| Unicode | `icu_normalizer`, `unicode-segmentation` | 2.3 / 1.13 | Unicode-3.0 (class B) / MIT OR Apache-2.0 |
| Errors | `thiserror` (libraries), `anyhow` (binary and xtask only) | 2.x / 1.x | MIT OR Apache-2.0 |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | 0.1 / 0.3 | MIT |
| XDG paths | Own implementation in `nikomiru-core` (§3.6), no crate | — | — |
| System calls | `libc`, only in `nikomiru-app::linux` (`setlocale` for libmpv, termination signals, iconv) | 0.2 | MIT OR Apache-2.0 |
| D-Bus (portals) | Own minimal client in `nikomiru-app::dbus` (PLY-013), no crate | — | — |
| File dialogs, external links | The XDG portal (`FileChooser`, `OpenURI`) through the own D-Bus client, no crate; `xdg-open` as the fallback for links | — | — |
| i18n | `fluent-bundle`, `unic-langid` | 0.16 | Apache-2.0 OR MIT |
| Identifiers and hashes | `uuid`, `sha2`, `md4` (ed2k, M7) | — | MIT OR Apache-2.0 |
| Dictionary HTML (post-1.0 only, MDict): sanitized to the POP-009 allowlist, then converted to the popup's view model | `ammonia` | 4.x | MIT OR Apache-2.0 |
| Tests | `proptest`, `insta`, `criterion`, `wiremock`, `tempfile`, `egui_kittest` (UI tests through AccessKit, no renderer) | — | MIT/Apache |

Every new crate requires, in the PR: the license (verified by `cargo deny`), a justification and an assessment of the "write the code ourselves" alternative.

---

## 5. Architecture

### 5.1 Workspace

```
nikomiru/
├── Cargo.toml              # workspace, centralized [workspace.dependencies]
├── rust-toolchain.toml
├── deny.toml  about.toml
├── xtask/                  # cargo xtask: ci, fixtures, about, bench
├── crates/
│   ├── nikomiru-core/       # shared types, config, paths, errors, clock
│   ├── nikomiru-text/       # normalization, kana, preprocessors, sentences, furigana, character count
│   ├── nikomiru-dict/       # Yomitan import, redb storage, deinflection, lookup, word audio
│   ├── nikomiru-subs/       # SRT/VTT/ASS parsers, cue index, offset, encodings
│   ├── nikomiru-media/      # ffmpeg/ffprobe runner: subtitle extraction, clips, frames
│   ├── nikomiru-player/     # libmpv core (libmpv2): init, properties, commands, events;
│   │                       # the OpenGL render context of the video
│   ├── nikomiru-anki/       # AnkiConnect client, note composer, Lapis, return link
│   ├── nikomiru-library/    # library, file names, progress, statistics
│   ├── nikomiru-online/     # (M7) AniList/Jikan, Jimaku, OpenSubtitles
│   ├── nikomiru-ui/         # egui interface: player page, subtitle overlay, popup, pages, controls
│   └── nikomiru-app/        # binary: winit event loop, glutin (EGL) window, egui_glow, wiring, single instance
├── assets/                 # transforms/, anki/lapis/, i18n/*.ftl, icons
├── tests/fixtures/         # small committed files; media are generated with xtask
└── docs/
```

**ARC-010 — Dependency rules** (verified by T-ARC-010 with `cargo metadata`):

- `nikomiru-core` depends on no workspace crate.
- `nikomiru-text`, `nikomiru-subs`, `nikomiru-dict`, `nikomiru-anki` and `nikomiru-library` **MUST NOT** depend on the UI toolkit (`egui` and its crates, `winit`, `glutin`, `glow`, `accesskit`) or on `libmpv2`. They stay pure logic, testable without a display.
- Only `nikomiru-player` depends on `libmpv2` (and `libmpv2-sys`). Only `nikomiru-ui` and `nikomiru-app` depend on the UI toolkit. No crate depends on a retired toolkit (Dioxus, D-10; Qt through cxx-qt, D-13).
- **ARC-011**: `unsafe` is allowed only in `nikomiru-player` (mpv FFI and the render context), in `nikomiru-app::linux` (environment, `setlocale`, signals, iconv, the Wayland connection for drag and drop) and in `nikomiru-app::window` (the OpenGL context through glutin, the GL state around mpv's render). There is no C++. Every `unsafe` block MUST have a `// SAFETY:` comment on the line above it. `nikomiru-app` declares `#![deny(unsafe_code)]`, with `#[allow(unsafe_code)]` only on those modules; every other crate except `nikomiru-player` declares `#![forbid(unsafe_code)]`.

### 5.2 Video and UI in the same window (egui)

This was the **main technical risk** (R-1). The Dioxus design (a transparent WebKitGTK webview over a `GtkGLArea`) recomposed the whole window on the GTK main thread for every video frame and dropped frames at 4K under CPU load ([ADR 0001](adr/0001-video-composition.md)). D-10 moved to Qt Quick, which rendered mpv into a scene-graph item on its render thread ([ADR 0003](adr/0003-qt-quick-ui.md)). With D-13 the UI is egui: mpv draws each video frame into the window's framebuffer, and egui paints the interface over it in the same frame ([ADR 0006](adr/0006-toolkit-comparison.md)).

#### 5.2.1 Composition

```
winit window + glutin surface (OpenGL through EGL; GLX only as a fallback)
└── each frame, on the UI thread
    1. egui lays out the interface      (controls, subtitles, popups, pages)
    2. mpv draws the video              (libmpv render API, framebuffer 0)
    3. egui paints over it              (egui_glow, no clear)
    4. buffer swap, then mpv's report_swap
```

- The OpenGL context MUST be created through EGL when it is available (glutin, on X11 too), before the first frame.
- The video fills the window and the interface is painted over it. Pages without video (library, settings) do not call mpv's render.
- Input goes to egui; a click no element takes goes to the player page (play/pause, or closing the popups; a double click toggles fullscreen). Keys go to the shortcuts first (KEY-002); egui gets them only when a text field has the keyboard (KEY-004).
- The UI thread renders the video, so it MUST NOT wait for the mpv core (`<mpv/render.h>`): the player's commands are queued to a command thread (§5.3), and long work stays on worker threads, because on this thread it delays video frames.

Measured in ADR 0006 (spike S5): at 4K HEVC 10-bit under heavy CPU load, 60 renders per second and 0–2 dropped frames in 20 s, with about half of Qt Quick's process CPU. ADR 0003 keeps the Qt figures.

#### 5.2.2 mpv rendering

1. The mpv core is owned by `nikomiru-player` (Rust, `libmpv2`): options (PLY-005), properties, commands and events. Its `render` module holds the render context; the app hands it only the GL context's `get_proc_address`, the framebuffer and the native display.
2. The render context is created once, as soon as the window's GL context is current and before the first frame: `mpv_render_context_create` with the OpenGL API, `get_proc_address` from glutin's display, and the native Wayland or X11 display, which mpv needs for zero-copy hwdec interop (VA-API on EGL).
3. `loadfile` MUST wait until the render context exists: with `vo=libmpv`, a file loaded earlier has no video output. A file opened earlier (command line, second launch) waits in the session until then.
4. The mpv update callback (called from any thread) **MUST NOT** call mpv APIs. It only wakes the event loop for a redraw.
5. Each frame resets the GL state mpv expects (no blending, scissor, program, vertex array or texture bound), calls `mpv_render_context_render` on framebuffer 0 at the window's physical size with `FLIP_Y`, lets egui paint without clearing, swaps the buffers and calls `mpv_render_context_report_swap`.
6. Every `mpv_render_*` call runs on the UI thread, with the window's GL context current.
7. Shutdown, in this order: the render context is freed with the GL context current; then egui's painter, the session (which saves the position, joins its workers and closes the library) and the window; then, once the command thread has run what it had queued, the `quit` command, which sends `MPV_EVENT_SHUTDOWN` to the event-thread client; then the join of the `mpv-events` thread; finally the core is destroyed. The lifetimes of `libmpv2::render::RenderContext<'a>` are not enough to guarantee this: `'a` is only a `PhantomData`, and the `Drop` of `Mpv` calls `mpv_destroy`, which does not stop the core while another client exists (ADR 0001). This is why the render context is driven through `libmpv2-sys`.
8. `LC_NUMERIC` MUST be `"C"` before mpv is created, or libmpv refuses to start (`mpv_create` returns NULL); on X11 winit sets `LC_CTYPE` when its event loop starts. `nikomiru-app` calls `setlocale(LC_NUMERIC, "C")` right after creating the event loop, before the player.
9. On X11 the GL context is EGL too (glutin prefers EGL), so mpv gets zero-copy VA-API. If EGL is not available, glutin falls back to GLX, where `hwdec=auto-safe` uses a copy mode (`vulkan-copy` or `vaapi-copy`).
10. `gpu-hwdec-interop` is `vaapi` unless NVIDIA's proprietary driver is loaded (`/proc/driver/nvidia/version` exists), in which case mpv's automatic choice, CUDA included, is kept. It is set when the core is created, because the render context probes the interops as soon as it exists, before any file is loaded. On AMD and Intel the CUDA interop is useless, and probing it prints `Cannot load libcuda.so.1` on stderr (seen on the developer's session, 2026-10-19).
11. libmpv's messages go to Nikomiru's log (`mpv_request_log_messages`, level `warn` by default, `v` with debug logging), never to stderr; PRV-002 applies to them as well.

#### 5.2.3 Fallback

- Without OpenGL (neither EGL nor GLX) there is no window: the app reports the error and exits. Option D (mpv in its own window) is withdrawn: egui needs OpenGL too, so it would keep the video without the interface.
- The GtkOverlay design of ADR 0001 and its fallbacks B (separate transparent window over a `wid` child window) and C (`dioxus-native`) are withdrawn (D-10).

#### 5.2.4 Other platforms (post-1.0)

- Windows: the same integration, with glutin on WGL.
- macOS: OpenGL is deprecated (glutin still supports CGL); a Metal path may be needed, to be evaluated.

### 5.3 Threads and events

```
 ┌──────────── UI thread (main) ────────────┐   queued     ┌──── mpv-commands thread ────┐
 │ winit event loop, egui, the session      ├─────────────►│ commands, in order          │
 │ each frame: layout, mpv render, paint,   │              └─────────────────────────────┘
 │ swap                                     │   messages   ┌──── mpv-events thread ──────┐
 │                                          │◄─────────────┤ wait_event → PlayerEvent    │
 └──────────────────────▲───────────────────┘   (proxy)    └─────────────────────────────┘
                        │ messages (proxy)
 ┌──────────────────────┴──────────────────────────────────────────────────────────────┐
 │ workers: subtitles (ffmpeg), lookup, import, library writes, portal, single          │
 │ instance, signals, test harness                                                       │
 └───────────────────────────────────────────────────────────────────────────────────────┘
```

- The **UI thread** runs the winit event loop, egui and the session (the UI state), and renders the video in every frame (§5.2.2). It never waits for the mpv core.
- The **`mpv-commands` thread** runs the player's commands in order with the synchronous libmpv API: `PlayerHandle` queues them and never waits for them. Their errors are logged; a refused `loadfile` becomes an `EndFile` error of its load.
- The **`mpv-events` thread** waits for mpv events and turns them into `PlayerEvent`s stamped with their load's token (ARC-002).
- **Workers**, each on its own thread: subtitle loading and extraction, dictionary lookups and imports, library writes, the screensaver portal, the single-instance socket, the termination signals. Their results reach the UI thread as messages through the event loop's proxy, which also wakes it.

- **ARC-001**: `nikomiru-player` exposes `PlayerHandle` (asynchronous commands) and a stream of `PlayerEvent`. The observed properties (`observe_property`) are: `time-pos`, `duration`, `pause`, `speed`, `volume`, `mute`, `track-list`, `aid`, `sid`, `chapter-list`, `eof-reached`, `seeking`, `paused-for-cache`, `video-params`, `demuxer-cache-state`.
- **ARC-002**: every load has an increasing `LoadToken`. Events and asynchronous results with an old token are discarded. This is the equivalent of the Fushi bugs "re-check player and loadToken after every await".
- **ARC-003**: no synchronous read of mpv properties on the GUI thread during playback (Fushi BUG-3507). State is read from events.
- **ARC-004**: `time-pos` updates to the UI are limited to ≤ 30 Hz. Cue synchronization uses the most recent value with a binary search. Fushi's 125 ms timer is not needed.

### 5.4 Persistence

| What | Where | Format |
|---|---|---|
| Settings | `$XDG_CONFIG_HOME/nikomiru/config.toml` | Human-readable TOML; atomic write (temporary file + rename); permissions 0600 |
| Library, progress, statistics | `$XDG_DATA_HOME/nikomiru/library.redb` | redb; `postcard` values; `meta.schema_version` table |
| Dictionaries | `$XDG_DATA_HOME/nikomiru/dicts/<id>/dict.redb` | One file per dictionary (§6.5.3) |
| Cache (extracted subtitles, thumbnails, word audio) | `$XDG_CACHE_HOME/nikomiru/` | Files; can be deleted at any time |
| Logs | `$XDG_STATE_HOME/nikomiru/logs/` | Text, daily rotation, 7 files |

- **DAT-001**: schema migrations go forward only, are numbered and are idempotent. Before migrating, the file is copied to `library.redb.bak-v<N>`.
- **DAT-002**: if the file was written by a newer version of Nikomiru (schema higher than the supported one), it is opened read-only with a warning. It is never modified.
- **DAT-003**: the identity of a video is `uid = "video/" + sanitize(file name without extension)`, as in Fushi. In addition, `size` and `mtime` are stored to recognize a moved file.

### 5.5 External processes

- **EXT-001**: ffmpeg and ffprobe are searched in this order: the `paths.ffmpeg` setting, the `NIKOMIRU_FFMPEG` variable, `PATH`. At startup the version and the presence of the required encoders are checked (`ffmpeg -hide_banner -encoders`). If one is missing, the related feature is disabled with a clear message.
- **EXT-002**: arguments are always passed as a vector, never through a shell. Input paths are prefixed with `file:` to neutralize names that start with `-` or contain `:`.
- **EXT-003**: every process has a timeout (default 60 s; for subtitle extraction it scales with the duration) and can be cancelled. On cancellation the process is killed and partial files are removed.

### 5.6 Errors, logs and privacy

- **ERR-001**: libraries define typed errors with `thiserror`. The UI shows translated, actionable messages ("AnkiConnect is not responding on 127.0.0.1:8765: is Anki open?").
- **ERR-002**: no `panic!`/`unwrap()` on external input (files, network, dictionaries). Clippy with `unwrap_used` and `expect_used` counts as an error in library crates.
- **PRV-001**: zero telemetry. Without a user action or a feature the user has enabled, the only allowed connection is to AnkiConnect on loopback.
- **PRV-002**: logs contain neither subtitle text nor API keys. Paths are allowed.

### 5.7 Single instance and URL scheme

- **APP-001**: only one instance per user. A second launch passes its arguments (files to open, `nikomiru://` URLs) to the running instance through the Unix socket `$XDG_RUNTIME_DIR/nikomiru.sock`, then exits.
- **APP-002**: the `.desktop` file registers `MimeType=x-scheme-handler/nikomiru;` and the video types, with `Exec=nikomiru %U`.

---

## 6. Functional requirements

### 6.1 Video library (LIB)

| ID | Requirement | Fushi |
|---|---|---|
| LIB-001 | MUST open a single video file: from the file dialog, by drag & drop onto the window or as a command-line argument. Extensions offered in the dialog: mp4 mkv avi mov webm m4v ts m2ts mts flv wmv mpg mpeg ogv; the dialog MUST also allow "all files" | `kVideoExtensions` |
| LIB-002 | MUST import a folder recursively, in two modes: **by series** (default) and **by folder** | `video_library_import.dart` |
| LIB-003 | By series: the file name is parsed Anitomy-style (series, season, episode, group, resolution). If the season is missing, it is derived from the folder (`Season 2`, `S02`, `第2季`, trailing number). If a name cannot be parsed, the episode number is inferred by comparing it with neighboring files | `video_filename_parser.dart` |
| LIB-004 | Changing mode or rescanning MUST NOT lose progress, chosen subtitles or metadata. A scan MUST NOT recreate series the user has deleted | spec `2026-09-07-video-library-workflow.md` |
| LIB-005 | The home page shows "Continue watching" (latest episodes started and not finished) and the series grid with covers. The cover is the frame at 10% of the first episode (ffmpeg, cached) until metadata are available | |
| LIB-006 | Episode progress: saved at most once per second, only after the file is open and the resume seek has happened. On exit and on episode change it is saved immediately | `video_player_controller.dart` |
| LIB-007 | Resume rule: restart from 0 if the saved progress is ≥ 90% or if less than 3 s remain. "Previous episode" and auto-advance always start from 0 | `video_episode_start_policy.dart` |
| LIB-008 | `completed_at` is set the first time progress reaches 90%. The grid MUST show the watched / in progress / new state | |
| LIB-009 | At the end of an episode the next one MUST be able to start (setting `player.auto_next`, default yes), with a cancellable 5 s countdown | |
| LIB-010 | For each series the audio track, the chosen subtitle (by language or track title) and the subtitle offset are remembered. They apply to all episodes, which can however have their own values | `VideoBooks.delayMs` |
| LIB-011 | It MUST be possible to rename, merge and split series, and to move episodes from one series to another | |
| LIB-012 | Removing an item from the library MUST NOT delete the video file from disk | |

### 6.2 Player (PLY)

| ID | Requirement | Fushi |
|---|---|---|
| PLY-001 | Commands: play/pause, seek ±N s (default 3, range 1–30), absolute seek from the bar, speed 0.25–4.0 (step 0.1), volume 0–150 (step 5), mute, frame step forward/back, previous/next chapter, fullscreen | `video_player_shortcuts.dart` |
| PLY-002 | "Hold to speed up": speed 2.0× while the key is held (default `E`) | |
| PLY-003 | Audio track selection from the `track-list` list (language, title, codec, channels), saved per series | |
| PLY-004 | Overlaid control bar: it hides after 2 s without mouse movement during playback (it also hides the cursor) and reappears on movement. It stays visible while paused, with the mouse over the subtitles and with a popup open | |
| PLY-005 | mpv options applied **before** `initialize` (fixed): `vo=libmpv`, `config=no`, `input-default-bindings=no`, `input-vo-keyboard=no`, `osc=no`, `terminal=no`, `idle=yes`, `keep-open=yes`, `ytdl=no`, `sub-auto=no` | `video_mpv_config.dart` |
| PLY-006 | Configurable options, set before `loadfile`: `hwdec` (`auto-safe` default, `auto-copy-safe`, `no`), quality profile (`default` or `high-quality`), `deband`, `interpolation` (with `video-sync=display-resample` and `tscale=oversample`), `audio-channels` (default `auto-safe`; Fushi preset `7.1,5.1,stereo` as an option), `audio-pitch-correction=yes`, `tone-mapping=auto`. In addition, a "raw mpv options" field (`key=value` per line) applied last, with validation and a per-line error | `video_mpv_config.dart` (BUG-798, BUG-1545) |
| PLY-007 | Resume: the position is set with the `start` property before `loadfile` and reset (`start=none`) after `file-loaded`. NO seek is done after loading, because `loadfile` overwrites it | BUG-1288 |
| PLY-008 | After `file-loaded`: adopt the actual `aid`; disable mpv rendering for text tracks (`sid=no`, `sub-visibility=no`); apply the saved volume and speed | `video_player_controller.dart` `load()` |
| PLY-009 | The player and the render context are **reused** from one episode to the next: only the content is recreated with `loadfile replace` | BUG-120 |
| PLY-010 | Speed uses only the `speed` property, never the `scaletempo` filter | TODO-116 |
| PLY-011 | Screenshot (`S`): the frame without subtitles goes into the `Pictures/Nikomiru` folder (`$XDG_PICTURES_DIR/Nikomiru`) | |
| PLY-012 | Playback errors (missing file, unsupported codec, `end-file` with an error) MUST show an understandable message and must not block the app | |
| PLY-013 | Screensaver inhibition during playback, through the XDG `Inhibit` portal (flag *idle*), kept for one minute after a pause. Nikomiru talks to the portal with its own minimal D-Bus client (`nikomiru-app::dbus`): `zbus` would add about 45 crates for three fixed calls. If the portal is not available, a warning is logged: mpv's `stop-screensaver` has no effect under the render API, because mpv owns no window | |
| PLY-014 | Thumbnails on the progress bar: post-1.0 | |
| PLY-015 | Screenshot with the overlay subtitles (`Shift+S`): post-1.0, because it requires compositing the overlay onto the frame | |
| PLY-016 | **mpv option presets in `mpv.conf` format.** The mpv options set from the UI (PLY-006, PLY-018) and the raw options form one *option set*. In Settings → Player the user MUST be able to: (a) **save** the option set as a named preset in `$XDG_CONFIG_HOME/nikomiru/mpv/<name>.conf`, or to any file chosen in the file dialog; (b) **load** an `mpv.conf` file (a preset, or any file such as the user's own `~/.config/mpv/mpv.conf`); (c) switch between saved presets. Loading replaces the whole option set: options that the UI exposes fill its controls, the others go to the raw options. Accepted syntax: `key=value`, `key` (flag = `yes`), `no-key`, `--key=value`, quoted values, `#` comments, blank lines, `profile=` with a built-in profile (`fast`, `high-quality`, `gpu-hq`). Before applying, a preview lists every line as applied, ignored (with the reason) or invalid (with mpv's error); nothing changes until the user confirms. User-defined `[sections]`, `include=` and conditional profiles are ignored with a warning. Saving writes a header comment (Nikomiru version, date) and only the options that differ from the defaults, so the file also works in mpv | |
| PLY-017 | **Options that the raw options and the presets can never set** (PLY-006, PLY-016): they are rejected with a reason and never reach mpv. (1) The fixed options of PLY-005. (2) Options that run code, load other configuration, open sockets or write files outside the XDG folders (NFR-SEC-02): `script*`, `scripts`, `load-*`, `include`, `config-dir`, `use-filedir-conf`, `input-*`, `log-file`, `dump-stats`, `stream-dump`, `stream-record`, `record-file`, `watch-later-*`, `write-filename-in-watch-later-config`, `screenshot-*` (Nikomiru owns screenshots, PLY-011), `ytdl*`. (3) Options with no effect under the libmpv render API (`gpu-api`, `gpu-context`, `vo-*`) are ignored with a warning, and so are `sub-*` options for text subtitles, which Nikomiru draws itself (§6.4). The list is a single table in `nikomiru-player`, covered by tests | |
| PLY-018 | **Render and audio settings in the UI** (Settings → Player, groups Rendering, Audio and Advanced): profile (`default`, `fast`, `high-quality`), `scale`, `cscale`, `dscale`, `deband` (with `deband-iterations` and `deband-threshold`), `dither-depth`, `interpolation` (with `tscale` and `video-sync`), `tone-mapping`, `target-peak`, `hdr-compute-peak`, `deinterlace`, `brightness`, `contrast`, `saturation`, `gamma`, `hwdec`; audio: `audio-channels`, `audio-pitch-correction`, `volume-max`, `audio-normalize-downmix`. Each control writes the same mpv option that a preset contains (PLY-016). Options that mpv can change at runtime apply immediately (SET-002); the others apply from the next file, with a note next to the control. Each group has a reset to the defaults (SET-006) | |

### 6.3 Shortcuts (KEY)

| ID | Requirement |
|---|---|
| KEY-001 | All actions can be remapped; the bindings are saved in `config.toml` (`[shortcuts]`). |
| KEY-002 | Keys are recognized by their **physical key**, i.e. the scan code (winit's physical key, as a Linux evdev code), not by the produced character: shortcuts stay the same with any layout and input method (Fushi BUG-936). |
| KEY-003 | Two actions in the same context (`global`, `library`, `player`, `popup`) cannot have the same key: the editor reports the conflict and does not save. |
| KEY-004 | While a text field is focused, single-key shortcuts are disabled. |

Default player bindings, from Fushi `shortcut_defaults.dart`:

| Action | Keys | Milestone |
|---|---|---|
| Play/pause | Space, P, MediaPlayPause | M1 |
| Seek back / forward | ←, A / →, D | M1 |
| Previous / next subtitle | Ctrl+← / Ctrl+→ | M2 |
| Replay current / previous cue | R / Shift+R | M2 |
| Volume + / − ; mute | ↑, 0 / ↓, 9 ; M | M1 |
| Speed + / − / normal | ], = / [, - / Backspace | M1 |
| Hold for 2× speed | E | M1 |
| Previous / next frame | , / . | M1 |
| Screenshot | S | M1 |
| Fullscreen | F, F11 | M1 |
| Subtitle list / search | L / Ctrl+F | M2 |
| Blur subtitle / hide subtitle | B / H | M2 |
| Subtitles off / on (the track stays chosen) | V | M2 |
| Subtitle offset −100 / +100 ms | Z / X | M2 |
| Align the start of the previous / next cue to the position | Ctrl+Shift+← / Ctrl+Shift+→ | M2 |
| Previous / next chapter | PageUp / PageDown | M1 |
| Character cursor for keyboard lookup | Enter | M3 |
| Create card from the popup entry | Ctrl+Enter | M4 |
| Back (closes popup → leaves fullscreen → returns to the library) | Esc, Alt+← | M1 |

### 6.4 Subtitles (SUB)

**Principle (from Fushi):** mpv does **not** draw text subtitles. Nikomiru parses them into cues and draws them in its own overlay (egui). This is what makes clicking a single character, blur, styles and mining possible. Only graphic tracks (PGS, VobSub, DVB) are drawn by mpv (`sub-visibility=yes`), without lookup.

| ID | Requirement | Fushi |
|---|---|---|
| SUB-001 | **SRT** parser: BOM, CRLF/LF, missing or out-of-order indices, `{...}` removed (BUG-105), `<i> <b> <u> <font color>` tags converted to markup, timestamps with `,` or `.` | `srt_parser.dart` |
| SUB-002 | **WebVTT** parser: header, cue settings (`line`, `position`, `align`), `<c>`, `<v>`, `<ruby>`, `<rt>` tags, cues with an identifier, `NOTE` and `STYLE` blocks ignored | `vtt_parser.dart` |
| SUB-003 | **ASS/SSA** parser (basic for 1.0): `[Script Info]` (PlayResX/Y, default 384×288), `[V4+ Styles]`, `[Events]` (Dialogue with Layer, Start, End, Style, Text), overrides `\N \n \h \an1-9 \pos \b \i \u \s \c \1c \fs \fn \r` (`\n` is a line break only with `WrapStyle: 2`, a space otherwise, as in libass; a missing PlayResX or PlayResY is derived from the other at 4:3, as in libass). Other overrides are ignored without error. Lines with `\p1` drawings are excluded from the clickable text | `ass_parser.dart`, `subtitle_markup.dart` |
| SUB-004 | Every cue has `start_ms`, `end_ms`, `plain_text` (used for lookup, mining and counts) and a separate `markup`, used only for drawing. In `plain_text` the lines of a cue are separated by `\n` (Fushi joins them with a space and keeps the break positions apart): lookup treats `\n` as a boundary, and mining (M4) joins the lines as the note type needs | |
| SUB-005 | Encoding: UTF-8/UTF-16 BOM → otherwise UTF-8 validation → otherwise try Shift_JIS (CP932), EUC-JP, GB18030, Big5, CP1252 through `iconv`, choosing the first one that decodes without errors and with the highest score of plausible characters. The detected encoding is shown with a readable name ("Japanese — Shift_JIS (CP932)") and can be forced | `text_file_io.dart` |
| SUB-006 | External files next to the video: same base name with `.srt .ass .ssa .vtt`, also with a language suffix (`.ja.srt`, `.jpn.ass`, `.jp.srt`). They are loaded automatically according to the preferred language (default `ja`) | |
| SUB-007 | Embedded text tracks: extracted with `ffmpeg -y -i file:<video> -map 0:<ff-index> -c:s <srt\|ass\|webvtt> <cache>`. Several tracks are extracted in a single pass; if the pass fails, it is retried track by track. The cache key is (path, size, mtime, ff-index) | `video_subtitle_source.dart` |
| SUB-008 | Automatic subtitle selection: (1) the choice saved for the series; (2) an external file in the preferred language; (3) an embedded track in the preferred language (`lang` = ja/jpn, or a title that contains "Japanese" or "日本語"); (4) none | |
| SUB-009 | Overlay: several cues active at once are allowed (sorted by Layer and then by order in the file). Text is split into **graphemes** (`unicode-segmentation`); each grapheme is a hit target of the laid-out text carrying its byte offset in `plain_text` | `video_subtitle_overlay.dart` |
| SUB-010 | Default style: white text, weight 400, soft shadow, no box, 36 px relative to a video height of 1080 (scales proportionally), 75 px from the bottom; it moves up when the control bar is visible. The style options (font, size, outline, shadow, box, position) can be changed. "Respect ASS styles" (default no) uses the ASS colors, font and position | `video_subtitle_style.dart` |
| SUB-011 | Offset: positive `delay_ms` = the subtitle appears later. The compared cue time is `pos − delay_ms`; seek and mining apply the inverse. Z/X = ∓100 ms; the offset is shown in a toast | |
| SUB-012 | Navigation: "previous/next" goes to `cue.start − 180 ms`, never before the start of the previous cue, taking `delay_ms` into account (BUG-259). The target is computed from the current position with a *floor* search, not from the cached index. Without subtitles it falls back to the ±N s seek. With the keyboard, if the previous cue is more than 10 s away, a time seek is done (BUG-3339) | `video_player_controller.dart` |
| SUB-013 | "Replay cue" goes to the start of the current cue and, if triggered from the popup, pauses at the end of the cue | `replayCue` |
| SUB-014 | Blur / hidden: modes for the main subtitle (`none`, `blur`, `hidden`). Blur is a soft blur of the text, with sigma = max(12, font_size·0.45). The text is revealed on mouse hover. Blur applies only during playback and with no popup open (BUG-2198, BUG-2235). **V** turns the subtitles off and on: nothing is drawn, not even on hover, and the chosen track stays for when they come back (maintainer request after the M2 checklist) | |
| SUB-015 | Subtitle list panel (`L`): list of all cues with time, text search (`Ctrl+F`), automatic scrolling that follows playback (in gaps between cues it stays on the previous cue), click on the time to jump, click on a word for lookup | `video_subtitle_jump_panel.dart` |
| SUB-016 | Align the start of the previous or next cue to the current position (Ctrl+Shift+←/→), updating `delay_ms` | |
| SUB-017 | **Secondary** subtitle (e.g. a translation): second layer with its own offset (default: follows the main one), its own blur mode and position. Milestone M7 | |
| SUB-018 | Automatic alignment with the waveform: RMS envelope with ffmpeg `astats` and cross-correlation (100 ms bins, ±15 s search, minimum confidence 0.15). Milestone M7 | `subtitle_auto_align.dart`, `audio_energy_probe.dart` |
| SUB-019 | Advanced ASS (`\t`, `\move`, `\fad`, `\clip`, karaoke `\k`, rotations): post-1.0 | |

### 6.5 Dictionaries: import and management (DIC)

#### 6.5.1 Formats

| ID | Requirement | Fushi |
|---|---|---|
| DIC-001 | MUST import **Yomitan/Yomichan** dictionaries (zip with `index.json`, format 3; format 1/2 is SHOULD): `term_bank_*`, `term_meta_bank_*` (`freq`, `pitch`, `ipa`), `kanji_bank_*`, `kanji_meta_bank_*` (`freq`), `tag_bank_*`, `styles.css`, media files | `importer.cpp` |
| DIC-002 | Unlike Fushi, MUST import the `tag_bank_*` files (category, order, notes, score) and use them for chips and for sorting. MUST keep `sequence` and `score` | Fushi gap |
| DIC-003 | Frequencies: values as a number, `{value, displayValue?}` and `{reading, frequency}` (also written `{reading, value}`, which Fushi reads too). Unlike Fushi, MUST also accept string values (reading the leading integer) and respect `frequencyMode` from `index.json` (`rank-based` default; `occurrence-based` = higher is better) | Fushi gap |
| DIC-004 | MDict, StarDict, DSL, Migaku: post-1.0 | |

#### 6.5.2 Import pipeline

| ID | Requirement |
|---|---|
| DIC-010 | Import runs in a background task with progress (files and entries) and can be cancelled; the UI stays responsive. |
| DIC-011 | ZIP safety: reject absolute paths, `..` components, symbolic links, entries > 512 MiB uncompressed, compression ratio > 200:1 on entries > 1 MiB, total uncompressed > 8 GiB. Strip the wrapper folder that holds the whole archive, and the one nested in it, until `index.json` is at the root; ignore `__MACOSX` and `.DS_Store` (BUG-2053). Support the Info-ZIP extra field for Unicode paths (BUG-3277) and ZIP64. |
| DIC-012 | Limits: 10 M entries per dictionary, glossary ≤ 10 MiB per entry. If a limit is exceeded, the import fails cleanly. |
| DIC-013 | Import writes to `dicts/<id>.importing/` and, only on complete success, does an atomic rename to `dicts/<id>/` and registers the dictionary. After an error or a crash nothing partial remains; leftover `.importing` folders are deleted at startup. |
| DIC-014 | Disk full: specific message (`ERR_STORAGE_FULL`), with cleanup of partial files. |
| DIC-015 | The **type** is detected: term if there are term banks; otherwise kanji if there are kanji banks; otherwise the mode of the first meta is checked (`freq` → frequency, `pitch`/`ipa` → pitch). A term dictionary with kanji banks counts as both. |
| DIC-016 | A dictionary with the same title and the same revision as one already imported is rejected. With a different revision, an update is offered (atomic replacement). |

#### 6.5.3 Storage (one redb file per dictionary)

| Table | Key → value | Notes |
|---|---|---|
| `info` | `&str` → `&[u8]` | `index.json`, `styles.css`, schema version, counts |
| `terms` | `u64` (id) → `TermRecord` (postcard) | expression, reading, definition tags, rules, term tags, score, sequence, JSON glossary compressed with lz4 |
| `term_by_expression` | multimap `&str` → `u64` | |
| `term_by_reading` | multimap `&str` → `u64` | Only if the reading differs from the expression |
| `term_meta` | multimap `&str` → `MetaRecord` | freq / pitch / ipa |
| `kanji` | `&str` (character) → `KanjiRecord` | |
| `kanji_meta` | multimap `&str` → `MetaRecord` | |
| `tags` | `&str` → `TagRecord` | |
| `media` | `&str` (normalized path: `\` → `/`, no leading `/`) → `&[u8]` | |

- **DIC-020**: storage sits behind a `DictStore` trait; the redb implementation is the first one. Spike S2 must meet the limits of NFR-PERF-04/05; if it cannot, we switch to a custom format (mmap + hash table + blob, like fushidicts) behind the same trait.
- **DIC-021**: one file per dictionary: deleting a dictionary = closing it and deleting its folder (Fushi BUG-1756: mmap locks on Windows). The registry (order, enabled, language, aliases) lives in `library.redb`.

#### 6.5.4 Management

| ID | Requirement |
|---|---|
| DIC-030 | List of dictionaries by type (terms, kanji, frequencies, pitch accent): title, revision, declared language, import date, number of entries, size. A dictionary with several types (DIC-015) has its row under the first of them in this order; under each of its other types a reference shows where it ranks. |
| DIC-031 | Reordering by drag and drop: the order decides the position of the glossaries in the popup and the priority of the frequency dictionaries. The order is one for every type: a dictionary dropped in its type's list goes just above the row under the drop marker (or just below the list's last row), and every other dictionary keeps its place. ▲/▼ buttons do the same one row at a time, for the keyboard and screen readers. |
| DIC-032 | Every dictionary can be enabled or disabled. A disabled one is **not queried** (Fushi BUG-3454/2997: exclude it before truncation, not at render time). |
| DIC-033 | Deletion with confirmation; renaming for display only (`display_name`). |
| DIC-034 | Collapse/expand state of the dictionary in the popup: `expanded` / `collapsed` / `automatic` (only the first N are expanded). |
| DIC-035 | Per-dictionary styling: a safe subset of CSS properties taken from the dictionary's `styles.css` and from the user's CSS (color, font size, weight and style, margins, padding, borders, background), applied only to that dictionary's popup content. The subset is the one of spike S4 ([ADR 0005](adr/0005-popup-qml.md)). The user's CSS of a term dictionary is written on the dictionaries page, in a code field with Save, Revert and Clear, and applied after its `styles.css` through the same subset; the page says what the subset keeps of it: the rules applied, the properties left out and the rules ignored. |

### 6.6 Lookup (LKP)

The algorithm is a port of Yomitan's, as implemented by fushidicts. Fushi: `native/fushidicts/fushidicts_src/lookup.cpp`, `deinflector.cpp`, `text_processor/text_processor.cpp`, `scan/word_scan.cpp`.

#### 6.6.1 Text scanning

| ID | Requirement |
|---|---|
| LKP-001 | Lookup starts from the clicked grapheme. For scripts with spaces (Latin, Cyrillic, etc.) it moves back to the start of the word, without crossing an apostrophe (`l’homme` clicked in `homme` gives `homme`, as in Fushi). |
| LKP-002 | The query text goes from the starting point to the end of the cue (in subtitles) or up to 400 characters (in free text), and stops at the delimiters ``。、！？…‥「」『』（）()【】〈〉《》〔〕｛｝{}［］[]・：；:;，,.─\n\r"'“”‘’«»‹›``. A single space does not stop the scan (BUG-1773) unless whitespace or a delimiter follows it, so the query never ends with a space (`rock 'n' roll` gives `rock`); an apostrophe inside a word does not stop it either (BUG-2056). |
| LKP-003 | Normalization before the search: truncate to 2000 characters, `\n` → space, emoji → space, strip punctuation at the start and end, replace lone surrogates. |
| LKP-004 | Candidates: prefixes of the first **16** code points, from longest to shortest. Never cut between two letters of scripts with spaces; discard prefixes that end with a space; for CJK every code point is a valid cut point. |

#### 6.6.2 Preprocessors (text variants)

| ID | Requirement |
|---|---|
| LKP-010 | Every preprocessor generates a set of variants. The Cartesian product is deduplicated, keeping the minimum number of "steps" (one step for each preprocessor that changes the text). |
| LKP-011 | Chain for Japanese, in this order: NFKC → kana (unchanged / katakana→hiragana, with `ー` replaced by the vowel of the previous kana and ヵヶ kept / hiragana→katakana) → collapse of repeated っっ and ーー (**never** full collapse, BUG-1777) → ASCII → full width → kanji variants (table of 2122 entries) → expansion of the iteration marks 々ゝゞヽヾ → full-width digits → kanji numerals. |
| LKP-012 | Chains for other languages (apostrophes, lowercase, removal of diacritics and combining marks, harakat, niqqud, Korean jamo **last**, BUG-2148): they are enabled only for the configured **study language**, or for the language declared by each dictionary. This is an improvement over Fushi, which applies everything to all languages. |

#### 6.6.3 Deinflection

| ID | Requirement |
|---|---|
| LKP-020 | Data: the Yomitan tables in `assets/transforms/<lang>.json` (schema `conditions`, `transforms`, `rules` of type suffix/prefix/wholeWord, `conditionsIn`/`conditionsOut`; `phrasalVerbs` for English only). |
| LKP-021 | Conditions as bit masks, up to 64 per language; `subConditions` are expanded to a fixed point. |
| LKP-022 | The algorithm is recursive, with a maximum depth of 10. It always emits `(text, conditions, trace)`. A rule applies if `conditions == 0` or `conditions & conditionsIn ≠ 0`. For suffixes, lengths are tried from the maximum down to 1; for `wholeWord` the remaining prefix must be empty. |
| LKP-023 | Part-of-speech filter: with conditions 0 all terms are kept. Otherwise a term is kept only if the rules in its `rules` intersect the conditions. `*` = all. A dictionary with no rules in any entry treats empty rules as `*` (BUG-2543). |
| LKP-024 | The trace is kept in removal order and shown reversed (`-て « -いる « -た`), with translated names and descriptions. |

#### 6.6.4 Search, merging, sorting

| ID | Requirement |
|---|---|
| LKP-030 | For each deinflected form, all enabled term dictionaries are queried, in order, for an equal expression or reading. |
| LKP-031 | Mixed kanji+kana spelling (BUG-3261): each kanji group (at most 3 groups) is replaced with the readings found for that group (at most 16 per group, 64 combinations), and the reading index is queried. A result is accepted only if every kanji group of the headword appears literally in the query or is spelled out in full in the corresponding kana. |
| LKP-032 | Redirects: a glossary made only of `[formOf, [rules…]]` (≤ 1 KiB) is replaced with a one-level lookup of the lemma, without the part-of-speech filter; the rules are appended to the trace. Records that have a redirect label **and** a real definition are kept (BUG-2853, BUG-2566). |
| LKP-033 | Merging by (expression, reading): the longest `matched` in code points wins; on a tie, the one with fewer preprocessor steps wins; Kansai exception -た → ため/たび (BUG-3212). Glossaries are appended in dictionary order; the score is the maximum. |
| LKP-034 | Sorting, in this order: (1) longest `matched`; (2) fewer preprocessor steps; (3) shorter deinflection trace; (4) headword equal to the deinflected form; (5) frequency: for each frequency dictionary, in user order, the minimum value ≥ 0, ascending (missing ones go last); (6) **dictionary priority** (improvement: Fushi does not have it); (7) score descending; (8) expression equal to the reading. |
| LKP-035 | The `max_results` budget (default 10) counts **headword groups**, not glossary rows (BUG-1472). Frequencies are added after deduplication, pitch accent after truncation (BUG-1304). |
| LKP-036 | Grouping: key `expression\n(reading or expression)`. Inside a group, glossaries are sorted by dictionary. Frequencies deduplicated by (dictionary, values); pitch accents by (dictionary, positions, pattern, transcriptions). `best_length` = maximum `matched`, used to highlight the text. |
| LKP-037 | Kanji lookup: if the query is exactly one Han character, or by clicking a kanji of the headword in the popup. Result: character, onyomi, kunyomi, meanings, strokes, radical, statistics, frequencies. |
| LKP-038 | LRU cache of results (5000 entries, key = first 16 code points + the set of active dictionaries); it is invalidated when a dictionary changes. |

#### 6.6.5 Frequencies and pitch accent

| ID | Requirement |
|---|---|
| LKP-040 | Frequencies: one chip per dictionary `Dictionary: v1, v2` (prefers `displayValue`). Harmonic value = `floor(n / Σ 1/vᵢ)` over the first value > 0 of each dictionary; no values = `9999999`. |
| LKP-041 | Pitch accent: `pitch` mode (`position` integer or pattern, `nasal`, `devoice`) and `ipa` mode. Split into morae (small kana attach to the previous mora), high/low per mora like Yomitan's `isMoraPitchHigh`, downstep line and `[n]` label. Unlike Fushi, it also shows nasal and devoicing. |
| LKP-042 | Pitch accent category: heiban / atamadaka / nakadaka / odaka / kifuku (`v*` and `adj-i` = kifuku when the position is > 0). |
| LKP-043 | Deduplication of pitch accents across dictionaries (default yes): identical payloads become one row with the label "pitch accent · N". |

#### 6.6.6 Furigana

| ID | Requirement |
|---|---|
| LKP-050 | `distribute_furigana(expression, reading)`, like Yomitan's `distributeFurigana`/`segmentizeFurigana`: splits into kana and kanji groups and aligns the reading; if the alignment is ambiguous, the reading goes over the whole expression. |
| LKP-051 | Automatic furigana on subtitle text is post-1.0; it needs the greedy segmentation of LKP-060. |

#### 6.6.7 Segmentation

| ID | Requirement |
|---|---|
| LKP-060 | Word segmentation without a morphological tokenizer: greedy longest-match with `lookup(max_results = 1)` (like Fushi `japanese_language.dart`). It is used for "next/previous word" with the keyboard cursor and, in the future, to highlight already mined words. No MeCab/IPADIC dictionary (licenses). |

#### 6.6.8 Intended differences from Fushi

Imported tag banks (DIC-002) · string frequencies and `frequencyMode` (DIC-003) · per-language preprocessors (LKP-012) · dictionary priority in sorting (LKP-034) · nasal and devoicing shown (LKP-041). Each one has a dedicated test in [TEST.md](TEST.md).

### 6.7 Dictionary popup (POP)

| ID | Requirement | Fushi |
|---|---|---|
| POP-001 | Click on a subtitle character → if the video was playing it is paused and `paused_for_lookup` is remembered → the popup opens without waiting for the pause to be confirmed. | `video_fushi_page.dart` |
| POP-002 | The matched text (`best_length` graphemes) is highlighted in the subtitle. | |
| POP-003 | Hover with Shift (movement threshold 8 px; not repeated on the same character). Option `lookup.hover_without_modifier` (default no). Hover lookup pauses like a click. If the pointer leaves the subtitle and the popup for 320 ms, the popup closes and playback resumes (`lookup.resume_on_leave`, default yes). | |
| POP-004 | When all popups are closed, playback resumes **only** if the lookup paused it (BUG-072). | |
| POP-005 | With a popup open, a click on another character of the subtitle or the list replaces the entry and stays paused. | |
| POP-006 | Position: above the subtitle if there is room, otherwise below. It never covers the looked-up word and shrinks to fit in the window. Configurable maximum size (`[popup]`, §9; 560 × 500 points by default): a popup is that wide and as tall as its content up to that height. Resizable by dragging the border: the edge away from the word, the side edges and the corners between them (the edge next to the word has no grip: moving it would cover the word). The dragged edge follows the pointer while the popup keeps its side of the word, stays centred on it and inside the window; the size it gets, within 240 × 120 and 4096 × 4096 points, holds for the next popups until the app closes. `config.toml` is not written: the app has no writer yet that keeps a hand-edited file and its comments (SET-001). | `dictionary_popup_layer.dart` |
| POP-007 | Header of each entry: headword with furigana (LKP-050), ♪ audio button (AUD), create-card button (+ / ✓ / ✓↩, ANK-030), deinflection trace chips with a description on hover, frequency row, pitch accent row, term tag chips. | `popup.js` |
| POP-008 | One collapsible block per dictionary, with the collapsed state according to DIC-034. Definition tags appear as chips: numeric tags are hidden, a part-of-speech tag repeated on consecutive senses is omitted. | |
| POP-009 | Yomitan **structured content**: converted into the popup's view model (typed blocks and runs, drawn by egui) from an allowlist of tags (`br ruby rt rp table thead tbody tfoot tr td th span div ol ul li details summary img a`). The style hooks of each tag (Yomitan's `gloss-sc-<tag>` class), the `data-sc-*` values, `lang` and an allowlist of `style` properties feed the styling subset of DIC-035. **No raw HTML** from Yomitan dictionaries and no scripts. Tables in a scrollable container. | `renderStructuredContent` |
| POP-010 | Dictionary images, read from the dictionary's `media` table and decoded on the lookup thread: PNG, JPEG, GIF (first frame), WebP and BMP, recognised by their bytes; files up to 4 MiB, decoded within 8192 pixels a side and 128 MiB, then scaled down to 1280 pixels a side; at most 64 MiB of pixels per popup. The UI never loads an address from a dictionary. Respects `width`, `height`, `sizeUnits`, `imageRendering`, `title` (tooltip) and `collapsed` (the alternative text, as a link that shows the image); a missing, unreadable or too large image shows its alternative text. | |
| POP-011 | Internal links (`?query=` or the link text without `<rt>`, BUG-2456) → new lookup in a **nested popup**. External links (`http`/`https` only) → system browser through the OpenURI portal, or `xdg-open` with the URL as its only argument. | |
| POP-012 | Nested popups: select text inside a glossary and a child popup opens. Esc or Back closes the last one; a click on the parent closes the children; a click outside closes everything. Maximum depth 8. | |
| POP-013 | Kanji entries (LKP-037) shown with a dedicated card. | |
| POP-014 | Keyboard navigation: Tab/Shift+Tab between entries, Enter = audio, Ctrl+Enter = create card, Esc = close. Alt+wheel = next or previous entry. | |
| POP-015 | "Replay sentence" and "go to sentence" from the popup (SUB-013). | |

### 6.8 Word audio (AUD)

| ID | Requirement | Fushi |
|---|---|---|
| AUD-001 | Audio sources in an ordered, configurable list; each one can be enabled or disabled. | `audio_source_config.dart` |
| AUD-002 | **URL template** source with `{term}` and `{reading}` URI-encoded. The response can be Yomitan JSON `{type:"audioSourceList", audioSources:[{name,url}]}` or, unlike Fushi, direct audio (`Content-Type: audio/*`). | `word_audio_resolver.dart` |
| AUD-003 | **Local database** source in the "Local Audio Server for Yomichan" format (`entries(expression, reading, file, source)`, `android(file, source, data)`): first the exact pair, then the expression only, with priority by sub-source (nhk16, forvo, jpod, …). The format is SQLite: it is read with `turso` (MIT), to be validated in spike S3 (D-2). | `local_audio_db.dart` |
| AUD-004 | Resolution: for playback the first valid result in order is enough; the menu (right click on ♪) queries all sources in parallel and shows them all. Timeouts: 8 s for the connection, 10 s for the response. A 4xx response = "no audio"; a connection error pauses the host for 45 s. | |
| AUD-005 | Playback uses a **second audio-only libmpv instance** (`vid=no`, `video=no`), independent of the player. Configurable volume. | |
| AUD-006 | Option "play automatically when the popup opens", default yes: it does not overlap the dialogue because the lookup already pauses the video (POP-001). It can be disabled. | |
| AUD-007 | There are no default JapanesePod101 or Forvo URLs (terms of service). The user adds their own templates. | |
| AUD-008 | On-disk cache of downloaded audio (key = URL), limited to 500 MiB with an LRU policy. | |

### 6.9 Mining to Anki (ANK)

#### 6.9.1 AnkiConnect

| ID | Requirement | Fushi |
|---|---|---|
| ANK-001 | Default endpoint `http://127.0.0.1:8765`, configurable (host, port, HTTPS, API key). Request body: `{"action", "version": 6, "params"?, "key"?}`; `key` only if not empty. Timeouts: 10 s per request, 5 s for the connection. | `ankiconnect_service.dart` |
| ANK-002 | Response: an HTTP status other than 200, a non-JSON body or a body without `result`/`error` → protocol error. A non-null `error` → error. For `addNote`, the text `cannot create note because it is a duplicate` → dedicated duplicate error. | |
| ANK-003 | Retry: a single new attempt, if the pooled connection had expired. `addNote` and `createModel` are **not idempotent**: they are retried only if the connection phase failed. Otherwise the error is "unknown outcome, check in Anki", without retrying. | |
| ANK-004 | Error codes shown to the user: connection refused, timeout (port probably taken by another program), HTTP error, invalid field mapping, empty first field, Lapis missing. | |
| ANK-005 | Actions used: `version`, `deckNames`, `modelNames`, `modelFieldNames`, `addNote`, `canAddNotesWithErrorDetail`, `findNotes`, `notesInfo`, `updateNoteFields`, `addTags`, `storeMediaFile`, `getMediaFilesNames`, `deleteMediaFile`, `createModel`, `createDeck`, `guiBrowse`, `multi`. | |
| ANK-006 | Refresh configuration: call `version`, `deckNames`, `modelNames` and `modelFieldNames` for the selected model. The selection is kept by name. | |

#### 6.9.2 Lapis note

| ID | Requirement |
|---|---|
| ANK-010 | Vendored Lapis templates (front, back, CSS) in `assets/anki/lapis/`, with a pinned version (Fushi uses v1.7.0). Fields, in order: Expression, ExpressionFurigana, ExpressionReading, ExpressionAudio, SelectionText, MainDefinition, DefinitionPicture, Sentence, SentenceFurigana, SentenceAudio, Picture, Glossary, Hint, IsWordAndSentenceCard, IsClickCard, IsSentenceCard, IsAudioCard, PitchPosition, PitchCategories, Frequency, FreqSort, MiscInfo. |
| ANK-011 | "Create Lapis deck": `createModel` if no model named "Lapis" exists; `createDeck("Lapis")`; then refresh the configuration and check that both exist; select them and apply the default mapping. It is idempotent. Outcome: `created` / `already exists` / `failed`. |
| ANK-012 | Default Lapis mapping: Expression `{expression}` · ExpressionFurigana `{furigana-plain}` · ExpressionReading `{reading}` · ExpressionAudio `{audio}` · MainDefinition `{glossary-first}` · Sentence `{sentence}` · SentenceAudio `{sentence-audio}` · Picture `{card-image}` · Glossary `{glossary}` · PitchPosition `{pitch-accent-positions}` · PitchCategories `{pitch-accent-categories}` · Frequency `{frequencies}` · FreqSort `{frequency-harmonic-rank}` · MiscInfo `{source-link} {clip-timestamp}` · IsWordAndSentenceCard `x`. |
| ANK-013 | Lapis detection: the name contains "lapis" (case-insensitive), or the fields include Expression, MainDefinition and Sentence. |

#### 6.9.3 Field composition

| ID | Requirement |
|---|---|
| ANK-020 | Every note field has a template with `{name}` placeholders (regex `\{[^}]*\}`); an unknown placeholder becomes an empty string. The mapping is saved per note type. |
| ANK-021 | Placeholders to implement: `{expression}` `{reading}` `{furigana-plain}` `{glossary}` `{glossary-first}` `{single-glossary-<dictionary>}` `{sentence}` `{cue-sentence}` `{frequencies}` `{frequency-harmonic-rank}` `{pitch-accent-positions}` `{pitch-accent-categories}` `{phonetic-transcriptions}` `{audio}` `{sentence-audio}` `{card-image}` `{document-title}` `{source-link}` `{clip-timestamp}` `{tags}`. The HTML format of each one must match Fushi `anki_note_composer.dart` and `popup.js` (test vectors in T-ANK-02x). |
| ANK-022 | `{furigana-plain}`: a kanji group becomes ` 漢字[かんじ]`, a kana group stays the same; the result is trimmed (e.g. `食[た]べ 物[もの]`). |
| ANK-023 | `{glossary}`: `<div style="text-align: left;" class="yomitan-glossary"><ol>`, then one `<li data-dictionary="D"><i>(tag, D)</i> <span>…</span></li>` per sense. The `(tag, D)` label appears only on the first sense of each dictionary, repeated part-of-speech tags are removed, disabled dictionaries and redirects are skipped. The dictionary CSS is added as a `<style>` scoped to `.yomitan-glossary [data-dictionary="D"]`. |
| ANK-024 | `{sentence}`: the cue (or the merged cues, ANK-041) with the looked-up word wrapped in `<b>`, at the exact position of the click offset (or at the first occurrence, if the offset is not valid). |
| ANK-025 | `{frequency-harmonic-rank}` as in LKP-040. `{frequencies}`: `<ul style="text-align: left;"><li>D: value</li>…</ul>`. |
| ANK-026 | Empty fields: omitted on creation. The first field of the note type cannot be empty (dedicated error). Only fields that actually exist in the note type are sent. |
| ANK-027 | Tags: user tags + `nikomiru` + `video` + cleaned-up series title (spaces → `_`). Deduplicated, in order. |

#### 6.9.4 Mining flow from video

| ID | Requirement |
|---|---|
| ANK-030 | State of the button in the popup: `+` (can be added), `✓` (already exists: the click opens a menu with overwrite / open in Anki / add anyway), `✓↩` (last card of this session, can be overwritten). The check uses `canAddNotesWithErrorDetail` on the first field, with the same `duplicateScope` and `duplicateScopeOptions` as `addNote`; if AnkiConnect is old, it falls back to `findNotes`. After a transport error there is a 30 s pause for the whole process. |
| ANK-031 | The reference cue is chosen in this order: the one the lookup started from, the current one, the one found by position. |
| ANK-032 | Audio interval: `[start − pad_head, end + pad_tail]` (defaults 120 ms and 200 ms, maximum 1000 ms each), bounded by the adjacent cues, then shifted by `+delay_ms`. Maximum duration 30 s. |
| ANK-033 | Sentence audio: `ffmpeg -y -ss S -t D -i file:IN -vn -sn -map_chapters -1 -map 0:<ff-index of the active audio track> -c:a aac -ac 1 -b:a 64k out.m4a`. Quality levels: mono 64k (default), stereo 128k, stereo 192k. Alternative formats: Opus/OGG, MP3. |
| ANK-034 | Image: the exact frame at the moment of the click, captured by mpv (`screenshot-to-file <tmp>.png video`), then resized and encoded with ffmpeg. Levels: long side 720 px / q80, **1000 px / q90 (default)**, 2000 px / q95, original. Formats: JPEG (default), WebP. Alternative mode: frame at the start of the cue. |
| ANK-035 | Audio and image are extracted in parallel. If the audio is missing, the card is cancelled with a message (as in Fushi). |
| ANK-036 | Media in Anki: content-based name `nikomiru_<sha256[0..16]>.<ext>`. If the host is loopback, `storeMediaFile` is used with `path`; otherwise `data` in base64. If `addNote` fails, the media uploaded in that attempt are deleted. |
| ANK-037 | Overwriting the last card: `notesInfo`, rejection if the note type is different, then `updateNoteFields` with **all** fields (unmapped ones become `''`) and `addTags`. Default scope: only the last card mined in the session. |
| ANK-038 | "Open in Anki": `guiBrowse` with `nid:<id>`; for a word, `findNotes` on the first field and then `guiBrowse` with the ids found. |
| ANK-039 | Every created card is recorded in `mined_cards` (note id, expression, reading, sentence, video uid, start/end, `source_id`, date) for statistics and to highlight "already mined" in the future. |
| ANK-040 | "Advanced mining" dialog (Shift+click on +): editing of start/end with preview, frame choice, exclusion of audio or image. Milestone M6. |
| ANK-041 | Adding the previous or next cues to the sentence (merging text and interval). Milestone M6. |
| ANK-042 | WebM video clips and animated covers (AVIF/WebP/GIF): post-1.0. |

#### 6.9.5 Return to the scene

| ID | Requirement |
|---|---|
| ANK-050 | `{source-link}` = `<a href="nikomiru://source?v=1&kind=video&uid=…&ep=…&startMs=…&endMs=…&sourceId=<uuid-v4>">Title</a>` (title HTML-escaped). |
| ANK-051 | Parsing is strict: unknown or duplicate parameters, absolute paths and `file:` uids are rejected; maximum length 16 KiB. |
| ANK-052 | Opening the link (APP-001/002) opens the video, goes to `startMs` paused and highlights the cue. If the file cannot be found (unknown uid or moved file), a message is shown with the option "locate the file". |
| ANK-053 | `{clip-timestamp}` = `HH:MM:SS - HH:MM:SS`, only if `end > start`. |

### 6.10 Statistics (STA)

| ID | Requirement | Fushi |
|---|---|---|
| STA-001 | Watch time counts each interval of the video **only once** (interval coverage). Sampling every 1 s; a jump larger than `real_time × speed × 1.25 + 500 ms` is a seek and does not count. Coverage is saved per video: the total never exceeds the duration (BUG-2108). | `video_watch_tracker.dart`, `interval_coverage.dart` |
| STA-002 | Subtitle characters are counted once per cue, after at least `min(1500 ms, cue duration)` of actual playback in that cue. Cues already counted in previous sessions are not counted again (BUG-1763). | |
| STA-003 | Character counting rule: Han, kana, Hangul, Thai and similar scripts = 1 per code point; a sequence of Latin letters or digits = 1; combining marks and apostrophes = 0. | `study_char_count.dart` |
| STA-004 | Sessions (`study_segments`): clock with 60 s ticks, a pause > 120 s closes the session, segments split at every hour. For each segment: media, start, end, date, duration, characters, lookups, cards. | `study_clock.dart` |
| STA-005 | UI: calendar with heat map, chart by range (7/30/365 days), total per series, list of sessions with deletion. CSV export. | `video_statistics_page.dart` |

### 6.11 Settings, appearance, languages (SET)

| ID | Requirement |
|---|---|
| SET-001 | Settings page with sections: General, Player, Subtitles, Dictionaries, Lookup, Word audio, Anki, Shortcuts, Advanced (mpv options, ffmpeg paths, logs), About (version, licenses, `THIRD_PARTY_LICENSES`). |
| SET-002 | Changes apply immediately, without restarting, except where noted (e.g. `hwdec` takes effect from the next file). |
| SET-003 | Light / dark / system theme (the system theme follows the desktop's `color-scheme`, read from the XDG settings portal), with a configurable accent color. Design tokens in a single Rust `Theme`, applied to egui's visuals. |
| SET-004 | i18n with Fluent (`assets/i18n/<locale>/*.ftl`): the interface takes every string from the Fluent bundle. No user-visible string literal in the code. UI language: system (`LANG`) with fallback to English; it can be changed from the settings. |
| SET-005 | UI scale 75–200% (egui's zoom factor, on top of the display's scale), independent of the subtitle size. |
| SET-006 | Reset to default settings per section. |
| SET-007 | Configurable HTTP(S) proxy; used for all network requests except AnkiConnect on loopback. |

### 6.12 Online subtitles and metadata (MET) — milestone M7

| ID | Requirement | Fushi |
|---|---|---|
| MET-001 | Metadata from **AniList** (GraphQL, no key): romaji/native/English title, cover, episodes, `anilist_id`. Identification from the series title obtained by LIB-003; it is accepted automatically only with an exact, unique match, otherwise the user is asked to choose. | — (Fushi uses Jikan/TMDB) |
| MET-002 | **Jikan v4** (MAL) as an alternative source: limit of 1 request/s, cache, pause after a 429. | `mal_video_metadata_provider.dart` |
| MET-003 | **Jimaku**: `https://jimaku.cc/api`, header `Authorization: <key>`. `GET /entries/search?anilist_id=…` (with fallback to `query=`), `GET /entries/{id}/files?episode=n`, download, zip archives extracted with the rules of DIC-011. Episode and language are inferred from the file name. | `jimaku_client.dart` |
| MET-004 | **OpenSubtitles** (user key): `moviehash` + `moviebytesize`, then `query` + `season_number`/`episode_number`; files ≤ 64 MiB. | `open_subtitles_client.dart` |
| MET-005 | API keys are saved in the config with permissions 0600 (the keyring is in the ROADMAP backlog). | |
| MET-006 | File identification with AniDB ed2k: post-1.0 (requires registering an AniDB client). | `anidb_ed2k.dart` |

---

## 7. Non-functional requirements (NFR)

**Reference hardware** (REF): x86_64, 4 cores / 8 threads (2020 or later), integrated Intel or AMD GPU with VA-API, 16 GB of RAM, NVMe SSD, 1920×1080 display at 60 Hz, Wayland session (GNOME or KDE).

| ID | Requirement | Measure (see TEST) |
|---|---|---|
| NFR-PERF-01 | Cold start until the library is visible ≤ 1.5 s on REF | T-PERF-01 |
| NFR-PERF-02 | Opening a local file until the first frame ≤ 1.0 s (h264/hevc, 1 GB mkv file) | T-PERF-02 |
| NFR-PERF-03 | 1080p60 H.264 and 10-bit HEVC playback with the overlay visible (subtitles + controls): **0 dropped frames** in 5 minutes (`frame-drop-count`, `vo-delayed-frame-count`), process CPU ≤ 25% of one core with hwdec active | T-PERF-03 (exit criterion of spike S1) |
| NFR-PERF-04 | Engine lookup (scan of 16 code points, 5 term dictionaries + 3 frequency + 1 pitch accent) p95 ≤ 10 ms, p99 ≤ 25 ms | T-PERF-04 |
| NFR-PERF-05 | Import of a dictionary with 300k entries / 150 MB JSON ≤ 60 s, peak RAM ≤ 1 GiB | T-PERF-05 |
| NFR-PERF-06 | From the click on the character to the drawn popup ≤ 100 ms p95 | T-PERF-06 |
| NFR-PERF-07 | A cue appears within ≤ 50 ms of its actual `start` | T-PERF-07 |
| NFR-PERF-08 | Card created (5 s audio + image) ≤ 3 s from the click, with a local Anki | T-PERF-08 |
| NFR-PERF-09 | Idle RAM (RSS) with the library open ≤ 400 MB | T-PERF-09 |
| NFR-REL-01 | No crash and no data loss if the app is closed during: import, config write, progress save, mining | T-REL-001, T-REL-002 |
| NFR-REL-02 | A corrupted or malicious video file, subtitle or dictionary does not crash the app (parser fuzzing) | T-FUZZ-* |
| NFR-SEC-01 | Dictionary contents and subtitle text are untrusted data: the interface lays them out as literal text from a typed view model (allowlisted blocks and runs, POP-009); no markup is ever parsed from them for display, never raw HTML or scripts; the interface never loads a URL from them (images are decoded from the dictionary's own media, POP-010; external links open only `http`/`https` in the browser, POP-011) | T-SEC-* |
| NFR-SEC-02 | No file written outside Nikomiru's XDG directories and those chosen by the user (screenshots, exports) | T-SEC-03 |
| NFR-A11Y-01 | The whole UI is usable from the keyboard, with focus always visible; AA contrast in the default themes; AccessKit roles and names on the controls (AT-SPI on Linux) | T-A11Y-* |
| NFR-PORT-01 | Works on Wayland and X11; the build needs no network apart from `cargo fetch` | T-PORT-001, T-PORT-002 |
| NFR-MAINT-01 | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean; `///` documentation on every `pub` item of the library crates | T-MAINT-001 |
| NFR-MAINT-02 | Line coverage ≥ 80% for `nikomiru-text`, `nikomiru-subs`, `nikomiru-dict`, `nikomiru-anki` (measured with `cargo llvm-cov`, MIT/Apache license) | T-MAINT-002 |

---

## 8. Data model (library.redb)

| Table | Key | Value (main fields) |
|---|---|---|
| `meta` | `&str` | `schema_version`, `created_with` |
| `series` | `u64` | title, native title, `anilist_id?`, `mal_id?`, cover, grouping mode, added on, deleted (tombstone, for LIB-004), preferences (audio, subtitle, `delay_ms`) |
| `videos` | `&str` uid | `series_id?`, path, size, mtime, season, episode, title, duration, `last_position_ms`, `last_played_at`, `completed_at`, chosen audio, main and secondary `SubtitleChoice`, `delay_ms?`, `secondary_delay_ms?`, `speed?`, `volume?` |
| `watch_coverage` | `&str` uid | merged `[(start_ms, end_ms)]` intervals |
| `cue_credit` | `&str` uid | bitset of the indices of cues already counted + hash of the subtitle file |
| `study_segments` | `u128` uuid | media uid, start, end, `date_key`, hour, duration, characters, lookups, cards |
| `mined_cards` | `u64` note id | expression, reading, sentence, media uid, start, end, `source_id`, date |
| `dictionaries` | `&str` id | title, revision, type, order, enabled, language, `display_name`, collapse state, user CSS, counts |
| `subtitle_cache` | key from SUB-007 | path of the extracted file, format, encoding |

`SubtitleChoice = File(path) | Embedded { ff_index, lang, title } | Online { provider, id, path } | Off`

---

## 9. Configuration (excerpt of `config.toml`)

```toml
[general]
ui_language = "system"      # "it", "en", …
theme = "system"            # "light" | "dark" | "system"
study_language = "ja"

[player]
hwdec = "auto-safe"
quality_profile = "default" # "default" | "high-quality"
seek_seconds = 3
auto_next = true
raw_mpv_options = ""
mpv_preset = ""             # last loaded or saved preset, mpv/<name>.conf (PLY-016)

[player.mpv_options]        # options set from the UI or loaded from a preset (PLY-016, PLY-018)
# deband = "yes"
# scale = "ewa_lanczos"

[subtitles]
preferred_languages = ["ja", "jpn"]
font_size = 36
respect_ass_styles = false
obscure = "none"            # "none" | "blur" | "hidden"

[lookup]
max_results = 10
hover_without_modifier = false
resume_on_leave = true

[popup]                     # the largest dictionary popup, in points (POP-006)
max_width = 560             # 240–4096; never wider than the window
max_height = 500            # 120–4096; a popup is as tall as its content, up to this

[anki]
url = "http://127.0.0.1:8765"
api_key = ""
deck = "Lapis"
note_type = "Lapis"
duplicate_scope = "deck"    # "deck" | "collection"
audio_pad_head_ms = 120
audio_pad_tail_ms = 200
audio_format = "aac-m4a"    # "aac-m4a" | "opus-ogg" | "mp3"
image_format = "jpeg"       # "jpeg" | "webp"
image_quality_level = 1     # 0..3
tags = []

[anki.field_mapping.Lapis]
Expression = "{expression}"
Sentence = "{sentence}"
# …

[paths]
ffmpeg = ""                 # empty = search in PATH
```

---

## 10. Decisions and risks

### 10.1 Maintainer decisions

Decisions D-1…D-12 approved by the maintainer on 2026-10-19 (D-7…D-9 after the M0 spikes, D-10 after spike S1q), D-13 on 2026-10-20 after spike S5. A decision is reopened only through an explicit change to this table.

| ID | Decision | Outcome |
|---|---|---|
| D-1 | Class B exceptions (§3.2), needed to use Dioxus desktop | **Approved**: closed list; CI blocks any addition. Superseded by D-10: the exceptions forced by Dioxus are gone (§3.2) |
| D-2 | Reading the local audio DB (SQLite, public domain) | **Approved** option (a): `turso` (MIT, a Rust rewrite of SQLite), to be validated in spike S3. If S3 fails, the decision is reopened between (b) an exception for the system SQLite library and (c) dropping AUD-003 |
| D-3 | GPL-3.0-only or GPL-3.0-or-later | **Approved**: GPL-3.0-only (§3.1) |
| D-4 | Default audio format for cards | **Approved**: AAC in M4A, compatible with Anki desktop, AnkiDroid and AnkiMobile |
| D-5 | Packages for 1.0 | **Approved**: PKGBUILD (AUR) + Flatpak (**freedesktop runtime**, with libmpv and ffmpeg included in the manifest; runtime updated by D-10, then by D-13: the KDE runtime and Qt are no longer needed) |
| D-6 | Name of the app, the folder and the repository | **Approved**, then superseded by D-11: the app was called **Anikoto**, a name that turned out to be taken |
| D-7 | The `cargo deny` check on the real graph (M0, 2026-10-19) finds 5 class B crates missing from the §3.2 table: `yoke-derive`, `zerofrom-derive`, `zerovec-derive` (Unicode-3.0) and `const_format_proc_macros`, `konst_macro_rules` (Zlib). They are procedural macros of the already approved families (`yoke`, `zerofrom`, `zerovec`, `const_format`, `konst`), with the same license and the same requester. `foldhash` and `zlib-rs`, on the other hand, do not appear in the Linux graph | **Approved**: the 5 crates are in the §3.2 table and in `deny.toml`; `foldhash` and `zlib-rs` stay, since they may reappear with other versions or platforms. Superseded by D-10: only the Unicode-3.0 macro crates remain |
| D-8 | The `router` feature of `dioxus` (§4) brings in `base16` 0.2.1, which is **CC0-1.0** (forbidden, §3.2), through `dioxus-router-macro` | **Obsolete (D-10)**: no Dioxus, no `base16`. CC0-1.0 is forbidden with no exception (§3.2) |
| D-9 | Language of documents and code comments | **Approved**: documents and code comments are in English (§0). For consistency, commit messages and developer-tool messages (`cargo xtask`) are in English too. The UI stays localized (SET-004) |
| D-10 | UI toolkit | **Approved**: Qt 6 Quick (QML) through cxx-qt instead of Dioxus ([ADR 0003](adr/0003-qt-quick-ui.md)). On the developer's 4K, 120 Hz, fractional-scale session no frame was dropped, where the Dioxus design dropped 2456 in 60 s; the class B table shrinks to the ICU/Unicode family and `foldhash`. Superseded by D-13: the UI is egui; ADR 0003's findings on mpv (render context before loading, native display, shutdown order) still apply |
| D-11 | Name of the project | **Approved**: the app, the crates (`nikomiru-*`), the binary, the URL scheme (`nikomiru://`), the XDG folders and the GitHub repository (`nikomiru`, created at the first push) are called **Nikomiru**, because "Anikoto" was already taken. Git history before the rename keeps the old name |
| D-12 | Direction after 1.0 | **Approved** (2026-10-19): books (EPUB), manga and online resources are a planned direction after 1.0, not a permanent exclusion. Until 1.0 the focus is video with clickable subtitles and the dictionary explanation of the clicked word, in the overlay and in the subtitle list (SUB-009, SUB-015, M3) |
| D-13 | UI toolkit, after spike S5 (iced and egui against Qt Quick) | **Approved** (2026-10-20): egui 0.37 on winit + glutin (EGL first) + egui_glow instead of Qt 6 Quick ([ADR 0006](adr/0006-toolkit-comparison.md)). Under heavy load at 4K HEVC 10-bit egui drops 0–2 frames in 20 s like Qt, with about half of its CPU; the popup draws a whole entry in one frame; everything is Rust (no QML, C++ or cxx-qt). Class B exception `libloading` (ISC) approved with it (§3.2). The Flatpak of D-5 no longer needs the KDE runtime: the freedesktop one has what egui needs |

### 10.2 Risks

| ID | Risk | Prob. | Impact | Mitigation |
|---|---|---|---|---|
| R-1 | Video and UI composition in the same window (§5.2) does not render smoothly | Low (D-13) | High | egui composition measured in [ADR 0006](adr/0006-toolkit-comparison.md): at 4K HEVC 10-bit under load 60 renders/s and 0–2 dropped frames, like Qt Quick in [ADR 0003](adr/0003-qt-quick-ui.md) (the Dioxus design of [ADR 0001](adr/0001-video-composition.md) dropped frames at 4K). Residual: video and interface share the UI thread, so long work on it delays frames (§5.2.1); OpenGL is required (§5.2.3); the egui build still has to be measured on the developer's session and on REF |
| R-2 | egui and winit are still 0.x: API changes between versions | Medium | Medium | Pinned versions; the toolkit only in `nikomiru-ui`/`nikomiru-app` (ARC-010); upgrades only through a dedicated PR |
| R-3 | A new version of egui, winit, glutin or reqwest adds dependencies with new licenses | Medium | Medium | `cargo deny` in CI; updates only through a dedicated PR |
| R-4 | System ffmpeg without encoders (e.g. `aac` missing in reduced builds) | Low | Medium | EXT-001: check at startup, fallback to Opus/MP3, clear message |
| R-5 | redb performance not sufficient for lookup | Low | Medium | Spike S2; `DictStore` trait and a custom format as plan B |
| R-6 | AnkiConnect changes or removes `canAddNotesWithErrorDetail` | Low | Low | Fallback to `findNotes` (ANK-030) |
| R-7 | The size of Fushi pushes toward widening the scope | High | High | §1.3 is binding; every new feature goes through a change to this document |
| R-8 | ~~Distributions ship Qt older than 6.5~~ | — | — | Obsolete (D-13): no Qt |
| R-9 | winit's gaps on Linux: no Wayland drag and drop, no window activation on Wayland, the desktop's colour scheme not reported; no client-side decorations without a further class B exception (GNOME shows no title bar) | Medium | Medium | Own Wayland data device (§4); XDG portals through the own D-Bus client; revisit with each winit release |

---

## 11. Glossary

| Term | Meaning |
|---|---|
| Cue | A single subtitle line, with a start and an end |
| Lookup | Dictionary search starting from a point in the text |
| Deinflection | Bringing an inflected form (食べました) back to the dictionary form (食べる) |
| Mining | Creating an Anki card from a word encountered, with its context |
| Lapis | Very common Anki note type for Japanese (donkuri/lapis) |
| AnkiConnect | Anki add-on (code 2055492159) that exposes a local HTTP API |
| Structured content | Yomitan JSON format for rich glossaries (tables, ruby, images) |
| hwdec | mpv hardware video decoding |
| Spike | Time-boxed prototype to reduce a technical risk; the code is thrown away |
