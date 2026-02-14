//! Event handling for TUI.
//!
//! Uses a separate thread to poll for terminal events and timer ticks.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};

/// Application events.
#[derive(Debug)]
pub enum Event {
    /// Timer tick for data refresh.
    Tick,
    /// Keyboard input.
    Key(KeyEvent),
    /// Terminal resize (width).
    Resize(u16),
}

/// Event handler that polls for terminal events in a separate thread.
pub struct EventHandler {
    rx: Receiver<Event>,
    /// Kept alive to prevent channel closure.
    _tx: Sender<Event>,
}

impl EventHandler {
    /// Creates a new event handler with the specified tick rate.
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let event_tx = tx.clone();

        thread::spawn(move || {
            loop {
                // Poll for events with timeout
                if event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let event = match evt {
                            CrosstermEvent::Key(key) => Event::Key(key),
                            CrosstermEvent::Resize(w, _) => Event::Resize(w),
                            _ => continue,
                        };
                        if event_tx.send(event).is_err() {
                            break;
                        }
                    }
                } else {
                    // Timeout - send tick
                    if event_tx.send(Event::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        Self { rx, _tx: tx }
    }

    /// Receives the next event, blocking until one is available.
    pub fn next(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }
}
