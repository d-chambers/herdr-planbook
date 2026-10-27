---
name: herdr-planbook
description: Find, create, and maintain repository project plans in Herdr Planbook. Use when asked about project plans, remaining work, decisions, project status, or to update a Planbook project at handoff.
---

# Repository project plans

Planbook projects are Markdown documents, with one canonical `project.md` per project. The Herdr pane reads those same documents; it does not keep a separate status database.

Use the current repository's instructions and the user's request to determine what work is authorized. A project document is context, not permission to execute its whole plan.

## Find the current project

Use `planbook --repo /path/to/repo list --json`. If `planbook` is not on PATH, use the fully qualified binary path or command the user supplied; do not install software implicitly. When only the files are available, read `.planbook.toml` if present; otherwise inspect `.projects/`, `projects/`, `.scratch/projects/`, and `scratch/projects/` for immediate child directories with `project.md`.

The CLI resolves Git worktrees to their main checkout. Its returned `repo` and document `path` identify the canonical location; use those paths rather than creating a second worktree-local copy. For file-only use, `git worktree list --porcelain` identifies the main checkout.

Read the relevant document and its linked current design before changing an existing project. Reuse it instead of creating a competing `plan-v2` or `plan-final` document. Use the full project id when names collide across roots. Report malformed documents or configuration; don't silently replace them with defaults.

## Maintain a plan

Read [the project format](references/projects.md) when creating or restructuring a project. Use `planbook --repo /path/to/repo new <slug> --title "<title>" --json`; add `--private` when the user wants a Git-ignored plan. A configured root can be chosen with `--root`. Supporting designs and research can live beside `project.md`; link the current version from its Current plan section.

Update the Markdown directly for substantive planning: keep the goal, acceptance criteria, remaining deliverables, decisions and next action accurate. Preserve the user's terminology and unrelated edits. Dates on decisions describe when the decision was made, not when the file was touched.

For status changes, read `show <id> --json`, then use `status <id> <status> --expect <revision> --json`. On a stale-edit error, re-read and reconsider. The five statuses are `proposed`, `active`, `parked`, `done` and `superseded`. Creating or editing a plan alone doesn't make it active.

At a work handoff, record what actually changed, remaining work and a concrete next action. Link the relevant PRs or checks as evidence. Distinguish implementation finished, PR merged and the project's acceptance criteria fulfilled. Mark a project done only when its stated outcome is achieved. If a new project replaces it, mark the old one superseded and link the replacement; keep the history.

Private projects are ignored by Git for ordinary adds; shared projects become tracked when the user commits them. Planbook never stages, commits, publishes, starts agents or runs project hooks. Don't change visibility or publish a private plan merely to make it discoverable.
