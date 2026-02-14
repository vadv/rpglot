//! PostgreSQL activity table widget for PGA tab.
//! Thin TUI wrapper over [`crate::view::pga::build_activity_view`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::tui::state::AppState;
use crate::tui::style::Styles;
use crate::view::pga::build_activity_view;

/// Renders the PostgreSQL activity table.
pub fn render_postgres(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Activity (PGA) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            let paragraph = Paragraph::new("No data available").block(block);
            frame.render_widget(paragraph, area);
            return;
        }
    };

    let vm = match build_activity_view(snapshot, &state.pga, &state.pgs, interner) {
        Some(vm) => vm,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Activity (PGA) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            let message = state
                .pga
                .last_error
                .as_deref()
                .unwrap_or("No active PostgreSQL sessions");
            let paragraph = Paragraph::new(message).block(block);
            frame.render_widget(paragraph, area);
            return;
        }
    };

    // Resolve selection
    let row_pids: Vec<i32> = vm.rows.iter().map(|r| r.id).collect();
    state.pga.resolve_selection(&row_pids);

    // Header with sort indicator
    let headers: Vec<Span> = vm
        .headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let indicator = if i == vm.sort_column {
                if vm.sort_ascending { "▲" } else { "▼" }
            } else {
                ""
            };
            Span::styled(format!("{}{}", h, indicator), Styles::table_header())
        })
        .collect();
    let header = Row::new(headers).style(Styles::table_header()).height(1);

    // Rows
    let rows: Vec<Row> = vm
        .rows
        .iter()
        .enumerate()
        .map(|(idx, vr)| {
            let is_selected = idx == state.pga.selected;
            let row_style = if is_selected {
                Styles::selected()
            } else {
                Styles::from_class(vr.style)
            };

            let cells = vr.cells.iter().map(|c| match c.style {
                Some(s) => Span::styled(c.text.clone(), Styles::from_class(s)),
                None => Span::raw(c.text.clone()),
            });
            Row::new(cells).style(row_style).height(1)
        })
        .collect();

    // Widths with QUERY filling remaining space
    let mut widths: Vec<ratatui::layout::Constraint> = vm
        .widths
        .iter()
        .map(|&w| ratatui::layout::Constraint::Length(w))
        .collect();
    widths.push(ratatui::layout::Constraint::Fill(1));

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(vm.title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .row_highlight_style(Styles::selected());

    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pga.ratatui_state);
}
