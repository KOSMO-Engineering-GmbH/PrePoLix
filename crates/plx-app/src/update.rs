//! Help > Check for Updates and the check at startup: asks GitHub for the latest release and,
//! where PrePoLix can replace itself, downloads and installs it. The Windows installer is run
//! with `/UPDATE`, which skips its pages, waits for PrePoLix to close and starts the new
//! version; the Linux AppImage is replaced in place and restarted. Other builds (from source,
//! a distribution package) only point to the release page.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use serde::Deserialize;
use sha2::Digest;

use crate::settings::Updates;

/// The latest release that is neither a draft nor a pre-release.
const LATEST_RELEASE: &str = "https://api.github.com/repos/KwentiN-ui/PrePoLix/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/KwentiN-ui/PrePoLix/releases/latest";

const CANCELLED: &str = "download cancelled";

/// A version number `major.minor.patch`, compared numerically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    /// `0.2.5` or a tag like `v0.2.5`; a suffix such as `-rc1` is ignored.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let core = text.split(['-', '+']).next()?;
        let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
        match parts?[..] {
            [major] => Some(Self(major, 0, 0)),
            [major, minor] => Some(Self(major, minor, 0)),
            [major, minor, patch] => Some(Self(major, minor, patch)),
            _ => None,
        }
    }

    /// The version of this program.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self(0, 0, 0))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// How this copy of PrePoLix was installed, which decides how it is updated.
#[derive(Clone, Debug, PartialEq)]
pub enum Installation {
    /// Installed by the Windows installer (`uninstall.exe` next to the program).
    WindowsInstaller,
    /// Started from this AppImage file.
    AppImage(PathBuf),
    /// Built from source or installed otherwise; updated by hand.
    Other,
}

impl Installation {
    pub fn detect() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        if cfg!(windows) && exe_dir.is_some_and(|dir| dir.join("uninstall.exe").is_file()) {
            return Self::WindowsInstaller;
        }
        // The AppImage runtime names the file it was started from.
        if cfg!(target_os = "linux")
            && let Some(file) = std::env::var_os("APPIMAGE").map(PathBuf::from)
            && file.is_file()
        {
            return Self::AppImage(file);
        }
        Self::Other
    }

    /// Which file of a release updates this installation.
    fn wants(&self, asset_name: &str) -> bool {
        match self {
            Self::WindowsInstaller => asset_name.ends_with("-setup.exe"),
            Self::AppImage(_) => asset_name.ends_with("-x86_64.AppImage"),
            Self::Other => false,
        }
    }

    /// One line for the settings window.
    pub fn description(&self) -> String {
        match self {
            Self::WindowsInstaller => {
                "Installed with the Windows installer: updates are downloaded and installed from \
                 within PrePoLix."
                    .into()
            }
            Self::AppImage(file) => format!(
                "Running from the AppImage {}: updates replace this file.",
                file.display()
            ),
            Self::Other => "Not installed with the Windows installer or the AppImage: new \
                            versions are announced, but must be installed by hand."
                .into(),
        }
    }
}

/// The latest release, as far as this installation is concerned.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    /// The release notes, Markdown as written on GitHub.
    pub notes: String,
    pub page: String,
    /// The file that updates this installation, if the release has one.
    pub asset: Option<Asset>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// SHA-256 of the file in hexadecimal, as GitHub reports it.
    pub sha256: Option<String>,
}

/// What GitHub's REST API returns for a release, as far as needed.
#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    /// `sha256:<hex>`, given for files uploaded since mid 2025.
    #[serde(default)]
    digest: Option<String>,
}

fn parse_release(json: &str, installation: &Installation) -> Result<Release, String> {
    let release: ApiRelease =
        serde_json::from_str(json).map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
    let version = Version::parse(&release.tag_name).ok_or_else(|| {
        format!(
            "the latest release has no version number: {}",
            release.tag_name
        )
    })?;
    let asset = (release.assets.into_iter())
        .find(|a| installation.wants(&a.name))
        .map(|a| Asset {
            sha256: (a.digest.as_deref())
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(str::to_ascii_lowercase),
            name: a.name,
            url: a.browser_download_url,
            size: a.size,
        });
    Ok(Release {
        version,
        notes: release.body.unwrap_or_default().replace("\r\n", "\n"),
        page: release.html_url,
        asset,
    })
}

fn agent(timeout: Option<Duration>) -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .timeout_global(timeout)
        .timeout_connect(Some(Duration::from_secs(20)))
        .user_agent(concat!("PrePoLix/", env!("CARGO_PKG_VERSION")))
        // See `get`.
        .max_redirects(0)
        // The certificates of the system, so that a company proxy that inspects TLS works.
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

/// A GET request that follows redirects itself: ureq reads the large redirect of a GitHub
/// download (a few kB of headers) only partly and then fails on its body.
fn get(
    agent: &ureq::Agent,
    url: &str,
    accept: &str,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let mut url = url.to_owned();
    for _ in 0..5 {
        let response = agent.get(&url).header("Accept", accept).call()?;
        let location = (response.status().is_redirection())
            .then(|| response.headers().get("location")?.to_str().ok())
            .flatten();
        match location {
            Some(location) => url = location.to_owned(),
            None => return Ok(response),
        }
    }
    Err(ureq::Error::TooManyRedirects)
}

/// Asks GitHub for the latest release.
fn fetch_latest(installation: &Installation) -> Result<Release, String> {
    let agent = agent(Some(Duration::from_secs(30)));
    let mut response = get(&agent, LATEST_RELEASE, "application/vnd.github+json")
        .map_err(|e| format!("GitHub not reachable: {e}"))?;
    let json = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("GitHub not reachable: {e}"))?;
    parse_release(&json, installation)
}

/// Downloads the asset to `dest`, through a `.part` file next to it, and checks its size and
/// checksum. `progress` counts the bytes received; `cancel` stops the download.
fn download(
    asset: &Asset,
    dest: &Path,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let part = dest.with_file_name(format!(
        ".{}.part",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ));
    let result = download_to(asset, &part, progress, cancel)
        .and_then(|()| std::fs::rename(&part, dest).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn download_to(
    asset: &Asset,
    part: &Path,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let response = get(&agent(None), &asset.url, "application/octet-stream")
        .map_err(|e| format!("download failed: {e}"))?;
    let mut reader = response.into_body().into_reader();
    let mut file =
        std::fs::File::create(part).map_err(|e| format!("cannot write {}: {e}", part.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    let mut received = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n])
            .map_err(|e| format!("cannot write {}: {e}", part.display()))?;
        received += n as u64;
        progress.store(received, Ordering::Relaxed);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    check_download(asset, received, &hex(&hasher.finalize()))
}

fn check_download(asset: &Asset, size: u64, sha256: &str) -> Result<(), String> {
    if size != asset.size {
        return Err(format!(
            "download incomplete: {size} of {} bytes received",
            asset.size
        ));
    }
    match &asset.sha256 {
        Some(expected) if expected != sha256 => {
            Err("the downloaded file does not match its checksum on GitHub".into())
        }
        _ => Ok(()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Starts the downloaded Windows installer with `/UPDATE`. It needs administrator rights, which
/// only ShellExecute asks for (CreateProcess fails), so PowerShell's `Start-Process -Verb RunAs`
/// starts it. Returns once the user allowed it in the UAC prompt, or with the error.
#[cfg(windows)]
fn start_installer(installer: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let path = installer.display().to_string().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         Start-Process -FilePath '{path}' -ArgumentList '/UPDATE' -Verb RunAs"
    );
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("cannot start PowerShell: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        // Declining the UAC prompt is the usual reason.
        if error.contains("canceled") || error.contains("cancelled") {
            Err("the installation was not allowed".into())
        } else {
            Err(format!("cannot start the installer: {}", error.trim()))
        }
    }
}

#[cfg(not(windows))]
fn start_installer(_installer: &Path) -> Result<(), String> {
    Err("the Windows installer only runs on Windows".into())
}

/// Puts the downloaded AppImage in place of the running one. Renaming over the file is safe:
/// the running program keeps the old file open until it exits.
#[cfg(unix)]
fn replace_appimage(new: &Path, target: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))
        .and_then(|()| std::fs::rename(new, target))
        .map_err(|e| format!("cannot replace {}: {e}", target.display()))
}

#[cfg(not(unix))]
fn replace_appimage(_new: &Path, _target: &Path) -> Result<(), String> {
    Err("AppImages only run on Linux".into())
}

/// Where the asset is downloaded to: the AppImage next to the one it replaces, so that it can
/// be renamed over it; the installer to the temporary directory.
fn download_path(installation: &Installation, asset: &Asset) -> Result<PathBuf, String> {
    match installation {
        Installation::AppImage(file) => Ok(file.with_file_name(format!(".{}.new", asset.name))),
        _ => {
            let dir = std::env::temp_dir().join("prepolix-update");
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
            Ok(dir.join(&asset.name))
        }
    }
}

/// Downloads and installs: replaces the AppImage, or starts the installer. On success the
/// program must close, and with an AppImage start the new file afterwards.
fn download_and_install(
    installation: &Installation,
    asset: &Asset,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let dest = download_path(installation, asset)?;
    download(asset, &dest, progress, cancel)?;
    match installation {
        Installation::WindowsInstaller => start_installer(&dest),
        Installation::AppImage(file) => replace_appimage(&dest, file).inspect_err(|_| {
            let _ = std::fs::remove_file(&dest);
        }),
        Installation::Other => Err("this installation cannot update itself".into()),
    }
}

enum State {
    Idle,
    Checking {
        manual: bool,
        receiver: Receiver<Result<Release, String>>,
    },
    /// A newer release, shown in the update window.
    Available(Release),
    /// Answer to Help > Check for Updates when there is nothing newer.
    UpToDate(Version),
    Failed(String),
    Installing {
        release: Release,
        progress: Arc<AtomicU64>,
        cancel: Arc<AtomicBool>,
        receiver: Receiver<Result<(), String>>,
    },
}

/// What a button of the update window asks for.
enum Next {
    Close,
    Install(Release),
}

/// The update check and its window.
pub struct Updater {
    installation: Installation,
    state: State,
    /// The AppImage to start once the program has closed.
    restart: Option<PathBuf>,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            installation: Installation::detect(),
            state: State::Idle,
            restart: None,
        }
    }
}

impl Updater {
    /// Whether a check or an installation is under way.
    pub fn busy(&self) -> bool {
        matches!(
            self.state,
            State::Checking { .. } | State::Installing { .. }
        )
    }

    /// Asks GitHub for the latest release in the background. `manual` for Help > Check for
    /// Updates, which also reports that there is nothing new or that the check failed.
    pub fn check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.busy() {
            return;
        }
        let (sender, receiver) = channel();
        let (installation, ctx) = (self.installation.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = sender.send(fetch_latest(&installation));
            ctx.request_repaint();
        });
        self.state = State::Checking { manual, receiver };
    }

    /// Starts the AppImage that replaced this one; called when the program exits.
    pub fn restart_if_updated(&mut self) {
        if let Some(file) = self.restart.take() {
            // The variables of the old AppImage's runtime; the new one sets its own.
            let started = std::process::Command::new(&file)
                .env_remove("APPDIR")
                .env_remove("APPIMAGE")
                .env_remove("ARGV0")
                .env_remove("OWD")
                .spawn();
            if let Err(error) = started {
                log::error!("cannot start {}: {error}", file.display());
            }
        }
    }

    /// Takes the answer of a running check or installation and shows the update window.
    /// Lines for the output pane are pushed to `output`.
    pub fn show(&mut self, ctx: &egui::Context, settings: &mut Updates, output: &mut Vec<String>) {
        self.poll(ctx, settings, output);
        let mut open = true;
        let mut next = None;
        let title = match &self.state {
            State::Idle | State::Checking { .. } => return,
            State::Available(_) | State::Installing { .. } => "Update Available",
            State::UpToDate(_) | State::Failed(_) => "Check for Updates",
        };
        egui::Window::new(title)
            .id(egui::Id::new("update window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| match &self.state {
                State::Available(release) => next = self.available_ui(ui, release, settings),
                State::Installing {
                    release,
                    progress,
                    cancel,
                    ..
                } => {
                    ui.label(format!("Downloading PrePoLix {} …", release.version));
                    let size = release.asset.as_ref().map_or(0, |a| a.size).max(1);
                    let done = progress.load(Ordering::Relaxed);
                    let fraction = (done as f32 / size as f32).clamp(0.0, 1.0);
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .text(format!("{:.1} of {:.1} MB", mb(done), mb(size)))
                            .desired_width(420.0),
                    );
                    if fraction >= 1.0 {
                        ui.label("Starting the installation …");
                    }
                    ui.add_space(4.0);
                    if ui.button("Cancel").clicked() {
                        cancel.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
                State::UpToDate(version) => {
                    ui.label(format!("PrePoLix {version} is the latest version."));
                    ui.add_space(4.0);
                    if ui.button("OK").clicked() {
                        next = Some(Next::Close);
                    }
                }
                State::Failed(error) => {
                    ui.add(egui::Label::new(error.as_str()).wrap());
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            next = Some(Next::Close);
                        }
                        if ui.button("Open Release Page").clicked() {
                            open_page(RELEASES_PAGE, output);
                            next = Some(Next::Close);
                        }
                    });
                }
                State::Idle | State::Checking { .. } => {}
            });
        match next {
            Some(Next::Close) => self.state = State::Idle,
            Some(Next::Install(release)) => self.install(release, ctx),
            None => {}
        }
        if !open {
            if let State::Installing { cancel, .. } = &self.state {
                cancel.store(true, Ordering::Relaxed);
            } else {
                self.state = State::Idle;
            }
        }
    }

    /// The window of a newer release; returns the next state for a pressed button.
    fn available_ui(
        &self,
        ui: &mut egui::Ui,
        release: &Release,
        settings: &mut Updates,
    ) -> Option<Next> {
        let mut next = None;
        ui.label(
            egui::RichText::new(format!("PrePoLix {} is available.", release.version)).strong(),
        );
        ui.label(format!("You are using version {}.", Version::current()));
        if !release.notes.trim().is_empty() {
            ui.add_space(4.0);
            ui.label("What's new:");
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, crate::style::BORDER))
                .inner_margin(4)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(release.notes.trim()).wrap());
                        });
                });
        }
        ui.add_space(4.0);
        let installable = release.asset.is_some();
        match (&self.installation, installable) {
            (Installation::WindowsInstaller, true) => {
                ui.add(
                    egui::Label::new(
                        "PrePoLix closes to install the update and starts again afterwards. \
                         Save your project first. Windows asks for administrator rights.",
                    )
                    .wrap(),
                );
            }
            (Installation::AppImage(_), true) => {
                ui.add(
                    egui::Label::new(
                        "The AppImage file is replaced and PrePoLix starts again. Save your \
                         project first.",
                    )
                    .wrap(),
                );
            }
            (Installation::Other, _) => {
                ui.add(
                    egui::Label::new(
                        "This copy of PrePoLix cannot update itself. Download the new version \
                         from the release page.",
                    )
                    .wrap(),
                );
            }
            (_, false) => {
                ui.add(
                    egui::Label::new(
                        "The release has no file for this installation. Download it from the \
                         release page.",
                    )
                    .wrap(),
                );
            }
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if installable && ui.button("Update Now").clicked() {
                next = Some(Next::Install(release.clone()));
            }
            if ui.button("Open Release Page").clicked() {
                let _ = open::that_detached(&release.page);
                next = Some(Next::Close);
            }
            if ui
                .button("Skip This Version")
                .on_hover_text("No reminder at startup until a newer version is released.")
                .clicked()
            {
                settings.skipped = release.version.to_string();
                next = Some(Next::Close);
            }
            if ui.button("Later").clicked() {
                next = Some(Next::Close);
            }
        });
        next
    }

    /// Downloads and installs the release in the background.
    fn install(&mut self, release: Release, ctx: &egui::Context) {
        let (progress, cancel) = (Arc::<AtomicU64>::default(), Arc::<AtomicBool>::default());
        let Some(asset) = release.asset.clone() else {
            return;
        };
        let (sender, receiver) = channel();
        let (installation, ctx) = (self.installation.clone(), ctx.clone());
        let (thread_progress, thread_cancel) = (progress.clone(), cancel.clone());
        std::thread::spawn(move || {
            let result =
                download_and_install(&installation, &asset, &thread_progress, &thread_cancel);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        self.state = State::Installing {
            release,
            progress,
            cancel,
            receiver,
        };
    }

    fn poll(&mut self, ctx: &egui::Context, settings: &Updates, output: &mut Vec<String>) {
        match &self.state {
            State::Checking { manual, receiver } => {
                let Ok(result) = receiver.try_recv() else {
                    return;
                };
                let manual = *manual;
                self.state = match result {
                    Ok(release) if release.version > Version::current() => {
                        output.push(format!("PrePoLix {} is available", release.version));
                        // The check at startup does not remind of a skipped version.
                        if manual || settings.skipped != release.version.to_string() {
                            State::Available(release)
                        } else {
                            State::Idle
                        }
                    }
                    Ok(_) if manual => State::UpToDate(Version::current()),
                    Ok(_) => State::Idle,
                    Err(error) if manual => {
                        State::Failed(format!("No update information: {error}."))
                    }
                    Err(error) => {
                        log::info!("update check: {error}");
                        State::Idle
                    }
                };
            }
            State::Installing {
                release, receiver, ..
            } => {
                let Ok(result) = receiver.try_recv() else {
                    return;
                };
                match result {
                    Ok(()) => {
                        output.push(format!("Installing PrePoLix {}", release.version));
                        if let Installation::AppImage(file) = &self.installation {
                            self.restart = Some(file.clone());
                        }
                        self.state = State::Idle;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        output.push(format!("Update not installed: {error}"));
                        self.state = if error == CANCELLED {
                            State::Idle
                        } else {
                            State::Failed(format!("The update was not installed: {error}."))
                        };
                    }
                }
            }
            _ => {}
        }
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1e6
}

fn open_page(url: &str, output: &mut Vec<String>) {
    if let Err(error) = open::that_detached(url) {
        output.push(format!("Error opening {url} in the browser: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(Version::parse("v0.2.5"), Some(Version(0, 2, 5)));
        assert_eq!(Version::parse("0.10.0"), Some(Version(0, 10, 0)));
        assert_eq!(Version::parse("v1.0"), Some(Version(1, 0, 0)));
        assert_eq!(Version::parse("v0.3.0-rc1"), Some(Version(0, 3, 0)));
        assert_eq!(Version::parse("nightly"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert_eq!(Version::parse("1.x"), None);
        assert!(Version::parse("v0.10.0") > Version::parse("v0.9.9"));
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    const RELEASE: &str = r###"{
        "tag_name": "v0.3.0",
        "html_url": "https://github.com/KwentiN-ui/PrePoLix/releases/tag/v0.3.0",
        "body": "## What's Changed\r\n* Self update",
        "assets": [
            {"name": "prepolix-0.3.0-user-guide.pdf", "size": 10,
             "browser_download_url": "https://example.com/guide.pdf"},
            {"name": "prepolix-0.3.0-setup.exe", "size": 1234,
             "browser_download_url": "https://example.com/setup.exe",
             "digest": "sha256:ABCDEF"},
            {"name": "prepolix-0.3.0-x86_64.AppImage.zsync", "size": 5,
             "browser_download_url": "https://example.com/a.zsync", "digest": null},
            {"name": "prepolix-0.3.0-x86_64.AppImage", "size": 99,
             "browser_download_url": "https://example.com/a.AppImage"}
        ]
    }"###;

    #[test]
    fn each_installation_takes_its_file() {
        let windows = parse_release(RELEASE, &Installation::WindowsInstaller).unwrap();
        assert_eq!(windows.version, Version(0, 3, 0));
        assert_eq!(windows.notes, "## What's Changed\n* Self update");
        let asset = windows.asset.unwrap();
        assert_eq!(asset.name, "prepolix-0.3.0-setup.exe");
        assert_eq!(asset.size, 1234);
        assert_eq!(asset.sha256.as_deref(), Some("abcdef"));
        let appimage = parse_release(RELEASE, &Installation::AppImage("/x".into())).unwrap();
        let asset = appimage.asset.unwrap();
        assert_eq!(asset.name, "prepolix-0.3.0-x86_64.AppImage");
        assert_eq!(asset.sha256, None);
        let other = parse_release(RELEASE, &Installation::Other).unwrap();
        assert_eq!(other.asset, None);
        assert!(parse_release("{}", &Installation::Other).is_err());
    }

    /// Asks GitHub; run by hand with `cargo test -p plx-app -- --ignored latest_release`.
    #[test]
    #[ignore = "needs the internet"]
    fn latest_release_is_found() {
        let release = fetch_latest(&Installation::WindowsInstaller).unwrap();
        println!("{release:#?}");
        assert!(release.version >= Version(0, 2, 0));
        // The installer's download, with its size and checksum checked.
        let asset = release.asset.unwrap();
        let dest = std::env::temp_dir().join(format!("plx-update-{}", asset.name));
        let progress = AtomicU64::new(0);
        download(&asset, &dest, &progress, &AtomicBool::new(false)).unwrap();
        assert_eq!(progress.into_inner(), asset.size);
        std::fs::remove_file(dest).unwrap();
    }

    #[test]
    fn downloads_are_checked() {
        let asset = Asset {
            name: "a".into(),
            url: String::new(),
            size: 3,
            sha256: Some(hex(&sha2::Sha256::digest(b"abc"))),
        };
        let sha = hex(&sha2::Sha256::digest(b"abc"));
        assert_eq!(
            sha,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(check_download(&asset, 3, &sha).is_ok());
        assert!(check_download(&asset, 2, &sha).is_err());
        assert!(check_download(&asset, 3, &hex(&sha2::Sha256::digest(b"abd"))).is_err());
        let unknown = Asset {
            sha256: None,
            ..asset
        };
        assert!(check_download(&unknown, 3, "anything").is_ok());
    }

    #[test]
    fn appimage_is_downloaded_next_to_itself() {
        let asset = Asset {
            name: "prepolix-0.3.0-x86_64.AppImage".into(),
            url: String::new(),
            size: 0,
            sha256: None,
        };
        let installation = Installation::AppImage("/home/u/Apps/prepolix.AppImage".into());
        assert_eq!(
            download_path(&installation, &asset).unwrap(),
            Path::new("/home/u/Apps/.prepolix-0.3.0-x86_64.AppImage.new")
        );
    }

    #[cfg(unix)]
    #[test]
    fn appimage_is_replaced_in_place() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("plx-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (old, new) = (dir.join("prepolix.AppImage"), dir.join(".new"));
        std::fs::write(&old, "old").unwrap();
        std::fs::write(&new, "new").unwrap();
        replace_appimage(&new, &old).unwrap();
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "new");
        assert!(!new.exists());
        let mode = std::fs::metadata(&old).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
