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
import hashlib
from typing import Dict, List, Any, Optional, Tuple
from collections import defaultdict, Counter

try:
    from tqdm import tqdm
except ImportError:
    tqdm = None

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

def candidate_retrieval(
    kalshi_event_id: str,
    kalshi_text: str,
    pm_candidates: List[Tuple[str, str]],  # (event_id, title)
    client,
    max_candidates: int = 30
) -> List[Tuple[str, float]]:
    """
    Steg A: Candidate retrieval - LLM väljer top-3 från menu.
    """
    if len(pm_candidates) <= 3:
        return [(pid, 1.0) for pid, _ in pm_candidates]
    
    # Chunk om för många
    if len(pm_candidates) > max_candidates:
        # Tournament: dela i chunks, ta winner från varje
        chunk_size = max_candidates
        winners = []
        for i in range(0, len(pm_candidates), chunk_size):
            chunk = pm_candidates[i:i+chunk_size]
            chunk_winners = candidate_retrieval(kalshi_event_id, kalshi_text, chunk, client, max_candidates)
            winners.extend(chunk_winners)
        # Rekursivt: matcha winners
        if len(winners) > 3:
            return candidate_retrieval(kalshi_event_id, kalshi_text, 
                                      [(pid, "") for pid, _ in winners[:max_candidates]], 
                                      client, max_candidates)
        return winners[:3]
    
    # Bygg menu
    menu_items = []
    for idx, (pm_id, pm_title) in enumerate(pm_candidates, 1):
        menu_items.append(f"{idx}. {pm_title} (id: {pm_id})")
    
    menu_text = "\n".join(menu_items)
    
    prompt = f"""Given this Kalshi event, select the top 3 most relevant Polymarket events from the menu below.

Kalshi event:
{kalshi_text}

Polymarket candidates:
{menu_text}

Respond with ONLY valid JSON (no markdown):
{{"top3": [{{"index": 1, "score": 0.0}}, {{"index": 2, "score": 0.0}}, {{"index": 3, "score": 0.0}}]}}

Rules:
- index: number from menu (1-N)
- score: 0.0 to 1.0 (relevance)
- Return exactly 3 items
"""
    
    messages = [
        {"role": "system", "content": "You are a precise event matcher. Always respond with valid JSON only."},
        {"role": "user", "content": prompt}
    ]
    
    try:
        response = client.chat(messages)
        response = response.strip()
        if response.startswith("```json"):
            response = response[7:]
        if response.startswith("```"):
            response = response[3:]
        if response.endswith("```"):
            response = response[:-3]
        response = response.strip()
        
        # Försök hitta JSON-objekt i response (hantera extra text)
        json_start = response.find("{")
        json_end = response.rfind("}") + 1
        if json_start >= 0 and json_end > json_start:
            response = response[json_start:json_end]
        
        result = json.loads(response)
        top3 = result.get("top3", [])
        
        # Konvertera index till event_id
        candidates_with_scores = []
        for item in top3[:3]:
            idx = item.get("index", 0) - 1
            score = float(item.get("score", 0.0))
            if 0 <= idx < len(pm_candidates):
                pm_id = pm_candidates[idx][0]
                candidates_with_scores.append((pm_id, score))
        
        return candidates_with_scores
    except Exception as e:
        print(f"  ⚠️  Candidate retrieval failed: {e}, using first 3", file=sys.stderr)
        return [(pid, 0.5) for pid, _ in pm_candidates[:3]]

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
        response = response.strip()
        if response.startswith("```json"):
            response = response[7:]
        if response.startswith("```"):
            response = response[3:]
        if response.endswith("```"):
            response = response[:-3]
        response = response.strip()
        
        # Försök hitta JSON-objekt i response (hantera extra text)
        json_start = response.find("{")
        json_end = response.rfind("}") + 1
        if json_start >= 0 and json_end > json_start:
            response = response[json_start:json_end]
        
        result = json.loads(response)
        result["kalshi_event_id"] = kalshi_event_id
        result["pm_event_id"] = pm_event_id
        return result
    except json.JSONDecodeError as e:
        print(f"  ⚠️  Pair decision JSON parse error: {e}", file=sys.stderr)
        print(f"     Response: {response[:200]}...", file=sys.stderr)
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
    pm_texts = load_event_texts("data/polymarket/derived/polymarket_market_docs.jsonl")
    kalshi_texts = load_event_texts("data/kalshi/derived/kalshi_market_docs.jsonl")
    
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
    
    # Räkna totalt antal Kalshi events att matcha
    total_kalshi_to_match = 0
    for category in categories:
        kalshi_events = kalshi_by_category.get(category, [])
        pm_events = pm_by_category.get(category, [])
        if kalshi_events and pm_events:
            # Filtrera bort redan matchade
            for kalshi_id in kalshi_events:
                already_matched = any(b.get("kalshi_event_ticker") == kalshi_id for b in high_conf_bridges)
                if not already_matched:
                    total_kalshi_to_match += 1
    
    print(f"\n🔄 Matchar events per kategori...", file=sys.stderr)
    print(f"   Totalt att matcha: {total_kalshi_to_match} Kalshi events", file=sys.stderr)
    
    # Progress bar
    matched_count = 0
    if tqdm:
        pbar = tqdm(total=total_kalshi_to_match, desc="Matching", unit="event", file=sys.stderr)
    else:
        pbar = None
    
    try:
        for category in categories:
            kalshi_events = kalshi_by_category.get(category, [])
            pm_events = pm_by_category.get(category, [])
            
            if not kalshi_events or not pm_events:
                continue
            
            if not pbar:
                print(f"\n   Kategori: {category} ({len(kalshi_events)} Kalshi, {len(pm_events)} Polymarket)", file=sys.stderr)
            
            # Bygg PM candidates (exkludera redan matchade)
            pm_candidates_list = [
                (pid, pm_texts.get(pid, pid)) 
                for pid in pm_events 
                if pid not in pm_matched
            ]
            
            if not pm_candidates_list:
                continue
            
            for kalshi_id in kalshi_events:
                # Checkpoint: hoppa över om redan matchad
                already_matched = any(b.get("kalshi_event_ticker") == kalshi_id for b in high_conf_bridges)
                if already_matched:
                    if pbar:
                        pbar.update(1)
                    continue
                
                kalshi_text = kalshi_texts.get(kalshi_id, kalshi_id)
                
                # Steg A: Candidate retrieval
                top3 = candidate_retrieval(kalshi_id, kalshi_text, pm_candidates_list, client)
                
                # Steg B: Pair decision för top-3
                best_match = None
                best_confidence = 0.0
                
                for pm_id, _ in top3:
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
                            {"pm_event_id": pid, "retrieval_score": score}
                            for pid, score in top3
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
                
                # Uppdatera progress
                if pbar:
                    pbar.update(1)
                else:
                    matched_count += 1
                    if matched_count % 10 == 0:
                        print(f"   Processed: {matched_count}/{total_kalshi_to_match}", file=sys.stderr)
    
    finally:
        if pbar:
            pbar.close()
    
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
