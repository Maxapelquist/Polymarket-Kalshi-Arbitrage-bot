//! Embedding generation and similarity computation.
//!
//! Supports both pre-computed embeddings (from Python script) and
//! runtime embedding generation (if Rust embedding library is available).

use anyhow::{Context, Result};
use std::path::Path;
use tracing::{info, warn};

use super::types::{EmbeddingCache, EmbeddingVector};

/// Cosine similarity threshold for auto-matching
pub const SIMILARITY_THRESHOLD: f32 = 0.88;

/// Default embedding dimension (MiniLM-L6-v2 = 384)
pub const DEFAULT_EMBEDDING_DIM: usize = 384;

/// Compute cosine similarity between two embedding vectors
///
/// Returns a value in [0, 1] where 1 is identical and 0 is orthogonal.
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;

    // Unroll for better performance
    let len = a.len();
    let chunks = len / 4;
    let remainder = len % 4;

    // Process 4 elements at a time
    for i in 0..chunks {
        let base = i * 4;
        for j in 0..4 {
            let idx = base + j;
            let va = a[idx];
            let vb = b[idx];
            dot += va * vb;
            norm_a += va * va;
            norm_b += vb * vb;
        }
    }

    // Process remainder
    for i in (len - remainder)..len {
        let va = a[i];
        let vb = b[i];
        dot += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a.sqrt() * norm_b.sqrt())
}

/// Find the best match for a query embedding from a list of candidates
///
/// Returns (index, similarity_score) of the best match, or None if no match
/// exceeds the threshold.
pub fn find_best_match(
    query: &[f32],
    candidates: &[(usize, &[f32])],
    threshold: f32,
) -> Option<(usize, f32)> {
    let mut best_idx = 0;
    let mut best_score = threshold;
    let mut found = false;

    for (idx, candidate) in candidates {
        let score = cosine_similarity(query, candidate);
        if score > best_score {
            best_score = score;
            best_idx = *idx;
            found = true;
        }
    }

    if found {
        Some((best_idx, best_score))
    } else {
        None
    }
}

/// Load embedding cache from disk
pub async fn load_embedding_cache(path: impl AsRef<Path>) -> Result<EmbeddingCache> {
    let path = path.as_ref();
    info!("📖 Loading embedding cache from {}", path.display());

    let data = tokio::fs::read_to_string(path).await
        .context("Failed to read embedding cache file")?;
    
    let cache: EmbeddingCache = serde_json::from_str(&data)
        .context("Failed to parse embedding cache")?;

    info!("✅ Loaded {} embeddings (dimension: {})", 
          cache.embeddings.len(), cache.dimension);

    Ok(cache)
}

/// Save embedding cache to disk
pub async fn save_embedding_cache(cache: &EmbeddingCache, path: impl AsRef<Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(cache)
        .context("Failed to serialize embedding cache")?;
    
    tokio::fs::write(path.as_ref(), json).await
        .context("Failed to write embedding cache")?;

    Ok(())
}

/// Simple TF-IDF based embedding as fallback
///
/// This is a very basic fallback if no pre-computed embeddings are available.
/// Uses term frequency to create a sparse vector representation.
///
/// **Note**: This is NOT as good as proper sentence embeddings, but provides
/// a zero-dependency fallback for initial testing.
pub fn simple_tfidf_embedding(text: &str, vocab_size: usize) -> EmbeddingVector {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut embedding = vec![0.0_f32; vocab_size];
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return embedding;
    }

    // Use word hashing to map to vector indices
    for word in &words {
        let mut hasher = DefaultHasher::new();
        word.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % vocab_size;
        embedding[idx] += 1.0;
    }

    // Normalize by L2 norm
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in &mut embedding {
            *val /= norm;
        }
    }

    embedding
}

/// Embedding generator interface
///
/// TODO: Integrate with Rust-native embedding library (e.g., fastembed, rust-bert)
/// For now, relies on pre-computed embeddings from Python script.
pub struct EmbeddingGenerator {
    cache: Option<EmbeddingCache>,
    fallback_dim: usize,
}

impl EmbeddingGenerator {
    /// Create a new generator with optional pre-computed cache
    pub fn new(cache: Option<EmbeddingCache>) -> Self {
        let fallback_dim = cache.as_ref()
            .map(|c| c.dimension)
            .unwrap_or(DEFAULT_EMBEDDING_DIM);

        Self {
            cache,
            fallback_dim,
        }
    }

    /// Generate embedding for text
    pub fn embed(&self, event_id: &str, text: &str) -> EmbeddingVector {
        // Try to get from cache first
        if let Some(cache) = &self.cache {
            if let Some(embedding) = cache.get(event_id) {
                return embedding.clone();
            }
        }

        // Fallback: simple TF-IDF
        warn!("No cached embedding for '{}', using TF-IDF fallback", event_id);
        simple_tfidf_embedding(text, self.fallback_dim)
    }

    /// Check if embedding is available in cache
    pub fn has_embedding(&self, event_id: &str) -> bool {
        self.cache.as_ref()
            .and_then(|c| c.get(event_id))
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![1.0, 2.0, 3.0, 4.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6, "Identical vectors should have similarity 1.0");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Orthogonal vectors should have similarity ~0.0");
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![-1.0, -2.0, -3.0, -4.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6, "Opposite vectors should have similarity -1.0");
    }

    #[test]
    fn test_simple_tfidf() {
        let text = "lakers versus warriors winner";
        let embedding = simple_tfidf_embedding(text, 128);
        
        assert_eq!(embedding.len(), 128);
        
        // Check normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "Embedding should be normalized");
    }

    #[test]
    fn test_find_best_match() {
        let query = vec![1.0, 0.0, 0.0, 0.0];
        
        // Create vectors that live long enough
        let cand0 = vec![0.9, 0.1, 0.0, 0.0];
        let cand1 = vec![0.0, 1.0, 0.0, 0.0];
        let cand2 = vec![0.95, 0.05, 0.0, 0.0];
        
        let candidates = vec![
            (0, cand0.as_slice()),
            (1, cand1.as_slice()),
            (2, cand2.as_slice()),
        ];

        let result = find_best_match(&query, &candidates, 0.5);
        assert!(result.is_some());
        let (idx, score) = result.unwrap();
        assert_eq!(idx, 2, "Should find candidate 2 as best match");
        assert!(score > 0.9, "Score should be high");
    }
}
