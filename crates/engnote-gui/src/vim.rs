use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorMode {
    Normal,
    Insert,
}

/// Applies one Normal-mode command character to `text`/`cursor` (cursor
/// motion or an edit), possibly switching `editor_mode` to `Insert`. Works on
/// any field, not just the note body — a field with no `\n` (title, tags) is
/// just a "document" with a single line, so `0`/`$`/`gg`/`G`/`dd` degrade
/// naturally to "start/end of field" and "clear the field".
///
/// `g` and `d` are two-key commands (`gg`, `dd`); a lone `g`/`d` just arms
/// `pending_g`/`pending_d` for the next call.
#[allow(clippy::too_many_arguments)]
pub fn handle_normal_key(
    text: &mut String,
    cursor: &mut usize,
    pending_g: &mut bool,
    pending_d: &mut bool,
    editor_mode: &mut EditorMode,
    ch: char,
) {
    let mut chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    if *pending_g {
        *pending_g = false;
        if ch == 'g' {
            *cursor = 0;
        }
        return;
    }
    if *pending_d {
        *pending_d = false;
        if ch == 'd' {
            delete_line(text, cursor);
        }
        return;
    }

    match ch {
        'h' => *cursor = cursor.saturating_sub(1),
        'l' => *cursor = (*cursor + 1).min(len),
        'j' => *cursor = move_vertical(&chars, *cursor, 1),
        'k' => *cursor = move_vertical(&chars, *cursor, -1),
        'w' => *cursor = word_forward(&chars, *cursor),
        'b' => *cursor = word_backward(&chars, *cursor),
        '0' => *cursor = line_bounds(&chars, *cursor).0,
        '$' => {
            let (start, end) = line_bounds(&chars, *cursor);
            *cursor = if end > start { end - 1 } else { start };
        }
        'G' => *cursor = *line_starts(&chars).last().unwrap_or(&0),
        'g' => *pending_g = true,
        'd' => *pending_d = true,
        'x' => {
            if *cursor < len {
                chars.remove(*cursor);
                *cursor = (*cursor).min(chars.len());
                *text = chars.into_iter().collect();
            }
        }
        'i' => *editor_mode = EditorMode::Insert,
        'a' => {
            *cursor = (*cursor + 1).min(len);
            *editor_mode = EditorMode::Insert;
        }
        'I' => {
            *cursor = line_bounds(&chars, *cursor).0;
            *editor_mode = EditorMode::Insert;
        }
        'A' => {
            *cursor = line_bounds(&chars, *cursor).1;
            *editor_mode = EditorMode::Insert;
        }
        'o' => {
            let (_, end) = line_bounds(&chars, *cursor);
            chars.insert(end, '\n');
            *cursor = end + 1;
            *text = chars.into_iter().collect();
            *editor_mode = EditorMode::Insert;
        }
        'O' => {
            let (start, _) = line_bounds(&chars, *cursor);
            chars.insert(start, '\n');
            *cursor = start;
            *text = chars.into_iter().collect();
            *editor_mode = EditorMode::Insert;
        }
        _ => {}
    }
}

fn delete_line(text: &mut String, cursor: &mut usize) {
    let mut chars: Vec<char> = text.chars().collect();
    let (start, end) = line_bounds(&chars, *cursor);
    let remove_end = if end < chars.len() { end + 1 } else { end };
    chars.drain(start..remove_end);
    *cursor = start.min(chars.len());
    *text = chars.into_iter().collect();
}

fn line_starts(chars: &[char]) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, &c) in chars.iter().enumerate() {
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// `(line_start, line_end)`; `line_end` is the index of the line's `\n` (or
/// the text length for the last line) — i.e. one past the last character.
fn line_bounds(chars: &[char], pos: usize) -> (usize, usize) {
    let starts = line_starts(chars);
    let li = starts.binary_search(&pos).unwrap_or_else(|i| i - 1);
    let start = starts[li];
    let end = starts.get(li + 1).map_or(chars.len(), |s| s - 1);
    (start, end)
}

fn move_vertical(chars: &[char], pos: usize, delta: i32) -> usize {
    let starts = line_starts(chars);
    let li = starts.binary_search(&pos).unwrap_or_else(|i| i - 1);
    let col = pos - starts[li];
    let target_li = (li as i32 + delta).clamp(0, starts.len() as i32 - 1) as usize;
    let line_start = starts[target_li];
    let line_end = starts.get(target_li + 1).map_or(chars.len(), |s| s - 1);
    // `line_end` is the exclusive end of the line's content (the index of its
    // `\n`, if any); resting *on* that index would put the cursor on the
    // newline itself, so the last valid resting spot is one before it.
    let max_pos = if line_end > line_start { line_end - 1 } else { line_start };
    (line_start + col).min(max_pos)
}

fn word_forward(chars: &[char], pos: usize) -> usize {
    let len = chars.len();
    let mut i = pos;
    while i < len && !chars[i].is_whitespace() {
        i += 1;
    }
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn word_backward(chars: &[char], pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut i = pos - 1;
    while i > 0 && chars[i].is_whitespace() {
        i -= 1;
    }
    while i > 0 && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

/// Builds the read-only, cursor-highlighted view of the body shown in Normal
/// mode (a plain `egui::Label` can't show a caret, so we fake a block cursor
/// with a `LayoutJob` background span instead).
pub fn normal_mode_body_job(
    body: &str,
    cursor: usize,
    font_id: egui::FontId,
    text_color: egui::Color32,
    cursor_bg: egui::Color32,
    cursor_fg: egui::Color32,
) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};

    let chars: Vec<char> = body.chars().collect();
    let cursor = cursor.min(chars.len());

    let normal_fmt = TextFormat {
        font_id: font_id.clone(),
        color: text_color,
        ..Default::default()
    };
    let cursor_fmt = TextFormat {
        font_id,
        color: cursor_fg,
        background: cursor_bg,
        ..Default::default()
    };

    let mut job = LayoutJob::default();
    let before: String = chars[..cursor].iter().collect();
    job.append(&before, 0.0, normal_fmt.clone());

    if cursor < chars.len() && chars[cursor] != '\n' {
        job.append(&chars[cursor].to_string(), 0.0, cursor_fmt);
        let after: String = chars[cursor + 1..].iter().collect();
        job.append(&after, 0.0, normal_fmt);
    } else {
        // Cursor sits on a newline (or at EOF): fake a block cursor with a
        // highlighted space, then continue with the real remaining text.
        job.append(" ", 0.0, cursor_fmt);
        let after: String = chars[cursor..].iter().collect();
        job.append(&after, 0.0, normal_fmt);
    }

    job
}

#[cfg(test)]
mod tests {
    use super::*;

    struct St {
        text: String,
        cursor: usize,
        pending_g: bool,
        pending_d: bool,
        mode: EditorMode,
    }

    fn form_with(text: &str, cursor: usize) -> St {
        St {
            text: text.to_string(),
            cursor,
            pending_g: false,
            pending_d: false,
            mode: EditorMode::Normal,
        }
    }

    fn keys(st: &mut St, s: &str) {
        for ch in s.chars() {
            handle_normal_key(
                &mut st.text,
                &mut st.cursor,
                &mut st.pending_g,
                &mut st.pending_d,
                &mut st.mode,
                ch,
            );
        }
    }

    #[test]
    fn h_l_move_and_clamp() {
        let mut st = form_with("abc", 0);
        keys(&mut st, "llll");
        assert_eq!(st.cursor, 3); // clamped to len, not past it
        keys(&mut st, "h");
        assert_eq!(st.cursor, 2);
    }

    #[test]
    fn w_b_word_motion() {
        let mut st = form_with("foo bar baz", 0);
        keys(&mut st, "w");
        assert_eq!(st.cursor, 4); // start of "bar"
        keys(&mut st, "w");
        assert_eq!(st.cursor, 8); // start of "baz"
        keys(&mut st, "b");
        assert_eq!(st.cursor, 4); // back to "bar"
    }

    #[test]
    fn line_start_and_end() {
        let mut st = form_with("hello\nworld", 8); // 'r' in "world"
        keys(&mut st, "0");
        assert_eq!(st.cursor, 6); // start of "world"
        keys(&mut st, "$");
        assert_eq!(st.cursor, 10); // the 'd' in "world", not one past it
    }

    #[test]
    fn gg_and_g_uppercase() {
        let mut st = form_with("a\nb\nc", 0);
        keys(&mut st, "G");
        assert_eq!(st.cursor, 4); // start of last line ("c")
        keys(&mut st, "gg");
        assert_eq!(st.cursor, 0);
    }

    #[test]
    fn lone_g_is_pending_until_second_key() {
        let mut st = form_with("a\nb\nc", 4);
        keys(&mut st, "g");
        assert!(st.pending_g);
        assert_eq!(st.cursor, 4); // unchanged until the second `g`
        keys(&mut st, "x"); // anything other than `g` cancels it without acting
        assert!(!st.pending_g);
        assert_eq!(st.cursor, 4);
    }

    #[test]
    fn x_deletes_char_under_cursor() {
        let mut st = form_with("abc", 1);
        keys(&mut st, "x");
        assert_eq!(st.text, "ac");
        assert_eq!(st.cursor, 1);
    }

    #[test]
    fn dd_deletes_whole_line_and_clamps_cursor() {
        let mut st = form_with("line1\nline2\nline3", 7); // inside "line2"
        keys(&mut st, "dd");
        assert_eq!(st.text, "line1\nline3");
        assert_eq!(st.cursor, 6); // start of what's now "line3"
    }

    #[test]
    fn dd_on_last_line_without_trailing_newline() {
        let mut st = form_with("only", 2);
        keys(&mut st, "dd");
        assert_eq!(st.text, "");
        assert_eq!(st.cursor, 0);
    }

    #[test]
    fn dd_clears_a_single_line_field_like_title_or_tags() {
        let mut st = form_with("my title", 3);
        keys(&mut st, "dd");
        assert_eq!(st.text, "");
        assert_eq!(st.cursor, 0);
    }

    #[test]
    fn o_opens_line_below_and_enters_insert() {
        let mut st = form_with("abc", 1);
        keys(&mut st, "o");
        assert_eq!(st.text, "abc\n");
        assert_eq!(st.cursor, 4);
        assert_eq!(st.mode, EditorMode::Insert);
    }

    #[test]
    fn shift_o_opens_line_above_and_enters_insert() {
        let mut st = form_with("abc", 1);
        keys(&mut st, "O");
        assert_eq!(st.text, "\nabc");
        assert_eq!(st.cursor, 0);
        assert_eq!(st.mode, EditorMode::Insert);
    }

    #[test]
    fn i_enters_insert_at_cursor() {
        let mut st = form_with("abc", 1);
        keys(&mut st, "i");
        assert_eq!(st.cursor, 1);
        assert_eq!(st.mode, EditorMode::Insert);
    }

    #[test]
    fn a_enters_insert_after_cursor_and_clamps_at_eol() {
        let mut st = form_with("abc", 2);
        keys(&mut st, "a");
        assert_eq!(st.cursor, 3);
        assert_eq!(st.mode, EditorMode::Insert);

        let mut st = form_with("abc", 3);
        keys(&mut st, "a");
        assert_eq!(st.cursor, 3); // already at end, doesn't overshoot
    }

    #[test]
    fn shift_a_appends_at_end_of_line_not_just_after_cursor() {
        let mut st = form_with("hello\nworld", 1); // 'e' in "hello"
        keys(&mut st, "A");
        assert_eq!(st.cursor, 5); // right after "hello", before the '\n'
        assert_eq!(st.mode, EditorMode::Insert);
    }

    #[test]
    fn shift_i_inserts_at_start_of_line() {
        let mut st = form_with("hello\nworld", 8); // 'r' in "world"
        keys(&mut st, "I");
        assert_eq!(st.cursor, 6); // start of "world"
        assert_eq!(st.mode, EditorMode::Insert);
    }

    #[test]
    fn j_k_move_between_lines() {
        let mut st = form_with("hello\nworld", 2); // col 2 of "hello"
        keys(&mut st, "j");
        assert_eq!(st.cursor, 8); // col 2 of "world" (starts at 6)
        keys(&mut st, "k");
        assert_eq!(st.cursor, 2);
    }

    #[test]
    fn j_clamps_to_end_of_shorter_line() {
        // NB: this simplified implementation doesn't remember a "desired
        // column" across hops the way real vim does — the clamped position
        // becomes the new reference column for the next move.
        let mut st = form_with("hello\nhi", 4); // col 4 ('o') of "hello"
        keys(&mut st, "j");
        assert_eq!(st.cursor, 7); // clamped to 'i', the last char of "hi"
    }

    #[test]
    fn single_line_field_has_no_second_line_for_j_k() {
        // Confirms `j`/`k` are safe no-ops on a field with no `\n` (as used
        // for title/tags) rather than panicking or corrupting the cursor.
        let mut st = form_with("single line", 3);
        keys(&mut st, "jjkk");
        assert_eq!(st.cursor, 3);
    }
}
