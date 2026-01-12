//! Polymarket platform integration client - RAW OBSERVATION ONLY
//!
//! This branch observes market reality.
//! It does not attempt to understand it.
//!
//! This module provides REST API client for fetching ALL ACTIVE Polymarket markets
//! via the Gamma API events endpoint.
//!
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

    /// Fetch ALL ACTIVE markets from Polymarket Gamma API
    ///
    /// - Fetches `/events?closed=false`
    /// - Paginates over events
    /// - Extracts all markets from each event
    ///
    /// NO interpretation
    pub async fn discover_all_markets(&self) -> Result<Vec<PolymarketMarketRaw>> {
        info!("🔍 [POLYMARKET] Starting ACTIVE market discovery via Gamma events...");

        let mut all_markets: Vec<PolymarketMarketRaw> = Vec::new();
        let mut offset: usize = 0;
        let limit: usize = 100;

        loop {
            let url = format!(
                "{}/events?closed=false&limit={}&offset={}",
                GAMMA_API_BASE, limit, offset
            );

            debug!("[POLYMARKET] Fetching events from: {}", url);

            let resp = self.http
                .get(&url)
                .send()
                .await
                .context("Failed to fetch events from Polymarket Gamma API")?;

            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("Polymarket API error {}: {}", status, body);
            }

            let json: serde_json::Value = resp.json().await
                .context("Failed to parse Polymarket Gamma events response")?;

            // ✅ FIX: own the Vec so lifetime is correct
            let events: Vec<serde_json::Value> = json
                .get("events")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_else(Vec::new);

            if events.is_empty() {
                break;
            }

            for event in events {
                if let Some(markets) = event.get("markets").and_then(|m| m.as_array()) {
                    for market in markets {
                        let raw: PolymarketMarketRaw =
                            serde_json::from_value(market.clone())
                                .context("Failed to parse Polymarket market from event")?;
                        all_markets.push(raw);
                    }
                }
            }

            offset += limit;
        }

        info!(
            "✅ [POLYMARKET] Active Gamma discovery complete: {} markets",
            all_markets.len()
        );

        Ok(all_markets)
    }

    /// Fetch a specific market by slug (unchanged)
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
