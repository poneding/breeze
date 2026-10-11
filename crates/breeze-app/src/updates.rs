//! Read-only stable-release discovery. Only a user action opens the release page.
use crate::*;
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

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
    fn message(&self) -> String {
        match self {
            Self::Idle => tr!("尚未检查更新").into(),
            Self::Checking => tr!("正在检查更新…").into(),
            Self::Unpublished => {
                tr!("未找到公开正式版本；仓库可能尚未创建、尚未公开或尚未发布 Release。").into()
            }
            Self::Current { latest } => tf!("当前版本无需更新 · 最新正式版本 {}", latest),
            Self::Available { version, .. } => tf!("发现新版本 {}", version),
            Self::Failed(error) => match error {
                CheckError::Timeout => tr!("检查更新超时，请重试。").into(),
                CheckError::Network(detail) => tf!("无法连接更新服务：{}", detail),
                CheckError::TooLarge => tr!("更新响应过大，已停止读取。请稍后重试。").into(),
                CheckError::InvalidMetadata => tr!("更新信息无效；需要有效的正式版本号。").into(),
                CheckError::RateLimited => tr!("GitHub 请求限额已用尽，请稍后重试。").into(),
                CheckError::Http(status) => tf!("更新服务返回 HTTP {}，请稍后重试。", status),
            },
        }
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
                tr!("正在检查更新…")
            } else {
                tr!("立即检查")
            })
            .disabled(self.updates.in_flight)
            .on_click(cx.listener(|this, _, _, cx| this.check_updates(false, cx)))
    }

    pub(super) fn update_details(&self, cx: &Context<Self>) -> Div {
        let p = self.p(cx);
        column()
            .gap_1()
            .py_2()
            .text_size(px(11.))
            .text_color(p.muted)
            .child(tf!("当前版本 {} · 发布源 poneding/breeze", CURRENT_VERSION))
            .child(
                row()
                    .gap_2()
                    .child(
                        div()
                            .id("update-status")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .child(self.updates.status.message()),
                    )
                    .when_some(self.update_release_url(), |row, url| {
                        row.child(
                            Button::new("open-update-release")
                                .small()
                                .label(tr!("查看发布版本"))
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    }),
            )
    }

    fn update_release_url(&self) -> Option<String> {
        match &self.updates.status {
            CheckStatus::Available { url, .. } => Some(url.clone()),
            _ => None,
        }
    }

    pub(super) fn update_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.screen == Screen::Settings || self.updates.banner_dismissed {
            return None;
        }
        let url = self.update_release_url()?;
        Some(
            row()
                .gap_2()
                .px_3()
                .py_1()
                .border_b_1()
                .border_color(self.p(cx).line)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(self.updates.status.message()),
                )
                .child(
                    Button::new("update-banner-release")
                        .small()
                        .label(tr!("查看发布版本"))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
                .child(
                    Button::new("dismiss-update-banner")
                        .ghost()
                        .small()
                        .icon(IconName::X)
                        .accessibility_label(tr!("关闭更新提示"))
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
