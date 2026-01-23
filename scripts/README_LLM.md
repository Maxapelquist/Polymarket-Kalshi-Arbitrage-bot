# LLM-driven Event Matching Pipeline

Denna pipeline använder lokal LLM för att klassificera och matcha events mellan Kalshi och Polymarket.

## Setup

### 1. Starta lokal LLM-server

**Ollama (rekommenderat):**
```bash
ollama serve
# I annat terminal:
ollama pull llama3.2  # eller annan modell
```

**LM Studio:**
- Starta LM Studio
- Starta local server på port 1234

**llama.cpp:**
```bash
./server -m model.gguf --port 8080
```

### 2. Konfigurera environment variables

```bash
export LOCAL_LLM_PROVIDER=ollama  # eller lmstudio, llamacpp
export LOCAL_LLM_BASE_URL=http://localhost:11434  # eller annan port
export LOCAL_LLM_MODEL=llama3.2  # eller din modell
export LOCAL_LLM_TEMPERATURE=0
export LOCAL_LLM_MAX_TOKENS=2048
```

## Körordning

```bash
# 1. Klassificera events till kategorier
python3 scripts/llm_classify_events.py

# 2. Matcha events inom kategori
python3 scripts/llm_match_events_within_category.py

# 3. Verifiera resultat
python3 scripts/sanity_check_docs.py
```

## Output

- `data/matching/pm_event_categories.jsonl` - Polymarket events med kategorier
- `data/matching/kalshi_event_categories.jsonl` - Kalshi events med kategorier
- `data/matching/event_bridge.llm.json` - High-confidence bridges
- `data/matching/event_bridge.llm.low_conf.json` - Low-confidence bridges
- `data/matching/event_bridge.llm.stats.json` - Statistik

## Checkpoint/Resume

Båda scripten stödjer checkpoint/resume:
- Klassificering: hoppar över redan klassificerade events (via cache)
- Matching: kan köras om utan att tappa arbete

## Cache

LLM-responser cachas i:
- `data/cache/llm/classify/<provider>_<model>/<hash>.json`

Vid omkörning används cache direkt.
