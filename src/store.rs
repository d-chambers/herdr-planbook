//! Filesystem operations; Markdown is the only status database.
use crate::{
    TEMPLATE,
    documents::{self, Project},
    paths::{git, git_input, roots, within},
};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::LazyLock,
};

#[derive(Default)]
pub struct Discovery {
    pub projects: Vec<Project>,
    pub issues: Vec<String>,
}

pub struct Store {
    pub root: PathBuf,
    pub roots: Vec<String>,
    state: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf, state: PathBuf) -> Result<Self> {
        let root = root.canonicalize()?;
        let roots = roots(&root)?;
        ensure!(state.is_absolute(), "Lock directory must be absolute");
        Ok(Self { root, roots, state })
    }
    fn lock(&self, entry: &Path) -> Result<File> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.state)?;
        let name = format!(
            "{:x}.lock",
            Sha256::digest(entry.as_os_str().as_encoded_bytes())
        );
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.state.join(name))?;
        file.lock_exclusive()?;
        Ok(file)
    }
    fn visibility(&self, mut project: Project) -> Result<Project> {
        self.mark_visibility(std::slice::from_mut(&mut project))?;
        Ok(project)
    }
    fn mark_visibility(&self, projects: &mut [Project]) -> Result<()> {
        if projects.is_empty() {
            return Ok(());
        }
        let paths = projects
            .iter()
            .map(|p| {
                p.path
                    .strip_prefix(&self.root)
                    .context("Project escaped repository")?
                    .to_str()
                    .map(str::to_owned)
                    .context("Project paths must be UTF-8")
            })
            .collect::<Result<Vec<_>>>()?;
        let mut tracked = HashSet::new();
        // Bound argv size for macOS too, while loading the index once for normal collections.
        let mut start = 0;
        while start < paths.len() {
            let mut end = start;
            let mut bytes = 0;
            while end < paths.len() && (end == start || bytes + paths[end].len() < 48 * 1024) {
                bytes += paths[end].len() + 1;
                end += 1;
            }
            let mut args = vec!["ls-files", "-z", "--"];
            args.extend(paths[start..end].iter().map(String::as_str));
            if let Some(output) = git(&self.root, &args) {
                tracked.extend(
                    output
                        .split(|b| *b == 0)
                        .filter(|p| !p.is_empty())
                        .map(<[u8]>::to_vec),
                );
            }
            start = end;
        }
        let mut input = Vec::new();
        for path in &paths {
            if !tracked.contains(path.as_bytes()) {
                input.extend_from_slice(path.as_bytes());
                input.push(0);
            }
        }
        let ignored: HashSet<Vec<u8>> = if input.is_empty() {
            HashSet::new()
        } else {
            git_input(&self.root, &["check-ignore", "--stdin", "-z"], Some(&input))
                .unwrap_or_default()
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(<[u8]>::to_vec)
                .collect()
        };
        for (project, path) in projects.iter_mut().zip(paths) {
            project.visibility = if tracked.contains(path.as_bytes()) {
                "tracked"
            } else if ignored.contains(path.as_bytes()) {
                "ignored"
            } else {
                "untracked"
            }
            .into();
        }
        Ok(())
    }
    pub fn discover(&self) -> Result<Discovery> {
        let mut result = Discovery::default();
        let mut seen = HashSet::new();
        for name in &self.roots {
            let base = within(&self.root, Path::new(name))?;
            if !base.exists() {
                continue;
            }
            if !base.is_dir() {
                result
                    .issues
                    .push(format!("{name}: project root is not a directory"));
                continue;
            }
            let children = match fs::read_dir(&base) {
                Ok(children) => children,
                Err(error) => {
                    result.issues.push(format!("{name}: {error}"));
                    continue;
                }
            };
            let mut children = children.collect::<std::io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                if child.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let path = child.path();
                let relative = path.strip_prefix(&self.root)?;
                let read_project = || -> Result<Option<Project>> {
                    within(&self.root, relative)?;
                    if !path.is_dir() {
                        return Ok(None);
                    }
                    let entry = path.join("project.md");
                    match fs::symlink_metadata(&entry) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(None);
                        }
                        Err(error) => return Err(error.into()),
                        Ok(_) => (),
                    }
                    within(&self.root, entry.strip_prefix(&self.root)?)?;
                    let id = relative
                        .to_str()
                        .context("Project ids must be UTF-8")?
                        .to_owned();
                    Ok(Some(documents::read(entry, id)?))
                };
                match read_project() {
                    Ok(Some(project)) if seen.insert(project.path.clone()) => {
                        result.projects.push(project)
                    }
                    Ok(_) => (),
                    Err(error) => result
                        .issues
                        .push(format!("{}: {error:#}", relative.display())),
                }
            }
        }
        self.mark_visibility(&mut result.projects)?;
        let mut counts = BTreeMap::new();
        for project in &result.projects {
            *counts
                .entry(
                    project
                        .path
                        .parent()
                        .and_then(Path::file_name)
                        .unwrap_or_default()
                        .to_owned(),
                )
                .or_insert(0) += 1;
        }
        for (name, count) in counts {
            if count > 1 {
                result.issues.push(format!(
                    "Duplicate project name {:?}; use its full id",
                    name
                ));
            }
        }
        result.projects.sort_by(|a, b| {
            (matches!(a.status.as_str(), "done" | "superseded"), &a.title)
                .cmp(&(matches!(b.status.as_str(), "done" | "superseded"), &b.title))
        });
        Ok(result)
    }
    pub fn get(&self, selector: &str) -> Result<Project> {
        let projects = self.discover()?.projects;
        if let Some(project) = projects.iter().find(|p| p.id == selector) {
            return Ok(project.clone());
        }
        let mut matches = projects.into_iter().filter(|p| {
            p.path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == selector)
        });
        let project = matches
            .next()
            .context("Unknown or invalid project; run list to see ids and issues")?;
        ensure!(
            matches.next().is_none(),
            "Ambiguous project; use its full id"
        );
        Ok(project)
    }
    pub fn create(
        &self,
        slug: &str,
        title: &str,
        private: bool,
        location: Option<&str>,
    ) -> Result<Project> {
        static SLUG: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").expect("valid slug regex"));
        ensure!(
            slug.len() <= 80 && SLUG.is_match(slug),
            "Use a lowercase directory slug with letters, digits and dashes (max 80)"
        );
        let location = location.unwrap_or_else(|| {
            if private {
                self.roots
                    .iter()
                    .find(|r| {
                        Path::new(r)
                            .components()
                            .any(|c| c.as_os_str() == "scratch" || c.as_os_str() == ".scratch")
                    })
                    .unwrap_or(&self.roots[0])
            } else {
                &self.roots[0]
            }
        });
        ensure!(
            self.roots.iter().any(|r| r == location),
            "Choose a configured project root"
        );
        let relative = Path::new(location).join(slug);
        let directory = within(&self.root, &relative)?;
        let entry = directory.join("project.md");
        let source = TEMPLATE.replace("__TITLE__", &serde_json::to_string(title)?);
        let project = documents::parse(source, entry.clone(), relative.to_string_lossy().into())?;
        let _lock = self.lock(&entry)?;
        within(&self.root, &relative)?;
        fs::create_dir_all(directory.parent().context("Missing project parent")?)?;
        fs::DirBuilder::new()
            .mode(if private { 0o700 } else { 0o755 })
            .create(&directory)
            .context("Project directory already exists or cannot be created")?;
        if private {
            fs::write(directory.join(".gitignore"), "*\n")?;
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(entry)?
            .write_all(project.source.as_bytes())?;
        self.visibility(project)
    }
    pub fn set_status(
        &self,
        selector: &str,
        status: &str,
        expected: Option<&str>,
    ) -> Result<Project> {
        let project = self.get(selector)?;
        let _lock = self.lock(&project.path)?;
        let relative = project.path.strip_prefix(&self.root)?;
        within(&self.root, relative)?;
        let current = documents::read(project.path.clone(), project.id)?;
        if let Some(expected) = expected {
            ensure!(
                current.revision() == expected,
                "This project changed; refresh and try again"
            );
        }
        let updated = documents::with_status(&current, status)?;
        let mut temporary = tempfile::NamedTempFile::new_in(
            project.path.parent().context("Missing project directory")?,
        )?;
        temporary.write_all(updated.source.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .as_file()
            .set_permissions(fs::metadata(&project.path)?.permissions())?;
        within(&self.root, relative)?;
        // Catch ordinary editor saves during preparation too; the lock only coordinates Planbook.
        if documents::read(project.path.clone(), current.id.clone())?.source != current.source {
            bail!("This project changed; refresh and try again");
        }
        temporary.persist(&project.path)?;
        self.visibility(updated)
    }
}
