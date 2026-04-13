//! Event parsing modules for Polymarket WebSocket messages

mod book;
mod event_parser;
mod price_change;

pub use event_parser::EventParser;
