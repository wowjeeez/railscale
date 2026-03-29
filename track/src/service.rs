use std::pin::pin;
use tokio_stream::StreamExt;
use crate::destination::StreamDestination;
use crate::frame::{Frame, ParsedData};
use crate::parser::FrameParser;
use crate::pipeline::FramePipeline;
use crate::source::StreamSource;
use crate::RailscaleError;

pub trait Service: Send + Sync {
    fn run(&self) -> impl std::future::Future<Output = Result<(), RailscaleError>> + Send;
}

pub struct Pipeline<Src, Par, Pip, Dst>
where
    Src: StreamSource,
    Par: FrameParser<Src::Stream>,
    Pip: FramePipeline<Frame = Par::Frame>,
    Dst: StreamDestination<Frame = Par::Frame>,
{
    pub source: Src,
    pub parser_factory: fn() -> Par,
    pub pipeline: Pip,
    pub destination_factory: fn() -> Dst,
}

impl<Src, Par, Pip, Dst> Pipeline<Src, Par, Pip, Dst>
where
    Src: StreamSource + Sync,
    Par: FrameParser<Src::Stream> + 'static,
    Pip: FramePipeline<Frame = Par::Frame> + 'static,
    Dst: StreamDestination<Frame = Par::Frame> + 'static,
{
    async fn handle_connection(
        stream: Src::Stream,
        parser_factory: fn() -> Par,
        pipeline: &Pip,
        destination_factory: fn() -> Dst,
    ) -> Result<(), RailscaleError> {
        let mut parser = parser_factory();
        let mut dest = destination_factory();
        let frames = parser.parse(stream);
        let mut frames = pin!(frames);
        let mut routed = false;

        while let Some(result) = frames.next().await {
            match result {
                Ok(ParsedData::Passthrough(bytes)) => {
                    dest.write_raw(bytes).await.map_err(Into::into)?;
                }
                Ok(ParsedData::Parsed(frame)) => {
                    if frame.is_routing_frame() && !routed {
                        dest.provide(&frame).await.map_err(Into::into)?;
                        routed = true;
                    }
                    let frame = pipeline.process(frame);
                    dest.write(frame).await.map_err(Into::into)?;
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(())
    }
}

impl<Src, Par, Pip, Dst> Service for Pipeline<Src, Par, Pip, Dst>
where
    Src: StreamSource + Sync + 'static,
    Par: FrameParser<Src::Stream> + 'static,
    Par::Error: Send,
    Pip: FramePipeline<Frame = Par::Frame> + 'static,
    Dst: StreamDestination<Frame = Par::Frame> + 'static,
{
    async fn run(&self) -> Result<(), RailscaleError> {
        loop {
            let stream = self.source.accept().await.map_err(Into::into)?;
            let parser_factory = self.parser_factory;
            let destination_factory = self.destination_factory;
            let pipeline = &self.pipeline;

            Self::handle_connection(stream, parser_factory, pipeline, destination_factory).await?;
        }
    }
}
