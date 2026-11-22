use std::env;

use protocol::{Command, LookPayload, ServerEvent, ServerMessage, ServerReply};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

struct CliConfig {
    host: String,
    port: u16,
}

impl CliConfig {
    fn from_args() -> Self {
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

        Self { host, port }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CliConfig::from_args();
    let addr = format!("{}:{}", config.host, config.port);

    println!("Connecting to TAP server at {addr}...");
    let stream = TcpStream::connect(&addr).await?;
    let (read_half, mut write_half) = stream.into_split();

    let mut server_reader = BufReader::new(read_half);
    let mut stdin_reader = BufReader::new(io::stdin());

    let mut server_line = String::new();
    let mut input_line = String::new();

    println!("Connected! Type 'CONNECT <username>' to enter Arrakis, or 'HELP' for commands.");

    loop {
        tokio::select! {
            res = server_reader.read_line(&mut server_line) => {
                match res {
                    Ok(0) => {
                        println!("\n[Disconnected by server]");
                        break;
                    }
                    Ok(_) => {
                        display_server_line(&server_line);
                        server_line.clear();
                    }
                    Err(err) => {
                        eprintln!("\n[Network read error: {err}]");
                        break;
                    }
                }
            }
            res = stdin_reader.read_line(&mut input_line) => {
                match res {
                    Ok(0) => {
                        let _ = write_half.write_all(Command::Quit.to_wire_line().as_bytes()).await;
                        break;
                    }
                    Ok(_) => {
                        let trimmed = input_line.trim();
                        if !trimmed.is_empty() {
                            if trimmed.eq_ignore_ascii_case("HELP") {
                                print_help();
                            } else {
                                match trimmed.parse::<Command>() {
                                    Ok(cmd) => {
                                        let is_quit = matches!(cmd, Command::Quit);
                                        let wire = cmd.to_wire_line();
                                        if let Err(err) = write_half.write_all(wire.as_bytes()).await {
                                            eprintln!("[Send failed: {err}]");
                                            break;
                                        }
                                        if is_quit {
                                            // Wait briefly for server's `OK bye` reply
                                        }
                                    }
                                    Err(err) => {
                                        println!("[Invalid syntax: {err}] Type HELP for RFC 42TAP commands.");
                                    }
                                }
                            }
                        }
                        input_line.clear();
                    }
                    Err(err) => {
                        eprintln!("[Input error: {err}]");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn display_server_line(raw: &str) {
    match raw.parse::<ServerMessage>() {
        Ok(ServerMessage::Reply(reply)) => format_reply(&reply),
        Ok(ServerMessage::Event(event)) => format_event(&event),
        Err(_) => println!("<< {}", raw.trim_end()),
    }
}

fn format_reply(reply: &ServerReply) {
    match reply {
        ServerReply::Ok(Some(payload)) => {
            if let Ok(look) = serde_json::from_str::<LookPayload>(payload) {
                println!("\n=== {} ({}) ===", look.room.name, look.room.id);
                println!("{}", look.room.description);
                let exits: Vec<String> = look
                    .room
                    .exits
                    .iter()
                    .map(|(dir, dst)| format!("{dir} -> {dst}"))
                    .collect();
                println!(
                    "Exits:   {}",
                    if exits.is_empty() {
                        "none".to_string()
                    } else {
                        exits.join(", ")
                    }
                );
                println!(
                    "Players: {}",
                    if look.players.is_empty() {
                        "none".to_string()
                    } else {
                        look.players.join(", ")
                    }
                );
                println!(
                    "Items:   {}",
                    if look.items.is_empty() {
                        "none".to_string()
                    } else {
                        look.items.join(", ")
                    }
                );
                println!(
                    "NPCs:    {}\n",
                    if look.npcs.is_empty() {
                        "none".to_string()
                    } else {
                        look.npcs.join(", ")
                    }
                );
            } else {
                println!("OK {payload}");
            }
        }
        ServerReply::Ok(None) => println!("OK"),
        ServerReply::Err { code, message } => println!("ERR {code} {message}"),
    }
}

fn format_event(event: &ServerEvent) {
    match event {
        ServerEvent::RoomPresenceEnter { username } => {
            println!("[Room] {username} entered the area.");
        }
        ServerEvent::RoomPresenceLeave { username } => {
            println!("[Room] {username} left the area.");
        }
        ServerEvent::RoomChat { username, message } => {
            println!("[Room Chat] {username}: {message}");
        }
        ServerEvent::RoomItemTaken { username, item_id } => {
            println!("[Room] {username} picked up {item_id}.");
        }
        ServerEvent::RoomItemDropped { username, item_id } => {
            println!("[Room] {username} dropped {item_id}.");
        }
        ServerEvent::GlobalChat { username, message } => {
            println!("[Global] {username}: {message}");
        }
        ServerEvent::GroupInvite { leader } => {
            println!("[Group] {leader} invited you to a group (use: GROUP JOIN {leader}).");
        }
        ServerEvent::GroupJoin { username } => {
            println!("[Group] {username} joined the group.");
        }
        ServerEvent::GroupLeave { username } => {
            println!("[Group] {username} left the group.");
        }
        ServerEvent::GroupChat { username, message } => {
            println!("[Group Chat] {username}: {message}");
        }
        ServerEvent::StatsPlayers { count } => {
            println!("[Server] Online players: {count}");
        }
        ServerEvent::CombatStart { attacker, target } => {
            println!("[Combat] {attacker} engaged {target}!");
        }
        ServerEvent::CombatHit {
            attacker,
            target,
            damage,
            target_hp,
        } => {
            println!("[Combat] {attacker} hit {target} for {damage} dmg (HP left: {target_hp}).");
        }
        ServerEvent::CombatDefend { username } => {
            println!("[Combat] {username} takes a defensive stance (+5 HP, half damage).");
        }
        ServerEvent::CombatFlee { username } => {
            println!("[Combat] {username} disengaged from combat.");
        }
        ServerEvent::CombatDefeat { victim, by } => {
            println!("[Combat] {victim} was defeated by {by}!");
        }
        ServerEvent::CombatRespawn { username, room } => {
            println!("[Combat] {username} respawned at {room}.");
        }
        ServerEvent::QuestAccepted { quest_id } => {
            println!("[Quest] Accepted: {quest_id}");
        }
        ServerEvent::QuestProgress { quest_id, progress } => {
            println!("[Quest] {quest_id} progress: {progress}");
        }
        ServerEvent::QuestComplete { quest_id, reward } => {
            println!("[Quest] Completed {quest_id}! Reward added: {reward}");
        }
    }
}

fn print_help() {
    println!(
        "\nRFC 42TAP Commands:\n\
         - CONNECT <username>          Authenticate on the server\n\
         - LOOK                        Inspect current room, exits, items, NPCs\n\
         - MOVE <direction>            Move (north, south, east, west)\n\
         - CHAT <GLOBAL|ROOM|GROUP> <msg>\n\
         - TAKE <item> / DROP <item>   Pick up or drop items (ID or display name)\n\
         - INVENTORY                   List carried items\n\
         - TALK <npc>                  Speak with an NPC\n\
         - ATTACK <npc> | DEFEND | FLEE | STATUS\n\
         - QUEST <npc> | QUESTS        Accept and track quests\n\
         - WHO                         Show online player count\n\
         - GROUP CREATE | INVITE <user> | JOIN <leader> | LEAVE\n\
         - QUIT                        Disconnect\n"
    );
}
