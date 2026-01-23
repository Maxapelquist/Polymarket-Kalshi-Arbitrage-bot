#!/usr/bin/env python3
"""
Bygg event bridge mapping mellan Kalshi event_ticker och Polymarket pm_event_id.
Använder market-aware matching med embeddings.

Input:
  - data/kalshi/derived/kalshi_market_docs.jsonl
  - data/polymarket/derived/polymarket_market_docs.jsonl
Output:
  - data/matching/event_bridge.json (high-confidence bridges)
  - data/matching/event_bridge.low_conf.json (low-confidence bridges)
  - data/matching/event_bridge.stats.json (statistik)
"""

import json
import sys
import os
import subprocess
import numpy as np
from typing import Dict, List, Any, Tuple
from collections import defaultdict, Counter

# Embedding script path
EMBED_SCRIPT = "scripts/embed.py"

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

def cosine_similarity(vec1: List[float], vec2: List[float]) -> float:
    """Beräkna cosine similarity mellan två vektorer"""
    vec1 = np.array(vec1)
    vec2 = np.array(vec2)
    dot_product = np.dot(vec1, vec2)
    norm1 = np.linalg.norm(vec1)
    norm2 = np.linalg.norm(vec2)
    if norm1 == 0 or norm2 == 0:
        return 0.0
    return float(dot_product / (norm1 * norm2))

def embed_texts(texts: List[str], ids: List[str]) -> Dict[str, List[float]]:
    """
    Generera embeddings för texter via embed.py script.
    Returnerar dict med id -> embedding vector.
    """
    if not texts:
        return {}
    
    # Bygg input
    input_data = {
        "items": [{"id": id_val, "text": text} for id_val, text in zip(ids, texts)]
    }
    
    # Kör embed.py script
    try:
        result = subprocess.run(
            ["python3", EMBED_SCRIPT],
            input=json.dumps(input_data),
            capture_output=True,
            text=True,
            check=True
        )
        
        output_data = json.loads(result.stdout)
        embeddings = {}
        for emb in output_data.get("embeddings", []):
            embeddings[emb["id"]] = emb["vector"]
        
        return embeddings
    except Exception as e:
        print(f"  ❌ Fel vid embedding: {e}", file=sys.stderr)
        return {}

def build_event_texts(market_docs: List[Dict[str, Any]], top_n: int = 10) -> Dict[str, str]:
    """
    Bygg sammanslagen event_text för varje event.
    Använder text-fältet från market_docs (som innehåller title + subtitle + category + event title).
    Tar top N markets per event.
    """
    # Gruppera markets per event
    events_to_markets = defaultdict(list)
    for doc in market_docs:
        event_id = doc.get("event_id", "")
        if event_id:
            events_to_markets[event_id].append(doc)
    
    event_texts = {}
    for event_id, markets in events_to_markets.items():
        # Hämta text från top N markets
        texts = []
        for market in markets[:top_n]:
            # Prioritera text-fältet (som innehåller mer information)
            text = market.get("text", "")
            if not text:
                # Fallback: använd title
                text = market.get("title", "")
            if text:
                texts.append(text)
        
        # Bygg sammanslagen text
        event_text = " | ".join(texts)
        if not event_text:
            # Fallback: använd event_id
            event_text = event_id
        
        event_texts[event_id] = event_text
    
    return event_texts

def match_events_with_embeddings(
    kalshi_event_texts: Dict[str, str],
    pm_event_texts: Dict[str, str],
    score_threshold: float = 0.75,
    margin_threshold: float = 0.05
) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]], Dict[str, Any]]:
    """
    Matcha events med embeddings.
    Returnerar: (high_conf_bridges, low_conf_bridges, stats)
    """
    if not kalshi_event_texts or not pm_event_texts:
        return [], [], {}
    
    # Generera embeddings för alla events
    print("  🔄 Genererar embeddings för Kalshi events...", file=sys.stderr)
    kalshi_ids = list(kalshi_event_texts.keys())
    kalshi_texts = [kalshi_event_texts[kid] for kid in kalshi_ids]
    kalshi_embeddings = embed_texts(kalshi_texts, kalshi_ids)
    
    print("  🔄 Genererar embeddings för Polymarket events...", file=sys.stderr)
    pm_ids = list(pm_event_texts.keys())
    pm_texts = [pm_event_texts[pid] for pid in pm_ids]
    pm_embeddings = embed_texts(pm_texts, pm_ids)
    
    if not kalshi_embeddings or not pm_embeddings:
        print("  ⚠️  Kunde inte generera embeddings", file=sys.stderr)
        return [], [], {}
    
    # Matcha varje Kalshi event mot alla Polymarket events
    print("  🔄 Matchar events...", file=sys.stderr)
    high_conf_bridges = []
    low_conf_bridges = []
    all_scores = []
    
    for kalshi_event_id, kalshi_text in kalshi_event_texts.items():
        if kalshi_event_id not in kalshi_embeddings:
            continue
        
        kalshi_emb = kalshi_embeddings[kalshi_event_id]
        
        # Beräkna similarity mot alla Polymarket events
        scores = []
        for pm_event_id, pm_text in pm_event_texts.items():
            if pm_event_id not in pm_embeddings:
                continue
            
            pm_emb = pm_embeddings[pm_event_id]
            similarity = cosine_similarity(kalshi_emb, pm_emb)
            scores.append((pm_event_id, similarity))
            all_scores.append(similarity)
        
        if not scores:
            continue
        
        # Sortera efter score
        scores.sort(key=lambda x: x[1], reverse=True)
        
        top_score = scores[0][1]
        top_pm_event = scores[0][0]
        second_score = scores[1][1] if len(scores) > 1 else 0.0
        margin = top_score - second_score
        
        bridge_entry = {
            "kalshi_event_ticker": kalshi_event_id,
            "pm_event_id": top_pm_event,
            "score": top_score,
            "margin": margin,
            "top3_candidates": [
                {"pm_event_id": pid, "score": score} 
                for pid, score in scores[:3]
            ]
        }
        
        # High-confidence: score >= threshold AND margin >= margin_threshold
        if top_score >= score_threshold and margin >= margin_threshold:
            bridge_entry["source"] = "high_conf"
            high_conf_bridges.append(bridge_entry)
        else:
            bridge_entry["source"] = "low_conf"
            low_conf_bridges.append(bridge_entry)
    
    # Bygg statistik
    stats = {
        "total_kalshi_events": len(kalshi_event_texts),
        "total_pm_events": len(pm_event_texts),
        "high_conf_bridges": len(high_conf_bridges),
        "low_conf_bridges": len(low_conf_bridges),
        "unbridged_kalshi": len(kalshi_event_texts) - len(high_conf_bridges) - len(low_conf_bridges),
        "score_threshold": score_threshold,
        "margin_threshold": margin_threshold,
        "score_stats": {
            "mean": float(np.mean(all_scores)) if all_scores else 0.0,
            "median": float(np.median(all_scores)) if all_scores else 0.0,
            "min": float(np.min(all_scores)) if all_scores else 0.0,
            "max": float(np.max(all_scores)) if all_scores else 0.0,
            "std": float(np.std(all_scores)) if all_scores else 0.0
        },
        "score_histogram": {}
    }
    
    # Score histogram (10 bins)
    if all_scores:
        hist, bins = np.histogram(all_scores, bins=10, range=(0.0, 1.0))
        stats["score_histogram"] = {
            f"{bins[i]:.2f}-{bins[i+1]:.2f}": int(hist[i]) 
            for i in range(len(hist))
        }
    
    return high_conf_bridges, low_conf_bridges, stats

def main():
    kalshi_docs_path = "data/kalshi/derived/kalshi_market_docs.jsonl"
    pm_docs_path = "data/polymarket/derived/polymarket_market_docs.jsonl"
    
    print("📂 Läser market_docs...", file=sys.stderr)
    
    kalshi_docs = load_jsonl(kalshi_docs_path)
    pm_docs = load_jsonl(pm_docs_path)
    
    print(f"   Kalshi market_docs: {len(kalshi_docs)}", file=sys.stderr)
    print(f"   Polymarket market_docs: {len(pm_docs)}", file=sys.stderr)
    
    if not kalshi_docs or not pm_docs:
        print("ERROR: Market_docs saknas. Kör först build_market_docs.py", file=sys.stderr)
        sys.exit(1)
    
    # Bygg event_texts
    print("\n🔍 Bygger event_texts från markets...", file=sys.stderr)
    kalshi_event_texts = build_event_texts(kalshi_docs, top_n=10)
    pm_event_texts = build_event_texts(pm_docs, top_n=10)
    
    print(f"   Kalshi events: {len(kalshi_event_texts)}", file=sys.stderr)
    print(f"   Polymarket events: {len(pm_event_texts)}", file=sys.stderr)
    
    # Matcha events med embeddings
    print("\n🔄 Matchar events med embeddings...", file=sys.stderr)
    high_conf_bridges, low_conf_bridges, stats = match_events_with_embeddings(
        kalshi_event_texts,
        pm_event_texts,
        score_threshold=0.75,
        margin_threshold=0.05
    )
    
    # Spara resultat
    os.makedirs("data/matching", exist_ok=True)
    
    bridge_path = "data/matching/event_bridge.json"
    with open(bridge_path, "w") as f:
        json.dump(high_conf_bridges, f, indent=2)
    
    low_conf_path = "data/matching/event_bridge.low_conf.json"
    with open(low_conf_path, "w") as f:
        json.dump(low_conf_bridges, f, indent=2)
    
    stats_path = "data/matching/event_bridge.stats.json"
    with open(stats_path, "w") as f:
        json.dump(stats, f, indent=2)
    
    # Statistik
    print(f"\n📊 Statistik:", file=sys.stderr)
    print(f"   Total Kalshi events: {stats['total_kalshi_events']}", file=sys.stderr)
    print(f"   Total Polymarket events: {stats['total_pm_events']}", file=sys.stderr)
    print(f"   High-confidence bridges: {stats['high_conf_bridges']}", file=sys.stderr)
    print(f"   Low-confidence bridges: {stats['low_conf_bridges']}", file=sys.stderr)
    print(f"   Unbridged Kalshi events: {stats['unbridged_kalshi']}", file=sys.stderr)
    
    print(f"\n📈 Score statistik:", file=sys.stderr)
    score_stats = stats["score_stats"]
    print(f"   Mean: {score_stats['mean']:.3f}", file=sys.stderr)
    print(f"   Median: {score_stats['median']:.3f}", file=sys.stderr)
    print(f"   Min: {score_stats['min']:.3f}", file=sys.stderr)
    print(f"   Max: {score_stats['max']:.3f}", file=sys.stderr)
    print(f"   Std: {score_stats['std']:.3f}", file=sys.stderr)
    
    # Top 10 high-confidence bridges
    if high_conf_bridges:
        sorted_bridges = sorted(high_conf_bridges, key=lambda x: x["score"], reverse=True)[:10]
        print(f"\n🏆 Top 10 high-confidence bridges:", file=sys.stderr)
        for bridge in sorted_bridges:
            print(f"   {bridge['kalshi_event_ticker']} ↔ {bridge['pm_event_id']} (score: {bridge['score']:.3f}, margin: {bridge['margin']:.3f})", file=sys.stderr)
    
    print(f"\n✅ Saved: {bridge_path} ({stats['high_conf_bridges']} bridges)", file=sys.stderr)
    print(f"✅ Saved: {low_conf_path} ({stats['low_conf_bridges']} bridges)", file=sys.stderr)
    print(f"✅ Saved: {stats_path}", file=sys.stderr)

if __name__ == "__main__":
    main()
