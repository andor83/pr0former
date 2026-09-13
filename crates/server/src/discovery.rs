//! LAN discovery runs on the mDNS worker, never on an audio callback.
use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use std::net::SocketAddr;

const SERVICE: &str = "_pr0former._tcp.local.";
pub struct Advertisement {
    daemon: ServiceDaemon,
    fullname: String,
}
impl Drop for Advertisement {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

pub(crate) fn host_label(hostname: &str) -> String {
    let label: String = hostname
        .trim_end_matches('.')
        .trim_end_matches(".local")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .take(48)
        .collect();
    let label = label.trim_matches('-');
    if label.is_empty() {
        "pr0former".into()
    } else {
        label.into()
    }
}

fn service(socket: SocketAddr, https: bool, hostname: &str) -> Result<ServiceInfo, String> {
    if socket.ip().is_loopback() || socket.port() == 0 {
        return Err("Loopback-only servers are not advertised".into());
    }
    let label = host_label(hostname);
    let name = format!("pr0former on {label} ({})", socket.port());
    let host = format!("{label}.local.");
    let props = [
        ("version", "1"),
        ("scheme", if https { "https" } else { "http" }),
    ];
    let addresses = if socket.ip().is_unspecified() {
        String::new()
    } else {
        socket.ip().to_string()
    };
    let mut info = ServiceInfo::new(
        SERVICE,
        &name,
        &host,
        addresses.as_str(),
        socket.port(),
        &props[..],
    )
    .map_err(|e| e.to_string())?;
    if socket.ip().is_unspecified() {
        info = info.enable_addr_auto();
    }
    Ok(info)
}

pub fn advertise(socket: SocketAddr, https: bool) -> Option<Advertisement> {
    if socket.ip().is_loopback() || std::env::var("PR0_DISCOVERY").as_deref() == Ok("0") {
        return None;
    }
    let result = (|| -> Result<Advertisement, String> {
        let hostname = hostname::get()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        let info = service(socket, https, &hostname)?;
        let fullname = info.get_fullname().to_owned();
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let advertisement = Advertisement { daemon, fullname };
        let daemon = &advertisement.daemon;
        daemon
            .disable_interface(vec![IfKind::LoopbackV4, IfKind::LoopbackV6])
            .map_err(|e| e.to_string())?;
        if !socket.ip().is_unspecified() {
            daemon
                .disable_interface(IfKind::All)
                .map_err(|e| e.to_string())?;
            daemon
                .enable_interface(IfKind::Addr(socket.ip()))
                .map_err(|e| e.to_string())?;
        } else if socket.is_ipv4() {
            daemon
                .disable_interface(IfKind::IPv6)
                .map_err(|e| e.to_string())?;
        }
        daemon.register(info).map_err(|e| e.to_string())?;
        Ok(advertisement)
    })();
    match result {
        Ok(advertisement) => Some(advertisement),
        Err(error) => {
            tracing::warn!(%error, "LAN discovery unavailable; manual connections still work");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advertises_the_actual_protocol_and_port_without_credentials() {
        let info = service("192.168.1.3:8443".parse().unwrap(), true, "Studio.local").unwrap();
        assert_eq!(info.get_hostname(), "Studio.local.");
        assert_eq!(info.get_port(), 8443);
        assert_eq!(info.get_property_val_str("scheme"), Some("https"));
        assert_eq!(info.get_properties().len(), 2);
    }
    #[test]
    fn private_desktop_and_unbound_ports_are_not_advertised() {
        for socket in ["127.0.0.1:8080", "[::1]:8080", "0.0.0.0:0"] {
            assert!(service(socket.parse().unwrap(), false, "test").is_err());
        }
    }
}
