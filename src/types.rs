/// A file entry representing a changed file in the git repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// The path of the file relative to the repository root.
    pub path: String,
    /// The original path for renamed files (None if not a rename).
    pub old_path: Option<String>,
    /// The type of change (added, modified, deleted, etc.).
    pub status: FileStatus,
    /// Number of lines added (None if not computable).
    pub added_lines: Option<usize>,
    /// Number of lines deleted (None if not computable).
    pub deleted_lines: Option<usize>,
    /// Whether the file is binary.
    pub is_binary: bool,
    /// Whether the file is a submodule.
    pub is_submodule: bool,
}

/// The type of change for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflict,
}

impl FileStatus {
    /// Returns the single-character symbol for this status.
    pub fn symbol(&self) -> &'static str {
        match self {
            FileStatus::Added => "A",
            FileStatus::Modified => "M",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "?",
            FileStatus::Conflict => "C",
        }
    }
}

/// Which section a file belongs to (staged or unstaged).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Staged,
    Unstaged,
}

/// Information about the current branch or detached HEAD.
#[derive(Debug, Clone)]
pub enum BranchInfo {
    /// On a named branch.
    Branch(String),
    /// Detached HEAD at a specific commit (short hash).
    Detached(String),
}

impl std::fmt::Display for BranchInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BranchInfo::Branch(name) => write!(f, "{}", name),
            BranchInfo::Detached(hash) => write!(f, "HEAD@{}", hash),
        }
    }
}

/// The content of a diff to display in the diff panel.
#[derive(Debug, Clone)]
pub enum DiffContent {
    /// No file selected yet.
    Empty,
    /// Working tree is clean (no changes).
    Clean,
    /// Diff text with line-by-line content.
    Text(Vec<DiffLine>),
    /// File is binary.
    Binary,
    /// File contains invalid UTF-8.
    InvalidUtf8,
    /// File has merge conflicts.
    Conflict,
}

/// A single line in a diff.
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// The type of line (header, hunk, context, added, deleted).
    pub kind: DiffLineKind,
    /// The text content of the line.
    pub content: String,
    /// The line number in the new file (for context and added lines).
    pub new_line_number: Option<usize>,
}

/// The type of a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// Diff header line (e.g., "diff --git a/... b/...").
    Header,
    /// Hunk header (e.g., "@@ -1,3 +1,4 @@").
    Hunk,
    /// Unchanged context line.
    Context,
    /// Added line.
    Added,
    /// Deleted line.
    Deleted,
}

/// A row in the visible file list (for navigation).
#[derive(Debug, Clone)]
pub struct VisibleRow {
    /// Which section this row belongs to.
    pub section: Section,
    /// The file path.
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_status_symbol() {
        assert_eq!(FileStatus::Added.symbol(), "A");
        assert_eq!(FileStatus::Modified.symbol(), "M");
        assert_eq!(FileStatus::Deleted.symbol(), "D");
        assert_eq!(FileStatus::Renamed.symbol(), "R");
        assert_eq!(FileStatus::Untracked.symbol(), "?");
        assert_eq!(FileStatus::Conflict.symbol(), "C");
    }

    #[test]
    fn branch_info_display() {
        let branch = BranchInfo::Branch("main".to_string());
        assert_eq!(branch.to_string(), "main");

        let detached = BranchInfo::Detached("abc1234".to_string());
        assert_eq!(detached.to_string(), "HEAD@abc1234");
    }
}
