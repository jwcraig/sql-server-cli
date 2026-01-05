use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;

const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";
const DEFAULT_GITHUB_REPO: &str = "jwcraig/sql-server-cli";

#[derive(Debug, Clone, Deserialize)]
struct GitHubLatestRelease {
    tag_name: String,
    html_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateCheckResult {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) update_available: bool,
    pub(crate) repo: String,
    pub(crate) release_url: Option<String>,
}

pub(crate) fn check_latest_release() -> Result<UpdateCheckResult> {
    let repo = github_repo();
    let api_base = github_api_base();
    let release = fetch_latest_release(&api_base, &repo)?;

    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .context("Failed to parse current version from build metadata")?;
    let latest = parse_tag_version(&release.tag_name)
        .with_context(|| format!("Failed to parse GitHub tag {}", release.tag_name))?;

    Ok(UpdateCheckResult {
        current_version: current.to_string(),
        latest_version: latest.to_string(),
        update_available: latest > current,
        repo,
        release_url: release.html_url,
    })
}

fn github_api_base() -> String {
    std::env::var("SSCLI_GITHUB_API_BASE").unwrap_or_else(|_| DEFAULT_GITHUB_API_BASE.to_string())
}

fn github_repo() -> String {
    if let Ok(repo) = std::env::var("SSCLI_UPDATE_REPO") {
        if !repo.trim().is_empty() {
            return repo;
        }
    }

    parse_repo_from_url(env!("CARGO_PKG_REPOSITORY")).unwrap_or_else(|| DEFAULT_GITHUB_REPO.into())
}

fn parse_repo_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let without_git = trimmed.trim_end_matches(".git");
    let parts: Vec<&str> = without_git.split('/').collect();
    let owner = parts.get(parts.len().saturating_sub(2))?;
    let repo = parts.get(parts.len().saturating_sub(1))?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{}/{}", owner, repo))
}

fn fetch_latest_release(api_base: &str, repo: &str) -> Result<GitHubLatestRelease> {
    let base = api_base.trim_end_matches('/');
    let url = format!("{}/repos/{}/releases/latest", base, repo);

    let client_builder = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("sscli/{}", env!("CARGO_PKG_VERSION")));

    let client = client_builder
        .build()
        .context("Failed to build HTTP client")?;

    let mut request = client
        .get(&url)
        .header("Accept", "application/vnd.github+json");

    if let Some(token) = github_token() {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .with_context(|| format!("GitHub request failed: {}", url))?;
    let status = response.status();
    let body = response
        .text()
        .with_context(|| format!("Failed reading GitHub response body: {}", url))?;

    if !status.is_success() {
        let snippet = body.lines().next().unwrap_or_default();
        return Err(anyhow!(
            "GitHub API request failed ({}): {}",
            status.as_u16(),
            snippet
        ));
    }

    serde_json::from_str(&body).context("Failed to parse GitHub release JSON")
}

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok())
        .or_else(|| std::env::var("GITHUB_API_TOKEN").ok())
}

fn parse_tag_version(tag: &str) -> Result<Version> {
    let raw = tag.trim();
    let without_prefix = raw.strip_prefix('v').unwrap_or(raw);
    Version::parse(without_prefix).context("Invalid semantic version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repo_from_url() {
        assert_eq!(
            parse_repo_from_url("https://github.com/jwcraig/sql-server-cli"),
            Some("jwcraig/sql-server-cli".to_string())
        );
        assert_eq!(
            parse_repo_from_url("https://github.com/jwcraig/sql-server-cli.git"),
            Some("jwcraig/sql-server-cli".to_string())
        );
    }

    #[test]
    fn parses_semver_tags() {
        assert_eq!(parse_tag_version("v1.2.3").unwrap(), Version::new(1, 2, 3));
        assert_eq!(parse_tag_version("1.2.3").unwrap(), Version::new(1, 2, 3));
    }
}
