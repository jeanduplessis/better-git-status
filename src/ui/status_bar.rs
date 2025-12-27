use crate::types::BranchInfo;
use crate::ui::colors;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    branch: &BranchInfo,
    staged_count: usize,
    unstaged_count: usize,
    untracked_count: usize,
) {
    let branch_display = branch.display();

    let line = Line::from(vec![
        Span::styled(&branch_display, Style::default().fg(colors::CYAN)),
        Span::raw(" "),
        Span::styled("S:", Style::default().fg(colors::TEXT)),
        Span::styled(staged_count.to_string(), Style::default().fg(colors::GREEN)),
        Span::raw(" "),
        Span::styled("U:", Style::default().fg(colors::TEXT)),
        Span::styled(
            unstaged_count.to_string(),
            Style::default().fg(colors::YELLOW),
        ),
        Span::raw(" "),
        Span::styled("?:", Style::default().fg(colors::TEXT)),
        Span::styled(
            untracked_count.to_string(),
            Style::default().fg(colors::GRAY),
        ),
    ]);

    let paragraph = Paragraph::new(line).style(Style::default().bg(colors::SURFACE));
    frame.render_widget(paragraph, area);
}
