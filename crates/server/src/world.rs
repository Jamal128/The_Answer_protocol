use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorldError {
    #[error("failed to read world file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse YAML world data: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("world validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NpcRole {
    Dialogue,
    QuestGiver,
    Enemy,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestObjectiveType {
    CollectItem,
    DefeatNpc,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoomDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub exits: BTreeMap<String, String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub npcs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ItemDef {
    pub name: String,
    pub description: String,
    #[serde(default = "default_true")]
    pub obtainable: bool,
    #[serde(default)]
    pub attack_bonus: i32,
    #[serde(default)]
    pub heal_hp: i32,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct NpcDef {
    pub name: String,
    pub role: NpcRole,
    pub description: String,
    #[serde(default)]
    pub dialogue: Vec<String>,
    pub hp: i32,
    #[serde(default)]
    pub damage: i32,
    #[serde(default)]
    pub quest_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestDef {
    pub giver: String,
    pub description: String,
    pub objective_type: QuestObjectiveType,
    pub target: String,
    #[serde(default = "default_one")]
    pub required_count: usize,
    pub reward_item: String,
}

const fn default_one() -> usize {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorldData {
    pub spawn_room: String,
    pub respawn_room: String,
    pub rooms: HashMap<String, RoomDef>,
    pub items: HashMap<String, ItemDef>,
    pub npcs: HashMap<String, NpcDef>,
    pub quests: HashMap<String, QuestDef>,
}

impl WorldData {
    /// Loads and validates static world definitions from a YAML file.
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, WorldError> {
        let raw = fs::read_to_string(path)?;
        let world: Self = serde_yaml::from_str(&raw)?;
        world.validate()?;
        Ok(world)
    }

    /// Validates mandatory project constraints and all cross-references.
    pub fn validate(&self) -> Result<(), WorldError> {
        if self.rooms.len() < 8 {
            return Err(WorldError::Validation(format!(
                "expected at least 8 rooms, found {}",
                self.rooms.len()
            )));
        }
        if !self.rooms.contains_key(&self.spawn_room) {
            return Err(WorldError::Validation(format!(
                "spawn_room '{}' does not exist",
                self.spawn_room
            )));
        }
        if !self.rooms.contains_key(&self.respawn_room) {
            return Err(WorldError::Validation(format!(
                "respawn_room '{}' does not exist",
                self.respawn_room
            )));
        }

        if self.items.len() < 4 {
            return Err(WorldError::Validation(format!(
                "expected at least 4 items, found {}",
                self.items.len()
            )));
        }
        let obtainable_count = self.items.values().filter(|i| i.obtainable).count();
        if obtainable_count < 2 {
            return Err(WorldError::Validation(format!(
                "expected at least 2 obtainable items, found {obtainable_count}"
            )));
        }

        let roles: HashSet<_> = self
            .npcs
            .values()
            .map(|n| std::mem::discriminant(&n.role))
            .collect();
        if roles.len() < 3 {
            return Err(WorldError::Validation(
                "expected at least 3 distinct NPC roles (dialogue, quest_giver, enemy)".to_string(),
            ));
        }

        if self.quests.len() < 2 {
            return Err(WorldError::Validation(format!(
                "expected at least 2 quests, found {}",
                self.quests.len()
            )));
        }

        for (room_id, room) in &self.rooms {
            for (dir, target_room) in &room.exits {
                if !self.rooms.contains_key(target_room) {
                    return Err(WorldError::Validation(format!(
                        "room '{room_id}' exit '{dir}' points to missing room '{target_room}'"
                    )));
                }
            }
            for item_id in &room.items {
                if !self.items.contains_key(item_id) {
                    return Err(WorldError::Validation(format!(
                        "room '{room_id}' references missing item '{item_id}'"
                    )));
                }
            }
            for npc_id in &room.npcs {
                if !self.npcs.contains_key(npc_id) {
                    return Err(WorldError::Validation(format!(
                        "room '{room_id}' references missing npc '{npc_id}'"
                    )));
                }
            }
        }

        for (npc_id, npc) in &self.npcs {
            if let Some(qid) = &npc.quest_id {
                if !self.quests.contains_key(qid) {
                    return Err(WorldError::Validation(format!(
                        "npc '{npc_id}' references missing quest '{qid}'"
                    )));
                }
            }
        }

        for (quest_id, quest) in &self.quests {
            if !self.npcs.contains_key(&quest.giver) {
                return Err(WorldError::Validation(format!(
                    "quest '{quest_id}' references missing giver '{}'",
                    quest.giver
                )));
            }
            if !self.items.contains_key(&quest.reward_item) {
                return Err(WorldError::Validation(format!(
                    "quest '{quest_id}' references missing reward item '{}'",
                    quest.reward_item
                )));
            }
            match quest.objective_type {
                QuestObjectiveType::CollectItem => {
                    if !self.items.contains_key(&quest.target) {
                        return Err(WorldError::Validation(format!(
                            "quest '{quest_id}' target item '{}' does not exist",
                            quest.target
                        )));
                    }
                }
                QuestObjectiveType::DefeatNpc => {
                    if !self.npcs.contains_key(&quest.target) {
                        return Err(WorldError::Validation(format!(
                            "quest '{quest_id}' target npc '{}' does not exist",
                            quest.target
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dune_world_validates() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = format!("{manifest_dir}/../../data/world.yaml");
        let world = WorldData::load_from_path(path).expect("world.yaml should be valid");
        assert!(world.rooms.len() >= 8);
        assert!(world.quests.len() >= 2);
    }
}
