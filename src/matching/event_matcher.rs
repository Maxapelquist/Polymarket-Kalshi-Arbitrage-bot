//! Event-level semantic matching
//!
//! TWO-STAGE approach:
//! 1) Embedding similarity (high recall) - keep top-K candidates
//! 2) LLM verification (high precision) - verify if same event

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::ai::embedding::EmbeddingEngine;
use crate::ai::verifier::LLMVerifier;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMatch {
    pub kalshi_event_id: String,
    pub polymarket_event_id: String,
    pub confidence: f32,
    pub embedding_score: f32,
    pub llm_verified: bool,
    pub llm_confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCandidate {
    pub kalshi_event_id: String,
    pub polymarket_event_id: String,
    pub embedding_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketCandidate {
    pub polymarket_event_id: String,
    pub embedding_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KalshiEventCandidates {
    pub kalshi_event_id: String,
    pub candidates: Vec<PolymarketCandidate>,
}

pub struct EventMatcher {
    embedding_engine: Box<dyn EmbeddingEngine>,
    llm_verifier: Box<dyn LLMVerifier>,
    top_k: usize,
}

impl EventMatcher {
    pub fn new(
        embedding_engine: Box<dyn EmbeddingEngine>,
        llm_verifier: Box<dyn LLMVerifier>,
        top_k: usize,
    ) -> Self {
        Self {
            embedding_engine,
            llm_verifier,
            top_k,
        }
    }
    
    /// Matcha events inom samma kategori
    pub fn match_events(
        &mut self,
        kalshi_events: &[KalshiEvent],
        polymarket_events: &[PolymarketEvent],
    ) -> Result<Vec<EventMatch>> {
        let mut matches = Vec::new();
        
        // STAGE 1: Embedding similarity (high recall)
        for poly_event in polymarket_events {
            let poly_text = format!("{}", poly_event.title);
            let poly_emb = self.embedding_engine.embed(&poly_text)?;
            
            // Hitta top-K kandidater baserat på embedding similarity
            let mut candidates: Vec<(usize, f32)> = Vec::new();
            
            for (idx, kalshi_event) in kalshi_events.iter().enumerate() {
                // Endast matcha inom samma kategori
                if kalshi_event.category != poly_event.assigned_category {
                    continue;
                }
                
                let kalshi_text = format!("{} {}", kalshi_event.title, kalshi_event.subtitle);
                let kalshi_emb = self.embedding_engine.embed(&kalshi_text)?;
                
                let similarity = self.embedding_engine.similarity(&poly_emb, &kalshi_emb)?;
                candidates.push((idx, similarity));
            }
            
            // Sortera och ta top-K
            candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            candidates.truncate(self.top_k);
            
            // STAGE 2: LLM verification (high precision)
            for (kalshi_idx, embedding_score) in candidates {
                let kalshi_event = &kalshi_events[kalshi_idx];
                
                let (is_match, llm_confidence, reason) = self.llm_verifier.verify_match(
                    &kalshi_event.title,
                    &kalshi_event.subtitle,
                    &poly_event.title,
                )?;
                
                if is_match {
                    matches.push(EventMatch {
                        kalshi_event_id: kalshi_event.event_id.clone(),
                        polymarket_event_id: poly_event.event_id.clone(),
                        confidence: (embedding_score * 0.4 + llm_confidence * 0.6), // Weighted
                        embedding_score,
                        llm_verified: true,
                        llm_confidence,
                        reason,
                    });
                }
            }
        }
        
        Ok(matches)
    }
}

/// Embedding-only matcher (utan LLM verification)
pub struct EmbeddingOnlyMatcher {
    embedding_engine: Box<dyn EmbeddingEngine>,
    top_k: usize,
}

impl EmbeddingOnlyMatcher {
    pub fn new(embedding_engine: Box<dyn EmbeddingEngine>, top_k: usize) -> Self {
        Self { embedding_engine, top_k }
    }
    
    /// Hitta top-K kandidater baserat på embedding similarity
    /// Grupperat per Kalshi event
    /// Returnerar (kandidater, cache_hits, cache_misses)
    pub fn find_candidates(
        &mut self,
        kalshi_events: &[KalshiEvent],
        polymarket_events: &[PolymarketEvent],
    ) -> Result<(Vec<KalshiEventCandidates>, usize, usize)> {
        use std::collections::HashMap;
        use std::time::Instant;
        
        // STEG 1: Samla alla unika texter som behöver embed:as
        let mut text_to_id = HashMap::new();
        let mut kalshi_texts = Vec::new();
        let mut poly_texts = Vec::new();
        
        // Kalshi texts
        for (idx, kalshi_event) in kalshi_events.iter().enumerate() {
            let text = format!("{} {}", kalshi_event.title, kalshi_event.subtitle);
            let id = format!("kalshi_{}", idx);
            text_to_id.insert(text.clone(), (id.clone(), "kalshi".to_string(), idx));
            kalshi_texts.push((id, text));
        }
        
        // Polymarket texts (endast inom relevanta kategorier)
        let mut poly_text_to_indices = HashMap::new();
        for (idx, poly_event) in polymarket_events.iter().enumerate() {
            let text = poly_event.title.clone();
            let id = format!("poly_{}", idx);
            poly_text_to_indices.entry(text.clone()).or_insert_with(Vec::new).push((idx, poly_event.assigned_category.clone()));
            if !text_to_id.contains_key(&text) {
                text_to_id.insert(text.clone(), (id.clone(), "poly".to_string(), idx));
                poly_texts.push((id, text));
            }
        }
        
        let total_texts = text_to_id.len();
        println!("   📊 Total unique texts to embed: {}", total_texts);
        
        // STEG 2: Batch-embed alla texter
        let start_time = Instant::now();
        let mut all_items = Vec::new();
        all_items.extend(kalshi_texts);
        all_items.extend(poly_texts);
        
        println!("   🔄 Computing embeddings (batch processing)...");
        let embeddings_map: HashMap<String, Vec<f32>> = self.embedding_engine
            .embed_batch(&all_items)?
            .into_iter()
            .collect();
        
        let elapsed = start_time.elapsed();
        let items_per_sec = total_texts as f64 / elapsed.as_secs_f64().max(0.001);
        println!("   ✅ Embeddings computed in {:.2}s ({:.1} items/sec)", elapsed.as_secs_f64(), items_per_sec);
        
        // Cache-statistik kommer från PythonEmbeddingEngine internt
        // (visas i Python-scriptet via stderr när batch-embedding körs)
        // Vi kan inte enkelt hämta det från trait object, så vi returnerar 0,0
        // Cache-statistik visas ändå i Python-scriptets stderr output
        let cache_hits = 0;
        let cache_misses = 0;
        
        // STEG 3: Matcha events
        println!("   🔄 Matching events...");
        let mut all_results = Vec::new();
        let mut processed = 0;
        
        for (kalshi_idx, kalshi_event) in kalshi_events.iter().enumerate() {
            let kalshi_text = format!("{} {}", kalshi_event.title, kalshi_event.subtitle);
            let kalshi_id = format!("kalshi_{}", kalshi_idx);
            
            let kalshi_emb = embeddings_map.get(&kalshi_id)
                .ok_or_else(|| anyhow::anyhow!("Missing Kalshi embedding for {}", kalshi_id))?;
            
            // Hitta top-K Polymarket kandidater inom samma kategori
            let mut candidates: Vec<(usize, f32)> = Vec::new();
            
            for (poly_idx, poly_event) in polymarket_events.iter().enumerate() {
                // Endast matcha inom samma kategori
                if kalshi_event.category != poly_event.assigned_category {
                    continue;
                }
                
                let poly_id = format!("poly_{}", poly_idx);
                let poly_emb = embeddings_map.get(&poly_id)
                    .ok_or_else(|| anyhow::anyhow!("Missing Polymarket embedding for {}", poly_id))?;
                
                let similarity = self.embedding_engine.similarity(kalshi_emb, poly_emb)?;
                candidates.push((poly_idx, similarity));
            }
            
            // Sortera och ta top-K (DESC by embedding_score)
            candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            candidates.truncate(self.top_k);
            
            // Bygg kandidatlista
            let polymarket_candidates: Vec<PolymarketCandidate> = candidates
                .into_iter()
                .map(|(poly_idx, embedding_score)| {
                    let poly_event = &polymarket_events[poly_idx];
                    PolymarketCandidate {
                        polymarket_event_id: poly_event.event_id.clone(),
                        embedding_score,
                    }
                })
                .collect();
            
            // Lägg till resultat (endast om det finns kandidater)
            if !polymarket_candidates.is_empty() {
                all_results.push(KalshiEventCandidates {
                    kalshi_event_id: kalshi_event.event_id.clone(),
                    candidates: polymarket_candidates,
                });
            }
            
            processed += 1;
            if processed % 100 == 0 {
                println!("      Processed {}/{} Kalshi events...", processed, kalshi_events.len());
            }
        }
        
        Ok((all_results, cache_hits, cache_misses))
    }
}

#[derive(Debug, Clone)]
pub struct KalshiEvent {
    pub event_id: String,
    pub title: String,
    pub subtitle: String,
    pub category: String,
}

#[derive(Debug, Clone)]
pub struct PolymarketEvent {
    pub event_id: String,
    pub title: String,
    pub assigned_category: String,
}
