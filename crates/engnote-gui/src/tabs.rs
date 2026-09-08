use std::path::PathBuf;

use eframe::egui;
use engnote_core::note::Note;

use crate::{theme, EditForm};

pub type GroupId = u64;

/// A small fixed palette cycled through when creating new groups, so groups
/// are visually distinct without the user having to pick a color.
const GROUP_COLORS: &[egui::Color32] = &[
    theme::ACCENT,
    egui::Color32::from_rgb(88, 214, 155),
    egui::Color32::from_rgb(240, 165, 90),
    egui::Color32::from_rgb(235, 110, 140),
    egui::Color32::from_rgb(90, 180, 235),
];

pub struct TabGroup {
    pub id: GroupId,
    pub name: String,
    pub color: egui::Color32,
    pub collapsed: bool,
}

/// What a single tab is currently showing.
pub enum TabContent {
    /// A freshly opened tab with nothing loaded into it yet ("new tab page").
    Empty,
    /// Read-only view of `notes[idx]` (see `App::notes`).
    Viewing(usize),
    /// The vim-style edit form for a note (new or existing).
    Editing(EditForm),
}

pub struct Tab {
    pub id: u64,
    pub content: TabContent,
    pub group: Option<GroupId>,
}

/// All open tabs plus tab-group metadata. Tabs are tracked by a stable `id`
/// (not position) so that reordering/grouping/closing can freely shuffle
/// `items` without needing to separately patch up "the active index".
pub struct Tabs {
    pub items: Vec<Tab>,
    pub active_id: u64,
    pub groups: Vec<TabGroup>,
    next_tab_id: u64,
    next_group_id: GroupId,
}

impl Tabs {
    pub fn new() -> Self {
        Tabs {
            items: vec![Tab { id: 0, content: TabContent::Empty, group: None }],
            active_id: 0,
            groups: Vec::new(),
            next_tab_id: 1,
            next_group_id: 0,
        }
    }

    fn index_of(&self, id: u64) -> usize {
        self.items
            .iter()
            .position(|t| t.id == id)
            .expect("active_id should always name a live tab")
    }

    pub fn active_index(&self) -> usize {
        self.index_of(self.active_id)
    }

    pub fn active_tab(&self) -> &Tab {
        &self.items[self.active_index()]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        let i = self.active_index();
        &mut self.items[i]
    }

    /// Replaces the active tab's content in place — the "reuse the current
    /// tab" policy used for sidebar clicks, `n`/新規ノート, etc.
    pub fn open_here(&mut self, content: TabContent) {
        self.active_tab_mut().content = content;
    }

    /// Opens `content` in a brand-new tab right after the active one, and
    /// focuses it (Cmd+click on a note, Cmd+Enter in quick open).
    pub fn open_new(&mut self, content: TabContent) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let insert_at = self.active_index() + 1;
        self.items.insert(insert_at, Tab { id, content, group: None });
        self.active_id = id;
    }

    /// Cmd+T: a blank new tab, ungrouped.
    pub fn new_empty_tab(&mut self) {
        self.open_new(TabContent::Empty);
    }

    /// Closes tab at `idx`. The last remaining tab can't be closed outright —
    /// it's reset to blank instead, so there's always at least one tab.
    pub fn close(&mut self, idx: usize) {
        if self.items.len() == 1 {
            let id = self.next_tab_id;
            self.next_tab_id += 1;
            self.items[0] = Tab { id, content: TabContent::Empty, group: None };
            self.active_id = id;
            self.prune_empty_groups();
            return;
        }
        let closing_active = self.items[idx].id == self.active_id;
        self.items.remove(idx);
        if closing_active {
            let new_idx = idx.min(self.items.len() - 1);
            self.active_id = self.items[new_idx].id;
        }
        self.prune_empty_groups();
    }

    pub fn close_others(&mut self, keep_idx: usize) {
        let keep_id = self.items[keep_idx].id;
        self.items.retain(|t| t.id == keep_id);
        self.active_id = keep_id;
        self.prune_empty_groups();
    }

    /// Drag-to-reorder: moves the tab at `from` to sit at `to` (post-removal
    /// indexing, like `Vec::remove` + `Vec::insert`). Reordering within the
    /// same group keeps it grouped; moving it anywhere else (ungrouped space,
    /// or onto a different group) pulls it out of its group, since dragging
    /// a member out from between its siblings would otherwise split the
    /// group's contiguous block.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.items.len() || to >= self.items.len() {
            return;
        }
        let dragged_group = self.items[from].group;
        let target_group = self.items[to].group;
        let tab_id = self.items[from].id;

        let tab = self.items.remove(from);
        let insert_at = to.min(self.items.len());
        self.items.insert(insert_at, tab);

        if dragged_group != target_group {
            if let Some(t) = self.items.iter_mut().find(|t| t.id == tab_id) {
                t.group = None;
            }
        }
        self.prune_empty_groups();
    }

    /// Ctrl+K / Ctrl+J: move the active tab up/down, wrapping around.
    /// Auto-expands the landing tab's group if it was collapsed, so cycling
    /// through tabs never lands somewhere invisible.
    pub fn select_relative(&mut self, delta: i32) {
        let len = self.items.len() as i32;
        if len == 0 {
            return;
        }
        let cur = self.active_index() as i32;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.active_id = self.items[next].id;
        if let Some(group_id) = self.items[next].group {
            if let Some(group) = self.group_mut(group_id) {
                group.collapsed = false;
            }
        }
    }

    fn next_color(&self) -> egui::Color32 {
        GROUP_COLORS[self.groups.len() % GROUP_COLORS.len()]
    }

    /// Creates a new single-tab group containing the tab at `idx`.
    pub fn create_group(&mut self, idx: usize) -> GroupId {
        let id = self.next_group_id;
        self.next_group_id += 1;
        self.groups.push(TabGroup {
            id,
            name: String::new(),
            color: self.next_color(),
            collapsed: false,
        });
        self.items[idx].group = Some(id);
        id
    }

    /// Adds the tab at `idx` to an existing group, moving it adjacent to the
    /// group's other members — groups render as a contiguous block, matching
    /// how Chrome's tab groups behave.
    pub fn add_to_group(&mut self, idx: usize, group_id: GroupId) {
        self.items[idx].group = Some(group_id);
        let last_member = self
            .items
            .iter()
            .enumerate()
            .filter(|(i, t)| *i != idx && t.group == Some(group_id))
            .map(|(i, _)| i)
            .max();
        if let Some(last_member) = last_member {
            let insert_at = if idx < last_member { last_member } else { last_member + 1 };
            if insert_at != idx {
                let tab = self.items.remove(idx);
                self.items.insert(insert_at, tab);
            }
        }
    }

    pub fn remove_from_group(&mut self, idx: usize) {
        self.items[idx].group = None;
        self.prune_empty_groups();
    }

    pub fn rename_group(&mut self, id: GroupId, name: String) {
        if let Some(g) = self.group_mut(id) {
            g.name = name;
        }
    }

    pub fn set_group_color(&mut self, id: GroupId, color: egui::Color32) {
        if let Some(g) = self.group_mut(id) {
            g.color = color;
        }
    }

    /// Ungroups all members without closing them.
    pub fn delete_group(&mut self, id: GroupId) {
        for t in &mut self.items {
            if t.group == Some(id) {
                t.group = None;
            }
        }
        self.groups.retain(|g| g.id != id);
    }

    /// Closes every tab in the group.
    pub fn close_group(&mut self, id: GroupId) {
        let mut indices: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, t)| t.group == Some(id))
            .map(|(i, _)| i)
            .collect();
        indices.sort_unstable_by(|a, b| b.cmp(a));
        for i in indices {
            self.close(i);
        }
    }

    pub fn group(&self, id: GroupId) -> Option<&TabGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn group_mut(&mut self, id: GroupId) -> Option<&mut TabGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    fn prune_empty_groups(&mut self) {
        self.groups.retain(|g| self.items.iter().any(|t| t.group == Some(g.id)));
    }
}

/// Intents produced by clicking around the tab bar; applied by the caller
/// after rendering (the bar itself only reads `Tabs`, it doesn't mutate it —
/// keeps the render pass free of index-invalidation hazards).
pub enum BarAction {
    Switch(u64),
    Close(usize),
    CloseOthers(usize),
    NewTab,
    /// Drag-to-reorder: move the tab at `from` to sit at `to`.
    MoveTab { from: usize, to: usize },
    ToggleGroupCollapse(GroupId),
    CreateGroup(usize),
    AddToGroup(usize, GroupId),
    RemoveFromGroup(usize),
    RenameGroup(GroupId, String),
    SetGroupColor(GroupId, egui::Color32),
    DeleteGroup(GroupId),
    CloseGroup(GroupId),
}

impl Tabs {
    pub fn apply(&mut self, action: BarAction) {
        match action {
            BarAction::Switch(id) => self.active_id = id,
            BarAction::Close(idx) => self.close(idx),
            BarAction::CloseOthers(idx) => self.close_others(idx),
            BarAction::NewTab => self.new_empty_tab(),
            BarAction::MoveTab { from, to } => self.move_tab(from, to),
            BarAction::ToggleGroupCollapse(id) => {
                if let Some(g) = self.group_mut(id) {
                    g.collapsed = !g.collapsed;
                }
            }
            BarAction::CreateGroup(idx) => {
                self.create_group(idx);
            }
            BarAction::AddToGroup(idx, gid) => self.add_to_group(idx, gid),
            BarAction::RemoveFromGroup(idx) => self.remove_from_group(idx),
            BarAction::RenameGroup(gid, name) => self.rename_group(gid, name),
            BarAction::SetGroupColor(gid, color) => self.set_group_color(gid, color),
            BarAction::DeleteGroup(gid) => self.delete_group(gid),
            BarAction::CloseGroup(gid) => self.close_group(gid),
        }
    }
}

fn tab_label(content: &TabContent, notes: &[(PathBuf, Note)]) -> String {
    match content {
        TabContent::Empty => "新しいタブ".to_string(),
        TabContent::Viewing(i) => {
            let title = &notes[*i].1.frontmatter.title;
            if title.is_empty() {
                "(無題)".to_string()
            } else {
                title.clone()
            }
        }
        TabContent::Editing(form) => {
            let title = if form.title.is_empty() { "(無題)" } else { &form.title };
            format!("✎ {title}")
        }
    }
}

/// Renders the browser-style tab list (and any tab groups) as a vertical
/// strip down the left edge of the window (à la Arc/Dia), leftmost of all
/// the side panels. Read-only over `tabs`/`notes` — returns what the user
/// clicked or dragged, if anything, for the caller to apply via `Tabs::apply`.
///
/// `dragging` is the id of the tab currently being click-dragged, if any; it
/// persists across frames in the caller's state (a single mouse-down/drag
/// gesture spans many frames) and is threaded through here so tabs can be
/// live-reordered by dragging one over another.
pub fn render_bar(
    ctx: &egui::Context,
    tabs: &Tabs,
    notes: &[(PathBuf, Note)],
    dragging: &mut Option<u64>,
) -> Option<BarAction> {
    let mut action = None;
    // Each tab's drag-hit rect, collected in `tabs.items` order, so the
    // centralized reorder decision below can look up "the slot right next to
    // the dragged tab" without needing per-widget state.
    let mut rects: Vec<(u64, egui::Rect)> = Vec::with_capacity(tabs.items.len());

    // A drag that ends anywhere outside a tab's own hit-test (e.g. released
    // over empty space in the panel) should still stop being "in progress".
    if !ctx.input(|i| i.pointer.any_down()) {
        *dragging = None;
    }

    egui::SidePanel::left("tabbar")
        .resizable(true)
        .default_width(220.0)
        .min_width(160.0)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(egui::Margin::symmetric(8, 8)),
        )
        .show(ctx, |ui| {
            if ui
                .add(egui::Button::new(egui::RichText::new("＋ 新しいタブ").color(theme::TEXT)).frame(false))
                .clicked()
            {
                action = Some(BarAction::NewTab);
            }
            ui.add_space(6.0);

            egui::ScrollArea::vertical().id_salt("tabbar-scroll").show(ui, |ui| {
                ui.vertical(|ui| {
                    let mut i = 0;
                    while i < tabs.items.len() {
                        match tabs.items[i].group {
                            Some(gid) => {
                                let start = i;
                                let mut end = i;
                                while end < tabs.items.len() && tabs.items[end].group == Some(gid) {
                                    end += 1;
                                }
                                if let Some(a) = render_group(ui, tabs, notes, gid, start, end, dragging, &mut rects) {
                                    action = Some(a);
                                }
                                i = end;
                            }
                            None => {
                                if let Some(a) = render_tab(ui, tabs, notes, i, dragging, &mut rects) {
                                    action = Some(a);
                                }
                                i += 1;
                            }
                        }
                    }
                });
            });
        });

    // Live reorder, decided once per frame from the full picture of every
    // tab's rect (not from inside each tab's own render call): step the
    // dragged tab exactly one slot toward wherever the pointer currently is,
    // gated by whether the pointer has crossed the midpoint of that one
    // adjacent slot (vertically, since tabs stack top-to-bottom here). An
    // earlier version jumped straight to whichever tab's rect contained the
    // pointer; since every swap changes tab heights (grouped rows, wrapped
    // long titles) and thus shifts rect boundaries, that could immediately
    // re-trigger another jump next frame — a feedback loop that raced the
    // tab up/down the list. Stepping one slot at a time and gating on a
    // midpoint crossing has no such loop: it settles as soon as the pointer
    // stops crossing a boundary, and a fast/far drag still gets there
    // quickly by repeating this once per frame while held.
    if let Some(dragged_id) = *dragging {
        if let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) {
            if let Some(from) = tabs.items.iter().position(|t| t.id == dragged_id) {
                if let Some(&(hovered_id, _)) = rects.iter().find(|(_, r)| r.contains(pointer)) {
                    if hovered_id != dragged_id {
                        if let Some(to) = rects.iter().position(|&(id, _)| id == hovered_id) {
                            let step: i32 = if to > from { 1 } else { -1 };
                            let next = (from as i32 + step) as usize;
                            if let Some(&(_, next_rect)) = rects.get(next) {
                                let mid = next_rect.center().y;
                                let crossed = if step > 0 { pointer.y > mid } else { pointer.y < mid };
                                if crossed {
                                    action = Some(BarAction::MoveTab { from, to: next });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    action
}

fn render_tab(
    ui: &mut egui::Ui,
    tabs: &Tabs,
    notes: &[(PathBuf, Note)],
    idx: usize,
    dragging: &mut Option<u64>,
    rects: &mut Vec<(u64, egui::Rect)>,
) -> Option<BarAction> {
    let mut action = None;
    let tab = &tabs.items[idx];
    let is_active = tab.id == tabs.active_id;
    let label = tab_label(&tab.content, notes);
    let can_close = tabs.items.len() > 1;
    let is_dragged = *dragging == Some(tab.id);

    let (mut bg, border) = if is_active {
        (theme::ACCENT.gamma_multiply(0.22), theme::ACCENT)
    } else {
        (theme::CARD_BG, theme::BORDER)
    };
    if is_dragged {
        bg = theme::ACCENT.gamma_multiply(0.35);
    }

    let mut close_clicked = false;
    // Filled in by the closure below; the drag/click sense is attached to
    // just this rect afterward (see the comment further down for why).
    let mut text_rect = egui::Rect::NOTHING;

    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let text_resp = ui.add(
                    egui::Label::new(egui::RichText::new(&label).color(theme::TEXT)).selectable(false),
                );
                text_rect = text_resp.rect;

                if can_close {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_resp = ui.add(
                            egui::Label::new(egui::RichText::new("×").color(theme::TEXT_DIM))
                                .sense(egui::Sense::click()),
                        );
                        if close_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        close_clicked = close_resp.clicked();
                        // The label above already claimed the rest of the row via
                        // `text_resp.rect`; extend it to the close button's left
                        // edge so the drag/click hit-test below covers the whole
                        // row (not just the label glyphs themselves), without
                        // overlapping the close button.
                        text_rect.set_right(ui.min_rect().left());
                    });
                } else {
                    text_rect.set_right(ui.max_rect().right());
                }
            });
        });

    // Interact over the row minus the close button (which sits to its right
    // with no overlap) using an id keyed by the tab's own stable id rather
    // than its position — a plain `Sense`-on-`Label` here would get an auto
    // id derived from tree position, which changes when we reorder tabs
    // mid-drag and would silently drop the in-progress drag.
    let drag_id = egui::Id::new("engnote-tab-drag").with(tab.id);
    let drag_resp = ui.interact(text_rect, drag_id, egui::Sense::click_and_drag());
    rects.push((tab.id, drag_resp.rect));

    if drag_resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if drag_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    if close_clicked {
        action = Some(BarAction::Close(idx));
    } else if drag_resp.drag_started() {
        *dragging = Some(tab.id);
    } else if drag_resp.clicked() {
        action = Some(BarAction::Switch(tab.id));
    }
    if drag_resp.drag_stopped() {
        *dragging = None;
    }

    drag_resp.context_menu(|ui| {
        if let Some(a) = tab_context_menu(ui, tabs, idx) {
            action = Some(a);
        }
    });

    ui.add_space(4.0);

    action
}

fn render_group(
    ui: &mut egui::Ui,
    tabs: &Tabs,
    notes: &[(PathBuf, Note)],
    gid: GroupId,
    start: usize,
    end: usize,
    dragging: &mut Option<u64>,
    rects: &mut Vec<(u64, egui::Rect)>,
) -> Option<BarAction> {
    let mut action = None;
    let Some(group) = tabs.group(gid) else { return None };
    let color = group.color;
    let collapsed = group.collapsed;
    let name = group.name.clone();
    let member_count = end - start;

    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                let arrow = if collapsed { "▸" } else { "▾" };
                let label = if name.is_empty() {
                    format!("{arrow} タブグループ ({member_count})")
                } else {
                    format!("{arrow} {name} ({member_count})")
                };
                let header = ui.add(
                    egui::Label::new(egui::RichText::new(label).color(color).strong())
                        .sense(egui::Sense::click()),
                );
                if header.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if header.clicked() {
                    action = Some(BarAction::ToggleGroupCollapse(gid));
                }
                header.context_menu(|ui| {
                    if let Some(a) = group_context_menu(ui, tabs, gid) {
                        action = Some(a);
                    }
                });

                if !collapsed {
                    ui.add_space(4.0);
                    for idx in start..end {
                        if let Some(a) = render_tab(ui, tabs, notes, idx, dragging, rects) {
                            action = Some(a);
                        }
                    }
                }
            });
        });
    ui.add_space(4.0);

    action
}

fn tab_context_menu(ui: &mut egui::Ui, tabs: &Tabs, idx: usize) -> Option<BarAction> {
    let mut action = None;
    let current_group = tabs.items[idx].group;

    if current_group.is_none() && ui.button("新しいタブグループを作成").clicked() {
        action = Some(BarAction::CreateGroup(idx));
        ui.close();
    }

    if !tabs.groups.is_empty() {
        ui.menu_button("グループに追加", |ui| {
            for g in &tabs.groups {
                if Some(g.id) == current_group {
                    continue;
                }
                let label = if g.name.is_empty() { "無題のグループ".to_string() } else { g.name.clone() };
                if ui.button(label).clicked() {
                    action = Some(BarAction::AddToGroup(idx, g.id));
                    ui.close();
                }
            }
        });
    }

    if current_group.is_some() && ui.button("グループから外す").clicked() {
        action = Some(BarAction::RemoveFromGroup(idx));
        ui.close();
    }

    ui.separator();
    if ui.button("タブを閉じる").clicked() {
        action = Some(BarAction::Close(idx));
        ui.close();
    }
    if tabs.items.len() > 1 && ui.button("他のタブを閉じる").clicked() {
        action = Some(BarAction::CloseOthers(idx));
        ui.close();
    }

    action
}

fn group_context_menu(ui: &mut egui::Ui, tabs: &Tabs, gid: GroupId) -> Option<BarAction> {
    let mut action = None;
    let group = tabs.group(gid)?;
    let mut name = group.name.clone();
    let current_color = group.color;

    ui.label("グループ名");
    let resp = ui.text_edit_singleline(&mut name);
    if resp.lost_focus() && name != group.name {
        action = Some(BarAction::RenameGroup(gid, name));
    }

    ui.add_space(4.0);
    ui.label("色");
    ui.horizontal(|ui| {
        for &color in GROUP_COLORS {
            let selected = current_color == color;
            let resp = egui::Frame::new()
                .fill(color)
                .stroke(egui::Stroke::new(if selected { 2.0 } else { 0.0 }, theme::TEXT))
                .corner_radius(egui::CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                })
                .response
                .interact(egui::Sense::click());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                action = Some(BarAction::SetGroupColor(gid, color));
                ui.close();
            }
        }
    });

    ui.separator();
    if ui.button("グループを閉じる").clicked() {
        action = Some(BarAction::CloseGroup(gid));
        ui.close();
    }
    if ui.button("グループを解除").clicked() {
        action = Some(BarAction::DeleteGroup(gid));
        ui.close();
    }

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(tabs: &Tabs) -> Vec<u64> {
        tabs.items.iter().map(|t| t.id).collect()
    }

    #[test]
    fn new_tab_starts_with_one_empty_tab() {
        let tabs = Tabs::new();
        assert_eq!(tabs.items.len(), 1);
        assert!(matches!(tabs.active_tab().content, TabContent::Empty));
    }

    #[test]
    fn open_here_replaces_active_tab_in_place() {
        let mut tabs = Tabs::new();
        tabs.open_here(TabContent::Viewing(3));
        assert_eq!(tabs.items.len(), 1);
        assert!(matches!(tabs.active_tab().content, TabContent::Viewing(3)));
    }

    #[test]
    fn open_new_inserts_after_active_and_focuses_it() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Viewing(1));
        tabs.open_new(TabContent::Viewing(2));
        assert_eq!(tabs.items.len(), 3);
        assert!(matches!(tabs.active_tab().content, TabContent::Viewing(2)));
        assert_eq!(tabs.active_index(), 2);
    }

    #[test]
    fn select_relative_wraps_around() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Empty);
        tabs.open_new(TabContent::Empty);
        assert_eq!(tabs.active_index(), 2);
        tabs.select_relative(1);
        assert_eq!(tabs.active_index(), 0); // wrapped forward past the end
        tabs.select_relative(-1);
        assert_eq!(tabs.active_index(), 2); // wrapped backward past the start
    }

    #[test]
    fn closing_the_last_tab_resets_it_to_blank_instead_of_vanishing() {
        let mut tabs = Tabs::new();
        tabs.open_here(TabContent::Viewing(5));
        tabs.close(0);
        assert_eq!(tabs.items.len(), 1);
        assert!(matches!(tabs.active_tab().content, TabContent::Empty));
    }

    #[test]
    fn closing_active_tab_picks_a_neighbor() {
        let mut tabs = Tabs::new(); // [0]
        tabs.open_new(TabContent::Empty); // [0, 1*]
        tabs.open_new(TabContent::Empty); // [0, 1, 2*]
        let active_idx = tabs.active_index();
        tabs.close(active_idx);
        assert_eq!(tabs.items.len(), 2);
        // closing the last tab should fall back to the new last tab
        assert_eq!(tabs.active_index(), 1);
    }

    #[test]
    fn closing_inactive_tab_does_not_move_active() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Empty);
        let active_id = tabs.active_id;
        tabs.close(0); // close the first (inactive) tab
        assert_eq!(tabs.active_id, active_id);
        assert_eq!(tabs.items.len(), 1);
    }

    #[test]
    fn create_group_and_add_to_group_keeps_members_contiguous() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Empty);
        tabs.open_new(TabContent::Empty);
        tabs.open_new(TabContent::Empty); // items: [A, B, C, D]

        let gid = tabs.create_group(1); // group B
        tabs.add_to_group(3, gid); // add D, should move next to B

        assert_eq!(ids(&tabs), vec![tabs.items[0].id, tabs.items[1].id, tabs.items[2].id, tabs.items[3].id]);
        let group_positions: Vec<usize> = tabs
            .items
            .iter()
            .enumerate()
            .filter(|(_, t)| t.group == Some(gid))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(group_positions.len(), 2);
        assert_eq!(group_positions[1], group_positions[0] + 1, "group members should be adjacent");
    }

    #[test]
    fn removing_last_group_member_prunes_the_group() {
        let mut tabs = Tabs::new();
        let gid = tabs.create_group(0);
        assert_eq!(tabs.groups.len(), 1);
        tabs.remove_from_group(0);
        assert!(tabs.group(gid).is_none());
    }

    #[test]
    fn select_relative_expands_a_collapsed_group_when_landing_inside_it() {
        let mut tabs = Tabs::new(); // [A*]
        tabs.open_new(TabContent::Empty); // [A, B*]
        let gid = tabs.create_group(1); // group B
        tabs.open_new(TabContent::Empty); // [A, B, C*]
        tabs.group_mut(gid).unwrap().collapsed = true;
        tabs.select_relative(-1); // move from C back onto B (grouped, collapsed)
        assert!(!tabs.group(gid).unwrap().collapsed);
    }

    #[test]
    fn close_group_closes_every_member() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Empty);
        tabs.open_new(TabContent::Empty); // [A, B, C]
        let gid = tabs.create_group(1);
        tabs.add_to_group(2, gid);
        tabs.close_group(gid);
        assert_eq!(tabs.items.len(), 1);
        assert!(tabs.group(gid).is_none());
    }

    #[test]
    fn move_tab_reorders_by_dragging() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Empty);
        tabs.open_new(TabContent::Empty); // [A, B, C]
        let a = tabs.items[0].id;
        let b = tabs.items[1].id;
        let c = tabs.items[2].id;
        tabs.move_tab(0, 2); // drag A to where C was
        assert_eq!(ids(&tabs), vec![b, c, a]);
    }

    #[test]
    fn move_tab_within_same_group_keeps_it_grouped() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Empty);
        tabs.open_new(TabContent::Empty); // [A, B, C]
        let gid = tabs.create_group(1);
        tabs.add_to_group(2, gid); // group = [B, C]
        tabs.move_tab(1, 2); // reorder within the group: [A, C, B]
        assert_eq!(tabs.group(gid).is_some(), true);
        let grouped: Vec<usize> = tabs
            .items
            .iter()
            .enumerate()
            .filter(|(_, t)| t.group == Some(gid))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(grouped.len(), 2, "both members should still be in the group");
    }

    #[test]
    fn move_tab_out_of_group_ungroups_it() {
        let mut tabs = Tabs::new();
        tabs.open_new(TabContent::Empty); // [A, B]
        let gid = tabs.create_group(1); // group B
        tabs.open_new(TabContent::Empty); // [A, B(g), C]
        tabs.move_tab(1, 0); // drag B out to the front, next to ungrouped A
        let b = tabs.items[0].id;
        assert_eq!(tabs.items.iter().find(|t| t.id == b).unwrap().group, None);
        assert!(tabs.group(gid).is_none(), "group should be pruned once empty");
    }
}
