mod config;
mod github;
mod proxy;

use anyhow::{Context, Result};
use hyper::client::HttpConnector;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Server};
use hyper_rustls::HttpsConnectorBuilder;
use jsonwebtoken::EncodingKey;
use octocrab::models::AppId;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::config::Config;

pub(crate) struct CachedToken {
    pub(crate) token: String,
    pub(crate) expires_at: Instant,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: Config,
    pub(crate) http_client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Body>,
    pub(crate) octocrab: octocrab::Octocrab,
    pub(crate) token_cache: Mutex<Option<CachedToken>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let http_client = build_http_client();
    let app_key = EncodingKey::from_rsa_pem(config.private_key.as_bytes())
        .context("loading GitHub App private key")?;
    let octocrab = octocrab::Octocrab::builder()
        .app(AppId(config.app_id), app_key)
        .base_uri(config.api_base.as_str())
        .context("building GitHub API base uri")?
        .build()
        .context("building octocrab client")?;
    let state = Arc::new(AppState {
        config,
        http_client,
        octocrab,
        token_cache: Mutex::new(None),
    });

    let addr = state.config.listen_addr;
    eprintln!("listening on {}", addr);

    let make_svc = make_service_fn(move |_| {
        let state = state.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| proxy::handle(req, state.clone())))
        }
    });

    Server::bind(&addr)
        .serve(make_svc)
        .await
        .context("server error")?;

    Ok(())
}

fn build_http_client() -> Client<hyper_rustls::HttpsConnector<HttpConnector>, Body> {
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .build();
    Client::builder().build(https)
}
