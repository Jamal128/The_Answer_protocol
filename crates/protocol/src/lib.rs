//! # protocol — RFC 42TAP (The Answer Protocol) wire format
//!
//! **EN:** This crate contains *only* the language spoken between a TAP server and its
//! clients: how to parse a client command, how to format server replies and events, and
//! how a client can parse what the server sent. It performs **no I/O** (no sockets, no
//! files), so the server, the CLI and the GUI can all share it.
//!
//! **RU:** Этот крейт содержит *только* «язык», на котором общаются сервер TAP и клиенты:
//! как разобрать команду клиента, как сформировать ответы и события сервера и как клиент
//! разбирает то, что прислал сервер. Здесь **нет ввода-вывода** (ни сокетов, ни файлов),
//! поэтому сервер, CLI и GUI могут использовать один и тот же крейт.
//!
//! ## Layout / Структура
//!
//! | Part / Часть | Types / Типы |
//! |---|---|
//! | Errors / Ошибки | [`RfcErrorCode`], [`ErrorKind`], [`ProtocolError`] |
//! | Client → server | [`Command`], [`ChatScope`], [`GroupAction`] |
//! | JSON bodies / JSON-тела | [`LookPayload`], [`StatusPayload`], [`AttackPayload`], ... |
//! | Server → client | [`ServerReply`], [`ServerEvent`], [`ServerMessage`] |
//! | Helpers / Помощники | [`validate_username`], [`matches_reference`] |
//!
//! ## Extensions beyond the RFC / Расширения сверх RFC
//!
//! The RFC leaves combat and quests open, so this crate adds (document them in the README):
//! RFC оставляет бой и квесты на усмотрение команды, поэтому добавлено (описать в README):
//!
//! * commands `DEFEND`, `FLEE`;
//! * events `EVT ROOM ITEM TAKE|DROP`, `EVT COMBAT ...`, `EVT QUEST ...`;
//! * error codes `400`, `403`, `409`, `410`, `413`, `429`, `500`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Constants / Константы
// ---------------------------------------------------------------------------

/// Protocol version advertised in `OK hello proto=1`.
///
/// RU: Версия протокола, объявляемая в приветствии `OK hello proto=1`.
pub const PROTOCOL_VERSION: u8 = 1;

/// Recommended maximum line length in bytes, without the line terminator
/// (RFC 42TAP section 9.4).
///
/// RU: Рекомендуемая максимальная длина строки в байтах, без символа конца строки
/// (RFC 42TAP, раздел 9.4).
pub const MAX_LINE_LENGTH: usize = 1024;

/// Maximum username length, counted in Unicode characters (not bytes).
///
/// RU: Максимальная длина имени пользователя в символах Unicode (не в байтах).
pub const MAX_USERNAME_LENGTH: usize = 32;

/// Well-known values for the `status` JSON field of `STATUS` / `ATTACK` replies.
///
/// RU: Стандартные значения поля `status` в ответах `STATUS` / `ATTACK`.
pub mod status {
    /// Player is at full health and not fighting. / Игрок здоров и не в бою.
    pub const HEALTHY: &str = "healthy";
    /// Player is hurt but not fighting. / Игрок ранен, но не в бою.
    pub const WOUNDED: &str = "wounded";
    /// Player is in combat. / Игрок в бою.
    pub const COMBAT: &str = "combat";
    /// Player chose `DEFEND` this turn. / Игрок выбрал `DEFEND` в этом ходу.
    pub const DEFENDING: &str = "defending";
    /// The enemy was defeated. / Враг побеждён.
    pub const VICTORY: &str = "victory";
    /// The player was defeated (0 HP). / Игрок повержен (0 HP).
    pub const DEFEATED: &str = "defeated";
}

/// Well-known values for the `status` field of quest payloads.
///
/// RU: Стандартные значения поля `status` в данных о квестах.
pub mod quest_status {
    /// Quest can be taken. / Квест можно взять.
    pub const AVAILABLE: &str = "available";
    /// Quest is in progress. / Квест выполняется.
    pub const ACTIVE: &str = "active";
    /// Quest is finished. / Квест завершён.
    pub const COMPLETED: &str = "completed";
}

// ---------------------------------------------------------------------------
// Errors / Ошибки
// ---------------------------------------------------------------------------

/// Numeric error codes: RFC 42TAP section 8.2 plus our documented extensions.
///
/// RU: Числовые коды ошибок: RFC 42TAP, раздел 8.2, плюс наши задокументированные
/// расширения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RfcErrorCode {
    /// 201 — username already used. / имя уже занято.
    NameInUse = 201,
    /// 301 — no exit in that direction. / нет выхода в этом направлении.
    NoExit = 301,
    /// 400 — extension: malformed request. / расширение: некорректный запрос.
    BadRequest = 400,
    /// 401 — player is not in a group. / игрок не состоит в группе.
    NotInGroup = 401,
    /// 402 — player is already in a group. / игрок уже в группе.
    AlreadyInGroup = 402,
    /// 403 — extension: `CONNECT` required first. / расширение: сначала нужен `CONNECT`.
    NotAuthenticated = 403,
    /// 404 — item / NPC not found (the message tells which). / предмет или NPC не найден
    /// (сообщение уточняет, что именно).
    NotFound = 404,
    /// 405 — NPC cannot be attacked. / NPC нельзя атаковать.
    NpcNotHostile = 405,
    /// 406 — no quest available. / нет доступного квеста.
    NoQuestAvailable = 406,
    /// 409 — extension: `CONNECT` sent twice. / расширение: повторный `CONNECT`.
    AlreadyConnected = 409,
    /// 410 — extension: `DEFEND`/`FLEE` outside combat. / расширение: `DEFEND`/`FLEE` вне боя.
    NotInCombat = 410,
    /// 413 — extension: line longer than [`MAX_LINE_LENGTH`]. / расширение: строка длиннее
    /// [`MAX_LINE_LENGTH`].
    LineTooLong = 413,
    /// 429 — extension: command flooding. / расширение: слишком частые команды.
    RateLimited = 429,
    /// 500 — extension: internal server error. / расширение: внутренняя ошибка сервера.
    InternalError = 500,
    /// 900 — connection establishment failed. / не удалось установить соединение.
    ConnectionFailed = 900,
    /// 901 — message transmission failed. / не удалось отправить сообщение.
    SendFailed = 901,
}

impl RfcErrorCode {
    /// The numeric value sent on the wire.
    ///
    /// RU: Числовое значение, отправляемое по сети.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }
}

impl fmt::Display for RfcErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u16())
    }
}

/// A complete error: numeric code **and** its fixed symbolic name, exactly as in the RFC
/// table. Using this avoids typos such as sending code 404 with the wrong message.
///
/// RU: Полная ошибка: числовой код **и** его фиксированное символьное имя, как в таблице
/// RFC. Это исключает опечатки вроде кода 404 с неправильным сообщением.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    NameInUse,
    NoExit,
    NotInGroup,
    AlreadyInGroup,
    ItemNotFound,
    ItemNotInInventory,
    NpcNotFound,
    NpcNotHostile,
    NoQuestAvailable,
    BadRequest,
    NotAuthenticated,
    AlreadyConnected,
    NotInCombat,
    LineTooLong,
    RateLimited,
    InternalError,
    ConnectionFailed,
    SendFailed,
}

impl ErrorKind {
    /// Numeric code of this error. / Числовой код этой ошибки.
    #[must_use]
    pub const fn code(self) -> RfcErrorCode {
        match self {
            Self::NameInUse => RfcErrorCode::NameInUse,
            Self::NoExit => RfcErrorCode::NoExit,
            Self::NotInGroup => RfcErrorCode::NotInGroup,
            Self::AlreadyInGroup => RfcErrorCode::AlreadyInGroup,
            Self::ItemNotFound | Self::ItemNotInInventory | Self::NpcNotFound => {
                RfcErrorCode::NotFound
            }
            Self::NpcNotHostile => RfcErrorCode::NpcNotHostile,
            Self::NoQuestAvailable => RfcErrorCode::NoQuestAvailable,
            Self::BadRequest => RfcErrorCode::BadRequest,
            Self::NotAuthenticated => RfcErrorCode::NotAuthenticated,
            Self::AlreadyConnected => RfcErrorCode::AlreadyConnected,
            Self::NotInCombat => RfcErrorCode::NotInCombat,
            Self::LineTooLong => RfcErrorCode::LineTooLong,
            Self::RateLimited => RfcErrorCode::RateLimited,
            Self::InternalError => RfcErrorCode::InternalError,
            Self::ConnectionFailed => RfcErrorCode::ConnectionFailed,
            Self::SendFailed => RfcErrorCode::SendFailed,
        }
    }

    /// Symbolic name sent after the code (e.g. `NAME_IN_USE`).
    ///
    /// RU: Символьное имя, отправляемое после кода (например, `NAME_IN_USE`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NameInUse => "NAME_IN_USE",
            Self::NoExit => "NO_EXIT",
            Self::NotInGroup => "NOT_IN_GROUP",
            Self::AlreadyInGroup => "ALREADY_IN_GROUP",
            Self::ItemNotFound => "ITEM_NOT_FOUND",
            Self::ItemNotInInventory => "ITEM_NOT_IN_INVENTORY",
            Self::NpcNotFound => "NPC_NOT_FOUND",
            Self::NpcNotHostile => "NPC_NOT_HOSTILE",
            Self::NoQuestAvailable => "NO_QUEST_AVAILABLE",
            Self::BadRequest => "BAD_REQUEST",
            Self::NotAuthenticated => "NOT_AUTHENTICATED",
            Self::AlreadyConnected => "ALREADY_CONNECTED",
            Self::NotInCombat => "NOT_IN_COMBAT",
            Self::LineTooLong => "LINE_TOO_LONG",
            Self::RateLimited => "RATE_LIMITED",
            Self::InternalError => "INTERNAL_ERROR",
            Self::ConnectionFailed => "CONNECTION_FAILED",
            Self::SendFailed => "SEND_FAILED",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code(), self.name())
    }
}

/// Errors produced while parsing wire messages (either direction).
///
/// RU: Ошибки при разборе сетевых сообщений (в обоих направлениях).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    /// Empty line. / Пустая строка.
    #[error("EMPTY_COMMAND")]
    EmptyCommand,
    /// Line exceeds [`MAX_LINE_LENGTH`]. / Строка длиннее [`MAX_LINE_LENGTH`].
    #[error("LINE_TOO_LONG")]
    LineTooLong,
    /// Unknown command verb. / Неизвестная команда.
    #[error("UNKNOWN_COMMAND")]
    UnknownCommand,
    /// A required argument is missing. / Не хватает обязательного аргумента.
    #[error("MISSING_ARGUMENT")]
    MissingArgument,
    /// An argument has an invalid value. / Недопустимое значение аргумента.
    #[error("INVALID_ARGUMENT")]
    InvalidArgument,
    /// The line contains a control character (RFC 9.2). / В строке есть управляющий символ
    /// (RFC 9.2).
    #[error("CONTROL_CHARACTER")]
    ControlCharacter,
    /// (Client side) a server reply could not be parsed. / (На стороне клиента) не удалось
    /// разобрать ответ сервера.
    #[error("MALFORMED_REPLY")]
    MalformedReply,
    /// (Client side) an `EVT` line of an unknown type. / (На стороне клиента) событие
    /// неизвестного типа.
    #[error("UNKNOWN_EVENT")]
    UnknownEvent,
}

impl ProtocolError {
    /// Numeric code to answer a client with.
    ///
    /// RU: Числовой код для ответа клиенту.
    #[must_use]
    pub const fn code(&self) -> RfcErrorCode {
        self.kind().code()
    }

    /// The matching [`ErrorKind`]: `413` for long lines, `400` for everything else.
    ///
    /// RU: Соответствующий [`ErrorKind`]: `413` для длинных строк, `400` для остального.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::LineTooLong => ErrorKind::LineTooLong,
            _ => ErrorKind::BadRequest,
        }
    }
}

// ---------------------------------------------------------------------------
// Small parsing helpers / Небольшие помощники разбора
// ---------------------------------------------------------------------------

/// Removes the trailing `\n` / `\r\n`. / Убирает завершающие `\n` / `\r\n`.
fn strip_eol(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

/// Splits `"word rest of line"` into `("word", "rest of line")`; `rest` is trimmed and
/// may be empty.
///
/// RU: Делит `"слово остальное"` на `("слово", "остальное")`; `rest` без пробелов по
/// краям, может быть пустым.
fn split_first_word(s: &str) -> (&str, &str) {
    match s.split_once(char::is_whitespace) {
        Some((head, tail)) => (head, tail.trim()),
        None => (s, ""),
    }
}

/// Returns `rest` or `MissingArgument` when it is empty.
///
/// RU: Возвращает `rest` или `MissingArgument`, если он пуст.
fn required(rest: &str) -> Result<&str, ProtocolError> {
    if rest.is_empty() {
        Err(ProtocolError::MissingArgument)
    } else {
        Ok(rest)
    }
}

/// Checks a username: 1..=[`MAX_USERNAME_LENGTH`] characters, no whitespace, no control
/// characters. Any other Unicode is allowed (RFC 9.2).
///
/// RU: Проверка имени: от 1 до [`MAX_USERNAME_LENGTH`] символов, без пробелов и
/// управляющих символов. Любой другой Unicode разрешён (RFC 9.2).
///
/// # Errors
/// `MissingArgument` if empty, `InvalidArgument` otherwise.
pub fn validate_username(name: &str) -> Result<(), ProtocolError> {
    if name.is_empty() {
        return Err(ProtocolError::MissingArgument);
    }
    let too_long = name.chars().count() > MAX_USERNAME_LENGTH;
    let bad_char = name.chars().any(|c| c.is_whitespace() || c.is_control());
    if too_long || bad_char {
        return Err(ProtocolError::InvalidArgument);
    }
    Ok(())
}

/// Does a player-typed reference (`"Healing Potion"`, `"healing_potion"`, `"herbs"`,
/// `"item.herbs"`) designate the thing with canonical `id` and `display_name`?
/// Comparison ignores case, repeated spaces, `_` vs space, and the `item.` / `npc.` /
/// `loc.` prefix. Multi-word names work because the whole argument is compared.
///
/// RU: Обозначает ли ссылка, введённая игроком (`"Healing Potion"`, `"healing_potion"`,
/// `"herbs"`, `"item.herbs"`), объект с каноническим `id` и `display_name`? Сравнение не
/// зависит от регистра, повторных пробелов, `_` вместо пробела и префикса `item.` /
/// `npc.` / `loc.`. Имена из нескольких слов работают, так как сравнивается весь аргумент.
#[must_use]
pub fn matches_reference(reference: &str, id: &str, display_name: &str) -> bool {
    fn normalize(s: &str) -> String {
        s.replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    let wanted = normalize(reference);
    if wanted.is_empty() {
        return false;
    }
    let short_id = id.split_once('.').map_or(id, |(_, tail)| tail);
    [id, short_id, display_name]
        .iter()
        .any(|candidate| normalize(candidate) == wanted)
}

// ---------------------------------------------------------------------------
// Client -> server commands / Команды клиент -> сервер
// ---------------------------------------------------------------------------

/// Supported chat scopes (RFC 5.2.1). / Поддерживаемые области чата (RFC 5.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatScope {
    /// Whole server. / Весь сервер.
    Global,
    /// Players in the same room. / Игроки в той же комнате.
    Room,
    /// Members of the same group. / Участники той же группы.
    Group,
}

impl FromStr for ChatScope {
    type Err = ProtocolError;

    /// Case-insensitive. / Без учёта регистра.
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

/// Sub-commands of `GROUP` (RFC 5.3). / Подкоманды `GROUP` (RFC 5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupAction {
    /// `GROUP CREATE`
    Create,
    /// `GROUP INVITE <username>`
    Invite { username: String },
    /// `GROUP JOIN <leader-name>`
    Join { leader: String },
    /// `GROUP LEAVE`
    Leave,
}

/// Every client-to-server command: the 15 from the RFC plus `DEFEND` and `FLEE`.
///
/// RU: Все команды клиент → сервер: 15 из RFC плюс `DEFEND` и `FLEE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CONNECT <username>` — choose a name. / выбор имени.
    Connect { username: String },
    /// `LOOK` — describe the current room. / описать текущую комнату.
    Look,
    /// `MOVE <direction>` — walk through an exit. / пройти через выход.
    Move { direction: String },
    /// `CHAT <scope> <message>` — talk to other players. / сообщение другим игрокам.
    Chat { scope: ChatScope, message: String },
    /// `TAKE <item>` — pick an item up (ID or display name). / взять предмет (ID или имя).
    Take { item_ref: String },
    /// `DROP <item>` — drop an item. / бросить предмет.
    Drop { item_ref: String },
    /// `INVENTORY` — list carried items. / список предметов в инвентаре.
    Inventory,
    /// `TALK <npc>` — speak to an NPC. / поговорить с NPC.
    Talk { npc_ref: String },
    /// `ATTACK <npc>` — attack an enemy. / атаковать врага.
    Attack { target: String },
    /// `STATUS` — own HP and combat state. / своё здоровье и состояние боя.
    Status,
    /// `QUEST <npc>` — ask an NPC for a quest. / запросить квест у NPC.
    Quest { npc_ref: String },
    /// `QUESTS` — list own quests. / список своих квестов.
    Quests,
    /// `WHO` — number of players online. / число игроков онлайн.
    Who,
    /// `GROUP <sub-command>` — group management. / управление группой.
    Group(GroupAction),
    /// `DEFEND` — extension: brace for the enemy's next hit. / расширение: приготовиться
    /// к следующему удару врага.
    Defend,
    /// `FLEE` — extension: try to leave the fight. / расширение: попытаться сбежать из боя.
    Flee,
    /// `QUIT` — disconnect. / отключиться.
    Quit,
}

impl Command {
    /// The line to send over the wire, including the final `\n`. Used by clients.
    ///
    /// RU: Строка для отправки по сети, с завершающим `\n`. Используется клиентами.
    #[must_use]
    pub fn to_wire_line(&self) -> String {
        format!("{self}\n")
    }
}

impl FromStr for Command {
    type Err = ProtocolError;

    /// Parses one received line (with or without `\n` / `\r\n`). The verb is
    /// case-insensitive. Commands without arguments ignore any trailing text.
    ///
    /// RU: Разбирает одну полученную строку (с `\n` / `\r\n` или без). Команда
    /// нечувствительна к регистру. Команды без аргументов игнорируют лишний текст.
    fn from_str(raw_line: &str) -> Result<Self, Self::Err> {
        let line = strip_eol(raw_line);
        if line.len() > MAX_LINE_LENGTH {
            return Err(ProtocolError::LineTooLong);
        }
        if line.chars().any(char::is_control) {
            return Err(ProtocolError::ControlCharacter);
        }
        let line = line.trim();
        if line.is_empty() {
            return Err(ProtocolError::EmptyCommand);
        }

        let (verb, rest) = split_first_word(line);
        match verb.to_ascii_uppercase().as_str() {
            "CONNECT" => {
                validate_username(rest)?;
                Ok(Self::Connect {
                    username: rest.to_string(),
                })
            }
            "LOOK" => Ok(Self::Look),
            "MOVE" => Ok(Self::Move {
                direction: required(rest)?.to_ascii_lowercase(),
            }),
            "CHAT" => {
                let (scope_str, message) = split_first_word(rest);
                required(scope_str)?;
                let scope = scope_str.parse::<ChatScope>()?;
                Ok(Self::Chat {
                    scope,
                    message: required(message)?.to_string(),
                })
            }
            "TAKE" => Ok(Self::Take {
                item_ref: required(rest)?.to_string(),
            }),
            "DROP" => Ok(Self::Drop {
                item_ref: required(rest)?.to_string(),
            }),
            "INVENTORY" => Ok(Self::Inventory),
            "TALK" => Ok(Self::Talk {
                npc_ref: required(rest)?.to_string(),
            }),
            "ATTACK" => Ok(Self::Attack {
                target: required(rest)?.to_string(),
            }),
            "STATUS" => Ok(Self::Status),
            "QUEST" => Ok(Self::Quest {
                npc_ref: required(rest)?.to_string(),
            }),
            "QUESTS" => Ok(Self::Quests),
            "WHO" => Ok(Self::Who),
            "GROUP" => Self::parse_group(required(rest)?),
            "DEFEND" => Ok(Self::Defend),
            "FLEE" => Ok(Self::Flee),
            "QUIT" => Ok(Self::Quit),
            _ => Err(ProtocolError::UnknownCommand),
        }
    }
}

impl Command {
    /// Parses what follows `GROUP `. / Разбирает то, что идёт после `GROUP `.
    fn parse_group(rest: &str) -> Result<Self, ProtocolError> {
        let (sub, arg) = split_first_word(rest);
        match sub.to_ascii_uppercase().as_str() {
            "CREATE" => Ok(Self::Group(GroupAction::Create)),
            "LEAVE" => Ok(Self::Group(GroupAction::Leave)),
            "INVITE" => Ok(Self::Group(GroupAction::Invite {
                username: required(arg)?.to_string(),
            })),
            "JOIN" => Ok(Self::Group(GroupAction::Join {
                leader: required(arg)?.to_string(),
            })),
            _ => Err(ProtocolError::InvalidArgument),
        }
    }
}

impl fmt::Display for Command {
    /// Canonical wire form, without the final `\n`. / Каноничная форма без `\n`.
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
            Self::Defend => write!(f, "DEFEND"),
            Self::Flee => write!(f, "FLEE"),
            Self::Quit => write!(f, "QUIT"),
        }
    }
}

// ---------------------------------------------------------------------------
// JSON payloads / JSON-данные в ответах
// ---------------------------------------------------------------------------

/// Body of `OK` for `LOOK` (RFC 5.1.2). / Тело `OK` для `LOOK` (RFC 5.1.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LookPayload {
    /// The room itself. / Сама комната.
    pub room: RoomInfo,
    /// Usernames in the room. / Имена игроков в комнате.
    pub players: Vec<String>,
    /// Item IDs lying in the room. / ID предметов в комнате.
    pub items: Vec<String>,
    /// NPC IDs in the room. / ID NPC в комнате.
    pub npcs: Vec<String>,
}

/// Static description of a room. / Статическое описание комнаты.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomInfo {
    /// Canonical ID, e.g. `loc.bakery`. / Канонический ID, например `loc.bakery`.
    pub id: String,
    /// Display name. / Отображаемое имя.
    pub name: String,
    /// Description text. / Текст описания.
    pub description: String,
    /// Direction → room ID. A `BTreeMap` keeps JSON key order stable, which makes tests
    /// deterministic. / Направление → ID комнаты. `BTreeMap` сохраняет порядок ключей в
    /// JSON, поэтому тесты детерминированы.
    pub exits: BTreeMap<String, String>,
}

/// Body of `OK` for `ATTACK` (RFC 5.4.5). / Тело `OK` для `ATTACK` (RFC 5.4.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttackPayload {
    /// Attacker's HP after the exchange. / HP атакующего после обмена ударами.
    pub attacker_hp: i32,
    /// Target's HP after the exchange. / HP цели после обмена ударами.
    pub target_hp: i32,
    /// Damage dealt by this attack. / Урон, нанесённый этой атакой.
    pub damage: i32,
    /// See [`status`]. / См. [`status`].
    pub status: String,
}

/// Body of `OK` for `STATUS` (RFC 5.4.6). / Тело `OK` для `STATUS` (RFC 5.4.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusPayload {
    /// Current HP. / Текущие HP.
    pub hp: i32,
    /// Maximum HP. / Максимум HP.
    pub max_hp: i32,
    /// See [`status`]. / См. [`status`].
    pub status: String,
}

/// Body of `OK` for `QUEST <npc>` (RFC 5.4.7). / Тело `OK` для `QUEST <npc>` (RFC 5.4.7).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestOfferPayload {
    /// Quest ID. / ID квеста.
    pub quest_id: String,
    /// What to do. / Что нужно сделать.
    pub description: String,
    /// Reward (item ID or text). / Награда (ID предмета или текст).
    pub reward: String,
    /// See [`quest_status`]. / См. [`quest_status`].
    pub status: String,
}

/// One element of the `QUESTS` list (RFC 5.4.8). / Элемент списка `QUESTS` (RFC 5.4.8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestProgressPayload {
    /// Quest ID. / ID квеста.
    pub quest_id: String,
    /// See [`quest_status`]. / См. [`quest_status`].
    pub status: String,
    /// E.g. `"1/3"`; omitted for finished quests. / Например `"1/3"`; опускается у
    /// завершённых квестов.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
}

// ---------------------------------------------------------------------------
// Server -> client: events / События сервер -> клиент
// ---------------------------------------------------------------------------

/// Asynchronous `EVT ...` notifications (RFC section 7 plus extensions).
///
/// Names that appear in events must not contain whitespace; for NPCs and items use their
/// canonical IDs, not display names.
///
/// RU: Асинхронные уведомления `EVT ...` (RFC, раздел 7, плюс расширения). Имена в
/// событиях не должны содержать пробелов; для NPC и предметов используйте
/// канонические ID, а не отображаемые имена.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// `EVT ROOM PRESENCE ENTER <user>`
    RoomPresenceEnter { username: String },
    /// `EVT ROOM PRESENCE LEAVE <user>`
    RoomPresenceLeave { username: String },
    /// `EVT ROOM CHAT <user> <message>`
    RoomChat { username: String, message: String },
    /// Extension: `EVT ROOM ITEM TAKE <user> <item-id>` — lets other players in the room
    /// refresh their view. / Расширение: позволяет другим игрокам в комнате обновить вид.
    RoomItemTaken { username: String, item_id: String },
    /// Extension: `EVT ROOM ITEM DROP <user> <item-id>`
    RoomItemDropped { username: String, item_id: String },
    /// `EVT GLOBAL CHAT <user> <message>`
    GlobalChat { username: String, message: String },
    /// `EVT GROUP INVITE <leader>`
    GroupInvite { leader: String },
    /// `EVT GROUP JOIN <user>`
    GroupJoin { username: String },
    /// `EVT GROUP LEAVE <user>`
    GroupLeave { username: String },
    /// `EVT GROUP CHAT <user> <message>`
    GroupChat { username: String, message: String },
    /// `EVT STATS players=<n>`
    StatsPlayers { count: usize },
    /// Extension: `EVT COMBAT START <attacker> <target>`
    CombatStart { attacker: String, target: String },
    /// Extension: `EVT COMBAT HIT <attacker> <target> <damage> <target-hp>`
    CombatHit {
        attacker: String,
        target: String,
        damage: i32,
        target_hp: i32,
    },
    /// Extension: `EVT COMBAT DEFEND <user>`
    CombatDefend { username: String },
    /// Extension: `EVT COMBAT FLEE <user>`
    CombatFlee { username: String },
    /// Extension: `EVT COMBAT DEFEAT <victim> <by>` — victim reached 0 HP.
    /// / жертва достигла 0 HP.
    CombatDefeat { victim: String, by: String },
    /// Extension: `EVT COMBAT RESPAWN <user> <room-id>`
    CombatRespawn { username: String, room: String },
    /// Extension: `EVT QUEST ACCEPTED <quest-id>` (sent only to the player).
    /// / (отправляется только игроку).
    QuestAccepted { quest_id: String },
    /// Extension: `EVT QUEST PROGRESS <quest-id> <progress>`, e.g. `1/3`.
    QuestProgress { quest_id: String, progress: String },
    /// Extension: `EVT QUEST COMPLETE <quest-id> <reward>`
    QuestComplete { quest_id: String, reward: String },
}

impl fmt::Display for ServerEvent {
    /// Wire form without the final `\n`. / Сетевая форма без `\n`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoomPresenceEnter { username } => write!(f, "EVT ROOM PRESENCE ENTER {username}"),
            Self::RoomPresenceLeave { username } => write!(f, "EVT ROOM PRESENCE LEAVE {username}"),
            Self::RoomChat { username, message } => write!(f, "EVT ROOM CHAT {username} {message}"),
            Self::RoomItemTaken { username, item_id } => {
                write!(f, "EVT ROOM ITEM TAKE {username} {item_id}")
            }
            Self::RoomItemDropped { username, item_id } => {
                write!(f, "EVT ROOM ITEM DROP {username} {item_id}")
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
            Self::CombatStart { attacker, target } => {
                write!(f, "EVT COMBAT START {attacker} {target}")
            }
            Self::CombatHit {
                attacker,
                target,
                damage,
                target_hp,
            } => write!(f, "EVT COMBAT HIT {attacker} {target} {damage} {target_hp}"),
            Self::CombatDefend { username } => write!(f, "EVT COMBAT DEFEND {username}"),
            Self::CombatFlee { username } => write!(f, "EVT COMBAT FLEE {username}"),
            Self::CombatDefeat { victim, by } => write!(f, "EVT COMBAT DEFEAT {victim} {by}"),
            Self::CombatRespawn { username, room } => {
                write!(f, "EVT COMBAT RESPAWN {username} {room}")
            }
            Self::QuestAccepted { quest_id } => write!(f, "EVT QUEST ACCEPTED {quest_id}"),
            Self::QuestProgress { quest_id, progress } => {
                write!(f, "EVT QUEST PROGRESS {quest_id} {progress}")
            }
            Self::QuestComplete { quest_id, reward } => {
                write!(f, "EVT QUEST COMPLETE {quest_id} {reward}")
            }
        }
    }
}

/// Exactly `N` whitespace-separated tokens, nothing more, nothing less.
///
/// RU: Ровно `N` токенов, разделённых пробелами, не больше и не меньше.
fn tokens<const N: usize>(rest: &str) -> Result<[&str; N], ProtocolError> {
    let mut parts = rest.split_whitespace();
    let mut out = [""; N];
    for slot in &mut out {
        *slot = parts.next().ok_or(ProtocolError::MalformedReply)?;
    }
    if parts.next().is_some() {
        return Err(ProtocolError::MalformedReply);
    }
    Ok(out)
}

/// One token. / Один токен.
fn single(rest: &str) -> Result<String, ProtocolError> {
    let [a] = tokens::<1>(rest)?;
    Ok(a.to_string())
}

/// `<word> <free text with spaces>`, both required.
///
/// RU: `<слово> <свободный текст с пробелами>`, обе части обязательны.
fn word_and_text(rest: &str) -> Result<(String, String), ProtocolError> {
    let (word, text) = split_first_word(rest);
    if word.is_empty() || text.is_empty() {
        return Err(ProtocolError::MalformedReply);
    }
    Ok((word.to_string(), text.to_string()))
}

/// Parses a number, mapping failure to `MalformedReply`. / Разбор числа.
fn number<T: FromStr>(s: &str) -> Result<T, ProtocolError> {
    s.parse().map_err(|_| ProtocolError::MalformedReply)
}

impl FromStr for ServerEvent {
    type Err = ProtocolError;

    /// Parses a full `EVT ...` line (used by clients). Exact inverse of [`fmt::Display`].
    ///
    /// RU: Разбирает строку `EVT ...` целиком (для клиентов). Точная обратная операция к
    /// [`fmt::Display`].
    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let (evt, rest) = split_first_word(strip_eol(line).trim());
        if evt != "EVT" {
            return Err(ProtocolError::MalformedReply);
        }
        let (category, rest) = split_first_word(rest);
        let (kind, rest) = split_first_word(rest);

        match (category, kind) {
            ("ROOM", "PRESENCE") => {
                let (sub, user) = split_first_word(rest);
                let username = single(user)?;
                match sub {
                    "ENTER" => Ok(Self::RoomPresenceEnter { username }),
                    "LEAVE" => Ok(Self::RoomPresenceLeave { username }),
                    _ => Err(ProtocolError::UnknownEvent),
                }
            }
            ("ROOM", "CHAT") => {
                let (username, message) = word_and_text(rest)?;
                Ok(Self::RoomChat { username, message })
            }
            ("ROOM", "ITEM") => {
                let (sub, args) = split_first_word(rest);
                let [user, item] = tokens::<2>(args)?;
                let (username, item_id) = (user.to_string(), item.to_string());
                match sub {
                    "TAKE" => Ok(Self::RoomItemTaken { username, item_id }),
                    "DROP" => Ok(Self::RoomItemDropped { username, item_id }),
                    _ => Err(ProtocolError::UnknownEvent),
                }
            }
            ("GLOBAL", "CHAT") => {
                let (username, message) = word_and_text(rest)?;
                Ok(Self::GlobalChat { username, message })
            }
            ("GROUP", "INVITE") => Ok(Self::GroupInvite {
                leader: single(rest)?,
            }),
            ("GROUP", "JOIN") => Ok(Self::GroupJoin {
                username: single(rest)?,
            }),
            ("GROUP", "LEAVE") => Ok(Self::GroupLeave {
                username: single(rest)?,
            }),
            ("GROUP", "CHAT") => {
                let (username, message) = word_and_text(rest)?;
                Ok(Self::GroupChat { username, message })
            }
            ("STATS", _) => {
                // Here `kind` holds "players=<n>" because STATS has no sub-type.
                // RU: здесь в `kind` лежит "players=<n>", так как у STATS нет подтипа.
                let value = kind
                    .strip_prefix("players=")
                    .ok_or(ProtocolError::MalformedReply)?;
                if !rest.is_empty() {
                    return Err(ProtocolError::MalformedReply);
                }
                Ok(Self::StatsPlayers {
                    count: number(value)?,
                })
            }
            ("COMBAT", "START") => {
                let [attacker, target] = tokens::<2>(rest)?;
                Ok(Self::CombatStart {
                    attacker: attacker.to_string(),
                    target: target.to_string(),
                })
            }
            ("COMBAT", "HIT") => {
                let [attacker, target, damage, target_hp] = tokens::<4>(rest)?;
                Ok(Self::CombatHit {
                    attacker: attacker.to_string(),
                    target: target.to_string(),
                    damage: number(damage)?,
                    target_hp: number(target_hp)?,
                })
            }
            ("COMBAT", "DEFEND") => Ok(Self::CombatDefend {
                username: single(rest)?,
            }),
            ("COMBAT", "FLEE") => Ok(Self::CombatFlee {
                username: single(rest)?,
            }),
            ("COMBAT", "DEFEAT") => {
                let [victim, by] = tokens::<2>(rest)?;
                Ok(Self::CombatDefeat {
                    victim: victim.to_string(),
                    by: by.to_string(),
                })
            }
            ("COMBAT", "RESPAWN") => {
                let [username, room] = tokens::<2>(rest)?;
                Ok(Self::CombatRespawn {
                    username: username.to_string(),
                    room: room.to_string(),
                })
            }
            ("QUEST", "ACCEPTED") => Ok(Self::QuestAccepted {
                quest_id: single(rest)?,
            }),
            ("QUEST", "PROGRESS") => {
                let [quest_id, progress] = tokens::<2>(rest)?;
                Ok(Self::QuestProgress {
                    quest_id: quest_id.to_string(),
                    progress: progress.to_string(),
                })
            }
            ("QUEST", "COMPLETE") => {
                let (quest_id, reward) = word_and_text(rest)?;
                Ok(Self::QuestComplete { quest_id, reward })
            }
            _ => Err(ProtocolError::UnknownEvent),
        }
    }
}

// ---------------------------------------------------------------------------
// Server -> client: replies / Ответы сервер -> клиент
// ---------------------------------------------------------------------------

/// Synchronous answer to one command: `OK [payload]` or `ERR <code> <message>`.
///
/// RU: Синхронный ответ на одну команду: `OK [данные]` или `ERR <код> <сообщение>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerReply {
    /// Success with optional payload (plain text, `key=value` or JSON).
    /// / Успех с необязательными данными (текст, `key=value` или JSON).
    Ok(Option<String>),
    /// Failure. / Ошибка.
    Err { code: u16, message: String },
}

impl ServerReply {
    /// `OK hello proto=1` — greeting sent on connect. / приветствие при подключении.
    #[must_use]
    pub fn hello() -> Self {
        Self::Ok(Some(format!("hello proto={PROTOCOL_VERSION}")))
    }

    /// `OK` without payload. / `OK` без данных.
    #[must_use]
    pub const fn ok() -> Self {
        Self::Ok(None)
    }

    /// `OK <payload>`.
    #[must_use]
    pub fn ok_with(payload: impl Into<String>) -> Self {
        Self::Ok(Some(payload.into()))
    }

    /// `OK <json>` from any serializable value; falls back to `ERR 500` if serialization
    /// fails (which cannot happen for the payload types of this crate).
    ///
    /// RU: `OK <json>` из любого сериализуемого значения; при сбое сериализации возвращает
    /// `ERR 500` (для типов этого крейта это невозможно).
    #[must_use]
    pub fn ok_json<T: Serialize>(value: &T) -> Self {
        match serde_json::to_string(value) {
            Ok(json) => Self::Ok(Some(json)),
            Err(_) => Self::from(ErrorKind::InternalError),
        }
    }

    /// `ERR <code> <message>` with a free-form message (prefer `ServerReply::from(kind)`).
    ///
    /// RU: `ERR <код> <сообщение>` со свободным сообщением (лучше `ServerReply::from(kind)`).
    #[must_use]
    pub fn err(code: RfcErrorCode, message: impl Into<String>) -> Self {
        Self::Err {
            code: code.as_u16(),
            message: message.into(),
        }
    }

    /// `OK connected`
    #[must_use]
    pub fn connected() -> Self {
        Self::ok_with("connected")
    }

    /// `OK bye`
    #[must_use]
    pub fn bye() -> Self {
        Self::ok_with("bye")
    }

    /// `OK room=<room-id>` — answer to a successful `MOVE`.
    #[must_use]
    pub fn moved(room_id: &str) -> Self {
        Self::ok_with(format!("room={room_id}"))
    }

    /// `OK taken=<item-id>`
    #[must_use]
    pub fn taken(item_id: &str) -> Self {
        Self::ok_with(format!("taken={item_id}"))
    }

    /// `OK dropped=<item-id>`
    #[must_use]
    pub fn dropped(item_id: &str) -> Self {
        Self::ok_with(format!("dropped={item_id}"))
    }

    /// `OK players=<count>` — answer to `WHO` (RFC 5.2.2).
    #[must_use]
    pub fn who(count: usize) -> Self {
        Self::ok_with(format!("players={count}"))
    }

    /// `OK group=<group-id>`
    #[must_use]
    pub fn group(group_id: &str) -> Self {
        Self::ok_with(format!("group={group_id}"))
    }

    /// Is this an `OK` reply? / Это ответ `OK`?
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    /// The text after `OK`, if any. / Текст после `OK`, если есть.
    #[must_use]
    pub fn payload(&self) -> Option<&str> {
        match self {
            Self::Ok(Some(p)) => Some(p.as_str()),
            _ => None,
        }
    }

    /// For payloads like `room=loc.bakery`, returns the value of `key`.
    ///
    /// RU: Для данных вида `room=loc.bakery` возвращает значение ключа `key`.
    #[must_use]
    pub fn value_of(&self, key: &str) -> Option<&str> {
        self.payload()?
            .strip_prefix(key)
            .and_then(|tail| tail.strip_prefix('='))
    }

    /// Deserializes the JSON payload (clients).
    ///
    /// RU: Десериализует JSON-данные (для клиентов).
    ///
    /// # Errors
    /// `MalformedReply` if the reply is `ERR`, has no payload, or the JSON is invalid.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, ProtocolError> {
        let payload = self.payload().ok_or(ProtocolError::MalformedReply)?;
        serde_json::from_str(payload).map_err(|_| ProtocolError::MalformedReply)
    }
}

impl From<ErrorKind> for ServerReply {
    /// Builds `ERR <code> <NAME>` from an [`ErrorKind`]. / Строит `ERR <код> <ИМЯ>`.
    fn from(kind: ErrorKind) -> Self {
        Self::err(kind.code(), kind.name())
    }
}

impl From<ProtocolError> for ServerReply {
    /// Turns a parse failure into the reply to send, e.g. `ERR 400 MISSING_ARGUMENT`.
    ///
    /// RU: Превращает ошибку разбора в ответ, например `ERR 400 MISSING_ARGUMENT`.
    fn from(error: ProtocolError) -> Self {
        Self::err(error.code(), error.to_string())
    }
}

impl fmt::Display for ServerReply {
    /// Wire form without the final `\n`. / Сетевая форма без `\n`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok(Some(payload)) => write!(f, "OK {payload}"),
            Self::Ok(None) => write!(f, "OK"),
            Self::Err { code, message } => write!(f, "ERR {code} {message}"),
        }
    }
}

impl FromStr for ServerReply {
    type Err = ProtocolError;

    /// Parses an `OK ...` / `ERR ...` line (used by clients).
    ///
    /// RU: Разбирает строку `OK ...` / `ERR ...` (для клиентов).
    ///
    /// Note: `Self::Err` would be ambiguous with the `Err` enum variant, hence the
    /// explicit error type. / `Self::Err` двусмысленно из-за варианта `Err`, поэтому
    /// тип ошибки указан явно.
    fn from_str(line: &str) -> Result<Self, ProtocolError> {
        let line = strip_eol(line).trim();
        let (head, rest) = split_first_word(line);
        match head {
            "OK" if rest.is_empty() => Ok(Self::Ok(None)),
            "OK" => Ok(Self::Ok(Some(rest.to_string()))),
            "ERR" => {
                let (code, message) = split_first_word(rest);
                Ok(Self::Err {
                    code: number(code)?,
                    message: message.to_string(),
                })
            }
            _ => Err(ProtocolError::MalformedReply),
        }
    }
}

// ---------------------------------------------------------------------------
// Any server line / Любая строка сервера
// ---------------------------------------------------------------------------

/// Anything the server can send: a reply or an event.
///
/// RU: Всё, что может отправить сервер: ответ или событие.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    /// Answer to a command. / Ответ на команду.
    Reply(ServerReply),
    /// Asynchronous notification. / Асинхронное уведомление.
    Event(ServerEvent),
}

impl ServerMessage {
    /// The line to write to the socket, including the final `\n`.
    ///
    /// RU: Строка для записи в сокет, с завершающим `\n`.
    #[must_use]
    pub fn to_wire_line(&self) -> String {
        format!("{self}\n")
    }
}

impl From<ServerReply> for ServerMessage {
    fn from(reply: ServerReply) -> Self {
        Self::Reply(reply)
    }
}

impl From<ServerEvent> for ServerMessage {
    fn from(event: ServerEvent) -> Self {
        Self::Event(event)
    }
}

impl fmt::Display for ServerMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reply(reply) => write!(f, "{reply}"),
            Self::Event(event) => write!(f, "{event}"),
        }
    }
}

impl FromStr for ServerMessage {
    type Err = ProtocolError;

    /// Decides between event and reply by the first word, then parses. Clients call this
    /// on every received line.
    ///
    /// RU: Выбирает между событием и ответом по первому слову и разбирает строку.
    /// Клиенты вызывают это для каждой полученной строки.
    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let trimmed = strip_eol(line).trim();
        if split_first_word(trimmed).0 == "EVT" {
            trimmed.parse().map(Self::Event)
        } else {
            trimmed.parse().map(Self::Reply)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests / Тесты
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Command, ProtocolError> {
        line.parse()
    }

    fn s(text: &str) -> String {
        text.to_string()
    }

    /// One example of every command. / По одному примеру каждой команды.
    fn all_commands() -> Vec<Command> {
        vec![
            Command::Connect {
                username: s("alice"),
            },
            Command::Look,
            Command::Move {
                direction: s("north"),
            },
            Command::Chat {
                scope: ChatScope::Global,
                message: s("Hello  everyone"),
            },
            Command::Chat {
                scope: ChatScope::Room,
                message: s("psst"),
            },
            Command::Chat {
                scope: ChatScope::Group,
                message: s("go!"),
            },
            Command::Take {
                item_ref: s("Healing Potion"),
            },
            Command::Drop {
                item_ref: s("item.herbs"),
            },
            Command::Inventory,
            Command::Talk {
                npc_ref: s("Village Guard"),
            },
            Command::Attack {
                target: s("goblin"),
            },
            Command::Status,
            Command::Quest {
                npc_ref: s("merchant"),
            },
            Command::Quests,
            Command::Who,
            Command::Group(GroupAction::Create),
            Command::Group(GroupAction::Invite { username: s("bob") }),
            Command::Group(GroupAction::Join { leader: s("alice") }),
            Command::Group(GroupAction::Leave),
            Command::Defend,
            Command::Flee,
            Command::Quit,
        ]
    }

    fn all_events() -> Vec<ServerEvent> {
        vec![
            ServerEvent::RoomPresenceEnter {
                username: s("alice"),
            },
            ServerEvent::RoomPresenceLeave {
                username: s("alice"),
            },
            ServerEvent::RoomChat {
                username: s("alice"),
                message: s("hi there  friend"),
            },
            ServerEvent::RoomItemTaken {
                username: s("alice"),
                item_id: s("item.herbs"),
            },
            ServerEvent::RoomItemDropped {
                username: s("alice"),
                item_id: s("item.herbs"),
            },
            ServerEvent::GlobalChat {
                username: s("bob"),
                message: s("Привет, мир!"),
            },
            ServerEvent::GroupInvite { leader: s("alice") },
            ServerEvent::GroupJoin { username: s("bob") },
            ServerEvent::GroupLeave { username: s("bob") },
            ServerEvent::GroupChat {
                username: s("bob"),
                message: s("on my way"),
            },
            ServerEvent::StatsPlayers { count: 4 },
            ServerEvent::CombatStart {
                attacker: s("alice"),
                target: s("npc.goblin"),
            },
            ServerEvent::CombatHit {
                attacker: s("alice"),
                target: s("npc.goblin"),
                damage: 10,
                target_hp: 15,
            },
            ServerEvent::CombatDefend {
                username: s("alice"),
            },
            ServerEvent::CombatFlee {
                username: s("alice"),
            },
            ServerEvent::CombatDefeat {
                victim: s("npc.goblin"),
                by: s("alice"),
            },
            ServerEvent::CombatRespawn {
                username: s("alice"),
                room: s("loc.square"),
            },
            ServerEvent::QuestAccepted {
                quest_id: s("fetch_herbs"),
            },
            ServerEvent::QuestProgress {
                quest_id: s("fetch_herbs"),
                progress: s("1/3"),
            },
            ServerEvent::QuestComplete {
                quest_id: s("fetch_herbs"),
                reward: s("gold coin"),
            },
        ]
    }

    #[test]
    fn every_command_round_trips() {
        for command in all_commands() {
            let line = command.to_wire_line();
            assert!(line.ends_with('\n'));
            assert_eq!(parse(&line).unwrap(), command, "line: {line:?}");
        }
    }

    #[test]
    fn every_event_round_trips() {
        for event in all_events() {
            let wire = ServerMessage::Event(event.clone()).to_wire_line();
            assert_eq!(
                wire.parse::<ServerEvent>().unwrap(),
                event,
                "line: {wire:?}"
            );
            assert_eq!(
                wire.parse::<ServerMessage>().unwrap(),
                ServerMessage::Event(event)
            );
        }
    }

    #[test]
    fn verbs_are_case_insensitive_and_crlf_is_accepted() {
        assert_eq!(parse("look\r\n").unwrap(), Command::Look);
        assert_eq!(parse("  LoOk  ").unwrap(), Command::Look);
        assert_eq!(
            parse("move NORTH").unwrap(),
            Command::Move {
                direction: s("north")
            }
        );
        assert_eq!(
            parse("chat room hola").unwrap(),
            Command::Chat {
                scope: ChatScope::Room,
                message: s("hola")
            }
        );
        assert_eq!(
            parse("group invite Bob").unwrap(),
            Command::Group(GroupAction::Invite { username: s("Bob") })
        );
    }

    #[test]
    fn multi_word_references_are_kept_whole() {
        assert_eq!(
            parse("TAKE Healing Potion").unwrap(),
            Command::Take {
                item_ref: s("Healing Potion")
            }
        );
        assert_eq!(
            parse("TALK Old  Wise Man").unwrap(),
            Command::Talk {
                npc_ref: s("Old  Wise Man")
            }
        );
    }

    #[test]
    fn malformed_commands_are_rejected() {
        assert_eq!(parse("").unwrap_err(), ProtocolError::EmptyCommand);
        assert_eq!(parse("   \r\n").unwrap_err(), ProtocolError::EmptyCommand);
        assert_eq!(parse("DANCE").unwrap_err(), ProtocolError::UnknownCommand);
        assert_eq!(parse("MOVE").unwrap_err(), ProtocolError::MissingArgument);
        assert_eq!(parse("TAKE ").unwrap_err(), ProtocolError::MissingArgument);
        assert_eq!(parse("CHAT").unwrap_err(), ProtocolError::MissingArgument);
        assert_eq!(
            parse("CHAT GLOBAL").unwrap_err(),
            ProtocolError::MissingArgument
        );
        assert_eq!(
            parse("CHAT MARS hi").unwrap_err(),
            ProtocolError::InvalidArgument
        );
        assert_eq!(parse("GROUP").unwrap_err(), ProtocolError::MissingArgument);
        assert_eq!(
            parse("GROUP INVITE").unwrap_err(),
            ProtocolError::MissingArgument
        );
        assert_eq!(
            parse("GROUP DANCE").unwrap_err(),
            ProtocolError::InvalidArgument
        );
        assert_eq!(
            parse("CONNECT").unwrap_err(),
            ProtocolError::MissingArgument
        );
        assert_eq!(
            parse("CONNECT al ice").unwrap_err(),
            ProtocolError::InvalidArgument
        );
    }

    #[test]
    fn long_lines_and_control_characters_are_rejected() {
        let long = format!("CHAT GLOBAL {}", "x".repeat(MAX_LINE_LENGTH));
        assert_eq!(parse(&long).unwrap_err(), ProtocolError::LineTooLong);
        assert_eq!(ProtocolError::LineTooLong.code(), RfcErrorCode::LineTooLong);

        let exactly = format!("CHAT GLOBAL {}", "x".repeat(MAX_LINE_LENGTH - 12));
        assert_eq!(exactly.len(), MAX_LINE_LENGTH);
        assert!(parse(&exactly).is_ok());

        assert_eq!(
            parse("CHAT GLOBAL hi\x1b[31m").unwrap_err(),
            ProtocolError::ControlCharacter
        );
        assert_eq!(
            ProtocolError::ControlCharacter.code(),
            RfcErrorCode::BadRequest
        );
    }

    #[test]
    fn usernames_support_unicode_but_not_junk() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("Алиса_99").is_ok());
        assert!(validate_username("勇者").is_ok());
        assert_eq!(validate_username(""), Err(ProtocolError::MissingArgument));
        assert_eq!(
            validate_username("a b"),
            Err(ProtocolError::InvalidArgument)
        );
        assert_eq!(
            validate_username("a\u{7}b"),
            Err(ProtocolError::InvalidArgument)
        );
        let long = "x".repeat(MAX_USERNAME_LENGTH + 1);
        assert_eq!(
            validate_username(&long),
            Err(ProtocolError::InvalidArgument)
        );
        assert!(validate_username(&"я".repeat(MAX_USERNAME_LENGTH)).is_ok());
    }

    #[test]
    fn reference_matching_accepts_id_name_and_variants() {
        let (id, name) = ("item.healing_potion", "Healing Potion");
        assert!(matches_reference("item.healing_potion", id, name));
        assert!(matches_reference("ITEM.HEALING_POTION", id, name));
        assert!(matches_reference("healing_potion", id, name));
        assert!(matches_reference("healing potion", id, name));
        assert!(matches_reference("  Healing   Potion ", id, name));
        assert!(!matches_reference("potion", id, name));
        assert!(!matches_reference("", id, name));
        assert!(matches_reference("Herbs", "item.herbs", "Herbs"));
    }

    #[test]
    fn standard_errors_match_the_rfc_table() {
        let table = [
            (ErrorKind::NameInUse, "ERR 201 NAME_IN_USE"),
            (ErrorKind::NoExit, "ERR 301 NO_EXIT"),
            (ErrorKind::NotInGroup, "ERR 401 NOT_IN_GROUP"),
            (ErrorKind::AlreadyInGroup, "ERR 402 ALREADY_IN_GROUP"),
            (ErrorKind::ItemNotFound, "ERR 404 ITEM_NOT_FOUND"),
            (
                ErrorKind::ItemNotInInventory,
                "ERR 404 ITEM_NOT_IN_INVENTORY",
            ),
            (ErrorKind::NpcNotFound, "ERR 404 NPC_NOT_FOUND"),
            (ErrorKind::NpcNotHostile, "ERR 405 NPC_NOT_HOSTILE"),
            (ErrorKind::NoQuestAvailable, "ERR 406 NO_QUEST_AVAILABLE"),
            (ErrorKind::ConnectionFailed, "ERR 900 CONNECTION_FAILED"),
            (ErrorKind::SendFailed, "ERR 901 SEND_FAILED"),
        ];
        for (kind, expected) in table {
            assert_eq!(ServerReply::from(kind).to_string(), expected);
        }
    }

    #[test]
    fn parse_errors_become_replies() {
        let reply = ServerReply::from(ProtocolError::MissingArgument);
        assert_eq!(reply.to_string(), "ERR 400 MISSING_ARGUMENT");
        let reply = ServerReply::from(ProtocolError::LineTooLong);
        assert_eq!(reply.to_string(), "ERR 413 LINE_TOO_LONG");
    }

    #[test]
    fn replies_format_and_parse() {
        assert_eq!(ServerReply::hello().to_string(), "OK hello proto=1");
        assert_eq!(ServerReply::ok().to_string(), "OK");
        assert_eq!(ServerReply::connected().to_string(), "OK connected");
        assert_eq!(
            ServerReply::moved("loc.bakery").to_string(),
            "OK room=loc.bakery"
        );
        assert_eq!(
            ServerReply::taken("item.herbs").to_string(),
            "OK taken=item.herbs"
        );
        assert_eq!(
            ServerReply::dropped("item.herbs").to_string(),
            "OK dropped=item.herbs"
        );
        assert_eq!(ServerReply::who(5).to_string(), "OK players=5");
        assert_eq!(ServerReply::group("g1").to_string(), "OK group=g1");
        assert_eq!(ServerReply::bye().to_string(), "OK bye");

        for reply in [
            ServerReply::hello(),
            ServerReply::ok(),
            ServerReply::ok_with("Welcome to my bakery!"),
            ServerReply::from(ErrorKind::NameInUse),
        ] {
            let wire = ServerMessage::Reply(reply.clone()).to_wire_line();
            assert_eq!(wire.parse::<ServerReply>().unwrap(), reply);
            assert_eq!(
                wire.parse::<ServerMessage>().unwrap(),
                ServerMessage::Reply(reply)
            );
        }
    }

    #[test]
    fn reply_helpers_for_clients() {
        let reply = ServerReply::moved("loc.bakery");
        assert!(reply.is_ok());
        assert_eq!(reply.value_of("room"), Some("loc.bakery"));
        assert_eq!(reply.value_of("taken"), None);
        assert_eq!(ServerReply::ok().payload(), None);
        assert!(!ServerReply::from(ErrorKind::NoExit).is_ok());
    }

    #[test]
    fn look_payload_matches_the_rfc_example() {
        let mut exits = BTreeMap::new();
        exits.insert(s("south"), s("loc.square"));
        let look = LookPayload {
            room: RoomInfo {
                id: s("loc.bakery"),
                name: s("Village Bakery"),
                description: s("A warm bakery filled with the scent of fresh bread."),
                exits,
            },
            players: vec![s("alice")],
            items: vec![s("item.herbs")],
            npcs: vec![s("npc.baker")],
        };
        let reply = ServerReply::ok_json(&look);
        let wire = reply.to_string();
        assert!(wire.starts_with("OK {"));
        assert!(wire.contains(r#""exits":{"south":"loc.square"}"#));
        assert!(!wire.contains('\n'));

        let parsed: ServerReply = wire.parse().unwrap();
        assert_eq!(parsed.json::<LookPayload>().unwrap(), look);
    }

    #[test]
    fn rfc_json_examples_deserialize() {
        let attack: AttackPayload = serde_json::from_str(
            r#"{"attacker_hp": 80, "target_hp": 15, "damage": 10, "status": "combat"}"#,
        )
        .unwrap();
        assert_eq!(attack.damage, 10);

        let status: StatusPayload =
            serde_json::from_str(r#"{"hp": 80, "max_hp": 100, "status": "healthy"}"#).unwrap();
        assert_eq!(status.max_hp, 100);

        let offer: QuestOfferPayload = serde_json::from_str(
            r#"{"quest_id": "fetch_herbs", "description": "Bring me 3 healing herbs", "reward": "gold_coin", "status": "available"}"#,
        )
        .unwrap();
        assert_eq!(offer.status, quest_status::AVAILABLE);

        let quests: Vec<QuestProgressPayload> = serde_json::from_str(
            r#"[{"quest_id": "fetch_herbs", "status": "active", "progress": "1/3"}, {"quest_id": "defeat_goblin", "status": "completed"}]"#,
        )
        .unwrap();
        assert_eq!(quests[0].progress.as_deref(), Some("1/3"));
        assert_eq!(quests[1].progress, None);

        // `progress` must be omitted, not `null`, when absent.
        // RU: `progress` должен отсутствовать, а не быть `null`.
        let json = serde_json::to_string(&quests[1]).unwrap();
        assert!(!json.contains("progress"));
    }

    #[test]
    fn rfc_example_session_lines_parse() {
        assert_eq!(
            "OK hello proto=1".parse::<ServerMessage>().unwrap(),
            ServerMessage::Reply(ServerReply::hello())
        );
        assert_eq!(
            "ERR 201 NAME_IN_USE".parse::<ServerMessage>().unwrap(),
            ServerMessage::Reply(ServerReply::Err {
                code: 201,
                message: s("NAME_IN_USE")
            })
        );
        assert_eq!(
            "EVT STATS players=4".parse::<ServerMessage>().unwrap(),
            ServerMessage::Event(ServerEvent::StatsPlayers { count: 4 })
        );
        assert_eq!(
            "EVT GLOBAL CHAT alice Hello everyone"
                .parse::<ServerMessage>()
                .unwrap(),
            ServerMessage::Event(ServerEvent::GlobalChat {
                username: s("alice"),
                message: s("Hello everyone")
            })
        );
    }

    #[test]
    fn garbage_from_the_server_is_an_error_not_a_panic() {
        assert!("".parse::<ServerMessage>().is_err());
        assert!("HELLO".parse::<ServerMessage>().is_err());
        assert!("ERR abc oops".parse::<ServerReply>().is_err());
        assert_eq!(
            "EVT NOPE THING x".parse::<ServerEvent>().unwrap_err(),
            ProtocolError::UnknownEvent
        );
        assert_eq!(
            "EVT COMBAT HIT a b ten 5"
                .parse::<ServerEvent>()
                .unwrap_err(),
            ProtocolError::MalformedReply
        );
        assert_eq!(
            "EVT STATS players=x".parse::<ServerEvent>().unwrap_err(),
            ProtocolError::MalformedReply
        );
        assert!("EVT ROOM PRESENCE ENTER".parse::<ServerEvent>().is_err());
    }
}
