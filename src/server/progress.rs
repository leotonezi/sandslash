use crate::model::AuditReport;
use axum::response::sse::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum ProgressEvent {
    Started {
        job_id: String,
        root: String,
    },
    PageDone {
        url: String,
        score: u8,
        queue_depth: usize,
        pages_done: usize,
    },
    Done {
        report: AuditReport,
    },
    Error {
        message: String,
    },
}

impl ProgressEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::Started { .. } => "Started",
            Self::PageDone { .. } => "PageDone",
            Self::Done { .. } => "Done",
            Self::Error { .. } => "Error",
        }
    }

    pub fn to_sse_event(&self) -> Result<Event, serde_json::Error> {
        let data = serde_json::to_string(self)?;
        Ok(Event::default().event(self.event_name()).data(data))
    }
}
