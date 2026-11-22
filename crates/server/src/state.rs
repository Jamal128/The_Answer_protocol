use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::Instant;

use protocol::{
    matches_reference, quest_status, status, AttackPayload, ChatScope, Command, ErrorKind,
    GroupAction, LookPayload, QuestOfferPayload, QuestProgressPayload, RoomInfo, ServerEvent,
    ServerMessage, ServerReply, StatusPayload,
};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::world::{NpcRole, QuestObjectiveType, WorldData};

pub const PLAYER_MAX_HP: i32 = 100;
pub const PLAYER_RESPAWN_HP: i32 = 50;
pub const PLAYER_BASE_DAMAGE: i32 = 10;

pub type ClientTx = mpsc::UnboundedSender<ServerMessage>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestState {
    Active { progress: usize },
    Completed,
}

#[derive(Debug)]
pub struct PlayerSession {
    pub username: String,
    pub addr: SocketAddr,
    pub tx: ClientTx,
    pub room_id: String,
    pub hp: i32,
    pub max_hp: i32,
    pub inventory: Vec<String>,
    pub in_combat_with: Option<String>,
    pub defending: bool,
    pub group_id: Option<String>,
    pub pending_invite_from: Option<String>,
    pub quests: HashMap<String, QuestState>,
    pub cmd_window_start: Instant,
    pub cmd_count_in_window: u32,
}

#[derive(Debug)]
pub struct GroupState {
    pub id: String,
    pub leader: String,
    pub members: HashSet<String>,
}

#[derive(Debug)]
pub struct GameState {
    pub world: WorldData,
    /// Dynamic items currently lying on the floor of each room.
    pub room_items: HashMap<String, Vec<String>>,
    /// Dynamic current HP for each NPC instance in the world.
    pub npc_hp: HashMap<String, i32>,
    /// Connected authenticated players by username.
    pub players: HashMap<String, PlayerSession>,
    /// Active groups by group_id.
    pub groups: HashMap<String, GroupState>,
    pub next_group_seq: usize,
}

impl GameState {
    #[must_use]
    pub fn new(world: WorldData) -> Self {
        let mut room_items = HashMap::new();
        for (room_id, room) in &world.rooms {
            room_items.insert(room_id.clone(), room.items.clone());
        }

        let mut npc_hp = HashMap::new();
        for (npc_id, npc) in &world.npcs {
            npc_hp.insert(npc_id.clone(), npc.hp);
        }

        Self {
            world,
            room_items,
            npc_hp,
            players: HashMap::new(),
            groups: HashMap::new(),
            next_group_seq: 1,
        }
    }

    /// Registers an authenticated player and broadcasts presence and player count.
    pub fn connect_player(
        &mut self,
        username: String,
        addr: SocketAddr,
        tx: ClientTx,
    ) -> ServerReply {
        if self.players.contains_key(&username) {
            warn!(event = "auth_failed", %username, %addr, reason = "NAME_IN_USE");
            return ServerReply::from(ErrorKind::NameInUse);
        }

        let spawn_room = self.world.spawn_room.clone();
        let session = PlayerSession {
            username: username.clone(),
            addr,
            tx,
            room_id: spawn_room.clone(),
            hp: PLAYER_MAX_HP,
            max_hp: PLAYER_MAX_HP,
            inventory: Vec::new(),
            in_combat_with: None,
            defending: false,
            group_id: None,
            pending_invite_from: None,
            quests: HashMap::new(),
            cmd_window_start: Instant::now(),
            cmd_count_in_window: 0,
        };

        self.players.insert(username.clone(), session);
        info!(event = "player_connected", %username, %addr, room = %spawn_room);

        self.broadcast_to_room(
            &spawn_room,
            Some(&username),
            ServerEvent::RoomPresenceEnter {
                username: username.clone(),
            },
        );
        self.broadcast_global(
            Some(&username),
            ServerEvent::StatsPlayers {
                count: self.players.len(),
            },
        );

        ServerReply::connected()
    }

    /// Removes player state BEFORE broadcasting leave events (Chapter IV requirement).
    pub fn disconnect_player(&mut self, username: &str) {
        let Some(removed) = self.players.remove(username) else {
            return;
        };

        info!(
            event = "player_disconnected",
            %username,
            addr = %removed.addr,
            room = %removed.room_id
        );

        if let Some(group_id) = &removed.group_id {
            self.remove_from_group(username, group_id);
        }

        self.broadcast_to_room(
            &removed.room_id,
            None,
            ServerEvent::RoomPresenceLeave {
                username: username.to_string(),
            },
        );
        self.broadcast_global(
            None,
            ServerEvent::StatsPlayers {
                count: self.players.len(),
            },
        );
    }

    /// Checks command flooding rate limits (max 20 commands per 2 seconds).
    pub fn check_rate_limit(&mut self, username: &str) -> bool {
        let Some(player) = self.players.get_mut(username) else {
            return true;
        };
        let now = Instant::now();
        if now.duration_since(player.cmd_window_start).as_secs() >= 2 {
            player.cmd_window_start = now;
            player.cmd_count_in_window = 1;
            true
        } else {
            player.cmd_count_in_window += 1;
            if player.cmd_count_in_window > 20 {
                warn!(
                    event = "abuse_flooding",
                    %username,
                    addr = %player.addr,
                    commands_in_2s = player.cmd_count_in_window
                );
                false
            } else {
                true
            }
        }
    }

    pub fn handle_command(&mut self, username: &str, cmd: Command) -> (ServerReply, bool) {
        if !self.check_rate_limit(username) {
            return (ServerReply::from(ErrorKind::RateLimited), false);
        }

        match cmd {
            Command::Connect { .. } => (ServerReply::from(ErrorKind::AlreadyConnected), false),
            Command::Quit => (ServerReply::bye(), true),
            Command::Look => (self.cmd_look(username), false),
            Command::Move { direction } => (self.cmd_move(username, &direction), false),
            Command::Chat { scope, message } => (self.cmd_chat(username, scope, message), false),
            Command::Who => (ServerReply::who(self.players.len()), false),
            Command::Take { item_ref } => (self.cmd_take(username, &item_ref), false),
            Command::Drop { item_ref } => (self.cmd_drop(username, &item_ref), false),
            Command::Inventory => (self.cmd_inventory(username), false),
            Command::Talk { npc_ref } => (self.cmd_talk(username, &npc_ref), false),
            Command::Attack { target } => (self.cmd_attack(username, &target), false),
            Command::Defend => (self.cmd_defend(username), false),
            Command::Flee => (self.cmd_flee(username), false),
            Command::Status => (self.cmd_status(username), false),
            Command::Quest { npc_ref } => (self.cmd_quest(username, &npc_ref), false),
            Command::Quests => (self.cmd_quests(username), false),
            Command::Group(action) => (self.cmd_group(username, action), false),
        }
    }

    fn cmd_look(&self, username: &str) -> ServerReply {
        let Some(player) = self.players.get(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        let Some(room_def) = self.world.rooms.get(&player.room_id) else {
            return ServerReply::from(ErrorKind::InternalError);
        };

        let mut players_in_room: Vec<String> = self
            .players
            .values()
            .filter(|p| p.room_id == player.room_id)
            .map(|p| p.username.clone())
            .collect();
        players_in_room.sort();

        let items = self
            .room_items
            .get(&player.room_id)
            .cloned()
            .unwrap_or_default();

        let payload = LookPayload {
            room: RoomInfo {
                id: player.room_id.clone(),
                name: room_def.name.clone(),
                description: room_def.description.clone(),
                exits: room_def.exits.clone(),
            },
            players: players_in_room,
            items,
            npcs: room_def.npcs.clone(),
        };
        ServerReply::ok_json(&payload)
    }

    fn cmd_move(&mut self, username: &str, direction: &str) -> ServerReply {
        let Some(player) = self.players.get(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        let old_room = player.room_id.clone();
        let Some(room_def) = self.world.rooms.get(&old_room) else {
            return ServerReply::from(ErrorKind::InternalError);
        };
        let Some(next_room) = room_def.exits.get(direction).cloned() else {
            return ServerReply::from(ErrorKind::NoExit);
        };

        if let Some(p) = self.players.get_mut(username) {
            p.room_id = next_room.clone();
            p.in_combat_with = None;
            p.defending = false;
        }

        info!(event = "player_moved", %username, from = %old_room, to = %next_room);

        self.broadcast_to_room(
            &old_room,
            Some(username),
            ServerEvent::RoomPresenceLeave {
                username: username.to_string(),
            },
        );
        self.broadcast_to_room(
            &next_room,
            Some(username),
            ServerEvent::RoomPresenceEnter {
                username: username.to_string(),
            },
        );

        ServerReply::moved(&next_room)
    }

    fn cmd_chat(&self, username: &str, scope: ChatScope, message: String) -> ServerReply {
        let Some(player) = self.players.get(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };

        match scope {
            ChatScope::Global => {
                self.broadcast_global(
                    None,
                    ServerEvent::GlobalChat {
                        username: username.to_string(),
                        message,
                    },
                );
                ServerReply::ok()
            }
            ChatScope::Room => {
                self.broadcast_to_room(
                    &player.room_id,
                    None,
                    ServerEvent::RoomChat {
                        username: username.to_string(),
                        message,
                    },
                );
                ServerReply::ok()
            }
            ChatScope::Group => {
                let Some(group_id) = &player.group_id else {
                    return ServerReply::from(ErrorKind::NotInGroup);
                };
                self.broadcast_to_group(
                    group_id,
                    None,
                    ServerEvent::GroupChat {
                        username: username.to_string(),
                        message,
                    },
                );
                ServerReply::ok()
            }
        }
    }

    fn cmd_take(&mut self, username: &str, item_ref: &str) -> ServerReply {
        let Some(room_id) = self.players.get(username).map(|p| p.room_id.clone()) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };

        let found_idx = self.room_items.get(&room_id).and_then(|items| {
            items.iter().position(|id| {
                self.world
                    .items
                    .get(id)
                    .is_some_and(|def| def.obtainable && matches_reference(item_ref, id, &def.name))
            })
        });

        let Some(idx) = found_idx else {
            return ServerReply::from(ErrorKind::ItemNotFound);
        };

        let item_id = self
            .room_items
            .get_mut(&room_id)
            .expect("room exists")
            .remove(idx);

        if let Some(player) = self.players.get_mut(username) {
            player.inventory.push(item_id.clone());
        }

        info!(event = "item_taken", %username, %room_id, %item_id);

        self.broadcast_to_room(
            &room_id,
            Some(username),
            ServerEvent::RoomItemTaken {
                username: username.to_string(),
                item_id: item_id.clone(),
            },
        );

        self.evaluate_collect_quests(username);

        ServerReply::taken(&item_id)
    }

    fn cmd_drop(&mut self, username: &str, item_ref: &str) -> ServerReply {
        let Some((room_id, inv_idx)) = self.players.get(username).map(|p| {
            let idx = p.inventory.iter().position(|id| {
                self.world
                    .items
                    .get(id)
                    .is_some_and(|def| matches_reference(item_ref, id, &def.name))
            });
            (p.room_id.clone(), idx)
        }) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };

        let Some(idx) = inv_idx else {
            return ServerReply::from(ErrorKind::ItemNotInInventory);
        };

        let item_id = self
            .players
            .get_mut(username)
            .expect("player exists")
            .inventory
            .remove(idx);

        self.room_items
            .entry(room_id.clone())
            .or_default()
            .push(item_id.clone());

        info!(event = "item_dropped", %username, %room_id, %item_id);

        self.broadcast_to_room(
            &room_id,
            Some(username),
            ServerEvent::RoomItemDropped {
                username: username.to_string(),
                item_id: item_id.clone(),
            },
        );

        ServerReply::dropped(&item_id)
    }

    fn cmd_inventory(&self, username: &str) -> ServerReply {
        let Some(player) = self.players.get(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        ServerReply::ok_json(&player.inventory)
    }

    fn find_npc_in_room(&self, room_id: &str, npc_ref: &str) -> Option<String> {
        let room = self.world.rooms.get(room_id)?;
        room.npcs
            .iter()
            .find(|npc_id| {
                self.world
                    .npcs
                    .get(*npc_id)
                    .is_some_and(|def| matches_reference(npc_ref, npc_id, &def.name))
            })
            .cloned()
    }

    fn cmd_talk(&self, username: &str, npc_ref: &str) -> ServerReply {
        let Some(player) = self.players.get(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        let Some(npc_id) = self.find_npc_in_room(&player.room_id, npc_ref) else {
            return ServerReply::from(ErrorKind::NpcNotFound);
        };
        let Some(npc) = self.world.npcs.get(&npc_id) else {
            return ServerReply::from(ErrorKind::InternalError);
        };

        let line = npc
            .dialogue
            .first()
            .cloned()
            .unwrap_or_else(|| format!("{} nods silently.", npc.name));

        info!(event = "npc_talk", %username, %npc_id);
        ServerReply::ok_with(line)
    }

    fn player_attack_power(&self, username: &str) -> i32 {
        let Some(player) = self.players.get(username) else {
            return PLAYER_BASE_DAMAGE;
        };
        let bonus: i32 = player
            .inventory
            .iter()
            .filter_map(|id| self.world.items.get(id))
            .map(|item| item.attack_bonus)
            .sum();
        PLAYER_BASE_DAMAGE + bonus
    }

    fn cmd_attack(&mut self, username: &str, target_ref: &str) -> ServerReply {
        let Some(room_id) = self.players.get(username).map(|p| p.room_id.clone()) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        let Some(npc_id) = self.find_npc_in_room(&room_id, target_ref) else {
            return ServerReply::from(ErrorKind::NpcNotFound);
        };
        let Some(npc_def) = self.world.npcs.get(&npc_id).cloned() else {
            return ServerReply::from(ErrorKind::InternalError);
        };

        if npc_def.role != NpcRole::Enemy {
            return ServerReply::from(ErrorKind::NpcNotHostile);
        }

        let starting_combat = self
            .players
            .get(username)
            .and_then(|p| p.in_combat_with.as_ref())
            .is_none();

        if starting_combat {
            self.broadcast_to_room(
                &room_id,
                None,
                ServerEvent::CombatStart {
                    attacker: username.to_string(),
                    target: npc_id.clone(),
                },
            );
        }

        let damage = self.player_attack_power(username);
        let current_npc_hp = *self.npc_hp.get(&npc_id).unwrap_or(&npc_def.hp);
        let new_npc_hp = (current_npc_hp - damage).max(0);
        self.npc_hp.insert(npc_id.clone(), new_npc_hp);

        self.broadcast_to_room(
            &room_id,
            None,
            ServerEvent::CombatHit {
                attacker: username.to_string(),
                target: npc_id.clone(),
                damage,
                target_hp: new_npc_hp,
            },
        );

        // Enemy defeated
        if new_npc_hp == 0 {
            // Reset enemy HP for subsequent encounters in the persistent world
            self.npc_hp.insert(npc_id.clone(), npc_def.hp);
            let player_hp = if let Some(p) = self.players.get_mut(username) {
                p.in_combat_with = None;
                p.defending = false;
                p.hp
            } else {
                PLAYER_MAX_HP
            };

            info!(event = "combat_victory", %username, %npc_id, %damage);
            self.broadcast_to_room(
                &room_id,
                None,
                ServerEvent::CombatDefeat {
                    victim: npc_id.clone(),
                    by: username.to_string(),
                },
            );

            self.evaluate_defeat_quests(username, &npc_id);

            return ServerReply::ok_json(&AttackPayload {
                attacker_hp: player_hp,
                target_hp: 0,
                damage,
                status: status::VICTORY.to_string(),
            });
        }

        // Enemy counterattack
        let was_defending = self.players.get(username).is_some_and(|p| p.defending);
        let counter_damage = if was_defending {
            (npc_def.damage / 2).max(1)
        } else {
            npc_def.damage
        };

        let new_player_hp = if let Some(p) = self.players.get_mut(username) {
            p.hp = (p.hp - counter_damage).max(0);
            p.in_combat_with = Some(npc_id.clone());
            p.defending = false;
            p.hp
        } else {
            0
        };

        self.broadcast_to_room(
            &room_id,
            None,
            ServerEvent::CombatHit {
                attacker: npc_id.clone(),
                target: username.to_string(),
                damage: counter_damage,
                target_hp: new_player_hp,
            },
        );

        // Player defeated -> respawn at safe room with reduced HP
        if new_player_hp == 0 {
            let respawn_room = self.world.respawn_room.clone();
            if let Some(p) = self.players.get_mut(username) {
                p.hp = PLAYER_RESPAWN_HP;
                p.room_id = respawn_room.clone();
                p.in_combat_with = None;
                p.defending = false;
            }

            info!(event = "combat_defeat_respawn", %username, by = %npc_id, %respawn_room);

            self.broadcast_to_room(
                &room_id,
                None,
                ServerEvent::CombatDefeat {
                    victim: username.to_string(),
                    by: npc_id,
                },
            );
            self.broadcast_to_room(
                &room_id,
                Some(username),
                ServerEvent::RoomPresenceLeave {
                    username: username.to_string(),
                },
            );
            self.broadcast_to_room(
                &respawn_room,
                None,
                ServerEvent::CombatRespawn {
                    username: username.to_string(),
                    room: respawn_room.clone(),
                },
            );
            self.broadcast_to_room(
                &respawn_room,
                Some(username),
                ServerEvent::RoomPresenceEnter {
                    username: username.to_string(),
                },
            );

            return ServerReply::ok_json(&AttackPayload {
                attacker_hp: PLAYER_RESPAWN_HP,
                target_hp: new_npc_hp,
                damage,
                status: status::DEFEATED.to_string(),
            });
        }

        info!(
            event = "combat_round",
            %username,
            %npc_id,
            player_damage = damage,
            npc_damage = counter_damage,
            player_hp = new_player_hp,
            npc_hp = new_npc_hp
        );

        ServerReply::ok_json(&AttackPayload {
            attacker_hp: new_player_hp,
            target_hp: new_npc_hp,
            damage,
            status: status::COMBAT.to_string(),
        })
    }

    fn cmd_defend(&mut self, username: &str) -> ServerReply {
        let Some(player) = self.players.get_mut(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        if player.in_combat_with.is_none() {
            return ServerReply::from(ErrorKind::NotInCombat);
        }

        player.defending = true;
        player.hp = (player.hp + 5).min(player.max_hp);
        let room_id = player.room_id.clone();
        let hp = player.hp;
        let max_hp = player.max_hp;

        self.broadcast_to_room(
            &room_id,
            None,
            ServerEvent::CombatDefend {
                username: username.to_string(),
            },
        );

        ServerReply::ok_json(&StatusPayload {
            hp,
            max_hp,
            status: status::DEFENDING.to_string(),
        })
    }

    fn cmd_flee(&mut self, username: &str) -> ServerReply {
        let Some(player) = self.players.get_mut(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        if player.in_combat_with.is_none() {
            return ServerReply::from(ErrorKind::NotInCombat);
        }

        player.in_combat_with = None;
        player.defending = false;
        let room_id = player.room_id.clone();

        self.broadcast_to_room(
            &room_id,
            None,
            ServerEvent::CombatFlee {
                username: username.to_string(),
            },
        );

        ServerReply::ok()
    }

    fn cmd_status(&self, username: &str) -> ServerReply {
        let Some(player) = self.players.get(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };

        let current_status = if player.defending {
            status::DEFENDING
        } else if player.in_combat_with.is_some() {
            status::COMBAT
        } else if player.hp < player.max_hp {
            status::WOUNDED
        } else {
            status::HEALTHY
        };

        ServerReply::ok_json(&StatusPayload {
            hp: player.hp,
            max_hp: player.max_hp,
            status: current_status.to_string(),
        })
    }

    fn cmd_quest(&mut self, username: &str, npc_ref: &str) -> ServerReply {
        let Some(room_id) = self.players.get(username).map(|p| p.room_id.clone()) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };
        let Some(npc_id) = self.find_npc_in_room(&room_id, npc_ref) else {
            return ServerReply::from(ErrorKind::NpcNotFound);
        };
        let Some(quest_id) = self
            .world
            .npcs
            .get(&npc_id)
            .and_then(|n| n.quest_id.clone())
        else {
            return ServerReply::from(ErrorKind::NoQuestAvailable);
        };
        let Some(quest_def) = self.world.quests.get(&quest_id).cloned() else {
            return ServerReply::from(ErrorKind::InternalError);
        };

        if let Some(player) = self.players.get(username) {
            if player.quests.contains_key(&quest_id) {
                return ServerReply::from(ErrorKind::NoQuestAvailable);
            }
        }

        if let Some(player) = self.players.get_mut(username) {
            player
                .quests
                .insert(quest_id.clone(), QuestState::Active { progress: 0 });
        }

        info!(event = "quest_accepted", %username, %quest_id);
        self.send_to_player(
            username,
            ServerEvent::QuestAccepted {
                quest_id: quest_id.clone(),
            },
        );

        // Immediately check if player already holds required items
        self.evaluate_collect_quests(username);

        ServerReply::ok_json(&QuestOfferPayload {
            quest_id,
            description: quest_def.description,
            reward: quest_def.reward_item,
            status: quest_status::AVAILABLE.to_string(),
        })
    }

    fn cmd_quests(&self, username: &str) -> ServerReply {
        let Some(player) = self.players.get(username) else {
            return ServerReply::from(ErrorKind::NotAuthenticated);
        };

        let mut list: Vec<QuestProgressPayload> = player
            .quests
            .iter()
            .filter_map(|(qid, state)| {
                let def = self.world.quests.get(qid)?;
                match state {
                    QuestState::Active { progress } => Some(QuestProgressPayload {
                        quest_id: qid.clone(),
                        status: quest_status::ACTIVE.to_string(),
                        progress: Some(format!("{progress}/{}", def.required_count)),
                    }),
                    QuestState::Completed => Some(QuestProgressPayload {
                        quest_id: qid.clone(),
                        status: quest_status::COMPLETED.to_string(),
                        progress: None,
                    }),
                }
            })
            .collect();
        list.sort_by(|a, b| a.quest_id.cmp(&b.quest_id));

        ServerReply::ok_json(&list)
    }

    fn evaluate_collect_quests(&mut self, username: &str) {
        let Some(player) = self.players.get(username) else {
            return;
        };
        let active_ids: Vec<String> = player
            .quests
            .iter()
            .filter_map(|(qid, st)| matches!(st, QuestState::Active { .. }).then_some(qid.clone()))
            .collect();

        for qid in active_ids {
            let Some(def) = self.world.quests.get(&qid).cloned() else {
                continue;
            };
            if def.objective_type != QuestObjectiveType::CollectItem {
                continue;
            }
            let owned_count = self.players.get(username).map_or(0, |p| {
                p.inventory.iter().filter(|i| *i == &def.target).count()
            });

            if owned_count >= def.required_count {
                self.complete_quest(username, &qid, &def.reward_item);
            } else if owned_count > 0 {
                if let Some(p) = self.players.get_mut(username) {
                    p.quests.insert(
                        qid.clone(),
                        QuestState::Active {
                            progress: owned_count,
                        },
                    );
                }
                self.send_to_player(
                    username,
                    ServerEvent::QuestProgress {
                        quest_id: qid,
                        progress: format!("{owned_count}/{}", def.required_count),
                    },
                );
            }
        }
    }

    fn evaluate_defeat_quests(&mut self, username: &str, defeated_npc_id: &str) {
        let Some(player) = self.players.get(username) else {
            return;
        };
        let active_ids: Vec<String> = player
            .quests
            .iter()
            .filter_map(|(qid, st)| matches!(st, QuestState::Active { .. }).then_some(qid.clone()))
            .collect();

        for qid in active_ids {
            let Some(def) = self.world.quests.get(&qid).cloned() else {
                continue;
            };
            if def.objective_type != QuestObjectiveType::DefeatNpc || def.target != defeated_npc_id
            {
                continue;
            }
            let next_progress = match self.players.get(username).and_then(|p| p.quests.get(&qid)) {
                Some(QuestState::Active { progress }) => progress + 1,
                _ => 1,
            };

            if next_progress >= def.required_count {
                self.complete_quest(username, &qid, &def.reward_item);
            } else {
                if let Some(p) = self.players.get_mut(username) {
                    p.quests.insert(
                        qid.clone(),
                        QuestState::Active {
                            progress: next_progress,
                        },
                    );
                }
                self.send_to_player(
                    username,
                    ServerEvent::QuestProgress {
                        quest_id: qid,
                        progress: format!("{next_progress}/{}", def.required_count),
                    },
                );
            }
        }
    }

    fn complete_quest(&mut self, username: &str, quest_id: &str, reward_item: &str) {
        if let Some(p) = self.players.get_mut(username) {
            p.quests.insert(quest_id.to_string(), QuestState::Completed);
            p.inventory.push(reward_item.to_string());
        }

        info!(event = "quest_completed", %username, %quest_id, %reward_item);
        self.send_to_player(
            username,
            ServerEvent::QuestComplete {
                quest_id: quest_id.to_string(),
                reward: reward_item.to_string(),
            },
        );
    }

    fn cmd_group(&mut self, username: &str, action: GroupAction) -> ServerReply {
        match action {
            GroupAction::Create => {
                if self
                    .players
                    .get(username)
                    .and_then(|p| p.group_id.as_ref())
                    .is_some()
                {
                    return ServerReply::from(ErrorKind::AlreadyInGroup);
                }
                let group_id = format!("grp.{}", self.next_group_seq);
                self.next_group_seq += 1;

                let mut members = HashSet::new();
                members.insert(username.to_string());

                self.groups.insert(
                    group_id.clone(),
                    GroupState {
                        id: group_id.clone(),
                        leader: username.to_string(),
                        members,
                    },
                );
                if let Some(p) = self.players.get_mut(username) {
                    p.group_id = Some(group_id.clone());
                }
                ServerReply::group(&group_id)
            }
            GroupAction::Invite { username: target } => {
                let Some(group_id) = self.players.get(username).and_then(|p| p.group_id.clone())
                else {
                    return ServerReply::from(ErrorKind::NotInGroup);
                };
                if !self.players.contains_key(&target) {
                    return ServerReply::err(protocol::RfcErrorCode::NotFound, "PLAYER_NOT_FOUND");
                }
                if self
                    .players
                    .get(&target)
                    .and_then(|p| p.group_id.as_ref())
                    .is_some()
                {
                    return ServerReply::from(ErrorKind::AlreadyInGroup);
                }
                if let Some(target_player) = self.players.get_mut(&target) {
                    target_player.pending_invite_from = Some(group_id);
                }
                self.send_to_player(
                    &target,
                    ServerEvent::GroupInvite {
                        leader: username.to_string(),
                    },
                );
                ServerReply::ok()
            }
            GroupAction::Join { leader } => {
                if self
                    .players
                    .get(username)
                    .and_then(|p| p.group_id.as_ref())
                    .is_some()
                {
                    return ServerReply::from(ErrorKind::AlreadyInGroup);
                }
                let target_group_id = self
                    .groups
                    .values()
                    .find(|g| g.leader == leader)
                    .map(|g| g.id.clone());
                let Some(group_id) = target_group_id else {
                    return ServerReply::from(ErrorKind::NotInGroup);
                };

                if let Some(group) = self.groups.get_mut(&group_id) {
                    group.members.insert(username.to_string());
                }
                if let Some(p) = self.players.get_mut(username) {
                    p.group_id = Some(group_id.clone());
                    p.pending_invite_from = None;
                }

                self.broadcast_to_group(
                    &group_id,
                    None,
                    ServerEvent::GroupJoin {
                        username: username.to_string(),
                    },
                );
                ServerReply::group(&group_id)
            }
            GroupAction::Leave => {
                let Some(group_id) = self.players.get(username).and_then(|p| p.group_id.clone())
                else {
                    return ServerReply::from(ErrorKind::NotInGroup);
                };
                self.remove_from_group(username, &group_id);
                ServerReply::ok()
            }
        }
    }

    fn remove_from_group(&mut self, username: &str, group_id: &str) {
        if let Some(p) = self.players.get_mut(username) {
            p.group_id = None;
        }
        self.broadcast_to_group(
            group_id,
            None,
            ServerEvent::GroupLeave {
                username: username.to_string(),
            },
        );
        if let Some(group) = self.groups.get_mut(group_id) {
            group.members.remove(username);
            if group.members.is_empty() {
                self.groups.remove(group_id);
            } else if group.leader == username {
                if let Some(new_leader) = group.members.iter().next().cloned() {
                    group.leader = new_leader;
                }
            }
        }
    }

    fn send_to_player(&self, username: &str, event: ServerEvent) {
        if let Some(player) = self.players.get(username) {
            let _ = player.tx.send(ServerMessage::Event(event));
        }
    }

    fn broadcast_to_room(&self, room_id: &str, exclude: Option<&str>, event: ServerEvent) {
        let msg = ServerMessage::Event(event);
        for player in self.players.values() {
            if player.room_id == room_id && exclude != Some(player.username.as_str()) {
                // Non-blocking send per client channel: disconnected clients never stall broadcasts
                let _ = player.tx.send(msg.clone());
            }
        }
    }

    fn broadcast_to_group(&self, group_id: &str, exclude: Option<&str>, event: ServerEvent) {
        let Some(group) = self.groups.get(group_id) else {
            return;
        };
        let msg = ServerMessage::Event(event);
        for member in &group.members {
            if exclude != Some(member.as_str()) {
                if let Some(player) = self.players.get(member) {
                    let _ = player.tx.send(msg.clone());
                }
            }
        }
    }

    fn broadcast_global(&self, exclude: Option<&str>, event: ServerEvent) {
        let msg = ServerMessage::Event(event);
        for player in self.players.values() {
            if exclude != Some(player.username.as_str()) {
                let _ = player.tx.send(msg.clone());
            }
        }
    }
}
