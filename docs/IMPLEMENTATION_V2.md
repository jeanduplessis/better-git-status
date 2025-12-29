---
status: IN-PROGRESS
last_updated: 2025-12-29
completed_phases: 1, 2, 3, 4, 5, 6
---

# Implementation Plan v2

Implementation plan for the v2 requirements defined in [REQUIREMENTS_V2.md](REQUIREMENTS_V2.md).

## Overview

Building on v1's viewing capabilities, v2 adds comprehensive git operations: staging, committing, branching, pushing, pulling, and stashing—all without leaving the TUI.

**Key Constraints:**
- Safety first: destructive operations require confirmation
- Single-level undo for stage/unstage only
- No in-TUI credential prompts (rely on system credentials)
- Out of scope: vim navigation, rebase, cherry-pick, stash browser, tag management

**Assumptions:**
- Existing v1 architecture (app.rs, git.rs, ui/) is stable and well-tested
- git2 crate supports all required operations (stage, unstage, commit, branch, push, pull, stash)
- Push/pull operations will shell out to `git` CLI for better credential handling

**Technology Choices:**
- **Git operations:** git2 for local ops; CLI shelling for push/pull (credential helpers)
- **Modal system:** New `ui/modal.rs` module with overlay rendering
- **State management:** Extend `App` struct with modal state, multi-select, undo stack

---

## Phase 1: Multi-Select Infrastructure

### Description

Add multi-select capability to the file list, allowing users to mark multiple files for batch operations. This is foundational for all staging/unstaging/discard operations.

### Deliverables

**1. Types Extension (`/src/types.rs`)**

- Add `MultiSelectSet` type alias: `HashSet<(Section, String)>`
- Update `VisibleRow` if needed for multi-select state

**2. App State Extension (`/src/app.rs`)**

- Add `multi_selected: HashSet<(Section, String)>` field to `App`
- Add `toggle_multi_select()` method for Space key
- Add `clear_multi_select()` method for Esc key
- Add `get_action_targets()` method: returns multi-selected if non-empty, else highlighted
- On refresh, prune `multi_selected` to remove files that no longer exist

**3. UI Multi-Select Indicators (`/src/ui/file_list.rs`)**

- Add visual marker `◆` prefix for multi-selected files
- Ensure marker is distinct from highlight `>` and selection `●`

**4. Key Handling (`/src/app.rs`)**

- `Space`: toggle multi-select on highlighted file (instead of select for diff)
- `Enter`: set diff focus (moved from Space/Enter to Enter only)
- `Esc`: clear multi-select when no modal is open

### Acceptance Criteria

- [ ] Pressing `Space` on a file adds/removes it from multi-select set
- [ ] Multi-selected files display `◆` marker in file list
- [ ] Pressing `Esc` clears all multi-selections
- [ ] `Enter` sets diff focus (Space no longer does)
- [ ] Multi-select persists across navigation (up/down)
- [ ] Files removed during refresh are pruned from multi-select

### Checkpoint

**Commit:** "feat(ui): add multi-select infrastructure for file list"
**Verify:** `cargo test && cargo clippy`
**State:** Can multi-select files with Space, clear with Esc, see visual markers

---

## Phase 2: Git Stage/Unstage Operations

### Description

Implement the core staging and unstaging operations using git2, including single file and bulk operations.

### Deliverables

**1. Git Operations Module (`/src/git.rs`)**

- `stage_files(repo, paths)` — Add files to index (`git add`)
- `unstage_files(repo, paths)` — Reset files from HEAD to index (`git reset HEAD`)
- `stage_all(repo)` — Stage all unstaged files
- `unstage_all(repo)` — Unstage all staged files
- Handle edge cases: deleted files, renamed files, untracked files

**2. App Stage/Unstage Methods (`/src/app.rs`)**

- `stage_selected()` — Stage highlighted or multi-selected files
- `unstage_selected()` — Unstage highlighted or multi-selected files
- `stage_all()` — Stage all with confirmation
- `unstage_all()` — Unstage all with confirmation
- Clear multi-select after operation completes
- Auto-refresh after operations

**3. Key Bindings (`/src/app.rs`)**

- `s`: stage selected file(s)
- `u`: unstage selected file(s)
- `S`: stage all (requires confirmation)
- `U`: unstage all (requires confirmation)

### Acceptance Criteria

- [ ] `s` stages highlighted file when no multi-select
- [ ] `s` stages all multi-selected files when multi-select exists
- [ ] `u` unstages highlighted file when no multi-select
- [ ] `u` unstages all multi-selected files when multi-select exists
- [ ] `S` prompts "Stage N files? [y/N]" and stages all on `y`
- [ ] `U` prompts "Unstage N files? [y/N]" and unstages all on `y`
- [ ] File list refreshes after stage/unstage
- [ ] Multi-select is cleared after operation

### Checkpoint

**Commit:** "feat(git): implement stage/unstage operations"
**Verify:** `cargo test && cargo clippy`
**State:** Can stage/unstage individual and all files with keyboard

---

## Phase 3: Undo System

### Description

Implement single-level undo for stage/unstage operations only.

### Deliverables

**1. Undo State (`/src/types.rs`)**

- `UndoAction` enum: `Stage { paths: Vec<String> }`, `Unstage { paths: Vec<String> }`

**2. App Undo Integration (`/src/app.rs`)**

- Add `last_action: Option<UndoAction>` field
- Record action in `stage_selected()`, `unstage_selected()`, bulk ops
- `undo()` method: reverse the last action
- Clear `last_action` after undo (single-level only)

**3. Key Binding (`/src/app.rs`)**

- `Ctrl+z`: undo last stage/unstage

### Acceptance Criteria

- [ ] Staging 3 files records one undo unit
- [ ] `Ctrl+z` after staging unstages all 3 files
- [ ] `Ctrl+z` after unstaging restages the files
- [ ] Second `Ctrl+z` is a no-op (single-level)
- [ ] Discard operations are NOT recorded in undo
- [ ] Raw terminal mode captures `Ctrl+z` (no SIGTSTP)

### Checkpoint

**Commit:** "feat(app): implement single-level undo for stage/unstage"
**Verify:** `cargo test && cargo clippy`
**State:** Can undo last stage/unstage operation with Ctrl+z

---

## Phase 4: Confirmation Prompt System

### Description

Implement inline confirmation prompts in the status bar for destructive operations.

### Deliverables

**1. Prompt State (`/src/types.rs`)**

- `ConfirmPrompt` struct: `message: String`, `on_confirm: ConfirmAction`
- `ConfirmAction` enum: `StageAll`, `UnstageAll`, `DiscardAll`, `DiscardSelected`, `ForcePush`

**2. App Prompt Integration (`/src/app.rs`)**

- Add `confirm_prompt: Option<ConfirmPrompt>` field
- `show_confirm(message, action)` method
- `handle_confirm_input(key)` method: `y`/`Y` confirms, any other dismisses
- Block normal key handling while prompt is active

**3. Status Bar Prompt Rendering (`/src/ui/status_bar.rs`)**

- When `confirm_prompt` is active, render prompt message instead of normal status
- Format: `[message] [y/N]`
- Distinct styling (e.g., yellow background)

### Acceptance Criteria

- [ ] `S` shows "Stage N files? [y/N]" in status bar
- [ ] Pressing `y` or `Y` executes the action
- [ ] Pressing any other key dismisses prompt without action
- [ ] Normal key bindings are blocked while prompt is visible
- [ ] Prompt disappears after confirm or dismiss

### Checkpoint

**Commit:** "feat(ui): implement inline confirmation prompts"
**Verify:** `cargo test && cargo clippy`
**State:** Bulk operations show confirmation prompts that work correctly

---

## Phase 5: Flash Messages

### Description

Implement auto-dismissing flash messages for operation feedback.

### Deliverables

**1. Flash State (`/src/types.rs`)**

- `FlashMessage` struct: `text: String`, `is_error: bool`, `shown_at: Instant`

**2. App Flash Integration (`/src/app.rs`)**

- Add `flash_message: Option<FlashMessage>` field
- `show_flash(text, is_error)` method
- Auto-dismiss logic in event loop (after 2-3 seconds)

**3. Status Bar Flash Rendering (`/src/ui/status_bar.rs`)**

- When `flash_message` is active (and no prompt), render flash instead of normal status
- Success: `✓ message` in green
- Error: `✗ message` in red

### Acceptance Criteria

- [ ] After staging, "✓ Staged N files" appears briefly
- [ ] After error, "✗ Error message" appears in red
- [ ] Flash auto-dismisses after ~2-3 seconds
- [ ] Flash is replaced by new flash if another action occurs
- [ ] Prompt takes priority over flash when active

### Checkpoint

**Commit:** "feat(ui): implement flash messages for operation feedback"
**Verify:** `cargo test && cargo clippy`
**State:** Operations show success/error feedback that auto-dismisses

---

## Phase 6: Discard Operations

### Description

Implement file discard with proper handling for different file types.

### Deliverables

**1. Git Discard Operations (`/src/git.rs`)**

- `discard_unstaged_file(repo, path)` — Restore from index (`git checkout -- <file>`)
- `discard_untracked_file(repo, path)` — Delete the file (`git clean -f`)
- `discard_staged_file(repo, path)` — Reset from HEAD (`git reset HEAD <file>`)
- `discard_all_unstaged(repo)` — Discard all unstaged changes including untracked

**2. App Discard Methods (`/src/app.rs`)**

- `discard_selected()` — Discard with appropriate confirmation
- `discard_all()` — Discard all with explicit warning about untracked files
- Different confirmation messages for untracked vs tracked files

**3. Key Bindings (`/src/app.rs`)**

- `d`: discard selected (with confirmation)
- `D`: discard all (with confirmation, warns about untracked deletion)

### Acceptance Criteria

- [ ] `d` on modified file shows "Discard changes? [y/N]"
- [ ] `d` on untracked file shows "Delete untracked file? [y/N]"
- [ ] `D` shows "Discard all changes and delete untracked files (N files)? [y/N]"
- [ ] Confirmed discard removes changes and refreshes
- [ ] Discard is NOT undoable
- [ ] Multi-select discard applies to all selected files

### Checkpoint

**Commit:** "feat(git): implement discard operations with confirmations"
**Verify:** `cargo test && cargo clippy`
**State:** Can discard individual and all changes with proper confirmations

---

## Phase 7: Modal System Infrastructure

### Description

Create the modal overlay system for commit, branch, and help screens.

### Deliverables

**1. Modal Types (`/src/types.rs`)**

- `ModalState` enum: `None`, `Commit(CommitModal)`, `Branch(BranchModal)`, `Help`, `Progress(String)`
- `CommitModal` struct: `title: String`, `body: String`, `focus: Field`, `amend: bool`, `error: Option<String>`
- `BranchModal` struct: `filter: String`, `branches: Vec<String>`, `selected_idx: usize`

**2. Modal Module (`/src/ui/modal.rs`)**

- `draw_modal_overlay(frame, area, content)` — Centered overlay with border
- `draw_commit_modal(frame, state)` — Commit form layout
- `draw_branch_modal(frame, state)` — Searchable list layout
- `draw_help_modal(frame)` — Keybinding reference
- `draw_progress_overlay(frame, message)` — Spinner with message

**3. App Modal State (`/src/app.rs`)**

- Add `modal: ModalState` field
- Modal-aware key handling (block global ops, route to modal handlers)
- `q` always quits, `Esc` closes modal

### Acceptance Criteria

- [ ] Modal renders as centered overlay on top of main UI
- [ ] Main UI remains visible (dimmed) behind modal
- [ ] `Esc` closes any modal
- [ ] `q` quits from any modal
- [ ] Global keys (`s`, `u`, etc.) are blocked while modal is open

### Checkpoint

**Commit:** "feat(ui): implement modal overlay system"
**Verify:** `cargo test && cargo clippy`
**State:** Modal infrastructure ready for commit/branch/help screens

---

## Phase 8: Commit Modal

### Description

Implement the commit modal with title, body, amend option, and validation.

### Deliverables

**1. Commit Modal UI (`/src/ui/modal.rs`)**

- Staged files list at top
- Title field (single line) with 50-char soft limit indicator
- Body field (multi-line text area)
- Amend checkbox/toggle
- Inline validation error display
- Controls hint: `Tab: next field | Ctrl+Enter: commit | Esc: cancel`

**2. Commit Modal Logic (`/src/app.rs`)**

- `open_commit_modal()` — Initialize modal state, pre-fill if amending
- `handle_commit_modal_input(key)` — Text input, Tab navigation, submit
- `execute_commit()` — Validate and perform commit

**3. Git Commit (`/src/git.rs`)**

- `commit(repo, title, body, amend)` — Create commit with message
- `get_last_commit_message(repo)` — For amend pre-fill

**4. Key Binding (`/src/app.rs`)**

- `c`: open commit modal

### Acceptance Criteria

- [ ] `c` opens commit modal with staged files list
- [ ] Tab moves between title and body fields
- [ ] Title shows character count indicator
- [ ] Commit blocked with empty title (shows inline error)
- [ ] Commit blocked with no staged files (shows error)
- [ ] Amend toggle pre-fills message from last commit
- [ ] `Ctrl+Enter` submits commit
- [ ] Successful commit shows flash, closes modal, refreshes
- [ ] Failed commit keeps modal open, shows error, preserves fields

### Checkpoint

**Commit:** "feat(commit): implement commit modal with validation"
**Verify:** `cargo test && cargo clippy`
**State:** Can compose and submit commits via modal

---

## Phase 9: Branch Modal

### Description

Implement the branch modal with search, switch, and create functionality.

### Deliverables

**1. Branch Modal UI (`/src/ui/modal.rs`)**

- Search/filter input at top
- Scrollable branch list with current branch indicated (`*` prefix)
- "Create new branch" option when filter doesn't match existing
- Controls hint: `↑/↓: navigate | Enter: switch/create | Esc: cancel`

**2. Branch Modal Logic (`/src/app.rs`)**

- `open_branch_modal()` — Load branches, initialize state
- `handle_branch_modal_input(key)` — Text input, navigation, selection
- `switch_branch(name)` — Check for changes, switch or show error
- `create_branch(name)` — Create and switch

**3. Git Branch Operations (`/src/git.rs`)**

- `list_local_branches(repo)` — Return Vec<String> of branch names
- `get_current_branch(repo)` — Return Option<String>
- `switch_branch(repo, name)` — Checkout existing branch
- `create_and_switch_branch(repo, name)` — Create new branch at HEAD, checkout
- `has_uncommitted_changes(repo)` — Check for staged or unstaged changes

**4. Key Binding (`/src/app.rs`)**

- `b`: open branch modal

### Acceptance Criteria

- [ ] `b` opens branch modal with list of local branches
- [ ] Typing filters branch list incrementally
- [ ] Backspace removes characters from filter
- [ ] Current branch shown with `*` prefix
- [ ] Selecting current branch shows "Already on branch X" flash
- [ ] Switch blocked if uncommitted changes exist (shows error message)
- [ ] Non-matching filter shows "Create: [name]" option
- [ ] Creating branch from detached HEAD works correctly
- [ ] Enter on branch switches to it
- [ ] Enter on "Create" option creates and switches

### Checkpoint

**Commit:** "feat(branch): implement branch modal with search and create"
**Verify:** `cargo test && cargo clippy`
**State:** Can switch and create branches via modal

---

## Phase 10: Help Overlay

### Description

Implement the full help overlay with keybinding reference.

### Deliverables

**1. Help Overlay UI (`/src/ui/modal.rs`)**

- Full-screen or large modal overlay
- Keybindings organized by category:
  - File Operations: `s`, `u`, `d`, `Space`
  - Bulk Operations: `S`, `U`, `D`
  - Commit & Branch: `c`, `b`
  - Remote Operations: `p`, `P`, `l`, `z`, `Z`
  - Navigation: `↑`, `↓`, `Enter`, `PageUp`, `PageDown`
  - General: `r`, `?`, `q`, `Esc`, `Ctrl+z`

**2. Help Toggle (`/src/app.rs`)**

- `?` toggles help overlay
- `?` or `Esc` closes help

**3. Status Bar Key Hints (`/src/ui/status_bar.rs`)**

- Add persistent key hints: `s:stage u:unstage d:discard c:commit b:branch p:push ?:help`
- Adaptive width: truncate least important hints first

### Acceptance Criteria

- [ ] `?` opens help overlay
- [ ] `?` or `Esc` closes help overlay
- [ ] All keybindings listed with descriptions
- [ ] Categories clearly separated
- [ ] Status bar shows minimal key hints when space allows
- [ ] Key hints truncate gracefully on narrow terminals

### Checkpoint

**Commit:** "feat(help): implement help overlay and key hints"
**Verify:** `cargo test && cargo clippy`
**State:** Users can discover all keybindings via ? key

---

## Phase 11: Push/Pull Operations

### Description

Implement push and pull using CLI for better credential handling.

### Deliverables

**1. Remote Operations (`/src/git.rs`)**

- `push(repo_path)` — Shell out to `git push`, handle upstream auto-set
- `force_push(repo_path)` — Shell out to `git push --force`
- `pull(repo_path)` — Shell out to `git pull`
- `has_upstream(repo)` — Check if current branch has upstream
- `has_remote_origin(repo)` — Check if origin remote exists
- `is_detached_head(repo)` — Check for detached HEAD state

**2. Progress Overlay (`/src/app.rs`)**

- Show "Pushing..." or "Pulling..." during operations
- Run operations in blocking mode (UI frozen but visible)
- `Ctrl+c` sets cancellation flag (best-effort)

**3. Pull Conflict Handling (`/src/app.rs`)**

- Detect conflicts after pull
- Show "Pull resulted in conflicts. Abort merge? [y/N]"
- `y` runs `git merge --abort`
- Other key dismisses, conflicts shown in file list

**4. Key Bindings (`/src/app.rs`)**

- `p`: push to remote
- `P`: force push (with confirmation)
- `l`: pull from remote

### Acceptance Criteria

- [ ] `p` pushes current branch
- [ ] Push auto-sets upstream if not configured
- [ ] Push from detached HEAD shows error
- [ ] Push with no origin shows error
- [ ] `P` shows force push confirmation
- [ ] `l` pulls from remote
- [ ] Pull conflicts show abort prompt
- [ ] Progress overlay shown during operations
- [ ] Auth failures show git error message

### Checkpoint

**Commit:** "feat(remote): implement push/pull operations"
**Verify:** `cargo test && cargo clippy`
**State:** Can push and pull from within TUI

---

## Phase 12: Stash Operations

### Description

Implement basic stash and stash pop operations.

### Deliverables

**1. Git Stash Operations (`/src/git.rs`)**

- `stash_all(repo)` — Stash including untracked (`git stash -u`)
- `stash_pop(repo)` — Pop latest stash (`git stash pop`)
- `has_stashes(repo)` — Check if any stashes exist

**2. App Stash Methods (`/src/app.rs`)**

- `stash()` — Stash all changes, show flash
- `pop_stash()` — Pop stash, handle conflicts

**3. Key Bindings (`/src/app.rs`)**

- `z`: stash all changes
- `Z`: pop latest stash

### Acceptance Criteria

- [ ] `z` stashes all changes including untracked
- [ ] `z` with no changes shows "Nothing to stash"
- [ ] `Z` pops latest stash
- [ ] `Z` with no stashes shows "No stashes to pop"
- [ ] Stash pop conflicts appear in file list
- [ ] Flash messages confirm stash/pop success

### Checkpoint

**Commit:** "feat(stash): implement stash and stash pop"
**Verify:** `cargo test && cargo clippy`
**State:** Can stash and pop changes with z/Z

---

## Phase 13: Edge Cases and Polish

### Description

Handle remaining edge cases and polish the user experience.

### Deliverables

**1. Edge Case Handling (`/src/app.rs`, `/src/git.rs`)**

- Stage/unstage with no files: no-op, no error
- Push with nothing to push: show "Nothing to push"
- Branch modal with detached HEAD: show "detached" state
- All error states from requirements spec

**2. UI Polish**

- Consistent color scheme for all new components
- Proper scrolling in branch modal for many branches
- Text wrapping in commit body field
- Spinner animation for progress overlay

**3. Terminal Handling**

- Ensure `Ctrl+z` doesn't suspend in raw mode
- Handle resize during modals
- Clean exit on all error paths

### Acceptance Criteria

- [ ] All edge cases from requirements spec handled correctly
- [ ] No panics on unexpected states
- [ ] Terminal restored properly on exit/error
- [ ] Resize works correctly with modals open
- [ ] Color scheme consistent with existing Catppuccin Mocha theme

### Checkpoint

**Commit:** "fix: handle edge cases and polish UI"
**Verify:** `cargo test && cargo clippy`
**State:** All edge cases handled, ready for release

---

## Dependency Graph

```
Phase 1 (Multi-Select)
    │
    ├──→ Phase 2 (Stage/Unstage) ──→ Phase 3 (Undo)
    │         │
    │         └──→ Phase 6 (Discard)
    │
    └──→ Phase 4 (Confirm Prompts) ──→ Phase 5 (Flash Messages)
                   │
                   └──→ Phase 6 (Discard)

Phase 7 (Modal System)
    │
    ├──→ Phase 8 (Commit Modal)
    │
    ├──→ Phase 9 (Branch Modal)
    │
    ├──→ Phase 10 (Help Overlay)
    │
    └──→ Phase 11 (Push/Pull) ──→ Phase 12 (Stash)

All Phases ──→ Phase 13 (Polish)
```

**Parallel Tracks:**
- Phases 1-6 (file operations) can proceed independently of Phases 7-12 (modals/remotes)
- Phases 8, 9, 10 can proceed in parallel after Phase 7
- Phase 13 depends on all others

---

## Critical Files

| File | Purpose |
|------|---------|
| `/src/app.rs` | Central state management, event loop, all key handlers |
| `/src/git.rs` | All git operations via git2 and CLI |
| `/src/types.rs` | Core types: ModalState, ConfirmPrompt, FlashMessage, UndoAction |
| `/src/ui/modal.rs` | New modal overlay rendering system |
| `/src/ui/status_bar.rs` | Prompts, flashes, key hints rendering |
| `/src/ui/file_list.rs` | Multi-select visual indicators |
| `/src/main.rs` | CLI entry point (minimal changes expected) |
