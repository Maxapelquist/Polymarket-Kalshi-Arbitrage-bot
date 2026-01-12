# AI-Ready Market Schema

Detta dokument beskriver det strukturerade format som är förberett för AI-analys.

## Filosofi

> **"This branch observes Kalshi reality. It does not attempt to understand it."**

Detta schema är **mekaniskt genererat** från rådata. Ingen tolkning. Ingen AI. Bara normalisering.

## Pipeline

```
Kalshi API → RawMarketObservation → MarketForAI → JSONL
```

1. **RAW** (`data/kalshi/raw/kalshi_raw_markets.json`)
   - Direkt från API
   - Komplett dump med all metadata
   - JSON-format (pretty printed)

2. **STRUCTURED** (`data/kalshi/structured/kalshi_markets_structured.jsonl`)
   - Mekaniskt normaliserad
   - Ett market per rad (JSONL)
   - Redo för streaming till LLM

## MarketForAI Schema

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

### Fält

| Fält | Typ | Beskrivning | Exempel |
|------|-----|-------------|---------|
| `source` | string | Platform (alltid "kalshi" i denna branch) | `"kalshi"` |
| `market_ticker` | string | Unik market-identifierare | `"KXEPLGAME-25JAN13CHEARS-CHE"` |
| `event_ticker` | string | Event som marknaden tillhör | `"KXEPLGAME-25JAN13CHEARS"` |
| `series_ticker` | string | Serie/kategori | `"KXEPLGAME"` |
| `event_text` | string | Vad marknaden förutspår (ren text) | `"Will Chelsea win vs Arsenal on Jan 13?"` |
| `rules_text` | string? | Regler och förtydliganden | `"Regular time + injury time"` |
| `category` | string? | Kategori (för framtida användning) | `null` |
| `time_window` | object | När marknaden är öppen | Se nedan |
| `status` | string | Marknadsstatus | `"open"` / `"closed"` |
| `raw_json` | object | Komplett rådata-referens | `{ "event": {...}, "market": {...} }` |

### TimeWindow

```json
{
  "open": "2025-01-10T12:00:00Z",
  "close": "2025-01-13T19:00:00Z"
}
```

Båda fälten är `Option<String>` (kan vara null).

## Användning för AI (nästa branch)

Detta format är designat för:

1. **LLM-ingång**: Varje rad i JSONL kan feedas direkt till en LLM
2. **Streaming**: Kan processas en market i taget (inte hela filen i minnet)
3. **Gruppering**: AI kan läsa `event_text` + `rules_text` och förstå vad marknaden handlar om
4. **Matchning**: AI kan jämföra markets och hitta semantiska kopplingar

### Exempel på AI-prompt (nästa branch)

```
Du får följande marknad från Kalshi:

{
  "event_text": "Will Chelsea win vs Arsenal on Jan 13?",
  "rules_text": "Premier League match | Regular time + injury time",
  "time_window": { "open": "...", "close": "..." }
}

Uppgift:
1. Beskriv vad denna marknad handlar om
2. Identifiera nyckelbegrepp (lag, datum, liga, regler)
3. Föreslå relaterade marknader som skulle kunna matcha
```

**Men det är INTE i denna branch.**

## Format-konventioner

- **JSONL**: En JSON-object per rad, inga kommatecken mellan rader
- **UTF-8**: All text är UTF-8
- **ISO 8601**: Alla timestamps i ISO 8601-format
- **Null-hantering**: Saknade fält är `null`, inte tomma strängar

## Validering

För att validera strukturerade data:

```bash
# Räkna rader (antal markets)
wc -l data/kalshi/structured/kalshi_markets_structured.jsonl

# Visa första market (pretty printed)
head -1 data/kalshi/structured/kalshi_markets_structured.jsonl | jq .

# Kontrollera att alla har 'source' = 'kalshi'
cat data/kalshi/structured/kalshi_markets_structured.jsonl | jq -r .source | sort | uniq
```

## Nästa steg

I nästa branch kommer vi att:

1. ✅ Läsa `kalshi_markets_structured.jsonl`
2. ✅ Mata varje market till state-of-the-art LLM
3. ✅ Låta LLM:en gruppera markets baserat på semantisk förståelse
4. ✅ Bygga probabilistisk matchning

Men först måste denna data vara perfekt strukturerad.

---

**Status:** ✅ Schema definierat och implementerat  
**Branch:** `raw-kalshi-v4-clean`  
**Nästa:** AI-driven market understanding
