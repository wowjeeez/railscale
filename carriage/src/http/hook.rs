use train_track::{ConnectionHook, DeriverSession, RailscaleError, ErrorKind};
use crate::http_v1::{HttpFrame, HttpPhase};
use crate::http_v1::derive::{Matcher, HttpDerivationInput, ConnectionMode};

pub struct HttpDeriverHook {
    session: DeriverSession<Matcher>,
}

impl HttpDeriverHook {
    pub fn new() -> Self {
        Self {
            session: DeriverSession::new(HttpDerivationInput::all_matchers()),
        }
    }

    pub fn session(&self) -> &DeriverSession<Matcher> {
        &self.session
    }

    pub fn into_session(self) -> DeriverSession<Matcher> {
        self.session
    }

    pub fn resolve(&self) -> HttpDerivationInput {
        HttpDerivationInput::resolve_all(&self.session)
    }
}

impl ConnectionHook<HttpFrame> for HttpDeriverHook {
    fn on_frame(&mut self, frame: &HttpFrame) {
        use train_track::PhasedFrame;
        use train_track::Frame;
        let phase = frame.phase();
        if phase == HttpPhase::EndOfHeaders || phase == HttpPhase::Body {
            return;
        }
        self.session.feed(&phase, frame.as_bytes());
    }

    fn validate(&self) -> Result<(), RailscaleError> {
        let derived = self.resolve();
        if derived.cl_te_conflict {
            return Err(RailscaleError::from(ErrorKind::Parse(
                "Content-Length and Transfer-Encoding both present (request smuggling)".into(),
            )));
        }
        if derived.has_conflicts {
            return Err(RailscaleError::from(ErrorKind::Parse(
                "conflicting header values detected".into(),
            )));
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.session = DeriverSession::new(HttpDerivationInput::all_matchers());
    }

    fn should_close_connection(&self) -> bool {
        let derived = self.resolve();
        derived.connection == ConnectionMode::Close
    }
}
