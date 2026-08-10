//! Notion-style `/` slash commands for the note body editor.
//!
//! Typing `/` as the first character of a line (in vim Insert mode, body
//! field only — see `main.rs`'s `update_slash_menu`) opens a filterable menu
//! of Markdown snippets; picking one replaces `/query` with the snippet and
//! places the cursor where typing should continue.

/// One entry in the `/` menu.
pub struct SlashCommand {
    /// Shown as the menu item's main label.
    pub label: &'static str,
    /// Shown dimmed on the right of the label, as a preview of what gets inserted.
    pub hint: &'static str,
    /// Extra words matched against the typed query, beyond `label` itself
    /// (e.g. English aliases for a Japanese label).
    pub keywords: &'static str,
    /// Markdown text inserted in place of `/query`.
    pub insert: &'static str,
    /// Where the cursor lands afterward, as a char offset into `insert`.
    pub cursor_offset: usize,
}

pub const COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        label: "見出し1",
        hint: "# ",
        keywords: "h1 heading midashi",
        insert: "# ",
        cursor_offset: 2,
    },
    SlashCommand {
        label: "見出し2",
        hint: "## ",
        keywords: "h2 heading midashi",
        insert: "## ",
        cursor_offset: 3,
    },
    SlashCommand {
        label: "見出し3",
        hint: "### ",
        keywords: "h3 heading midashi",
        insert: "### ",
        cursor_offset: 4,
    },
    SlashCommand {
        label: "箇条書き",
        hint: "- ",
        keywords: "bullet list ul kajougaki risuto",
        insert: "- ",
        cursor_offset: 2,
    },
    SlashCommand {
        label: "番号付きリスト",
        hint: "1. ",
        keywords: "number ordered list ol bangou risuto",
        insert: "1. ",
        cursor_offset: 3,
    },
    SlashCommand {
        label: "チェックボックス",
        hint: "- [ ] ",
        keywords: "todo check task checkbox tasuku",
        insert: "- [ ] ",
        cursor_offset: 6,
    },
    SlashCommand {
        label: "引用",
        hint: "> ",
        keywords: "quote blockquote inyou",
        insert: "> ",
        cursor_offset: 2,
    },
    SlashCommand {
        label: "コードブロック",
        hint: "```",
        keywords: "code block kakomi",
        insert: "```\n\n```",
        cursor_offset: 4,
    },
    SlashCommand {
        label: "区切り線",
        hint: "---",
        keywords: "divider hr rule line kugiri",
        insert: "---\n",
        cursor_offset: 4,
    },
    SlashCommand {
        label: "リンク",
        hint: "[[...]]",
        keywords: "link wikilink rinku",
        insert: "[[]]",
        cursor_offset: 2,
    },
];

/// Commands whose label or keywords contain `query` (case-insensitive
/// substring match), in menu order. Empty `query` matches everything.
pub fn matching(query: &str) -> Vec<&'static SlashCommand> {
    if query.is_empty() {
        return COMMANDS.iter().collect();
    }
    let q = query.to_lowercase();
    COMMANDS
        .iter()
        .filter(|c| c.label.to_lowercase().contains(&q) || c.keywords.to_lowercase().contains(&q))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(matching("").len(), COMMANDS.len());
    }

    #[test]
    fn matches_japanese_label_substring() {
        let m = matching("見出し");
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn matches_english_keyword_alias() {
        let m = matching("todo");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].label, "チェックボックス");
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(matching("zzz-no-such-command").is_empty());
    }
}
