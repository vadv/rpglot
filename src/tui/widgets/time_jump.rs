//! Time jump input popup (history mode).

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

/// Renders a centered time jump popup.
pub fn render_time_jump(frame: &mut Frame, area: Rect, input: &str, error: Option<&str>) {
    // Calculate popup size (70% width, fixed height), clamped.
    let popup_width = (area.width * 70 / 100).clamp(50, 90);
    let popup_height = area.height.clamp(9, 13);

    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Jump to time (UTC) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Input: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                input,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Examples:",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "  -1h        (relative to current selected snapshot)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "  16:00      (time on selected day)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "  2026-02-07T17:00:00",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "  1738944000 (unix timestamp)",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    if let Some(err) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Error: {err}"),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::styled(" → jump", Style::default().fg(Color::DarkGray)),
        Span::styled("   Esc", Style::default().fg(Color::Yellow)),
        Span::styled(" → cancel", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, inner);
}
