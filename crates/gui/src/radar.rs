use std::collections::{BTreeMap, HashMap};

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Sense, Stroke, Vec2};
use protocol::LookPayload;

use crate::theme::{
    BORDER_SUBTLE, CRIMSON_DANGER, EMERALD_HP, FREMEN_CYAN, SPICE_AMBER, SPICE_GOLD, TEXT_MUTED,
    TEXT_SAND,
};

#[derive(Debug, Clone)]
pub struct MapNode {
    pub id: String,
    pub name: String,
    pub gx: i32,
    pub gy: i32,
    pub visited: bool,
    pub has_enemies: bool,
    pub has_items: bool,
    pub exits: BTreeMap<String, String>,
}

pub struct WorldRadar {
    pub nodes: HashMap<String, MapNode>,
    pub current_room_id: Option<String>,
    pub travel_trail: Vec<String>,
    pub zoom: f32,
    pub pan_offset: Vec2,
}

impl Default for WorldRadar {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            current_room_id: None,
            travel_trail: Vec::new(),
            zoom: 1.0,
            pan_offset: Vec2::ZERO,
        }
    }
}

impl WorldRadar {
    pub fn reset(&mut self) {
        self.nodes.clear();
        self.current_room_id = None;
        self.travel_trail.clear();
        self.pan_offset = Vec2::ZERO;
    }

    pub fn observe_room(&mut self, look: &LookPayload) {
        let room_id = look.room.id.clone();

        if self.current_room_id.as_deref() != Some(room_id.as_str()) {
            self.travel_trail.push(room_id.clone());
            if self.travel_trail.len() > 24 {
                self.travel_trail.remove(0);
            }
            self.current_room_id = Some(room_id.clone());
        }

        // Determine coordinates if not already placed
        let (base_x, base_y) = if let Some(existing) = self.nodes.get(&room_id) {
            (existing.gx, existing.gy)
        } else {
            self.infer_coordinates(&room_id, &look.room.exits)
        };

        let has_enemies = look.npcs.iter().any(|n| {
            let l = n.to_ascii_lowercase();
            l.contains("harkonnen") || l.contains("sandworm") || l.contains("goblin")
        });

        self.nodes.insert(
            room_id.clone(),
            MapNode {
                id: room_id.clone(),
                name: look.room.name.clone(),
                gx: base_x,
                gy: base_y,
                visited: true,
                has_enemies,
                has_items: !look.items.is_empty(),
                exits: look.room.exits.clone(),
            },
        );

        // Pre-register connected neighbor nodes as unvisited fog-of-war contacts
        for (dir, target_id) in &look.room.exits {
            if !self.nodes.contains_key(target_id) {
                let (dx, dy) = dir_delta(dir);
                let (nx, ny) = self.find_free_cell(base_x + dx, base_y + dy);
                let short_name = target_id
                    .strip_prefix("loc.")
                    .unwrap_or(target_id)
                    .replace('_', " ");
                self.nodes.insert(
                    target_id.clone(),
                    MapNode {
                        id: target_id.clone(),
                        name: short_name,
                        gx: nx,
                        gy: ny,
                        visited: false,
                        has_enemies: false,
                        has_items: false,
                        exits: BTreeMap::new(),
                    },
                );
            }
        }
    }

    fn infer_coordinates(&self, room_id: &str, exits: &BTreeMap<String, String>) -> (i32, i32) {
        if self.nodes.is_empty() {
            return (0, 0);
        }
        // Check if any known room links to this room
        for node in self.nodes.values() {
            for (dir, target) in &node.exits {
                if target == room_id {
                    let (dx, dy) = dir_delta(dir);
                    return self.find_free_cell(node.gx + dx, node.gy + dy);
                }
            }
        }
        // Check reverse link from our own exits
        for (dir, target) in exits {
            if let Some(known) = self.nodes.get(target) {
                let (dx, dy) = dir_delta(dir);
                return self.find_free_cell(known.gx - dx, known.gy - dy);
            }
        }
        self.find_free_cell(0, 0)
    }

    fn find_free_cell(&self, wanted_x: i32, wanted_y: i32) -> (i32, i32) {
        if !self
            .nodes
            .values()
            .any(|n| n.gx == wanted_x && n.gy == wanted_y)
        {
            return (wanted_x, wanted_y);
        }
        for step in 1..=6 {
            for (ox, oy) in [(step, 0), (-step, 0), (0, step), (0, -step)] {
                let cx = wanted_x + ox;
                let cy = wanted_y + oy;
                if !self.nodes.values().any(|n| n.gx == cx && n.gy == cy) {
                    return (cx, cy);
                }
            }
        }
        (wanted_x, wanted_y)
    }

    /// Renders the full discovered Arrakis cartographic radar and returns a direction if clicked.
    pub fn draw(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut chosen_direction: Option<String> = None;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("🛰 ARRAKIS CARTOGRAPHIC RADAR")
                    .font(FontId::monospace(12.0))
                    .strong()
                    .color(SPICE_AMBER),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⌖ Center").clicked() {
                    self.pan_offset = Vec2::ZERO;
                }
                if ui.small_button("+").clicked() {
                    self.zoom = (self.zoom + 0.15).min(1.6);
                }
                if ui.small_button("-").clicked() {
                    self.zoom = (self.zoom - 0.15).max(0.65);
                }
            });
        });

        let discovered_count = self.nodes.values().filter(|n| n.visited).count();
        ui.label(
            egui::RichText::new(format!(
                "Sectors Mapped: {discovered_count}/{} | Click glowing adjacent sector to MOVE",
                self.nodes.len()
            ))
            .small()
            .color(TEXT_MUTED),
        );
        ui.add_space(4.0);

        let avail = ui.available_size();
        let canvas_size = Vec2::new(avail.x.max(260.0), (avail.y - 36.0).max(260.0));
        let (rect, canvas_resp) = ui.allocate_exact_size(canvas_size, Sense::drag());

        if canvas_resp.dragged() {
            self.pan_offset += canvas_resp.drag_delta();
        }

        let painter = ui.painter_at(rect);

        // Radar background & tactical grid
        painter.rect_filled(rect, Rounding::same(8.0), Color32::from_rgb(8, 11, 17));
        painter.rect_stroke(
            rect,
            Rounding::same(8.0),
            Stroke::new(1.2_f32, Color32::from_rgb(65, 50, 24)),
        );

        let grid_step = 32.0 * self.zoom;
        let mut gx = rect.left() + (self.pan_offset.x % grid_step);
        while gx < rect.right() {
            painter.line_segment(
                [Pos2::new(gx, rect.top()), Pos2::new(gx, rect.bottom())],
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(56, 189, 248, 14)),
            );
            gx += grid_step;
        }
        let mut gy = rect.top() + (self.pan_offset.y % grid_step);
        while gy < rect.bottom() {
            painter.line_segment(
                [Pos2::new(rect.left(), gy), Pos2::new(rect.right(), gy)],
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(56, 189, 248, 14)),
            );
            gy += grid_step;
        }

        // Camera anchor centered on current room
        let (cam_gx, cam_gy) = self
            .current_room_id
            .as_ref()
            .and_then(|id| self.nodes.get(id))
            .map_or((0, 0), |n| (n.gx, n.gy));

        let cell_w = 108.0 * self.zoom;
        let cell_h = 74.0 * self.zoom;
        let center = rect.center() + self.pan_offset;

        let to_screen = |x: i32, y: i32| -> Pos2 {
            Pos2::new(
                center.x + ((x - cam_gx) as f32) * cell_w,
                center.y + ((y - cam_gy) as f32) * cell_h,
            )
        };

        // Radar sweep circles around current player position
        let player_pos = to_screen(cam_gx, cam_gy);
        painter.circle_stroke(
            player_pos,
            64.0 * self.zoom,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(234, 164, 58, 32)),
        );
        painter.circle_stroke(
            player_pos,
            132.0 * self.zoom,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(56, 189, 248, 22)),
        );

        // 1. Draw all known corridor edges
        for node in self.nodes.values() {
            let p1 = to_screen(node.gx, node.gy);
            for target_id in node.exits.values() {
                if let Some(target_node) = self.nodes.get(target_id) {
                    let p2 = to_screen(target_node.gx, target_node.gy);
                    let stroke_col = if node.visited && target_node.visited {
                        Color32::from_rgb(56, 125, 168)
                    } else {
                        Color32::from_rgb(48, 56, 74)
                    };
                    painter.line_segment([p1, p2], Stroke::new(1.8_f32, stroke_col));
                }
            }
        }

        // 2. Highlight recent movement trail in glowing amber
        for pair in self.travel_trail.windows(2) {
            if let (Some(a), Some(b)) = (self.nodes.get(&pair[0]), self.nodes.get(&pair[1])) {
                let pa = to_screen(a.gx, a.gy);
                let pb = to_screen(b.gx, b.gy);
                painter.line_segment(
                    [pa, pb],
                    Stroke::new(3.0_f32, Color32::from_rgba_unmultiplied(250, 204, 21, 145)),
                );
            }
        }

        // Lookup adjacent exits from current room for click-to-move
        let adjacent_dirs: HashMap<String, String> = self
            .current_room_id
            .as_ref()
            .and_then(|id| self.nodes.get(id))
            .map(|cur| {
                cur.exits
                    .iter()
                    .map(|(dir, target)| (target.clone(), dir.clone()))
                    .collect()
            })
            .unwrap_or_default();

        // 3. Draw every discovered & frontier node
        let node_w = 88.0 * self.zoom;
        let node_h = 38.0 * self.zoom;

        for node in self.nodes.values() {
            let pos = to_screen(node.gx, node.gy);
            let node_rect = Rect::from_center_size(pos, Vec2::new(node_w, node_h));
            if !rect.intersects(node_rect) {
                continue;
            }

            let is_current = self.current_room_id.as_deref() == Some(node.id.as_str());
            let adj_dir = adjacent_dirs.get(&node.id);

            let resp = ui.interact(
                node_rect,
                ui.id().with(("world_radar_node", &node.id)),
                if adj_dir.is_some() {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );

            let (fill, stroke_col, title_col) = if is_current {
                (Color32::from_rgb(64, 44, 16), SPICE_GOLD, SPICE_GOLD)
            } else if adj_dir.is_some() {
                let bg = if resp.hovered() {
                    Color32::from_rgb(28, 58, 84)
                } else {
                    Color32::from_rgb(18, 34, 52)
                };
                (bg, FREMEN_CYAN, Color32::WHITE)
            } else if node.visited {
                (Color32::from_rgb(22, 27, 38), BORDER_SUBTLE, TEXT_SAND)
            } else {
                (
                    Color32::from_rgb(15, 18, 26),
                    Color32::from_rgb(42, 48, 62),
                    TEXT_MUTED,
                )
            };

            painter.rect_filled(node_rect, Rounding::same(6.0), fill);
            painter.rect_stroke(
                node_rect,
                Rounding::same(6.0),
                Stroke::new(if is_current { 2.0 } else { 1.2_f32 }, stroke_col),
            );

            let label = if node.visited {
                truncate_label(&node.name, 13)
            } else {
                format!("? {}", truncate_label(&node.name, 10))
            };

            painter.text(
                Pos2::new(pos.x, pos.y - 6.0 * self.zoom),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::monospace((9.5 * self.zoom).max(8.0)),
                title_col,
            );

            let subtitle = if is_current {
                "◉ YOU".to_string()
            } else if let Some(dir) = adj_dir {
                format!("➡ {}", dir.to_uppercase())
            } else if node.visited {
                "MAPPED".to_string()
            } else {
                "UNEXPLORED".to_string()
            };

            let sub_col = if is_current {
                EMERALD_HP
            } else if adj_dir.is_some() {
                FREMEN_CYAN
            } else {
                TEXT_MUTED
            };

            painter.text(
                Pos2::new(pos.x, pos.y + 7.0 * self.zoom),
                egui::Align2::CENTER_CENTER,
                subtitle,
                FontId::monospace((8.5 * self.zoom).max(7.5)),
                sub_col,
            );

            // Small status beacons for enemies / loot
            if node.has_enemies {
                painter.circle_filled(
                    Pos2::new(node_rect.right() - 6.0, node_rect.top() + 6.0),
                    3.2,
                    CRIMSON_DANGER,
                );
            }
            if node.has_items {
                painter.circle_filled(
                    Pos2::new(node_rect.left() + 6.0, node_rect.top() + 6.0),
                    3.2,
                    SPICE_GOLD,
                );
            }

            if let Some(dir) = adj_dir {
                if resp.clicked() {
                    chosen_direction = Some(dir.clone());
                }
            }
        }

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("◉ You").small().color(SPICE_GOLD));
            ui.label(egui::RichText::new("— Trail").small().color(SPICE_AMBER));
            ui.label(
                egui::RichText::new("● Hostile")
                    .small()
                    .color(CRIMSON_DANGER),
            );
            ui.label(egui::RichText::new("● Loot").small().color(SPICE_GOLD));
        });

        chosen_direction
    }
}

fn dir_delta(dir: &str) -> (i32, i32) {
    match dir {
        "north" => (0, -1),
        "south" => (0, 1),
        "east" => (1, 0),
        "west" => (-1, 0),
        "northeast" => (1, -1),
        "northwest" => (-1, -1),
        "southeast" => (1, 1),
        "southwest" => (-1, 1),
        _ => (1, -1),
    }
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
