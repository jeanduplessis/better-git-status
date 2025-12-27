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
            let short = &oid.to_string()[..7.min(oid.to_string().len())];
            return BranchInfo::Detached(short.to_string());
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
        .include_unreadable(false);

    let statuses = repo.statuses(Some(&mut opts))?;

    let mut staged_files = Vec::new();
    let mut unstaged_files = Vec::new();
    let mut staged_paths = HashSet::new();
    let mut unstaged_paths = HashSet::new();
    let mut untracked_files = HashSet::new();

    for entry in statuses.iter() {
        let path = entry.path().unwrap_or("").to_string();
        let status = entry.status();

        let is_conflict = status.is_conflicted();
        let is_submodule = status.is_index_typechange() || status.is_wt_typechange();

        if is_conflict {
            unstaged_paths.insert(path.clone());
            let entry = FileEntry {
                path,
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
            let entry = FileEntry {
                path,
                status: FileStatus::Untracked,
                added_lines: None,
                deleted_lines: None,
                is_binary: false,
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
            staged_paths.insert(path.clone());
            let file_status = get_staged_status(status);
            let (added, deleted, is_binary) =
                get_line_counts_for_section(repo, &path, Section::Staged);
            staged_files.push(FileEntry {
                path: path.clone(),
                status: file_status,
                added_lines: added,
                deleted_lines: deleted,
                is_binary,
                is_submodule: false,
            });
        }

        if has_unstaged {
            unstaged_paths.insert(path.clone());
            let file_status = get_unstaged_status(status);
            let (added, deleted, is_binary) =
                get_line_counts_for_section(repo, &path, Section::Unstaged);
            unstaged_files.push(FileEntry {
                path,
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

fn has_staged_changes(status: Status) -> bool {
    status.is_index_new()
        || status.is_index_modified()
        || status.is_index_deleted()
        || status.is_index_renamed()
        || status.is_index_typechange()
}

fn has_unstaged_changes(status: Status) -> bool {
    status.is_wt_modified()
        || status.is_wt_deleted()
        || status.is_wt_renamed()
        || status.is_wt_typechange()
}

fn get_staged_status(status: Status) -> FileStatus {
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

fn get_unstaged_status(status: Status) -> FileStatus {
    if status.is_wt_deleted() {
        FileStatus::Deleted
    } else if status.is_wt_renamed() {
        FileStatus::Renamed
    } else {
        FileStatus::Modified
    }
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

pub fn get_diff(repo: &Repository, path: &str, section: Section) -> DiffContent {
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
