//! Shared navigation trait for tab states with selectable rows.

/// Navigation trait for tab states that have a selectable row and an optional tracked entity.
///
/// All navigation methods clear the tracked entity (forcing position-based selection)
/// and adjust the `selected` index.  The actual clamping to valid bounds happens
/// later in each tab's `resolve_selection()`.
pub trait NavigableTable {
    fn selected(&self) -> usize;
    fn selected_mut(&mut self) -> &mut usize;
    /// Clear the tracked entity ID so the next `resolve_selection` uses position-based lookup.
    fn clear_tracked(&mut self);

    fn select_up(&mut self) {
        *self.selected_mut() = self.selected().saturating_sub(1);
        self.clear_tracked();
    }

    fn select_down(&mut self) {
        *self.selected_mut() = self.selected().saturating_add(1);
        self.clear_tracked();
    }

    fn page_up(&mut self, n: usize) {
        *self.selected_mut() = self.selected().saturating_sub(n);
        self.clear_tracked();
    }

    fn page_down(&mut self, n: usize) {
        *self.selected_mut() = self.selected().saturating_add(n);
        self.clear_tracked();
    }

    fn home(&mut self) {
        *self.selected_mut() = 0;
        self.clear_tracked();
    }

    fn end(&mut self) {
        *self.selected_mut() = usize::MAX;
        self.clear_tracked();
    }
}
