#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub status: FileStatus,
    pub added_lines: Option<usize>,
    pub deleted_lines: Option<usize>,
    pub is_binary: bool,
    pub is_submodule: bool,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Staged,
    Unstaged,
}

#[derive(Debug, Clone)]
pub enum BranchInfo {
    Branch(String),
    Detached(String),
}

impl BranchInfo {
    pub fn display(&self) -> String {
        match self {
            BranchInfo::Branch(name) => name.clone(),
            BranchInfo::Detached(hash) => format!("HEAD@{}", hash),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DiffContent {
    Empty,
    Clean,
    Text(Vec<DiffLine>),
    Binary,
    InvalidUtf8,
    Conflict,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub new_line_number: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Header,
    Hunk,
    Context,
    Added,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct VisibleRow {
    pub section: Section,
    pub path: String,
}
