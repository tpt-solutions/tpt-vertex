//! WebSocket adapter for the transport-agnostic collaboration [`SyncHub`].
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! `server.rs` deliberately knows nothing about sockets: callers feed it
//! [`ClientMessage`]s tagged with a [`ConnId`] and route the [`Outbound`]s it
//! returns. This binary is that missing adapter — the thin, production-shaped
//! layer described in the `server` module docs:
//!
//! 1. accept TCP connections and upgrade them to WebSockets
//!    (`tokio-tungstenite`), assigning each socket a unique [`ConnId`];
//! 2. deserialize every inbound text frame as a JSON [`ClientMessage`] and hand
//!    it to the hub (one shared [`SyncHub`] behind a mutex — `handle` is cheap
//!    and never awaits, so the lock is only ever held for a synchronous call);
//! 3. serialize each returned [`Outbound`] as a JSON [`ServerMessage`] and push
//!    it to that connection's writer task, which fans it out to the socket.
//!
//! Because routing is driven entirely by the hub, access control, presence
//! propagation, snapshots, and offline resync all behave exactly as they do in
//! the in-process unit tests.
//!
//! Run it with:
//!
//! ```text
//! cargo run -p tpt-vertex-collab --bin sync_server -- --port 8787 --host 127.0.0.1
//! ```
//!
//! The frontend client lives in `frontend/src/collab/client.ts` and speaks the
//! same JSON encoding (serde's default externally-tagged enum representation).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

use tpt_vertex_collab::{
    AccessLevel, ClientMessage, ConnId, MemoryAuth, Outbound, ServerMessage, SyncHub,
};

/// Default listen port for the dev sync server.
const DEFAULT_PORT: u16 = 8787;
/// Default listen host (loopback only; bind `0.0.0.0` to expose it on the LAN).
const DEFAULT_HOST: &str = "127.0.0.1";
/// Join token accepted when no `--token` is supplied.
const DEFAULT_TOKEN: &str = "dev";

/// Parsed command-line configuration.
struct Args {
    host: String,
    port: u16,
    /// Accepted join tokens. Each token doubles as its subject identity.
    tokens: Vec<String>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            tokens: Vec::new(),
        }
    }
}

const USAGE: &str = "\
tpt-vertex collab sync server

USAGE:
    sync_server [--host <addr>] [--port <port>] [--token <token>]...

OPTIONS:
    --host <addr>    Interface to bind (default 127.0.0.1)
    --port <port>    TCP port to listen on (default 8787)
    --token <token>  Accepted join token, repeatable (default \"dev\")
    -h, --help       Print this help
";

/// Parse `--host` / `--port` / `--token`, accepting both `--k v` and `--k=v`.
fn parse_args<I: IntoIterator<Item = String>>(argv: I) -> Result<Option<Args>> {
    let mut args = Args::default();
    let mut it = argv.into_iter();
    while let Some(arg) = it.next() {
        let (key, inline) = match arg.split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (arg, None),
        };
        let mut value = || -> Result<String> {
            match inline.clone() {
                Some(v) => Ok(v),
                None => it.next().context(format!("missing value for {key}")),
            }
        };
        match key.as_str() {
            "-h" | "--help" => return Ok(None),
            "--host" => args.host = value()?,
            "--port" => {
                let raw = value()?;
                args.port = raw
                    .parse()
                    .with_context(|| format!("invalid port {raw:?}"))?;
            }
            "--token" => args.tokens.push(value()?),
            other => anyhow::bail!("unrecognized argument {other:?}\n\n{USAGE}"),
        }
    }
    if args.tokens.is_empty() {
        args.tokens.push(DEFAULT_TOKEN.to_string());
    }
    Ok(Some(args))
}

/// State shared by every connection task.
struct Shared {
    hub: Mutex<SyncHub<MemoryAuth>>,
    /// Live sockets, by connection id, addressed through their writer task.
    peers: Mutex<HashMap<ConnId, UnboundedSender<Message>>>,
}

/// Lock helper that ignores poisoning: a panicking connection task must not take
/// the whole room down with it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Shared {
    /// Route hub output to the sockets it is addressed to.
    fn dispatch(&self, outbound: Vec<Outbound>) {
        if outbound.is_empty() {
            return;
        }
        let peers = lock(&self.peers);
        for out in outbound {
            let Some(tx) = peers.get(&out.to) else {
                continue;
            };
            match serde_json::to_string(&out.message) {
                // A closed receiver just means the peer is already gone.
                Ok(json) => {
                    let _ = tx.send(Message::Text(json));
                }
                Err(err) => eprintln!("failed to encode server message: {err}"),
            }
        }
    }

    /// Send one message straight to a single connection (used for rejections).
    fn send_to(&self, conn: ConnId, message: ServerMessage) {
        self.dispatch(vec![Outbound { to: conn, message }]);
    }

    /// Feed one inbound text frame through the hub.
    fn on_text(&self, conn: ConnId, text: &str) {
        let msg: ClientMessage = match serde_json::from_str(text) {
            Ok(msg) => msg,
            Err(err) => {
                self.send_to(
                    conn,
                    ServerMessage::Rejected {
                        reason: format!("malformed message: {err}"),
                    },
                );
                return;
            }
        };
        let outbound = lock(&self.hub).handle(conn, msg);
        self.dispatch(outbound);
    }
}

/// Serve one WebSocket connection until the socket closes.
async fn serve_conn(
    shared: Arc<Shared>,
    stream: TcpStream,
    peer: SocketAddr,
    conn: ConnId,
) -> Result<()> {
    let ws = tokio_tungstenite::accept_async(stream)
        .await
        .with_context(|| format!("websocket handshake with {peer} failed"))?;
    let (mut sink, mut incoming) = ws.split();

    // A per-connection queue keeps the hub lock synchronous: broadcasting never
    // has to await a slow socket.
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    lock(&shared.peers).insert(conn, tx);
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    println!("connection {conn} open ({peer})");
    while let Some(frame) = incoming.next().await {
        match frame {
            Ok(Message::Text(text)) => shared.on_text(conn, &text),
            // Tolerate clients that send JSON as binary frames.
            Ok(Message::Binary(bytes)) => match std::str::from_utf8(&bytes) {
                Ok(text) => shared.on_text(conn, text),
                Err(_) => shared.send_to(
                    conn,
                    ServerMessage::Rejected {
                        reason: "binary frames must be UTF-8 JSON".to_string(),
                    },
                ),
            },
            Ok(Message::Close(_)) => break,
            // Ping/Pong/Frame are handled by tungstenite itself.
            Ok(_) => {}
            Err(err) => {
                eprintln!("connection {conn} read error: {err}");
                break;
            }
        }
    }

    // Tell the room the peer is gone, then let the writer task drain and exit.
    let outbound = lock(&shared.hub).disconnect(conn);
    shared.dispatch(outbound);
    lock(&shared.peers).remove(&conn);
    let _ = writer.await;
    println!("connection {conn} closed ({peer})");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(args) = parse_args(std::env::args().skip(1))? else {
        print!("{USAGE}");
        return Ok(());
    };

    // MemoryAuth is the single-node authenticator from the library: every
    // accepted token maps to a subject of the same name, and members without an
    // explicit grant are editors.
    let mut auth = MemoryAuth::new();
    for token in &args.tokens {
        auth.add_token(token, token);
    }
    auth.set_default_level(Some(AccessLevel::Editor));

    let shared = Arc::new(Shared {
        hub: Mutex::new(SyncHub::new(auth)),
        peers: Mutex::new(HashMap::new()),
    });

    let addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    println!("tpt-vertex collab sync server listening on ws://{addr}");
    println!("accepted join tokens: {}", args.tokens.join(", "));

    let next_conn = AtomicU64::new(1);
    loop {
        let (stream, peer) = listener.accept().await.context("accept failed")?;
        let conn = next_conn.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            if let Err(err) = serve_conn(shared, stream, peer, conn).await {
                eprintln!("connection {conn} ({peer}) ended: {err:#}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Args {
        parse_args(items.iter().map(|s| s.to_string()))
            .expect("args parse")
            .expect("not help")
    }

    #[test]
    fn defaults_apply_without_arguments() {
        let a = args(&[]);
        assert_eq!(a.host, DEFAULT_HOST);
        assert_eq!(a.port, DEFAULT_PORT);
        assert_eq!(a.tokens, vec![DEFAULT_TOKEN.to_string()]);
    }

    #[test]
    fn parses_space_and_equals_forms() {
        let a = args(&[
            "--port",
            "9000",
            "--host=0.0.0.0",
            "--token",
            "t1",
            "--token=t2",
        ]);
        assert_eq!(a.port, 9000);
        assert_eq!(a.host, "0.0.0.0");
        assert_eq!(a.tokens, vec!["t1".to_string(), "t2".to_string()]);
    }

    #[test]
    fn help_returns_none_and_bad_port_errors() {
        assert!(parse_args(["--help".to_string()]).unwrap().is_none());
        assert!(parse_args(["--port".to_string(), "nope".to_string()]).is_err());
    }
}
