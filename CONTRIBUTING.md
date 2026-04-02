# Contributing to Rippy

Thanks for your interest in contributing! Here's how to get started.

## Finding work

Check `ROADMAP.md` for open items. Pick any unchecked task and open a PR. For larger features (semantic search, image support, etc.), open an issue first so we can discuss the approach.

## Development setup

```bash
# Clone and build
git clone https://github.com/brianchuang/rippy.git
cd rippy
cargo build

# Run tests
cargo test

# Run the TUI
cargo run

# Run a CLI command
cargo run -- list --json
cargo run -- search "query" --json
```

Requires macOS and Rust 1.70+.

## Workflow

1. Fork the repo and create a branch from `main`:
   - `feat/<name>` for new features
   - `fix/<name>` for bug fixes
2. Make your changes, keeping the scope focused on one thing
3. Write tests (see below)
4. Make sure `cargo test` passes and `cargo build` produces no new warnings
5. Commit with a clear, concise message explaining what and why
6. Open a PR against `main` with:
   - A short summary of what changed
   - Example usage (if adding CLI features)
   - Test plan

## Testing

Every PR should include tests. We use Rust's built-in test framework with `#[cfg(test)]` modules colocated in each source file.

- **DB tests**: use `Store::open(Path::new(":memory:"))` for in-memory SQLite — no mocks
- **Formatting/output tests**: test both empty and populated cases
- **Serialization tests**: if you change `ClipEntry` fields, verify the JSON contract

Run the full suite with `cargo test`.

## Code guidelines

- Follow the existing patterns in the codebase
- Keep it simple — avoid unnecessary abstractions or speculative features
- Don't add features beyond what the PR is scoped to
- The `--json` flag on CLI commands outputs structured data for scripting; keep this contract stable
