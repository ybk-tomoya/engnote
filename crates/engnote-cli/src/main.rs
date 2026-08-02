use std::io::Write;

use anyhow::{bail, Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use engnote_core::note::{parse_status, Note};
use engnote_core::vault;

#[derive(Parser)]
#[command(name = "engnote")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Quick capture: save text as a draft note immediately.
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
        #[arg(long)]
        context: Option<String>,
    },
    /// List notes.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long = "tag")]
        tag: Option<String>,
        #[arg(long)]
        context: Option<String>,
    },
    /// List notes whose context_path covers the current directory.
    Here {
        #[arg(long)]
        status: Option<String>,
    },
    /// Print a note's contents.
    Show { note_ref: String },
    /// Open a note in $EDITOR.
    Edit { note_ref: String },
    /// Delete a note.
    Rm { note_ref: String },
    /// Add or remove a tag.
    Tag {
        note_ref: String,
        #[command(subcommand)]
        action: TagAction,
    },
    /// Change a note's status.
    Status { note_ref: String, value: String },
}

#[derive(Subcommand)]
enum TagAction {
    Add { tag: String },
    Rm { tag: String },
}

fn main() -> Result<()> {
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
        Command::List { status, tag, context } => list(&vault_dir, status, tag, context),
        Command::Here { status } => here(&vault_dir, status),
        Command::Show { note_ref } => show(&vault_dir, &note_ref),
        Command::Edit { note_ref } => edit(&vault_dir, &note_ref),
        Command::Rm { note_ref } => rm(&vault_dir, &note_ref),
        Command::Tag { note_ref, action } => tag(&vault_dir, &note_ref, action),
        Command::Status { note_ref, value } => status(&vault_dir, &note_ref, &value),
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

fn list(
    vault_dir: &std::path::Path,
    status: Option<String>,
    tag: Option<String>,
    context: Option<String>,
) -> Result<()> {
    let status = status.map(|s| parse_status(&s)).transpose()?;
    let notes = vault::list(vault_dir)?;
    for (_, note) in notes.iter().filter(|(_, n)| {
        status.is_none_or(|s| n.frontmatter.status == s)
            && tag.as_deref().is_none_or(|t| n.frontmatter.tags.iter().any(|nt| nt == t))
            && context
                .as_deref()
                .is_none_or(|c| n.frontmatter.context_path.as_deref() == Some(c))
    }) {
        print_row(note);
    }
    Ok(())
}

fn here(vault_dir: &std::path::Path, status: Option<String>) -> Result<()> {
    let status = status.map(|s| parse_status(&s)).transpose()?;
    let cwd = std::env::current_dir().context("reading current directory")?;
    let notes = vault::list(vault_dir)?;
    for (_, note) in notes.iter().filter(|(_, n)| {
        status.is_none_or(|s| n.frontmatter.status == s)
            && n.frontmatter
                .context_path
                .as_deref()
                .is_some_and(|ctx| cwd.starts_with(ctx))
    }) {
        print_row(note);
    }
    Ok(())
}

fn print_row(note: &Note) {
    let id = &note.frontmatter.id;
    let short_id = &id[id.len().saturating_sub(7)..];
    let tags = note.frontmatter.tags.join(",");
    println!(
        "{short_id}  {status:<8}  {title}  [{tags}]",
        status = note.frontmatter.status.to_string(),
        title = note.frontmatter.title,
    );
}

fn show(vault_dir: &std::path::Path, note_ref: &str) -> Result<()> {
    let (_, note) = vault::find_by_ref(vault_dir, note_ref)?;
    print!("{}", note.to_file_contents()?);
    Ok(())
}

fn edit(vault_dir: &std::path::Path, note_ref: &str) -> Result<()> {
    let (path, note) = vault::find_by_ref(vault_dir, note_ref)?;
    let edited = edit_in_editor(&note.to_file_contents()?)?;
    let mut note = Note::parse(&edited)?;
    note.frontmatter.updated = Local::now();
    vault::save_at(&path, &note)?;
    println!("updated {}", note.frontmatter.id);
    Ok(())
}

fn rm(vault_dir: &std::path::Path, note_ref: &str) -> Result<()> {
    let (path, note) = vault::find_by_ref(vault_dir, note_ref)?;
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

fn tag(vault_dir: &std::path::Path, note_ref: &str, action: TagAction) -> Result<()> {
    let (path, mut note) = vault::find_by_ref(vault_dir, note_ref)?;
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

fn status(vault_dir: &std::path::Path, note_ref: &str, value: &str) -> Result<()> {
    let (path, mut note) = vault::find_by_ref(vault_dir, note_ref)?;
    note.frontmatter.status = parse_status(value)?;
    note.frontmatter.updated = Local::now();
    vault::save_at(&path, &note)?;
    println!("status: {}", note.frontmatter.status);
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
