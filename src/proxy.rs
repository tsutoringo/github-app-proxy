use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use hyper::header::{self, HeaderMap, HeaderName, HeaderValue};
use hyper::{Body, Request, Response, StatusCode, Uri};
use std::sync::Arc;
use url::Url;

use crate::config::with_trailing_slash;
use crate::{github, AppState};

pub(crate) async fn handle(
    req: Request<Body>,
    state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    if req.uri().path() == "/healthz" {
        return Ok(health_response());
    }

    let response = match proxy_request(req, state).await {
        Ok(response) => response,
        Err(err) => {
            eprintln!("proxy error: {:#}", err);
            bad_gateway_response()
        }
    };

    Ok(response)
}

async fn proxy_request(req: Request<Body>, state: Arc<AppState>) -> Result<Response<Body>> {
    let token = github::get_cached_token(&state)
        .await?;
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let target_uri = build_target_uri(&state.config.git_base, path_and_query)?;
    let authority = target_uri
        .authority()
        .map(|value| value.as_str().to_string())
        .context("missing target authority")?;

    let mut builder = Request::builder()
        .method(req.method())
        .uri(target_uri)
        .version(req.version());

    {
        let headers = builder
            .headers_mut()
            .context("building request headers")?;
        copy_headers(req.headers(), headers);
        headers.insert(header::AUTHORIZATION, build_basic_header(&token)?);
        headers.insert(
            header::HOST,
            HeaderValue::from_str(&authority).context("invalid host header")?,
        );
    }

    let outbound = builder
        .body(req.into_body())
        .context("building outbound request")?;

    let response = state
        .http_client
        .request(outbound)
        .await
        .context("proxy request failed")?;

    Ok(response)
}

fn health_response() -> Response<Body> {
    let mut response = Response::new(Body::from("ok"));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain"),
    );
    response
}

fn bad_gateway_response() -> Response<Body> {
    let mut response = Response::new(Body::from("bad gateway"));
    *response.status_mut() = StatusCode::BAD_GATEWAY;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain"),
    );
    response
}

fn build_target_uri(base: &Url, path_and_query: &str) -> Result<Uri> {
    let base = with_trailing_slash(base);
    let relative = path_and_query.trim_start_matches('/');
    let url = base.join(relative).context("joining target URL")?;
    url.as_str().parse::<Uri>().context("parsing target URI")
}

fn build_basic_header(token: &str) -> Result<HeaderValue> {
    let credentials = format!("x-access-token:{}", token);
    let encoded = general_purpose::STANDARD.encode(credentials);
    let value = format!("Basic {}", encoded);
    HeaderValue::from_str(&value).context("invalid authorization header")
}

fn copy_headers(src: &HeaderMap, dst: &mut HeaderMap) {
    for (name, value) in src.iter() {
        if is_hop_header(name) || name == header::AUTHORIZATION || name == header::HOST {
            continue;
        }
        dst.append(name.clone(), value.clone());
    }
}

fn is_hop_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
