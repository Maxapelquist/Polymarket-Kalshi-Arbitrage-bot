//! Kalshi platform integration client - RAW OBSERVATION ONLY
//!
//! This branch observes Kalshi reality.
//! It does not attempt to understand it.
//!
//! This module provides REST API client for fetching ALL Kalshi events and markets.
//! NO execution, NO WebSocket, NO arbitrage detection.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use pkcs1::DecodeRsaPrivateKey;
use rsa::{
    pss::SigningKey,
    sha2::Sha256,
    signature::{RandomizedSigner, SignatureEncoding},
    RsaPrivateKey,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info};

use crate::config::{KALSHI_API_BASE, KALSHI_API_DELAY_MS};
use crate::types::{KalshiEventsResponse, KalshiEventFull};

// === Kalshi Auth Config ===

pub struct KalshiConfig {
    pub api_key_id: String,
    pub private_key: RsaPrivateKey,
}

impl KalshiConfig {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();
        let api_key_id = std::env::var("KALSHI_API_KEY_ID").context("KALSHI_API_KEY_ID not set")?;
        // Support both KALSHI_PRIVATE_KEY_PATH and KALSHI_PRIVATE_KEY_FILE for compatibility
        let key_path = std::env::var("KALSHI_PRIVATE_KEY_PATH")
            .or_else(|_| std::env::var("KALSHI_PRIVATE_KEY_FILE"))
            .unwrap_or_else(|_| "test.txt".to_string());
        let private_key_pem = std::fs::read_to_string(&key_path)
            .with_context(|| format!("Failed to read private key from {}", key_path))?
            .trim()
            .to_owned();
        let private_key = RsaPrivateKey::from_pkcs1_pem(&private_key_pem)
            .context("Failed to parse private key PEM")?;
        Ok(Self { api_key_id, private_key })
    }

    pub fn sign(&self, message: &str) -> Result<String> {
        tracing::debug!("[KALSHI-DEBUG] Signing message: {}", message);
        let signing_key = SigningKey::<Sha256>::new(self.private_key.clone());
        let signature = signing_key.sign_with_rng(&mut rand::thread_rng(), message.as_bytes());
        let sig_b64 = BASE64.encode(signature.to_bytes());
        tracing::debug!("[KALSHI-DEBUG] Signature (first 50 chars): {}...", &sig_b64[..50.min(sig_b64.len())]);
        Ok(sig_b64)
    }
}

// === Kalshi REST API Client ===

pub struct KalshiApiClient {
    http: reqwest::Client,
    pub config: KalshiConfig,
}

impl KalshiApiClient {
    pub fn new(config: KalshiConfig) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
            config,
        }
    }
    
    /// Generic authenticated GET request with retry on rate limit
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut retries = 0;
        const MAX_RETRIES: u32 = 5;

        loop {
            let url = format!("{}{}", KALSHI_API_BASE, path);
            let timestamp_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            // Kalshi signature uses FULL path including /trade-api/v2 prefix
            let full_path = format!("/trade-api/v2{}", path);
            let signature = self.config.sign(&format!("{}GET{}", timestamp_ms, full_path))?;
            
            let resp = self.http
                .get(&url)
                .header("KALSHI-ACCESS-KEY", &self.config.api_key_id)
                .header("KALSHI-ACCESS-SIGNATURE", &signature)
                .header("KALSHI-ACCESS-TIMESTAMP", timestamp_ms.to_string())
                .send()
                .await?;
            
            let status = resp.status();
            
            // Handle rate limit with exponential backoff
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                retries += 1;
                if retries > MAX_RETRIES {
                    anyhow::bail!("Kalshi API rate limited after {} retries", MAX_RETRIES);
                }
                let backoff_ms = 2000 * (1 << retries); // 4s, 8s, 16s, 32s, 64s
                debug!("[KALSHI] Rate limited, backing off {}ms (retry {}/{})", 
                       backoff_ms, retries, MAX_RETRIES);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                continue;
            }
            
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("Kalshi API error {}: {}", status, body);
            }
            
            let data: T = resp.json().await?;
            tokio::time::sleep(Duration::from_millis(KALSHI_API_DELAY_MS)).await;
            return Ok(data);
        }
    }
    
    /// **UNIVERSAL DISCOVERY**: Fetch ALL open events from Kalshi with cursor pagination
    /// 
    /// This is the core observation method:
    /// - Fetches ALL events with nested markets
    /// - Uses cursor pagination to get everything
    /// - Returns complete raw data structure
    /// 
    /// ⚠️ IMPORTANT: This is an expensive operation (rate limits apply)
    pub async fn discover_all_events_paginated(&self) -> Result<Vec<KalshiEventFull>> {
        info!("🔍 [KALSHI] Starting full event discovery with cursor pagination...");
        
        let mut all_events = Vec::new();
        let mut cursor: Option<String> = None;
        let mut page = 0;
        
        loop {
            page += 1;
            
            // Build query with cursor if we have one
            let path = if let Some(ref c) = cursor {
                format!("/events?status=open&limit=200&with_nested_markets=true&cursor={}", c)
            } else {
                "/events?status=open&limit=200&with_nested_markets=true".to_string()
            };
            
            debug!("[KALSHI] Fetching page {} (cursor: {:?})", page, cursor.is_some());
            
            let resp: KalshiEventsResponse = self.get(&path).await
                .context(format!("Failed to fetch events page {}", page))?;
            
            let fetched = resp.events.len();
            all_events.extend(resp.events);
            
            info!("[KALSHI] Page {}: fetched {} events (total: {})", page, fetched, all_events.len());
            
            // Check if we have more pages
            if let Some(next_cursor) = resp.cursor {
                if !next_cursor.is_empty() && fetched > 0 {
                    cursor = Some(next_cursor);
                    // Rate limit: sleep between pages
                    tokio::time::sleep(Duration::from_millis(KALSHI_API_DELAY_MS * 2)).await;
                } else {
                    break;
                }
            } else {
                break;
            }
            
            // Safety: prevent infinite loops
            if page > 100 {
                error!("[KALSHI] Stopped pagination after 100 pages (safety limit)");
                break;
            }
        }
        
        info!("✅ [KALSHI] Full discovery complete: {} events across all categories", all_events.len());
        Ok(all_events)
    }
}
