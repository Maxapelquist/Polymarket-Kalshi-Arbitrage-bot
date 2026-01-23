//! Embedding engine för semantisk likhet
//!
//! Använder bge-m3 för att generera embeddings och beräkna cosine similarity

use anyhow::Result;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::io::Write;
use std::path::PathBuf;
use sha2::{Sha256, Digest};

/// Embedding engine trait - abstraktion för olika embedding-modeller
pub trait EmbeddingEngine {
    /// Generera embedding för en text
    fn embed(&mut self, text: &str) -> Result<Vec<f32>>;
    
    /// Beräkna cosine similarity mellan två embeddings
    fn similarity(&self, emb1: &[f32], emb2: &[f32]) -> Result<f32>;
    
    /// Batch-embedding för effektivitet
    fn embed_batch(&mut self, items: &[(String, String)]) -> Result<Vec<(String, Vec<f32>)>>;
}

/// Python-baserad embedding engine (använder sentence-transformers via Python)
pub struct PythonEmbeddingEngine {
    script_path: PathBuf,
    cache_dir: PathBuf,
    cache_hits: usize,
    cache_misses: usize,
}

impl PythonEmbeddingEngine {
    pub fn new(script_path: PathBuf, cache_dir: PathBuf) -> Self {
        // Skapa cache-dir om den inte finns
        std::fs::create_dir_all(&cache_dir).ok();
        Self { 
            script_path, 
            cache_dir,
            cache_hits: 0,
            cache_misses: 0,
        }
    }
    
    pub fn default() -> Self {
        let script_path = PathBuf::from("scripts/embed.py");
        let cache_dir = PathBuf::from("data/cache/embeddings");
        Self::new(script_path, cache_dir)
    }
    
    /// Hämta cache-statistik
    pub fn get_cache_stats(&self) -> (usize, usize) {
        (self.cache_hits, self.cache_misses)
    }
    
    /// Reset cache-statistik
    pub fn reset_cache_stats(&mut self) {
        self.cache_hits = 0;
        self.cache_misses = 0;
    }
    
    /// Hash text för cache-nyckel
    fn hash_text(&self, text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    /// Läs embedding från cache
    fn load_from_cache(&self, text_hash: &str) -> Result<Option<Vec<f32>>> {
        let cache_file = self.cache_dir.join(format!("{}.json", text_hash));
        if !cache_file.exists() {
            return Ok(None);
        }
        
        let content = std::fs::read_to_string(&cache_file)?;
        let cached: serde_json::Value = serde_json::from_str(&content)?;
        
        if let Some(vector) = cached.get("vector").and_then(|v| v.as_array()) {
            let embedding: Result<Vec<f32>, _> = vector
                .iter()
                .map(|v| v.as_f64().ok_or_else(|| anyhow::anyhow!("Invalid float")))
                .map(|v| v.map(|f| f as f32))
                .collect();
            return Ok(Some(embedding?));
        }
        
        Ok(None)
    }
    
    /// Spara embedding till cache
    fn save_to_cache(&self, text_hash: &str, embedding: &[f32]) -> Result<()> {
        let cache_file = self.cache_dir.join(format!("{}.json", text_hash));
        let cached = serde_json::json!({
            "vector": embedding
        });
        std::fs::write(&cache_file, serde_json::to_string_pretty(&cached)?)?;
        Ok(())
    }
    
    /// Anropa Python-script för embeddings
    fn call_python_script(&self, items: &[(String, String)]) -> Result<Vec<(String, Vec<f32>)>> {
        // Bygg input JSON
        let input_items: Vec<serde_json::Value> = items
            .iter()
            .map(|(id, text)| {
                serde_json::json!({
                    "id": id,
                    "text": text
                })
            })
            .collect();
        
        let input = serde_json::json!({
            "items": input_items
        });
        
        let input_str = serde_json::to_string(&input)?;
        
        // Kör Python-script (använd system Python, inte .venv om det har arkitekturproblem)
        // Försök hitta rätt Python-interpretator
        let python_cmd = std::env::var("PYTHON")
            .unwrap_or_else(|_| "python3".to_string());
        
        let mut child = Command::new(&python_cmd)
            .arg(&self.script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        // Skriv input
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input_str.as_bytes())?;
        }
        
        // Läs output
        let output = child.wait_with_output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Python script failed: {}", stderr));
        }
        
        let stdout = String::from_utf8(output.stdout)?;
        let result: serde_json::Value = serde_json::from_str(&stdout)?;
        
        // Parse embeddings
        let embeddings_array = result
            .get("embeddings")
            .and_then(|e| e.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid output format"))?;
        
        let mut results = Vec::new();
        for emb_obj in embeddings_array {
            let id = emb_obj.get("id")
                .and_then(|i| i.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
            
            let vector = emb_obj.get("vector")
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow::anyhow!("Missing vector"))?;
            
            let embedding: Result<Vec<f32>, _> = vector
                .iter()
                .map(|v| v.as_f64().ok_or_else(|| anyhow::anyhow!("Invalid float")))
                .map(|v| v.map(|f| f as f32))
                .collect();
            
            results.push((id.to_string(), embedding?));
        }
        
        Ok(results)
    }
}

impl EmbeddingEngine for PythonEmbeddingEngine {
    fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        // Kolla cache först
        let text_hash = self.hash_text(text);
        if let Some(cached) = self.load_from_cache(&text_hash)? {
            self.cache_hits += 1;
            return Ok(cached);
        }
        
        self.cache_misses += 1;
        
        // Generera embedding
        let items = vec![("single".to_string(), text.to_string())];
        let results = self.call_python_script(&items)?;
        
        let embedding = results
            .into_iter()
            .find(|(id, _)| id == "single")
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))?
            .1;
        
        // Spara till cache
        self.save_to_cache(&text_hash, &embedding)?;
        
        Ok(embedding)
    }
    
    fn similarity(&self, emb1: &[f32], emb2: &[f32]) -> Result<f32> {
        // Cosine similarity (embeddings är redan normaliserade från Python)
        let dot_product: f32 = emb1.iter().zip(emb2.iter()).map(|(a, b)| a * b).sum();
        
        // Extra normalisering för säkerhet
        let norm1: f32 = emb1.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm2: f32 = emb2.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm1 == 0.0 || norm2 == 0.0 {
            return Ok(0.0);
        }
        
        Ok(dot_product / (norm1 * norm2))
    }
    
    fn embed_batch(&mut self, items: &[(String, String)]) -> Result<Vec<(String, Vec<f32>)>> {
        let total_items = items.len();
        
        // Kolla cache för varje item
        let mut to_compute = Vec::new();
        let mut cached_results = HashMap::new();
        
        for (id, text) in items {
            let text_hash = self.hash_text(text);
            if let Some(cached) = self.load_from_cache(&text_hash)? {
                self.cache_hits += 1;
                cached_results.insert(id.clone(), cached);
            } else {
                self.cache_misses += 1;
                to_compute.push((id.clone(), text.clone()));
            }
        }
        
        // Logga cache-statistik
        let cache_hit_rate = if total_items > 0 {
            (self.cache_hits as f64 / total_items as f64) * 100.0
        } else {
            0.0
        };
        eprintln!("Cache: {} hits, {} misses ({:.1}% hit rate) | Computing {} new embeddings...", 
                 self.cache_hits, self.cache_misses, cache_hit_rate, to_compute.len());
        
        // Beräkna embeddings för items som inte är i cache
        let computed_results = if to_compute.is_empty() {
            Vec::new()
        } else {
            self.call_python_script(&to_compute)?
        };
        
        // Spara nya embeddings till cache
        for (id, embedding) in &computed_results {
            let text = to_compute.iter()
                .find(|(i, _)| i == id)
                .map(|(_, t)| t)
                .ok_or_else(|| anyhow::anyhow!("ID mismatch"))?;
            let text_hash = self.hash_text(text);
            self.save_to_cache(&text_hash, embedding)?;
        }
        
        // Kombinera cached och computed
        let mut all_results = Vec::new();
        for (id, _text) in items {
            if let Some(cached) = cached_results.get(id) {
                all_results.push((id.clone(), cached.clone()));
            } else {
                let computed = computed_results.iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, e)| e.clone())
                    .ok_or_else(|| anyhow::anyhow!("Missing computed embedding"))?;
                all_results.push((id.clone(), computed));
            }
        }
        
        Ok(all_results)
    }
}
