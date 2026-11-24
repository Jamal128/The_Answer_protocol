use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Sense, Shape, Stroke, Vec2};

use crate::theme::{CRIMSON_DANGER, FREMEN_CYAN, SPICE_AMBER, SPICE_GOLD};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityArchetype {
    PlayerPilot,
    Sandworm,
    HostileRaider,
    QuestGiver,
    AllyGuard,
}

impl EntityArchetype {
    #[must_use]
    pub fn from_npc_id(npc_id: &str) -> Self {
        let lower = npc_id.to_ascii_lowercase();
        if lower.contains("sandworm") || lower.contains("worm") || lower.contains("shai") {
            Self::Sandworm
        } else if lower.contains("harkonnen")
            || lower.contains("goblin")
            || lower.contains("skeleton")
            || lower.contains("enemy")
        {
            Self::HostileRaider
        } else if lower.contains("merchant") || lower.contains("stilgar") || lower.contains("quest")
        {
            Self::QuestGiver
        } else {
            Self::AllyGuard
        }
    }

    #[must_use]
    pub const fn accent_color(self) -> Color32 {
        match self {
            Self::PlayerPilot => FREMEN_CYAN,
            Self::Sandworm | Self::HostileRaider => CRIMSON_DANGER,
            Self::QuestGiver => SPICE_GOLD,
            Self::AllyGuard => Color32::from_rgb(96, 165, 250),
        }
    }

    #[must_use]
    pub const fn role_badge(self) -> &'static str {
        match self {
            Self::PlayerPilot => "PILOT",
            Self::Sandworm => "APEX BEAST",
            Self::HostileRaider => "HOSTILE",
            Self::QuestGiver => "CONTRACT",
            Self::AllyGuard => "ALLY",
        }
    }
}

pub fn draw_portrait(ui: &mut egui::Ui, archetype: EntityArchetype, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    let accent = archetype.accent_color();
    let c = rect.center();
    let r = size * 0.44;

    // Portrait frame background
    painter.rect_filled(rect, Rounding::same(8.0), Color32::from_rgb(13, 17, 26));
    painter.rect_stroke(rect, Rounding::same(8.0), Stroke::new(1.5_f32, accent));

    match archetype {
        EntityArchetype::PlayerPilot => {
            // Stillsuit desert hood
            let hood = vec![
                Pos2::new(c.x, c.y - r * 0.85),
                Pos2::new(c.x + r * 0.78, c.y + r * 0.75),
                Pos2::new(c.x - r * 0.78, c.y + r * 0.75),
            ];
            painter.add(Shape::convex_polygon(
                hood,
                Color32::from_rgb(32, 46, 68),
                Stroke::new(1.2_f32, FREMEN_CYAN),
            ));
            // Face shadow & glowing blue-within-blue eyes
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::new(r * 0.85, r * 0.42)),
                Rounding::same(3.0),
                Color32::from_rgb(12, 16, 24),
            );
            painter.circle_filled(
                Pos2::new(c.x - r * 0.2, c.y - r * 0.02),
                r * 0.11,
                FREMEN_CYAN,
            );
            painter.circle_filled(
                Pos2::new(c.x + r * 0.2, c.y - r * 0.02),
                r * 0.11,
                FREMEN_CYAN,
            );
        }
        EntityArchetype::Sandworm => {
            // Concentric segmented sandworm maw
            painter.circle_filled(c, r * 0.85, Color32::from_rgb(115, 58, 22));
            painter.circle_stroke(c, r * 0.85, Stroke::new(1.6_f32, SPICE_AMBER));
            painter.circle_filled(c, r * 0.56, Color32::from_rgb(45, 14, 14));
            painter.circle_stroke(c, r * 0.56, Stroke::new(1.4_f32, CRIMSON_DANGER));
            // Radial crystalline teeth
            for i in 0..8 {
                let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
                let outer = Pos2::new(c.x + angle.cos() * r * 0.54, c.y + angle.sin() * r * 0.54);
                let inner = Pos2::new(c.x + angle.cos() * r * 0.24, c.y + angle.sin() * r * 0.24);
                painter.line_segment(
                    [outer, inner],
                    Stroke::new(1.6_f32, Color32::from_rgb(245, 240, 225)),
                );
            }
        }
        EntityArchetype::HostileRaider => {
            // Harkonnen angular helmet & crimson visor
            let helm = vec![
                Pos2::new(c.x - r * 0.65, c.y - r * 0.6),
                Pos2::new(c.x + r * 0.65, c.y - r * 0.6),
                Pos2::new(c.x + r * 0.5, c.y + r * 0.7),
                Pos2::new(c.x - r * 0.5, c.y + r * 0.7),
            ];
            painter.add(Shape::convex_polygon(
                helm,
                Color32::from_rgb(38, 20, 26),
                Stroke::new(1.4_f32, CRIMSON_DANGER),
            ));
            painter.line_segment(
                [
                    Pos2::new(c.x - r * 0.38, c.y - r * 0.1),
                    Pos2::new(c.x + r * 0.38, c.y - r * 0.1),
                ],
                Stroke::new(3.0_f32, CRIMSON_DANGER),
            );
        }
        EntityArchetype::QuestGiver => {
            // Golden spice cowl & glowing emblem
            painter.circle_filled(c, r * 0.75, Color32::from_rgb(56, 42, 18));
            painter.circle_stroke(c, r * 0.75, Stroke::new(1.5_f32, SPICE_GOLD));
            painter.circle_filled(
                Pos2::new(c.x - r * 0.2, c.y - r * 0.05),
                r * 0.1,
                FREMEN_CYAN,
            );
            painter.circle_filled(
                Pos2::new(c.x + r * 0.2, c.y - r * 0.05),
                r * 0.1,
                FREMEN_CYAN,
            );
            painter.circle_filled(Pos2::new(c.x, c.y + r * 0.36), r * 0.14, SPICE_GOLD);
        }
        EntityArchetype::AllyGuard => {
            // Atreides crest & visor
            painter.circle_filled(c, r * 0.72, Color32::from_rgb(24, 38, 58));
            painter.circle_stroke(c, r * 0.72, Stroke::new(1.4_f32, accent));
            painter.line_segment(
                [
                    Pos2::new(c.x - r * 0.34, c.y - r * 0.08),
                    Pos2::new(c.x + r * 0.34, c.y - r * 0.08),
                ],
                Stroke::new(2.5_f32, FREMEN_CYAN),
            );
        }
    }
}
