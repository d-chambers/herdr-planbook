use super::{Fixture, commit, git};
use herdr_planbook::paths::{self, Environment};
use std::os::unix::fs::symlink;
use std::{fs, path::Path};

#[test]
fn worktrees_and_subfolders_use_main_checkout() {
    let f = Fixture::new();
    commit(&f.root);
    let tree = f._temp.path().join("checkout");
    git(
        &f.root,
        &["worktree", "add", "-b", "task", tree.to_str().unwrap()],
    );
    let nested = tree.join("nested");
    fs::create_dir(&nested).unwrap();
    assert_eq!(
        paths::repository(Some(&nested), &Environment::new()).unwrap(),
        f.root
    );
}
#[test]
fn bare_repository_is_never_a_project_location() {
    let f = Fixture::new();
    commit(&f.root);
    let bare = f._temp.path().join("bare.git");
    let tree = f._temp.path().join("checkout");
    git(
        &f.root,
        &[
            "clone",
            "--bare",
            f.root.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    git(&bare, &["worktree", "add", tree.to_str().unwrap(), "main"]);
    assert_eq!(
        paths::repository(Some(&tree), &Environment::new()).unwrap(),
        tree.canonicalize().unwrap()
    );
}
#[test]
fn plugin_context_is_explicit_and_cannot_fall_back_to_plugin_cwd() {
    let f = Fixture::new();
    let mut env: Environment = [("HERDR_PLUGIN_ID".into(), "dchambers.planbook".into())].into();
    assert!(paths::repository(None, &env).is_err());
    env.insert(
        "HERDR_PLUGIN_CONTEXT_JSON".into(),
        serde_json::json!({"workspace_cwd":f.root}).to_string(),
    );
    assert_eq!(paths::repository(None, &env).unwrap(), f.root);
    env.insert("HERDR_PLUGIN_CONTEXT_JSON".into(), "invalid".into());
    assert_eq!(paths::repository(Some(&f.root), &env).unwrap(), f.root);
}
#[test]
fn configuration_cannot_widen_discovery() {
    let f = Fixture::new();
    for config in [
        "roots = [\"../outside\"]",
        "roots = [\"/tmp\"]",
        "roots = [\".\"]",
        "roots = []",
        "roots = [\".projects\", \".projects\"]",
        "roots = [\".projects\"]\ncommand=\"run\"",
        "roots = [",
    ] {
        fs::write(f.root.join(".planbook.toml"), config).unwrap();
        assert!(paths::roots(&f.root).is_err(), "accepted {config}");
    }
    fs::write(f.root.join(".planbook.toml"), "roots = [\"project\"]").unwrap();
    assert_eq!(paths::roots(&f.root).unwrap(), ["project"]);
    assert!(paths::within(&f.root, Path::new("../outside")).is_err());
}
#[test]
fn root_and_config_symlinks_are_rejected() {
    let f = Fixture::new();
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, f.root.join(".projects")).unwrap();
    assert!(paths::roots(&f.root).is_err());
    fs::remove_file(f.root.join(".projects")).unwrap();
    let config = outside.join("config");
    fs::write(&config, "roots = [\".projects\"]").unwrap();
    symlink(&config, f.root.join(".planbook.toml")).unwrap();
    assert!(paths::roots(&f.root).is_err());
}
#[test]
fn a_fifo_cannot_block_config_loading() {
    let f = Fixture::new();
    let output = std::process::Command::new("mkfifo")
        .arg(f.root.join(".planbook.toml"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        paths::roots(&f.root)
            .unwrap_err()
            .to_string()
            .contains("regular file")
    );
}
