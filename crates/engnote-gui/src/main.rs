mod theme;

use std::path::PathBuf;

use chrono::Local;
use eframe::egui;
use engnote_core::note::{Note, Status};
use engnote_core::vault;

fn main() -> eframe::Result<()> {
    let vault_dir = vault::resolve_vault_dir().expect("resolving vault dir");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 680.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "engnote",
        options,
        Box::new(|cc| {
            install_japanese_font(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new(vault_dir)))
        }),
    )
}

/// Explicitly sets the Dock tile image. Needed because winit's NSApplication
/// setup on macOS resets the icon to a generic placeholder (the executable
/// name's first letter) after launch, even though the .app bundle's
/// Info.plist/AppIcon.icns are correct — see README's GUI section.
#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSString;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    // Loading a flat PNG here makes AppKit treat it as a small "badge" and
    // composite it onto the generic gray app-icon template instead of using
    // it as the icon. A real multi-resolution .icns (the bundle's own
    // Resources/AppIcon.icns) doesn't have that problem.
    let Some(icns_path) = std::env::current_exe().ok().and_then(|exe| {
        exe.parent()?
            .parent() // Contents/
            .map(|contents| contents.join("Resources/AppIcon.icns"))
    }) else {
        return;
    };
    if !icns_path.exists() {
        return;
    }
    let Some(path_str) = icns_path.to_str() else {
        return;
    };

    let ns_path = NSString::from_str(path_str);
    let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &ns_path) else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    unsafe { app.setApplicationIconImage(Some(&image)) };
}

#[cfg(not(target_os = "macos"))]
fn set_dock_icon() {}

/// egui's bundled default fonts don't cover Japanese glyphs, so text like
/// "＋ 新規" renders as tofu boxes. Fall back to a system CJK font if one can
/// be found; if not (e.g. non-macOS), just leave the defaults as-is.
fn install_japanese_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJKjp-Regular.otf",
        "C:\\Windows\\Fonts\\meiryo.ttc",
    ];

    let Some(bytes) = CANDIDATES.iter().find_map(|path| std::fs::read(path).ok()) else {
        return;
    };

    // Hiragino's baseline sits higher within its em box than the bundled
    // Latin font's, so mixed Latin+Japanese text (e.g. button labels) renders
    // with the Japanese glyphs riding noticeably above center; nudge them down.
    let font_data = egui::FontData::from_owned(bytes).tweak(egui::FontTweak {
        y_offset_factor: 0.15,
        ..Default::default()
    });

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("japanese".to_owned(), font_data.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("japanese".to_owned());
    }
    ctx.set_fonts(fonts);
}

enum Mode {
    Viewing,
    Editing(EditForm),
}

struct EditForm {
    /// `None` for a brand-new note, `Some(path)` when editing an existing one.
    path: Option<PathBuf>,
    id: String,
    created: chrono::DateTime<Local>,
    title: String,
    tags: String,
    body: String,
    status: Status,
}

impl EditForm {
    fn blank() -> Self {
        let note = Note::new(String::new(), None);
        EditForm {
            path: None,
            id: note.frontmatter.id,
            created: note.frontmatter.created,
            title: String::new(),
            tags: String::new(),
            body: String::new(),
            status: Status::Draft,
        }
    }

    fn from_note(path: PathBuf, note: &Note) -> Self {
        EditForm {
            path: Some(path),
            id: note.frontmatter.id.clone(),
            created: note.frontmatter.created,
            title: note.frontmatter.title.clone(),
            tags: note.frontmatter.tags.join(", "),
            body: note.body.clone(),
            status: note.frontmatter.status,
        }
    }

    fn into_note(self, context_path: Option<String>) -> Note {
        let tags = self
            .tags
            .split(',')
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect();
        Note {
            frontmatter: engnote_core::note::Frontmatter {
                id: self.id,
                title: self.title,
                status: self.status,
                tags,
                context_path,
                created: self.created,
                updated: Local::now(),
            },
            body: self.body,
        }
    }
}

struct App {
    vault_dir: PathBuf,
    notes: Vec<(PathBuf, Note)>,
    selected: Option<usize>,
    filter: String,
    mode: Mode,
    error: Option<String>,
    dock_icon_set: bool,
}

impl App {
    fn new(vault_dir: PathBuf) -> Self {
        let mut app = App {
            vault_dir,
            notes: Vec::new(),
            selected: None,
            filter: String::new(),
            mode: Mode::Viewing,
            error: None,
            dock_icon_set: false,
        };
        app.reload();
        app
    }

    fn reload(&mut self) {
        match vault::list(&self.vault_dir) {
            Ok(notes) => self.notes = notes,
            Err(e) => self.error = Some(format!("ノート一覧の読み込みに失敗: {e:#}")),
        }
    }

    fn select_by_id(&mut self, id: &str) {
        self.selected = self.notes.iter().position(|(_, n)| n.frontmatter.id == id);
    }

    fn save_edit(&mut self, form: EditForm) {
        let existing_context = form
            .path
            .as_ref()
            .and_then(|p| self.notes.iter().find(|(np, _)| np == p))
            .and_then(|(_, n)| n.frontmatter.context_path.clone());
        let id = form.id.clone();
        let note = form.into_note(existing_context);
        let result = match &self
            .notes
            .iter()
            .find(|(_, n)| n.frontmatter.id == note.frontmatter.id)
        {
            Some((path, _)) => vault::save_at(path, &note).map(|_| path.clone()),
            None => vault::save(&self.vault_dir, &note),
        };
        match result {
            Ok(_) => {
                self.reload();
                self.select_by_id(&id);
                self.mode = Mode::Viewing;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("保存に失敗: {e:#}")),
        }
    }
}

enum GuiAction {
    StartNew,
    StartEdit(EditForm),
    Save,
    Cancel,
}

/// Renders one note as a clickable card and returns its (post-hoc clickable) response.
fn note_card(ui: &mut egui::Ui, note: &Note, selected: bool) -> egui::Response {
    let (bg, border) = if selected {
        (theme::ACCENT.gamma_multiply(0.22), theme::ACCENT)
    } else {
        (theme::CARD_BG, theme::BORDER)
    };

    let inner = egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = if note.frontmatter.title.is_empty() {
                "(無題)"
            } else {
                note.frontmatter.title.as_str()
            };
            ui.label(egui::RichText::new(title).strong());
            ui.horizontal(|ui| {
                theme::badge(
                    ui,
                    &note.frontmatter.status.to_string(),
                    theme::status_color(note.frontmatter.status),
                );
                ui.label(
                    egui::RichText::new(note.short_id())
                        .monospace()
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
            });
            if !note.frontmatter.tags.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for t in &note.frontmatter.tags {
                        theme::badge(ui, t, theme::TEXT_DIM);
                    }
                });
            }
        });

    let response = inner.response.interact(egui::Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.dock_icon_set {
            // Called on the first frame rather than at app-creation time:
            // winit's own NSApplication setup on macOS resets the Dock icon
            // to a generic placeholder shortly after launch, so we need to
            // set it *after* that setup has run, not before.
            set_dock_icon();
            self.dock_icon_set = true;
        }

        let mut action: Option<GuiAction> = None;

        if let Some(err) = &self.error {
            egui::TopBottomPanel::top("error")
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(46, 20, 22))
                        .inner_margin(egui::Margin::symmetric(14, 8)),
                )
                .show(ctx, |ui| {
                    ui.colored_label(egui::Color32::from_rgb(255, 130, 130), format!("⚠ {err}"));
                });
        }

        egui::SidePanel::left("notes")
            .resizable(true)
            .default_width(300.0)
            .min_width(260.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_BG)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ctx, |ui| {
                theme::logo(ui);
                ui.add_space(12.0);

                if theme::primary_button(ui, "＋ 新規ノート").clicked() {
                    action = Some(GuiAction::StartNew);
                }
                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("タイトル・タグで絞り込み")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                let filter = self.filter.to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, (_, note)) in self.notes.iter().enumerate() {
                        let matches = filter.is_empty()
                            || note.frontmatter.title.to_lowercase().contains(&filter)
                            || note
                                .frontmatter
                                .tags
                                .iter()
                                .any(|t| t.to_lowercase().contains(&filter));
                        if !matches {
                            continue;
                        }
                        if note_card(ui, note, self.selected == Some(i)).clicked() {
                            self.selected = Some(i);
                            self.mode = Mode::Viewing;
                        }
                        ui.add_space(6.0);
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| match &mut self.mode {
            Mode::Viewing => {
                if let Some(i) = self.selected {
                    let (path, note) = &self.notes[i];
                    egui::ScrollArea::vertical()
                        .id_salt("view-scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(28, 24))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.heading(if note.frontmatter.title.is_empty() {
                                            "(無題)"
                                        } else {
                                            note.frontmatter.title.as_str()
                                        });
                                        ui.add_space(8.0);
                                        theme::badge(
                                            ui,
                                            &note.frontmatter.status.to_string(),
                                            theme::status_color(note.frontmatter.status),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.button("編集").clicked() {
                                                    action = Some(GuiAction::StartEdit(
                                                        EditForm::from_note(path.clone(), note),
                                                    ));
                                                }
                                            },
                                        );
                                    });

                                    if !note.frontmatter.tags.is_empty() {
                                        ui.add_space(6.0);
                                        ui.horizontal_wrapped(|ui| {
                                            for t in &note.frontmatter.tags {
                                                theme::badge(ui, t, theme::TEXT_DIM);
                                            }
                                        });
                                    }

                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "作成 {}    更新 {}",
                                            note.frontmatter.created.format("%Y-%m-%d %H:%M"),
                                            note.frontmatter.updated.format("%Y-%m-%d %H:%M"),
                                        ))
                                        .small()
                                        .color(theme::TEXT_DIM),
                                    );

                                    ui.add_space(16.0);
                                    egui::Frame::new()
                                        .fill(theme::CARD_BG)
                                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                        .corner_radius(egui::CornerRadius::same(10))
                                        .inner_margin(egui::Margin::same(18))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.label(&note.body);
                                        });
                                });
                        });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(
                                "左の一覧からノートを選ぶか、「＋ 新規ノート」で作成してください",
                            )
                            .color(theme::TEXT_DIM),
                        );
                    });
                }
            }
            Mode::Editing(form) => {
                egui::ScrollArea::vertical()
                    .id_salt("edit-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(28, 24))
                            .show(ui, |ui| {
                                let is_new = form.path.is_none();
                                ui.heading(if is_new {
                                    "新規ノート"
                                } else {
                                    "ノートを編集"
                                });
                                ui.add_space(16.0);

                                ui.label(
                                    egui::RichText::new("タイトル")
                                        .small()
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut form.title)
                                        .desired_width(f32::INFINITY),
                                );
                                ui.add_space(10.0);

                                ui.label(
                                    egui::RichText::new("タグ(カンマ区切り)")
                                        .small()
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut form.tags)
                                        .desired_width(f32::INFINITY),
                                );
                                ui.add_space(10.0);

                                ui.label(
                                    egui::RichText::new("ステータス")
                                        .small()
                                        .color(theme::TEXT_DIM),
                                );
                                egui::ComboBox::from_id_salt("status")
                                    .selected_text(form.status.to_string())
                                    .show_ui(ui, |ui| {
                                        for s in [Status::Draft, Status::Active, Status::Archived] {
                                            ui.selectable_value(&mut form.status, s, s.to_string());
                                        }
                                    });
                                ui.add_space(16.0);

                                ui.label(
                                    egui::RichText::new("本文").small().color(theme::TEXT_DIM),
                                );
                                egui::Frame::new()
                                    .fill(theme::CARD_BG)
                                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                    .corner_radius(egui::CornerRadius::same(8))
                                    .inner_margin(egui::Margin::same(10))
                                    .show(ui, |ui| {
                                        egui::ScrollArea::vertical()
                                            .id_salt("edit-body-scroll")
                                            .max_height(360.0)
                                            .show(ui, |ui| {
                                                ui.add(
                                                    egui::TextEdit::multiline(&mut form.body)
                                                        .frame(false)
                                                        .desired_rows(18)
                                                        .desired_width(f32::INFINITY),
                                                );
                                            });
                                    });

                                ui.add_space(18.0);
                                ui.horizontal(|ui| {
                                    if theme::primary_button(ui, "保存").clicked() {
                                        action = Some(GuiAction::Save);
                                    }
                                    if ui.button("キャンセル").clicked() {
                                        action = Some(GuiAction::Cancel);
                                    }
                                });
                            });
                    });
            }
        });

        match action {
            Some(GuiAction::StartNew) => {
                self.mode = Mode::Editing(EditForm::blank());
                self.selected = None;
            }
            Some(GuiAction::StartEdit(form)) => self.mode = Mode::Editing(form),
            Some(GuiAction::Save) => {
                if let Mode::Editing(form) = std::mem::replace(&mut self.mode, Mode::Viewing) {
                    self.save_edit(form);
                }
            }
            Some(GuiAction::Cancel) => self.mode = Mode::Viewing,
            None => {}
        }
    }
}
