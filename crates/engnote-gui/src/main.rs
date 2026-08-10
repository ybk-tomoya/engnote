mod markdown;
mod slash;
mod theme;
mod vim;

use std::path::PathBuf;

use chrono::Local;
use eframe::egui;
use engnote_core::note::{Note, Status};
use engnote_core::vault;

/// Id of the sidebar filter `TextEdit`, so global key handling can tell
/// whether it currently has focus (and can request/surrender that focus).
const FILTER_ID: &str = "engnote-filter";
/// Id of the body `TextEdit` shown in vim Insert mode; used to read back its
/// live cursor position when leaving Insert mode (see `EditForm::cursor`).
const EDIT_BODY_ID: &str = "engnote-edit-body";
/// Same idea as `EDIT_BODY_ID`, for the title and tags fields.
const TITLE_FIELD_ID: &str = "engnote-edit-title";
const TAGS_FIELD_ID: &str = "engnote-edit-tags";
/// Id of the `:command` line's `TextEdit` in the bottom status bar.
const COMMAND_ID: &str = "engnote-command";

fn main() -> eframe::Result<()> {
    let vault_dir = vault::resolve_vault_dir().expect("resolving vault dir");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 900.0])
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

/// Which field of the edit form vim motions currently apply to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveField {
    Title,
    Tags,
    Body,
}

impl ActiveField {
    fn id(self) -> egui::Id {
        egui::Id::new(match self {
            ActiveField::Title => TITLE_FIELD_ID,
            ActiveField::Tags => TAGS_FIELD_ID,
            ActiveField::Body => EDIT_BODY_ID,
        })
    }
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
    /// Which field (`title`/`tags`/`body`) vim motions currently target.
    active_field: ActiveField,
    /// vim-style Normal/Insert mode for the active field (see `vim.rs`).
    editor_mode: vim::EditorMode,
    /// Cursor position within the active field, as a *character* (not byte)
    /// index. Authoritative in Normal mode; synced from the live `TextEdit`
    /// on entering/leaving Insert mode (see `App::handle_editing_keys`).
    cursor: usize,
    /// Set for exactly one frame after switching to Insert mode, so the
    /// active field's `TextEdit` picks up `cursor`/focus once without
    /// fighting the user's subsequent typing every frame after.
    just_entered_insert: bool,
    pending_g: bool,
    pending_d: bool,
    /// `Some` while the `/` command menu (Notion-style block insertion) is
    /// open over the body field — see `update_slash_menu`.
    slash_menu: Option<SlashMenu>,
}

/// State for the `/` command menu: which query it's currently filtering on
/// and which of the filtered matches is highlighted.
struct SlashMenu {
    /// Char index into `body` right after the triggering `/`, i.e. where the
    /// typed filter query starts.
    query_start: usize,
    /// Text typed after `/` so far, used to filter `slash::COMMANDS`.
    query: String,
    /// Index into the *filtered* match list (`slash::matching(&query)`), not
    /// into `slash::COMMANDS` itself.
    selected: usize,
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
            // A brand-new note has nothing to edit yet, so start at the top.
            active_field: ActiveField::Title,
            editor_mode: vim::EditorMode::Normal,
            cursor: 0,
            just_entered_insert: false,
            pending_g: false,
            pending_d: false,
            slash_menu: None,
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
            // Editing an existing note: jump straight to its content.
            active_field: ActiveField::Body,
            editor_mode: vim::EditorMode::Normal,
            cursor: 0,
            just_entered_insert: false,
            pending_g: false,
            pending_d: false,
            slash_menu: None,
        }
    }

    fn active_field_len(&self) -> usize {
        match self.active_field {
            ActiveField::Title => self.title.chars().count(),
            ActiveField::Tags => self.tags.chars().count(),
            ActiveField::Body => self.body.chars().count(),
        }
    }

    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.active_field_len());
    }

    /// Full wrap-around cycle (Title -> Tags -> Body -> Title), used by `Tab`.
    fn cycle_field(&mut self, forward: bool) {
        use ActiveField::*;
        self.active_field = match (self.active_field, forward) {
            (Title, true) => Tags,
            (Tags, true) => Body,
            (Body, true) => Title,
            (Title, false) => Body,
            (Tags, false) => Title,
            (Body, false) => Tags,
        };
        self.clamp_cursor();
        self.slash_menu = None;
    }

    /// `j`/`k` between Title/Tags only (Body has real vertical motion, and
    /// isn't reachable "downward" from here — `Tab` reaches it explicitly).
    /// Clamps at Title rather than wrapping, since real vim doesn't wrap at
    /// the top of a buffer either.
    fn step_field(&mut self, forward: bool) {
        use ActiveField::*;
        self.active_field = match (self.active_field, forward) {
            (Title, true) => Tags,
            (Tags, true) => Body,
            (Tags, false) => Title,
            (other, _) => other,
        };
        self.clamp_cursor();
        self.slash_menu = None;
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
    /// `Some(buffer)` while the `:command` line (opened with `:`) is active.
    command_input: Option<String>,
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
            command_input: None,
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

    /// Moves `self.selected` up/down by one within the currently filtered
    /// note list (`j`/`k`).
    fn select_relative(&mut self, delta: i32) {
        let filter = self.filter.to_lowercase();
        let visible: Vec<usize> = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| {
                filter.is_empty()
                    || n.frontmatter.title.to_lowercase().contains(&filter)
                    || n.frontmatter.tags.iter().any(|t| t.to_lowercase().contains(&filter))
            })
            .map(|(i, _)| i)
            .collect();
        if visible.is_empty() {
            return;
        }
        let current = self.selected.and_then(|s| visible.iter().position(|&i| i == s));
        let next = match current {
            Some(p) => (p as i32 + delta).clamp(0, visible.len() as i32 - 1) as usize,
            None => 0,
        };
        self.selected = Some(visible[next]);
    }

    /// App-wide vim-style keyboard shortcuts. Skipped whenever some other
    /// widget (a plain text field) currently wants the keyboard, so normal
    /// typing in the title/tags/filter fields is unaffected.
    fn handle_global_keys(&mut self, ctx: &egui::Context) {
        let filter_id = egui::Id::new(FILTER_ID);
        if ctx.memory(|m| m.focused()) == Some(filter_id) {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ctx.memory_mut(|m| m.surrender_focus(filter_id));
            }
            return;
        }

        if self.command_input.is_some() {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.command_input = None;
            } else if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                let cmd = self.command_input.take().unwrap_or_default();
                self.execute_command(&cmd);
            }
            return;
        }

        if !ctx.wants_keyboard_input() && consume_text_event(ctx, ':') {
            self.command_input = Some(String::new());
            return;
        }

        if matches!(self.mode, Mode::Editing(_)) {
            self.handle_editing_keys(ctx);
        } else {
            self.handle_viewing_keys(ctx, filter_id);
        }
    }

    /// Executes a vim-style `:command` (see the bottom command line).
    fn execute_command(&mut self, cmd: &str) {
        match cmd.trim() {
            "e" | "new" | "enew" => {
                self.mode = Mode::Editing(EditForm::blank());
                self.selected = None;
            }
            "w" | "wq" | "x" => {
                if let Mode::Editing(form) = std::mem::replace(&mut self.mode, Mode::Viewing) {
                    self.save_edit(form);
                }
            }
            _ => {}
        }
    }

    fn handle_viewing_keys(&mut self, ctx: &egui::Context, filter_id: egui::Id) {
        if ctx.wants_keyboard_input() {
            return;
        }
        for ch in collect_text_chars(ctx) {
            match ch {
                'j' => self.select_relative(1),
                'k' => self.select_relative(-1),
                'n' => {
                    self.mode = Mode::Editing(EditForm::blank());
                    self.selected = None;
                    return;
                }
                'e' | 'i' => {
                    if let Some(idx) = self.selected {
                        let path = self.notes[idx].0.clone();
                        let note = self.notes[idx].1.clone();
                        self.mode = Mode::Editing(EditForm::from_note(path, &note));
                        return;
                    }
                }
                '/' => {
                    // Consume this event so the `/` doesn't also land as the
                    // filter's first typed character once it gets focus.
                    consume_text_event(ctx, '/');
                    ctx.memory_mut(|m| m.request_focus(filter_id));
                    return;
                }
                _ => {}
            }
        }
    }

    fn handle_editing_keys(&mut self, ctx: &egui::Context) {
        enum Intent {
            None,
            ExitToViewing,
            Save,
        }
        let mut intent = Intent::None;

        if let Mode::Editing(form) = &mut self.mode {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
                intent = Intent::Save;
            } else if form.editor_mode == vim::EditorMode::Insert {
                if form.active_field == ActiveField::Body && form.slash_menu.is_some() {
                    // The `/` menu is open: steal Up/Down/Enter/Escape from the
                    // live body `TextEdit` (which would otherwise move its real
                    // cursor or insert a newline) to drive the menu instead.
                    // Everything else (ordinary typing) still falls through to
                    // the `TextEdit` below, which is what keeps the query text
                    // in sync as the user types.
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                        form.slash_menu = None;
                    } else if ctx
                        .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
                    {
                        move_slash_selection(form, 1);
                    } else if ctx
                        .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
                    {
                        move_slash_selection(form, -1);
                    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                    {
                        if !apply_slash_command(form) {
                            // Nothing matched the typed query (e.g. the user
                            // is just writing a `/etc/hosts`-style path, not
                            // a command): close the menu and fall back to an
                            // ordinary newline, since we already stole this
                            // Enter from the `TextEdit` above.
                            close_slash_menu_with_newline(form);
                        }
                    }
                } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    let field_id = form.active_field.id();
                    if let Some(range) = egui::text_edit::TextEditState::load(ctx, field_id)
                        .and_then(|state| state.cursor.char_range())
                    {
                        form.cursor = range.primary.index;
                    }
                    form.editor_mode = vim::EditorMode::Normal;
                }
            } else if !ctx.wants_keyboard_input() {
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    intent = Intent::ExitToViewing;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
                    let forward = !ctx.input(|i| i.modifiers.shift);
                    form.cycle_field(forward);
                } else {
                    for ch in collect_text_chars(ctx) {
                        // On Title/Tags (single-line fields), `j`/`k` step
                        // between fields instead of the usual vertical
                        // motion, which would be a no-op there anyway.
                        if form.active_field != ActiveField::Body && (ch == 'j' || ch == 'k') {
                            form.step_field(ch == 'j');
                            continue;
                        }
                        let text = match form.active_field {
                            ActiveField::Title => &mut form.title,
                            ActiveField::Tags => &mut form.tags,
                            ActiveField::Body => &mut form.body,
                        };
                        vim::handle_normal_key(
                            text,
                            &mut form.cursor,
                            &mut form.pending_g,
                            &mut form.pending_d,
                            &mut form.editor_mode,
                            ch,
                        );
                    }
                    if form.editor_mode == vim::EditorMode::Insert {
                        form.just_entered_insert = true;
                        // Don't let the `i`/`a`/`o`/`O` that opened Insert
                        // mode also get typed into the field's TextEdit once
                        // it gains focus later this same frame.
                        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
                    }
                }
            }
        }

        match intent {
            Intent::ExitToViewing => self.mode = Mode::Viewing,
            Intent::Save => {
                if let Mode::Editing(form) = std::mem::replace(&mut self.mode, Mode::Viewing) {
                    self.save_edit(form);
                }
            }
            Intent::None => {}
        }
    }
}

/// Re-derives `slash_menu`'s open/closed state and filter query from the
/// live cursor position; called every frame the body field is shown in
/// Insert mode (see `main.rs`'s body-editing block). Notion-style trigger:
/// only fires when `/` is the very first character of the current line (not,
/// say, inside a URL pasted into the body), and only while nothing after it
/// is whitespace — finishing the query with a space just types a literal
/// space and closes the menu, as in most editors.
fn update_slash_menu(body: &str, cursor: Option<usize>, slash_menu: &mut Option<SlashMenu>) {
    let Some(cursor) = cursor else {
        *slash_menu = None;
        return;
    };
    let chars: Vec<char> = body.chars().collect();
    let cursor = cursor.min(chars.len());
    let line_start = chars[..cursor]
        .iter()
        .rposition(|&c| c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_prefix: String = chars[line_start..cursor].iter().collect();
    let query = match line_prefix.strip_prefix('/') {
        Some(q) if !q.chars().any(char::is_whitespace) => q,
        _ => {
            *slash_menu = None;
            return;
        }
    };
    let query_start = line_start + 1;
    let match_count = slash::matching(query).len();
    let selected = match slash_menu {
        Some(menu) if menu.query_start == query_start => menu.selected,
        _ => 0,
    }
    .min(match_count.saturating_sub(1));
    *slash_menu = Some(SlashMenu { query_start, query: query.to_string(), selected });
}

/// Moves the highlighted match up/down (wrapping), clamped to however many
/// commands currently match the typed query.
fn move_slash_selection(form: &mut EditForm, delta: i32) {
    let Some(menu) = &mut form.slash_menu else { return };
    let count = slash::matching(&menu.query).len();
    if count == 0 {
        return;
    }
    menu.selected = (menu.selected as i32 + delta).rem_euclid(count as i32) as usize;
}

/// Replaces `/query` (the triggering `/` through the current cursor) with
/// the highlighted match's snippet, and positions the cursor within it.
/// Returns `false` (menu left untouched) if nothing matches the typed
/// query — the caller decides what a "confirm" keystroke should do then
/// (see `close_slash_menu_with_newline`).
fn apply_slash_command(form: &mut EditForm) -> bool {
    let Some(existing) = &form.slash_menu else { return false };
    let matches = slash::matching(&existing.query);
    let Some(cmd) = matches.get(existing.selected).or_else(|| matches.first()).copied() else {
        return false;
    };
    let menu = form.slash_menu.take().expect("checked Some above");
    insert_slash_command(form, &menu, cmd);
    true
}

/// Closes the `/` menu without applying a command and inserts a plain
/// newline at the query's end, so an Enter that matched nothing (e.g. the
/// user typed a `/etc/hosts`-style path rather than a command) still behaves
/// like a normal newline instead of silently vanishing — we've already
/// stolen that Enter keystroke from the body `TextEdit`.
fn close_slash_menu_with_newline(form: &mut EditForm) {
    let Some(menu) = form.slash_menu.take() else { return };
    let mut chars: Vec<char> = form.body.chars().collect();
    let pos = (menu.query_start + menu.query.chars().count()).min(chars.len());
    chars.insert(pos, '\n');
    form.body = chars.into_iter().collect();
    form.cursor = pos + 1;
    form.just_entered_insert = true;
}

/// Like `apply_slash_command`, but for an explicit filtered-list index
/// (used when a menu item is clicked rather than confirmed with Enter).
fn apply_slash_command_at(form: &mut EditForm, index: usize) {
    let Some(menu) = form.slash_menu.take() else { return };
    let matches = slash::matching(&menu.query);
    let Some(cmd) = matches.get(index) else { return };
    insert_slash_command(form, &menu, cmd);
}

fn insert_slash_command(form: &mut EditForm, menu: &SlashMenu, cmd: &slash::SlashCommand) {
    let mut chars: Vec<char> = form.body.chars().collect();
    let replace_start = menu.query_start - 1; // include the leading '/'
    let replace_end = (menu.query_start + menu.query.chars().count()).min(chars.len());
    chars.splice(replace_start..replace_end, cmd.insert.chars());
    form.body = chars.into_iter().collect();
    form.cursor = replace_start + cmd.cursor_offset;
    // Reuse the same mechanism vim Insert-mode entry uses to hand the live
    // `TextEdit` a specific cursor position next frame (see `main.rs`'s body
    // rendering block).
    form.just_entered_insert = true;
}

/// Renders the `/` command popup near the text cursor; returns the filtered
/// index of a clicked item, if any. Returns rather than applies directly
/// because the body `TextEdit` shown just above still owns the only mutable
/// borrow of `form.body`/`form.cursor` this frame — the caller applies it
/// once that borrow has ended.
fn show_slash_menu(
    ui: &mut egui::Ui,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    menu: &SlashMenu,
) -> Option<usize> {
    let cursor_idx = menu.query_start + menu.query.chars().count();
    let cursor_rect = galley
        .pos_from_cursor(egui::text::CCursor::new(cursor_idx))
        .translate(galley_pos.to_vec2());
    let pos = egui::pos2(cursor_rect.left(), cursor_rect.bottom() + 4.0);

    let matches = slash::matching(&menu.query);
    let mut clicked = None;
    egui::Area::new(egui::Id::new("engnote-slash-menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(theme::CARD_BG)
                .stroke(egui::Stroke::new(1.0, theme::ACCENT))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(220.0);
                    if matches.is_empty() {
                        ui.label(
                            egui::RichText::new("一致するコマンドなし")
                                .small()
                                .color(theme::TEXT_DIM),
                        );
                        return;
                    }
                    for (i, cmd) in matches.iter().enumerate() {
                        let bg = if i == menu.selected {
                            theme::ACCENT.gamma_multiply(0.25)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        let resp = egui::Frame::new()
                            .fill(bg)
                            .corner_radius(egui::CornerRadius::same(5))
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(cmd.label).color(theme::TEXT).strong(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(cmd.hint)
                                                    .small()
                                                    .monospace()
                                                    .color(theme::TEXT_DIM),
                                            );
                                        },
                                    );
                                });
                            })
                            .response
                            .interact(egui::Sense::click());
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            clicked = Some(i);
                        }
                    }
                });
        });
    clicked
}

/// Single-character text-input events for this frame (i.e. actual typed
/// characters post-keyboard-layout, not raw physical keys — this is what
/// makes `$`/`0`/`g` etc. work the same regardless of keyboard layout).
fn collect_text_chars(ctx: &egui::Context) -> Vec<char> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) if t.chars().count() == 1 => t.chars().next(),
                _ => None,
            })
            .collect()
    })
}

/// Removes this frame's `target` text-input event, if present, and reports
/// whether it was there. Used when `target` toggles some *other* widget into
/// focus (e.g. `/` focuses the filter box), so it doesn't also get typed into
/// that widget once it has focus later in the same frame.
fn consume_text_event(ctx: &egui::Context, target: char) -> bool {
    ctx.input_mut(|i| {
        let before = i.events.len();
        i.events
            .retain(|e| !matches!(e, egui::Event::Text(t) if t.chars().eq(std::iter::once(target))));
        i.events.len() != before
    })
}

enum GuiAction {
    StartNew,
    StartEdit(EditForm),
    Save,
    Cancel,
    /// Jump to another note by index (clicking an outgoing link/backlink).
    SelectNote(usize),
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

/// Renders a row of clickable note-reference chips (outgoing `[[links]]` or
/// backlinks); returns the note index whose chip was clicked, if any.
fn link_row(ui: &mut egui::Ui, links: &[(usize, String)]) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for (idx, title) in links {
            let title = if title.is_empty() { "(無題)" } else { title.as_str() };
            let resp = ui.add(
                egui::Button::new(egui::RichText::new(format!("→ {title}")).color(theme::ACCENT))
                    .fill(theme::ACCENT.gamma_multiply(0.14))
                    .stroke(egui::Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.5)))
                    .corner_radius(egui::CornerRadius::same(14)),
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                clicked = Some(*idx);
            }
        }
    });
    clicked
}

/// Like `Ui::add_sized`, but top-left aligned instead of centered (egui's
/// `add_sized` always uses `Layout::centered_and_justified`, which is wrong
/// for body text — it visually centers every line instead of just filling
/// the box).
fn add_sized_top_left(ui: &mut egui::Ui, size: impl Into<egui::Vec2>, widget: impl egui::Widget) -> egui::Response {
    ui.allocate_ui_with_layout(size.into(), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.add(widget)
    })
    .inner
}

/// Renders a single-line vim field (title/tags): the live `TextEdit` when
/// it's active and in Insert mode, a cursor-highlighted read-only view when
/// active and in Normal mode, or a plain clickable label when inactive.
/// Returns `true` if the user clicked it while inactive (the caller should
/// then make it the active field).
fn vim_line_field(
    ui: &mut egui::Ui,
    text: &mut String,
    is_active: bool,
    editor_mode: vim::EditorMode,
    cursor: usize,
    field_id: egui::Id,
    just_entered_insert: &mut bool,
) -> bool {
    if !is_active {
        let display = if text.is_empty() {
            "(空)".to_string()
        } else {
            text.clone()
        };
        return ui
            .add(
                egui::Label::new(egui::RichText::new(display).color(theme::TEXT_DIM))
                    .selectable(false)
                    .sense(egui::Sense::click()),
            )
            .clicked();
    }

    match editor_mode {
        vim::EditorMode::Insert => {
            let mut output = egui::TextEdit::singleline(text)
                .id(field_id)
                .desired_width(f32::INFINITY)
                .show(ui);
            if *just_entered_insert {
                output.state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
                output.state.store(ui.ctx(), field_id);
                output.response.request_focus();
                *just_entered_insert = false;
            }
        }
        vim::EditorMode::Normal => {
            let job = vim::normal_mode_body_job(
                text,
                cursor,
                egui::FontId::proportional(14.5),
                theme::TEXT,
                theme::ACCENT,
                theme::BG,
            );
            ui.add(egui::Label::new(job).selectable(false));
        }
    }
    false
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

        self.handle_global_keys(ctx);

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

        egui::TopBottomPanel::bottom("statusbar")
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_BG)
                    .inner_margin(egui::Margin::symmetric(14, 6)),
            )
            .show(ctx, |ui| {
                if let Some(cmd) = &mut self.command_input {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(":").monospace().color(theme::TEXT));
                        let command_id = egui::Id::new(COMMAND_ID);
                        let response = ui.add(
                            egui::TextEdit::singleline(cmd)
                                .id(command_id)
                                .frame(false)
                                .desired_width(f32::INFINITY),
                        );
                        if ctx.memory(|m| m.focused()) != Some(command_id) {
                            response.request_focus();
                        }
                    });
                } else if let Mode::Editing(form) = &self.mode {
                    let (text, color) = match form.editor_mode {
                        vim::EditorMode::Normal => ("-- NORMAL --", theme::TEXT_DIM),
                        vim::EditorMode::Insert => {
                            ("-- INSERT --", theme::status_color(Status::Active))
                        }
                    };
                    ui.label(egui::RichText::new(text).monospace().strong().color(color));
                } else {
                    ui.label(
                        egui::RichText::new("j/k 移動 ・ n 新規 ・ e 編集 ・ / 検索 ・ : コマンド")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                }
            });

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
                        .id(egui::Id::new(FILTER_ID))
                        .hint_text("タイトル・タグで絞り込み (/)")
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

                    let mut outgoing: Vec<(usize, String)> = Vec::new();
                    for link_text in engnote_core::linking::extract_links(&note.body) {
                        if let engnote_core::linking::LinkTarget::Resolved(id) =
                            engnote_core::linking::resolve_link(&self.notes, link_text)
                        {
                            if let Some(idx) = self.notes.iter().position(|(_, n)| n.frontmatter.id == id) {
                                if idx != i && !outgoing.iter().any(|(oi, _)| *oi == idx) {
                                    outgoing.push((idx, self.notes[idx].1.frontmatter.title.clone()));
                                }
                            }
                        }
                    }
                    let backlinks: Vec<(usize, String)> = engnote_core::linking::backlinks(
                        &self.notes,
                        &note.frontmatter.id,
                    )
                    .iter()
                    .filter_map(|n| {
                        self.notes
                            .iter()
                            .position(|(_, x)| x.frontmatter.id == n.frontmatter.id)
                            .map(|idx| (idx, n.frontmatter.title.clone()))
                    })
                    .collect();

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
                                            markdown::render(
                                                ui,
                                                &note.body,
                                                &markdown::Palette {
                                                    text: theme::TEXT,
                                                    dim: theme::TEXT_DIM,
                                                    strong: egui::Color32::WHITE,
                                                    accent: theme::ACCENT,
                                                    code_bg: theme::BG,
                                                },
                                            );
                                        });

                                    if !outgoing.is_empty() {
                                        ui.add_space(14.0);
                                        ui.label(
                                            egui::RichText::new("リンク").small().color(theme::TEXT_DIM),
                                        );
                                        ui.add_space(4.0);
                                        if let Some(idx) = link_row(ui, &outgoing) {
                                            action = Some(GuiAction::SelectNote(idx));
                                        }
                                    }

                                    if !backlinks.is_empty() {
                                        ui.add_space(14.0);
                                        ui.label(
                                            egui::RichText::new("バックリンク")
                                                .small()
                                                .color(theme::TEXT_DIM),
                                        );
                                        ui.add_space(4.0);
                                        if let Some(idx) = link_row(ui, &backlinks) {
                                            action = Some(GuiAction::SelectNote(idx));
                                        }
                                    }
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
                                if vim_line_field(
                                    ui,
                                    &mut form.title,
                                    form.active_field == ActiveField::Title,
                                    form.editor_mode,
                                    form.cursor,
                                    egui::Id::new(TITLE_FIELD_ID),
                                    &mut form.just_entered_insert,
                                ) {
                                    form.active_field = ActiveField::Title;
                                    form.clamp_cursor();
                                    form.slash_menu = None;
                                }
                                ui.add_space(10.0);

                                ui.label(
                                    egui::RichText::new("タグ(カンマ区切り)")
                                        .small()
                                        .color(theme::TEXT_DIM),
                                );
                                if vim_line_field(
                                    ui,
                                    &mut form.tags,
                                    form.active_field == ActiveField::Tags,
                                    form.editor_mode,
                                    form.cursor,
                                    egui::Id::new(TAGS_FIELD_ID),
                                    &mut form.just_entered_insert,
                                ) {
                                    form.active_field = ActiveField::Tags;
                                    form.clamp_cursor();
                                    form.slash_menu = None;
                                }
                                ui.add_space(16.0);

                                ui.label(egui::RichText::new("本文").small().color(theme::TEXT_DIM));
                                // Fill whatever room is left in the window instead of a fixed
                                // height, so the body editor doesn't look cramped on a large
                                // window (leave some space for the save/cancel row below it).
                                let body_height = (ui.available_height() - 70.0).max(180.0);
                                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                                let body_rows = ((body_height / row_height).floor() as usize).max(6);
                                egui::Frame::new()
                                    .fill(theme::CARD_BG)
                                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                    .corner_radius(egui::CornerRadius::same(8))
                                    .inner_margin(egui::Margin::same(10))
                                    .show(ui, |ui| {
                                        egui::ScrollArea::vertical()
                                            .id_salt("edit-body-scroll")
                                            .max_height(body_height)
                                            .show(ui, |ui| {
                                                if form.active_field != ActiveField::Body {
                                                    let display = if form.body.is_empty() {
                                                        "(空)".to_string()
                                                    } else {
                                                        form.body.clone()
                                                    };
                                                    let clicked = add_sized_top_left(
                                                        ui,
                                                        [ui.available_width(), body_height],
                                                        egui::Label::new(
                                                            egui::RichText::new(display)
                                                                .color(theme::TEXT_DIM),
                                                        )
                                                        .selectable(false)
                                                        .sense(egui::Sense::click())
                                                        .wrap_mode(egui::TextWrapMode::Wrap),
                                                    )
                                                    .clicked();
                                                    if clicked {
                                                        form.active_field = ActiveField::Body;
                                                        form.clamp_cursor();
                                                    }
                                                } else {
                                                    match form.editor_mode {
                                                        vim::EditorMode::Insert => {
                                                            let body_id = egui::Id::new(EDIT_BODY_ID);
                                                            let mut output = egui::TextEdit::multiline(&mut form.body)
                                                                .id(body_id)
                                                                .frame(false)
                                                                .desired_rows(body_rows)
                                                                .desired_width(f32::INFINITY)
                                                                .show(ui);
                                                            let live_cursor = if form.just_entered_insert {
                                                                output.state.cursor.set_char_range(Some(
                                                                    egui::text::CCursorRange::one(
                                                                        egui::text::CCursor::new(form.cursor),
                                                                    ),
                                                                ));
                                                                output.state.store(ui.ctx(), body_id);
                                                                output.response.request_focus();
                                                                form.just_entered_insert = false;
                                                                Some(form.cursor)
                                                            } else {
                                                                output.cursor_range.map(|r| r.primary.index)
                                                            };
                                                            update_slash_menu(
                                                                &form.body,
                                                                live_cursor,
                                                                &mut form.slash_menu,
                                                            );
                                                            let clicked = form.slash_menu.as_ref().and_then(|menu| {
                                                                show_slash_menu(ui, &output.galley, output.galley_pos, menu)
                                                            });
                                                            if let Some(idx) = clicked {
                                                                apply_slash_command_at(form, idx);
                                                            }
                                                        }
                                                        vim::EditorMode::Normal => {
                                                            let job = vim::normal_mode_body_job(
                                                                &form.body,
                                                                form.cursor,
                                                                egui::FontId::monospace(13.5),
                                                                theme::TEXT,
                                                                theme::ACCENT,
                                                                theme::BG,
                                                            );
                                                            add_sized_top_left(
                                                                ui,
                                                                [ui.available_width(), body_height],
                                                                egui::Label::new(job)
                                                                    .selectable(false)
                                                                    .wrap_mode(egui::TextWrapMode::Wrap),
                                                            );
                                                        }
                                                    }
                                                }
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
            Some(GuiAction::SelectNote(i)) => {
                self.selected = Some(i);
                self.mode = Mode::Viewing;
            }
            None => {}
        }
    }
}
