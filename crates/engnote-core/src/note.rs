use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Active,
    Archived,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Status::Draft => "draft",
            Status::Active => "active",
            Status::Archived => "archived",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub id: String,
    pub title: String,
    pub status: Status,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<String>,
    pub created: DateTime<Local>,
    pub updated: DateTime<Local>,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub frontmatter: Frontmatter,
    pub body: String,
}

impl Note {
    pub fn new(title: impl Into<String>, context_path: Option<String>) -> Self {
        let now = Local::now();
        Note {
            frontmatter: Frontmatter {
                id: Ulid::generate().to_string(),
                title: title.into(),
                status: Status::Draft,
                tags: Vec::new(),
                context_path,
                created: now,
                updated: now,
            },
            body: String::new(),
        }
    }

    /// Filename per .claude/rules/note-format.md: `YYYYMMDD-HHMMSS-{title}.md`,
    /// slug omitted when the title is empty.
    pub fn filename(&self) -> String {
        let timestamp = self.frontmatter.created.format("%Y%m%d-%H%M%S");
        let slug = sanitize_title(&self.frontmatter.title);
        if slug.is_empty() {
            format!("{timestamp}.md")
        } else {
            format!("{timestamp}-{slug}.md")
        }
    }

    pub fn to_file_contents(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.frontmatter).context("serializing frontmatter")?;
        Ok(format!("---\n{yaml}---\n{}", self.body))
    }

    pub fn parse(contents: &str) -> Result<Self> {
        let rest = contents
            .strip_prefix("---\n")
            .context("missing frontmatter delimiter")?;
        let end = rest.find("\n---\n").context("missing closing frontmatter delimiter")?;
        let (yaml, body) = rest.split_at(end);
        let body = &body[5..]; // skip "\n---\n"
        let frontmatter: Frontmatter = serde_yaml::from_str(yaml).context("parsing frontmatter")?;
        Ok(Note {
            frontmatter,
            body: body.to_string(),
        })
    }
}

/// Strips characters illegal in filenames and truncates to a reasonable length,
/// per .claude/rules/note-format.md. Non-ASCII (Japanese) titles are kept as-is.
fn sanitize_title(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let trimmed = cleaned.trim();
    trimmed.chars().take(50).collect()
}

pub fn parse_status(s: &str) -> Result<Status> {
    match s {
        "draft" => Ok(Status::Draft),
        "active" => Ok(Status::Active),
        "archived" => Ok(Status::Archived),
        other => bail!("invalid status: {other} (expected draft, active, or archived)"),
    }
}
