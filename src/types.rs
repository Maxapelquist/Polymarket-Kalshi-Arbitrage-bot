//! Core type definitions for RAW market observation
//!
//! This branch observes market reality.
//! It does not attempt to understand it.
//!
//! These types are 1:1 mappings of API responses (Kalshi + Polymarket).
//! NO interpretation, NO filtering, NO matching logic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use uuid::Uuid;

// === Market Fingerprint (Deterministic Identity) ===

/// Generate a deterministic SHA-256 fingerprint for a market
/// 
/// This is pure mechanical hashing - NO interpretation.
/// Same input → same fingerprint.
/// 
/// Input format (exact):
/// ```
/// SOURCE=<source>
/// EVENT=<event_text>
/// RULES=<rules_text>
/// OPEN=<open_time|null>
/// CLOSE=<close_time|null>
/// ```
fn generate_market_fingerprint(
    source: &str,
    event_text: &str,
    rules_text: Option<&str>,
    open_time: Option<&str>,
    close_time: Option<&str>,
) -> String {
    // Build canonical fingerprint input (exact format, no normalization)
    let fingerprint_input = format!(
        "SOURCE={}\nEVENT={}\nRULES={}\nOPEN={}\nCLOSE={}",
        source,
        event_text,
        rules_text.unwrap_or("null"),
        open_time.unwrap_or("null"),
        close_time.unwrap_or("null")
    );

    // SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(fingerprint_input.as_bytes());
    format!("{:x}", hasher.finalize())
}

// === Kalshi API Response Types ===

#[derive(Debug, Deserialize)]
pub struct KalshiEventsResponse {
    pub events: Vec<KalshiEventFull>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Full Kalshi event with optional nested markets (when with_nested_markets=true)
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct KalshiEventFull {
    pub event_ticker: String,
    pub series_ticker: String,
    pub title: String,
    #[serde(default)]
    pub sub_title: Option<String>,
    #[serde(default)]
    pub mutually_exclusive: Option<bool>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub strike_date: Option<String>,
    #[serde(default)]
    pub open_time: Option<String>,
    #[serde(default)]
    pub close_time: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub markets: Option<Vec<KalshiMarket>>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct KalshiMarket {
    pub ticker: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub yes_sub_title: Option<String>,
    #[serde(default)]
    pub yes_ask: Option<i64>,
    #[serde(default)]
    pub yes_bid: Option<i64>,
    #[serde(default)]
    pub no_ask: Option<i64>,
    #[serde(default)]
    pub no_bid: Option<i64>,
    #[serde(default)]
    pub floor_strike: Option<f64>,
    #[serde(default)]
    pub volume: Option<i64>,
    #[serde(default)]
    pub liquidity: Option<i64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub open_time: Option<String>,
    #[serde(default)]
    pub close_time: Option<String>,
}

// === Raw Market Observation Record ===

/// A raw market observation ready for AI analysis
#[derive(Debug, Clone, Serialize)]
pub struct RawMarketObservation {
    /// Event identifier
    pub event_ticker: String,
    /// Series identifier
    pub series_ticker: String,
    /// Market identifier
    pub market_ticker: String,
    /// Full market title
    pub title: String,
    /// Subtitle (if any)
    pub subtitle: Option<String>,
    /// Rules text (from various fields)
    pub rules: Option<String>,
    /// Open time
    pub open_time: Option<String>,
    /// Close time
    pub close_time: Option<String>,
    /// Status
    pub status: Option<String>,
    /// Complete raw JSON for future reference
    pub raw_json: serde_json::Value,
}

impl RawMarketObservation {
    pub fn from_event_and_market(event: &KalshiEventFull, market: &KalshiMarket) -> Self {
        // Combine all text fields that might contain rules/context
        let rules = vec![
            event.sub_title.as_ref(),
            market.subtitle.as_ref(),
            market.yes_sub_title.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
        
        let rules = if rules.is_empty() { None } else { Some(rules) };

        Self {
            event_ticker: event.event_ticker.clone(),
            series_ticker: event.series_ticker.clone(),
            market_ticker: market.ticker.clone(),
            title: market.title.clone(),
            subtitle: market.subtitle.clone(),
            rules,
            open_time: market.open_time.clone().or_else(|| event.open_time.clone()),
            close_time: market.close_time.clone().or_else(|| event.close_time.clone()),
            status: market.status.clone().or_else(|| event.status.clone()),
            raw_json: serde_json::json!({
                "event": event,
                "market": market,
            }),
        }
    }
}

// === Structured Market (Normalized for AI) ===

/// Time window for a market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Open time (ISO 8601)
    pub open: Option<String>,
    /// Close time (ISO 8601)
    pub close: Option<String>,
}

/// Market structured for AI analysis
/// 
/// This format is mechanically derived from RawMarketObservation.
/// NO interpretation, just clean normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketForAI {
    /// Source platform
    pub source: String,
    /// Unique market identifier
    pub market_ticker: String,
    /// Event identifier
    pub event_ticker: String,
    /// Series identifier
    pub series_ticker: String,
    /// Complete event text (what is being predicted)
    pub event_text: String,
    /// Rules and clarifications
    pub rules_text: Option<String>,
    /// LLM-optimized text (mechanically concatenated, structured for reading)
    pub llm_text: String,
    /// Market fingerprint (deterministic SHA-256 hash for identity tracking)
    /// 
    /// Market fingerprints are deterministic identifiers.
    /// They do not imply semantic equivalence.
    /// They enable traceability and reproducibility.
    pub market_fingerprint: String,
    /// Market category (from series or event)
    pub category: Option<String>,
    /// Time window for the market
    pub time_window: TimeWindow,
    /// Market status
    pub status: String,
    /// Reference to complete raw data
    pub raw_json: serde_json::Value,
}

impl MarketForAI {
    /// Mechanically convert from RawMarketObservation (Kalshi) to AI-ready format
    /// 
    /// This is pure transformation - NO interpretation
    pub fn from_raw_kalshi(raw: &RawMarketObservation) -> Self {
        let status = raw.status.clone().unwrap_or_else(|| "unknown".to_string());
        
        // Build LLM-optimized text (strict mechanical concatenation)
        let llm_text = format!(
            "SOURCE: Kalshi\n\
             EVENT:\n{}\n\n\
             RULES:\n{}\n\n\
             TIME WINDOW:\n\
             Open: {}\n\
             Close: {}\n\n\
             STATUS:\n{}",
            raw.title,
            raw.rules.as_deref().unwrap_or("null"),
            raw.open_time.as_deref().unwrap_or("null"),
            raw.close_time.as_deref().unwrap_or("null"),
            status
        );

        // Generate deterministic market fingerprint
        let market_fingerprint = generate_market_fingerprint(
            "Kalshi",
            &raw.title,
            raw.rules.as_deref(),
            raw.open_time.as_deref(),
            raw.close_time.as_deref(),
        );

        Self {
            source: "kalshi".to_string(),
            market_ticker: raw.market_ticker.clone(),
            event_ticker: raw.event_ticker.clone(),
            series_ticker: raw.series_ticker.clone(),
            event_text: raw.title.clone(),
            rules_text: raw.rules.clone(),
            llm_text,
            market_fingerprint,
            category: None, // Will be extracted from series_ticker pattern if needed
            time_window: TimeWindow {
                open: raw.open_time.clone(),
                close: raw.close_time.clone(),
            },
            status,
            raw_json: raw.raw_json.clone(),
        }
    }

    /// Mechanically convert from PolymarketMarketRaw to AI-ready format
    /// 
    /// This is pure transformation - NO interpretation
    pub fn from_raw_polymarket(raw: &PolymarketMarketRaw) -> Self {
        // Combine question and description for event_text
        let event_text = raw.question.clone();
        
        // Use description as rules_text if available
        let rules_text = raw.description.clone();

        // Extract time window from various date fields
        let time_window = TimeWindow {
            open: raw.start_date_iso.clone(),
            close: raw.end_date_iso.clone(),
        };

        // Determine status
        let status = if raw.closed.unwrap_or(false) {
            "closed".to_string()
        } else if raw.active.unwrap_or(false) {
            "open".to_string()
        } else {
            "unknown".to_string()
        };

        // Build LLM-optimized text (strict mechanical concatenation, same structure as Kalshi)
        let llm_text = format!(
            "SOURCE: Polymarket\n\
             EVENT:\n{}\n\n\
             RULES:\n{}\n\n\
             TIME WINDOW:\n\
             Open: {}\n\
             Close: {}\n\n\
             STATUS:\n{}",
            event_text,
            rules_text.as_deref().unwrap_or("null"),
            time_window.open.as_deref().unwrap_or("null"),
            time_window.close.as_deref().unwrap_or("null"),
            status
        );

        // Generate deterministic market fingerprint (same algorithm as Kalshi)
        let market_fingerprint = generate_market_fingerprint(
            "Polymarket",
            &event_text,
            rules_text.as_deref(),
            time_window.open.as_deref(),
            time_window.close.as_deref(),
        );

        Self {
            source: "polymarket".to_string(),
            market_ticker: raw.condition_id.clone().unwrap_or_else(|| raw.id.clone()),
            event_ticker: raw.id.clone(),
            series_ticker: raw.market_slug.clone().unwrap_or_default(),
            event_text,
            rules_text,
            llm_text,
            market_fingerprint,
            category: None,
            time_window,
            status,
            raw_json: serde_json::to_value(raw).unwrap_or(serde_json::Value::Null),
        }
    }
}

// === Polymarket API Types ===

/// Raw Polymarket market from Gamma API
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct PolymarketMarketRaw {
    /// Unique market ID
    pub id: String,
    
    /// Market question/title
    pub question: String,
    
    /// Description and rules
    #[serde(default)]
    pub description: Option<String>,
    
    /// Market slug (URL-friendly identifier)
    #[serde(rename = "market_slug")]
    #[serde(default)]
    pub market_slug: Option<String>,
    
    /// Condition ID (from CTF contract)
    #[serde(rename = "conditionId")]
    #[serde(default)]
    pub condition_id: Option<String>,
    
    /// Market is active
    #[serde(default)]
    pub active: Option<bool>,
    
    /// Market is closed
    #[serde(default)]
    pub closed: Option<bool>,
    
    /// Start date (ISO 8601)
    #[serde(rename = "startDate")]
    #[serde(default)]
    pub start_date_iso: Option<String>,
    
    /// End date (ISO 8601)
    #[serde(rename = "endDate")]
    #[serde(default)]
    pub end_date_iso: Option<String>,
    
    /// Outcomes (can be array or JSON string in API)
    #[serde(default)]
    pub outcomes: Option<serde_json::Value>,
    
    /// Outcome prices (can be array or JSON string in API)
    #[serde(rename = "outcomePrices")]
    #[serde(default)]
    pub outcome_prices: Option<serde_json::Value>,
    
    /// CLOB token IDs (can be array or JSON string in API)
    #[serde(rename = "clobTokenIds")]
    #[serde(default)]
    pub clob_token_ids: Option<serde_json::Value>,
    
    /// Volume (can be string or number in API)
    #[serde(default)]
    pub volume: Option<serde_json::Value>,
    
    /// Liquidity (can be string or number in API)
    #[serde(default)]
    pub liquidity: Option<serde_json::Value>,
    
    /// Tags/categories (can be array or JSON string in API)
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
}

// === Market Grouping (AI Output Container) ===

/// MarketGroup is an AI output container.
/// This branch does not populate or modify groups.
/// Grouping logic is intentionally absent.
/// 
/// Future AI-driven grouping will:
/// - Read MarketForAI records
/// - Analyze llm_text for semantic similarity
/// - Create groups with probabilistic confidence
/// - Write group_reason explaining the match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketGroup {
    /// Unique group identifier
    pub group_id: Uuid,
    
    /// Deterministic identifiers to markets in this group
    pub market_fingerprints: Vec<String>,
    
    /// Which sources this group contains (e.g., ["kalshi", "polymarket"])
    pub sources: Vec<String>,
    
    /// When this group was created
    pub created_at: DateTime<Utc>,
    
    /// AI explanation for why these markets are grouped (NULL in observation branch)
    /// 
    /// This field will be populated by future AI-driven grouping logic.
    /// In the observation branch, it remains None.
    pub group_reason: Option<String>,
}
