use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use tui_textarea::TextArea;
use unicode_width::UnicodeWidthChar;

use crate::app::{AppMode, ConfirmChoice, FileTree, Overlay, TreeNodeKind};
use crate::core::TextBuffer;

const ASCII_FULL: [&str; 9] = [
    "▓█████▄  ▒█████   ██▓███  ▓█████  ██▓███   ▄▄▄      ▓█████▄",
    "▒██▀ ██▌▒██▒  ██▒▓██░  ██▒▓█   ▀ ▓██░  ██▒▒████▄    ▒██▀ ██▌",
    "░██   █▌▒██░  ██▒▓██░ ██▓▒▒███   ▓██░ ██▓▒▒██  ▀█▄  ░██   █▌",
    "░▓█▄   ▌▒██   ██░▒██▄█▓▒ ▒▒▓█  ▄ ▒██▄█▓▒ ▒░██▄▄▄▄██ ░▓█▄   ▌",
    "░▒████▓ ░ ████▓▒░▒██▒ ░  ░░▒████▒▒██▒ ░  ░ ▓█   ▓██▒░▒████▓",
    "▒▒▓  ▒ ░ ▒░▒░▒░ ▒▓▒░ ░  ░░░ ▒░ ░▒▓▒░ ░  ░ ▒▒   ▓▒█░ ▒▒▓  ▒",
    "░ ▒  ▒   ░ ▒ ▒░ ░▒ ░      ░ ░  ░░▒ ░       ▒   ▒▒ ░ ░ ▒  ▒",
    "░ ░  ░ ░ ░ ░ ▒  ░░          ░   ░░         ░   ▒    ░ ░  ░",
    "░        ░ ░              ░  ░               ░  ░   ░",
];

const ASCII_COMPACT: [&str; 4] = [
    "░██   █▌▒██░  ██▒▓██░ ██▓▒▒███   ▓██░ ██▓▒▒██  ▀█▄  ░██   █▌",
    "░▓█▄   ▌▒██   ██░▒██▄█▓▒ ▒▒▓█  ▄ ▒██▄█▓▒ ▒░██▄▄▄▄██ ░▓█▄   ▌",
    "░▒████▓ ░ ████▓▒░▒██▒ ░  ░░▒████▒▒██▒ ░  ░ ▓█   ▓██▒░▒████▓",
    "▒▒▓  ▒ ░ ▒░▒░▒░ ▒▓▒░ ░  ░░░ ▒░ ░▒▓▒░ ░  ░ ▒▒   ▓▒█░ ▒▒▓  ▒",
];

const ASCII_MICRO: &str = "▓█████▄  ▒█████   ██▓███  ▓█████  ██▓███";
pub struct UiModel<'a> {
    pub buffer: &'a TextBuffer,
    pub mode: AppMode,
    pub overlay: &'a Overlay,
    pub file_title: String,
    pub hint: String,
    pub no_style: bool,
    pub file_tree: &'a FileTree,
    pub categories: &'a [String],
}

pub fn draw(frame: &mut Frame<'_>, model: UiModel<'_>) {
    let size = frame.area();
    if size.width >= 100 {
        draw_wide(frame, size, model);
    } else {
        draw_narrow(frame, size, model);
    }
}

fn draw_wide(frame: &mut Frame<'_>, area: Rect, model: UiModel<'_>) {
    let sidebar_width = 68.min(area.width.saturating_sub(20)).max(28);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_width), Constraint::Min(20)])
        .split(chunks[0]);

    draw_ascii_sidebar(frame, body[0], &model);
    let cursor = draw_editor(frame, body[1], &model);
    draw_status(frame, chunks[1], &model);

    if let Some((x, y)) = cursor {
        frame.set_cursor_position((x, y));
    }
    draw_overlay(frame, area, model.overlay, model.categories);
}

fn draw_narrow(frame: &mut Frame<'_>, area: Rect, model: UiModel<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    let mut micro = ASCII_MICRO.to_string();
    if micro.len() > chunks[0].width as usize {
        micro.truncate(chunks[0].width as usize);
    }
    let header = Paragraph::new(Line::from(vec![
        Span::styled(micro, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" | DopePad | Ctrl+O Tree"),
    ]));
    frame.render_widget(header, chunks[0]);

    let cursor = draw_editor(frame, chunks[1], &model);
    draw_status(frame, chunks[2], &model);
    if let Some((x, y)) = cursor {
        frame.set_cursor_position((x, y));
    }
    draw_overlay(frame, area, model.overlay, model.categories);
}

fn draw_ascii_sidebar(frame: &mut Frame<'_>, area: Rect, model: &UiModel<'_>) {
    let variant = if area.height >= 22 && area.width >= 64 {
        &ASCII_FULL[..]
    } else if area.height >= 12 {
        &ASCII_COMPACT[..]
    } else {
        &[ASCII_MICRO]
    };

    let ascii_height = (variant.len() as u16).min(area.height.saturating_sub(6));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(ascii_height.max(1)),
            Constraint::Length(6),
            Constraint::Min(1),
        ])
        .split(area);

    let ascii_lines: Vec<Line<'_>> = variant.iter().map(|l| Line::from(*l)).collect();
    frame.render_widget(
        Paragraph::new(ascii_lines)
            .block(Block::default().title("DopePad").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let hotkeys = vec![
        Line::from("Ctrl+O Tree"),
        Line::from("Ctrl+N New File"),
        Line::from("C New Category"),
        Line::from("Del/D Delete Note"),
        Line::from("Ctrl+S Save"),
        Line::from("Ctrl+Shift+S Save As"),
    ];
    frame.render_widget(
        Paragraph::new(hotkeys).block(Block::default().title("Hotkeys").borders(Borders::ALL)),
        chunks[1],
    );

    let mut tree_lines = Vec::new();
    for (idx, node) in model.file_tree.nodes.iter().enumerate() {
        let selected = model.file_tree.focus && idx == model.file_tree.selected;
        let marker = if selected { ">" } else { " " };
        let style = match node.kind {
            TreeNodeKind::Category => Style::default().add_modifier(Modifier::BOLD),
            TreeNodeKind::Empty => Style::default().fg(Color::DarkGray),
            TreeNodeKind::File => Style::default(),
        };
        tree_lines.push(Line::from(vec![
            Span::raw(format!("{marker} ")),
            Span::styled(node.label.clone(), style),
        ]));
    }
    if tree_lines.is_empty() {
        tree_lines.push(Line::from("Tree is empty."));
        tree_lines.push(Line::from("Press C to create a category."));
    }

    frame.render_widget(
        Paragraph::new(tree_lines)
            .block(Block::default().title("Files").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );
}

fn draw_status(frame: &mut Frame<'_>, area: Rect, model: &UiModel<'_>) {
    let dirty = if model.buffer.dirty { "*" } else { "" };
    let mode = match model.mode {
        AppMode::Edit => "EDIT",
        AppMode::ReadOnly => "READONLY",
    };
    let ln = model.buffer.cursor.line + 1;
    let col = model.buffer.cursor.col + 1;
    let color = model
        .buffer
        .current_char_color()
        .map(|c| format!("C{c}"))
        .unwrap_or_else(|| "C0".to_string());
    let text = format!(
        " {}{} | {} | Ln {}, Col {} | {} | {}",
        model.file_title, dirty, mode, ln, col, color, model.hint
    );
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_editor(frame: &mut Frame<'_>, area: Rect, model: &UiModel<'_>) -> Option<(u16, u16)> {
    let block = Block::default().borders(Borders::ALL).title("Editor");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 2 || inner.height < 1 {
        return None;
    }

    let buffer = model.buffer;
    let top = buffer.viewport.top_line;
    let height = inner.height as usize;
    let mut lines: Vec<Line<'_>> = Vec::with_capacity(height);
    let mut cursor_xy: Option<(u16, u16)> = None;

    for row in 0..height {
        let line_idx = top + row;
        if line_idx >= buffer.line_count() {
            lines.push(Line::from("~"));
            continue;
        }

        let source = buffer.line_text(line_idx);
        let line_start_idx = buffer.line_start_char_idx(line_idx);
        let (mut line, cursor_x_on_line) = render_styled_line(
            buffer,
            &source,
            line_start_idx,
            buffer.viewport.left_col,
            inner.width as usize,
            buffer.cursor.col,
            line_idx == buffer.cursor.line,
            model.no_style,
        );

        if line_idx == buffer.cursor.line {
            line.style = line.style.add_modifier(Modifier::UNDERLINED);
        }
        lines.push(line);

        if line_idx == buffer.cursor.line {
            let x = inner.x + cursor_x_on_line as u16;
            let y = inner.y + row as u16;
            cursor_xy = Some((x, y));
        }
    }

    if let Overlay::Search { state, .. } = model.overlay {
        if let Some(curr) = state.current {
            if let Some(&line_idx) = state.matches.get(curr) {
                if line_idx >= top && line_idx < top + height {
                    let row = line_idx - top;
                    let mut st = lines[row].style;
                    st = st.add_modifier(Modifier::UNDERLINED);
                    lines[row].style = st;
                }
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
    cursor_xy
}

fn render_styled_line(
    buffer: &TextBuffer,
    source: &str,
    line_start_idx: usize,
    left_col: usize,
    max_cols: usize,
    cursor_col: usize,
    cursor_line: bool,
    no_style: bool,
) -> (Line<'static>, usize) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut col = 0usize;
    let mut cursor_x = 0usize;
    let mut char_idx_in_line = 0usize;

    for ch in source.chars() {
        let (render_chars, source_width) = if ch == '\t' {
            let spaces = 4 - (col % 4);
            (vec![' '; spaces], spaces)
        } else {
            let w = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
            (vec![ch], w)
        };
        let next_col = col + source_width;
        if next_col <= left_col {
            col = next_col;
            char_idx_in_line += 1;
            continue;
        }
        if col >= left_col + max_cols {
            break;
        }

        if cursor_line && col <= cursor_col && cursor_col < next_col {
            cursor_x = col.saturating_sub(left_col);
        }

        let mut style = Style::default();
        if !no_style {
            if let Some(cid) = buffer.char_color(line_start_idx + char_idx_in_line) {
                style = style.fg(color_for_id(cid));
            }
        }
        for rc in render_chars {
            if col >= left_col + max_cols {
                break;
            }
            spans.push(Span::styled(rc.to_string(), style));
            col += 1;
        }
        if ch != '\t' {
            col = next_col;
        } else if col < next_col {
            col = next_col;
        }
        char_idx_in_line += 1;
    }

    if cursor_line && cursor_col >= col {
        cursor_x = cursor_col
            .saturating_sub(left_col)
            .min(max_cols.saturating_sub(1));
    }

    (Line::from(spans), cursor_x)
}

fn color_for_id(id: u8) -> Color {
    match id {
        1 => Color::Yellow,
        2 => Color::Cyan,
        3 => Color::Green,
        4 => Color::Blue,
        5 => Color::Red,
        6 => Color::Magenta,
        7 => Color::LightYellow,
        8 => Color::LightCyan,
        _ => Color::Reset,
    }
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, overlay: &Overlay, categories: &[String]) {
    match overlay {
        Overlay::None => {}
        Overlay::Help => {
            let rect = centered_rect(70, 70, area);
            frame.render_widget(Clear, rect);
            let text = vec![
                Line::from("F1 Help | Ctrl+F Search | Ctrl+G Goto | Ctrl+O Tree"),
                Line::from("Ctrl+N New | Ctrl+S Save | Ctrl+Shift+S Save As | Ctrl+Q Quit"),
                Line::from("F2..F9 set char color | F10 reset color"),
                Line::from("Tree mode: Up/Down, Enter open, N new, Del/D delete, Esc back"),
                Line::from("Esc close overlay"),
            ];
            let widget = Paragraph::new(text)
                .alignment(Alignment::Left)
                .block(Block::default().title("Help").borders(Borders::ALL));
            frame.render_widget(widget, rect);
        }
        Overlay::Search { input, state } => {
            let rect = centered_rect(70, 20, area);
            frame.render_widget(Clear, rect);
            let info = state
                .current
                .map(|i| format!("{}/{}", i + 1, state.matches.len()))
                .unwrap_or_else(|| "0/0".to_string());
            let mut textarea = TextArea::default();
            textarea.insert_str(input);
            textarea.set_block(Block::default().title("Search").borders(Borders::ALL));
            frame.render_widget(&textarea, rect);

            let footer = Rect {
                x: rect.x + 2,
                y: rect.y + rect.height.saturating_sub(1),
                width: rect.width.saturating_sub(4),
                height: 1,
            };
            frame.render_widget(Paragraph::new(format!("Matches: {info}")), footer);
        }
        Overlay::Goto { input } => {
            let rect = centered_rect(40, 20, area);
            frame.render_widget(Clear, rect);
            let mut textarea = TextArea::default();
            textarea.insert_str(input);
            textarea.set_block(Block::default().title("Goto Line").borders(Borders::ALL));
            frame.render_widget(&textarea, rect);
        }
        Overlay::SaveAs {
            filename,
            category_index,
        } => {
            let rect = centered_rect(80, 40, area);
            frame.render_widget(Clear, rect);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(3),
                    Constraint::Length(2),
                ])
                .split(rect);

            let mut textarea = TextArea::default();
            textarea.insert_str(filename);
            textarea.set_block(Block::default().title("File name").borders(Borders::ALL));
            frame.render_widget(&textarea, chunks[0]);

            let mut lines = Vec::new();
            for (idx, category) in categories.iter().enumerate() {
                let marker = if idx == *category_index { ">" } else { " " };
                lines.push(Line::from(format!("{marker} {category}")));
            }
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .title("Category (Up/Down)")
                        .borders(Borders::ALL),
                ),
                chunks[1],
            );

            frame.render_widget(
                Paragraph::new("Enter save | Esc cancel")
                    .block(Block::default().borders(Borders::ALL)),
                chunks[2],
            );
        }
        Overlay::NewFile {
            filename,
            category_index,
        } => {
            let rect = centered_rect(80, 40, area);
            frame.render_widget(Clear, rect);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(3),
                    Constraint::Length(2),
                ])
                .split(rect);

            let mut textarea = TextArea::default();
            textarea.insert_str(filename);
            textarea.set_block(Block::default().title("New file").borders(Borders::ALL));
            frame.render_widget(&textarea, chunks[0]);

            let mut lines = Vec::new();
            for (idx, category) in categories.iter().enumerate() {
                let marker = if idx == *category_index { ">" } else { " " };
                lines.push(Line::from(format!("{marker} {category}")));
            }
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .title("Category (Up/Down)")
                        .borders(Borders::ALL),
                ),
                chunks[1],
            );

            frame.render_widget(
                Paragraph::new("Enter create and open | Esc cancel")
                    .block(Block::default().borders(Borders::ALL)),
                chunks[2],
            );
        }
        Overlay::NewCategory { name, .. } => {
            let rect = centered_rect(65, 25, area);
            frame.render_widget(Clear, rect);
            let mut textarea = TextArea::default();
            textarea.insert_str(name);
            textarea.set_block(Block::default().title("New category").borders(Borders::ALL));
            frame.render_widget(&textarea, rect);
            let footer = Rect {
                x: rect.x + 2,
                y: rect.y + rect.height.saturating_sub(1),
                width: rect.width.saturating_sub(4),
                height: 1,
            };
            frame.render_widget(Paragraph::new("Enter create | Esc cancel"), footer);
        }
        Overlay::ConfirmUnsaved {
            file_name, choice, ..
        } => {
            let rect = centered_rect(70, 30, area);
            frame.render_widget(Clear, rect);
            let yes = if *choice == ConfirmChoice::Yes {
                "[Yes]"
            } else {
                " Yes "
            };
            let no = if *choice == ConfirmChoice::No {
                "[No]"
            } else {
                " No "
            };
            let widget = Paragraph::new(format!(
                "Unsaved changes in {}.\nSave before leaving?\n\n{}   {}",
                file_name, yes, no
            ))
            .alignment(Alignment::Center)
            .block(Block::default().title("Confirm").borders(Borders::ALL));
            frame.render_widget(widget, rect);
        }
        Overlay::ConfirmDelete {
            file_name, choice, ..
        } => {
            let rect = centered_rect(70, 30, area);
            frame.render_widget(Clear, rect);
            let yes = if *choice == ConfirmChoice::Yes {
                "[Yes]"
            } else {
                " Yes "
            };
            let no = if *choice == ConfirmChoice::No {
                "[No]"
            } else {
                " No "
            };
            let widget = Paragraph::new(format!(
                "Do you want to delete {}?\n\n{}   {}",
                file_name, yes, no
            ))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title("Confirm Delete")
                    .borders(Borders::ALL),
            );
            frame.render_widget(widget, rect);
        }
        Overlay::Error { message } => {
            let rect = centered_rect(80, 30, area);
            frame.render_widget(Clear, rect);
            let widget = Paragraph::new(message.as_str())
                .alignment(Alignment::Left)
                .block(Block::default().title("Error").borders(Borders::ALL));
            frame.render_widget(widget, rect);
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
