#!/usr/bin/env python3
"""
Bygg market_docs på market-nivå för båda sidor.
Output:
  - data/polymarket/derived/polymarket_market_docs.jsonl
  - data/kalshi/derived/kalshi_market_docs.jsonl
"""

import json
import sys
import os
from typing import Dict, Any, List

def get_polymarket_market_id(market: Dict[str, Any]) -> str:
    """Extrahera stabilt market_id från Polymarket market"""
    if market.get("conditionId"):
        return market["conditionId"]
    if market.get("slug"):
        return market["slug"]
    return str(market.get("id", "unknown"))

def build_polymarket_docs(
    markets_path: str,
    mapping_path: str,
    events_path: str
) -> List[Dict[str, Any]]:
    """Bygg Polymarket market_docs"""
    print("📊 Bygger Polymarket market_docs...", file=sys.stderr)
    
    # Läs markets
    with open(markets_path, "r") as f:
        markets = json.load(f)
    
    # Läs market→event mapping
    market_to_event = {}
    if os.path.exists(mapping_path):
        with open(mapping_path, "r") as f:
            mapping_data = json.load(f)
            market_to_event = {m["market_id"]: m["event_id"] for m in mapping_data.values()}
    
    # Läs events för att få event titles
    event_titles = {}
    if os.path.exists(events_path):
        with open(events_path, "r") as f:
            events = json.load(f)
            for event in events:
                event_id = event.get("synthetic_event_id") or event.get("event_id", "")
                title = event.get("title") or (event.get("event_titles", [""])[0] if event.get("event_titles") else "")
                if event_id and title:
                    event_titles[event_id] = title
    
    docs = []
    for market in markets:
        market_id = get_polymarket_market_id(market)
        event_id = market_to_event.get(market_id, "")
        
        # Extrahera fält
        title = market.get("question") or market.get("title", "")
        subtitle = market.get("description", "")[:200]  # Begränsa längd
        category = market.get("category", "")
        
        # Hämta event title om finns
        event_title = event_titles.get(event_id, "")
        
        # Bygg text
        text_parts = []
        if title:
            text_parts.append(title)
        if subtitle:
            text_parts.append(subtitle)
        if category:
            text_parts.append(f"Category: {category}")
        if event_title:
            text_parts.append(f"Event: {event_title}")
        
        slug = market.get("slug") or market.get("events", [{}])[0].get("slug", "")
        if slug:
            text_parts.append(f"Slug: {slug}")
        
        text = " | ".join(text_parts)
        
        docs.append({
            "market_id": market_id,
            "event_id": event_id,
            "title": title,
            "subtitle": subtitle,
            "category": category,
            "text": text
        })
    
    print(f"   ✅ {len(docs)} Polymarket market_docs", file=sys.stderr)
    return docs

def build_kalshi_docs(markets_path: str) -> List[Dict[str, Any]]:
    """Bygg Kalshi market_docs från JSONL"""
    print("📊 Bygger Kalshi market_docs...", file=sys.stderr)
    
    if not os.path.exists(markets_path):
        print(f"   ⚠️  {markets_path} finns inte - hoppar över", file=sys.stderr)
        return []
    
    docs = []
    event_titles = {}  # Cache för event titles
    
    # Läs event titles från events_minimal.json
    events_path = "data/kalshi/derived/kalshi_events_minimal.json"
    if os.path.exists(events_path):
        with open(events_path, "r") as f:
            events = json.load(f)
            for event in events:
                event_ticker = event.get("event_ticker", "")
                title = event.get("title", "")
                if event_ticker and title:
                    event_titles[event_ticker] = title
    
    # Läs markets från JSONL
    with open(markets_path, "r") as f:
        for line in f:
            if not line.strip():
                continue
            market = json.loads(line)
            
            market_id = market.get("market_ticker", "")
            event_id = market.get("event_ticker", "")
            title = market.get("title", "")
            subtitle = market.get("subtitle", "")
            category = market.get("category", "")
            
            # Hämta event title
            event_title = event_titles.get(event_id, "")
            
            # Bygg text
            text_parts = []
            if title:
                text_parts.append(title)
            if subtitle:
                text_parts.append(subtitle)
            if category:
                text_parts.append(f"Category: {category}")
            if event_title:
                text_parts.append(f"Event: {event_title}")
            
            text = " | ".join(text_parts)
            
            docs.append({
                "market_id": market_id,
                "event_id": event_id,
                "title": title,
                "subtitle": subtitle,
                "category": category,
                "text": text
            })
    
    print(f"   ✅ {len(docs)} Kalshi market_docs", file=sys.stderr)
    return docs

def main():
    # Polymarket
    polymarket_markets_path = "data/polymarket/raw/markets_full.json"
    polymarket_mapping_path = "data/polymarket/derived/market_to_event.json"
    polymarket_events_path = "data/polymarket/derived/polymarket_events_minimal.json"
    
    polymarket_docs = build_polymarket_docs(
        polymarket_markets_path,
        polymarket_mapping_path,
        polymarket_events_path
    )
    
    # Kalshi
    kalshi_markets_path = "data/kalshi/derived/kalshi_markets_full.jsonl"
    kalshi_docs = build_kalshi_docs(kalshi_markets_path)
    
    # Spara Polymarket docs
    os.makedirs("data/polymarket/derived", exist_ok=True)
    polymarket_output = "data/polymarket/derived/polymarket_market_docs.jsonl"
    with open(polymarket_output, "w") as f:
        for doc in polymarket_docs:
            f.write(json.dumps(doc) + "\n")
    
    # Spara Kalshi docs
    os.makedirs("data/kalshi/derived", exist_ok=True)
    kalshi_output = "data/kalshi/derived/kalshi_market_docs.jsonl"
    with open(kalshi_output, "w") as f:
        for doc in kalshi_docs:
            f.write(json.dumps(doc) + "\n")
    
    # Statistik
    print(f"\n📊 Statistik:", file=sys.stderr)
    print(f"   Polymarket docs: {len(polymarket_docs)}", file=sys.stderr)
    polymarket_with_event = sum(1 for d in polymarket_docs if d["event_id"])
    print(f"   Polymarket med event_id: {polymarket_with_event} ({polymarket_with_event/len(polymarket_docs)*100:.1f}%)" if polymarket_docs else "   Polymarket med event_id: 0", file=sys.stderr)
    
    print(f"   Kalshi docs: {len(kalshi_docs)}", file=sys.stderr)
    kalshi_with_event = sum(1 for d in kalshi_docs if d["event_id"])
    print(f"   Kalshi med event_id: {kalshi_with_event} ({kalshi_with_event/len(kalshi_docs)*100:.1f}%)" if kalshi_docs else "   Kalshi med event_id: 0", file=sys.stderr)
    
    # Sample 5 rader från varje
    print(f"\n📝 Sample Polymarket docs (5):", file=sys.stderr)
    for doc in polymarket_docs[:5]:
        print(f"   {json.dumps(doc, ensure_ascii=False)}", file=sys.stderr)
    
    print(f"\n📝 Sample Kalshi docs (5):", file=sys.stderr)
    for doc in kalshi_docs[:5]:
        print(f"   {json.dumps(doc, ensure_ascii=False)}", file=sys.stderr)
    
    print(f"\n✅ Saved: {polymarket_output}", file=sys.stderr)
    print(f"✅ Saved: {kalshi_output}", file=sys.stderr)

if __name__ == "__main__":
    main()
