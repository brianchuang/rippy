# CLAUDE.md

Instructions for AI agents working on this codebase.

## Project overview

Rippy is a macOS clipboard history manager with a vim-style TUI, written in Rust. See `ROADMAP.md` for open work items.

## Build and test

```bash
cargo build          # compile
cargo test           # run all tests (must pass before committing)
cargo run             # launch TUI
cargo run -- <cmd>    # run a subcommand (list, search, copy, clear, etc.)
```

## Architecture

- `src/main.rs` — CLI (clap), subcommand handlers, output formatting
- `src/db.rs` — SQLite store (`ClipEntry`, CRUD operations)
- `src/tui.rs` — ratatui TUI with vim modal keybindings
- `src/watcher.rs` — background clipboard polling
- `src/clipboard.rs` — macOS NSPasteboard FFI
- `src/hotkey.rs` — global hotkey via CGEventTap
- `src/config.rs` — config.toml (hotkey, terminal settings)
- `src/terminal.rs` — terminal app detection/launching

Data lives in `~/.local/share/rippy/` (SQLite DB + config).

## Contribution workflow

1. **Pick an item** from `ROADMAP.md` (open an issue first for large features)
2. **Read the relevant source** before writing code — understand the existing patterns
3. **Branch** off `main`: `git checkout -b feat/<short-name>` or `fix/<short-name>`
4. **Implement** the change, following existing code style (no unnecessary abstractions)
5. **Write tests** — this project uses `#[cfg(test)]` modules colocated in each source file. Use `:memory:` SQLite for DB tests. Every PR should include tests.
6. **Verify**: `cargo test` passes, `cargo build` has no new warnings
7. **Commit** with a concise message: what changed and why (1-2 sentences)
8. **Push** and open a PR against `main` with a summary, example usage, and test plan

## Design philosophy

This codebase is **functional and composition-first**. Prefer pure functions that take inputs and return values over stateful methods and side effects. Build complex behavior by composing small, well-typed functions — not by layering abstractions or reaching for traits and generics prematurely.

Concretely:
- Functions that transform data should be pure: `fn(&[ClipEntry]) -> String`, not methods that write to stdout
- Push side effects (I/O, clipboard access, DB writes) to the edges; keep the core logic testable without mocks
- Compose at the call site. If two functions can be piped together, that's better than a new abstraction that wraps both
- Reach for `impl Trait` or generics only when you have two or more concrete callers — not speculatively

This is a CLI tool, not a framework. The right unit of reuse is a function, not a type hierarchy.

## Code style

- No unnecessary abstractions — three similar lines > a premature helper
- Tests live in `#[cfg(test)] mod tests` at the bottom of each file
- DB tests use `Store::open(Path::new(":memory:"))` — fast, no disk I/O
- `ClipEntry` is the core data type; keep its `Serialize` contract stable (no `hash` in JSON)
- Output formatting functions live in `main.rs` alongside the CLI handlers

## Testing conventions

- Unit tests for data serialization, DB operations, and formatting logic
- Test both the happy path and edge cases (empty inputs, nulls, limits)
- No mocks for SQLite — use in-memory databases for real query coverage
