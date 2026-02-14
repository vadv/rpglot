//! PostgreSQL statements table widget for PGS tab.
//! Thin TUI wrapper over [`crate::view::statements::build_statements_view`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::tui::state::AppState;
use crate::tui::style::Styles;
use crate::view::statements::build_statements_view;

pub fn render_pg_statements(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Statements (PGS) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            frame.render_widget(Paragraph::new("No data available").block(block), area);
            return;
        }
    };

    let vm = match build_statements_view(snapshot, &state.pgs, interner, state.is_live) {
        Some(vm) => vm,
        None => {
            let block = Block::default()
                .title(" PostgreSQL Statements (PGS) ")
                .borders(Borders::ALL)
                .style(Styles::default());
            let message = state
                .pga
                .last_error
                .as_deref()
                .unwrap_or("pg_stat_statements is not available");
            frame.render_widget(Paragraph::new(message).block(block), area);
            return;
        }
    };

    // Resolve selection
    let row_queryids: Vec<i64> = vm.rows.iter().map(|r| r.id).collect();
    state.pgs.resolve_selection(&row_queryids);

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
            let is_selected = idx == state.pgs.selected;
            let base_style = if is_selected {
                Styles::selected()
            } else {
                Styles::from_class(vr.style)
            };

            let cells = vr.cells.iter().map(|c| match c.style {
                Some(s) => Span::styled(c.text.clone(), Styles::from_class(s)),
                None => Span::raw(c.text.clone()),
            });
            Row::new(cells).style(base_style).height(1)
        })
        .collect();

    let mut constraints: Vec<ratatui::layout::Constraint> = vm
        .widths
        .iter()
        .map(|&w| ratatui::layout::Constraint::Length(w))
        .collect();
    constraints.push(ratatui::layout::Constraint::Fill(1));

    let table = Table::new(rows, constraints)
        .header(header)
        .block(
            Block::default()
                .title(vm.title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .column_spacing(1)
        .row_highlight_style(Styles::selected());

    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pgs.ratatui_state);
}
