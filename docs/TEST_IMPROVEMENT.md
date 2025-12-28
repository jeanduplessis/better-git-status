---
status: COMPLETED
last_updated: 2025-12-28
---

# Test Improvement Plan

## Overview

This document outlines a comprehensive testing strategy to improve reliability of the better-git-status TUI application. The plan is organized by priority and effort level.

## Current Test Coverage

Existing tests only cover utility functions:

| Module | Functions Tested |
|--------|------------------|
| `types.rs` | `FileStatus::symbol`, `BranchInfo::Display` |
| `file_list.rs` | `format_line_counts`, `calculate_height`, `format_path_with_priority` |
| `diff_panel.rs` | `max_scroll` |

**Gaps**: No tests for application state management, git operations, UI rendering, or user interactions.

---

## Priority 1: High-Value Unit Tests

**Effort**: Small (1-3 hours)  
**Impact**: High - covers core logic without external dependencies

### 1.1 Application State Logic (`app.rs`)

#### `build_visible_rows`

```rust
#[test]
fn build_visible_rows_staged_only() {
    let staged = vec![file_entry("a.rs"), file_entry("b.rs")];
    let unstaged = vec![];
    let rows = build_visible_rows(&staged, &unstaged);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.section == Section::Staged));
}

#[test]
fn build_visible_rows_both_sections() {
    let staged = vec![file_entry("a.rs")];
    let unstaged = vec![file_entry("b.rs")];
    let rows = build_visible_rows(&staged, &unstaged);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].section, Section::Staged);
    assert_eq!(rows[1].section, Section::Unstaged);
}

#[test]
fn build_visible_rows_empty() {
    let rows = build_visible_rows(&[], &[]);
    assert!(rows.is_empty());
}
```

#### `count_headers_before`

Test cases:
- Index 0 with staged files → 1 header
- Index in unstaged section → 2 headers
- Only unstaged files → 1 header

#### `move_highlight`

Test cases:
- Move down from 0 → 1
- Move up from 0 → stays 0 (clamp)
- Move down past end → clamps to last
- Empty `visible_rows` → no panic, stays None

#### `scroll_diff` and `page_scroll_diff`

Test cases:
- Negative scroll clamps to 0
- Scroll beyond max clamps to max
- Page scroll uses viewport height correctly

#### `click_file_list`

Test cases:
- Click on `[STAGED]` header → no-op
- Click on `[UNSTAGED]` header → no-op
- Click on first staged file → `highlight_index = Some(0)`
- Click on first unstaged file (after staged) → correct index
- Click beyond file list → no-op

### 1.2 Git Status Helpers (`git.rs`)

These can be tested without a repository by constructing `git2::Status` flags directly:

```rust
use git2::Status;

#[test]
fn has_staged_changes_index_new() {
    let status = Status::INDEX_NEW;
    assert!(has_staged_changes(status));
    assert!(!has_unstaged_changes(status));
}

#[test]
fn has_staged_changes_index_modified() {
    let status = Status::INDEX_MODIFIED;
    assert!(has_staged_changes(status));
}

#[test]
fn has_unstaged_changes_wt_modified() {
    let status = Status::WT_MODIFIED;
    assert!(!has_staged_changes(status));
    assert!(has_unstaged_changes(status));
}

#[test]
fn get_staged_status_returns_correct_type() {
    assert_eq!(get_staged_status(Status::INDEX_NEW), FileStatus::Added);
    assert_eq!(get_staged_status(Status::INDEX_DELETED), FileStatus::Deleted);
    assert_eq!(get_staged_status(Status::INDEX_RENAMED), FileStatus::Renamed);
    assert_eq!(get_staged_status(Status::INDEX_MODIFIED), FileStatus::Modified);
}

#[test]
fn get_unstaged_status_returns_correct_type() {
    assert_eq!(get_unstaged_status(Status::WT_DELETED), FileStatus::Deleted);
    assert_eq!(get_unstaged_status(Status::WT_RENAMED), FileStatus::Renamed);
    assert_eq!(get_unstaged_status(Status::WT_MODIFIED), FileStatus::Modified);
}
```

---

## Priority 2: UI Buffer Tests

**Effort**: Medium (2-4 hours)  
**Impact**: High - ensures UI renders correctly without real terminal

### 2.1 Setup

Use ratatui's `TestBackend` for in-memory rendering:

```rust
use ratatui::{backend::TestBackend, Terminal};

fn render_to_buffer(width: u16, height: u16, app: &mut App) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn buffer_contains(buffer: &Buffer, text: &str) -> bool {
    let content: String = (0..buffer.area.height)
        .flat_map(|y| {
            (0..buffer.area.width)
                .map(move |x| buffer.get(x, y).symbol().to_string())
        })
        .collect();
    content.contains(text)
}
```

### 2.2 Layout Tests (`ui/mod.rs`)

#### Terminal Too Small

```rust
#[test]
fn draw_too_small_shows_message() {
    let mut app = create_test_app_with_files();
    let buffer = render_to_buffer(20, 5, &mut app);
    assert!(buffer_contains(&buffer, "Terminal too small"));
}
```

#### Normal Layout

```rust
#[test]
fn draw_sets_layout_areas() {
    let mut app = create_test_app_with_files();
    let _ = render_to_buffer(80, 24, &mut app);
    
    assert!(app.file_list_area.height > 0);
    assert!(app.diff_area.height > 0);
    assert!(app.file_list_height > 0);
}
```

### 2.3 File List Tests (`ui/file_list.rs`)

#### Section Headers

```rust
#[test]
fn file_list_shows_staged_header() {
    let staged = vec![test_file_entry("file.rs", FileStatus::Modified)];
    let buffer = render_file_list(&staged, &[], None, None, 0, 80, 10);
    assert!(buffer_contains(&buffer, "[STAGED]"));
}

#[test]
fn file_list_shows_unstaged_header() {
    let unstaged = vec![test_file_entry("file.rs", FileStatus::Modified)];
    let buffer = render_file_list(&[], &unstaged, None, None, 0, 80, 10);
    assert!(buffer_contains(&buffer, "[UNSTAGED]"));
}
```

#### Highlight and Selection Indicators

```rust
#[test]
fn file_list_shows_highlight_indicator() {
    let staged = vec![test_file_entry("file.rs", FileStatus::Modified)];
    let buffer = render_file_list(&staged, &[], Some(0), None, 0, 80, 10);
    assert!(buffer_contains(&buffer, ">"));
}

#[test]
fn file_list_shows_selection_indicator() {
    let staged = vec![test_file_entry("file.rs", FileStatus::Modified)];
    let selected = Some((Section::Staged, "file.rs".to_string()));
    let buffer = render_file_list(&staged, &[], None, selected.as_ref(), 0, 80, 10);
    assert!(buffer_contains(&buffer, "●"));
}
```

#### Rename Display

```rust
#[test]
fn file_list_shows_rename_arrow() {
    let mut entry = test_file_entry("new.rs", FileStatus::Renamed);
    entry.old_path = Some("old.rs".to_string());
    let staged = vec![entry];
    let buffer = render_file_list(&staged, &[], None, None, 0, 80, 10);
    assert!(buffer_contains(&buffer, "old.rs → new.rs"));
}
```

### 2.4 Diff Panel Tests (`ui/diff_panel.rs`)

#### Placeholder Messages

```rust
#[test]
fn diff_panel_empty_shows_hint() {
    let buffer = render_diff_panel(&DiffContent::Empty, 0, 80, 20);
    assert!(buffer_contains(&buffer, "navigate"));
}

#[test]
fn diff_panel_clean_shows_message() {
    let buffer = render_diff_panel(&DiffContent::Clean, 0, 80, 20);
    assert!(buffer_contains(&buffer, "No changes"));
}

#[test]
fn diff_panel_binary_shows_message() {
    let buffer = render_diff_panel(&DiffContent::Binary, 0, 80, 20);
    assert!(buffer_contains(&buffer, "Binary file"));
}

#[test]
fn diff_panel_conflict_shows_message() {
    let buffer = render_diff_panel(&DiffContent::Conflict, 0, 80, 20);
    assert!(buffer_contains(&buffer, "Conflict"));
}
```

#### Line Numbers and Prefixes

```rust
#[test]
fn diff_panel_shows_added_line_prefix() {
    let lines = vec![DiffLine {
        kind: DiffLineKind::Added,
        content: "new line".to_string(),
        new_line_number: Some(1),
    }];
    let buffer = render_diff_panel(&DiffContent::Text(lines), 0, 80, 20);
    assert!(buffer_contains(&buffer, "+"));
}

#[test]
fn diff_panel_shows_deleted_line_prefix() {
    let lines = vec![DiffLine {
        kind: DiffLineKind::Deleted,
        content: "old line".to_string(),
        new_line_number: None,
    }];
    let buffer = render_diff_panel(&DiffContent::Text(lines), 0, 80, 20);
    assert!(buffer_contains(&buffer, "-"));
}
```

---

## Priority 3: Integration Tests with Temp Repos

**Effort**: Medium-Large (3-8 hours)  
**Impact**: High - validates real git behavior

### 3.1 Setup

Add dev dependency:

```toml
[dev-dependencies]
tempfile = "3"
```

Create test helpers:

```rust
use tempfile::TempDir;
use git2::{Repository, Signature};
use std::fs;

struct TestRepo {
    dir: TempDir,
    repo: Repository,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        Self { dir, repo }
    }

    fn path(&self) -> &str {
        self.dir.path().to_str().unwrap()
    }

    fn write_file(&self, name: &str, content: &str) {
        let path = self.dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn stage(&self, path: &str) {
        let mut index = self.repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();
    }

    fn commit(&self, message: &str) {
        let mut index = self.repo.index().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = self.repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test", "test@test.com").unwrap();
        
        let parent = self.repo.head().ok()
            .and_then(|h| h.peel_to_commit().ok());
        
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        
        self.repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            message,
            &tree,
            &parents,
        ).unwrap();
    }
}
```

### 3.2 Status Tests

#### Untracked File

```rust
#[test]
fn get_status_untracked_file() {
    let test_repo = TestRepo::new();
    test_repo.write_file("new.txt", "content\n");

    let status = get_status(&test_repo.repo).unwrap();

    assert_eq!(status.unstaged_files.len(), 1);
    assert_eq!(status.unstaged_files[0].status, FileStatus::Untracked);
    assert_eq!(status.unstaged_files[0].path, "new.txt");
    assert_eq!(status.untracked_count, 1);
    assert!(status.staged_files.is_empty());
}
```

#### Modified Unstaged

```rust
#[test]
fn get_status_modified_unstaged() {
    let test_repo = TestRepo::new();
    test_repo.write_file("file.txt", "original\n");
    test_repo.stage("file.txt");
    test_repo.commit("initial");
    test_repo.write_file("file.txt", "modified\n");

    let status = get_status(&test_repo.repo).unwrap();

    assert_eq!(status.unstaged_files.len(), 1);
    assert_eq!(status.unstaged_files[0].status, FileStatus::Modified);
    assert!(status.staged_files.is_empty());
}
```

#### Modified Staged

```rust
#[test]
fn get_status_modified_staged() {
    let test_repo = TestRepo::new();
    test_repo.write_file("file.txt", "original\n");
    test_repo.stage("file.txt");
    test_repo.commit("initial");
    test_repo.write_file("file.txt", "modified\n");
    test_repo.stage("file.txt");

    let status = get_status(&test_repo.repo).unwrap();

    assert_eq!(status.staged_files.len(), 1);
    assert_eq!(status.staged_files[0].status, FileStatus::Modified);
    assert!(status.unstaged_files.is_empty());
}
```

#### Renamed File

```rust
#[test]
fn get_status_renamed_file() {
    let test_repo = TestRepo::new();
    test_repo.write_file("old.txt", "content\n");
    test_repo.stage("old.txt");
    test_repo.commit("initial");
    
    fs::rename(
        test_repo.dir.path().join("old.txt"),
        test_repo.dir.path().join("new.txt"),
    ).unwrap();
    
    let mut index = test_repo.repo.index().unwrap();
    index.remove_path(Path::new("old.txt")).unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    let status = get_status(&test_repo.repo).unwrap();

    assert_eq!(status.staged_files.len(), 1);
    assert_eq!(status.staged_files[0].status, FileStatus::Renamed);
    assert_eq!(status.staged_files[0].path, "new.txt");
    assert_eq!(status.staged_files[0].old_path, Some("old.txt".to_string()));
}
```

#### Binary File

```rust
#[test]
fn get_status_binary_file() {
    let test_repo = TestRepo::new();
    // Write binary content (contains null byte)
    fs::write(
        test_repo.dir.path().join("binary.bin"),
        &[0x00, 0x01, 0x02, 0x03],
    ).unwrap();

    let status = get_status(&test_repo.repo).unwrap();

    assert_eq!(status.unstaged_files.len(), 1);
    assert!(status.unstaged_files[0].is_binary);
}
```

### 3.3 Diff Tests

#### Text Diff Staged

```rust
#[test]
fn get_diff_staged_shows_changes() {
    let test_repo = TestRepo::new();
    test_repo.write_file("file.txt", "line1\n");
    test_repo.stage("file.txt");
    test_repo.commit("initial");
    test_repo.write_file("file.txt", "line1\nline2\n");
    test_repo.stage("file.txt");

    let diff = get_diff(&test_repo.repo, "file.txt", None, Section::Staged);

    match diff {
        DiffContent::Text(lines) => {
            assert!(lines.iter().any(|l| l.kind == DiffLineKind::Added));
            assert!(lines.iter().any(|l| l.content.contains("line2")));
        }
        _ => panic!("Expected Text diff"),
    }
}
```

#### Untracked Diff

```rust
#[test]
fn get_untracked_diff_shows_all_lines() {
    let test_repo = TestRepo::new();
    test_repo.write_file("new.txt", "line1\nline2\nline3\n");

    let diff = get_untracked_diff(&test_repo.repo, "new.txt");

    match diff {
        DiffContent::Text(lines) => {
            let added_count = lines.iter()
                .filter(|l| l.kind == DiffLineKind::Added)
                .count();
            assert_eq!(added_count, 3);
        }
        _ => panic!("Expected Text diff"),
    }
}
```

### 3.4 Branch Info Tests

```rust
#[test]
fn get_branch_info_on_branch() {
    let test_repo = TestRepo::new();
    test_repo.write_file("file.txt", "content\n");
    test_repo.stage("file.txt");
    test_repo.commit("initial");

    let info = get_branch_info(&test_repo.repo);

    match info {
        BranchInfo::Branch(name) => {
            assert!(name == "main" || name == "master");
        }
        _ => panic!("Expected Branch"),
    }
}

#[test]
fn get_branch_info_detached() {
    let test_repo = TestRepo::new();
    test_repo.write_file("file.txt", "content\n");
    test_repo.stage("file.txt");
    test_repo.commit("initial");
    
    let head = test_repo.repo.head().unwrap();
    let oid = head.target().unwrap();
    test_repo.repo.set_head_detached(oid).unwrap();

    let info = get_branch_info(&test_repo.repo);

    match info {
        BranchInfo::Detached(hash) => {
            assert_eq!(hash.len(), 7);
        }
        _ => panic!("Expected Detached"),
    }
}
```

---

## Recommended Dependencies

Add to `Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
insta = "1"  # Optional: for snapshot testing
```

---

## Implementation Order

1. **Week 1**: Priority 1 unit tests
   - Status helpers in `git.rs`
   - Navigation functions in `app.rs`
   
2. **Week 2**: Priority 2 UI tests
   - Set up `TestBackend` helpers
   - File list and diff panel rendering
   
3. **Week 3**: Priority 3 integration tests
   - Temp repo helpers
   - Status and diff scenarios

---

## Success Metrics

- [ ] All Priority 1 tests passing
- [ ] All Priority 2 tests passing  
- [ ] All Priority 3 tests passing
- [ ] No regressions when refactoring
- [ ] New features include corresponding tests

---

## Optional Enhancements

### Snapshot Testing with `insta`

For UI tests, consider using `insta` to capture and compare rendered buffers:

```rust
#[test]
fn snapshot_file_list_with_changes() {
    let buffer = render_file_list_scenario();
    let snapshot = buffer_to_string(&buffer);
    insta::assert_snapshot!(snapshot);
}
```

### Property-Based Testing

Use `proptest` for edge cases in path truncation:

```rust
proptest! {
    #[test]
    fn format_path_never_panics(path in ".*", width in 0..200usize) {
        let (result, _) = format_path_with_priority(&path, "", width);
        assert!(result.chars().count() <= width.max(1));
    }
}
```

### Refactoring for Testability

Extract event handling from `run_app` into testable methods:

```rust
impl App {
    pub fn handle_key(&mut self, key: KeyCode) -> AppAction {
        match key {
            KeyCode::Down => { self.move_highlight(1); AppAction::Continue }
            KeyCode::Char('q') => AppAction::Quit,
            // ...
        }
    }
}
```

This allows testing keyboard interactions without a terminal.
