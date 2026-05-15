#![allow(dead_code)]

use converge_pack::{ContentHash, FactPayload, Timestamp};
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

    pub fn with_seed(mut self, id: &str, payload: impl FactPayload + PartialEq) -> Self {
        self.facts
            .entry(ContextKey::Seeds)
            .or_default()
            .push(seed_fact(id, payload));
        self
    }

    pub fn with_strategy(mut self, id: &str, payload: impl FactPayload + PartialEq) -> Self {
        self.facts
            .entry(ContextKey::Strategies)
            .or_default()
            .push(strategy_fact(id, payload));
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

pub fn seed_fact(id: &str, payload: impl FactPayload + PartialEq) -> ContextFact {
    fact(ContextKey::Seeds, id, payload)
}

pub fn strategy_fact(id: &str, payload: impl FactPayload + PartialEq) -> ContextFact {
    fact(ContextKey::Strategies, id, payload)
}

fn fact(key: ContextKey, id: &str, payload: impl FactPayload + PartialEq) -> ContextFact {
    ContextFact::new_projection(
        key,
        id,
        payload,
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
