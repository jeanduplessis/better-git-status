use crate::git;
use crate::types::{BranchInfo, DiffContent, FileEntry, Section, VisibleRow};
use crate::ui;
use crate::watcher::{FileWatcher, WatcherEvent};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
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

/// Application state for the interactive git status TUI.
pub struct App {
    repo: Repository,

    pub(crate) staged_files: Vec<FileEntry>,
    pub(crate) unstaged_files: Vec<FileEntry>,

    pub(crate) highlight_index: Option<usize>,
    pub(crate) selected: Option<(Section, String)>,
    pub(crate) file_list_scroll: usize,

    pub(crate) current_diff: DiffContent,
    pub(crate) diff_scroll: usize,

    pub(crate) staged_count: usize,
    pub(crate) unstaged_count: usize,
    pub(crate) untracked_count: usize,

    pub(crate) branch: BranchInfo,

    visible_rows: Vec<VisibleRow>,

    pub(crate) file_list_height: usize,

    pub(crate) file_list_area: Rect,
    pub(crate) diff_area: Rect,
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
            self.current_diff = DiffContent::Clean;
            self.diff_scroll = 0;
            return Ok(());
        }

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
                    self.current_diff = git::get_diff(&self.repo, path, *section);
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

    fn move_highlight(&mut self, delta: isize) {
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

fn build_visible_rows(staged: &[FileEntry], unstaged: &[FileEntry]) -> Vec<VisibleRow> {
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
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Down => app.move_highlight(1),
                            KeyCode::Up => app.move_highlight(-1),
                            KeyCode::Char(' ') | KeyCode::Enter => app.select_current(),
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
                            _ => {}
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
                        eprintln!(
                            "Warning: file watcher disconnected. Falling back to polling."
                        );
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
    }

    Ok(())
}
