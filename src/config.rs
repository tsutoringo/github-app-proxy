use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use std::env;
use std::net::SocketAddr;
use url::Url;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) git_base: Url,
    pub(crate) api_base: Url,
    pub(crate) app_id: u64,
    pub(crate) installation_id: u64,
    pub(crate) private_key: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self> {
        let listen_addr = env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse::<SocketAddr>()
            .context("invalid LISTEN_ADDR")?;

        let git_base = env::var("GIT_BASE_URL")
            .unwrap_or_else(|_| "https://github.com".to_string());
        let git_base = Url::parse(&git_base).context("invalid GIT_BASE_URL")?;

        let app_id = env::var("GITHUB_APP_ID")
            .context("GITHUB_APP_ID is required")?
            .parse::<u64>()
            .context("invalid GITHUB_APP_ID")?;
        let installation_id = env::var("GITHUB_APP_INSTALLATION_ID")
            .context("GITHUB_APP_INSTALLATION_ID is required")?
            .parse::<u64>()
            .context("invalid GITHUB_APP_INSTALLATION_ID")?;
        let private_key = env::var("GITHUB_APP_PRIVATE_KEY")
            .context("GITHUB_APP_PRIVATE_KEY (base64) is required")?;
        let private_key = decode_private_key(private_key)?;

        let api_prefix = env::var("GITHUB_API_PREFIX").ok();
        let api_base = build_api_base(&git_base, api_prefix)?;

        Ok(Self {
            listen_addr,
            git_base,
            api_base,
            app_id,
            installation_id,
            private_key,
        })
    }
}

pub(crate) fn with_trailing_slash(url: &Url) -> Url {
    let mut base = url.clone();
    let path = base.path();
    if !path.ends_with('/') {
        let new_path = format!("{}/", path.trim_end_matches('/'));
        base.set_path(&new_path);
    }
    base
}

fn normalize_private_key(value: String) -> String {
    if value.contains("\\n") && !value.contains('\n') {
        return value.replace("\\n", "\n");
    }
    value
}

fn decode_private_key(value: String) -> Result<String> {
    let decoded = general_purpose::STANDARD
        .decode(value.trim())
        .context("GITHUB_APP_PRIVATE_KEY is not valid base64")?;
    let decoded = String::from_utf8(decoded).context("GITHUB_APP_PRIVATE_KEY is not valid UTF-8")?;
    Ok(normalize_private_key(decoded))
}

fn build_api_base(git_base: &Url, api_prefix: Option<String>) -> Result<Url> {
    if let Some(prefix) = api_prefix {
        let trimmed = prefix.trim();
        if trimmed.is_empty() {
            return Ok(git_base.clone());
        }
        let normalized = if trimmed.starts_with('/') {
            trimmed.to_string()
        } else {
            format!("/{}", trimmed)
        };
        let mut base = git_base.clone();
        let combined_path = join_paths(base.path(), &normalized);
        base.set_path(&combined_path);
        return Ok(base);
    }

    if git_base.domain() == Some("github.com") {
        return Url::parse("https://api.github.com")
            .context("default GitHub API URL invalid");
    }

    let mut base = git_base.clone();
    let combined_path = join_paths(base.path(), "/api/v3");
    base.set_path(&combined_path);
    Ok(base)
}

fn join_paths(base: &str, suffix: &str) -> String {
    let base = base.trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');

    if suffix.is_empty() {
        return if base.is_empty() { "/".to_string() } else { base.to_string() };
    }

    if base.is_empty() {
        return format!("/{}", suffix);
    }

    format!("{}/{}", base, suffix)
}
