#!/usr/bin/env python3
"""
Sanity checks för market_docs med bridge coverage.
Kontrollerar:
  - Counts
  - Top 10 events med flest markets (per sida)
  - Bridge coverage (via event_bridge.json)
  - Exempel på bridgade eventpar
"""

import json
import sys
import os
from collections import defaultdict, Counter
from typing import Dict, List, Set, Any

def load_jsonl(path: str) -> List[Dict]:
    """Ladda JSONL-fil"""
    if not os.path.exists(path):
        return []
    docs = []
    with open(path, "r") as f:
        for line in f:
            if not line.strip():
                continue
            docs.append(json.loads(line))
    return docs

def load_bridge() -> Dict[str, Dict[str, Any]]:
    """Ladda event bridge mapping (prioritera LLM bridges, fallback till embedding bridges)"""
    # Prioritera LLM bridges
    bridge_path = "data/matching/event_bridge.llm.json"
    if not os.path.exists(bridge_path):
        # Fallback till embedding bridges
        bridge_path = "data/matching/event_bridge.json"
        if not os.path.exists(bridge_path):
            return {}
    
    with open(bridge_path, "r") as f:
        bridges = json.load(f)
    
    # Bygg lookup: kalshi_event_ticker -> bridge
    lookup = {}
    for bridge in bridges:
        kalshi_ticker = bridge.get("kalshi_event_ticker", "")
        if kalshi_ticker:
            lookup[kalshi_ticker] = bridge
    
    return lookup

def main():
    polymarket_path = "data/polymarket/derived/polymarket_market_docs.jsonl"
    kalshi_path = "data/kalshi/derived/kalshi_market_docs.jsonl"
    bridge_path = "data/matching/event_bridge.json"
    
    print("🔍 Sanity Check: Market Docs (via Bridge)\n", file=sys.stderr)
    
    # Ladda docs
    polymarket_docs = load_jsonl(polymarket_path)
    kalshi_docs = load_jsonl(kalshi_path)
    
    # Ladda bridge
    bridge_lookup = load_bridge()
    
    # Counts
    print("📊 Counts:", file=sys.stderr)
    print(f"   Polymarket docs: {len(polymarket_docs)}", file=sys.stderr)
    print(f"   Kalshi docs: {len(kalshi_docs)}", file=sys.stderr)
    print(f"   Total: {len(polymarket_docs) + len(kalshi_docs)}", file=sys.stderr)
    
    # Bridge coverage
    print(f"\n🌉 Bridge Coverage:", file=sys.stderr)
    print(f"   Bridgade eventpar: {len(bridge_lookup)}", file=sys.stderr)
    
    # Räkna markets under bridgade events
    kalshi_bridged_events = set(bridge_lookup.keys())
    pm_bridged_events = set(bridge.get("pm_event_id", "") for bridge in bridge_lookup.values())
    pm_bridged_events.discard("")  # Ta bort tomma
    
    kalshi_markets_under_bridge = sum(
        1 for d in kalshi_docs 
        if d.get("event_id") in kalshi_bridged_events
    )
    pm_markets_under_bridge = sum(
        1 for d in polymarket_docs 
        if d.get("event_id") in pm_bridged_events
    )
    
    print(f"   Kalshi markets under bridgade events: {kalshi_markets_under_bridge}/{len(kalshi_docs)} ({kalshi_markets_under_bridge/len(kalshi_docs)*100:.1f}%)" if kalshi_docs else "   Kalshi markets under bridgade events: 0/0", file=sys.stderr)
    print(f"   Polymarket markets under bridgade events: {pm_markets_under_bridge}/{len(polymarket_docs)} ({pm_markets_under_bridge/len(polymarket_docs)*100:.1f}%)" if polymarket_docs else "   Polymarket markets under bridgade events: 0/0", file=sys.stderr)
    
    # Top 10 events med flest markets (Polymarket)
    polymarket_event_counts = Counter(d["event_id"] for d in polymarket_docs if d.get("event_id"))
    print(f"\n🏆 Top 10 Polymarket events (flest markets):", file=sys.stderr)
    for event_id, count in polymarket_event_counts.most_common(10):
        print(f"   {event_id}: {count} markets", file=sys.stderr)
    
    # Top 10 events med flest markets (Kalshi)
    kalshi_event_counts = Counter(d["event_id"] for d in kalshi_docs if d.get("event_id"))
    print(f"\n🏆 Top 10 Kalshi events (flest markets):", file=sys.stderr)
    for event_id, count in kalshi_event_counts.most_common(10):
        print(f"   {event_id}: {count} markets", file=sys.stderr)
    
    # Exempel på bridgade eventpar
    if bridge_lookup:
        # Hämta event titles
        kalshi_events_path = "data/kalshi/derived/kalshi_events_minimal.json"
        pm_events_path = "data/polymarket/derived/polymarket_events_minimal.json"
        
        kalshi_titles = {}
        if os.path.exists(kalshi_events_path):
            with open(kalshi_events_path, "r") as f:
                events = json.load(f)
                for event in events:
                    ticker = event.get("event_ticker", "")
                    title = event.get("title", "")
                    if ticker:
                        kalshi_titles[ticker] = title
        
        pm_titles = {}
        if os.path.exists(pm_events_path):
            with open(pm_events_path, "r") as f:
                events = json.load(f)
                for event in events:
                    event_id = event.get("synthetic_event_id", "")
                    title = event.get("title", "") or (event.get("event_titles", [""])[0] if event.get("event_titles") else "")
                    if event_id:
                        pm_titles[event_id] = title
        
        print(f"\n✅ Exempel på bridgade eventpar (10):", file=sys.stderr)
        sorted_bridges = sorted(
            bridge_lookup.values(),
            key=lambda x: x.get("score", 0.0),
            reverse=True
        )[:10]
        
        for bridge in sorted_bridges:
            kalshi_ticker = bridge.get("kalshi_event_ticker", "")
            pm_event_id = bridge.get("pm_event_id", "")
            score = bridge.get("score", 0.0)
            
            kalshi_title = kalshi_titles.get(kalshi_ticker, "")[:60]
            pm_title = pm_titles.get(pm_event_id, "")[:60]
            
            kalshi_count = kalshi_event_counts.get(kalshi_ticker, 0)
            pm_count = polymarket_event_counts.get(pm_event_id, 0)
            
            print(f"   {kalshi_ticker} ({kalshi_count} markets) ↔ {pm_event_id} ({pm_count} markets) [score: {score:.3f}]", file=sys.stderr)
            if kalshi_title:
                print(f"      Kalshi: {kalshi_title}", file=sys.stderr)
            if pm_title:
                print(f"      PM: {pm_title}", file=sys.stderr)
    else:
        print(f"\n⚠️  Ingen bridge mapping hittades", file=sys.stderr)
        print(f"   Kör först: python3 scripts/build_event_bridge.py", file=sys.stderr)
    
    # Kvalitetskontroller
    print(f"\n🔍 Kvalitetskontroller:", file=sys.stderr)
    
    # Polymarket: hur många har event_id?
    pm_with_event = sum(1 for d in polymarket_docs if d.get("event_id"))
    pm_without_event = len(polymarket_docs) - pm_with_event
    print(f"   Polymarket med event_id: {pm_with_event}/{len(polymarket_docs)} ({pm_with_event/len(polymarket_docs)*100:.1f}%)" if polymarket_docs else "   Polymarket med event_id: 0/0", file=sys.stderr)
    if pm_without_event > 0:
        print(f"   Polymarket utan event_id: {pm_without_event}", file=sys.stderr)
    
    # Kalshi: hur många har event_id?
    kalshi_with_event = sum(1 for d in kalshi_docs if d.get("event_id"))
    kalshi_without_event = len(kalshi_docs) - kalshi_with_event
    print(f"   Kalshi med event_id: {kalshi_with_event}/{len(kalshi_docs)} ({kalshi_with_event/len(kalshi_docs)*100:.1f}%)" if kalshi_docs else "   Kalshi med event_id: 0/0", file=sys.stderr)
    if kalshi_without_event > 0:
        print(f"   Kalshi utan event_id: {kalshi_without_event}", file=sys.stderr)
    
    # Kontrollera tomma market_id
    pm_empty_market_id = sum(1 for d in polymarket_docs if not d.get("market_id"))
    kalshi_empty_market_id = sum(1 for d in kalshi_docs if not d.get("market_id"))
    
    if pm_empty_market_id > 0:
        print(f"   ⚠️  Polymarket med tom market_id: {pm_empty_market_id}", file=sys.stderr)
    if kalshi_empty_market_id > 0:
        print(f"   ⚠️  Kalshi med tom market_id: {kalshi_empty_market_id}", file=sys.stderr)
    
    print(f"\n✅ Sanity check klar", file=sys.stderr)

if __name__ == "__main__":
    main()
