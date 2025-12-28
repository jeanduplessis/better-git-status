---
status: IN-PROGRESS
last_updated: 2025-12-28
---

# Requirements Specification v2

## Overview

An interactive TUI git status **management tool** for the CLI. Building on v1's viewing capabilities, v2 adds comprehensive git operations: staging, committing, branching, pushing, pulling, and stashing—all without leaving the TUI.

## Core Principles

- **Safety first**: Destructive operations require confirmation
- **Discoverable**: Persistent key hints + full help overlay
- **Focused**: Essential operations only; advanced features use CLI

---

## Terminology

To avoid confusion, this spec uses precise terms for cursor and selection states:

| Term | Definition |
|------|------------|
| **Highlight** | The cursor position in the file list (one file at a time) |
| **Multi-select** | Files marked for bulk operations via `Space` (zero or more files) |
| **Diff focus** | The file whose diff is currently displayed in the diff panel |

- **Highlight** moves with `↑`/`↓` navigation
- **Multi-select** is a set of marked files, independent of highlight
- **Diff focus** is set by pressing `Enter` on a highlighted file
- When `s`/`u`/`d` is pressed:
  - If multi-select is non-empty: action applies to all multi-selected files
  - If multi-select is empty: action applies to the highlighted file

---

## Git Operations

### Single File Operations

| Key | Action | Confirmation |
|-----|--------|--------------|
| `s` | Stage highlighted/multi-selected file(s) | No |
| `u` | Unstage highlighted/multi-selected file(s) | No |
| `d` | Discard highlighted/multi-selected file(s) | Yes (inline prompt) |

- If files are multi-selected, action applies to all multi-selected files
- If no files multi-selected, action applies to highlighted file only
- Discard confirmation: inline prompt (e.g., `Discard changes? [y/N]`)

### Discard Behavior by File Type

| File State | Discard Action |
|------------|----------------|
| Modified (tracked) | Restore file from index (like `git checkout -- <file>`) |
| Deleted (tracked) | Restore file from index |
| Renamed (tracked) | Revert rename (restore original name and content) |
| Untracked | **Delete the file** (like `git clean -f <file>`) |
| Staged changes | Restore index entry from HEAD (like `git reset HEAD <file>`) |

- **Warning**: Discarding untracked files permanently deletes them
- Discard prompt for untracked files should be explicit: `Delete untracked file(s)? [y/N]`

### Bulk Operations

| Key | Action | Confirmation |
|-----|--------|--------------|
| `S` | Stage all unstaged files | Yes |
| `U` | Unstage all staged files | Yes |
| `D` | Discard all unstaged changes | Yes |

- All bulk operations require confirmation to prevent accidental override of individual file decisions
- Confirmation: inline prompt with count (e.g., `Stage 5 files? [y/N]`)
- Bulk discard (`D`) includes both tracked and untracked files; prompt should warn: `Discard all changes and delete untracked files (N files)? [y/N]`

### Multi-Select

| Key | Action |
|-----|--------|
| `Space` | Toggle multi-selection on highlighted file |
| `Esc` | Clear all multi-selections (when no modal is open) |

- Multi-selected files shown with distinct visual marker (e.g., `●` prefix or background highlight)
- `s`/`u`/`d` applies to all multi-selected files when multi-selection exists
- Multi-selection persists across navigation until cleared or action applied
- After action completes, multi-selection is cleared
- On refresh, remove any multi-selected files that no longer exist in the file list

### Undo

| Key | Action |
|-----|--------|
| `Ctrl+z` | Undo last stage/unstage operation |

- Scope: Stage and unstage operations only
- Discard, commit, push, pull are NOT undoable
- Single-level undo (most recent operation only)
- **Granularity**: One command invocation = one undo unit
  - Example: Pressing `S` to stage 10 files is one undo unit; `Ctrl+z` unstages all 10
  - Example: Multi-selecting 3 files and pressing `s` is one undo unit

**Terminal Note**: In raw terminal mode, `Ctrl+z` is captured by the application and does not trigger shell job suspension (SIGTSTP). This is expected behavior.

---

## Commit

| Key | Action |
|-----|--------|
| `c` | Open commit modal |

### Commit Modal

- **Layout**: Overlay modal centered on screen
- **Fields**:
  1. Title (required, single line, ~50 char soft limit indicator)
  2. Body (optional, multi-line text area)
- **Staged files list**: Display list of files to be committed above input fields
- **Amend option**: Toggle to amend last commit instead of creating new
  - When amend is enabled, pre-fill title and body from last commit message
- **Controls**:
  - `Tab` to move between fields
  - `Enter` in title field moves to body
  - `Ctrl+Enter` or dedicated submit key to confirm commit
  - `Esc` to cancel and close modal

### Commit Validation

- Block commit if title is empty (show inline validation error)
- Block commit if no staged files (unless amending)
- On commit failure (hooks, conflicts, etc.): keep modal open, display error message inline, preserve field contents

---

## Branch Operations

| Key | Action |
|-----|--------|
| `b` | Open branch modal |

### Branch Modal

- **Layout**: Overlay modal with searchable list
- **Features**:
  - List all local branches
  - Filter/search by typing (incremental, free-form)
  - `Backspace` to delete characters from search query
  - Current branch indicated (e.g., `*` prefix or distinct color)
  - "Create new branch" option when typed name doesn't match existing
- **Behavior**:
  - Select existing branch: switch to it
  - Create new branch: create and switch to it immediately
  - Selecting current branch: no-op with flash message "Already on branch X"
- **Controls**:
  - Type to filter/search
  - `↑`/`↓` to navigate list
  - `Enter` to select/create
  - `Esc` to cancel

### Branch Switching Rules

- **Uncommitted changes block switching**: If working directory has staged or unstaged changes, show error message: "Commit or stash changes before switching branches"
- Do NOT allow switch with uncommitted changes (no force option)
- **Detached HEAD**: If HEAD is detached, branch modal still works:
  - Current state labeled as "detached" (not a branch name)
  - Creating a new branch attaches HEAD at current commit

---

## Remote Operations

### Push

| Key | Action | Confirmation |
|-----|--------|--------------|
| `p` | Push to remote | No |
| `P` | Force push to remote | Yes |

- **Smart push behavior**:
  - If upstream is set: `git push`
  - If no upstream: `git push -u origin <branch>` (auto set-upstream)
- **Force push**: Requires confirmation (e.g., `Force push? This may overwrite remote history. [y/N]`)
- **Detached HEAD**: Block push with message "Cannot push from detached HEAD; create a branch first (b)"

### Pull

| Key | Action |
|-----|--------|
| `l` | Pull from remote |

- Executes `git pull` (fetch + merge)
- **On merge conflict**:
  - Show inline prompt: `Pull resulted in conflicts. Abort merge? [y/N]`
  - If user presses `y`: execute `git merge --abort`, restore pre-pull state
  - If user presses any other key: dismiss prompt, conflicts appear in unstaged section as `C` status
  - User resolves conflicts manually, then stages and commits

### Authentication & Errors

- Rely on system credentials (SSH agent, git credential helper)
- On auth failure: display git error message, user handles externally
- No in-TUI credential prompts

### Progress Indication

- Show spinner/progress indicator during push/pull operations
- Block UI interaction during operation
- Display operation name: `Pushing...` or `Pulling...`
- **Cancellation**: `Ctrl+c` signals cancellation intent; operation may not be immediately interruptible, but UI will show "Cancelling..." and ignore results once complete

---

## Stash Operations

| Key | Action |
|-----|--------|
| `z` | Stash all changes |
| `Z` | Pop latest stash |

- **Stash behavior**: Include untracked files (equivalent to `git stash -u`)
- **Pop behavior**: Apply and drop latest stash (`git stash pop`)
- No stash list/browser (use CLI for advanced stash management)
- On stash pop conflict: show error, conflicts appear in unstaged section

---

## Navigation & Controls

### File List Navigation

| Key | Action |
|-----|--------|
| `↑` | Move highlight up |
| `↓` | Move highlight down |
| `Enter` | View diff of highlighted file (sets diff focus) |
| `Space` | Toggle multi-selection on highlighted file |

### Diff Panel Navigation

| Key | Action |
|-----|--------|
| `Page Up` | Scroll diff up (full page) |
| `Page Down` | Scroll diff down (full page) |

### Global Controls

| Key | Action |
|-----|--------|
| `r` | Manual refresh |
| `?` | Show help overlay |
| `q` | Quit (always quits, even from modals) |
| `Esc` | Close current modal / Clear multi-selection |

---

## Modal Behavior

### Key Handling in Modals

- **`Esc`**: Close modal, return to normal view (does not quit)
- **`q`**: Quits application immediately, even when modal is open
- **`?`**: In help overlay, closes help; in other modals, no effect
- Modal-specific keys (Tab, Enter, etc.) only active when that modal is open
- Global operations (`s`, `u`, `d`, `p`, etc.) are blocked while a modal is open

### Modal Types

1. **Commit Modal**: Text input for commit message
2. **Branch Modal**: Searchable list with text filter
3. **Help Overlay**: Read-only keybinding reference
4. **Confirmation Prompt**: Inline yes/no prompt in status bar
5. **Progress Overlay**: Spinner with operation name (blocks all input except `Ctrl+c`)

---

## Help & Discoverability

### Persistent Key Hints

- Display minimal key hints in status bar area
- Format: `s:stage u:unstage d:discard c:commit b:branch p:push ?:help`
- Adapt based on available width (truncate least important first)

### Help Overlay

| Key | Action |
|-----|--------|
| `?` | Toggle help overlay |

- Full-screen or large modal overlay
- Complete keybinding reference organized by category:
  - File Operations
  - Bulk Operations
  - Commit & Branch
  - Remote Operations
  - Navigation
- Dismiss with `?` or `Esc`

---

## Visual Feedback

### Flash Messages

- Display operation results in status bar area
- Auto-dismiss after 2-3 seconds
- Examples:
  - `✓ Staged 3 files`
  - `✓ Committed: "Fix login bug"`
  - `✓ Pushed to origin/main`
  - `✗ Push failed: authentication error`

### Confirmation Prompts

- Inline prompt replacing status bar temporarily
- Format: `Action description? [y/N]`
- `y` or `Y` confirms, any other key cancels
- Case-insensitive for `y`

### Progress Indicators

- Spinner animation for long-running operations (push, pull)
- Display operation name: `Pushing...` or `Pulling...`

---

## Keybinding Summary

| Key | Action | Context |
|-----|--------|---------|
| `↑`/`↓` | Navigate file list | File list |
| `Enter` | View diff | File list |
| `Space` | Toggle multi-selection | File list |
| `s` | Stage | File list |
| `u` | Unstage | File list |
| `d` | Discard (confirm) | File list |
| `S` | Stage all (confirm) | Global |
| `U` | Unstage all (confirm) | Global |
| `D` | Discard all (confirm) | Global |
| `c` | Commit modal | Global |
| `b` | Branch modal | Global |
| `p` | Push | Global |
| `P` | Force push (confirm) | Global |
| `l` | Pull | Global |
| `z` | Stash | Global |
| `Z` | Stash pop | Global |
| `Ctrl+z` | Undo stage/unstage | Global |
| `r` | Refresh | Global |
| `?` | Help overlay | Global |
| `q` | Quit | Global (always) |
| `Esc` | Close modal / Clear multi-selection | Global |
| `Page Up` | Scroll diff up | Diff panel |
| `Page Down` | Scroll diff down | Diff panel |

---

## Layout Changes from v1

- **Status bar**: Now includes persistent key hints
- **Diff activation**: Changed from `Space`/`Enter` to `Enter` only (`Space` now toggles multi-selection)
- **Modals**: New overlay system for commit, branch, help, and confirmations

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Stage/unstage with no files | No-op, no error |
| Commit with no staged files | Block, show error in modal |
| Commit with empty title | Block, show validation error |
| Commit hook/operation failure | Keep modal open, show error, preserve fields |
| Switch branch with changes | Block, show "commit or stash first" message |
| Switch to current branch | No-op, flash message "Already on branch X" |
| Branch switch from detached HEAD | Allowed; creates branch at current commit |
| Push from detached HEAD | Block, show "Cannot push from detached HEAD" |
| Push with no upstream | Auto set-upstream to origin |
| Push with no origin remote | Show "No remote named 'origin'; configure via git CLI" |
| Push with no commits ahead | Show "Nothing to push" message |
| Pull with uncommitted changes | Allow (git handles this), show conflicts if any |
| Pull with merge conflict | Show abort prompt `[y/N]`; `y` = abort, other = show conflicts |
| Pull with no upstream | Show "No upstream configured; set with git push -u" |
| Stash with no changes | Show "Nothing to stash" message |
| Stash pop with conflicts | Show error, conflicts in file list |
| Stash pop with no stashes | Show "No stashes to pop" message |
| Force push to protected branch | Show git error (remote rejection) |
| Network timeout on push/pull | Show error message after timeout |
| Undo with nothing to undo | No-op, no error |
| Discard untracked file | Delete the file (with explicit confirmation) |
| Multi-select file that disappears on refresh | Remove from multi-selection silently |

---

## Out of Scope for v2

- Vim-style navigation (`j`/`k`)
- Interactive rebase
- Cherry-pick
- Stash list/browser
- Remote management (add/remove remotes)
- Tag management
- Submodule operations
- Commit history viewer
- Blame view
- Multiple undo levels
- Configurable keybindings
- Configurable themes
