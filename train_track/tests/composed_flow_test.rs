use bytes::Bytes;
use train_track::{
    Frame, SwitchRail, Turnout, SimpleTurnout, IdentityRail, FramePipelineAdapter,
    FramePipeline, ShuttleLink, Shunt, RouterShunt, Departure, StreamDestination,
    DestinationRouter, ChannelTransload, RailscaleError,
};

struct RawFrame(Bytes);

impl Frame for RawFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct HttpFrame(Bytes);

impl Frame for HttpFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { Some(&self.0) }
}

struct NormalizedFrame(Bytes);

impl Frame for NormalizedFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { Some(&self.0) }
}

struct RawToHttpRail;

impl SwitchRail for RawToHttpRail {
    type Input = RawFrame;
    type Output = HttpFrame;
    fn switch(&self, input: RawFrame) -> HttpFrame {
        HttpFrame(input.0)
    }
}

struct HttpToNormalizedRail;

impl SwitchRail for HttpToNormalizedRail {
    type Input = HttpFrame;
    type Output = NormalizedFrame;
    fn switch(&self, input: HttpFrame) -> NormalizedFrame {
        NormalizedFrame(Bytes::from(input.0.to_ascii_lowercase()))
    }
}

#[test]
fn two_stage_shuttle_processes_through_all_stages() {
    let stage1 = SimpleTurnout::new(RawToHttpRail, |f: HttpFrame| Some(f));
    let stage2 = SimpleTurnout::new(HttpToNormalizedRail, |f: NormalizedFrame| Some(f));
    let flow = ShuttleLink::new(stage1, stage2);

    let input = RawFrame(Bytes::from_static(b"GET / HTTP/1.1"));
    let output = flow.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"get / http/1.1");
}

#[test]
fn three_stage_shuttle_composes() {
    struct NormalizedToRawRail;
    impl SwitchRail for NormalizedToRawRail {
        type Input = NormalizedFrame;
        type Output = RawFrame;
        fn switch(&self, input: NormalizedFrame) -> RawFrame {
            let mut v = b"FINAL:".to_vec();
            v.extend_from_slice(&input.0);
            RawFrame(Bytes::from(v))
        }
    }

    let stage1 = SimpleTurnout::new(RawToHttpRail, |f: HttpFrame| Some(f));
    let stage2 = SimpleTurnout::new(HttpToNormalizedRail, |f: NormalizedFrame| Some(f));
    let stage3 = SimpleTurnout::new(NormalizedToRawRail, |f: RawFrame| Some(f));
    let flow = ShuttleLink::new(ShuttleLink::new(stage1, stage2), stage3);

    let input = RawFrame(Bytes::from_static(b"HELLO"));
    let output = flow.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"FINAL:hello");
}

#[test]
fn frame_pipeline_adapter_in_shuttle() {
    struct UpperPipeline;
    impl FramePipeline for UpperPipeline {
        type Frame = HttpFrame;
        fn process(&self, frame: Self::Frame) -> Self::Frame {
            HttpFrame(Bytes::from(frame.0.to_ascii_uppercase()))
        }
    }

    let stage1 = SimpleTurnout::new(RawToHttpRail, |f: HttpFrame| Some(f));
    let stage2 = FramePipelineAdapter::new(UpperPipeline);
    let flow = ShuttleLink::new(stage1, stage2);

    let input = RawFrame(Bytes::from_static(b"hello"));
    let output = flow.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"HELLO");
}

#[test]
fn identity_rail_turnout_in_shuttle() {
    let stage1 = SimpleTurnout::new(
        IdentityRail::<HttpFrame>::new(),
        |f: HttpFrame| Some(f),
    );
    let stage2 = SimpleTurnout::new(HttpToNormalizedRail, |f: NormalizedFrame| Some(f));
    let flow = ShuttleLink::new(stage1, stage2);

    let input = HttpFrame(Bytes::from_static(b"PASS THROUGH"));
    let output = flow.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"pass through");
}

#[tokio::test]
async fn full_flow_with_shunt() {
    struct FixedRouter;

    #[async_trait::async_trait]
    impl DestinationRouter for FixedRouter {
        type Destination = NullDest;
        async fn route(&self, _key: &[u8]) -> Result<NullDest, RailscaleError> {
            Ok(NullDest::new())
        }
    }

    struct NullDest {
        empty: tokio::io::Empty,
    }

    impl NullDest {
        fn new() -> Self { Self { empty: tokio::io::empty() } }
    }

    #[async_trait::async_trait]
    impl StreamDestination for NullDest {
        type Error = RailscaleError;
        type ResponseReader = tokio::io::Empty;

        async fn write(&mut self, _bytes: Bytes) -> Result<(), Self::Error> { Ok(()) }
        fn response_reader(&mut self) -> &mut tokio::io::Empty { &mut self.empty }
    }

    let flow = ShuttleLink::new(
        SimpleTurnout::new(RawToHttpRail, |f: HttpFrame| Some(f)),
        SimpleTurnout::new(HttpToNormalizedRail, |f: NormalizedFrame| Some(f)),
    );

    let input = RawFrame(Bytes::from_static(b"GET /INDEX"));
    let output = flow.process(input).unwrap();
    assert_eq!(output.routing_key(), Some(&b"get /index"[..]));

    let shunt = RouterShunt::<NormalizedFrame, _>::new(FixedRouter);
    let mut departure = shunt.connect(output.routing_key().unwrap()).await.unwrap();
    departure.depart(output.into_bytes()).await.unwrap();
}

#[tokio::test]
async fn full_flow_with_channel_transload() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut transload = ChannelTransload::new(tx);

    let flow = SimpleTurnout::new(
        RawToHttpRail,
        |f: HttpFrame| Some(f),
    );

    let input = RawFrame(Bytes::from_static(b"test data"));
    let output = flow.process(input).unwrap();
    transload.depart(output.into_bytes()).await.unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(&received[..], b"test data");
}

#[test]
fn shuttle_drops_filtered_frames() {
    let stage1 = SimpleTurnout::new(RawToHttpRail, |f: HttpFrame| {
        if f.as_bytes().starts_with(b"DROP") { None } else { Some(f) }
    });
    let stage2 = SimpleTurnout::new(HttpToNormalizedRail, |f: NormalizedFrame| Some(f));
    let flow = ShuttleLink::new(stage1, stage2);

    let keep = RawFrame(Bytes::from_static(b"KEEP"));
    assert!(flow.process(keep).is_some());

    let drop_it = RawFrame(Bytes::from_static(b"DROP THIS"));
    assert!(flow.process(drop_it).is_none());
}
