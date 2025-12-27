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
