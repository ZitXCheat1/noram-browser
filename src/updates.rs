//! HTTPS downloads, bounded inputs, atomic filter replacement and signed app updates.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Read,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const FILTER_LIMIT: u64 = 12 * 1024 * 1024;
const FILTER_FILE: &str = "filter-lists.json";
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("NoRAM/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?)
}
fn text_response(response: reqwest::blocking::Response, limit: u64) -> Result<String> {
    let response = response.error_for_status()?;
    anyhow::ensure!(
        response.content_length().is_none_or(|n| n <= limit),
        "Response exceeds size limit"
    );
    let mut bytes = Vec::new();
    response.take(limit + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= limit, "Response exceeds size limit");
    Ok(String::from_utf8(bytes)?)
}
#[derive(Serialize, Deserialize)]
pub struct Lists {
    pub updated: u64,
    pub easylist: String,
    pub easyprivacy: String,
}
fn valid_list(text: &str) -> bool {
    text.starts_with("[Adblock Plus")
        && text
            .lines()
            .filter(|s| !s.is_empty() && !s.starts_with(['!', '[']))
            .count()
            >= 1000
        && text.len() as u64 <= FILTER_LIMIT
}
pub fn cached_lists(profile: &Path) -> Option<Lists> {
    for name in [FILTER_FILE, "filter-lists.json.bak"] {
        let path = profile.join(name);
        if !std::fs::metadata(&path).is_ok_and(|m| m.len() <= FILTER_LIMIT * 2 + 4096) {
            continue;
        }
        if let Ok(lists) = std::fs::read(&path).and_then(|bytes| {
            serde_json::from_slice::<Lists>(&bytes).map_err(std::io::Error::other)
        }) {
            if valid_list(&lists.easylist) && valid_list(&lists.easyprivacy) {
                return Some(lists);
            }
        }
    }
    None
}
pub fn filters_due(profile: &Path) -> bool {
    let last = cached_lists(profile).map_or(0, |l| l.updated);
    let attempt = std::fs::read_to_string(profile.join("filter-attempt.txt"))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    now().saturating_sub(last.max(attempt)) >= 86400
}
pub fn filter_status(profile: &Path) -> String {
    cached_lists(profile).map_or_else(
        || "Bundled lists: 17 October 2026".into(),
        |l| {
            format!(
                "Downloaded filters active; updated {} hours ago.",
                now().saturating_sub(l.updated) / 3600
            )
        },
    )
}
pub fn update_filters(profile: &Path) -> Result<String> {
    std::fs::write(profile.join("filter-attempt.txt"), now().to_string())?;
    let client = client()?;
    let easylist = text_response(
        client
            .get("https://easylist.to/easylist/easylist.txt")
            .send()?,
        FILTER_LIMIT,
    )?;
    let easyprivacy = text_response(
        client
            .get("https://easylist.to/easylist/easyprivacy.txt")
            .send()?,
        FILTER_LIMIT,
    )?;
    anyhow::ensure!(
        valid_list(&easylist) && valid_list(&easyprivacy),
        "Invalid filter-list format or too few rules"
    );
    // Compile both lists before committing either; existing views switch only after success.
    let _ = crate::policy::Blocker::from_lists(&[&easylist, &easyprivacy]);
    let lists = Lists {
        updated: now(),
        easylist,
        easyprivacy,
    };
    let file = profile.join(FILTER_FILE);
    if let Some(previous) = cached_lists(profile) {
        let backup = profile.join("filter-lists.json.bak.tmp");
        std::fs::write(&backup, serde_json::to_vec(&previous)?)?;
        std::fs::rename(backup, profile.join("filter-lists.json.bak"))?;
    }
    let tmp = profile.join("filter-lists.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&lists)?)?;
    std::fs::rename(tmp, file)?;
    Ok("Filters updated and active. Reload pages to reapply cosmetic rules.".into())
}
fn release_version(value: &Value) -> Result<semver::Version> {
    anyhow::ensure!(
        value["draft"] == false && value["prerelease"] == false,
        "Only stable published updates are accepted"
    );
    let tag = value["tag_name"]
        .as_str()
        .context("Missing release version")?;
    Ok(semver::Version::parse(tag.trim_start_matches('v'))?)
}
pub fn app_update(profile: &Path, install: bool) -> Result<Value> {
    let client = client()?;
    let response = client
        .get("https://api.github.com/repos/ZitXCheat1/noram-browser/releases/latest")
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(
            json!({"status":"current","message":"No stable releases have been published yet."}),
        );
    }
    let raw = text_response(response, 1024 * 1024)?;
    let release: Value = serde_json::from_str(&raw)?;
    let version = release_version(&release)?;
    if version <= semver::Version::parse(env!("CARGO_PKG_VERSION"))? {
        return Ok(json!({"status":"current","message":"No newer stable release is available."}));
    }
    let status = json!({"status":"available","version":version.to_string(),"message":format!("NoRAM {version} is available. Installation requires a trusted signed installer.")});
    if !install {
        return Ok(status);
    }
    let signer = option_env!("NORAM_UPDATE_SIGNER_THUMBPRINT").unwrap_or("");
    anyhow::ensure!(signer.len()==40&&signer.bytes().all(|b|b.is_ascii_hexdigit()),"This build has no pinned release signing certificate. Install an official signed release manually.");
    let asset = release["assets"]
        .as_array()
        .context("Missing release assets")?
        .iter()
        .find(|v| v["name"] == "NoRAM-Setup-x64.exe")
        .context("The release has no Windows installer")?;
    let url = asset["browser_download_url"]
        .as_str()
        .context("Missing installer URL")?;
    anyhow::ensure!(
        url.starts_with("https://github.com/ZitXCheat1/noram-browser/releases/download/"),
        "Unexpected installer source"
    );
    let digest = asset["digest"]
        .as_str()
        .context("Release asset has no SHA-256 digest")?;
    let expected = digest
        .strip_prefix("sha256:")
        .context("Expected SHA-256 digest")?;
    anyhow::ensure!(
        expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid digest"
    );
    let response = client.get(url).send()?.error_for_status()?;
    let limit = 200 * 1024 * 1024;
    anyhow::ensure!(
        response.content_length().is_none_or(|n| n <= limit),
        "Installer too large"
    );
    let mut bytes = Vec::new();
    response.take(limit + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= limit, "Installer too large");
    use sha2::{Digest, Sha256};
    anyhow::ensure!(
        format!("{:x}", Sha256::digest(&bytes)).eq_ignore_ascii_case(expected),
        "Installer checksum mismatch"
    );
    let dir = profile.join("Updates");
    std::fs::create_dir_all(&dir)?;
    let installer = dir.join(format!("NoRAM-{version}-{}.exe", now()));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&installer)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    use std::os::windows::process::CommandExt;
    let powershell =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot is missing")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let check=std::process::Command::new(powershell).creation_flags(0x08000000)
        .args(["-NoProfile","-NonInteractive","-Command", "$s=Get-AuthenticodeSignature -LiteralPath $env:NORAM_INSTALLER; if($s.Status -ne 'Valid' -or $s.SignerCertificate.Thumbprint -ne $env:NORAM_SIGNER){exit 1}"])
        .env("NORAM_INSTALLER",&installer).env("NORAM_SIGNER",signer).output()?;
    if !check.status.success() {
        let _ = std::fs::remove_file(installer);
        anyhow::bail!("Installer signature or pinned publisher verification failed");
    }
    std::process::Command::new(&installer).spawn()?;
    Ok(
        json!({"status":"installing","message":"Verified installer opened. Close NoRAM before completing installation."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_download_cache_falls_back_to_last_valid_lists() {
        let dir = std::env::temp_dir().join(format!("noram-filters-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = format!(
            "[Adblock Plus 2.0]\n{}",
            (0..1001)
                .map(|i| format!("||ads{i}.example^\n"))
                .collect::<String>()
        );
        let lists = Lists {
            updated: 123,
            easylist: list.clone(),
            easyprivacy: list,
        };
        std::fs::write(
            dir.join("filter-lists.json.bak"),
            serde_json::to_vec(&lists).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join(FILTER_FILE), b"broken").unwrap();
        assert_eq!(cached_lists(&dir).unwrap().updated, 123);
        let blocker = crate::policy::Blocker::load(&dir, false);
        assert!(blocker.blocks(
            "https://ads42.example/banner.js",
            "https://example.org",
            "script"
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn update_metadata_and_filter_validation_fail_closed() {
        assert!(!valid_list("<html>Error</html>"));
        assert!(!valid_list("[Adblock Plus 2.0]\n||ads.example^"));
        assert!(
            release_version(&json!({"draft":true,"prerelease":false,"tag_name":"v99.0.0"}))
                .is_err()
        );
        assert!(
            release_version(&json!({"draft":false,"prerelease":true,"tag_name":"v99.0.0"}))
                .is_err()
        );
        assert_eq!(
            release_version(&json!({"draft":false,"prerelease":false,"tag_name":"v0.3.0"}))
                .unwrap()
                .to_string(),
            "0.3.0"
        );
    }
}
