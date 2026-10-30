//! Release discovery and in-app installation.
//!
//! A check reads only metadata from the fixed repository and never acts on a
//! server-supplied URL. Installing always downloads the platform package from
//! that repository, verifies its published SHA-256 checksum, and only then
//! replaces application files. macOS and AppImage updates finish in place and
//! prompt for a restart; Windows prepares the setup package and runs it after
//! Breeze exits; package-manager installs keep the downloaded file for the
//! system tool that owns them.
use crate::*;
use gpui_kit::component::{Icon, WindowExt as _};
use install::{
    Completion, DownloadProgress, InstallError, InstallPlan, InstallTarget, RestartAction,
};
use semver::Version;
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc, time::Duration};

mod install;

/// User-visible project address, shared with the About page.
pub(super) const REPOSITORY: &str = "https://github.com/poneding/breeze";
const ENDPOINT: &str = "https://api.github.com/repos/poneding/breeze/releases/latest";
const RELEASES: &str = "https://github.com/poneding/breeze/releases";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
/// How often the download banner refreshes its byte count while a job runs.
const PROGRESS_TICK: Duration = Duration::from_millis(200);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum CheckStatus {
    #[default]
    Idle,
    Checking,
    Unpublished,
    Current {
        latest: String,
    },
    Available {
        version: String,
        tag: String,
        url: String,
        /// `None` when this release has no package for the running system.
        install: Option<InstallPlan>,
    },
    Failed(CheckError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CheckError {
    Timeout,
    Network(String),
    TooLarge,
    InvalidMetadata,
    RateLimited,
    Http(u16),
}

/// What the install half of this feature is doing. A check result stays valid
/// alongside it, so the About page can still report the release version.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Downloading {
        version: String,
    },
    Installing {
        version: String,
    },
    /// Application files are replaced or the setup package is staged.
    Ready {
        version: String,
        action: RestartAction,
    },
    /// The system package manager must install the downloaded file.
    Manual {
        version: String,
        path: PathBuf,
    },
    Failed {
        version: String,
        error: InstallError,
    },
}

impl Phase {
    fn version(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Downloading { version }
            | Self::Installing { version }
            | Self::Ready { version, .. }
            | Self::Manual { version, .. }
            | Self::Failed { version, .. } => Some(version),
        }
    }

    fn busy(&self) -> bool {
        matches!(self, Self::Downloading { .. } | Self::Installing { .. })
    }

    fn restart_action(&self) -> Option<(&str, &RestartAction)> {
        match self {
            Self::Ready { version, action } => Some((version, action)),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(super) struct Updates {
    status: CheckStatus,
    phase: Phase,
    in_flight: bool,
    auto_checked: bool,
    banner_dismissed: bool,
    progress: Arc<DownloadProgress>,
    /// The restart dialog was shown for this install; a later dismissal only
    /// leaves the banner entry instead of reopening the modal on every render.
    restart_prompted: bool,
    /// A restart was requested and is waiting for state to finish saving.
    restart_pending: bool,
    restart_error: Option<String>,
}

impl Updates {
    fn begin(&mut self, automatic: bool, policy: usize) -> bool {
        if self.in_flight || (automatic && (policy != 1 || self.auto_checked)) {
            return false;
        }
        if automatic {
            self.auto_checked = true;
        }
        self.in_flight = true;
        self.banner_dismissed = false;
        self.status = CheckStatus::Checking;
        true
    }

    /// Current check or install state, in the active language. States that carry
    /// no news for the reader render nothing instead of filler text.
    pub(super) fn message(&self) -> Option<String> {
        if let Some(text) = self.phase_message() {
            return Some(text);
        }
        match &self.status {
            CheckStatus::Available {
                version,
                install: None,
                ..
            } => Some(tf!(
                "New version {} available · no package for this system",
                version
            )),
            _ => self.status.message(),
        }
    }

    fn phase_message(&self) -> Option<String> {
        match &self.phase {
            Phase::Idle => None,
            Phase::Downloading { .. } => Some(match self.progress.percent() {
                Some(percent) => tf!("Downloading update… {}%", percent),
                None => tr!("Downloading update…").into(),
            }),
            Phase::Installing { version } => Some(tf!("Installing update {}…", version)),
            Phase::Ready { version, action } => Some(match action {
                RestartAction::RunInstaller { .. } => {
                    tf!("Breeze {} downloaded · restart to install", version)
                }
                RestartAction::Relaunch { .. } => {
                    tf!("Breeze {} installed · restart to finish", version)
                }
            }),
            Phase::Manual { path, .. } => Some(tf!(
                "Downloaded {} · install it with your package manager",
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            )),
            Phase::Failed { error, .. } => Some(tf!("Update failed: {}", error.message())),
        }
    }

    fn finish(&mut self, automatic: bool, policy: usize, result: CheckStatus) {
        self.in_flight = false;
        // A disabled automatic check may finish its bounded request, but must not
        // publish a stale notification or start a replacement request.
        if automatic && policy != 1 {
            self.status = CheckStatus::Idle;
            return;
        }
        // A different release starts a fresh install flow; an unchanged one keeps
        // a pending restart or a readable failure visible.
        let offered = match &result {
            CheckStatus::Available { version, .. } => Some(version.as_str()),
            _ => None,
        };
        if let Some(version) = offered
            && self.phase.version() != Some(version)
        {
            self.phase = Phase::Idle;
            self.restart_prompted = false;
            self.restart_error = None;
        }
        self.status = result;
    }

    fn banner_visible(&self) -> bool {
        !matches!(self.phase, Phase::Idle) || matches!(self.status, CheckStatus::Available { .. })
    }

    fn available(&self) -> Option<(&str, &str, Option<&InstallPlan>)> {
        match &self.status {
            CheckStatus::Available {
                version,
                tag,
                install,
                ..
            } => Some((version, tag, install.as_ref())),
            _ => None,
        }
    }

    /// Whether the banner and About page offer an install entry. A release
    /// without this system's package only offers the release page.
    fn can_install(&self) -> bool {
        !self.phase.busy()
            && matches!(self.phase, Phase::Idle | Phase::Failed { .. })
            && matches!(self.available(), Some((_, _, Some(_))))
    }

    fn manual_path(&self) -> Option<&std::path::Path> {
        match &self.phase {
            Phase::Manual { path, .. } => Some(path),
            _ => None,
        }
    }

    fn restart_action(&self) -> Option<(&str, &RestartAction)> {
        self.phase.restart_action()
    }

    pub(super) fn restart_pending(&self) -> bool {
        self.restart_pending
    }
    fn take_restart(&mut self) -> Option<RestartAction> {
        let Phase::Ready { action, .. } = &self.phase else {
            return None;
        };
        let action = action.clone();
        self.restart_pending = false;
        self.restart_error = None;
        Some(action)
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
}

fn transport_error(error: ureq::Error) -> CheckError {
    match error {
        ureq::Error::Timeout(_) => CheckError::Timeout,
        ureq::Error::BodyExceedsLimit(_) => CheckError::TooLarge,
        _ => CheckError::Network(error.to_string()),
    }
}

fn client(timeout: Duration, https_only: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .https_only(https_only)
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .user_agent(concat!("Breeze/", env!("CARGO_PKG_VERSION")));
    // Loopback fixtures must not use a developer machine's proxy settings.
    #[cfg(test)]
    let config = config.proxy(None);
    config.build().into()
}

fn parse_release(body: &[u8], current: &str) -> Result<CheckStatus, CheckError> {
    parse_release_for(body, current, &InstallTarget::detect())
}

fn parse_release_for(
    body: &[u8],
    current: &str,
    target: &InstallTarget,
) -> Result<CheckStatus, CheckError> {
    let release: Release = serde_json::from_slice(body).map_err(|_| CheckError::InvalidMetadata)?;
    if release.draft || release.prerelease || release.tag_name.len() > 128 {
        return Err(CheckError::InvalidMetadata);
    }
    let tag = &release.tag_name;
    let version = Version::parse(
        tag.strip_prefix('v')
            .or_else(|| tag.strip_prefix('V'))
            .unwrap_or(tag),
    )
    .map_err(|_| CheckError::InvalidMetadata)?;
    let installed = Version::parse(current).map_err(|_| CheckError::InvalidMetadata)?;
    if !version.pre.is_empty() {
        return Err(CheckError::InvalidMetadata);
    }
    if version.cmp_precedence(&installed).is_gt() {
        let version = version.to_string();
        // Asset names are derived from the validated version and a fixed target,
        // then checked against the release so the UI never offers a package the
        // release does not publish.
        let install = target.plan(&version).filter(|plan| {
            release.assets.iter().any(|asset| asset.name == plan.asset)
                && release
                    .assets
                    .iter()
                    .any(|asset| asset.name == plan.checksum)
        });
        // Construct the URL from the fixed repository and a validated SemVer tag.
        // Never open arbitrary URLs supplied by the response body.
        Ok(CheckStatus::Available {
            version,
            tag: tag.clone(),
            url: format!("{RELEASES}/tag/{tag}"),
            install,
        })
    } else {
        Ok(CheckStatus::Current {
            latest: version.to_string(),
        })
    }
}

fn fetch(agent: &ureq::Agent, endpoint: &str, current: &str) -> Result<CheckStatus, CheckError> {
    let mut response = agent
        .get(endpoint)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(transport_error)?;
    let status = response.status().as_u16();
    match status {
        200 => {}
        404 => return Ok(CheckStatus::Unpublished),
        429 => return Err(CheckError::RateLimited),
        403 if response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|value| value == "0") =>
        {
            return Err(CheckError::RateLimited);
        }
        _ => return Err(CheckError::Http(status)),
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_METADATA_BYTES)
        .read_to_vec()
        .map_err(transport_error)?;
    parse_release(&body, current)
}

fn check_latest() -> CheckStatus {
    fetch(&client(TIMEOUT, true), ENDPOINT, CURRENT_VERSION).unwrap_or_else(CheckStatus::Failed)
}

impl CheckStatus {
    /// The check outcome as a sentence, or `None` when there is nothing to report:
    /// the button already labels an in-flight check, and an untouched check is not
    /// news worth the reader's attention.
    fn message(&self) -> Option<String> {
        Some(match self {
            // Silence beats stating that nothing has happened yet.
            Self::Idle | Self::Checking => return None,
            Self::Unpublished => {
                tr!("No public stable release found. The repository may not exist, may be private, or may have no releases yet.").into()
            }
            Self::Current { latest } => tf!("No update needed · Latest stable version {}", latest),
            Self::Available { version, .. } => tf!("New version {} available", version),
            Self::Failed(error) => match error {
                CheckError::Timeout => tr!("Update check timed out. Try again.").into(),
                CheckError::Network(detail) => tf!("Cannot connect to the update service: {}", detail),
                CheckError::TooLarge => tr!("The update response is too large. Try again later.").into(),
                CheckError::InvalidMetadata => tr!("Invalid update metadata; a valid stable version is required.").into(),
                CheckError::RateLimited => tr!("GitHub rate limit reached. Try again later.").into(),
                CheckError::Http(status) => tf!("Update service returned HTTP {}. Try again later.", status),
            },
        })
    }
}

impl InstallError {
    /// One user-facing sentence. Diagnostics from local tools are already
    /// bounded by the installer and never echo remote response bodies.
    fn message(&self) -> String {
        match self {
            Self::Network(detail) => tf!("Cannot connect to the update service: {}", detail),
            Self::Timeout => tr!("The update download timed out. Try again.").into(),
            Self::Http(status) => tf!("Update service returned HTTP {}. Try again later.", status),
            Self::TooLarge => tr!("The update package is larger than Breeze will install.").into(),
            Self::ChecksumMissing => {
                tr!("The release does not publish a checksum for this update.").into()
            }
            Self::ChecksumInvalid => tr!("The published update checksum is invalid.").into(),
            Self::ChecksumMismatch => {
                tr!("The update is corrupted: it does not match its SHA-256 checksum.").into()
            }
            Self::Io(detail) => tf!("Cannot store the update: {}", detail),
            Self::VersionMismatch { found, expected } => tf!(
                "The downloaded update is version {} instead of {}.",
                found,
                expected
            ),
            Self::MissingApplication => {
                tr!("The downloaded package does not contain Breeze.").into()
            }
            Self::InstallFailed(detail) => {
                tf!("The system could not install the update: {}", detail)
            }
            Self::Unsupported => {
                tr!("This installation cannot update itself automatically.").into()
            }
        }
    }
}

impl Breeze {
    /// Update state belongs to the primary pane. A split peer shares the window
    /// but not the state store, so it only links to the release page.
    fn owns_updates(&self) -> bool {
        self.workspace.parent.is_none()
    }

    pub(super) fn check_updates(&mut self, automatic: bool, cx: &mut Context<Self>) {
        if !self.updates.begin(automatic, self.update_policy) {
            return;
        }
        let work = cx.background_spawn(async { check_latest() });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.updates.finish(automatic, this.update_policy, result);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn update_check_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("check-updates")
            .small()
            .icon(IconName::RefreshCw)
            .label(if self.updates.in_flight {
                tr!("Checking for updates…")
            } else {
                tr!("Check for updates")
            })
            .disabled(self.updates.in_flight)
            .on_click(cx.listener(|this, _, _, cx| this.check_updates(false, cx)))
    }

    pub(super) fn update_release_url(&self) -> Option<String> {
        match &self.updates.status {
            CheckStatus::Available { url, .. } => Some(url.clone()),
            _ => None,
        }
    }

    /// Download the offered release. The metadata request and the installer are
    /// both bounded background jobs; only their results touch the UI state.
    fn install_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.owns_updates() {
            return;
        }
        let Some((version, tag)) = self
            .updates
            .available()
            .map(|(version, tag, _)| (version.to_owned(), tag.to_owned()))
        else {
            return;
        };
        if !self.updates.can_install() {
            return;
        }
        let target = InstallTarget::detect();
        let Some(plan) = target.plan(&version) else {
            self.updates.phase = Phase::Failed {
                version,
                error: InstallError::Unsupported,
            };
            cx.notify();
            return;
        };
        self.updates.phase = Phase::Downloading {
            version: version.clone(),
        };
        self.updates.progress.reset(0);
        self.updates.restart_prompted = false;
        self.updates.restart_error = None;
        let progress = self.updates.progress.clone();
        let work = cx.background_spawn({
            let progress = progress.clone();
            async move { install::download(&tag, &plan, &progress) }
        });
        cx.spawn_in(window, async move |this, window| {
            let result = work.await;
            let _ = this.update_in(window, |this, window, cx| {
                match result {
                    Ok(artifact) => {
                        this.updates.phase = Phase::Installing {
                            version: version.clone(),
                        };
                        this.finish_install(artifact, target, version, window, cx);
                    }
                    Err(error) => {
                        this.updates.phase = Phase::Failed { version, error };
                    }
                }
                cx.notify();
            });
        })
        .detach();
        self.tick_download_progress(window, cx);
        cx.notify();
    }

    fn finish_install(
        &mut self,
        artifact: PathBuf,
        target: InstallTarget,
        version: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let work = cx.background_spawn({
            let install_version = version.clone();
            async move { install::install(&target, &artifact, &install_version) }
        });
        cx.spawn_in(window, async move |this, window| {
            let result = work.await;
            let _ = this.update_in(window, |this, window, cx| {
                match result {
                    Ok(Completion::Installed { path, launch }) => {
                        this.updates.phase = Phase::Ready {
                            version,
                            action: RestartAction::Relaunch { path, kind: launch },
                        };
                        this.prompt_restart(window, cx);
                    }
                    Ok(Completion::Installer {
                        path,
                        version: installed,
                    }) => {
                        this.updates.phase = Phase::Ready {
                            version: installed,
                            action: RestartAction::RunInstaller { path },
                        };
                        this.prompt_restart(window, cx);
                    }
                    Ok(Completion::Manual { path }) => {
                        this.updates.phase = Phase::Manual { version, path };
                    }
                    Err(error) => {
                        this.updates.phase = Phase::Failed { version, error };
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Keep the byte counter moving while the download job runs. The loop ends
    /// as soon as the phase leaves `Downloading`, including on failure.
    fn tick_download_progress(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, window| {
            loop {
                executor.timer(PROGRESS_TICK).await;
                let keep = this
                    .update_in(window, |this, _, cx| {
                        let keep = matches!(this.updates.phase, Phase::Downloading { .. });
                        if keep {
                            cx.notify();
                        }
                        keep
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    fn prompt_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.owns_updates() || self.updates.restart_prompted {
            return;
        }
        let Some((version, action)) = self
            .updates
            .restart_action()
            .map(|(version, action)| (version.to_owned(), action.clone()))
        else {
            return;
        };
        self.updates.restart_prompted = true;
        let description = match &action {
            RestartAction::RunInstaller { .. } => tf!(
                "Breeze {} has been downloaded. Restart now to run the installer; the system will ask for permission.",
                version
            ),
            RestartAction::Relaunch { .. } => tf!(
                "Breeze {} has been installed. Restart now to finish.",
                version
            ),
        };
        let view = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            alert
                .title(tr!("Update ready"))
                .description(description.clone())
                .confirm()
                .ok_text(tr!("Restart now"))
                .cancel_text(tr!("Later"))
                .on_ok(move |_, window, cx| {
                    // Let the dialog finish closing before the restart closes
                    // the window, so neither surface operates on a dead window.
                    let handle = window.window_handle();
                    let view = view.clone();
                    let executor = cx.background_executor().clone();
                    cx.spawn(async move |cx| {
                        // Let the dialog's own deferred close run first.
                        executor.timer(Duration::from_millis(50)).await;
                        let _ = handle.update(cx, |_, window, cx| {
                            view.update(cx, |this, cx| this.restart_after_update(window, cx))
                        });
                    })
                    .detach();
                    true
                })
        });
    }

    fn restart_after_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.owns_updates() || self.updates.restart_action().is_none() {
            return;
        }
        self.updates.restart_pending = true;
        self.updates.restart_error = None;
        // Closing first saves state and refuses to discard unsaved annotations.
        if self.request_close(window, cx) {
            self.perform_restart(window, cx);
        } else if !self.closing {
            // The close was refused before a save started (unsaved work,
            // restoring, or a storage failure). A later ordinary close must not
            // launch the updater.
            self.updates.restart_pending = false;
        }
    }

    /// Called once the window is safe to close, either directly or after the
    /// state store finished writing. A failed launch keeps the window readable.
    pub(super) fn perform_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.updates.take_restart() else {
            window.remove_window();
            return;
        };
        match action.launch() {
            Ok(()) => window.remove_window(),
            Err(error) => {
                self.updates.restart_error = Some(error.message());
                self.updates.restart_pending = false;
                self.updates.restart_prompted = true;
                cx.notify();
            }
        }
    }

    fn restart_button(&self, id: &'static str, label: String, cx: &mut Context<Self>) -> Button {
        Button::new(id)
            .small()
            .primary()
            .label(label)
            .on_click(cx.listener(|this, _, window, cx| this.restart_after_update(window, cx)))
    }

    fn install_button(&self, id: &'static str, cx: &mut Context<Self>) -> Button {
        Button::new(id)
            .small()
            .primary()
            .label(tr!("Update now"))
            .on_click(cx.listener(|this, _, window, cx| this.install_update(window, cx)))
    }

    fn reveal_update_button(&self, id: &'static str, cx: &mut Context<Self>) -> Button {
        Button::new(id)
            .small()
            .label(tr!("Show downloaded file"))
            .on_click(cx.listener(|this, _, _, cx| {
                let Some(path) = this.updates.manual_path().map(|path| path.to_path_buf()) else {
                    return;
                };
                let work = cx.background_spawn(async move { reveal::reveal(&path) });
                cx.spawn(async move |this, cx| {
                    if let Err(error) = work.await {
                        let detail = match error {
                            reveal::Error::Missing => {
                                tr!("Cannot show the downloaded update: the file is missing")
                                    .to_string()
                            }
                            reveal::Error::Launch(error) => {
                                tf!("Cannot show the downloaded update: {}", error)
                            }
                        };
                        let _ = this.update(cx, |this, cx| {
                            this.updates.restart_error = Some(detail);
                            cx.notify();
                        });
                    }
                })
                .detach();
            }))
    }

    /// Action buttons shared by the banner and the About page. The restart entry
    /// stays available after the modal prompt was dismissed once.
    pub(super) fn update_actions(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut actions = Vec::new();
        if self.owns_updates() {
            match &self.updates.phase {
                Phase::Ready { .. } => actions.push(
                    self.restart_button("restart-update", tr!("Restart now").to_string(), cx)
                        .into_any_element(),
                ),
                Phase::Manual { .. } => actions.push(
                    self.reveal_update_button("show-update-file", cx)
                        .into_any_element(),
                ),
                Phase::Failed { .. } if self.updates.available().is_some() => actions.push(
                    Button::new("retry-update")
                        .small()
                        .label(tr!("Retry update"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.install_update(window, cx)),
                        )
                        .into_any_element(),
                ),
                _ if self.updates.can_install() => {
                    actions.push(self.install_button("install-update", cx).into_any_element())
                }
                _ => {}
            }
        }
        if let Some(url) = self.update_release_url() {
            actions.push(
                Button::new("open-update-release")
                    .small()
                    .label(tr!("View release"))
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .into_any_element(),
            );
        }
        actions
    }

    pub(super) fn update_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // An explicit Home page already shows the update state in place.
        if (self.screen == Screen::Home && self.home_page != HomePage::Library)
            || self.updates.banner_dismissed
            || !self.updates.banner_visible()
        {
            return None;
        }
        let p = self.p(cx);
        let message = self.updates.message()?;
        let mut actions = Vec::new();
        if self.owns_updates() {
            match &self.updates.phase {
                Phase::Ready { .. } => actions.push(
                    self.restart_button(
                        "update-banner-restart",
                        tr!("Restart now").to_string(),
                        cx,
                    )
                    .into_any_element(),
                ),
                Phase::Manual { .. } => actions.push(
                    self.reveal_update_button("update-banner-show", cx)
                        .into_any_element(),
                ),
                Phase::Failed { .. } if self.updates.available().is_some() => actions.push(
                    Button::new("update-banner-retry")
                        .small()
                        .label(tr!("Retry update"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.install_update(window, cx)),
                        )
                        .into_any_element(),
                ),
                _ if self.updates.can_install() => actions.push(
                    self.install_button("update-banner-install", cx)
                        .into_any_element(),
                ),
                _ => {}
            }
        }
        let release = self.update_release_url();
        let mut banner = column()
            .id("update-banner")
            .test_support()
            .bg(p.notice.opacity(0.18))
            .border_b_1()
            .border_color(p.notice.opacity(0.6))
            .child(
                row()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .child(
                        Icon::new(IconName::Download)
                            .size(px(14.))
                            .text_color(p.notice),
                    )
                    .child(div().flex_1().min_w_0().child(message))
                    .children(actions)
                    .when_some(release, |row, url| {
                        row.child(
                            Button::new("update-banner-release")
                                .small()
                                .label(tr!("View release"))
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    })
                    .child(
                        Button::new("dismiss-update-banner")
                            .ghost()
                            .small()
                            .icon(IconName::X)
                            .accessibility_label(tr!("Dismiss update notification"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.updates.banner_dismissed = true;
                                cx.notify();
                            })),
                    ),
            );
        if let Some(error) = self.updates.restart_error.clone() {
            banner = banner.child(
                div()
                    .px_3()
                    .pb_1()
                    .text_size(px(12.))
                    .text_color(rgb(0xb56b3c))
                    .child(error),
            );
        }
        Some(banner.into_any_element())
    }
}

#[cfg(test)]
mod tests;
