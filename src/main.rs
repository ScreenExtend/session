use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    http::{header, HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

const INDEX_HTML: &str = include_str!("../static/index.html");
const STYLES_CSS: &str = include_str!("../static/styles.css");
const LOGO_SVG: &str = include_str!("../static/logo.svg");
const TRANSFORM_WORKER_JS: &str = include_str!("../static/transform-worker.js");

const SESSION_ID_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const HEARTBEAT_SEC: u64 = 20;
const HOST_GRACE: Duration = Duration::from_secs(12);

type HmacSha1 = Hmac<Sha1>;

#[derive(Clone)]
struct Config {
    turn_secret: Option<String>,
    turn_ttl: u64,
    stun_urls: Vec<String>,
    turn_urls: Vec<String>,
    body_limit: usize,
    join_origin: String,
    ip_salt: String,
}

impl Config {
    fn from_env() -> Self {
        fn list(key: &str, default: &[&str]) -> Vec<String> {
            match std::env::var(key) {
                Ok(v) if !v.trim().is_empty() => v
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                _ => default.iter().map(|s| s.to_string()).collect(),
            }
        }

        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);

        Config {
            turn_secret: std::env::var("TURN_STATIC_AUTH_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
            turn_ttl: std::env::var("TURN_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(600),
            stun_urls: list("STUN_URLS", &["stun:stun.l.google.com:19302"]),
            turn_urls: list(
                "TURN_URLS",
                &[
                    "turn:turn.screenextend.app:3478?transport=udp",
                    "turn:turn.screenextend.app:3478?transport=tcp",
                    "turns:turn.screenextend.app:5349?transport=tcp",
                ],
            ),
            body_limit: std::env::var("BODY_LIMIT_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(65536),
            join_origin: std::env::var("JOIN_ORIGIN")
                .unwrap_or_else(|_| "https://session.screenextend.app".to_string()),
            ip_salt: base64::engine::general_purpose::STANDARD_NO_PAD.encode(salt),
        }
    }

    fn ice_servers(&self) -> Value {
        let mut servers = Vec::new();
        if !self.stun_urls.is_empty() {
            servers.push(json!({ "urls": self.stun_urls }));
        }
        if let Some(secret) = &self.turn_secret {
            if !self.turn_urls.is_empty() {
                let (username, credential) = turn_credentials(secret, self.turn_ttl);
                servers.push(json!({
                    "urls": self.turn_urls,
                    "username": username,
                    "credential": credential,
                }));
            }
        }
        json!({ "iceServers": servers })
    }
}

fn turn_credentials(secret: &str, ttl: u64) -> (String, String) {
    let expiry = now_unix() + ttl;
    let username = format!("{expiry}:se");
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(username.as_bytes());
    let credential = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    (username, credential)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct RateLimiter {
    windows: DashMap<String, (Instant, u32)>,
    max: u32,
    window: Duration,
}

impl RateLimiter {
    fn new(max: u32, window: Duration) -> Self {
        RateLimiter {
            windows: DashMap::new(),
            max,
            window,
        }
    }

    fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut entry = self
            .windows
            .entry(key.to_string())
            .or_insert((now, 0));
        let (start, count) = *entry;
        if now.duration_since(start) > self.window {
            *entry = (now, 1);
            true
        } else if count >= self.max {
            false
        } else {
            *entry = (start, count + 1);
            true
        }
    }

    fn sweep(&self) {
        let now = Instant::now();
        self.windows
            .retain(|_, (start, _)| now.duration_since(*start) <= self.window);
    }
}

#[derive(Clone)]
struct Relay {
    sessions: Arc<DashMap<String, HostConn>>,
    cid_sessions: Arc<DashMap<String, String>>,
    whep_limiter: Arc<RateLimiter>,
    register_limiter: Arc<RateLimiter>,
    cfg: Config,
}

#[derive(Clone)]
struct HostConn {
    conn_id: String,
    tx: mpsc::UnboundedSender<Message>,
    pending: Arc<DashMap<String, oneshot::Sender<SignalResponse>>>,
    draining: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct SignalResponse {
    #[serde(default)]
    status: u16,
    #[serde(default)]
    headers: Value,
    #[serde(default)]
    body: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = Config::from_env();
    let body_limit = cfg.body_limit;

    fn env_u32(key: &str, default: u32) -> u32 {
        std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    }
    let rate_window = Duration::from_secs(env_u32("RATE_WINDOW_SECS", 60) as u64);
    let whep_limiter = Arc::new(RateLimiter::new(env_u32("WHEP_RATE_MAX", 30), rate_window));
    let register_limiter = Arc::new(RateLimiter::new(env_u32("REGISTER_RATE_MAX", 60), rate_window));

    let relay = Relay {
        sessions: Arc::new(DashMap::new()),
        cid_sessions: Arc::new(DashMap::new()),
        whep_limiter: whep_limiter.clone(),
        register_limiter: register_limiter.clone(),
        cfg,
    };

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(300));
        tick.tick().await;
        loop {
            tick.tick().await;
            whep_limiter.sweep();
            register_limiter.sweep();
        }
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/styles.css", get(styles))
        .route("/logo.svg", get(logo))
        .route("/transform-worker.js", get(worker))
        .route("/health", get(health))
        .route("/ice-config", get(ice_config))
        .route("/net-config", get(net_config))
        .route("/whep", post(tunnel))
        .route("/reconfig", get(tunnel))
        .route("/leave", post(tunnel))
        .route("/host/v1/connect", get(host_ws))
        .layer(RequestBodyLimitLayer::new(body_limit))
        .layer(TraceLayer::new_for_http())
        .with_state(relay);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind listener");
    tracing::info!("relay listening on {addr}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}

async fn index(headers: HeaderMap) -> Response {
    let mut resp = (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        INDEX_HTML,
    )
        .into_response();
    if read_cid(&headers).is_none() {
        let cid = mint_cid();
        if let Ok(v) = cookie_header(&cid).parse() {
            resp.headers_mut().insert(header::SET_COOKIE, v);
        }
    }
    resp
}

async fn styles() -> Response {
    ([(header::CONTENT_TYPE, "text/css")], STYLES_CSS).into_response()
}

async fn logo() -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml")], LOGO_SVG).into_response()
}

async fn worker() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript")],
        TRANSFORM_WORKER_JS,
    )
        .into_response()
}

async fn health() -> &'static str {
    "ok"
}

async fn ice_config(State(relay): State<Relay>) -> Response {
    let mut resp = Json(relay.cfg.ice_servers()).into_response();
    no_store(resp.headers_mut());
    resp
}

async fn net_config() -> Response {
    Json(json!({ "httpsPort": 443 })).into_response()
}

async fn tunnel(
    State(relay): State<Relay>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let path = uri.path();

    let (cid, set_cookie) = match read_cid(&headers) {
        Some(c) => (c, None),
        None => {
            let c = mint_cid();
            (c.clone(), Some(c))
        }
    };

    let session_id = if path == "/whep" {
        let ip = client_ip(&headers, &peer);
        if !relay.whep_limiter.check(&format!("cid:{cid}"))
            || !relay.whep_limiter.check(&format!("ip:{ip}"))
        {
            return finish(
                StatusCode::TOO_MANY_REQUESTS,
                "too many requests",
                set_cookie,
                "text/plain",
            );
        }
        match parse_session_id(&body) {
            Some(s) if valid_session_id(&s) => s,
            _ => return finish(StatusCode::BAD_REQUEST, "bad request", set_cookie, "text/plain"),
        }
    } else {
        match relay.cid_sessions.get(&cid).map(|e| e.clone()) {
            Some(s) => s,
            None => {
                return if path == "/reconfig" {
                    finish(StatusCode::NO_CONTENT, "", set_cookie, "text/plain")
                } else {
                    finish(StatusCode::NO_CONTENT, "", set_cookie, "text/plain")
                };
            }
        }
    };

    let host = match relay.sessions.get(&session_id) {
        Some(h) if !h.draining.load(Ordering::Relaxed) => h.clone(),
        _ => return finish(StatusCode::SERVICE_UNAVAILABLE, "host offline", set_cookie, "text/plain"),
    };

    if path == "/whep" {
        relay.cid_sessions.insert(cid.clone(), session_id.clone());
    }

    let request_id = mint_id("r_");
    let (otx, orx) = oneshot::channel();
    host.pending.insert(request_id.clone(), otx);

    let mut msg = json!({
        "type": "signal_request",
        "requestId": request_id,
        "sessionId": session_id,
        "clientId": cid,
        "method": method.as_str(),
        "path": path,
        "query": uri.query().unwrap_or(""),
        "headers": forward_headers(&headers),
        "body": String::from_utf8_lossy(&body),
        "remote": { "ipHash": ip_hash(&relay.cfg.ip_salt, &headers, &peer) },
    });
    if path == "/whep" {
        if let Some(ice) = relay.cfg.ice_servers().get("iceServers") {
            msg["iceServers"] = ice.clone();
        }
    }

    if host.tx.send(Message::Text(msg.to_string().into())).is_err() {
        host.pending.remove(&request_id);
        return finish(StatusCode::BAD_GATEWAY, "host gone", set_cookie, "text/plain");
    }

    let timeout = route_timeout(path);
    match tokio::time::timeout(timeout, orx).await {
        Ok(Ok(resp)) => build_tunnel_response(resp, set_cookie),
        Ok(Err(_)) => {
            host.pending.remove(&request_id);
            finish(StatusCode::BAD_GATEWAY, "host gone", set_cookie, "text/plain")
        }
        Err(_) => {
            host.pending.remove(&request_id);
            finish(
                StatusCode::GATEWAY_TIMEOUT,
                "host did not respond",
                set_cookie,
                "text/plain",
            )
        }
    }
}

fn route_timeout(path: &str) -> Duration {
    match path {
        "/whep" => Duration::from_secs(15),
        "/reconfig" => Duration::from_secs(5),
        "/leave" => Duration::from_secs(3),
        _ => Duration::from_secs(10),
    }
}

fn build_tunnel_response(resp: SignalResponse, set_cookie: Option<String>) -> Response {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = resp
        .headers
        .get("content-type")
        .or_else(|| resp.headers.get("Content-Type"))
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();

    let mut out = Response::new(resp.body.into());
    *out.status_mut() = status;
    if let Ok(v) = content_type.parse() {
        out.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    no_store(out.headers_mut());
    apply_set_cookie(&mut out, set_cookie);
    out
}

fn finish(status: StatusCode, body: &'static str, set_cookie: Option<String>, ct: &str) -> Response {
    let mut out = Response::new(body.to_string().into());
    *out.status_mut() = status;
    if let Ok(v) = ct.parse() {
        out.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    no_store(out.headers_mut());
    apply_set_cookie(&mut out, set_cookie);
    out
}

fn apply_set_cookie(out: &mut Response, set_cookie: Option<String>) {
    if let Some(cid) = set_cookie {
        if let Ok(v) = cookie_header(&cid).parse() {
            out.headers_mut().insert(header::SET_COOKIE, v);
        }
    }
}

fn no_store(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
}

async fn host_ws(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(relay): State<Relay>,
) -> Response {
    let ip = client_ip(&headers, &peer);
    ws.on_upgrade(move |socket| handle_host(socket, relay, ip))
}

async fn handle_host(socket: WebSocket, relay: Relay, ip: String) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let conn_id = mint_id("conn_");
    let pending: Arc<DashMap<String, oneshot::Sender<SignalResponse>>> = Arc::new(DashMap::new());
    let draining = Arc::new(AtomicBool::new(false));

    let host = HostConn {
        conn_id: conn_id.clone(),
        tx: tx.clone(),
        pending: pending.clone(),
        draining: draining.clone(),
    };

    let writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval(Duration::from_secs(HEARTBEAT_SEC));
        ping.tick().await;
        loop {
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some(m) => {
                        if sink.send(m).await.is_err() { break; }
                    }
                    None => break,
                },
                _ = ping.tick() => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
                }
            }
        }
    });

    let mut my_sessions: Vec<String> = Vec::new();

    while let Some(Ok(msg)) = stream.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        match v.get("type").and_then(Value::as_str) {
            Some("register") => {
                handle_register(&relay, &host, &v, &ip, &mut my_sessions);
            }
            Some("signal_response") => {
                if let Some(req_id) = v.get("requestId").and_then(Value::as_str) {
                    if let Some((_, sender)) = pending.remove(req_id) {
                        if let Ok(resp) = serde_json::from_value::<SignalResponse>(v.clone()) {
                            let _ = sender.send(resp);
                        }
                    }
                }
            }
            Some("unregister") => {
                if let Some(sid) = v.get("sessionId").and_then(Value::as_str) {
                    remove_session(&relay, sid, &conn_id);
                    my_sessions.retain(|s| s != sid);
                }
            }
            _ => {}
        }
    }

    draining.store(true, Ordering::Relaxed);
    writer.abort();
    pending.clear();

    for sid in my_sessions {
        let relay = relay.clone();
        let conn_id = conn_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(HOST_GRACE).await;
            remove_session(&relay, &sid, &conn_id);
        });
    }
}

fn handle_register(
    relay: &Relay,
    host: &HostConn,
    v: &Value,
    ip: &str,
    my_sessions: &mut Vec<String>,
) {
    let sid = v.get("sessionId").and_then(Value::as_str).unwrap_or("");

    if !relay.register_limiter.check(ip) {
        let _ = host
            .tx
            .send(register_error(sid, "rate_limited", "too many register attempts"));
        return;
    }

    if !valid_session_id(sid) {
        let _ = host.tx.send(register_error(sid, "invalid_session", "malformed session ID"));
        return;
    }
    if let Some(p) = v.get("protocol").and_then(Value::as_u64) {
        if p != 1 {
            let _ = host
                .tx
                .send(register_error(sid, "version_unsupported", "protocol must be 1"));
            return;
        }
    }

    if let Some(existing) = relay.sessions.get(sid) {
        if !existing.draining.load(Ordering::Relaxed) && existing.conn_id != host.conn_id {
            drop(existing);
            let _ = host.tx.send(register_error(sid, "session_taken", "session already hosted"));
            return;
        }
    }

    relay.sessions.insert(sid.to_string(), host.clone());
    if !my_sessions.iter().any(|s| s == sid) {
        my_sessions.push(sid.to_string());
    }

    let join_url = format!("{}/?id={}", relay.cfg.join_origin, sid);
    let _ = host.tx.send(Message::Text(
        json!({
            "type": "registered",
            "sessionId": sid,
            "joinUrl": join_url,
            "heartbeatSec": HEARTBEAT_SEC,
        })
        .to_string()
        .into(),
    ));
    tracing::info!(session = %sid, "registered");
}

fn register_error(sid: &str, code: &str, message: &str) -> Message {
    Message::Text(
        json!({
            "type": "register_error",
            "sessionId": sid,
            "code": code,
            "message": message,
        })
        .to_string()
        .into(),
    )
}

fn remove_session(relay: &Relay, sid: &str, conn_id: &str) {
    let should_remove = relay
        .sessions
        .get(sid)
        .map(|h| h.conn_id == conn_id)
        .unwrap_or(false);
    if should_remove {
        relay.sessions.remove(sid);
        tracing::info!(session = %sid, "unregistered");
    }
}

fn valid_session_id(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| SESSION_ID_ALPHABET.contains(&b))
}

fn parse_session_id(body: &Bytes) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    v.get("sessionId")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}

fn forward_headers(headers: &HeaderMap) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(ct) = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        out.insert("content-type".to_string(), Value::String(ct.to_string()));
    }
    Value::Object(out)
}

fn read_cid(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("se_cid=") {
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn mint_cid() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "c_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    )
}

fn mint_id(prefix: &str) -> String {
    format!("{prefix}{}", uuid::Uuid::new_v4().simple())
}

fn cookie_header(cid: &str) -> String {
    format!("se_cid={cid}; Path=/; Max-Age=86400; Secure; HttpOnly; SameSite=Lax")
}

fn client_ip(headers: &HeaderMap, peer: &SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| peer.ip().to_string())
}

fn ip_hash(salt: &str, headers: &HeaderMap, peer: &SocketAddr) -> String {
    let ip = client_ip(headers, peer);

    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(ip.as_bytes());
    let digest = hasher.finalize();
    format!(
        "sha256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    )
}
