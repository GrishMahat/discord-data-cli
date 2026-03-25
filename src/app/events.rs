use super::AttachmentFile;
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

pub(crate) enum DownloadEvent {
    Progress(downloader::DownloadProgress),
    Finished(Result<(), String>),
}
