//! Download and install a published release on the running platform.
//!
//! The release check only reads metadata. This module is the only place that
//! writes application files, and it first verifies the installer against the
//! SHA-256 checksum published beside it. Both URLs are assembled from the fixed
//! repository, a validated release tag, and asset names Breeze owns; no download
//! target comes from the response body.
//!
//! Installers are unsigned development packages. The checksum detects damaged or
//! mismatched downloads and locks the update to the release assets; it does not
//! establish publisher identity, so a compromised GitHub release would defeat it.

use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// Published download root; `/tag/asset` is appended for a validated tag.
pub(super) const DOWNLOAD_ROOT: &str = "https://github.com/poneding/breeze/releases/download";
/// Installers are a few hundred MiB at most; anything larger is refused before
/// it can fill the user's disk.
const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 4096;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const BODY_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const READ_CHUNK: usize = 64 * 1024;

/// One installer asset and the checksum file published beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InstallPlan {
    pub(super) asset: String,
    pub(super) checksum: String,
}

/// How this installation was started, which decides where an update can go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InstallTarget {
    /// A macOS application bundle; the image is mounted and the bundle replaced.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    MacBundle(PathBuf),
    /// A Windows installation; the setup package runs after Breeze exits.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    WindowsSetup(WindowsPackage),
    /// A running AppImage; the image file is replaced in place.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    AppImage(PathBuf),
    /// A package-manager installation that needs its own tooling.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    DebianPackage,
    /// A development binary or an architecture without a published package.
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(super) enum WindowsPackage {
    Inno,
    Msi,
}

impl InstallTarget {
    /// Detect how the running executable was installed. Detection is repeated
    /// when the reader asks to install, so a moved or rebuilt binary cannot
    /// silently install over the wrong path.
    pub(super) fn detect() -> Self {
        #[cfg(target_os = "macos")]
        if let Some(bundle) = macos_bundle() {
            return Self::MacBundle(bundle);
        }
        #[cfg(target_os = "linux")]
        {
            if let Some(path) = appimage_path() {
                return Self::AppImage(path);
            }
            if Path::new("/usr/share/doc/breeze/VERSION").is_file() {
                return Self::DebianPackage;
            }
        }
        #[cfg(target_os = "windows")]
        if let Some(package) = windows_package() {
            return Self::WindowsSetup(package);
        }
        Self::Unsupported
    }

    /// The asset this installation needs for `version`, or `None` when the
    /// platform has no matching published package.
    pub(super) fn plan(&self, version: &str) -> Option<InstallPlan> {
        let (target, extension) = match self {
            Self::MacBundle(_) => (macos_triple()?, "dmg"),
            Self::WindowsSetup(WindowsPackage::Inno) => ("x86_64-pc-windows-msvc", "exe"),
            Self::WindowsSetup(WindowsPackage::Msi) => ("x86_64-pc-windows-msvc", "msi"),
            Self::AppImage(_) => ("x86_64-unknown-linux-gnu", "AppImage"),
            Self::DebianPackage => ("x86_64-unknown-linux-gnu", "deb"),
            Self::Unsupported => return None,
        };
        let asset = format!("Breeze-{version}-{target}.{extension}");
        let checksum = format!("{asset}.sha256");
        Some(InstallPlan { asset, checksum })
    }
}

fn macos_triple() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "aarch64" => Some("aarch64-apple-darwin"),
        "x86_64" => Some("x86_64-apple-darwin"),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn macos_bundle() -> Option<PathBuf> {
    let executable = fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    // <bundle>.app/Contents/MacOS/<executable>
    let contents = executable.parent()?.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = bundle.extension().is_some_and(|name| name == "app")
        && contents.join("Info.plist").is_file();
    is_bundle.then(|| bundle.to_path_buf())
}

#[cfg(target_os = "linux")]
fn appimage_path() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("APPIMAGE")?);
    path.is_file()
        .then(|| fs::canonicalize(&path).unwrap_or(path))
}

#[cfg(target_os = "windows")]
fn windows_package() -> Option<WindowsPackage> {
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    if directory.join("unins000.exe").is_file() {
        Some(WindowsPackage::Inno)
    } else if directory.join("VERSION").is_file() {
        // The MSI payload carries VERSION but no uninstaller beside the binary.
        Some(WindowsPackage::Msi)
    } else {
        None
    }
}

/// Download progress in bytes. The UI reads it from the render thread while the
/// background job advances it.
#[derive(Default)]
pub(super) struct DownloadProgress {
    received: AtomicU64,
    total: AtomicU64,
}

impl DownloadProgress {
    pub(super) fn percent(&self) -> Option<u8> {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return None;
        }
        let received = self.received.load(Ordering::Relaxed);
        Some((received.saturating_mul(100) / total).min(100) as u8)
    }

    pub(super) fn reset(&self, total: u64) {
        self.received.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    fn add(&self, bytes: u64) {
        self.received.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InstallError {
    /// No transport at all, with the operating system's description.
    Network(String),
    Timeout,
    Http(u16),
    TooLarge,
    ChecksumMissing,
    ChecksumInvalid,
    ChecksumMismatch,
    /// A local file operation failed, with the operating system's description.
    Io(String),
    VersionMismatch {
        found: String,
        expected: String,
    },
    MissingApplication,
    /// A platform installer failed, with its bounded diagnostic.
    InstallFailed(String),
    /// The replacement could not be started after the close.
    LaunchFailed(String),
    Unsupported,
}

/// What a completed install leaves for the reader.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Completion {
    /// Application files are already replaced; relaunch them after a restart.
    Installed { path: PathBuf, launch: LaunchKind },
    /// The setup package is staged; the restart runs it after Breeze exits.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Installer { path: PathBuf, version: String },
    /// A package-manager file was downloaded for the reader to install.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Manual { path: PathBuf },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LaunchKind {
    /// A macOS `.app` bundle, opened through Launch Services.
    Bundle,
    /// An AppImage or other executable file.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Executable,
}

/// The action a restart performs for the installed update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RestartAction {
    Relaunch {
        path: PathBuf,
        kind: LaunchKind,
    },
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    RunInstaller {
        path: PathBuf,
    },
}

impl RestartAction {
    pub(super) fn launch(&self) -> Result<(), InstallError> {
        match self {
            Self::Relaunch { path, kind } => launch_after_exit(path, *kind),
            Self::RunInstaller { path } => run_installer(path),
        }
    }
}

/// Start the new application only after this process has exited, so it can
/// acquire the state-store lock and restore the saved session.
///
/// The helper is a detached shell: it polls the old process id, then replaces
/// itself with the real launcher. Both the id and the path are positional
/// arguments, never interpolated into the script.
#[cfg(unix)]
fn launch_after_exit(path: &Path, kind: LaunchKind) -> Result<(), InstallError> {
    match kind {
        LaunchKind::Bundle => {
            if !path.join("Contents/MacOS/Breeze").is_file() {
                return Err(InstallError::LaunchFailed(format!(
                    "{} is not an application bundle",
                    path.display()
                )));
            }
        }
        LaunchKind::Executable => {
            if !path.is_file() {
                return Err(InstallError::LaunchFailed(format!(
                    "{} is missing",
                    path.display()
                )));
            }
        }
    }
    let script = match kind {
        LaunchKind::Bundle => {
            "while kill -0 \"$1\" 2>/dev/null; do sleep 0.1; done; exec open -n \"$2\""
        }
        LaunchKind::Executable => {
            "while kill -0 \"$1\" 2>/dev/null; do sleep 0.1; done; exec \"$2\""
        }
    };
    Command::new("sh")
        .args(["-c", script, "breeze-update"])
        .arg(std::process::id().to_string())
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| InstallError::LaunchFailed(error.to_string()))
}

#[cfg(not(unix))]
fn launch_after_exit(path: &Path, kind: LaunchKind) -> Result<(), InstallError> {
    let mut command = match kind {
        LaunchKind::Bundle => {
            let mut command = Command::new("open");
            command.arg("-n").arg(path);
            command
        }
        LaunchKind::Executable => Command::new(path),
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| InstallError::LaunchFailed(error.to_string()))
}

#[cfg(target_os = "windows")]
fn run_installer(path: &Path) -> Result<(), InstallError> {
    // Breeze exits as this callback returns, so the setup program finds no image
    // in use. A silent install still shows UAC and its own progress window.
    let mut command = if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("msi"))
    {
        let mut command = Command::new("msiexec");
        command.arg("/i").arg(path).args(["/qb", "/norestart"]);
        command
    } else {
        let mut command = Command::new(path);
        command.args(["/SILENT", "/CLOSEAPPLICATIONS", "/NORESTART"]);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| InstallError::LaunchFailed(error.to_string()))
}

#[cfg(not(target_os = "windows"))]
fn run_installer(_: &Path) -> Result<(), InstallError> {
    Err(InstallError::Unsupported)
}

/// Download and verify the installer for `plan`.
///
/// The artifact lands in `<data>/updates` so a Windows or package-manager update
/// survives the restart prompt. Earlier files in that directory are Breeze's own
/// downloads and are discarded first.
pub(super) fn download(
    tag: &str,
    plan: &InstallPlan,
    progress: &DownloadProgress,
) -> Result<PathBuf, InstallError> {
    let directory = update_directory()?;
    download_into(
        &download_agent(),
        DOWNLOAD_ROOT,
        tag,
        plan,
        &directory,
        progress,
    )
}

/// Shared download path. Tests point it at a loopback fixture; production always
/// passes the fixed GitHub release root.
pub(super) fn download_into(
    agent: &ureq::Agent,
    base: &str,
    tag: &str,
    plan: &InstallPlan,
    directory: &Path,
    progress: &DownloadProgress,
) -> Result<PathBuf, InstallError> {
    let artifact = directory.join(&plan.asset);
    let checksum_url = format!("{base}/{tag}/{}", plan.checksum);
    let artifact_url = format!("{base}/{tag}/{}", plan.asset);
    let expected = download_checksum(agent, &checksum_url)?;
    stream_verified(agent, &artifact_url, &artifact, &expected, progress)?;
    Ok(artifact)
}

fn update_directory() -> Result<PathBuf, InstallError> {
    let root =
        breeze_storage::data_directory().map_err(|error| InstallError::Io(format!("{error:#}")))?;
    let directory = root.join("updates");
    fs::create_dir_all(&directory).map_err(|error| io_failure(&error))?;
    // Only Breeze writes here; stale setup files must not be presented as the
    // current download.
    for entry in fs::read_dir(&directory).map_err(|error| io_failure(&error))? {
        let entry = entry.map_err(|error| io_failure(&error))?;
        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(directory)
}

fn download_agent() -> ureq::Agent {
    download_client(true)
}

pub(super) fn download_client(https_only: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .https_only(https_only)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(BODY_TIMEOUT))
        .http_status_as_error(false)
        .user_agent(concat!("Breeze/", env!("CARGO_PKG_VERSION")));
    // Loopback fixtures must not use a developer machine's proxy settings.
    #[cfg(test)]
    let config = config.proxy(None);
    config.build().into()
}

fn download_checksum(agent: &ureq::Agent, url: &str) -> Result<String, InstallError> {
    let mut response = agent.get(url).call().map_err(transport)?;
    match response.status().as_u16() {
        200 => {}
        404 => return Err(InstallError::ChecksumMissing),
        status => return Err(InstallError::Http(status)),
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_CHECKSUM_BYTES)
        .read_to_vec()
        .map_err(transport)?;
    parse_checksum(&body)
}

/// The sidecar is `<sha256>  <filename>\n`. Only the digest is used; the file
/// name is advisory and the URL already selects the asset.
pub(super) fn parse_checksum(body: &[u8]) -> Result<String, InstallError> {
    let text = std::str::from_utf8(body).map_err(|_| InstallError::ChecksumInvalid)?;
    let digest = text
        .split_whitespace()
        .next()
        .ok_or(InstallError::ChecksumInvalid)?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InstallError::ChecksumInvalid);
    }
    Ok(digest.to_ascii_lowercase())
}

fn stream_verified(
    agent: &ureq::Agent,
    url: &str,
    destination: &Path,
    expected: &str,
    progress: &DownloadProgress,
) -> Result<(), InstallError> {
    let mut response = agent.get(url).call().map_err(transport)?;
    match response.status().as_u16() {
        200 => {}
        status => return Err(InstallError::Http(status)),
    }
    let announced = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if announced > MAX_ARTIFACT_BYTES {
        return Err(InstallError::TooLarge);
    }
    progress.reset(announced);
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_ARTIFACT_BYTES)
        .reader();
    let mut file = File::create(destination).map_err(|error| io_failure(&error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; READ_CHUNK];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| io_failure(&error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| io_failure(&error))?;
        progress.add(read as u64);
    }
    file.sync_all().map_err(|error| io_failure(&error))?;
    drop(file);
    let found: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if found != expected {
        let _ = fs::remove_file(destination);
        return Err(InstallError::ChecksumMismatch);
    }
    Ok(())
}

/// Install a verified artifact for the detected target.
pub(super) fn install(
    target: &InstallTarget,
    artifact: &Path,
    version: &str,
) -> Result<Completion, InstallError> {
    match target {
        #[cfg(target_os = "macos")]
        InstallTarget::MacBundle(bundle) => install_macos(artifact, version, bundle),
        #[cfg(target_os = "linux")]
        InstallTarget::AppImage(appimage) => install_appimage(artifact, appimage),
        #[cfg(target_os = "linux")]
        InstallTarget::DebianPackage => Ok(Completion::Manual {
            path: artifact.to_path_buf(),
        }),
        #[cfg(target_os = "windows")]
        InstallTarget::WindowsSetup(_) => Ok(Completion::Installer {
            path: artifact.to_path_buf(),
            version: version.to_owned(),
        }),
        _ => Err(InstallError::Unsupported),
    }
}

#[cfg(target_os = "macos")]
fn install_macos(dmg: &Path, version: &str, bundle: &Path) -> Result<Completion, InstallError> {
    let mountpoint = std::env::temp_dir().join(format!(
        "breeze-update-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0)
    ));
    if mountpoint.exists() {
        fs::remove_dir_all(&mountpoint).map_err(|error| io_failure(&error))?;
    }
    fs::create_dir_all(&mountpoint).map_err(|error| io_failure(&error))?;
    let attached = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mountpoint)
        .arg(dmg)
        .output()
        .map_err(|error| io_failure(&error))?;
    if !attached.status.success() {
        let _ = fs::remove_dir_all(&mountpoint);
        return Err(InstallError::InstallFailed(diagnostic(&attached.stderr)));
    }
    let _detach = Detach(mountpoint.clone());

    let mounted_app = mountpoint.join("Breeze.app");
    verify_bundle(&mounted_app, version)?;
    let parent = bundle.parent().ok_or(InstallError::MissingApplication)?;
    let staged = parent.join(format!(".Breeze-update-{}", std::process::id()));
    let backup = parent.join(format!(".Breeze-backup-{}", std::process::id()));
    remove_tree(&staged);
    remove_tree(&backup);
    let copied = Command::new("ditto")
        .arg(&mounted_app)
        .arg(&staged)
        .output()
        .map_err(|error| io_failure(&error))?;
    if !copied.status.success() {
        remove_tree(&staged);
        return Err(InstallError::InstallFailed(diagnostic(&copied.stderr)));
    }
    // Keep the old bundle until the replacement is in place; restoring it on
    // failure leaves a launchable application even when the update is bad.
    fs::rename(bundle, &backup).map_err(|error| {
        remove_tree(&staged);
        io_failure(&error)
    })?;
    if let Err(error) = fs::rename(&staged, bundle) {
        let _ = fs::rename(&backup, bundle);
        remove_tree(&staged);
        return Err(io_failure(&error));
    }
    remove_tree(&backup);
    let _ = fs::remove_file(dmg);
    Ok(Completion::Installed {
        path: bundle.to_path_buf(),
        launch: LaunchKind::Bundle,
    })
}

#[cfg(target_os = "macos")]
fn verify_bundle(app: &Path, version: &str) -> Result<(), InstallError> {
    if !app.join("Contents/MacOS/Breeze").is_file() {
        return Err(InstallError::MissingApplication);
    }
    let file = app.join("Contents/Resources/VERSION");
    let found = fs::read_to_string(&file).map_err(|_| InstallError::MissingApplication)?;
    let found = found.trim();
    if found != version {
        return Err(InstallError::VersionMismatch {
            found: found.to_owned(),
            expected: version.to_owned(),
        });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
struct Detach(PathBuf);

#[cfg(target_os = "macos")]
impl Drop for Detach {
    fn drop(&mut self) {
        let _ = Command::new("hdiutil").arg("detach").arg(&self.0).output();
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(target_os = "linux")]
fn install_appimage(artifact: &Path, appimage: &Path) -> Result<Completion, InstallError> {
    use std::os::unix::fs::PermissionsExt as _;

    let parent = appimage.parent().ok_or(InstallError::MissingApplication)?;
    let name = appimage
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(InstallError::MissingApplication)?;
    let staged = parent.join(format!(".{name}.new-{}", std::process::id()));
    let backup = parent.join(format!(".{name}.old-{}", std::process::id()));
    remove_file(&staged);
    remove_file(&backup);
    fs::copy(artifact, &staged).map_err(|error| io_failure(&error))?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
        .map_err(|error| io_failure(&error))?;
    fs::rename(appimage, &backup).map_err(|error| {
        remove_file(&staged);
        io_failure(&error)
    })?;
    if let Err(error) = fs::rename(&staged, appimage) {
        let _ = fs::rename(&backup, appimage);
        remove_file(&staged);
        return Err(io_failure(&error));
    }
    remove_file(&backup);
    let _ = fs::remove_file(artifact);
    Ok(Completion::Installed {
        path: appimage.to_path_buf(),
        launch: LaunchKind::Executable,
    })
}

fn remove_tree(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn remove_file(path: &Path) {
    let _ = fs::remove_file(path);
}

fn transport(error: ureq::Error) -> InstallError {
    match error {
        ureq::Error::Timeout(_) => InstallError::Timeout,
        ureq::Error::BodyExceedsLimit(_) => InstallError::TooLarge,
        _ => InstallError::Network(error.to_string()),
    }
}

fn io_failure(error: &io::Error) -> InstallError {
    InstallError::Io(error.to_string())
}

/// Bounded first line of a tool's diagnostic; the installer's output is more
/// useful than its exit status but must not flood the banner.
fn diagnostic(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let detail = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("the system installer reported no detail");
    detail.chars().take(300).collect()
}
