#!/usr/bin/env python3
"""
Hämta Kalshi markets/contracts per event_ticker för matchade events.
Input: data/matching/event_candidates.filtered.json
Output: data/kalshi/derived/kalshi_markets_full.jsonl (en rad per market/contract)
"""

import json
import sys
import os
import requests
import hmac
import hashlib
import base64
import time
import random
from typing import List, Dict, Any, Set
from collections import defaultdict
from urllib.parse import urlencode

# Kalshi API base (uppdaterad till elections host)
KALSHI_API_BASE = "https://api.elections.kalshi.com"


def sign_kalshi_request(
    method: str,
    path: str,
    query_params: Dict[str, Any],
    api_secret: str
) -> str:
    """
    Signera Kalshi API request enligt HMAC-SHA256.
    
    Format: timestamp + method + path + query_string
    """
    timestamp = str(int(time.time()))
    
    # Bygg query string (sorterad)
    if query_params:
        sorted_params = sorted(query_params.items())
        query_string = urlencode(sorted_params)
    else:
        query_string = ""
    
    # Bygg message att signera
    message = f"{timestamp}{method}{path}{query_string}"
    
    # Signera med HMAC-SHA256
    signature = hmac.new(
        api_secret.encode('utf-8'),
        message.encode('utf-8'),
        hashlib.sha256
    ).digest()
    
    # Base64 encode
    signature_b64 = base64.b64encode(signature).decode('utf-8')
    
    return timestamp, signature_b64


def get_kalshi_auth_headers(
    method: str,
    path: str,
    query_params: Dict[str, Any]
) -> Dict[str, str]:
    """
    Hämta auth headers med signed requests (HMAC-SHA256).
    """
    api_key_id = os.getenv("KALSHI_API_KEY_ID")
    api_secret = os.getenv("KALSHI_API_SECRET")
    
    if not api_key_id or not api_secret:
        print("ERROR: KALSHI_API_KEY_ID och KALSHI_API_SECRET måste sättas", file=sys.stderr)
        print("   Ex: export KALSHI_API_KEY_ID='your_key'", file=sys.stderr)
        print("   Ex: export KALSHI_API_SECRET='your_secret'", file=sys.stderr)
        sys.exit(1)
    
    timestamp, signature = sign_kalshi_request(method, path, query_params, api_secret)
    
    return {
        "Authorization": f"{api_key_id}:{signature}",
        "Kalshi-Access-Timestamp": timestamp,
        "Content-Type": "application/json"
    }

def fetch_markets_for_event(event_ticker: str, max_retries: int = 7) -> List[Dict[str, Any]]:
    """
    Hämta alla markets/contracts för ett event_ticker med retry och backoff.
    Kalshi API: GET /trade-api/v2/markets?event_ticker=...
    
    Args:
        event_ticker: Event ticker att hämta markets för
        max_retries: Max antal retries för 429 (default 7)
    """
    all_markets = []
    cursor = None
    limit = 1000
    request_count = 0
    path = "/trade-api/v2/markets"
    
    while True:
        params = {
            "event_ticker": event_ticker,
            "status": "open",
            "limit": limit
        }
        if cursor:
            params["cursor"] = cursor
        
        # Hämta signed headers för denna request
        headers = get_kalshi_auth_headers("GET", path, params)
        
        # Bygg URL: BASE_URL + path
        url = f"{KALSHI_API_BASE}{path}"
        
        # Retry loop för 429 (rate limiting)
        retry_count = 0
        
        while retry_count <= max_retries:
            try:
                response = requests.get(url, headers=headers, params=params, timeout=30)
                request_count += 1
                
                if response.status_code == 429:
                    # Rate limited - exponential backoff
                    retry_count += 1
                    if retry_count > max_retries:
                        print(f"  ⚠️  429 Rate limited för {event_ticker} efter {max_retries} retries - hoppar över", file=sys.stderr)
                        return all_markets
                    
                    # Exponential backoff: 2^retry_count sekunder + jitter
                    backoff_time = (2 ** retry_count) + random.uniform(0, 1)
                    print(f"  ⏳ 429 Rate limited, väntar {backoff_time:.1f}s (retry {retry_count}/{max_retries})...", file=sys.stderr)
                    time.sleep(backoff_time)
                    continue
                
                if response.status_code != 200:
                    # Logga hela response body för 401 (en gång per run)
                    if response.status_code == 401:
                        print(f"  ❌ 401 Authentication failed för {event_ticker}", file=sys.stderr)
                        print(f"  Response body: {response.text}", file=sys.stderr)
                        # Om det fortfarande är "API moved" efter host-bytet, logga det
                        if "API has been moved" in response.text or "moved" in response.text.lower():
                            print(f"  ⚠️  API verkar fortfarande vara flyttad - kontrollera host", file=sys.stderr)
                    else:
                        print(f"  ⚠️  Status {response.status_code} för {event_ticker}: {response.text[:200]}", file=sys.stderr)
                    return all_markets
                
                # Success - parse response
                data = response.json()
                markets = data.get("markets", [])
                
                if not markets:
                    return all_markets
                
                all_markets.extend(markets)
                
                # Kolla om det finns mer data
                cursor = data.get("cursor")
                if not cursor or len(markets) < limit:
                    return all_markets
                
                # Pace även på 200-svar (0.25-0.5s mellan requests)
                sleep_time = random.uniform(0.25, 0.5)
                time.sleep(sleep_time)
                break  # Break ur retry loop, fortsätt med nästa pagination
                
            except Exception as e:
                if retry_count >= max_retries:
                    print(f"  ❌ Fel vid hämtning av {event_ticker}: {e}", file=sys.stderr)
                    return all_markets
                
                # Retry med exponential backoff
                retry_count += 1
                backoff_time = (2 ** retry_count) + random.uniform(0, 1)
                print(f"  ⏳ Exception, väntar {backoff_time:.1f}s (retry {retry_count}/{max_retries})...", file=sys.stderr)
                time.sleep(backoff_time)
    
    return all_markets

def extract_market_data(market: Dict[str, Any], event_ticker: str) -> Dict[str, Any]:
    """
    Extrahera relevanta fält från market-objektet.
    
    VIKTIGT: market_ticker MÅSTE vara ifylld (inte tom) för att market-level data ska vara korrekt.
    """
    market_ticker = market.get("ticker", "")
    
    # Validera att vi har market_ticker
    if not market_ticker:
        # Försök hitta från andra fält
        market_ticker = market.get("market_ticker", "") or market.get("contract_ticker", "")
    
    return {
        "event_ticker": event_ticker,
        "market_ticker": market_ticker,
        "title": market.get("title", ""),
        "subtitle": market.get("sub_title", ""),
        "strike_period": market.get("strike_period", ""),
        "category": market.get("category", ""),
        "close_time": market.get("close_time", ""),
        "settlement": market.get("settlement", ""),
        "yes_bid": market.get("yes_bid"),
        "yes_ask": market.get("yes_ask"),
        "no_bid": market.get("no_bid"),
        "no_ask": market.get("no_ask"),
        "last_price": market.get("last_price"),
        "raw": market  # Hela objektet för debugging
    }

def load_checkpoint(output_path: str) -> Set[str]:
    """
    Ladda checkpoint: hämta alla event_tickers som redan är hämtade.
    Returnerar set med event_tickers.
    """
    if not os.path.exists(output_path):
        return set()
    
    fetched_events = set()
    try:
        with open(output_path, "r") as f:
            for line in f:
                if not line.strip():
                    continue
                market = json.loads(line)
                event_ticker = market.get("event_ticker", "")
                if event_ticker:
                    fetched_events.add(event_ticker)
    except Exception as e:
        print(f"  ⚠️  Kunde inte läsa checkpoint: {e}", file=sys.stderr)
        return set()
    
    return fetched_events


def main():
    # Läs matchade events
    input_path = "data/matching/event_candidates.filtered.json"
    if not os.path.exists(input_path):
        print(f"ERROR: {input_path} finns inte", file=sys.stderr)
        print("Kör först: cargo run match_events_embeddings", file=sys.stderr)
        sys.exit(1)
    
    with open(input_path, "r") as f:
        candidates_data = json.load(f)
    
    # Extrahera unika Kalshi event_tickers
    matched_kalshi_events = sorted(set(
        obj["kalshi_event_id"] 
        for obj in candidates_data 
        if "kalshi_event_id" in obj
    ))
    
    print(f"📊 Input: {len(matched_kalshi_events)} matchade Kalshi events", file=sys.stderr)
    print(f"📋 Events: {', '.join(matched_kalshi_events[:5])}...", file=sys.stderr)
    
    # Skapa output directory
    os.makedirs("data/kalshi/derived", exist_ok=True)
    output_path = "data/kalshi/derived/kalshi_markets_full.jsonl"
    
    # Ladda checkpoint (redan hämtade events)
    fetched_events = load_checkpoint(output_path)
    if fetched_events:
        print(f"📂 Checkpoint: {len(fetched_events)} events redan hämtade, hoppar över dem", file=sys.stderr)
    
    # Öppna output-fil i append mode för checkpoint/resume
    output_file = open(output_path, "a" if fetched_events else "w")
    
    # Hämta markets för varje event
    events_fetched = 0
    events_skipped = 0
    events_failed = 0
    event_market_counts = defaultdict(int)
    markets_with_ticker = 0
    markets_without_ticker = 0
    
    for idx, event_ticker in enumerate(matched_kalshi_events, 1):
        # Checkpoint: hoppa över redan hämtade events
        if event_ticker in fetched_events:
            events_skipped += 1
            print(f"[{idx}/{len(matched_kalshi_events)}] ⏭️  Hoppar över {event_ticker} (redan hämtad)", file=sys.stderr)
            continue
        
        print(f"[{idx}/{len(matched_kalshi_events)}] Hämtar markets för {event_ticker}...", file=sys.stderr)
        
        markets = fetch_markets_for_event(event_ticker)
        
        if markets:
            events_fetched += 1
            event_market_counts[event_ticker] = len(markets)
            
            # Skriv markets direkt till fil (checkpoint per event)
            for market in markets:
                # Extrahera event_ticker från market (kan finnas i market objektet)
                market_event_ticker = market.get("event_ticker", event_ticker)
                if not market_event_ticker:
                    # Försök hitta från ticker format (t.ex. "KXELONMARS-99-Y" -> "KXELONMARS-99")
                    ticker = market.get("ticker", "")
                    if "-" in ticker:
                        parts = ticker.split("-")
                        if len(parts) >= 2:
                            market_event_ticker = "-".join(parts[:-1])
                
                market_doc = extract_market_data(market, market_event_ticker)
                
                # Validera market_ticker
                if market_doc["market_ticker"]:
                    markets_with_ticker += 1
                else:
                    markets_without_ticker += 1
                    print(f"  ⚠️  Market utan market_ticker: {json.dumps(market_doc.get('title', ''))[:50]}", file=sys.stderr)
                
                output_file.write(json.dumps(market_doc) + "\n")
                output_file.flush()  # Flush för att säkerställa att data sparas
            
            print(f"  ✅ {len(markets)} markets (totalt: {markets_with_ticker + markets_without_ticker} markets)", file=sys.stderr)
        else:
            events_failed += 1
            print(f"  ⚠️  Inga markets hittades", file=sys.stderr)
    
    output_file.close()
    
    # Statistik
    total_markets = markets_with_ticker + markets_without_ticker
    print(f"\n📊 Statistik:", file=sys.stderr)
    print(f"   Events input: {len(matched_kalshi_events)}", file=sys.stderr)
    print(f"   Events hämtade: {events_fetched}", file=sys.stderr)
    print(f"   Events hoppade över (checkpoint): {events_skipped}", file=sys.stderr)
    print(f"   Events failed: {events_failed}", file=sys.stderr)
    print(f"   Total markets/contracts: {total_markets}", file=sys.stderr)
    print(f"   Markets med market_ticker: {markets_with_ticker}", file=sys.stderr)
    if markets_without_ticker > 0:
        print(f"   ⚠️  Markets utan market_ticker: {markets_without_ticker}", file=sys.stderr)
    
    # Top 10 events med flest markets
    if event_market_counts:
        top_events = sorted(event_market_counts.items(), key=lambda x: x[1], reverse=True)[:10]
        print(f"\n🏆 Top 10 events med flest markets:", file=sys.stderr)
        for event_ticker, count in top_events:
            print(f"   {event_ticker}: {count} markets", file=sys.stderr)
    
    print(f"\n✅ Saved: {output_path} ({total_markets} markets)", file=sys.stderr)

if __name__ == "__main__":
    main()
