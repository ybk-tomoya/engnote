use std::ops::Range;

use eframe::egui;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

pub struct Palette {
    pub text: egui::Color32,
    pub dim: egui::Color32,
    pub strong: egui::Color32,
    pub code_bg: egui::Color32,
}

/// A chunk of rendered markdown: either plain rich text, or a task-list item
/// that needs a real, clickable `egui::Checkbox` (a `LayoutJob` can't have
/// per-glyph click targets, so task items are pulled out of the surrounding
/// text and rendered as their own row).
enum Block {
    Rich(egui::text::LayoutJob),
    Task {
        /// Byte range of the `[ ]`/`[x]` marker in the *source* text, so the
        /// caller can flip it in place when the checkbox is clicked.
        marker_range: Range<usize>,
        checked: bool,
        content: egui::text::LayoutJob,
    },
}

/// Renders `source` as read-only rich text (headings, bold/italic, lists,
/// task-list checkboxes, inline/block code, block quotes, rules). `[[...]]`
/// wikilinks aren't CommonMark, so pulldown-cmark just sees them as literal
/// text here — resolving/navigating them is handled separately by the
/// outgoing-links/backlinks rows shown below the body (see `main.rs`).
///
/// Returns the source byte range of a task checkbox's `[ ]`/`[x]` marker if
/// the user just clicked it, so the caller can toggle and persist it.
pub fn render_interactive(ui: &mut egui::Ui, source: &str, palette: &Palette) -> Option<Range<usize>> {
    let mut toggled = None;
    for block in build_blocks(source, palette) {
        match block {
            Block::Rich(job) => {
                if !job.text.is_empty() {
                    ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Wrap));
                }
            }
            Block::Task { marker_range, checked, content } => {
                ui.horizontal(|ui| {
                    let mut checked = checked;
                    if ui.checkbox(&mut checked, "").changed() {
                        toggled = Some(marker_range.clone());
                    }
                    ui.add(egui::Label::new(content).wrap_mode(egui::TextWrapMode::Wrap));
                });
            }
        }
    }
    toggled
}

fn heading_size(level: HeadingLevel) -> f32 {
    match level {
        HeadingLevel::H1 => 23.0,
        HeadingLevel::H2 => 20.0,
        HeadingLevel::H3 => 17.5,
        HeadingLevel::H4 => 15.5,
        HeadingLevel::H5 => 14.5,
        HeadingLevel::H6 => 14.0,
    }
}

fn build_blocks(source: &str, palette: &Palette) -> Vec<Block> {
    use egui::text::{LayoutJob, TextFormat};

    const BASE_SIZE: f32 = 14.5;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let plain = |size: f32, color: egui::Color32| TextFormat {
        font_id: egui::FontId::proportional(size),
        color,
        ..Default::default()
    };

    let mut blocks = Vec::new();
    let mut current = LayoutJob::default();
    let mut bold_depth = 0u32;
    let mut code_depth = 0u32;
    let mut quote_depth = 0u32;
    let mut heading: Option<HeadingLevel> = None;
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    // Set between a task item's `TaskListMarker` and its `End(Item)`: text
    // events in that span accumulate into the task's own job instead of
    // `current`, so the checkbox and its label end up in one `ui.horizontal`.
    let mut task: Option<(Range<usize>, bool, LayoutJob)> = None;

    let mut events = Parser::new_ext(source, options).into_offset_iter().peekable();
    while let Some((event, range)) = events.next() {
        match event {
            Event::Start(tag) => match tag {
                Tag::Strong => bold_depth += 1,
                Tag::Heading { level, .. } => heading = Some(level),
                Tag::CodeBlock(_) => code_depth += 1,
                Tag::BlockQuote(_) => quote_depth += 1,
                Tag::List(start) => list_stack.push(start),
                Tag::Item => {
                    let is_task = matches!(events.peek(), Some((Event::TaskListMarker(_), _)));
                    if !is_task {
                        let depth = list_stack.len().saturating_sub(1);
                        let indent = "    ".repeat(depth);
                        let marker = match list_stack.last() {
                            Some(Some(n)) => format!("{indent}{n}. "),
                            _ => format!("{indent}•  "),
                        };
                        current.append(&marker, 0.0, plain(BASE_SIZE, palette.dim));
                    }
                }
                _ => {}
            },
            Event::TaskListMarker(checked) => {
                task = Some((range, checked, LayoutJob::default()));
            }
            Event::End(tag_end) => match tag_end {
                TagEnd::Strong => bold_depth = bold_depth.saturating_sub(1),
                TagEnd::Heading(_) => {
                    heading = None;
                    current.append("\n\n", 0.0, plain(BASE_SIZE, palette.text));
                }
                TagEnd::CodeBlock => {
                    code_depth = code_depth.saturating_sub(1);
                    current.append("\n", 0.0, plain(BASE_SIZE, palette.text));
                }
                TagEnd::BlockQuote(_) => quote_depth = quote_depth.saturating_sub(1),
                TagEnd::List(_) => {
                    list_stack.pop();
                }
                TagEnd::Item => {
                    if let Some((marker_range, checked, content)) = task.take() {
                        if !current.text.is_empty() {
                            blocks.push(Block::Rich(std::mem::take(&mut current)));
                        }
                        blocks.push(Block::Task { marker_range, checked, content });
                    } else {
                        current.append("\n", 0.0, plain(BASE_SIZE, palette.text));
                    }
                }
                TagEnd::Paragraph => current.append("\n\n", 0.0, plain(BASE_SIZE, palette.text)),
                _ => {}
            },
            Event::Text(text) => {
                let size = heading.map(heading_size).unwrap_or(BASE_SIZE);
                let mut color = if bold_depth > 0 || heading.is_some() {
                    palette.strong
                } else if quote_depth > 0 {
                    palette.dim
                } else {
                    palette.text
                };
                let font_id = if code_depth > 0 {
                    color = palette.text;
                    egui::FontId::monospace(size)
                } else {
                    egui::FontId::proportional(size)
                };
                let background = if code_depth > 0 { palette.code_bg } else { egui::Color32::TRANSPARENT };
                let fmt = TextFormat { font_id, color, background, ..Default::default() };
                match task.as_mut() {
                    Some((_, _, content)) => content.append(&text, 0.0, fmt),
                    None => current.append(&text, 0.0, fmt),
                }
            }
            Event::Code(text) => {
                let fmt = TextFormat {
                    font_id: egui::FontId::monospace(BASE_SIZE),
                    color: palette.text,
                    background: palette.code_bg,
                    ..Default::default()
                };
                match task.as_mut() {
                    Some((_, _, content)) => content.append(&text, 0.0, fmt),
                    None => current.append(&text, 0.0, fmt),
                }
            }
            Event::SoftBreak => current.append(" ", 0.0, plain(BASE_SIZE, palette.text)),
            Event::HardBreak => current.append("\n", 0.0, plain(BASE_SIZE, palette.text)),
            Event::Rule => current.append("─────────────\n\n", 0.0, plain(BASE_SIZE, palette.dim)),
            _ => {}
        }
    }

    if !current.text.is_empty() {
        blocks.push(Block::Rich(current));
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            text: egui::Color32::WHITE,
            dim: egui::Color32::GRAY,
            strong: egui::Color32::WHITE,
            code_bg: egui::Color32::BLACK,
        }
    }

    /// The exact toggle logic `main.rs` applies to `marker_range` when a
    /// checkbox is clicked (see `GuiAction::ToggleCheckbox`), so a change to
    /// either side of that contract gets caught here.
    fn toggle(source: &str, marker_range: Range<usize>) -> String {
        let mid = marker_range.start + 1;
        let mut out = source.to_string();
        let is_unchecked = out.as_bytes().get(mid) == Some(&b' ');
        out.replace_range(mid..mid + 1, if is_unchecked { "x" } else { " " });
        out
    }

    #[test]
    fn task_items_become_task_blocks_with_correct_marker_ranges() {
        let source = "- [ ] foo\n- [x] bar\n";
        let blocks = build_blocks(source, &palette());
        let tasks: Vec<_> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Task { marker_range, checked, .. } => Some((marker_range.clone(), *checked)),
                Block::Rich(_) => None,
            })
            .collect();
        assert_eq!(tasks, vec![(2..5, false), (12..15, true)]);
        assert_eq!(&source[2..5], "[ ]");
        assert_eq!(&source[12..15], "[x]");
    }

    #[test]
    fn toggling_marker_range_flips_only_that_checkbox() {
        let source = "- [ ] foo\n- [x] bar\n";
        let (marker_range, _) = build_blocks(source, &palette())
            .into_iter()
            .find_map(|b| match b {
                Block::Task { marker_range, .. } => Some((marker_range, ())),
                Block::Rich(_) => None,
            })
            .unwrap();

        let toggled = toggle(source, marker_range);
        assert_eq!(toggled, "- [x] foo\n- [x] bar\n");

        // Toggling the (still unmoved) second checkbox afterwards must not
        // be affected by the first edit, since both edits are same-length.
        let toggled_again = toggle(&toggled, 12..15);
        assert_eq!(toggled_again, "- [x] foo\n- [ ] bar\n");
    }

    #[test]
    fn plain_bullet_items_are_not_task_blocks() {
        let blocks = build_blocks("- foo\n- bar\n", &palette());
        assert!(blocks.iter().all(|b| matches!(b, Block::Rich(_))));
    }
}
