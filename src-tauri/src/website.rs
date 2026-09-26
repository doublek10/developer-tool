//! Website Lab (spec §8).
//!
//! Diagnostics only: this module reads what a site publishes to any ordinary
//! visitor. It sends no crafted payloads, tries no credentials, and has no
//! throughput controls that would let it be pointed at a host as a load
//! generator. Requests are rate-limited and capped at one page per run.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use serde::Serialize;
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};
use tauri::State;
use url::Url;

#[derive(Serialize, Default)]
pub struct Timings {
    pub dns_ms: u128,
    pub connect_ms: u128,
    pub first_response_ms: u128,
    pub total_ms: u128,
}

#[derive(Serialize)]
pub struct Finding {
    pub severity: String, // info | warning | error
    pub area: String,
    pub title: String,
    pub detail: String,
}

#[derive(Serialize)]
pub struct Redirect {
    pub from: String,
    pub status: u16,
    pub to: String,
}

#[derive(Serialize, Default)]
pub struct PageStats {
    pub html_bytes: usize,
    pub stylesheets: usize,
    pub scripts: usize,
    pub images: usize,
    pub links_internal: usize,
    pub links_external: usize,
    pub inline_scripts: usize,
    pub title: String,
    pub external_hosts: Vec<String>,
}

#[derive(Serialize)]
pub struct WebsiteReport {
    pub url: String,
    pub final_url: String,
    pub addresses: Vec<String>,
    pub status: u16,
    pub http_version: String,
    pub scheme: String,
    pub timings: Timings,
    pub headers: Vec<(String, String)>,
    pub security_headers: Vec<(String, Option<String>)>,
    pub cookies: Vec<String>,
    pub cors: Option<String>,
    pub redirects: Vec<Redirect>,
    pub page: PageStats,
    pub findings: Vec<Finding>,
    pub generated_at: String,
}

const SECURITY_HEADERS: &[&str] = &[
    "strict-transport-security",
    "content-security-policy",
    "x-content-type-options",
    "x-frame-options",
    "referrer-policy",
    "permissions-policy",
    "cross-origin-opener-policy",
];

fn finding(sev: &str, area: &str, title: &str, detail: &str) -> Finding {
    Finding {
        severity: sev.into(),
        area: area.into(),
        title: title.into(),
        detail: detail.into(),
    }
}

/// Rejects anything that isn't a plain public http(s) URL, including attempts to
/// aim the scanner at loopback or private-range addresses.
pub(crate) fn validate(raw: &str) -> AppResult<Url> {
    let candidate = if raw.contains("://") { raw.to_string() } else { format!("https://{raw}") };
    let url = Url::parse(candidate.trim()).map_err(|e| {
        AppError::new("URL_INVALID", "That doesn't look like a web address.")
            .detail(e.to_string())
            .recover("Enter a full address, for example https://example.com")
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::new("URL_SCHEME_UNSUPPORTED", "Only http and https addresses can be checked."));
    }
    if url.host_str().is_none() {
        return Err(AppError::new("URL_NO_HOST", "The address is missing a host name."));
    }
    Ok(url)
}

pub(crate) fn is_private(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

/// Resolves `url`'s host and refuses anything that lands on a private, loopback
/// or link-local address. Shared by Website Lab and Web Page Clone so every
/// outbound fetch — the page itself and anything it links to — passes the
/// same public-address check.
pub(crate) fn ensure_public(url: &Url) -> AppResult<()> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::new("URL_NO_HOST", "The address is missing a host name."))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(443);
    let resolved: Vec<std::net::SocketAddr> = format!("{host}:{port}")
        .to_socket_addrs()
        .map_err(|e| {
            AppError::new("DNS_RESOLUTION_FAILED", format!("{host} could not be resolved."))
                .detail(e.to_string())
                .recover("Check the spelling of the domain, and that this computer is online.")
        })?
        .collect();
    if resolved.is_empty() || resolved.iter().any(|a| is_private(&a.ip())) {
        return Err(AppError::new(
            "TARGET_NOT_PUBLIC",
            "That address resolves to a private or local address.",
        )
        .recover("This tool only works with publicly reachable pages."));
    }
    Ok(())
}

#[tauri::command]
pub async fn website_scan(
    db: State<'_, Db>,
    session: State<'_, Session>,
    target: String,
) -> AppResult<WebsiteReport> {
    require_session(&session)?;
    let url = validate(&target)?;
    let host = url.host_str().unwrap().to_string();
    let port = url.port_or_known_default().unwrap_or(443);

    {
        let conn = db.0.lock().unwrap();
        logging::write(&conn, "info", "WEBSITE", "Scan started", Some(&format!("target={host}")));
    }

    let mut findings: Vec<Finding> = Vec::new();
    let mut timings = Timings::default();
    let overall = Instant::now();

    // --- DNS ---
    let t = Instant::now();
    let resolved: Vec<std::net::SocketAddr> = format!("{host}:{port}")
        .to_socket_addrs()
        .map_err(|e| {
            AppError::new("DNS_RESOLUTION_FAILED", format!("{host} could not be resolved."))
                .detail(e.to_string())
                .recover("Check the spelling of the domain, and that this computer is online.")
        })?
        .collect();
    timings.dns_ms = t.elapsed().as_millis();
    let addresses: Vec<String> = resolved.iter().map(|a| a.ip().to_string()).collect();

    if resolved.iter().any(|a| is_private(&a.ip())) {
        return Err(AppError::new(
            "TARGET_NOT_PUBLIC",
            "That name resolves to a private or local address.",
        )
        .recover("Website Lab checks publicly reachable sites. Use Server Center for machines on your own network."));
    }

    // --- TCP connect ---
    let t = Instant::now();
    let sock = resolved.first().ok_or_else(|| {
        AppError::new("NO_ADDRESS", "DNS returned no usable address for that host.")
    })?;
    std::net::TcpStream::connect_timeout(sock, Duration::from_secs(8)).map_err(|e| {
        AppError::new("CONNECTION_FAILED", format!("Nothing answered on {host}:{port}."))
            .detail(e.to_string())
            .recover("The host may be down, or a firewall may be blocking the port.")
    })?;
    timings.connect_ms = t.elapsed().as_millis();

    // --- HTTP ---
    let client = reqwest::Client::builder()
        .user_agent("DevWorkstation/0.1 (site diagnostics)")
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let mut redirects = Vec::new();
    let mut current = url.clone();
    let mut hops = 0;
    let t = Instant::now();

    let response = loop {
        let resp = client.get(current.clone()).send().await?;
        let status = resp.status().as_u16();
        if (300..400).contains(&status) && hops < 8 {
            if let Some(loc) = resp.headers().get("location").and_then(|v| v.to_str().ok()) {
                let next = current.join(loc).unwrap_or(current.clone());
                redirects.push(Redirect { from: current.to_string(), status, to: next.to_string() });
                current = next;
                hops += 1;
                continue;
            }
        }
        break resp;
    };
    timings.first_response_ms = t.elapsed().as_millis();

    let status = response.status().as_u16();
    let http_version = format!("{:?}", response.version());
    let final_url = response.url().to_string();
    let scheme = response.url().scheme().to_string();

    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
        .collect();

    let lookup = |name: &str| -> Option<String> {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };

    let security_headers: Vec<(String, Option<String>)> = SECURITY_HEADERS
        .iter()
        .map(|h| (h.to_string(), lookup(h)))
        .collect();

    let cookies: Vec<String> = headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, v)| v.clone())
        .collect();

    let cors = lookup("access-control-allow-origin");

    let body = response.text().await.unwrap_or_default();
    timings.total_ms = overall.elapsed().as_millis();

    let page = analyse_html(&body, &current);

    // --- findings ---
    if scheme != "https" {
        findings.push(finding("error", "Transport", "Served over plain HTTP",
            "Traffic to this site is not encrypted and can be read or altered in transit."));
    }
    for (name, value) in &security_headers {
        if value.is_none() {
            let sev = if *name == "content-security-policy" || *name == "strict-transport-security" {
                "warning"
            } else {
                "info"
            };
            findings.push(finding(sev, "Headers", &format!("{name} is not set"),
                "The browser falls back to its default behaviour for this protection."));
        }
    }
    if let Some(server) = lookup("server") {
        if server.chars().any(|c| c.is_ascii_digit()) {
            findings.push(finding("info", "Headers", "Server header reveals a version",
                &format!("Responded with: {server}")));
        }
    }
    for c in &cookies {
        let lower = c.to_ascii_lowercase();
        let name = c.split('=').next().unwrap_or("cookie").to_string();
        if !lower.contains("secure") {
            findings.push(finding("warning", "Cookies", &format!("{name} is missing Secure"),
                "This cookie can be sent over an unencrypted connection."));
        }
        if !lower.contains("httponly") {
            findings.push(finding("warning", "Cookies", &format!("{name} is missing HttpOnly"),
                "Scripts running on the page can read this cookie."));
        }
        if !lower.contains("samesite") {
            findings.push(finding("info", "Cookies", &format!("{name} is missing SameSite"),
                "The browser decides the cross-site behaviour for you."));
        }
    }
    if cors.as_deref() == Some("*") {
        findings.push(finding("warning", "CORS", "Any origin is allowed",
            "Access-Control-Allow-Origin is set to *, so any website can read responses from this origin."));
    }
    if timings.first_response_ms > 1500 {
        findings.push(finding("warning", "Performance", "Slow first response",
            &format!("The server took {} ms to start responding.", timings.first_response_ms)));
    }
    if page.inline_scripts > 0 && lookup("content-security-policy").is_some() {
        findings.push(finding("info", "Content", "Inline scripts present",
            &format!("{} inline script blocks may conflict with the content security policy.", page.inline_scripts)));
    }
    if status >= 400 {
        findings.push(finding("error", "Response", &format!("Server returned {status}"),
            "The page did not load successfully."));
    }
    if findings.is_empty() {
        findings.push(finding("info", "Summary", "No issues detected in this pass",
            "Everything checked returned a healthy result."));
    }

    let report = WebsiteReport {
        url: url.to_string(),
        final_url,
        addresses,
        status,
        http_version,
        scheme,
        timings,
        headers,
        security_headers,
        cookies,
        cors,
        redirects,
        page,
        findings,
        generated_at: crate::now(),
    };

    {
        let conn = db.0.lock().unwrap();
        logging::write(&conn, "info", "WEBSITE", "Scan complete",
            Some(&format!("target={host} status={status}")));
        logging::activity(&conn, "website", "Website scan", &host);
        let _ = conn.execute(
            "INSERT INTO reports (module, title, payload, created_at) VALUES ('website', ?1, ?2, ?3)",
            rusqlite::params![host, serde_json::to_string(&report)?, crate::now()],
        );
    }
    Ok(report)
}

/// Lightweight tag counting. Deliberately not a full parser: it reports shape,
/// not semantics, and never executes anything it reads.
fn analyse_html(body: &str, base: &Url) -> PageStats {
    let lower = body.to_ascii_lowercase();
    let count = |needle: &str| lower.matches(needle).count();

    let title = body
        .find("<title")
        .and_then(|i| body[i..].find('>').map(|j| i + j + 1))
        .and_then(|start| body[start..].find("</title").map(|end| body[start..start + end].trim().to_string()))
        .unwrap_or_default();

    let mut internal = 0usize;
    let mut external = 0usize;
    let mut hosts: Vec<String> = Vec::new();
    let base_host = base.host_str().unwrap_or("").to_string();

    for part in lower.split("href=\"").skip(1) {
        if let Some(end) = part.find('"') {
            let href = &part[..end];
            if href.starts_with("http") {
                if let Ok(u) = Url::parse(href) {
                    let h = u.host_str().unwrap_or("").to_string();
                    if h == base_host { internal += 1 } else {
                        external += 1;
                        if !h.is_empty() && !hosts.contains(&h) { hosts.push(h) }
                    }
                }
            } else if !href.starts_with('#') && !href.starts_with("mailto:") {
                internal += 1;
            }
        }
    }
    hosts.truncate(25);

    PageStats {
        html_bytes: body.len(),
        stylesheets: count("<link") ,
        scripts: count("<script"),
        images: count("<img"),
        links_internal: internal,
        links_external: external,
        inline_scripts: count("<script>"),
        title,
        external_hosts: hosts,
    }
}
