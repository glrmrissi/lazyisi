use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, DiffLine, EntryKind, FileSection, Mode, Pane};

pub fn draw(f: &mut Frame, app: &App) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(vertical[0]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(horizontal[0]);

    draw_files(f, app, left[0]);
    draw_log(f, app, left[1]);
    draw_diff(f, app, horizontal[1]);
    draw_statusbar(f, app, vertical[1]);

    if let Mode::CommitInput = app.mode {
        draw_commit_popup(f, app);
    }
}

fn draw_files(f: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_flat_idx = None;
    let mut flat_idx = 0usize;

    if !app.unstaged.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            format!(" Unstaged ({}) ", app.unstaged.len()),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))));

        for (i, entry) in app.unstaged.iter().enumerate() {
            let is_sel = app.pane == Pane::Files
                && app.file_section == FileSection::Unstaged
                && i == app.unstaged_idx;

            if is_sel {
                selected_flat_idx = Some(flat_idx);
            }
            flat_idx += 1;

            let (sigil, color) = match entry.kind {
                EntryKind::Modified => ("M", Color::Yellow),
                EntryKind::Deleted => ("D", Color::Red),
                EntryKind::Untracked => ("?", Color::Green),
            };

            let style = if is_sel {
                Style::default().bg(Color::DarkGray).fg(color)
            } else {
                Style::default().fg(color)
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {sigil} "), style),
                Span::styled(entry.path.clone(), style),
            ])));
        }
    }

    if !app.untracked.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            format!(" Untracked ({}) ", app.untracked.len()),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ))));

        for (i, entry) in app.untracked.iter().enumerate() {
            let is_sel = app.pane == Pane::Files
                && app.file_section == FileSection::Untracked
                && i == app.untracked_idx;

            if is_sel {
                selected_flat_idx = Some(flat_idx);
            }
            flat_idx += 1;

            let style = if is_sel {
                Style::default().bg(Color::DarkGray).fg(Color::Green)
            } else {
                Style::default().fg(Color::Green)
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled("  ? ", style),
                Span::styled(entry.path.clone(), style),
            ])));
        }
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "  nothing to show",
            Style::default().fg(Color::DarkGray),
        ))));
    }

    let border_style = if app.pane == Pane::Files {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items).block(
        Block::default()
            .title(" Files ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    let mut state = ListState::default();
    // offset by header items when selecting
    if let Some(idx) = selected_flat_idx {
        let header_count = usize::from(!app.unstaged.is_empty())
            + usize::from(!app.untracked.is_empty() && app.file_section == FileSection::Untracked);
        state.select(Some(idx + header_count.saturating_sub(
            if app.file_section == FileSection::Untracked && !app.unstaged.is_empty() { 0 } else { 0 }
        )));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_log(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = if app.log.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  no commits yet",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.log
            .iter()
            .enumerate()
            .map(|(i, commit)| {
                let is_sel = app.pane == Pane::Log && i == app.log_idx;
                let hash_short = &commit.hash[..7.min(commit.hash.len())];

                let style = if is_sel {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {hash_short} "), style.fg(Color::Yellow)),
                    Span::styled(commit.message.clone(), style),
                ]))
            })
            .collect()
    };

    let border_style = if app.pane == Pane::Log {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items).block(
        Block::default()
            .title(" Log ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    let mut state = ListState::default();
    if app.pane == Pane::Log && !app.log.is_empty() {
        state.select(Some(app.log_idx));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_diff(f: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .diff
        .iter()
        .skip(app.diff_scroll)
        .map(|d| match d {
            DiffLine::Header(s) => Line::from(Span::styled(
                s.clone(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            DiffLine::Added(s) => Line::from(Span::styled(
                format!("+ {s}"),
                Style::default().fg(Color::Green),
            )),
            DiffLine::Removed(s) => Line::from(Span::styled(
                format!("- {s}"),
                Style::default().fg(Color::Red),
            )),
            DiffLine::Context(s) => Line::from(Span::raw(format!("  {s}"))),
        })
        .collect();

    let para = Paragraph::new(lines)
        .block(Block::default().title(" Diff ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn draw_statusbar(f: &mut Frame, app: &App, area: Rect) {
    let help = " q:quit  a:add  c:commit  Tab:switch pane  ↑↓/jk:navigate  JK:scroll diff";
    let text = match &app.status {
        Some(s) => format!("{help}  │  {s}"),
        None => help.to_string(),
    };

    let para = Paragraph::new(text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(para, area);
}

fn draw_commit_popup(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 3, f.area());

    let block = Block::default()
        .title(" Commit message — Enter to confirm, Esc to cancel ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(app.commit_msg.as_str()).block(block);

    f.render_widget(Clear, area);
    f.render_widget(para, area);

    let cursor_x = area.x + 1 + app.commit_msg.len() as u16;
    let cursor_y = area.y + 1;
    f.set_cursor_position((cursor_x, cursor_y));
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(height),
            Constraint::Percentage(45),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
