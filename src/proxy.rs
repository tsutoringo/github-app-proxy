use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use hyper::header::{self, HeaderMap, HeaderName, HeaderValue};
use hyper::{Body, Request, Response, StatusCode, Uri};
use std::sync::Arc;
use url::Url;

use crate::config::with_trailing_slash;
use crate::{github, AppState};

#[derive(Clone, Copy)]
enum RouteTarget {
    Git,
    RestApi,
    Mcp,
}

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
    let token = github::get_cached_token(&state).await?;
    let route = route_target(req.uri().path());
    let target_base = match route {
        RouteTarget::Git => &state.config.git_base,
        RouteTarget::RestApi => &state.config.api_base,
        RouteTarget::Mcp => &state.config.githubcopilot_api_base,
    };
    let path_and_query = rewritten_path_and_query(req.uri(), route);

    let target_uri = build_target_uri(target_base, &path_and_query)?;
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
        let auth_header = match route {
            RouteTarget::Git => build_basic_header(&token)?,
            RouteTarget::RestApi | RouteTarget::Mcp => build_bearer_header(&token)?,
        };
        headers.insert(header::AUTHORIZATION, auth_header);
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

fn is_mcp_request(path: &str) -> bool {
    path == "/mcp" || path.starts_with("/mcp/")
}

fn is_rest_api_request(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

fn route_target(path: &str) -> RouteTarget {
    if is_mcp_request(path) {
        RouteTarget::Mcp
    } else if is_rest_api_request(path) {
        RouteTarget::RestApi
    } else {
        RouteTarget::Git
    }
}

fn rewritten_path_and_query(uri: &Uri, route: RouteTarget) -> String {
    let path = match route {
        RouteTarget::RestApi => strip_api_prefix(uri.path()),
        RouteTarget::Git | RouteTarget::Mcp => uri.path(),
    };

    match uri.query() {
        Some(query) => format!("{}?{}", path, query),
        None => path.to_string(),
    }
}

fn strip_api_prefix(path: &str) -> &str {
    match path.strip_prefix("/api") {
        Some("") | None => "/",
        Some(stripped) => stripped,
    }
}

fn build_bearer_header(token: &str) -> Result<HeaderValue> {
    let value = format!("Bearer {}", token);
    HeaderValue::from_str(&value).context("invalid authorization header")
}

#[cfg(test)]
mod tests {
    use super::{rewritten_path_and_query, route_target, RouteTarget};
    use hyper::Uri;

    #[test]
    fn routes_api_prefix_to_rest_api() {
        assert!(matches!(route_target("/api"), RouteTarget::RestApi));
        assert!(matches!(route_target("/api/repos/octo/example"), RouteTarget::RestApi));
        assert!(matches!(route_target("/apix"), RouteTarget::Git));
    }

    #[test]
    fn strips_api_prefix_when_forwarding_rest_requests() {
        let uri: Uri = "/api/repos/octo/example/issues?per_page=100"
            .parse()
            .expect("valid uri");

        assert_eq!(
            rewritten_path_and_query(&uri, RouteTarget::RestApi),
            "/repos/octo/example/issues?per_page=100"
        );
    }

    #[test]
    fn rewrites_api_root_to_rest_root() {
        let uri: Uri = "/api?per_page=100".parse().expect("valid uri");

        assert_eq!(
            rewritten_path_and_query(&uri, RouteTarget::RestApi),
            "/?per_page=100"
        );
    }
}
