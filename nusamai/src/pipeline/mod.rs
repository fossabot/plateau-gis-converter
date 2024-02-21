//! The main pipeline for processing CityGML data
//!
//! [Source] => [Transformer] => [Sink]

pub mod feedback;
pub mod runner;

use std::sync::mpsc;
use thiserror::Error;

pub use nusamai_plateau::Entity;

pub use feedback::*;
pub use runner::*;

pub type Sender = mpsc::SyncSender<Parcel>;
pub type Receiver = mpsc::Receiver<Parcel>;

/// Message passing through the main processing pipeline
#[derive(Debug)]
pub struct Parcel {
    // Entity (Feature, Data, etc.)
    pub entity: Entity,
}

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    ParseError(#[from] nusamai_citygml::ParseError),

    #[error("canceled")]
    Canceled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PipelineError>;
