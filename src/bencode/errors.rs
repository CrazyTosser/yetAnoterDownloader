use std::num::ParseIntError;
use std::str::Utf8Error;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum BencodeParseError {
    #[error("empty integer found")]
    EmptyIntegerFound,
    #[error("missing terminator, expected '{expected}'")]
    MissingTerminator {
        expected: u8,
    },
    #[error("negative zero found")]
    NegativeZero,
    #[error("leading zero not approved")]
    LeadingZero,
    #[error(transparent)]
    InvalidInteger(#[from] Utf8Error),
    #[error(transparent)]
    Parse(#[from] ParseIntError),

    #[error("missing colon symbol")]
    ColonMissing,
    #[error("negative or zero size given: {given}")]
    NegativeOrZeroSize {
        given: i64,
    },
    #[error("unterminated string: expected len {expected}, got {got}")]
    InsufficientData {
        expected: usize,
        got: usize,
    },
    #[error("input string is empty")]
    NoDataFound,
    #[error("wrong bencode structure")]
    WrongStructure,
}
