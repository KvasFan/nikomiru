# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed (rename)

- **The project is now called Nikomiru** (D-11): "Anikoto" was already taken. Crates `nikomiru-*`, binary `nikomiru`, URL scheme `nikomiru://`, XDG folders `nikomiru`, environment variables `NIKOMIRU_*`, GitHub repository `nikomiru`.

### Added

- Specification (`docs/SPECIFICATION.md`), roadmap (`docs/ROADMAP.md`) and test plan (`docs/TEST.md`) for the Rust rewrite of Fushi.
- GPL-3.0-only license.
- Cargo workspace with the 11 crates of SPECIFICATION §5.1 (still empty), Rust 1.99.1 pinned in `rust-toolchain.toml`, shared lints (`unwrap_used` and `expect_used` forbidden, ERR-002).
- License policy enforced: `deny.toml` (Class A, Class B exceptions of SPECIFICATION §3.2, ban on the alternatives rejected in §3.6), `about.toml` and the `THIRD_PARTY_LICENSES.html` template.
- `cargo xtask` with the commands `ci [--full]`, `fixtures [--large] [--perf]`, `check-headers`, `check-licenses [--canary]`, `trace` and `about`.
- Spikes S1 (video composition) and S2 (dictionary storage) in `spikes/`, with the measurements and the decisions in `docs/adr/`.
- Spike S1q (Qt 6 Quick + cxx-qt + mpv render on the scene graph thread) and ADR 0003: Qt Quick as the UI toolkit instead of Dioxus.
- Spike S5 (the video overlay and the dictionary popup with iced and with egui, measured against S1q's Qt Quick) and ADR 0006.
- T-SEC-001 tests the hostile entry of `mini-ja` for real: looked up through the real import, lookup and popup content builder, its view model walked against the allowlist, drawn with `egui_kittest`, and opened in the real app, where a spy proxy sees no request. The GUI test harness has `lookup {text}` and `popup_content {level}` for it.
- Conformance with every fushidicts test (TEST §5.3): `tests/conformance/import.json` and its runner (45 import cases: wrapper folders, Unicode Path fields, ZIP64 sentinels, broken deflate streams, lying sizes, media, styles, damaged dictionary files), 14 more lookup cases (kanji stats, glossary round trip, pitch patterns, score and frequency order, non-ASCII titles, malformed kanji rows) and an import that reaches a file size limit halfway. Five import cases are known failures of the importer, listed in TEST §5.3.
- The dictionaries page lists the dictionaries by type (terms, kanji, frequencies, pitch accent) with their language (DIC-030): a dictionary with several types has its row under the first and a reference under the others, where it ranks too; ▲/▼ move it within its type, and so does dragging it by its handle onto the drop marker (DIC-031). A term dictionary has an editor for the user's CSS (DIC-035), applied after its `styles.css` through the same safe subset, which says what the subset keeps: the rules applied, the properties left out, the rules ignored.
- `[popup]` in `config.toml` (POP-006): the largest dictionary popup, 560 × 500 points by default; it was 560 points wide and up to 70% of the window high.
- A dictionary popup is resized by dragging its border (POP-006): the edge away from the word, the side edges and the corners between them. The popup keeps its side of the word, and the next popups take the new size until the app closes.
- Text selected in a glossary opens a nested popup (POP-012): a drag across a paragraph, or across a line of ruby, chips and links (the readings left out), looks the selection up in a child popup next to it, and the selection stays lit while that popup is open. Links still open with a click.

### Changed

- **D-13: the UI toolkit is egui instead of Qt 6 Quick** (ADR 0006, accepted after spike S5): under heavy load at 4K HEVC 10-bit egui keeps the video like Qt (0–2 dropped frames in 20 s) with about half of its CPU, draws a whole popup entry in one frame, and is all Rust; iced was rejected (no zero-copy video, dropped frames under load, a CC0 crate, no accessibility). Class B exception `libloading` (ISC) in SPECIFICATION §3.2, `deny.toml` and `about.toml`. The port is ROADMAP M3 work track 5; ADR 0003 and the QML part of ADR 0005 are superseded.
- **The interface runs on egui** (ROADMAP M3, work track 5): winit, glutin with EGL first, egui-winit and egui_glow; mpv draws the video with the libmpv OpenGL render API into the window under the interface, and the player's commands run on a thread of their own so that the UI thread never waits for mpv. Qt, QML, cxx-qt and the C++ video item are gone. Everything the Qt interface did is there: the control bar (it fades in and out), the subtitle overlay (outline, shadow and blur as copies of the text), the subtitle list (only the rows in view are laid out), the dictionary popups and the dictionaries page; the L6 suite is the same, plus a real drag from another Wayland client.
- The dictionary popup is built from a typed view model on the lookup thread: no generated StyledText any more, colours as RGBA (the whole CSS name table, `hsl()`), list markers in their own system (roman, alpha, CJK), relative font sizes multiplied down the tree, images decoded by their bytes with decoder limits and scaled down. Tables have row and column spans, boxes their partial borders, sub- and superscripts are raised and lowered, a collapsed image opens on click, the keyboard cursor scrolls to entry 0 too, a press anywhere in a parent popup closes its children, and the parent keeps its state while children open.
- The subtitle list picks the character under the pointer (Qt picked the nearest cursor position, so the right half of a character selected the next one).
- Open, the subtitle file and the dictionary import use the desktop's file dialog through the XDG portal (`org.freedesktop.portal.FileChooser`), with a message when there is none; web links in dictionaries open through the `OpenURI` portal, or `xdg-open` without one.
- Drag and drop works on Wayland (winit has none there): the app's own data device takes `text/uri-list` drops, with the copy action only.
- Decisions D-1…D-6 approved and recorded in `docs/SPECIFICATION.md` §10.1.
- SPECIFICATION §5.1: ARC-010 and ARC-011 written as definitions, so that `cargo xtask trace` reads them.
- TEST: test marker rule (§8), L7 test names (`l7_…`), off-screen compositor for L6 (headless Weston or virtual KWin).
- SPECIFICATION §5.2: composition strategy (a) adopted, with the constraints found by spike S1 (`with_disable_dma_buf_on_wayland(false)`, `LC_NUMERIC=C`, native display for hwdec, mpv shutdown order); risk R-1 updated.
- SPECIFICATION §10.1: decisions D-7 (5 Class B procedural macros found in the real graph), D-8 (CC0 exception for `base16`, needed by the Dioxus `router` feature) and D-9 (English for documents, comments and commit messages) approved; §3.2 table and `deny.toml` updated.
- Documents and code comments translated to English (D-9); `docs/SPECIFICHE.md` renamed to `docs/SPECIFICATION.md`.
- ROADMAP: the X11 T-PERF-03 measurement moved from M0 to M1; M4 (release 0.1.0) now includes restyling the repository page after storytold/cadcraft.
- **D-10: the UI toolkit is Qt 6 Quick (QML) through cxx-qt instead of Dioxus** (ADR 0003, accepted after the developer's 4K/120 Hz session showed no dropped frames). SPECIFICATION v0.5, TEST and ROADMAP revised: architecture (§5), class B table reduced to the ICU/Unicode family and `foldhash`, XDG paths implemented in-house (no `directories`, no MPL-2.0 exception), popup and styling as QML items (spike S4 before M3), Flatpak with the KDE runtime. ADR 0001 superseded.
- M0 done: all exit criteria met (the X11 measurement on a real Xorg session moved to M1). SPECIFICATION §5.2.2: CUDA interop only with NVIDIA's proprietary driver (no more `Cannot load libcuda.so.1`), libmpv messages routed to the log.
- `cargo xtask`: dependency rules for cxx-qt/libmpv2 and a guard against the retired toolkit, `unsafe` allowed in cxx-qt bridge modules, C++ only in `nikomiru-player/cpp/`, SPDX headers also on `.cpp`, `.h` and `.qml` files.
- SPECIFICATION: new requirements PLY-016 (save and load mpv option presets in `mpv.conf` format, M6), PLY-017 (options that raw options and presets can never set, M1) and PLY-018 (render and audio settings in the UI, M6), with tests T-PLY-016…018.

### Fixed

- A dictionary zip whose wrapper folder holds a second one (a wrapped dictionary extracted and zipped again) imports: wrapper folders are stripped until `index.json` is at the root, as in Fushi (BUG-2053).
- The dictionary registry is read and written in one transaction: an edit on the dictionaries page and the registration of an import, on its own thread, no longer overwrite each other (a deleted dictionary could come back). The registration checks the registry again: the same dictionary imported twice at once is registered once, and an update whose old dictionary was deleted meanwhile is added instead of failing.
- An update whose old folder cannot be removed is no longer reported as an error with the old dictionary still in the popups: it is registered, the lookup cache is dropped, and the next start removes the folder.
- A dictionary import stops promptly when cancelled, at every step: large banks are read, parsed and written a stretch at a time, and the final step (indexes, commit, compaction) looks at the cancel too, where it used to ignore it and register the dictionary anyway. With a 300k-entry dictionary in one bank (release build), a cancel used to wait up to 1.1 s inside the bank and was ignored in the final step; it now stops the import within 0.1 s, or 0.45 s when it comes during redb's commit.
- Quitting during a dictionary import cancels it and waits for its thread: the library is closed at exit, where the import's thread used to keep it open and the next start had to repair it.
- The Yomitan import keeps a kanji bank with malformed rows (a number where a string belongs, meanings that are not a list): those rows are skipped, as in Fushi (BUG-2952). An `index.json` that is not valid UTF-8 is read with U+FFFD instead of failing, and a reading-scoped frequency written `{reading, value}` keeps its reading (DIC-003).
