//! Core analysis engine for the Discord Data Analyzer.
//!
//! Everything here is UI-free: the TUI binary, headless mode, and future
//! frontends (desktop) all drive the same library — analyzer pipeline,
//! insights engine, snapshot comparison, data loaders, and attachment
//! downloader.

pub mod analyzer;
pub mod compare;
pub mod config;
pub mod data;
pub mod downloader;
pub mod insights;
