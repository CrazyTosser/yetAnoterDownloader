use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TorrentFileError {
    #[error("file root is not dict")]
    WrongRoot,
    #[error("no info field in file")]
    NoInfo,
    #[error("announce-list has wrong structure")]
    AnnounceListWrong,
    #[error("no required fields")]
    NoRequiredFields,
    #[error("no length and files fields")]
    NoLengthAndFilesFields,
    #[error("file contains the length and files fields.")]
    FileContainsLengthAndFilesFields,
    #[error("in files path field is not string")]
    FilesPathNotString,
    #[error("total_size/piece_length not equal to pieces/20")]
    PieceLengthNotEqual,
    #[error("pieces size is incompletely divisible by 20")]
    PieceLengthIncorrect,
    #[error("piece_length must be gt 0")]
    PieceLengthLwZero,
    #[error("name is not string")]
    NameNotString,
    #[error("{name} wrong type")]
    WrongType { name: String },
}

impl From<TorrentFileError> for std::io::Error {
    fn from(err: TorrentFileError) -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
    }
}