//! Knowledge Graph Building System för Kalshi ↔ Polymarket
//!
//! PHASE 1: Full data ingestion
//! PHASE 2: Semantic categorization (Polymarket → Kalshi taxonomy)
//! PHASE 3: Event-level semantic matching (embedding + LLM verification)
//! PHASE 4: Contract-level matching (within matched events)
//! PHASE 5: Runtime scanning (no AI, only odds/liquidity)

mod ai;
mod matching;

use std::fs;
use std::collections::HashMap;
use anyhow::Result;

const POLYMARKET_API_BASE: &str = "https://gamma-api.polymarket.com";

// Kalshi-kategorier (source of truth) - läs från config/kalshi_categories.json
fn load_kalshi_categories() -> Result<Vec<String>> {
    let content = fs::read_to_string("config/kalshi_categories.json")?;
    let categories: Vec<String> = serde_json::from_str(&content)?;
    Ok(categories)
}

fn get_kalshi_categories() -> Vec<String> {
    // Läs från config-fil, fallback till hardcoded lista
    load_kalshi_categories().unwrap_or_else(|_| {
        vec![
            "Politics".to_string(),
            "Elections".to_string(),
            "Economics".to_string(),
            "World".to_string(),
            "Business".to_string(),
            "Science and Technology".to_string(),
            "Climate and Weather".to_string(),
            "Health".to_string(),
            "Entertainment".to_string(),
            "Sports".to_string(),
            "Culture".to_string(),
            "Energy".to_string(),
            "Finance".to_string(),
            "Crypto".to_string(),
            "Law and Justice".to_string(),
        ]
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match mode {
        // PHASE 1: FULL DATA INGESTION
        "ingest_kalshi" => {
            println!("🔍 PHASE 1: Hämtar ALLA Kalshi events...\n");
            fs::create_dir_all("data/kalshi/raw")?;
            ingest_kalshi_events().await?;
        }
        "ingest_polymarket" => {
            println!("🔍 PHASE 1: Hämtar ALLA Polymarket markets...\n");
            fs::create_dir_all("data/polymarket/raw")?;
            ingest_polymarket_markets().await?;
        }
        "build_polymarket_events" => {
            println!("🔍 PHASE 1: Bygger Polymarket event-candidates...\n");
            fs::create_dir_all("data/polymarket/derived")?;
            build_polymarket_events().await?;
        }
        
        // PHASE 2: SEMANTIC CATEGORIZATION
        "categorize_polymarket" => {
            println!("🔍 PHASE 2: Semantic categorization (Polymarket → Kalshi)...\n");
            fs::create_dir_all("data/polymarket/derived")?;
            categorize_polymarket_events().await?;
        }
        
        // PHASE 3: EVENT-LEVEL SEMANTIC MATCHING
        "match_events" => {
            println!("🔍 PHASE 3: Event-level semantic matching...\n");
            fs::create_dir_all("data/matching")?;
            match_events_semantic().await?;
        }
        "match_events_embeddings" => {
            println!("🔍 PHASE 3: Event-level matching (embeddings only)...\n");
            fs::create_dir_all("data/matching")?;
            match_events_embeddings_only().await?;
        }
        
        // PHASE 4: CONTRACT-LEVEL MATCHING (placeholder)
        "match_contracts" => {
            println!("🔍 PHASE 4: Contract-level matching...\n");
            println!("⚠️  Not yet implemented");
        }
        
        // VERIFICATION
        "count_events" => {
            count_events()?;
        }
        "show_samples" => {
            show_samples().await?;
        }
        "show_categories" => {
            show_categories()?;
        }
        
        _ => {
            println!("Knowledge Graph Building System för Kalshi ↔ Polymarket\n");
            println!("PHASE 1 - Full Data Ingestion:");
            println!("  cargo run ingest_kalshi          # Hämta ALLA Kalshi events");
            println!("  cargo run ingest_polymarket      # Hämta ALLA Polymarket markets");
            println!("  cargo run build_polymarket_events # Bygg Polymarket event-candidates");
            println!();
            println!("PHASE 2 - Semantic Categorization:");
            println!("  cargo run categorize_polymarket   # Kategorisera med lokal AI");
            println!();
            println!("PHASE 3 - Event-Level Matching:");
            println!("  cargo run match_events_embeddings  # Embeddings only (top-10 candidates)");
            println!("  cargo run match_events             # Embedding + LLM verification (pending)");
            println!();
            println!("PHASE 4 - Contract-Level Matching:");
            println!("  cargo run match_contracts         # (Not yet implemented)");
            println!();
            println!("Verification:");
            println!("  cargo run count_events            # Räkna events");
            println!("  cargo run show_samples            # Visa exempel");
            println!("  cargo run show_categories          # Visa kategorier");
        }
    }

    Ok(())
}

// ============================================================================
// PHASE 1: FULL DATA INGESTION
// ============================================================================

async fn ingest_kalshi_events() -> Result<()> {
    // Läser från befintlig data (ingen API-hämtning av markets)
    let possible_paths = vec![
        "data/kalshi/derived/kalshi_events_minimal.json",
        "data/kalshi/raw/kalshi_events.json",
    ];
    
    let mut events_data: Vec<serde_json::Value> = Vec::new();
    
    for path in &possible_paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                events_data = parsed;
                println!("   📂 Läser från: {}", path);
                break;
            }
        }
    }
    
    if events_data.is_empty() {
        return Err(anyhow::anyhow!("Ingen Kalshi events-fil hittades"));
    }
    
    println!("   📊 Found {} events", events_data.len());
    
    // Normalisera struktur
    let mut events = Vec::new();
    for event_data in events_data {
        let event = serde_json::json!({
            "event_id": event_data.get("event_ticker").and_then(|e| e.as_str()).unwrap_or(""),
            "title": event_data.get("title").and_then(|t| t.as_str()).unwrap_or(""),
            "subtitle": event_data.get("sub_title").and_then(|s| s.as_str()).unwrap_or(""),
            "category": event_data.get("category").and_then(|c| c.as_str()).unwrap_or("")
        });
        events.push(event);
    }
    
    let output_path = "data/kalshi/derived/events.json";
    fs::write(output_path, serde_json::to_string_pretty(&events)?)?;
    println!("   ✅ Saved: {} ({} events)", output_path, events.len());
    
    Ok(())
}

async fn ingest_polymarket_markets() -> Result<()> {
    let client = reqwest::Client::new();
    let mut all_markets = Vec::new();
    let mut offset = 0;
    let limit = 1000;
    let mut request_count = 0usize;
    
    loop {
        let url = format!(
            "{}/markets?closed=false&limit={}&offset={}",
            POLYMARKET_API_BASE, limit, offset
        );
        
        println!("   GET {} (request #{})", url, request_count + 1);
        request_count += 1;
        
        let response = client.get(&url).send().await?;
        let status = response.status();
        let text = response.text().await?;
        
        if !status.is_success() {
            eprintln!("   ⚠️  Status: {}", status);
            break;
        }
        
        let json: serde_json::Value = serde_json::from_str(&text)?;
        let markets_opt = json.as_array()
            .or_else(|| json.get("data").and_then(|d| d.as_array()))
            .or_else(|| json.get("markets").and_then(|m| m.as_array()));
        
        let markets = match markets_opt {
            Some(arr) => arr,
            None => break,
        };
        
        if markets.is_empty() {
            break;
        }
        
        let mut page_markets = 0usize;
        for market in markets {
            let active = market.get("active").and_then(|a| a.as_bool()).unwrap_or(false);
            let closed = market.get("closed").and_then(|c| c.as_bool()).unwrap_or(true);
            
            if active && !closed {
                all_markets.push(market.clone());
                page_markets += 1;
            }
        }
        
        println!("      Fetched {} active markets (total: {})", page_markets, all_markets.len());
        
        if markets.len() < limit {
            break;
        }
        
        offset += limit;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    let markets_path = "data/polymarket/raw/markets_full.json";
    fs::write(&markets_path, serde_json::to_string_pretty(&all_markets)?)?;
    println!("\n   ✅ Saved: {} ({} markets, {} requests)", markets_path, all_markets.len(), request_count);
    
    Ok(())
}

async fn build_polymarket_events() -> Result<()> {
    let raw_content = fs::read_to_string("data/polymarket/raw/markets_full.json")?;
    let markets: Vec<serde_json::Value> = serde_json::from_str(&raw_content)?;
    
    println!("   📊 Found {} markets", markets.len());
    
    let mut events_map: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    
    for market in &markets {
        let synthetic_event_id = market
            .get("events")
            .and_then(|e| e.as_array())
            .and_then(|arr| arr.get(0))
            .and_then(|e| e.get("seriesSlug"))
            .and_then(|s| s.as_str())
            .map(|s| format!("pm_{}", s))
            .or_else(|| {
                market.get("events")
                    .and_then(|e| e.as_array())
                    .and_then(|arr| arr.get(0))
                    .and_then(|e| e.get("title"))
                    .and_then(|t| t.as_str())
                    .map(|t| format!("pm_{}", normalize_question(t)))
            });
        
        if let Some(event_id) = synthetic_event_id {
            events_map.entry(event_id).or_insert_with(Vec::new).push(market);
        }
    }
    
    let mut events = Vec::new();
    for (event_id, markets) in events_map {
        let first_market = markets[0];
        let title = first_market.get("question")
            .and_then(|q| q.as_str())
            .or_else(|| {
                first_market.get("events")
                    .and_then(|e| e.as_array())
                    .and_then(|arr| arr.get(0))
                    .and_then(|e| e.get("title"))
                    .and_then(|t| t.as_str())
            })
            .unwrap_or("")
            .to_string();
        
        events.push(serde_json::json!({
            "event_id": event_id,
            "title": title,
            "markets_count": markets.len(),
            "startDate": first_market.get("startDate"),
            "endDate": first_market.get("endDate")
        }));
    }
    
    let output_path = "data/polymarket/derived/events.json";
    fs::write(output_path, serde_json::to_string_pretty(&events)?)?;
    println!("   ✅ Saved: {} ({} events)", output_path, events.len());
    
    Ok(())
}

fn normalize_question(question: &str) -> String {
    question
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

// ============================================================================
// PHASE 2: SEMANTIC CATEGORIZATION
// ============================================================================

async fn categorize_polymarket_events() -> Result<()> {
    // TODO: Implementera med lokal AI (embedding + LLM)
    // För nu: placeholder med keyword-matching
    println!("   ⚠️  Using keyword-based categorization (AI implementation pending)");
    
    let events_content = fs::read_to_string("data/polymarket/derived/events.json")?;
    let events: Vec<serde_json::Value> = serde_json::from_str(&events_content)?;
    
    println!("   📊 Found {} Polymarket events", events.len());
    
    // Placeholder: keyword-matching (skulle vara AI här)
    let category_keywords = get_category_keywords();
    let mut categorized = Vec::new();
    
    for event in events {
        let event_id = event.get("event_id").and_then(|id| id.as_str()).unwrap_or("");
        let title = event.get("title").and_then(|t| t.as_str()).unwrap_or("").to_lowercase();
        
        let (assigned_category, confidence) = categorize_with_keywords(&title, &category_keywords);
        
        categorized.push(serde_json::json!({
            "event_id": event_id,
            "assigned_category": assigned_category,
            "confidence": confidence
        }));
    }
    
    let output_path = "data/polymarket/derived/events_categorized.json";
    fs::write(output_path, serde_json::to_string_pretty(&categorized)?)?;
    println!("   ✅ Saved: {} ({} events)", output_path, categorized.len());
    
    Ok(())
}

fn get_category_keywords() -> HashMap<&'static str, Vec<&'static str>> {
    let mut map = HashMap::new();
    map.insert("Politics", vec!["president", "congress", "senate", "trump", "biden", "political"]);
    map.insert("Elections", vec!["election", "vote", "voting", "ballot", "electoral"]);
    map.insert("Economics", vec!["economy", "economic", "gdp", "inflation", "tariff", "trade"]);
    map.insert("World", vec!["international", "global", "country", "nation", "war"]);
    map.insert("Business", vec!["company", "corporate", "business", "merger", "ipo"]);
    map.insert("Science and Technology", vec!["science", "technology", "tech", "ai", "space"]);
    map.insert("Climate and Weather", vec!["climate", "weather", "temperature", "warming"]);
    map.insert("Health", vec!["health", "medical", "disease", "pandemic", "vaccine"]);
    map.insert("Entertainment", vec!["movie", "film", "tv", "celebrity", "award"]);
    map.insert("Sports", vec!["sport", "football", "basketball", "olympics", "championship"]);
    map.insert("Culture", vec!["culture", "art", "music", "literature"]);
    map.insert("Energy", vec!["energy", "oil", "gas", "renewable", "solar"]);
    map.insert("Finance", vec!["finance", "bank", "currency", "interest rate"]);
    map.insert("Crypto", vec!["crypto", "bitcoin", "ethereum", "blockchain"]);
    map.insert("Law and Justice", vec!["law", "court", "judge", "trial", "lawsuit"]);
    map
}

fn categorize_with_keywords(title: &str, keywords: &HashMap<&str, Vec<&str>>) -> (String, f32) {
    let mut best_category = "Unknown".to_string();
    let mut best_score = 0.0;
    
    for (category, category_keywords) in keywords {
        let mut score = 0.0;
        for keyword in category_keywords {
            if title.contains(keyword) {
                score += 1.0;
            }
        }
        if score > best_score {
            best_score = score;
            best_category = category.to_string();
        }
    }
    
    let confidence = if best_score > 0.0 {
        (best_score / 10.0_f64).min(1.0_f64).max(0.5_f64)
    } else {
        0.0
    };
    
    (best_category, confidence as f32)
}

// ============================================================================
// PHASE 3: EVENT-LEVEL SEMANTIC MATCHING
// ============================================================================

async fn match_events_semantic() -> Result<()> {
    println!("   ⚠️  Semantic matching with embeddings + LLM (implementation pending)");
    println!("   📝 This will use:");
    println!("      - bge-m3 for embeddings (high recall)");
    println!("      - Qwen2.5-7B-Instruct for verification (high precision)");
    
    // TODO: Implementera med EventMatcher
    // let embedding_engine = Box::new(ai::embedding::PythonEmbeddingEngine::default());
    // let llm_verifier = Box::new(ai::verifier::PythonLLMVerifier::default());
    // let mut matcher = matching::event_matcher::EventMatcher::new(embedding_engine, llm_verifier, 10);
    
    // Läs data
    let kalshi_content = fs::read_to_string("data/kalshi/derived/events.json")?;
    let poly_categorized_content = fs::read_to_string("data/polymarket/derived/events_categorized.json")?;
    
    println!("   📊 Kalshi events: {}", serde_json::from_str::<Vec<serde_json::Value>>(&kalshi_content)?.len());
    println!("   📊 Polymarket events: {}", serde_json::from_str::<Vec<serde_json::Value>>(&poly_categorized_content)?.len());
    
    // Placeholder output
    let matches: Vec<serde_json::Value> = Vec::new();
    let output_path = "data/matching/event_matches.json";
    fs::write(output_path, serde_json::to_string_pretty(&matches)?)?;
    println!("   ✅ Saved: {} (0 matches - AI not yet implemented)", output_path);
    
    Ok(())
}

async fn match_events_embeddings_only() -> Result<()> {
    use crate::ai::embedding::PythonEmbeddingEngine;
    use crate::matching::event_matcher::{EmbeddingOnlyMatcher, KalshiEvent, PolymarketEvent};
    
    println!("   📝 Using bge-m3 embeddings (no LLM verification)");
    println!("   📝 Finding top-10 candidates per Kalshi event");
    
    // Läs Kalshi events
    let kalshi_content = fs::read_to_string("data/kalshi/derived/events.json")?;
    let kalshi_events_json: Vec<serde_json::Value> = serde_json::from_str(&kalshi_content)?;
    
    let mut kalshi_events = Vec::new();
    for event_json in kalshi_events_json {
        kalshi_events.push(KalshiEvent {
            event_id: event_json.get("event_id").and_then(|e| e.as_str()).unwrap_or("").to_string(),
            title: event_json.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            subtitle: event_json.get("subtitle").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            category: event_json.get("category").and_then(|c| c.as_str()).unwrap_or("").to_string(),
        });
    }
    
    // Läs Polymarket categorized events
    let poly_categorized_content = fs::read_to_string("data/polymarket/derived/events_categorized.json")?;
    let poly_categorized: Vec<serde_json::Value> = serde_json::from_str(&poly_categorized_content)?;
    
    let poly_events_content = fs::read_to_string("data/polymarket/derived/events.json")?;
    let poly_events_json: Vec<serde_json::Value> = serde_json::from_str(&poly_events_content)?;
    let poly_events_map: HashMap<String, serde_json::Value> = poly_events_json
        .into_iter()
        .map(|e| {
            let id = e.get("event_id").and_then(|id| id.as_str()).unwrap_or("").to_string();
            (id, e)
        })
        .collect();
    
    let mut polymarket_events = Vec::new();
    for cat_event in poly_categorized {
        let event_id = cat_event.get("event_id").and_then(|id| id.as_str()).unwrap_or("");
        let assigned_category = cat_event.get("assigned_category").and_then(|c| c.as_str()).unwrap_or("Unknown");
        
        if assigned_category == "Unknown" {
            continue;
        }
        
        if let Some(event_data) = poly_events_map.get(event_id) {
            let title = event_data.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
            polymarket_events.push(PolymarketEvent {
                event_id: event_id.to_string(),
                title,
                assigned_category: assigned_category.to_string(),
            });
        }
    }
    
    println!("   📊 Kalshi events: {}", kalshi_events.len());
    println!("   📊 Polymarket events (categorized): {}", polymarket_events.len());
    
    // Skapa embedding engine
    let mut embedding_engine = PythonEmbeddingEngine::default();
    embedding_engine.reset_cache_stats();
    let mut matcher = EmbeddingOnlyMatcher::new(Box::new(embedding_engine), 10);
    
    println!("\n   🔄 Computing embeddings and finding candidates...");
    println!("   ⏳ This may take a while (embeddings are cached)");
    println!("   📝 Progress will be shown below (from Python script)\n");
    
    // Hitta kandidater
    let (kalshi_event_candidates, _cache_hits, _cache_misses) = matcher.find_candidates(&kalshi_events, &polymarket_events)?;
    
    // Cache-statistik visas i Python-scriptets stderr output
    println!();
    
    // Räkna totala antalet kandidater
    let total_candidates: usize = kalshi_event_candidates.iter().map(|kec| kec.candidates.len()).sum();
    
    println!("   ✅ Found {} Kalshi events with candidates", kalshi_event_candidates.len());
    println!("   ✅ Total candidates: {}", total_candidates);
    
    // Spara kandidater (grupperat per Kalshi event)
    let output_path = "data/matching/event_candidates.json";
    let candidates_json: Vec<serde_json::Value> = kalshi_event_candidates
        .iter()
        .map(|kec| {
            let candidates_json: Vec<serde_json::Value> = kec.candidates
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "polymarket_event_id": c.polymarket_event_id,
                        "embedding_score": c.embedding_score
                    })
                })
                .collect();
            
            serde_json::json!({
                "kalshi_event_id": kec.kalshi_event_id,
                "candidates": candidates_json
            })
        })
        .collect();
    
    fs::write(output_path, serde_json::to_string_pretty(&candidates_json)?)?;
    println!("   ✅ Saved: {} ({} Kalshi events with candidates)", output_path, kalshi_event_candidates.len());
    
    // Visa statistik
    if total_candidates > 0 {
        let all_scores: Vec<f32> = kalshi_event_candidates
            .iter()
            .flat_map(|kec| kec.candidates.iter().map(|c| c.embedding_score))
            .collect();
        
        let avg_score: f32 = all_scores.iter().sum::<f32>() / all_scores.len() as f32;
        let max_score = all_scores.iter().fold(0.0f32, |a, &b| a.max(b));
        let min_score = all_scores.iter().fold(1.0f32, |a, &b| a.min(b));
        
        println!("\n   📊 Statistics:");
        println!("      Average score: {:.3}", avg_score);
        println!("      Max score: {:.3}", max_score);
        println!("      Min score: {:.3}", min_score);
    }
    
    Ok(())
}

// ============================================================================
// VERIFICATION
// ============================================================================

fn count_events() -> Result<()> {
    if let Ok(content) = fs::read_to_string("data/kalshi/derived/events.json") {
        if let Ok(events) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
            println!("Kalshi events: {}", events.len());
        }
    }
    if let Ok(content) = fs::read_to_string("data/polymarket/derived/events.json") {
        if let Ok(events) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
            println!("Polymarket events: {}", events.len());
        }
    }
    Ok(())
}

async fn show_samples() -> Result<()> {
    if let Ok(content) = fs::read_to_string("data/kalshi/derived/events.json") {
        if let Ok(events) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
            println!("=== KALSHI SAMPLE (3) ===\n");
            for (idx, event) in events.iter().take(3).enumerate() {
                println!("[{}] {}", idx + 1, serde_json::to_string_pretty(event)?);
                println!();
            }
        }
    }
    if let Ok(content) = fs::read_to_string("data/polymarket/derived/events.json") {
        if let Ok(events) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
            println!("=== POLYMARKET SAMPLE (3) ===\n");
            for (idx, event) in events.iter().take(3).enumerate() {
                println!("[{}] {}", idx + 1, serde_json::to_string_pretty(event)?);
                println!();
            }
        }
    }
    Ok(())
}

fn show_categories() -> Result<()> {
    println!("=== KALSHI KATEGORIER (Source of Truth) ===\n");
    let categories = get_kalshi_categories();
    for (idx, category) in categories.iter().enumerate() {
        println!("{}. {}", idx + 1, category);
    }
    Ok(())
}
