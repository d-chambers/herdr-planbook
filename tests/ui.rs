use super::Fixture;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use herdr_planbook::ui::{App, Dialog, Intent};
use ratatui::{Terminal, backend::TestBackend};
use std::fs;

fn key(app: &mut App, code: KeyCode) -> Intent {
    app.key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn render(app: &mut App, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 36)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}
#[test]
fn create_preview_filter_and_explicit_status() {
    let f = Fixture::new();
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    key(&mut app, KeyCode::Char('n'));
    for c in "Useful 🚀 project".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.selected().unwrap().title, "Useful 🚀 project");
    assert!(render(&mut app, 110).contains("Goal"));
    key(&mut app, KeyCode::Char('s'));
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    assert_eq!(f.store().get("useful-project").unwrap().status, "active");
    key(&mut app, KeyCode::Char('/'));
    for c in "no-match".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    assert!(app.selected().is_none());
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Esc);
    assert!(app.selected().is_some());
    assert!(matches!(key(&mut app, KeyCode::Char('q')), Intent::Quit));
}
#[test]
fn status_dialog_refuses_external_changes() {
    let f = Fixture::new();
    let p = f.store().create("one", "One", false, None).unwrap();
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    key(&mut app, KeyCode::Char('s'));
    fs::write(&p.path, format!("{}\nHuman addition.\n", p.source)).unwrap();
    app.refresh();
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    assert!(app.notice.contains("changed"));
    let p = f.store().get("one").unwrap();
    assert_eq!(p.status, "proposed");
    assert!(p.source.contains("Human addition."));
}
#[test]
fn invalid_config_is_visible_and_recovers_without_restart() {
    let f = Fixture::new();
    let config = f.root.join(".planbook.toml");
    fs::write(&config, "roots = [").unwrap();
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    assert!(render(&mut app, 110).contains("Cannot read"));
    fs::write(config, "roots = [\".projects\"]").unwrap();
    f.store().create("one", "Working", false, None).unwrap();
    app.refresh();
    assert_eq!(app.selected().unwrap().title, "Working");
}
#[test]
fn narrow_preview_escapes_controls_and_refresh_preserves_selection() {
    let f = Fixture::new();
    let p = f.store().create("one", "First", false, None).unwrap();
    f.store().create("two", "Second", false, None).unwrap();
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    key(&mut app, KeyCode::Down);
    app.refresh();
    assert_eq!(app.selected().unwrap().title, "Second");
    key(&mut app, KeyCode::Up);
    fs::write(
        &p.path,
        p.source.replace(
            "Refine the goal and first deliverable.",
            "\x1b]52;c;data\x1b\\",
        ),
    )
    .unwrap();
    app.refresh();
    let text = render(&mut app, 70);
    assert!(!text.contains('\x1b'));
    assert!(text.contains("\\x1b"), "{text}");
    assert!(text.contains("First"));
}
#[test]
fn private_toggle_and_grapheme_backspace() {
    let f = Fixture::new();
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    key(&mut app, KeyCode::Char('n'));
    for c in "One 👩‍💻".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Backspace);
    if let Some(Dialog::New { title, .. }) = &app.dialog {
        assert_eq!(title, "One ");
    } else {
        panic!("Expected new dialog");
    }
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char(' '));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.selected().unwrap().visibility, "ignored");
}

#[test]
fn scrolling_reaches_the_end_of_wrapped_paragraphs() {
    let f = Fixture::new();
    let body = format!(
        "---\ntitle: Long plan\nstatus: active\n---\n\n{}\nFINAL-MARKER",
        "lengthyword ".repeat(400)
    );
    f.put(".projects", "long", &body);
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    app.scroll = u16::MAX;
    assert!(render(&mut app, 70).contains("FINAL-MARKER"));
}

#[test]
fn paste_is_text_in_fields_and_never_navigation_commands() {
    let f = Fixture::new();
    f.store().create("one", "One", false, None).unwrap();
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    app.paste("s\nj\n\nq");
    assert!(app.dialog.is_none());
    assert_eq!(f.store().get("one").unwrap().status, "proposed");
    key(&mut app, KeyCode::Char('n'));
    app.paste("A plan\nq");
    assert!(matches!(app.dialog, Some(Dialog::New { .. })));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.selected().unwrap().title, "A plan q");
    key(&mut app, KeyCode::Char('/'));
    app.paste("One");
    assert_eq!(app.selected().unwrap().title, "One");
    key(&mut app, KeyCode::Esc);
    assert!(app.filter.is_empty());
}

#[test]
fn modified_keys_and_table_borders_do_not_trigger_actions() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let f = Fixture::new();
    for number in 0..30 {
        f.store()
            .create(&format!("p{number:02}"), "Plan", false, None)
            .unwrap();
    }
    let mut app = App::new(f.root.clone(), f.state.clone(), f.env());
    for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        app.key(KeyEvent::new(KeyCode::Char('s'), modifiers));
        assert!(app.dialog.is_none());
    }
    let mut terminal = Terminal::new(TestBackend::new(70, 36)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let border = (0..36)
        .find(|row| terminal.backend().buffer()[(0, *row)].symbol() == "└")
        .unwrap();
    let before = app.selected().unwrap().id.clone();
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: border,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.selected().unwrap().id, before);
}
