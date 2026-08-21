use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Inbox,
    Next,
    Waiting,
    Scheduled,
    Someday,
    Reference,
    Done,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Status::Inbox => "inbox",
            Status::Next => "next",
            Status::Waiting => "waiting",
            Status::Scheduled => "scheduled",
            Status::Someday => "someday",
            Status::Reference => "reference",
            Status::Done => "done",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for Status {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "inbox" => Ok(Status::Inbox),
            "next" => Ok(Status::Next),
            "waiting" => Ok(Status::Waiting),
            "scheduled" => Ok(Status::Scheduled),
            "someday" => Ok(Status::Someday),
            "reference" => Ok(Status::Reference),
            "done" => Ok(Status::Done),
            _ => Err(format!("Invalid status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub status: Status,
    pub rrule: Option<String>,
    pub created_at: i64,
    pub clarified_at: Option<i64>,
    pub due_at: Option<i64>,
    pub scheduled_start_at: Option<i64>,
    pub scheduled_end_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub archive_reason: Option<String>,
    pub updated_at: i64,
    pub delegated_to: Option<String>,
    pub checklist: Vec<ChecklistItem>,
}
