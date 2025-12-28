pub mod colors;
pub mod diff_panel;
pub mod file_list;
pub mod status_bar;

use crate::app::App;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

const MIN_WIDTH: u16 = 30;
const MIN_HEIGHT: u16 = 10;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area);
        return;
    }

    let max_file_list_height = (area.height / 3).max(5);
    let file_list_height = file_list::calculate_height(
        app.staged_files.len(),
        app.unstaged_files.len(),
        max_file_list_height,
    );

    app.file_list_height = file_list_height.saturating_sub(2) as usize;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(file_list_height),
            Constraint::Min(5),
        ])
        .split(area);

    status_bar::draw(
        frame,
        chunks[0],
        &app.branch,
        app.staged_count,
        app.unstaged_count,
        app.untracked_count,
    );

    app.file_list_area = chunks[1];
    app.diff_area = chunks[2];

    file_list::draw(
        frame,
        chunks[1],
        &app.staged_files,
        &app.unstaged_files,
        app.highlight_index,
        app.selected.as_ref(),
        app.file_list_scroll,
    );

    diff_panel::draw(frame, chunks[2], &app.current_diff, app.diff_scroll);
}

fn draw_too_small(frame: &mut Frame, area: Rect) {
    let message = Paragraph::new(Line::from(Span::raw("Terminal too small")))
        .block(Block::default().borders(Borders::NONE))
        .style(Style::default().fg(colors::GRAY));
    frame.render_widget(message, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DiffContent, FileEntry, FileStatus, Section};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn test_file_entry(path: &str, status: FileStatus) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            old_path: None,
            status,
            added_lines: Some(5),
            deleted_lines: Some(3),
            is_binary: false,
            is_submodule: false,
        }
    }

    fn buffer_to_string(buffer: &Buffer) -> String {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn buffer_contains(buffer: &Buffer, text: &str) -> bool {
        let content = buffer_to_string(buffer);
        content.contains(text)
    }

    #[test]
    fn draw_too_small_shows_message() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw_too_small(frame, frame.area());
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "Terminal too small"));
    }

    #[test]
    fn file_list_shows_staged_header() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let staged = vec![test_file_entry("file.rs", FileStatus::Modified)];
        terminal
            .draw(|frame| {
                file_list::draw(
                    frame,
                    frame.area(),
                    &staged,
                    &[],
                    None,
                    None,
                    0,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "[STAGED]"));
    }

    #[test]
    fn file_list_shows_unstaged_header() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let unstaged = vec![test_file_entry("file.rs", FileStatus::Modified)];
        terminal
            .draw(|frame| {
                file_list::draw(
                    frame,
                    frame.area(),
                    &[],
                    &unstaged,
                    None,
                    None,
                    0,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "[UNSTAGED]"));
    }

    #[test]
    fn file_list_shows_both_headers() {
        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let staged = vec![test_file_entry("staged.rs", FileStatus::Added)];
        let unstaged = vec![test_file_entry("unstaged.rs", FileStatus::Modified)];
        terminal
            .draw(|frame| {
                file_list::draw(
                    frame,
                    frame.area(),
                    &staged,
                    &unstaged,
                    None,
                    None,
                    0,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "[STAGED]"));
        assert!(buffer_contains(&buffer, "[UNSTAGED]"));
    }

    #[test]
    fn file_list_shows_highlight_indicator() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let staged = vec![test_file_entry("file.rs", FileStatus::Modified)];
        terminal
            .draw(|frame| {
                file_list::draw(
                    frame,
                    frame.area(),
                    &staged,
                    &[],
                    Some(0),
                    None,
                    0,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, ">"));
    }

    #[test]
    fn file_list_shows_selection_indicator() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let staged = vec![test_file_entry("file.rs", FileStatus::Modified)];
        let selected = (Section::Staged, "file.rs".to_string());
        terminal
            .draw(|frame| {
                file_list::draw(
                    frame,
                    frame.area(),
                    &staged,
                    &[],
                    None,
                    Some(&selected),
                    0,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "●"));
    }

    #[test]
    fn file_list_shows_rename_arrow() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut entry = test_file_entry("new.rs", FileStatus::Renamed);
        entry.old_path = Some("old.rs".to_string());
        let staged = vec![entry];
        terminal
            .draw(|frame| {
                file_list::draw(
                    frame,
                    frame.area(),
                    &staged,
                    &[],
                    None,
                    None,
                    0,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "old.rs → new.rs"));
    }

    #[test]
    fn diff_panel_empty_shows_hint() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                diff_panel::draw(frame, frame.area(), &DiffContent::Empty, 0);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "navigate"));
    }

    #[test]
    fn diff_panel_clean_shows_message() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                diff_panel::draw(frame, frame.area(), &DiffContent::Clean, 0);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "No changes"));
    }

    #[test]
    fn diff_panel_binary_shows_message() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                diff_panel::draw(frame, frame.area(), &DiffContent::Binary, 0);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "Binary file"));
    }

    #[test]
    fn diff_panel_invalid_utf8_shows_message() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                diff_panel::draw(frame, frame.area(), &DiffContent::InvalidUtf8, 0);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "invalid UTF-8"));
    }

    #[test]
    fn diff_panel_conflict_shows_message() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                diff_panel::draw(frame, frame.area(), &DiffContent::Conflict, 0);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer_contains(&buffer, "Conflict"));
    }
}
