use eframe::egui::{self, Color32, FontId, Rounding, Stroke};
use protocol::{ChatScope, Command, GroupAction};

use crate::avatars::{draw_portrait, EntityArchetype};
use crate::net::NetCommand;
use crate::theme::{
    card_frame, draw_hp_bar, BG_OBSIDIAN, BORDER_SUBTLE, CRIMSON_DANGER, EMERALD_HP, FREMEN_CYAN,
    PANEL_SLATE, SPICE_AMBER, SPICE_GOLD, TEXT_MUTED, TEXT_SAND,
};
use crate::{ActiveChatTab, LogKind, TapGuiApp};

pub fn render_app(app: &mut TapGuiApp, ctx: &egui::Context) {
    render_top_hud(app, ctx);
    render_left_pilot_dock(app, ctx);
    render_right_radar_dock(app, ctx);
    render_bottom_terminal(app, ctx);
    render_center_sector(app, ctx);
}

fn render_top_hud(app: &mut TapGuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top_hud")
        .frame(
            egui::Frame::none()
                .fill(Color32::from_rgb(14, 17, 25))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(72, 54, 24)))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("◈ TAP // ARRAKIS MODERN MUD")
                        .font(FontId::monospace(14.5))
                        .strong()
                        .color(SPICE_AMBER),
                );
                ui.separator();

                if !app.connected {
                    ui.label(egui::RichText::new("HOST").color(TEXT_MUTED).small());
                    ui.add(egui::TextEdit::singleline(&mut app.host).desired_width(105.0));
                    ui.label(egui::RichText::new("PORT").color(TEXT_MUTED).small());
                    ui.add(egui::TextEdit::singleline(&mut app.port).desired_width(52.0));
                    ui.label(egui::RichText::new("PILOT").color(TEXT_MUTED).small());
                    ui.add(egui::TextEdit::singleline(&mut app.username).desired_width(95.0));

                    let btn = egui::Button::new(
                        egui::RichText::new("⚡ CONNECT")
                            .strong()
                            .color(Color32::BLACK),
                    )
                    .fill(SPICE_AMBER);

                    if ui.add(btn).clicked() && !app.username.trim().is_empty() {
                        let addr = format!("{}:{}", app.host.trim(), app.port.trim());
                        let username = app.username.trim().to_string();
                        app.status_banner = format!("LINKING {addr}...");
                        let _ = app.net_tx.send(NetCommand::Connect { addr, username });
                    }
                } else {
                    ui.label(
                        egui::RichText::new(&app.status_banner)
                            .font(FontId::monospace(12.0))
                            .strong()
                            .color(EMERALD_HP),
                    );
                    ui.separator();

                    draw_hp_bar(
                        ui,
                        app.player_status.hp,
                        app.player_status.max_hp,
                        &app.player_status.status,
                        175.0,
                    );
                    ui.separator();

                    let room_players = app.current_room.as_ref().map_or(0, |r| r.players.len());
                    ui.label(
                        egui::RichText::new(format!("SECTOR: {room_players}"))
                            .font(FontId::monospace(11.5))
                            .color(FREMEN_CYAN),
                    );
                    ui.label(
                        egui::RichText::new(format!("WORLD: {}", app.server_player_count))
                            .font(FontId::monospace(11.5))
                            .color(SPICE_GOLD),
                    );

                    ui.separator();
                    if ui.button("🔍 LOOK").clicked() {
                        app.send_cmd(Command::Look);
                    }
                    if ui.button("💓 STATUS").clicked() {
                        app.send_cmd(Command::Status);
                    }
                    if ui.button("📜 QUESTS").clicked() {
                        app.send_cmd(Command::Quests);
                    }
                    if ui.button("🌐 WHO").clicked() {
                        app.send_cmd(Command::Who);
                    }
                    let quit_btn =
                        egui::Button::new(egui::RichText::new("✖ QUIT").color(Color32::WHITE))
                            .fill(Color32::from_rgb(135, 32, 32));
                    if ui.add(quit_btn).clicked() {
                        app.send_cmd(Command::Quit);
                        let _ = app.net_tx.send(NetCommand::Disconnect);
                    }
                }
            });
        });
}

fn render_left_pilot_dock(app: &mut TapGuiApp, ctx: &egui::Context) {
    egui::SidePanel::left("left_pilot_dock")
        .default_width(275.0)
        .frame(
            egui::Frame::none()
                .fill(PANEL_SLATE)
                .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                .inner_margin(egui::Margin::same(10.0)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Pilot Profile Card with Avatar
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        draw_portrait(ui, EntityArchetype::PlayerPilot, 52.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(app.username.to_uppercase())
                                    .font(FontId::monospace(14.0))
                                    .strong()
                                    .color(SPICE_GOLD),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "State: {}",
                                    app.player_status.status.to_uppercase()
                                ))
                                .font(FontId::monospace(11.0))
                                .color(FREMEN_CYAN),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Squad: {}",
                                    app.group_id.as_deref().unwrap_or("SOLO")
                                ))
                                .font(FontId::monospace(11.0))
                                .color(TEXT_MUTED),
                            );
                        });
                    });
                });

                ui.add_space(8.0);

                // Cargo / Inventory Card
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("🎒 CARGO / INVENTORY")
                                .font(FontId::monospace(12.0))
                                .strong()
                                .color(SPICE_AMBER),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("↻").clicked() {
                                app.send_cmd(Command::Inventory);
                            }
                        });
                    });
                    ui.add_space(4.0);

                    let inv_snapshot = app.inventory.clone();
                    if inv_snapshot.is_empty() {
                        ui.label(
                            egui::RichText::new("Cargo hold empty.")
                                .italics()
                                .color(TEXT_MUTED),
                        );
                    } else {
                        for item_id in inv_snapshot {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("◆ {item_id}"))
                                        .font(FontId::monospace(11.5))
                                        .color(TEXT_SAND),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("DROP").clicked() {
                                            app.send_cmd(Command::Drop { item_ref: item_id });
                                        }
                                    },
                                );
                            });
                        }
                    }

                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Take / Drop by ID or Name:")
                            .small()
                            .color(TEXT_MUTED),
                    );
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.item_input)
                                .hint_text("e.g. Crysknife")
                                .desired_width(130.0),
                        );
                        if ui.button("TAKE").clicked() && !app.item_input.trim().is_empty() {
                            let item_ref = app.item_input.trim().to_string();
                            app.send_cmd(Command::Take { item_ref });
                            app.item_input.clear();
                        }
                        if ui.button("DROP").clicked() && !app.item_input.trim().is_empty() {
                            let item_ref = app.item_input.trim().to_string();
                            app.send_cmd(Command::Drop { item_ref });
                            app.item_input.clear();
                        }
                    });
                });

                ui.add_space(8.0);

                // Quests Card
                card_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("📜 ACTIVE CONTRACTS")
                            .font(FontId::monospace(12.0))
                            .strong()
                            .color(SPICE_GOLD),
                    );
                    ui.add_space(4.0);
                    if app.quests.is_empty() {
                        ui.label(
                            egui::RichText::new("No active quests. Click QUEST on an NPC.")
                                .italics()
                                .color(TEXT_MUTED),
                        );
                    } else {
                        for q in &app.quests {
                            let (badge_col, prog_txt) = if q.status == "completed" {
                                (EMERALD_HP, "DONE".to_string())
                            } else {
                                (
                                    SPICE_AMBER,
                                    q.progress.clone().unwrap_or_else(|| "ACTIVE".to_string()),
                                )
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("[{prog_txt}]"))
                                        .font(FontId::monospace(11.0))
                                        .strong()
                                        .color(badge_col),
                                );
                                ui.label(
                                    egui::RichText::new(&q.quest_id)
                                        .font(FontId::monospace(11.5))
                                        .color(TEXT_SAND),
                                );
                            });
                        }
                    }
                });

                ui.add_space(8.0);

                // Squad / Group Card
                card_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("🛡 SQUAD OPERATIONS")
                            .font(FontId::monospace(12.0))
                            .strong()
                            .color(FREMEN_CYAN),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Active Squad ID: {} (auto-assigned)",
                            app.group_id.as_deref().unwrap_or("NONE")
                        ))
                        .small()
                        .color(TEXT_MUTED),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("CREATE").clicked() {
                            app.send_cmd(Command::Group(GroupAction::Create));
                        }
                        if ui.button("LEAVE").clicked() {
                            app.send_cmd(Command::Group(GroupAction::Leave));
                            app.group_id = None;
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Pilot name (for INVITE / JOIN):")
                            .small()
                            .color(TEXT_MUTED),
                    );
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.group_target_input)
                                .hint_text("e.g. Chani")
                                .desired_width(115.0),
                        );
                        if ui.button("INVITE").clicked()
                            && !app.group_target_input.trim().is_empty()
                        {
                            let username = app.group_target_input.trim().to_string();
                            app.send_cmd(Command::Group(GroupAction::Invite { username }));
                        }
                        if ui.button("JOIN").clicked() && !app.group_target_input.trim().is_empty()
                        {
                            let leader = app.group_target_input.trim().to_string();
                            app.send_cmd(Command::Group(GroupAction::Join { leader }));
                        }
                    });
                });
            });
        });
}

fn render_right_radar_dock(app: &mut TapGuiApp, ctx: &egui::Context) {
    egui::SidePanel::right("right_radar_dock")
        .default_width(335.0)
        .frame(
            egui::Frame::none()
                .fill(PANEL_SLATE)
                .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                .inner_margin(egui::Margin::same(10.0)),
        )
        .show(ctx, |ui| {
            if let Some(dir) = app.radar.draw(ui) {
                app.send_cmd(Command::Move { direction: dir });
            }
        });
}

fn render_bottom_terminal(app: &mut TapGuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("bottom_terminal")
        .default_height(215.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(Color32::from_rgb(9, 11, 17))
                .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                .inner_margin(egui::Margin::same(10.0)),
        )
        .show(ctx, |ui| {
            ui.columns(2, |cols| {
                // Left: Separated Chat View (Global / Room / Group)
                cols[0].horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("📡 COMMS")
                            .font(FontId::monospace(12.0))
                            .strong()
                            .color(FREMEN_CYAN),
                    );
                    ui.selectable_value(&mut app.active_chat_tab, ActiveChatTab::Global, "Global");
                    ui.selectable_value(&mut app.active_chat_tab, ActiveChatTab::Room, "Room");
                    ui.selectable_value(&mut app.active_chat_tab, ActiveChatTab::Group, "Group");
                });
                cols[0].separator();

                let messages = match app.active_chat_tab {
                    ActiveChatTab::Global => &app.global_chat,
                    ActiveChatTab::Room => &app.room_chat,
                    ActiveChatTab::Group => &app.group_chat,
                };

                egui::ScrollArea::vertical()
                    .id_salt("chat_scroll")
                    .max_height(130.0)
                    .stick_to_bottom(true)
                    .show(&mut cols[0], |ui| {
                        for (sender, text) in messages {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("<{sender}>"))
                                        .font(FontId::monospace(11.5))
                                        .strong()
                                        .color(SPICE_AMBER),
                                );
                                ui.label(
                                    egui::RichText::new(text)
                                        .font(FontId::monospace(11.5))
                                        .color(TEXT_SAND),
                                );
                            });
                        }
                    });

                cols[0].horizontal(|ui| {
                    let enter_pressed = ui
                        .add(
                            egui::TextEdit::singleline(&mut app.chat_input)
                                .hint_text("Transmit message...")
                                .desired_width(ui.available_width() - 68.0),
                        )
                        .lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if (ui.button("SEND").clicked() || enter_pressed)
                        && !app.chat_input.trim().is_empty()
                    {
                        let scope = match app.active_chat_tab {
                            ActiveChatTab::Global => ChatScope::Global,
                            ActiveChatTab::Room => ChatScope::Room,
                            ActiveChatTab::Group => ChatScope::Group,
                        };
                        let message = app.chat_input.trim().to_string();
                        app.send_cmd(Command::Chat { scope, message });
                        app.chat_input.clear();
                    }
                });

                // Right: System & Combat Telemetry Log
                cols[1].horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("📟 TELEMETRY & COMBAT LOG")
                            .font(FontId::monospace(12.0))
                            .strong()
                            .color(SPICE_AMBER),
                    );
                });
                cols[1].separator();
                egui::ScrollArea::vertical()
                    .id_salt("log_scroll")
                    .max_height(160.0)
                    .stick_to_bottom(true)
                    .show(&mut cols[1], |ui| {
                        for entry in &app.system_logs {
                            let color = match entry.kind {
                                LogKind::Info => TEXT_SAND,
                                LogKind::Combat => Color32::from_rgb(251, 146, 60),
                                LogKind::Quest => SPICE_GOLD,
                                LogKind::Room => FREMEN_CYAN,
                                LogKind::Error => CRIMSON_DANGER,
                            };
                            ui.label(
                                egui::RichText::new(&entry.text)
                                    .font(FontId::monospace(11.5))
                                    .color(color),
                            );
                        }
                    });
            });
        });
}

fn render_center_sector(app: &mut TapGuiApp, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(BG_OBSIDIAN)
                .inner_margin(egui::Margin::same(12.0)),
        )
        .show(ctx, |ui| {
            let Some(room_state) = app.current_room.clone() else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "◈ AWAITING UPLINK ◈\nEnter Host, Port & Pilot Name above and press CONNECT.",
                        )
                        .font(FontId::monospace(15.0))
                        .color(SPICE_AMBER),
                    );
                });
                return;
            };

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Sector Header Card
                card_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(room_state.room.id.to_uppercase())
                            .font(FontId::monospace(11.0))
                            .color(FREMEN_CYAN),
                    );
                    ui.label(
                        egui::RichText::new(&room_state.room.name)
                            .size(20.0)
                            .strong()
                            .color(SPICE_GOLD),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(&room_state.room.description)
                            .size(13.0)
                            .color(TEXT_SAND),
                    );
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("PILOTS PRESENT:")
                                .font(FontId::monospace(11.0))
                                .color(TEXT_MUTED),
                        );
                        for p in &room_state.players {
                            ui.label(
                                egui::RichText::new(format!("● {p}"))
                                    .font(FontId::monospace(11.5))
                                    .strong()
                                    .color(EMERALD_HP),
                            );
                        }
                    });
                });

                ui.add_space(8.0);

                // Compass Exits Bar
                card_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("🧭 NAVIGATION EXITS (MOVE)")
                            .font(FontId::monospace(12.0))
                            .strong()
                            .color(FREMEN_CYAN),
                    );
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for (dir, target) in &room_state.room.exits {
                            let short = target.strip_prefix("loc.").unwrap_or(target);
                            if ui
                                .button(format!("➡ {} ({short})", dir.to_uppercase()))
                                .clicked()
                            {
                                app.send_cmd(Command::Move {
                                    direction: dir.clone(),
                                });
                            }
                        }
                    });
                });

                ui.add_space(8.0);

                // Sector Loot Card
                card_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("📦 SECTOR LOOT (TAKE)")
                            .font(FontId::monospace(12.0))
                            .strong()
                            .color(SPICE_AMBER),
                    );
                    ui.add_space(4.0);
                    if room_state.items.is_empty() {
                        ui.label(
                            egui::RichText::new("No items detected in this sector.")
                                .italics()
                                .color(TEXT_MUTED),
                        );
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            for item_id in &room_state.items {
                                let btn = egui::Button::new(
                                    egui::RichText::new(format!("✦ TAKE {item_id}"))
                                        .font(FontId::monospace(11.5))
                                        .color(SPICE_GOLD),
                                )
                                .fill(Color32::from_rgb(40, 32, 18));
                                if ui.add(btn).clicked() {
                                    app.send_cmd(Command::Take {
                                        item_ref: item_id.clone(),
                                    });
                                }
                            }
                        });
                    }
                });

                ui.add_space(8.0);

                // Characters & Entities with Portraits
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("⚔ CHARACTERS & ENTITIES")
                                .font(FontId::monospace(12.0))
                                .strong()
                                .color(CRIMSON_DANGER),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.button("🏃 FLEE").clicked() {
                                    app.send_cmd(Command::Flee);
                                }
                                if ui.button("🛡 DEFEND (+5 HP)").clicked() {
                                    app.send_cmd(Command::Defend);
                                }
                            },
                        );
                    });
                    ui.add_space(6.0);

                    if room_state.npcs.is_empty() {
                        ui.label(
                            egui::RichText::new("No characters or creatures in this sector.")
                                .italics()
                                .color(TEXT_MUTED),
                        );
                    } else {
                        for npc_id in &room_state.npcs {
                            let archetype = EntityArchetype::from_npc_id(npc_id);
                            let accent = archetype.accent_color();

                            egui::Frame::none()
                                .fill(Color32::from_rgb(17, 21, 31))
                                .stroke(Stroke::new(1.0_f32, accent))
                                .rounding(Rounding::same(6.0))
                                .inner_margin(egui::Margin::same(8.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        draw_portrait(ui, archetype, 46.0);
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(npc_id)
                                                        .font(FontId::monospace(13.0))
                                                        .strong()
                                                        .color(accent),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "[{}]",
                                                        archetype.role_badge()
                                                    ))
                                                    .font(FontId::monospace(10.0))
                                                    .color(TEXT_MUTED),
                                                );
                                            });
                                            ui.add_space(2.0);
                                            ui.horizontal(|ui| {
                                                if ui.button("💬 TALK").clicked() {
                                                    app.send_cmd(Command::Talk {
                                                        npc_ref: npc_id.clone(),
                                                    });
                                                }
                                                if ui.button("📜 QUEST").clicked() {
                                                    app.send_cmd(Command::Quest {
                                                        npc_ref: npc_id.clone(),
                                                    });
                                                }
                                                let atk_btn = egui::Button::new(
                                                    egui::RichText::new("⚔ ATTACK")
                                                        .color(Color32::WHITE),
                                                )
                                                .fill(Color32::from_rgb(140, 34, 34));
                                                if ui.add(atk_btn).clicked() {
                                                    app.send_cmd(Command::Attack {
                                                        target: npc_id.clone(),
                                                    });
                                                }
                                            });
                                        });
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    }

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Custom Target:")
                                .small()
                                .color(TEXT_MUTED),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut app.npc_input)
                                .hint_text("NPC ID or Name")
                                .desired_width(135.0),
                        );
                        if ui.button("TALK").clicked() && !app.npc_input.trim().is_empty() {
                            let npc_ref = app.npc_input.trim().to_string();
                            app.send_cmd(Command::Talk { npc_ref });
                        }
                        if ui.button("QUEST").clicked() && !app.npc_input.trim().is_empty() {
                            let npc_ref = app.npc_input.trim().to_string();
                            app.send_cmd(Command::Quest { npc_ref });
                        }
                        if ui.button("ATTACK").clicked() && !app.npc_input.trim().is_empty() {
                            let target = app.npc_input.trim().to_string();
                            app.send_cmd(Command::Attack { target });
                        }
                    });
                });

                ui.add_space(8.0);

                // Dialogue Transmission Box
                egui::Frame::none()
                    .fill(Color32::from_rgb(24, 21, 16))
                    .stroke(Stroke::new(1.0_f32, SPICE_AMBER))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("💬 TRANSMISSION / DIALOGUE")
                                .font(FontId::monospace(11.5))
                                .strong()
                                .color(SPICE_AMBER),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new(&app.last_npc_dialogue)
                                .size(13.0)
                                .color(TEXT_SAND),
                        );
                    });
            });
        });
}
