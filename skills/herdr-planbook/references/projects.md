# Project format and guidelines

A project is a goal that may take several deliverables or pull requests. Each project has one entry point:

```text
.projects/
  coordinate-overhaul/
    project.md
    design.md
    reader-findings.md
```

Only `project.md` is required. Supporting files are ordinary Markdown; link the current ones from the entry point. The directory's path relative to the repository is the project's id (`.projects/coordinate-overhaul`). Keep that directory name stable; titles can change freely. Short directory names work as CLI selectors only when unambiguous.

## Metadata

```yaml
---
title: Coordinate overhaul
status: active
---
```

Use exactly two keys, `title` and `status`, with a plain `status:` line. The title is one nonblank line, at most 200 characters. YAML must contain unique string keys and literal scalar values, with no tags or aliases. Documents are UTF-8 and at most 256 KiB. Invalid metadata is reported without hiding valid projects.

| Status | Meaning |
| --- | --- |
| `proposed` | Under consideration or being refined |
| `active` | Work toward the outcome is under way |
| `parked` | Retained for later, with the reason recorded |
| `done` | The acceptance criteria are fulfilled |
| `superseded` | Replaced by a linked project or design |

A checkbox count describes listed work items, not a prediction or proof of completion. Code-fenced examples do not count. Status is an explicit statement; agent activity and file timestamps never change it automatically.

## Document sections

The template supplies Goal, Current plan, Acceptance criteria, Remaining work, Decisions and open questions, and Next action. These are useful conventions rather than mandatory prose validation. Use ordinary Markdown checkboxes for deliverables. Keep the Next action heading when a concise next step should appear in the CLI and pane; without it, the first unchecked work item is used.

Write decisions with their reasons and link relevant PRs. Record open questions separately from settled decisions. Update the existing entry point as understanding improves instead of leaving several competing final plans. Park or supersede abandoned work explicitly.

## Discovery and configuration

The default roots are `.projects`, `projects`, `.scratch/projects`, and `scratch/projects`. Discovery reads only their immediate child directories. It never searches unrelated scratch files or follows symbolic links.

Override the roots in the repository's `.planbook.toml`:

```toml
roots = [".projects", "project", ".scratch/projects"]
```

Paths are relative to the repository and cannot escape it. A malformed configuration is an error. There is no manifest or index file to keep synchronized with project documents.

Git worktrees use their main checkout's project roots. This keeps private projects and planning edits in one place. The pane and CLI report the actual repository and document paths. Editing a shared plan therefore changes the main checkout; commit it there when wanted. Repos with a bare Git directory and no main checkout use the invoking worktree. Separate clones remain independent.

## Private and shared

`new --private` prefers a configured scratch root and creates a `.gitignore` containing `*` inside the new project directory. It never changes ignore rules for other projects. Ordinary Git adds leave this directory out; explicitly forcing files into Git still publishes them. In a folder without Git the files stay local, but Git visibility is unavailable.

The pane reports observed visibility: **ignored** (private), **tracked** (shared with the repository), or **untracked** (not yet committed or ignored). An untracked file is not automatically private. Repository access determines who can read a shared project; there is no hosting service.

## Updates

Status changes preserve document prose and detect stale revisions. Use the revision returned by `show --json` with `status --expect` when acting on a previously read document. Hand edits are supported; refresh before retrying a rejected update. Creation refuses to overwrite an existing directory. Planbook locks coordinate its own writers; ordinary editors do not participate in those locks.
