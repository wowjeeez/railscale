use bytes::Bytes;
use std::collections::HashSet;
use std::hash::Hash;
#[cfg(feature = "derive-debug")]
use tracing::debug;

pub trait MatchAtom: Hash + Eq + Clone + Send + Sync + 'static {
    type Phase: Eq + Copy + Send + Sync + 'static;
    fn phase(&self) -> Self::Phase;
    fn try_match(&self, data: &[u8]) -> Option<Bytes>;
}

pub trait DerivedEffect<E> {
    fn apply_effect(&mut self, effect: E);
}

pub trait DerivationFormula: Send + Sync + 'static {
    type Matcher: MatchAtom;
    type Effect: Send + 'static;
    const MATCHERS: &'static [Self::Matcher];
    fn resolve(matched: &[Option<Bytes>]) -> Self::Effect;
}

pub struct DeriverSession<M: MatchAtom> {
    matchers: Vec<M>,
    results: Vec<Option<Bytes>>,
    conflicts: Vec<bool>,
}

impl<M: MatchAtom> DeriverSession<M> {
    pub fn new(matchers: Vec<M>) -> Self {
        let deduped: Vec<M> = {
            let mut seen = HashSet::new();
            matchers.into_iter().filter(|m| seen.insert(m.clone())).collect()
        };
        let len = deduped.len();
        Self {
            matchers: deduped,
            results: vec![None; len],
            conflicts: vec![false; len],
        }
    }

    pub fn feed(&mut self, phase: &M::Phase, data: &[u8]) {
        for (i, matcher) in self.matchers.iter().enumerate() {
            if matcher.phase() != *phase {
                continue;
            }
            match &self.results[i] {
                None => {
                    let result = matcher.try_match(data);
                    #[cfg(feature = "derive-debug")]
                    if let Some(ref matched) = result {
                        debug!(
                            matcher_idx = i,
                            matched_bytes = matched.len(),
                            "derive: matcher hit"
                        );
                    }
                    self.results[i] = result;
                }
                Some(existing) => {
                    if matcher.try_match(data).is_some_and(|new_val| existing != &new_val) {
                        #[cfg(feature = "derive-debug")]
                        debug!(
                            matcher_idx = i,
                            "derive: conflicting duplicate detected"
                        );
                        self.conflicts[i] = true;
                    }
                }
            }
        }
    }

    pub fn results_for<F: DerivationFormula<Matcher = M>>(&self) -> Vec<Option<Bytes>> {
        F::MATCHERS.iter().map(|m| {
            self.matchers.iter()
                .position(|x| x == m)
                .and_then(|idx| self.results[idx].clone())
        }).collect()
    }

    pub fn has_conflicts_for<F: DerivationFormula<Matcher = M>>(&self) -> bool {
        F::MATCHERS.iter().any(|m| {
            self.matchers.iter()
                .position(|x| x == m)
                .is_some_and(|idx| self.conflicts[idx])
        })
    }

    pub fn matcher_count(&self) -> usize {
        self.matchers.len()
    }
}
