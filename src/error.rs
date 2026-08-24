use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("tag not found: {0}")]
    TagNotFound(String),

    #[error("invalid status transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("already checked in today: {0}")]
    AlreadyCheckedInToday(String),

    #[error("task is not archived: {0}")]
    NotArchived(String),

    #[error("system task is protected and cannot be modified this way: {0}")]
    SystemTaskProtected(String),
}
