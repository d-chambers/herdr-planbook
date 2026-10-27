//! The two-field YAML header and the original, unmodified Markdown source.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::PathBuf, sync::LazyLock};
use yaml_rust2::{
    Yaml,
    parser::{Event, Parser},
    scanner::TScalarStyle,
};

pub const STATUSES: [&str; 5] = ["proposed", "active", "parked", "done", "superseded"];
pub const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub status: String,
    pub body: String,
    pub source: String,
    pub visibility: String,
}

#[derive(Serialize)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

impl Project {
    pub fn revision(&self) -> String {
        format!("{:x}", Sha256::digest(self.source.as_bytes()))
    }
    pub fn progress(&self) -> Progress {
        let tasks = tasks(&self.body);
        Progress {
            done: tasks.iter().filter(|(done, _)| *done).count(),
            total: tasks.len(),
        }
    }
    pub fn next_action(&self) -> String {
        let mut inside = false;
        let mut paragraph = Vec::new();
        for line in prose_lines(&self.body) {
            if line.starts_with("## ") {
                if inside {
                    break;
                }
                inside = line.trim().eq_ignore_ascii_case("## Next action");
            } else if inside {
                if line.trim().is_empty() {
                    if !paragraph.is_empty() {
                        break;
                    }
                } else {
                    paragraph.push(line.trim());
                }
            }
        }
        if !paragraph.is_empty() {
            return paragraph.join(" ");
        }
        tasks(&self.body)
            .into_iter()
            .find_map(|(done, text)| (!done).then_some(text))
            .unwrap_or_default()
    }
    pub fn record(&self) -> serde_json::Value {
        serde_json::json!({"id":self.id,"path":self.path.to_string_lossy(),"title":self.title,"status":self.status,
            "visibility":self.visibility,"progress":self.progress(),"next_action":self.next_action(),"revision":self.revision()})
    }
}

fn prose_lines(body: &str) -> Vec<&str> {
    let mut fence: Option<(char, usize)> = None;
    body.lines()
        .filter(|line| {
            let trimmed = line.trim_start_matches(' ');
            if line.len() - trimmed.len() <= 3 {
                let mark = trimmed.chars().next().unwrap_or_default();
                let count = trimmed.chars().take_while(|c| *c == mark).count();
                if matches!(mark, '`' | '~') && count >= 3 {
                    match fence {
                        None => fence = Some((mark, count)),
                        Some((old, length))
                            if mark == old
                                && count >= length
                                && trimmed[count..].trim().is_empty() =>
                        {
                            fence = None
                        }
                        _ => (),
                    }
                    return false;
                }
            }
            fence.is_none()
        })
        .collect()
}

pub fn tasks(body: &str) -> Vec<(bool, String)> {
    static TASK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*[-*+] \[([ xX])\]\s+(.+)").expect("valid task regex"));
    prose_lines(body)
        .into_iter()
        .filter_map(|line| {
            TASK.captures(line)
                .map(|c| (c[1].eq_ignore_ascii_case("x"), c[2].to_owned()))
        })
        .collect()
}

fn metadata(header: &str) -> Result<BTreeMap<String, String>> {
    ensure!(header.len() <= 8192, "Metadata exceeds 8 KiB");
    let mut parser = Parser::new_from_str(header);
    let mut values = BTreeMap::new();
    let mut key = None;
    let mut opened = false;
    let mut closed = false;
    let mut documents = 0;
    loop {
        let (event, _) = parser.next_token().context("Invalid YAML metadata")?;
        match event {
            Event::StreamEnd => break,
            Event::StreamStart | Event::DocumentEnd => (),
            Event::DocumentStart => {
                documents += 1;
                ensure!(documents == 1, "Use one metadata mapping");
            }
            Event::MappingStart(0, None) if !opened => opened = true,
            Event::MappingEnd if opened && !closed && key.is_none() => closed = true,
            Event::Scalar(value, style, 0, None) if opened && !closed => {
                if let Some(name) = key.take() {
                    ensure!(
                        style != TScalarStyle::Plain
                            || matches!(Yaml::from_str(&value), Yaml::String(_)),
                        "Metadata values must be text"
                    );
                    ensure!(
                        values.insert(name, value).is_none(),
                        "Duplicate metadata key"
                    );
                } else {
                    ensure!(
                        matches!(value.as_str(), "title" | "status"),
                        "Metadata must contain exactly title and status"
                    );
                    key = Some(value);
                }
            }
            _ => bail!(
                "Use one literal title/status mapping; nested values, tags, anchors and aliases are unsupported"
            ),
        }
    }
    ensure!(
        opened && closed && values.len() == 2,
        "Metadata must contain exactly title and status"
    );
    Ok(values)
}

pub fn parse(source: String, path: PathBuf, id: String) -> Result<Project> {
    ensure!(source.len() <= MAX_BYTES, "project.md exceeds 256 KiB");
    let mut lines = source.split_inclusive('\n');
    let first = lines.next().context("project.md needs YAML frontmatter")?;
    ensure!(
        first.trim() == "---",
        "project.md needs YAML frontmatter with title and status"
    );
    let mut offset = first.len();
    let mut header_end = None;
    for line in lines {
        if line.trim() == "---" {
            header_end = Some((offset, offset + line.len()));
            break;
        }
        offset += line.len();
    }
    let (end, body_start) = header_end.context("YAML frontmatter has no closing ---")?;
    let meta = metadata(&source[first.len()..end])?;
    let title = meta["title"].clone();
    ensure!(
        !title.trim().is_empty()
            && title.chars().count() <= 200
            && !title.chars().any(char::is_control),
        "title must be one nonblank line without control characters (max 200)"
    );
    let status = meta["status"].clone();
    ensure!(
        STATUSES.contains(&status.as_str()),
        "status must be one of: {}",
        STATUSES.join(", ")
    );
    Ok(Project {
        id,
        path,
        title,
        status,
        body: source[body_start..].to_owned(),
        source,
        visibility: "untracked".into(),
    })
}

pub fn read(path: PathBuf, id: String) -> Result<Project> {
    let metadata = std::fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file(),
        "project.md must be a regular file, not a symbolic link"
    );
    let mut bytes = Vec::new();
    File::open(&path)?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_BYTES, "project.md exceeds 256 KiB");
    parse(
        String::from_utf8(bytes).context("project.md must be UTF-8")?,
        path,
        id,
    )
}

pub fn with_status(project: &Project, status: &str) -> Result<Project> {
    ensure!(
        STATUSES.contains(&status),
        "Unknown project status: {status}"
    );
    static STATUS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(status[ \t]*:[ \t]*)([^\r\n]*?)([ \t]+#.*)?(\r?\n)?$")
            .expect("valid status regex")
    });
    let mut source = String::new();
    let mut replaced = false;
    let mut delimiters = 0;
    for line in project.source.split_inclusive('\n') {
        if line.trim() == "---" {
            delimiters += 1;
        }
        if !replaced
            && delimiters == 1
            && let Some(c) = STATUS.captures(line)
        {
            let scalar = c[2].trim_end();
            let quote = match (scalar.as_bytes().first(), scalar.as_bytes().last()) {
                (Some(b'\''), Some(b'\'')) => "'",
                (Some(b'"'), Some(b'"')) => "\"",
                _ => "",
            };
            source.push_str(&format!(
                "{}{quote}{status}{quote}{}{}{}",
                &c[1],
                &c[2][scalar.len()..],
                c.get(3).map_or("", |m| m.as_str()),
                c.get(4).map_or("", |m| m.as_str())
            ));
            replaced = true;
        } else {
            source.push_str(line);
        }
    }
    ensure!(
        replaced,
        "Use a plain status: line to change status through Planbook"
    );
    parse(source, project.path.clone(), project.id.clone())
}

pub fn terminal_text(value: &str) -> String {
    let mut safe = String::new();
    for c in value.replace("\r\n", "\n").chars() {
        if c.is_control() && !matches!(c, '\t' | '\n') {
            safe.push_str(&format!("\\x{:02x}", c as u32));
        } else {
            safe.push(c);
        }
    }
    safe
}

/// JSON stays machine-readable while C1 terminal controls remain escaped.
pub fn json_text(value: &serde_json::Value) -> String {
    value
        .to_string()
        .chars()
        .map(|c| {
            if ('\u{7f}'..='\u{9f}').contains(&c) {
                format!("\\u{:04x}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}
