mod cli;
mod documents;
mod paths;
mod process;
mod store;
mod ui;

use herdr_planbook::{paths::Environment, store::Store};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let state = temp.path().join("state");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "-b", "main"]);
        git(&root, &["config", "core.excludesFile", "/dev/null"]);
        Self {
            _temp: temp,
            root,
            state,
        }
    }
    fn store(&self) -> Store {
        Store::new(self.root.clone(), self.state.clone()).unwrap()
    }
    fn env(&self) -> Environment {
        [(
            "XDG_STATE_HOME".into(),
            self.state.to_string_lossy().into_owned(),
        )]
        .into()
    }
    fn put(&self, root: &str, slug: &str, source: &str) -> PathBuf {
        let directory = self.root.join(root).join(slug);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("project.md");
        fs::write(&path, source).unwrap();
        path
    }
    fn cli(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_planbook"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("HERDR_") || key == "PLANBOOK_REPO" {
                command.env_remove(key);
            }
        }
        command
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("XDG_STATE_HOME", &self.state)
            .arg("--repo")
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
}
fn git(root: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn commit(root: &Path) {
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "Initial",
        ],
    );
}
fn source() -> &'static str {
    "---\ntitle: Existing\nstatus: proposed # keep reason\n---\n\n## Goal\n\nKeep my prose.\n\n## Next action\n\nRead the code.\n"
}
