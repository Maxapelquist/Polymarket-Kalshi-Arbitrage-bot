#!/usr/bin/env python3
"""
Matcha Kalshi ↔ Polymarket events inom samma kategori med lokal LLM.
Input: event_categories.jsonl
Output: event_bridge.llm.json
"""

import json
import sys
import os
import time
from typing import Dict, List, Any, Optional, Tuple
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from local_llm_client import get_client

def load_jsonl(path: str) -> List[Dict[str, Any]]:
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

def load_event_texts(market_docs_path: str) -> Dict[str, str]:
    """Ladda event_texts från market_docs"""
    from collections import defaultdict
    
    docs = load_jsonl(market_docs_path)
    events_to_markets = defaultdict(list)
    for doc in docs:
        event_id = doc.get("event_id", "")
        if event_id:
            events_to_markets[event_id].append(doc)
    
    event_texts = {}
    for event_id, markets in events_to_markets.items():
        texts = []
        for market in markets[:10]:
            text = market.get("text", "") or market.get("title", "")
            if text:
                texts.append(text)
        event_text = " | ".join(texts) if texts else event_id
        event_texts[event_id] = event_text
    
    return event_texts

def extract_json_object(response: str) -> Optional[str]:
    """Extrahera första JSON-objektet från en LLM-respons."""
    if not response:
        return None
    response = response.strip()
    if response.startswith("```"):
        response = response.strip("`")
        response = response.replace("json", "", 1).strip()

    start = response.find("{")
    if start == -1:
        return None

    depth = 0
    in_string = False
    escape = False
    for i in range(start, len(response)):
        ch = response[i]
        if ch == "\\" and in_string:
            escape = not escape
            continue
        if ch == "\"" and not escape:
            in_string = not in_string
        escape = False
        if in_string:
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return response[start:i + 1]
    return None

def parse_llm_json(response: str) -> Optional[Dict[str, Any]]:
    payload = extract_json_object(response)
    if not payload:
        return None
    try:
        return json.loads(payload)
    except json.JSONDecodeError:
        return None

def normalize_end_time(value: Optional[str]) -> Optional[str]:
    if not value:
        return None
    return value.split("T")[0]

def load_event_end_times(market_docs_path: str) -> Dict[str, List[str]]:
    docs = load_jsonl(market_docs_path)
    end_times: Dict[str, List[str]] = defaultdict(list)
    for doc in docs:
        event_id = doc.get("event_id", "")
        end_time = normalize_end_time(doc.get("end_time") or doc.get("close_time"))
        if not event_id or not end_time:
            continue
        if end_time not in end_times[event_id]:
            end_times[event_id].append(end_time)
    return end_times

def build_end_time_index(event_end_times: Dict[str, List[str]]) -> Dict[str, List[str]]:
    index: Dict[str, List[str]] = defaultdict(list)
    for event_id, end_times in event_end_times.items():
        for end_time in end_times:
            index[end_time].append(event_id)
    return index

def pair_decision(
    kalshi_event_id: str,
    kalshi_text: str,
    pm_event_id: str,
    pm_text: str,
    client
) -> Dict[str, Any]:
    """
    Steg B: Pair decision - LLM avgör om events matchar.
    """
    prompt = f"""Determine if these two events refer to the same real-world event.

Kalshi event:
{kalshi_text}

Polymarket event:
{pm_text}

Respond with ONLY valid JSON (no markdown):
{{"match": true/false, "confidence": 0.0, "why_short": "...", "fields_aligned": ["..."], "fields_conflict": ["..."]}}

Rules:
- match: true if same event, false otherwise
- confidence: 0.0 to 1.0
- why_short: brief explanation (max 30 words)
- fields_aligned: list of matching aspects
- fields_conflict: list of conflicting aspects
"""
    
    messages = [
        {"role": "system", "content": "You are a precise event matcher. Always respond with valid JSON only."},
        {"role": "user", "content": prompt}
    ]
    
    try:
        response = client.chat(messages)
        result = parse_llm_json(response)
        if not isinstance(result, dict):
            raise ValueError("Missing or invalid JSON payload")
        result["kalshi_event_id"] = kalshi_event_id
        result["pm_event_id"] = pm_event_id
        return result
    except Exception as e:
        print(f"  ⚠️  Pair decision failed: {e}", file=sys.stderr)
        return {
            "match": False,
            "confidence": 0.0,
            "why_short": f"Error: {str(e)}",
            "fields_aligned": [],
            "fields_conflict": [],
            "kalshi_event_id": kalshi_event_id,
            "pm_event_id": pm_event_id
        }

def main():
    # Läs kategorier
    categories_path = "config/kalshi_categories.json"
    with open(categories_path, "r") as f:
        categories = json.load(f)
    
    print("📂 Läser event categories...", file=sys.stderr)
    
    # Ladda categories
    pm_categories = load_jsonl("data/matching/pm_event_categories.jsonl")
    kalshi_categories = load_jsonl("data/matching/kalshi_event_categories.jsonl")
    
    # Ladda event texts
    pm_docs_path = "data/polymarket/derived/polymarket_market_docs.jsonl"
    kalshi_docs_path = "data/kalshi/derived/kalshi_market_docs.jsonl"
    pm_texts = load_event_texts(pm_docs_path)
    kalshi_texts = load_event_texts(kalshi_docs_path)
    pm_end_times = load_event_end_times(pm_docs_path)
    kalshi_end_times = load_event_end_times(kalshi_docs_path)
    pm_end_time_index = build_end_time_index(pm_end_times)

    max_candidates = int(os.getenv("MAX_TIME_CANDIDATES", "10"))
    allow_no_time_match = os.getenv("ALLOW_NO_TIME_MATCH", "0") == "1"
    
    # Gruppera events per kategori
    pm_by_category = defaultdict(list)
    for cat in pm_categories:
        category = cat.get("category", "")
        event_id = cat.get("event_id", "")
        if category and event_id:
            pm_by_category[category].append(event_id)
    
    kalshi_by_category = defaultdict(list)
    for cat in kalshi_categories:
        category = cat.get("category", "")
        event_id = cat.get("event_id", "")
        if category and event_id:
            kalshi_by_category[category].append(event_id)
    
    print(f"   Polymarket events per kategori: {dict(pm_by_category)}", file=sys.stderr)
    print(f"   Kalshi events per kategori: {dict(kalshi_by_category)}", file=sys.stderr)
    
    # Initiera LLM client
    try:
        client = get_client()
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)
    
    # Matcha per kategori
    os.makedirs("data/matching", exist_ok=True)
    bridge_output = "data/matching/event_bridge.llm.json"
    low_conf_output = "data/matching/event_bridge.llm.low_conf.json"
    
    # Checkpoint: ladda redan matchade bridges
    high_conf_bridges = []
    low_conf_bridges = []
    pm_matched = set()  # One-to-one constraint
    
    if os.path.exists(bridge_output):
        try:
            with open(bridge_output, "r") as f:
                existing_bridges = json.load(f)
                high_conf_bridges = existing_bridges
                for bridge in existing_bridges:
                    pm_matched.add(bridge.get("pm_event_id", ""))
            print(f"   Laddade {len(high_conf_bridges)} befintliga bridges", file=sys.stderr)
        except:
            pass
    
    if os.path.exists(low_conf_output):
        try:
            with open(low_conf_output, "r") as f:
                low_conf_bridges = json.load(f)
        except:
            pass
    
    stats = defaultdict(int)
    
    print(f"\n🔄 Matchar events per kategori...", file=sys.stderr)
    
    for category in categories:
        kalshi_events = kalshi_by_category.get(category, [])
        pm_events = pm_by_category.get(category, [])
        
        if not kalshi_events or not pm_events:
            continue
        
        print(f"\n   Kategori: {category} ({len(kalshi_events)} Kalshi, {len(pm_events)} Polymarket)", file=sys.stderr)
        
        for kalshi_id in kalshi_events:
            # Checkpoint: hoppa över om redan matchad
            already_matched = any(b.get("kalshi_event_ticker") == kalshi_id for b in high_conf_bridges)
            if already_matched:
                continue
            
            kalshi_text = kalshi_texts.get(kalshi_id, kalshi_id)
            kalshi_time_values = kalshi_end_times.get(kalshi_id, [])

            pm_candidates = []
            for end_time in kalshi_time_values:
                for pm_id in pm_end_time_index.get(end_time, []):
                    if pm_id in pm_matched:
                        continue
                    if pm_id not in pm_events:
                        continue
                    pm_candidates.append(pm_id)

            if not pm_candidates and allow_no_time_match:
                pm_candidates = [pid for pid in pm_events if pid not in pm_matched]

            if not pm_candidates:
                low_conf_entry = {
                    "kalshi_event_ticker": kalshi_id,
                    "category": category,
                    "top3_candidates": [],
                    "reason": "no_time_aligned_candidates"
                }
                low_conf_bridges.append(low_conf_entry)
                stats[f"low_conf_{category}"] += 1
                continue

            pm_candidates = pm_candidates[:max_candidates]
            
            # Steg B: Pair decision för top-3
            best_match = None
            best_confidence = 0.0
            
            for pm_id in pm_candidates:
                if pm_id in pm_matched:
                    continue
                
                pm_text = pm_texts.get(pm_id, pm_id)
                decision = pair_decision(kalshi_id, kalshi_text, pm_id, pm_text, client)
                
                if decision.get("match") and decision.get("confidence", 0.0) >= 0.80:
                    conf = decision.get("confidence", 0.0)
                    if conf > best_confidence:
                        best_match = {
                            "kalshi_event_ticker": kalshi_id,
                            "pm_event_id": pm_id,
                            "category": category,
                            "confidence": conf,
                            "method": "llm_pair",
                            "model": os.getenv("LOCAL_LLM_MODEL", "unknown"),
                            "ts": str(int(time.time())),
                            "kalshi_title": kalshi_text[:100],
                            "pm_title": pm_text[:100],
                            "why_short": decision.get("why_short", ""),
                            "fields_aligned": decision.get("fields_aligned", []),
                            "fields_conflict": decision.get("fields_conflict", [])
                        }
                        best_confidence = conf
            
            if best_match:
                high_conf_bridges.append(best_match)
                pm_matched.add(best_match["pm_event_id"])
                stats[f"matched_{category}"] += 1
                
                # Spara incrementally (atomiskt)
                tmp_bridge = bridge_output + ".tmp"
                with open(tmp_bridge, "w") as f:
                    json.dump(high_conf_bridges, f, indent=2)
                os.replace(tmp_bridge, bridge_output)
            else:
                # Low confidence
                low_conf_entry = {
                    "kalshi_event_ticker": kalshi_id,
                    "category": category,
                    "top3_candidates": [
                        {"pm_event_id": pid}
                        for pid in pm_candidates[:3]
                    ],
                    "reason": "no_match_above_threshold"
                }
                low_conf_bridges.append(low_conf_entry)
                stats[f"low_conf_{category}"] += 1
                
                # Spara incrementally (atomiskt)
                tmp_low = low_conf_output + ".tmp"
                with open(tmp_low, "w") as f:
                    json.dump(low_conf_bridges, f, indent=2)
                os.replace(tmp_low, low_conf_output)
    
    # Spara resultat (atomiskt: tmp → rename)
    tmp_bridge = bridge_output + ".tmp"
    with open(tmp_bridge, "w") as f:
        json.dump(high_conf_bridges, f, indent=2)
    os.rename(tmp_bridge, bridge_output)
    
    tmp_low = low_conf_output + ".tmp"
    with open(tmp_low, "w") as f:
        json.dump(low_conf_bridges, f, indent=2)
    os.rename(tmp_low, low_conf_output)
    
    # Statistik
    stats_dict = {
        "total_bridges": len(high_conf_bridges),
        "total_low_conf": len(low_conf_bridges),
        "by_category": dict(stats),
        "mean_confidence": sum(b["confidence"] for b in high_conf_bridges) / len(high_conf_bridges) if high_conf_bridges else 0.0
    }
    
    stats_path = "data/matching/event_bridge.llm.stats.json"
    tmp_stats = stats_path + ".tmp"
    with open(tmp_stats, "w") as f:
        json.dump(stats_dict, f, indent=2)
    os.rename(tmp_stats, stats_path)
    
    print(f"\n📊 Statistik:", file=sys.stderr)
    print(f"   High-confidence bridges: {len(high_conf_bridges)}", file=sys.stderr)
    print(f"   Low-confidence: {len(low_conf_bridges)}", file=sys.stderr)
    print(f"   Mean confidence: {stats_dict['mean_confidence']:.3f}", file=sys.stderr)
    
    print(f"\n✅ Saved: {bridge_output}", file=sys.stderr)
    print(f"✅ Saved: {low_conf_output}", file=sys.stderr)
    print(f"✅ Saved: {stats_path}", file=sys.stderr)

if __name__ == "__main__":
    main()
