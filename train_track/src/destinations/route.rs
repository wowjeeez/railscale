use std::marker::PhantomData;
use bytes::Bytes;
use memchr::memmem;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tokio::io::AsyncWrite;
use crate::{Frame, RailscaleError, StreamDestination};

pub trait DomainMatcher<T: StreamDestination>: Send + Sync {
    fn matches(&self, domain: &[u8]) -> bool;
    fn get_destination(&self) -> T;
}

pub enum MatchStrategy {
    Exact(Vec<u8>),
    Suffix(Vec<u8>),
    Prefix(Vec<u8>),
    Contains(memmem::Finder<'static>),
}

impl MatchStrategy {
    pub fn exact(domain: impl Into<Vec<u8>>) -> Self {
        Self::Exact(Self::lowercase(domain.into()))
    }

    pub fn suffix(domain: impl Into<Vec<u8>>) -> Self {
        Self::Suffix(Self::lowercase(domain.into()))
    }

    pub fn prefix(domain: impl Into<Vec<u8>>) -> Self {
        Self::Prefix(Self::lowercase(domain.into()))
    }

    pub fn contains(needle: impl Into<Vec<u8>>) -> Self {
        let owned = Self::lowercase(needle.into());
        Self::Contains(memmem::Finder::new(&owned).into_owned())
    }

    fn lowercase(mut v: Vec<u8>) -> Vec<u8> {
        v.make_ascii_lowercase();
        v
    }

    pub fn is_match(&self, domain: &[u8]) -> bool {
        match self {
            Self::Exact(pat) => domain.len() == pat.len() && memmem::find(domain, pat).is_some(),
            Self::Suffix(pat) => domain.len() >= pat.len() && domain[domain.len() - pat.len()..] == **pat,
            Self::Prefix(pat) => domain.len() >= pat.len() && domain[..pat.len()] == **pat,
            Self::Contains(finder) => finder.find(domain).is_some(),
        }
    }
}

pub struct MemchrDomainMatcher<D: StreamDestination> {
    strategy: MatchStrategy,
    destination_factory: Box<dyn Fn() -> D + Send + Sync>,
}

impl<D: StreamDestination> MemchrDomainMatcher<D> {
    pub fn new(strategy: MatchStrategy, factory: impl Fn() -> D + Send + Sync + 'static) -> Self {
        Self { strategy, destination_factory: Box::new(factory) }
    }
}

impl<D: StreamDestination> DomainMatcher<D> for MemchrDomainMatcher<D> {
    fn matches(&self, domain: &[u8]) -> bool {
        self.strategy.is_match(domain)
    }

    fn get_destination(&self) -> D {
        (self.destination_factory)()
    }
}

pub struct RouterDestination<T: Frame, R: StreamDestination<Frame=T>, M: DomainMatcher<R>> {
    target: Option<R>,
    _t: PhantomData<T>,
    matchers: Vec<M>,
}

impl<T: Frame, R: StreamDestination<Frame=T>, M: DomainMatcher<R>> RouterDestination<T, R, M> {
    pub fn new(matchers: Vec<M>) -> Self {
        Self { target: None, _t: PhantomData, matchers }
    }
}

#[async_trait::async_trait]
impl<T: Frame + Sync, D: StreamDestination<Frame=T>, M: DomainMatcher<D>> StreamDestination for RouterDestination<T, D, M> {
    type Frame = T;
    type Error = RailscaleError;

    async fn provide(&mut self, routing_frame: &Self::Frame) -> Result<(), Self::Error> {
        let route = self.matchers.par_iter().find_map_first(|x| {
            if x.matches(&routing_frame.as_bytes()) {
                Some(x.get_destination())
            } else {
                None
            }
        });
        self.target = route;
        Ok(())
    }

    async fn write(&mut self, frame: Self::Frame) -> Result<(), Self::Error> {
        let Some(ref mut target) = self.target else { Err(Self::Error::RoutingFailed("no route".into()))? };
        target.write(frame).await.map_err(Into::into)
    }

    async fn write_raw(&mut self, bytes: Bytes) -> Result<(), Self::Error> {
        let Some(ref mut target) = self.target else { Err(Self::Error::RoutingFailed("no route".into()))? };
        target.write_raw(bytes).await.map_err(Into::into)
    }

    async fn relay_response<W: AsyncWrite + Send + Unpin>(&mut self, client: &mut W) -> Result<u64, Self::Error> {
        let Some(ref mut target) = self.target else { Err(Self::Error::RoutingFailed("no route".into()))? };
        target.relay_response::<W>(client).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let s = MatchStrategy::exact("example.com");
        assert!(s.is_match(b"example.com"));
        assert!(!s.is_match(b"sub.example.com"));
        assert!(!s.is_match(b"example.com.evil"));
    }

    #[test]
    fn exact_case_insensitive() {
        let s = MatchStrategy::exact("Example.COM");
        assert!(s.is_match(b"example.com"));
        assert!(!s.is_match(b"Example.COM"));
    }

    #[test]
    fn suffix_match() {
        let s = MatchStrategy::suffix(".example.com");
        assert!(s.is_match(b"api.example.com"));
        assert!(s.is_match(b"deep.sub.example.com"));
        assert!(!s.is_match(b"example.com"));
        assert!(!s.is_match(b"notexample.com"));
    }

    #[test]
    fn prefix_match() {
        let s = MatchStrategy::prefix("api.");
        assert!(s.is_match(b"api.example.com"));
        assert!(s.is_match(b"api.other.io"));
        assert!(!s.is_match(b"web.example.com"));
    }

    #[test]
    fn contains_match() {
        let s = MatchStrategy::contains("example");
        assert!(s.is_match(b"sub.example.com"));
        assert!(s.is_match(b"example.org"));
        assert!(!s.is_match(b"exmple.com"));
    }

    #[test]
    fn empty_domain_no_panic() {
        let exact = MatchStrategy::exact("a.com");
        let suffix = MatchStrategy::suffix(".com");
        let prefix = MatchStrategy::prefix("a.");
        let contains = MatchStrategy::contains("a");
        assert!(!exact.is_match(b""));
        assert!(!suffix.is_match(b""));
        assert!(!prefix.is_match(b""));
        assert!(!contains.is_match(b""));
    }
}
