use git2::{Repository, Signature};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

struct TestRepo {
    dir: TempDir,
    repo: Repository,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }

        Self { dir, repo }
    }

    fn write_file(&self, name: &str, content: &str) {
        let path = self.dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn stage(&self, name: &str) {
        let mut index = self.repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
    }

    fn commit(&self, message: &str) {
        let mut index = self.repo.index().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = self.repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();

        let parent = self.repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();

        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

mod status_tests {
    use super::*;
    use better_git_status::git::get_status;
    use better_git_status::types::FileStatus;

    #[test]
    fn get_status_new_untracked() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let status = get_status(&test_repo.repo).unwrap();

        assert_eq!(status.unstaged_files.len(), 1);
        assert_eq!(status.unstaged_files[0].status, FileStatus::Untracked);
        assert!(status.staged_files.is_empty());
        assert_eq!(status.untracked_count, 1);
    }

    #[test]
    fn get_status_new_staged() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");
        test_repo.stage("file.txt");

        let status = get_status(&test_repo.repo).unwrap();

        assert_eq!(status.staged_files.len(), 1);
        assert_eq!(status.staged_files[0].status, FileStatus::Added);
        assert!(status.unstaged_files.is_empty());
    }

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

    #[test]
    fn get_status_deleted_unstaged() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");
        test_repo.stage("file.txt");
        test_repo.commit("initial");
        fs::remove_file(test_repo.path().join("file.txt")).unwrap();

        let status = get_status(&test_repo.repo).unwrap();

        assert_eq!(status.unstaged_files.len(), 1);
        assert_eq!(status.unstaged_files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn get_status_renamed_file() {
        let test_repo = TestRepo::new();
        test_repo.write_file("old.txt", "content for rename detection\n");
        test_repo.stage("old.txt");
        test_repo.commit("initial");

        fs::rename(
            test_repo.path().join("old.txt"),
            test_repo.path().join("new.txt"),
        )
        .unwrap();

        let mut index = test_repo.repo.index().unwrap();
        index.remove_path(Path::new("old.txt")).unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let status = get_status(&test_repo.repo).unwrap();

        assert_eq!(status.staged_files.len(), 1);
        assert_eq!(status.staged_files[0].status, FileStatus::Renamed);
        assert_eq!(status.staged_files[0].path, "new.txt");
        assert_eq!(
            status.staged_files[0].old_path,
            Some("old.txt".to_string())
        );
    }

    #[test]
    fn get_status_binary_file() {
        let test_repo = TestRepo::new();
        fs::write(
            test_repo.path().join("binary.bin"),
            &[0x00, 0x01, 0x02, 0x03],
        )
        .unwrap();

        let status = get_status(&test_repo.repo).unwrap();

        assert_eq!(status.unstaged_files.len(), 1);
        assert!(status.unstaged_files[0].is_binary);
    }

    #[test]
    fn get_status_both_staged_and_unstaged() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "original\n");
        test_repo.stage("file.txt");
        test_repo.commit("initial");
        test_repo.write_file("file.txt", "staged change\n");
        test_repo.stage("file.txt");
        test_repo.write_file("file.txt", "unstaged change\n");

        let status = get_status(&test_repo.repo).unwrap();

        assert_eq!(status.staged_files.len(), 1);
        assert_eq!(status.unstaged_files.len(), 1);
        assert_eq!(status.staged_files[0].path, "file.txt");
        assert_eq!(status.unstaged_files[0].path, "file.txt");
    }
}

mod diff_tests {
    use super::*;
    use better_git_status::git::{get_diff, get_untracked_diff};
    use better_git_status::types::{DiffContent, DiffLineKind, Section};

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

    #[test]
    fn get_diff_unstaged_shows_changes() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "line1\n");
        test_repo.stage("file.txt");
        test_repo.commit("initial");
        test_repo.write_file("file.txt", "line1\nline2\n");

        let diff = get_diff(&test_repo.repo, "file.txt", None, Section::Unstaged);

        match diff {
            DiffContent::Text(lines) => {
                assert!(lines.iter().any(|l| l.kind == DiffLineKind::Added));
            }
            _ => panic!("Expected Text diff"),
        }
    }

    #[test]
    fn get_untracked_diff_shows_all_lines() {
        let test_repo = TestRepo::new();
        test_repo.write_file("new.txt", "line1\nline2\nline3\n");

        let diff = get_untracked_diff(&test_repo.repo, "new.txt");

        match diff {
            DiffContent::Text(lines) => {
                let added_count = lines
                    .iter()
                    .filter(|l| l.kind == DiffLineKind::Added)
                    .count();
                assert_eq!(added_count, 3);
            }
            _ => panic!("Expected Text diff"),
        }
    }

    #[test]
    fn get_diff_deleted_shows_removed_lines() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "line1\nline2\n");
        test_repo.stage("file.txt");
        test_repo.commit("initial");
        test_repo.write_file("file.txt", "line1\n");
        test_repo.stage("file.txt");

        let diff = get_diff(&test_repo.repo, "file.txt", None, Section::Staged);

        match diff {
            DiffContent::Text(lines) => {
                assert!(lines.iter().any(|l| l.kind == DiffLineKind::Deleted));
            }
            _ => panic!("Expected Text diff"),
        }
    }
}

mod branch_tests {
    use super::*;
    use better_git_status::git::get_branch_info;
    use better_git_status::types::BranchInfo;

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
}
