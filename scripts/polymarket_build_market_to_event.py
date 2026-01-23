#!/usr/bin/env python3
"""
Bygg robust market→event mapping för Polymarket.
Input:
  - data/polymarket/raw/markets_full.json
Output:
  - data/polymarket/derived/market_to_event.json (final)
  - data/polymarket/derived/market_to_event.miss.json (missed markets)
"""

import json
import sys
import os
import re
from typing import Dict, List, Any, Optional, Tuple
from collections import defaultdict, Counter

def normalize_slug(slug: str) -> str:
    """
    Normalisera slug till pm_<slug> format.
    - Lowercase
    - Trim whitespace
    - Byt whitespace → -
    - Ta bort prefix/suffix
    """
    if not slug:
        return ""
    
    # Lowercase och trim
    slug = slug.lower().strip()
    
    # Byt whitespace och special chars till -
    slug = re.sub(r'[\s_]+', '-', slug)
    
    # Ta bort prefix pm_ om redan finns
    if slug.startswith("pm_"):
        slug = slug[3:]
    
    # Ta bort leading/trailing -
    slug = slug.strip('-')
    
    return f"pm_{slug}" if slug else ""

def get_market_id(market: Dict[str, Any]) -> str:
    """
    Extrahera stabilt market_id från market-objektet.
    VIKTIGT: Returnerar alltid en icke-tom sträng.
    """
    # Prioritera conditionId (mest stabilt)
    condition_id = market.get("conditionId", "")
    if condition_id:
        return condition_id
    
    # Fallback till id
    market_id = market.get("id")
    if market_id:
        return str(market_id)
    
    # Fallback till slug
    slug = market.get("slug", "")
    if slug:
        return slug
    
    # Absolute fallback
    return "unknown_market"

def build_event_lookup_from_markets(markets: List[Dict[str, Any]]) -> Dict[str, str]:
    """
    Bygg event_lookup direkt från markets.
    För varje market:
    - event = market.get("events")[0] om finns
    - event_id = normalize_slug(event.get("slug") or event.get("ticker") or market.get("seriesSlug") or market.get("slug"))
    - Lagra mapping: key->event_id för alla dessa keys:
        a) event.slug
        b) event.ticker
        c) market.seriesSlug
        d) event.series[0].slug (om finns)
        e) market.slug
    
    Returnerar: lookup dict där key -> event_id (normalized)
    """
    lookup = {}
    
    for market in markets:
        # Hämta event från events[0]
        market_events = market.get("events", [])
        if not market_events:
            continue
        
        event = market_events[0]
        
        # Bestäm event_id (normalized) - prioritet:
        # 1. event.slug
        # 2. event.ticker
        # 3. market.seriesSlug
        # 4. market.slug
        event_id = None
        event_slug = event.get("slug", "")
        event_ticker = event.get("ticker", "")
        series_slug = market.get("seriesSlug", "")
        market_slug = market.get("slug", "")
        
        if event_slug:
            event_id = normalize_slug(event_slug)
        elif event_ticker:
            event_id = normalize_slug(event_ticker)
        elif series_slug:
            event_id = normalize_slug(series_slug)
        elif market_slug:
            event_id = normalize_slug(market_slug)
        
        if not event_id:
            continue
        
        # Hämta series (säkerställ att det alltid är en lista)
        series = event.get("series", []) or market.get("series", []) or []
        series_0 = series[0] if series and len(series) > 0 else {}
        series_0_slug = series_0.get("slug", "") if series_0 else ""
        
        # Indexera alla möjliga keys som pekar till samma event_id
        keys_to_index = []
        
        # a) event.slug
        if event_slug:
            normalized_key = normalize_slug(event_slug)
            if normalized_key:
                keys_to_index.append(normalized_key)
        
        # b) event.ticker
        if event_ticker:
            normalized_key = normalize_slug(event_ticker)
            if normalized_key:
                keys_to_index.append(normalized_key)
        
        # c) market.seriesSlug
        if series_slug:
            normalized_key = normalize_slug(series_slug)
            if normalized_key:
                keys_to_index.append(normalized_key)
        
        # d) event.series[0].slug
        if series_0_slug:
            normalized_key = normalize_slug(series_0_slug)
            if normalized_key:
                keys_to_index.append(normalized_key)
        
        # e) market.slug
        if market_slug:
            normalized_key = normalize_slug(market_slug)
            if normalized_key:
                keys_to_index.append(normalized_key)
        
        # Lägg till alla keys i lookup
        for key in keys_to_index:
            if key and key not in lookup:
                lookup[key] = event_id
    
    return lookup

def extract_event_candidates(market: Dict[str, Any]) -> List[Tuple[str, str]]:
    """
    Extrahera alla möjliga event-identifiers från ett market-objekt.
    Returnerar lista med (normalized_event_id, source) tuples.
    """
    candidates = []
    
    events = market.get("events", [])
    if events and len(events) > 0:
        event = events[0]
        
        # 1. events[0].ticker (högsta prioritet)
        if event.get("ticker"):
            normalized = normalize_slug(event["ticker"])
            if normalized:
                candidates.append((normalized, "events[0].ticker"))
        
        # 2. events[0].slug
        if event.get("slug"):
            normalized = normalize_slug(event["slug"])
            if normalized:
                candidates.append((normalized, "events[0].slug"))
        
        # 3. events[0].series[0].slug
        series = event.get("series", []) or []
        if series and len(series) > 0:
            series_slug = series[0].get("slug", "")
            if series_slug:
                normalized = normalize_slug(series_slug)
                if normalized:
                    candidates.append((normalized, "events[0].series[0].slug"))
    
    # 4. seriesSlug (på market-nivå)
    if market.get("seriesSlug"):
        normalized = normalize_slug(market["seriesSlug"])
        if normalized:
            candidates.append((normalized, "seriesSlug"))
    
    # 5. slug (market slug) - lägre prioritet
    if market.get("slug"):
        normalized = normalize_slug(market["slug"])
        if normalized:
            candidates.append((normalized, "market.slug"))
    
    # Ta bort duplicater (behåll första förekomsten)
    seen = set()
    unique_candidates = []
    for candidate in candidates:
        if candidate[0] not in seen:
            seen.add(candidate[0])
            unique_candidates.append(candidate)
    
    return unique_candidates

def match_market_to_event(
    market: Dict[str, Any],
    event_lookup: Dict[str, str]
) -> Tuple[Optional[str], str, Dict[str, Any]]:
    """
    Matcha ett market mot ett event.
    Returnerar: (event_id, reason, debug_fields)
    """
    market_id = get_market_id(market)
    candidates = extract_event_candidates(market)
    
    # Försök matcha varje kandidat i prioritetsordning
    for candidate_id, source in candidates:
        if candidate_id in event_lookup:
            event_id = event_lookup[candidate_id]
            return event_id, f"matched_via_{source}", {}
    
    # Om ingen match: bygg debug_fields
    events = market.get("events", [])
    event_0 = events[0] if events else {}
    
    # Säkerställ att series alltid är en lista
    series = event_0.get("series", []) or market.get("series", []) or []
    series_0 = series[0] if series and len(series) > 0 else {}
    
    debug_fields = {
        "events[0].ticker": event_0.get("ticker", ""),
        "events[0].slug": event_0.get("slug", ""),
        "events[0].series[0].slug": series_0.get("slug", "") if series_0 else "",
        "seriesSlug": market.get("seriesSlug", ""),
        "slug": market.get("slug", ""),
        "conditionId": market.get("conditionId", ""),
        "id": str(market.get("id", ""))
    }
    
    # Bygg reason
    reason_parts = []
    if candidates:
        candidate_strs = [f"{c[0]}({c[1]})" for c in candidates]
        reason_parts.append(f"candidates={','.join(candidate_strs)}")
        reason_parts.append("not_in_lookup")
    else:
        reason_parts.append("no_candidates_extracted")
    
    if events:
        reason_parts.append(f"has_{len(events)}_events")
    
    return None, "; ".join(reason_parts), debug_fields

def main():
    # Läs input-filer
    markets_path = "data/polymarket/raw/markets_full.json"
    
    if not os.path.exists(markets_path):
        print(f"ERROR: {markets_path} finns inte", file=sys.stderr)
        sys.exit(1)
    
    print("📂 Läser markets...", file=sys.stderr)
    
    with open(markets_path, "r") as f:
        markets = json.load(f)
    
    print(f"   Markets: {len(markets)}", file=sys.stderr)
    
    # Printa top-level keys från första market
    if markets:
        print(f"   Top-level keys i markets: {', '.join(sorted(markets[0].keys()))}", file=sys.stderr)
    
    # Bygg event lookup direkt från markets
    print("\n🔍 Bygger event_lookup från markets...", file=sys.stderr)
    event_lookup = build_event_lookup_from_markets(markets)
    
    print(f"\n📊 Event lookup statistik:", file=sys.stderr)
    print(f"   Total lookup keys: {len(event_lookup)}", file=sys.stderr)
    
    # Räkna unika event_id
    unique_events = set(event_lookup.values())
    print(f"   Unika events: {len(unique_events)}", file=sys.stderr)
    
    # Matcha markets
    print(f"\n🔄 Matchar markets...", file=sys.stderr)
    mappings = {}
    misses = []
    match_reasons = Counter()
    miss_reasons = Counter()
    
    for idx, market in enumerate(markets):
        if (idx + 1) % 100 == 0:
            print(f"   Processed {idx + 1}/{len(markets)} markets...", file=sys.stderr)
        
        market_id = get_market_id(market)
        event_id, reason, debug_fields = match_market_to_event(market, event_lookup)
        
        if event_id:
            mappings[market_id] = {
                "market_id": market_id,
                "event_id": event_id,
                "title": market.get("question") or market.get("title", ""),
                "reason": reason
            }
            match_reasons[reason] += 1
        else:
            miss_entry = {
                "market_id": market_id,
                "title": market.get("question") or market.get("title", ""),
                "reason": reason,
                "debug_fields": debug_fields
            }
            misses.append(miss_entry)
            miss_reasons[reason] += 1
    
    # Spara resultat
    os.makedirs("data/polymarket/derived", exist_ok=True)
    
    output_path = "data/polymarket/derived/market_to_event.json"
    with open(output_path, "w") as f:
        json.dump(mappings, f, indent=2)
    
    miss_path = "data/polymarket/derived/market_to_event.miss.json"
    with open(miss_path, "w") as f:
        json.dump(misses, f, indent=2)
    
    # Statistik
    total = len(markets)
    matched = len(mappings)
    missed = len(misses)
    match_rate = (matched / total * 100) if total > 0 else 0
    
    print(f"\n📊 Statistik:", file=sys.stderr)
    print(f"   Total markets: {total}", file=sys.stderr)
    print(f"   Matched: {matched} ({match_rate:.1f}%)", file=sys.stderr)
    print(f"   Missed: {missed} ({100-match_rate:.1f}%)", file=sys.stderr)
    
    # Top 20 miss reasons
    print(f"\n❌ Top 20 miss reasons:", file=sys.stderr)
    for reason, count in miss_reasons.most_common(20):
        print(f"   {reason}: {count}", file=sys.stderr)
    
    # Top 20 events by #markets
    event_counts = Counter(mapping["event_id"] for mapping in mappings.values())
    print(f"\n🏆 Top 20 events med flest markets:", file=sys.stderr)
    for event_id, count in event_counts.most_common(20):
        print(f"   {event_id}: {count} markets", file=sys.stderr)
    
    # Top 20 match reasons
    print(f"\n✅ Top 20 match reasons:", file=sys.stderr)
    for reason, count in match_reasons.most_common(20):
        print(f"   {reason}: {count}", file=sys.stderr)
    
    # 10 exempel på missar
    print(f"\n📝 Exempel på missar (10):", file=sys.stderr)
    for miss in misses[:10]:
        print(f"   Title: {miss['title'][:60]}", file=sys.stderr)
        print(f"   Reason: {miss['reason']}", file=sys.stderr)
        print(f"   Debug: {json.dumps(miss['debug_fields'], ensure_ascii=False)}", file=sys.stderr)
        print(f"", file=sys.stderr)
    
    print(f"\n✅ Saved: {output_path} ({matched} mappings)", file=sys.stderr)
    print(f"✅ Saved: {miss_path} ({missed} misses)", file=sys.stderr)

if __name__ == "__main__":
    main()
