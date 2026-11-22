use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use eframe::egui;
use protocol::{Command, ServerMessage, ServerReply};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc as tokio_mpsc;

pub enum NetCommand {
    Connect { addr: String, username: String },
    Send(Command),
    Disconnect,
}

pub enum NetEvent {
    Connected,
    Disconnected(String),
    Message(ServerMessage),
}

pub fn spawn_network_thread(
    cmd_rx: Receiver<NetCommand>,
    evt_tx: Sender<NetEvent>,
    egui_ctx: egui::Context,
) {
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to initialize tokio runtime for GUI");
        rt.block_on(network_worker(cmd_rx, evt_tx, egui_ctx));
    });
}

async fn network_worker(
    cmd_rx: Receiver<NetCommand>,
    evt_tx: Sender<NetEvent>,
    egui_ctx: egui::Context,
) {
    let (async_cmd_tx, mut async_cmd_rx) = tokio_mpsc::unbounded_channel::<NetCommand>();

    thread::spawn(move || {
        while let Ok(cmd) = cmd_rx.recv() {
            if async_cmd_tx.send(cmd).is_err() {
                break;
            }
        }
    });

    while let Some(cmd) = async_cmd_rx.recv().await {
        let NetCommand::Connect { addr, username } = cmd else {
            continue;
        };

        let stream = match TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(err) => {
                let _ = evt_tx.send(NetEvent::Disconnected(err.to_string()));
                egui_ctx.request_repaint();
                continue;
            }
        };

        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();

        // Read initial `OK hello proto=1`
        if reader.read_line(&mut line).await.is_err() {
            let _ = evt_tx.send(NetEvent::Disconnected("failed reading greeting".into()));
            egui_ctx.request_repaint();
            continue;
        }
        if let Ok(msg) = line.parse::<ServerMessage>() {
            let _ = evt_tx.send(NetEvent::Message(msg));
        }
        line.clear();

        // Authenticate with `CONNECT <username>`
        let connect_wire = Command::Connect { username }.to_wire_line();
        if write_half.write_all(connect_wire.as_bytes()).await.is_err() {
            let _ = evt_tx.send(NetEvent::Disconnected("failed sending CONNECT".into()));
            egui_ctx.request_repaint();
            continue;
        }

        // Wait for OK/ERR while forwarding any interleaved async EVT frames
        let mut auth_succeeded = false;
        loop {
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => {
                    let _ = evt_tx.send(NetEvent::Disconnected("closed during auth".into()));
                    egui_ctx.request_repaint();
                    break;
                }
                Ok(_) => {
                    if let Ok(msg) = line.parse::<ServerMessage>() {
                        match &msg {
                            ServerMessage::Event(_) => {
                                let _ = evt_tx.send(NetEvent::Message(msg));
                                line.clear();
                                continue;
                            }
                            ServerMessage::Reply(ServerReply::Ok(_)) => {
                                let _ = evt_tx.send(NetEvent::Message(msg));
                                auth_succeeded = true;
                                line.clear();
                                break;
                            }
                            ServerMessage::Reply(ServerReply::Err { .. }) => {
                                let _ = evt_tx.send(NetEvent::Message(msg));
                                let _ = evt_tx
                                    .send(NetEvent::Disconnected("authentication rejected".into()));
                                egui_ctx.request_repaint();
                                break;
                            }
                        }
                    }
                    line.clear();
                }
            }
        }

        if !auth_succeeded {
            continue;
        }

        let _ = evt_tx.send(NetEvent::Connected);
        egui_ctx.request_repaint();

        loop {
            tokio::select! {
                res = reader.read_line(&mut line) => {
                    match res {
                        Ok(0) => {
                            let _ = evt_tx.send(NetEvent::Disconnected("server closed connection".into()));
                            egui_ctx.request_repaint();
                            break;
                        }
                        Ok(_) => {
                            if let Ok(msg) = line.parse::<ServerMessage>() {
                                let _ = evt_tx.send(NetEvent::Message(msg));
                                egui_ctx.request_repaint();
                            }
                            line.clear();
                        }
                        Err(err) => {
                            let _ = evt_tx.send(NetEvent::Disconnected(err.to_string()));
                            egui_ctx.request_repaint();
                            break;
                        }
                    }
                }
                Some(next_cmd) = async_cmd_rx.recv() => {
                    match next_cmd {
                        NetCommand::Send(c) => {
                            let wire = c.to_wire_line();
                            if write_half.write_all(wire.as_bytes()).await.is_err() {
                                let _ = evt_tx.send(NetEvent::Disconnected("write failed".into()));
                                egui_ctx.request_repaint();
                                break;
                            }
                        }
                        NetCommand::Disconnect => {
                            let _ = evt_tx.send(NetEvent::Disconnected("user disconnected".into()));
                            egui_ctx.request_repaint();
                            break;
                        }
                        NetCommand::Connect { .. } => {}
                    }
                }
            }
        }
    }
}
