use eframe::egui;

pub const BG: egui::Color32 = egui::Color32::from_rgb(13, 13, 15);
pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(19, 19, 22);
pub const CARD_BG: egui::Color32 = egui::Color32::from_rgb(24, 24, 28);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(42, 42, 48);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(230, 230, 236);
pub const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(142, 142, 152);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(138, 124, 255);
/// Used for the "-- INSERT --" indicator in the vim status bar.
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(88, 214, 155);

/// Draws the "engnote" wordmark: a small notepad-style mark (a few
/// horizontal "lines of text" on an accent square) plus a tracked-out
/// subtitle underneath the name.
pub fn logo(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let size = 32.0;
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, egui::CornerRadius::same(9), ACCENT);

        let mark_color = egui::Color32::from_rgb(18, 15, 32);
        let pad = 8.0;
        let inner_w = size - pad * 2.0;
        for (i, w) in [0.62_f32, 0.62, 0.38].into_iter().enumerate() {
            let top = rect.top() + pad + 1.0 + i as f32 * 5.4;
            let line = egui::Rect::from_min_size(
                egui::pos2(rect.left() + pad, top),
                egui::vec2(inner_w * w, 2.6),
            );
            painter.rect_filled(line, egui::CornerRadius::same(2), mark_color);
        }

        ui.add_space(9.0);
        ui.vertical(|ui| {
            ui.add_space(1.0);
            ui.label(
                egui::RichText::new("engnote")
                    .size(19.0)
                    .strong()
                    .color(TEXT),
            );
            ui.label(
                egui::RichText::new("ENGINEER'S NOTES")
                    .size(9.0)
                    .extra_letter_spacing(1.6)
                    .color(TEXT_DIM),
            );
        });
    });
}

/// A small pill-shaped, colored label (used for tags).
pub fn badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.55)))
        .corner_radius(egui::CornerRadius::same(20))
        .inner_margin(egui::Margin::symmetric(9, 2))
        .show(ui, |ui| {
            ui.colored_label(color, text);
        });
}

/// A filled, accent-colored button for the primary action in a view (save, create, ...).
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(text)
                .color(egui::Color32::from_rgb(12, 12, 14))
                .strong(),
        )
        .fill(ACCENT)
        .corner_radius(egui::CornerRadius::same(8)),
    )
}

/// Sets up a dark, near-black theme with a single violet accent color.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    visuals.window_fill = BG;
    visuals.panel_fill = BG;
    visuals.faint_bg_color = PANEL_BG;
    visuals.extreme_bg_color = egui::Color32::from_rgb(8, 8, 10);
    visuals.code_bg_color = CARD_BG;
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    visuals.menu_corner_radius = egui::CornerRadius::same(8);

    let corner_radius = egui::CornerRadius::same(8);

    visuals.widgets.noninteractive.bg_fill = BG;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_DIM);
    visuals.widgets.noninteractive.corner_radius = corner_radius;

    visuals.widgets.inactive.bg_fill = CARD_BG;
    visuals.widgets.inactive.weak_bg_fill = CARD_BG;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = corner_radius;

    visuals.widgets.hovered.bg_fill = ACCENT.gamma_multiply(0.28);
    visuals.widgets.hovered.weak_bg_fill = ACCENT.gamma_multiply(0.22);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = corner_radius;

    // NB: `visuals.widgets.active.fg_stroke` also backs `strong_text_color()`
    // (used by `RichText::strong()` all over the app, e.g. note titles), so
    // this must stay a normal readable color, not the dark-on-accent color
    // `primary_button` needs — that button sets its own text color directly.
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.active.corner_radius = corner_radius;

    visuals.widgets.open = visuals.widgets.hovered;

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(14);
    style.spacing.indent = 16.0;

    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(21.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(14.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.5, FontFamily::Monospace),
        ),
    ]
    .into();

    ctx.set_style(style);
}
