//! Intelligent market discovery and matching system.
//!
//! This module handles the discovery of matching markets between Kalshi and Polymarket,
//! with support for caching, incremental updates, and parallel processing.

use anyhow::{Context, Result};
use futures_util::{stream, StreamExt};
use governor::{Quota, RateLimiter, state::NotKeyed, clock::DefaultClock, middleware::NoOpMiddleware};
use serde::{Serialize, Deserialize};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::cache::TeamCache;
use crate::config::{LeagueConfig, get_league_configs, get_league_config};
use crate::kalshi::KalshiApiClient;
use crate::matcher::{MatchRegistry, MatchEngine};
use crate::polymarket::GammaClient;
use crate::types::{MarketPair, MarketType, DiscoveryResult, KalshiMarket, KalshiEvent};

/// Max concurrent Gamma API requests
const GAMMA_CONCURRENCY: usize = 20;

/// Kalshi rate limit: 2 requests per second (very conservative - they rate limit aggressively)
/// Must be conservative because discovery runs many leagues/series in parallel
const KALSHI_RATE_LIMIT_PER_SEC: u32 = 2;

/// Max concurrent Kalshi API requests GLOBALLY across all leagues/series
/// This is the hard cap - prevents bursting even when rate limiter has tokens
const KALSHI_GLOBAL_CONCURRENCY: usize = 1;

/// Cache file path
const DISCOVERY_CACHE_PATH: &str = ".discovery_cache.json";

/// Cache TTL in seconds (2 hours - new markets appear every ~2 hours)
const CACHE_TTL_SECS: u64 = 2 * 60 * 60;

/// Task for parallel Gamma lookup
struct GammaLookupTask {
    event: Arc<KalshiEvent>,
    market: KalshiMarket,
    poly_slug: String,
    market_type: MarketType,
    league: String,
}

/// Type alias for Kalshi rate limiter
type KalshiRateLimiter = RateLimiter<NotKeyed, governor::state::InMemoryState, DefaultClock, NoOpMiddleware>;

/// Persistent cache for discovered market pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryCache {
    /// Unix timestamp when cache was created
    timestamp_secs: u64,
    /// Cached market pairs
    pairs: Vec<MarketPair>,
    /// Set of known Kalshi market tickers (for incremental updates)
    known_kalshi_tickers: Vec<String>,
}

impl DiscoveryCache {
    fn new(pairs: Vec<MarketPair>) -> Self {
        let known_kalshi_tickers: Vec<String> = pairs.iter()
            .map(|p| p.kalshi_market_ticker.to_string())
            .collect();
        Self {
            timestamp_secs: current_unix_secs(),
            pairs,
            known_kalshi_tickers,
        }
    }

    fn is_expired(&self) -> bool {
        let now = current_unix_secs();
        now.saturating_sub(self.timestamp_secs) > CACHE_TTL_SECS
    }

    fn age_secs(&self) -> u64 {
        current_unix_secs().saturating_sub(self.timestamp_secs)
    }

    fn has_ticker(&self, ticker: &str) -> bool {
        self.known_kalshi_tickers.iter().any(|t| t == ticker)
    }
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Market discovery and matching client for cross-platform market identification
pub struct DiscoveryClient {
    kalshi: Arc<KalshiApiClient>,
    gamma: Arc<GammaClient>,
    pub team_cache: Arc<TeamCache>,
    kalshi_limiter: Arc<KalshiRateLimiter>,
    match_registry: Arc<MatchRegistry>,
    match_engine: Arc<MatchEngine>,
    kalshi_semaphore: Arc<Semaphore>,  // Global concurrency limit for Kalshi
    gamma_semaphore: Arc<Semaphore>,
}

impl DiscoveryClient {
    pub async fn new(kalshi: KalshiApiClient, team_cache: TeamCache) -> Result<Self> {
        // Create token bucket rate limiter for Kalshi
        let quota = Quota::per_second(NonZeroU32::new(KALSHI_RATE_LIMIT_PER_SEC).unwrap());
        let kalshi_limiter = Arc::new(RateLimiter::direct(quota));

        // Initialize semantic matcher registry and engine
        let registry = MatchRegistry::load_or_new("match_registry.json")
            .await
            .context("Failed to load match registry")?;
        let match_registry = Arc::new(registry);
        let match_engine = Arc::new(MatchEngine::new());

        info!("🧠 Semantic matcher initialized with {} existing matches", 
              match_registry.len().await);

        Ok(Self {
            kalshi: Arc::new(kalshi),
            gamma: Arc::new(GammaClient::new()),
            team_cache: Arc::new(team_cache),
            kalshi_limiter,
            match_registry,
            match_engine,
            kalshi_semaphore: Arc::new(Semaphore::new(KALSHI_GLOBAL_CONCURRENCY)),
            gamma_semaphore: Arc::new(Semaphore::new(GAMMA_CONCURRENCY)),
        })
    }

    /// Load cache from disk (async)
    async fn load_cache() -> Option<DiscoveryCache> {
        let data = tokio::fs::read_to_string(DISCOVERY_CACHE_PATH).await.ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save cache to disk (async)
    async fn save_cache(cache: &DiscoveryCache) -> Result<()> {
        let data = serde_json::to_string_pretty(cache)?;
        tokio::fs::write(DISCOVERY_CACHE_PATH, data).await?;
        Ok(())
    }
    
    /// Discover all market pairs with caching support
    ///
    /// Strategy:
    /// 1. Try to load cache from disk
    /// 2. If cache exists and is fresh (<2 hours), use it directly
    /// 3. If cache exists but is stale, load it + fetch incremental updates
    /// 4. If no cache, do full discovery
    pub async fn discover_all(&self, leagues: &[&str]) -> DiscoveryResult {
        // Try to load existing cache
        let cached = Self::load_cache().await;

        match cached {
            Some(cache) if !cache.is_expired() => {
                // Cache is fresh - use it directly
                info!("📂 Loaded {} pairs from cache (age: {}s)",
                      cache.pairs.len(), cache.age_secs());
                return DiscoveryResult {
                    pairs: cache.pairs,
                    kalshi_events_found: 0,  // From cache
                    poly_matches: 0,
                    poly_misses: 0,
                    errors: vec![],
                };
            }
            Some(cache) => {
                // Cache is stale - do incremental discovery
                info!("📂 Cache expired (age: {}s), doing incremental refresh...", cache.age_secs());
                return self.discover_incremental(leagues, cache).await;
            }
            None => {
                // No cache - do full discovery
                info!("📂 No cache found, doing full discovery...");
            }
        }

        // Full discovery (no cache)
        let result = self.discover_full(leagues).await;

        // Save to cache
        if !result.pairs.is_empty() {
            let cache = DiscoveryCache::new(result.pairs.clone());
            if let Err(e) = Self::save_cache(&cache).await {
                warn!("Failed to save discovery cache: {}", e);
            } else {
                info!("💾 Saved {} pairs to cache", result.pairs.len());
            }
        }

        result
    }

    /// Force full discovery (ignores cache)
    pub async fn discover_all_force(&self, leagues: &[&str]) -> DiscoveryResult {
        info!("🔄 Forced full discovery (ignoring cache)...");
        let result = self.discover_full(leagues).await;

        // Save to cache
        if !result.pairs.is_empty() {
            let cache = DiscoveryCache::new(result.pairs.clone());
            if let Err(e) = Self::save_cache(&cache).await {
                warn!("Failed to save discovery cache: {}", e);
            } else {
                info!("💾 Saved {} pairs to cache", result.pairs.len());
            }
        }

        result
    }

    /// Full discovery without cache
    async fn discover_full(&self, leagues: &[&str]) -> DiscoveryResult {
        // NEW: If leagues is empty, use universal discovery (all markets)
        if leagues.is_empty() {
            return self.discover_all_universal().await;
        }
        
        // OLD: Sport-specific discovery (kept for backwards compatibility)
        let configs: Vec<_> = leagues.iter()
            .filter_map(|l| get_league_config(l))
            .collect();

        // Parallel discovery across all leagues
        let league_futures: Vec<_> = configs.iter()
            .map(|config| self.discover_league(config, None))
            .collect();

        let league_results = futures_util::future::join_all(league_futures).await;

        // Merge results
        let mut result = DiscoveryResult::default();
        for league_result in league_results {
            result.pairs.extend(league_result.pairs);
            result.poly_matches += league_result.poly_matches;
            result.errors.extend(league_result.errors);
        }
        result.kalshi_events_found = result.pairs.len();

        result
    }
    
    /// Universal discovery: fetch ALL markets from both platforms and use semantic matching
    /// This is NOT limited to sports - works for politics, crypto, weather, etc.
    async fn discover_all_universal(&self) -> DiscoveryResult {
        info!("🌐 Starting UNIVERSAL market discovery (all categories)...");
        info!("   Fetching markets from both platforms...");
        
        let mut result = DiscoveryResult::default();
        
        // Step 1: Fetch ALL open markets from Kalshi (up to 2000 for diversity)
        let kalshi_markets = match self.kalshi.get_all_open_markets(2000).await {
            Ok(markets) => {
                info!("✅ Kalshi: {} total markets fetched (all categories)", markets.len());
                markets
            }
            Err(e) => {
                result.errors.push(format!("Failed to fetch Kalshi markets: {}", e));
                return result;
            }
        };
        
        // Step 2: Fetch ALL active markets from Polymarket (up to 1000)
        let poly_markets = match self.gamma.fetch_active_events(1000).await {
            Ok(markets) => {
                info!("✅ Polymarket: {} total markets fetched (all categories)", markets.len());
                markets
            }
            Err(e) => {
                result.errors.push(format!("Failed to fetch Polymarket markets: {}", e));
                return result;
            }
        };
        
        if poly_markets.is_empty() {
            warn!("⚠️ No Polymarket markets available");
            return result;
        }
        
        // Step 3: Prepare Polymarket descriptors for matching
        let poly_descriptors: Vec<_> = poly_markets.iter()
            .map(|(question, slug, _, _)| {
                self.match_engine.prepare_event(
                    Arc::from(slug.as_str()),
                    Arc::from(question.as_str()),
                )
            })
            .collect();
        
        let kalshi_count = kalshi_markets.len();
        
        info!("🧠 Starting semantic matching ({} Kalshi × {} Polymarket)...", 
              kalshi_count, poly_markets.len());
        info!("   This will process {} market combinations", kalshi_count * poly_markets.len());
        
        // Kalshi markets are multi-prop parlays - extract first proposition only
        // Example: "yes Buffalo,yes Josh Allen: 175+" → "yes Buffalo"
        let kalshi_markets: Vec<_> = kalshi_markets.into_iter()
            .map(|(ticker, title, market)| {
                // Extract first proposition (before first comma)
                let simple_title = title.split(',').next().unwrap_or(&title).trim().to_string();
                (ticker, simple_title, market)
            })
            .collect();
        
        let filtered_count = kalshi_markets.len();
        info!("📊 Extracted first proposition from {} Kalshi parlay markets", filtered_count);
        
        // DEBUG: Log first 3 Kalshi and Polymarket titles to verify data
        info!("📝 Sample Kalshi markets:");
        for (i, (ticker, title, _)) in kalshi_markets.iter().enumerate().take(3) {
            info!("   K{}: {} | {}", i+1, ticker, title);
        }
        info!("📝 Sample Polymarket markets:");
        for (i, (question, slug, _, _)) in poly_markets.iter().enumerate().take(3) {
            info!("   P{}: {} | {}", i+1, slug, question);
        }
        
        let mut matched_count = 0;
        let mut skipped_count = 0;
        let mut processed = 0;
        
        // Step 4: For each Kalshi market, try to find a semantic match
        for (event_ticker, event_title, market) in kalshi_markets {
            processed += 1;
            
            // Progress indicator every 20 markets
            if processed % 20 == 0 {
                info!("   🔄 Progress: {}/{} Kalshi markets processed ({} matches, {} skipped)", 
                      processed, filtered_count, matched_count, skipped_count);
            }
            // Check if already matched (O(1) lookup in registry)
            if self.match_registry.is_kalshi_matched(&market.ticker).await {
                if let Some(matched_pair) = self.match_registry.get_by_kalshi(&market.ticker).await {
                    let poly_slug = matched_pair.poly_event_id.to_string();
                    
                    // Lookup tokens from Polymarket
                    if let Ok(Some((yes_token, no_token))) = self.gamma.lookup_market(&poly_slug).await {
                        result.pairs.push(MarketPair {
                            pair_id: format!("{}-{}", poly_slug, market.ticker).into(),
                            league: "universal".into(),  // No specific league
                            market_type: MarketType::Moneyline,  // Default to moneyline for binary markets
                            description: format!("{} - {}", event_title, market.title).into(),
                            kalshi_event_ticker: event_ticker.into(),
                            kalshi_market_ticker: market.ticker.into(),
                            poly_slug: poly_slug.into(),
                            poly_yes_token: yes_token.into(),
                            poly_no_token: no_token.into(),
                            line_value: market.floor_strike,
                            team_suffix: None,
                        });
                        matched_count += 1;
                    }
                }
                continue;
            }
            
            // Prepare Kalshi event descriptor
            let kalshi_desc = format!("{} - {}", event_title, market.title);
            let kalshi_event = self.match_engine.prepare_event(
                Arc::from(market.ticker.as_str()),
                Arc::from(kalshi_desc.as_str()),
            );
            
            // Try to find best semantic match
            if let Some((best_idx, similarity)) = self.match_engine.match_event(&kalshi_event, &poly_descriptors) {
                let (question, slug, yes_token, no_token) = &poly_markets[best_idx];
                let quality = crate::matcher::MatchQuality::from_similarity(similarity);
                
                info!("  🎯 MATCH: '{}' ↔ '{}' (score: {:.3}, quality: {:?})",
                      kalshi_desc, question, similarity, quality);
                
                // Register the match
                let matched_pair = crate::matcher::MatchedPair {
                    kalshi_event_id: Arc::from(market.ticker.as_str()),
                    kalshi_description: Arc::from(kalshi_desc.as_str()),
                    poly_event_id: Arc::from(slug.as_str()),
                    poly_description: Arc::from(question.as_str()),
                    similarity,
                    matched_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    quality,
                };
                
                // Save to registry (async)
                if let Err(e) = self.match_registry.add_match(matched_pair).await {
                    warn!("Failed to add match to registry: {}", e);
                }
                
                // Create market pair
                result.pairs.push(MarketPair {
                    pair_id: format!("{}-{}", slug, market.ticker).into(),
                    league: "universal".into(),
                    market_type: MarketType::Moneyline,
                    description: format!("{} - {}", event_title, market.title).into(),
                    kalshi_event_ticker: event_ticker.into(),
                    kalshi_market_ticker: market.ticker.into(),
                    poly_slug: slug.clone().into(),
                    poly_yes_token: yes_token.clone().into(),
                    poly_no_token: no_token.clone().into(),
                    line_value: market.floor_strike,
                    team_suffix: None,
                });
                
                matched_count += 1;
            } else {
                skipped_count += 1;
            }
        }
        
        // Save registry to disk
        if let Err(e) = self.match_registry.save().await {
            warn!("Failed to save match registry: {}", e);
        }
        
        info!("✅ Universal discovery complete: {} matches, {} skipped",
              matched_count, skipped_count);
        
        result.kalshi_events_found = kalshi_count;
        result.poly_matches = matched_count;
        result.poly_misses = skipped_count;
        
        result
    }

    /// Incremental discovery - merge cached pairs with newly discovered ones
    async fn discover_incremental(&self, leagues: &[&str], cache: DiscoveryCache) -> DiscoveryResult {
        let configs: Vec<_> = if leagues.is_empty() {
            get_league_configs()
        } else {
            leagues.iter()
                .filter_map(|l| get_league_config(l))
                .collect()
        };

        // Discover with filter for known tickers
        let league_futures: Vec<_> = configs.iter()
            .map(|config| self.discover_league(config, Some(&cache)))
            .collect();

        let league_results = futures_util::future::join_all(league_futures).await;

        // Merge cached pairs with newly discovered ones
        let mut all_pairs = cache.pairs;
        let mut new_count = 0;

        for league_result in league_results {
            for pair in league_result.pairs {
                if !all_pairs.iter().any(|p| *p.kalshi_market_ticker == *pair.kalshi_market_ticker) {
                    all_pairs.push(pair);
                    new_count += 1;
                }
            }
        }

        if new_count > 0 {
            info!("🆕 Found {} new market pairs", new_count);

            // Update cache
            let new_cache = DiscoveryCache::new(all_pairs.clone());
            if let Err(e) = Self::save_cache(&new_cache).await {
                warn!("Failed to update discovery cache: {}", e);
            } else {
                info!("💾 Updated cache with {} total pairs", all_pairs.len());
            }
        } else {
            info!("✅ No new markets found, using {} cached pairs", all_pairs.len());

            // Just update timestamp to extend TTL
            let refreshed_cache = DiscoveryCache::new(all_pairs.clone());
            let _ = Self::save_cache(&refreshed_cache).await;
        }

        DiscoveryResult {
            pairs: all_pairs,
            kalshi_events_found: new_count,
            poly_matches: new_count,
            poly_misses: 0,
            errors: vec![],
        }
    }
    
    /// Discover all market types for a single league (PARALLEL)
    /// If cache is provided, only discovers markets not already in cache
    async fn discover_league(&self, config: &LeagueConfig, cache: Option<&DiscoveryCache>) -> DiscoveryResult {
        info!("🔍 Discovering {} markets...", config.league_code);

        let market_types = [MarketType::Moneyline, MarketType::Spread, MarketType::Total, MarketType::Btts];

        // Parallel discovery across market types
        let type_futures: Vec<_> = market_types.iter()
            .filter_map(|market_type| {
                let series = self.get_series_for_type(config, *market_type)?;
                Some(self.discover_series(config, series, *market_type, cache))
            })
            .collect();

        let type_results = futures_util::future::join_all(type_futures).await;

        let mut result = DiscoveryResult::default();
        for (pairs_result, market_type) in type_results.into_iter().zip(market_types.iter()) {
            match pairs_result {
                Ok(pairs) => {
                    let count = pairs.len();
                    if count > 0 {
                        info!("  ✅ {} {}: {} pairs", config.league_code, market_type, count);
                    }
                    result.poly_matches += count;
                    result.pairs.extend(pairs);
                }
                Err(e) => {
                    result.errors.push(format!("{} {}: {}", config.league_code, market_type, e));
                }
            }
        }

        result
    }
    
    fn get_series_for_type(&self, config: &LeagueConfig, market_type: MarketType) -> Option<&'static str> {
        match market_type {
            MarketType::Moneyline => Some(config.kalshi_series_game),
            MarketType::Spread => config.kalshi_series_spread,
            MarketType::Total => config.kalshi_series_total,
            MarketType::Btts => config.kalshi_series_btts,
        }
    }
    
    /// Discover markets for a specific series (PARALLEL Kalshi + Gamma lookups)
    /// If cache is provided, skips markets already in cache
    async fn discover_series(
        &self,
        config: &LeagueConfig,
        series: &str,
        market_type: MarketType,
        cache: Option<&DiscoveryCache>,
    ) -> Result<Vec<MarketPair>> {
        // Fetch Kalshi events
        {
            let _permit = self.kalshi_semaphore.acquire().await.map_err(|e| anyhow::anyhow!("semaphore closed: {}", e))?;
            self.kalshi_limiter.until_ready().await;
        }
        let events = self.kalshi.get_events(series, 50).await?;

        // PHASE 2: Parallel market fetching 
        let kalshi = self.kalshi.clone();
        let limiter = self.kalshi_limiter.clone();
        let semaphore = self.kalshi_semaphore.clone();

        // Parse events first, filtering out unparseable ones
        let parsed_events: Vec<_> = events.into_iter()
            .filter_map(|event| {
                let parsed = match parse_kalshi_event_ticker(&event.event_ticker) {
                    Some(p) => p,
                    None => {
                        warn!("  ⚠️ Could not parse event ticker {}", event.event_ticker);
                        return None;
                    }
                };
                Some((parsed, event))
            })
            .collect();

        // Execute market fetches with GLOBAL concurrency limit
        let market_results: Vec<_> = stream::iter(parsed_events)
            .map(|(parsed, event)| {
                let kalshi = kalshi.clone();
                let limiter = limiter.clone();
                let semaphore = semaphore.clone();
                let event_ticker = event.event_ticker.clone();
                async move {
                    let _permit = semaphore.acquire().await.ok();
                    // rate limit
                    limiter.until_ready().await;
                    let markets_result = kalshi.get_markets(&event_ticker).await;
                    (parsed, Arc::new(event), markets_result)
                }
            })
            .buffer_unordered(KALSHI_GLOBAL_CONCURRENCY * 2)  // Allow some buffering, semaphore is the real limit
            .collect()
            .await;

        // Collect all (event, market) pairs
        let mut event_markets = Vec::with_capacity(market_results.len() * 3);
        for (parsed, event, markets_result) in market_results {
            match markets_result {
                Ok(markets) => {
                    for market in markets {
                        // Skip if already in cache
                        if let Some(c) = cache {
                            if c.has_ticker(&market.ticker) {
                                continue;
                            }
                        }
                        event_markets.push((parsed.clone(), event.clone(), market));
                    }
                }
                Err(e) => {
                    warn!("  ⚠️ Failed to get markets for {}: {}", event.event_ticker, e);
                }
            }
        }
        
        // Parallel Gamma lookups with semaphore
        let lookup_futures: Vec<_> = event_markets
            .into_iter()
            .map(|(parsed, event, market)| {
                let poly_slug = self.build_poly_slug(config.poly_prefix, &parsed, market_type, &market);
                
                GammaLookupTask {
                    event,
                    market,
                    poly_slug,
                    market_type,
                    league: config.league_code.to_string(),
                }
            })
            .collect();
        
        // Execute lookups in parallel 
        let pairs: Vec<MarketPair> = stream::iter(lookup_futures)
            .map(|task| {
                let gamma = self.gamma.clone();
                let semaphore = self.gamma_semaphore.clone();
                let match_registry = self.match_registry.clone();
                let match_engine = self.match_engine.clone();
                
                async move {
                    let _permit = semaphore.acquire().await.ok()?;
                    
                    // Step 1: Try rule-based matching (existing logic)
                    match gamma.lookup_market(&task.poly_slug).await {
                        Ok(Some((yes_token, no_token))) => {
                            let team_suffix = extract_team_suffix(&task.market.ticker);
                            Some(MarketPair {
                                pair_id: format!("{}-{}", task.poly_slug, task.market.ticker).into(),
                                league: task.league.into(),
                                market_type: task.market_type,
                                description: format!("{} - {}", task.event.title, task.market.title).into(),
                                kalshi_event_ticker: task.event.event_ticker.clone().into(),
                                kalshi_market_ticker: task.market.ticker.into(),
                                poly_slug: task.poly_slug.into(),
                                poly_yes_token: yes_token.into(),
                                poly_no_token: no_token.into(),
                                line_value: task.market.floor_strike,
                                team_suffix: team_suffix.map(|s| s.into()),
                            })
                        }
                        Ok(None) => {
                            // Step 2: Rule-based matching failed, try semantic matching
                            try_semantic_match(
                                &task,
                                &gamma,
                                &match_registry,
                                &match_engine,
                            ).await
                        }
                        Err(e) => {
                            warn!("  ⚠️ Gamma lookup failed for {}: {}", task.poly_slug, e);
                            None
                        }
                    }
                }
            })
            .buffer_unordered(GAMMA_CONCURRENCY)
            .filter_map(|x| async { x })
            .collect()
            .await;
        
        Ok(pairs)
    }
    
    /// Build Polymarket slug from Kalshi event data
    fn build_poly_slug(
        &self,
        poly_prefix: &str,
        parsed: &ParsedKalshiTicker,
        market_type: MarketType,
        market: &KalshiMarket,
    ) -> String {
        // Convert Kalshi team codes to Polymarket codes using cache
        let poly_team1 = self.team_cache
            .kalshi_to_poly(poly_prefix, &parsed.team1)
            .unwrap_or_else(|| parsed.team1.to_lowercase());
        let poly_team2 = self.team_cache
            .kalshi_to_poly(poly_prefix, &parsed.team2)
            .unwrap_or_else(|| parsed.team2.to_lowercase());
        
        // Convert date from "25DEC27" to "2025-12-27"
        let date_str = kalshi_date_to_iso(&parsed.date);
        
        // Base slug: league-team1-team2-date
        let base = format!("{}-{}-{}-{}", poly_prefix, poly_team1, poly_team2, date_str);
        
        match market_type {
            MarketType::Moneyline => {
                if let Some(suffix) = extract_team_suffix(&market.ticker) {
                    if suffix.to_lowercase() == "tie" {
                        format!("{}-draw", base)
                    } else {
                        let poly_suffix = self.team_cache
                            .kalshi_to_poly(poly_prefix, &suffix)
                            .unwrap_or_else(|| suffix.to_lowercase());
                        format!("{}-{}", base, poly_suffix)
                    }
                } else {
                    base
                }
            }
            MarketType::Spread => {
                if let Some(floor) = market.floor_strike {
                    let floor_str = format!("{:.1}", floor).replace(".", "pt");
                    format!("{}-spread-{}", base, floor_str)
                } else {
                    format!("{}-spread", base)
                }
            }
            MarketType::Total => {
                if let Some(floor) = market.floor_strike {
                    let floor_str = format!("{:.1}", floor).replace(".", "pt");
                    format!("{}-total-{}", base, floor_str)
                } else {
                    format!("{}-total", base)
                }
            }
            MarketType::Btts => {
                format!("{}-btts", base)
            }
        }
    }
}

// === Helpers ===

/// Try semantic matching when rule-based matching fails
async fn try_semantic_match(
    task: &GammaLookupTask,
    gamma: &Arc<GammaClient>,
    match_registry: &Arc<MatchRegistry>,
    match_engine: &Arc<MatchEngine>,
) -> Option<MarketPair> {
    // Step 1: Check if already semantically matched (O(1) lookup)
    let kalshi_id = &task.market.ticker;
    if match_registry.is_kalshi_matched(kalshi_id).await {
        if let Some(matched_pair) = match_registry.get_by_kalshi(kalshi_id).await {
            // Found existing semantic match - use it
            let poly_slug = matched_pair.poly_event_id.to_string();
            
            // Lookup actual tokens from Polymarket
            match gamma.lookup_market(&poly_slug).await {
                Ok(Some((yes_token, no_token))) => {
                    info!("  ✅ SEMANTIC MATCH (cached): {} -> {} (score: {:.3}, quality: {:?})",
                          kalshi_id, poly_slug, matched_pair.similarity, matched_pair.quality);
                    let team_suffix = extract_team_suffix(&task.market.ticker);
                    return Some(MarketPair {
                        pair_id: format!("{}-{}", poly_slug, task.market.ticker).into(),
                        league: task.league.clone().into(),
                        market_type: task.market_type,
                        description: format!("{} - {}", task.event.title, task.market.title).into(),
                        kalshi_event_ticker: task.event.event_ticker.clone().into(),
                        kalshi_market_ticker: task.market.ticker.clone().into(),
                        poly_slug: poly_slug.into(),
                        poly_yes_token: yes_token.into(),
                        poly_no_token: no_token.into(),
                        line_value: task.market.floor_strike,
                        team_suffix: team_suffix.map(|s| s.into()),
                    });
                }
                _ => {
                    warn!("  ⚠️ Semantic match cached but tokens unavailable: {}", poly_slug);
                    return None;
                }
            }
        }
    }

    // Step 2: Not yet matched - try live semantic matching
    // Fetch active Polymarket events as candidates
    let poly_events = match gamma.fetch_active_events(100).await {
        Ok(events) if !events.is_empty() => events,
        Ok(_) => {
            warn!("  ⚠️ No Polymarket events available for semantic matching");
            return None;
        }
        Err(e) => {
            warn!("  ⚠️ Failed to fetch Polymarket events: {}", e);
            return None;
        }
    };

    // Prepare Kalshi event descriptor
    let kalshi_desc = format!("{} - {}", task.event.title, task.market.title);
    let kalshi_event = match_engine.prepare_event(
        Arc::from(task.market.ticker.as_str()),
        Arc::from(kalshi_desc.as_str()),
    );

    // Prepare Polymarket candidate descriptors
    let poly_descriptors: Vec<_> = poly_events.iter()
        .map(|(question, slug, _, _)| {
            match_engine.prepare_event(
                Arc::from(slug.as_str()),
                Arc::from(question.as_str()),
            )
        })
        .collect();

    // Try to match
    if poly_descriptors.is_empty() {
        return None;
    }

    match match_engine.match_event(&kalshi_event, &poly_descriptors) {
        Some((best_idx, similarity)) => {
            let (question, slug, yes_token, no_token) = &poly_events[best_idx];
            
            info!("  🎯 SEMANTIC MATCH (live): '{}' -> '{}' (score: {:.3}, quality: {:?})",
                  kalshi_desc, question, similarity,
                  crate::matcher::MatchQuality::from_similarity(similarity));

            // Register the match for future O(1) lookups
            let matched_pair = crate::matcher::MatchedPair {
                kalshi_event_id: Arc::from(task.market.ticker.as_str()),
                kalshi_description: Arc::from(kalshi_desc.as_str()),
                poly_event_id: Arc::from(slug.as_str()),
                poly_description: Arc::from(question.as_str()),
                similarity,
                matched_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                quality: crate::matcher::MatchQuality::from_similarity(similarity),
            };

            // Add to registry (non-blocking)
            let registry_clone = match_registry.clone();
            let pair_clone = matched_pair.clone();
            tokio::spawn(async move {
                if let Err(e) = registry_clone.add_match(pair_clone).await {
                    warn!("Failed to add match to registry: {}", e);
                } else if let Err(e) = registry_clone.save().await {
                    warn!("Failed to save match registry: {}", e);
                }
            });

            // Return MarketPair
            let team_suffix = extract_team_suffix(&task.market.ticker);
            Some(MarketPair {
                pair_id: format!("{}-{}", slug, task.market.ticker).into(),
                league: task.league.clone().into(),
                market_type: task.market_type,
                description: format!("{} - {}", task.event.title, task.market.title).into(),
                kalshi_event_ticker: task.event.event_ticker.clone().into(),
                kalshi_market_ticker: task.market.ticker.clone().into(),
                poly_slug: slug.clone().into(),
                poly_yes_token: yes_token.clone().into(),
                poly_no_token: no_token.clone().into(),
                line_value: task.market.floor_strike,
                team_suffix: team_suffix.map(|s| s.into()),
            })
        }
        None => {
            // No match found above threshold
            None
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedKalshiTicker {
    date: String,  // "25DEC27"
    team1: String, // "CFC"
    team2: String, // "AVL"
}

/// Parse Kalshi event ticker like "KXEPLGAME-25DEC27CFCAVL" or "KXNCAAFGAME-25DEC27M-OHFRES"
fn parse_kalshi_event_ticker(ticker: &str) -> Option<ParsedKalshiTicker> {
    let parts: Vec<&str> = ticker.split('-').collect();
    if parts.len() < 2 {
        return None;
    }

    // Handle two formats:
    // 1. "KXEPLGAME-25DEC27CFCAVL" - date+teams in parts[1]
    // 2. "KXNCAAFGAME-25DEC27M-OHFRES" - date in parts[1], teams in parts[2]
    let (date, teams_part) = if parts.len() >= 3 && parts[2].len() >= 4 {
        // Format 2: 3-part ticker with separate teams section
        // parts[1] is like "25DEC27M" (date + optional suffix)
        let date_part = parts[1];
        let date = if date_part.len() >= 7 {
            date_part[..7].to_uppercase()
        } else {
            return None;
        };
        (date, parts[2])
    } else {
        // Format 1: 2-part ticker with combined date+teams
        let date_teams = parts[1];
        // Minimum: 7 (date) + 2 + 2 (min team codes) = 11
        if date_teams.len() < 11 {
            return None;
        }
        let date = date_teams[..7].to_uppercase();
        let teams = &date_teams[7..];
        (date, teams)
    };

    // Split team codes - try to find the best split point
    // Team codes range from 2-4 chars (e.g., OM, CFC, FRES)
    let (team1, team2) = split_team_codes(teams_part);

    Some(ParsedKalshiTicker { date, team1, team2 })
}

/// Split a combined team string into two team codes
/// Tries multiple split strategies based on string length
fn split_team_codes(teams: &str) -> (String, String) {
    let len = teams.len();

    // For 6 chars, could be 3+3, 2+4, or 4+2
    // For 5 chars, could be 2+3 or 3+2
    // For 4 chars, must be 2+2
    // For 7 chars, could be 3+4 or 4+3
    // For 8 chars, could be 4+4, 3+5, 5+3

    match len {
        4 => (teams[..2].to_uppercase(), teams[2..].to_uppercase()),
        5 => {
            // Prefer 2+3 (common for OM+ASM, OL+PSG)
            (teams[..2].to_uppercase(), teams[2..].to_uppercase())
        }
        6 => {
            // Check if it looks like 2+4 pattern (e.g., OHFRES = OH+FRES)
            // Common 2-letter codes: OM, OL, OH, SF, LA, NY, KC, TB, etc.
            let first_two = &teams[..2].to_uppercase();
            if is_likely_two_letter_code(first_two) {
                (first_two.clone(), teams[2..].to_uppercase())
            } else {
                // Default to 3+3
                (teams[..3].to_uppercase(), teams[3..].to_uppercase())
            }
        }
        7 => {
            // Could be 3+4 or 4+3 - prefer 3+4
            (teams[..3].to_uppercase(), teams[3..].to_uppercase())
        }
        _ if len >= 8 => {
            // 4+4 or longer
            (teams[..4].to_uppercase(), teams[4..].to_uppercase())
        }
        _ => {
            let mid = len / 2;
            (teams[..mid].to_uppercase(), teams[mid..].to_uppercase())
        }
    }
}

/// Check if a 2-letter code is a known/likely team abbreviation
fn is_likely_two_letter_code(code: &str) -> bool {
    matches!(
        code,
        // European football (Ligue 1, etc.)
        "OM" | "OL" | "FC" |
        // US sports common abbreviations
        "OH" | "SF" | "LA" | "NY" | "KC" | "TB" | "GB" | "NE" | "NO" | "LV" |
        // Generic short codes
        "BC" | "SC" | "AC" | "AS" | "US"
    )
}

/// Convert Kalshi date "25DEC27" to ISO "2025-12-27"
fn kalshi_date_to_iso(kalshi_date: &str) -> String {
    if kalshi_date.len() != 7 {
        return kalshi_date.to_string();
    }
    
    let year = format!("20{}", &kalshi_date[..2]);
    let month = match &kalshi_date[2..5].to_uppercase()[..] {
        "JAN" => "01", "FEB" => "02", "MAR" => "03", "APR" => "04",
        "MAY" => "05", "JUN" => "06", "JUL" => "07", "AUG" => "08",
        "SEP" => "09", "OCT" => "10", "NOV" => "11", "DEC" => "12",
        _ => "01",
    };
    let day = &kalshi_date[5..7];
    
    format!("{}-{}-{}", year, month, day)
}

/// Extract team suffix from market ticker (e.g., "KXEPLGAME-25DEC27CFCAVL-CFC" -> "CFC")
fn extract_team_suffix(ticker: &str) -> Option<String> {
    let mut splits = ticker.splitn(3, '-');
    splits.next()?; // series
    splits.next()?; // event
    splits.next().map(|s| s.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_kalshi_ticker() {
        let parsed = parse_kalshi_event_ticker("KXEPLGAME-25DEC27CFCAVL").unwrap();
        assert_eq!(parsed.date, "25DEC27");
        assert_eq!(parsed.team1, "CFC");
        assert_eq!(parsed.team2, "AVL");
    }
    
    #[test]
    fn test_kalshi_date_to_iso() {
        assert_eq!(kalshi_date_to_iso("25DEC27"), "2025-12-27");
        assert_eq!(kalshi_date_to_iso("25JAN01"), "2025-01-01");
    }
}
