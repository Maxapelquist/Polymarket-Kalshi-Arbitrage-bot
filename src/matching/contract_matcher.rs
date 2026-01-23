//! Contract-level matching (within matched events)
//!
//! Använder samma två-stegs approach som event matching

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMatch {
    pub kalshi_contract_id: String,
    pub polymarket_market_id: String,
    pub event_match_id: String, // Referens till event match
    pub confidence: f32,
    pub embedding_score: f32,
    pub llm_verified: bool,
    pub llm_confidence: f32,
    pub reason: String,
}

pub struct ContractMatcher {
    // TODO: Implementera när event matching är klar
}

impl ContractMatcher {
    pub fn new() -> Self {
        Self {}
    }
    
    pub fn match_contracts(
        &mut self,
        _event_match_id: &str,
        _kalshi_contracts: &[()],
        _polymarket_markets: &[()],
    ) -> Result<Vec<ContractMatch>> {
        // TODO: Implementera contract matching
        Ok(Vec::new())
    }
}
