# Semantic Event Matcher

AI-powered semantic event matching subsystem for intelligent cross-platform event identification.

## Overview

The matcher module enables semantic matching of equivalent events across platforms even when wording differs significantly. It uses sentence embeddings and cosine similarity to match events like:

- "Trump wins the election" ≈ "Republicans win presidency"
- "Inter vs Lecce winner" ≈ "Inter ML"
- "Chiefs defeat Bills" ≈ "Bills lose to Chiefs"

## Architecture

```
matcher/
├── mod.rs           # Module entry point & public API
├── types.rs         # Core types (MatchedPair, EventDescriptor, etc.)
├── registry.rs      # Persistent matched event pairs (O(1) lookups)
├── normalize.rs     # Text normalization & cleaning
├── embed.rs         # Embedding generation & cosine similarity
└── match_engine.rs  # Orchestration & matching logic
```

## Key Features

- **Offline matching**: All AI operations run once at startup or via background task
- **Fast lookups**: O(1) hash-based registry lookups in hot path (zero allocations)
- **Persistent storage**: Matches are cached to disk and never re-computed
- **Graceful degradation**: System continues with rule-based matching if matcher fails
- **No network calls**: Embeddings are pre-computed or generated locally

## Usage

### 1. Generate Embeddings (One-Time Setup)

Use the Python helper script to pre-compute embeddings:

```bash
# Create event lists (example)
cat > events_kalshi.json << EOF
[
  {"event_id": "KXNBA-24-LAL-GSW", "description": "Lakers vs Warriors winner"},
  {"event_id": "KXNFL-24-KC-BUF", "description": "Chiefs defeat Bills"}
]
EOF

cat > events_poly.json << EOF
[
  {"event_id": "lakers-warriors-ml", "description": "Lakers ML"},
  {"event_id": "chiefs-bills", "description": "Bills lose to Chiefs"}
]
EOF

# Generate embeddings
python3 scripts/generate_embeddings.py \
    --kalshi events_kalshi.json \
    --poly events_poly.json \
    -o embeddings.json
```

### 2. Initialize Matcher in Rust

```rust
use prediction_market_arbitrage::matcher::{
    MatchRegistry, MatchEngine, EventDescriptor,
};
use prediction_market_arbitrage::matcher::embed::load_embedding_cache;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load registry (persistent storage)
    let registry = MatchRegistry::load_or_new("match_registry.json").await?;

    // Load pre-computed embeddings
    let embeddings = load_embedding_cache("embeddings.json").await?;

    // Create match engine
    let engine = MatchEngine::with_embeddings(embeddings);

    // Prepare events for matching
    let kalshi_event = engine.prepare_event(
        Arc::from("KXNBA-24-LAL-GSW"),
        Arc::from("Lakers vs Warriors winner"),
    );

    let poly_candidates = vec![
        engine.prepare_event(
            Arc::from("lakers-warriors-ml"),
            Arc::from("Lakers ML"),
        ),
        engine.prepare_event(
            Arc::from("other-event"),
            Arc::from("Unrelated event"),
        ),
    ];

    // Match and register
    if let Some(pair) = engine.match_and_register(
        kalshi_event,
        poly_candidates,
        &registry,
    ).await? {
        println!("Matched: {} <-> {} (score: {:.3})",
                 pair.kalshi_description,
                 pair.poly_description,
                 pair.similarity);
    }

    // Save registry to disk
    registry.save().await?;

    Ok(())
}
```

### 3. Fast Lookups in Hot Path

```rust
// Check if event is already matched (O(1) lookup, zero allocations)
if registry.is_kalshi_matched("KXNBA-24-LAL-GSW").await {
    // Skip matching, use existing pair
    let pair = registry.get_by_kalshi("KXNBA-24-LAL-GSW").await.unwrap();
    println!("Using cached match: {}", pair.poly_event_id);
}
```

## Configuration

### Match Quality Thresholds

```rust
use prediction_market_arbitrage::matcher::match_engine::MatchConfig;

let config = MatchConfig {
    similarity_threshold: 0.88,  // Minimum cosine similarity
    max_candidates: 1000,        // Max candidates to consider
    strict_mode: false,          // Only accept excellent/good matches
};

let engine = MatchEngine::with_config(config);
```

### Quality Levels

| Quality | Similarity | Auto-Match |
|---------|-----------|------------|
| Excellent | ≥ 0.95 | ✅ |
| Good | ≥ 0.90 | ✅ |
| Acceptable | ≥ 0.88 | ✅ |
| Review Needed | < 0.88 | ❌ (manual review) |

## Performance

- **Registry lookups**: O(1) via FxHashMap (< 50ns)
- **Matching**: O(N*D) where N = candidates, D = embedding dimension (384)
- **Memory**: ~1.5KB per embedding (384 floats)
- **Startup**: < 100ms to load 1000 pre-computed embeddings

## File Formats

### Embedding Cache (`embeddings.json`)

```json
{
  "model": "all-MiniLM-L6-v2",
  "dimension": 384,
  "embeddings": {
    "KXNBA-24-LAL-GSW": [0.123, 0.456, ...],
    "lakers-warriors-ml": [0.125, 0.450, ...]
  },
  "created_at": 1704067200
}
```

### Match Registry (`match_registry.json`)

```json
{
  "version": 1,
  "pairs": [
    {
      "kalshi_event_id": "KXNBA-24-LAL-GSW",
      "kalshi_description": "Lakers vs Warriors winner",
      "poly_event_id": "lakers-warriors-ml",
      "poly_description": "Lakers ML",
      "similarity": 0.92,
      "matched_at": 1704067200,
      "quality": "Good"
    }
  ]
}
```

## Integration with Existing Bot

The matcher is designed to augment, not replace, the existing discovery system:

```rust
// In discovery pipeline:
async fn discover_with_matching(
    kalshi_events: Vec<KalshiEvent>,
    registry: &MatchRegistry,
    engine: &MatchEngine,
) -> Vec<MarketPair> {
    let mut pairs = Vec::new();

    for kalshi_event in kalshi_events {
        // 1. Check if already matched
        if let Some(match_pair) = registry.get_by_kalshi(&kalshi_event.ticker).await {
            pairs.push(create_market_pair_from_match(match_pair));
            continue;
        }

        // 2. Try rule-based matching (existing logic)
        if let Some(pair) = try_rule_based_match(&kalshi_event) {
            pairs.push(pair);
            continue;
        }

        // 3. Try semantic matching (new capability)
        let poly_candidates = fetch_poly_candidates().await;
        if let Some(match_pair) = engine.match_and_register(
            prepare_kalshi_event(&kalshi_event),
            poly_candidates,
            registry,
        ).await? {
            pairs.push(create_market_pair_from_match(match_pair));
        }
    }

    pairs
}
```

## Future Enhancements

- [ ] Support for 3-way outcomes (1X2 markets)
- [ ] Support for spread/total markets with line matching
- [ ] Fuzzy team name matching for better sports coverage
- [ ] Active learning for threshold tuning
- [ ] Multi-language support
- [ ] Rust-native embedding generation (using ONNX runtime)

## Dependencies

**Python (for embedding generation)**:
- `sentence-transformers` - Pre-compute embeddings offline

**Rust** (already in `Cargo.toml`):
- `rustc-hash` - Fast hash maps
- `serde` / `serde_json` - Serialization
- `tokio` - Async runtime
- `anyhow` - Error handling

## Troubleshooting

### "No cached embedding for X, using TF-IDF fallback"

The event wasn't in the pre-computed embeddings. Either:
1. Re-run `generate_embeddings.py` with updated event lists
2. Accept TF-IDF fallback (less accurate but functional)

### "No match found" even though events seem similar

- Check similarity threshold (default 0.88 is conservative)
- Verify embeddings were generated correctly
- Check normalized text with `normalize_text()` to see cleaned version
- Consider lowering threshold or using strict_mode=false

### Performance degradation

- Ensure embeddings are loaded at startup, not generated at runtime
- Use pre-computed embeddings for all known events
- Registry lookups should be O(1) - if slow, check for lock contention
