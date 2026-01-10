# Matcher Usage Examples

Practical examples for integrating the semantic matcher into your arbitrage bot.

## Example 1: Standalone Matcher Test

Test the matcher independently without modifying the main bot:

```rust
use prediction_market_arbitrage::matcher::{
    MatchRegistry, MatchEngine, EventDescriptor,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize
    let registry = MatchRegistry::load_or_new("match_registry.json").await?;
    let engine = MatchEngine::new();

    // Sample events
    let kalshi = engine.prepare_event(
        Arc::from("KXNBA-24-LAL-GSW-01"),
        Arc::from("Will the Lakers defeat the Warriors on January 15?"),
    );

    let poly_candidates = vec![
        engine.prepare_event(
            Arc::from("lakers-ml"),
            Arc::from("Lakers moneyline winner"),
        ),
        engine.prepare_event(
            Arc::from("warriors-win"),
            Arc::from("Warriors victory over Lakers"),
        ),
        engine.prepare_event(
            Arc::from("unrelated"),
            Arc::from("Trump wins New Hampshire"),
        ),
    ];

    // Try matching
    match engine.match_and_register(kalshi, poly_candidates, &registry).await? {
        Some(pair) => {
            println!("✅ Match found!");
            println!("   Kalshi: {}", pair.kalshi_description);
            println!("   Poly:   {}", pair.poly_description);
            println!("   Score:  {:.3}", pair.similarity);
            println!("   Quality: {:?}", pair.quality);
        }
        None => {
            println!("❌ No match found above threshold");
        }
    }

    // Save registry
    registry.save().await?;
    
    // Print stats
    println!("\nRegistry stats: {}", registry.stats().await);

    Ok(())
}
```

## Example 2: Integration with Discovery Pipeline

Add semantic matching as a fallback in your discovery logic:

```rust
use prediction_market_arbitrage::matcher::{MatchRegistry, MatchEngine};
use prediction_market_arbitrage::discovery::DiscoveryClient;
use prediction_market_arbitrage::types::MarketPair;
use std::sync::Arc;

pub struct EnhancedDiscovery {
    discovery: Arc<DiscoveryClient>,
    registry: Arc<MatchRegistry>,
    engine: Arc<MatchEngine>,
}

impl EnhancedDiscovery {
    pub async fn new(discovery: DiscoveryClient) -> anyhow::Result<Self> {
        let registry = Arc::new(
            MatchRegistry::load_or_new("match_registry.json").await?
        );
        let engine = Arc::new(MatchEngine::new());

        Ok(Self {
            discovery: Arc::new(discovery),
            registry,
            engine,
        })
    }

    /// Discover markets with semantic matching fallback
    pub async fn discover_with_matching(
        &self,
        leagues: &[&str],
    ) -> anyhow::Result<Vec<MarketPair>> {
        // Run standard discovery first
        let mut pairs = self.discovery.discover(leagues).await?.pairs;

        // For unmatched Kalshi events, try semantic matching
        let unmatched_kalshi = self.get_unmatched_kalshi_events().await?;
        let poly_candidates = self.get_poly_candidate_events().await?;

        for kalshi_event in unmatched_kalshi {
            // Check registry first
            if self.registry.is_kalshi_matched(&kalshi_event.ticker).await {
                if let Some(match_pair) = self.registry.get_by_kalshi(&kalshi_event.ticker).await {
                    // Convert matched pair to MarketPair and add to results
                    if let Some(market_pair) = self.convert_to_market_pair(match_pair).await? {
                        pairs.push(market_pair);
                    }
                }
                continue;
            }

            // Try semantic matching
            let kalshi_desc = self.engine.prepare_event(
                Arc::from(kalshi_event.ticker.as_str()),
                Arc::from(kalshi_event.title.as_str()),
            );

            let poly_descs: Vec<_> = poly_candidates.iter()
                .map(|p| self.engine.prepare_event(
                    Arc::from(p.slug.as_str()),
                    Arc::from(p.question.as_str()),
                ))
                .collect();

            if let Some(match_pair) = self.engine.match_and_register(
                kalshi_desc,
                poly_descs,
                &self.registry,
            ).await? {
                println!("🎯 Semantic match: {} <-> {}",
                         match_pair.kalshi_description,
                         match_pair.poly_description);

                // Convert to MarketPair and add
                if let Some(market_pair) = self.convert_to_market_pair(match_pair).await? {
                    pairs.push(market_pair);
                }
            }
        }

        // Persist registry updates
        self.registry.save().await?;

        Ok(pairs)
    }

    // Helper methods
    async fn get_unmatched_kalshi_events(&self) -> anyhow::Result<Vec<KalshiEvent>> {
        // Fetch from Kalshi API
        todo!("Implement Kalshi event fetching")
    }

    async fn get_poly_candidate_events(&self) -> anyhow::Result<Vec<PolyEvent>> {
        // Fetch from Polymarket Gamma API
        todo!("Implement Poly event fetching")
    }

    async fn convert_to_market_pair(
        &self,
        match_pair: MatchedPair,
    ) -> anyhow::Result<Option<MarketPair>> {
        // Convert MatchedPair to your existing MarketPair type
        // This requires looking up additional details (tokens, tickers, etc.)
        todo!("Implement conversion logic")
    }
}
```

## Example 3: Background Matching Task

Run the matcher as a background task that continuously updates the registry:

```rust
use prediction_market_arbitrage::matcher::{MatchRegistry, MatchEngine};
use tokio::time::{interval, Duration};
use tracing::{info, error};

/// Background task that continuously matches new events
pub async fn run_matcher_background_task(
    kalshi_api: Arc<KalshiApiClient>,
    poly_api: Arc<GammaClient>,
) -> anyhow::Result<()> {
    let registry = Arc::new(
        MatchRegistry::load_or_new("match_registry.json").await?
    );
    let engine = Arc::new(MatchEngine::new());
    
    let mut tick = interval(Duration::from_secs(300)); // Every 5 minutes

    loop {
        tick.tick().await;

        info!("🔄 Running background matcher...");

        match run_matching_cycle(
            Arc::clone(&kalshi_api),
            Arc::clone(&poly_api),
            Arc::clone(&registry),
            Arc::clone(&engine),
        ).await {
            Ok(new_matches) => {
                if new_matches > 0 {
                    info!("✨ Found {} new matches", new_matches);
                    if let Err(e) = registry.save().await {
                        error!("Failed to save registry: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Matcher cycle failed: {}", e);
            }
        }
    }
}

async fn run_matching_cycle(
    kalshi_api: Arc<KalshiApiClient>,
    poly_api: Arc<GammaClient>,
    registry: Arc<MatchRegistry>,
    engine: Arc<MatchEngine>,
) -> anyhow::Result<usize> {
    // Fetch new events from both platforms
    let kalshi_events = fetch_new_kalshi_events(&kalshi_api).await?;
    let poly_events = fetch_new_poly_events(&poly_api).await?;

    // Filter out already matched
    let unmatched_kalshi: Vec<_> = kalshi_events.into_iter()
        .filter(|e| !registry.is_kalshi_matched(&e.ticker).await)
        .collect();

    if unmatched_kalshi.is_empty() {
        return Ok(0);
    }

    // Prepare descriptors
    let kalshi_descs: Vec<_> = unmatched_kalshi.iter()
        .map(|e| engine.prepare_event(
            Arc::from(e.ticker.as_str()),
            Arc::from(e.title.as_str()),
        ))
        .collect();

    let poly_descs: Vec<_> = poly_events.iter()
        .map(|e| engine.prepare_event(
            Arc::from(e.slug.as_str()),
            Arc::from(e.question.as_str()),
        ))
        .collect();

    // Run batch matching
    engine.batch_match(kalshi_descs, poly_descs, &registry).await
}
```

## Example 4: CLI Tool for Manual Matching

Create a standalone tool to test matches interactively:

```rust
use prediction_market_arbitrage::matcher::{MatchRegistry, MatchEngine};
use std::io::{self, Write};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let registry = MatchRegistry::load_or_new("match_registry.json").await?;
    let engine = MatchEngine::new();

    println!("🎯 Semantic Event Matcher - Interactive Mode\n");

    loop {
        // Get Kalshi event
        print!("Enter Kalshi event ID (or 'quit'): ");
        io::stdout().flush()?;
        let mut kalshi_id = String::new();
        io::stdin().read_line(&mut kalshi_id)?;
        let kalshi_id = kalshi_id.trim();

        if kalshi_id == "quit" {
            break;
        }

        print!("Enter Kalshi event description: ");
        io::stdout().flush()?;
        let mut kalshi_desc = String::new();
        io::stdin().read_line(&mut kalshi_desc)?;
        let kalshi_desc = kalshi_desc.trim();

        // Get Poly candidates
        print!("Enter number of Poly candidates: ");
        io::stdout().flush()?;
        let mut num_str = String::new();
        io::stdin().read_line(&mut num_str)?;
        let num: usize = num_str.trim().parse()?;

        let mut poly_candidates = Vec::new();
        for i in 1..=num {
            print!("  Poly candidate {} ID: ", i);
            io::stdout().flush()?;
            let mut id = String::new();
            io::stdin().read_line(&mut id)?;

            print!("  Poly candidate {} description: ", i);
            io::stdout().flush()?;
            let mut desc = String::new();
            io::stdin().read_line(&mut desc)?;

            poly_candidates.push(engine.prepare_event(
                Arc::from(id.trim()),
                Arc::from(desc.trim()),
            ));
        }

        // Match
        let kalshi_event = engine.prepare_event(
            Arc::from(kalshi_id),
            Arc::from(kalshi_desc),
        );

        match engine.match_and_register(kalshi_event, poly_candidates, &registry).await? {
            Some(pair) => {
                println!("\n✅ MATCH FOUND!");
                println!("   Similarity: {:.3}", pair.similarity);
                println!("   Quality: {:?}", pair.quality);
                println!("   Kalshi: {}", pair.kalshi_description);
                println!("   Poly:   {}", pair.poly_description);
            }
            None => {
                println!("\n❌ No match found (similarity below threshold)");
            }
        }

        println!();
    }

    // Save and show stats
    registry.save().await?;
    println!("\n📊 Final stats: {}", registry.stats().await);

    Ok(())
}
```

## Example 5: Export Matches for Review

Export matched pairs to CSV for manual review:

```rust
use prediction_market_arbitrage::matcher::MatchRegistry;
use std::fs::File;
use std::io::Write;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let registry = MatchRegistry::load_or_new("match_registry.json").await?;
    let pairs = registry.get_all().await;

    let mut csv = File::create("matches_review.csv")?;
    writeln!(csv, "kalshi_id,kalshi_desc,poly_id,poly_desc,similarity,quality")?;

    for pair in pairs {
        writeln!(
            csv,
            "\"{}\",\"{}\",\"{}\",\"{}\",{:.3},{:?}",
            pair.kalshi_event_id,
            pair.kalshi_description,
            pair.poly_event_id,
            pair.poly_description,
            pair.similarity,
            pair.quality,
        )?;
    }

    println!("✅ Exported {} matches to matches_review.csv", registry.len().await);

    Ok(())
}
```

Run with:
```bash
cargo run --example export_matches
```

## Testing the Matcher

Add tests to verify matching behavior:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_similar_events_match() {
        let engine = MatchEngine::new();
        let registry = MatchRegistry::load_or_new(":memory:").await.unwrap();

        let kalshi = engine.prepare_event(
            Arc::from("test-1"),
            Arc::from("Lakers win against Warriors"),
        );

        let poly = vec![
            engine.prepare_event(
                Arc::from("poly-1"),
                Arc::from("Lakers victory over Warriors"),
            ),
        ];

        let result = engine.match_and_register(kalshi, poly, &registry).await.unwrap();
        assert!(result.is_some());
        
        let pair = result.unwrap();
        assert!(pair.similarity >= 0.88);
    }

    #[tokio::test]
    async fn test_dissimilar_events_dont_match() {
        let engine = MatchEngine::new();
        let registry = MatchRegistry::load_or_new(":memory:").await.unwrap();

        let kalshi = engine.prepare_event(
            Arc::from("test-1"),
            Arc::from("Lakers win basketball game"),
        );

        let poly = vec![
            engine.prepare_event(
                Arc::from("poly-1"),
                Arc::from("Trump wins election"),
            ),
        ];

        let result = engine.match_and_register(kalshi, poly, &registry).await.unwrap();
        assert!(result.is_none());
    }
}
```
