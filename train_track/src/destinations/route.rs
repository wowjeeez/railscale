use std::future::Future;
use std::pin::Pin;
use memchr::memmem;
use crate::io::destination::StreamDestination;
use crate::io::router::DestinationRouter;
use crate::RailscaleError;

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

type RouteFactory<D> = Box<dyn Fn(&[u8]) -> Pin<Box<dyn Future<Output = Result<D, RailscaleError>> + Send>> + Send + Sync>;

pub struct MatchingRouter<D: StreamDestination> {
    matchers: Vec<(MatchStrategy, RouteFactory<D>)>,
}

impl<D: StreamDestination> MatchingRouter<D> {
    pub fn new() -> Self {
        Self { matchers: Vec::new() }
    }

    pub fn add_route(mut self, strategy: MatchStrategy, factory: RouteFactory<D>) -> Self {
        self.matchers.push((strategy, factory));
        self
    }
}

#[async_trait::async_trait]
impl<D: StreamDestination + 'static> DestinationRouter for MatchingRouter<D> {
    type Destination = D;

    async fn route(&self, routing_key: &[u8]) -> Result<Self::Destination, RailscaleError> {
        for (strategy, factory) in &self.matchers {
            if strategy.is_match(routing_key) {
                return factory(routing_key).await;
            }
        }
        Err(RailscaleError::RoutingFailed("no matching route".into()))
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
