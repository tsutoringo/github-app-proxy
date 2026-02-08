use anyhow::{Context, Result};
use octocrab::models::InstallationId;
use octocrab::Octocrab;
use secrecy::ExposeSecret;

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
