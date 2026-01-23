# Polymarket-Kalshi Arbitrage Bot

AI-based event and contract matching system for Kalshi ↔ Polymarket arbitrage opportunities.

## Overview

This system builds a knowledge graph connecting events and contracts between Kalshi and Polymarket prediction markets, enabling automated arbitrage detection.

## Architecture

### PHASE 1: Full Data Ingestion
- Fetch ALL Kalshi events with categories
- Fetch ALL Polymarket markets
- Store raw data for processing

### PHASE 2: Semantic Categorization
- Classify Polymarket events into Kalshi's category taxonomy using local LLM
- Store confidence scores and rationale

### PHASE 3: Event-level Semantic Matching
- Match events within the same category using local LLM
- Two-stage process: candidate retrieval → pair decision
- High-confidence bridges only (confidence >= 0.80)

### PHASE 4: Contract-level Matching
- Match individual markets/contracts within matched events
- Enable precise arbitrage detection

### PHASE 5: Runtime Scanning
- Scan odds, liquidity, and timing
- No AI models used (only price data)

## Setup

### Prerequisites

- Rust (latest stable)
- Python 3.10+
- Local LLM server (Ollama, LM Studio, or llama.cpp)

### Installation

```bash
# Rust dependencies
cargo build

# Python dependencies
python3 -m venv .venv
source .venv/bin/activate
pip install sentence-transformers numpy tqdm requests
```

### LLM Configuration

```bash
export LOCAL_LLM_PROVIDER=ollama  # or lmstudio, llamacpp
export LOCAL_LLM_BASE_URL=http://localhost:11434
export LOCAL_LLM_MODEL=llama3.2
export LOCAL_LLM_TEMPERATURE=0
export LOCAL_LLM_MAX_TOKENS=2048
```

Start LLM server:
```bash
# Ollama
ollama serve
ollama pull llama3.2

# LM Studio: Start server on port 1234
# llama.cpp: ./server -m model.gguf --port 8080
```

## Usage

### Data Ingestion

```bash
# Fetch Kalshi events
cargo run ingest_kalshi

# Fetch Polymarket markets
cargo run ingest_polymarket
```

### Event Classification

```bash
# Classify events to categories
python3 scripts/llm_classify_events.py
```

### Event Matching

```bash
# Match events within categories
python3 scripts/llm_match_events_within_category.py
```

### Market-level Data

```bash
# Build Polymarket market→event mapping
python3 scripts/polymarket_build_market_to_event.py

# Fetch Kalshi markets for matched events
export KALSHI_API_KEY_ID="..."
export KALSHI_API_SECRET="..."
python3 scripts/kalshi_fetch_markets_for_matched_events.py

# Build market docs
python3 scripts/build_market_docs.py

# Build event bridge
python3 scripts/build_event_bridge.py
```

### Verification

```bash
# Sanity checks
python3 scripts/sanity_check_docs.py
```

## Project Structure

```
.
├── src/                    # Rust source code
│   ├── main.rs            # CLI entry point
│   ├── ai/                # AI abstractions (embedding, LLM)
│   └── matching/          # Matching logic
├── scripts/               # Python scripts
│   ├── embed.py           # Embedding generation
│   ├── llm_classify_events.py
│   ├── llm_match_events_within_category.py
│   └── ...
├── config/                # Configuration files
│   └── kalshi_categories.json
└── data/                  # Data storage
    ├── kalshi/
    ├── polymarket/
    └── matching/
```

## Features

- **Local AI**: All AI inference runs locally (no external APIs)
- **Checkpoint/Resume**: Can resume interrupted runs
- **Caching**: LLM responses cached to disk
- **Deterministic**: Temperature=0, fixed prompts
- **Robust**: Rate limiting, backoff, error handling

## License

[Your License Here]
