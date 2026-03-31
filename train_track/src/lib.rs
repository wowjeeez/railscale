mod error;
#[cfg(feature = "metrics-full")]
pub mod recorder;
mod io;
mod atom;
mod core;
mod destinations;

pub use io::destination::StreamDestination;
pub use error::{RailscaleError, ErrorKind, Phase};
pub use atom::frame::{Frame, ParsedData};
pub use atom::phase::{FramePhase, PhasedFrame, PhasedBuffer};
pub use atom::derive::{MatchAtom, DerivedEffect, DerivationFormula, DeriverSession};
pub use atom::parser::FrameParser;
pub use core::pipeline::FramePipeline;
pub use core::composed::{Composed, StreamTransform};
pub use core::service::{Pipeline, Service};
pub use io::router::DestinationRouter;
pub use io::source::StreamSource;
pub use destinations::route::{MatchStrategy, MatchingRouter};
pub use destinations::file::FileDestination;
pub use core::error_mapper::{ErrorToFrames, ErrorToBytes};
pub use core::service::BufferLimits;
pub use io::batcher::BatchWriter;
pub use tokio_util::sync::CancellationToken;
