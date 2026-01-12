//! System configuration for RAW market observation
//!
//! This branch observes market reality.
//! It does not attempt to understand it.

/// Kalshi REST API base URL
pub const KALSHI_API_BASE: &str = "https://api.elections.kalshi.com/trade-api/v2";

/// Kalshi API rate limit delay (milliseconds between requests)
/// Kalshi limit: 20 req/sec = 50ms minimum. We use 60ms for safety margin.
pub const KALSHI_API_DELAY_MS: u64 = 60;

/// Polymarket Gamma API base URL (market data)
pub const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";
