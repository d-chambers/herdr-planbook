//! Explicit repository context and bounded discovery roots.
use crate::process::capture_input;
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};

pub type Environment = BTreeMap<String, String>;
pub const DEFAULT_ROOTS: [&str; 4] = [
    ".projects",
    "projects",
    ".scratch/projects",
    "scratch/projects",
];

pub fn environment() -> Environment {
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
}

pub fn git(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    git_input(root, args, None)
}

pub fn git_input(root: &Path, args: &[&str], input: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut command = Command::new("git");
    command.args(["--no-optional-locks", "-c", "core.fsmonitor=false"]);
    // check-ignore accepts literal paths already and rejects this pathspec option.
    if args.first() == Some(&"ls-files") {
        command.arg("--literal-pathspecs");
    }
    let output = capture_input(
        command.arg("-C").arg(root).args(args),
        Duration::from_secs(5),
        input,
    )
    .ok()?;
    output.status.success().then_some(output.stdout)
}

pub fn invocation_context(env: &Environment) -> Result<serde_json::Value> {
    let context: serde_json::Value = serde_json::from_str(
        env.get("HERDR_PLUGIN_CONTEXT_JSON")
            .map_or("{}", String::as_str),
    )
    .context("Invalid Herdr invocation context")?;
    ensure!(context.is_object(), "Invalid Herdr invocation context");
    Ok(context)
}

fn git_path(bytes: &[u8]) -> Result<PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        Ok(PathBuf::from(std::str::from_utf8(bytes)?))
    }
}

pub fn repository(explicit: Option<&Path>, env: &Environment) -> Result<PathBuf> {
    let mut given = explicit
        .map(Path::to_owned)
        .or_else(|| env.get("PLANBOOK_REPO").map(PathBuf::from));
    if given.is_none() && env.contains_key("HERDR_PLUGIN_ID") {
        let context = invocation_context(env)?;
        given = context["focused_pane_cwd"]
            .as_str()
            .or_else(|| context["workspace_cwd"].as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        ensure!(
            given.is_some(),
            "Herdr supplied no repository context; pass --repo PATH"
        );
    }
    let given = match given {
        Some(path) => path,
        None => std::env::current_dir()?,
    };
    let given = if given == Path::new("~") || given.starts_with("~/") {
        PathBuf::from(env.get("HOME").context("HOME is unavailable")?)
            .join(given.strip_prefix("~")?)
    } else {
        given
    };
    let path = given
        .canonicalize()
        .context("Repository directory does not exist")?;
    ensure!(path.is_dir(), "Repository must be a directory");
    let Some(top) = git(&path, &["rev-parse", "--show-toplevel"]) else {
        return Ok(path);
    };
    let top = git_path(top.strip_suffix(b"\n").unwrap_or(&top))?;
    let trees = git(&path, &["worktree", "list", "--porcelain", "-z"])
        .context("Cannot list Git worktrees; Git 2.36+ is required")?;
    let first = trees
        .split(|b| *b == 0)
        .take_while(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if first.contains(&b"bare".as_slice()) {
        return Ok(top.canonicalize()?);
    }
    if let Some(main) = first
        .first()
        .and_then(|line| line.strip_prefix(b"worktree "))
    {
        let main = git_path(main)?;
        ensure!(
            main.is_dir(),
            "Main checkout is unavailable: {}",
            main.display()
        );
        return Ok(main.canonicalize()?);
    }
    bail!("Git returned no main checkout")
}

pub fn within(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        !relative.as_os_str().is_empty() && !relative.is_absolute(),
        "Project paths must be relative and stay inside the repository"
    );
    let mut path = root.to_owned();
    let mut components = 0;
    for component in relative.components() {
        match component {
            Component::Normal(name) => {
                path.push(name);
                components += 1;
            }
            Component::CurDir => (),
            _ => bail!("Project paths must stay inside the repository"),
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "Symbolic links are not project locations: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(
        components > 0,
        "The repository itself cannot be a project root"
    );
    Ok(path)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    roots: Vec<String>,
}

pub fn roots(root: &Path) -> Result<Vec<String>> {
    let config = root.join(".planbook.toml");
    let names = match fs::symlink_metadata(&config) {
        Ok(meta) => {
            within(root, Path::new(".planbook.toml"))?;
            ensure!(meta.is_file(), ".planbook.toml must be a regular file");
            let mut bytes = Vec::new();
            fs::File::open(config)?
                .take(65537)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 65536, ".planbook.toml exceeds 64 KiB");
            toml::from_str::<Config>(std::str::from_utf8(&bytes)?)
                .context("Cannot read .planbook.toml")?
                .roots
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            DEFAULT_ROOTS.map(str::to_owned).to_vec()
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(!names.is_empty(), "Configure at least one project root");
    let mut checked = Vec::new();
    for name in names {
        let path = within(root, Path::new(&name))?;
        let normalized = path
            .strip_prefix(root)?
            .to_str()
            .context("Project roots must be UTF-8")?
            .to_owned();
        ensure!(
            !checked.contains(&normalized),
            "Duplicate project root: {normalized}"
        );
        checked.push(normalized);
    }
    Ok(checked)
}

pub fn state_dir(env: &Environment) -> Result<PathBuf> {
    let path = if let Some(state) = env.get("XDG_STATE_HOME") {
        PathBuf::from(state)
    } else {
        PathBuf::from(env.get("HOME").context("Set HOME or XDG_STATE_HOME")?).join(".local/state")
    };
    ensure!(path.is_absolute(), "XDG_STATE_HOME must be absolute");
    Ok(path.join("herdr-planbook/locks"))
}
