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
        assert_eq!(status.staged_files[0].old_path, Some("old.txt".to_string()));
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

mod stage_unstage_tests {
    use super::*;
    use better_git_status::git::{get_status, stage_all, stage_files, unstage_all, unstage_files};
    use better_git_status::types::FileStatus;

    #[test]
    fn stage_files_adds_to_index() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let status = get_status(&test_repo.repo).unwrap();
        assert_eq!(status.unstaged_files.len(), 1);
        assert!(status.staged_files.is_empty());

        stage_files(&test_repo.repo, &["file.txt".to_string()]).unwrap();

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.unstaged_files.is_empty());
        assert_eq!(status.staged_files.len(), 1);
        assert_eq!(status.staged_files[0].status, FileStatus::Added);
    }

    #[test]
    fn stage_files_handles_deleted_file() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");
        test_repo.stage("file.txt");
        test_repo.commit("initial");
        fs::remove_file(test_repo.path().join("file.txt")).unwrap();

        stage_files(&test_repo.repo, &["file.txt".to_string()]).unwrap();

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.unstaged_files.is_empty());
        assert_eq!(status.staged_files.len(), 1);
        assert_eq!(status.staged_files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn unstage_files_removes_from_index() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");
        test_repo.stage("file.txt");

        let status = get_status(&test_repo.repo).unwrap();
        assert_eq!(status.staged_files.len(), 1);

        unstage_files(&test_repo.repo, &["file.txt".to_string()]).unwrap();

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.staged_files.is_empty());
        assert_eq!(status.unstaged_files.len(), 1);
        assert_eq!(status.unstaged_files[0].status, FileStatus::Untracked);
    }

    #[test]
    fn unstage_files_resets_modified() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "original\n");
        test_repo.stage("file.txt");
        test_repo.commit("initial");
        test_repo.write_file("file.txt", "modified\n");
        test_repo.stage("file.txt");

        let status = get_status(&test_repo.repo).unwrap();
        assert_eq!(status.staged_files.len(), 1);
        assert!(status.unstaged_files.is_empty());

        unstage_files(&test_repo.repo, &["file.txt".to_string()]).unwrap();

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.staged_files.is_empty());
        assert_eq!(status.unstaged_files.len(), 1);
        assert_eq!(status.unstaged_files[0].status, FileStatus::Modified);
    }

    #[test]
    fn stage_all_stages_all_unstaged() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let status = get_status(&test_repo.repo).unwrap();
        assert_eq!(status.unstaged_files.len(), 2);

        let paths = stage_all(&test_repo.repo).unwrap();
        assert_eq!(paths.len(), 2);

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.unstaged_files.is_empty());
        assert_eq!(status.staged_files.len(), 2);
    }

    #[test]
    fn unstage_all_unstages_all_staged() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");
        test_repo.stage("file1.txt");
        test_repo.stage("file2.txt");

        let status = get_status(&test_repo.repo).unwrap();
        assert_eq!(status.staged_files.len(), 2);

        let paths = unstage_all(&test_repo.repo).unwrap();
        assert_eq!(paths.len(), 2);

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.staged_files.is_empty());
        assert_eq!(status.unstaged_files.len(), 2);
    }

    #[test]
    fn stage_multiple_files() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");
        test_repo.write_file("file3.txt", "content3\n");

        stage_files(
            &test_repo.repo,
            &["file1.txt".to_string(), "file3.txt".to_string()],
        )
        .unwrap();

        let status = get_status(&test_repo.repo).unwrap();
        assert_eq!(status.staged_files.len(), 2);
        assert_eq!(status.unstaged_files.len(), 1);
        assert_eq!(status.unstaged_files[0].path, "file2.txt");
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

mod app_stage_unstage_tests {
    use super::*;
    use better_git_status::app::App;
    use better_git_status::git::get_status;
    use better_git_status::types::{FileStatus, Section};

    #[test]
    fn app_stage_selected_single_file() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert_eq!(app.unstaged_count, 1);
        assert_eq!(app.staged_count, 0);

        app.stage_selected().unwrap();

        assert_eq!(app.unstaged_count, 0);
        assert_eq!(app.staged_count, 1);

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.unstaged_files.is_empty());
        assert_eq!(status.staged_files.len(), 1);
        assert_eq!(status.staged_files[0].status, FileStatus::Added);
    }

    #[test]
    fn app_unstage_selected_single_file() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");
        test_repo.stage("file.txt");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert_eq!(app.staged_count, 1);
        assert_eq!(app.unstaged_count, 0);

        app.unstage_selected().unwrap();

        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 1);

        let status = get_status(&test_repo.repo).unwrap();
        assert!(status.staged_files.is_empty());
        assert_eq!(status.unstaged_files.len(), 1);
    }

    #[test]
    fn app_stage_multi_selected_files() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert_eq!(app.unstaged_count, 2);

        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();

        assert_eq!(app.multi_selected.len(), 2);

        app.stage_selected().unwrap();

        assert_eq!(app.unstaged_count, 0);
        assert_eq!(app.staged_count, 2);
        assert!(app.multi_selected.is_empty());

        let status = get_status(&test_repo.repo).unwrap();
        assert_eq!(status.staged_files.len(), 2);
    }

    #[test]
    fn app_unstage_multi_selected_files() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");
        test_repo.stage("file1.txt");
        test_repo.stage("file2.txt");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert_eq!(app.staged_count, 2);

        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();

        assert_eq!(app.multi_selected.len(), 2);

        app.unstage_selected().unwrap();

        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 2);
        assert!(app.multi_selected.is_empty());
    }

    #[test]
    fn app_stage_ignores_already_staged_files() {
        let test_repo = TestRepo::new();
        test_repo.write_file("staged.txt", "staged\n");
        test_repo.stage("staged.txt");
        test_repo.write_file("unstaged.txt", "unstaged\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert_eq!(app.staged_count, 1);
        assert_eq!(app.unstaged_count, 1);

        app.multi_selected
            .insert((Section::Staged, "staged.txt".to_string()));
        app.multi_selected
            .insert((Section::Unstaged, "unstaged.txt".to_string()));

        app.stage_selected().unwrap();

        assert_eq!(app.staged_count, 2);
        assert_eq!(app.unstaged_count, 0);
    }

    #[test]
    fn app_unstage_ignores_already_unstaged_files() {
        let test_repo = TestRepo::new();
        test_repo.write_file("staged.txt", "staged\n");
        test_repo.stage("staged.txt");
        test_repo.write_file("unstaged.txt", "unstaged\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.multi_selected
            .insert((Section::Staged, "staged.txt".to_string()));
        app.multi_selected
            .insert((Section::Unstaged, "unstaged.txt".to_string()));

        app.unstage_selected().unwrap();

        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 2);
    }

    #[test]
    fn app_stage_clears_multi_select() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();
        app.toggle_multi_select();

        assert!(!app.multi_selected.is_empty());

        app.stage_selected().unwrap();

        assert!(app.multi_selected.is_empty());
    }

    #[test]
    fn app_stage_sets_flash_message() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert!(app.flash_message.is_none());

        app.stage_selected().unwrap();

        assert!(app.flash_message.is_some());
        let flash = app.flash_message.as_ref().unwrap();
        assert!(flash.text.contains("Staged"));
        assert!(!flash.is_error);
    }

    #[test]
    fn app_undo_after_stage_unstages_files() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();
        app.stage_selected().unwrap();

        assert_eq!(app.staged_count, 2);
        assert_eq!(app.unstaged_count, 0);
        assert!(app.last_action.is_some());

        app.undo().unwrap();

        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 2);
        assert!(app.last_action.is_none());
        let flash = app.flash_message.as_ref().unwrap();
        assert!(flash.text.contains("Undid stage"));
    }

    #[test]
    fn app_undo_after_unstage_restages_files() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");
        test_repo.stage("file.txt");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.unstage_selected().unwrap();

        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 1);
        assert!(app.last_action.is_some());

        app.undo().unwrap();

        assert_eq!(app.staged_count, 1);
        assert_eq!(app.unstaged_count, 0);
        assert!(app.last_action.is_none());
        let flash = app.flash_message.as_ref().unwrap();
        assert!(flash.text.contains("Undid unstage"));
    }

    #[test]
    fn app_second_undo_is_noop() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.stage_selected().unwrap();
        app.undo().unwrap();

        assert!(app.last_action.is_none());
        let _msg_after_first_undo = app.flash_message.clone();

        app.clear_flash();
        app.undo().unwrap();

        assert!(app.flash_message.is_none());
        assert!(app.last_action.is_none());
    }
}

mod confirm_prompt_tests {
    use super::*;
    use better_git_status::app::App;
    use better_git_status::types::ConfirmAction;

    #[test]
    fn show_stage_all_confirm_sets_prompt() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert!(app.confirm_prompt.is_none());

        app.show_stage_all_confirm();

        assert!(app.confirm_prompt.is_some());
        let prompt = app.confirm_prompt.as_ref().unwrap();
        assert!(prompt.message.contains("2 files"));
        assert!(prompt.message.contains("[y/N]"));
        assert_eq!(prompt.action, ConfirmAction::StageAll);
    }

    #[test]
    fn show_unstage_all_confirm_sets_prompt() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");
        test_repo.stage("file1.txt");
        test_repo.stage("file2.txt");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert!(app.confirm_prompt.is_none());

        app.show_unstage_all_confirm();

        assert!(app.confirm_prompt.is_some());
        let prompt = app.confirm_prompt.as_ref().unwrap();
        assert!(prompt.message.contains("2 files"));
        assert!(prompt.message.contains("[y/N]"));
        assert_eq!(prompt.action, ConfirmAction::UnstageAll);
    }

    #[test]
    fn stage_all_confirm_with_no_files_does_nothing() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");
        test_repo.stage("file.txt");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert_eq!(app.unstaged_count, 0);
        app.show_stage_all_confirm();

        assert!(app.confirm_prompt.is_none());
    }

    #[test]
    fn unstage_all_confirm_with_no_files_does_nothing() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        assert_eq!(app.staged_count, 0);
        app.show_unstage_all_confirm();

        assert!(app.confirm_prompt.is_none());
    }

    #[test]
    fn confirm_prompt_y_executes_stage_all() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.show_stage_all_confirm();
        assert!(app.confirm_prompt.is_some());

        app.handle_confirm(true).unwrap();

        assert!(app.confirm_prompt.is_none());
        assert_eq!(app.staged_count, 2);
        assert_eq!(app.unstaged_count, 0);
        let flash = app.flash_message.as_ref().unwrap();
        assert!(flash.text.contains("Staged"));
    }

    #[test]
    fn confirm_prompt_y_executes_unstage_all() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");
        test_repo.stage("file1.txt");
        test_repo.stage("file2.txt");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.show_unstage_all_confirm();
        assert!(app.confirm_prompt.is_some());

        app.handle_confirm(true).unwrap();

        assert!(app.confirm_prompt.is_none());
        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 2);
        let flash = app.flash_message.as_ref().unwrap();
        assert!(flash.text.contains("Unstaged"));
    }

    #[test]
    fn confirm_prompt_dismiss_does_not_execute() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.show_stage_all_confirm();
        assert!(app.confirm_prompt.is_some());

        app.handle_confirm(false).unwrap();

        assert!(app.confirm_prompt.is_none());
        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 2);
    }

    #[test]
    fn confirm_stage_all_records_undo_action() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.show_stage_all_confirm();
        app.handle_confirm(true).unwrap();

        assert!(app.last_action.is_some());

        app.undo().unwrap();

        assert_eq!(app.staged_count, 0);
        assert_eq!(app.unstaged_count, 2);
    }

    #[test]
    fn confirm_unstage_all_records_undo_action() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");
        test_repo.stage("file1.txt");
        test_repo.stage("file2.txt");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.show_unstage_all_confirm();
        app.handle_confirm(true).unwrap();

        assert!(app.last_action.is_some());

        app.undo().unwrap();

        assert_eq!(app.staged_count, 2);
        assert_eq!(app.unstaged_count, 0);
    }

    #[test]
    fn confirm_stage_all_clears_multi_select() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file1.txt", "content1\n");
        test_repo.write_file("file2.txt", "content2\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.toggle_multi_select();
        assert!(!app.multi_selected.is_empty());

        app.show_stage_all_confirm();
        app.handle_confirm(true).unwrap();

        assert!(app.multi_selected.is_empty());
    }

    #[test]
    fn singular_file_prompt_message() {
        let test_repo = TestRepo::new();
        test_repo.write_file("file.txt", "content\n");

        let mut app = App::new(test_repo.path().to_str().unwrap()).unwrap();

        app.show_stage_all_confirm();

        let prompt = app.confirm_prompt.as_ref().unwrap();
        assert!(prompt.message.contains("1 file?"));
        assert!(!prompt.message.contains("files"));
    }
}
