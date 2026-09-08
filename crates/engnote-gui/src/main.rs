mod markdown;
mod quick_open;
mod tabs;
mod theme;
mod vim;

use std::path::PathBuf;

use chrono::Local;
use eframe::egui;
use engnote_core::note::Note;
use engnote_core::vault;
use tabs::{TabContent, Tabs};

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
            // A brand-new note has nothing to edit yet, so start at the top.
            active_field: ActiveField::Title,
            editor_mode: vim::EditorMode::Normal,
            cursor: 0,
            just_entered_insert: false,
            pending_g: false,
            pending_d: false,
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
            // Editing an existing note: jump straight to its content.
            active_field: ActiveField::Body,
            editor_mode: vim::EditorMode::Normal,
            cursor: 0,
            just_entered_insert: false,
            pending_g: false,
            pending_d: false,
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
    tabs: Tabs,
    filter: String,
    error: Option<String>,
    dock_icon_set: bool,
    /// `Some(buffer)` while the `:command` line (opened with `:`) is active.
    command_input: Option<String>,
    /// `Some(state)` while the Cmd+P quick-open palette is on screen.
    quick_open: Option<quick_open::QuickOpen>,
    /// Id of the tab currently being click-dragged in the tab bar, if any
    /// (see `tabs::render_bar`).
    drag_tab: Option<u64>,
}

impl App {
    fn new(vault_dir: PathBuf) -> Self {
        let mut app = App {
            vault_dir,
            notes: Vec::new(),
            tabs: Tabs::new(),
            filter: String::new(),
            error: None,
            dock_icon_set: false,
            command_input: None,
            quick_open: None,
            drag_tab: None,
        };
        app.reload();
        app
    }

    fn note_index_by_id(&self, id: &str) -> Option<usize> {
        self.notes.iter().position(|(_, n)| n.frontmatter.id == id)
    }

    /// Reloads the note list from disk and remaps every tab currently
    /// `Viewing` a note so it still points at the right note afterward (the
    /// reload can reorder/insert/remove entries in `self.notes`). Tabs mid-
    /// edit are untouched — an `EditForm` carries its own note id, not an
    /// index into `self.notes`.
    fn reload(&mut self) {
        let viewing: Vec<(usize, String)> = self
            .tabs
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, t)| match &t.content {
                TabContent::Viewing(idx) => Some((i, self.notes[*idx].1.frontmatter.id.clone())),
                _ => None,
            })
            .collect();

        match vault::list(&self.vault_dir) {
            Ok(notes) => self.notes = notes,
            Err(e) => {
                self.error = Some(format!("ノート一覧の読み込みに失敗: {e:#}"));
                return;
            }
        }

        for (tab_idx, id) in viewing {
            self.tabs.items[tab_idx].content = match self.note_index_by_id(&id) {
                Some(idx) => TabContent::Viewing(idx),
                None => TabContent::Empty, // the note was deleted out from under this tab
            };
        }
    }

    /// Saves the `EditForm` that was in tab `tab_idx`, then points that tab
    /// back at the (possibly newly created) note.
    fn save_edit(&mut self, tab_idx: usize, form: EditForm) {
        let existing_context = form
            .path
            .as_ref()
            .and_then(|p| self.notes.iter().find(|(np, _)| np == p))
            .and_then(|(_, n)| n.frontmatter.context_path.clone());
        let id = form.id.clone();
        let note = form.into_note(existing_context);
        let result = match self
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
                self.error = None;
                self.tabs.items[tab_idx].content = match self.note_index_by_id(&id) {
                    Some(idx) => TabContent::Viewing(idx),
                    None => TabContent::Empty,
                };
            }
            Err(e) => self.error = Some(format!("保存に失敗: {e:#}")),
        }
    }

    /// Moves the active tab's `Viewing` note up/down by one within the
    /// currently filtered note list (`j`/`k`).
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
        let current = match self.tabs.active_tab().content {
            TabContent::Viewing(idx) => visible.iter().position(|&i| i == idx),
            _ => None,
        };
        let next = match current {
            Some(p) => (p as i32 + delta).clamp(0, visible.len() as i32 - 1) as usize,
            None => 0,
        };
        self.tabs.open_here(TabContent::Viewing(visible[next]));
    }

    /// App-wide vim-style keyboard shortcuts, plus the browser-style tab
    /// shortcuts (Ctrl+H/L, Cmd+P/T/W). The tab shortcuts are checked first
    /// and consumed via `consume_key` so they win regardless of which text
    /// field currently has focus — same idea as the existing Cmd+S save
    /// shortcut in `handle_editing_keys`.
    fn handle_global_keys(&mut self, ctx: &egui::Context) {
        if self.quick_open.is_some() {
            // Its own key handling (arrows/Enter/Esc) happens in
            // `quick_open::render` during the UI pass below.
            return;
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            self.quick_open = Some(quick_open::QuickOpen::new());
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::T)) {
            self.tabs.new_empty_tab();
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)) {
            let idx = self.tabs.active_index();
            self.tabs.close(idx);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::H)) {
            self.tabs.select_relative(-1);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::L)) {
            self.tabs.select_relative(1);
            return;
        }

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

        if matches!(self.tabs.active_tab().content, TabContent::Editing(_)) {
            self.handle_editing_keys(ctx);
        } else {
            self.handle_viewing_keys(ctx, filter_id);
        }
    }

    /// Executes a vim-style `:command` (see the bottom command line).
    fn execute_command(&mut self, cmd: &str) {
        match cmd.trim() {
            "e" | "new" | "enew" => {
                self.tabs.open_here(TabContent::Editing(EditForm::blank()));
            }
            "w" | "wq" | "x" => {
                let tab_idx = self.tabs.active_index();
                if let TabContent::Editing(_) = self.tabs.items[tab_idx].content {
                    if let TabContent::Editing(form) =
                        std::mem::replace(&mut self.tabs.items[tab_idx].content, TabContent::Empty)
                    {
                        self.save_edit(tab_idx, form);
                    }
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
                    self.tabs.open_here(TabContent::Editing(EditForm::blank()));
                    return;
                }
                'e' | 'i' => {
                    if let TabContent::Viewing(idx) = self.tabs.active_tab().content {
                        let path = self.notes[idx].0.clone();
                        let note = self.notes[idx].1.clone();
                        self.tabs.open_here(TabContent::Editing(EditForm::from_note(path, &note)));
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
        let tab_idx = self.tabs.active_index();

        if let TabContent::Editing(form) = &mut self.tabs.items[tab_idx].content {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
                intent = Intent::Save;
            } else if form.active_field == ActiveField::Body
                && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::V))
            {
                // Peek only (don't consume): TextEdit's own paste reacts to
                // `Event::Paste`, not this raw key, so a plain-text paste is
                // unaffected either way — we only act when the clipboard
                // actually holds an image.
                paste_clipboard_image(ctx, form, &self.vault_dir);
            } else if form.editor_mode == vim::EditorMode::Insert {
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
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
            Intent::ExitToViewing => {
                let back = match &self.tabs.items[tab_idx].content {
                    TabContent::Editing(form) => {
                        form.path.as_ref().and_then(|p| self.notes.iter().position(|(np, _)| np == p))
                    }
                    _ => None,
                };
                self.tabs.items[tab_idx].content = match back {
                    Some(idx) => TabContent::Viewing(idx),
                    None => TabContent::Empty,
                };
            }
            Intent::Save => {
                if let TabContent::Editing(form) =
                    std::mem::replace(&mut self.tabs.items[tab_idx].content, TabContent::Empty)
                {
                    self.save_edit(tab_idx, form);
                }
            }
            Intent::None => {}
        }
    }
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

/// If the OS clipboard holds an image, saves it under `<vault>/assets/` and
/// inserts a Markdown image link at the cursor in `form.body` — at the live
/// `TextEdit`'s cursor in Insert mode, or `form.cursor` in Normal mode. A
/// no-op if the clipboard has no image (e.g. it's plain text).
fn paste_clipboard_image(ctx: &egui::Context, form: &mut EditForm, vault_dir: &std::path::Path) {
    let Some(snippet) = save_clipboard_image(vault_dir) else {
        return;
    };

    match form.editor_mode {
        vim::EditorMode::Insert => {
            let field_id = form.active_field.id();
            let mut cursor = form.body.chars().count();
            let state = egui::text_edit::TextEditState::load(ctx, field_id);
            if let Some(range) = state.as_ref().and_then(|s| s.cursor.char_range()) {
                cursor = range.primary.index.min(form.body.chars().count());
            }
            form.body.insert_str(char_byte_index(&form.body, cursor), &snippet);
            let new_cursor = cursor + snippet.chars().count();
            let mut state = state.unwrap_or_default();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(new_cursor))));
            state.store(ctx, field_id);
        }
        vim::EditorMode::Normal => {
            form.body.insert_str(char_byte_index(&form.body, form.cursor), &snippet);
            form.cursor += snippet.chars().count();
        }
    }
}

/// Byte offset of the `char_idx`-th character in `s` (or `s.len()` if
/// `char_idx` is at or past the end) — `String::insert_str` needs a byte
/// index, but our cursors are character indices (see `EditForm::cursor`).
fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

/// Saves the clipboard's image (if any) as a PNG under `<vault>/assets/` and
/// returns a Markdown snippet linking it. `None` if the clipboard has no
/// image or saving failed.
fn save_clipboard_image(vault_dir: &std::path::Path) -> Option<String> {
    let assets_dir = vault_dir.join("assets");
    std::fs::create_dir_all(&assets_dir).ok()?;

    let filename = format!("{}.png", ulid::Ulid::generate());
    let path = assets_dir.join(&filename);
    write_clipboard_image_as_png(&path)?;

    Some(format!("![](assets/{filename})"))
}

/// Writes whatever image is on the OS clipboard to `path` as PNG.
///
/// On macOS this reads `NSPasteboardTypePNG` bytes straight off the
/// pasteboard and writes them as-is (no re-encoding) — deliberately *not*
/// going through `arboard::Clipboard::get_image()`, which on macOS only
/// requests the TIFF representation and decodes it with `image`'s TIFF
/// decoder; that decoder can't handle the TIFF macOS synthesizes from a
/// PNG-originated clipboard image (`Error::ConversionFailure`), even though
/// a `public.png` representation was on the pasteboard the whole time.
#[cfg(target_os = "macos")]
fn write_clipboard_image_as_png(path: &std::path::Path) -> Option<()> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};

    let bytes = unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.dataForType(NSPasteboardTypePNG)
    }?
    .to_vec();

    std::fs::write(path, bytes).ok()
}

#[cfg(not(target_os = "macos"))]
fn write_clipboard_image_as_png(path: &std::path::Path) -> Option<()> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let image_data = clipboard.get_image().ok()?;
    image::save_buffer(
        path,
        &image_data.bytes,
        image_data.width as u32,
        image_data.height as u32,
        image::ColorType::Rgba8,
    )
    .ok()
}

enum GuiAction {
    StartNew,
    StartEdit(EditForm),
    Save,
    Cancel,
    /// Jump to another note by index (clicking an outgoing link/backlink, or
    /// a sidebar card). `new_tab` mirrors the Cmd+click "open in new tab"
    /// convention used elsewhere (quick open, tab bar).
    SelectNote { index: usize, new_tab: bool },
    /// Flip a `[ ]`/`[x]` task checkbox clicked in the body view and persist it.
    ToggleCheckbox {
        index: usize,
        marker_range: std::ops::Range<usize>,
    },
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
            ui.label(
                egui::RichText::new(note.short_id())
                    .monospace()
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
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

        if let Some(bar_action) = tabs::render_bar(ctx, &self.tabs, &self.notes, &mut self.drag_tab) {
            self.tabs.apply(bar_action);
        }

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
                } else if let TabContent::Editing(form) = &self.tabs.active_tab().content {
                    let (text, color) = match form.editor_mode {
                        vim::EditorMode::Normal => ("-- NORMAL --", theme::TEXT_DIM),
                        vim::EditorMode::Insert => ("-- INSERT --", theme::GREEN),
                    };
                    ui.label(egui::RichText::new(text).monospace().strong().color(color));
                } else {
                    ui.label(
                        egui::RichText::new(
                            "j/k 移動 ・ n 新規 ・ e 編集 ・ / 検索 ・ : コマンド ・ ⌃H/⌃L タブ移動 ・ ⌘P 開く ・ ⌘T 新規タブ",
                        )
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

                let active_viewing = match self.tabs.active_tab().content {
                    TabContent::Viewing(idx) => Some(idx),
                    _ => None,
                };

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
                        let resp = note_card(ui, note, active_viewing == Some(i));
                        if resp.clicked() {
                            let new_tab = ui.input(|inp| inp.modifiers.command);
                            action = Some(GuiAction::SelectNote { index: i, new_tab });
                        }
                        ui.add_space(6.0);
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let tab_idx = self.tabs.active_index();
            let is_editing = matches!(self.tabs.items[tab_idx].content, TabContent::Editing(_));

            if is_editing {
                let TabContent::Editing(form) = &mut self.tabs.items[tab_idx].content else {
                    unreachable!()
                };
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
                                                            if form.just_entered_insert {
                                                                output.state.cursor.set_char_range(Some(
                                                                    egui::text::CCursorRange::one(
                                                                        egui::text::CCursor::new(form.cursor),
                                                                    ),
                                                                ));
                                                                output.state.store(ui.ctx(), body_id);
                                                                output.response.request_focus();
                                                                form.just_entered_insert = false;
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
                return;
            }

            match self.tabs.items[tab_idx].content {
                TabContent::Viewing(i) => {
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
                                            let toggled = markdown::render_interactive(
                                                ui,
                                                &note.body,
                                                &markdown::Palette {
                                                    text: theme::TEXT,
                                                    dim: theme::TEXT_DIM,
                                                    strong: egui::Color32::WHITE,
                                                    code_bg: theme::BG,
                                                },
                                            );
                                            if let Some(marker_range) = toggled {
                                                action = Some(GuiAction::ToggleCheckbox {
                                                    index: i,
                                                    marker_range,
                                                });
                                            }
                                        });

                                    if !outgoing.is_empty() {
                                        ui.add_space(14.0);
                                        ui.label(
                                            egui::RichText::new("リンク").small().color(theme::TEXT_DIM),
                                        );
                                        ui.add_space(4.0);
                                        if let Some(idx) = link_row(ui, &outgoing) {
                                            let new_tab = ui.input(|inp| inp.modifiers.command);
                                            action = Some(GuiAction::SelectNote { index: idx, new_tab });
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
                                            let new_tab = ui.input(|inp| inp.modifiers.command);
                                            action = Some(GuiAction::SelectNote { index: idx, new_tab });
                                        }
                                    }
                                });
                        });
                }
                TabContent::Empty => {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(
                                "左の一覧からノートを選ぶか、「＋ 新規ノート」で作成、または ⌘P でノートを開いてください",
                            )
                            .color(theme::TEXT_DIM),
                        );
                    });
                }
                TabContent::Editing(_) => unreachable!(),
            }
        });

        if let Some(qo) = &mut self.quick_open {
            match quick_open::render(ctx, qo, &self.notes) {
                Some(quick_open::Action::Open(idx)) => {
                    self.tabs.open_here(TabContent::Viewing(idx));
                    self.quick_open = None;
                }
                Some(quick_open::Action::OpenInNewTab(idx)) => {
                    self.tabs.open_new(TabContent::Viewing(idx));
                    self.quick_open = None;
                }
                Some(quick_open::Action::Cancel) => self.quick_open = None,
                None => {}
            }
        }

        match action {
            Some(GuiAction::StartNew) => {
                self.tabs.open_here(TabContent::Editing(EditForm::blank()));
            }
            Some(GuiAction::StartEdit(form)) => self.tabs.open_here(TabContent::Editing(form)),
            Some(GuiAction::Save) => {
                let tab_idx = self.tabs.active_index();
                if let TabContent::Editing(form) =
                    std::mem::replace(&mut self.tabs.items[tab_idx].content, TabContent::Empty)
                {
                    self.save_edit(tab_idx, form);
                }
            }
            Some(GuiAction::Cancel) => {
                let tab_idx = self.tabs.active_index();
                let back = match &self.tabs.items[tab_idx].content {
                    TabContent::Editing(form) => {
                        form.path.as_ref().and_then(|p| self.notes.iter().position(|(np, _)| np == p))
                    }
                    _ => None,
                };
                self.tabs.items[tab_idx].content = match back {
                    Some(idx) => TabContent::Viewing(idx),
                    None => TabContent::Empty,
                };
            }
            Some(GuiAction::SelectNote { index, new_tab }) => {
                if new_tab {
                    self.tabs.open_new(TabContent::Viewing(index));
                } else {
                    self.tabs.open_here(TabContent::Viewing(index));
                }
            }
            Some(GuiAction::ToggleCheckbox { index, marker_range }) => {
                let (path, note) = &self.notes[index];
                let mid = marker_range.start + 1;
                let mut new_body = note.body.clone();
                let is_unchecked = new_body.as_bytes().get(mid) == Some(&b' ');
                new_body.replace_range(mid..mid + 1, if is_unchecked { "x" } else { " " });
                let mut updated = note.clone();
                updated.body = new_body;
                updated.frontmatter.updated = Local::now();
                match vault::save_at(path, &updated) {
                    Ok(()) => {
                        let id = updated.frontmatter.id.clone();
                        self.reload();
                        let tab_idx = self.tabs.active_index();
                        if let Some(idx) = self.note_index_by_id(&id) {
                            self.tabs.items[tab_idx].content = TabContent::Viewing(idx);
                        }
                    }
                    Err(e) => self.error = Some(format!("チェック状態の保存に失敗: {e:#}")),
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual check, not run by `cargo test` by default: copy an image
    /// (e.g. a screenshot) to the clipboard first, then
    /// `cargo test -p engnote-gui save_clipboard_image_writes_a_real_png -- --ignored`.
    #[test]
    #[ignore = "manual: requires a real image on the OS clipboard"]
    fn save_clipboard_image_writes_a_real_png() {
        let dir = std::env::temp_dir().join("engnote-clip-probe");
        let _ = std::fs::remove_dir_all(&dir);

        let snippet = save_clipboard_image(&dir).expect("expected an image on the clipboard");
        let filename = snippet
            .strip_prefix("![](assets/")
            .and_then(|s| s.strip_suffix(')'))
            .expect("snippet should be `![](assets/<file>.png)`");

        let bytes = std::fs::read(dir.join("assets").join(filename)).expect("saved file should be readable");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "saved file should be a real PNG");
    }

    #[test]
    fn char_byte_index_handles_multibyte_chars() {
        let s = "aあb";
        assert_eq!(char_byte_index(s, 0), 0);
        assert_eq!(char_byte_index(s, 1), 1); // right after 'a'
        assert_eq!(char_byte_index(s, 2), 1 + "あ".len()); // right after 'あ' (3 bytes)
        assert_eq!(char_byte_index(s, 3), s.len()); // at end
        assert_eq!(char_byte_index(s, 99), s.len()); // past end clamps
    }
}
