# Kodinventering - Polymarket-Kalshi-Arbitrage-bot

## 1. Hög-nivå Modulöversikt

### Rust (`src/`)

#### `src/main.rs` (597 rader)
- **Syfte**: CLI orchestrator för hela pipeline
- **Kommandon**:
  - `ingest_kalshi` - Hämtar Kalshi events
  - `ingest_polymarket` - Hämtar Polymarket markets
  - `build_polymarket_events` - Bygger event-candidates
  - `categorize_polymarket` - Kategorisering (placeholder)
  - `match_events_embeddings` - Embedding-only matching
  - `match_events` - Full matching (placeholder)
  - `match_contracts` - Contract matching (placeholder)
- **Status**: Fungerar för PHASE 1-3 (embeddings), PHASE 4-5 ej implementerad

#### `src/ai/` - AI Abstraktioner
- **`embedding.rs`**: `PythonEmbeddingEngine`
  - Spawnar `scripts/embed.py` som subprocess
  - Disk-caching (SHA256 hash)
  - Batch processing
  - Cache statistics tracking
  - **Status**: ✅ Fungerar

- **`verifier.rs`**: `PythonLLMVerifier`
  - Trait definition för LLM verification
  - Placeholder implementation
  - **Status**: ⚠️ TODO - inte implementerad

- **`mod.rs`**: Module exports
  - **Status**: ✅ Fungerar

#### `src/matching/` - Matching Logic
- **`event_matcher.rs`**: Event-level matching
  - `EmbeddingOnlyMatcher` - ✅ Fungerar
  - `EventMatcher` - ⚠️ Använder placeholder verifier
  - Batch embedding för effektivitet
  - Top-K candidate selection
  - **Status**: Embedding-del fungerar, LLM-verifiering saknas

- **`contract_matcher.rs`**: Contract-level matching
  - Struct definition
  - **Status**: ⚠️ TODO - inte implementerad

- **`mod.rs`**: Module exports
  - **Status**: ✅ Fungerar

### Python (`scripts/`)

#### Data Processing Scripts
- **`embed.py`**: Embedding generation (bge-m3)
  - ✅ Fungerar, stödjer CPU/MPS/CUDA
  - Progress tracking med tqdm
  - Batch processing

- **`polymarket_build_market_to_event.py`**: Market→event mapping
  - ✅ Fungerar (500/500 matched)
  - Robust lookup från markets
  - Debug-statistik

- **`kalshi_fetch_markets_for_matched_events.py`**: Kalshi market fetching
  - ✅ Fungerar med signed requests
  - Rate limiting (429 backoff)
  - Checkpoint/resume
  - Pace på 200-svar

- **`build_market_docs.py`**: Market docs builder
  - ✅ Fungerar
  - Bygger joinable docs för båda sidor

- **`build_event_bridge.py`**: Bridge mapping (embedding-baserad)
  - ✅ Fungerar men används inte längre (ersatt av LLM)

#### LLM Scripts
- **`local_llm_client.py`**: LLM adapter
  - ✅ Stödjer Ollama, LM Studio, llama.cpp
  - Retry/backoff
  - Timeout handling

- **`llm_classify_events.py`**: Event classification
  - ✅ Fungerar
  - Cache per event
  - Checkpoint/resume

- **`llm_match_events_within_category.py`**: Event matching
  - ✅ Fungerar
  - Two-stage: candidate retrieval → pair decision
  - One-to-one constraint
  - Incremental save

- **`sanity_check_docs.py`**: Validation
  - ✅ Fungerar
  - Bridge coverage reporting

## 2. Identifierade Risker / Tech Debt

### 🔴 Kritiska

1. **`unwrap()` i `event_matcher.rs` (2 ställen)**
   - Rad 91, 225: `partial_cmp().unwrap()`
   - Risk: Panic om NaN i similarity scores
   - **Åtgärd**: Använd `unwrap_or()` eller explicit NaN-handling

2. **Stora filer i git**
   - `data/polymarket/raw/polymarket_markets.json` (64.94 MB)
   - Över GitHub's 50 MB rekommendation
   - **Åtgärd**: Lägg till i `.gitignore` eller använd Git LFS

3. **Duplicerad kategori-lista**
   - `src/main.rs` har hardcoded `KALSHI_CATEGORIES`
   - `config/kalshi_categories.json` har samma data
   - **Åtgärd**: Läs från config-fil istället

### 🟡 Viktiga

4. **Placeholder implementations**
   - `PythonLLMVerifier::verify_match()` returnerar alltid `(false, 0.0, ...)`
   - `ContractMatcher::match_contracts()` returnerar tom Vec
   - **Status**: Inte blockerande (LLM scripts används istället)

5. **Error handling i Python scripts**
   - `sys.exit(1)` används flitigt (ok men kan förbättras)
   - Vissa scripts saknar try/except för JSON parsing

6. **Ingen validering av LLM JSON responses**
   - `llm_classify_events.py` och `llm_match_events_within_category.py`
   - Parsar JSON utan validering av schema
   - **Risk**: Kunde få felaktiga kategorier/matches

### 🟢 Mindre

7. **Kommenterad kod i mod.rs**
   - Exports är kommenterade men används ändå i main.rs
   - **Status**: Fungerar men otydligt

8. **Ingen enhetstestning**
   - Inga tests för kritiska funktioner
   - **Status**: Acceptabelt för nuvarande fas

9. **Hardcoded paths**
   - Många scripts har hardcoded paths som `data/matching/...`
   - **Status**: Fungerar men gör testing svårare

## 3. Föreslagna Förbättringar

### 1. Fixa `unwrap()` i event_matcher.rs
**Prioritet**: 🔴 Hög
**Effort**: 5 min
**Impact**: Förhindrar potentiella panics

```rust
// Istället för:
candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

// Använd:
candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
```

### 2. Konsolidera kategori-lista
**Prioritet**: 🟡 Medium
**Effort**: 15 min
**Impact**: Single source of truth, enklare underhåll

- Läs kategorier från `config/kalshi_categories.json` i Rust
- Ta bort hardcoded lista från `main.rs`

### 3. Förbättra error handling i Python scripts
**Prioritet**: 🟡 Medium
**Effort**: 30 min
**Impact**: Bättre felmeddelanden, mindre risk för dataförlust

- Lägg till try/except för JSON parsing
- Använd custom exceptions istället för `sys.exit()`
- Logga errors till fil för debugging

### 4. Validera LLM JSON responses
**Prioritet**: 🟡 Medium
**Effort**: 20 min
**Impact**: Förhindrar felaktiga kategorier/matches

- Validera JSON schema innan användning
- Fallback till "Unknown" kategori om validering misslyckas
- Logga invalid responses för debugging

### 5. Lägg till `.gitignore` för stora filer
**Prioritet**: 🟡 Medium
**Effort**: 2 min
**Impact**: Undviker GitHub-varningar

- Lägg till `data/polymarket/raw/polymarket_markets.json` i `.gitignore`
- Eller använd Git LFS för stora datafiler

## 4. Arkitektur-översikt

```
┌─────────────────────────────────────────────────────────┐
│                    CLI (main.rs)                        │
│  ingest_kalshi | ingest_polymarket | match_events_*     │
└─────────────────────────────────────────────────────────┘
                          │
        ┌─────────────────┴─────────────────┐
        │                                     │
┌───────▼────────┐              ┌────────────▼──────────┐
│  src/ai/       │              │  src/matching/       │
│  - embedding   │              │  - event_matcher     │
│  - verifier    │              │  - contract_matcher  │
└───────┬────────┘              └────────────┬──────────┘
        │                                     │
        └─────────────────┬─────────────────┘
                          │
        ┌─────────────────▼─────────────────┐
        │      Python Scripts (scripts/)    │
        │  - embed.py (bge-m3)              │
        │  - llm_*.py (local LLM)          │
        │  - build_*.py (data processing)   │
        └───────────────────────────────────┘
```

## 5. Data Flow

```
PHASE 1: Raw Data Ingestion
  ├─ Kalshi: events → kalshi_events_minimal.json
  └─ Polymarket: markets → markets_full.json

PHASE 2: Event Building
  ├─ Polymarket: markets → polymarket_events_minimal.json
  └─ Market→Event: market_to_event.json

PHASE 3: Classification & Matching
  ├─ LLM Classify: events → event_categories.jsonl
  ├─ LLM Match: categories → event_bridge.llm.json
  └─ Embedding Match: event_candidates.json (backup)

PHASE 4: Market-level Data
  ├─ Kalshi: fetch markets → kalshi_markets_full.jsonl
  └─ Build docs: market_docs.jsonl (both sides)

PHASE 5: Contract Matching
  └─ TODO: Match contracts within matched events
```

## 6. Dependencies

### Rust
- `reqwest` - HTTP client
- `tokio` - Async runtime
- `serde` - Serialization
- `anyhow` - Error handling
- `hmac`, `sha2`, `hex`, `base64` - Crypto (Kalshi auth)

### Python
- `sentence-transformers` - Embeddings (bge-m3)
- `requests` - HTTP client
- `numpy` - Numerics
- `tqdm` - Progress bars

## 7. Status Summary

| Komponent | Status | Notes |
|-----------|--------|-------|
| Data Ingestion | ✅ | Fungerar |
| Embedding Matching | ✅ | Fungerar |
| LLM Classification | ✅ | Fungerar |
| LLM Matching | ✅ | Fungerar |
| Market→Event Mapping | ✅ | 100% match rate |
| Contract Matching | ⚠️ | TODO |
| LLM Verifier (Rust) | ⚠️ | Placeholder |
| Error Handling | 🟡 | Kan förbättras |
| Testing | ❌ | Saknas |

## 8. Nästa Steg (Prioriterat)

1. Fixa `unwrap()` i event_matcher.rs
2. Konsolidera kategori-lista
3. Lägg till `.gitignore` för stora filer
4. Validera LLM JSON responses
5. Implementera contract matching (PHASE 4)
