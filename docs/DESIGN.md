---
status: COMPLETED
last_updated: 2025-12-27
---

# Technical Design Document

## Overview

better-git-status is an interactive TUI git status viewer optimized for narrow terminal widths. It provides real-time visibility into repository changes with a file tree and diff preview.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         main.rs                                  │
│                    CLI parsing (clap)                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         app.rs                                   │
│              Application state & event loop                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐│
│  │  State   │  │  Events  │  │  Input   │  │  File Watcher    ││
│  │ Manager  │  │  Handler │  │  Handler │  │  (notify crate)  ││
│  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘│
└─────────────────────────────────────────────────────────────────┘
          │                              │
          ▼                              ▼
┌─────────────────────────┐    ┌─────────────────────────────────┐
│       git.rs            │    │           ui.rs                  │
│  Repository operations  │    │     UI rendering (ratatui)       │
│  - Status collection    │    │  ┌────────────────────────────┐ │
│  - Diff generation      │    │  │  status_bar.rs             │ │
│  - Branch info          │    │  │  file_list.rs              │ │
│                         │    │  │  diff_panel.rs             │ │
└─────────────────────────┘    └──┴────────────────────────────┴─┘
```

## Module Breakdown

### `main.rs`
- CLI argument parsing with clap
- Entry point calling `app::run()`
- Error formatting for user-facing messages

### `app.rs`
- `App` struct: central application state
- Event loop coordinating input and file system events
- State transitions and refresh logic

### `git.rs`
- `get_repo()`: Opens repository at current directory only (no discover)
- `get_status()`: Collects staged/unstaged/untracked files with +/- counts
  - **Staged files**: Uses `HEAD..INDEX` diff (like `git diff --cached`)
  - **Unstaged files**: Uses `INDEX..WORKTREE` diff (like `git diff`)
  - **Dual-state files**: Creates two `FileEntry` rows (one per section), each with its own +/- counts
  - **Conflicts**: Appear only in unstaged section, count toward U only (never in staged, never count toward S)
  - **Submodules**: Always a single entry (exception to dual-state rule). If a submodule has both staged and unstaged changes, show only in `[STAGED]` section with `M` status, but count toward both S and U. Status is M/A/D only (never R or C)
  - **Type changes**: Map to Modified status
  - **Untracked**: Only files (not directories), excludes ignored
- `get_diff(path, section)`: Generates unified diff for a specific file and section
  - `Section::Staged`: Returns `HEAD..INDEX` diff
  - `Section::Unstaged`: Returns `INDEX..WORKTREE` diff
  - Conflicts always return `DiffContent::Conflict` (no diff attempt)
- `get_branch_info()`: Returns branch name or detached HEAD hash

### `ui.rs`
- Main `draw()` function composing all panels
- Sub-modules for each panel component

### `watcher.rs`
- File system watcher setup (notify crate)
- Watches: working directory, `.git/index`, `.git/HEAD`
- 150ms debounce timer
- Fallback to 2s polling on watcher failure

## Data Structures

```rust
pub struct App {
    repo: Repository,
    
    // File list state
    staged_files: Vec<FileEntry>,
    unstaged_files: Vec<FileEntry>,
    
    // Navigation state
    highlight_index: Option<usize>,      // Current cursor position (flattened index)
    selected: Option<(Section, String)>, // File whose diff is shown
    file_list_scroll: usize,             // Scroll offset for file list
    
    // Diff state
    current_diff: DiffContent,
    diff_scroll: usize,
    
    // Summary counts (computed from HashSets of distinct paths)
    // - A path with both staged and unstaged changes contributes to both S and U
    // - Conflicts contribute to U only (never S)
    // - Submodules with both states contribute to both S and U (but show once in UI)
    staged_count: usize,   // S: distinct paths with staged changes
    unstaged_count: usize, // U: distinct paths with unstaged changes (incl. conflicts)
    untracked_count: usize,// ?: number of untracked files (not directories)
    
    // Branch info
    branch: BranchInfo,
}

pub struct FileEntry {
    pub path: String,
    pub status: FileStatus,           // Conflict status is authoritative (no separate flag)
    pub added_lines: Option<usize>,   // None for binary
    pub deleted_lines: Option<usize>, // None for binary
    pub is_binary: bool,              // If true, UI shows "-/-" for counts
    pub is_submodule: bool,           // If true, single-entry rules apply
}

// For navigation - flattened list of visible file rows
pub struct VisibleRow {
    pub section: Section,
    pub path: String,
    pub file_index: usize,  // Index into staged_files or unstaged_files
}

pub enum FileStatus {
    Added,    // A
    Modified, // M
    Deleted,  // D
    Renamed,  // R
    Untracked,// ?
    Conflict, // C
}

pub enum Section {
    Staged,
    Unstaged,
}

pub enum BranchInfo {
    Branch(String),
    Detached(String), // short commit hash
}

pub enum DiffContent {
    Empty,                          // No file selected
    Clean,                          // Repo is clean
    Text(Vec<DiffLine>),            // Normal diff
    Binary,                         // Binary file
    InvalidUtf8,                    // Non-UTF-8 file
    Conflict,                       // Unmerged conflict
}

pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub new_line_number: Option<usize>, // None for deleted lines
}

pub enum DiffLineKind {
    Header,   // diff --git, ---, +++
    Hunk,     // @@ ... @@
    Context,  // unchanged
    Added,    // +
    Deleted,  // -
}
```

## UI Layout

### Overall Structure

```
┌─────────────────────────────────┐
│ main S:3 U:5 ?:2                │  ← Status bar (1 row)
├─────────────────────────────────┤
│ [STAGED]                        │
│   M src/app.rs          +12/-3  │
│   A src/new.rs          +45/-0  │  ← File list panel
│ [UNSTAGED]                      │     (dynamic, max 33% height)
│ > M src/app.rs          +5/-1   │  ← Highlighted (cursor)
│ ● ? README.md                   │  ← Selected (diff shown)
├─────────────────────────────────┤
│ diff --git a/README.md ...      │
│ --- a/README.md                 │
│ +++ b/README.md                 │
│ @@ -1,3 +1,5 @@                 │  ← Diff preview panel
│  1 │ # Title                    │     (fills remaining space)
│  2 │+New line                   │
│  3 │ Existing                   │
│    │-Removed line               │
└─────────────────────────────────┘
```

### Status Bar Format

```
<branch> S:<staged> U:<unstaged> ?:<untracked>

Examples:
  main S:3 U:5 ?:2
  feature/auth S:0 U:1 ?:0
  HEAD@1a2b3c4 S:1 U:0 ?:0     ← Detached HEAD
```

### File List Panel - Normal State

```
[STAGED]
  M src/app.rs              +12/-3
  A src/new_file.rs         +45/-0
  R src/renamed.rs          +0/-0
[UNSTAGED]
> M src/app.rs              +5/-1    ← Highlighted
● M src/git.rs              +8/-2    ← Selected
  D old_file.rs             +0/-15
  ? untracked.txt
  C conflicted.rs
```

Visual indicators:
- `>` prefix: Highlighted (cursor position)
- `●` prefix: Selected (diff is shown for this file)
- Both can apply: `>●` when highlighted item is also selected

### File List Panel - Tree View Indentation

Tree view uses **visual indentation only** - no explicit directory rows.
Indentation is derived by splitting paths on `/` and indenting based on shared prefixes.

```
[STAGED]
  M   src/app.rs              +12/-3
  M   src/git.rs              +5/-0
  M     src/ui/diff.rs        +20/-8
  M     src/ui/file_list.rs   +15/-3
[UNSTAGED]
  ?   tests/integration.rs
```

Each row is a selectable file entry. Directories are never rendered as separate rows.

### Diff Panel States

**Empty State (no file selected):**
```
┌─ Diff ──────────────────────────┐
│                                 │
│   ↑/↓ navigate, Space to view  │
│            diff                 │
│                                 │
└─────────────────────────────────┘
```

**Clean Repository:**
```
┌─ Diff ──────────────────────────┐
│                                 │
│      No changes (q to quit)     │
│                                 │
└─────────────────────────────────┘
```

**Normal Diff Display:**
```
┌─ Diff ──────────────────────────┐
│diff --git a/src/app.rs b/src/..│
│--- a/src/app.rs                 │
│+++ b/src/app.rs                 │
│@@ -10,6 +10,8 @@ impl App {      │
│ 10 │    fn new() -> Self {      │
│ 11 │        Self {              │
│ 12 │+           field: value,   │
│ 13 │+           other: 42,      │
│ 14 │            existing: true, │
│    │-           removed: false, │
│ 15 │        }                   │
│ 16 │    }                       │
└─────────────────────────────────┘
```

Line number column:
- Shows new file line number for added/context lines
- Shows `-` or empty for deleted lines
- Number appears on first visual line when wrapping

**Binary File:**
```
┌─ Diff ──────────────────────────┐
│                                 │
│         Binary file             │
│                                 │
└─────────────────────────────────┘
```

**Invalid UTF-8:**
```
┌─ Diff ──────────────────────────┐
│                                 │
│  File contains invalid UTF-8   │
│           encoding              │
│                                 │
└─────────────────────────────────┘
```

**Conflict:**
```
┌─ Diff ──────────────────────────┐
│                                 │
│   Conflict - resolve before    │
│         viewing diff            │
│                                 │
└─────────────────────────────────┘
```

### Terminal Too Small

```
┌─────────────────────────────────┐
│                                 │
│      Terminal too small         │
│    (min: 30 cols × 10 rows)     │
│                                 │
└─────────────────────────────────┘
```

### Narrow Width Degradation

Width is calculated after reserving space for:
- Marker column (`>` and/or `●`): 2 chars
- Status symbol + space: 2 chars

Degradation priority (applied in order as width shrinks):

**Full width (sufficient space):**
```
>● M src/components/file.rs    +12/-3
```

**Step 1 - Drop +/- counts first:**
```
>● M src/components/file.rs
```

**Step 2 - Truncate path from start with `…/`:**
```
>● M …/components/file.rs
```

Preserve at least parent directory + filename when possible.

**Step 3 - Filename only:**
```
>● M file.rs
```

**Step 4 - Status symbol only:**
```
>● M
```

## State Management

### Flattened Row Model

Navigation uses a flattened `Vec<VisibleRow>` built from visible sections:

```rust
fn build_visible_rows(staged: &[FileEntry], unstaged: &[FileEntry]) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    // Only include section if non-empty (headers are implicit, not in this list)
    for (i, file) in staged.iter().enumerate() {
        rows.push(VisibleRow { section: Staged, path: file.path.clone(), file_index: i });
    }
    for (i, file) in unstaged.iter().enumerate() {
        rows.push(VisibleRow { section: Unstaged, path: file.path.clone(), file_index: i });
    }
    rows
}
```

`highlight_index` is an index into this flattened list.

### Highlight vs Selection Semantics

| Aspect | Highlight | Selection |
|--------|-----------|-----------|
| What it tracks | Cursor position | File whose diff is shown |
| Storage | `highlight_index: Option<usize>` (index) | `selected: Option<(Section, String)>` (identity) |
| Updated by | ↑/↓ navigation | Space/Enter only |
| Preservation on refresh | **By index** (clamp if out of bounds) | **By identity** (section, path) |
| On disappear | Clamp to nearest valid index | Clear, show empty diff |

**Note on renames**: Selection uses path identity. If a file is renamed, the old `(section, path)` will not match the new path after refresh, so selection will be cleared. This is intentional v1 behavior.

### Navigation State Machine

```
                    ┌───────────────────┐
                    │     Initial       │
                    │ highlight=Some(0) │  ← If files exist
                    │ selected=None     │
                    │ diff=Empty        │
                    └────────┬──────────┘
                             │ Space/Enter
                             ▼
                    ┌───────────────────┐
                    │     Active        │◄────┐
                    │ highlight=Some(N) │     │ ↑/↓
                    │ selected=Some(..) │─────┘
                    │ diff=Text/Binary  │
                    └────────┬──────────┘
                             │ Selected file disappears
                             ▼
                    ┌───────────────────┐
                    │    Recovery       │
                    │ highlight=clamped │
                    │ selected=None     │
                    │ diff=Empty        │
                    └───────────────────┘

Clean repo state:
  highlight=None, selected=None, diff=Clean
```

### Refresh Logic

```
File System Event
        │
        ▼
┌─────────────────────────┐
│  Debounce Timer         │
│  - Reset on each event  │
│  - Fire after 150ms     │
│    of no new events     │
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Re-run git status      │
│  (always recompute)     │
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Check if repo clean:   │
│  - If both staged and   │
│    unstaged are empty:  │
│    → highlight=None     │
│    → selected=None      │
│    → diff=Clean         │
│    → skip to re-render  │
└───────────┬─────────────┘
            │ (repo has changes)
            ▼
┌─────────────────────────┐
│  Preserve selection:    │
│  - Find (section, path) │
│    in new file list     │
│  - If not found: clear  │
│    selection, diff=Empty│
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Preserve highlight:    │
│  - Keep same index      │
│  - If past end: clamp   │
│    to last row          │
│  - If list empty: None  │
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Recompute diff only    │
│  for selected file      │
│  (if selection exists)  │
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Re-render UI           │
└─────────────────────────┘
```

## Event Handling

### Input Events

| Key | Action | State Change |
|-----|--------|--------------|
| ↑ | Move highlight up | `highlight_index -= 1` (clamp) |
| ↓ | Move highlight down | `highlight_index += 1` (clamp) |
| Space/Enter | Select file | `selected = current highlight`, update diff |
| Page Up | Scroll diff up | `diff_scroll -= viewport_height` |
| Page Down | Scroll diff down | `diff_scroll += viewport_height` |
| q | Quit | Exit application |

### File System Events

- Debounce window: 150ms
- Watched paths:
  - Working directory (recursive)
  - `.git/index`
  - `.git/HEAD`

## Error Handling

| Condition | Action |
|-----------|--------|
| `Repository::open(".")` fails | Exit: "Not a git repository" |
| Bare repository detected | Exit: "Repository has no working directory" |
| Terminal < 30×10 | Show "Terminal too small", wait for resize |
| File watcher init fails | Log warning, use 2s polling |
| Repo disappears during poll | Exit: "Not a git repository" |
| Diff generation fails | Show error message in diff panel |

## Color Scheme (Catppuccin Mocha)

```rust
// Status colors
const GREEN: Color = Color::Rgb(166, 227, 161);   // Added
const RED: Color = Color::Rgb(243, 139, 168);     // Deleted
const YELLOW: Color = Color::Rgb(249, 226, 175);  // Modified
const BLUE: Color = Color::Rgb(137, 180, 250);    // Renamed
const GRAY: Color = Color::Rgb(147, 153, 178);    // Untracked
const MAGENTA: Color = Color::Rgb(245, 194, 231); // Conflict

// UI colors
const CYAN: Color = Color::Rgb(148, 226, 213);    // Headers, hunks
const TEXT: Color = Color::Rgb(205, 214, 244);    // Default text
const SURFACE: Color = Color::Rgb(49, 50, 68);    // Backgrounds
const OVERLAY: Color = Color::Rgb(108, 112, 134); // Borders
```

## File Structure

```
src/
├── main.rs           # Entry point, CLI parsing
├── app.rs            # Application state, event loop
├── git.rs            # Git operations (status, diff, branch)
├── watcher.rs        # File system watcher
├── ui/
│   ├── mod.rs        # Main draw function, layout
│   ├── status_bar.rs # Status bar component
│   ├── file_list.rs  # File list panel
│   ├── diff_panel.rs # Diff preview panel
│   └── colors.rs     # Catppuccin Mocha palette
└── types.rs          # Shared type definitions
```

## Implementation Order

1. **Phase 1: Core Git Operations**
   - Repository opening with `Repository::open(".")` (no discover)
   - Bare repository detection
   - Status collection with correct semantics:
     - HEAD..INDEX for staged, INDEX..WORKTREE for unstaged
     - Dual-state files create two entries
     - Conflicts only in unstaged, count toward U only
     - Submodules as single entries (M/A/D only)
     - Type changes mapped to Modified
     - Untracked files only (not directories)
   - Counts using HashSets: `staged_paths`, `unstaged_paths`, `untracked_files`
   - Basic diff generation with section parameter

2. **Phase 2: UI Foundation**
   - Layout with three rows (status bar, file list, diff panel)
   - Status bar with branch and counts
   - File list with section headers (hidden when empty)
   - Basic diff display with syntax coloring

3. **Phase 3: Navigation**
   - Flattened row model for navigation
   - Highlight (by index) vs selected (by identity) separation
   - Keyboard navigation (↑/↓ moves highlight, Space/Enter selects)
   - Diff scrolling with Page Up/Down

4. **Phase 4: Polish**
   - Tree view visual indentation (no directory rows)
   - +/- line counts (show `-/-` for binary)
   - Path truncation with graceful degradation
   - Catppuccin Mocha color scheme

5. **Phase 5: File Watching**
   - notify crate integration
   - Watch: working dir, `.git/index`, `.git/HEAD`
   - Debounce timer (reset on event, fire after 150ms idle)
   - State preservation: selection by identity, highlight by index
   - Only recompute diff for selected file
   - 2s polling fallback on watcher failure

6. **Phase 6: Edge Cases**
   - Terminal size handling (min 30×10)
   - Binary file detection → show "Binary file", counts show `-/-`
   - Non-UTF-8 detection → show encoding error message
   - Conflict detection → show conflict message (no diff)
   - Repo disappears during poll → exit with error
