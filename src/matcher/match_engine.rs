//! Core matching engine for semantic event matching.
//!
//! Orchestrates the matching pipeline: normalization → embedding → similarity → registration.

use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{info, warn, debug};

use super::types::{EventDescriptor, MatchedPair, MatchQuality};
use super::registry::MatchRegistry;
use super::normalize::normalize_text;
use super::embed::{EmbeddingGenerator, find_best_match, SIMILARITY_THRESHOLD};

/// Match engine configuration
pub struct MatchConfig {
    /// Minimum similarity threshold for auto-matching
    pub similarity_threshold: f32,
    /// Maximum candidates to consider per query
    pub max_candidates: usize,
    /// Enable strict mode (only excellent/good matches)
    pub strict_mode: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: SIMILARITY_THRESHOLD,
            max_candidates: 1000,
            strict_mode: false,
        }
    }
}

/// Semantic event matching engine
pub struct MatchEngine {
    config: MatchConfig,
    embedding_gen: EmbeddingGenerator,
}

impl MatchEngine {
    /// Create a new match engine with default configuration
    pub fn new() -> Self {
        Self {
            config: MatchConfig::default(),
            embedding_gen: EmbeddingGenerator::new(None),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: MatchConfig) -> Self {
        Self {
            config,
            embedding_gen: EmbeddingGenerator::new(None),
        }
    }

    /// Create with pre-loaded embeddings
    pub fn with_embeddings(embeddings: super::types::EmbeddingCache) -> Self {
        Self {
            config: MatchConfig::default(),
            embedding_gen: EmbeddingGenerator::new(Some(embeddings)),
        }
    }

    /// Create with custom config and embeddings
    pub fn with_config_and_embeddings(
        config: MatchConfig,
        embeddings: super::types::EmbeddingCache,
    ) -> Self {
        Self {
            config,
            embedding_gen: EmbeddingGenerator::new(Some(embeddings)),
        }
    }

    /// Prepare an event descriptor with normalization and embedding
    pub fn prepare_event(
        &self,
        event_id: Arc<str>,
        description: Arc<str>,
    ) -> EventDescriptor {
        let normalized = normalize_text(&description);
        let embedding = self.embedding_gen.embed(&event_id, &normalized);
        
        EventDescriptor::new(event_id, description, normalized)
            .with_embedding(embedding)
    }

    /// Match a single Kalshi event against multiple Polymarket candidates
    ///
    /// Returns the best match if similarity exceeds threshold, otherwise None.
    pub fn match_event(
        &self,
        kalshi: &EventDescriptor,
        poly_candidates: &[EventDescriptor],
    ) -> Option<(usize, f32)> {
        let kalshi_emb = kalshi.embedding.as_ref()?;

        // Build candidate list with embeddings
        let candidates: Vec<(usize, &[f32])> = poly_candidates
            .iter()
            .enumerate()
            .filter_map(|(idx, desc)| {
                desc.embedding.as_ref().map(|emb| (idx, emb.as_slice()))
            })
            .collect();

        if candidates.is_empty() {
            warn!("No poly candidates with embeddings for '{}'", kalshi.description);
            return None;
        }

        // Find best match
        find_best_match(kalshi_emb, &candidates, self.config.similarity_threshold)
    }

    /// Match a Kalshi event and register the result if above threshold
    pub async fn match_and_register(
        &self,
        kalshi: EventDescriptor,
        poly_candidates: Vec<EventDescriptor>,
        registry: &MatchRegistry,
    ) -> Result<Option<MatchedPair>> {
        // Check if already matched
        if registry.is_kalshi_matched(&kalshi.event_id).await {
            debug!("Kalshi event '{}' already matched, skipping", kalshi.event_id);
            return Ok(None);
        }

        // Attempt match
        let match_result = self.match_event(&kalshi, &poly_candidates);

        if let Some((idx, similarity)) = match_result {
            let poly = &poly_candidates[idx];

            // Check if poly event is already matched
            if registry.is_poly_matched(&poly.event_id).await {
                debug!("Poly event '{}' already matched to another event", poly.event_id);
                return Ok(None);
            }

            let quality = MatchQuality::from_similarity(similarity);

            // Check strict mode
            if self.config.strict_mode && !quality.is_auto_acceptable() {
                warn!(
                    "Match quality below strict threshold: {} <-> {} (score: {:.3}, quality: {:?})",
                    kalshi.description, poly.description, similarity, quality
                );
                return Ok(None);
            }

            // Create matched pair
            let matched_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let pair = MatchedPair {
                kalshi_event_id: kalshi.event_id.clone(),
                kalshi_description: kalshi.description.clone(),
                poly_event_id: poly.event_id.clone(),
                poly_description: poly.description.clone(),
                similarity,
                matched_at,
                quality,
            };

            info!(
                "✨ MATCHED: '{}' <-> '{}' (score: {:.3}, quality: {:?})",
                kalshi.description, poly.description, similarity, quality
            );

            // Register the match
            registry.add_match(pair.clone()).await
                .context("Failed to add match to registry")?;

            Ok(Some(pair))
        } else {
            debug!(
                "No match found for '{}' (best similarity below threshold)",
                kalshi.description
            );
            Ok(None)
        }
    }

    /// Batch matching: match multiple Kalshi events against multiple Poly events
    ///
    /// Returns the number of new matches found.
    pub async fn batch_match(
        &self,
        kalshi_events: Vec<EventDescriptor>,
        poly_events: Vec<EventDescriptor>,
        registry: &MatchRegistry,
    ) -> Result<usize> {
        info!(
            "🔄 Running batch matcher: {} Kalshi events, {} Poly candidates",
            kalshi_events.len(),
            poly_events.len()
        );

        let mut new_matches = 0;

        for kalshi in kalshi_events {
            if let Some(_pair) = self.match_and_register(
                kalshi,
                poly_events.clone(),
                registry,
            ).await? {
                new_matches += 1;
            }
        }

        info!("✅ Batch matching complete: {} new matches", new_matches);

        Ok(new_matches)
    }
}

impl Default for MatchEngine {
    fn default() -> Self {
        Self::new()
    }
}

// TODO: Add support for 3-way outcomes (1X2 markets)
// TODO: Add support for spread/total markets with line matching
// TODO: Add fuzzy team name matching for better sports coverage
// TODO: Consider adding active learning for threshold tuning
