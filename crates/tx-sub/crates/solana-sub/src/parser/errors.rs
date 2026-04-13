use thiserror::Error;

/// Errors that can occur during transaction data parsing
#[derive(Error, Debug)]
pub enum DataParserError {
    #[error("Failed to parse instruction data")]
    InstructionParseError,

    #[error("Invalid data format")]
    InvalidFormat,

    #[error("Insufficient data provided")]
    InsufficientData,

    #[error("Unknown parsing error: {0}")]
    Other(String),
}
