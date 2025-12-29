use crate::git;
use crate::types::{
    BranchInfo, ConfirmAction, ConfirmPrompt, DiffContent, FileEntry, FlashMessage, MultiSelectSet,
    Section, UndoAction, VisibleRow,
};
use crate::ui;
use crate::watcher::{FileWatcher, WatcherEvent};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use git2::Repository;
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use std::io;
use std::path::Path;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

const FLASH_TIMEOUT: Duration = Duration::from_secs(3);

/// Application state for the interactive git status TUI.
pub struct App {
    repo: Repository,

    pub staged_files: Vec<FileEntry>,
    pub unstaged_files: Vec<FileEntry>,

    pub highlight_index: Option<usize>,
    pub selected: Option<(Section, String)>,
    pub multi_selected: MultiSelectSet,
    pub file_list_scroll: usize,

    pub current_diff: DiffContent,
    pub diff_scroll: usize,

    pub staged_count: usize,
    pub unstaged_count: usize,
    pub untracked_count: usize,

    pub branch: BranchInfo,

    visible_rows: Vec<VisibleRow>,

    pub file_list_height: usize,

    pub file_list_area: Rect,
    pub diff_area: Rect,

    pub confirm_prompt: Option<ConfirmPrompt>,
    pub flash_message: Option<FlashMessage>,
    pub last_action: Option<UndoAction>,
}

impl App {
    pub fn new(path: &str) -> Result<Self> {
        let repo = git::get_repo(path)?;
        let branch = git::get_branch_info(&repo);
        let status = git::get_status(&repo)?;

        let visible_rows = build_visible_rows(&status.staged_files, &status.unstaged_files);
        let highlight_index = if visible_rows.is_empty() {
            None
        } else {
            Some(0)
        };

        let current_diff = if visible_rows.is_empty() {
            if status.staged_files.is_empty() && status.unstaged_files.is_empty() {
                DiffContent::Clean
            } else {
                DiffContent::Empty
            }
        } else {
            DiffContent::Empty
        };

        Ok(Self {
            repo,
            staged_files: status.staged_files,
            unstaged_files: status.unstaged_files,
            highlight_index,
            selected: None,
            multi_selected: MultiSelectSet::new(),
            file_list_scroll: 0,
            current_diff,
            diff_scroll: 0,
            staged_count: status.staged_count,
            unstaged_count: status.unstaged_count,
            untracked_count: status.untracked_count,
            branch,
            visible_rows,
            file_list_height: 0,
            file_list_area: Rect::default(),
            diff_area: Rect::default(),
            confirm_prompt: None,
            flash_message: None,
            last_action: None,
        })
    }

    fn refresh(&mut self) -> Result<()> {
        self.branch = git::get_branch_info(&self.repo);

        let status = git::get_status(&self.repo)?;
        self.staged_files = status.staged_files;
        self.unstaged_files = status.unstaged_files;
        self.staged_count = status.staged_count;
        self.unstaged_count = status.unstaged_count;
        self.untracked_count = status.untracked_count;

        self.visible_rows = build_visible_rows(&self.staged_files, &self.unstaged_files);

        if self.visible_rows.is_empty() {
            self.highlight_index = None;
            self.selected = None;
            self.multi_selected.clear();
            self.current_diff = DiffContent::Clean;
            self.diff_scroll = 0;
            return Ok(());
        }

        self.prune_multi_select();

        if let Some(idx) = self.highlight_index {
            if idx >= self.visible_rows.len() {
                self.highlight_index = Some(self.visible_rows.len() - 1);
            }
        } else {
            self.highlight_index = Some(0);
        }

        if let Some((section, path)) = &self.selected {
            let still_exists = self
                .visible_rows
                .iter()
                .any(|r| r.section == *section && r.path == *path);
            if !still_exists {
                self.selected = None;
                self.current_diff = DiffContent::Empty;
                self.diff_scroll = 0;
            } else {
                self.update_diff_for_selected();
            }
        } else {
            self.current_diff = DiffContent::Empty;
        }

        self.update_scroll_for_highlight();
        Ok(())
    }

    fn update_diff_for_selected(&mut self) {
        if let Some((section, path)) = &self.selected {
            let file = match section {
                Section::Staged => self.staged_files.iter().find(|f| &f.path == path),
                Section::Unstaged => self.unstaged_files.iter().find(|f| &f.path == path),
            };

            if let Some(file) = file {
                if file.status == crate::types::FileStatus::Conflict {
                    self.current_diff = DiffContent::Conflict;
                } else if file.is_binary {
                    self.current_diff = DiffContent::Binary;
                } else if file.status == crate::types::FileStatus::Untracked {
                    self.current_diff = git::get_untracked_diff(&self.repo, path);
                } else {
                    self.current_diff =
                        git::get_diff(&self.repo, path, file.old_path.as_deref(), *section);
                }
            }
        }
    }

    fn select_current(&mut self) {
        if let Some(idx) = self.highlight_index {
            if let Some(row) = self.visible_rows.get(idx) {
                self.selected = Some((row.section, row.path.clone()));
                self.diff_scroll = 0;
                self.update_diff_for_selected();
            }
        }
    }

    pub fn toggle_multi_select(&mut self) {
        if let Some(idx) = self.highlight_index {
            if let Some(row) = self.visible_rows.get(idx) {
                let key = (row.section, row.path.clone());
                if self.multi_selected.contains(&key) {
                    self.multi_selected.remove(&key);
                } else {
                    self.multi_selected.insert(key);
                }
            }
        }
    }

    pub fn clear_multi_select(&mut self) {
        self.multi_selected.clear();
    }

    fn prune_multi_select(&mut self) {
        self.multi_selected.retain(|(section, path)| {
            self.visible_rows
                .iter()
                .any(|r| r.section == *section && &r.path == path)
        });
    }

    pub fn get_action_targets(&self) -> Vec<(Section, String)> {
        if self.multi_selected.is_empty() {
            if let Some(idx) = self.highlight_index {
                if let Some(row) = self.visible_rows.get(idx) {
                    return vec![(row.section, row.path.clone())];
                }
            }
            vec![]
        } else {
            self.multi_selected.iter().cloned().collect()
        }
    }

    pub fn stage_selected(&mut self) -> Result<()> {
        let targets = self.get_action_targets();
        let paths: Vec<String> = targets
            .into_iter()
            .filter(|(section, _)| *section == Section::Unstaged)
            .map(|(_, path)| path)
            .collect();

        if paths.is_empty() {
            return Ok(());
        }

        let count = paths.len();
        git::stage_files(&self.repo, &paths)?;
        self.last_action = Some(UndoAction::Stage { paths });
        self.clear_multi_select();
        self.refresh()?;
        self.show_flash_success(format!("Staged {} file{}", count, plural_s(count)));
        Ok(())
    }

    pub fn unstage_selected(&mut self) -> Result<()> {
        let targets = self.get_action_targets();
        let paths: Vec<String> = targets
            .into_iter()
            .filter(|(section, _)| *section == Section::Staged)
            .map(|(_, path)| path)
            .collect();

        if paths.is_empty() {
            return Ok(());
        }

        let count = paths.len();
        git::unstage_files(&self.repo, &paths)?;
        self.last_action = Some(UndoAction::Unstage { paths });
        self.clear_multi_select();
        self.refresh()?;
        self.show_flash_success(format!("Unstaged {} file{}", count, plural_s(count)));
        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        let action = match &self.last_action {
            Some(a) => a.clone(),
            None => return Ok(()),
        };

        match action {
            UndoAction::Stage { paths } => {
                let count = paths.len();
                git::unstage_files(&self.repo, &paths)?;
                self.last_action = None;
                self.refresh()?;
                self.show_flash_success(format!(
                    "Undid stage of {} file{}",
                    count,
                    plural_s(count)
                ));
            }
            UndoAction::Unstage { paths } => {
                let count = paths.len();
                git::stage_files(&self.repo, &paths)?;
                self.last_action = None;
                self.refresh()?;
                self.show_flash_success(format!(
                    "Undid unstage of {} file{}",
                    count,
                    plural_s(count)
                ));
            }
        }

        Ok(())
    }

    pub fn show_discard_selected_confirm(&mut self) {
        let targets = self.get_action_targets();
        if targets.is_empty() {
            return;
        }

        let unstaged_targets: Vec<(Section, String)> = targets
            .into_iter()
            .filter(|(section, _)| *section == Section::Unstaged)
            .collect();

        if unstaged_targets.is_empty() {
            return;
        }

        let has_conflict = unstaged_targets.iter().any(|(_, path)| {
            self.unstaged_files
                .iter()
                .any(|f| &f.path == path && f.status == crate::types::FileStatus::Conflict)
        });

        if has_conflict {
            self.show_flash_error("Cannot discard conflicted files. Resolve conflicts first.");
            return;
        }

        let has_untracked = unstaged_targets.iter().any(|(_, path)| {
            self.unstaged_files
                .iter()
                .any(|f| &f.path == path && f.status == crate::types::FileStatus::Untracked)
        });

        let count = unstaged_targets.len();
        let message = if count == 1 {
            if has_untracked {
                "Delete untracked file? [y/N]".to_string()
            } else {
                "Discard changes? [y/N]".to_string()
            }
        } else if has_untracked {
            format!(
                "Discard {} changes (including untracked files)? [y/N]",
                count
            )
        } else {
            format!("Discard {} changes? [y/N]", count)
        };

        self.confirm_prompt = Some(ConfirmPrompt {
            message,
            action: ConfirmAction::DiscardSelected {
                paths: unstaged_targets,
            },
        });
    }

    pub fn show_discard_all_confirm(&mut self) {
        let count = self.unstaged_files.len();
        if count == 0 {
            return;
        }

        let has_untracked = self
            .unstaged_files
            .iter()
            .any(|f| f.status == crate::types::FileStatus::Untracked);

        let message = if has_untracked {
            format!(
                "Discard all changes and delete untracked files ({} files)? [y/N]",
                count
            )
        } else {
            format!("Discard all changes ({} files)? [y/N]", count)
        };

        self.confirm_prompt = Some(ConfirmPrompt {
            message,
            action: ConfirmAction::DiscardAll,
        });
    }

    fn discard_files(&mut self, paths: &[(Section, String)]) -> Result<()> {
        let mut count = 0;
        for (section, path) in paths {
            if *section != Section::Unstaged {
                continue;
            }

            let is_untracked = self
                .unstaged_files
                .iter()
                .any(|f| &f.path == path && f.status == crate::types::FileStatus::Untracked);

            if is_untracked {
                git::discard_untracked_file(&self.repo, path)?;
            } else {
                git::discard_unstaged_file(&self.repo, path)?;
            }
            count += 1;
        }

        self.last_action = None;
        self.clear_multi_select();
        self.refresh()?;
        if count > 0 {
            self.show_flash_success(format!("Discarded {} file{}", count, plural_s(count)));
        }
        Ok(())
    }

    fn discard_all(&mut self) -> Result<()> {
        let (paths, skipped_conflicts) = git::discard_all_unstaged(&self.repo)?;
        let count = paths.len();
        self.last_action = None;
        self.clear_multi_select();
        self.refresh()?;
        if count > 0 && skipped_conflicts > 0 {
            self.show_flash_success(format!(
                "Discarded {} file{} ({} conflict{} skipped)",
                count,
                plural_s(count),
                skipped_conflicts,
                plural_s(skipped_conflicts)
            ));
        } else if count > 0 {
            self.show_flash_success(format!("Discarded {} file{}", count, plural_s(count)));
        } else if skipped_conflicts > 0 {
            self.show_flash_error(format!(
                "No files discarded ({} conflict{} skipped)",
                skipped_conflicts,
                plural_s(skipped_conflicts)
            ));
        }
        Ok(())
    }

    pub fn show_stage_all_confirm(&mut self) {
        let count = self.unstaged_files.len();
        if count == 0 {
            return;
        }
        self.confirm_prompt = Some(ConfirmPrompt {
            message: format!("Stage {} file{}? [y/N]", count, plural_s(count)),
            action: ConfirmAction::StageAll,
        });
    }

    pub fn show_unstage_all_confirm(&mut self) {
        let count = self.staged_files.len();
        if count == 0 {
            return;
        }
        self.confirm_prompt = Some(ConfirmPrompt {
            message: format!("Unstage {} file{}? [y/N]", count, plural_s(count)),
            action: ConfirmAction::UnstageAll,
        });
    }

    pub fn show_flash_success(&mut self, text: impl Into<String>) {
        self.flash_message = Some(FlashMessage::success(text));
    }

    pub fn show_flash_error(&mut self, text: impl Into<String>) {
        self.flash_message = Some(FlashMessage::error(text));
    }

    pub fn clear_flash(&mut self) {
        self.flash_message = None;
    }

    pub fn check_flash_expiry(&mut self) {
        if let Some(ref flash) = self.flash_message {
            if flash.is_expired(FLASH_TIMEOUT) {
                self.flash_message = None;
            }
        }
    }

    pub fn handle_confirm(&mut self, confirmed: bool) -> Result<()> {
        if let Some(prompt) = self.confirm_prompt.take() {
            if confirmed {
                match prompt.action {
                    ConfirmAction::StageAll => {
                        let paths = git::stage_all(&self.repo)?;
                        let count = paths.len();
                        if count > 0 {
                            self.last_action = Some(UndoAction::Stage { paths });
                        }
                        self.clear_multi_select();
                        self.refresh()?;
                        if count > 0 {
                            self.show_flash_success(format!("Staged {} file{}", count, plural_s(count)));
                        }
                    }
                    ConfirmAction::UnstageAll => {
                        let paths = git::unstage_all(&self.repo)?;
                        let count = paths.len();
                        if count > 0 {
                            self.last_action = Some(UndoAction::Unstage { paths });
                        }
                        self.clear_multi_select();
                        self.refresh()?;
                        if count > 0 {
                            self.show_flash_success(format!("Unstaged {} file{}", count, plural_s(count)));
                        }
                    }
                    ConfirmAction::DiscardSelected { paths } => {
                        self.discard_files(&paths)?;
                    }
                    ConfirmAction::DiscardAll => {
                        self.discard_all()?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn move_highlight(&mut self, delta: isize) {
        if self.visible_rows.is_empty() {
            return;
        }

        let current = self.highlight_index.unwrap_or(0) as isize;
        let new_idx = (current + delta).clamp(0, self.visible_rows.len() as isize - 1) as usize;
        self.highlight_index = Some(new_idx);
        self.update_scroll_for_highlight();
    }

    fn update_scroll_for_highlight(&mut self) {
        if let Some(idx) = self.highlight_index {
            let header_offset = self.count_headers_before(idx);
            let visual_idx = idx + header_offset;

            if visual_idx < self.file_list_scroll {
                self.file_list_scroll = visual_idx;
            } else if self.file_list_height > 0
                && visual_idx >= self.file_list_scroll + self.file_list_height
            {
                self.file_list_scroll = visual_idx - self.file_list_height + 1;
            }
        }
    }

    fn count_headers_before(&self, file_idx: usize) -> usize {
        let mut headers = 0;
        if !self.staged_files.is_empty() {
            headers += 1;
        }
        if !self.unstaged_files.is_empty() && file_idx >= self.staged_files.len() {
            headers += 1;
        }
        headers
    }

    fn scroll_diff(&mut self, delta: isize, viewport_height: usize, viewport_width: usize) {
        let max_scroll =
            crate::ui::diff_panel::max_scroll(&self.current_diff, viewport_height, viewport_width);
        let current = self.diff_scroll as isize;
        self.diff_scroll = (current + delta).clamp(0, max_scroll as isize) as usize;
    }

    fn page_scroll_diff(&mut self, down: bool, viewport_height: usize, viewport_width: usize) {
        let delta = if down {
            viewport_height as isize
        } else {
            -(viewport_height as isize)
        };
        self.scroll_diff(delta, viewport_height, viewport_width);
    }

    fn click_file_list(&mut self, row: u16) {
        let inner_row = row.saturating_sub(self.file_list_area.y + 1) as usize;
        let visual_row = self.file_list_scroll + inner_row;

        let staged_count = self.staged_files.len();
        let unstaged_count = self.unstaged_files.len();

        let file_index = if staged_count > 0 && unstaged_count > 0 {
            let staged_header = 0;
            let unstaged_header = 1 + staged_count;

            if visual_row == staged_header || visual_row == unstaged_header {
                return;
            } else if visual_row < unstaged_header {
                visual_row - 1
            } else {
                visual_row - 2
            }
        } else if staged_count > 0 || unstaged_count > 0 {
            if visual_row == 0 {
                return;
            }
            visual_row - 1
        } else {
            return;
        };

        if file_index < self.visible_rows.len() {
            self.highlight_index = Some(file_index);
            self.select_current();
        }
    }
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

pub(crate) fn build_visible_rows(staged: &[FileEntry], unstaged: &[FileEntry]) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    for file in staged.iter() {
        rows.push(VisibleRow {
            section: Section::Staged,
            path: file.path.clone(),
        });
    }
    for file in unstaged.iter() {
        rows.push(VisibleRow {
            section: Section::Unstaged,
            path: file.path.clone(),
        });
    }
    rows
}

pub fn run(path: &str) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, path);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, path: &str) -> Result<()> {
    let mut app = App::new(path)?;

    let watcher = FileWatcher::new(Path::new(path));
    let mut use_polling = watcher.is_err();
    if let Err(ref e) = watcher {
        eprintln!("Warning: file watcher initialization failed: {e}. Falling back to polling.");
    }
    let watcher = watcher.ok();

    let mut last_poll = Instant::now();
    let poll_interval = Duration::from_secs(2);
    let debounce_duration = Duration::from_millis(150);
    let mut pending_refresh: Option<Instant> = None;

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        let timeout = if pending_refresh.is_some() {
            Duration::from_millis(10)
        } else {
            Duration::from_millis(100)
        };

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        if app.confirm_prompt.is_some() {
                            let confirmed = matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'));
                            app.handle_confirm(confirmed)?;
                        } else {
                            app.clear_flash();
                            match key.code {
                                KeyCode::Char('q') => break,
                                KeyCode::Esc => {
                                    if app.multi_selected.is_empty() {
                                        break;
                                    } else {
                                        app.clear_multi_select();
                                    }
                                }
                                KeyCode::Down => app.move_highlight(1),
                                KeyCode::Up => app.move_highlight(-1),
                                KeyCode::Char(' ') => app.toggle_multi_select(),
                                KeyCode::Enter => app.select_current(),
                                KeyCode::Char('s') => {
                                    if let Err(e) = app.stage_selected() {
                                        app.show_flash_error(format!("Error: {}", e));
                                    }
                                }
                                KeyCode::Char('u') => {
                                    if let Err(e) = app.unstage_selected() {
                                        app.show_flash_error(format!("Error: {}", e));
                                    }
                                }
                                KeyCode::Char('S') => app.show_stage_all_confirm(),
                                KeyCode::Char('U') => app.show_unstage_all_confirm(),
                                KeyCode::Char('d') => app.show_discard_selected_confirm(),
                                KeyCode::Char('D') => app.show_discard_all_confirm(),
                                KeyCode::PageDown => {
                                    let size = terminal.size()?;
                                    let height = size.height.saturating_sub(10) as usize;
                                    let width = size.width.saturating_sub(2) as usize;
                                    app.page_scroll_diff(true, height, width);
                                }
                                KeyCode::PageUp => {
                                    let size = terminal.size()?;
                                    let height = size.height.saturating_sub(10) as usize;
                                    let width = size.width.saturating_sub(2) as usize;
                                    app.page_scroll_diff(false, height, width);
                                }
                                KeyCode::Char('z')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    if let Err(e) = app.undo() {
                                        app.show_flash_error(format!("Error: {}", e));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    let (col, row) = (mouse.column, mouse.row);
                    let in_file_list = app.file_list_area.contains((col, row).into());
                    let in_diff = app.diff_area.contains((col, row).into());

                    match mouse.kind {
                        MouseEventKind::ScrollDown => {
                            if in_file_list {
                                app.move_highlight(3);
                            } else if in_diff {
                                let height = app.diff_area.height.saturating_sub(2) as usize;
                                let width = app.diff_area.width.saturating_sub(2) as usize;
                                app.scroll_diff(3, height, width);
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if in_file_list {
                                app.move_highlight(-3);
                            } else if in_diff {
                                let height = app.diff_area.height.saturating_sub(2) as usize;
                                let width = app.diff_area.width.saturating_sub(2) as usize;
                                app.scroll_diff(-3, height, width);
                            }
                        }
                        MouseEventKind::Down(event::MouseButton::Left) => {
                            if in_file_list {
                                app.click_file_list(row);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if let Some(ref w) = watcher {
            match w.receiver.try_recv() {
                Ok(WatcherEvent::Changed) => {
                    pending_refresh = Some(Instant::now());
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    if !use_polling {
                        eprintln!("Warning: file watcher disconnected. Falling back to polling.");
                    }
                    use_polling = true;
                }
            }

            while w.receiver.try_recv().is_ok() {
                pending_refresh = Some(Instant::now());
            }
        }

        if let Some(pending_time) = pending_refresh {
            if pending_time.elapsed() >= debounce_duration {
                app.refresh()?;
                pending_refresh = None;
            }
        }

        if use_polling && last_poll.elapsed() >= poll_interval {
            app.refresh()?;
            last_poll = Instant::now();
        }

        app.check_flash_expiry();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileEntry, FileStatus, Section};

    fn file_entry(path: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            old_path: None,
            status: FileStatus::Modified,
            added_lines: Some(1),
            deleted_lines: Some(0),
            is_binary: false,
            is_submodule: false,
        }
    }

    #[test]
    fn build_visible_rows_staged_only() {
        let staged = vec![file_entry("a.rs"), file_entry("b.rs")];
        let unstaged = vec![];
        let rows = build_visible_rows(&staged, &unstaged);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.section == Section::Staged));
    }

    #[test]
    fn build_visible_rows_unstaged_only() {
        let staged = vec![];
        let unstaged = vec![file_entry("a.rs"), file_entry("b.rs")];
        let rows = build_visible_rows(&staged, &unstaged);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.section == Section::Unstaged));
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

    // Shared helper functions for multi-select operations.
    // These mirror the logic in App but work on raw state, avoiding duplication.

    fn toggle_multi_select_helper(
        highlight_index: Option<usize>,
        visible_rows: &[VisibleRow],
        multi_selected: &mut MultiSelectSet,
    ) {
        if let Some(idx) = highlight_index {
            if let Some(row) = visible_rows.get(idx) {
                let key = (row.section, row.path.clone());
                if multi_selected.contains(&key) {
                    multi_selected.remove(&key);
                } else {
                    multi_selected.insert(key);
                }
            }
        }
    }

    fn prune_multi_select_helper(visible_rows: &[VisibleRow], multi_selected: &mut MultiSelectSet) {
        multi_selected.retain(|(section, path)| {
            visible_rows
                .iter()
                .any(|r| r.section == *section && &r.path == path)
        });
    }

    fn get_action_targets_helper(
        highlight_index: Option<usize>,
        visible_rows: &[VisibleRow],
        multi_selected: &MultiSelectSet,
    ) -> Vec<(Section, String)> {
        if multi_selected.is_empty() {
            if let Some(idx) = highlight_index {
                if let Some(row) = visible_rows.get(idx) {
                    return vec![(row.section, row.path.clone())];
                }
            }
            vec![]
        } else {
            multi_selected.iter().cloned().collect()
        }
    }

    fn move_highlight_helper(
        highlight_index: Option<usize>,
        visible_rows: &[VisibleRow],
        delta: isize,
    ) -> Option<usize> {
        if visible_rows.is_empty() {
            return None;
        }
        let current = highlight_index.unwrap_or(0) as isize;
        let new_idx = (current + delta).clamp(0, visible_rows.len() as isize - 1) as usize;
        Some(new_idx)
    }

    fn count_headers_before_helper(
        staged_count: usize,
        unstaged_count: usize,
        file_idx: usize,
    ) -> usize {
        let mut headers = 0;
        if staged_count > 0 {
            headers += 1;
        }
        if unstaged_count > 0 && file_idx >= staged_count {
            headers += 1;
        }
        headers
    }

    /// Minimal test harness that uses shared helper functions.
    struct TestApp {
        staged_files: Vec<FileEntry>,
        unstaged_files: Vec<FileEntry>,
        highlight_index: Option<usize>,
        visible_rows: Vec<VisibleRow>,
        multi_selected: MultiSelectSet,
    }

    impl TestApp {
        fn new(staged: Vec<FileEntry>, unstaged: Vec<FileEntry>) -> Self {
            let visible_rows = build_visible_rows(&staged, &unstaged);
            let highlight_index = if visible_rows.is_empty() {
                None
            } else {
                Some(0)
            };
            Self {
                staged_files: staged,
                unstaged_files: unstaged,
                highlight_index,
                visible_rows,
                multi_selected: MultiSelectSet::new(),
            }
        }

        fn count_headers_before(&self, file_idx: usize) -> usize {
            count_headers_before_helper(
                self.staged_files.len(),
                self.unstaged_files.len(),
                file_idx,
            )
        }

        fn move_highlight(&mut self, delta: isize) {
            self.highlight_index =
                move_highlight_helper(self.highlight_index, &self.visible_rows, delta);
        }

        fn toggle_multi_select(&mut self) {
            toggle_multi_select_helper(
                self.highlight_index,
                &self.visible_rows,
                &mut self.multi_selected,
            );
        }

        fn clear_multi_select(&mut self) {
            self.multi_selected.clear();
        }

        fn prune_multi_select(&mut self) {
            prune_multi_select_helper(&self.visible_rows, &mut self.multi_selected);
        }

        fn get_action_targets(&self) -> Vec<(Section, String)> {
            get_action_targets_helper(
                self.highlight_index,
                &self.visible_rows,
                &self.multi_selected,
            )
        }
    }

    #[test]
    fn count_headers_before_index_0_with_staged() {
        let app = TestApp::new(vec![file_entry("a.rs")], vec![]);
        assert_eq!(app.count_headers_before(0), 1);
    }

    #[test]
    fn count_headers_before_in_unstaged_section() {
        let app = TestApp::new(vec![file_entry("a.rs")], vec![file_entry("b.rs")]);
        assert_eq!(app.count_headers_before(1), 2);
    }

    #[test]
    fn count_headers_before_only_unstaged() {
        let app = TestApp::new(vec![], vec![file_entry("a.rs")]);
        assert_eq!(app.count_headers_before(0), 1);
    }

    #[test]
    fn move_highlight_down_from_0() {
        let mut app = TestApp::new(vec![file_entry("a.rs"), file_entry("b.rs")], vec![]);
        app.move_highlight(1);
        assert_eq!(app.highlight_index, Some(1));
    }

    #[test]
    fn move_highlight_up_from_0_stays() {
        let mut app = TestApp::new(vec![file_entry("a.rs")], vec![]);
        app.move_highlight(-1);
        assert_eq!(app.highlight_index, Some(0));
    }

    #[test]
    fn move_highlight_down_past_end_clamps() {
        let mut app = TestApp::new(vec![file_entry("a.rs")], vec![]);
        app.move_highlight(10);
        assert_eq!(app.highlight_index, Some(0));
    }

    #[test]
    fn move_highlight_empty_no_panic() {
        let mut app = TestApp::new(vec![], vec![]);
        app.move_highlight(1);
        assert_eq!(app.highlight_index, None);
    }

    #[test]
    fn toggle_multi_select_adds_file() {
        let mut app = TestApp::new(vec![file_entry("a.rs")], vec![]);
        assert!(app.multi_selected.is_empty());
        app.toggle_multi_select();
        assert_eq!(app.multi_selected.len(), 1);
        assert!(app
            .multi_selected
            .contains(&(Section::Staged, "a.rs".to_string())));
    }

    #[test]
    fn toggle_multi_select_removes_file() {
        let mut app = TestApp::new(vec![file_entry("a.rs")], vec![]);
        app.toggle_multi_select();
        assert_eq!(app.multi_selected.len(), 1);
        app.toggle_multi_select();
        assert!(app.multi_selected.is_empty());
    }

    #[test]
    fn multi_select_persists_across_navigation() {
        let mut app = TestApp::new(vec![file_entry("a.rs"), file_entry("b.rs")], vec![]);
        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();
        assert_eq!(app.multi_selected.len(), 2);
        assert!(app
            .multi_selected
            .contains(&(Section::Staged, "a.rs".to_string())));
        assert!(app
            .multi_selected
            .contains(&(Section::Staged, "b.rs".to_string())));
    }

    #[test]
    fn clear_multi_select_clears_all() {
        let mut app = TestApp::new(vec![file_entry("a.rs"), file_entry("b.rs")], vec![]);
        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();
        assert_eq!(app.multi_selected.len(), 2);
        app.clear_multi_select();
        assert!(app.multi_selected.is_empty());
    }

    #[test]
    fn prune_multi_select_removes_deleted_files() {
        let mut app = TestApp::new(
            vec![file_entry("a.rs"), file_entry("b.rs")],
            vec![file_entry("c.rs")],
        );
        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();
        assert_eq!(app.multi_selected.len(), 3);

        app.visible_rows = build_visible_rows(&[file_entry("a.rs")], &[]);
        app.prune_multi_select();
        assert_eq!(app.multi_selected.len(), 1);
        assert!(app
            .multi_selected
            .contains(&(Section::Staged, "a.rs".to_string())));
    }

    #[test]
    fn get_action_targets_returns_highlighted_when_no_multi_select() {
        let app = TestApp::new(vec![file_entry("a.rs")], vec![]);
        let targets = app.get_action_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], (Section::Staged, "a.rs".to_string()));
    }

    #[test]
    fn get_action_targets_returns_multi_selected_when_present() {
        let mut app = TestApp::new(vec![file_entry("a.rs"), file_entry("b.rs")], vec![]);
        app.toggle_multi_select();
        app.move_highlight(1);
        app.toggle_multi_select();
        let targets = app.get_action_targets();
        assert_eq!(targets.len(), 2);
    }
}
