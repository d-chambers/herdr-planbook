use super::Fixture;
use herdr_planbook::{cli::pane_command, paths::Environment};
use serde_json::Value;
use std::fs;

#[test]
fn create_show_status_list_roundtrip() {
    let f = Fixture::new();
    let out = f.cli(&["new", "first", "--title", "Ship 🚀 *args", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let first: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(first["title"], "Ship 🚀 *args");
    let shown: Value = serde_json::from_slice(&f.cli(&["show", "first", "--json"]).stdout).unwrap();
    assert!(shown["document"].as_str().unwrap().contains("## Goal"));
    let out = f.cli(&[
        "status",
        "first",
        "active",
        "--expect",
        shown["revision"].as_str().unwrap(),
        "--json",
    ]);
    assert!(out.status.success());
    let listed: Value = serde_json::from_slice(&f.cli(&["list", "--json"]).stdout).unwrap();
    assert_eq!(listed["projects"][0]["status"], "active");
}
#[test]
fn diagnostics_and_failures_are_json_with_nonzero_status() {
    let f = Fixture::new();
    let out = f.cli(&["show", "missing", "--json"]);
    assert!(!out.status.success());
    assert!(serde_json::from_slice::<Value>(&out.stdout).unwrap()["error"].is_string());
    f.put(".projects", "broken", "bad metadata");
    let out = f.cli(&["list", "--json"]);
    assert!(!out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["issues"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn raw_terminal_controls_do_not_escape_human_or_json_output() {
    let f = Fixture::new();
    let p = f.store().create("one", "Example", false, None).unwrap();
    let source = format!("{}\n\x1b]52;c;example\x1b\\\u{9b}31m\n", p.source);
    fs::write(p.path, &source).unwrap();
    for args in [
        ["show", "one", "--json"].as_slice(),
        ["show", "one"].as_slice(),
    ] {
        let out = f.cli(args);
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(!text.contains('\x1b'));
        assert!(!text.contains('\u{9b}'));
        if args.contains(&"--json") {
            assert_eq!(
                serde_json::from_str::<Value>(&text).unwrap()["document"],
                source
            );
        }
    }
}
#[test]
fn pane_target_comes_from_invocation_context() {
    let f = Fixture::new();
    let mut env: Environment = [
        ("HERDR_ENV".into(), "1".into()),
        ("HERDR_WORKSPACE_ID".into(), "w7".into()),
        ("HERDR_PANE_ID".into(), "w1:p1".into()),
        ("HERDR_PLUGIN_ID".into(), "dchambers.planbook".into()),
        (
            "HERDR_PLUGIN_CONTEXT_JSON".into(),
            serde_json::json!({"focused_pane_id":"w7:p9"}).to_string(),
        ),
    ]
    .into();
    let command = pane_command(&f.root, &env).unwrap();
    let args = command
        .get_args()
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        args[args.iter().position(|s| s == "--target-pane").unwrap() + 1],
        "w7:p9"
    );
    assert!(!args.contains(&"--workspace".into()));
    assert!(args.contains(&format!("PLANBOOK_REPO={}", f.root.display())));
    env.remove("HERDR_PLUGIN_CONTEXT_JSON");
    assert!(pane_command(&f.root, &env).is_err());
    env.remove("HERDR_PLUGIN_ID");
    assert!(pane_command(&f.root, &env).is_ok());
}
#[test]
fn resources_do_not_require_repository_context() {
    for (command, needle) in [
        ("skill", "Repository project plans"),
        ("guide", "Project format"),
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_planbook"))
            .args(["--repo", "/path-that-does-not-exist", command])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8(out.stdout).unwrap().contains(needle));
    }
}
