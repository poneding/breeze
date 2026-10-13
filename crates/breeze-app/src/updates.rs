//! Read-only stable-release discovery. Only a user action opens the release page.
use crate::*;
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

/// User-visible project address, shared with the About page.
pub(super) const REPOSITORY: &str = "https://github.com/poneding/breeze";
const ENDPOINT: &str = "https://api.github.com/repos/poneding/breeze/releases/latest";
const RELEASES: &str = "https://github.com/poneding/breeze/releases";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);

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
        url: String,
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

#[derive(Default)]
pub(super) struct Updates {
    status: CheckStatus,
    in_flight: bool,
    auto_checked: bool,
    banner_dismissed: bool,
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

    /// Current check state for the About page, in the active language. States that
    /// carry no news for the reader render nothing instead of filler text.
    pub(super) fn message(&self) -> Option<String> {
        self.status.message()
    }

    fn finish(&mut self, automatic: bool, policy: usize, result: CheckStatus) {
        self.in_flight = false;
        // A disabled automatic check may finish its bounded request, but must not
        // publish a stale notification or start a replacement request.
        self.status = if automatic && policy != 1 {
            CheckStatus::Idle
        } else {
            result
        };
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
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
        // Construct the URL from the fixed repository and a validated SemVer tag.
        // Never open arbitrary URLs supplied by the response body.
        Ok(CheckStatus::Available {
            version: version.to_string(),
            url: format!("{RELEASES}/tag/{tag}"),
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

impl Breeze {
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

    pub(super) fn update_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        // An explicit Home page already shows the update state in place.
        if (self.screen == Screen::Home && self.home_page != HomePage::Library)
            || self.updates.banner_dismissed
        {
            return None;
        }
        let url = self.update_release_url()?;
        let message = self.updates.message()?;
        Some(
            row()
                .gap_2()
                .px_3()
                .py_1()
                .border_b_1()
                .border_color(self.p(cx).line)
                .child(div().flex_1().min_w_0().child(message))
                .child(
                    Button::new("update-banner-release")
                        .small()
                        .label(tr!("View release"))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
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
        )
    }
}

#[cfg(test)]
mod tests;
