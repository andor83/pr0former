//! Opt-in local-network HTTPS hosting for embedded runtimes. TLS preparation
//! and discovery stay off audio threads, and the certificate directory comes
//! from the host's typed configuration rather than the process environment.
use axum::{
    Router,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub struct Hosting {
    pub status: Value,
    handle: axum_server::Handle,
    https: tokio::task::JoinHandle<()>,
    setup: tokio::task::JoinHandle<()>,
    _discovery: Option<crate::discovery::Advertisement>,
}
impl Drop for Hosting {
    fn drop(&mut self) {
        self.handle.shutdown();
        self.https.abort();
        self.setup.abort();
    }
}
struct Certificates {
    certificate: PathBuf,
    key: PathBuf,
    ca_der: Vec<u8>,
    ca_path: PathBuf,
    hostname: String,
}

fn certificates(directory: &Path) -> Result<Certificates, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let hostname = crate::discovery::host_label(
        &hostname::get()
            .map_err(|e| e.to_string())?
            .to_string_lossy(),
    );
    let mut names = vec![
        "localhost".into(),
        "127.0.0.1".into(),
        "::1".into(),
        hostname.clone(),
        format!("{hostname}.local"),
    ];
    for interface in if_addrs::get_if_addrs().map_err(|e| e.to_string())? {
        if !interface.is_loopback() {
            names.push(interface.ip().to_string());
        }
    }
    names.sort();
    names.dedup();
    let ca_path = directory.join("ca.pem");
    let ca_key_path = directory.join("ca-key.pem");
    let now = time::OffsetDateTime::now_utc();
    let (ca, ca_key) = if ca_path.exists() || ca_key_path.exists() {
        let key =
            KeyPair::from_pem(&std::fs::read_to_string(&ca_key_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let params = CertificateParams::from_ca_cert_pem(
            &std::fs::read_to_string(&ca_path).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        (params.self_signed(&key).map_err(|e| e.to_string())?, key)
    } else {
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
        params
            .distinguished_name
            .push(DnType::CommonName, "pr0former Performance CA");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.not_before = now - time::Duration::days(1);
        params.not_after = now + time::Duration::days(3650);
        let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
        private_write(&ca_key_path, key.serialize_pem().as_bytes())?;
        private_write(&ca_path, cert.pem().as_bytes())?;
        (cert, key)
    };
    let certificate = directory.join("server.pem");
    let key_path = directory.join("server-key.pem");
    let metadata_path = directory.join("server.json");
    let metadata: Value = std::fs::read(&metadata_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    if !certificate.exists()
        || !key_path.exists()
        || metadata["format"] != 1
        || metadata["names"] != json!(names)
        || metadata["expires"].as_i64().unwrap_or(0)
            < (now + time::Duration::days(7)).unix_timestamp()
    {
        let key = KeyPair::generate().map_err(|e| e.to_string())?;
        let mut params = CertificateParams::new(names.clone()).map_err(|e| e.to_string())?;
        params
            .distinguished_name
            .push(DnType::CommonName, "pr0former performance server");
        params.use_authority_key_identifier_extension = true;
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.not_before = now - time::Duration::days(1);
        params.not_after = now + time::Duration::days(397);
        let expires = params.not_after.unix_timestamp();
        let cert = params
            .signed_by(&key, &ca, &ca_key)
            .map_err(|e| e.to_string())?;
        private_write(&key_path, key.serialize_pem().as_bytes())?;
        private_write(
            &certificate,
            format!(
                "{}{}",
                cert.pem(),
                std::fs::read_to_string(&ca_path).map_err(|e| e.to_string())?
            )
            .as_bytes(),
        )?;
        private_write(
            &metadata_path,
            json!({"format":1,"names":names,"expires":expires})
                .to_string()
                .as_bytes(),
        )?;
    }
    // Export the original CA bytes, not a freshly re-signed reconstruction.
    let original = std::fs::read_to_string(&ca_path).map_err(|e| e.to_string())?;
    let encoded: String = original
        .lines()
        .filter(|line| !line.starts_with("---"))
        .collect();
    let ca_der = STANDARD.decode(encoded).map_err(|e| e.to_string())?;
    Ok(Certificates {
        certificate,
        key: key_path,
        ca_der,
        ca_path,
        hostname,
    })
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| e.to_string())
}

/// `private_token` is the embedded owner's session: LAN clients presenting it
/// are rejected, so the private identity can never leave the loopback listener.
pub async fn start(
    config: &crate::config::RuntimeConfig,
    router: Router,
    private_token: String,
    port: u16,
) -> Result<Hosting, String> {
    let directory = config.hosting_dir();
    let discovery = config.discovery;
    let certs = tokio::task::spawn_blocking(move || certificates(&directory))
        .await
        .map_err(|e| e.to_string())??;
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(&certs.certificate, &certs.key)
        .await
        .map_err(|e| e.to_string())?;
    let listener = std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))
        .map_err(|e| format!("Cannot open HTTPS port {port}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    let url = format!("https://{}.local:{}", certs.hostname, address.port());
    let setup_listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| e.to_string())?;
    let setup_port = setup_listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    let fingerprint = Sha256::digest(&certs.ca_der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":");
    let leaf_pem = std::fs::read_to_string(&certs.certificate).map_err(|e| e.to_string())?;
    let leaf_encoded: String = leaf_pem
        .lines()
        .skip(1)
        .take_while(|line| !line.starts_with("---"))
        .collect();
    let leaf = STANDARD.decode(leaf_encoded).map_err(|e| e.to_string())?;
    let server_fingerprint = Sha256::digest(&leaf)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":");
    let profile = profile(&certs.ca_der);
    let ca_der = certs.ca_der.clone();
    let profile_route = move || {
        let profile = profile.clone();
        async move {
            (
                [
                    (header::CONTENT_TYPE, "application/x-apple-aspen-config"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=pr0former.mobileconfig",
                    ),
                ],
                profile,
            )
        }
    };
    let ca_route = move || {
        let ca = ca_der.clone();
        async move {
            (
                [
                    (header::CONTENT_TYPE, "application/x-x509-ca-cert"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=pr0former-ca.cer",
                    ),
                ],
                ca,
            )
        }
    };
    let page = format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>pr0former certificate setup</title><style>body{{font:18px system-ui;max-width:38em;margin:3em auto;padding:1em;line-height:1.6}}a{{display:block;padding:12px}}</style><h1>Connect to pr0former</h1><p>On iPad, install the certificate profile, then enable full trust for pr0former Performance CA in Settings → General → About → Certificate Trust Settings.</p><p>Compare this CA SHA-256 fingerprint with the hosting dialog on the laptop before installing: <code style='overflow-wrap:anywhere'>{fingerprint}</code></p><a href=/pr0former.mobileconfig>Download iPad certificate profile</a><a href=/pr0former-ca.cer>Download CA certificate for other devices</a><a href='{url}'>Open the performance server</a><p>Sign in using an invited account. The certificate does not grant access to projects.</p>"
    );
    let setup_router = Router::new()
        .route(
            "/",
            get(move || {
                let page = page.clone();
                async move { Html(page) }
            }),
        )
        .route("/pr0former.mobileconfig", get(profile_route.clone()))
        .route("/pr0former-ca.cer", get(ca_route.clone()));
    let router = router
        .route("/pr0former.mobileconfig", get(profile_route))
        .route("/pr0former-ca.cer", get(ca_route))
        .layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                // Every `cookie` field is checked: an HTTP/2 client may carry
                // the private token in a second field, which must not slip
                // past this guard any more than a first-field copy would.
                let private_session = req
                    .headers()
                    .get_all(header::COOKIE)
                    .iter()
                    .filter_map(|v| v.to_str().ok())
                    .flat_map(|cookie| cookie.split(';'))
                    .any(|part| {
                        part.trim().strip_prefix("pr0_session=") == Some(private_token.as_str())
                    });
                async move {
                    if private_session {
                        return StatusCode::FORBIDDEN.into_response();
                    }
                    let mut response = next.run(req).await;
                    let cookies: Vec<_> = response
                        .headers()
                        .get_all(header::SET_COOKIE)
                        .iter()
                        .cloned()
                        .collect();
                    response.headers_mut().remove(header::SET_COOKIE);
                    for cookie in cookies {
                        if let Ok(value) = cookie.to_str() {
                            let secure = format!("{value}; Secure");
                            if let Ok(value) = secure.parse() {
                                response.headers_mut().append(header::SET_COOKIE, value);
                            }
                        }
                    }
                    response
                }
            },
        ));
    let handle = axum_server::Handle::new();
    let server = axum_server::from_tcp_rustls(listener, tls).handle(handle.clone());
    let https = tokio::spawn(async move {
        if let Err(error) = server.serve(router.into_make_service()).await {
            tracing::warn!(%error,"Desktop HTTPS listener stopped");
        }
    });
    let setup = tokio::spawn(async move {
        let _ = axum::serve(setup_listener, setup_router).await;
    });
    let discovery = crate::discovery::advertise(address, true, discovery);
    Ok(Hosting {
        status: json!({"enabled":true,"url":url,"setup_url":format!("http://{}.local:{setup_port}",certs.hostname),"port":address.port(),"ca_path":certs.ca_path,"ca_fingerprint":fingerprint,"server_fingerprint":server_fingerprint}),
        handle,
        https,
        setup,
        _discovery: discovery,
    })
}
fn profile(ca: &[u8]) -> String {
    let identity = format!("{:x}", Sha256::digest(ca));
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>PayloadType</key><string>Configuration</string><key>PayloadVersion</key><integer>1</integer><key>PayloadIdentifier</key><string>org.pr0former.performance-ca.{identity}</string><key>PayloadUUID</key><string>{}</string><key>PayloadDisplayName</key><string>pr0former Performance CA</string><key>PayloadContent</key><array><dict><key>PayloadType</key><string>com.apple.security.root</string><key>PayloadVersion</key><integer>1</integer><key>PayloadIdentifier</key><string>org.pr0former.performance-ca.{identity}.root</string><key>PayloadUUID</key><string>{}</string><key>PayloadDisplayName</key><string>pr0former Performance CA</string><key>PayloadContent</key><data>{}</data></dict></array></dict></plist>",
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        STANDARD.encode(ca)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_certificates_reuse_the_ca_and_unchanged_leaf() {
        let directory =
            std::env::temp_dir().join(format!("pr0-host-cert-{}", uuid::Uuid::new_v4()));
        let first = certificates(&directory).unwrap();
        let leaf = std::fs::read(&first.certificate).unwrap();
        let second = certificates(&directory).unwrap();
        assert_eq!(first.ca_der, second.ca_der);
        assert_eq!(leaf, std::fs::read(second.certificate).unwrap());
        assert!(profile(&first.ca_der).contains("com.apple.security.root"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
