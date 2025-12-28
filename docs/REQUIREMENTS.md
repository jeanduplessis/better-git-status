---
status: COMPLETED
last_updated: 2025-12-27
---

# Requirements Specification

## Overview

An interactive TUI git status viewer designed as a companion for terminal-based coding agents. Optimized for narrow terminal widths (like an IDE sidebar), providing real-time visibility into repository changes.

## Git Semantics

- **Staged section**: Shows `HEAD..INDEX` changes (equivalent to `git diff --cached`)
- **Unstaged section**: Shows `INDEX..WORKTREE` changes (equivalent to `git diff`)
- **Repository discovery**: Use `git2::Repository::open(".")` only (no walking up parent directories). If it fails, exit with error: `Not a git repository`
- **Ignored files**: Not displayed (follows `.gitignore`)
- **Submodules**: Shown as single entry; use `M` if changed from recorded commit, `A` if newly added, `D` if removed; never shown as `R` or `C`
- **Type changes**: Display as `M` (Modified) with same color

## Layout

- **Single-column, three-row layout**
  - Top: Status bar (branch + summary counts)
  - Middle: File list panel
  - Bottom: Diff preview panel
- **Status bar format**: `<branch> S:<count> U:<count> ?:<count>`
  - Example: `main S:3 U:5 ?:2`
  - **Detached HEAD**: Show short commit hash, e.g., `HEAD@1234abc S:0 U:0 ?:0`
  - **Count definitions**:
    - `S`: Number of distinct paths with any staged changes
    - `U`: Number of distinct paths with any unstaged changes (including conflicts)
    - `?`: Number of untracked files (not directories)
    - A path with both staged and unstaged changes contributes to both S and U
- **File list sizing**: Dynamic height, up to 33% of terminal height maximum; scrolls when content exceeds allocation
- **Diff panel sizing**: Fills remaining vertical space
- **Minimum dimensions**: 30 columns × 10 rows; show "Terminal too small" message and wait for resize if below minimum
- **Graceful degradation**: Adapt display based on available space

## File List Panel

### Organization
- **Tree view**: Non-collapsible; files grouped by directory with visual indentation; directories are implicit (no separate directory rows). Tree view is purely visual indentation derived from splitting the path.
- **Sort order**: Alphabetical by full path string within each section
- **Sections**: Two distinct sections with minimal headers
  - `[STAGED]` (shown first)
  - `[UNSTAGED]`
- **Empty sections**: Hidden entirely (no header shown)
- **Untracked files**: Show all individual untracked files that aren't ignored; untracked directories are not shown as separate rows; appear only in `[UNSTAGED]` section
- **Conflicts**: Conflicted paths appear **only in `[UNSTAGED]`** section, regardless of any staged state

### Path Display
- **Truncation**: From start with ellipsis prefix (e.g., `…/long/path/file.rs`)
- **Preserve**: At least filename + one parent directory when possible
- **Column priority** (when width is constrained):
  1. Full: `[STATUS] [+/-] path`
  2. Drop +/- counts first
  3. Then truncate path
  4. Then: status + filename tail only
  5. Finally: status symbol alone (if even 1-char filename doesn't fit)

### File Display
- **Status indicators**: Classic git symbols
  - `M` - Modified (also used for type changes)
  - `A` - Added
  - `D` - Deleted
  - `R` - Renamed (shows new path only; old path visible in diff header)
  - `?` - Untracked
  - `C` - Conflict (unmerged states like `UU`, `AA`, `DD`)
- **Status colors** (Catppuccin Mocha palette):
  - Green: Added
  - Red: Deleted
  - Yellow: Modified
  - Blue: Renamed
  - Gray: Untracked
  - Magenta: Conflict
- **+/- counts**: Shown per file individually; show `-/-` for binary files
- **Dual appearance**: Files with both staged and unstaged changes appear in both sections, each showing only its respective +/- counts. This rule applies only to non-conflict paths; conflicted paths always appear only once in `[UNSTAGED]` and never in both sections.
- **Conflicts in counts**: Conflicted paths count toward `U` only (not `S`)

### Visual States
- **Highlighted**: Current cursor position during navigation (distinct style)
- **Selected**: File whose diff is currently displayed (distinct style, e.g., marker or inverted colors)
- **Initial state**:
  - If files exist: first file highlighted, no file selected
  - If repo is clean: no row highlighted, diff panel shows clean-repo placeholder
- **File moves between sections**: If selected/highlighted `(section, path)` pair disappears (e.g., file becomes only staged or only unstaged), treat as "disappeared" and apply disappear rules

## Diff Preview Panel

- **Activation**: Only shows diff after user presses Space or Enter to select a file. Moving the highlight with ↑/↓ does **not** change the diff; the diff always shows the last file that was explicitly selected with Space/Enter.
- **Empty state**: Placeholder message with hint: `↑/↓ navigate, Space to view diff`
- **Clean repo state**: Placeholder message: `No changes (q to quit)`
- **Diff headers**: Include standard unified diff headers (`diff --git a/... b/...`, `---`, `+++`, `@@` lines)
- **Line numbers**: Single column showing new file line numbers (added/context lines show number, deleted lines show `-`). Line numbers shown on **first visual line** of each logical diff line; wrapped continuation lines show empty number column.
- **Long lines**: Soft-wrap to viewport width (no horizontal scrolling)
- **Syntax highlighting**: Color-formatted diff output (Catppuccin Mocha)
  - Green: Added lines
  - Red: Deleted lines
  - Cyan: Hunk headers and diff headers
- **Binary files**: Show "Binary file" message instead of diff
- **Non-UTF-8 files**: Show "File contains invalid UTF-8 encoding" message
- **Conflict files**: Show "Conflict - resolve before viewing diff" message (no 2-way diff for v1)
- **Scrolling**: Page Up/Down scrolls by viewport height; no-op at top/bottom bounds
- **Dual-state files**: Staged section shows `HEAD..INDEX` diff; Unstaged section shows `INDEX..WORKTREE` diff

## Navigation & Controls

| Key | Action |
|-----|--------|
| ↑/↓ | Navigate file list (move highlight) |
| Space / Enter | Select highlighted file and show diff |
| Page Up | Scroll diff up (full page) |
| Page Down | Scroll diff down (full page) |
| q | Quit |

## Auto-Refresh

- **Method**: File system watching (using `notify` crate)
- **Scope**: Watch working directory, `.git/index`, and `.git/HEAD` so that changes to the current commit/branch, index, and worktree all trigger a refresh
- **Debounce**: 150ms (fixed) - time since last event before re-running status + diff computation
- **Selection preservation**: Preserve by file identity `(section, path)`, not index position
- **Highlight preservation**: If highlighted file disappears, move to nearest remaining entry (preserve index if possible, clamp to last)
- **Selection on disappear**: Clear selection, show placeholder in diff panel
- **Fallback**: If file watcher fails, log warning to stderr and fall back to timer-based polling (fixed 2s interval)
- **Repo disappears during polling**: Exit with "Not a git repository" error

## Color Scheme

- **Palette**: Catppuccin Mocha
- **Implementation**: Use ANSI colors mapped to Catppuccin Mocha values

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Clean working directory | Show placeholder: `No changes (q to quit)` |
| `git2::Repository::open(".")` fails | Exit with error: `Not a git repository` |
| Bare repository | Exit with error: `Repository has no working directory` |
| Detached HEAD | Show status normally; status bar shows `HEAD@<short-hash>` |
| Terminal too small | Show "Terminal too small" message; resume when resized |
| Binary file selected | Show "Binary file" message in diff panel |
| Non-UTF-8 file selected | Show "File contains invalid UTF-8 encoding" in diff panel |
| Conflict file selected | Show "Conflict - resolve before viewing diff" in diff panel |
| File watcher fails | Log warning, fall back to 2s polling |
| Repo disappears during polling | Exit with "Not a git repository" error |

## Future Considerations (Out of Scope for v1)

- Vim-style navigation (j/k)
- Stage/unstage files from UI
- Manual refresh command
- Commit from UI
- Multiple file selection
- Collapsible directory tree
- Agent integration (output selected file path)
- Home/End keys for diff navigation
- Tab to focus between panels
- Conflict 2-way/3-way diff viewing
- Configurable themes and keymaps
