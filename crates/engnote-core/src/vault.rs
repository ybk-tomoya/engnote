use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::note::Note;

/// Resolves the vault directory: `$ENGNOTE_VAULT_DIR` if set, else `~/.engnote/vault`.
/// Creates it if missing.
pub fn resolve_vault_dir() -> Result<PathBuf> {
    let dir = match std::env::var_os("ENGNOTE_VAULT_DIR") {
        Some(path) => PathBuf::from(path),
        None => dirs::home_dir()
            .context("could not determine home directory")?
            .join(".engnote")
            .join("vault"),
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("creating vault dir {}", dir.display()))?;
    Ok(dir)
}

pub fn save(vault_dir: &Path, note: &Note) -> Result<PathBuf> {
    let path = vault_dir.join(note.filename());
    save_at(&path, note)?;
    Ok(path)
}

/// Overwrites a note at its existing path. The filename is a creation-time hint
/// only (see .claude/rules/note-format.md), so edits never rename the file —
/// identity is tracked via frontmatter.id instead.
pub fn save_at(path: &Path, note: &Note) -> Result<()> {
    std::fs::write(path, note.to_file_contents()?)
        .with_context(|| format!("writing note to {}", path.display()))
}

pub fn list(vault_dir: &Path) -> Result<Vec<(PathBuf, Note)>> {
    let mut notes = Vec::new();
    for entry in std::fs::read_dir(vault_dir)
        .with_context(|| format!("reading vault dir {}", vault_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let note = Note::parse(&contents).with_context(|| format!("parsing {}", path.display()))?;
        notes.push((path, note));
    }
    notes.sort_by(|a, b| a.1.frontmatter.created.cmp(&b.1.frontmatter.created));
    Ok(notes)
}

/// Resolves a note by ULID suffix match against `frontmatter.id` (suffix, not
/// prefix, because a ULID's leading characters encode the timestamp, so notes
/// created close together share a prefix; the trailing characters are random).
/// Falls back to a case-insensitive title substring match when no id matches.
pub fn find_by_ref(vault_dir: &Path, note_ref: &str) -> Result<(PathBuf, Note)> {
    let mut notes = list(vault_dir)?;

    let mut candidates: Vec<usize> = notes
        .iter()
        .enumerate()
        .filter(|(_, (_, note))| note.frontmatter.id.ends_with(note_ref))
        .map(|(i, _)| i)
        .collect();

    if candidates.is_empty() {
        let query = note_ref.to_lowercase();
        candidates = notes
            .iter()
            .enumerate()
            .filter(|(_, (_, note))| note.frontmatter.title.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();
    }

    match candidates.len() {
        0 => bail!("no note matches ref '{note_ref}'"),
        1 => Ok(notes.remove(candidates[0])),
        _ => {
            let labels: Vec<_> = candidates
                .iter()
                .map(|&i| format!("{} ({})", notes[i].1.frontmatter.id, notes[i].1.frontmatter.title))
                .collect();
            bail!("ref '{note_ref}' is ambiguous, matches: {}", labels.join(", "))
        }
    }
}
