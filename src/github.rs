use crate::AppState;
use anyhow::{Context, Result};
use octocrab::Octocrab;
use octocrab::models::InstallationId;
use secrecy::ExposeSecret;
use std::time::{Duration, Instant};

const TOKEN_TTL: Duration = Duration::from_secs(3600); // GitHub tokens last ~1 hour

pub(crate) struct CachedToken {
    pub(crate) token: String,
    pub(crate) expires_at: std::time::Instant,
}

pub(crate) async fn fetch_installation_token(
    octocrab: &Octocrab,
    installation_id: u64,
) -> Result<String> {
    let (_crab, token) = octocrab
        .installation_and_token(InstallationId(installation_id))
        .await
        .context("requesting installation token")?;

    Ok(token.expose_secret().to_string())
}

pub(crate) async fn get_cached_token(state: &AppState) -> Result<String> {
    // Scope for the lock
    {
        let mut cache = state.token_cache.lock().await;

        // Check if we have a valid cached token
        if let Some(cached) = cache.as_ref() {
            if cached.expires_at > std::time::Instant::now() {
                return Ok(cached.token.clone());
            }
        }
    }

    // Fetch new token
    let token = fetch_installation_token(&state.octocrab, state.config.installation_id)
        .await
        .context("fetching installation token")?;

    // Cache the token
    let mut cache = state.token_cache.lock().await;
    if let Some(cached) = cache.as_ref() {
        if cached.expires_at > Instant::now() {
            return Ok(cached.token.clone());
        }
    }

    *cache = Some(CachedToken {
        token: token.clone(),
        expires_at: Instant::now() + TOKEN_TTL,
    });

    Ok(token)
}
