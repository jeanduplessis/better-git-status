use crate::types::{FileEntry, FileStatus, Section};
use crate::ui::colors;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    staged_files: &[FileEntry],
    unstaged_files: &[FileEntry],
    highlight_index: Option<usize>,
    selected: Option<&(Section, String)>,
    scroll_offset: usize,
) {
    let mut items: Vec<ListItem> = Vec::new();
    let mut current_index = 0usize;

    if !staged_files.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "[STAGED]",
            Style::default()
                .fg(colors::CYAN)
                .add_modifier(Modifier::BOLD),
        ))));

        for file in staged_files {
            let is_highlighted = highlight_index == Some(current_index);
            let is_selected = selected
                .map(|(s, p)| *s == Section::Staged && p == &file.path)
                .unwrap_or(false);
            items.push(create_file_item(
                file,
                is_highlighted,
                is_selected,
                area.width,
            ));
            current_index += 1;
        }
    }

    if !unstaged_files.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "[UNSTAGED]",
            Style::default()
                .fg(colors::CYAN)
                .add_modifier(Modifier::BOLD),
        ))));

        for file in unstaged_files {
            let is_highlighted = highlight_index == Some(current_index);
            let is_selected = selected
                .map(|(s, p)| *s == Section::Unstaged && p == &file.path)
                .unwrap_or(false);
            items.push(create_file_item(
                file,
                is_highlighted,
                is_selected,
                area.width,
            ));
            current_index += 1;
        }
    }

    let visible_height = area.height.saturating_sub(2) as usize;
    let start = scroll_offset.min(items.len().saturating_sub(1));
    let end = (start + visible_height).min(items.len());
    let visible_items: Vec<ListItem> = items.into_iter().skip(start).take(end - start).collect();

    let list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::OVERLAY)),
    );

    frame.render_widget(list, area);
}

fn create_file_item(
    file: &FileEntry,
    is_highlighted: bool,
    is_selected: bool,
    width: u16,
) -> ListItem<'static> {
    let prefix = match (is_highlighted, is_selected) {
        (true, true) => ">● ",
        (true, false) => ">  ",
        (false, true) => " ● ",
        (false, false) => "   ",
    };

    let status_color = get_status_color(file.status);
    let status_symbol = file.status.symbol();

    let counts = format_line_counts(file.added_lines, file.deleted_lines, file.is_binary);

    let indent_level = compute_indent(&file.path);
    let indent = "  ".repeat(indent_level.min(4));

    let fixed_width = prefix.len() + 2 + indent.len() + counts.len() + 2;
    let available_width = (width as usize).saturating_sub(fixed_width);

    let (path_display, show_counts) =
        format_path_with_priority(&file.path, &counts, available_width);

    let base_style = if is_highlighted {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let mut spans = vec![
        Span::styled(prefix, base_style.fg(colors::TEXT)),
        Span::styled(status_symbol, base_style.fg(status_color)),
        Span::styled(" ", base_style),
        Span::styled(indent.clone(), base_style),
        Span::styled(path_display, base_style.fg(colors::TEXT)),
    ];

    if show_counts && !counts.is_empty() {
        spans.push(Span::styled(
            format!(" {}", counts),
            Style::default().fg(colors::GRAY),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn compute_indent(path: &str) -> usize {
    path.matches('/').count()
}

fn format_path_with_priority(path: &str, counts: &str, available_width: usize) -> (String, bool) {
    let counts_len = if counts.is_empty() {
        0
    } else {
        counts.len() + 1
    };

    if path.len() + counts_len <= available_width {
        return (path.to_string(), true);
    }

    if path.len() <= available_width {
        return (path.to_string(), false);
    }

    let filename = path.rsplit('/').next().unwrap_or(path);

    if filename.len() < available_width {
        let remaining = available_width.saturating_sub(1);
        if path.len() <= remaining {
            return (path.to_string(), false);
        }
        return (
            format!("…{}", &path[path.len().saturating_sub(remaining)..]),
            false,
        );
    }

    if filename.len() <= available_width {
        return (filename.to_string(), false);
    }

    if available_width > 0 {
        return (filename.chars().take(available_width).collect(), false);
    }

    (String::new(), false)
}

fn get_status_color(status: FileStatus) -> ratatui::style::Color {
    match status {
        FileStatus::Added => colors::GREEN,
        FileStatus::Modified => colors::YELLOW,
        FileStatus::Deleted => colors::RED,
        FileStatus::Renamed => colors::BLUE,
        FileStatus::Untracked => colors::GRAY,
        FileStatus::Conflict => colors::MAGENTA,
    }
}

fn format_line_counts(added: Option<usize>, deleted: Option<usize>, is_binary: bool) -> String {
    if is_binary {
        return "-/-".to_string();
    }
    match (added, deleted) {
        (Some(a), Some(d)) => format!("+{}/-{}", a, d),
        _ => String::new(),
    }
}

pub fn calculate_height(staged_count: usize, unstaged_count: usize, max_height: u16) -> u16 {
    let mut total = 0;
    if staged_count > 0 {
        total += 1 + staged_count;
    }
    if unstaged_count > 0 {
        total += 1 + unstaged_count;
    }
    let content_height = (total as u16).saturating_add(2);
    content_height.min(max_height)
}
