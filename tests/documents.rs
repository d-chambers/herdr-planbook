use herdr_planbook::documents::{self, parse};
use std::path::PathBuf;

fn document(header: &str, body: &str) -> anyhow::Result<documents::Project> {
    parse(
        format!("---\n{header}\n---\n{body}"),
        PathBuf::from("project.md"),
        "example".into(),
    )
}

#[test]
fn rejects_ambiguous_or_executable_metadata() {
    for header in [
        "title: Example\nstatus: active\nstatus: done",
        "title: Example\nstatus: finished",
        "title: Example",
        "title: Example\nstatus: active\ncommand: echo hi",
        "title: !!python/object/apply:os.system ['echo wrong']\nstatus: active",
        "title: &name Example\nstatus: *name",
        "title: [one, two]\nstatus: active",
        "title: 123\nstatus: active",
        "title: true\nstatus: active",
        "title: \"\\uD83D\\uDE80\"\nstatus: active",
        "title: \"\\x9bInjected\"\nstatus: active",
    ] {
        assert!(document(header, "").is_err(), "accepted {header:?}");
    }
    assert!(
        document(
            &format!(
                "title: {}text{}\nstatus: active",
                "[".repeat(1100),
                "]".repeat(1100)
            ),
            ""
        )
        .is_err()
    );
}
#[test]
fn unicode_and_quoted_punctuation_remain_text() {
    let p = document("title: \"Ship 🚀 *args &kwargs\"\nstatus: active", "").unwrap();
    assert_eq!(p.title, "Ship 🚀 *args &kwargs");
}
#[test]
fn status_preserves_comments_crlf_and_prose() {
    let original = super::source().replace('\n', "\r\n");
    let p = parse(
        original.clone(),
        PathBuf::from("project.md"),
        "example".into(),
    )
    .unwrap();
    assert_eq!(
        documents::with_status(&p, "active").unwrap().source,
        original.replace("status: proposed", "status: active")
    );
}
#[test]
fn examples_are_not_work_items_or_next_actions() {
    let body = "- [x] First\n- [ ] Second\n```md\n- [ ] Example only\n## Next action\nWrong\n```\n## Next action\n\nRead the\ncode.\n\nMore explanation.\n";
    let p = document("title: Example\nstatus: proposed", body).unwrap();
    assert_eq!((p.progress().done, p.progress().total), (1, 2));
    assert_eq!(p.next_action(), "Read the code.");
    assert_eq!(
        document("title: Example\nstatus: proposed", "- [ ] Fallback\n")
            .unwrap()
            .next_action(),
        "Fallback"
    );
}
#[test]
fn terminal_controls_are_visible_and_json_retains_source() {
    let raw = "OSC \x1b]52;c;value\x1b\\\n\u{9b}31m";
    assert!(!documents::terminal_text(raw).contains('\x1b'));
    let json = documents::json_text(&serde_json::json!({"document":raw}));
    assert!(!json.contains('\u{9b}'));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["document"],
        raw
    );
}

#[test]
fn a_status_lookalike_in_a_multiline_title_is_not_modified() {
    assert!(
        document(
            "title: \"Summary\nstatus: proposed\ncontinues\"\nstatus: proposed",
            ""
        )
        .is_err()
    );
    let p = document(
        "title: \"Summary\n status: proposed\n continues\"\nstatus: proposed",
        "",
    )
    .unwrap();
    let updated = documents::with_status(&p, "active").unwrap();
    assert_eq!(updated.title, p.title);
    assert_eq!(updated.status, "active");
}

#[test]
fn status_quotes_and_trailing_whitespace_are_preserved() {
    for line in [
        "status: \"proposed\"   # explanation",
        "status: 'proposed'  ",
    ] {
        let p = document(&format!("title: Example\n{line}"), "Keep this prose.").unwrap();
        assert_eq!(
            documents::with_status(&p, "active").unwrap().source,
            p.source.replace("proposed", "active")
        );
    }
}
