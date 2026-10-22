# SPDX-License-Identifier: GPL-3.0-only
# Nikomiru — English interface strings (SET-004).

app-title = Nikomiru
open-file = Open file…
file-filter-video = Video files
file-filter-all = All files
drop-hint = Drop a video here, or press Ctrl+O to open one

play = Play
pause = Pause
fullscreen = Fullscreen
exit-fullscreen = Exit fullscreen
volume = Volume
mute = Mute
unmute = Unmute
speed = Speed
speed-up = Faster
speed-down = Slower
position = Position
audio-track = Audio track
no-audio = No audio
track-unknown-language = Unknown language
chapter-previous = Previous chapter
chapter-next = Next chapter

toast-volume = Volume { $value }%
toast-speed = Speed { $value }×
toast-muted = Muted
toast-unmuted = Sound on
toast-paused = Paused
toast-playing = Playing

error-file = Cannot play “{ $file }”: { $reason }.

# Settings (nikomiru-core ConfigError, PathsError)
error-config = The settings file { $path } is not valid at line { $line } ({ $reason }). The defaults are in use until you fix it.
error-config-read = The settings file { $path } cannot be read ({ $reason }). The defaults are in use.
error-config-write = The settings cannot be saved to { $path } ({ $reason }). Check that the folder is writable.
error-config-serialize = The settings cannot be saved ({ $reason }).
error-paths = HOME is not set: Nikomiru cannot find its folders.

# Library (nikomiru-library LibraryError)
error-library = The library cannot be used ({ $reason }): positions and history are not saved.
error-library-record = A library record is damaged ({ $reason }).
error-library-io = The library file { $path } cannot be used ({ $reason }).
error-library-read-only = The library was written by a newer version of Nikomiru: it is open read-only.
error-library-schema = The library file is damaged: it has no valid version. Nikomiru leaves it as it is.

# Player (nikomiru-player PlayerError, PlaybackError)
error-player = The player cannot do this ({ $reason }).
error-player-thread = The player cannot start ({ $reason }).
error-video-output = The video cannot be shown: { $reason }
playback-loading-failed = the file is missing, unreadable or damaged
playback-audio-output = no audio output is available
playback-video-output = the video output cannot start
playback-nothing-to-play = the file has no audio or video
playback-unknown-format = the file format is not recognized
playback-unsupported = the file is not supported
playback-other = mpv error { $code }

# FFmpeg (nikomiru-media ToolsError)
error-ffmpeg-missing = FFmpeg was not found. Install it, or set its path in Settings → Advanced: until then, subtitle extraction and card media are off.
error-ffprobe-missing = FFmpeg was found at { $path }, but ffprobe is not next to it.
error-ffmpeg-run = { $path } cannot be run ({ $reason }).
error-ffmpeg-timeout = { $path } did not answer within { $seconds } seconds.
error-ffmpeg-old = FFmpeg { $version } is too old: version { $minimum } or newer is needed.

# External processes (nikomiru-media JobError)
error-job-spawn = { $program } cannot be started ({ $reason }).
error-job-timeout = { $program } did not finish within { $seconds } seconds and was stopped.
error-job-cancelled = { $program } was cancelled.
error-job-failed = { $program } failed: { $reason }

# Dictionaries (nikomiru-dict ImportError, StoreError)
error-dict-io = { $path } cannot be used ({ $reason }).
error-dict-zip = The file is not a valid zip archive ({ $reason }).
error-dict-unsafe = The dictionary contains an unsafe file ({ $entry }): it was not imported.
error-dict-no-index = This is not a Yomitan dictionary: index.json is missing.
error-dict-format = { $file } is not in the Yomitan format ({ $reason }).
error-dict-too-many = The dictionary has more than 10 million entries.
error-dict-glossary = The definition of { $expression } is larger than 10 MiB.
error-dict-empty = The dictionary contains nothing Nikomiru can use.
error-dict-storage-full = The disk is full: free some space and import the dictionary again.
error-dict-cancelled = Import cancelled.
error-dict-store = The dictionary database cannot be used ({ $reason }).
error-dict-same-revision = { $title } revision { $revision } is already installed.
error-dict-unknown = The dictionary no longer exists.

# Single instance (nikomiru-app InstanceError)
error-instance-io = { $path } cannot be used ({ $reason }): more Nikomiru windows may open.
error-instance-no-answer = Nikomiru is already running but does not answer. Close it and try again.

# Subtitles (M2)
subtitles = Subtitles
subtitles-off = Off
subtitle-open-file = Open subtitle file…
subtitle-encoding = Encoding
encoding-auto = Automatic
subtitle-list = Subtitle list
subtitle-search = Search the subtitles
subtitle-list-close = Close the subtitle list
file-filter-subtitles = Subtitle files
toast-sub-loaded = Subtitles: { $name }
toast-sub-loaded-encoding = Subtitles: { $name } ({ $encoding })
toast-sub-delay = Subtitle offset { $value } ms
toast-sub-blurred = Subtitles blurred
toast-sub-hidden = Subtitles hidden
toast-sub-visible = Subtitles visible
subtitles-disable = Subtitles off
toast-sub-off = Subtitles off (V to show them again)
toast-sub-on = Subtitles on
encoding-utf-8 = Unicode — UTF-8
encoding-utf-16le = Unicode — UTF-16 (little-endian)
encoding-utf-16be = Unicode — UTF-16 (big-endian)
encoding-cp932 = Japanese — Shift_JIS (CP932)
encoding-euc-jp = Japanese — EUC-JP
encoding-gb18030 = Chinese, simplified — GB18030
encoding-big5 = Chinese, traditional — Big5
encoding-cp1252 = Western European — Windows-1252
encoding-iso-8859-1 = Western European — ISO-8859-1
error-subtitles = The subtitles cannot be shown: { $reason }
error-subtitle-read = { $path } cannot be read ({ $reason })
error-subtitle-encoding = the encoding of { $path } is not recognized: choose it in Subtitles → Encoding
error-subtitle-empty = the file has no subtitle lines
error-subtitle-bitmap = image subtitles are drawn by the player

# Shown by the desktop while the screensaver is inhibited (PLY-013).
inhibit-reason = Playing a video

# Dictionary popup
popup-loading = Looking up…
popup-no-results = No dictionary has this word. Import dictionaries from the menu.
popup-pitch-accent = pitch accent
popup-replay = Replay
popup-goto = Go to the sentence
# The name of the popup's border for screen readers: dragging it resizes the popup.
popup-resize = Resize the popup
kanji-strokes = strokes: { $count }
kanji-radical = radical: { $radical }

# Dictionaries page
dict-page-title = Dictionaries
dict-import = Import…
dict-import-filter = Yomitan dictionaries
dict-empty = No dictionaries yet. Import a Yomitan dictionary (.zip), for example JMdict or Jitendex, a frequency dictionary or a pitch accent dictionary, or drop it on the window.
dict-importing = Importing
dict-cancel = Cancel
dict-entries = entries
dict-update-offer = Another revision of this dictionary is installed. Update it? Its settings are kept.
dict-update = Update
dict-keep = Keep the installed one
dict-revision = revision
dict-enabled = Used for lookups
dict-move-up = Move up (higher priority)
dict-move-down = Move down
dict-rename = Rename (empty restores the title)
dict-delete = Delete
dict-delete-confirm = Delete this dictionary? Its files are removed.
dict-collapse = Block in the popup
dict-collapse-automatic = Automatic
dict-collapse-expanded = Expanded
dict-collapse-collapsed = Collapsed
dict-close = Close
dict-type-terms = terms
dict-type-kanji = kanji
dict-type-frequency = frequencies
dict-type-pitch = pitch accents
dict-size-mb = { $whole }.{ $tenth } MB
dict-size-kb = { $size } kB
dict-imported = { $title } imported.
dict-same-revision = { $title } is already installed with this revision.
dict-import-busy = Wait for the import in progress to finish.
dict-css-saved = The style of { $title } is saved.
dict-group-terms = Terms
dict-group-kanji = Kanji
dict-group-frequency = Frequencies
dict-group-pitch = Pitch accent
dict-reference = { $name }, listed under { $group }
dict-language = language { $code }
dict-drag = Drag to change the order
dict-css = Your style for this dictionary (CSS)
dict-css-intro = Your CSS for this dictionary's definitions, applied after its own styles.css. Only colours, font size, weight and style, margins, padding, borders and backgrounds are used, on the selectors Yomitan dictionaries use (a tag, gloss-sc-… classes, data-sc-… attributes).
dict-css-field = CSS of { $title }
dict-css-save = Save
dict-css-revert = Revert
dict-css-clear = Clear
dict-css-unsaved = Not saved yet
dict-css-rules = Rules applied: { $count }
dict-css-dropped = Properties left out: { $list }
dict-css-ignored = Rules ignored: { $list }
error-dict-unavailable = The dictionaries are not available: the library could not be opened.
error-file-dialog = The file dialog is not available: drop the file on the window, or give it on the command line.
