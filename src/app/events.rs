use super::{AttachmentFile, MessageChannel, SearchResult};
use crate::{analyzer, data::SupportTicketView, data::activity::ActivityEventPreview, downloader};
use std::result::Result;

pub(crate) enum AnalysisEvent {
    Progress(analyzer::AnalysisProgress),
    Finished(Box<Result<analyzer::AnalysisData, String>>),
}

pub(crate) enum SupportActivityEvent {
    TicketsFinished(Result<Vec<SupportTicketView>, String>),
    ActivityFinished(Result<Vec<ActivityEventPreview>, String>),
}

pub(crate) enum GalleryEvent {
    Finished(Result<Vec<AttachmentFile>, String>),
}

pub(crate) enum ChannelEvent {
    Finished(Result<Vec<MessageChannel>, String>),
}

pub(crate) enum DownloadEvent {
    Progress(downloader::DownloadProgress),
    Finished(Result<(), String>),
}

pub(crate) enum ChannelPreviewEvent {
    Finished {
        key: String,
        result: std::result::Result<Vec<String>, String>,
    },
}

pub(crate) enum SearchEvent {
    Batch {
        generation: u64,
        matches: Vec<SearchResult>,
    },
    Progress {
        generation: u64,
        scanned_files: usize,
        total_files: usize,
        total_matches: usize,
    },
    Finished {
        generation: u64,
        total_matches: usize,
        truncated: bool,
    },
}
