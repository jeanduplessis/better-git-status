use crate::types::{BranchInfo, ConfirmPrompt, FlashMessage};
use crate::ui::colors;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub struct StatusBarState<'a> {
    pub branch: &'a BranchInfo,
    pub staged_count: usize,
    pub unstaged_count: usize,
    pub untracked_count: usize,
    pub confirm_prompt: Option<&'a ConfirmPrompt>,
    pub flash_message: Option<&'a FlashMessage>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: StatusBarState<'_>) {
    let line = if let Some(prompt) = state.confirm_prompt {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(&prompt.message, Style::default().fg(colors::YELLOW)),
        ])
    } else if let Some(flash) = state.flash_message {
        let (prefix, color) = if flash.is_error {
            ("✗ ", colors::RED)
        } else {
            ("✓ ", colors::GREEN)
        };
        Line::from(vec![
            Span::raw(" "),
            Span::styled(prefix, Style::default().fg(color)),
            Span::styled(&flash.text, Style::default().fg(color)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(state.branch.to_string(), Style::default().fg(colors::CYAN)),
            Span::raw(" "),
            Span::styled("S:", Style::default().fg(colors::TEXT)),
            Span::styled(
                state.staged_count.to_string(),
                Style::default().fg(colors::GREEN),
            ),
            Span::raw(" "),
            Span::styled("U:", Style::default().fg(colors::TEXT)),
            Span::styled(
                state.unstaged_count.to_string(),
                Style::default().fg(colors::YELLOW),
            ),
            Span::raw(" "),
            Span::styled("?:", Style::default().fg(colors::TEXT)),
            Span::styled(
                state.untracked_count.to_string(),
                Style::default().fg(colors::GRAY),
            ),
            Span::raw("  "),
            Span::styled("s", Style::default().fg(colors::CYAN)),
            Span::styled(":stage ", Style::default().fg(colors::GRAY)),
            Span::styled("u", Style::default().fg(colors::CYAN)),
            Span::styled(":unstage ", Style::default().fg(colors::GRAY)),
            Span::styled("q", Style::default().fg(colors::CYAN)),
            Span::styled(":quit", Style::default().fg(colors::GRAY)),
        ])
    };

    let paragraph = Paragraph::new(line).style(Style::default().bg(colors::SURFACE));
    frame.render_widget(paragraph, area);
}
