//! Market Observer (Kalshi + Polymarket)
//!
//! ╔═══════════════════════════════════════════════════════════════════════╗
//! ║                                                                       ║
//! ║         This branch observes market reality.                         ║
//! ║         It does not attempt to understand it.                        ║
//! ║                                                                       ║
//! ╚═══════════════════════════════════════════════════════════════════════╝
//!
//! ## Purpose
//!
//! This program fetches ALL Kalshi markets via the official API and saves them
//! as raw JSON. No filtering. No interpretation. No matching. No AI.
//!
//! ## Output
//!
//! - `kalshi_raw_markets.json` - Complete dump of all markets with metadata
//!
//! ## Next Steps (NOT in this branch)
//!
//! In the next branch, we will:
//! 1. Feed this raw data to a state-of-the-art LLM
//! 2. Let the LLM read contract texts and understand rules
//! 3. Group markets the way a human would
//! 4. Build probabilistic market matching

use anyhow::Result;
use tracing::info;

mod config;
mod kalshi;
mod polymarket;
mod types;

use kalshi::{KalshiApiClient, KalshiConfig};
use polymarket::PolymarketApiClient;
use types::{RawMarketObservation, MarketForAI};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("kalshi_observer=info".parse().unwrap()),
        )
        .init();

    info!("╔═══════════════════════════════════════════════════════════════════════╗");
    info!("║                                                                       ║");
    info!("║              🔍 Market Observer (Kalshi + Polymarket)                ║");
    info!("║                                                                       ║");
    info!("║         This branch observes market reality.                         ║");
    info!("║         It does not attempt to understand it.                        ║");
    info!("║                                                                       ║");
    info!("╚═══════════════════════════════════════════════════════════════════════╝");
    info!("");

    // Load Kalshi credentials
    let kalshi_config = KalshiConfig::from_env()?;
    info!("✅ [AUTH] Kalshi credentials loaded");

    // Create API client
    let kalshi_client = KalshiApiClient::new(kalshi_config);
    info!("✅ [CLIENT] Kalshi API client initialized");
    info!("");

    // Fetch ALL events with nested markets
    info!("🚀 [DISCOVERY] Starting full Kalshi market discovery...");
    info!("   This will fetch ALL open events and markets from Kalshi");
    info!("   No filtering. No interpretation. Just raw observation.");
    info!("");

    let events = kalshi_client.discover_all_events_paginated().await?;
    
    info!("📊 [DISCOVERY] Raw statistics:");
    info!("   Events discovered: {}", events.len());
    
    // Count total markets
    let total_markets: usize = events.iter()
        .filter_map(|e| e.markets.as_ref())
        .map(|markets| markets.len())
        .sum();
    
    info!("   Markets discovered: {}", total_markets);
    info!("");

    // Convert to observation records
    info!("🔄 [PROCESSING] Converting to observation records...");
    let mut observations: Vec<RawMarketObservation> = Vec::new();
    
    for event in &events {
        if let Some(markets) = &event.markets {
            for market in markets {
                observations.push(RawMarketObservation::from_event_and_market(event, market));
            }
        }
    }
    
    info!("✅ [PROCESSING] Created {} observation records", observations.len());
    info!("");

    // Save RAW data to JSON
    let raw_output_path = "data/kalshi/raw/kalshi_raw_markets.json";
    info!("💾 [SAVE-RAW] Writing raw data to {}...", raw_output_path);
    
    let json = serde_json::to_string_pretty(&observations)?;
    std::fs::write(raw_output_path, json)?;
    
    info!("✅ [SAVE-RAW] Successfully saved {} markets to {}", observations.len(), raw_output_path);
    info!("");

    // Show sample of what we captured
    info!("📋 [SAMPLE] First 5 markets captured:");
    for (i, obs) in observations.iter().take(5).enumerate() {
        info!("   {}. {} | {}", i + 1, obs.market_ticker, obs.title);
        if let Some(rules) = &obs.rules {
            info!("      Rules: {}", rules);
        }
    }
    info!("");

    // === STRUCTURED OBSERVATION (NO AI) ===
    info!("🔄 [STRUCTURE] Converting to AI-ready format...");
    info!("   This is mechanical transformation - NO interpretation");
    info!("");

    let mut structured_markets: Vec<MarketForAI> = Vec::new();
    
    for raw in &observations {
        structured_markets.push(MarketForAI::from_raw_kalshi(raw));
    }

    info!("✅ [STRUCTURE] Created {} structured records", structured_markets.len());
    info!("");

    // Save STRUCTURED data as JSONL (one line per market for easy streaming)
    let structured_output_path = "data/kalshi/structured/kalshi_markets_structured.jsonl";
    info!("💾 [SAVE-STRUCTURED] Writing structured data to {}...", structured_output_path);
    
    let mut jsonl_lines = Vec::new();
    for market in &structured_markets {
        jsonl_lines.push(serde_json::to_string(&market)?);
    }
    std::fs::write(structured_output_path, jsonl_lines.join("\n"))?;
    
    info!("✅ [SAVE-STRUCTURED] Successfully saved {} markets to {}", structured_markets.len(), structured_output_path);
    info!("");

    // Show sample of structured format
    info!("📋 [SAMPLE-STRUCTURED] First 3 structured records:");
    for (i, market) in structured_markets.iter().take(3).enumerate() {
        info!("   {}. [{}] {}", i + 1, market.market_ticker, market.event_text);
        info!("      Series: {} | Status: {}", market.series_ticker, market.status);
        if let Some(rules) = &market.rules_text {
            info!("      Rules: {}", rules);
        }
        info!("      Time: {} → {}", 
            market.time_window.open.as_deref().unwrap_or("N/A"),
            market.time_window.close.as_deref().unwrap_or("N/A")
        );
    }
    info!("");

    // ═══════════════════════════════════════════════════════════════════════
    // POLYMARKET OBSERVATION
    // ═══════════════════════════════════════════════════════════════════════

    info!("════════════════════════════════════════════════════════════════════════");
    info!("");
    info!("🔵 [POLYMARKET] Starting observation...");
    info!("");

    // Create Polymarket client (no auth needed for Gamma API)
    let poly_client = PolymarketApiClient::new();
    
    // Fetch ALL markets
    let poly_markets = poly_client.discover_all_markets().await?;
    
    info!("📊 [POLYMARKET] Raw statistics:");
    info!("   Markets discovered: {}", poly_markets.len());
    info!("");

    // Save RAW Polymarket data
    let poly_raw_path = "data/polymarket/raw/polymarket_raw_markets.json";
    info!("💾 [SAVE-RAW] Writing raw Polymarket data to {}...", poly_raw_path);
    
    let poly_json = serde_json::to_string_pretty(&poly_markets)?;
    std::fs::write(poly_raw_path, poly_json)?;
    
    info!("✅ [SAVE-RAW] Successfully saved {} markets to {}", poly_markets.len(), poly_raw_path);
    info!("");

    // Show sample
    info!("📋 [SAMPLE] First 5 Polymarket markets:");
    for (i, market) in poly_markets.iter().take(5).enumerate() {
        info!("   {}. {} | {}", i + 1, market.id, market.question);
        if let Some(desc) = &market.description {
            info!("      Desc: {}", desc);
        }
    }
    info!("");

    // === STRUCTURED OBSERVATION (Polymarket) ===
    info!("🔄 [STRUCTURE] Converting Polymarket to AI-ready format...");
    info!("   This is mechanical transformation - NO interpretation");
    info!("");

    let mut poly_structured: Vec<MarketForAI> = Vec::new();
    
    for raw in &poly_markets {
        poly_structured.push(MarketForAI::from_raw_polymarket(raw));
    }

    info!("✅ [STRUCTURE] Created {} structured Polymarket records", poly_structured.len());
    info!("");

    // Save STRUCTURED Polymarket data as JSONL
    let poly_structured_path = "data/polymarket/structured/polymarket_markets_structured.jsonl";
    info!("💾 [SAVE-STRUCTURED] Writing structured Polymarket data to {}...", poly_structured_path);
    
    let mut poly_jsonl_lines = Vec::new();
    for market in &poly_structured {
        poly_jsonl_lines.push(serde_json::to_string(&market)?);
    }
    std::fs::write(poly_structured_path, poly_jsonl_lines.join("\n"))?;
    
    info!("✅ [SAVE-STRUCTURED] Successfully saved {} markets to {}", poly_structured.len(), poly_structured_path);
    info!("");

    // Show sample of structured format
    info!("📋 [SAMPLE-STRUCTURED] First 3 structured Polymarket records:");
    for (i, market) in poly_structured.iter().take(3).enumerate() {
        info!("   {}. [{}] {}", i + 1, market.market_ticker, market.event_text);
        info!("      Series: {} | Status: {}", market.series_ticker, market.status);
        if let Some(rules) = &market.rules_text {
            let rules_preview = if rules.len() > 100 {
                format!("{}...", &rules[..100])
            } else {
                rules.clone()
            };
            info!("      Rules: {}", rules_preview);
        }
        info!("      Time: {} → {}", 
            market.time_window.open.as_deref().unwrap_or("N/A"),
            market.time_window.close.as_deref().unwrap_or("N/A")
        );
    }
    info!("");

    // ═══════════════════════════════════════════════════════════════════════
    // GROUPING SCAFFOLD (NO AI, NO LOGIC)
    // ═══════════════════════════════════════════════════════════════════════

    info!("════════════════════════════════════════════════════════════════════════");
    info!("");
    info!("📦 [GROUPING] Creating empty grouping scaffold...");
    info!("   This branch does NOT perform grouping.");
    info!("   AI-driven grouping logic comes in next branch.");
    info!("");

    // Create empty groups array (NO auto-grouping, NO logic)
    // MarketGroup is an AI output container.
    // This branch does not populate or modify groups.
    // Grouping logic is intentionally absent.
    let groups: Vec<types::MarketGroup> = Vec::new();

    // Write empty groups to file
    let groups_path = "data/groups/market_groups.json";
    let groups_json = serde_json::to_string_pretty(&groups)?;
    std::fs::write(groups_path, groups_json)?;

    info!("✅ [GROUPING] Empty scaffold created: {}", groups_path);
    info!("   Groups: {} (intentionally empty)", groups.len());
    info!("   Ready for future AI-driven grouping logic");
    info!("");

    info!("╔═══════════════════════════════════════════════════════════════════════╗");
    info!("║                                                                       ║");
    info!("║                  ✅ OBSERVATION + STRUCTURING COMPLETE                ║");
    info!("║                                                                       ║");
    info!("║   Pipeline executed:                                                 ║");
    info!("║   1. ✅ Fetched all Kalshi markets (RAW)                            ║");
    info!("║   2. ✅ Fetched all Polymarket markets (RAW)                        ║");
    info!("║   3. ✅ Mechanically structured for AI ingestion                    ║");
    info!("║                                                                       ║");
    info!("║   Outputs:                                                           ║");
    info!("║   Kalshi:                                                            ║");
    info!("║   • data/kalshi/raw/kalshi_raw_markets.json                         ║");
    info!("║   • data/kalshi/structured/kalshi_markets_structured.jsonl          ║");
    info!("║                                                                       ║");
    info!("║   Polymarket:                                                        ║");
    info!("║   • data/polymarket/raw/polymarket_raw_markets.json                 ║");
    info!("║   • data/polymarket/structured/polymarket_markets_structured.jsonl  ║");
    info!("║                                                                       ║");
    info!("║   Total markets: {} Kalshi + {} Polymarket                      ║", 
          structured_markets.len(), poly_structured.len());
    info!("║                                                                       ║");
    info!("║   Next branch: AI-driven cross-platform market grouping             ║");
    info!("║                                                                       ║");
    info!("╚═══════════════════════════════════════════════════════════════════════╝");

    Ok(())
}
