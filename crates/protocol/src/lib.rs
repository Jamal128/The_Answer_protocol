use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Protocol version advertised in `OK hello proto=1`.
pub const PROTOCOL_VERSION: u8 = 1;

/// Recommended maximum line length in bytes (RFC 42TAP Section 9.4).
pub const MAX_LINE_LENGTH: usize = 1024;

/// Standard RFC 42TAP error codes (Section 8.2) + syntax validation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfcErrorCode {
    NameInUse = 201,
    NoExit = 301,
    BadRequest = 400,
    NotInGroup = 401,
    AlreadyInGroup = 402,
    NotFound = 404,
    NpcNotHostile = 405,
    NoQuestAvailable = 406,
    LineTooLong = 413,
    ConnectionFailed = 900,
    SendFailed = 901,
}

impl RfcErrorCode {
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Errors produced when parsing RFC 42TAP wire messages.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("EMPTY_COMMAND")]
    EmptyCommand,
    #[error("LINE_TOO_LONG")]
    LineTooLong,
    #[error("UNKNOWN_COMMAND")]
    UnknownCommand,
    #[error("MISSING_ARGUMENT")]
    MissingArgument,
    #[error("INVALID_ARGUMENT")]
    InvalidArgument,
}

impl ProtocolError {
    #[must_use]
    pub const fn code(&self) -> RfcErrorCode {
        match self {
            Self::LineTooLong => RfcErrorCode::LineTooLong,
            Self::EmptyCommand
            | Self::UnknownCommand
            | Self::MissingArgument
            | Self::InvalidArgument => RfcErrorCode::BadRequest,
        }
    }
}

/// Supported chat scopes in RFC 42TAP (Section 5.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatScope {
    Global,
    Room,
    Group,
}

impl FromStr for ChatScope {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "GLOBAL" => Ok(Self::Global),
            "ROOM" => Ok(Self::Room),
            "GROUP" => Ok(Self::Group),
            _ => Err(ProtocolError::InvalidArgument),
        }
    }
}

impl fmt::Display for ChatScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Global => write!(f, "GLOBAL"),
            Self::Room => write!(f, "ROOM"),
            Self::Group => write!(f, "GROUP"),
        }
    }
}

/// Subcommands for `GROUP` (Section 5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupAction {
    Create,
    Invite { username: String },
    Join { leader: String },
    Leave,
}

/// All 15 mandatory client-to-server commands defined in RFC 42TAP (Section 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Connect { username: String },
    Look,
    Move { direction: String },
    Chat { scope: ChatScope, message: String },
    Take { item_ref: String },
    Drop { item_ref: String },
    Inventory,
    Talk { npc_ref: String },
    Attack { target: String },
    Status,
    Quest { npc_ref: String },
    Quests,
    Who,
    Group(GroupAction),
    Quit,
}

impl FromStr for Command {
    type Err = ProtocolError;

    fn from_str(raw_line: &str) -> Result<Self, Self::Err> {
        if raw_line.len() > MAX_LINE_LENGTH {
            return Err(ProtocolError::LineTooLong);
        }

        let trimmed = raw_line.trim_end_matches(&['\r', '\n'][..]).trim();
        if trimmed.is_empty() {
            return Err(ProtocolError::EmptyCommand);
        }

        let (verb, rest) = match trimmed.split_once(char::is_whitespace) {
            Some((v, r)) => (v.to_ascii_uppercase(), r.trim()),
            None => (trimmed.to_ascii_uppercase(), ""),
        };

        match verb.as_str() {
            "CONNECT" => {
                if rest.is_empty() || rest.contains(char::is_whitespace) {
                    return Err(ProtocolError::InvalidArgument);
                }
                Ok(Self::Connect {
                    username: rest.to_string(),
                })
            }
            "LOOK" => Ok(Self::Look),
            "MOVE" => {
                if rest.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                Ok(Self::Move {
                    direction: rest.to_ascii_lowercase(),
                })
            }
            "CHAT" => {
                let (scope_str, msg) = rest
                    .split_once(char::is_whitespace)
                    .ok_or(ProtocolError::MissingArgument)?;
                let scope = ChatScope::from_str(scope_str)?;
                let message = msg.trim();
                if message.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                Ok(Self::Chat {
                    scope,
                    message: message.to_string(),
                })
            }
            "TAKE" => {
                if rest.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                Ok(Self::Take {
                    item_ref: rest.to_string(),
                })
            }
            "DROP" => {
                if rest.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                Ok(Self::Drop {
                    item_ref: rest.to_string(),
                })
            }
            "INVENTORY" => Ok(Self::Inventory),
            "TALK" => {
                if rest.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                Ok(Self::Talk {
                    npc_ref: rest.to_string(),
                })
            }
            "ATTACK" => {
                if rest.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                Ok(Self::Attack {
                    target: rest.to_string(),
                })
            }
            "STATUS" => Ok(Self::Status),
            "QUEST" => {
                if rest.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                Ok(Self::Quest {
                    npc_ref: rest.to_string(),
                })
            }
            "QUESTS" => Ok(Self::Quests),
            "WHO" => Ok(Self::Who),
            "GROUP" => {
                if rest.is_empty() {
                    return Err(ProtocolError::MissingArgument);
                }
                let (sub, arg) = match rest.split_once(char::is_whitespace) {
                    Some((s, a)) => (s.to_ascii_uppercase(), a.trim()),
                    None => (rest.to_ascii_uppercase(), ""),
                };
                match sub.as_str() {
                    "CREATE" => Ok(Self::Group(GroupAction::Create)),
                    "LEAVE" => Ok(Self::Group(GroupAction::Leave)),
                    "INVITE" => {
                        if arg.is_empty() {
                            return Err(ProtocolError::MissingArgument);
                        }
                        Ok(Self::Group(GroupAction::Invite {
                            username: arg.to_string(),
                        }))
                    }
                    "JOIN" => {
                        if arg.is_empty() {
                            return Err(ProtocolError::MissingArgument);
                        }
                        Ok(Self::Group(GroupAction::Join {
                            leader: arg.to_string(),
                        }))
                    }
                    _ => Err(ProtocolError::InvalidArgument),
                }
            }
            "QUIT" => Ok(Self::Quit),
            _ => Err(ProtocolError::UnknownCommand),
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect { username } => write!(f, "CONNECT {username}"),
            Self::Look => write!(f, "LOOK"),
            Self::Move { direction } => write!(f, "MOVE {direction}"),
            Self::Chat { scope, message } => write!(f, "CHAT {scope} {message}"),
            Self::Take { item_ref } => write!(f, "TAKE {item_ref}"),
            Self::Drop { item_ref } => write!(f, "DROP {item_ref}"),
            Self::Inventory => write!(f, "INVENTORY"),
            Self::Talk { npc_ref } => write!(f, "TALK {npc_ref}"),
            Self::Attack { target } => write!(f, "ATTACK {target}"),
            Self::Status => write!(f, "STATUS"),
            Self::Quest { npc_ref } => write!(f, "QUEST {npc_ref}"),
            Self::Quests => write!(f, "QUESTS"),
            Self::Who => write!(f, "WHO"),
            Self::Group(GroupAction::Create) => write!(f, "GROUP CREATE"),
            Self::Group(GroupAction::Invite { username }) => write!(f, "GROUP INVITE {username}"),
            Self::Group(GroupAction::Join { leader }) => write!(f, "GROUP JOIN {leader}"),
            Self::Group(GroupAction::Leave) => write!(f, "GROUP LEAVE"),
            Self::Quit => write!(f, "QUIT"),
        }
    }
}

/// JSON payload returned by `LOOK` (Section 5.1.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LookPayload {
    pub room: RoomInfo,
    pub players: Vec<String>,
    pub items: Vec<String>,
    pub npcs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub exits: HashMap<String, String>,
}

/// JSON payload returned by `ATTACK` (Section 5.4.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttackPayload {
    pub attacker_hp: i32,
    pub target_hp: i32,
    pub damage: i32,
    pub status: String,
}

/// JSON payload returned by `STATUS` (Section 5.4.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusPayload {
    pub hp: i32,
    pub max_hp: i32,
    pub status: String,
}

/// JSON payload returned by `QUEST <npc>` (Section 5.4.7).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestOfferPayload {
    pub quest_id: String,
    pub description: String,
    pub reward: String,
    pub status: String,
}

/// JSON element returned in `QUESTS` list (Section 5.4.8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestProgressPayload {
    pub quest_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
}

/// Asynchronous server-to-client events (`EVT ...`, Section 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    RoomPresenceEnter {
        username: String,
    },
    RoomPresenceLeave {
        username: String,
    },
    RoomChat {
        username: String,
        message: String,
    },
    GlobalChat {
        username: String,
        message: String,
    },
    GroupInvite {
        leader: String,
    },
    GroupJoin {
        username: String,
    },
    GroupLeave {
        username: String,
    },
    GroupChat {
        username: String,
        message: String,
    },
    StatsPlayers {
        count: usize,
    },
}

impl fmt::Display for ServerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoomPresenceEnter { username } => {
                write!(f, "EVT ROOM PRESENCE ENTER {username}")
            }
            Self::RoomPresenceLeave { username } => {
                write!(f, "EVT ROOM PRESENCE LEAVE {username}")
            }
            Self::RoomChat { username, message } => {
                write!(f, "EVT ROOM CHAT {username} {message}")
            }
            Self::GlobalChat { username, message } => {
                write!(f, "EVT GLOBAL CHAT {username} {message}")
            }
            Self::GroupInvite { leader } => write!(f, "EVT GROUP INVITE {leader}"),
            Self::GroupJoin { username } => write!(f, "EVT GROUP JOIN {username}"),
            Self::GroupLeave { username } => write!(f, "EVT GROUP LEAVE {username}"),
            Self::GroupChat { username, message } => {
                write!(f, "EVT GROUP CHAT {username} {message}")
            }
            Self::StatsPlayers { count } => write!(f, "EVT STATS players={count}"),
        }
    }
}

/// Synchronous server-to-client replies (`OK ...` or `ERR ...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReply {
    Ok(Option<String>),
    Err { code: u16, message: String },
}

impl ServerReply {
    #[must_use]
    pub fn hello() -> Self {
        Self::Ok(Some(format!("hello proto={PROTOCOL_VERSION}")))
    }

    #[must_use]
    pub fn ok() -> Self {
        Self::Ok(None)
    }

    #[must_use]
    pub fn ok_with(payload: impl Into<String>) -> Self {
        Self::Ok(Some(payload.into()))
    }

    #[must_use]
    pub fn err(code: RfcErrorCode, message: impl Into<String>) -> Self {
        Self::Err {
            code: code.as_u16(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ServerReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok(Some(payload)) => write!(f, "OK {payload}"),
            Self::Ok(None) => write!(f, "OK"),
            Self::Err { code, message } => write!(f, "ERR {code} {message}"),
        }
    }
}

/// Outbound server wire message (`\n`-terminated UTF-8 frame).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    Reply(ServerReply),
    Event(ServerEvent),
}

impl ServerMessage {
    #[must_use]
    pub fn to_wire_line(&self) -> String {
        match self {
            Self::Reply(reply) => format!("{reply}\n"),
            Self::Event(event) => format!("{event}\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_group_subcommands() {
        assert_eq!(
            Command::from_str("GROUP CREATE").unwrap(),
            Command::Group(GroupAction::Create)
        );
        assert_eq!(
            Command::from_str("GROUP INVITE bob").unwrap(),
            Command::Group(GroupAction::Invite {
                username: "bob".to_string()
            })
        );
        assert_eq!(
            Command::from_str("GROUP JOIN alice").unwrap(),
            Command::Group(GroupAction::Join {
                leader: "alice".to_string()
            })
        );
        assert_eq!(
            Command::from_str("GROUP LEAVE").unwrap(),
            Command::Group(GroupAction::Leave)
        );
    }

    #[test]
    fn formats_rfc_errors_and_events() {
        let err = ServerReply::err(RfcErrorCode::NameInUse, "NAME_IN_USE");
        assert_eq!(err.to_string(), "ERR 201 NAME_IN_USE");

        let evt = ServerEvent::StatsPlayers { count: 4 };
        assert_eq!(evt.to_string(), "EVT STATS players=4");
    }
}