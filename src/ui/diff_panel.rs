use crate::types::{DiffContent, DiffLine, DiffLineKind};
use crate::ui::colors;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
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
        .scroll((scroll_offset as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_diff_lines(diff_lines: &[DiffLine], width: usize) -> Vec<Line<'static>> {
    let max_line_num = diff_lines
        .iter()
        .filter_map(|l| l.new_line_number)
        .max()
        .unwrap_or(0);
    let line_num_width = max_line_num.to_string().len().max(3);
    let gutter_width = line_num_width + 3; // " │" + prefix char

    let content_width = width.saturating_sub(gutter_width);

    diff_lines
        .iter()
        .flat_map(|line| {
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

            let content = &line.content;
            let continuation_gutter = format!("{:>width$} │ ", "", width = line_num_width);

            if content_width == 0 || content.is_empty() {
                return vec![Line::from(vec![
                    Span::styled(line_num_str, Style::default().fg(colors::GRAY)),
                    Span::styled(prefix, content_style),
                    Span::styled(content.clone(), content_style),
                ])];
            }

            let mut result_lines = Vec::new();
            let mut chars: Vec<char> = content.chars().collect();
            let mut first = true;

            while !chars.is_empty() {
                let take = if first {
                    content_width.saturating_sub(1) // account for prefix
                } else {
                    content_width
                };
                let chunk: String = chars.drain(..take.min(chars.len())).collect();

                if first {
                    result_lines.push(Line::from(vec![
                        Span::styled(line_num_str.clone(), Style::default().fg(colors::GRAY)),
                        Span::styled(prefix, content_style),
                        Span::styled(chunk, content_style),
                    ]));
                    first = false;
                } else {
                    result_lines.push(Line::from(vec![
                        Span::styled(continuation_gutter.clone(), Style::default().fg(colors::GRAY)),
                        Span::styled(chunk, content_style),
                    ]));
                }
            }

            result_lines
        })
        .collect()
}

/// Calculate the maximum scroll offset for the diff content.
pub fn max_scroll(diff: &DiffContent, viewport_height: usize, viewport_width: usize) -> usize {
    let total = match diff {
        DiffContent::Text(lines) => {
            let rendered = render_diff_lines(lines, viewport_width);
            rendered.len()
        }
        _ => 0,
    };
    total.saturating_sub(viewport_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_scroll_empty() {
        assert_eq!(max_scroll(&DiffContent::Empty, 10, 80), 0);
        assert_eq!(max_scroll(&DiffContent::Clean, 10, 80), 0);
        assert_eq!(max_scroll(&DiffContent::Binary, 10, 80), 0);
        assert_eq!(max_scroll(&DiffContent::InvalidUtf8, 10, 80), 0);
        assert_eq!(max_scroll(&DiffContent::Conflict, 10, 80), 0);
    }

    #[test]
    fn test_max_scroll_text() {
        let lines: Vec<DiffLine> = (0..20)
            .map(|i| DiffLine {
                kind: DiffLineKind::Context,
                content: format!("line {}", i),
                new_line_number: Some(i + 1),
            })
            .collect();

        let diff = DiffContent::Text(lines);

        // 20 lines, viewport 10, wide enough: can scroll 10
        assert_eq!(max_scroll(&diff, 10, 80), 10);

        // 20 lines, viewport 20: no scroll
        assert_eq!(max_scroll(&diff, 20, 80), 0);

        // 20 lines, viewport 30: no scroll
        assert_eq!(max_scroll(&diff, 30, 80), 0);
    }
}
