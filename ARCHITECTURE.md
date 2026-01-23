# Knowledge Graph Building System - Arkitektur

## Översikt

Detta system bygger en knowledge graph för Kalshi ↔ Polymarket event- och contract-matching. Det är **inte en bot**, utan ett system för att bygga och underhålla en permanent mapping mellan plattformarna.

## Arkitektur

### Modulär struktur

```
src/
├── main.rs              # Pipeline orchestration
├── ai/
│   ├── mod.rs          # AI-modul exports
│   ├── embedding.rs     # Embedding engine (bge-m3)
│   └── verifier.rs     # LLM verifier (Qwen2.5-7B-Instruct)
└── matching/
    ├── mod.rs          # Matching-modul exports
    ├── event_matcher.rs # Event-level matching
    └── contract_matcher.rs # Contract-level matching
```

### PHASE 1: Full Data Ingestion

**Kommando:** `cargo run ingest_kalshi` / `cargo run ingest_polymarket`

- Hämtar **ALLA** Kalshi events (från befintlig data)
- Hämtar **ALLA** Polymarket markets (via API)
- Ingen filtrering - samlar all data

**Output:**
- `data/kalshi/derived/events.json`
- `data/polymarket/raw/markets_full.json`
- `data/polymarket/derived/events.json`

### PHASE 2: Semantic Categorization

**Kommando:** `cargo run categorize_polymarket`

- Använder Kalshi-kategorier som ground truth
- Tilldelar varje Polymarket event till **EXAKT EN** Kalshi-kategori
- Använder lokal AI (embedding + LLM)

**Output:**
- `data/polymarket/derived/events_categorized.json`

**Struktur:**
```json
{
  "event_id": "pm_deportation-count",
  "assigned_category": "Politics",
  "confidence": 0.93
}
```

### PHASE 3: Event-Level Semantic Matching

**Kommando:** `cargo run match_events`

**TWO-STAGE approach:**

1. **Embedding similarity (high recall)**
   - Model: `bge-m3`
   - Beräknar cosine similarity mellan event-titlar
   - Behåller top-K kandidater (t.ex. 10) per Polymarket event
   - Matchar **endast inom samma kategori**

2. **LLM verification (high precision)**
   - Model: `Qwen2.5-7B-Instruct` (local, quantized)
   - Prompt: "Are these two descriptions referring to the same real-world event?"
   - Returnerar: `(is_match: bool, confidence: f32, reason: String)`

**Output:**
- `data/matching/event_matches.json`

**Struktur:**
```json
{
  "kalshi_event_id": "KXTRUMPDEPORT-25",
  "polymarket_event_id": "pm_deportation-count",
  "confidence": 0.87,
  "embedding_score": 0.82,
  "llm_verified": true,
  "llm_confidence": 0.91,
  "reason": "Both refer to Trump's deportation policy in 2025"
}
```

### PHASE 4: Contract-Level Matching

**Kommando:** `cargo run match_contracts` (not yet implemented)

- För varje matchat event-par:
  - Hämta alla Kalshi contracts
  - Hämta alla Polymarket markets
  - Använd samma två-stegs approach (embedding → LLM)

**Output:**
- `data/matching/contract_matches.json`

### PHASE 5: Runtime Scanning

- **Ingen AI** används här
- Endast scannar odds, liquidity, timing
- Använder permanent mappings från PHASE 3-4

## AI Implementation

### Nuvarande status

AI-modulerna är **abstraherade** men inte fullt implementerade:

- `EmbeddingEngine` trait - kan bytas ut
- `LLMVerifier` trait - kan bytas ut
- Placeholder-implementationer finns

### Nästa steg för AI

1. **Embeddings (bge-m3)**
   - Alternativ 1: Python subprocess med `sentence-transformers`
   - Alternativ 2: Native Rust med `candle` (när det är mer mogen)
   - Alternativ 3: ONNX Runtime med konverterad modell

2. **LLM (Qwen2.5-7B-Instruct)**
   - Alternativ 1: Python subprocess med `transformers` + quantization
   - Alternativ 2: `llama.cpp` via Rust bindings
   - Alternativ 3: `candle` (när det stödjer inference)

### Rekommendation

**För nu:** Python subprocess (enklast att komma igång)
- Skapa `scripts/embed.py` för embeddings
- Skapa `scripts/verify.py` för LLM verification
- Anropa från Rust via `std::process::Command`

**Framtida:** Native Rust när `candle` är mer mogen

## Debugging & Verifiering

Alla matches sparar:
- `embedding_score` - similarity från embeddings
- `llm_verified` - om LLM verifierade matchningen
- `llm_confidence` - LLM:s confidence
- `reason` - förklaring från LLM

Detta gör det möjligt att:
- Debugga varför matches misslyckades
- Justera trösklar
- Förbättra prompts

## Optimering

Systemet är optimerat för:
- ✅ **Precision** över hastighet
- ✅ **Körs sällan** (1-2 gånger/dag)
- ✅ **Lokal körning** (ingen API-LLM)
- ✅ **Debuggbarhet** (sparar alla scores)

**INTE** optimerat för:
- ❌ Realtid matching
- ❌ Höga frekvenser
- ❌ Låg latens

## Nästa steg

1. **Implementera embeddings** (bge-m3 via Python)
2. **Implementera LLM verification** (Qwen2.5 via Python)
3. **Testa på riktig data**
4. **Justera prompts och trösklar**
5. **Implementera contract-level matching**
