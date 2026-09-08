use std::path::PathBuf;

use eframe::egui;
use engnote_core::note::Note;

use crate::theme;

const FIELD_ID: &str = "engnote-quick-open";

/// State for the Cmd+P "open a note" overlay.
pub struct QuickOpen {
    pub query: String,
    pub selected: usize,
}

impl QuickOpen {
    pub fn new() -> Self {
        QuickOpen { query: String::new(), selected: 0 }
    }
}

pub enum Action {
    /// Open in the active tab (reuses it, same policy as sidebar clicks).
    Open(usize),
    /// Open in a brand-new tab (Cmd+Enter / Cmd+click on a result).
    OpenInNewTab(usize),
    Cancel,
}

/// Indices into `notes` matching `query` (case-insensitive substring on
/// title, falling back to tags), title matches ranked ahead of tag-only
/// matches and by how early the match starts.
fn matches(notes: &[(PathBuf, Note)], query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    let mut scored: Vec<(usize, usize)> = notes
        .iter()
        .enumerate()
        .filter_map(|(i, (_, n))| {
            if q.is_empty() {
                return Some((i, 0));
            }
            let title = n.frontmatter.title.to_lowercase();
            if let Some(pos) = title.find(&q) {
                return Some((i, pos));
            }
            if n.frontmatter.tags.iter().any(|t| t.to_lowercase().contains(&q)) {
                return Some((i, usize::MAX / 2));
            }
            None
        })
        .collect();
    scored.sort_by_key(|&(_, score)| score);
    scored.into_iter().map(|(i, _)| i).take(50).collect()
}

/// Draws the overlay and handles its keyboard input (arrows/Enter/Esc) for
/// this frame. Call only while the palette is open; the caller owns
/// opening/closing `Option<QuickOpen>` based on the returned `Action`.
pub fn render(ctx: &egui::Context, state: &mut QuickOpen, notes: &[(PathBuf, Note)]) -> Option<Action> {
    let mut action = None;
    let ids = matches(notes, &state.query);
    if !ids.is_empty() {
        state.selected = state.selected.min(ids.len() - 1);
    } else {
        state.selected = 0;
    }

    // Dim the rest of the window so the palette reads as a modal.
    egui::Area::new(egui::Id::new("quick-open-scrim"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::Pos2::ZERO)
        .show(ctx, |ui| {
            let screen = ctx.screen_rect();
            ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(120));
        });

    egui::Area::new(egui::Id::new("quick-open-overlay"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 90.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(480.0);
                    let field_id = egui::Id::new(FIELD_ID);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut state.query)
                            .id(field_id)
                            .hint_text("ノートを開く…")
                            .desired_width(f32::INFINITY),
                    );
                    if ctx.memory(|m| m.focused()) != Some(field_id) {
                        resp.request_focus();
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        if ids.is_empty() {
                            ui.label(
                                egui::RichText::new("一致するノートがありません").color(theme::TEXT_DIM),
                            );
                            return;
                        }
                        for (row, &idx) in ids.iter().enumerate() {
                            let note = &notes[idx].1;
                            let title = if note.frontmatter.title.is_empty() {
                                "(無題)"
                            } else {
                                note.frontmatter.title.as_str()
                            };
                            let selected = row == state.selected;
                            let bg = if selected {
                                theme::ACCENT.gamma_multiply(0.22)
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            let resp = egui::Frame::new()
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(8, 6))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(egui::RichText::new(title).color(theme::TEXT));
                                })
                                .response
                                .interact(egui::Sense::click());
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if resp.clicked() {
                                action = Some(if ui.input(|i| i.modifiers.command) {
                                    Action::OpenInNewTab(idx)
                                } else {
                                    Action::Open(idx)
                                });
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("↑↓ 選択 ・ Enter 開く ・ ⌘Enter 新規タブ ・ Esc 閉じる")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                });
        });

    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) && !ids.is_empty() {
            state.selected = (state.selected + 1).min(ids.len() - 1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            state.selected = state.selected.saturating_sub(1);
        }
        if i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter) {
            if let Some(&idx) = ids.get(state.selected) {
                action = Some(Action::OpenInNewTab(idx));
            }
        } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
            if let Some(&idx) = ids.get(state.selected) {
                action = Some(Action::Open(idx));
            }
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            action = Some(Action::Cancel);
        }
    });

    action
}
