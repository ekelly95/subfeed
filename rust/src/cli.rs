//! Headless commands. `main.cpp` hands these over before any Qt GUI object exists, so the
//! systemd timer can run `subfeed --refresh` without a display.

use std::ffi::{CStr, c_char, c_int};

use anyhow::{Context, Result, bail};

use crate::db::{self, Db};
use crate::refresh::{self, HttpSource, Options};
use crate::{notify, takeout, thumbs, ytdlp};

pub const USAGE: &str = "\
Headless commands:
  --import <subscriptions.csv>   Add channels from a Google Takeout export (never removes any)
  --add <channel>...             Add channels by @handle, URL, or UC… id
  --remove <UC…id>...            Remove channels and their videos
  --refresh [--force] [--no-fallback] [--notify]
                                 Fetch new videos (--force ignores backoff and metered networks;
                                 --no-fallback skips yt-dlp; --notify posts a desktop notification)
  --install-timer                Refresh every 30 minutes in the background (systemd user timer, on AC)
  --uninstall-timer              Remove that timer
                                 (In the Flatpak these use the desktop's Background portal instead.)
  --update-yt-dlp                Flatpak: install the latest yt-dlp now (it also updates itself daily)
  --list [N]                     Print the newest N videos (default 30)
  --channels                     List channels with fetch status
  --notes <video-id> <folder>    Save the video as a Markdown note with its transcript (Obsidian-ready)
Database: $SUBFEED_DB, or ~/.local/share/subfeed/subfeed.db";

const COMMANDS: &[&str] =
    &["--import", "--add", "--remove", "--refresh", "--list", "--channels", "--notes", "--install-timer", "--uninstall-timer", "--background", "--update-yt-dlp"];

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Called from C++ with the process arguments. Returns -1 when `argv` holds no headless
/// command, so the GUI should start; otherwise the process exit code.
///
/// # Safety
/// `argv` must point to `argc` valid NUL-terminated strings, as passed to `main`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn subfeed_cli_main(argc: c_int, argv: *const *const c_char) -> c_int {
    let args: Vec<String> = (1..argc.max(0) as usize)
        .map(|i| unsafe { CStr::from_ptr(*argv.add(i)) }.to_string_lossy().into_owned())
        .collect();
    if !args.first().is_some_and(|a| COMMANDS.contains(&a.as_str())) {
        return -1;
    }
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("subfeed: {e:#}");
            1
        }
    }
}

fn run(args: &[String]) -> Result<c_int> {
    let path = db::default_path();
    let db = Db::open(&path)?;
    let (cmd, rest) = args.split_first().expect("checked by caller");
    match cmd.as_str() {
        "--import" => {
            let [file] = rest else { bail!("usage: subfeed --import <subscriptions.csv>") };
            let (added, total) = takeout::import(&db, std::path::Path::new(file), now())?;
            println!("{added} new channels ({} already followed). Run `subfeed --refresh` to fetch videos.", total - added);
        }
        "--add" => {
            if rest.is_empty() {
                bail!("usage: subfeed --add <@handle | URL | UC…id>...");
            }
            let mut failed = false;
            for input in rest {
                match ytdlp::resolve_channel(input) {
                    Ok(ch) => {
                        let new = db.add_channel(&ch, now())?;
                        let name = if ch.title.is_empty() { &ch.id } else { &ch.title };
                        println!("{} {name} ({})", if new { "added" } else { "already following" }, ch.id);
                    }
                    Err(e) => {
                        eprintln!("{input}: {e:#}");
                        failed = true;
                    }
                }
            }
            return Ok(failed as c_int);
        }
        "--remove" => {
            for id in rest {
                println!("{id}: {}", if db.remove_channel(id)? { "removed" } else { "not followed" });
            }
        }
        "--refresh" => {
            let mut opts = Options::default();
            let mut notify_new = false;
            for flag in rest {
                match flag.as_str() {
                    "--force" => opts.force = true,
                    "--no-fallback" => opts.fallback = false,
                    "--notify" => notify_new = true,
                    other => bail!("unknown --refresh option {other}"),
                }
            }
            return refresh_command(db, &opts, notify_new);
        }
        "--list" => {
            let n = match rest {
                [] => 30,
                [n] => n.parse().context("--list takes a number")?,
                _ => bail!("usage: subfeed --list [N]"),
            };
            for v in db.feed(n, false)? {
                let when = chrono::DateTime::from_timestamp(v.published_at, 0)
                    .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                let when = if v.published_approx { format!("~{}", &when[..10]) } else { when };
                let dur = v.duration_s.map(|d| format!(" [{}:{:02}]", d / 60, d % 60)).unwrap_or_default();
                let mark = if v.watched { "✓" } else { " " };
                println!("{mark} {when:<16}  {:<20.20}  {}{dur}  https://youtu.be/{}", v.channel_title, v.title, v.id);
            }
        }
        "--notes" => {
            let [id, folder] = rest else { bail!("usage: subfeed --notes <video-id> <folder>") };
            let info = crate::stream::fetch_info(id)?;
            let lang = std::env::var("LANG").unwrap_or_default();
            let saved = crate::notes::save(&info, std::path::Path::new(folder), lang.split('.').next().unwrap_or(""))?;
            let what = if saved.existed { "Already saved" } else { "Saved" };
            println!("{what}: {}", saved.path.display());
        }
        "--channels" => {
            let now = now();
            for c in db.channels()? {
                let status = match (c.retry_after, c.last_fetch) {
                    (Some(t), _) if t > now => format!("backing off {}m: {}", (t - now) / 60, c.last_error.unwrap_or_default()),
                    (_, Some(t)) => format!("fetched {}m ago", (now - t) / 60),
                    (_, None) => "never fetched".into(),
                };
                println!("{}  {:<30.30}  {:>3} videos  {status}", c.id, c.title, c.video_count);
            }
        }
        "--background" => return background(),
        "--update-yt-dlp" => {
            if ytdlp::managed_path().is_none() {
                println!("yt-dlp belongs to the system here. {}", ytdlp::update_hint(ytdlp::install()));
                return Ok(0);
            }
            let latest = ytdlp::latest_release()?;
            ytdlp::install_release(&latest)?;
            println!("yt-dlp {latest} installed ({}).", ytdlp::binary().display());
        }
        // A systemd unit can't point into the Flatpak's sandbox; the Background portal can.
        "--install-timer" if in_flatpak() => background_on()?,
        "--uninstall-timer" if in_flatpak() => background_off()?,
        "--install-timer" => install_timer()?,
        "--uninstall-timer" => uninstall_timer()?,
        _ => unreachable!("checked by caller"),
    }
    Ok(0)
}

/// `--refresh`: fetch every due channel, then thumbnails, then (with `notify_new`) the
/// notification. Also what `--background` runs every half hour.
fn refresh_command(mut db: Db, opts: &Options, notify_new: bool) -> Result<c_int> {
    if notify_new && !opts.force && notify::metered() {
        println!("Skipped: the network connection is metered (use --force to refresh anyway).");
        return Ok(0);
    }
    let s = refresh::refresh(&mut db, &HttpSource::new(), opts, now())?;
    for (channel, err) in &s.errors {
        eprintln!("  {channel}: {err}");
    }
    if s.rss_outage {
        eprintln!("YouTube's RSS endpoint looks down; used yt-dlp instead (dates are approximate until it's back).");
    }
    if s.via_fallback > 0 || s.failed > 0 {
        eprintln!("(using {})", ytdlp::version_label());
    }
    println!(
        "{} new video{}. {} channels checked: {} updated, {} unchanged, {} via yt-dlp, {} failed; {} backing off.",
        s.new_videos,
        if s.new_videos == 1 { "" } else { "s" },
        s.due,
        s.updated,
        s.unchanged,
        s.via_fallback,
        s.failed,
        s.backing_off
    );
    if s.durations_filled > 0 {
        println!("Lengths filled in for {} videos.", s.durations_filled);
    }
    let targets = db.thumb_targets()?;
    let dir = thumbs::cache_dir();
    let keep = targets.iter().map(|(id, _)| id.clone()).collect();
    let pruned = thumbs::prune(&dir, &keep);
    let (got, failed) = thumbs::fetch_missing(&dir, targets, 6, &|_, _| {});
    if got + failed + pruned > 0 {
        println!("Thumbnails: {got} downloaded, {failed} failed, {pruned} removed.");
    }
    if notify_new
        && let Some(note) = notify::compose(&db.notify_candidates(&s.fresh)?, now())
    {
        drop(db); // don't hold the database while waiting for a click
        notify::show_and_wait(&note)?;
    }
    Ok((s.due > 0 && s.failed == s.due) as c_int)}

fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// The open window's D-Bus name (see cpp/appbus.h), and the background loop's.
const APP_NAME: &str = "io.github.ekelly95.Subfeed";
const BACKGROUND_NAME: &str = "io.github.ekelly95.Subfeed.Background";

/// Flatpak: ask to run at login, then start the background loop now.
pub(crate) fn background_on() -> Result<()> {
    if !notify::request_background(true)? {
        bail!("the desktop didn't allow subfeed to run in the background");
    }
    Db::open(&db::default_path())?.set_setting("background", "on")?;
    // In its own sandbox, so it runs on after this command (or the window) exits; at the next
    // login the portal's autostart takes over.
    notify::spawn_sandboxed(&["--background"])?;
    println!("subfeed now checks for new videos every 30 minutes in the background, from login on.");
    println!("Turn it off with: flatpak run io.github.ekelly95.Subfeed --uninstall-timer");
    Ok(())
}

fn background_off() -> Result<()> {
    Db::open(&db::default_path())?.set_setting("background", "off")?;
    let _ = notify::request_background(false);
    println!("Background refresh is off (a running one stops within 30 minutes).");
    Ok(())
}

/// Plugged in, or a machine without a battery.
fn on_ac() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else { return true };
    let read = |p: &std::path::Path, f: &str| std::fs::read_to_string(p.join(f)).unwrap_or_default().trim().to_string();
    let mains: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| read(p, "type") == "Mains").collect();
    mains.is_empty() || mains.iter().any(|p| read(p, "online") == "1")
}

/// `--background` (the Flatpak's stand-in for the systemd timer): every 30 minutes, when on AC
/// and the window isn't open (it refreshes itself), refresh with notifications and keep yt-dlp
/// current. Exits when turned off, or when another one is already running.
fn background() -> Result<c_int> {
    use zbus::fdo::{RequestNameFlags, RequestNameReply};
    let conn = zbus::blocking::Connection::session().context("connecting to the session bus")?;
    match conn.request_name_with_flags(BACKGROUND_NAME, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner) => {}
        _ => return Ok(0),
    }
    let bus = zbus::blocking::fdo::DBusProxy::new(&conn)?;
    std::thread::sleep(std::time::Duration::from_secs(60));
    loop {
        let db = Db::open(&db::default_path())?;
        if db.setting("background")?.as_deref() == Some("off") {
            return Ok(0);
        }
        let window_open = bus.name_has_owner(APP_NAME.try_into()?).unwrap_or(false);
        if on_ac() && !window_open {
            if let Err(e) = refresh_command(db, &Options::default(), true) {
                eprintln!("subfeed: background refresh: {e:#}");
            }
            if let Err(e) = ytdlp::auto_update(now(), false) {
                eprintln!("subfeed: updating yt-dlp: {e:#}");
            }
        } else {
            drop(db);
        }
        std::thread::sleep(std::time::Duration::from_secs(30 * 60));
    }
}

const UNIT: &str = "subfeed-refresh";

fn unit_dir() -> Result<std::path::PathBuf> {
    Ok(dirs::config_dir().context("no config directory")?.join("systemd/user"))
}

fn systemctl(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("systemctl").arg("--user").args(args).status().context("running systemctl")?;
    if !status.success() {
        bail!("systemctl --user {} failed", args.join(" "));
    }
    Ok(())
}

fn install_timer() -> Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let (service, timer) = notify::timer_units(&exe);
    let dir = unit_dir()?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{UNIT}.service")), service)?;
    std::fs::write(dir.join(format!("{UNIT}.timer")), timer)?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", &format!("{UNIT}.timer")])?;
    println!("Installed: subfeed checks for new videos every 30 minutes on AC power ({}).", dir.display());
    println!("Runs {}; reinstall if you move it. Remove with: subfeed --uninstall-timer", exe.display());
    Ok(())
}

fn uninstall_timer() -> Result<()> {
    let _ = systemctl(&["disable", "--now", &format!("{UNIT}.timer")]);
    let dir = unit_dir()?;
    for ext in ["service", "timer"] {
        let _ = std::fs::remove_file(dir.join(format!("{UNIT}.{ext}")));
    }
    systemctl(&["daemon-reload"])?;
    println!("Removed the background refresh timer.");
    Ok(())
}
