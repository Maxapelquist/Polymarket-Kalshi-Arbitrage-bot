# Scripts för Data Layer

Detta dokument beskriver scripten för att bygga market-level data och joinable docs.

## Översikt

1. **polymarket_build_market_to_event.py** - Bygger market→event mapping för Polymarket
2. **kalshi_fetch_markets_for_matched_events.py** - Hämtar Kalshi markets/contracts per event_ticker (med signed requests)
3. **build_event_bridge.py** - Bygger bridge mapping mellan Kalshi event_ticker och Polymarket pm_event_id
4. **build_market_docs.py** - Bygger market_docs för båda sidor
5. **sanity_check_docs.py** - Validerar market_docs med bridge coverage

## Körordning (Pipeline)

### Steg 1: Bygg Polymarket market→event mapping

```bash
python3 scripts/polymarket_build_market_to_event.py
```

**Input:**
- `data/polymarket/raw/markets_full.json`
- `data/polymarket/derived/polymarket_events_minimal.json`

**Output:**
- `data/polymarket/derived/market_to_event.json` (matched markets)
- `data/polymarket/derived/market_to_event.miss.json` (missed markets med debug-fält)

### Steg 2: Hämta Kalshi markets för matchade events

```bash
# Sätt Kalshi API credentials
export KALSHI_API_KEY_ID="your_key_id"
export KALSHI_API_SECRET="your_secret"

# Kör scriptet
python3 scripts/kalshi_fetch_markets_for_matched_events.py
```

**Input:** `data/matching/event_candidates.filtered.json`  
**Output:** `data/kalshi/derived/kalshi_markets_full.jsonl`

**Notera:** Scriptet använder signed requests (HMAC-SHA256) enligt Kalshi API specifikation.

### Steg 3: Bygg event bridge

```bash
python3 scripts/build_event_bridge.py
```

**Input:** `data/matching/event_candidates.filtered.json` (eller `event_candidates.json`)  
**Output:** `data/matching/event_bridge.json`

### Steg 4: Bygg market_docs

```bash
python3 scripts/build_market_docs.py
```

**Input:**
- `data/polymarket/raw/markets_full.json`
- `data/polymarket/derived/market_to_event.json`
- `data/kalshi/derived/kalshi_markets_full.jsonl` (market-level, INTE raw events)

**Output:**
- `data/polymarket/derived/polymarket_market_docs.jsonl`
- `data/kalshi/derived/kalshi_market_docs.jsonl`

### Steg 5: Sanity check (med bridge coverage)

```bash
python3 scripts/sanity_check_docs.py
```

Kontrollerar:
- Counts (total docs, unique event_id)
- Top 10 events med flest markets (per sida)
- Bridge coverage (hur många markets faller under bridgade events)
- Exempel på bridgade eventpar med titles
- Kvalitetskontroller (tomma market_id, event_id coverage)

## Data Format

### kalshi_markets_full.jsonl

En rad per market/contract:
```json
{
  "event_ticker": "KXELONMARS-99",
  "market_ticker": "KXELONMARS-99-Y",
  "title": "Will Elon Musk visit Mars in his lifetime?",
  "subtitle": "Before 2099",
  "category": "World",
  "yes_bid": 0.45,
  "yes_ask": 0.50,
  "raw": {...}
}
```

### polymarket_market_docs.jsonl

En rad per market:
```json
{
  "market_id": "0xaf9d0e448129a9f657f851d49495ba4742055d80e0ef1166ba0ee81d4d594214",
  "event_id": "pm_deportation-count",
  "title": "Will Trump deport less than 250,000?",
  "subtitle": "During the 2024 FY ICE removed...",
  "category": "",
  "text": "Will Trump deport less than 250,000? | During the 2024 FY... | Event: How many people will Trump deport in 2025? | Slug: how-many-people-will-trump-deport-in-2025"
}
```

### kalshi_market_docs.jsonl

En rad per market:
```json
{
  "market_id": "KXELONMARS-99-Y",
  "event_id": "KXELONMARS-99",
  "title": "Will Elon Musk visit Mars in his lifetime?",
  "subtitle": "Before 2099",
  "category": "World",
  "text": "Will Elon Musk visit Mars in his lifetime? | Before 2099 | Category: World | Event: Will Elon Musk visit Mars in his lifetime?"
}
```

## Troubleshooting

### Kalshi API authentication errors

Om du får 401/403 errors:
1. Kontrollera att `KALSHI_API_KEY_ID` och `KALSHI_API_SECRET` är satta
2. Scriptet använder signed requests (HMAC-SHA256) - kontrollera att credentials är korrekta
3. Kalshi API kräver timestamp och signature i headers - se `sign_kalshi_request()` funktionen

### Tomma market_id i Kalshi

Om `market_ticker` är tom i `kalshi_markets_full.jsonl`:
- Kalshi API returnerar markets med `ticker` field
- Kontrollera att API-anropet fungerar korrekt
- Kolla `raw` field för debugging

### Låg mapping rate för Polymarket

Om `market_to_event.json` har få mappings:
- Kontrollera att `polymarket_events_minimal.json` har korrekta `synthetic_event_id` (pm_<slug>)
- Kolla `market_to_event.miss.json` för att se varför markets missade
- Förbättra `extract_event_candidates()` i `polymarket_build_market_to_event.py`

## Definition of Done

✅ **Kalshi market-level data:**
- `kalshi_markets_full.jsonl` innehåller market_ticker (inte tom) för ~100% av fetched markets
- Minst ett prisfält (bid/ask/last) per market

✅ **Polymarket market→event mapping:**
- Mapping rate ≥ nuvarande (319/500) med miss-fil
- `market_to_event.miss.json` innehåller alla debug-fält (events[0].ticker, slug, seriesSlug, etc.)

✅ **Market docs:**
- Båda market_docs JSONL genereras med market_id, event_id, text
- Kalshi market_docs har market_id coverage ~100% för fetched markets
- Polymarket market_docs har event_id coverage = mapped/total

✅ **Bridge coverage:**
- Minst X bridgade eventpar (från top-1 candidates)
- Minst Y markets på båda sidor under bridgade events
- sanity_check visar bridge coverage, inte direkt event_id overlap

## Nästa steg

Efter att market_docs är byggda kan du:
1. Köra contract-level matching (PHASE 4)
2. Bygga arbitrage-scanner (PHASE 5)
3. Använda market_docs för embedding-based contract matching
