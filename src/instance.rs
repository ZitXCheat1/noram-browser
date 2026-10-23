//! URL-only forwarding for OS default-browser launches. No general command IPC.
use crate::Action;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    time::Duration,
};
use tao::event_loop::EventLoopProxy;
#[derive(Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}
#[derive(Serialize, Deserialize)]
struct Open {
    token: String,
    url: String,
    foreground: bool,
}
pub fn listen(profile: &Path, proxy: EventLoopProxy<Action>) -> Result<()> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = Endpoint {
        port: listener.local_addr()?.port(),
        token: format!("{:?}", unsafe {
            windows::Win32::System::Com::CoCreateGuid()?
        }),
    };
    std::fs::write(
        profile.join("instance.json"),
        serde_json::to_vec(&endpoint)?,
    )?;
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            let mut packet = Vec::new();
            if (&mut stream).take(16385).read_to_end(&mut packet).is_err() || packet.len() > 16384 {
                continue;
            }
            let Ok(request) = serde_json::from_slice::<Open>(&packet) else {
                continue;
            };
            if request.token != endpoint.token || !valid_url(&request.url) {
                continue;
            }
            if proxy
                .send_event(Action::ExternalUrl(request.url, request.foreground))
                .is_ok()
            {
                let _ = stream.write_all(b"ok");
            }
        }
    });
    Ok(())
}
fn valid_url(raw: &str) -> bool {
    raw.len() <= 8192
        && url::Url::parse(raw).is_ok_and(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.username().is_empty()
                && u.password().is_none()
        })
}
pub fn forward(profile: &Path, url: &str, foreground: bool) -> Result<()> {
    anyhow::ensure!(
        valid_url(url),
        "Only ordinary HTTP/HTTPS links can be forwarded"
    );
    let path = profile.join("instance.json");
    anyhow::ensure!(
        std::fs::metadata(&path)?.len() < 4096,
        "Invalid instance metadata"
    );
    let endpoint: Endpoint = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, endpoint.port)),
        Duration::from_secs(3),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(&serde_json::to_vec(&Open {
        token: endpoint.token,
        url: url.into(),
        foreground,
    })?)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut ack = [0; 2];
    stream
        .read_exact(&mut ack)
        .context("Existing browser did not accept the link")?;
    anyhow::ensure!(&ack == b"ok", "Existing browser rejected the link");
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn url_forwarding_cannot_be_used_for_commands_or_active_schemes() {
        for input in [
            "javascript:alert(1)",
            "file:///secret",
            "https://user:password@example.org",
            "{\"cmd\":\"quit\"}",
        ] {
            assert!(!super::valid_url(input));
        }
        assert!(super::valid_url("https://example.org/path?a=b"));
    }
}
