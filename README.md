# Herdr Planbook

A Herdr pane for repository project plans, with a CLI and an agent skill. The Markdown documents are the record: the pane shows the same title, status, work items and next action that agents read.

This MVP supports discovery, preview, filtering, project creation, explicit status changes, and opening a document in your editor. It uses Herdr for its terminal pane. It has no agent launcher, repository shell hooks, network service, updater, or automatic GitHub synchronization.

## Install locally

Requires Rust 1.88+ and Herdr 0.9.1+ on Linux or macOS. Git [2.36+](https://raw.githubusercontent.com/git/git/v2.36.0/Documentation/RelNotes/2.36.0.txt) enables worktree discovery and tracking/ignore indicators. A plain folder works too.

```sh
cd /path/to/herdr-planbook
cargo build --release --locked
herdr plugin link "$PWD"
```

Linking does not build the binary; run the build first. You can also install directly through Herdr:

```sh
herdr plugin install d-chambers/herdr-planbook
```

The manifest runs `cargo build --release --locked`, building this checkout with locked dependencies. The pane and CLI run as one native Rust binary.

Inside a Herdr workspace:

```sh
herdr plugin action invoke open --plugin dchambers.planbook
```

The action opens a right split for the invoking repository. You can bind it in Herdr's configuration if desired; Planbook never changes keybindings. Closing the pane ends the UI and leaves your documents in place.

Outside Herdr, use the same pane directly:

```sh
./target/release/planbook --repo /path/to/repository ui
```

## Project documents

```text
.projects/
  coordinate-overhaul/
    project.md
    design.md              # optional; linked from project.md
```

Each `project.md` begins with YAML containing exactly `title` and `status`. The statuses are `proposed`, `active`, `parked`, `done`, and `superseded`. The rest is ordinary Markdown. The template includes the goal, current plan, acceptance criteria, remaining work, decisions and open questions, and next action.

The default roots are `.projects/`, `projects/`, `.scratch/projects/`, and `scratch/projects/`. Configure other locations, including `project/`, with `.planbook.toml`:

```toml
roots = [".projects", "project", ".scratch/projects"]
```

Discovery is limited to immediate child directories and rejects symbolic links. Invalid documents and configuration produce visible diagnostics. There is no separate project index to maintain.

**Worktrees:** every Git worktree uses the main checkout's project roots. The pane and CLI show their actual paths. This keeps private notes and planning edits in one location; shared plan edits change that main checkout and can be committed there. Repos with a bare Git directory and no main checkout use the invoking worktree. Separate clones remain independent.

**Visibility:** the pane distinguishes Git-tracked, Git-ignored, and untracked documents. Private creation puts `*` in the new project's own `.gitignore`; ordinary Git adds leave it out. It does not alter neighboring projects or the repository's ignore rules. Shared documents are ordinary files that become tracked when you commit them. No files are staged or published automatically.

Read the [full guidelines](skills/herdr-planbook/references/projects.md) or run `planbook guide` for the format and update rules.

## Pane controls

| Key | Action |
| --- | --- |
| Up / Down, mouse | Select a project and preview its document |
| `/` | Filter by title, status, directory or next action |
| `n` | Create a project, with an optional private checkbox |
| `s` | Choose a status explicitly |
| `e` | Edit the document with your `VISUAL` or `EDITOR` |
| `r` | Refresh immediately |
| Page Up / Page Down | Scroll the plan preview |
| Escape | Clear the filter, or cancel a dialog |
| `q` | Close the pane |

The creation form uses Tab to choose title, optional slug, or privacy; Space toggles privacy and Enter creates the project. The status dialog uses Up/Down and Enter. Search uses Enter to return to the list.

The pane notices external edits every three seconds. Git tracking queries are batched rather than run separately for every project. Narrow panes stack the list and preview vertically. Markdown links are displayed without opening URLs or reading linked files automatically. Editor invocation uses your environment, not project configuration, and passes an argument list without a shell. Terminal control characters in documents are shown as visible escapes; the original Markdown is preserved. Headings and completed tasks are highlighted; Markdown links and formatting remain readable text.

## Agent CLI

Use `/path/to/herdr-planbook/target/release/planbook` in place of `planbook` below when the executable is not on PATH. `cargo install --path /path/to/herdr-planbook --locked` is an optional explicit way to add it to PATH.

```sh
planbook --repo /path/to/repo list --json
planbook --repo /path/to/repo new coordinate-overhaul --title "Coordinate overhaul" --json
planbook --repo /path/to/repo new query-expressions --title "Query expressions" --private --json
planbook --repo /path/to/repo show coordinate-overhaul --json
planbook --repo /path/to/repo status coordinate-overhaul active --expect REVISION --json
planbook skill
planbook guide
```

Use the `revision` returned by `show` with `status --expect` to detect a stale edit. Status changes preserve prose, comments and line endings. Edit the Markdown directly for substantive planning. Duplicate directory names require a full id such as `.projects/coordinate-overhaul`. `list` reports valid projects and diagnostics together and exits nonzero when any issue needs attention.

`--repo` precedes the subcommand; `--json` follows it. Operation failures return a nonzero exit code and, with `--json`, an error object. Argument errors use the CLI parser's diagnostics. Read-only commands never create projects or lock files. Planbook's write locks live under `$XDG_STATE_HOME/herdr-planbook/locks` or `~/.local/state/herdr-planbook/locks`; project status is never stored there.

## Agent skill

The bundled [herdr-planbook skill](skills/herdr-planbook/SKILL.md) teaches agents to discover existing projects, maintain one canonical entry point, record decisions and next actions, and distinguish finished implementation from a fulfilled project outcome. It does not authorize execution of a plan or change an agent's permissions.

Install it explicitly into your agent's skills directory. For agents using `~/.agents/skills`, one local option is:

```sh
mkdir -p ~/.agents/skills
ln -s /absolute/path/to/herdr-planbook/skills/herdr-planbook ~/.agents/skills/herdr-planbook
```

For Claude Code, use its `~/.claude/skills` directory instead. The directory contains the skill, guidelines and template together; copy or link the whole directory. Existing agent configuration and skills are never modified by the plugin itself.

## Rust libraries

The core stack follows [Herdr Projects](https://github.com/d-chambers/herdr-projects): Ratatui 0.30 with only its Crossterm backend, Crossterm 0.29, Serde/serde_json, TOML 0.9, yaml-rust2 0.10, regex, unicode-width (through Ratatui), unicode-segmentation, and libc. The rendered-line-info feature provides exact preview scroll bounds; dependencies are locked. This is an independent implementation with a smaller scope.

Clap supplies the CLI, anyhow supplies error context, SHA-256 identifies document revisions, fs2 coordinates writers, tempfile supplies atomic replacement files, subprocess waits are bounded with the standard library, and shell-words parses the user's editor command without a shell. No async runtime or network client is needed.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Tests use temporary repositories and Ratatui's test backend. No test launches a coding agent, shares a server, or changes the user's Herdr setup. CI runs formatting, Clippy, tests, and a release build on Linux and macOS, and checks compilation on Rust 1.88. The agent skill, guidelines, and template are compiled into the CLI; the complete skill directory also ships alongside the source for agent installation.
