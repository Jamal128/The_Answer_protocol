mod avatars;
mod net;
mod radar;
mod theme;
mod ui;

use std::env;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use protocol::{
    AttackPayload, Command, LookPayload, QuestOfferPayload, QuestProgressPayload, ServerEvent,
    ServerMessage, ServerReply, StatusPayload,
};

use crate::net::{spawn_network_thread, NetCommand, NetEvent};
use crate::radar::WorldRadar;
use crate::theme::apply_modern_mud_theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveChatTab {
    Global,
    Room,
    Group,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Combat,
    Quest,
    Room,
    Error,
}

pub struct LogEntry {
    pub kind: LogKind,
    pub text: String,
}

pub struct TapGuiApp {
    pub host: String,
    pub port: String,
    pub username: String,
    pub connected: bool,
    pub status_banner: String,

    pub net_tx: Sender<NetCommand>,
    pub net_rx: Receiver<NetEvent>,

    pub current_room: Option<LookPayload>,
    pub radar: WorldRadar,
    pub inventory: Vec<String>,
    pub player_status: StatusPayload,
    pub quests: Vec<QuestProgressPayload>,
    pub server_player_count: usize,
    pub last_npc_dialogue: String,
    pub group_id: Option<String>,

    pub item_input: String,
    pub npc_input: String,
    pub group_target_input: String,
    pub chat_input: String,
    pub active_chat_tab: ActiveChatTab,

    pub global_chat: Vec<(String, String)>,
    pub room_chat: Vec<(String, String)>,
    pub group_chat: Vec<(String, String)>,
    pub system_logs: Vec<LogEntry>,
}

impl TapGuiApp {
    fn new(cc: &eframe::CreationContext<'_>, default_host: String, default_port: u16) -> Self {
        apply_modern_mud_theme(&cc.egui_ctx);

        let (net_tx, cmd_rx) = mpsc::channel::<NetCommand>();
        let (evt_tx, net_rx) = mpsc::channel::<NetEvent>();
        spawn_network_thread(cmd_rx, evt_tx, cc.egui_ctx.clone());

        Self {
            host: default_host,
            port: default_port.to_string(),
            username: "Paul".to_string(),
            connected: false,
            status_banner: "OFFLINE".to_string(),
            net_tx,
            net_rx,
            current_room: None,
            radar: WorldRadar::default(),
            inventory: Vec::new(),
            player_status: StatusPayload {
                hp: 100,
                max_hp: 100,
                status: "healthy".to_string(),
            },
            quests: Vec::new(),
            server_player_count: 0,
            last_npc_dialogue: "Select a character in the sector and click TALK or QUEST."
                .to_string(),
            group_id: None,
            item_input: String::new(),
            npc_input: String::new(),
            group_target_input: String::new(),
            chat_input: String::new(),
            active_chat_tab: ActiveChatTab::Global,
            global_chat: Vec::new(),
            room_chat: Vec::new(),
            group_chat: Vec::new(),
            system_logs: vec![LogEntry {
                kind: LogKind::Info,
                text: ">> TAP: ARRAKIS TERMINAL READY (RFC 42TAP v1)".to_string(),
            }],
        }
    }

    pub fn send_cmd(&mut self, cmd: Command) {
        if self.connected {
            let _ = self.net_tx.send(NetCommand::Send(cmd));
        }
    }

    pub fn push_log(&mut self, kind: LogKind, line: impl Into<String>) {
        self.system_logs.push(LogEntry {
            kind,
            text: line.into(),
        });
        if self.system_logs.len() > 250 {
            self.system_logs.remove(0);
        }
    }

    fn poll_network(&mut self) {
        while let Ok(evt) = self.net_rx.try_recv() {
            match evt {
                NetEvent::Connected => {
                    self.connected = true;
                    self.radar.reset();
                    self.status_banner = format!("ONLINE // {}", self.username.to_uppercase());
                    self.push_log(
                        LogKind::Info,
                        format!(">> Uplink established with {}:{}", self.host, self.port),
                    );
                    self.send_cmd(Command::Look);
                    self.send_cmd(Command::Inventory);
                    self.send_cmd(Command::Status);
                    self.send_cmd(Command::Who);
                    self.send_cmd(Command::Quests);
                }
                NetEvent::Disconnected(reason) => {
                    self.connected = false;
                    self.status_banner = format!("OFFLINE ({reason})");
                    self.current_room = None;
                    self.group_id = None;
                    self.push_log(LogKind::Error, format!("!! Disconnected: {reason}"));
                }
                NetEvent::Message(ServerMessage::Reply(reply)) => self.handle_reply(reply),
                NetEvent::Message(ServerMessage::Event(event)) => self.handle_event(event),
            }
        }
    }

    fn handle_reply(&mut self, reply: ServerReply) {
        match reply {
            ServerReply::Ok(None) => self.push_log(LogKind::Info, "OK"),
            ServerReply::Ok(Some(payload)) => {
                if let Ok(look) = serde_json::from_str::<LookPayload>(&payload) {
                    self.server_player_count = self.server_player_count.max(look.players.len());
                    self.radar.observe_room(&look);
                    self.current_room = Some(look);
                    return;
                }
                if let Ok(inv) = serde_json::from_str::<Vec<String>>(&payload) {
                    self.inventory = inv;
                    return;
                }
                if let Ok(st) = serde_json::from_str::<StatusPayload>(&payload) {
                    self.player_status = st;
                    return;
                }
                if let Ok(atk) = serde_json::from_str::<AttackPayload>(&payload) {
                    self.player_status.hp = atk.attacker_hp;
                    self.player_status.status = atk.status.clone();
                    self.push_log(
                        LogKind::Combat,
                        format!(
                            "[COMBAT] Dealt {} DMG | Your HP: {} | Target HP: {} [{}]",
                            atk.damage,
                            atk.attacker_hp,
                            atk.target_hp,
                            atk.status.to_uppercase()
                        ),
                    );
                    self.send_cmd(Command::Quests);
                    if atk.status == "defeated" {
                        self.send_cmd(Command::Look);
                    }
                    return;
                }
                if let Ok(offer) = serde_json::from_str::<QuestOfferPayload>(&payload) {
                    self.last_npc_dialogue = format!(
                        "[QUEST: {}] {} — Reward: {}",
                        offer.quest_id, offer.description, offer.reward
                    );
                    self.push_log(
                        LogKind::Quest,
                        format!("[QUEST ACCEPTED] {}", offer.quest_id),
                    );
                    self.send_cmd(Command::Quests);
                    return;
                }
                if let Ok(qlist) = serde_json::from_str::<Vec<QuestProgressPayload>>(&payload) {
                    self.quests = qlist;
                    return;
                }

                if let Some(count_str) = payload.strip_prefix("players=") {
                    if let Ok(count) = count_str.parse::<usize>() {
                        self.server_player_count = count;
                    }
                    self.push_log(
                        LogKind::Info,
                        format!("[SERVER] Online players: {count_str}"),
                    );
                } else if payload.starts_with("room=") {
                    self.push_log(LogKind::Room, format!("[MOVE] Entered {payload}"));
                    self.send_cmd(Command::Look);
                    self.send_cmd(Command::Status);
                } else if payload.starts_with("taken=") {
                    self.push_log(LogKind::Info, format!("[LOOT] Acquired {payload}"));
                    self.send_cmd(Command::Look);
                    self.send_cmd(Command::Inventory);
                    self.send_cmd(Command::Quests);
                } else if payload.starts_with("dropped=") {
                    self.push_log(LogKind::Info, format!("[DROP] Released {payload}"));
                    self.send_cmd(Command::Look);
                    self.send_cmd(Command::Inventory);
                } else if let Some(gid) = payload.strip_prefix("group=") {
                    self.group_id = Some(gid.to_string());
                    self.push_log(LogKind::Info, format!("[GROUP] Active squad: {gid}"));
                } else if payload == "connected"
                    || payload.starts_with("hello ")
                    || payload == "bye"
                {
                    self.push_log(LogKind::Info, format!("[SYS] {payload}"));
                } else {
                    self.last_npc_dialogue = payload.clone();
                    self.push_log(LogKind::Room, format!("[NPC] \"{payload}\""));
                }
            }
            ServerReply::Err { code, message } => {
                self.push_log(LogKind::Error, format!("[ERR {code}] {message}"));
            }
        }
    }

    fn handle_event(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::RoomPresenceEnter { username } => {
                self.push_log(LogKind::Room, format!("[SECTOR] {username} arrived."));
                self.send_cmd(Command::Look);
            }
            ServerEvent::RoomPresenceLeave { username } => {
                self.push_log(LogKind::Room, format!("[SECTOR] {username} departed."));
                self.send_cmd(Command::Look);
            }
            ServerEvent::RoomChat { username, message } => {
                self.room_chat.push((username, message));
            }
            ServerEvent::RoomItemTaken { username, item_id } => {
                self.push_log(
                    LogKind::Room,
                    format!("[SECTOR] {username} picked up {item_id}."),
                );
                self.send_cmd(Command::Look);
            }
            ServerEvent::RoomItemDropped { username, item_id } => {
                self.push_log(
                    LogKind::Room,
                    format!("[SECTOR] {username} dropped {item_id}."),
                );
                self.send_cmd(Command::Look);
            }
            ServerEvent::GlobalChat { username, message } => {
                self.global_chat.push((username, message));
            }
            ServerEvent::GroupInvite { leader } => {
                self.group_target_input = leader.clone();
                self.group_chat.push((
                    "SYSTEM".to_string(),
                    format!("{leader} invited you to a squad! Click JOIN to accept."),
                ));
                self.push_log(LogKind::Info, format!("[GROUP] Invite from {leader}"));
            }
            ServerEvent::GroupJoin { username } => {
                self.group_chat.push((
                    "SYSTEM".to_string(),
                    format!("{username} joined the squad."),
                ));
            }
            ServerEvent::GroupLeave { username } => {
                self.group_chat
                    .push(("SYSTEM".to_string(), format!("{username} left the squad.")));
            }
            ServerEvent::GroupChat { username, message } => {
                self.group_chat.push((username, message));
            }
            ServerEvent::StatsPlayers { count } => {
                self.server_player_count = count;
            }
            ServerEvent::CombatStart { attacker, target } => {
                self.push_log(
                    LogKind::Combat,
                    format!("[COMBAT] {attacker} engaged {target}!"),
                );
            }
            ServerEvent::CombatHit {
                attacker,
                target,
                damage,
                target_hp,
            } => {
                self.push_log(
                    LogKind::Combat,
                    format!("[COMBAT] {attacker} -> {target} (-{damage} HP, left: {target_hp})"),
                );
            }
            ServerEvent::CombatDefend { username } => {
                self.push_log(
                    LogKind::Combat,
                    format!("[COMBAT] {username} raised defensive guard."),
                );
            }
            ServerEvent::CombatFlee { username } => {
                self.push_log(
                    LogKind::Combat,
                    format!("[COMBAT] {username} disengaged from battle."),
                );
            }
            ServerEvent::CombatDefeat { victim, by } => {
                self.push_log(
                    LogKind::Combat,
                    format!("[COMBAT] {victim} was defeated by {by}!"),
                );
            }
            ServerEvent::CombatRespawn { username, room } => {
                self.push_log(
                    LogKind::Combat,
                    format!("[RESPAWN] {username} recovered at {room}."),
                );
                if username == self.username {
                    self.send_cmd(Command::Look);
                    self.send_cmd(Command::Status);
                }
            }
            ServerEvent::QuestAccepted { quest_id } => {
                self.push_log(LogKind::Quest, format!("[QUEST] Accepted {quest_id}"));
                self.send_cmd(Command::Quests);
            }
            ServerEvent::QuestProgress { quest_id, progress } => {
                self.push_log(
                    LogKind::Quest,
                    format!("[QUEST] {quest_id} progress: {progress}"),
                );
                self.send_cmd(Command::Quests);
            }
            ServerEvent::QuestComplete { quest_id, reward } => {
                self.push_log(
                    LogKind::Quest,
                    format!("[QUEST COMPLETE] {quest_id} -> Reward: {reward}"),
                );
                self.send_cmd(Command::Quests);
                self.send_cmd(Command::Inventory);
            }
        }
    }
}

impl eframe::App for TapGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_network();
        ui::render_app(self, ctx);
    }
}

fn main() -> eframe::Result<()> {
    let mut host = "127.0.0.1".to_string();
    let mut port = 4242_u16;

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
            _ => {}
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1340.0, 820.0]),
        ..Default::default()
    };

    eframe::run_native(
        "TAP // Arrakis Modern MUD",
        options,
        Box::new(move |cc| Ok(Box::new(TapGuiApp::new(cc, host, port)))),
    )
}
