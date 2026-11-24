use eframe::egui::{self, Color32, FontId, Rounding, Sense, Stroke, Vec2};

pub const BG_OBSIDIAN: Color32 = Color32::from_rgb(11, 13, 19);
pub const PANEL_SLATE: Color32 = Color32::from_rgb(18, 22, 32);
pub const CARD_BG: Color32 = Color32::from_rgb(25, 30, 44);
pub const CARD_ELEVATED: Color32 = Color32::from_rgb(31, 38, 56);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(48, 57, 78);

pub const SPICE_AMBER: Color32 = Color32::from_rgb(234, 164, 58);
pub const SPICE_GOLD: Color32 = Color32::from_rgb(250, 204, 21);
pub const FREMEN_CYAN: Color32 = Color32::from_rgb(56, 189, 248);
pub const EMERALD_HP: Color32 = Color32::from_rgb(34, 197, 94);
pub const CRIMSON_DANGER: Color32 = Color32::from_rgb(239, 68, 68);
pub const TEXT_SAND: Color32 = Color32::from_rgb(236, 229, 216);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(148, 163, 184);

pub fn apply_modern_mud_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = BG_OBSIDIAN;
    visuals.panel_fill = PANEL_SLATE;
    visuals.faint_bg_color = CARD_BG;
    visuals.extreme_bg_color = Color32::from_rgb(8, 10, 15);

    visuals.widgets.noninteractive.bg_fill = PANEL_SLATE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_SAND);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    visuals.widgets.noninteractive.rounding = Rounding::same(6.0);

    visuals.widgets.inactive.bg_fill = CARD_ELEVATED;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_SAND);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    visuals.widgets.inactive.rounding = Rounding::same(5.0);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(46, 56, 82);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.2_f32, SPICE_GOLD);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, SPICE_AMBER);
    visuals.widgets.hovered.rounding = Rounding::same(5.0);

    visuals.widgets.active.bg_fill = Color32::from_rgb(74, 52, 20);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, SPICE_GOLD);
    visuals.widgets.active.rounding = Rounding::same(5.0);

    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(234, 164, 58, 75);
    visuals.selection.stroke = Stroke::new(1.0_f32, SPICE_GOLD);

    ctx.set_visuals(visuals);
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .rounding(Rounding::same(7.0))
        .inner_margin(egui::Margin::same(10.0))
}

pub fn draw_hp_bar(ui: &mut egui::Ui, hp: i32, max_hp: i32, status_label: &str, width: f32) {
    let max_f = max_hp.max(1) as f32;
    let cur_f = hp.clamp(0, max_hp) as f32;
    let ratio = cur_f / max_f;

    let bar_color = if ratio > 0.55 {
        EMERALD_HP
    } else if ratio > 0.25 {
        SPICE_AMBER
    } else {
        CRIMSON_DANGER
    };

    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(13, 16, 23));
    let mut fill = rect;
    fill.set_width(rect.width() * ratio);
    painter.rect_filled(fill, Rounding::same(4.0), bar_color);
    painter.rect_stroke(
        rect,
        Rounding::same(4.0),
        Stroke::new(1.0_f32, BORDER_SUBTLE),
    );

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("HP {hp}/{max_hp} [{}]", status_label.to_uppercase()),
        FontId::monospace(11.0),
        Color32::WHITE,
    );
}
