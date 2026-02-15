//! PostgreSQL log errors (PGE) tab widget.
//! Thin TUI wrapper over [`crate::view::pge::build_errors_view`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::tui::state::AppState;
use crate::tui::style::Styles;
use crate::view::pge::build_errors_view;

pub fn render_pg_errors(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let vm = match build_errors_view(&state.pge.accumulated, &state.pge, interner) {
        Some(vm) => vm,
        None => {
            let label = if state.pge.filter.is_some() {
                "No matching events (filter active)"
            } else {
                "No events in the current hour"
            };
            let msg = Paragraph::new(label)
                .block(Block::default().title("PGE: Events").borders(Borders::ALL));
            frame.render_widget(Clear, area);
            frame.render_widget(msg, area);
            return;
        }
    };

    // Resolve selection
    let row_hashes: Vec<u64> = vm.rows.iter().map(|r| r.id).collect();
    state.pge.resolve_selection(&row_hashes);

    // Header
    let header_cells: Vec<Span> = vm
        .headers
        .iter()
        .map(|h| Span::styled(h.clone(), Styles::table_header()))
        .collect();
    let header = Row::new(header_cells).style(Styles::table_header());

    // Widths
    let mut widths: Vec<ratatui::layout::Constraint> = vm
        .widths
        .iter()
        .map(|&w| ratatui::layout::Constraint::Length(w))
        .collect();
    // PATTERN and SAMPLE get Fill(1) each
    widths.push(ratatui::layout::Constraint::Fill(1));
    widths.push(ratatui::layout::Constraint::Fill(1));

    // Rows
    let rows: Vec<Row> = vm
        .rows
        .iter()
        .map(|vr| {
            let style = Styles::from_class(vr.style);
            let cells = vr.cells.iter().map(|c| match c.style {
                Some(s) => Span::styled(c.text.clone(), Styles::from_class(s)),
                None => Span::raw(c.text.clone()),
            });
            Row::new(cells).style(style)
        })
        .collect();

    let table = Table::new(rows, widths)
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
    frame.render_stateful_widget(table, area, &mut state.pge.ratatui_state);
}
