use crate::{
    GUIDE, SKILL,
    documents::{STATUSES, json_text, terminal_text},
    paths::{self, Environment},
    process::capture,
    store::Store,
    ui,
};
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "planbook",
    version,
    about = "Repository project plans in a Herdr pane"
)]
pub struct Cli {
    /// Repository or folder (before the subcommand).
    #[arg(long)]
    pub repo: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Action,
}

#[derive(Subcommand)]
pub enum Action {
    /// Discover valid projects and report document errors.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Read a project by full id or unambiguous slug.
    Show {
        project: String,
        #[arg(long)]
        json: bool,
    },
    /// Create a project without replacing any existing directory.
    New {
        slug: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        private: bool,
        #[arg(long)]
        root: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Change a status explicitly, optionally checking a previously read revision.
    Status {
        project: String,
        #[arg(value_parser=STATUSES)]
        status: String,
        #[arg(long)]
        expect: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Run the terminal pane directly.
    Ui,
    /// Open a split pane inside the invoking Herdr workspace.
    Open,
    /// Print the bundled agent skill.
    Skill,
    /// Print project-format guidelines.
    Guide,
}

impl Cli {
    pub fn json(&self) -> bool {
        match self.command {
            Action::List { json }
            | Action::Show { json, .. }
            | Action::New { json, .. }
            | Action::Status { json, .. } => json,
            _ => false,
        }
    }
}

pub fn pane_command(root: &Path, env: &Environment) -> Result<Command> {
    ensure!(
        env.get("HERDR_ENV").is_some_and(|v| v == "1") && env.contains_key("HERDR_WORKSPACE_ID"),
        "Open must run inside Herdr; use planbook --repo PATH ui outside"
    );
    let context = paths::invocation_context(env)?;
    let target = context["focused_pane_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if env.contains_key("HERDR_PLUGIN_ID") {
                None
            } else {
                env.get("HERDR_PANE_ID").map(String::as_str)
            }
        })
        .context("Herdr supplied no target pane; open from a workspace pane")?;
    let mut command = Command::new(env.get("HERDR_BIN_PATH").map_or("herdr", String::as_str));
    command
        .args([
            "plugin",
            "pane",
            "open",
            "--plugin",
            "dchambers.planbook",
            "--entrypoint",
            "projects",
            "--target-pane",
            target,
            "--placement",
            "split",
            "--direction",
            "right",
            "--env",
        ])
        .arg(format!(
            "PLANBOOK_REPO={}",
            root.to_str()
                .context("Herdr repository path must be UTF-8")?
        ))
        .arg("--focus");
    Ok(command)
}

pub fn execute(cli: &Cli) -> Result<u8> {
    match cli.command {
        Action::Skill => {
            print!("{SKILL}");
            return Ok(0);
        }
        Action::Guide => {
            print!("{GUIDE}");
            return Ok(0);
        }
        _ => (),
    }
    let env = paths::environment();
    let root = paths::repository(cli.repo.as_deref(), &env)?;
    if matches!(cli.command, Action::Open) {
        let output = capture(&mut pane_command(&root, &env)?, Duration::from_secs(15))?;
        ensure!(
            output.status.success(),
            "Herdr could not open the pane: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(0);
    }
    let state = paths::state_dir(&env)?;
    if matches!(cli.command, Action::Ui) {
        ui::run(root, state, env)?;
        return Ok(0);
    }
    let store = Store::new(root.clone(), state)?;
    if let Action::List { json } = cli.command {
        let found = store.discover()?;
        if json {
            println!(
                "{}",
                json_text(
                    &serde_json::json!({"repo":root.to_string_lossy(),"projects":found.projects.iter().map(|p|p.record()).collect::<Vec<_>>(),"issues":found.issues})
                )
            );
        } else {
            for project in &found.projects {
                println!(
                    "{}",
                    terminal_text(&format!(
                        "{}\t{}\t{}",
                        project.id, project.status, project.title
                    ))
                );
            }
            for issue in &found.issues {
                eprintln!("{}", terminal_text(&format!("Issue: {issue}")));
            }
        }
        return Ok(u8::from(!found.issues.is_empty()));
    }
    let project = match &cli.command {
        Action::Show { project, .. } => store.get(project)?,
        Action::New {
            slug,
            title,
            private,
            root,
            ..
        } => store.create(slug, title, *private, root.as_deref())?,
        Action::Status {
            project,
            status,
            expect,
            ..
        } => store.set_status(project, status, expect.as_deref())?,
        _ => unreachable!("other commands returned above"),
    };
    if cli.json() {
        let mut value = project.record();
        if matches!(cli.command, Action::Show { .. }) {
            value["document"] = project.source.into();
        }
        println!("{}", json_text(&value));
    } else if matches!(cli.command, Action::Show { .. }) {
        print!("{}", terminal_text(&project.source));
    } else {
        println!("{}", terminal_text(&project.path.to_string_lossy()));
    }
    Ok(0)
}
