use eframe::egui;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

pub struct Palette {
    pub text: egui::Color32,
    pub dim: egui::Color32,
    pub strong: egui::Color32,
    pub accent: egui::Color32,
    pub code_bg: egui::Color32,
}

/// Renders `source` as read-only rich text (headings, bold/italic, lists,
/// task-list checkboxes, inline/block code, block quotes, rules). `[[...]]`
/// wikilinks aren't CommonMark, so pulldown-cmark just sees them as literal
/// text here — resolving/navigating them is handled separately by the
/// outgoing-links/backlinks rows shown below the body (see `main.rs`).
pub fn render(ui: &mut egui::Ui, source: &str, palette: &Palette) {
    let job = build_job(source, palette);
    ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Wrap));
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

fn build_job(source: &str, palette: &Palette) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};

    const BASE_SIZE: f32 = 14.5;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let mut job = LayoutJob::default();
    let mut bold_depth = 0u32;
    let mut code_depth = 0u32;
    let mut quote_depth = 0u32;
    let mut heading: Option<HeadingLevel> = None;
    let mut list_stack: Vec<Option<u64>> = Vec::new();

    let plain = |size: f32, color: egui::Color32| TextFormat {
        font_id: egui::FontId::proportional(size),
        color,
        ..Default::default()
    };

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Strong => bold_depth += 1,
                Tag::Heading { level, .. } => heading = Some(level),
                Tag::CodeBlock(_) => code_depth += 1,
                Tag::BlockQuote(_) => quote_depth += 1,
                Tag::List(start) => list_stack.push(start),
                Tag::Item => {
                    let depth = list_stack.len().saturating_sub(1);
                    let indent = "    ".repeat(depth);
                    let marker = match list_stack.last() {
                        Some(Some(n)) => format!("{indent}{n}. "),
                        _ => format!("{indent}•  "),
                    };
                    job.append(&marker, 0.0, plain(BASE_SIZE, palette.dim));
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Strong => bold_depth = bold_depth.saturating_sub(1),
                TagEnd::Heading(_) => {
                    heading = None;
                    job.append("\n\n", 0.0, plain(BASE_SIZE, palette.text));
                }
                TagEnd::CodeBlock => {
                    code_depth = code_depth.saturating_sub(1);
                    job.append("\n", 0.0, plain(BASE_SIZE, palette.text));
                }
                TagEnd::BlockQuote(_) => quote_depth = quote_depth.saturating_sub(1),
                TagEnd::List(_) => {
                    list_stack.pop();
                }
                TagEnd::Item => job.append("\n", 0.0, plain(BASE_SIZE, palette.text)),
                TagEnd::Paragraph => job.append("\n\n", 0.0, plain(BASE_SIZE, palette.text)),
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
                job.append(&text, 0.0, TextFormat { font_id, color, background, ..Default::default() });
            }
            Event::Code(text) => {
                job.append(
                    &text,
                    0.0,
                    TextFormat {
                        font_id: egui::FontId::monospace(BASE_SIZE),
                        color: palette.text,
                        background: palette.code_bg,
                        ..Default::default()
                    },
                );
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "☑ " } else { "☐ " };
                job.append(marker, 0.0, plain(BASE_SIZE, palette.accent));
            }
            Event::SoftBreak => job.append(" ", 0.0, plain(BASE_SIZE, palette.text)),
            Event::HardBreak => job.append("\n", 0.0, plain(BASE_SIZE, palette.text)),
            Event::Rule => job.append("─────────────\n\n", 0.0, plain(BASE_SIZE, palette.dim)),
            _ => {}
        }
    }

    job
}
