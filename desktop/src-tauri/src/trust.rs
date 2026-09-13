//! Explicit, per-origin leaf-certificate pins. Never changes the OS trust store.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager, Url, WebviewWindow};

#[derive(Clone, Serialize)]
pub struct CertificatePrompt {
    origin: String,
    fingerprint: String,
    changed: bool,
}
#[derive(Serialize)]
pub struct ConnectError {
    message: String,
    certificate: Option<CertificatePrompt>,
}
impl From<String> for ConnectError {
    fn from(message: String) -> Self {
        Self {
            message,
            certificate: None,
        }
    }
}
impl From<&str> for ConnectError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
pub struct TrustStore {
    path: PathBuf,
    pins: Mutex<BTreeMap<String, String>>,
}
#[derive(Clone)]
pub struct Pin {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
}
pub fn start(app: &AppHandle, path: PathBuf) {
    let pins = std::fs::read(&path)
        .ok()
        .filter(|b| b.len() < 1024 * 1024)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    app.manage(TrustStore {
        path,
        pins: Mutex::new(pins),
    });
}
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}
pub fn pin(app: &AppHandle, url: &Url) -> Option<Pin> {
    let store = app.try_state::<TrustStore>()?;
    let fingerprint = store
        .pins
        .lock()
        .ok()?
        .get(&url.origin().ascii_serialization())?
        .clone();
    Some(Pin {
        host: url.host_str()?.into(),
        port: url.port_or_known_default()?,
        fingerprint,
    })
}

pub async fn check(
    app: &AppHandle,
    url: &Url,
    approved: Option<&str>,
) -> Result<Option<Pin>, ConnectError> {
    let known = pin(app, url);
    if known.is_none() {
        match crate::connections::check_server(url).await {
            Ok(()) => return Ok(None),
            Err(error) if url.scheme() != "https" => return Err(error.into()),
            Err(_) => (),
        }
    }
    // Inspection sends no cookies, credentials, query strings, or request body.
    // The exception applies only to this inspection, never to the webview.
    let response = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .tls_info(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?
        .get(url.clone())
        .send()
        .await
        .map_err(|e| format!("Could not inspect the server certificate: {e}"))?;
    let der = response
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .and_then(|info| info.peer_certificate())
        .ok_or("The server did not provide a certificate to inspect.")?;
    let current = fingerprint(der);
    let matches = known.as_ref().is_some_and(|pin| pin.fingerprint == current);
    if !matches && approved != Some(current.as_str()) {
        return Err(ConnectError {
            message: if known.is_some() {
                "This server's certificate has changed. Verify the fingerprint with the server owner before trusting it again.".into()
            } else {
                "This server uses a certificate this Mac does not trust. Verify its fingerprint, then choose Trust This Server to remember it in pr0former only.".into()
            },
            certificate: Some(CertificatePrompt {
                origin: url.origin().ascii_serialization(),
                fingerprint: current,
                changed: known.is_some(),
            }),
        });
    }
    #[cfg(not(target_os = "macos"))]
    return Err("App-specific certificate trust is currently supported on macOS; on this platform install the server CA in the system trust store.".into());
    #[cfg(target_os = "macos")]
    {
        if response.status().is_redirection() {
            return Err("This pinned server redirects to another address. Enter the final HTTPS server address explicitly.".into());
        }
        response.error_for_status().map_err(|e| e.to_string())?;
        if !matches {
            let store = app.state::<TrustStore>();
            let mut pins = store.pins.lock().unwrap();
            let mut updated = pins.clone();
            updated.insert(url.origin().ascii_serialization(), current.clone());
            let temp = store.path.with_extension("tmp");
            std::fs::write(
                &temp,
                serde_json::to_vec_pretty(&updated).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            std::fs::rename(temp, &store.path).map_err(|e| e.to_string())?;
            *pins = updated;
        }
        Ok(Some(Pin {
            host: url.host_str().unwrap().into(),
            port: url.port_or_known_default().unwrap(),
            fingerprint: current,
        }))
    }
}
pub fn install(window: &WebviewWindow, pin: Option<Pin>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if let Some(pin) = pin {
        return macos::install(window, pin);
    }
    let _ = (window, pin);
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::Block;
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject, Bool, Imp, Sel},
        sel,
    };
    use objc2_foundation::NSString;
    use std::{
        ffi::{c_char, c_void},
        sync::OnceLock,
    };
    static PINS: OnceLock<Mutex<BTreeMap<usize, Pin>>> = OnceLock::new();
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    #[link(name = "objc")]
    unsafe extern "C" {
        fn class_addMethod(
            class: *const AnyClass,
            selector: Sel,
            implementation: Imp,
            types: *const c_char,
        ) -> Bool;
    }
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecTrustGetCertificateAtIndex(trust: *const c_void, index: isize) -> *const c_void;
        fn SecCertificateCopyData(certificate: *const c_void) -> *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataGetLength(data: *const c_void) -> isize;
        fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
        fn CFRelease(value: *const c_void);
    }

    // Optional WKNavigationDelegate method added to Wry's existing delegate.
    // All its navigation, downloads and page-load callbacks remain intact.
    unsafe extern "C-unwind" fn challenge(
        _delegate: *mut AnyObject,
        _selector: Sel,
        webview: *mut AnyObject,
        challenge: *mut AnyObject,
        completion: &Block<dyn Fn(isize, *mut AnyObject)>,
    ) {
        unsafe {
            let space: *mut AnyObject = msg_send![challenge, protectionSpace];
            let method: *const NSString = msg_send![space, authenticationMethod];
            if method.is_null() || (*method).to_string() != "NSURLAuthenticationMethodServerTrust" {
                completion.call((1, std::ptr::null_mut()));
                return;
            }
            let pin = PINS
                .get()
                .and_then(|pins| pins.lock().ok()?.get(&(webview as usize)).cloned());
            let Some(pin) = pin else {
                completion.call((1, std::ptr::null_mut()));
                return;
            };
            let host: *const NSString = msg_send![space, host];
            let port: isize = msg_send![space, port];
            if host.is_null()
                || !(*host).to_string().eq_ignore_ascii_case(&pin.host)
                || port != pin.port as isize
            {
                completion.call((1, std::ptr::null_mut()));
                return;
            }
            let trust: *const c_void = msg_send![space, serverTrust];
            let certificate = if trust.is_null() {
                std::ptr::null()
            } else {
                SecTrustGetCertificateAtIndex(trust, 0)
            };
            let data = if certificate.is_null() {
                std::ptr::null()
            } else {
                SecCertificateCopyData(certificate)
            };
            let accepted = if data.is_null() {
                false
            } else {
                let length = CFDataGetLength(data);
                let bytes = CFDataGetBytePtr(data);
                let matches = (1..=1024 * 1024).contains(&length)
                    && !bytes.is_null()
                    && fingerprint(std::slice::from_raw_parts(bytes, length as usize))
                        == pin.fingerprint;
                CFRelease(data);
                matches
            };
            if accepted {
                if let Some(class) = AnyClass::get(c"NSURLCredential") {
                    let credential: *mut AnyObject = msg_send![class, credentialForTrust: trust];
                    completion.call((0, credential));
                    return;
                }
            }
            completion.call((2, std::ptr::null_mut()));
        }
    }

    pub fn install(window: &WebviewWindow, pin: Pin) -> Result<(), String> {
        let handle = window.clone();
        window
            .with_webview(move |platform| unsafe {
                let webview = platform.inner() as *mut AnyObject;
                let delegate: *mut AnyObject = msg_send![webview, navigationDelegate];
                if delegate.is_null() {
                    return;
                }
                let installed = *INSTALLED.get_or_init(|| {
                    let class = (&*delegate).class();
                    let implementation: Imp = std::mem::transmute(
                        challenge as unsafe extern "C-unwind" fn(_, _, _, _, _),
                    );
                    class_addMethod(
                        class,
                        sel!(webView:didReceiveAuthenticationChallenge:completionHandler:),
                        implementation,
                        c"v@:@@@?".as_ptr(),
                    )
                    .as_bool()
                });
                if !installed {
                    eprintln!("Certificate trust hook unavailable; refusing the exception");
                    return;
                }
                let key = webview as usize;
                PINS.get_or_init(|| Mutex::new(BTreeMap::new()))
                    .lock()
                    .unwrap()
                    .insert(key, pin);
                // WebKit caches optional delegate selectors when setting the delegate.
                let _: () =
                    msg_send![webview, setNavigationDelegate: std::ptr::null_mut::<AnyObject>()];
                let _: () = msg_send![webview, setNavigationDelegate: delegate];
                handle.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::Destroyed) {
                        if let Some(pins) = PINS.get() {
                            if let Ok(mut pins) = pins.lock() {
                                pins.remove(&key);
                            }
                        }
                    }
                });
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_changes_with_certificate_bytes() {
        assert_eq!(
            fingerprint(b"abc"),
            "BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD"
        );
        assert_ne!(fingerprint(b"abc"), fingerprint(b"abd"));
    }
}
