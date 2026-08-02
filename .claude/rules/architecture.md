# Architecture

Cargo workspace with two crates:

- `crates/engnote-core` — library crate holding all domain logic (note storage, search, linking, etc.). No UI code belongs here.
- `crates/engnote-cli` — binary crate (`engnote`), currently a thin entry point that calls into `engnote-core`.

Intended trajectory (not yet implemented):
1. Build out note management, Markdown parsing, and search inside `engnote-core`.
2. Drive it from `engnote-cli` as a CLI/TUI first.
3. Later add a Tauri-based GUI crate that reuses `engnote-core` without duplicating logic.

Product requirements and the full feature brainstorm live in the owner's Notion workspace (page "要件定義" under "自分好みのメモアプリを作ろう"), not in this repo.
