# Architecture

Cargo workspace with three crates:

- `crates/engnote-core` — library crate holding all domain logic (note storage, search, linking, etc.). No UI code belongs here.
- `crates/engnote-cli` — binary crate (`engnote`), currently a thin entry point that calls into `engnote-core`.
- `crates/engnote-gui` — binary crate, a native GUI built with `egui`/`eframe` that reuses `engnote-core`. Chosen over Tauri to avoid a Node/web frontend toolchain; pure Rust end to end.

Intended trajectory:
1. Build out note management, Markdown parsing, and search inside `engnote-core`. (in progress)
2. Drive it from `engnote-cli` as a CLI/TUI first. (in progress)
3. `engnote-gui` reuses `engnote-core` without duplicating logic. (in progress)

Product requirements and the full feature brainstorm live in the owner's Notion workspace (page "要件定義" under "自分好みのメモアプリを作ろう"), not in this repo.
