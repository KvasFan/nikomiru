# Nikomiru — Test plan

| Field | Value |
|---|---|
| Status | Draft v0.1 |
| Date | 2026-10-19 |
| Related documents | [SPECIFICATION.md](SPECIFICATION.md) · [ROADMAP.md](ROADMAP.md) |

## 1. Principles

1. **Every requirement has at least one test.** The test carries the requirement ID (`T-SUB-012` verifies `SUB-012`). Coverage is checked by `cargo xtask trace` (§8).
2. **Pure logic without a display.** Text, subtitles, dictionaries, Anki and library are tested without the UI toolkit (egui, winit, OpenGL) or mpv (SPECIFICATION ARC-010). Most of the tests are concentrated here.
3. **Deterministic and offline.** The default suite does not use the network, does not depend on the real clock (injectable clock in `nikomiru-core`) and does not depend on files outside the repository. Tests with network, real data or real Anki are separate and optional (§2, levels L5–L7).
4. **Fushi is the oracle.** Where a Fushi test exists, its input and output vectors are ported as conformance tests (§5). Every Fushi bug cited in SPECIFICATION becomes a regression test.
5. **Every Nikomiru bug produces a test** before the fix (§7).
6. **Test tools follow the license policy** (SPECIFICATION §3.5), even though they are not distributed.

---

## 2. Levels

| Level | What | Where | When it runs |
|---|---|---|---|
| L1 Unit | Functions and types, specific cases | `#[cfg(test)]` in the crate | Every push |
| L2 Property | Invariants with generated inputs (`proptest`) | `#[cfg(test)]`, `tests/` | Every push (256 cases); nightly 100k cases |
| L3 Snapshot | Structured outputs compared with approved files (`insta`): the popup's view model, Anki field HTML, ffmpeg argv, the AccessKit trees of the views (roles and names, through `egui_kittest`) | `tests/snapshots/` | Every push |
| L4 Integration | Crates against real local dependencies: ffmpeg, filesystem, redb, fake AnkiConnect (`wiremock`) | `crates/*/tests/` | Every PR |
| L5 mpv integration | Headless libmpv (`vo=null`, `ao=null`) on the generated fixtures | `crates/nikomiru-player/tests/` | Every PR |
| L6 GUI end-to-end | Real app driven by the **test harness** (§2.1), under Xvfb (X11) and headless Weston or virtual KWin (Wayland), with Mesa llvmpipe | `tests/e2e/` | Every PR (smoke); nightly (full) |
| L7 External | Real Anki desktop with AnkiConnect; real online services (AniList, Jimaku); real user data (JMdict, …) | `#[ignore]`, name starting with `l7_`, + `cargo xtask test --anki-e2e / --live / --realdata` | Nightly or by hand |
| L8 Performance | `criterion` benchmarks and harness metrics, with thresholds (§6.15) | `benches/`, `tests/e2e/perf_*` | Nightly |
| L9 Manual | Exploratory milestone and release checklists (§9) | this document | End of milestone, release |

### 2.1 GUI test harness

- Cargo feature `test-harness` of `nikomiru-app`, **never enabled in release builds** (verified by T-SEC-004). It enables `nikomiru-ui/test-harness`.
- Start: `nikomiru --test-harness <socket>`. The test listens on the socket and the application connects to it. Protocol: JSON lines on a Unix socket; each command `{"cmd": …}` gets one answer, `{"ok":true,"result":…}` or `{"ok":false,"error":"…"}`.
- The L6 tests live in `tests/e2e/`, a test target of `nikomiru-app` with `required-features = ["test-harness"]`, so `CARGO_BIN_EXE_nikomiru` is the harness build. Each test starts its own `kwin_wayland --virtual` on a private D-Bus bus without service activation, with isolated XDG folders and a private `XDG_RUNTIME_DIR`: no real portal, audio server or user file is reached, and mpv's audio output is `null` (without an audio server, probing PipeWire can hang mpv's core on a quick second load). A desktop kills the app a failed test leaves running. `cargo xtask ci --full` runs them.
- Commands:
  - `open {path}`, `wait {event, timeout_ms}`, `key {code, mods}` (`code` is a W3C key name such as `Space`, `mods` a list of `Ctrl`, `Shift`, `Alt`; the key goes through the window's key handler, as a real one does), `drop {paths}` (paths; the first goes through the window's drop handler);
  - `click_grapheme {layer, cue_index, grapheme_index}` (through the overlay's click command, with the popup's fallback place), `popup_button {replay | goto}`;
  - `lookup {text}`: a root popup for `text`, opened as a click on a subtitle word opens one but with no cue behind it (fallback place); `popup_content {level}`: the view model of that popup as JSON (POP-009), `null` while it looks up;
  - `state {}`: player state, open popups, visible cues, toasts, dictionaries page;
  - `dict_css {id, css}`: a dictionary's CSS saved through the command of its editor's Save button;
  - `mpv_get {property}`: read-only, for metrics; read on the harness thread, never on the UI thread, which renders the video;
  - `quit {}`;
  - planned: `hover_grapheme {…}`, `click {selector}` (through the AccessKit tree).
- Events: `file-loaded`, `first-frame` (the first position after the load), `cue-shown {index, time_pos}`, `popup-rendered {level, query, headwords}` (after the buffer swap of the first frame that draws the popup), `dictionary-imported`, `error {code}` (`code` is the Fluent id of the error, ERR-001); planned: `library-ready` (M5), `card-created {note_id}` (M4). `wait` drops the events before the one it waits for and fails on an `error`.
- The harness runs its commands on the UI thread through the same entry points as the user's input (the key and drop handlers, the view's commands, the close path), not through internal shortcuts that skip the logic under test.

---

## 3. Tools

| Tool | License | Use |
|---|---|---|
| `cargo test`, `cargo-nextest` | MIT/Apache · Apache-2.0 OR MIT | Running the tests |
| `proptest` | MIT OR Apache-2.0 | L2 and structured fuzzing |
| `insta`, `cargo-insta` | Apache-2.0 | L3 |
| `egui_kittest` | MIT OR Apache-2.0 | Tests of the egui views through AccessKit, with no renderer |
| `wiremock` | MIT/Apache-2.0 | Fake AnkiConnect, fake online services |
| `tempfile` | MIT OR Apache-2.0 | Isolated XDG directories for each test |
| `criterion` | Apache-2.0 OR MIT | L8 |
| `cargo-llvm-cov` | Apache-2.0 OR MIT | Coverage (NFR-MAINT-02) |
| `cargo-deny`, `cargo-about` | MIT OR Apache-2.0 | Licenses (T-LIC-*) |
| Xvfb, Weston, Mesa llvmpipe | MIT/X11 | Headless display for L6 |
| KWin (`kwin_wayland --virtual`) | GPL-2.0-or-later | Off-screen Wayland compositor for L6 and for the S1, S1q and S5 measurements (ADR 0001, ADR 0003, ADR 0006); **only as an external program** |
| ffmpeg, ffprobe, libmpv | LGPL/GPL | Generating and checking the fixtures |
| Anki desktop + AnkiConnect | AGPL-3.0 / GPL | **Only as external programs** in L7: they are neither linked nor distributed |

**Excluded**: `cargo-fuzz`/`libfuzzer-sys`, because the license includes NCSA (not on the list). Coverage-guided fuzzing with `afl` (AFL++, Apache-2.0) can be adopted only after a `cargo deny` check. Until then, fuzzing is done with `proptest` on arbitrary bytes and structured inputs (T-FUZZ-*).

---

## 4. Fixtures

All fixtures are **created by the project** (GPL-3.0) or **generated**. No third-party dictionary, video or subtitle enters the repository.

### 4.1 Generated media (`cargo xtask fixtures`)

They are generated with ffmpeg `lavfi` in `target/fixtures/`, with the `+bitexact` flag and without metadata. The tests check properties (durations, tracks, tones), not exact bytes.

| File | Content | Used by |
|---|---|---|
| `av_basic.mkv` | 12 s, 1280×720@30 `testsrc2`, `mpeg4` video (native encoder). **Audio 1** `jpn`: silence with 1 kHz tones at [2.0–2.5], [5.0–5.5], [8.0–8.5] s. **Audio 2** `eng`: 660 Hz tones at the same instants. **Subtitle 0** SRT `jpn` with cues aligned to the tones (`食べました`, `日本語を勉強しています。`, `ラーメンが好き`). **Subtitle 1** ASS `jpn` with styles, `\an8` and `\pos`. 3 chapters | PLY, SUB, ANK, STA |
| `av_noaudio.mp4` | 5 s of video only | ANK-035, PLY-012 |
| `av_vfr.mkv` | Variable frame rate (`setpts`) | PLY-001 (frame step) |
| `av_long.mkv` | 10 min, 320×180, tones every 30 s | STA-001, PLY-007, LIB-006 |
| `av_hd60_h264.mkv`, `av_hd60_hevc10.mkv` | 5 min 1920×1080@60 `testsrc2` (`libx264`, `libx265` 10-bit; only with `cargo xtask fixtures --perf`, nightly, cached) | T-PERF-03 |
| `-weird: name & spaces ü 日本.mkv` | Copy of `av_basic.mkv` | EXT-002, T-SEC-005 |
| `http:fake.mkv` | Copy of `av_basic.mkv` | EXT-002 (`file:` prefix) |
| `corrupt_*.mkv` | `av_basic.mkv` truncated or with random bytes | PLY-012, NFR-REL-02 |

### 4.2 Subtitles (versioned in `tests/fixtures/subs/`)

`basic.srt` · `bom_crlf.srt` · `tags_braces.srt` (`{\an8}`, `<i>`, `<font>`) · `out_of_order.srt` · `negative_duration.srt` · `overlap.srt` · `shift_jis.srt` · `euc_jp.srt` · `utf16le_bom.srt` · `gb18030.srt` · `cp1252.srt` · `settings.vtt` · `notes_styles.vtt` · `ruby.vtt` · `styles.ass` · `no_playres.ass` · `drawing.ass` (`\p1`) · `layers.ass` · `karaoke.ass` · `malformed_*.{srt,vtt,ass}`.

### 4.3 Dictionaries

- `tests/fixtures/dicts/mini-ja/` (handwritten JSON source; the zip is created by `xtask`):
  - about 80 terms chosen for the cases of §5 and §6.5: verbs of all classes (v1, v5k/k-s/g/s/t/n/b/m/r/u, vk, vs-i), i-adjectives and na-adjectives, compound nouns (食べ物, 日本語), ため, 取り扱い/取扱い, kana variants;
  - `tag_bank`, structured content (table, ruby, 2×2 px image `img/a.png`, `?query=` link), a `formOf` redirect, a term with `<script>`, `on*` attributes and a remote `<img>` (for T-SEC-001).
- `mini-freq` (rank-based) · `mini-freq-occ` (`frequencyMode: occurrence-based`, string values) · `mini-pitch` (pitch with `nasal`/`devoice` + ipa) · `mini-kanji` · `mini-norules` (no rules in any entry).
- Malicious ZIPs, generated: `evil-traversal.zip` (`../x`) · `evil-absolute.zip` (`/etc/x`) · `evil-symlink.zip` · `evil-bomb.zip` (ratio > 1000:1) · `evil-huge-entry.zip` (declared size > 512 MiB) · `evil-badjson.zip` · `wrapped-dir.zip` (legitimate, inside a folder: accepted) · `unicode-path.zip` (extra field 0x7075) · `zip64.zip`.
- Large synthetic dictionary (`xtask fixtures --large`): 300k terms, about 150 MB of JSON, PRNG with a fixed seed. Variants: 5 term + 3 freq + 1 pitch for T-PERF-04, plus `queries.txt` with 10k inflected sentences for the lookup benchmark (ADR 0002).

### 4.4 Anki

`tests/fixtures/anki/*.json`: handwritten AnkiConnect v6 responses for every action of ANK-005, including errors (duplicate, unsupported action, wrong API key).

### 4.5 Real data (optional, L7)

Variable `NIKOMIRU_REALDATA=/path`, with files provided by the user: `jmdict_english.zip`, `jmnedict.zip`, a frequency dictionary, a pitch accent dictionary, `android.db`, a real episode with ASS. If the variable is missing, the `#[ignore = "realdata"]` tests are skipped.

---

## 5. Conformance with Fushi

### 5.1 Vector sources

| Area | Fushi file (commit `ac8034e`) | Nikomiru test |
|---|---|---|
| Word scanning | `native/fushidicts/tests/word_scan_test.cpp`, `fushi/test/lookup/apostrophe_word_scan_bug2056_test.dart`, `reader_cue_gap_word_scan_bug3453_test.dart` | T-LKP-001…004 |
| Preprocessors | `native/fushidicts/tests/text_processor_test.cpp` | T-LKP-010…012 |
| Deinflection and popup | `native/fushidicts/tests/popup_json_deinflection_test.cpp`, `simple_dict_deinflection_test.cpp`, `en_inflection_lookup_test.cpp`, `fushi/test/dictionary/deinflection_grammar_tags_test.dart` | T-LKP-020…024 |
| Mixed orthography, Kansai | `native/fushidicts/tests/ja_mixed_orthography_lookup_test.cpp`, `ja_kansai_ta_boundary_test.cpp` | T-LKP-031, T-LKP-033 |
| Frequencies and pitch accent | `native/fushidicts/tests/freq_pitch_import_query_test.cpp`, `freq_pitch_merge_query_test.cpp`, `lookup_freq_pitch_enrichment_test.cpp`, `ipa_import_query_test.cpp` | T-LKP-035, T-LKP-040…043 |
| Korean (future) | `native/fushidicts/tests/korean_hangul_lookup_test.cpp` | T-LKP-012 |
| Import robustness | `native/fushidicts/tests/import_failure_boundary_test.cpp`, `import_memory_bound_test.cpp`, `hash_truncated_table_guard_test.cpp`, the `zip_*` tests, `media_import_query_test.cpp`, `simple_dict_css_test.cpp` (§5.3) | T-DIC-001, T-DIC-011…015 |
| Kanji, formats, titles | `native/fushidicts/tests/kanji_import_query_test.cpp`, `format_v2_upstream_sync_test.cpp`, `dict_name_uaf_e2e_test.cpp`, `yomitan_redirect_lookup_test.cpp` | T-LKP-032, T-LKP-034, T-LKP-037, T-DIC-003 |
| Sentence extraction | `fushi/test/lookup/sentence_extraction_test.dart` | T-TXT-007 |
| Character count | `fushi/test/stats/study_char_count_test.dart`, `study_char_count_parity_test.dart` | T-STA-003 |
| Interval coverage | `fushi/test/stats/interval_coverage_test.dart` | T-STA-001 |
| Subtitle parsers | `fushi/test/media/audiobook/srt_parser_test.dart`, `vtt_parser_test.dart`, `ass_parser_test.dart`, `subtitle_markup_test.dart` | T-SUB-001…004 |
| File names and resume | `fushi/test/media/video/video_filename_parser_test.dart`, `video_episode_start_policy_test.dart` | T-LIB-003, T-LIB-007 |
| AnkiConnect | `packages/fushi_anki/test/ankiconnect_service_test.dart` | T-ANK-001…005 |
| Return link | `packages/fushi_anki/test/card_source_link_test.dart`, `fushi/test/anki/card_source_link_uid_test.dart` | T-ANK-050, T-ANK-051 |
| Lapis | `fushi/test/anki/lapis_preset_test.dart` | T-ANK-012, T-ANK-013 |

### 5.2 Procedure

1. For each file in the table, the cases (input, configuration, expected output) are extracted into `tests/conformance/<area>.json`. Each case has a `provenance` field: Fushi file, test name, commit.
2. One Rust test per area reads the JSON and runs all the cases. A failing case shows its `provenance` in the message.
3. Cases that depend on **intentionally different** behaviors (SPECIFICATION §6.6.8) have `"divergence": "<requirement ID>"` and an updated expected output.
4. The dictionaries of Fushi's C++ tests are built in code: they are rewritten as Yomitan dictionaries in the `dictionaries` section of `tests/conformance/lookup.json`, which the runner zips and imports like any other dictionary.
5. The import-robustness cases (`tests/conformance/import.json`, run by `crates/nikomiru-dict/tests/conformance_import.rs`) describe their zip entry by entry: raw names, extra fields, ZIP64 sentinels, broken deflate streams, lying sizes. The runner writes the zip field by field, imports it and checks the error, or the title, types, counts, styles, media and terms; a damaged dictionary file must open or fail without a panic or a hang.
6. A case that fails because of a bug in a file that another work track owns gets `"known_failure": {"owner", "note"}`: the runner expects it to fail and fails as soon as it passes, so the mark goes with the fix. The open ones are listed in §5.3.
7. A Fushi case that cannot be expressed (source scans, internal APIs, Fushi's storage files, MDX) stays in the file with `"skip": true` and a note saying why, or where it went.

### 5.3 Coverage of fushidicts

Every test file of `native/fushidicts/tests` at `ac8034e`, and the Dart tests of §5.1 for the M3 areas. The prefixes are the case ids in the conformance files; skipped cases are counted apart.

| Fushi file | Nikomiru test, or why it does not apply |
|---|---|
| `word_scan_test.cpp` | `text.json` `scan-*` (9) |
| `text_processor_test.cpp` | `text.json` `tp-*` (35) |
| `korean_hangul_lookup_test.cpp` | `text.json` `ko-*` (46, 2 skipped: their lookups are the next ones), `lookup.json` `khl-*` (2) |
| `en_apostrophe_lookup_test.cpp` | `lookup.json` `eal-*` (6, 1 skipped: an asset guard) |
| `en_inflection_lookup_test.cpp` | `lookup.json` `eil-*` (43, 6 skipped: Fushi's `term_rules.flag` file) |
| `simple_dict_deinflection_test.cpp` | `lookup.json` `sdd-*` (8) |
| `popup_json_deinflection_test.cpp` | `lookup.json` `pjd-*` (4, 1 skipped: an engine state no lookup produces) |
| `popup_json_reading_merge_test.cpp` | `lookup.json` `prm-*` (2, 2 skipped: the MDX empty-reading merge) |
| `ja_mixed_orthography_lookup_test.cpp` | `lookup.json` `jmo-*` (10) |
| `ja_kansai_ta_boundary_test.cpp` | `lookup.json` `jkb-*` (25) |
| `yomitan_redirect_lookup_test.cpp` | `lookup.json` `yrl-*` (8) |
| `freq_pitch_import_query_test.cpp` | `lookup.json` `fpi-*` (1) |
| `freq_pitch_merge_query_test.cpp` | `lookup.json` `fpm-*` (3, 2 skipped: one dictionary registered in one role only) |
| `lookup_freq_pitch_enrichment_test.cpp` | `lookup.json` `lfe-*` (6) |
| `ipa_import_query_test.cpp` | `lookup.json` `iiq-*` (1) |
| `pitch_duplicate_notation_test.cpp` | `lookup.json` `pdn-*` (2) |
| `kanji_import_query_test.cpp` | `lookup.json` `kiq-*` (4) |
| `format_v2_upstream_sync_test.cpp` | `lookup.json` `fv2-*` (7: kanji stats, glossary round trip, pitch pattern, score, frequency order, the last a divergence (DIC-003); 4 skipped: the completion marker, the separate Bloom file, `primary_reading` and the frequency options) |
| `dict_name_uaf_e2e_test.cpp` | `lookup.json` `dnu-titles` |
| `dict_name_lifetime_test.cpp` | Not self-contained (a dictionary folder on its command line); `dict_name_uaf_e2e_test.cpp` is its successor |
| `import_failure_boundary_test.cpp` | `lookup.json` `ifb-kanji-malformed-*` (2), `import.json` `ifb-invalid-utf8-title`, `import_l4.rs` `an_import_that_cannot_write_fails_and_leaves_nothing` (the write limit, in a child process); the memory mapping, errno and MDX cases do not apply (skipped) |
| `import_memory_bound_test.cpp` | `import.json` `imb-*` (4: members byte for byte, a corrupt member, banks and media; the low-RAM mode does not exist); C is MDict (skipped) |
| `hash_truncated_table_guard_test.cpp` | `import.json` `htg-*` (4): the damaged file is the redb dictionary file |
| `media_import_query_test.cpp` | `import.json` `miq-media` |
| `zip64_central_dir_test.cpp` | `import.json` `z64-*` (2) |
| `zip_high_ratio_bank_test.cpp` | `import.json` `zhr-*` (5; 3 divergences, DIC-011: the 200:1 ratio limit and the 512 MiB cap) |
| `zip_malformed_deflate_test.cpp` | `import.json` `zmd-*` (4) |
| `zip_unicode_path_extra_test.cpp` | `import.json` `zup-*` (8; 4 known failures) |
| `zip_wrapper_directory_test.cpp` | `import.json` `zwd-*` (8) |
| `simple_dict_css_test.cpp` | `import.json` `sdc-styles`: the Yomitan counterpart, `styles.css` kept byte for byte; the `.css` next to an MDX file does not apply |
| `win_utf8_import_test.cpp` | `import.json` `wui-non-ascii-paths`: non-ASCII zip, folder and title (the Windows ANSI path bug itself does not apply) |
| `ffi_guard_coverage_test.cpp`, `decompressor_null_guard_test.cpp` | Do not apply: source scans of Fushi's C ABI and of its libdeflate calls; Nikomiru has no C ABI and inflates with `flate2` |
| `import_breadcrumb_test.cpp` | Does not apply: Fushi's crash breadcrumb file; a crashed import leaves its `.importing` folder, removed at the next start (DIC-013, T-DIC-013) |
| `mdd_*_test.cpp`, `mdx_*_test.cpp` (8 files) | Out of scope: MDict (DIC-004, post-1.0) |
| `fushi/test/lookup/apostrophe_word_scan_bug2056_test.dart` | `text.json` `q2056-*` (62, 2 skipped: source guards) |
| `fushi/test/lookup/reader_cue_gap_word_scan_bug3453_test.dart` | `text.json` `q3453-*`, `sent-3453-*` (7, 1 skipped: the reader's drag selection) |
| `fushi/test/dictionary/deinflection_grammar_tags_test.dart` | `lookup.json` `dgt-*` (5, 7 skipped: Fushi's stored extras, its second result builder, an engine state no lookup produces, source guards) |
| `fushi/test/lookup/sentence_extraction_test.dart` | `text.json` `sent-*` (9, 2 skipped: a negative click offset and a source guard) |

Known failures, owner `crates/nikomiru-dict/src/import.rs`: `zup-bit11`, `zup-unknown-version`, `zup-overrun`, `zup-empty-name` (the importer takes the `zip` crate's Unicode Path handling, which applies the field with bit 11 set or with any version, refuses an archive whose field overruns its block and takes an empty name).

---

## 6. Case catalog

Format: **ID** · what is checked · data and expected result · level. Each section lists the main cases; every requirement of the milestone MUST have at least its own `T-<ID>`.

### 6.1 Architecture and licenses

| ID | Check | Expected | L |
|---|---|---|---|
| T-LIC-001 | `cargo deny check licenses bans sources advisories` | Exit code 0. The Class B exceptions match SPECIFICATION §3.2 **exactly**: `xtask` compares the markdown table with `deny.toml` | L4 |
| T-LIC-002 | Canary: temporary crate that depends on `xxhash-rust` (BSL-1.0) | `cargo deny` fails | nightly |
| T-LIC-003 | `cargo about generate` | `THIRD_PARTY_LICENSES.html` generated, with all the crates in the graph | L4 |
| T-ARC-010 | Crate dependency rules (`cargo metadata`) | No logic crate depends on the UI toolkit (egui, winit, glutin, glow, AccessKit) or `libmpv2`; only `nikomiru-ui` and `nikomiru-app` depend on the toolkit; no retired toolkit (Dioxus, cxx-qt) | L4 |
| T-ARC-011 | `#![forbid(unsafe_code)]` present in the required crates; every `unsafe` has `// SAFETY:`; no `unsafe` outside the allowed modules (`nikomiru-player`, `nikomiru-app::linux` and `nikomiru-app::window`); no C++ in the crates | `xtask` check | L4 |
| T-ARC-020 | `SPDX-License-Identifier: GPL-3.0-only` header in every `.rs` file (and in the `.cpp`, `.h` and `.qml` files of the spikes) | `xtask check-headers` | L4 |
| T-ARC-001 | Player API | After `file-loaded`, at least one event arrives for each observed property of ARC-001 | L5 |
| T-ARC-002 | LoadToken: events from a previous load | They are discarded and do not change the state (fake event source) | L1 |
| T-ARC-003 | No synchronous reads from the UI thread | `PlayerHandle` exposes no synchronous getters (compile-time check with a `trybuild`-like test in `xtask`); lint: no call to `get_property` outside `nikomiru-player` | L4 |
| T-ARC-004 | `time-pos` throttling | With 240 updates/s, the UI receives ≤ 30/s; the last value always arrives | L1 |
| T-ERR-001 | Translated errors | Every variant of the public errors has a Fluent key in `it` and `en` (test that enumerates the variants) | L1 |
| T-ERR-002 | No `unwrap`/`expect` on external input | Clippy `unwrap_used`, `expect_used` = error in library crates; T-FUZZ-* without panics | L4 |
| T-EXT-001 | Discovery of ffmpeg and encoders | Order: setting → `NIKOMIRU_FFMPEG` → `PATH` (fake scripts in temporary directories); a fake `-encoders` without `aac` → AAC format disabled with a warning, fallback to Opus | L4 |
| T-EXT-002 | Arguments as a vector and `file:` prefix | argv snapshot; no invocation goes through `sh -c` (see also T-SEC-005) | L3 |
| T-EXT-003 | Timeout and cancellation | Fake ffmpeg that sleeps 120 s → killed at the timeout; with cancellation, the process ends within 1 s and the partial files are removed | L4 |
| T-PRV-001 | No unrequested network traffic | L6 suite under `strace -f -e trace=connect`: no `connect()` to non-loopback addresses if the user has not started network features | L6 |
| T-PRV-002 | Clean logs | Flows with sentinel text in the subtitles and a sentinel API key: neither of the two appears in the log files | L4 |
| T-MAINT-001 | Code quality | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `#![deny(missing_docs)]` in library crates | CI |
| T-MAINT-002 | Coverage | `cargo llvm-cov` ≥ 80% of lines for `nikomiru-text`, `nikomiru-subs`, `nikomiru-dict`, `nikomiru-anki` | CI |
| T-PORT-001 | Wayland and X11 | The L6 smoke test passes both under headless Weston and under Xvfb | L6 |
| T-PORT-002 | Offline build | After `cargo fetch`, `cargo build --offline --workspace` succeeds | CI |

### 6.2 Text (`nikomiru-text`)

| ID | Check | Expected | L |
|---|---|---|---|
| T-LKP-011-a | Katakana → hiragana | `カタカナ` → `かたかな`; `ラーメン` → `らあめん`; `ヵヶ` unchanged | L1 |
| T-LKP-011-b | NFKC | `ｶﾀｶﾅ` → `カタカナ`; `１２３` → `123` | L1 |
| T-LKP-011-c | Collapsing | The variants of `すっっごい` include `すっごい` and **not** `すごい` (BUG-1777) | L1 |
| T-LKP-011-d | Iteration marks | `時々` → `時時`; `こゝろ` → `こころ` | L1 |
| T-LKP-011-e | Digits and numerals | `３` → `三` | L1 |
| T-LKP-010 | Deduplicated variants with minimal steps | Property: for every input, each variant appears only once, with the minimum number of steps; the original input has 0 steps | L2 |
| T-TXT-001 | Hiragana → katakana → hiragana round trip | Property: identity on hiragana-only strings (documented cases excluded) | L2 |
| T-TXT-007 | Sentence extraction | Fushi vectors (§5.1). E.g. `彼は「こんにちは」と言った。次の文。` with a click on `言` → `彼は「こんにちは」と言った。` | L1 |
| T-LKP-050 | Furigana | (`食べ物`, `たべもの`) → `[食\|た][べ][物\|もの]`; (`お茶`, `おちゃ`) → `[お][茶\|ちゃ]`; (`日本`, `にっぽん`) → `[日本\|にっぽん]` | L1 |
| T-ANK-022 | `{furigana-plain}` | `食[た]べ 物[もの]` | L1 |
| T-STA-003 | Character count | `日本語abc 123です` → 7 (3 + 1 + 1 + 2); `가나다` → 3; cases with apostrophes and combining marks from the Fushi vectors | L1 |

### 6.3 Subtitles (`nikomiru-subs`, `nikomiru-media`)

| ID | Check | Expected | L |
|---|---|---|---|
| T-SUB-001 | SRT | `basic`, `bom_crlf`, `tags_braces` (text without `{…}`, italic markup preserved), `out_of_order` (sorted by start) | L1 |
| T-SUB-002 | VTT | `settings.vtt` (line/position/align in the markup), `notes_styles.vtt` (NOTE and STYLE ignored), `ruby.vtt` (`plain_text` without `<rt>`) | L1 |
| T-SUB-003 | ASS | `no_playres.ass` → 384×288; `\N` → line break; `\p1` lines excluded from the clickable text; sorting by Layer | L1 |
| T-SUB-004 | Text / markup separation | `plain_text` never contains tags or overrides (property over all fixture files) | L2 |
| T-SUB-005 | Encodings | Each file in `shift_jis`, `euc_jp`, `utf16le_bom`, `gb18030`, `cp1252` produces the expected text; a forced encoding takes precedence | L4 |
| T-SUB-006 | External files | `Ep01.mkv` ↔ `Ep01.ja.srt` yes, `Ep01.jpn.ass` yes, `Ep011.srt` no, `ep01.SRT` yes | L1 |
| T-SUB-007 | Extraction of embedded tracks (`av_basic.mkv`) | 2 tracks extracted in a single invocation; 3 cues with the expected text; second request served from the cache (no ffmpeg process started) | L4 |
| T-SUB-008 | Automatic selection | The 4 rules in the order given in SPECIFICATION | L1 |
| T-SUB-009 | Clickable overlay | `av_basic.mkv` in L6: the Japanese embedded track is extracted and drawn; `click_grapheme` on the second grapheme selects byte offset 3 and pauses | L6 |
| T-SUB-010 | Style | Without "respect ASS styles" no ASS font, size, colour or alignment reaches the overlay; with it, the style's values and the inline colours do | L1 |
| T-SUB-011 | Offset | `delay_ms = +500`, cue [1000, 2000]: active at `pos` = 1600, not at 1400. Jumping to that cue takes the player to 1000 + 500 − 180 = **1320** | L1 |
| T-SUB-012 | Navigation | Cues [1000–1100], [1150–2000], [5000–6000], `delay_ms = 0`. From `pos` = 5500 (inside the third), "previous" → max(1150 − 180, 1000) = **1000** (never before the start of the cue that precedes the target). From 1200, "next" → 5000 − 180 = **4820**. Without subtitles → seek of ±3 s. From the keyboard, with the previous cue more than 10 s away → time seek | L1 |
| T-SUB-013 | Cue replay from the popup | Pause at the cue `end` (± one frame), with `av_basic.mkv` in L5 | L5 |
| T-SUB-014 | Blur | Sigma = max(12, 0.45 · font_size); absent while paused and with the popup open (state machine) | L1/L3 |
| T-SUB-015 | List that follows playback | In the gaps between cues, it stays on the previous cue; search filters without losing the position | L1 |
| T-SUB-016 | Alignment to the playhead | `pos` = 4900, next [5000–6000] → `delay_ms` decreases by 100 | L1 |
| T-FUZZ-001…003 | SRT / VTT / ASS parsers on arbitrary bytes | No panic; for every cue produced, `start ≤ end` | L2 |

### 6.4 Dictionaries: import and management

| ID | Check | Expected | L |
|---|---|---|---|
| T-DIC-001 | Import of `mini-ja` | The counts of terms, meta, kanji, tags and media match the source; `styles.css` preserved | L4 |
| T-DIC-002 | Tag bank and `sequence` | The `v5k` tag has category and notes; `sequence` preserved | L4 |
| T-DIC-003 | `mini-freq-occ` | String values read; inverted ordering (higher = better) | L4 |
| T-DIC-010 | Cancellation | Cancelling halfway → no dictionary registered, no leftover files. A cancel inside one large bank (60k entries, while it is read, parsed or written) or in the final step stops the import in < 1 s; the parser, the writes and the final step stop at their next check | L1, L4 |
| T-DIC-011 | Malicious ZIPs (§4.3) | Each one is rejected with its specific error; no file written outside `dicts/<id>.importing`. `wrapped-dir`, `unicode-path` and `zip64` are accepted. The zips of `import.json` (§5.3): wrapper folders, macOS files, directory entries, Unicode Path fields, ZIP64 sentinels, broken deflate streams, lying sizes, a damaged dictionary file | L4 |
| T-DIC-012 | Limits | A glossary > 10 MiB makes the import fail cleanly | L4 |
| T-DIC-013 | Crash during import | Child process killed with SIGKILL halfway → on restart, no partial dictionary; the `.importing` folder is deleted | L4 |
| T-DIC-014 | Disk full | Small tmpfs filesystem → `ERR_STORAGE_FULL`, full cleanup; a file size limit reached halfway (`ulimit -f`, child process) → failed import, full cleanup | L4 (Linux) |
| T-DIC-015 | Type detection | Vectors: term, kanji, freq, pitch, term+kanji | L1 |
| T-DIC-016 | Same revision / new revision | Rejection / update offer with atomic replacement. The registration checks the registry again: the same dictionary imported twice at once is registered once; an update whose old dictionary was deleted meanwhile is added; an old folder that cannot be removed does not fail the update (the lookup cache is dropped, the next start removes the folder) | L4 |
| T-DIC-021 | Deletion | The file is closed and deleted; the registry is updated; the lookup cache is invalidated | L4 |
| T-DIC-022 | Concurrent registry edits | 8 threads edit the registry at once while one dictionary is deleted → no edit is lost and the deleted one does not come back | L4 |
| T-DIC-030 | Dictionaries by type | Each dictionary is under its main type (terms, kanji, frequencies, pitch accent, in this order), with a reference row under each of its other types; title, revision, language, entries, size and import date shown; a reference has no controls | L1 |
| T-DIC-031 | Order by drag and drop | A row dragged by its handle over the upper or lower half of a row of its type (a reference too) shows the marker there and moves just above or below it; next to itself or over another type nothing is marked or sent; in the order only the dragged dictionary moves, and ▲/▼ step over the rows of its type; the order decides the glossaries (L4) | L1, L4 |
| T-DIC-032 | Disabled dictionary | It is not queried (spy store) and does not consume the budget | L1 |
| T-DIC-035 | Per-dictionary style subset | The style subset of dictionary A does not affect dictionary B (view-model snapshot); the check of a stylesheet reports the rules applied, the properties left out and the rules ignored (L1); the page's editor shows that report, saves, reverts and clears the user's CSS of a term dictionary (L1); the registry keeps it across an update and a restart and drops the cached lookup (L4); saved on the page, it restyles the popup after the dictionary's `styles.css`, and stays after a restart (L6) | L1/L3/L4/L6 |

### 6.5 Lookup

| ID | Check | Expected | L |
|---|---|---|---|
| T-LKP-001 | Scan start | Click on `l` in `hello world` → `hello world`; click on `語` in `日本語を` → `語を` | L1 |
| T-LKP-002 | Delimiters and spaces | Stops at `。` and `「`; `listen to` stays joined (BUG-1773); `don't` stays joined (BUG-2056) | L1 |
| T-LKP-004 | Candidates | At most 16 code points; no cut inside a Latin word; no prefix that ends with a space | L1/L2 |
| T-LKP-020…024 | Deinflection (`mini-ja` + real `ja.json` tables) | `食べました` → `食べる`; `行かなかった` → `行く`; `読んでいる` → `読む` with the trace shown as `-て « -いる`; `書いた` → `書く`; `高くない` → `高い`; depth never > 10 (property) | L1/L4 |
| T-LKP-023 | Empty rules | `mini-norules`: `食べました` finds `食べる` (BUG-2543) | L4 |
| T-LKP-030 | Longest match | `食べ物を食べる` from the start → first result `食べ物` | L4 |
| T-LKP-031 | Mixed orthography | `取扱い` finds `取り扱い` (vectors from `ja_mixed_orthography_lookup_test.cpp`) | L4 |
| T-LKP-032 | Redirects | `formOf` followed one level deep; an entry with a redirect label and a real definition stays (BUG-2566) | L4 |
| T-LKP-033 | Merging and Kansai exception | `ためになる` → `ため` and not `た` + transformation (BUG-3212) | L4 |
| T-LKP-034 | Sorting | Vectors for each of the 8 criteria, isolated with minimal fixtures; dictionary priority decides only when criteria 1–5 are tied | L1 |
| T-LKP-035 | Budget | `max_results = 2` with 3 headwords and 10 glossaries → 2 complete groups (BUG-1472) | L1 |
| T-LKP-036 | Grouping | Same (expression, reading) from 2 dictionaries → 1 group, glossaries in dictionary order; frequencies and pitch accents deduplicated | L1 |
| T-LKP-037 | Kanji | `食` → entry with on/kun readings, meanings, strokes | L4 |
| T-LKP-038 | Cache | Disabling a dictionary invalidates the cached results | L1 |
| T-LKP-040 | Harmonic frequency | [100, 400] → 160; [50] → 50; no value → 9999999; values ≤ 0 ignored | L1 |
| T-LKP-041 | Morae and pitch accent | `きょう` → [`きょ`, `う`]; nasal and devoicing from `mini-pitch` present in the rendering model | L1 |
| T-LKP-042 | Categories | `はし`: pos 0 → heiban; pos 1 → atamadaka; `おとうと` (4 morae): pos 4 → odaka, pos 2 → nakadaka; verb with pos 2 → kifuku | L1 |
| T-LKP-043 | Pitch accent deduplication | 2 dictionaries with an identical payload → 1 row "pitch accent · 2" | L1 |
| T-LKP-060 | Segmentation | `日本語を勉強します` → [`日本語`, `を`, `勉強`, `します`] with `mini-ja` | L4 |
| T-FUZZ-004 | Deinflector and lookup on arbitrary Unicode strings | No panic; time per query ≤ 50 ms | L2 |

### 6.6 Popup

| ID | Check | Expected | L |
|---|---|---|---|
| T-POP-001/004/005 | Pause/resume state machine | (playing) click → pause and `paused_for_lookup`; close → resume. (paused) click → no change; close → stays paused. Click on another word with the popup open → replacement, stays paused. Nested: resume only when the stack is empty | L1 |
| T-POP-002 | Highlighting | Overlay snapshot: exactly `best_length` graphemes marked starting from the clicked one, even with composite graphemes (emoji, combining marks) | L3 |
| T-POP-003 | Hover | Shift + movement ≥ 8 px → lookup; no new lookup on the same character; leaving for 320 ms → close and resume (fake clock) | L1 |
| T-POP-006 | Positioning and size | Property: for every word rectangle and every window ≥ 320×240, the popup does not cover the word and fits inside the window (L2). A popup is as wide as the configured maximum (`[popup]`) and as tall as its content up to it; sizes out of range in `config.toml` are brought within it, and values that are not numbers take the default (L1). Dragging the border (L1, `egui_kittest`): the edge away from the word and the side edges follow the pointer, the popup keeps its side of the word (also once small enough for the other side) and stays centred on it; dragged past the window it stops at the window's edges and at the configured bounds; the next popups take the dragged size, and a size that was not dragged keeps its value. Property (L2): a popup kept on either side, at any size, does not cover the word and fits inside the window | L1, L2 |
| T-POP-007 | Header | View-model and AccessKit snapshot of the entry `食べました` → headword `食べる` with furigana, trace chip, frequencies, pitch accent, tags | L3 |
| T-POP-008 | Per-dictionary blocks | View-model snapshot: numeric tags hidden; a part-of-speech tag repeated on consecutive senses is omitted; collapsed state respected | L3 |
| T-POP-009 | Structured content | View-model snapshot for table, ruby, list, details. Disallowed tags (`script`, `iframe`, `style`) and `on*` attributes removed; disallowed style properties removed | L3 |
| T-POP-010 | Images | A PNG from the media table is decoded to its pixels, once per path; a nonexistent path, an SVG or a damaged file → alternative text only; a large image is scaled down to 1280 pixels; `..` → rejected; the drawn size keeps the aspect and the flow's width | L1 |
| T-POP-011 | Internal links | The link text excludes `<rt>` (BUG-2456); a click on a link asks for a nested lookup next to it, or for the browser; only `http(s)` addresses are opened (L1); an external link reaches the `OpenURI` portal (spy portal on a private bus, L4) | L1, L4 |
| T-POP-012 | Nesting | Depth 8 at most; Esc closes only the last one; a click on the parent closes the children. A drag across glossary text (one paragraph, or a line of runs with ruby, chips and links, the readings left out) looks the selection up in a child popup next to it and stays lit while that popup is open; a click still follows a link; the summary of a `details` does not select; a drag on text does not scroll the popup (`egui_kittest`) | L1 |
| T-POP-013 | Kanji entry | View-model and AccessKit snapshot for `食` from `mini-kanji` | L3 |
| T-POP-014 | Keyboard | Tab, Enter, Ctrl+Enter, Esc, Alt+wheel (harness) | L6 |
| T-POP-015 | Replay and go to sentence | From the popup: "replay" restarts from the start of the cue and stops at the end; "go" closes the popup and takes the player to the start of the cue (harness) | L6 |

### 6.7 Security

| ID | Check | Expected | L |
|---|---|---|---|
| T-SEC-001 | `mini-ja` entry `危険` with `<script>`, `on*` attributes, a remote `<img>` and a `javascript:` link, through the real import, lookup and popup content builder | Only allowlisted view-model nodes are produced: exactly the span's text and colour and a lookup link, every link a lookup or `http(s)`, every image a path inside the dictionary with decoded pixels, every style within the bounds of DIC-035 (L1); drawn with `egui_kittest` it opens nothing until the link is clicked, and the click is a lookup (L3); in the real app (`lookup`, `popup_content`) the same view model, and no network request (spy proxy, no internet socket held, L6) | L1/L3/L6 |
| T-SEC-002 | Malicious ZIPs | See T-DIC-011 | L4 |
| T-SEC-003 | Disk writes | During the whole L6 suite, no file created outside the isolated XDG directories and the test's export directories (checked with inotify on the test `$HOME`) | L6 |
| T-SEC-004 | Release build | `cargo tree -e normal,build,features -p nikomiru-app` without `test-harness`, `test_harness` or `test-support`; the release binary contains none of the harness's strings and starts, opens a file and quits on SIGTERM (L6) | L4 |
| T-SEC-005 | Argument injection into ffmpeg | `-weird: name & spaces ü 日本.mkv` and `http:fake.mkv` are treated as local files; argv captured with a snapshot | L4 |
| T-SEC-006 | Network isolation of the interface | No network-capable image loader is in the UI's graph (no egui_extras `http`); a remote image source injected into a glossary generates no network requests (spy proxy) | L6 |
| T-SEC-007 | Hostile `nikomiru://` links | Absolute paths, `file:`, duplicate or unknown parameters, > 16 KiB → rejected without effects (see T-ANK-051) | L1 |
| T-SEC-008 | Config permissions | `config.toml` created with permissions 0600 | L4 |

**Security review in M8**: besides the tests, a manual review of every point where external data enters (zips, dictionary JSON, subtitles, HTTP responses, `nikomiru://` links, arguments from the second instance) and of every `unsafe` block. The results go into `docs/security-review-1.0.md`.

### 6.8 Player, shortcuts, single instance

| ID | Check | Expected | L |
|---|---|---|---|
| T-PLY-001 | Commands on `av_basic.mkv` | play/pause; seek ±3 s (expected `time-pos` ± 0.1 s); speed clamped to 0.25–4.0; volume clamped to 0–150; frame step on `av_vfr.mkv` | L5 |
| T-PLY-002 | Hold to speed up | keydown `E` → speed 2.0; key auto-repeats are ignored; keyup → back to the previous speed | L1 |
| T-PLY-003 | Audio tracks | 2 tracks listed with their language; switching track updates `aid` | L5 |
| T-PLY-004 | Control bar | State machine with a fake clock: hides after 2 s without movement during playback; stays visible while paused, with the mouse over the subtitles (moved onto them: subtitles that come back under a resting pointer do not count) and with a popup open | L1 |
| T-PLY-005 | Fixed options | After initialization, every option of PLY-005 has the expected value (`mpv_get`) | L5 |
| T-PLY-006 | Raw mpv options | `deband=yes` applied; `key without equals` → error at line 1; the application order is respected (raw options win) | L1/L5 |
| T-PLY-007 | Resume | `start = 5` → after `file-loaded`, `time-pos` ∈ [4.9, 5.5] and `start` goes back to `none` | L5 |
| T-PLY-008 | After loading | `sid = no`, `sub-visibility = no`; `aid` adopted | L5 |
| T-PLY-009 | Reuse | Two consecutive `load` calls use the same `Mpv` instance (instance counter) | L5 |
| T-PLY-010 | Speed | After a speed change, `af` does not contain `scaletempo` | L5 |
| T-PLY-011 | Screenshot | `S` → PNG/JPEG file in `$XDG_PICTURES_DIR/Nikomiru` with the video dimensions, without subtitles | L5 |
| T-PLY-012 | Errors | Nonexistent file, `corrupt_*.mkv` → typed error; the player stays usable for the next file | L5 |
| T-PLY-013 | Screensaver | During playback the inhibition is active; when paused for > 1 min it is released (spy portal) | L4 |
| T-PLY-016 | `mpv.conf` presets | Round trip: option set from the UI → saved preset → loaded → identical option set, with UI controls and raw options filled as before. A sample `mpv.conf` with comments, a flag (`deband`), `no-` prefix, `--key=value`, quoted values, `profile=high-quality`, a `[section]` and `include=` → preview with applied / ignored / invalid for each line, and no option reaches mpv before confirmation (spy on the option setter). A saved preset is accepted by a real mpv (`mpv --no-config --include=<file> --idle=once` exits with 0) | L1/L5 |
| T-PLY-017 | Blocked options | Every entry of the blocked table, in the raw options and in a preset, is rejected with its reason and never reaches mpv (spy). `script=x.lua`, `input-ipc-server=<tmp>/s`, `log-file=<tmp>/l`, `include=<tmp>/i.conf` → no file created and no socket opened (inotify on the temporary folder, see T-SEC-003); `sub-font=…` and `gpu-api=…` → ignored with a warning | L1/L5 |
| T-PLY-018 | Render and audio settings | Each control sets the expected mpv option (`mpv_get`); runtime options (`deband`, `scale`, `brightness`…) change during playback without reloading the file; `hwdec` shows the "from the next file" note and takes effect on the next `loadfile`; reset of a group restores only that group | L5/L6 |
| T-KEY-001…004 | Shortcuts | Remapping saved and reloaded; binding by scan code (with a simulated AZERTY layout, the physical key of evdev `KEY_Q` still triggers the action bound to it); conflict in the same context rejected; single-key shortcuts disabled in a text field | L1/L6 |
| T-APP-002 | `.desktop` file | `desktop-file-validate` without errors; `MimeType` contains `x-scheme-handler/nikomiru` and the video types; `Exec=nikomiru %U` | L4 |
| T-APP-001 | Single instance | Second start with a file → the file opens in the first instance; the second process exits with 0 | L6 |
| T-E2E-001 | M1 flow | open → play → seek → close → reopen → resumes from the saved position | L6 |

### 6.9 Anki

| ID | Check | Expected | L |
|---|---|---|---|
| T-ANK-001 | Request shape | JSON snapshot of each action; `key` absent if empty | L3 |
| T-ANK-002 | Error mapping | Non-200, non-JSON, missing `result`/`error`, duplicate message → the right typed error | L4 |
| T-ANK-003 | Retry | `deckNames` with a timed-out connection → retried once (wiremock counts 2 requests). `addNote` with the response lost after sending → `CommitUnknown`, only **1** request | L4 |
| T-ANK-011 | "Create Lapis deck" | Model absent → `createModel` + `createDeck`; model present → only `createDeck`; failed check → `LapisSetupMissing`; second run → `already exists` | L4 |
| T-ANK-012/013 | Default mapping and Lapis recognition | Vectors from `lapis_preset_test.dart` | L1 |
| T-ANK-021…025 | Placeholders | HTML snapshots for `{glossary}` (label only on the first sense, scoped `<style>`), `{frequencies}`, `{pitch-accent-positions}`, `{pitch-accent-categories}`, compared with Fushi's output on `mini-ja` | L3 |
| T-ANK-024 | `{sentence}` | Valid offset → `<b>` at the exact position even if the word appears twice; invalid offset → first occurrence | L1 |
| T-ANK-026 | Empty fields | Empty first field → dedicated error; fields that do not exist in the note type are not sent | L1 |
| T-ANK-027 | Tags | `["mine", "nikomiru", "video", "Sousou_no_Frieren"]` deduplicated and in order | L1 |
| T-ANK-030 | Duplicates | `canAddNotesWithErrorDetail` with the same options as `addNote`; on "unsupported action" → fallback to `findNotes`; after a transport error, 30 s pause (fake clock) | L4 |
| T-ANK-032 | Audio interval | Cue [10000, 12000], padding 120/200, previous cue ending at 9950, next cue starting at 12100, `delay` +300 → **[10250, 12400]** | L1 |
| T-ANK-033 | Sentence audio | ffmpeg argv snapshot. On `av_basic.mkv`, cue 2 with track `jpn`: the produced file lasts D ± 50 ms and contains the 1 kHz tone (ffmpeg `astats`); with track `eng` the tone is at 660 Hz (check of `ff-index`, Fushi BUG-345) | L3/L4 |
| T-ANK-034 | Image | Long side 1000 px, JPEG; level 3 = original resolution | L4/L5 |
| T-ANK-035 | Without audio | `av_noaudio.mp4` → mining cancelled with a message; no `addNote` request | L4 |
| T-ANK-036 | Media | Stable content-based name; loopback host → `path`, remote host → base64 `data`; if `addNote` fails → `deleteMediaFile` only for the media uploaded in that attempt | L4 |
| T-ANK-037 | Overwrite | `updateNoteFields` with all fields (unmapped ones = `''`); different note type → rejection | L4 |
| T-ANK-050/051 | Return link | Round trip; rejection vectors from `card_source_link_test.dart` | L1/L2 |
| T-ANK-052 | Opening the link | `nikomiru://source?…` passed to the second instance → video opened at `startMs`, paused, cue highlighted (harness) | L6 |
| T-E2E-ANKI-001 | Real Anki | Anki in Xvfb with AnkiConnect, empty profile → "Create Lapis deck" → mining from `av_basic.mkv` → `notesInfo` shows the expected fields, the media files exist in `collection.media` | L7 |

### 6.10 Library and word audio

| ID | Check | Expected | L |
|---|---|---|---|
| T-LIB-001 | Opening a file | From the command line and with simulated drag & drop (`drop {paths}` in the harness); on Wayland a real drag from another client (a layer-shell surface and KWin's fake input) of a video and of a dictionary, copied and never moved; the XDG FileChooser request and its answer, chosen or cancelled, against a spy portal on a private bus (L4); the desktop's dialog by hand (§9.1) | L4, L6 |
| T-LIB-002 | Folder import | Temporary tree with 2 series, season subfolders and non-video files: per-series mode → 2 series with the correct episodes; per-folder mode → one collection per subfolder plus the root one | L4 |
| T-LIB-003 | File names | Vectors from `video_filename_parser_test.dart`. E.g.: `[SubsPlease] Sousou no Frieren - 01 (1080p) [ABCD1234].mkv` → series `Sousou no Frieren`, ep. 1; `Show.S02E05.1080p.mkv` → s2 e5; `Season 2/03.mkv` → s2 e3; `第2季/03.mkv` → s2 e3 | L1 |
| T-LIB-004 | Rescan | Progress, subtitles and offsets preserved; a deleted series is not recreated; mode change reversible without losses | L4 |
| T-LIB-005 | Home | "Continue watching" sorted by `last_played_at`, without completed episodes; cover generated at 10% of the duration and served from the cache on the second request | L1/L4 |
| T-LIB-006 | Progress saving | With the fake clock, ≤ 1 write/s; no write before the resume seek; synchronous write on exit | L1 |
| T-LIB-007 | Resume rule | (90% → 0), (2.9 s remaining → 0), (previous episode → 0), (50% → saved position) | L1 |
| T-LIB-008 | Completion | `completed_at` set the first time 90% is passed and never overwritten; state watched / in progress / new | L1 |
| T-LIB-009 | Next episode | The 5 s countdown is cancelled with Esc | L6 |
| T-LIB-010 | Per-series preferences | An episode without its own values inherits audio, subtitle and offset from the series; a value set on the episode wins | L1 |
| T-LIB-011 | Library edits | Renaming, merging, splitting, moving episodes: progress and preferences preserved | L4 |
| T-LIB-012 | Removal | The file on disk stays | L4 |
| T-DAT-001/002 | Migrations | A fixture v1 DB → migrated with a `.bak-v1` backup; a DB with a future schema → opened read-only | L4 |
| T-DAT-003 | uid | Stable `sanitize`; two files with the same name in different folders → ` (2)` | L1 |
| T-AUD-001 | Source list | Order and enabled state saved and read back; a disabled source is not queried | L1 |
| T-AUD-002 | URL template | Yomitan JSON response → first entry; direct `audio/mpeg` response → played; 404 → "no audio"; connection refused → 45 s pause for the host | L4 |
| T-AUD-003 | Local DB | Fixture `android.db` (generated by xtask, only after ADR 0003) → exact pair, then expression only, sub-source priority | L4 |
| T-AUD-004 | Source menu | Parallel queries, configuration order, deduplication | L4 |
| T-AUD-005 | Separate audio instance | Playing a word does not change `time-pos`, `pause` or `volume` of the main player | L5 |
| T-AUD-006 | Autoplay | On → the audio starts when the popup opens; off → only with ♪ | L1 |
| T-AUD-007 | No third-party default URL | The default configuration contains no remote sources | L1 |
| T-AUD-008 | Cache | Above 500 MiB, LRU eviction | L1 |

### 6.11 Statistics

| ID | Check | Expected | L |
|---|---|---|---|
| T-STA-001 | Coverage | Watch 0–10 s, seek to 5 s, watch until 15 s → **15 s** in total. Speed 2.0: a jump of 2.4 s in 1 s of real time is not a seek (threshold 1 × 2 × 1.25 + 0.5 = 3.0 s). Plus the vectors from `interval_coverage_test.dart` | L1 |
| T-STA-002 | Per-cue credits | 1000 ms cue: 900 ms watched → no credit, 1000 ms → credit; next session → no double counting | L1 |
| T-STA-004 | Sessions | Pause > 120 s → new session; session spanning 15:00 → 2 segments | L1 |
| T-STA-005 | CSV export | Snapshot | L3 |

### 6.12 Settings, i18n, accessibility

| ID | Check | Expected | L |
|---|---|---|---|
| T-SET-001 | Config | An unknown key is preserved and reported; an invalid value → default plus a warning; writing is atomic (SIGKILL during the write → the previous file stays intact) | L4 |
| T-SET-002 | Immediate application | Change of theme or subtitle size without restart (harness) | L6 |
| T-SET-003 | Contrast | For every text/background pair of the light and dark theme tokens, ratio ≥ 4.5:1 (computed in a unit test from the `Theme` tokens) | L1 |
| T-SET-004 | Language | `LANG=it_IT.UTF-8` → Italian; `LANG=de_DE.UTF-8` → fallback to English; the choice in the settings wins | L1 |
| T-SET-005 | Interface scale | 75% and 200%: the UI scale changes, the subtitle size does not (harness) | L6 |
| T-SET-006 | Per-section reset | Only the keys of that section go back to the default values | L1 |
| T-SET-007 | Proxy | HTTP requests go through the proxy (wiremock as proxy); AnkiConnect on loopback does not | L4 |
| T-I18N-001 | Keys | Every Fluent key exists in `it` and `en`; no unused key; same `{ $var }` arguments in all languages | L1 |
| T-I18N-002 | No strings in code | `xtask` lint: no user-visible string literal in the views of `nikomiru-ui` (explicit exception list) | L4 |
| T-A11Y-001 | Focus and accessibility | AccessKit tree snapshot of the player controls (role and name of each); visible focus | L3 |
| T-E2E-A11Y-001 | Keyboard only | Library → episode → lookup → mining → settings → back, without a mouse (harness) | L6 |

### 6.13 Online (M7)

| ID | Check | Expected | L |
|---|---|---|---|
| T-MET-001 | AniList | Fake GraphQL response → series identified only if there is an exact and unique result; otherwise chosen by the user | L4 |
| T-MET-002 | Jikan | At most 1 request/s; after a 429, pause and cache respected (fake clock) | L4 |
| T-MET-003 | Jimaku | `Authorization` header; search by `anilist_id` and then by `query`; zip extracted with the rules of DIC-011 | L4 |
| T-MET-004 | OpenSubtitles | Query order; file > 64 MiB rejected | L4 |
| T-MET-005 | API keys | Saved with permissions 0600 (see T-SEC-008); never in the logs (see T-PRV-002) | L4 |
| T-MET-LIVE-* | Real services | Smoke tests with keys from environment variables | L7 |

### 6.14 Reliability and fuzzing

| ID | Check | Expected | L |
|---|---|---|---|
| T-REL-001 | SIGKILL during: import, config write, progress saving, mining (child process); a normal quit during an import (L6) | On restart, consistent state: no corrupt DB, no partial dictionary, no orphan media registered; a quit during an import cancels it, exits at once and leaves no repair, record or staging folder | L4, L6 |
| T-REL-002 | Anki closed during mining | "Connection refused" error, no panic, no orphan media | L4 |
| T-FUZZ-005 | Yomitan term bank JSON | No panic; typed error | L2 |
| T-FUZZ-006 | Structured content → view model | No panic; the output contains only nodes from the allowlist | L2 |
| T-FUZZ-007 | Parser for `nikomiru://` links and for the TOML config | No panic | L2 |

### 6.15 Performance

They run nightly on REF (SPECIFICATION §7). The result is a failure if the threshold is exceeded **or** if the result is more than 15% worse than the baseline in `perf-baseline.json`.

| ID | Measure | Method | Threshold |
|---|---|---|---|
| T-PERF-01 | Cold start until `library-ready` | Harness, median of 10 runs (disk cache dropped with `vmtouch -e` or similar) | ≤ 1.5 s |
| T-PERF-02 | From `open` to `first-frame` | Harness | ≤ 1.0 s |
| T-PERF-03 | Dropped frames and CPU during playback | `frame-drop-count`, `vo-delayed-frame-count` (mpv) and `/proc/<pid>/stat` for 5 min on `av_hd60_*` with the overlay visible | 0 dropped; CPU ≤ 25% of one core |
| T-PERF-04 | Lookup in the engine | `cargo xtask bench` (release test `nikomiru-dict/tests/perf.rs`): synthetic dictionary ×5 + freq ×3 + pitch, 10k queries from a sentence corpus, percentiles of the single lookups | p95 ≤ 10 ms, p99 ≤ 25 ms |
| T-PERF-05 | Import of 300k entries | `cargo xtask bench`: total time and `VmHWM` of each import, in its own process | ≤ 60 s; ≤ 1 GiB |
| T-PERF-06 | From `click_grapheme` to `popup-rendered` | Harness, 100 clicks | p95 ≤ 100 ms |
| T-PERF-07 | Cue display delay | `cue-shown.time_pos − cue.start` | ≤ 50 ms |
| T-PERF-08 | Mining | From Ctrl+Enter to `card-created`, fake AnkiConnect, real ffmpeg | ≤ 3 s |
| T-PERF-09 | Idle RSS | 30 s after `library-ready` | ≤ 400 MB |

---

## 7. Bugs and regressions

- Every Nikomiru bug has a report `docs/bugs/AK-BUG-NNN.md` (symptom, steps to reproduce, cause, fix) and a test `ak_bug_nnn_<description>` marked `// AK-BUG-NNN`, written **before** the fix and verified to fail.
- The Fushi bugs cited in SPECIFICATION (`BUG-xxxx`) have a test `fushi_bug_xxxx_<description>` marked `// FUSHI-BUG-xxxx`.
- Priority: P1 = crash, data loss or wrong Anki card; P2 = main feature degraded; P3 = cosmetic issues or rare cases. No release with open P1 bugs.

---

## 8. Traceability (`cargo xtask trace`)

1. Reads the requirement IDs from SPECIFICATION.md: cells and paragraphs that start with `[A-Z]{2,4}-\d{3}` or `NFR-[A-Z0-9]+-\d{2}`.
2. Reads from ROADMAP.md, for each milestone, the **Requirements** line (expanding the ranges such as `PLY-001…PLY-010`) and the **Status**.
3. Looks for the markers `// T-<ID>[-<suffix>]` in the tests (`crates/**`, `tests/**`, `xtask/**`). A marker is a comment that **starts** with one or more IDs (`// T-POP-001, T-POP-004 state machine`); an ID cited in the middle of a comment does not count. NFRs map to the tests listed in the "Measure" column of SPECIFICATION §7 (e.g. `NFR-PERF-04` → `T-PERF-04`, `NFR-REL-02` → `T-FUZZ-*`). A marker that matches no requirement is valid only if the case is in the §6 catalog (e.g. `T-LIC-001`, `T-ARC-020`, `T-TXT-007`).
4. Produces `target/trace.md` (requirement → test → milestone) and **fails** if:
   - a requirement of a `done` milestone has no test;
   - a test cites a requirement that does not exist;
   - a requirement appears in two milestones.

---

## 9. Manual tests

They are recorded in `docs/qa/<version>.md`: who, date, environment (distribution, compositor, GPU, driver, mpv/ffmpeg/Mesa versions), result of each item.

### 9.1 Player (end of M1)

- [ ] Opening from the dialog, from drag & drop and from the command line, with Unicode file names and spaces.
- [ ] Fullscreen on a secondary monitor; exit with Esc.
- [ ] HiDPI 200%: sharp video, sharp overlay, no misaligned edges.
- [ ] hwdec active (check `hwdec-current`) on Intel, AMD and NVIDIA.
- [ ] System suspend and resume during playback.
- [ ] Audio on PipeWire with a device change during playback.

### 9.2 Subtitles (end of M2)

- [ ] Real Shift_JIS SRT, fansub ASS with styles and karaoke (the text stays readable even if the effects are not supported), MKV with 2 embedded tracks.
- [ ] Offset with Z/X and alignment with Ctrl+Shift+arrows on a subtitle that is 2 s out of sync.
- [ ] Blur: unreadable during playback, revealed on mouse hover.

### 9.3 Lookup (end of M3): comparison with Fushi

Import **the same** dictionaries into Nikomiru and into Fushi (JMdict English, a frequency dictionary, a pitch accent dictionary) and compare the first headword for each query. **The oracle is Fushi's result.** The "Indicative" column is only a rough guide.

| # | Query | Indicative | # | Query | Indicative |
|---|---|---|---|---|---|
| 1 | 食べました | 食べる | 26 | 高くて | 高い |
| 2 | 食べなかった | 食べる | 27 | 美しく | 美しい |
| 3 | 食べられる | 食べる | 28 | 寒そう | 寒い |
| 4 | 食べさせられた | 食べる | 29 | 寝ちゃった | 寝る |
| 5 | 食べたい | 食べる | 30 | 見ない | 見る |
| 6 | 食べて | 食べる | 31 | 見せて | 見せる |
| 7 | 食べよう | 食べる | 32 | 起きろ | 起きる |
| 8 | 食べれば | 食べる | 33 | 言わせる | 言う |
| 9 | 行った | 行く | 34 | 知らなかった | 知る |
| 10 | 行かない | 行く | 35 | 頑張って | 頑張る |
| 11 | 行きましょう | 行く | 36 | 思い出した | 思い出す |
| 12 | 書いている | 書く | 37 | 呼ばれた | 呼ぶ |
| 13 | 泳いだ | 泳ぐ | 38 | 歩いて | 歩く |
| 14 | 話します | 話す | 39 | 静かな | 静か |
| 15 | 待って | 待つ | 40 | 日本語を | 日本語 |
| 16 | 死んだ | 死ぬ | 41 | 食べ物 | 食べ物 |
| 17 | 遊ばない | 遊ぶ | 42 | 時々 | 時々 |
| 18 | 読んだ | 読む | 43 | 綺麗 | 綺麗 |
| 19 | 帰ろう | 帰る | 44 | きれい | 綺麗 |
| 20 | 買わなきゃ | 買う | 45 | ラーメン | ラーメン |
| 21 | 来なかった | 来る | 46 | ｶﾀｶﾅ | カタカナ |
| 22 | しなければ | する | 47 | ありがとうございます | ありがとうございます |
| 23 | 高かった | 高い | 48 | 取扱い | 取り扱い |
| 24 | 高くない | 高い | 49 | ためになる | ため |
| 25 | 高ければ | 高い | 50 | 読んでいた | 読む |

Criterion: 50/50 identical, or every difference explained by an intentional divergence (SPECIFICATION §6.6.8) and noted.

### 9.4 Mining (end of M4)

- [ ] 10 cards from 2 real episodes: in Anki desktop, audio and image play, the word is bold in the sentence, the furigana is correct.
- [ ] After syncing to AnkiDroid: audio and image work.
- [ ] The link in the card reopens the scene in Nikomiru (with Nikomiru closed and with Nikomiru open).
- [ ] Duplicate: the button shows ✓; "add anyway" creates the second card; "overwrite" replaces the last one.
- [ ] With Anki closed: clear message, no freeze.

### 9.5 Release (M8)

Sections 9.1–9.4, plus:

- [ ] Clean install from the package (AUR and Flatpak) on GNOME Wayland, KDE Wayland and X11.
- [ ] First start without configuration: language from the system, first-start guide (dictionaries, Anki).
- [ ] "About" shows the version and `THIRD_PARTY_LICENSES`.
- [ ] Upgrade from a previous version: DB migration with backup.
- [ ] A 2-hour session without abnormal memory growth.

---

## 10. CI pipeline

| Stage | When | Content | Blocking |
|---|---|---|---|
| Fast | Every push | `cargo xtask ci`: fmt, clippy `-D warnings`, `cargo deny`, `check-headers`, `trace`, L1+L2+L3 (`cargo nextest`), doctest | Yes |
| Full | Every PR | Fast stage + `cargo xtask fixtures` + L4 + L5 + L6 smoke (Xvfb and headless Weston or virtual KWin) + coverage (NFR-MAINT-02) | Yes |
| Nightly | Every night | L2 with 100k cases, fuzzing (T-FUZZ-*) for 10 minutes each, full L6, L8 with baseline, T-LIC-002, L7 where secrets or data are available | Opens an issue if it fails |
| Release | Tag `v*` | Full + nightly + checklist §9.5 + `THIRD_PARTY_LICENSES.html` | Yes |

The CI image contains: Rust stable (the version in `rust-toolchain.toml`), `libmpv-dev` ≥ 0.37, `ffmpeg` ≥ 6 with `libx264`/`libx265` (only to generate the performance fixtures), the libraries winit and glutin load at run time (Mesa EGL/GL, libwayland-client, libxkbcommon, the X11 client libraries), fontconfig and a CJK font (Noto Sans CJK), Xvfb, Weston (or KWin), `cargo-deny`, `cargo-about`.


**L6 tests never run on the developer's graphical session.** A covered window does not receive the compositor's frame callbacks and does not render (ADR 0001). An off-screen compositor is always used: headless Weston, virtual KWin or Xvfb.
