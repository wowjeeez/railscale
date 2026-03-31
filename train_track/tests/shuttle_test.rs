use bytes::Bytes;
use train_track::{Frame, SwitchRail, Turnout, SimpleTurnout, ShuttleLink};

struct RawFrame(Bytes);

impl Frame for RawFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct UpperFrame(Bytes);

impl Frame for UpperFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct PrefixedFrame(Bytes);

impl Frame for PrefixedFrame {
    fn as_bytes(&self) -> &[u8] { &self.0 }
    fn into_bytes(self) -> Bytes { self.0 }
    fn routing_key(&self) -> Option<&[u8]> { None }
}

struct RawToUpperRail;

impl SwitchRail for RawToUpperRail {
    type Input = RawFrame;
    type Output = UpperFrame;
    fn switch(&self, input: RawFrame) -> UpperFrame {
        UpperFrame(Bytes::from(input.0.to_ascii_uppercase()))
    }
}

struct UpperToPrefixedRail;

impl SwitchRail for UpperToPrefixedRail {
    type Input = UpperFrame;
    type Output = PrefixedFrame;
    fn switch(&self, input: UpperFrame) -> PrefixedFrame {
        let mut prefixed = b"PREFIX:".to_vec();
        prefixed.extend_from_slice(&input.0);
        PrefixedFrame(Bytes::from(prefixed))
    }
}

#[test]
fn shuttle_link_chains_two_turnouts() {
    let stage1 = SimpleTurnout::new(RawToUpperRail, |f: UpperFrame| Some(f));
    let stage2 = SimpleTurnout::new(UpperToPrefixedRail, |f: PrefixedFrame| Some(f));
    let shuttle = ShuttleLink::new(stage1, stage2);

    let input = RawFrame(Bytes::from_static(b"hello"));
    let output = shuttle.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"PREFIX:HELLO");
}

#[test]
fn shuttle_link_propagates_none_from_first() {
    let stage1 = SimpleTurnout::new(RawToUpperRail, |_: UpperFrame| None);
    let stage2 = SimpleTurnout::new(UpperToPrefixedRail, |f: PrefixedFrame| Some(f));
    let shuttle = ShuttleLink::new(stage1, stage2);

    let input = RawFrame(Bytes::from_static(b"hello"));
    assert!(shuttle.process(input).is_none());
}

#[test]
fn shuttle_link_propagates_none_from_second() {
    let stage1 = SimpleTurnout::new(RawToUpperRail, |f: UpperFrame| Some(f));
    let stage2 = SimpleTurnout::new(UpperToPrefixedRail, |_: PrefixedFrame| None);
    let shuttle = ShuttleLink::new(stage1, stage2);

    let input = RawFrame(Bytes::from_static(b"hello"));
    assert!(shuttle.process(input).is_none());
}

#[test]
fn nested_shuttle_links() {
    struct PrefixedToRawRail;
    impl SwitchRail for PrefixedToRawRail {
        type Input = PrefixedFrame;
        type Output = RawFrame;
        fn switch(&self, input: PrefixedFrame) -> RawFrame {
            RawFrame(input.0)
        }
    }

    let stage1 = SimpleTurnout::new(RawToUpperRail, |f: UpperFrame| Some(f));
    let stage2 = SimpleTurnout::new(UpperToPrefixedRail, |f: PrefixedFrame| Some(f));
    let stage3 = SimpleTurnout::new(PrefixedToRawRail, |f: RawFrame| Some(f));

    let shuttle = ShuttleLink::new(ShuttleLink::new(stage1, stage2), stage3);

    let input = RawFrame(Bytes::from_static(b"hello"));
    let output = shuttle.process(input).unwrap();
    assert_eq!(output.as_bytes(), b"PREFIX:HELLO");
}

#[test]
fn shuttle_link_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    type S1 = SimpleTurnout<RawToUpperRail, fn(UpperFrame) -> Option<UpperFrame>>;
    type S2 = SimpleTurnout<UpperToPrefixedRail, fn(PrefixedFrame) -> Option<PrefixedFrame>>;
    assert_send_sync::<ShuttleLink<S1, S2>>();
}
