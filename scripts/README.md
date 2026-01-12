# Market Similarity Calibration Script

```
╔═══════════════════════════════════════════════════════════════════════╗
║                                                                       ║
║         This script OBSERVES and MEASURES signal.                    ║
║         It does NOT match, decide, or threshold.                     ║
║                                                                       ║
╚═══════════════════════════════════════════════════════════════════════╝
```

## Purpose

**Signal calibration and understanding** before building matching logic.

### What it DOES:
✅ Measures embedding similarity distributions  
✅ Profiles top-k similarities per market  
✅ Samples pairs stratified by percentiles  
✅ Gets descriptive LLM reasoning (free text)  
✅ Generates transparent reports  

### What it DOES NOT do:
❌ Make matching decisions  
❌ Set thresholds  
❌ Auto-group markets  
❌ Run arbitrage logic  
❌ Use LLM verdicts or enums  

---

## Setup

### 1. Install Python dependencies:

```bash
cd scripts
pip install -r requirements.txt
```

### 2. Set environment variables:

Create a `.env` file in the project root:

```bash
# OpenAI API (for embeddings)
OPENAI_API_KEY=your_openai_key

# Anthropic API (for Claude descriptions)
ANTHROPIC_API_KEY=your_anthropic_key
```

---

## Usage

### Run calibration:

```bash
# From project root
python scripts/calibrate_similarity.py
```

### What happens:

1. **Loads active markets** from JSONL files
2. **Generates embeddings** (EVENT + TIME WINDOW)
3. **Computes similarities** (all Kalshi × Polymarket pairs)
4. **Profiles distributions** (top-1, top-5, top-20 per market)
5. **Stratified sampling** (using percentiles, not fixed thresholds)
6. **LLM descriptions** (free text reasoning, no verdicts)
7. **Generates reports** in `data/reports/`

---

## Output Files

All reports saved to `data/reports/`:

### `baseline_counts.json`
Raw counts of active markets per platform.

### `similarity_profiles.json`
Top-k similarity scores for each Kalshi market.

**Example:**
```json
{
  "kalshi_fingerprint": "abc123...",
  "kalshi_ticker": "KXEPLGAME-...",
  "top_1_score": 0.8234,
  "top_5_scores": [0.8234, 0.7891, 0.7456, ...],
  "top_20_scores": [...]
}
```

### `sampled_pairs_for_llm.json`
Pairs selected for LLM analysis, stratified by similarity.

**Buckets:**
- **High**: Top 33rd percentile
- **Medium**: 33rd-67th percentile  
- **Low**: Bottom 33rd percentile

### `llm_descriptions.json`
Claude's descriptive analysis of sampled pairs.

**Example:**
```json
{
  "kalshi_fingerprint": "abc123...",
  "polymarket_fingerprint": "def456...",
  "similarity_score": 0.8234,
  "bucket": "high",
  "llm_description": "Market A predicts UEFA Champions League...\nMarket B also concerns UEFA...\nThey appear to relate to the same underlying event..."
}
```

### `calibration_summary.txt`
Human-readable summary with distribution statistics.

---

## Configuration

Edit `CalibrationConfig` in the script:

```python
@dataclass
class CalibrationConfig:
    # Embedding model
    embedding_model: str = "text-embedding-3-small"
    
    # LLM model
    llm_model: str = "claude-sonnet-4-20250514"
    llm_temperature: float = 0.0
    
    # Sampling
    sample_size_per_bucket: int = 10  # Pairs per similarity bucket
    
    # Top-k profiling
    top_k_similarities: List[int] = [1, 5, 20]
```

---

## Interpreting Results

### 1. Similarity Distribution

Look at `calibration_summary.txt`:

```
Min similarity: 0.3245
25th percentile: 0.5123
Median: 0.6234
75th percentile: 0.7456
Max similarity: 0.9123
```

**Questions to ask:**
- Is there clear separation between "related" and "unrelated"?
- Where do most scores cluster?
- Are there natural breakpoints?

### 2. LLM Descriptions

Review `llm_descriptions.json`:

**Look for:**
- **High similarity + LLM sees connection** → Good signal
- **High similarity + LLM sees difference** → Embedding blindspot
- **Low similarity + LLM sees connection** → Missed by embeddings
- **Low similarity + LLM agrees** → True negatives

### 3. Manual Sanity Check

```bash
# View high similarity pairs
jq '.descriptions[] | select(.bucket == "high")' data/reports/llm_descriptions.json

# View low similarity pairs  
jq '.descriptions[] | select(.bucket == "low")' data/reports/llm_descriptions.json

# Find divergences (high similarity but LLM says different)
# → Read LLM descriptions manually
```

---

## Next Steps (After Review)

Based on calibration insights:

1. **If embeddings alone are sufficient:**
   - Design threshold-based matching
   - Use LLM only for edge cases

2. **If embeddings miss important cases:**
   - Design hybrid approach
   - Use LLM more extensively
   - Consider different embedding strategies

3. **If signal is weak:**
   - Revisit embedding text (include RULES?)
   - Try different embedding models
   - Consider pure LLM-driven approach

---

## Important Reminders

⚠️ **This is NOT matching logic** - it's signal calibration  
⚠️ **NO decisions are automated** - human review required  
⚠️ **NO thresholds are set** - that comes later with context  

**Review the data. Understand the distributions. Then design matching logic with full insight.**

---

## Troubleshooting

### "OpenAI API error"
Check your `OPENAI_API_KEY` in `.env`

### "Anthropic API error"
Check your `ANTHROPIC_API_KEY` in `.env`

### "File not found: kalshi_markets_structured.jsonl"
Run the main observation pipeline first:
```bash
cargo run --release
```

### "Too many API calls"
Reduce `sample_size_per_bucket` in config (default: 10)

---

## Design Philosophy

> **"Measure before you match. Understand before you automate."**

This script provides **transparent measurement** of similarity signal.

No black boxes. No hidden thresholds. No auto-decisions.

Just **data, distributions, and descriptive insights** to inform the next design step.
