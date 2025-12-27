use crate::types::{DiffContent, DiffLine, DiffLineKind};
use crate::ui::colors;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(frame: &mut Frame, area: Rect, diff: &DiffContent, scroll: usize) {
    let inner_height = area.height.saturating_sub(2) as usize;

    let (lines, total_lines) = match diff {
        DiffContent::Empty => {
            let placeholder = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "↑/↓ navigate, Space to view diff",
                    Style::default().fg(colors::GRAY),
                )),
            ];
            (placeholder, 2)
        }
        DiffContent::Clean => {
            let placeholder = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No changes (q to quit)",
                    Style::default().fg(colors::GRAY),
                )),
            ];
            (placeholder, 2)
        }
        DiffContent::Binary => {
            let placeholder = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Binary file",
                    Style::default().fg(colors::GRAY),
                )),
            ];
            (placeholder, 2)
        }
        DiffContent::InvalidUtf8 => {
            let placeholder = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "File contains invalid UTF-8 encoding",
                    Style::default().fg(colors::GRAY),
                )),
            ];
            (placeholder, 2)
        }
        DiffContent::Conflict => {
            let placeholder = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Conflict - resolve before viewing diff",
                    Style::default().fg(colors::MAGENTA),
                )),
            ];
            (placeholder, 2)
        }
        DiffContent::Text(diff_lines) => {
            let lines = render_diff_lines(diff_lines, area.width.saturating_sub(2) as usize);
            let len = lines.len();
            (lines, len)
        }
    };

    let scroll_offset = scroll.min(total_lines.saturating_sub(inner_height));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors::OVERLAY))
                .title("Diff"),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_diff_lines(diff_lines: &[DiffLine], _width: usize) -> Vec<Line<'static>> {
    let max_line_num = diff_lines
        .iter()
        .filter_map(|l| l.new_line_number)
        .max()
        .unwrap_or(0);
    let line_num_width = max_line_num.to_string().len().max(3);

    diff_lines
        .iter()
        .map(|line| {
            let (line_num_str, content_style) = match line.kind {
                DiffLineKind::Header => (
                    format!("{:>width$} │", "", width = line_num_width),
                    Style::default().fg(colors::CYAN),
                ),
                DiffLineKind::Hunk => (
                    format!("{:>width$} │", "", width = line_num_width),
                    Style::default().fg(colors::CYAN),
                ),
                DiffLineKind::Context => {
                    let num = line
                        .new_line_number
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    (
                        format!("{:>width$} │", num, width = line_num_width),
                        Style::default().fg(colors::TEXT),
                    )
                }
                DiffLineKind::Added => {
                    let num = line
                        .new_line_number
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    (
                        format!("{:>width$} │", num, width = line_num_width),
                        Style::default().fg(colors::GREEN),
                    )
                }
                DiffLineKind::Deleted => (
                    format!("{:>width$} │", "-", width = line_num_width),
                    Style::default().fg(colors::RED),
                ),
            };

            let prefix = match line.kind {
                DiffLineKind::Added => "+",
                DiffLineKind::Deleted => "-",
                DiffLineKind::Context => " ",
                _ => "",
            };

            Line::from(vec![
                Span::styled(line_num_str, Style::default().fg(colors::GRAY)),
                Span::styled(prefix, content_style),
                Span::styled(line.content.clone(), content_style),
            ])
        })
        .collect()
}

pub fn max_scroll(diff: &DiffContent, viewport_height: usize) -> usize {
    let total = match diff {
        DiffContent::Text(lines) => lines.len(),
        _ => 0,
    };
    total.saturating_sub(viewport_height)
}
