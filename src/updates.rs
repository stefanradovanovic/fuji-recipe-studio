//! Explicit, user-initiated updates from this project's GitHub Releases.
//! Credentials stay in memory and are sent only to api.github.com.
use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use url::Url;
use zeroize::Zeroizing;

pub const OWNER: &str = "stefanradovanovic";
pub const REPOSITORY: &str = "stefanradovanovic/fuji-recipe-studio";
pub const RELEASES_PAGE: &str =
    "https://github.com/stefanradovanovic/fuji-recipe-studio/releases/latest";
const MAX_INSTALLER: u64 = 256 * 1024 * 1024;
const MAX_METADATA: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    pub notes: String,
    asset_id: u64,
    filename: String,
    size: u64,
    sha256: [u8; 32],
}

impl Release {
    pub fn size_mb(&self) -> f32 {
        self.size as f32 / 1_048_576.
    }
    pub fn page_url(&self) -> String {
        format!(
            "https://github.com/{REPOSITORY}/releases/tag/v{}",
            self.version
        )
    }
}

#[derive(Debug)]
pub enum Check {
    Current { latest: Version },
    Available(Release),
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    id: u64,
    name: String,
    size: u64,
    digest: Option<String>,
    state: String,
}

pub fn check(current: &str) -> Result<Check> {
    let token = github_token();
    let agent = agent(Duration::from_secs(25));
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let response = request(
        &agent,
        &Url::parse(&url)?,
        token.as_deref().map(|s| s.as_str()),
        false,
    )
    .map_err(api_error)?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_METADATA + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA,
        "Release metadata is too large."
    );
    interpret_release(&bytes, current)
}

fn interpret_release(bytes: &[u8], current: &str) -> Result<Check> {
    let data: GithubRelease =
        serde_json::from_slice(bytes).context("GitHub returned invalid release metadata.")?;
    let current = Version::parse(current)?;
    let tag = data
        .tag_name
        .strip_prefix('v')
        .context("The release tag must start with v.")?;
    let version = Version::parse(tag).context("The release version is invalid.")?;
    ensure!(
        !data.draft && !data.prerelease && version.pre.is_empty() && version.build.is_empty(),
        "This release is not a stable update."
    );
    if version <= current {
        return Ok(Check::Current { latest: version });
    }
    let filename = format!("FujiRecipeStudio-Setup-{version}-x64.exe");
    let matches: Vec<_> = data
        .assets
        .into_iter()
        .filter(|a| a.name == filename)
        .collect();
    ensure!(
        matches.len() == 1,
        "This release does not contain exactly one Windows installer."
    );
    let asset = matches.into_iter().next().unwrap();
    ensure!(
        asset.id > 0 && asset.state == "uploaded" && (64..=MAX_INSTALLER).contains(&asset.size),
        "The release installer is incomplete or too large."
    );
    let sha256 = parse_digest(
        asset
            .digest
            .as_deref()
            .context("GitHub has not provided the installer's SHA-256. Try again later.")?,
    )?;
    Ok(Check::Available(Release {
        version,
        filename,
        asset_id: asset.id,
        size: asset.size,
        sha256,
        notes: data.body.unwrap_or_default().chars().take(12_000).collect(),
    }))
}

fn parse_digest(digest: &str) -> Result<[u8; 32]> {
    let hex = digest
        .strip_prefix("sha256:")
        .context("An SHA-256 digest is required.")?;
    ensure!(
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid installer SHA-256."
    );
    let mut result = [0; 32];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(result)
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(timeout)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build()
}

fn trusted_download_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

fn request(
    agent: &ureq::Agent,
    url: &Url,
    token: Option<&str>,
    binary: bool,
) -> std::result::Result<ureq::Response, ureq::Error> {
    let mut request = agent
        .get(url.as_str())
        .set(
            "User-Agent",
            concat!("FujiRecipeStudio/", env!("CARGO_PKG_VERSION")),
        )
        .set(
            "Accept",
            if binary {
                "application/octet-stream"
            } else {
                "application/vnd.github+json"
            },
        );
    if url.host_str() == Some("api.github.com") {
        request = request.set("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = token {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
    }
    request.call()
}

fn api_error(error: ureq::Error) -> anyhow::Error {
    match error {
        ureq::Error::Status(401 | 403 | 404, _) => anyhow::anyhow!(
            "GitHub releases are unavailable. This repository is private: use GitHub CLI to sign in as stefanradovanovic (or an account with access), then retry. A release must also be published."
        ),
        ureq::Error::Status(code, _) => {
            anyhow::anyhow!("GitHub returned HTTP {code}. Please retry later.")
        }
        ureq::Error::Transport(_) => anyhow::anyhow!(
            "Could not connect to GitHub. Check your internet connection and retry."
        ),
    }
}

pub struct Prepared {
    directory: Option<tempfile::TempDir>,
    path: PathBuf,
    release: Release,
}

impl Prepared {
    pub fn version(&self) -> &Version {
        &self.release.version
    }
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Recheck the file immediately before starting it. Keep the directory while
    /// Setup is running; its executable cannot be removed as the UI shuts down.
    pub fn install(&mut self) -> Result<()> {
        ensure!(
            self.directory.is_some(),
            "This installer has already been started."
        );
        verify_file(&self.path, &self.release)?;
        let mut command = Command::new(&self.path);
        command.args([
            "/SP-",
            "/SILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/CLOSEAPPLICATIONS",
            "/NOFORCECLOSEAPPLICATIONS",
            "/RESTARTAPP=1",
        ]);
        command.spawn().context(
            "Could not start the update installer. Your current installation has not been changed.",
        )?;
        if let Some(directory) = self.directory.take() {
            let _ = directory.keep();
        }
        Ok(())
    }
}

pub fn download(release: &Release, progress: impl Fn(u64, u64)) -> Result<Prepared> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .context("Windows Local App Data is unavailable.")?
        .join("FujiRecipeStudio")
        .join("updates");
    std::fs::create_dir_all(&base)?;
    let directory = tempfile::Builder::new()
        .prefix("download-")
        .tempdir_in(&base)?;
    let path = directory.path().join(&release.filename);
    let token = github_token();
    let agent = agent(Duration::from_secs(180));
    let mut url = Url::parse(&format!(
        "https://api.github.com/repos/{REPOSITORY}/releases/assets/{}",
        release.asset_id
    ))?;
    for _ in 0..6 {
        ensure!(
            trusted_download_url(&url),
            "The update download redirected outside GitHub. Download cancelled."
        );
        let response =
            request(&agent, &url, token.as_deref().map(|s| s.as_str()), true).map_err(api_error)?;
        if (300..400).contains(&response.status()) {
            url = url.join(
                response
                    .header("Location")
                    .context("GitHub returned an invalid download redirect.")?,
            )?;
            continue;
        }
        ensure!(
            response.status() == 200,
            "Unexpected installer download response."
        );
        let mut file = File::create_new(&path)?;
        copy_verified(response.into_reader(), &mut file, release, progress)?;
        file.sync_all()?;
        drop(file);
        return Ok(Prepared {
            directory: Some(directory),
            path,
            release: release.clone(),
        });
    }
    bail!("Too many update download redirects.")
}

fn copy_verified(
    mut input: impl Read,
    output: &mut impl Write,
    release: &Release,
    progress: impl Fn(u64, u64),
) -> Result<()> {
    let mut digest = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    let mut first = Vec::new();
    let mut last_notice = Instant::now();
    progress(0, release.size);
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        ensure!(
            count <= release.size,
            "Installer size does not match the release. Download cancelled."
        );
        if first.len() < 2 {
            first.extend_from_slice(&buffer[..n.min(2 - first.len())]);
        }
        digest.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
        if last_notice.elapsed() >= Duration::from_millis(80) {
            progress(count, release.size);
            last_notice = Instant::now();
        }
    }
    ensure!(
        count == release.size,
        "The installer download was incomplete. Please retry."
    );
    ensure!(first == b"MZ", "The update is not a Windows executable.");
    ensure!(
        <[u8; 32]>::from(digest.finalize()) == release.sha256,
        "Installer SHA-256 verification failed. Nothing was installed."
    );
    progress(count, release.size);
    Ok(())
}

fn verify_file(path: &Path, release: &Release) -> Result<()> {
    ensure!(
        path.metadata()?.len() == release.size,
        "The downloaded installer has changed. Download it again."
    );
    copy_verified(File::open(path)?, &mut std::io::sink(), release, |_, _| {})
}

fn github_token() -> Option<Zeroizing<String>> {
    let gh = find_gh()?;
    // Prefer the repository owner's existing sign-in; do not change the active
    // account, persist a token, or inherit a developer's GH_TOKEN override.
    read_gh_token(
        &gh,
        &["auth", "token", "--hostname", "github.com", "--user", OWNER],
    )
    .or_else(|| read_gh_token(&gh, &["auth", "token", "--hostname", "github.com"]))
}

fn find_gh() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(program_files)
                .join("GitHub CLI")
                .join("gh.exe"),
        );
    }
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(
            std::env::split_paths(&paths)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(if cfg!(windows) { "gh.exe" } else { "gh" })),
        );
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn read_gh_token(gh: &Path, args: &[&str]) -> Option<Zeroizing<String>> {
    let mut command = Command::new(gh);
    command
        .args(args)
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().ok()? {
            if !status.success() {
                return None;
            }
            let mut bytes = Zeroizing::new(Vec::new());
            child
                .stdout
                .take()?
                .take(8193)
                .read_to_end(&mut bytes)
                .ok()?;
            let token = std::str::from_utf8(&bytes).ok()?.trim();
            if !(10..=8192).contains(&token.len()) || token.chars().any(char::is_whitespace) {
                return None;
            }
            return Some(Zeroizing::new(token.to_owned()));
        }
        if started.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn metadata(version: &str, digest: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,"body":"Release notes",
            "assets":[{"id":12,"name":format!("FujiRecipeStudio-Setup-{version}-x64.exe"),"size":100,"state":"uploaded","digest":digest}]})).unwrap()
    }
    fn release(bytes: &[u8]) -> Release {
        Release {
            version: Version::new(0, 2, 0),
            notes: String::new(),
            asset_id: 1,
            filename: "FujiRecipeStudio-Setup-0.2.0-x64.exe".into(),
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        }
    }
    #[test]
    fn versions_compare_numerically_and_never_offer_downgrades() {
        let data = metadata("0.10.0", &format!("sha256:{}", "ab".repeat(32)));
        assert!(matches!(
            interpret_release(&data, "0.2.0").unwrap(),
            Check::Available(_)
        ));
        assert!(matches!(
            interpret_release(&data, "0.10.0").unwrap(),
            Check::Current { .. }
        ));
        assert!(matches!(
            interpret_release(&data, "0.11.0").unwrap(),
            Check::Current { .. }
        ));
    }
    #[test]
    fn rejects_prerelease_invalid_checksums_and_unexpected_asset_names() {
        assert!(
            interpret_release(
                &metadata("1.0.0-beta.1", &format!("sha256:{}", "a".repeat(64))),
                "0.2.0"
            )
            .is_err()
        );
        assert!(interpret_release(&metadata("1.0.0", "md5:abcdef"), "0.2.0").is_err());
        let mut data: serde_json::Value =
            serde_json::from_slice(&metadata("1.0.0", &format!("sha256:{}", "a".repeat(64))))
                .unwrap();
        data["assets"][0]["name"] = json!("../../untrusted.exe");
        assert!(interpret_release(&serde_json::to_vec(&data).unwrap(), "0.2.0").is_err());
        data["draft"] = json!(true);
        assert!(interpret_release(&serde_json::to_vec(&data).unwrap(), "0.2.0").is_err());
    }
    #[test]
    fn downloads_reject_tampering_truncation_extra_bytes_and_html() {
        let bytes = b"MZverified executable fixture";
        let info = release(bytes);
        assert!(copy_verified(&bytes[..], &mut Vec::new(), &info, |_, _| {}).is_ok());
        let mut corrupt = bytes.to_vec();
        corrupt[5] ^= 1;
        assert!(copy_verified(corrupt.as_slice(), &mut Vec::new(), &info, |_, _| {}).is_err());
        assert!(copy_verified(&bytes[..5], &mut Vec::new(), &info, |_, _| {}).is_err());
        let mut longer = bytes.to_vec();
        longer.push(0);
        assert!(copy_verified(longer.as_slice(), &mut Vec::new(), &info, |_, _| {}).is_err());
        let html = b"<html>not an installer</html>";
        assert!(copy_verified(&html[..], &mut Vec::new(), &release(html), |_, _| {}).is_err());
    }
    #[test]
    fn redirects_stay_on_explicit_github_https_hosts() {
        for bad in [
            "http://github.com/file",
            "https://github.com.attacker.test/file",
            "https://github.com:444/file",
            "https://user@api.github.com/file",
            "https://localhost/file",
        ] {
            assert!(!trusted_download_url(&Url::parse(bad).unwrap()));
        }
        assert!(trusted_download_url(
            &Url::parse("https://release-assets.githubusercontent.com/file?token=opaque").unwrap()
        ));
    }
    #[test]
    fn rechecks_download_before_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup.exe");
        let bytes = b"MZverified executable fixture";
        let info = release(bytes);
        std::fs::write(&path, bytes).unwrap();
        assert!(verify_file(&path, &info).is_ok());
        std::fs::write(&path, b"MZtampered executable fixture").unwrap();
        assert!(verify_file(&path, &info).is_err());
    }
}
