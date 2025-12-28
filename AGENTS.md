# AGENTS.md

## Project Overview
Interactive git status CLI tool showing a tree view of changed files with diff preview. Written in Rust using ratatui for TUI and git2 for git operations.

## Build Commands
- Build: `cargo build`
- Release build: `cargo build --release`
- Run: `cargo run`
- Test all: `cargo test`
- Test single: `cargo test test_name`
- Lint: `cargo clippy`
- Format: `cargo fmt`
- Check: `cargo check`

## Architecture

### Module Structure
- `main.rs` - Entry point, CLI parsing with clap
- `app.rs` - Application state, event loop, input handling
- `git.rs` - Git operations (status, diff, branch info) via git2
- `ui.rs` - UI rendering with ratatui (status bar, file list, diff panel)

### Key Dependencies
- `ratatui` - Terminal UI framework
- `crossterm` - Terminal manipulation
- `git2` - Git operations (libgit2 bindings)
- `clap` - CLI argument parsing
- `anyhow` - Error handling

### Key Concepts
- **Staged files**: HEAD→INDEX diff (`git diff --cached`)
- **Unstaged files**: INDEX→WORKTREE diff (`git diff`)
- **Dual-state files**: Files with both staged and unstaged changes appear in both sections
- **Navigation**: Highlight (cursor) vs Selection (diff shown) are separate states
- **Color scheme**: Catppuccin Mocha palette

## Code Style
- Follow Rust standard conventions (snake_case for functions/variables, PascalCase for types)
- Use `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Prefer `Result<T, E>` for error handling with `?` operator
- Use `anyhow` for error types (already in dependencies)
- Keep functions small and focused
- Write doc comments (`///`) for public APIs

## Testing
- Run tests with `cargo test`
- Unit tests go in the same file as the code being tested (inline `#[cfg(test)]` modules)
- Integration tests go in `tests/` directory

## Agent Instructions
After making code changes:
1. Run `cargo test` to verify all tests pass
2. Run `cargo clippy` to check for linter warnings
3. Update or add tests when modifying existing functionality or adding new features
4. Ensure new code has corresponding test coverage
