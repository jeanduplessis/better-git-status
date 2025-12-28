use crate::types::{
    BranchInfo, DiffContent, DiffLine, DiffLineKind, FileEntry, FileStatus, Section,
};
use anyhow::{bail, Context, Result};
use git2::{DiffOptions, Repository, Status, StatusOptions};
use std::collections::HashSet;

pub fn get_repo(path: &str) -> Result<Repository> {
    let repo = Repository::open(path).context("Not a git repository")?;
    if repo.is_bare() {
        bail!("Repository has no working directory");
    }
    Ok(repo)
}

pub fn get_branch_info(repo: &Repository) -> BranchInfo {
    if let Ok(head) = repo.head() {
        if head.is_branch() {
            if let Some(name) = head.shorthand() {
                return BranchInfo::Branch(name.to_string());
            }
        }
        if let Some(oid) = head.target() {
            let oid_str = oid.to_string();
            let len = 7.min(oid_str.len());
            return BranchInfo::Detached(oid_str[..len].to_string());
        }
    }
    BranchInfo::Detached("unknown".to_string())
}

pub struct StatusResult {
    pub staged_files: Vec<FileEntry>,
    pub unstaged_files: Vec<FileEntry>,
    pub staged_count: usize,
    pub unstaged_count: usize,
    pub untracked_count: usize,
}

pub fn get_status(repo: &Repository) -> Result<StatusResult> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .include_unreadable(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .renames_from_rewrites(true);

    let statuses = repo.statuses(Some(&mut opts))?;

    let mut staged_files = Vec::new();
    let mut unstaged_files = Vec::new();
    let mut staged_paths = HashSet::new();
    let mut unstaged_paths = HashSet::new();
    let mut untracked_files = HashSet::new();

    for entry in statuses.iter() {
        let Some(raw_path) = entry.path() else {
            continue;
        };
        let status = entry.status();

        let (staged_path, staged_old_path) = if status.is_index_renamed() {
            if let Some(delta) = entry.head_to_index() {
                let new_path = delta
                    .new_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| raw_path.to_string());
                let old_path = delta
                    .old_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string());
                (new_path, old_path)
            } else {
                (raw_path.to_string(), None)
            }
        } else {
            (raw_path.to_string(), None)
        };

        let (unstaged_path, unstaged_old_path) = if status.is_wt_renamed() {
            if let Some(delta) = entry.index_to_workdir() {
                let new_path = delta
                    .new_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| raw_path.to_string());
                let old_path = delta
                    .old_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string());
                (new_path, old_path)
            } else {
                (raw_path.to_string(), None)
            }
        } else {
            (raw_path.to_string(), None)
        };

        let path = raw_path.to_string();

        let is_conflict = status.is_conflicted();
        let is_submodule = status.is_index_typechange() || status.is_wt_typechange();

        if is_conflict {
            unstaged_paths.insert(path.clone());
            let entry = FileEntry {
                path,
                old_path: None,
                status: FileStatus::Conflict,
                added_lines: None,
                deleted_lines: None,
                is_binary: false,
                is_submodule: false,
            };
            unstaged_files.push(entry);
            continue;
        }

        let has_staged = has_staged_changes(status);
        let has_unstaged = has_unstaged_changes(status);
        let is_untracked = status.is_wt_new();

        if is_untracked {
            untracked_files.insert(path.clone());
            unstaged_paths.insert(path.clone());
            let (added, is_binary) = count_lines_in_workdir(repo, &path);
            let entry = FileEntry {
                path,
                old_path: None,
                status: FileStatus::Untracked,
                added_lines: Some(added),
                deleted_lines: Some(0),
                is_binary,
                is_submodule: false,
            };
            unstaged_files.push(entry);
            continue;
        }

        if is_submodule {
            if has_staged || has_unstaged {
                staged_paths.insert(path.clone());
                if has_unstaged {
                    unstaged_paths.insert(path.clone());
                }
                let file_status = get_staged_status(status);
                let (added, deleted, is_binary) =
                    get_line_counts_for_section(repo, &path, Section::Staged);
                staged_files.push(FileEntry {
                    path,
                    old_path: staged_old_path,
                    status: file_status,
                    added_lines: added,
                    deleted_lines: deleted,
                    is_binary,
                    is_submodule: true,
                });
            }
            continue;
        }

        if has_staged {
            staged_paths.insert(staged_path.clone());
            let file_status = get_staged_status(status);
            let (added, deleted, is_binary) =
                get_line_counts_for_section(repo, &staged_path, Section::Staged);
            staged_files.push(FileEntry {
                path: staged_path,
                old_path: staged_old_path,
                status: file_status,
                added_lines: added,
                deleted_lines: deleted,
                is_binary,
                is_submodule: false,
            });
        }

        if has_unstaged {
            unstaged_paths.insert(unstaged_path.clone());
            let file_status = get_unstaged_status(status);
            let (added, deleted, is_binary) =
                get_line_counts_for_section(repo, &unstaged_path, Section::Unstaged);
            unstaged_files.push(FileEntry {
                path: unstaged_path,
                old_path: unstaged_old_path,
                status: file_status,
                added_lines: added,
                deleted_lines: deleted,
                is_binary,
                is_submodule: false,
            });
        }
    }

    staged_files.sort_by(|a, b| a.path.cmp(&b.path));
    unstaged_files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(StatusResult {
        staged_files,
        unstaged_files,
        staged_count: staged_paths.len(),
        unstaged_count: unstaged_paths.len(),
        untracked_count: untracked_files.len(),
    })
}

pub(crate) fn has_staged_changes(status: Status) -> bool {
    status.is_index_new()
        || status.is_index_modified()
        || status.is_index_deleted()
        || status.is_index_renamed()
        || status.is_index_typechange()
}

pub(crate) fn has_unstaged_changes(status: Status) -> bool {
    status.is_wt_modified()
        || status.is_wt_deleted()
        || status.is_wt_renamed()
        || status.is_wt_typechange()
}

pub(crate) fn get_staged_status(status: Status) -> FileStatus {
    if status.is_index_new() {
        FileStatus::Added
    } else if status.is_index_deleted() {
        FileStatus::Deleted
    } else if status.is_index_renamed() {
        FileStatus::Renamed
    } else {
        FileStatus::Modified
    }
}

pub(crate) fn get_unstaged_status(status: Status) -> FileStatus {
    if status.is_wt_deleted() {
        FileStatus::Deleted
    } else if status.is_wt_renamed() {
        FileStatus::Renamed
    } else {
        FileStatus::Modified
    }
}

fn count_lines_in_workdir(repo: &Repository, path: &str) -> (usize, bool) {
    let workdir = match repo.workdir() {
        Some(w) => w,
        None => return (0, false),
    };
    let file_path = workdir.join(path);
    let content = match std::fs::read(&file_path) {
        Ok(c) => c,
        Err(_) => return (0, false),
    };

    if content.contains(&0) {
        return (0, true);
    }

    let text = match String::from_utf8(content) {
        Ok(t) => t,
        Err(_) => return (0, false),
    };

    let line_count = text.lines().count();
    (line_count, false)
}

fn get_line_counts_for_section(
    repo: &Repository,
    path: &str,
    section: Section,
) -> (Option<usize>, Option<usize>, bool) {
    let mut opts = DiffOptions::new();
    opts.pathspec(path);

    let diff_result = match section {
        Section::Staged => {
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
        }
        Section::Unstaged => repo.diff_index_to_workdir(None, Some(&mut opts)),
    };

    let diff = match diff_result {
        Ok(d) => d,
        Err(_) => return (None, None, false),
    };

    let mut is_binary = false;

    for delta_idx in 0..diff.deltas().len() {
        if let Some(delta) = diff.get_delta(delta_idx) {
            if delta.flags().is_binary() {
                is_binary = true;
            }
        }
    }

    if is_binary {
        return (None, None, true);
    }

    let stats = match diff.stats() {
        Ok(s) => s,
        Err(_) => return (Some(0), Some(0), false),
    };

    let added = stats.insertions();
    let deleted = stats.deletions();

    (Some(added), Some(deleted), false)
}

pub fn get_diff(
    repo: &Repository,
    path: &str,
    old_path: Option<&str>,
    section: Section,
) -> DiffContent {
    let mut opts = DiffOptions::new();
    opts.pathspec(path);
    if let Some(old) = old_path {
        opts.pathspec(old);
    }

    let diff_result = match section {
        Section::Staged => {
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
        }
        Section::Unstaged => repo.diff_index_to_workdir(None, Some(&mut opts)),
    };

    let diff = match diff_result {
        Ok(d) => d,
        Err(_) => return DiffContent::Empty,
    };

    for delta_idx in 0..diff.deltas().len() {
        if let Some(delta) = diff.get_delta(delta_idx) {
            if delta.flags().is_binary() {
                return DiffContent::Binary;
            }
        }
    }

    let mut lines = Vec::new();
    let mut current_new_line: Option<usize> = None;
    let mut has_invalid_utf8 = false;

    let result = diff.print(git2::DiffFormat::Patch, |_delta, hunk, line| {
        let raw_content = match std::str::from_utf8(line.content()) {
            Ok(s) => s,
            Err(_) => {
                has_invalid_utf8 = true;
                return false;
            }
        };

        match line.origin() {
            'F' => {
                for line_str in raw_content.lines() {
                    let kind = if line_str.starts_with("@@") {
                        DiffLineKind::Hunk
                    } else {
                        DiffLineKind::Header
                    };
                    lines.push(DiffLine {
                        kind,
                        content: line_str.to_string(),
                        new_line_number: None,
                    });
                }
            }
            'H' => {
                let content = raw_content.trim_end_matches('\n').to_string();
                if let Some(h) = hunk {
                    current_new_line = Some(h.new_start() as usize);
                }
                lines.push(DiffLine {
                    kind: DiffLineKind::Hunk,
                    content,
                    new_line_number: None,
                });
            }
            '+' => {
                let content = raw_content.trim_end_matches('\n').to_string();
                let ln = current_new_line;
                if let Some(ref mut n) = current_new_line {
                    *n += 1;
                }
                lines.push(DiffLine {
                    kind: DiffLineKind::Added,
                    content,
                    new_line_number: ln,
                });
            }
            '-' => {
                let content = raw_content.trim_end_matches('\n').to_string();
                lines.push(DiffLine {
                    kind: DiffLineKind::Deleted,
                    content,
                    new_line_number: None,
                });
            }
            ' ' => {
                let content = raw_content.trim_end_matches('\n').to_string();
                let ln = current_new_line;
                if let Some(ref mut n) = current_new_line {
                    *n += 1;
                }
                lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    content,
                    new_line_number: ln,
                });
            }
            _ => {
                let content = raw_content.trim_end_matches('\n').to_string();
                lines.push(DiffLine {
                    kind: DiffLineKind::Header,
                    content,
                    new_line_number: None,
                });
            }
        }
        true
    });

    if has_invalid_utf8 {
        return DiffContent::InvalidUtf8;
    }

    if result.is_err() {
        return DiffContent::Empty;
    }

    if lines.is_empty() {
        DiffContent::Empty
    } else {
        DiffContent::Text(lines)
    }
}

/// Stage files by adding them to the index.
///
/// Handles regular files (add to index) and deleted files (remove from index).
///
/// NOTE: Renamed files are handled on a best-effort basis. This function operates
/// on individual paths and does not automatically handle the old_path of a rename.
/// For full rename support, the caller should stage both the removal of the old path
/// and addition of the new path. See Phase 13 for potential improvements.
pub fn stage_files(repo: &Repository, paths: &[String]) -> Result<()> {
    let mut index = repo.index().context("Failed to get repository index")?;
    let workdir = repo.workdir().context("Repository has no working directory")?;

    for path in paths {
        let full_path = workdir.join(path);

        if full_path.exists() {
            index
                .add_path(std::path::Path::new(path))
                .with_context(|| format!("Failed to stage file: {}", path))?;
        } else {
            index
                .remove_path(std::path::Path::new(path))
                .with_context(|| format!("Failed to stage deleted file: {}", path))?;
        }
    }

    index.write().context("Failed to write index")?;
    Ok(())
}

/// Unstage files by resetting the index to HEAD.
///
/// For files that exist in HEAD, restores them to the HEAD version.
/// For files that don't exist in HEAD (new files), removes them from the index.
///
/// NOTE: Renamed files are handled on a best-effort basis. This function operates
/// on individual paths and does not automatically restore the old_path of a rename.
/// See Phase 13 for potential improvements.
pub fn unstage_files(repo: &Repository, paths: &[String]) -> Result<()> {
    let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let head_tree = head.as_ref().and_then(|c| c.tree().ok());

    let mut index = repo.index().context("Failed to get repository index")?;

    for path in paths {
        let path_obj = std::path::Path::new(path);

        if let Some(ref tree) = head_tree {
            if let Ok(entry) = tree.get_path(path_obj) {
                let blob = repo
                    .find_blob(entry.id())
                    .context("Failed to find blob for path")?;
                let entry = git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: entry.filemode() as u32,
                    uid: 0,
                    gid: 0,
                    file_size: blob.content().len() as u32,
                    id: entry.id(),
                    flags: 0,
                    flags_extended: 0,
                    path: path.as_bytes().to_vec(),
                };
                index
                    .add(&entry)
                    .with_context(|| format!("Failed to reset file: {}", path))?;
            } else {
                index
                    .remove_path(path_obj)
                    .with_context(|| format!("Failed to remove new file from index: {}", path))?;
            }
        } else {
            index
                .remove_path(path_obj)
                .with_context(|| format!("Failed to remove file from index: {}", path))?;
        }
    }

    index.write().context("Failed to write index")?;
    Ok(())
}

pub fn stage_all(repo: &Repository) -> Result<Vec<String>> {
    let status = get_status(repo)?;
    let paths: Vec<String> = status.unstaged_files.into_iter().map(|f| f.path).collect();
    if !paths.is_empty() {
        stage_files(repo, &paths)?;
    }
    Ok(paths)
}

pub fn unstage_all(repo: &Repository) -> Result<Vec<String>> {
    let status = get_status(repo)?;
    let paths: Vec<String> = status.staged_files.into_iter().map(|f| f.path).collect();
    if !paths.is_empty() {
        unstage_files(repo, &paths)?;
    }
    Ok(paths)
}

pub fn get_untracked_diff(repo: &Repository, path: &str) -> DiffContent {
    let workdir = match repo.workdir() {
        Some(w) => w,
        None => return DiffContent::Empty,
    };

    let file_path = workdir.join(path);
    let content = match std::fs::read(&file_path) {
        Ok(c) => c,
        Err(_) => return DiffContent::Empty,
    };

    let text = match std::str::from_utf8(&content) {
        Ok(t) => t,
        Err(_) => return DiffContent::InvalidUtf8,
    };

    let mut lines = Vec::new();

    lines.push(DiffLine {
        kind: DiffLineKind::Header,
        content: format!("diff --git a/{} b/{}", path, path),
        new_line_number: None,
    });
    lines.push(DiffLine {
        kind: DiffLineKind::Header,
        content: "new file".to_string(),
        new_line_number: None,
    });
    lines.push(DiffLine {
        kind: DiffLineKind::Header,
        content: "--- /dev/null".to_string(),
        new_line_number: None,
    });
    lines.push(DiffLine {
        kind: DiffLineKind::Header,
        content: format!("+++ b/{}", path),
        new_line_number: None,
    });

    let text_lines: Vec<&str> = text.lines().collect();
    let line_count = text_lines.len();

    if line_count > 0 {
        lines.push(DiffLine {
            kind: DiffLineKind::Hunk,
            content: format!("@@ -0,0 +1,{} @@", line_count),
            new_line_number: None,
        });

        for (i, line) in text_lines.iter().enumerate() {
            lines.push(DiffLine {
                kind: DiffLineKind::Added,
                content: line.to_string(),
                new_line_number: Some(i + 1),
            });
        }
    }

    DiffContent::Text(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(!has_unstaged_changes(status));
    }

    #[test]
    fn has_staged_changes_index_deleted() {
        let status = Status::INDEX_DELETED;
        assert!(has_staged_changes(status));
        assert!(!has_unstaged_changes(status));
    }

    #[test]
    fn has_staged_changes_index_renamed() {
        let status = Status::INDEX_RENAMED;
        assert!(has_staged_changes(status));
        assert!(!has_unstaged_changes(status));
    }

    #[test]
    fn has_unstaged_changes_wt_modified() {
        let status = Status::WT_MODIFIED;
        assert!(!has_staged_changes(status));
        assert!(has_unstaged_changes(status));
    }

    #[test]
    fn has_unstaged_changes_wt_deleted() {
        let status = Status::WT_DELETED;
        assert!(!has_staged_changes(status));
        assert!(has_unstaged_changes(status));
    }

    #[test]
    fn has_unstaged_changes_wt_renamed() {
        let status = Status::WT_RENAMED;
        assert!(!has_staged_changes(status));
        assert!(has_unstaged_changes(status));
    }

    #[test]
    fn get_staged_status_returns_correct_type() {
        assert_eq!(get_staged_status(Status::INDEX_NEW), FileStatus::Added);
        assert_eq!(
            get_staged_status(Status::INDEX_DELETED),
            FileStatus::Deleted
        );
        assert_eq!(
            get_staged_status(Status::INDEX_RENAMED),
            FileStatus::Renamed
        );
        assert_eq!(
            get_staged_status(Status::INDEX_MODIFIED),
            FileStatus::Modified
        );
    }

    #[test]
    fn get_unstaged_status_returns_correct_type() {
        assert_eq!(get_unstaged_status(Status::WT_DELETED), FileStatus::Deleted);
        assert_eq!(get_unstaged_status(Status::WT_RENAMED), FileStatus::Renamed);
        assert_eq!(
            get_unstaged_status(Status::WT_MODIFIED),
            FileStatus::Modified
        );
    }

    #[test]
    fn has_both_staged_and_unstaged_changes() {
        let status = Status::INDEX_MODIFIED | Status::WT_MODIFIED;
        assert!(has_staged_changes(status));
        assert!(has_unstaged_changes(status));
    }
}
