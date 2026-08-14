//! Readiness polling: wait for a URL or TCP port to accept connections.

use std::net::TcpStream;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::error::OpsResult;
use crate::output;

/// Poll `curl -sf <url>` until it succeeds or `max_secs` elapses.
/// Mirrors the bash `wait_for_url`.
pub async fn wait_for_url(url: &str, max_secs: u64, label: &str) -> OpsResult<()> {
    let deadline = Instant::now() + Duration::from_secs(max_secs);
    loop {
        let status = Command::new("curl")
            .arg("-sf")
            .arg("--max-time")
            .arg("5")
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if matches!(status, Ok(s) if s.success()) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            output::fail(format!("{label} did not start within {max_secs}s"));
            return Err(crate::error::OpsError::Timeout(format!(
                "{label} not reachable at {url} within {max_secs}s"
            )));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Poll a TCP port until it accepts connections or `max_secs` elapses.
pub async fn wait_for_port(port: u16, max_secs: u64, label: &str) -> OpsResult<()> {
    let deadline = Instant::now() + Duration::from_secs(max_secs);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            output::fail(format!("{label} did not become ready on port {port} within {max_secs}s"));
            return Err(crate::error::OpsError::Timeout(format!(
                "{label} not ready on port {port} within {max_secs}s"
            )));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    async fn port_becomes_ready() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            drop(listener);
            // Bind again so the port remains open for the check.
            let _l2 = TcpListener::bind(("127.0.0.1", port)).unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let res = wait_for_port(port, 5, "test").await;
        assert!(res.is_ok());
        handle.abort();
    }

    #[tokio::test]
    async fn port_never_ready_times_out() {
        let res = wait_for_port(1, 1, "test").await; // port 1 unreachable
        assert!(res.is_err());
    }
}
