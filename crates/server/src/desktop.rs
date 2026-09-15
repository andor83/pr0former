//! The legacy `pr0-server --desktop` child-process protocol, and nothing else.
//!
//! This module is a host adapter: it reads control lines from stdin, prints the
//! `PR0_DESKTOP_READY` / `PR0_DESKTOP_HOSTING` handshake to stdout, and drives
//! the same embeddable API (`runtime::start` and [`RuntimeHandle`]) that an
//! in-process host uses. No lifecycle, identity, listener or hosting policy
//! lives here any more — see `crate::runtime`.
//!
//! Authentication is still delivered over the parent's private pipe, never an
//! HTTP login bypass, URL credential, default password, or log entry.
use crate::*;
use std::io::{BufRead, Write};
use std::process::ExitCode;

pub async fn run(config: RuntimeConfig) -> ExitCode {
    let runtime = match crate::runtime::start(config).await {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Desktop runtime failed to start: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (control_tx, mut control_rx) = tokio::sync::mpsc::unbounded_channel();
    // A standard thread avoids Tokio stdin's uncancellable blocking read at runtime exit.
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if value["hosting"].is_boolean() {
                    if control_tx.send(Some(value)).is_err() {
                        return;
                    }
                    continue;
                }
            }
            break; // Preserve the legacy stop-on-command behavior for other input.
        }
        let _ = control_tx.send(None); // EOF (including parent crash) stops both listeners.
    });
    let ready = runtime.readiness();
    println!(
        "PR0_DESKTOP_READY {}",
        json!({"url":ready.url,"session":ready.session.clone().unwrap_or_default()})
    );
    std::io::stdout().flush().expect("Notify desktop launcher");
    loop {
        tokio::select! {
            served = runtime.serving() => {
                if let Err(error) = served { eprintln!("Desktop listener stopped: {error}"); }
                break;
            }
            control = control_rx.recv() => {
                let Some(Some(value)) = control else { break; };
                let status = if value["hosting"].as_bool() == Some(false) {
                    runtime.stop_hosting().await
                } else {
                    let port = value["port"].as_u64().unwrap_or(8443);
                    if port > u16::MAX as u64 {
                        json!({"enabled":false,"error":"Invalid hosting port"})
                    } else {
                        match runtime.host_on_lan(port as u16).await {
                            Ok(status) => status,
                            Err(error) => json!({"enabled":false,"error":error}),
                        }
                    }
                };
                println!("PR0_DESKTOP_HOSTING {status}");
                let _ = std::io::stdout().flush();
            }
        }
    }
    match runtime.shutdown().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Desktop shutdown failed: {error}");
            ExitCode::FAILURE
        }
    }
}
