use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use anyhow::{Context, Result, anyhow};
use flate2::read::GzDecoder;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const RELEASES_URL: &str = "https://api.github.com/repos/getstassh/stassh/releases/latest";

#[derive(Debug, Clone)]
pub enum UpdateInstallStatus {
    Idle,
    Downloading { downloaded: u64, total: Option<u64> },
    Verifying,
    Installing,
    Done,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum UpdateCheckStatus {
    Idle,
    Checking,
    NoUpdate {
        current: Version,
    },
    UpdateAvailable {
        current: Version,
        latest: Version,
        asset: ReleaseAsset,
    },
    Error(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestRelease {
    pub tag_name: String,
    pub html_url: String,
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<ReleaseAsset>,
}

pub fn check_for_updates(current_version: &str) -> Result<UpdateCheckStatus> {
    let current = Version::parse(current_version)
        .with_context(|| format!("invalid current version: {current_version}"))?;

    let release = fetch_latest_release()?;
    let latest = version_from_tag(&release.tag_name)?;

    if release.draft || release.prerelease || latest <= current {
        return Ok(UpdateCheckStatus::NoUpdate { current });
    }

    let asset = select_asset(&release.assets)?;
    Ok(UpdateCheckStatus::UpdateAvailable {
        current,
        latest,
        asset,
    })
}

pub fn start_update_install(asset: ReleaseAsset) -> mpsc::Receiver<UpdateInstallStatus> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Err(err) = install_update(asset, tx.clone()) {
            let _ = tx.send(UpdateInstallStatus::Failed(err.to_string()));
        }
    });
    rx
}

fn install_update(asset: ReleaseAsset, tx: mpsc::Sender<UpdateInstallStatus>) -> Result<()> {
    let temp_dir = std::env::temp_dir().join("stassh-update");
    fs::create_dir_all(&temp_dir)?;

    let archive_path = temp_dir.join(&asset.name);
    let checksum_url = checksum_url_for(&asset.name);
    if let Some(checksum_url) = checksum_url {
        download_checksum(
            &checksum_url,
            &temp_dir.join(format!("{}.sha256", asset.name)),
        )?;
    }

    download_file(&asset.browser_download_url, &archive_path, tx.clone())?;

    tx.send(UpdateInstallStatus::Verifying).ok();
    verify_archive_checksum(&archive_path, &asset.name)?;

    let extracted = extract_archive(&archive_path, &temp_dir)?;

    tx.send(UpdateInstallStatus::Installing).ok();
    replace_current_binary(&extracted)?;

    tx.send(UpdateInstallStatus::Done).ok();
    Ok(())
}

fn download_file(url: &str, path: &Path, tx: mpsc::Sender<UpdateInstallStatus>) -> Result<()> {
    let mut response = ureq::get(url)
        .header("user-agent", "stassh-updater")
        .call()?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let mut file = File::create(path)?;
    let mut downloaded = 0u64;
    let mut buf = [0u8; 16 * 1024];

    loop {
        let read = response.body_mut().as_reader().read(&mut buf)?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])?;
        downloaded += read as u64;
        let _ = tx.send(UpdateInstallStatus::Downloading { downloaded, total });
    }

    Ok(())
}

fn verify_archive_checksum(archive_path: &Path, asset_name: &str) -> Result<()> {
    let checksum_path = current_checksum_path(asset_name);
    let checksum = fs::read_to_string(checksum_path).unwrap_or_else(|_| String::from(""));
    if checksum.is_empty() {
        return Ok(());
    }

    let mut file = File::open(archive_path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    let digest = hasher.finalize();
    let digest_hex = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if !checksum.contains(&digest_hex) {
        return Err(anyhow!("checksum mismatch for {}", asset_name));
    }
    Ok(())
}

fn current_checksum_path(asset_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir().join("stassh-update");
    path.push(format!("{}.sha256", asset_name));
    path
}

fn checksum_url_for(_asset_name: &str) -> Option<String> {
    Some(
        "https://github.com/getstassh/stassh/releases/latest/download/stassh-checksums.txt"
            .to_string(),
    )
}

fn download_checksum(url: &str, path: &Path) -> Result<()> {
    let mut response = ureq::get(url)
        .header("user-agent", "stassh-updater")
        .call()?;

    let mut body = String::new();
    response.body_mut().as_reader().read_to_string(&mut body)?;
    fs::write(path, body)?;
    Ok(())
}

fn extract_archive(archive_path: &Path, temp_dir: &Path) -> Result<PathBuf> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(temp_dir)?;

    let binary_name = if cfg!(target_os = "windows") {
        "stassh.exe"
    } else {
        "stassh"
    };
    let extracted = temp_dir.join(binary_name);
    if !extracted.exists() {
        return Err(anyhow!("extracted binary not found"));
    }
    Ok(extracted)
}

fn replace_current_binary(new_binary: &Path) -> Result<()> {
    let current = std::env::current_exe()?;
    let backup = current.with_extension("bak");

    if backup.exists() {
        let _ = fs::remove_file(&backup);
    }
    fs::copy(&current, &backup)?;
    fs::copy(new_binary, &current)?;

    if !cfg!(target_os = "windows") {
        let mut perms = fs::metadata(&current)?.permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        fs::set_permissions(&current, perms)?;
    }

    Ok(())
}

fn fetch_latest_release() -> Result<LatestRelease> {
    let mut response = ureq::get(RELEASES_URL)
        .header("user-agent", "stassh-updater")
        .call()?;

    let mut body = String::new();
    response.body_mut().as_reader().read_to_string(&mut body)?;
    Ok(serde_json::from_str(&body)?)
}

fn version_from_tag(tag: &str) -> Result<Version> {
    let normalized = tag.strip_prefix('v').unwrap_or(tag);
    Ok(Version::parse(normalized)?)
}

fn select_asset(assets: &[ReleaseAsset]) -> Result<ReleaseAsset> {
    let target = current_target_triple();
    assets
        .iter()
        .find(|asset| asset.name.contains(target))
        .cloned()
        .ok_or_else(|| anyhow!("no matching update asset for target {target}"))
}

fn current_target_triple() -> &'static str {
    if let Some(target) = option_env!("STASSH_BUILD_TARGET") {
        if !target.is_empty() {
            return target;
        }
    }

    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => "",
    }
}
