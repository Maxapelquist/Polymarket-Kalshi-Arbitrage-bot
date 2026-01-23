//! AI-moduler för embeddings och LLM-verifiering
//!
//! Abstraktioner för:
//! - Embedding engine (bge-m3)
//! - LLM verifier (Qwen2.5-7B-Instruct)

pub mod embedding;
pub mod verifier;

// Exports are used in matching modules, not directly in main
// pub use embedding::EmbeddingEngine;
// pub use verifier::LLMVerifier;
