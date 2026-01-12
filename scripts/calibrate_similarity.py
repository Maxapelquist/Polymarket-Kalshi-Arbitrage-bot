#!/usr/bin/env python3
"""
Market Similarity Calibration Script

╔═══════════════════════════════════════════════════════════════════════╗
║                                                                       ║
║         This script OBSERVES and MEASURES signal.                    ║
║         It does NOT match, decide, or threshold.                     ║
║                                                                       ║
╚═══════════════════════════════════════════════════════════════════════╝

Purpose:
    - Understand embedding similarity distributions
    - Calibrate signal before building matching logic
    - Explore when LLM adds information beyond similarity

NOT allowed:
    ❌ No matching decisions
    ❌ No threshold setting
    ❌ No auto-grouping
    ❌ No arbitrage logic
    ❌ No verdicts or enums from LLM

ONLY allowed:
    ✅ Measurement
    ✅ Distribution analysis
    ✅ Descriptive LLM reasoning
    ✅ Transparent reporting
"""

import json
import os
from dataclasses import dataclass, asdict
from datetime import datetime
from pathlib import Path
from typing import List, Dict, Any, Tuple, Optional
import numpy as np
from sklearn.metrics.pairwise import cosine_similarity
from openai import OpenAI
from anthropic import Anthropic
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

# ═══════════════════════════════════════════════════════════════════════
# CONFIGURATION (All configurable, no hardcoded decisions)
# ═══════════════════════════════════════════════════════════════════════

@dataclass
class CalibrationConfig:
    """Configuration for calibration run - NO THRESHOLDS, only measurement params"""
    
    # Input paths
    kalshi_jsonl: str = "data/kalshi/structured/kalshi_markets_structured.jsonl"
    polymarket_jsonl: str = "data/polymarket/structured/polymarket_markets_structured.jsonl"
    
    # Output paths
    output_dir: str = "data/reports"
    
    # Embedding configuration
    embedding_model: str = "text-embedding-3-small"  # OpenAI model
    embedding_provider: str = "openai"  # or "custom"
    
    # Sampling configuration (for LLM analysis)
    sample_size_per_bucket: int = 10  # How many pairs to sample from each similarity bucket
    
    # LLM configuration
    llm_model: str = "claude-sonnet-4-20250514"
    llm_temperature: float = 0.0  # Deterministic
    
    # Reporting
    top_k_similarities: List[int] = None  # Top-1, Top-5, Top-20
    
    def __post_init__(self):
        if self.top_k_similarities is None:
            self.top_k_similarities = [1, 5, 20]


# ═══════════════════════════════════════════════════════════════════════
# DATA MODELS
# ═══════════════════════════════════════════════════════════════════════

@dataclass
class Market:
    """Loaded market from JSONL"""
    source: str
    market_ticker: str
    event_ticker: str
    event_text: str
    rules_text: Optional[str]
    llm_text: str
    market_fingerprint: str
    status: str
    time_window: Dict[str, Optional[str]]
    
    @property
    def embedding_text(self) -> str:
        """Text to use for embedding (EVENT + TIME WINDOW, not RULES)"""
        time_str = f"Open: {self.time_window.get('open', 'null')} Close: {self.time_window.get('close', 'null')}"
        return f"{self.event_text}\n{time_str}"


@dataclass
class SimilarityProfile:
    """Similarity profile for a single Kalshi market - NO DECISIONS"""
    kalshi_fingerprint: str
    kalshi_ticker: str
    kalshi_event_text: str
    top_similarities: Dict[int, List[Tuple[str, float]]]  # {k: [(poly_fingerprint, score), ...]}
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            "kalshi_fingerprint": self.kalshi_fingerprint,
            "kalshi_ticker": self.kalshi_ticker,
            "kalshi_event_text": self.kalshi_event_text,
            "top_1_score": self.top_similarities[1][0][1] if 1 in self.top_similarities else None,
            "top_5_scores": [s for _, s in self.top_similarities.get(5, [])],
            "top_20_scores": [s for _, s in self.top_similarities.get(20, [])],
            "top_1_poly_fingerprint": self.top_similarities[1][0][0] if 1 in self.top_similarities else None,
        }


@dataclass
class MarketPair:
    """A pair selected for LLM analysis - NO VERDICT"""
    kalshi_market: Market
    polymarket_market: Market
    similarity_score: float
    bucket: str  # "high", "medium", "low" (descriptive, not decisive)


@dataclass
class LLMDescription:
    """LLM's descriptive analysis - FREE TEXT, NO ENUMS, NO VERDICTS"""
    pair_id: str
    kalshi_fingerprint: str
    polymarket_fingerprint: str
    similarity_score: float
    bucket: str
    llm_description: str  # Free text reasoning
    timestamp: str


# ═══════════════════════════════════════════════════════════════════════
# STEP 1: Load and Filter Markets
# ═══════════════════════════════════════════════════════════════════════

def load_markets_from_jsonl(filepath: str) -> List[Market]:
    """Load markets from JSONL file"""
    path = Path(filepath)
    if not path.exists():
        raise FileNotFoundError(
            f"Market data file not found: {filepath}\n"
            f"Please run the observation pipeline first:\n"
            f"  cargo run --release\n"
            f"This will generate the structured market data files."
        )
    
    markets = []
    with open(path, 'r') as f:
        for line in f:
            if line.strip():
                data = json.loads(line)
                markets.append(Market(
                    source=data['source'],
                    market_ticker=data['market_ticker'],
                    event_ticker=data['event_ticker'],
                    event_text=data['event_text'],
                    rules_text=data.get('rules_text'),
                    llm_text=data['llm_text'],
                    market_fingerprint=data['market_fingerprint'],
                    status=data['status'],
                    time_window=data['time_window'],
                ))
    return markets


def filter_active_markets(markets: List[Market]) -> List[Market]:
    """Filter to only active markets - simple status check, no complex logic"""
    return [m for m in markets if m.status in ('active', 'open')]


def generate_baseline_counts(kalshi_markets: List[Market], 
                            polymarket_markets: List[Market],
                            output_dir: str):
    """Step 1: Generate baseline statistics - NO ANALYSIS, just counts"""
    baseline = {
        "timestamp": datetime.utcnow().isoformat(),
        "kalshi_total": len(kalshi_markets),
        "polymarket_total": len(polymarket_markets),
        "note": "These are RAW counts - no filtering, no thresholds, no decisions"
    }
    
    output_path = Path(output_dir) / "baseline_counts.json"
    with open(output_path, 'w') as f:
        json.dump(baseline, f, indent=2)
    
    print(f"✅ Baseline counts saved to {output_path}")
    print(f"   Kalshi markets: {baseline['kalshi_total']}")
    print(f"   Polymarket markets: {baseline['polymarket_total']}")


# ═══════════════════════════════════════════════════════════════════════
# STEP 2: Embedding Generation
# ═══════════════════════════════════════════════════════════════════════

def generate_embeddings(markets: List[Market], config: CalibrationConfig) -> np.ndarray:
    """Generate embeddings for markets - NO DECISIONS, just encoding"""
    client = OpenAI(api_key=os.getenv("OPENAI_API_KEY"))
    
    texts = [m.embedding_text for m in markets]
    
    print(f"🔄 Generating embeddings for {len(texts)} markets...")
    print(f"   Model: {config.embedding_model}")
    print(f"   Using: EVENT + TIME WINDOW (not RULES)")
    
    response = client.embeddings.create(
        model=config.embedding_model,
        input=texts
    )
    
    embeddings = np.array([item.embedding for item in response.data])
    
    print(f"✅ Generated {embeddings.shape[0]} embeddings of dimension {embeddings.shape[1]}")
    
    return embeddings


# ═══════════════════════════════════════════════════════════════════════
# STEP 3: Similarity Profiling (NO THRESHOLDS)
# ═══════════════════════════════════════════════════════════════════════

def compute_similarity_profiles(kalshi_markets: List[Market],
                               polymarket_markets: List[Market],
                               kalshi_embeddings: np.ndarray,
                               poly_embeddings: np.ndarray,
                               config: CalibrationConfig) -> List[SimilarityProfile]:
    """Compute similarity profiles - LOG DISTRIBUTIONS, make NO DECISIONS"""
    
    print(f"\n🔄 Computing similarity profiles...")
    print(f"   Kalshi markets: {len(kalshi_markets)}")
    print(f"   Polymarket markets: {len(polymarket_markets)}")
    print(f"   Total comparisons: {len(kalshi_markets) * len(polymarket_markets):,}")
    print(f"   ⚠️  NO THRESHOLDS - just measuring distributions")
    
    # Compute all pairwise similarities
    similarities = cosine_similarity(kalshi_embeddings, poly_embeddings)
    
    profiles = []
    for i, kalshi_market in enumerate(kalshi_markets):
        sims = similarities[i]
        
        # Get top-k for each k
        top_k_sims = {}
        for k in config.top_k_similarities:
            top_k_indices = np.argsort(sims)[-k:][::-1]
            top_k_sims[k] = [
                (polymarket_markets[idx].market_fingerprint, float(sims[idx]))
                for idx in top_k_indices
            ]
        
        profile = SimilarityProfile(
            kalshi_fingerprint=kalshi_market.market_fingerprint,
            kalshi_ticker=kalshi_market.market_ticker,
            kalshi_event_text=kalshi_market.event_text,
            top_similarities=top_k_sims
        )
        profiles.append(profile)
    
    print(f"✅ Computed {len(profiles)} similarity profiles")
    
    return profiles


def save_similarity_profiles(profiles: List[SimilarityProfile], output_dir: str):
    """Save similarity profiles - RAW DATA, no interpretation"""
    output_path = Path(output_dir) / "similarity_profiles.json"
    
    data = {
        "timestamp": datetime.utcnow().isoformat(),
        "note": "These are RAW similarity scores - NO THRESHOLDS applied, NO MATCHES made",
        "profiles": [p.to_dict() for p in profiles]
    }
    
    with open(output_path, 'w') as f:
        json.dump(data, f, indent=2)
    
    print(f"✅ Similarity profiles saved to {output_path}")


# ═══════════════════════════════════════════════════════════════════════
# STEP 4: Stratified Sampling (PERCENTILES, not fixed thresholds)
# ═══════════════════════════════════════════════════════════════════════

def stratified_sampling(profiles: List[SimilarityProfile],
                       kalshi_markets: List[Market],
                       polymarket_markets: List[Market],
                       config: CalibrationConfig) -> Dict[str, List[MarketPair]]:
    """Sample pairs from different similarity ranges - EXPLORATORY, not decisive"""
    
    print(f"\n🔄 Stratified sampling for LLM analysis...")
    print(f"   Using PERCENTILES, not fixed thresholds")
    print(f"   Sample size per bucket: {config.sample_size_per_bucket}")
    
    # Extract all top-1 similarities
    top_1_scores = [p.top_similarities[1][0][1] for p in profiles]
    
    # Calculate percentiles
    p33 = np.percentile(top_1_scores, 33)
    p67 = np.percentile(top_1_scores, 67)
    
    print(f"   Similarity distribution:")
    print(f"   - Min: {min(top_1_scores):.4f}")
    print(f"   - 33rd percentile: {p33:.4f}")
    print(f"   - 67th percentile: {p67:.4f}")
    print(f"   - Max: {max(top_1_scores):.4f}")
    
    # Build lookup dictionaries
    kalshi_by_fp = {m.market_fingerprint: m for m in kalshi_markets}
    poly_by_fp = {m.market_fingerprint: m for m in polymarket_markets}
    
    # Bucket pairs
    buckets = {"high": [], "medium": [], "low": []}
    
    for profile in profiles:
        poly_fp, score = profile.top_similarities[1][0]
        
        if score >= p67:
            bucket = "high"
        elif score >= p33:
            bucket = "medium"
        else:
            bucket = "low"
        
        kalshi_market = kalshi_by_fp[profile.kalshi_fingerprint]
        poly_market = poly_by_fp[poly_fp]
        
        pair = MarketPair(
            kalshi_market=kalshi_market,
            polymarket_market=poly_market,
            similarity_score=score,
            bucket=bucket
        )
        buckets[bucket].append(pair)
    
    # Sample from each bucket
    sampled = {}
    for bucket_name, pairs in buckets.items():
        sample_size = min(config.sample_size_per_bucket, len(pairs))
        sampled[bucket_name] = np.random.choice(pairs, size=sample_size, replace=False).tolist()
        print(f"   {bucket_name.capitalize()} similarity: sampled {sample_size} from {len(pairs)} pairs")
    
    # Save sampled pairs
    output_path = Path(config.output_dir) / "sampled_pairs_for_llm.json"
    sampled_data = {
        "timestamp": datetime.utcnow().isoformat(),
        "percentiles": {"p33": p33, "p67": p67},
        "note": "Pairs sampled using PERCENTILES - no fixed thresholds",
        "buckets": {
            bucket_name: [
                {
                    "kalshi_fingerprint": pair.kalshi_market.market_fingerprint,
                    "kalshi_ticker": pair.kalshi_market.market_ticker,
                    "kalshi_event": pair.kalshi_market.event_text,
                    "polymarket_fingerprint": pair.polymarket_market.market_fingerprint,
                    "polymarket_ticker": pair.polymarket_market.market_ticker,
                    "polymarket_event": pair.polymarket_market.event_text,
                    "similarity_score": pair.similarity_score,
                    "bucket": bucket_name
                }
                for pair in pairs
            ]
            for bucket_name, pairs in sampled.items()
        }
    }
    
    with open(output_path, 'w') as f:
        json.dump(sampled_data, f, indent=2)
    
    print(f"✅ Sampled pairs saved to {output_path}")
    
    return sampled


# ═══════════════════════════════════════════════════════════════════════
# STEP 5: LLM Descriptions (DESCRIPTIVE ONLY, no verdicts)
# ═══════════════════════════════════════════════════════════════════════

def get_llm_description(pair: MarketPair, config: CalibrationConfig) -> LLMDescription:
    """Get descriptive analysis from LLM - FREE TEXT, NO ENUMS, NO VERDICTS"""
    
    client = Anthropic(api_key=os.getenv("ANTHROPIC_API_KEY"))
    
    prompt = f"""Du är analytiker som studerar prediction markets.

Market A (Kalshi):
{pair.kalshi_market.llm_text}

Market B (Polymarket):
{pair.polymarket_market.llm_text}

Cosine similarity score: {pair.similarity_score:.4f}

Uppgift:
1. Beskriv kort vad Market A handlar om
2. Beskriv kort vad Market B handlar om  
3. Resonera om de verkar relatera till samma underliggande händelse eller verklighetstillstånd

VIKTIGT:
- Använd fri text
- Dra inga definitiva slutsatser
- Säg INTE "yes/no" eller "match/no match"
- Resonera öppet om eventuella kopplingar eller skillnader
"""
    
    response = client.messages.create(
        model=config.llm_model,
        max_tokens=1000,
        temperature=config.llm_temperature,
        messages=[{"role": "user", "content": prompt}]
    )
    
    description = response.content[0].text
    
    return LLMDescription(
        pair_id=f"{pair.kalshi_market.market_fingerprint[:8]}_{pair.polymarket_market.market_fingerprint[:8]}",
        kalshi_fingerprint=pair.kalshi_market.market_fingerprint,
        polymarket_fingerprint=pair.polymarket_market.market_fingerprint,
        similarity_score=pair.similarity_score,
        bucket=pair.bucket,
        llm_description=description,
        timestamp=datetime.utcnow().isoformat()
    )


def analyze_pairs_with_llm(sampled_buckets: Dict[str, List[MarketPair]],
                          config: CalibrationConfig) -> List[LLMDescription]:
    """Analyze sampled pairs with LLM - DESCRIPTIVE ONLY"""
    
    print(f"\n🔄 Getting LLM descriptions...")
    print(f"   Model: {config.llm_model}")
    print(f"   Temperature: {config.llm_temperature} (deterministic)")
    print(f"   ⚠️  LLM is DESCRIBING, not DECIDING")
    
    descriptions = []
    
    for bucket_name, pairs in sampled_buckets.items():
        print(f"\n   Processing {bucket_name} similarity bucket ({len(pairs)} pairs)...")
        for i, pair in enumerate(pairs, 1):
            print(f"      {i}/{len(pairs)}: {pair.kalshi_market.market_ticker} <-> {pair.polymarket_market.market_ticker}")
            desc = get_llm_description(pair, config)
            descriptions.append(desc)
    
    # Save descriptions
    output_path = Path(config.output_dir) / "llm_descriptions.json"
    descriptions_data = {
        "timestamp": datetime.utcnow().isoformat(),
        "model": config.llm_model,
        "temperature": config.llm_temperature,
        "note": "LLM descriptions are EXPLORATORY - no verdicts, no enums, no decisions",
        "descriptions": [asdict(d) for d in descriptions]
    }
    
    with open(output_path, 'w') as f:
        json.dump(descriptions_data, f, indent=2)
    
    print(f"\n✅ LLM descriptions saved to {output_path}")
    
    return descriptions


# ═══════════════════════════════════════════════════════════════════════
# STEP 6: Final Calibration Report
# ═══════════════════════════════════════════════════════════════════════

def generate_calibration_summary(profiles: List[SimilarityProfile],
                                descriptions: List[LLMDescription],
                                config: CalibrationConfig):
    """Generate human-readable calibration summary"""
    
    print(f"\n🔄 Generating calibration summary...")
    
    # Extract top-1 similarities
    top_1_scores = [p.top_similarities[1][0][1] for p in profiles]
    
    summary_lines = [
        "╔═══════════════════════════════════════════════════════════════════════╗",
        "║                                                                       ║",
        "║              MARKET SIMILARITY CALIBRATION SUMMARY                   ║",
        "║                                                                       ║",
        "╚═══════════════════════════════════════════════════════════════════════╝",
        "",
        f"Timestamp: {datetime.utcnow().isoformat()}",
        f"Embedding Model: {config.embedding_model}",
        f"LLM Model: {config.llm_model}",
        "",
        "═══════════════════════════════════════════════════════════════════════",
        "SIMILARITY DISTRIBUTION (Top-1 scores)",
        "═══════════════════════════════════════════════════════════════════════",
        "",
        f"Total Kalshi markets: {len(profiles)}",
        f"Min similarity: {min(top_1_scores):.4f}",
        f"25th percentile: {np.percentile(top_1_scores, 25):.4f}",
        f"Median: {np.percentile(top_1_scores, 50):.4f}",
        f"75th percentile: {np.percentile(top_1_scores, 75):.4f}",
        f"Max similarity: {max(top_1_scores):.4f}",
        "",
        "═══════════════════════════════════════════════════════════════════════",
        "KEY OBSERVATIONS (NO CONCLUSIONS)",
        "═══════════════════════════════════════════════════════════════════════",
        "",
        "1. Distribution Shape:",
        f"   - Range span: {max(top_1_scores) - min(top_1_scores):.4f}",
        f"   - Standard deviation: {np.std(top_1_scores):.4f}",
        "",
        "2. LLM Analysis:",
        f"   - Total pairs analyzed: {len(descriptions)}",
        f"   - High similarity pairs: {sum(1 for d in descriptions if d.bucket == 'high')}",
        f"   - Medium similarity pairs: {sum(1 for d in descriptions if d.bucket == 'medium')}",
        f"   - Low similarity pairs: {sum(1 for d in descriptions if d.bucket == 'low')}",
        "",
        "3. Next Steps (Manual Review Needed):",
        "   - Review llm_descriptions.json for qualitative insights",
        "   - Look for cases where similarity and LLM reasoning diverge",
        "   - Identify edge cases and boundary conditions",
        "   - Determine if embeddings alone are sufficient signal",
        "",
        "═══════════════════════════════════════════════════════════════════════",
        "IMPORTANT NOTES",
        "═══════════════════════════════════════════════════════════════════════",
        "",
        "⚠️  NO THRESHOLDS have been set",
        "⚠️  NO MATCHES have been made",
        "⚠️  NO DECISIONS have been automated",
        "",
        "This calibration provides SIGNAL UNDERSTANDING, not MATCHING LOGIC.",
        "",
        "Review the data, understand the distributions, then design",
        "matching logic in the next phase with full context.",
        "",
        "═══════════════════════════════════════════════════════════════════════",
    ]
    
    summary_text = "\n".join(summary_lines)
    
    output_path = Path(config.output_dir) / "calibration_summary.txt"
    with open(output_path, 'w') as f:
        f.write(summary_text)
    
    print(f"✅ Calibration summary saved to {output_path}")
    print("")
    print(summary_text)


# ═══════════════════════════════════════════════════════════════════════
# MAIN EXECUTION
# ═══════════════════════════════════════════════════════════════════════

def main():
    """Main calibration pipeline - MEASUREMENT ONLY"""
    
    print("\n╔═══════════════════════════════════════════════════════════════════════╗")
    print("║                                                                       ║")
    print("║         Market Similarity Calibration (NO MATCHING)                  ║")
    print("║                                                                       ║")
    print("╚═══════════════════════════════════════════════════════════════════════╝\n")
    
    # Initialize configuration
    config = CalibrationConfig()
    
    # Create output directory
    Path(config.output_dir).mkdir(parents=True, exist_ok=True)
    
    # Step 1: Load and filter markets
    print("📊 STEP 1: Loading markets...")
    kalshi_markets = load_markets_from_jsonl(config.kalshi_jsonl)
    polymarket_markets = load_markets_from_jsonl(config.polymarket_jsonl)
    
    kalshi_active = filter_active_markets(kalshi_markets)
    poly_active = filter_active_markets(polymarket_markets)
    
    generate_baseline_counts(kalshi_active, poly_active, config.output_dir)
    
    # Step 2: Generate embeddings
    print("\n📊 STEP 2: Generating embeddings...")
    kalshi_embeddings = generate_embeddings(kalshi_active, config)
    poly_embeddings = generate_embeddings(poly_active, config)
    
    # Step 3: Compute similarity profiles
    print("\n📊 STEP 3: Computing similarity profiles...")
    profiles = compute_similarity_profiles(
        kalshi_active, poly_active,
        kalshi_embeddings, poly_embeddings,
        config
    )
    save_similarity_profiles(profiles, config.output_dir)
    
    # Step 4: Stratified sampling
    print("\n📊 STEP 4: Stratified sampling for LLM analysis...")
    sampled_buckets = stratified_sampling(profiles, kalshi_active, poly_active, config)
    
    # Step 5: LLM descriptions
    print("\n📊 STEP 5: Getting LLM descriptions...")
    descriptions = analyze_pairs_with_llm(sampled_buckets, config)
    
    # Step 6: Final report
    print("\n📊 STEP 6: Generating calibration summary...")
    generate_calibration_summary(profiles, descriptions, config)
    
    print("\n╔═══════════════════════════════════════════════════════════════════════╗")
    print("║                                                                       ║")
    print("║                   ✅ CALIBRATION COMPLETE                            ║")
    print("║                                                                       ║")
    print("║   Review the reports in data/reports/ before proceeding.            ║")
    print("║   This was MEASUREMENT, not MATCHING.                                ║")
    print("║                                                                       ║")
    print("╚═══════════════════════════════════════════════════════════════════════╝\n")


if __name__ == "__main__":
    main()
