use thiserror::Error;

/// Errors that can occur in the event coordinator module
#[derive(Debug, Error)]
pub enum EventCoordinatorError {
    /// Indicates that no more events are available to process
    #[error("No more events are available")]
    NoMoreEvents,
}
