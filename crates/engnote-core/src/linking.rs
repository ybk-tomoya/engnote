use std::path::PathBuf;

use crate::note::Note;

/// Result of resolving a `[[...]]` reference against the vault (see
/// .claude/rules/linking.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    Resolved(String),
    Ambiguous,
    Unresolved,
}

const ULID_CHARSET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Crockford Base32-looking, per .claude/rules/linking.md's id-vs-title
/// heuristic: not whether it's a *valid* ULID, just whether it's plausible.
///
/// Deliberately case-sensitive (checks the charset as typed, not
/// uppercased): ULIDs are conventionally written uppercase, and titles are
/// usually mixed/lowercase, so this is what actually distinguishes
/// `[[01J8X]]` from an ordinary title like `[[Target]]` that happens to
/// avoid the letters I/L/O/U.
fn looks_like_ulid_ref(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| ULID_CHARSET.contains(c))
}

/// Resolves one `[[...]]` reference's inner text against the vault: id
/// suffix match if it looks ULID-like, otherwise an exact (case-insensitive)
/// title match.
pub fn resolve_link(notes: &[(PathBuf, Note)], link_text: &str) -> LinkTarget {
    let link_text = link_text.trim();

    let matches: Vec<&str> = if looks_like_ulid_ref(link_text) {
        let needle = link_text.to_ascii_uppercase();
        notes
            .iter()
            .filter(|(_, n)| n.frontmatter.id.ends_with(&needle))
            .map(|(_, n)| n.frontmatter.id.as_str())
            .collect()
    } else {
        notes
            .iter()
            .filter(|(_, n)| n.frontmatter.title.eq_ignore_ascii_case(link_text))
            .map(|(_, n)| n.frontmatter.id.as_str())
            .collect()
    };

    match matches.len() {
        1 => LinkTarget::Resolved(matches[0].to_string()),
        0 => LinkTarget::Unresolved,
        _ => LinkTarget::Ambiguous,
    }
}

/// Extracts the raw contents of every `[[...]]` occurrence in a note body,
/// in order (duplicates kept as-is; caller dedupes if needed).
pub fn extract_links(body: &str) -> Vec<&str> {
    let mut links = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after_open = &rest[start + 2..];
        match after_open.find("]]") {
            Some(end) => {
                let content = after_open[..end].trim();
                if !content.is_empty() && !content.contains('\n') {
                    links.push(content);
                }
                rest = &after_open[end + 2..];
            }
            None => break,
        }
    }
    links
}

/// Notes whose body contains a `[[...]]` that resolves to `target_id`.
pub fn backlinks<'a>(notes: &'a [(PathBuf, Note)], target_id: &str) -> Vec<&'a Note> {
    notes
        .iter()
        .filter(|(_, n)| {
            extract_links(&n.body)
                .iter()
                .any(|link| matches!(resolve_link(notes, link), LinkTarget::Resolved(id) if id == target_id))
        })
        .map(|(_, n)| n)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;

    fn note_with(title: &str, body: &str) -> (PathBuf, Note) {
        let mut n = Note::new(title.to_string(), None);
        n.body = body.to_string();
        (PathBuf::from(format!("{title}.md")), n)
    }

    #[test]
    fn resolves_by_exact_title_case_insensitive() {
        let notes = vec![note_with("Rust Notes", ""), note_with("Other", "")];
        assert_eq!(
            resolve_link(&notes, "rust notes"),
            LinkTarget::Resolved(notes[0].1.frontmatter.id.clone())
        );
    }

    #[test]
    fn ambiguous_title_is_not_resolved() {
        let notes = vec![note_with("Dup", ""), note_with("Dup", "")];
        assert_eq!(resolve_link(&notes, "Dup"), LinkTarget::Ambiguous);
    }

    #[test]
    fn no_title_match_is_unresolved() {
        let notes = vec![note_with("Rust Notes", "")];
        assert_eq!(resolve_link(&notes, "Nonexistent"), LinkTarget::Unresolved);
    }

    #[test]
    fn resolves_by_ulid_suffix() {
        let notes = vec![note_with("Rust Notes", "")];
        let id = notes[0].1.frontmatter.id.clone();
        let suffix = &id[id.len() - 5..];
        assert_eq!(resolve_link(&notes, suffix), LinkTarget::Resolved(id));
    }

    #[test]
    fn extract_links_finds_all_and_ignores_unclosed() {
        let body = "See [[Foo]] and [[Bar Baz]], but not [[unclosed";
        assert_eq!(extract_links(body), vec!["Foo", "Bar Baz"]);
    }

    #[test]
    fn backlinks_finds_notes_referencing_target() {
        let target = note_with("Target", "");
        let target_id = target.1.frontmatter.id.clone();
        let referrer = note_with("Referrer", "see [[Target]]");
        let unrelated = note_with("Unrelated", "nothing here");
        let notes = vec![target, referrer, unrelated];

        let result = backlinks(&notes, &target_id);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].frontmatter.title, "Referrer");
    }
}
