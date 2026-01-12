# Kalshi Market Observer

```
╔═══════════════════════════════════════════════════════════════════════╗
║                                                                       ║
║         This branch observes Kalshi reality.                         ║
║         It does not attempt to understand it.                        ║
║                                                                       ║
╚═══════════════════════════════════════════════════════════════════════╝
```

## Branch: `raw-kalshi-v4-clean`

Detta är en **ren observationsbas** för Kalshi markets. Ingen tolkning. Ingen matchning. Ingen AI. Bara rådata.

## Vad gör detta program?

### Pipeline: RAW → STRUCTURED

1. 🔍 **Hämtar ALLA Kalshi events** via `/events` API med cursor-paginering
2. 📦 **Sparar komplett rådata** → `data/kalshi/raw/kalshi_raw_markets.json`
3. 🔄 **Mekanisk strukturering** (ingen AI) → `data/kalshi/structured/kalshi_markets_structured.jsonl`
4. ✅ **AI-ready format** för nästa steg

### 🚫 GÖR INTE:
- Ingen filtrering
- Ingen tolkning  
- Ingen arbitrage-logik
- Ingen Polymarket-integration
- Ingen AI-matchning (ännu)

## Struktur

```
src/
├── main.rs       # Kör observation och sparar till JSON
├── kalshi.rs     # Ren Kalshi API-klient (auth + fetch)
├── types.rs      # Rådata-typer (1:1 mapping av API)
├── config.rs     # Endast API URLs och delays
└── lib.rs        # Modul-exports
```

## Vad har tagits bort?

✅ **Borttaget från detta branch:**
- `src/polymarket.rs` - Hela Polymarket-integration
- `src/polymarket_clob.rs` - CLOB-klient
- `src/execution.rs` - Order execution
- `src/discovery.rs` - Gammal discovery-logik
- `src/matcher/` - All AI/embeddings-logik
- `src/circuit_breaker.rs`
- `src/position_tracker.rs`
- `src/cache.rs`
- `src/db.rs`
- `scripts/generate_embeddings.py`

✅ **Behållet från Kalshi:**
- `KalshiConfig` - Auth och signering
- `KalshiApiClient` - HTTP-klient med retry/rate limit
- `discover_all_events_paginated()` - Cursor-paginering
- Headers, base URL, rate limit-logik

## Kör programmet

```bash
# Sätt miljövariabler
export KALSHI_API_KEY_ID="your_key_id"
# Private key läses från test.txt (default) eller KALSHI_PRIVATE_KEY_PATH

# Kör observation
cargo run --release

# Output: kalshi_raw_markets.json
```

## Output-format

### RAW (`data/kalshi/raw/kalshi_raw_markets.json`)

Varje market sparas som en `RawMarketObservation`:

```json
{
  "event_ticker": "KXEPLGAME-25JAN13CHEARS",
  "series_ticker": "KXEPLGAME",
  "market_ticker": "KXEPLGAME-25JAN13CHEARS-CHE",
  "title": "Will Chelsea win vs Arsenal on Jan 13?",
  "subtitle": "Premier League match",
  "rules": "Match result at 90 minutes + added time",
  "open_time": "2025-01-10T12:00:00Z",
  "close_time": "2025-01-13T19:00:00Z",
  "status": "open",
  "raw_json": { ... }
}
```

### STRUCTURED (`data/kalshi/structured/kalshi_markets_structured.jsonl`)

AI-ready format (en rad per market):

```json
{
  "source": "kalshi",
  "market_ticker": "KXEPLGAME-25JAN13CHEARS-CHE",
  "event_ticker": "KXEPLGAME-25JAN13CHEARS",
  "series_ticker": "KXEPLGAME",
  "event_text": "Will Chelsea win vs Arsenal on Jan 13?",
  "rules_text": "Premier League match | Regular time + injury time",
  "category": null,
  "time_window": {
    "open": "2025-01-10T12:00:00Z",
    "close": "2025-01-13T19:00:00Z"
  },
  "status": "open",
  "raw_json": { ... }
}
```

Se [AI_SCHEMA.md](./AI_SCHEMA.md) för fullständig schema-dokumentation.

## Nästa steg (INTE i denna branch)

I nästa branch kommer vi att:

1. 🤖 **Mata rådata till state-of-the-art LLM**
2. 📖 **Låta modellen läsa kontrakt och regler**
3. 🧠 **Gruppera markets som en människa skulle göra**
4. 🎯 **Bygga probabilistisk matchning**

Men först måste vi ha perfekt rå input. **Detta är det.**

## Designprincip

> **"This branch observes Kalshi reality. It does not attempt to understand it."**

Detta betyder:
- Vi fetchar ALLT
- Vi sparar ALLT
- Vi tolkar INGET
- Nästa steg får göra tolkningen

---

**Branch:** `raw-kalshi-v4-clean`  
**Status:** ✅ Klar för observation  
**Nästa:** AI-driven market grouping
