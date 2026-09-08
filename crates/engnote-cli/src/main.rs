use std::io::Write;

use anyhow::{bail, Context, Result};
use chrono::Local;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use engnote_core::note::Note;
use engnote_core::vault;

#[derive(Parser)]
#[command(name = "engnote")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Quick capture: save text as a note immediately.
    Capture {
        text: Vec<String>,
        /// Attach the note to the current directory (context_path).
        #[arg(long)]
        here: bool,
    },
    /// Open $EDITOR to write a fuller note.
    New {
        #[arg(long)]
        title: Option<String>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Attach the note to the current directory (context_path).
        #[arg(long)]
        here: bool,
        #[arg(long, value_hint = clap::ValueHint::AnyPath)]
        context: Option<String>,
    },
    /// List notes.
    List {
        #[arg(long = "tag")]
        tag: Option<String>,
        #[arg(long, value_hint = clap::ValueHint::AnyPath)]
        context: Option<String>,
    },
    /// List notes whose context_path covers the current directory.
    Here,
    /// Print a note's contents. Omit <ref> to pick interactively.
    Show {
        #[arg(add = ArgValueCompleter::new(complete_note_ref))]
        note_ref: Option<String>,
    },
    /// Open a note in $EDITOR. Omit <ref> to pick interactively.
    Edit {
        #[arg(add = ArgValueCompleter::new(complete_note_ref))]
        note_ref: Option<String>,
    },
    /// Delete a note. Omit <ref> to pick interactively.
    Rm {
        #[arg(add = ArgValueCompleter::new(complete_note_ref))]
        note_ref: Option<String>,
    },
    /// Add or remove a tag. Omit <ref> to pick interactively.
    Tag {
        #[arg(add = ArgValueCompleter::new(complete_note_ref))]
        note_ref: Option<String>,
        #[command(subcommand)]
        action: TagAction,
    },
}

#[derive(Subcommand)]
enum TagAction {
    Add { tag: String },
    Rm { tag: String },
}

fn main() -> Result<()> {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    let vault_dir = vault::resolve_vault_dir()?;

    match cli.command {
        Command::Capture { text, here } => capture(&vault_dir, text, here),
        Command::New {
            title,
            tags,
            here,
            context,
        } => new(&vault_dir, title, tags, here, context),
        Command::List { tag, context } => list(&vault_dir, tag, context),
        Command::Here => here(&vault_dir),
        Command::Show { note_ref } => show(&vault_dir, note_ref),
        Command::Edit { note_ref } => edit(&vault_dir, note_ref),
        Command::Rm { note_ref } => rm(&vault_dir, note_ref),
        Command::Tag { note_ref, action } => tag(&vault_dir, note_ref, action),
    }
}

fn resolve_context(here: bool, context: Option<String>) -> Result<Option<String>> {
    if let Some(path) = context {
        return Ok(Some(path));
    }
    if here {
        let cwd = std::env::current_dir().context("reading current directory")?;
        return Ok(Some(cwd.to_string_lossy().into_owned()));
    }
    Ok(None)
}

fn derive_title(text: &str) -> String {
    text.lines().next().unwrap_or("").chars().take(60).collect()
}

fn capture(vault_dir: &std::path::Path, text: Vec<String>, here: bool) -> Result<()> {
    let text = text.join(" ");
    if text.trim().is_empty() {
        bail!("capture text must not be empty");
    }
    let context_path = resolve_context(here, None)?;
    let mut note = Note::new(derive_title(&text), context_path);
    note.body = text;
    let path = vault::save(vault_dir, &note)?;
    println!("captured {} ({})", note.frontmatter.id, path.display());
    Ok(())
}

fn new(
    vault_dir: &std::path::Path,
    title: Option<String>,
    tags: Vec<String>,
    here: bool,
    context: Option<String>,
) -> Result<()> {
    let context_path = resolve_context(here, context)?;
    let mut note = Note::new(title.unwrap_or_default(), context_path);
    note.frontmatter.tags = tags;

    let edited = edit_in_editor(&note.to_file_contents()?)?;
    let note = Note::parse(&edited)?;
    let path = vault::save(vault_dir, &note)?;
    println!("saved {} ({})", note.frontmatter.id, path.display());
    Ok(())
}

fn list(vault_dir: &std::path::Path, tag: Option<String>, context: Option<String>) -> Result<()> {
    let notes = vault::list(vault_dir)?;
    for (_, note) in notes.iter().filter(|(_, n)| {
        tag.as_deref().is_none_or(|t| n.frontmatter.tags.iter().any(|nt| nt == t))
            && context
                .as_deref()
                .is_none_or(|c| n.frontmatter.context_path.as_deref() == Some(c))
    }) {
        print_row(note);
    }
    Ok(())
}

fn here(vault_dir: &std::path::Path) -> Result<()> {
    let cwd = std::env::current_dir().context("reading current directory")?;
    let notes = vault::list(vault_dir)?;
    for (_, note) in notes.iter().filter(|(_, n)| {
        n.frontmatter
            .context_path
            .as_deref()
            .is_some_and(|ctx| cwd.starts_with(ctx))
    }) {
        print_row(note);
    }
    Ok(())
}

fn print_row(note: &Note) {
    let tags = note.frontmatter.tags.join(",");
    println!(
        "{short_id}  {title}  [{tags}]",
        short_id = note.short_id(),
        title = note.frontmatter.title,
    );
}

/// Dynamic shell completion for `<ref>` arguments (see `CompleteEnv` wiring in
/// `main`): matches on short id prefix or a title substring, same fallback as
/// `vault::find_by_ref`.
fn complete_note_ref(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let Ok(vault_dir) = vault::resolve_vault_dir() else {
        return Vec::new();
    };
    let Ok(notes) = vault::list(&vault_dir) else {
        return Vec::new();
    };
    let current = current.to_string_lossy();
    let current_lower = current.to_lowercase();

    notes
        .iter()
        .filter(|(_, note)| {
            current.is_empty()
                || note.short_id().starts_with(current.as_ref())
                || note.frontmatter.title.to_lowercase().contains(&current_lower)
        })
        .map(|(_, note)| {
            let help = format!(
                "{title} [{tags}]",
                title = note.frontmatter.title,
                tags = note.frontmatter.tags.join(","),
            );
            CompletionCandidate::new(note.short_id()).help(Some(help.into()))
        })
        .collect()
}

/// Resolves a note by `<ref>`, or, when omitted, lets the user pick one
/// interactively (fuzzy search over title/tags).
fn resolve_note(vault_dir: &std::path::Path, note_ref: Option<String>) -> Result<(std::path::PathBuf, Note)> {
    if let Some(note_ref) = note_ref {
        return vault::find_by_ref(vault_dir, &note_ref);
    }

    let mut notes = vault::list(vault_dir)?;
    if notes.is_empty() {
        bail!("no notes in vault");
    }
    let labels: Vec<String> = notes
        .iter()
        .map(|(_, note)| {
            format!(
                "{short_id}  {title}  [{tags}]",
                short_id = note.short_id(),
                title = note.frontmatter.title,
                tags = note.frontmatter.tags.join(","),
            )
        })
        .collect();

    let selection = dialoguer::FuzzySelect::new()
        .with_prompt("select a note")
        .items(&labels)
        .interact_opt()
        .context("running interactive note picker")?;

    match selection {
        Some(i) => Ok(notes.remove(i)),
        None => bail!("no note selected"),
    }
}

fn show(vault_dir: &std::path::Path, note_ref: Option<String>) -> Result<()> {
    let (_, note) = resolve_note(vault_dir, note_ref)?;
    print!("{}", note.to_file_contents()?);
    Ok(())
}

fn edit(vault_dir: &std::path::Path, note_ref: Option<String>) -> Result<()> {
    let (path, note) = resolve_note(vault_dir, note_ref)?;
    let edited = edit_in_editor(&note.to_file_contents()?)?;
    let mut note = Note::parse(&edited)?;
    note.frontmatter.updated = Local::now();
    vault::save_at(&path, &note)?;
    println!("updated {}", note.frontmatter.id);
    Ok(())
}

fn rm(vault_dir: &std::path::Path, note_ref: Option<String>) -> Result<()> {
    let (path, note) = resolve_note(vault_dir, note_ref)?;
    print!("delete '{}' ({})? [y/N] ", note.frontmatter.title, note.frontmatter.id);
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if answer.trim().eq_ignore_ascii_case("y") {
        std::fs::remove_file(&path)?;
        println!("deleted {}", note.frontmatter.id);
    } else {
        println!("cancelled");
    }
    Ok(())
}

fn tag(vault_dir: &std::path::Path, note_ref: Option<String>, action: TagAction) -> Result<()> {
    let (path, mut note) = resolve_note(vault_dir, note_ref)?;
    match action {
        TagAction::Add { tag } => {
            if !note.frontmatter.tags.contains(&tag) {
                note.frontmatter.tags.push(tag);
            }
        }
        TagAction::Rm { tag } => note.frontmatter.tags.retain(|t| t != &tag),
    }
    note.frontmatter.updated = Local::now();
    vault::save_at(&path, &note)?;
    println!("tags: [{}]", note.frontmatter.tags.join(","));
    Ok(())
}

fn edit_in_editor(initial: &str) -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("engnote-{}.md", ulid::Ulid::generate()));
    std::fs::write(&tmp, initial)?;

    let status = std::process::Command::new(&editor)
        .arg(&tmp)
        .status()
        .with_context(|| format!("launching editor '{editor}'"))?;
    if !status.success() {
        bail!("editor exited with {status}");
    }

    let edited = std::fs::read_to_string(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(edited)
}
