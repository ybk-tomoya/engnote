use std::path::PathBuf;

use chrono::Local;
use eframe::egui;
use engnote_core::note::{Note, Status};
use engnote_core::vault;

fn main() -> eframe::Result<()> {
    let vault_dir = vault::resolve_vault_dir().expect("resolving vault dir");
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "engnote",
        options,
        Box::new(|_cc| Ok(Box::new(App::new(vault_dir)))),
    )
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
        let result = match &self.notes.iter().find(|(_, n)| n.frontmatter.id == note.frontmatter.id) {
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

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut action: Option<GuiAction> = None;

        if let Some(err) = &self.error {
            egui::TopBottomPanel::top("error").show(ctx, |ui| {
                ui.colored_label(egui::Color32::RED, err);
            });
        }

        egui::SidePanel::left("notes").min_width(260.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("＋ 新規").clicked() {
                    action = Some(GuiAction::StartNew);
                }
                ui.text_edit_singleline(&mut self.filter);
            });
            ui.separator();

            let filter = self.filter.to_lowercase();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, (_, note)) in self.notes.iter().enumerate() {
                    let matches = filter.is_empty()
                        || note.frontmatter.title.to_lowercase().contains(&filter)
                        || note.frontmatter.tags.iter().any(|t| t.to_lowercase().contains(&filter));
                    if !matches {
                        continue;
                    }
                    let label = format!(
                        "{}  {:<8}  {}",
                        note.short_id(),
                        note.frontmatter.status,
                        note.frontmatter.title
                    );
                    let selected = self.selected == Some(i);
                    if ui.selectable_label(selected, label).clicked() {
                        self.selected = Some(i);
                        self.mode = Mode::Viewing;
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match &mut self.mode {
            Mode::Viewing => {
                if let Some(i) = self.selected {
                    let (path, note) = &self.notes[i];
                    ui.horizontal(|ui| {
                        ui.heading(if note.frontmatter.title.is_empty() {
                            "(無題)"
                        } else {
                            &note.frontmatter.title
                        });
                        if ui.button("編集").clicked() {
                            action = Some(GuiAction::StartEdit(EditForm::from_note(path.clone(), note)));
                        }
                    });
                    ui.label(format!(
                        "status: {}    tags: [{}]",
                        note.frontmatter.status,
                        note.frontmatter.tags.join(", ")
                    ));
                    ui.label(format!(
                        "created: {}    updated: {}",
                        note.frontmatter.created.format("%Y-%m-%d %H:%M"),
                        note.frontmatter.updated.format("%Y-%m-%d %H:%M"),
                    ));
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.label(&note.body);
                    });
                } else {
                    ui.label("左の一覧からノートを選ぶか、「＋ 新規」で作成してください。");
                }
            }
            Mode::Editing(form) => {
                let is_new = form.path.is_none();
                ui.heading(if is_new { "新規ノート" } else { "ノートを編集" });
                egui::Grid::new("edit-form").num_columns(2).show(ui, |ui| {
                    ui.label("タイトル");
                    ui.text_edit_singleline(&mut form.title);
                    ui.end_row();

                    ui.label("タグ(カンマ区切り)");
                    ui.text_edit_singleline(&mut form.tags);
                    ui.end_row();

                    ui.label("ステータス");
                    egui::ComboBox::from_id_salt("status")
                        .selected_text(form.status.to_string())
                        .show_ui(ui, |ui| {
                            for s in [Status::Draft, Status::Active, Status::Archived] {
                                ui.selectable_value(&mut form.status, s, s.to_string());
                            }
                        });
                    ui.end_row();
                });

                ui.label("本文");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut form.body)
                            .desired_rows(20)
                            .desired_width(f32::INFINITY),
                    );
                });

                ui.horizontal(|ui| {
                    if ui.button("保存").clicked() {
                        action = Some(GuiAction::Save);
                    }
                    if ui.button("キャンセル").clicked() {
                        action = Some(GuiAction::Cancel);
                    }
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
