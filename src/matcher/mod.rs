//! AI-powered semantic event matching subsystem.
//!
//! This module provides intelligent matching of equivalent events across platforms
//! using semantic embeddings and cosine similarity, enabling matches even when
//! event descriptions differ significantly in wording.
//!
//! ## Architecture
//!
//! - **Offline matching**: All AI operations run once at startup or via background task
//! - **Fast lookups**: O(1) hash-based registry lookups in hot path
//! - **Persistent storage**: Matches are cached to disk and never re-computed
//! - **Zero allocations**: Hot path uses only references and atomic operations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use matcher::{MatchRegistry, MatchEngine};
//!
//! // Initialize registry (loads from disk if exists)
//! let registry = MatchRegistry::load_or_new("matches.json")?;
//!
//! // Check if events are already matched
//! if let Some(pair) = registry.get_pair("kalshi_ticker_123") {
//!     println!("Already matched with: {}", pair.poly_event_id);
//! } else {
//!     // Run matcher for new events
//!     let engine = MatchEngine::new();
//!     engine.match_and_register(kalshi_event, poly_events, &registry)?;
//! }
//! ```

pub mod types;
pub mod registry;
pub mod normalize;
pub mod embed;
pub mod match_engine;

pub use types::{MatchedPair, EventDescriptor, MatchQuality};
pub use registry::MatchRegistry;
pub use match_engine::MatchEngine;
