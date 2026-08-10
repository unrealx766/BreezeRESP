//! Update checking based on GitHub Releases (Atom feed + REST API, raced concurrently).
//! Executed on the Rust side because WebView `fetch` to github.com is blocked by CORS.
//! The HTTP client honors proxy environment variables and the Windows system proxy,
//! matching the route the WebView itself uses (direct connections can be much slower).

use serde::Serialize;
use std::io::Read;
use std::time::Duration;

const RELEASE_FEED_URL: &str = "https://github.com/unrealx766/BreezeRESP/releases.atom";
const RELEASE_API: &str = "https://api.github.com/repos/unrealx766/BreezeRESP/releases/latest";
const RELEASES_URL: &str = "https://github.com/unrealx766/BreezeRESP/releases";
/// Fail fast on the connect phase (DNS/TCP/TLS)
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Overall cap per source request
const TOTAL_TIMEOUT: Duration = Duration::from_secs(8);
/// Cap response size to keep memory bounded
const MAX_BODY_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestRelease {
    pub latest_version: String,
    pub release_url: String,
}

/// Proxy from environment variables first, then from the Windows system settings.
fn detect_proxy() -> Option<String> {
    for var in ["https_proxy", "HTTPS_PROXY", "http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(normalize_proxy(&v));
            }
        }
    }
    system_proxy()
}

#[cfg(windows)]
fn system_proxy() -> Option<String> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let enabled: u32 = key.get_value("ProxyEnable").ok()?;
    if enabled != 1 {
        return None;
    }
    let server: String = key.get_value("ProxyServer").ok()?;
    let server = server.trim();
    if server.is_empty() {
        return None;
    }
    Some(normalize_proxy(server))
}

#[cfg(not(windows))]
fn system_proxy() -> Option<String> {
    None
}

/// Accept "host:port", "scheme://host:port" or "http=a;https=b" style values.
fn normalize_proxy(server: &str) -> String {
    for prefix in ["https=", "http="] {
        for part in server.split(';') {
            if let Some(addr) = part.trim().strip_prefix(prefix) {
                return format!("http://{addr}");
            }
        }
    }
    if server.contains("://") {
        server.to_string()
    } else {
        format!("http://{server}")
    }
}

fn build_agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .user_agent("BreezeResp-updater");
    if let Some(url) = detect_proxy() {
        if let Ok(proxy) = ureq::Proxy::new(url.as_str()) {
            builder = builder.proxy(proxy);
        }
    }
    builder.build()
}

fn http_get(agent: &ureq::Agent, url: &str, accept: &str) -> Option<String> {
    let response = agent.get(url).set("Accept", accept).call().ok()?;
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_BODY_BYTES)
        .read_to_string(&mut body)
        .ok()?;
    Some(body)
}

/// Extract the first release tag from the Atom feed (entries are newest-first).
fn parse_feed_tag(xml: &str) -> Option<String> {
    const MARKER: &str = "/releases/tag/";
    let rest = xml.split(MARKER).nth(1)?;
    let end = rest
        .find(|c: char| c == '"' || c == '<' || c.is_whitespace())
        .unwrap_or(rest.len());
    let raw = &rest[..end];
    urlencoding::decode(raw).ok().map(|t| t.into_owned())
}

/// The releases Atom feed — a regular web resource that is NOT subject to
/// the GitHub REST API rate limit on shared egress IPs.
fn fetch_from_feed(agent: &ureq::Agent) -> Option<LatestRelease> {
    let xml = http_get(agent, RELEASE_FEED_URL, "application/atom+xml, application/xml")?;
    let tag = parse_feed_tag(&xml)?;
    Some(LatestRelease {
        latest_version: tag.clone(),
        release_url: format!("{RELEASES_URL}/tag/{tag}"),
    })
}

/// The REST API (may fail with 403 on rate limit).
fn fetch_from_api(agent: &ureq::Agent) -> Option<LatestRelease> {
    let body = http_get(agent, RELEASE_API, "application/vnd.github+json")?;
    let data: serde_json::Value = serde_json::from_str(&body).ok()?;
    let tag = data.get("tag_name")?.as_str()?.to_string();
    let release_url = data
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or(RELEASES_URL)
        .to_string();
    Some(LatestRelease {
        latest_version: tag,
        release_url,
    })
}

/// Fetch the latest release info. Both sources run concurrently and the
/// first success wins, so a slow/hanging route doesn't delay the result.
#[tauri::command]
pub async fn get_latest_release() -> Result<LatestRelease, String> {
    // ureq is blocking — run it off the async runtime
    tokio::task::spawn_blocking(|| {
        let agent = build_agent();
        let (tx, rx) = std::sync::mpsc::channel::<Option<LatestRelease>>();

        let feed_agent = agent.clone();
        let tx_feed = tx.clone();
        std::thread::spawn(move || tx_feed.send(fetch_from_feed(&feed_agent)).ok());

        let tx_api = tx; // moves the last sender into the API thread
        let api_agent = agent.clone();
        std::thread::spawn(move || tx_api.send(fetch_from_api(&api_agent)).ok());

        for _ in 0..2 {
            match rx.recv() {
                Ok(Some(release)) => return Some(release), // first success wins
                Ok(None) => continue,                      // this source failed
                Err(_) => break,                           // both senders gone
            }
        }
        None
    })
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Failed to fetch latest release info".to_string())
}
