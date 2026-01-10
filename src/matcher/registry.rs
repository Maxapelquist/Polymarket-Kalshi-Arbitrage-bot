//! Persistent registry for matched event pairs.
//!
//! Provides O(1) lookups and append-only storage of matched events.

use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::types::MatchedPair;

/// Persistent registry of matched event pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryData {
    /// All matched pairs (append-only)
    pairs: Vec<MatchedPair>,
    /// Version identifier for schema evolution
    version: u32,
}

/// Thread-safe match registry with fast lookups
pub struct MatchRegistry {
    /// Registry file path
    path: String,
    /// In-memory state
    state: Arc<RwLock<RegistryState>>,
}

/// Internal registry state
struct RegistryState {
    /// All matched pairs
    pairs: Vec<MatchedPair>,
    /// Fast lookup: Kalshi event ID -> pair index
    kalshi_index: FxHashMap<Arc<str>, usize>,
    /// Fast lookup: Polymarket event ID -> pair index
    poly_index: FxHashMap<Arc<str>, usize>,
}

impl MatchRegistry {
    /// Load registry from disk or create new if not exists
    pub async fn load_or_new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        let state = if Path::new(&path).exists() {
            info!("📖 Loading match registry from {}", path);
            let data = tokio::fs::read_to_string(&path).await
                .context("Failed to read registry file")?;
            let reg_data: RegistryData = serde_json::from_str(&data)
                .context("Failed to parse registry file")?;
            
            info!("✅ Loaded {} matched pairs", reg_data.pairs.len());
            Self::build_state(reg_data.pairs)
        } else {
            info!("🆕 Creating new match registry at {}", path);
            Self::build_state(Vec::new())
        };

        Ok(Self {
            path,
            state: Arc::new(RwLock::new(state)),
        })
    }

    /// Build internal state with indices
    fn build_state(pairs: Vec<MatchedPair>) -> RegistryState {
        let mut kalshi_index = FxHashMap::default();
        let mut poly_index = FxHashMap::default();

        for (idx, pair) in pairs.iter().enumerate() {
            kalshi_index.insert(Arc::clone(&pair.kalshi_event_id), idx);
            poly_index.insert(Arc::clone(&pair.poly_event_id), idx);
        }

        RegistryState {
            pairs,
            kalshi_index,
            poly_index,
        }
    }

    /// Check if a Kalshi event is already matched
    #[inline]
    pub async fn is_kalshi_matched(&self, kalshi_id: &str) -> bool {
        let state = self.state.read().await;
        state.kalshi_index.contains_key(kalshi_id)
    }

    /// Check if a Polymarket event is already matched
    #[inline]
    pub async fn is_poly_matched(&self, poly_id: &str) -> bool {
        let state = self.state.read().await;
        state.poly_index.contains_key(poly_id)
    }

    /// Get matched pair by Kalshi event ID (O(1) lookup)
    pub async fn get_by_kalshi(&self, kalshi_id: &str) -> Option<MatchedPair> {
        let state = self.state.read().await;
        let idx = *state.kalshi_index.get(kalshi_id)?;
        state.pairs.get(idx).cloned()
    }

    /// Get matched pair by Polymarket event ID (O(1) lookup)
    pub async fn get_by_poly(&self, poly_id: &str) -> Option<MatchedPair> {
        let state = self.state.read().await;
        let idx = *state.poly_index.get(poly_id)?;
        state.pairs.get(idx).cloned()
    }

    /// Add a new matched pair to the registry
    pub async fn add_match(&self, pair: MatchedPair) -> Result<()> {
        let mut state = self.state.write().await;

        // Check for duplicates
        if state.kalshi_index.contains_key(&pair.kalshi_event_id) {
            warn!("Kalshi event {} already matched, skipping", pair.kalshi_event_id);
            return Ok(());
        }
        if state.poly_index.contains_key(&pair.poly_event_id) {
            warn!("Poly event {} already matched, skipping", pair.poly_event_id);
            return Ok(());
        }

        // Add to registry
        let idx = state.pairs.len();
        state.kalshi_index.insert(Arc::clone(&pair.kalshi_event_id), idx);
        state.poly_index.insert(Arc::clone(&pair.poly_event_id), idx);
        state.pairs.push(pair);

        Ok(())
    }

    /// Persist registry to disk
    pub async fn save(&self) -> Result<()> {
        let state = self.state.read().await;
        let data = RegistryData {
            pairs: state.pairs.clone(),
            version: 1,
        };

        let json = serde_json::to_string_pretty(&data)
            .context("Failed to serialize registry")?;
        
        tokio::fs::write(&self.path, json).await
            .context("Failed to write registry file")?;

        Ok(())
    }

    /// Get total number of matched pairs
    pub async fn len(&self) -> usize {
        self.state.read().await.pairs.len()
    }

    /// Check if registry is empty
    pub async fn is_empty(&self) -> bool {
        self.state.read().await.pairs.is_empty()
    }

    /// Get all matched pairs (for iteration/export)
    pub async fn get_all(&self) -> Vec<MatchedPair> {
        self.state.read().await.pairs.clone()
    }

    /// Get statistics about the registry
    pub async fn stats(&self) -> RegistryStats {
        let state = self.state.read().await;
        let mut excellent = 0;
        let mut good = 0;
        let mut acceptable = 0;
        let mut review_needed = 0;

        for pair in &state.pairs {
            match pair.quality {
                super::types::MatchQuality::Excellent => excellent += 1,
                super::types::MatchQuality::Good => good += 1,
                super::types::MatchQuality::Acceptable => acceptable += 1,
                super::types::MatchQuality::ReviewNeeded => review_needed += 1,
            }
        }

        RegistryStats {
            total: state.pairs.len(),
            excellent,
            good,
            acceptable,
            review_needed,
        }
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total: usize,
    pub excellent: usize,
    pub good: usize,
    pub acceptable: usize,
    pub review_needed: usize,
}

impl std::fmt::Display for RegistryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Total: {}, Excellent: {}, Good: {}, Acceptable: {}, Review: {}",
            self.total, self.excellent, self.good, self.acceptable, self.review_needed
        )
    }
}
