use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, Uri, uri::Authority},
    response::Redirect,
    routing::any,
};
use std::path::PathBuf;

pub struct Config {
    pub pair: Option<(PathBuf, PathBuf)>,
    pub port: Option<String>,
}
impl Config {
    pub fn load() -> Self {
        let directory = PathBuf::from("certs");
        let legacy = PathBuf::from(".local/ssl");
        if !directory.exists() && legacy.is_dir() {
            std::fs::rename(&legacy, &directory).expect("Move legacy TLS certificates to certs/");
        }
        let disabled = std::env::var("PR0_NO_SSL").is_ok_and(|s| s == "1")
            || directory.join("disabled").exists();
        let cert = directory.join("server.pem");
        let key = directory.join("server-key.pem");
        let managed = cert.exists() || key.exists() || directory.join("disabled").exists();
        let pair = if disabled {
            None
        } else if cert.exists() || key.exists() {
            assert!(
                cert.is_file() && key.is_file(),
                "Incomplete managed TLS certificate; run init.sh --setup-ssl"
            );
            Some((cert, key))
        } else {
            match (
                std::env::var_os("PR0_TLS_CERT"),
                std::env::var_os("PR0_TLS_KEY"),
            ) {
                (None, None) => None,
                (Some(cert), Some(key)) => Some((cert.into(), key.into())),
                _ => panic!("Both PR0_TLS_CERT and PR0_TLS_KEY are required"),
            }
        };
        let port = std::env::var("PR0_PORT").ok().or_else(|| {
            (managed || disabled).then(|| if pair.is_some() { "443" } else { "80" }.into())
        });
        Self { pair, port }
    }
}
fn destination(host: &str, uri: &Uri, port: u16) -> Result<String, StatusCode> {
    let authority: Authority = host.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    if authority.as_str().contains('@') {
        return Err(StatusCode::BAD_REQUEST);
    }
    let host = authority.host();
    let suffix = if port == 443 {
        String::new()
    } else {
        format!(":{port}")
    };
    Ok(format!(
        "https://{host}{suffix}{}",
        uri.path_and_query().map(|p| p.as_str()).unwrap_or("/")
    ))
}
async fn redirect(
    State(port): State<u16>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Redirect, StatusCode> {
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    Ok(Redirect::temporary(&destination(host, &uri, port)?))
}
pub fn redirects(port: u16) -> Router {
    Router::new().fallback(any(redirect)).with_state(port)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirect_preserves_paths_queries_and_ipv6() {
        let uri = "/a%20b?x=2".parse().unwrap();
        assert_eq!(
            destination("studio.local:80", &uri, 443).unwrap(),
            "https://studio.local/a%20b?x=2"
        );
        assert_eq!(
            destination("[::1]:8080", &uri, 8443).unwrap(),
            "https://[::1]:8443/a%20b?x=2"
        );
        assert!(destination("bad host", &uri, 443).is_err());
        assert!(destination("user@host", &uri, 443).is_err());
    }
}
