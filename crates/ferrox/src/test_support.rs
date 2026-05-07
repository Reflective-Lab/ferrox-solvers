#![allow(dead_code)]

use converge_pack::{ContentHash, Timestamp};
use converge_pack::{
    Context, ContextFact, ContextKey, FactActor, FactActorKind, FactLocalTrace,
    FactPromotionRecord, FactTraceLink, FactValidationSummary,
};
use std::collections::HashMap;

pub struct MockContext {
    facts: HashMap<ContextKey, Vec<ContextFact>>,
}

impl MockContext {
    pub fn empty() -> Self {
        Self {
            facts: HashMap::new(),
        }
    }

    pub fn with_seed(mut self, id: &str, content: &str) -> Self {
        self.facts
            .entry(ContextKey::Seeds)
            .or_default()
            .push(seed_fact(id, content));
        self
    }

    pub fn with_strategy(mut self, id: &str, content: &str) -> Self {
        self.facts
            .entry(ContextKey::Strategies)
            .or_default()
            .push(strategy_fact(id, content));
        self
    }
}

impl Context for MockContext {
    fn has(&self, key: ContextKey) -> bool {
        self.facts.get(&key).is_some_and(|v| !v.is_empty())
    }

    fn get(&self, key: ContextKey) -> &[ContextFact] {
        self.facts.get(&key).map_or(&[], Vec::as_slice)
    }
}

pub fn seed_fact(id: &str, content: &str) -> ContextFact {
    fact(ContextKey::Seeds, id, content)
}

pub fn strategy_fact(id: &str, content: &str) -> ContextFact {
    fact(ContextKey::Strategies, id, content)
}

fn fact(key: ContextKey, id: &str, content: &str) -> ContextFact {
    ContextFact::new_projection(
        key,
        id,
        content,
        FactPromotionRecord::new_projection(
            "projection-test",
            ContentHash::zero(),
            FactActor::new_projection("test", FactActorKind::System),
            FactValidationSummary::default(),
            Vec::new(),
            FactTraceLink::Local(FactLocalTrace::new_projection("trace", "span", None, true)),
            Timestamp::epoch(),
        ),
        Timestamp::epoch(),
    )
}
