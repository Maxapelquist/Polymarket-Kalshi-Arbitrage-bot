//! Polymarket platform integration client - RAW OBSERVATION ONLY
//!
//! This branch observes market reality.
//! It does not attempt to understand it.
//!
//! This module provides REST API client for fetching ALL Polymarket markets.
//! NO execution, NO WebSocket, NO arbitrage detection.

use anyhow::{Context, Result};
use std::time::Duration;
use tracing::{debug, info};

use crate::config::GAMMA_API_BASE;
use crate::types::PolymarketMarketRaw;

// === Polymarket Gamma API Client ===

pub struct PolymarketApiClient {
    http: reqwest::Client,
}

impl PolymarketApiClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Fetch ALL markets from Polymarket Gamma API
    /// 
    /// This is the core observation method:
    /// - Fetches all available markets
    /// - Returns complete raw data structure
    /// - NO filtering, NO interpretation
    pub async fn discover_all_markets(&self) -> Result<Vec<PolymarketMarketRaw>> {
        info!("🔍 [POLYMARKET] Starting full market discovery...");

        // Gamma API endpoint for all markets
        // We use /markets endpoint which returns all active markets
        let url = format!("{}/markets", GAMMA_API_BASE);

        debug!("[POLYMARKET] Fetching from: {}", url);

        let resp = self.http
            .get(&url)
            .send()
            .await
            .context("Failed to fetch markets from Polymarket")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Polymarket API error {}: {}", status, body);
        }

        // Gamma API returns an array of markets directly
        let markets: Vec<PolymarketMarketRaw> = resp.json().await
            .context("Failed to parse Polymarket markets response")?;

        info!("✅ [POLYMARKET] Full discovery complete: {} markets", markets.len());
        Ok(markets)
    }

    /// Fetch a specific market by slug (for future use)
    #[allow(dead_code)]
    pub async fn get_market(&self, slug: &str) -> Result<PolymarketMarketRaw> {
        let url = format!("{}/markets/{}", GAMMA_API_BASE, slug);
        
        let resp = self.http
            .get(&url)
            .send()
            .await
            .context("Failed to fetch market from Polymarket")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Polymarket API error {}: {}", status, body);
        }

        let market: PolymarketMarketRaw = resp.json().await
            .context("Failed to parse Polymarket market response")?;

        Ok(market)
    }
}

impl Default for PolymarketApiClient {
    fn default() -> Self {
        Self::new()
    }
}
