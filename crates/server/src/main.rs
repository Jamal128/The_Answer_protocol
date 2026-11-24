mod state;
mod world;

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use protocol::{Command, ErrorKind, ServerMessage, ServerReply, MAX_LINE_LENGTH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use crate::state::GameState;
use crate::world::WorldData;

struct ServerConfig {
    host: String,
    port: u16,
    world_path: String,
}

impl ServerConfig {
    fn from_args() -> Self {
        let mut host = "127.0.0.1".to_string();
        let mut port = 4242_u16;
        let mut world_path = "data/world.yaml".to_string();

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--host" => {
                    if let Some(val) = args.next() {
                        host = val;
                    }
                }
                "--port" => {
                    if let Some(val) = args.next() {
                        if let Ok(parsed) = val.parse() {
                            port = parsed;
                        }
                    }
                }
                "--world" => {
                    if let Some(val) = args.next() {
                        world_path = val;
                    }
                }
                _ => {}
            }
        }

        Self {
            host,
            port,
            world_path,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = ServerConfig::from_args();
    let world = WorldData::load_from_path(&config.world_path)?;
    info!(
        event = "world_loaded",
        rooms = world.rooms.len(),
        items = world.items.len(),
        npcs = world.npcs.len(),
        quests = world.quests.len()
    );

    let state = Arc::new(RwLock::new(GameState::new(world)));
    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&bind_addr).await?;
    info!(event = "server_listening", address = %bind_addr);

    loop {
        let (stream, addr) = listener.accept().await?;
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            handle_client(stream, addr, state_clone).await;
        });
    }
}

async fn handle_client(stream: TcpStream, addr: SocketAddr, state: Arc<RwLock<GameState>>) {
    info!(event = "tcp_connected", %addr);
    let (read_half, mut write_half) = stream.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Outbound writer task: isolates slow or broken sockets from broadcasts
    let writer_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let line = msg.to_wire_line();
            if write_half.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    // Send mandatory RFC 42TAP initial greeting
    if tx.send(ServerMessage::Reply(ServerReply::hello())).is_err() {
        return;
    }

    let mut reader = BufReader::new(read_half);
    let mut raw: Vec<u8> = Vec::with_capacity(MAX_LINE_LENGTH + 2);
    let mut authenticated_user: Option<String> = None;

    loop {
        let line = match read_bounded_line(&mut reader, &mut raw).await {
            Ok(LineRead::Eof) => break,
            Ok(LineRead::Line(l)) => l,
            Ok(LineRead::TooLong) => {
                let reply = ServerReply::from(ErrorKind::LineTooLong);
                warn!(event = "line_too_long", %addr, reply = %reply);
                let _ = tx.send(ServerMessage::Reply(reply));
                continue;
            }
            Ok(LineRead::BadUtf8) => {
                let reply = ServerReply::from(ErrorKind::BadRequest);
                warn!(event = "invalid_utf8", %addr, reply = %reply);
                let _ = tx.send(ServerMessage::Reply(reply));
                continue;
            }
            Err(err) => {
                warn!(event = "socket_read_error", %addr, error = %err);
                break;
            }
        };

        let cmd = match line.parse::<Command>() {
            Ok(c) => c,
            Err(err) => {
                let reply = ServerReply::from(err);
                warn!(
                    event = "malformed_command",
                    %addr,
                    player = authenticated_user.as_deref().unwrap_or("<unauthenticated>"),
                    reply = %reply
                );
                let _ = tx.send(ServerMessage::Reply(reply));
                continue;
            }
        };

        info!(
            event = "command_received",
            %addr,
            player = authenticated_user.as_deref().unwrap_or("<unauthenticated>"),
            command = %cmd
        );

        match (&authenticated_user, cmd) {
            (None, Command::Connect { username }) => {
                let mut guard = state.write().await;
                let reply = guard.connect_player(username.clone(), addr, tx.clone());
                let is_ok = reply.is_ok();
                let _ = tx.send(ServerMessage::Reply(reply));
                if is_ok {
                    authenticated_user = Some(username);
                }
            }
            (None, Command::Quit) => {
                let _ = tx.send(ServerMessage::Reply(ServerReply::bye()));
                break;
            }
            (None, _) => {
                let reply = ServerReply::from(ErrorKind::NotAuthenticated);
                let _ = tx.send(ServerMessage::Reply(reply));
            }
            (Some(username), cmd) => {
                let mut guard = state.write().await;
                let (reply, should_quit) = guard.handle_command(username, cmd);
                info!(event = "reply_sent", %addr, %username, reply = %reply);
                let _ = tx.send(ServerMessage::Reply(reply));
                if should_quit {
                    break;
                }
            }
        }
    }

    if let Some(username) = authenticated_user {
        let mut guard = state.write().await;
        guard.disconnect_player(&username);
    }

    drop(tx);
    if let Err(err) = writer_handle.await {
        error!(event = "writer_task_panic", %addr, error = %err);
    }
}

enum LineRead {
    Eof,
    Line(String),
    TooLong,
    BadUtf8,
}

async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> std::io::Result<LineRead> {
    let limit = (MAX_LINE_LENGTH + 2) as u64;
    buf.clear();
    let n = (&mut *reader).take(limit).read_until(b'\n', buf).await?;
    if n == 0 {
        return Ok(LineRead::Eof);
    }
    if buf.last() != Some(&b'\n') && n as u64 >= limit {
        loop {
            buf.clear();
            let m = (&mut *reader).take(4096).read_until(b'\n', buf).await?;
            if m == 0 || buf.last() == Some(&b'\n') {
                break;
            }
        }
        return Ok(LineRead::TooLong);
    }
    Ok(match String::from_utf8(std::mem::take(buf)) {
        Ok(s) => LineRead::Line(s),
        Err(_) => LineRead::BadUtf8,
    })
}