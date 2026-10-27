# Herdr Planbook

Keep project documents authoritative. Herdr supplies the terminal pane; Planbook supplies discovery, preview, and explicit document operations. Do not add agent execution, repository shell hooks, background services, or automatic status inference to this MVP.

Runtime dependencies are locked in `Cargo.lock`. Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked`. Tests use temporary repositories and Ratatui's test backend; never use a user's real projects or Herdr configuration as fixtures.

Read and write project Markdown through `documents.rs` and `store.rs`. Preserve prose and detect stale edits. Keep the bundled skill, guidelines, and template consistent with the CLI. Temporary reviews belong under the ignored `.scratch/` directory.
