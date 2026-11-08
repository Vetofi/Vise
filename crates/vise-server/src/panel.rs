
use crate::{
    api::{self, Shared},
    http::{self, Request, Response},
};
use std::{
    collections::VecDeque,
    net::{SocketAddr, TcpListener},
    sync::Arc,
    time::{Duration, Instant},
};

const COOKIE: &str = "vise_panel";
const LINK_LIFE: Duration = Duration::from_secs(600);
const FAILURES_PER_MINUTE: usize = 10;
const SESSIONS: usize = 32;

const PAGE: &str = include_str!("../panel/index.html");
const SCRIPT: &str = include_str!("../panel/panel.js");
const STYLE: &str = include_str!("../panel/panel.css");

#[derive(Default)]
pub struct Auth {
    address: Option<SocketAddr>,
    codes: Vec<(String, Instant)>,
    sessions: VecDeque<String>,
    failures: VecDeque<Instant>,
}

impl Auth {
    pub fn login_link(&mut self) -> Option<String> {
        let address = self.address?;
        self.codes.retain(|(_, at)| at.elapsed() < LINK_LIFE);
        let code = api::random_token();
        self.codes.push((code.clone(), Instant::now()));
        Some(format!("http://{address}/login?code={code}"))
    }

    fn redeem(&mut self, code: &str) -> Result<String, Response> {
        self.failures
            .retain(|at| at.elapsed() < Duration::from_secs(60));
        if self.failures.len() >= FAILURES_PER_MINUTE {
            return Err(page_error(
                429,
                "Too many failed logins: wait a minute, then open a new link.",
            ));
        }
        self.codes.retain(|(_, at)| at.elapsed() < LINK_LIFE);
        let Some(i) = self
            .codes
            .iter()
            .position(|(c, _)| api::same_token(c, code))
        else {
            self.failures.push_back(Instant::now());
            return Err(page_error(
                403,
                "This login link was used already or has expired. Run `vise-server panel` for a new one.",
            ));
        };
        self.codes.remove(i);
        let session = api::random_token();
        if self.sessions.len() == SESSIONS {
            self.sessions.pop_front();
        }
        self.sessions.push_back(session.clone());
        Ok(session)
    }

    fn signed_in(&self, request: &Request) -> bool {
        request
            .cookie(COOKIE)
            .is_some_and(|s| self.sessions.iter().any(|t| api::same_token(t, s)))
    }

    fn sign_out(&mut self, request: &Request) {
        if let Some(s) = request.cookie(COOKIE) {
            self.sessions.retain(|t| !api::same_token(t, s));
        }
    }
}

fn page_error(status: u16, message: &str) -> Response {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Vise server</title>\
         <link rel=stylesheet href=/panel.css><main class=message><p>{}</p></main>",
        message
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    );
    Response::text(status, "text/html; charset=utf-8", body)
}

fn host_allowed(host: &str, address: SocketAddr) -> bool {
    if host == address.to_string() {
        return true;
    }
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(']') || name.starts_with('[') => (name, Some(port)),
        _ => (host, None),
    };
    let same_port = port == Some(&address.port().to_string());
    if address.ip().is_loopback() && name.eq_ignore_ascii_case("localhost") && same_port {
        return true;
    }
    let tailnet_name = name.to_ascii_lowercase().ends_with(".ts.net")
        || (!name.is_empty()
            && !name.contains('.')
            && !name.contains('[')
            && !name.eq_ignore_ascii_case("localhost"));
    tailnet_name
        && (same_port || port.is_none())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

fn secure(response: Response) -> Response {
    response
        .with(
            "Content-Security-Policy",
            "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
             connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        )
        .with("X-Content-Type-Options", "nosniff")
        .with("X-Frame-Options", "DENY")
        .with("Referrer-Policy", "no-referrer")
}

fn handle(shared: &Arc<Shared>, address: SocketAddr, request: &Request) -> Response {
    if !request
        .header("host")
        .is_some_and(|h| host_allowed(h, address))
    {
        return Response::error(403, "wrong Host");
    }
    let method = request.method.as_str();
    let path = request.path.as_str();
    match (method, path) {
        ("GET", "/") => return Response::text(200, "text/html; charset=utf-8", PAGE),
        ("GET", "/panel.js") => {
            return Response::text(200, "text/javascript; charset=utf-8", SCRIPT);
        }
        ("GET", "/panel.css") => return Response::text(200, "text/css; charset=utf-8", STYLE),
        ("GET", "/login") => {
            let code = request.param("code").unwrap_or_default();
            return match shared.panel.lock().unwrap().redeem(code) {
                Ok(session) => Response::text(303, "text/plain", "")
                    .with("Location", "/")
                    .with(
                        "Set-Cookie",
                        format!("{COOKIE}={session}; HttpOnly; SameSite=Strict; Path=/"),
                    ),
                Err(response) => response,
            };
        }
        _ => {}
    }
    if method == "POST" && request.header("x-vise") != Some("1") {
        return Response::error(403, "missing the X-Vise header");
    }
    if (method, path) == ("POST", "/logout") {
        shared.panel.lock().unwrap().sign_out(request);
        return Response::json(200, &serde_json::json!({ "ok": true })).with(
            "Set-Cookie",
            format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
        );
    }
    if !path.starts_with("/api/v1/") {
        return Response::error(404, "no such page");
    }
    if !shared.panel.lock().unwrap().signed_in(request) {
        return Response::error(
            401,
            "sign in with a link from `vise-server panel` on the server",
        );
    }
    if path == "/api/v1/panel/login" {
        return Response::error(403, "login links come from `vise-server panel` only");
    }
    api::route(shared, request)
}

pub fn start(shared: Arc<Shared>, address: SocketAddr) -> Result<(), String> {
    let listener = TcpListener::bind(address).map_err(|e| e.to_string())?;
    shared.panel.lock().unwrap().address = Some(address);
    http::serve(listener, move |request| {
        secure(handle(&shared, address, request))
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn hosts() {
        let local: SocketAddr = "127.0.0.1:8130".parse().unwrap();
        assert!(host_allowed("127.0.0.1:8130", local));
        assert!(host_allowed("localhost:8130", local));
        assert!(!host_allowed("localhost:8131", local));
        assert!(!host_allowed("evil.example:8130", local));
        assert!(!host_allowed("127.0.0.1.nip.io:8130", local));
        let tailnet: SocketAddr = "100.100.1.2:8130".parse().unwrap();
        assert!(host_allowed("100.100.1.2:8130", tailnet));
        assert!(host_allowed("myserver:8130", tailnet));
        assert!(host_allowed("myserver.tail1234.ts.net", tailnet));
        assert!(!host_allowed("localhost:8130", tailnet));
        assert!(!host_allowed("evil.example", tailnet));
        let v6: SocketAddr = "[::1]:8130".parse().unwrap();
        assert!(host_allowed("[::1]:8130", v6));
        assert!(!host_allowed("[::2]:8130", v6));
    }

    fn send(address: SocketAddr, text: &str) -> (String, String) {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream.write_all(text.as_bytes()).unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).unwrap();
        let (head, body) = out.split_once("\r\n\r\n").unwrap();
        (head.to_owned(), body.to_owned())
    }

    #[test]
    fn sign_in_with_a_link() {
        let dir = std::env::temp_dir().join(format!("vise-server-panel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shared = Shared::new(&dir);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        start(shared.clone(), address).unwrap();
        let host = format!("Host: {address}\r\n");
        let get = |path: &str, extra: &str| {
            send(
                address,
                &format!("GET {path} HTTP/1.1\r\n{host}{extra}Connection: close\r\n\r\n"),
            )
        };
        let (head, body) = get("/", "");
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(head.contains("Content-Security-Policy"));
        assert!(body.contains("panel.js"));
        assert!(get("/api/v1/status", "").0.starts_with("HTTP/1.1 401"));
        let rebound = send(
            address,
            "GET / HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n",
        );
        assert!(rebound.0.starts_with("HTTP/1.1 403"));
        let url = shared.panel.lock().unwrap().login_link().unwrap();
        let path = url.split_once(&address.to_string()).unwrap().1.to_owned();
        let (head, _) = get(&path, "");
        assert!(head.starts_with("HTTP/1.1 303"), "{head}");
        let cookie = head
            .lines()
            .find_map(|l| l.strip_prefix("Set-Cookie: "))
            .unwrap();
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
        let session = cookie.split(';').next().unwrap().to_owned();
        assert!(get(&path, "").0.starts_with("HTTP/1.1 403"));
        let with_cookie = format!("Cookie: other=1; {session}\r\n");
        let (head, body) = get("/api/v1/status", &with_cookie);
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(body.contains("\"phase\":\"starting\""));
        let post = |extra: &str| {
            send(
                address,
                &format!(
                    "POST /api/v1/restart HTTP/1.1\r\n{host}{with_cookie}{extra}Content-Length: 0\r\nConnection: close\r\n\r\n"
                ),
            )
            .0
        };
        assert!(post("").starts_with("HTTP/1.1 403"));
        assert!(!shared.restart.load(std::sync::atomic::Ordering::SeqCst));
        assert!(post("X-Vise: 1\r\n").starts_with("HTTP/1.1 200"));
        assert!(shared.restart.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            send(
                address,
                &format!(
                    "POST /api/v1/panel/login HTTP/1.1\r\n{host}{with_cookie}X-Vise: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
            )
            .0
            .starts_with("HTTP/1.1 403")
        );
        for _ in 1..FAILURES_PER_MINUTE {
            assert!(get("/login?code=nope", "").0.starts_with("HTTP/1.1 403"));
        }
        assert!(get("/login?code=nope", "").0.starts_with("HTTP/1.1 429"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
