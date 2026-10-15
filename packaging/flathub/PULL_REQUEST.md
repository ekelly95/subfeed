Add io.github.ekelly95.Subfeed

subfeed is a distraction-free YouTube subscription player for the Linux desktop: the channels you
follow, newest first, in a quiet keyboard-first player. No Google account, no recommendations, no
autoplay. Source: https://github.com/ekelly95/subfeed (GPL-3.0-or-later). I'm the developer.

## Build
- KDE 6.11 runtime. The app is Rust (cxx-qt) + QML; its crates are vendored through
  `cargo-sources.json` (flatpak-cargo-generator) and cxx-qt-cmake is a source, so the build is
  offline.
- libmpv and MpvQt are built as in org.kde.haruna, minus libXpresent and uchardet: subfeed draws
  through libmpv's render API (no mpv X11 window) and plays YouTube captions, which are UTF-8.

## Prebuilt components
Following org.nickvision.tubeconverter (Parabolic):
- **yt-dlp:** the official release zipapp (Python source plus yt-dlp's EJS challenge solver, which
  YouTube playback needs), checked by sha256. x-checker-data opens a PR for each yt-dlp release; the
  build sets `SUBFEED_YTDLP_UPDATES=off`, so the app never downloads yt-dlp itself and updates arrive
  through Flathub.
- **Deno:** the official release binary, the JavaScript runtime yt-dlp's solver uses.

## Permissions
- `--share=network`: YouTube's feeds, thumbnails, streams; SponsorBlock (queried by a 4-hex-digit
  hash prefix).
- `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc`, `--device=dri`: the window and
  hardware video decoding (VA-API).
- `--socket=pulseaudio`: audio.
- `--own-name=org.mpris.MediaPlayer2.subfeed`: media keys and the desktop's media controls.
- `--talk-name=org.freedesktop.Notifications`: "new videos" notifications with a Play action.
- `--talk-name=org.freedesktop.ScreenSaver`: keeps the screen awake while playing where the Inhibit
  portal isn't available (the portal is tried first).
- `--system-talk-name=org.freedesktop.UPower`, `org.freedesktop.UPower.PowerProfiles`,
  `net.hadess.PowerProfiles`: on battery or in power-saver mode it picks lower-power streams, and
  switches live when that changes.
- `--system-talk-name=org.freedesktop.NetworkManager`: reads the `Metered` property only, so the
  background refresh skips metered connections.

Background refresh uses the Background portal (autostart, only after the user agrees) and the
Flatpak portal's Spawn. Notes are saved through the FileChooser portal, so there's no filesystem
permission.

## Checks
- `flatpak-builder-lint manifest`: passes.
- Built from the v0.1.1 tag with `--mirror-screenshots-url=https://dl.flathub.org/media/`;
  `builddir`/`repo` lint locally reports only the mirrored-media checks (screenshots, remote icon),
  whose catalogue entries are relative to that media base URL as on Flathub's builders.
- Built and run locally: channel import, refresh, playback with VA-API, captions, SponsorBlock,
  and the background refresh.
