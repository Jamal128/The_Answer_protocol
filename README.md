
*This project has been created as part of the 42 curriculum by jaatif-a, dzhambal.*

# TAP — The Answer Protocol (Arrakis Edition)

A real-time multiplayer Multi-User Dungeon (MUD) engine, CLI client, and graphical tactical terminal built in **Rust**, implementing **RFC 42TAP** over line-framed TCP sockets and set in the desert world of **Arrakis (Dune)**.

---

## Description

**The Answer Protocol (TAP)** is a shared-world, real-time multiplayer text adventure inspired by classic university MUDs. Multiple players connect concurrently over TCP to explore an interconnected map of Arrakis, scavenge unique equipment, converse with Fremen and Atreides characters, form squads, accept contracts, and engage in turn-based combat against Harkonnen raiders and sandworms.

The repository is organized as a **4-crate Rust Cargo Workspace**:
* **`protocol`**: Pure, zero-I/O implementation of the **RFC 42TAP** wire specification (ABNF command parser, reply/event serializers, JSON payloads, and error definitions).
* **`server`**: Asynchronous multi-client TCP server powered by `tokio`, featuring YAML world validation, unique instance item tracking, turn-based combat, quest progression, group management, anti-flooding protection, and structured JSON logging.
* **`cli`**: Non-blocking terminal client using `tokio::select!` to render real-time asynchronous server events while awaiting user input.
* **`gui`**: Native graphical client built with `eframe`/`egui` featuring a dark Modern-MUD aesthetic, procedural vector character portraits, an interactive fog-of-war cartographic radar with travel trails, separated chat channels, and a color-coded telemetry/combat log.

---

## Instructions

### Prerequisites
* **Rust Toolchain** (`rustc` and `cargo`, edition 2021+, with `clippy` and `rustfmt`)
* **GNU Make**
* *(Linux GUI)* Standard X11/Wayland and OpenGL development libraries required by `eframe`/`winit` (pre-installed on 42 Ubuntu workstations).

### Quick Start
1. **Install dependencies and build the workspace:**
   ```bash
   make install

```

2. **Start the game server (defaults to `127.0.0.1:4242`):**
```bash
make run-server PORT=4243

```


3. **Launch the Graphical Client (in a second terminal):**
```bash
make run-client-gui PORT=4243

```


4. **Or launch the CLI Client (in another terminal):**
```bash
make run-client PORT=4243

```



---

## Architecture

### Workspace Layout

```text
The_Answer_protocol/
├── Cargo.toml                  # Shared workspace dependencies & resolver
├── Makefile                    # Mandatory build, run, lint, test, and clean targets
├── data/
│   └── world.yaml              # Arrakis map, items, NPCs, and quest definitions
├── crates/
│   ├── protocol/src/lib.rs     # RFC 42TAP parser, serializer & JSON payload schemas
│   ├── server/src/
│   │   ├── main.rs             # TCP listener, per-client reader/writer tasks & JSON logger
│   │   ├── state.rs            # Authoritative GameState, combat, quests, groups & rate-limiter
│   │   └── world.rs            # YAML loader & startup referential integrity validator
│   ├── cli/src/main.rs         # Async terminal client (tokio::select!)
│   └── gui/src/
│       ├── main.rs             # Application state & network event dispatcher
│       ├── net.rs              # Background Tokio TCP worker bridged via mpsc channels
│       ├── ui.rs               # Multi-panel HUD, inventory, quests, entity cards & comms
│       ├── radar.rs            # Interactive cartographic world map & movement trail
│       ├── avatars.rs          # Procedural vector portraits by entity archetype
│       └── theme.rs            # Arrakis dark palette & custom RPG widgets
└── README.md

```

### Server Concurrency & Routing Model

* **Hybrid Task-per-Connection + Centralized State (`Arc<RwLock<GameState>>`):**
* Each accepted `TcpStream` is split into independent read and write halves (`into_split()`).
* A dedicated **outbound writer task** reads from a per-client `tokio::sync::mpsc::unbounded_channel<ServerMessage>`. This decouples slow or abruptly disconnected sockets from global and room broadcasts—if one client drops mid-send, `tx.send()` fails locally without blocking or stalling other connected players.
* Inbound lines are buffered via `tokio::io::BufReader::read_line()`, properly handling TCP packet fragmentation and coalescing across `\n` boundaries.


* **Command Dispatching:**
* Parsed `protocol::Command` variants are routed through an explicit match-based dispatcher in `GameState::handle_command`. Pre-authentication connections are restricted exclusively to `CONNECT <username>` and `QUIT` (rejecting all other commands with `ERR 403 NOT_AUTHENTICATED`).


* **Graceful Disconnect Ordering:**
* When a client sends `QUIT` or hits EOF/socket error, `GameState::disconnect_player` removes the player from `players` and any active `GroupState` **before** broadcasting `EVT ROOM PRESENCE LEAVE <username>` and `EVT STATS players=<count>`, preventing ghost entries in `LOOK` or `WHO`.



### Client Responsiveness Model

* **CLI Client:** Uses `tokio::select!` to multiplex `tokio::io::stdin()` and the TCP `BufReader` concurrently on a single async runtime, printing incoming `EVT` lines immediately without interrupting user typing.
* **GUI Client:** Runs `eframe` on the main OS UI thread while a dedicated background OS thread hosts a Tokio current-thread runtime (`crates/gui/src/net.rs`). Bidirectional `std::sync::mpsc` channels plus `egui::Context::request_repaint()` guarantee instant UI updates whenever network events arrive.

---

## Protocol Implementation

The implementation strictly adheres to **RFC 42TAP** (`OK hello proto=1`, UTF-8 encoding, `\n` line termination, case-insensitive command verbs, canonical error codes `201`, `301`, `401`, `402`, `404`, `405`, `406`, `900`, `901`, and standard `EVT` formats), ensuring cross-group client/server interoperability.

### Documented Extensions & Justifications

Because RFC 42TAP Section 6.1 intentionally leaves combat mechanics, quest progression, and certain edge-case errors open to team design, we added the following backwards-compatible extensions:

| Extension | Wire Format | Justification |
| --- | --- | --- |
| **Combat Stance Command** | `DEFEND` | Gives players a tactical defensive turn option in turn-based combat (`+5 HP` recovery and 50% incoming damage reduction). |
| **Combat Disengage Command** | `FLEE` | Allows players to cleanly exit an active combat state (`in_combat_with = None`) without disconnecting. |
| **Room Item Events** | `EVT ROOM ITEM TAKE <user> <item-id>`<br>

<br>`EVT ROOM ITEM DROP <user> <item-id>` | Allows co-located clients (especially the GUI) to refresh room loot automatically when another player picks up or drops an item. |
| **Combat Broadcast Events** | `EVT COMBAT START|HIT|DEFEND|FLEE|DEFEAT|RESPAWN ...` | Provides real-time multiplayer visibility of fights and respawns occurring in the same room (RFC Section 5 / Chapter V.1). |
| **Quest Progress Events** | `EVT QUEST ACCEPTED|PROGRESS|COMPLETE ...` | Notifies the player asynchronously when an objective advances or finishes after a `TAKE` or combat victory. |
| **Extended Error Codes** | `400 BAD_REQUEST`, `403 NOT_AUTHENTICATED`, `409 ALREADY_CONNECTED`, `410 NOT_IN_COMBAT`, `413 LINE_TOO_LONG`, `429 RATE_LIMITED`, `500 INTERNAL_ERROR` | Distinguishes malformed syntax, pre-login commands, duplicate `CONNECT`, out-of-combat `DEFEND`/`FLEE`, oversized frames (`>1024` bytes), and anti-flooding throttling from standard 404 errors. |

---

## Combat System

Combat is **turn-based** and initiated whenever a player executes `ATTACK <npc>` against an NPC with `role: enemy` in the same room:

1. **Player Stats & Damage Formula:**
* Players spawn with `100 / 100 HP` and a base attack power of **`10`**.
* Carrying weapons in the inventory adds passive attack bonuses (`item.crysknife`: `+8`, `item.maker_hooks`: `+5`, `item.thumper`: `+2`).
* $\text{Player Damage} = 10 + \sum \text{attack\_bonus}_{\text{inventory}}$


2. **Initiative & Turn Resolution:**
* **Player acts first** on `ATTACK <npc>`, dealing $\text{Player Damage}$ immediately to the NPC's current HP.
* **Enemy Victory:** If the NPC reaches `0 HP`, combat ends (`status: "victory"`), `EVT COMBAT DEFEAT` is broadcast to the room, defeat-based quests are evaluated, and the NPC's HP resets in the shared world so other players can also complete contracts.
* **Enemy Counterattack:** If the NPC survives (`target_hp > 0`), it immediately counterattacks in the same turn for its configured `damage` (`Harkonnen Scout`: `10`, `Shai-Hulud`: `18`).


3. **Tactical Commands (`DEFEND` & `FLEE`):**
* `DEFEND`: Restores `+5 HP` (capped at `100`) and sets a defensive guard flag that halves the next incoming counterattack ($\max(1, \lfloor \text{damage} / 2 \rfloor)$).
* `FLEE`: Clears active combat status (`status` returns to `healthy` or `wounded`).


4. **Death & Respawn (`0 HP`):**
* If a counterattack reduces a player to `0 HP`, they automatically respawn at the Fremen sanctuary (`loc.sietch_tabr`) with **`50 HP`**, and `EVT COMBAT RESPAWN` plus room presence events are broadcast.



---

## Quest System

Quests are defined declaratively in `data/world.yaml` and tracked per-player in `PlayerSession::quests`:

* **Discovery & Acceptance (`QUEST <npc>`):**
* Executing `QUEST <npc>` on a `quest_giver` NPC in the current room registers the quest as `active` (`progress: 0`) and returns the RFC JSON offer payload (`quest_id`, `description`, `reward`, `status: "available"`).
* Requesting an already active or completed quest returns `ERR 406 NO_QUEST_AVAILABLE`.


* **Objective Types & Automatic Validation:**
* **`collect_item` (`quest.spice_harvest`):** Validated immediately upon accepting the quest and after every `TAKE` command by counting matching `target` items (`item.raw_melange`) in the player's inventory.
* **`defeat_npc` (`quest.harkonnen_raid`):** Validated automatically when a player deals the finishing blow to the target NPC (`npc.harkonnen_scout`).


* **Reward Distribution & Tracking (`QUESTS`):**
* Upon reaching `required_count`, the quest transitions to `completed`, the `reward_item` (`item.guild_solari` or `item.maker_hooks`) is automatically added to the player's inventory, and `EVT QUEST COMPLETE <quest-id> <reward>` is pushed to the client.
* `QUESTS` returns a JSON array of all active (with `"progress": "x/y"`) and completed contracts.



---

## World Design

The game world (`data/world.yaml`) is modeled after **Arrakis (Dune)** and validated at server startup (`WorldData::validate`) to guarantee referential integrity across all exits, items, NPCs, and quests.

### Topology (9 Interconnected Rooms)

The map features a **4-room central circuit loop** inside Arrakeen plus **two optional exploration branches** (the Cantina and the Deep Desert / Sietch Tabr / Sandworm Pit):

![Captura de pantalla](map-example.png)

### NPCs & Roles (7 NPCs across 3 Distinct Roles)

| ID | Name | Role | Location | Function |
| --- | --- | --- | --- | --- |
| `npc.atreides_guard` | Atreides Trooper | `dialogue` | `loc.arrakeen_gate` | Lore & desert warnings |
| `npc.smuggler` | Gurney Smuggler | `dialogue` | `loc.cantina` | Spice trade rumors |
| `npc.mentat` | Thufir Mentat | `dialogue` | `loc.palace_courtyard` | Strategic advice |
| `npc.spice_merchant` | Chani Spice Trader | `quest_giver` | `loc.market` | Assigns `quest.spice_harvest` |
| `npc.stilgar` | Stilgar | `quest_giver` | `loc.sietch_tabr` | Assigns `quest.harkonnen_raid` |
| `npc.harkonnen_scout` | Harkonnen Scout | `enemy` | `loc.desert_edge` | Hostile raider (`35 HP`, `10 DMG`) |
| `npc.sandworm` | Shai-Hulud | `enemy` | `loc.worm_cavern` | Boss creature (`80 HP`, `18 DMG`) |

### Dynamic Items (10 Items, Multi-Word & Unique Instances)

Items exist as unique instances in `GameState::room_items`: `TAKE` removes an item from the room (preventing duplication), and `DROP` places it in the player's current room. Both canonical IDs (`item.crysknife`) and multi-word display names (`"Crysknife"`, `"Raw Melange"`, `"Litrejon Water Flask"`) are supported via `protocol::matches_reference`. `item.water_of_life` is marked `obtainable: false` as a scenic landmark object.

---

## Server Logging

The server uses **`tracing`** and **`tracing-subscriber`** (with the `json` and `time` features) to emit structured, machine-parseable JSON logs to standard output without blocking async Tokio worker threads.

* **Log Format & Levels:**
* Every log entry contains an ISO-8601 UTC `timestamp`, `level` (`INFO`, `WARN`, `ERROR`), `target`, and structured `fields`.
* **`INFO`**: Startup world validation stats, TCP connections (`tcp_connected` with client `SocketAddr`), player logins/logouts (`player_connected`, `player_disconnected`), every incoming command (`command_received`), outbound replies (`reply_sent`), movement, item transfers, combat rounds, and quest completions.
* **`WARN`**: Malformed commands (`malformed_command`), duplicate login attempts (`auth_failed`), oversized lines (`line_too_long`), socket read errors, and abuse alerts.
* **`ERROR`**: Critical task panics or startup configuration/validation failures.


* **Abuse & Flooding Detection:**
* **Command Rate Limiter:** Each `PlayerSession` tracks command counts within a 2-second window (`check_rate_limit`). Exceeding **20 commands in 2 seconds** triggers a `WARN` log (`event: "abuse_flooding"` with username, IP address, and command rate) and rejects the burst with `ERR 429 RATE_LIMITED`.
* **Frame Size Guard:** Lines exceeding `MAX_LINE_LENGTH` (`1024` bytes) are rejected with `ERR 413 LINE_TOO_LONG` and logged as `WARN`.



---

## Group Contributions

* **`jaatif-a`**:
* Designed and implemented the shared `crates/protocol` crate (RFC 42TAP parser, serializer, JSON payload structs, `matches_reference` resolver, and unit test suite).
* Architected and built the asynchronous TCP server (`crates/server`: YAML world loader/validator, `GameState`, turn-based combat, quest engine, squad system, rate-limiting, and structured JSON logging).
* Authored the Dune/Arrakis world dataset (`data/world.yaml`).

* **`dzhambal`**:
* Built the asynchronous terminal client (`crates/cli`) and modular Modern-MUD graphical client (`crates/gui` with `eframe`/`egui`, cartographic radar, procedural portraits, and comms/telemetry panels).
* Wrote build automation (`Makefile`) and project documentation (`README.md`).



---

## Building and Running

All lifecycle commands are automated through the root `Makefile`:

| Target | Command | Description |
| --- | --- | --- |
| `make install` | `rustup component add clippy rustfmt && cargo fetch && cargo build --workspace` | Installs toolchain linters, fetches crates, and compiles the workspace. |
| `make build` | `cargo build --workspace` | Compiles `protocol`, `server`, `cli`, and `gui`. |
| `make run-server` | `cargo run -p server -- --host $(HOST) --port $(PORT) --world $(WORLD_FILE)` | Starts the TCP game server (override port with `PORT=4243`). |
| `make run-client` | `cargo run -p cli -- --host $(HOST) --port $(PORT)` | Starts the interactive CLI client. |
| `make run-client-gui` | `cargo run -p gui -- --host $(HOST) --port $(PORT)` | Starts the `egui` graphical client. |
| `make lint` | `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings` | Enforces formatting and zero-warning strict Clippy rules. |
| `make test` | `cargo test --workspace` | Runs all protocol and world validation unit tests. |
| `make clean` | `cargo clean` | Removes build artifacts. |

---

## Testing

### 1. Automated Unit & Validation Tests

Run the full test suite and strict linter:

```bash
make test
make lint

```

* **`protocol` tests (16 tests):** Verify round-trip parsing/formatting for all commands and `EVT` notifications, case-insensitivity, CRLF/LF handling, multi-word item references, Unicode usernames, oversized frame rejection, and exact RFC error codes.
* **`server` tests:** Validate `data/world.yaml` room counts, NPC roles, obtainable items, quest definitions, and cross-reference integrity.

### 2. Multiplayer & Interoperability Testing

1. Start the server: `make run-server PORT=4243`
2. Open **two or more clients** (e.g., one `make run-client-gui PORT=4243` as `Paul` and one `make run-client PORT=4243` as `Chani`, or raw `nc 127.0.0.1 4243`).
3. **Presence & Chat:** Move between rooms (`MOVE north`) and send `CHAT GLOBAL`, `CHAT ROOM`, and `CHAT GROUP` messages to verify real-time `EVT` delivery.
4. **Item Uniqueness:** Have Player A run `TAKE Crysknife` while Player B is in `loc.arrakeen_gate`; confirm Player B receives `EVT ROOM ITEM TAKE` and cannot duplicate the item (`ERR 404 ITEM_NOT_FOUND`) until Player A runs `DROP Crysknife`.
5. **Combat & Quest Flow:**
* Travel `north` to `loc.market`, run `QUEST npc.spice_merchant`, travel `south` → `west` → `west` to `loc.spice_blow`, and run `TAKE Raw Melange` to verify automatic quest completion and `item.guild_solari` reward delivery.
* Engage `npc.harkonnen_scout` (`ATTACK npc.harkonnen_scout`, `DEFEND`, `FLEE`) and `npc.sandworm` (`ATTACK npc.sandworm`) to verify damage formulas, counterattacks, and `0 HP` respawn at `loc.sietch_tabr`.


6. **Abrupt Disconnects:** Kill a client with `Ctrl+C` or `kill -9` and verify the server logs `player_disconnected` in JSON and immediately broadcasts `EVT ROOM PRESENCE LEAVE` without interrupting remaining sessions.

---

## Resources

### Technical References

* **RFC 42TAP Specification** (`docs/rfc42tap.html`) — *The Answer Protocol: A TCP-Based Multi-User Dungeon Communication Protocol*, 42 School Network Working Group.
* **RFC 2119 & RFC 5234** — *Key words for use in RFCs* and *Augmented BNF for Syntax Specifications (ABNF)*.
* **The Rust Programming Language & Cargo Book** — https://doc.rust-lang.org/book/
* **Tokio Asynchronous Runtime Documentation** — https://tokio.rs/tokio/tutorial
* **`eframe` / `egui` Immediate Mode GUI Documentation** — https://docs.rs/eframe/
* **`tracing` & `tracing-subscriber` Structured Logging** — https://docs.rs/tracing/

### AI Usage Disclosure

In accordance with Chapter I and Chapter VI guidelines, AI assistance (Gemini) was used selectively as a collaborative engineering aid under full human review and verification:

* **Scaffolding & Boilerplate:** Generating initial `Cargo.toml` workspace manifests, `Makefile` targets, and unit test templates for RFC 42TAP command/event round-trips.
* **GUI Layout & Vector Drawing:** Assisting with `egui` layout structuring, procedural vector portrait coordinates (`avatars.rs`), and cartographic radar grid math (`radar.rs`).
* **Documentation:** Structuring and formatting the `README.md` to ensure all mandatory 42 evaluation sections were clearly addressed.
* **Human Verification:** All protocol parsers, server state locking, combat/quest logic, YAML world design, and client networking loops were reviewed, compiled under `cargo clippy -- -D warnings`, and tested interactively across multiple concurrent sessions.
