//! Core types for semantic event matching.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A matched event pair between platforms with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedPair {
    /// Kalshi event identifier (ticker)
    pub kalshi_event_id: Arc<str>,
    /// Kalshi event description
    pub kalshi_description: Arc<str>,
    /// Polymarket event identifier (slug or token ID)
    pub poly_event_id: Arc<str>,
    /// Polymarket event description
    pub poly_description: Arc<str>,
    /// Cosine similarity score [0.0, 1.0]
    pub similarity: f32,
    /// Unix timestamp when match was created
    pub matched_at: u64,
    /// Match quality classification
    pub quality: MatchQuality,
}

/// Quality classification for matches
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchQuality {
    /// Very high confidence (similarity ≥ 0.95)
    Excellent,
    /// High confidence (similarity ≥ 0.90)
    Good,
    /// Acceptable confidence (similarity ≥ 0.88)
    Acceptable,
    /// Manual review recommended (similarity < 0.88)
    ReviewNeeded,
}

impl MatchQuality {
    /// Classify match quality based on similarity score
    pub fn from_similarity(score: f32) -> Self {
        if score >= 0.95 {
            MatchQuality::Excellent
        } else if score >= 0.90 {
            MatchQuality::Good
        } else if score >= 0.88 {
            MatchQuality::Acceptable
        } else {
            MatchQuality::ReviewNeeded
        }
    }

    /// Check if quality meets minimum threshold for auto-matching
    #[inline]
    pub fn is_auto_acceptable(self) -> bool {
        matches!(self, MatchQuality::Excellent | MatchQuality::Good | MatchQuality::Acceptable)
    }
}

/// Event descriptor for matching
#[derive(Debug, Clone)]
pub struct EventDescriptor {
    /// Unique event identifier
    pub event_id: Arc<str>,
    /// Raw event description text
    pub description: Arc<str>,
    /// Normalized description (lowercase, cleaned)
    pub normalized: String,
    /// Pre-computed embedding vector (if available)
    pub embedding: Option<Vec<f32>>,
}

impl EventDescriptor {
    /// Create a new event descriptor with normalized text
    pub fn new(event_id: Arc<str>, description: Arc<str>, normalized: String) -> Self {
        Self {
            event_id,
            description,
            normalized,
            embedding: None,
        }
    }

    /// Create with pre-computed embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Embedding vector (384-dimensional for MiniLM)
pub type EmbeddingVector = Vec<f32>;

/// Precomputed embeddings cache for fast startup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingCache {
    /// Model identifier (e.g., "all-MiniLM-L6-v2")
    pub model: String,
    /// Dimension of embedding vectors
    pub dimension: usize,
    /// Map from event_id to embedding vector
    pub embeddings: rustc_hash::FxHashMap<String, Vec<f32>>,
    /// Unix timestamp when cache was created
    pub created_at: u64,
}

impl EmbeddingCache {
    /// Create a new empty cache
    pub fn new(model: String, dimension: usize) -> Self {
        Self {
            model,
            dimension,
            embeddings: rustc_hash::FxHashMap::default(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Insert an embedding
    pub fn insert(&mut self, event_id: String, embedding: Vec<f32>) {
        self.embeddings.insert(event_id, embedding);
    }

    /// Get an embedding by event ID
    pub fn get(&self, event_id: &str) -> Option<&Vec<f32>> {
        self.embeddings.get(event_id)
    }
}
