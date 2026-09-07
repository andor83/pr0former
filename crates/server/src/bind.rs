pub fn address(base: &str, host: Option<&str>, port: Option<&str>) -> Result<String, String> {
    if host.is_none() && port.is_none() {
        return Ok(base.to_owned());
    }
    let (base_host, base_port) = base.rsplit_once(':').ok_or("PR0_BIND requires host:port")?;
    let host = host.unwrap_or(base_host);
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if host.is_empty()
        || host
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '[' | ']'))
    {
        return Err("Invalid bind host".into());
    }
    let port = port
        .unwrap_or(base_port)
        .parse::<u16>()
        .map_err(|_| "Invalid bind port")?;
    if port == 0 {
        return Err("Bind port must be from 1 to 65535".into());
    }
    Ok(if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    })
}

#[cfg(test)]
mod tests {
    use super::address;

    #[test]
    fn component_overrides() {
        assert_eq!(address("0.0.0.0:4000", None, None).unwrap(), "0.0.0.0:4000");
        assert_eq!(
            address("127.0.0.1:4321", None, Some("4567")).unwrap(),
            "127.0.0.1:4567"
        );
        assert_eq!(
            address("0.0.0.0:4321", Some("localhost"), None).unwrap(),
            "localhost:4321"
        );
        assert_eq!(
            address("[::1]:4321", None, Some("4567")).unwrap(),
            "[::1]:4567"
        );
        for host in ["::1", "[::1]"] {
            assert_eq!(
                address("0.0.0.0:4000", Some(host), Some("4567")).unwrap(),
                "[::1]:4567"
            );
        }
        for port in ["0", "65536", "abc", "-1", ""] {
            assert!(address("0.0.0.0:4000", None, Some(port)).is_err());
        }
    }
}
