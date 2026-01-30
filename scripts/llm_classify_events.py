#!/usr/bin/env python3
"""
Klassificera Polymarket och Kalshi events till Kalshi-kategorier med lokal LLM.
Input: market_docs.jsonl
Output: event_categories.jsonl
"""

import json
import sys
import os
import hashlib
import time
from typing import Dict, List, Any, Optional
from collections import defaultdict

# Import local_llm_client från samma directory
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from local_llm_client import get_client

try:
    from tqdm import tqdm
except ImportError:
    tqdm = None

def normalize_category(category: str, valid_categories: List[str]) -> Optional[str]:
    """
    Normalisera kategori från LLM till exakt match mot valid_categories.
    Hanterar: lowercase, trim, alias-mappning, delvis match.
    """
    if not category:
        return None
    
    # Normalisera: lowercase, strip
    normalized = category.strip().lower()
    
    # Direkt match (case-insensitive)
    for valid in valid_categories:
        if valid.lower() == normalized:
            return valid
    
    # Alias-mappning (vanliga varianter)
    alias_map = {
        "political": "Politics",
        "politics": "Politics",
        "election": "Elections",
        "economic": "Economics",
        "economy": "Economics",
        "business": "Business",
        "tech": "Science and Technology",
        "technology": "Science and Technology",
        "science": "Science and Technology",
        "education": "Science and Technology",  # Education → Science and Technology
        "climate": "Climate and Weather",
        "weather": "Climate and Weather",
        "health": "Health",
        "entertainment": "Entertainment",
        "sport": "Sports",
        "sports": "Sports",
        "culture": "Culture",
        "energy": "Energy",
        "finance": "Finance",
        "financial": "Finance",
        "crypto": "Crypto",
        "cryptocurrency": "Crypto",
        "law": "Law and Justice",
        "justice": "Law and Justice",
        "legal": "Law and Justice",
        "world": "World",
    }
    
    if normalized in alias_map:
        return alias_map[normalized]
    
    # Delvis match (t.ex. "Science and Technology" matchar "science")
    for valid in valid_categories:
        valid_lower = valid.lower()
        if normalized in valid_lower or valid_lower in normalized:
            return valid
    
    return None

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

def build_event_texts(market_docs: List[Dict[str, Any]], top_n: int = 10) -> Dict[str, str]:
    """Bygg event_text från markets"""
    events_to_markets = defaultdict(list)
    for doc in market_docs:
        event_id = doc.get("event_id", "")
        if event_id:
            events_to_markets[event_id].append(doc)
    
    event_texts = {}
    for event_id, markets in events_to_markets.items():
        texts = []
        for market in markets[:top_n]:
            text = market.get("text", "") or market.get("title", "")
            if text:
                texts.append(text)
        
        event_text = " | ".join(texts) if texts else event_id
        event_texts[event_id] = event_text
    
    return event_texts

def get_cache_key(event_text: str, categories: List[str]) -> str:
    """Generera cache key"""
    combined = event_text + "|".join(categories)
    return hashlib.sha256(combined.encode()).hexdigest()

def get_cache_path(cache_key: str) -> str:
    """Hämta cache path"""
    provider = os.getenv("LOCAL_LLM_PROVIDER", "ollama")
    model = os.getenv("LOCAL_LLM_MODEL", "unknown")
    cache_dir = f"data/cache/llm/classify/{provider}_{model}"
    os.makedirs(cache_dir, exist_ok=True)
    return f"{cache_dir}/{cache_key}.json"

def load_from_cache(cache_key: str) -> Optional[Dict[str, Any]]:
    """Ladda från cache"""
    cache_path = get_cache_path(cache_key)
    if os.path.exists(cache_path):
        with open(cache_path, "r") as f:
            return json.load(f)
    return None

def save_to_cache(cache_key: str, result: Dict[str, Any]):
    """Spara till cache"""
    cache_path = get_cache_path(cache_key)
    with open(cache_path, "w") as f:
        json.dump(result, f, indent=2)

def classify_event(event_id: str, event_text: str, categories: List[str], client) -> Dict[str, Any]:
    """Klassificera ett event med LLM"""
    cache_key = get_cache_key(event_text, categories)
    
    # Kolla cache
    cached = load_from_cache(cache_key)
    if cached:
        return cached
    
    # Bygg prompt
    categories_str = ", ".join(categories)
    prompt = f"""Classify the following event into exactly ONE of these categories: {categories_str}

Event description:
{event_text}

Respond with ONLY valid JSON in this exact format (no markdown, no explanation):
{{"category":"<one_of_categories>","confidence":0.0,"rationale_short":"...","evidence":["..."]}}

Rules:
- category must be exactly one of: {categories_str}
- confidence: 0.0 to 1.0
- rationale_short: brief explanation (max 50 words)
- evidence: list of 1-3 key phrases from the event that support the category
"""
    
    messages = [
        {"role": "system", "content": "You are a precise event classifier. Always respond with valid JSON only."},
        {"role": "user", "content": prompt}
    ]
    
    try:
        response = client.chat(messages)
        
        # Parse JSON från response (ta bort markdown om finns)
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
        
        # Normalisera kategori
        raw_category = result.get("category", "")
        normalized_category = normalize_category(raw_category, categories)
        
        if not normalized_category:
            raise ValueError(f"Could not normalize category: '{raw_category}' (valid: {categories})")
        
        result["category"] = normalized_category
        
        # Validera confidence
        confidence = result.get("confidence", 0.0)
        if not isinstance(confidence, (int, float)) or confidence < 0 or confidence > 1:
            result["confidence"] = 0.5  # Default om ogiltig
        
        # Lägg till metadata
        result["event_id"] = event_id
        result["model"] = os.getenv("LOCAL_LLM_MODEL", "unknown")
        result["ts"] = str(int(time.time()))
        
        # Spara till cache
        save_to_cache(cache_key, result)
        
        return result
    except json.JSONDecodeError as e:
        print(f"  ❌ JSON parse error för {event_id}: {e}", file=sys.stderr)
        print(f"     Response: {response[:200]}...", file=sys.stderr)
        return {
            "event_id": event_id,
            "category": "Unknown",
            "confidence": 0.0,
            "rationale_short": f"JSON parse error: {str(e)}",
            "evidence": [],
            "model": "error",
            "ts": str(int(time.time()))
        }
    except Exception as e:
        print(f"  ❌ Fel vid klassificering av {event_id}: {e}", file=sys.stderr)
        return {
            "event_id": event_id,
            "category": "Unknown",
            "confidence": 0.0,
            "rationale_short": f"Error: {str(e)}",
            "evidence": [],
            "model": "error",
            "ts": str(int(time.time()))
        }

def main():
    
    # Läs kategorier
    categories_path = "config/kalshi_categories.json"
    with open(categories_path, "r") as f:
        categories = json.load(f)
    
    print(f"📂 Läser market_docs...", file=sys.stderr)
    
    # Polymarket
    pm_docs = load_jsonl("data/polymarket/derived/polymarket_market_docs.jsonl")
    pm_event_texts = build_event_texts(pm_docs)
    print(f"   Polymarket events: {len(pm_event_texts)}", file=sys.stderr)
    
    # Kalshi
    kalshi_docs = load_jsonl("data/kalshi/derived/kalshi_market_docs.jsonl")
    kalshi_event_texts = build_event_texts(kalshi_docs)
    print(f"   Kalshi events: {len(kalshi_event_texts)}", file=sys.stderr)
    
    # Initiera LLM client
    try:
        client = get_client()
        print(f"\n🤖 LLM Provider: {os.getenv('LOCAL_LLM_PROVIDER', 'ollama')}", file=sys.stderr)
        print(f"   Model: {os.getenv('LOCAL_LLM_MODEL', 'unknown')}", file=sys.stderr)
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)
    
    # Klassificera Polymarket events
    os.makedirs("data/matching", exist_ok=True)
    pm_output = "data/matching/pm_event_categories.jsonl"
    
    # Checkpoint: ladda redan klassificerade events (hoppa över "Unknown" med error)
    existing_pm = {}
    if os.path.exists(pm_output):
        for line in open(pm_output, "r"):
            if line.strip():
                try:
                    existing = json.loads(line)
                    event_id = existing.get("event_id", "")
                    category = existing.get("category", "")
                    model = existing.get("model", "")
                    # Hoppa över events med "Unknown" och "error" model (kör om dem)
                    if category == "Unknown" and model == "error":
                        continue
                    existing_pm[event_id] = existing
                except:
                    pass
    
    print(f"\n🔄 Klassificerar Polymarket events...", file=sys.stderr)
    print(f"   Redan klassificerade: {len(existing_pm)}", file=sys.stderr)
    
    pm_classified = list(existing_pm.values())
    remaining_pm = [(event_id, event_text) for event_id, event_text in pm_event_texts.items() if event_id not in existing_pm]
    processed = 0
    iterator = remaining_pm
    if tqdm:
        iterator = tqdm(remaining_pm, desc="PM klassificering", unit="event", dynamic_ncols=True)
    for event_id, event_text in iterator:
        # Hoppa över om redan klassificerad
        processed += 1
        if not tqdm and processed % 10 == 0:
            print(f"   Processed {processed} nya events...", file=sys.stderr)
        
        result = classify_event(event_id, event_text, categories, client)
        pm_classified.append(result)
        
        # Append till JSONL (atomiskt: skriv till tmp, rename)
        tmp_path = pm_output + ".tmp"
        # Kopiera befintlig fil om den finns
        if os.path.exists(pm_output):
            import shutil
            shutil.copy(pm_output, tmp_path)
        else:
            open(tmp_path, "w").close()
        
        with open(tmp_path, "a") as f:
            f.write(json.dumps(result) + "\n")
        os.replace(tmp_path, pm_output)
    
    # Klassificera Kalshi events
    kalshi_output = "data/matching/kalshi_event_categories.jsonl"
    
    # Checkpoint: ladda redan klassificerade events (hoppa över "Unknown" med error)
    existing_kalshi = {}
    if os.path.exists(kalshi_output):
        for line in open(kalshi_output, "r"):
            if line.strip():
                try:
                    existing = json.loads(line)
                    event_id = existing.get("event_id", "")
                    category = existing.get("category", "")
                    model = existing.get("model", "")
                    # Hoppa över events med "Unknown" och "error" model (kör om dem)
                    if category == "Unknown" and model == "error":
                        continue
                    existing_kalshi[event_id] = existing
                except:
                    pass
    
    print(f"\n🔄 Klassificerar Kalshi events...", file=sys.stderr)
    print(f"   Redan klassificerade: {len(existing_kalshi)}", file=sys.stderr)
    
    kalshi_classified = list(existing_kalshi.values())
    remaining_kalshi = [
        (event_id, event_text)
        for event_id, event_text in kalshi_event_texts.items()
        if event_id not in existing_kalshi
    ]
    processed = 0
    iterator = remaining_kalshi
    if tqdm:
        iterator = tqdm(remaining_kalshi, desc="Kalshi klassificering", unit="event", dynamic_ncols=True)
    for event_id, event_text in iterator:
        # Hoppa över om redan klassificerad
        processed += 1
        if not tqdm and processed % 10 == 0:
            print(f"   Processed {processed} nya events...", file=sys.stderr)
        
        result = classify_event(event_id, event_text, categories, client)
        kalshi_classified.append(result)
        
        # Append till JSONL (atomiskt)
        tmp_path = kalshi_output + ".tmp"
        # Kopiera befintlig fil om den finns
        if os.path.exists(kalshi_output):
            import shutil
            shutil.copy(kalshi_output, tmp_path)
        else:
            open(tmp_path, "w").close()
        
        with open(tmp_path, "a") as f:
            f.write(json.dumps(result) + "\n")
        os.replace(tmp_path, kalshi_output)
    
    # Statistik
    print(f"\n📊 Statistik:", file=sys.stderr)
    print(f"   Polymarket classified: {len(pm_classified)}", file=sys.stderr)
    print(f"   Kalshi classified: {len(kalshi_classified)}", file=sys.stderr)
    
    print(f"\n✅ Saved: {pm_output}", file=sys.stderr)
    print(f"✅ Saved: {kalshi_output}", file=sys.stderr)

if __name__ == "__main__":
    main()
