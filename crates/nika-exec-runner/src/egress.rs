// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The loopback egress proxy — the Anthropic sandbox-runtime (srt) model in
//! Nika (ADR-095 Layer 6 · the `allowlist` network arm's enforcement half).
//!
//! Started per-run (lazily, on the first exec whose
//! [`SandboxSpec`](nika_kernel::process::SandboxSpec) carries
//! [`NetPolicy::Allowlist`](nika_kernel::process::NetPolicy::Allowlist)) by
//! [`crate::TokioShell`]; binds `127.0.0.1` on an
//! ephemeral port; serves BOTH HTTP `CONNECT` and SOCKS5 on the ONE muxed
//! port — protocol-sniffed on the first byte (`0x05` = SOCKS5, anything else
//! = CONNECT), the same mux srt ships ("the mux serves both on one port",
//! `sandbox-runtime` `macos-sandbox-utils.ts`). Non-loopback peers are
//! refused outright: the proxy exists ONLY for the sandboxed child on this
//! host.
//!
//! Every CONNECT/SOCKS target is evaluated against the declared
//! `permits.net.http` set with the ONE host matcher —
//! [`nika_types::net::host_in_allowlist`], the same predicate `nika-http`
//! enforces at the wire boundary and `nika-schema`/`nika-cap` at check time,
//! so check ≡ run ≡ jail cannot drift on the glob. Only a match is dialed
//! upstream; anything else is refused (CONNECT → `403`, SOCKS5 →
//! `0x02` "not allowed by ruleset"). Every decision is journalised through
//! the injected [`EgressObserver`] — a refused host is a security event, an
//! allowed one a debug line, and a relayed tunnel reports its byte counters
//! at close (F-P5 metering: host + port + octets, NEVER the content — the
//! TLS-blind posture) — (the composer surfaces all three on stderr, the
//! FCI-009 honest seam: the run journal's `EventSink` is a `&mut` threaded
//! through the settle pass, unreachable from an out-of-band proxy thread —
//! the `StderrEmitter` precedent in `nika-cli`).
//!
//! The floor under the allowlist (the 2026-07-23 red-team stopgaps, now
//! law): (1) DNS-rebinding is REFUSED AT THE DIAL — every resolved address
//! passes [`nika_types::net::ip_is_blocked`] (the same oracle the
//! `nika-http` `GuardedResolver` runs) INSIDE the dial loop, before
//! `connect_timeout` on that address, so an allowlisted name that resolves
//! to a loopback/private/link-local/metadata range dies with
//! `PermissionDenied`, with zero window between resolve and connect. The
//! one carve-out is the author's EXACT loopback literal declared in the
//! allowlist (#395) — never another range. (2) The egress PORT floor
//! (`DANGEROUS_EGRESS_PORTS`): a `net.http` permit is an HTTP-egress grant
//! — it never admits the classic non-HTTP service ports (ssh · smtp ·
//! docker · kube · the databases), and no permit overrides the floor.
//!
//! The proxy runs WITHOUT a session token (srt's posture, stated honestly):
//! a co-located local process CAN connect to it — and gains nothing. An
//! unconfined process could dial the declared hosts directly anyway; a
//! process confined by ANOTHER run is stopped by its own fence before it
//! reaches this proxy. The boundary (the allowlist) holds for every
//! client: one run's authority never leaks to hosts it did not declare.
//! Plain-HTTP forward-proxy requests (`GET http://…`) are NOT served —
//! CONNECT and SOCKS5 only; anything else gets an honest `405` and a
//! closed socket (fail-closed).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use nika_kernel::process::EgressAllowlist;

/// Handshake-phase I/O deadline (greeting, CONNECT line, SOCKS request). A
/// stalled handshake cannot hold a connection thread forever; the tunnel
/// itself runs unbounded (the run's own lifetime bounds it — the proxy dies
/// with the runner).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Per-address upstream dial deadline (each resolved address is tried in
/// turn, mirroring the blocking resolver semantics of srt's Node proxies).
const UPSTREAM_DIAL_TIMEOUT: Duration = Duration::from_secs(10);

/// The byte ceiling on the CONNECT header block (request line + headers) —
/// a forward proxy needs the first line only, the rest is drained-and-ignored.
const CONNECT_HEADER_CAP: usize = 8192;

/// One allow/refuse verdict on a CONNECT/SOCKS target — the journal unit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EgressDecision {
    /// The requested target host, exactly as the client sent it.
    pub host: String,
    /// The requested target port.
    pub port: u16,
    /// Whether the declared allowlist admitted the target.
    pub allowed: bool,
}

/// One journalised egress event (F-P5) — the observer's unit: a verdict
/// on a target, or the closure of a relayed tunnel with its byte
/// counters. The TLS-blind posture swears host + port + OCTETS, never
/// the content.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EgressEvent {
    /// An allow/refuse verdict on a CONNECT/SOCKS target.
    Decision(EgressDecision),
    /// A relayed tunnel closed — the metering (F-P5): the bytes the
    /// proxy moved in each direction, nothing about their content.
    Closed {
        /// The upstream host, exactly as the client named it.
        host: String,
        /// The upstream port.
        port: u16,
        /// Bytes relayed client → upstream (the request direction).
        bytes_up: u64,
        /// Bytes relayed upstream → client (the response direction).
        bytes_down: u64,
    },
}

/// The event sink — the composer wires the stderr journal, tests wire a
/// collecting probe. `Fn` (never `FnMut`): the proxy calls it from
/// connection threads.
pub type EgressObserver = Arc<dyn Fn(&EgressEvent) + Send + Sync>;

/// The one journal line for an event (pure — the [`stderr_journal`]
/// wrapper's `eprintln` is the only impurity, so tests pin the EXACT
/// shapes: the REFUSED row is the greppable security event, `allowed`
/// the debug line, `closed` the metering row).
fn journal_line(event: &EgressEvent) -> String {
    match event {
        EgressEvent::Decision(d) if d.allowed => {
            format!("nika:egress allowed {}:{}", d.host, d.port)
        }
        EgressEvent::Decision(d) => format!(
            "nika:egress REFUSED {}:{} (not in permits.net.http)",
            d.host, d.port
        ),
        EgressEvent::Closed {
            host,
            port,
            bytes_up,
            bytes_down,
        } => format!("nika:egress closed {host}:{port} up={bytes_up} down={bytes_down}"),
    }
}

/// The default journal when no observer is injected — a namespaced stderr
/// line per event (see the module doc for the FCI-009 seam rationale).
/// REFUSED is the security event (greppable), `allowed` the debug line,
/// `closed` the metering row (F-P5 · octets, never content).
/// stderr, NOT `tracing::warn!`: no workspace tracing subscriber exists
/// (the `StderrEmitter` precedent in `nika-cli`), so a tracing call would
/// journal into the void — and a security journal that can silently vanish
/// is worse than an unformatted one.
#[allow(clippy::disallowed_macros, clippy::print_stderr)]
pub(crate) fn stderr_journal() -> EgressObserver {
    Arc::new(|e: &EgressEvent| eprintln!("{}", journal_line(e)))
}

/// The per-run loopback proxy. Owns the accept thread; `Drop` stops the
/// listener (no orphan listener outlives the runner — see `Drop`).
pub(crate) struct EgressProxy {
    port: u16,
    allowlist: Arc<[String]>,
    shutdown: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl EgressProxy {
    /// Bind `127.0.0.1:0` (an ephemeral, OS-assigned port) and start serving.
    ///
    /// # Errors
    /// The loopback bind or the accept-thread spawn failed — fail-closed:
    /// the caller (the runner) refuses the command rather than run an
    /// allowlisted child without its proxy.
    pub(crate) fn start(allowlist: Vec<String>, observer: EgressObserver) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let shutdown = Arc::new(AtomicBool::new(false));
        let allowlist: Arc<[String]> = allowlist.into();
        let accept = {
            let shutdown = Arc::clone(&shutdown);
            let allowlist = Arc::clone(&allowlist);
            std::thread::Builder::new()
                .name("nika-egress".to_owned())
                .spawn(move || accept_loop(listener, allowlist, observer, shutdown))?
        };
        Ok(Self {
            port,
            allowlist,
            shutdown,
            accept: Some(accept),
        })
    }

    /// The ephemeral loopback port the child env points at.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// The declared host set this proxy serves (one boundary per run — the
    /// runner refuses a second, conflicting set rather than widen silently).
    pub(crate) fn allowlist(&self) -> &[String] {
        &self.allowlist
    }
}

impl Drop for EgressProxy {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        // Wake the blocking accept() so the loop observes the flag NOW (the
        // wake connection arrives, sees the flag, and is closed unserved).
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port));
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        // The listener dies with the accept thread — the port is closed, no
        // orphan listener outlives the run (the orphan test pins this).
    }
}

/// The accept loop: loopback peers only, one thread per connection.
/// By-value by design: this fn IS the accept thread's body — the listener,
/// the shared allowlist and the observer move onto the thread with it.
#[allow(clippy::needless_pass_by_value)]
fn accept_loop(
    listener: TcpListener,
    allowlist: Arc<[String]>,
    observer: EgressObserver,
    shutdown: Arc<AtomicBool>,
) {
    for conn in listener.incoming() {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        let Ok(stream) = conn else {
            break; // the listener errored (closed from under us) — stop.
        };
        let Ok(peer) = stream.peer_addr() else {
            continue;
        };
        if !peer.ip().is_loopback() {
            continue; // this proxy exists ONLY for the sandboxed child on this host.
        }
        let (allowlist, observer) = (Arc::clone(&allowlist), Arc::clone(&observer));
        let _ = std::thread::Builder::new()
            .name("nika-egress-conn".to_owned())
            .spawn(move || serve(stream, &allowlist, &observer));
    }
}

/// Serve one connection: sniff the protocol, parse the target, decide,
/// tunnel. Every path closes the socket on exit — no half-served protocol.
fn serve(client: TcpStream, allowlist: &[String], observer: &EgressObserver) {
    let _ = client.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let _ = client.set_write_timeout(Some(HANDSHAKE_TIMEOUT));
    let mut first = [0u8; 1];
    let Ok(n) = client.peek(&mut first) else {
        return;
    };
    if n == 0 {
        return; // peer hung up before speaking (e.g. the Drop wake connection).
    }
    if first[0] == 0x05 {
        serve_socks5(client, allowlist, observer);
    } else {
        serve_connect(client, allowlist, observer);
    }
}

/// The always-on egress PORT floor (H3 · red team 2026-07-23): a
/// `net.http` permit is an HTTP-egress grant — it never admits the classic
/// non-HTTP service ports, no matter the host (the confused-deputy class:
/// `net.http: [api]` used as an SSH/SMTP/docker relay through the proxy).
/// This is a floor, like `DANGEROUS_ENV_VARS`: no permit overrides it.
const DANGEROUS_EGRESS_PORTS: &[u16] = &[
    22, 23, 25, 110, 143, 445, 2375, 2376, 3306, 5432, 6379, 9200, 10250, 27017,
];

/// The shared verdict: the ONE matcher over the declared set, journalised.
/// Refused targets never leave this fn with an open upstream.
fn decide(host: &str, port: u16, allowlist: &[String], observer: &EgressObserver) -> bool {
    let allowed = !DANGEROUS_EGRESS_PORTS.contains(&port)
        && nika_types::net::host_in_allowlist(allowlist, host);
    observer(&EgressEvent::Decision(EgressDecision {
        host: host.to_owned(),
        port,
        allowed,
    }));
    allowed
}

/// Dial the approved target — each resolved address in turn, per-address
/// deadline (the blocking-resolver semantics of srt's Node proxies).
/// H1 · DNS-rebinding (red team 2026-07-23): an allowlisted NAME that
/// resolves to a blocked range (loopback · private · link-local · cloud
/// metadata) is refused per-address — the same `ip_is_blocked` oracle the
/// `GuardedResolver` of nika-http runs, so an exec arm and an http fetch now
/// agree on the floor.
fn dial_upstream(host: &str, port: u16, allowlist: &[String]) -> std::io::Result<TcpStream> {
    use std::net::ToSocketAddrs;
    let mut last = std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no address resolved");
    for addr in (host, port).to_socket_addrs()? {
        // H1 · DNS rebinding (red team 2026-07-23) · the exec arm mirrors
        // the nika-http floor EXACTLY: a blocked range is refused — with
        // the one documented carve-out (#395): an EXACT loopback literal
        // declared in the allowlist declassifies that host (the local
        // service the author named on purpose). No other range declassifies.
        if nika_types::net::ip_is_blocked(addr.ip()) {
            let literal = addr.ip().to_string();
            if nika_types::net::loopback_declassified(allowlist, &literal) {
                // the author declared this exact loopback — the carve-out.
            } else {
                last = std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("DNS rebinding refused: {host} resolves to the blocked range {addr}"),
                );
                continue;
            }
        }
        match TcpStream::connect_timeout(&addr, UPSTREAM_DIAL_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Full-duplex tunnel after the upgrade: both directions copy until EOF,
/// each direction's EOF propagated as a write-shutdown to the other side
/// (half-close fidelity — a server's `close_notify` reaches the client).
/// The handshake deadlines are cleared first: the tunnel outlives them.
/// F-P5 metering: both directions' byte counts are captured and journalled
/// as ONE `Closed` event when the tunnel ends — the TLS-blind posture
/// swears host + port + octets, never the content.
fn relay(client: TcpStream, upstream: TcpStream, host: &str, port: u16, observer: &EgressObserver) {
    let _ = client.set_read_timeout(None);
    let _ = client.set_write_timeout(None);
    let _ = upstream.set_read_timeout(None);
    let _ = upstream.set_write_timeout(None);
    let (Ok(mut client_w), Ok(mut upstream_w)) = (client.try_clone(), upstream.try_clone()) else {
        return;
    };
    let (mut client_r, mut upstream_r) = (client, upstream);
    let west = std::thread::Builder::new()
        .name("nika-egress-relay".to_owned())
        .spawn(move || {
            let up = std::io::copy(&mut client_r, &mut upstream_w);
            let _ = upstream_w.shutdown(Shutdown::Write);
            up
        });
    let down = std::io::copy(&mut upstream_r, &mut client_w);
    let _ = client_w.shutdown(Shutdown::Write);
    // A copy error (or a failed relay-thread spawn) reports the bytes
    // moved before the failure — 0 when the direction never ran. The
    // counters are what the proxy RELAYED, errors included.
    let bytes_up = west
        .ok()
        .and_then(|w| w.join().ok())
        .and_then(Result::ok)
        .unwrap_or(0);
    let bytes_down = down.unwrap_or(0);
    observer(&EgressEvent::Closed {
        host: host.to_owned(),
        port,
        bytes_up,
        bytes_down,
    });
}

/// The HTTP `CONNECT` handler (HTTPS tunneling). Reads the header block
/// (capped), parses the authority-form target, decides, then either tunnels
/// (`200 Connection Established`) or refuses (`403`). A non-CONNECT request
/// gets an honest `405` — forward-proxy GETs are out of scope by design.
fn serve_connect(mut client: TcpStream, allowlist: &[String], observer: &EgressObserver) {
    let Some(head) = read_header_block(&mut client) else {
        return;
    };
    let Some(line) = head.lines().next() else {
        return;
    };
    let mut parts = line.split_whitespace();
    let (method, authority) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if !method.eq_ignore_ascii_case("CONNECT") {
        let _ = client.write_all(
            b"HTTP/1.1 405 Method Not Allowed\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        );
        return;
    }
    let Some((host, port)) = parse_authority(authority) else {
        let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
        return;
    };
    if !decide(&host, port, allowlist, observer) {
        let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n");
        return;
    }
    let Ok(upstream) = dial_upstream(&host, port, allowlist) else {
        let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n");
        return;
    };
    if client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .is_err()
    {
        return;
    }
    relay(client, upstream, &host, port, observer);
}

/// Read up to the end of the CONNECT header block (`\r\n\r\n`), capped at
/// [`CONNECT_HEADER_CAP`] bytes — a header flood is refused by silence
/// (close), never buffered unboundedly. Non-UTF8/control garbage fails the
/// line parse downstream.
fn read_header_block(client: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    loop {
        if buf.len() >= CONNECT_HEADER_CAP {
            return None;
        }
        let n = client.read(&mut chunk).ok()?;
        if n == 0 {
            return None; // EOF before the block completed.
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return Some(String::from_utf8_lossy(&buf).into_owned());
        }
    }
}

/// Parse the authority-form target `host:port` — bracketed v6 (`[::1]:443`)
/// included. The host token must be free of whitespace/control chars: it is
/// journalised verbatim to a terminal (the terminal-escape injection
/// boundary) and fed to the matcher (a control char can never be a host).
fn parse_authority(authority: &str) -> Option<(String, u16)> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (v6, port) = rest.split_once("]:")?;
        (v6, port)
    } else {
        authority.rsplit_once(':')?
    };
    if host.is_empty() || host.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    Some((host.to_owned(), port))
}

/// The SOCKS5 handler (raw TCP — SSH, databases, everything that is not
/// HTTP): greeting → no-auth method → CONNECT request → decide → tunnel.
/// Only the CONNECT command is served; BIND/UDP-ASSOCIATE are refused
/// (`0x07` command not supported).
fn serve_socks5(mut client: TcpStream, allowlist: &[String], observer: &EgressObserver) {
    // Greeting: VER=5, NMETHODS, methods… — answer no-auth (0x00) when
    // offered, 0xFF ("no acceptable methods") otherwise.
    let Some(greeting) = read_exact_buf(&mut client, 2) else {
        return;
    };
    let nmethods = usize::from(greeting[1]);
    let Some(methods) = read_exact_buf(&mut client, nmethods) else {
        return;
    };
    if greeting[0] != 0x05 || nmethods == 0 || !methods.contains(&0x00) {
        let _ = client.write_all(&[0x05, 0xFF]);
        return;
    }
    if client.write_all(&[0x05, 0x00]).is_err() {
        return;
    }
    // Request: VER CMD RSV ATYP ADDR PORT — CONNECT (0x01) only.
    let Some(head) = read_exact_buf(&mut client, 4) else {
        return;
    };
    if head[0] != 0x05 || head[1] != 0x01 {
        let _ = client.write_all(&socks_reply(0x07));
        return;
    }
    let Some((host, port)) = read_socks_address(&mut client, head[3]) else {
        let _ = client.write_all(&socks_reply(0x08));
        return;
    };
    if !decide(&host, port, allowlist, observer) {
        let _ = client.write_all(&socks_reply(0x02)); // not allowed by ruleset
        return;
    }
    let Ok(upstream) = dial_upstream(&host, port, allowlist) else {
        let _ = client.write_all(&socks_reply(0x05)); // connection refused
        return;
    };
    if client.write_all(&socks_reply(0x00)).is_err() {
        return;
    }
    relay(client, upstream, &host, port, observer);
}

/// Read exactly `n` bytes (handshake-sized — the read deadline bounds it).
fn read_exact_buf(client: &mut TcpStream, n: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; n];
    client.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Parse the SOCKS5 address body per ATYP: v4 (0x01), domain (0x03 — the
/// form that keeps DNS on the proxy side, `socks5h`), v6 (0x04). Domain
/// tokens pass through [`parse_authority`]'s host validation indirectly —
/// a control/whitespace char can never be a host (or a journal line).
fn read_socks_address(client: &mut TcpStream, atyp: u8) -> Option<(String, u16)> {
    let host = match atyp {
        0x01 => {
            let b = read_exact_buf(client, 4)?;
            Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string()
        }
        0x03 => {
            let len = read_exact_buf(client, 1)?[0] as usize;
            let b = read_exact_buf(client, len)?;
            let h = String::from_utf8(b).ok()?;
            if h.is_empty() || h.chars().any(|c| c.is_control() || c.is_whitespace()) {
                return None;
            }
            h
        }
        0x04 => {
            let b = read_exact_buf(client, 16)?;
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&b);
            std::net::Ipv6Addr::from(octets).to_string()
        }
        _ => return None,
    };
    let p = read_exact_buf(client, 2)?;
    Some((host, u16::from_be_bytes([p[0], p[1]])))
}

/// A SOCKS5 reply with a nil bind address (`0.0.0.0:0`) — clients ignore
/// the bind field on CONNECT replies.
fn socks_reply(rep: u8) -> [u8; 10] {
    [0x05, rep, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// The env contract injected into an allowlisted child — mirrors srt's
/// `generateProxyEnvVars` on the ONE muxed port: `HTTP(S)_PROXY` (both
/// cases) for the CONNECT speakers, `ALL_PROXY` + `GRPC_PROXY` as the http
/// URL (srt's own choice — gRPC C-core speaks CONNECT only, and Python
/// httpx crashes on a socks URL it cannot import), `FTP_PROXY` on the
/// `socks5h` form (transparent TCP), `RSYNC_PROXY` as bare host:port, and
/// `NO_PROXY` for the loopback + RFC1918 direct set (srt's verbatim list).
/// Every name is SET (both cases), so an ambient inherited value can never
/// outrank the boundary — the confined child's only egress is the proxy.
pub(crate) fn proxy_env(port: u16) -> [(&'static str, String); 13] {
    let http = format!("http://127.0.0.1:{port}");
    let socks = format!("socks5h://127.0.0.1:{port}");
    let no_proxy = "localhost,127.0.0.1,::1,169.254.0.0/16,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";
    [
        ("HTTP_PROXY", http.clone()),
        ("http_proxy", http.clone()),
        ("HTTPS_PROXY", http.clone()),
        ("https_proxy", http.clone()),
        ("ALL_PROXY", http.clone()),
        ("all_proxy", http.clone()),
        ("GRPC_PROXY", http.clone()),
        ("grpc_proxy", http),
        ("FTP_PROXY", socks.clone()),
        ("ftp_proxy", socks),
        ("RSYNC_PROXY", format!("127.0.0.1:{port}")),
        ("NO_PROXY", no_proxy.to_owned()),
        ("no_proxy", no_proxy.to_owned()),
    ]
}

/// The guard the runner applies before reusing the running proxy: a second,
/// DIFFERENT `permits.net.http` set in the same run is refused (fail-closed)
/// rather than silently widened — the per-run proxy serves one boundary.
pub(crate) fn conflicting_boundary(proxy: &EgressProxy, spec: &EgressAllowlist) -> bool {
    proxy.allowlist() != spec.hosts.as_slice()
}

/// The runner-side wiring of the allowlist arm (kept here so `lib.rs` stays
/// a lean process-lifecycle surface — this module owns the whole egress
/// concern, proxy AND confinement hand-off).
type AppliedSandbox = (
    nika_kernel::ShellCommand,
    Option<std::path::PathBuf>,
    Option<std::sync::Arc<dyn nika_kernel::command_sandbox::CommandSandbox>>,
);

impl crate::TokioShell {
    /// Apply the injected OS sandbox to a command that requests one. No spec =
    /// unchanged (today's behavior); spec + backend = confined; spec but NO
    /// backend = fail-closed (refuse rather than run an asked-for-confined
    /// command unconfined).
    ///
    /// The `NetPolicy::Allowlist` arm first secures its per-run loopback
    /// egress proxy (started lazily on first use): the child gets the proxy
    /// env contract (srt-mirrored — every name SET, both cases, so ambient
    /// values never outrank the boundary) and the spec handed to the backend
    /// learns the proxy's port (the seatbelt fence scopes outbound loopback
    /// to exactly it).
    ///
    /// The seatbelt arm also mints the per-spawn private scratch (issue 754
    /// · the blanket shared-tmp grant left the profile): a fresh dir under
    /// the runner's temp becomes the child's `TMPDIR` (the authored `env:`
    /// map wins if the author set one) and rides `fs_write` like any other
    /// declared prefix. Returned beside the command so [`crate::TokioShell`]
    /// removes it when the spawn settles. The landlock jail needs none — it
    /// already mounts a private tmpfs `/tmp`.
    pub(super) fn apply_sandbox(
        &self,
        mut command: nika_kernel::ShellCommand,
    ) -> Result<AppliedSandbox, nika_kernel::ShellError> {
        use nika_kernel::{ShellError, process::NetPolicy};
        let Some(mut spec) = command.sandbox.take() else {
            return Ok((command, None, None));
        };
        let Some(backend) = &self.sandbox else {
            return Err(ShellError::Blocked {
                reason: "command requires an OS sandbox but no backend is wired \
                         (refusing to run unconfined)"
                    .to_string(),
            });
        };
        // B05 / B29 · issue 1295 — seatbelt's preamble must keep
        // `/private/etc` so dyld/getpwuid can start; that system grant
        // must not become `cat /etc/passwd` under `permits.exec: ["cat"]`.
        // File-plumbing of a host path the jail does not admit is refused
        // here, before confine, without weakening the preamble.
        if !command.shell {
            let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
            let cwd = command.cwd.as_ref().and_then(|p| p.to_str());
            if let Some(path) =
                nika_cap::file_plumbing_host_escape(&command.program, &args, cwd, |p| {
                    spec.fs_read
                        .iter()
                        .chain(spec.fs_write.iter())
                        .any(|g| nika_cap::glob_admits(g, p))
                })
            {
                return Err(ShellError::Blocked {
                    reason: format!(
                        "file-plumbing `{path}` is outside the declared fs jail (NIKA-SEC-004)"
                    ),
                });
            }
        }
        if let NetPolicy::Allowlist(allowlist) = &mut spec.net {
            let port = self.ensure_egress_proxy(allowlist)?;
            for (name, value) in proxy_env(port) {
                command.env.insert(name.to_owned(), value);
            }
            allowlist.proxy_port = Some(port);
        }
        let scratch = if backend.backend() == "seatbelt" {
            let dir = crate::scratch::mint_scratch_dir()?;
            let path = dir.to_string_lossy().into_owned();
            command
                .env
                .entry("TMPDIR".to_owned())
                .or_insert_with(|| path.clone());
            // Trailing slash on purpose: a directory grant, so the profile
            // emits a plain subpath (no exact-file sidecars, no parent
            // listing of the shared temp root).
            spec.fs_write.push(format!("{path}/"));
            Some(dir)
        } else {
            None
        };
        let confined = backend
            .confine(&spec, command)
            .map_err(|e| crate::map_sandbox_error(&e))?;
        Ok((confined, scratch, Some(std::sync::Arc::clone(backend))))
    }

    /// Secure the per-run loopback egress proxy for the allowlist arm:
    /// started on first use, REUSED for the identical boundary, and a second
    /// DIFFERENT `permits.net.http` set in the same run is refused
    /// (fail-closed) rather than silently widened — one run, one boundary.
    fn ensure_egress_proxy(
        &self,
        allowlist: &EgressAllowlist,
    ) -> Result<u16, nika_kernel::ShellError> {
        use nika_kernel::ShellError;
        let mut cell = self.egress_proxy.lock().map_err(|_| ShellError::Other {
            reason: "the egress proxy lock is poisoned".to_string(),
        })?;
        if let Some(proxy) = cell.as_ref() {
            if conflicting_boundary(proxy, allowlist) {
                return Err(ShellError::Blocked {
                    reason: "a second, different permits.net.http set reached the \
                             per-run egress proxy — refusing to widen the boundary \
                             (one run, one allowlist)"
                        .to_string(),
                });
            }
            return Ok(proxy.port());
        }
        let proxy = EgressProxy::start(allowlist.hosts.clone(), Arc::clone(&self.egress_observer))
            .map_err(|e| ShellError::Blocked {
                reason: format!(
                    "could not start the loopback egress proxy for the allowlist arm: {e}"
                ),
            })?;
        let port = proxy.port();
        *cell = Some(proxy);
        Ok(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A collecting observer — every event the proxy journalised, in order.
    #[derive(Clone, Default)]
    struct Probe {
        events: Arc<Mutex<Vec<EgressEvent>>>,
    }

    impl Probe {
        fn observer(&self) -> EgressObserver {
            let events = Arc::clone(&self.events);
            Arc::new(move |e| {
                if let Ok(mut v) = events.lock() {
                    v.push(e.clone());
                }
            })
        }

        /// The Decision rail (the allow/refuse verdicts), in order.
        fn decisions(&self) -> Vec<EgressDecision> {
            self.events
                .lock()
                .map(|v| {
                    v.iter()
                        .filter_map(|e| match e {
                            EgressEvent::Decision(d) => Some(d.clone()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        }

        /// The metering rail — `(host, port, bytes_up, bytes_down)` per
        /// relayed tunnel, in order.
        fn closed(&self) -> Vec<(String, u16, u64, u64)> {
            self.events
                .lock()
                .map(|v| {
                    v.iter()
                        .filter_map(|e| match e {
                            EgressEvent::Closed {
                                host,
                                port,
                                bytes_up,
                                bytes_down,
                            } => Some((host.clone(), *port, *bytes_up, *bytes_down)),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    /// Poll `cond` until it holds (10 ms steps · 2 s ceiling) — the Closed
    /// event rides the relay thread, so a metering assertion waits for it.
    fn wait_for(cond: impl Fn() -> bool) {
        for _ in 0..200 {
            if cond() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("condition not met within 2s");
    }

    /// A throwaway upstream: accepts one connection, echoes what it reads.
    fn echo_upstream() -> (u16, JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind upstream");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::Builder::new()
            .name("nika-egress-test-echo".to_owned())
            .spawn(move || {
                if let Ok((mut s, _)) = listener.accept() {
                    let mut buf = [0u8; 64];
                    if let Ok(n) = s.read(&mut buf) {
                        let _ = s.write_all(&buf[..n]);
                    }
                }
            })
            .expect("echo thread");
        (port, handle)
    }

    fn start(hosts: &[&str]) -> (EgressProxy, Probe) {
        let probe = Probe::default();
        let proxy = EgressProxy::start(
            hosts.iter().map(|h| (*h).to_owned()).collect(),
            probe.observer(),
        )
        .expect("proxy starts on loopback");
        (proxy, probe)
    }

    fn dial(port: u16) -> TcpStream {
        TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("proxy accepts loopback")
    }

    #[test]
    fn connect_allowed_relays_bytes_and_journals() {
        let (upstream_port, echo) = echo_upstream();
        let (proxy, probe) = start(&["127.0.0.1"]);
        let mut c = dial(proxy.port());
        write!(
            c,
            "CONNECT 127.0.0.1:{upstream_port} HTTP/1.1\r\nHost: x\r\n\r\n"
        )
        .unwrap();
        let mut head = Vec::new();
        let mut buf = [0u8; 128];
        // read the 200 line, then the echoed payload through the tunnel
        let n = c.read(&mut buf).unwrap();
        head.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&head);
        assert!(text.contains("200 Connection Established"), "got: {text}");
        c.write_all(b"PING").unwrap();
        let n = c.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"PING", "the tunnel echoes both ways");
        echo.join().unwrap();
        let decisions = probe.decisions();
        assert_eq!(
            decisions,
            vec![EgressDecision {
                host: "127.0.0.1".to_owned(),
                port: upstream_port,
                allowed: true,
            }],
            "the allowed decision is journalised"
        );
    }

    #[test]
    fn a_relayed_tunnel_reports_its_byte_counters_at_close() {
        // F-P5 (e) · the TLS-blind metering: ONE Closed event per relayed
        // connection, swearing host + port + octets — never the content.
        let (upstream_port, echo) = echo_upstream();
        let (proxy, probe) = start(&["127.0.0.1"]);
        let mut c = dial(proxy.port());
        write!(c, "CONNECT 127.0.0.1:{upstream_port} HTTP/1.1\r\n\r\n").unwrap();
        let mut buf = [0u8; 64];
        let n = c.read(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).contains("200"));
        c.write_all(b"PING").unwrap(); // 4 octets up · 4 echoed down
        let n = c.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"PING");
        echo.join().unwrap();
        drop(c); // closing the client ends the tunnel → the Closed event
        wait_for(|| !probe.closed().is_empty());
        assert_eq!(
            probe.closed(),
            vec![("127.0.0.1".to_owned(), upstream_port, 4, 4)],
            "the metering swears octets, never content"
        );
    }

    #[test]
    fn the_journal_lines_are_the_greppable_contract() {
        // F-P5 (b) · REFUSED is the security event, verbatim — the
        // composer greps this line; `allowed` and `closed` are the debug
        // and metering rows.
        let refused = journal_line(&EgressEvent::Decision(EgressDecision {
            host: "evil.com".to_owned(),
            port: 443,
            allowed: false,
        }));
        assert_eq!(
            refused,
            "nika:egress REFUSED evil.com:443 (not in permits.net.http)"
        );
        let allowed = journal_line(&EgressEvent::Decision(EgressDecision {
            host: "api.github.com".to_owned(),
            port: 443,
            allowed: true,
        }));
        assert_eq!(allowed, "nika:egress allowed api.github.com:443");
        let closed = journal_line(&EgressEvent::Closed {
            host: "api.github.com".to_owned(),
            port: 443,
            bytes_up: 128,
            bytes_down: 4096,
        });
        assert_eq!(
            closed,
            "nika:egress closed api.github.com:443 up=128 down=4096"
        );
    }

    #[test]
    fn rebinding_to_a_blocked_range_is_refused_unless_exact_loopback_is_declared() {
        // H1 · localhost resolves to 127.0.0.1 (a blocked range): refused
        // by default, admitted only when the author declared THAT exact
        // literal (the #395 carve-out · parity with the nika-http floor).
        let err = dial_upstream("localhost", 80, &[]).expect_err("refused");
        let text = format!("{err}");
        assert!(text.contains("rebinding"), "got: {text}");
        // The exact loopback literal declassifies → the dial proceeds
        // (to a listener we own here, so it must succeed).
        let (port, echo) = echo_upstream();
        let mut stream = dial_upstream("localhost", port, &["127.0.0.1".to_owned()])
            .expect("the declared exact loopback passes");
        // The echo listener answers one write — close its thread cleanly.
        std::io::Write::write_all(&mut stream, b"x").expect("write the probe byte");
        echo.join().unwrap();
    }

    #[test]
    fn the_port_floor_refuses_classic_non_http_services() {
        // H3 · a net.http permit never admits ssh/smtp/docker/kube ports.
        // Every DECLARED port, not a sample: the four hand-typed ones this
        // test used to carry left ten of the fourteen unproven, and an
        // entry nobody exercises is an entry nobody would miss.
        let probe = Probe::default();
        let observer = probe.observer();
        for &port in DANGEROUS_EGRESS_PORTS {
            assert!(
                !decide(
                    "api.example.com",
                    port,
                    &["api.example.com".to_owned()],
                    &observer
                ),
                "port {port} is on the floor and must be refused"
            );
        }
        // The floor only means something against an open door.
        for port in [80, 443, 8080, 8443] {
            assert!(
                decide(
                    "api.example.com",
                    port,
                    &["api.example.com".to_owned()],
                    &observer
                ),
                "port {port} is ordinary HTTP egress and must pass"
            );
        }
    }

    #[test]
    fn the_floor_never_loses_a_port() {
        // Iterating the list above proves what IS there; it cannot notice a
        // deletion — the loop simply stops visiting what is gone. This names
        // each port and the relay a `net.http` permit would become without it.
        const FLOOR: &[(u16, &str)] = &[
            (22, "ssh — a shell on the far side of an HTTP grant"),
            (23, "telnet — the same, in clear text"),
            (25, "smtp — mail relay, the classic spam confused deputy"),
            (110, "pop3 — mailbox read"),
            (143, "imap — mailbox read"),
            (445, "smb — file shares, and the NTLM-relay family"),
            (2375, "docker daemon (plain) — root on the host"),
            (2376, "docker daemon (tls) — the same door, wearing a cert"),
            (3306, "mysql — direct database access"),
            (5432, "postgres — direct database access"),
            (6379, "redis — unauthenticated by default"),
            (9200, "elasticsearch — bulk read of whatever it indexes"),
            (10250, "kubelet — exec into any pod on the node"),
            (27017, "mongodb — direct database access"),
        ];
        for (port, why) in FLOOR {
            assert!(
                DANGEROUS_EGRESS_PORTS.contains(port),
                "port {port} left the floor · it opens: {why}"
            );
        }
    }

    #[test]
    fn connect_refused_is_403_and_journalised_as_a_security_event() {
        let (upstream_port, _echo) = echo_upstream();
        let (proxy, probe) = start(&["example.com"]); // 127.0.0.1 NOT declared
        let mut c = dial(proxy.port());
        write!(c, "CONNECT 127.0.0.1:{upstream_port} HTTP/1.1\r\n\r\n").unwrap();
        let mut buf = [0u8; 128];
        let n = c.read(&mut buf).unwrap();
        let text = String::from_utf8_lossy(&buf[..n]);
        assert!(text.starts_with("HTTP/1.1 403"), "refused: {text}");
        let decisions = probe.decisions();
        assert_eq!(decisions.len(), 1);
        assert!(!decisions[0].allowed, "the refusal is the security event");
        assert_eq!(decisions[0].host, "127.0.0.1");
    }

    #[test]
    fn socks5_allowed_relays_and_journals() {
        let (upstream_port, echo) = echo_upstream();
        let (proxy, probe) = start(&["127.0.0.1"]);
        let mut c = dial(proxy.port());
        // greeting: VER=5, 1 method, no-auth
        c.write_all(&[0x05, 0x01, 0x00]).unwrap();
        let mut reply = [0u8; 2];
        c.read_exact(&mut reply).unwrap();
        assert_eq!(reply, [0x05, 0x00], "no-auth accepted");
        // CONNECT 127.0.0.1:port (ATYP v4)
        let mut req = vec![0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1];
        req.extend_from_slice(&upstream_port.to_be_bytes());
        c.write_all(&req).unwrap();
        let mut granted = [0u8; 10];
        c.read_exact(&mut granted).unwrap();
        assert_eq!(granted[1], 0x00, "succeeded: {granted:?}");
        c.write_all(b"PONG").unwrap();
        let mut buf = [0u8; 4];
        c.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"PONG");
        echo.join().unwrap();
        assert_eq!(
            probe.decisions(),
            vec![EgressDecision {
                host: "127.0.0.1".to_owned(),
                port: upstream_port,
                allowed: true,
            }]
        );
    }

    #[test]
    fn socks5_refused_domain_is_ruleset_denied_and_journalised() {
        let (proxy, probe) = start(&["example.com"]);
        let mut c = dial(proxy.port());
        c.write_all(&[0x05, 0x01, 0x00]).unwrap();
        let mut reply = [0u8; 2];
        c.read_exact(&mut reply).unwrap();
        // CONNECT evil.com:443 (ATYP domain)
        let host = b"evil.com";
        let mut req = vec![0x05, 0x01, 0x00, 0x03, u8::try_from(host.len()).unwrap()];
        req.extend_from_slice(host);
        req.extend_from_slice(&443u16.to_be_bytes());
        c.write_all(&req).unwrap();
        let mut granted = [0u8; 10];
        c.read_exact(&mut granted).unwrap();
        assert_eq!(granted[1], 0x02, "not allowed by ruleset: {granted:?}");
        assert_eq!(
            probe.decisions(),
            vec![EgressDecision {
                host: "evil.com".to_owned(),
                port: 443,
                allowed: false,
            }]
        );
    }

    /// The wire verdict IS the ONE matcher's verdict: for a battery of
    /// (allowlist, host) rows, the proxy's decision over a real connection
    /// equals `host_in_allowlist` — the same predicate `nika-http` and the
    /// static check enforce (no second glob dialect, check ≡ run ≡ jail).
    #[test]
    fn the_wire_decision_is_the_shared_matchers_decision() {
        let hosts = ["api.example.com", "*.github.com"];
        let cases: &[(&str, bool)] = &[
            ("api.example.com", true),
            ("API.example.com", true), // case-insensitive (RFC 4343)
            ("github.com", true),      // *.x admits the bare apex
            ("raw.github.com", true),
            ("github.com.evil.com", false),
            ("example.com", false),
            ("127.0.0.1", false),
        ];
        let owned: Vec<String> = hosts.iter().map(|h| (*h).to_owned()).collect();
        for (host, _) in cases {
            assert_eq!(
                nika_types::net::host_in_allowlist(&owned, host),
                decide(host, 443, &owned, &Probe::default().observer()),
                "wire ≡ matcher for {host}"
            );
        }
        // …and the bare-* hatch opens every row (the explicit escape hatch).
        let star = vec!["*".to_owned()];
        assert!(decide(
            "anything.test",
            443,
            &star,
            &Probe::default().observer()
        ));
    }

    #[test]
    fn a_non_connect_request_gets_an_honest_405() {
        let (proxy, _probe) = start(&["example.com"]);
        let mut c = dial(proxy.port());
        write!(
            c,
            "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n"
        )
        .unwrap();
        let mut buf = [0u8; 128];
        let n = c.read(&mut buf).unwrap();
        let text = String::from_utf8_lossy(&buf[..n]);
        assert!(text.starts_with("HTTP/1.1 405"), "got: {text}");
    }

    #[test]
    fn authority_parsing_is_strict() {
        assert_eq!(
            parse_authority("api.example.com:443"),
            Some(("api.example.com".to_owned(), 443))
        );
        assert_eq!(
            parse_authority("[::1]:8443"),
            Some(("::1".to_owned(), 8443))
        );
        // a control char or space can never ride into a journal line
        assert!(parse_authority("evil.com:443\n(allow").is_none());
        assert!(parse_authority("evil host:443").is_none());
        assert!(parse_authority(":443").is_none());
        assert!(parse_authority("host:notaport").is_none());
        assert!(parse_authority("host:99999").is_none());
    }

    /// The no-orphan assertion, robust under the plain-`cargo test` shared
    /// process: a freed ephemeral port can be RE-BOUND by a parallel test's
    /// own proxy, so a bare `connect` failing is not the only green outcome.
    /// What must NEVER answer is OUR proxy: probe `127.0.0.2` (loopback, no
    /// DNS, in NO other test's allowlist) — our zombie would 502 it
    /// (allowlisted, undialable), a dead port refuses, a foreign proxy 403s.
    fn assert_no_orphan_on(port: u16, probe_hosts: &[&str]) {
        for host in probe_hosts {
            let Ok(mut c) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) else {
                continue; // dead, as required
            };
            let _ = c.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = write!(c, "CONNECT {host}:9 HTTP/1.1\r\n\r\n");
            let mut buf = [0u8; 64];
            let n = c.read(&mut buf).unwrap_or(0);
            let text = String::from_utf8_lossy(&buf[..n]);
            assert!(
                !text.contains("502"),
                "an orphan egress listener outlived its run on :{port}: {text}"
            );
        }
    }

    #[test]
    fn proxy_dies_with_its_owner_no_orphan_listener() {
        let (proxy, _probe) = start(&["127.0.0.2"]);
        let port = proxy.port();
        dial(port); // alive: the listener answers
        drop(proxy);
        assert_no_orphan_on(port, &["127.0.0.2"]);
    }

    #[test]
    fn boundary_reuse_and_conflict_detection() {
        let (proxy, _probe) = start(&["a.test", "b.test"]);
        let same = EgressAllowlist::new(vec!["a.test".to_owned(), "b.test".to_owned()]);
        let other = EgressAllowlist::new(vec!["c.test".to_owned()]);
        assert!(!conflicting_boundary(&proxy, &same));
        assert!(conflicting_boundary(&proxy, &other));
    }

    #[test]
    fn proxy_env_mirrors_the_srt_contract_on_one_muxed_port() {
        let env = proxy_env(60080);
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("HTTP_PROXY"), Some("http://127.0.0.1:60080"));
        assert_eq!(get("https_proxy"), Some("http://127.0.0.1:60080"));
        assert_eq!(get("ALL_PROXY"), Some("http://127.0.0.1:60080"));
        assert_eq!(get("grpc_proxy"), Some("http://127.0.0.1:60080"));
        assert_eq!(get("FTP_PROXY"), Some("socks5h://127.0.0.1:60080"));
        assert_eq!(get("RSYNC_PROXY"), Some("127.0.0.1:60080"));
        let no_proxy = get("NO_PROXY").expect("NO_PROXY set");
        for want in [
            "localhost",
            "127.0.0.1",
            "::1",
            "10.0.0.0/8",
            "192.168.0.0/16",
        ] {
            assert!(no_proxy.contains(want), "NO_PROXY missing {want}");
        }
        assert_eq!(get("no_proxy"), get("NO_PROXY"), "both cases, one value");
    }

    // ── the runner wiring (apply_sandbox · the allowlist arm end-to-end) ──

    use nika_kernel::command_sandbox::{CommandSandbox, CommandSandboxError};
    use nika_kernel::process::{NetPolicy, SandboxSpec};
    use nika_kernel::{ShellCommand, ShellError};

    /// A backend that records the spec + env it confined (pass-through).
    #[derive(Debug, Default)]
    struct CaptureSandbox {
        seen: Mutex<Vec<(SandboxSpec, std::collections::BTreeMap<String, String>)>>,
    }

    impl CommandSandbox for CaptureSandbox {
        fn confine(
            &self,
            spec: &SandboxSpec,
            command: ShellCommand,
        ) -> Result<ShellCommand, CommandSandboxError> {
            if let Ok(mut v) = self.lock() {
                v.push((spec.clone(), command.env.clone()));
            }
            Ok(command)
        }
        fn backend(&self) -> &'static str {
            "capture"
        }
    }

    impl CaptureSandbox {
        #[allow(clippy::type_complexity)]
        fn lock(
            &self,
        ) -> std::sync::LockResult<
            std::sync::MutexGuard<
                '_,
                Vec<(SandboxSpec, std::collections::BTreeMap<String, String>)>,
            >,
        > {
            self.seen.lock()
        }
    }

    fn allowlisted(hosts: &[&str]) -> SandboxSpec {
        let mut spec = SandboxSpec::new();
        spec.net = NetPolicy::Allowlist(EgressAllowlist::new(
            hosts.iter().map(|h| (*h).to_owned()).collect(),
        ));
        spec
    }

    fn confined(shell: &crate::TokioShell, spec: SandboxSpec) -> ShellCommand {
        let mut cmd = ShellCommand::new("echo");
        cmd.args = vec!["x".to_owned()];
        cmd.sandbox = Some(spec);
        shell.apply_sandbox(cmd).expect("confined").0
    }

    #[test]
    fn allowlist_arm_injects_the_env_and_the_proxy_port() {
        let backend = Arc::new(CaptureSandbox::default());
        let probe = Probe::default();
        let shell =
            crate::TokioShell::with_sandbox(backend.clone()).with_egress_observer(probe.observer());
        let out = confined(&shell, allowlisted(&["api.example.com", "127.0.0.2"]));

        // the srt env contract points at the running proxy, both cases set
        let url = out.env.get("HTTP_PROXY").expect("HTTP_PROXY injected");
        let port: u16 = url
            .strip_prefix("http://127.0.0.1:")
            .expect("loopback url")
            .parse()
            .expect("a port");
        assert_eq!(out.env.get("https_proxy"), Some(url));
        assert_eq!(
            out.env.get("ALL_PROXY").map(String::as_str),
            Some(url.as_str())
        );
        assert!(out.env.contains_key("NO_PROXY"));

        // the spec the backend confined carries the proxy's port (the
        // seatbelt fence scopes loopback to exactly it)
        let seen = backend.lock().unwrap();
        let (spec, _) = &seen[0];
        match &spec.net {
            NetPolicy::Allowlist(a) => assert_eq!(a.proxy_port, Some(port)),
            other => panic!("expected allowlist, got {other:?}"),
        }
        drop(seen);

        // a second command with the SAME boundary reuses the SAME proxy
        let out2 = confined(&shell, allowlisted(&["api.example.com", "127.0.0.2"]));
        assert_eq!(out2.env.get("HTTP_PROXY"), Some(url));

        // and the proxy refuses a non-declared host WITH the journal line
        let mut c = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(c, "CONNECT evil.com:443 HTTP/1.1\r\n\r\n").unwrap();
        let mut buf = [0u8; 64];
        let n = c.read(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 403"));
        let decisions = probe.decisions();
        assert_eq!(decisions.len(), 1);
        assert!(!decisions[0].allowed, "the refused host is journalised");
        assert_eq!(decisions[0].host, "evil.com");

        // the proxy dies with the shell — no orphan listener
        drop(shell);
        assert_no_orphan_on(port, &["127.0.0.2"]);
    }

    #[test]
    fn deny_and_allow_arms_start_no_proxy_and_inject_no_env() {
        for arm in [NetPolicy::Deny, NetPolicy::Allow] {
            let backend = Arc::new(CaptureSandbox::default());
            let shell = crate::TokioShell::with_sandbox(backend);
            let mut spec = SandboxSpec::new();
            spec.net = arm.clone();
            let out = confined(&shell, spec);
            assert!(
                !out.env.contains_key("HTTP_PROXY") && !out.env.contains_key("http_proxy"),
                "arm {arm:?} injects no proxy env"
            );
            assert!(
                shell.egress_proxy.lock().unwrap().is_none(),
                "arm {arm:?} starts no proxy"
            );
        }
    }

    #[test]
    fn a_conflicting_boundary_in_one_run_is_refused_fail_closed() {
        let backend = Arc::new(CaptureSandbox::default());
        let shell = crate::TokioShell::with_sandbox(backend);
        let _ = confined(&shell, allowlisted(&["a.test"]));
        let mut cmd = ShellCommand::new("echo");
        cmd.sandbox = Some(allowlisted(&["b.test"]));
        let Err(err) = shell.apply_sandbox(cmd) else {
            panic!("conflicting egress boundaries must refuse");
        };
        assert!(
            matches!(err, ShellError::Blocked { .. }),
            "a second boundary must not widen the per-run proxy: {err}"
        );
    }

    #[test]
    fn allowlist_without_a_backend_is_refused_before_the_proxy_starts() {
        let shell = crate::TokioShell::new(); // no backend wired
        let mut cmd = ShellCommand::new("echo");
        cmd.sandbox = Some(allowlisted(&["a.test"]));
        assert!(matches!(
            shell.apply_sandbox(cmd),
            Err(ShellError::Blocked { .. })
        ));
        assert!(
            shell.egress_proxy.lock().unwrap().is_none(),
            "a refused command starts no proxy"
        );
    }

    #[test]
    fn confined_cat_of_a_host_path_is_blocked_before_spawn() {
        // B05 / B29 · issue 1295 — `cat` of a host absolute path is
        // refused before confine. The backend records nothing.
        let backend = Arc::new(CaptureSandbox::default());
        let shell = crate::TokioShell::with_sandbox(backend.clone());
        let mut cmd = ShellCommand::new("cat");
        cmd.args = vec!["/etc/passwd".to_owned()];
        cmd.sandbox = Some(SandboxSpec::new());
        let Err(err) = shell.apply_sandbox(cmd) else {
            panic!("host-path cat must refuse before confine");
        };
        match err {
            ShellError::Blocked { reason } => {
                assert!(
                    reason.contains("NIKA-SEC-004"),
                    "the refusal speaks the code, not a host-file dump: {reason}"
                );
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(
            backend.lock().unwrap().is_empty(),
            "confine was not reached"
        );
    }

    #[test]
    fn confined_cat_of_a_cwd_file_still_reaches_the_backend() {
        let backend = Arc::new(CaptureSandbox::default());
        let shell = crate::TokioShell::with_sandbox(backend.clone());
        let mut spec = SandboxSpec::new();
        spec.fs_read = vec!["./README.md".into()];
        let mut cmd = ShellCommand::new("cat");
        cmd.args = vec!["README.md".to_owned()];
        cmd.sandbox = Some(spec);
        shell.apply_sandbox(cmd).expect("cwd cat reaches confine");
        assert_eq!(backend.lock().unwrap().len(), 1, "confine ran");
    }

    /// A pass-through recorder that CLAIMS the seatbelt name — the scratch
    /// arm (issue 754) keys on `backend() == "seatbelt"`.
    #[derive(Debug, Default)]
    struct SeatbeltishSandbox(CaptureSandbox);

    impl CommandSandbox for SeatbeltishSandbox {
        fn confine(
            &self,
            spec: &SandboxSpec,
            command: ShellCommand,
        ) -> Result<ShellCommand, CommandSandboxError> {
            self.0.confine(spec, command)
        }
        fn backend(&self) -> &'static str {
            "seatbelt"
        }
    }

    /// Issue 754 — the shared host tmp left the profile, so the seatbelt
    /// arm must hand each confined spawn its OWN scratch: minted on disk,
    /// granted via `fs_write` as a directory prefix, and set as the
    /// child's `TMPDIR` — unless the AUTHOR set one (authored env wins).
    #[test]
    fn the_seatbelt_arm_mints_a_private_scratch_and_the_author_tmpdir_wins() {
        let backend = Arc::new(SeatbeltishSandbox::default());
        let shell = crate::TokioShell::with_sandbox(Arc::clone(&backend) as _);

        let mut cmd = ShellCommand::new("echo");
        cmd.sandbox = Some(SandboxSpec::new());
        let (out, scratch, _classifier) = shell.apply_sandbox(cmd).expect("confined");
        let dir = scratch.expect("the seatbelt arm mints a scratch");
        assert!(dir.is_dir(), "the scratch exists before the spawn");
        let tmpdir = out.env.get("TMPDIR").expect("TMPDIR is set").clone();
        assert_eq!(tmpdir, dir.to_string_lossy());
        let seen = backend.0.lock().expect("recorder");
        let (spec_seen, _) = seen.last().expect("one confinement");
        assert!(
            spec_seen
                .fs_write
                .iter()
                .any(|g| *g == format!("{tmpdir}/")),
            "the scratch rides fs_write as a directory grant: {:?}",
            spec_seen.fs_write
        );
        drop(seen);
        let _ = std::fs::remove_dir_all(&dir);

        // The authored TMPDIR wins — the scratch is still minted and
        // granted (the child needs SOME writable temp), never clobbering.
        let mut cmd = ShellCommand::new("echo");
        cmd.sandbox = Some(SandboxSpec::new());
        cmd.env
            .insert("TMPDIR".to_owned(), "/authored/tmp".to_owned());
        let (out, scratch, _classifier) = shell.apply_sandbox(cmd).expect("confined");
        assert_eq!(
            out.env.get("TMPDIR").map(String::as_str),
            Some("/authored/tmp"),
            "the authored env map outranks the minted scratch"
        );
        if let Some(dir) = scratch {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
