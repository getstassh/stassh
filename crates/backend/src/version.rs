use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result};
use semver::Version;
use serde::Deserialize;
use serde_json::from_str;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/getstassh/stassh/releases/latest";

#[derive(Debug, Clone)]
pub enum VersionCheckStatus {
    Idle,
    Checking,
    UpToDate {
        current: Version,
    },
    UpdateAvailable {
        current: Version,
        latest: Version,
        url: String,
    },
    Error(String),
}

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
    html_url: String,
    prerelease: bool,
    draft: bool,
}

pub fn check_for_updates(current_version: &str) -> Result<VersionCheckStatus> {
    let current = Version::parse(current_version)
        .with_context(|| format!("invalid current version: {current_version}"))?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(2)))
        .timeout_global(Some(Duration::from_secs(4)))
        .build()
        .into();

    let response = agent
        .get(LATEST_RELEASE_URL)
        .header("user-agent", "stassh-version-checker")
        .call()?;

    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut body = String::new();
    reader.read_to_string(&mut body)?;
    let release: LatestRelease =
        from_str(&body).context("failed to parse latest release response")?;

    if release.draft || release.prerelease {
        return Ok(VersionCheckStatus::UpToDate { current });
    }

    let normalized = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let latest = Version::parse(normalized)
        .with_context(|| format!("invalid release tag: {}", release.tag_name))?;

    if latest > current {
        Ok(VersionCheckStatus::UpdateAvailable {
            current,
            latest,
            url: release.html_url,
        })
    } else {
        Ok(VersionCheckStatus::UpToDate { current })
    }
}
