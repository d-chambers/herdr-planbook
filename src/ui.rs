//! Immediate-mode terminal pane. No project commands, links, or agents execute here.
use crate::{
    documents::{Project, STATUSES, terminal_text},
    paths::{Environment, within},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState, Wrap},
};
use regex::Regex;
use std::{
    path::PathBuf,
    process::Command,
    sync::LazyLock,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub enum Dialog {
    New {
        title: String,
        slug: String,
        private: bool,
        field: usize,
    },
    Status {
        id: String,
        revision: String,
        choice: usize,
    },
}

pub enum Intent {
    Continue,
    Quit,
    Edit(PathBuf),
}

pub struct App {
    pub root: PathBuf,
    state: PathBuf,
    env: Environment,
    pub projects: Vec<Project>,
    pub visible: Vec<usize>,
    pub filter: String,
    pub filtering: bool,
    pub dialog: Option<Dialog>,
    pub notice: String,
    pub issues: Vec<String>,
    pub table: TableState,
    pub scroll: u16,
    list_area: Rect,
}

impl App {
    pub fn new(root: PathBuf, state: PathBuf, env: Environment) -> Self {
        let mut app = Self {
            root,
            state,
            env,
            projects: Vec::new(),
            visible: Vec::new(),
            filter: String::new(),
            filtering: false,
            dialog: None,
            notice: String::new(),
            issues: Vec::new(),
            table: TableState::default(),
            scroll: 0,
            list_area: Rect::default(),
        };
        app.refresh();
        app
    }
    fn store(&self) -> Result<Store> {
        Store::new(self.root.clone(), self.state.clone())
    }
    pub fn selected(&self) -> Option<&Project> {
        self.visible
            .get(self.table.selected()?)
            .and_then(|index| self.projects.get(*index))
    }
    pub fn refresh(&mut self) {
        let selected = self.selected().map(|p| p.id.clone());
        match self.store().and_then(|s| s.discover()) {
            Ok(found) => {
                self.projects = found.projects;
                self.issues = found.issues;
            }
            Err(error) => {
                self.projects.clear();
                self.issues = vec![format!("{error:#}")];
            }
        }
        self.refilter(selected.as_deref());
        if self.selected().map(|p| p.id.as_str()) != selected.as_deref() {
            self.scroll = 0;
        }
    }
    pub fn refilter(&mut self, selected: Option<&str>) {
        let filter = self.filter.to_lowercase();
        self.visible = self
            .projects
            .iter()
            .enumerate()
            .filter_map(|(index, p)| {
                format!("{} {} {} {}", p.id, p.title, p.status, p.next_action())
                    .to_lowercase()
                    .contains(&filter)
                    .then_some(index)
            })
            .collect();
        let row = selected
            .and_then(|id| {
                self.visible
                    .iter()
                    .position(|index| self.projects[*index].id == id)
            })
            .unwrap_or(0);
        self.table.select((!self.visible.is_empty()).then_some(row));
    }
    fn move_selection(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let current = self.table.selected().unwrap_or(0);
        let next = current
            .saturating_add_signed(delta)
            .min(self.visible.len() - 1);
        self.table.select(Some(next));
        self.scroll = 0;
    }
    pub fn key(&mut self, key: KeyEvent) -> Intent {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Intent::Quit;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Intent::Continue;
        }
        if self.dialog.is_some() {
            self.dialog_key(key.code);
            return Intent::Continue;
        }
        if self.filtering {
            match key.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.filtering = false;
                }
                KeyCode::Enter => self.filtering = false,
                KeyCode::Backspace => pop_grapheme(&mut self.filter),
                KeyCode::Char(c) => self.filter.push(c),
                _ => (),
            }
            self.refilter(None);
            self.scroll = 0;
            return Intent::Continue;
        }
        match key.code {
            KeyCode::Char('q') => return Intent::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Esc => {
                self.filter.clear();
                self.refilter(None);
                self.scroll = 0;
            }
            KeyCode::Char('r') => {
                self.notice.clear();
                self.refresh();
            }
            KeyCode::Char('n') => {
                self.dialog = Some(Dialog::New {
                    title: String::new(),
                    slug: String::new(),
                    private: false,
                    field: 0,
                })
            }
            KeyCode::Char('s') => {
                if let Some(p) = self.selected() {
                    self.dialog = Some(Dialog::Status {
                        id: p.id.clone(),
                        revision: p.revision(),
                        choice: STATUSES.iter().position(|s| *s == p.status).unwrap_or(0),
                    });
                }
            }
            KeyCode::Char('e') => {
                if let Some(p) = self.selected() {
                    return Intent::Edit(p.path.clone());
                }
            }
            _ => (),
        }
        Intent::Continue
    }
    fn dialog_key(&mut self, key: KeyCode) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        if key == KeyCode::Esc {
            return;
        }
        match &mut dialog {
            Dialog::New {
                title,
                slug,
                private,
                field,
            } => match key {
                KeyCode::Tab => *field = (*field + 1) % 3,
                KeyCode::BackTab => *field = (*field + 2) % 3,
                KeyCode::Char(' ') if *field == 2 => *private = !*private,
                KeyCode::Char(c) if *field == 0 => title.push(c),
                KeyCode::Char(c) if *field == 1 => slug.push(c),
                KeyCode::Backspace if *field == 0 => pop_grapheme(title),
                KeyCode::Backspace if *field == 1 => pop_grapheme(slug),
                KeyCode::Enter => {
                    static SLUG: LazyLock<Regex> =
                        LazyLock::new(|| Regex::new("[^a-z0-9]+").expect("valid slug regex"));
                    let name = if slug.is_empty() {
                        SLUG.replace_all(&title.to_lowercase(), "-")
                            .trim_matches('-')
                            .to_owned()
                    } else {
                        slug.clone()
                    };
                    match self
                        .store()
                        .and_then(|s| s.create(&name, title.trim(), *private, None))
                    {
                        Ok(project) => {
                            self.filter.clear();
                            self.notice.clear();
                            self.refresh();
                            self.refilter(Some(&project.id));
                            self.scroll = 0;
                            return;
                        }
                        Err(error) => self.notice = format!("{error:#}"),
                    }
                }
                _ => (),
            },
            Dialog::Status {
                id,
                revision,
                choice,
            } => match key {
                KeyCode::Down | KeyCode::Char('j') => *choice = (*choice + 1) % STATUSES.len(),
                KeyCode::Up | KeyCode::Char('k') => {
                    *choice = (*choice + STATUSES.len() - 1) % STATUSES.len()
                }
                KeyCode::Enter => {
                    match self
                        .store()
                        .and_then(|s| s.set_status(id, STATUSES[*choice], Some(revision)))
                    {
                        Ok(_) => {
                            self.notice.clear();
                            self.refresh();
                        }
                        Err(error) => self.notice = format!("{error:#}"),
                    }
                    return;
                }
                _ => (),
            },
        }
        self.dialog = Some(dialog);
    }
    pub fn paste(&mut self, text: &str) {
        let text: String = text
            .lines()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .map(|c| if c == '\t' { ' ' } else { c })
            .filter(|c| !c.is_control())
            .collect();
        if let Some(Dialog::New {
            title, slug, field, ..
        }) = &mut self.dialog
        {
            match field {
                0 => title.push_str(&text),
                1 => slug.push_str(&text),
                _ => (),
            }
        } else if self.dialog.is_none() && self.filtering {
            self.filter.push_str(&text);
            self.refilter(None);
            self.scroll = 0;
        }
    }
    pub fn mouse(&mut self, mouse: MouseEvent) {
        if self.dialog.is_some() {
            return;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
                if self.list_area.contains((mouse.column, mouse.row).into())
                    && mouse.row >= self.list_area.y + 2
                    && mouse.row < self.list_area.bottom().saturating_sub(1) =>
            {
                let row = self.table.offset() + usize::from(mouse.row - self.list_area.y - 2);
                if row < self.visible.len() {
                    self.table.select(Some(row));
                    self.scroll = 0;
                }
            }
            MouseEventKind::ScrollDown
                if self.list_area.contains((mouse.column, mouse.row).into()) =>
            {
                self.move_selection(1)
            }
            MouseEventKind::ScrollUp
                if self.list_area.contains((mouse.column, mouse.row).into()) =>
            {
                self.move_selection(-1)
            }
            MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_add(3),
            MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(3),
            _ => (),
        }
    }
    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(area);
        let header = format!(
            "Herdr Planbook · {}\n/ {}{}",
            self.root.display(),
            self.filter,
            if self.filtering {
                "▏  Enter to apply · Esc to leave"
            } else {
                ""
            }
        );
        frame.render_widget(
            Paragraph::new(terminal_text(&header)).style(Style::default().fg(Color::Cyan)),
            chunks[0],
        );
        let panels = Layout::default()
            .direction(if area.width < 85 {
                Direction::Vertical
            } else {
                Direction::Horizontal
            })
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(chunks[1]);
        self.list_area = panels[0];
        let rows = self.visible.iter().map(|index| {
            let p = &self.projects[*index];
            let count = p.progress();
            Row::new([
                p.title.clone(),
                p.status.clone(),
                if count.total == 0 {
                    "—".into()
                } else {
                    format!("{}/{}", count.done, count.total)
                },
            ])
        });
        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Min(10),
                    Constraint::Length(10),
                    Constraint::Length(7),
                ],
            )
            .header(
                Row::new(["Project", "Status", "Steps"])
                    .style(Style::default().add_modifier(Modifier::BOLD)),
            )
            .block(Block::default().borders(Borders::ALL).title(" Projects "))
            .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White)),
            panels[0],
            &mut self.table,
        );
        if let Some(project) = self.selected() {
            let label = match project.visibility.as_str() {
                "ignored" => "Private · Git ignored",
                "tracked" => "Shared · Git tracked",
                _ => "Untracked · not committed",
            };
            let heading = terminal_text(&format!(
                "{}\n{} · {}\nNext: {}\n{}",
                project.title,
                project.id,
                label,
                project.next_action(),
                project.path.display()
            ));
            let details =
                Layout::vertical([Constraint::Length(5), Constraint::Min(1)]).split(panels[1]);
            frame.render_widget(
                Paragraph::new(heading).wrap(Wrap { trim: false }),
                details[0],
            );
            let safe = terminal_text(&project.body);
            let lines = safe
                .lines()
                .map(|line| {
                    let style = if line.starts_with('#') {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else if line.contains("[x]") || line.contains("[X]") {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
                    };
                    Line::from(Span::styled(line.to_owned(), style))
                })
                .collect::<Vec<_>>();
            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Plan · PgUp/PgDn "),
                )
                .wrap(Wrap { trim: false });
            self.scroll = self.scroll.min(
                paragraph
                    .line_count(details[1].width)
                    .saturating_sub(usize::from(details[1].height))
                    .min(u16::MAX as usize) as u16,
            );
            frame.render_widget(paragraph.scroll((self.scroll, 0)), details[1]);
        } else {
            frame.render_widget(Paragraph::new(if self.projects.is_empty() {"No projects yet. Press n to create one, or add project.md under a configured root."} else {"No projects match the filter."}).wrap(Wrap{trim:false}).block(Block::default().borders(Borders::ALL).title(" Plan ")),panels[1]);
        }
        let message = if self.notice.is_empty() {
            self.issues
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            self.notice.clone()
        };
        frame.render_widget(
            Paragraph::new(terminal_text(&message))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: false }),
            chunks[2],
        );
        frame.render_widget(Paragraph::new("↑↓ Select  / Search  n New  s Status  e Edit  r Refresh  q Close\nEsc clears filter · PgUp/PgDn scroll plan"),chunks[3]);
        if let Some(dialog) = &self.dialog {
            let popup = centered(area, 70, 13);
            frame.render_widget(Clear, popup);
            let (title, lines) = match dialog {
                Dialog::New {
                    title,
                    slug,
                    private,
                    field,
                } => (
                    " New project ",
                    vec![
                        format!("{} Title: {title}", if *field == 0 { ">" } else { " " }),
                        String::new(),
                        format!("{} Slug: {slug}", if *field == 1 { ">" } else { " " }),
                        "  Leave slug empty to derive it from the title.".into(),
                        String::new(),
                        format!(
                            "{} [{}] Private · Git ignored (Space toggles)",
                            if *field == 2 { ">" } else { " " },
                            if *private { "x" } else { " " }
                        ),
                        String::new(),
                        "Tab: next field · Enter: create · Esc: cancel".into(),
                        terminal_text(&self.notice),
                    ],
                ),
                Dialog::Status { choice, .. } => (
                    " Project status ",
                    STATUSES
                        .iter()
                        .enumerate()
                        .map(|(i, s)| format!("{} {s}", if *choice == i { ">" } else { " " }))
                        .chain([
                            String::new(),
                            "↑↓ Choose · Enter: save · Esc: cancel".into(),
                        ])
                        .collect(),
                ),
            };
            frame.render_widget(
                Paragraph::new(terminal_text(&lines.join("\n")))
                    .wrap(Wrap { trim: false })
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(title)
                            .border_style(Style::default().fg(Color::Cyan)),
                    ),
                popup,
            );
        }
    }
    fn editor(&self, path: PathBuf) -> Result<()> {
        let path = within(&self.root, path.strip_prefix(&self.root)?)?;
        let editor = self
            .env
            .get("VISUAL")
            .filter(|s| !s.is_empty())
            .or_else(|| self.env.get("EDITOR"))
            .context("Set EDITOR or VISUAL, or edit the displayed path directly")?;
        let args = shell_words::split(editor)?;
        ensure!(!args.is_empty(), "EDITOR is empty");
        ensure!(
            Command::new(&args[0])
                .args(&args[1..])
                .arg(path)
                .status()?
                .success(),
            "Editor exited unsuccessfully"
        );
        Ok(())
    }
}

fn pop_grapheme(value: &mut String) {
    if let Some((last, _)) = value.grapheme_indices(true).next_back() {
        value.truncate(last);
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            std::io::stdout(),
            event::DisableMouseCapture,
            event::DisableBracketedPaste
        );
        ratatui::restore();
    }
}

pub fn run(root: PathBuf, state: PathBuf, env: Environment) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    let _restore = Restore;
    crossterm::execute!(
        std::io::stdout(),
        event::EnableMouseCapture,
        event::EnableBracketedPaste
    )?;
    let mut app = App::new(root, state, env);
    let mut refreshed = Instant::now();
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(Duration::from_millis(150))? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => match app.key(key) {
                    Intent::Quit => break,
                    Intent::Continue => (),
                    Intent::Edit(path) => {
                        crossterm::execute!(
                            std::io::stdout(),
                            event::DisableMouseCapture,
                            event::DisableBracketedPaste
                        )?;
                        ratatui::restore();
                        let result = app.editor(path);
                        terminal = ratatui::try_init()?;
                        crossterm::execute!(
                            std::io::stdout(),
                            event::EnableMouseCapture,
                            event::EnableBracketedPaste
                        )?;
                        if let Err(error) = result {
                            app.notice = format!("{error:#}");
                        }
                        app.refresh();
                    }
                },
                Event::Mouse(mouse) => app.mouse(mouse),
                Event::Paste(text) => app.paste(&text),
                _ => (),
            }
        }
        if refreshed.elapsed() >= Duration::from_secs(3) {
            app.refresh();
            refreshed = Instant::now();
        }
    }
    Ok(())
}
