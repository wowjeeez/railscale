mod error;
#[cfg(feature = "metrics-full")]
pub mod sampler;
mod io;
mod atom;
mod core;
mod destinations;

pub use io::destination::StreamDestination;
pub use error::RailscaleError;
pub use atom::frame::{Frame, ParsedData};
pub use atom::parser::FrameParser;
pub use core::pipeline::FramePipeline;
pub use core::service::{Pipeline, Service};
pub use io::source::StreamSource;
pub use destinations::route::{DomainMatcher, RouterDestination, MemchrDomainMatcher, MatchStrategy};
pub use destinations::file::{FileDestination, FrameSerializer, DefaultFrameSerializer};
