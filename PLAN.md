# subfeed — design and roadmap

A distraction-free YouTube subscription player for the Linux desktop.
No Google account and no algorithm: just the channels you follow, newest first, played beautifully.

## Principles
- **Chronological feed of your subscriptions only.** No recommendations, comments, trending, view counts or autoplay into suggestions. Shorts never mix into the feed; they have their own view, opened on purpose.
- **Linux-only on purpose.** Use what the platform gives for free (MPRIS, desktop notifications, UPower, power-profiles-daemon, systemd, VA-API) instead of cross-platform lowest common denominators. Works on any desktop (KDE, GNOME, others), X11 or Wayland.
- **Its own look ("Cinema").** Follows the desktop's light or dark setting (the player stays dark). Near-black ground (light grey in the light theme), bundled Geist/Geist Mono (OFL, `fonts/`), an image-led feed with a glowing hero, channel pills instead of a sidebar, and a player with fading controls, a chapter-split seek bar and a side panel. Only the accent color follows the desktop. Tokens live in `qml/Theme.qml`; controls are built on QtQuick.Templates, so no desktop widget style leaks in.
- **Keyboard-first:**
  - Feed: `j`/`k` move, `Enter` plays, `w` marks watched, `h` hides, `/` searches, `s` Shorts view, `m` channels
  - Player: `Space` play/pause, `←`/`→` seek, `f` fullscreen, `[`/`]` speed, `,`/`.` chapters, `c` captions, `n` save notes
- **Private by design.** Subscriptions and history live in a local SQLite file. The app never signs in to Google.
- **Easy on batteries.** Hardware decoding on the integrated GPU, quality that follows the power state, and nothing that wakes a sleeping discrete GPU.

## Stack
| Layer | Choice | Why |
|---|---|---|
| UI | QML on QtQuick.Templates with its own design tokens; Kirigami for `ShadowedImage` | a distinct, consistent look on every desktop |
| Logic | **Rust** via **cxx-qt 0.10** (KDAB), built with **CMake + Corrosion** | Qt types and signals from Rust |
| Player | **MpvQt** (KDE's libmpv QML item, also used by Haruna) over **libmpv** | embedded playback, proven on Wayland |
| Streams | **yt-dlp** (subprocess, JSON output) | the robust way to resolve stream URLs |
| Feed | Public channel RSS (`/feeds/videos.xml?channel_id=UC…`), with a yt-dlp fallback | no API key, no account |
| Storage | SQLite (`rusqlite`) at `$XDG_DATA_HOME/subfeed/subfeed.db` | one file, easy to back up |
| Optional | SponsorBlock API, DeArrow titles | skip sponsors, de-clickbait titles |

## Platform features
1. **Battery-smart playback**
   - Prefer the integrated GPU for decoding (VA-API), and pick codecs the GPU can actually decode (AV1 > VP9 > H.264 among the supported ones). On hybrid laptops the discrete GPU stays asleep.
   - Read UPower and power-profiles-daemon over D-Bus. On battery or Power Saver: at most 720p30. On AC: up to 1080p60 (or the screen's resolution).
2. **Desktop integration**
   - **MPRIS**, so media keys and any desktop's media widget control playback.
   - **Notifications:** "3 new videos from …", grouped, with a "Play" action.
   - Optional KDE extras: a **KRunner** runner (type a channel or title → play).
   - XDG paths, a `.desktop` file, a single instance (opening it again focuses the window).
3. **Background refresh**
   - A **systemd user timer** (every 30 min, skipped on metered connections) runs `subfeed --refresh`, so the app opens with the feed ready. On battery a refresh reads only the RSS feeds (no yt-dlp).
   - Polite fetching: ETag/If-Modified-Since, jitter, per-channel backoff.
4. **Study hooks** (optional)
   - A **"Notes" action**: the video's transcript as Markdown notes in a chosen folder (an Obsidian vault works well).
   - Optional **local summary** with Ollama, built from the transcript only and labelled as such.

## Data model (SQLite, versioned migrations in `rust/src/db.rs`)
- `channels(id PK /*UC…*/, title, added_at, folder, last_fetch, etag, last_modified, fail_count, retry_after, last_error)`
- `videos(id PK, channel_id, title, published_at, published_approx, duration_s, thumb_url, is_short, first_seen, watched_at, hidden, position_s, description, chapters_json, last_played_at)`
- `settings(key PK, value)`
- Search matches titles with `fold()`, a Rust Unicode lowercase registered on each connection (SQLite's `lower()` is ASCII-only). Accents still count: "cafe" doesn't find "café".

## Import
- **Google Takeout:** export `YouTube and YouTube Music → subscriptions` → `subscriptions.csv`. Import is idempotent: re-importing adds channels and never deletes.
- Or add a channel by URL (with or without `https://`), `@handle`, bare handle or `UC…` id (resolved once with yt-dlp).
- In the app: the first-run screen does both when there are no channels yet.

## Milestones
- **M0 — skeleton:** ✅ CMake + Corrosion + cxx-qt; QML window; MpvQt item; `--version`, `--smoke`.
  - Finding: on hybrid NVIDIA laptops, opening a PipeWire audio output can wake the dGPU (its HDMI audio device). So mpv has no audio output until something plays, and releases it on stop.
- **M1 — data:** ✅ Takeout import, `--add`, RSS parsing, SQLite with migrations, headless `--refresh/--list/--channels/--remove` (run before any GUI object exists).
  - Finding: YouTube's feed server sends no ETag or Last-Modified (only `Cache-Control: max-age=900`), so every fetch is a full download (~40 KB). The conditional GET stays for servers that do send them; "unchanged" in the summary means "no new videos".
  - Finding: YouTube's RSS endpoint periodically 404s for every channel. Refresh falls back to yt-dlp's `/videos` tab (no Shorts, real durations, day-precision dates marked approximate and corrected by RSS later). After 5 RSS failures with no success, a run skips RSS. Backoff (15 min → 24 h) only when both sources fail.
- **M2 — feed UI:** ✅ Rust list models (cxx-qt `QAbstractListModel`), card grid, search, unwatched-only, channel filter, hide with Undo, background refresh, thumbnail cache (`$XDG_CACHE_HOME/subfeed/thumbs`, prefetched by `--refresh`, pruned).
- **M3 — player and the "Cinema" design:** ✅ One `yt-dlp -J` per open (~3 s) gives metadata and formats; Rust picks the streams (`stream.rs`); mpv plays direct URLs with yt-dlp's headers. Resume, position saved every 5 s, watched at 95 %, no autoplay ("Next in your feed" offered), plain-language errors (bot check → VPN hint, Retry). Description and chapters cached.
  - Dev options: `--open <video-id>`, `--screenshot <png>` (the app grabs its own window).
  - ✅ Prefetch (`rust/src/lookup.rs`): half a second of hover or keyboard focus on a video looks it up in the background, hiding the lookup (~3 s normally, ~11 s in power-saver, where yt-dlp's challenge solving is CPU-bound). One prefetch at a time, newest request wins, at most 20 per 10 minutes; results kept 30 minutes; opening a video mid-prefetch waits for it instead of asking twice.
  - ✅ Captions (`c`, CC button; off by default, remembered): `stream::pick_captions` chooses the channel's own captions in the viewer's language, then in the video's language, then YouTube's automatic ones in the original language when it's the viewer's, then automatic ones translated to it (WebVTT, else SRT). mpv adds the track once the file has loaded; captions move above the controls while those show.
- **P — portable:** 🚧 run well on any distro and desktop.
  - Decoder: let mpv choose a safe hardware decoder (`auto-safe`), preferring VA-API; choose codecs from what the GPU decodes.
  - Bundled icons (no dependency on the desktop's icon theme); Qt's Basic style for the few stock controls.
  - Battery state from UPower (D-Bus), not `/sys`.
  - ✅ Flatpak (`packaging/flatpak/`): KDE 6.11 runtime; libmpv and MpvQt built as in Flathub's Haruna (without libXpresent and uchardet, which subfeed doesn't need); yt-dlp's official release (it carries the YouTube challenge solver) and Deno; the Rust crates vendored from `cargo-sources.json` (regenerate with flatpak-builder-tools' `flatpak-cargo-generator.py` when `Cargo.lock` changes) and cxx-qt-cmake as a source, since the build is offline. Built and tested locally: add, refresh, lengths, and playback with VA-API.
    - ✅ Background refresh: `--install-timer` asks the Background portal to autostart `subfeed --background` at login and starts it now through the Flatpak Spawn portal (a sandbox of its own, so the command returns at once). Every 30 minutes while the window isn't open (it refreshes itself), it refreshes with notifications (RSS only on battery) and, on AC, keeps yt-dlp current; one instance (D-Bus name `…Subfeed.Background`); `--uninstall-timer` turns it off. The app offers it once after the first import.
    - ✅ Its own yt-dlp: `$XDG_DATA_HOME/subfeed/yt-dlp`, updated from yt-dlp's official release (checked against its SHA2-256SUMS and test-run before it replaces the old one) when GitHub has a newer one (asked at most daily), at once after a playback failure caused by yt-dlp, and on `--update-yt-dlp`.
  - ✅ AUR (`packaging/aur/PKGBUILD`, `subfeed-git`): builds with makepkg, runs the tests. `!lto`: cxx-qt's C++ objects sit in a static library made by Rust's tools, which can't index LTO objects.
  - ✅ CI (`.github/workflows/ci.yml`): tests, clippy with warnings as errors, the full build, and the metadata validators, in an Arch container on every push. Also `packaging/check-version.sh` (CMakeLists.txt, Cargo.toml/.lock and the metainfo's newest release agree) and qmllint, reported but not enforced: it can't see the cxx-qt and MpvQt types, so most of its ~260 warnings are about those.
  - ✅ Releases (`.github/workflows/release.yml`): pushing a tag `vX.Y.Z` checks it matches the version, builds `subfeed.flatpak` from `packaging/flatpak` in Flathub's KDE 6.11 image and attaches it to that tag's release (a draft if none exists yet). To release: bump the three versions and add a `<release>` to the metainfo, commit, then `git tag -a vX.Y.Z -m "X.Y.Z: highlight"` and push the tag.
  - CI can't run `--smoke`: with `QT_QPA_PLATFORM=offscreen` the window never initialises libmpv (no GL context), and the Arch container has no display server.
  - ✅ App ID `io.github.ekelly95.Subfeed` (the .desktop file, icon, D-Bus service, MPRIS and notifications). Settings moved from `~/.config/subfeed.dev/` to `~/.config/subfeed/` (copied once on first start).
  - ✅ Screenshots (`docs/screenshots/`).
  - ✅ Public (2026-10-15): history squashed, v0.1.0 and v0.1.1 released; a `.flatpak` file is attached to releases.
  - Not on Flathub: its rules forbid AI-generated manifests and AI-opened submissions, and want a longer development history. If it's ever submitted, the author writes the manifest and submits it, disclosing the AI-written code. `SUBFEED_YTDLP_UPDATES=off` stays for builds whose store updates yt-dlp.
- **M4 — integration:** ✅
  - **MPRIS** (`cpp/mpris.*`): registered only while a video is open; title, channel, length, art, position; Play/Pause/Seek/SetPosition/Rate/Volume; Next = next in feed, Previous = restart; Raise/Quit.
  - **No screen blanking while playing** (`cpp/inhibitor.*`): XDG portal Inhibit (idle + suspend), falling back to org.freedesktop.ScreenSaver; released on pause.
  - **Single instance** (`cpp/appbus.*`, D-Bus `io.github.ekelly95.Subfeed` `/App`): a second `subfeed` or `subfeed --open ID` hands over (with its XDG activation token) and exits.
  - **Live quality:** UPower/power-profiles changes mid-video re-pick from the already fetched formats (no new YouTube request) and continue at the same position ("Switched to 720p to save power").
  - **Notifications** (`rust/src/notify.rs`, zbus): `--refresh --notify` posts one notification per run with a Play action (opens the video, or hands it to the open window). Not for a channel's first fetch, Shorts, hidden videos or anything older than 2 days. Skipped on metered connections (NetworkManager).
  - **Background refresh:** `subfeed --install-timer` / `--uninstall-timer` write a systemd user timer (every 30 min, low priority). The open app also refreshes every 30 min, and on opening when the last check is over 15 min old.
  - **On battery** (`refresh::Options::for_power`, or `--refresh --battery`): RSS only, about 40 KB per channel; no yt-dlp fallback or length lookups. A channel whose RSS fails waits for the next run on AC without backing off. (Until 2026-10-17 the timer had `ConditionACPower=true`, which left the feed a day stale on an unplugged laptop, long enough for a Shorts-heavy channel's videos to fall out of its 15-entry RSS.)
- **Shorts view:** ✅ a "Shorts" pill (or `s`, or `--shorts`) swaps the grid for tall 9:16 cards, newest first; the channel pills still filter and their counts switch to unwatched Shorts. No hero or "continue" row, no notifications. In the player, any video narrower than 16:9 plays in a centred column with its thumbnail blurred behind it.
  - Finding: yt-dlp gives a vertical Short's height as its long side (720×1280 has height 1280), so the quality cap compares the short side, as YouTube's "720p" does. Before, a Short got 360×640 on battery.
  - Finding: QML timers follow the animation clock, which lagged real time by about a quarter during playback here; the `--open --screenshot` safety net allows for that.
- **M5 — extras:**
  - ✅ Notes (`rust/src/notes.rs`; `N` or "Save notes" in the player, `Shift+N` picks the folder; `subfeed --notes <id> <folder>`): the video as a Markdown note (frontmatter, description in a folded callout, transcript in timestamped paragraphs under the video's chapters). The transcript comes from the channel's captions in the video's language, else YouTube's automatic captions of the original audio (labelled), a translation only as a last resort; sound cues and speaker markers are dropped. An existing note is never overwritten. "Open" opens it in Obsidian when the folder is inside a vault.
  - ✅ SponsorBlock (`rust/src/sponsorblock.rs`): sponsor reads, unpaid self-promotion and subscribe reminders are skipped ("Skipped sponsor · Undo") and shown in green on the seek bar. Private lookup: only the first 4 hex digits of the video id's SHA-256 go to sponsor.ajay.app, which answers for the whole bucket. Fetched after the video is ready, so playback never waits for it.
  - Later: KRunner runner, Ollama summary.
- **M6 — polish:** 🚧
  - ✅ `.desktop` file and app icon (`io.github.ekelly95.Subfeed`, installed by `cmake --install`). Outside Flatpak the app registers its ID with the portal's host Registry, so it's named correctly even when started from a terminal.
  - ✅ First-run screen (`qml/WelcomePage.qml`, `rust/src/setup.rs`): with no channels, a welcome page offers the Google Takeout import (steps, a button to Takeout's YouTube page, a file picker, or drop the file on the window) or adding channels by @handle, bare name or URL. Afterwards the feed takes over and refreshes.
  - ✅ Channels page (`qml/ChannelsPage.qml`, header button, `m`, or `--manage`): every channel with its video count, newest upload and any refresh error; click one to see its videos, follow more (the same `AddChannelBox` as the first-run page) or remove one, after a confirmation, since its videos and watch history go too.
  - AUR package.

## Testing
- `cd rust && cargo test`: data, parsing, stream choice, refresh logic, on fixtures (no network).
- `./build/subfeed --smoke`: the window opens and libmpv initialises. Needs a real display (not run in CI).
- `packaging/check-version.sh [vX.Y.Z]`: the version is the same everywhere (and matches the tag).
- UI checks: run against copies (`SUBFEED_DB=…`, `XDG_CONFIG_HOME=…`) and capture with `--screenshot` (and `--open <id>` for the player). `SUBFEED_SCREENSHOT_SCALE=2` captures at twice the size; the README images (`docs/screenshots/`, WebP at quality 90) come from a demo database of ten public channels, imported from a Takeout-style CSV with its own `XDG_CACHE_HOME`.
- Demo video (`docs/subfeed-demo.mp4`): `subfeed --tour --record <folder>` walks through the app by itself and saves the window 15 times a second as JPEG frames (the app films itself: no screen-capture permission, nothing else on screen; the window must stay uncovered). Run it on the demo database, measure the real frame rate from the files' times, and encode with ffmpeg (H.264, CRF 20).
- Video lengths: YouTube's feed has none, so after RSS a refresh makes one yt-dlp `/videos` call per channel that has new videos without a length (at most 10 channels per run, one at a time) and fills lengths only; it never adds videos. `channels.durations_at` remembers the lookup, so a channel is asked again only once newer videos arrive. A fresh 10-channel import: 83 of 83 lengths in a 20 s first refresh. `SUBFEED_TEST_MUTE=1` keeps playback silent while still using the real audio path.

## Risks
- **YouTube changes break yt-dlp:** keep yt-dlp current. subfeed helps (`rust/src/ytdlp_status.rs`, `ytdlp::outdated_note`):
  - When the installed yt-dlp is over six weeks old, it asks GitHub for the latest release (at most once a day, cached in `settings`) and, only if one is newer, shows a notice in the feed with the update command for how yt-dlp was installed (package manager, user install, or the Flatpak); "Later" hides it until the next release. A quiet stretch between releases doesn't nag.
  - When playback fails in yt-dlp (not for a removed/private video, a bot check or a live stream), the error card also says whether a newer yt-dlp is out.
  - If yt-dlp is missing, the feed says so.
  - Flatpak: bundling yt-dlp is what makes other clients lag behind YouTube, so the Flatpak updates its own copy (above).
- **VPN addresses get bot checks:** detect yt-dlp's sign-in/bot error and say so plainly; suggest disconnecting or retrying.
- **RSS lists only the ~15 newest videos per channel:** fine for a feed.
- **Terms of service:** a personal client like FreeTube or Pipeline; no redistribution, no ad-blocking claims; SponsorBlock is community data.

## Not doing
Comments, recommendations, search of all of YouTube, uploads, sign-in, Windows/macOS support.
