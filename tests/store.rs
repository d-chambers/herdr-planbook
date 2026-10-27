use super::{Fixture, git, source};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    sync::{Arc, Barrier},
};

#[test]
fn discovery_is_bounded_and_bad_documents_are_visible() {
    let f = Fixture::new();
    for (i, root) in [
        ".projects",
        "projects",
        ".scratch/projects",
        "scratch/projects",
    ]
    .iter()
    .enumerate()
    {
        f.put(root, &format!("project-{i}"), source());
    }
    f.put("unrelated", "hidden", source());
    f.put(".projects/parent/nested", "hidden", source());
    f.put(".projects", "broken", "bad metadata");
    let found = f.store().discover().unwrap();
    assert_eq!(found.projects.len(), 4);
    assert_eq!(found.issues.len(), 1);
    assert!(found.issues[0].contains("broken"));
    assert!(
        !f.state.exists(),
        "Read-only discovery must not create state"
    );
}
#[test]
fn duplicate_names_require_full_ids() {
    let f = Fixture::new();
    f.put(".projects", "same", source());
    f.put("projects", "same", source());
    assert!(f.store().get("same").is_err());
    assert_eq!(
        f.store().get(".projects/same").unwrap().id,
        ".projects/same"
    );
}
#[test]
fn symlinked_files_and_children_cannot_expose_or_write_outside() {
    let f = Fixture::new();
    let outside = f._temp.path().join("private.txt");
    fs::write(&outside, source()).unwrap();
    let path = f.put(".projects", "escape", source());
    fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(f.store().discover().unwrap().projects.is_empty());
    assert!(f.store().set_status("escape", "done", None).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), source());
    symlink(f._temp.path(), f.root.join(".projects/linked")).unwrap();
    assert_eq!(f.store().discover().unwrap().issues.len(), 2);
}
#[test]
fn private_creation_is_ignored_without_affecting_neighbors() {
    let f = Fixture::new();
    let neighbor = f.put(".scratch/projects", "public", source());
    let p = f.store().create("private", "Private", true, None).unwrap();
    assert_eq!(p.visibility, "ignored");
    assert_eq!(p.id, ".scratch/projects/private");
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&f.root)
        .arg("check-ignore")
        .arg(neighbor)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!f.root.join(".gitignore").exists());
    assert_eq!(
        fs::metadata(p.path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}
#[test]
fn creation_never_replaces_existing_content_or_accepts_paths() {
    let f = Fixture::new();
    let entry = f.put(".projects", "existing", source());
    assert!(
        f.store()
            .create("existing", "Different", false, None)
            .is_err()
    );
    assert_eq!(fs::read_to_string(entry).unwrap(), source());
    for slug in ["../outside", "/tmp/test", ".hidden", "UPPER", "a/b", ""] {
        assert!(f.store().create(slug, "Example", false, None).is_err());
    }
    assert!(f.store().create("valid", "", false, None).is_err());
    assert!(!f.root.join(".projects/valid").exists());
}
#[test]
fn custom_roots_and_tracking_are_observed() {
    let f = Fixture::new();
    fs::write(f.root.join(".planbook.toml"), "roots = [\"project\"]").unwrap();
    let p = f.store().create("example", "Example", true, None).unwrap();
    assert_eq!(p.id, "project/example");
    assert_eq!(p.visibility, "ignored");
    let p = f
        .store()
        .create("shared", "Shared", false, Some("project"))
        .unwrap();
    git(&f.root, &["add", p.path.to_str().unwrap()]);
    assert_eq!(f.store().get("shared").unwrap().visibility, "tracked");
}
#[test]
fn status_changes_preserve_prose_permissions_and_reject_stale_edits() {
    let f = Fixture::new();
    let entry = f.put(".projects", "one", source());
    fs::set_permissions(&entry, fs::Permissions::from_mode(0o640)).unwrap();
    let before = f.store().get("one").unwrap();
    let after = f
        .store()
        .set_status("one", "active", Some(&before.revision()))
        .unwrap();
    assert_eq!(
        after.source,
        source().replace("status: proposed", "status: active")
    );
    assert_eq!(
        fs::metadata(&entry).unwrap().permissions().mode() & 0o777,
        0o640
    );
    fs::write(&entry, format!("{}\nHuman addition.\n", after.source)).unwrap();
    assert!(
        f.store()
            .set_status("one", "done", Some(&after.revision()))
            .is_err()
    );
    assert!(
        fs::read_to_string(&entry)
            .unwrap()
            .contains("Human addition.")
    );
}
#[test]
fn concurrent_writers_cannot_both_commit_the_same_revision() {
    let f = Fixture::new();
    let p = f.store().create("one", "One", false, None).unwrap();
    let revision = p.revision();
    let barrier = Arc::new(Barrier::new(2));
    let threads = ["active", "parked"].map(|status| {
        let root = f.root.clone();
        let state = f.state.clone();
        let revision = revision.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let store = herdr_planbook::store::Store::new(root, state).unwrap();
            barrier.wait();
            store.set_status("one", status, Some(&revision)).is_ok()
        })
    });
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| usize::from(thread.join().unwrap()))
            .sum::<usize>(),
        1
    );
}
#[test]
fn bounded_reads_reject_large_non_utf8_and_nonregular_documents() {
    let f = Fixture::new();
    let path = f.put(".projects", "one", source());
    for bytes in [vec![b'a'; 256 * 1024 + 1], vec![0xff, 0xfe]] {
        fs::write(&path, bytes).unwrap();
        assert!(f.store().discover().unwrap().projects.is_empty());
    }
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(f.store().discover().unwrap().projects.is_empty());
}
#[test]
fn discovery_cannot_run_a_repository_fsmonitor_hook() {
    let f = Fixture::new();
    let path = f.put(".projects", "one", source());
    git(&f.root, &["add", path.to_str().unwrap()]);
    let marker = f._temp.path().join("hook-was-called");
    let hook = f._temp.path().join("fsmonitor");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf called >> {}\nprintf '\\0'\n",
            shell_words::quote(marker.to_str().unwrap())
        ),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    git(
        &f.root,
        &["config", "core.fsmonitor", hook.to_str().unwrap()],
    );
    git(&f.root, &["ls-files"]);
    assert!(
        marker.exists(),
        "Fixture must first prove that Git runs the hook"
    );
    fs::remove_file(&marker).unwrap();
    assert_eq!(
        f.store().discover().unwrap().projects[0].visibility,
        "tracked"
    );
    assert!(!marker.exists());
}

#[test]
fn hand_created_names_are_literal_git_paths() {
    let f = Fixture::new();
    let tracked = f.put(".projects", "a1", source());
    git(&f.root, &["add", tracked.to_str().unwrap()]);
    f.put(".projects", "a?", source());
    f.put(".projects", "a*", source());
    let ignored = f.put(".projects", "ignored", source());
    fs::write(ignored.parent().unwrap().join(".gitignore"), "*\n").unwrap();
    let projects = f.store().discover().unwrap().projects;
    for (id, visibility) in [
        (".projects/a1", "tracked"),
        (".projects/a?", "untracked"),
        (".projects/a*", "untracked"),
        (".projects/ignored", "ignored"),
    ] {
        assert_eq!(
            projects.iter().find(|p| p.id == id).unwrap().visibility,
            visibility
        );
    }
}
