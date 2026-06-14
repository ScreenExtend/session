# ScreenExtend Relay Server

A lightweight signaling relay for [ScreenExtend](https://screenextend.app), the Windows
app that turns any browser device into a wireless second monitor. This service is the
public matchmaker deployed at `https://session.screenextend.app`: it lets any client open
a single URL, enter a 12-character session ID + 6-digit OTP, and get routed to the correct
host wherever it is on the internet.

**The relay never touches video pixels.** It serves the join page, hands out short-lived
TURN credentials, and shuttles small WebRTC signaling messages between a client and the one
host that owns a session. The media stream itself goes peer-to-peer (or through a separate
TURN server) directly between host and client.

## How it works

The host (a desktop machine, usually behind NAT) can't accept inbound connections, so it
opens a **persistent outbound WebSocket** to the relay and registers its session ID. The
client speaks ordinary HTTPS to the relay; the relay terminates those requests and
**tunnels** them over the host's WebSocket, waits for the reply, and returns it to the
client.

```
 Client browser            Relay (this service)             Host (ScreenExtend app)
 ──────────────            ────────────────────             ───────────────────────
 GET /?id=SESSION   ─────▶  serve join page + se_cid
 GET /ice-config    ─────▶  STUN + ephemeral TURN creds
 POST /whep         ─────▶  tunnel ──────────────────────▶  validate OTP, build SDP answer
                            ◀──────────────── signal_response
                    ◀─────  SDP answer
        ◀═══════════ WebRTC media (direct via ICE, or relayed via TURN) ═══════════▶
```

Routing is by session ID. The relay assigns each browser a stable `se_cid` cookie so the
host can tell devices apart; `/reconfig` and `/leave` (which carry no session ID) are routed
via the `clientId → sessionId` binding recorded on the client's last successful `/whep`.

## Endpoints

| Method & path | Handled by | Notes |
| --- | --- | --- |
| `GET /` | relay | Serves `index.html`, sets the `se_cid` cookie. |
| `GET /styles.css`, `/logo.svg`, `/transform-worker.js` | relay | Static join-page assets, served verbatim. |
| `GET /health` | relay | Returns `ok` (relay liveness). |
| `GET /ice-config` | relay | STUN + time-limited TURN credentials. |
| `GET /net-config` | relay | `{"httpsPort":443}`. |
| `POST /whep` | tunneled | The join/offer. Body carries `sessionId`, `otp`, `sdp`. |
| `GET /reconfig` | tunneled | Settings-change / kick poll. Returns `{epoch,kick}`. |
| `POST /leave` | tunneled | `navigator.sendBeacon` on page hide. Returns `204`. |
| `GET /host/v1/connect` | relay | Host control WebSocket (registration + signaling tunnel). |

## Running

Requires a recent stable Rust toolchain.

```sh
cargo run            # development
cargo build --release && ./target/release/session-backend-rust
```

The server listens on `0.0.0.0:$PORT` (default `8080`). It speaks plain HTTP and expects to
sit behind a TLS-terminating proxy/load balancer that forwards `X-Forwarded-For` /
`X-Real-IP`.

## Configuration

All configuration is via environment variables; every one has a sensible default.

| Variable | Default | Purpose |
| --- | --- | --- |
| `PORT` | `8080` | Listen port. |
| `JOIN_ORIGIN` | `https://session.screenextend.app` | Origin used to build `joinUrl` returned to hosts. |
| `TURN_STATIC_AUTH_SECRET` | _(unset)_ | Shared secret with coturn (`use-auth-secret`). If unset, no TURN creds are issued — STUN only. |
| `TURN_TTL_SECS` | `600` | Lifetime of issued TURN credentials. |
| `STUN_URLS` | `stun:stun.l.google.com:19302` | Comma-separated STUN URLs advertised in `/ice-config`. |
| `TURN_URLS` | `turn:turn.screenextend.app:3478?transport=udp,...tcp, turns:...5349?transport=tcp` | Comma-separated TURN URLs. |
| `BODY_LIMIT_BYTES` | `65536` | Maximum request body size. |
| `RATE_WINDOW_SECS` | `60` | Rate-limit window. |
| `WHEP_RATE_MAX` | `30` | Max `/whep` attempts per `se_cid` and per IP per window. |
| `REGISTER_RATE_MAX` | `60` | Max host `register` attempts per IP per window. |
| `RUST_LOG` | `info` | Tracing filter (e.g. `debug`, `session_backend_rust=debug`). |

### TURN credentials

When `TURN_STATIC_AUTH_SECRET` is set, the relay mints ephemeral coturn credentials using
the standard time-limited scheme:

```
expiry   = now_unix + TURN_TTL_SECS
username = "<expiry>:se"
credential = base64( HMAC_SHA1( TURN_STATIC_AUTH_SECRET, username ) )
```

The same secret must be configured in coturn (running separately, e.g. at
`turn.screenextend.app`). The relay injects this same ICE list into the `/whep` tunnel so
host and client agree on credentials.

## Security notes

- **OTP never reaches relay logic** — it's forwarded opaquely inside the tunneled body and
  validated only by the host.
- **Session-takeover protection** — a `register` for a live session ID is rejected with
  `session_taken`.
- **No raw client IPs forwarded** — hosts receive a salted `sha256` `ipHash` only. The salt
  is randomized per process start.
- **Cookie hygiene** — `se_cid` is `Secure; HttpOnly; SameSite=Lax` and is a routing tag,
  not an auth token.
- **Body limits and rate limits** are enforced on `/whep` and host `register`.

The relay never logs OTPs, SDP bodies, or raw client IPs.

## Project layout

```
src/main.rs              # all relay logic (axum router, host WS handler, tunnel, TURN creds)
static/                  # join-page assets, embedded at build time via include_str!
  index.html             # the join page
  styles.css             # page styles
  logo.svg               # logo
  transform-worker.js    # client-side WebCodecs decode/render worker
```
