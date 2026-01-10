#!/usr/bin/env python3
"""
Generate semantic embeddings for event descriptions using sentence-transformers.

This script pre-computes embeddings for Kalshi and Polymarket events, storing
them in a JSON cache that can be loaded by the Rust matcher module.

Requirements:
    pip install sentence-transformers

Usage:
    python scripts/generate_embeddings.py --kalshi events_kalshi.json --poly events_poly.json -o embeddings.json

Input format (JSON):
    [
        {"event_id": "KXNBA-24-ABC", "description": "Lakers vs Warriors winner"},
        {"event_id": "KXNFL-24-XYZ", "description": "Chiefs ML"},
        ...
    ]

Output format (JSON):
    {
        "model": "all-MiniLM-L6-v2",
        "dimension": 384,
        "embeddings": {
            "KXNBA-24-ABC": [0.123, 0.456, ...],
            ...
        },
        "created_at": 1234567890
    }
"""

import argparse
import json
import sys
import time
from pathlib import Path
from typing import List, Dict, Any

try:
    from sentence_transformers import SentenceTransformer
except ImportError:
    print("ERROR: sentence-transformers not installed", file=sys.stderr)
    print("Install with: pip install sentence-transformers", file=sys.stderr)
    sys.exit(1)


DEFAULT_MODEL = "all-MiniLM-L6-v2"


def load_events(path: Path) -> List[Dict[str, str]]:
    """Load events from JSON file."""
    with open(path, "r") as f:
        events = json.load(f)
    
    # Validate format
    if not isinstance(events, list):
        raise ValueError(f"Expected list of events in {path}")
    
    for event in events:
        if "event_id" not in event or "description" not in event:
            raise ValueError(f"Event missing required fields: {event}")
    
    return events


def generate_embeddings(
    events: List[Dict[str, str]],
    model: SentenceTransformer,
    batch_size: int = 32,
) -> Dict[str, List[float]]:
    """Generate embeddings for all events."""
    event_ids = [e["event_id"] for e in events]
    descriptions = [e["description"] for e in events]
    
    print(f"Generating embeddings for {len(events)} events...")
    embeddings = model.encode(
        descriptions,
        batch_size=batch_size,
        show_progress_bar=True,
        convert_to_numpy=True,
    )
    
    # Convert to dict
    result = {}
    for event_id, embedding in zip(event_ids, embeddings):
        result[event_id] = embedding.tolist()
    
    return result


def main():
    parser = argparse.ArgumentParser(
        description="Generate semantic embeddings for event matching"
    )
    parser.add_argument(
        "--kalshi",
        type=Path,
        help="Path to Kalshi events JSON file",
    )
    parser.add_argument(
        "--poly",
        type=Path,
        help="Path to Polymarket events JSON file",
    )
    parser.add_argument(
        "--events",
        type=Path,
        help="Path to combined events JSON file (alternative to --kalshi/--poly)",
    )
    parser.add_argument(
        "-o", "--output",
        type=Path,
        default=Path("embeddings.json"),
        help="Output path for embeddings cache (default: embeddings.json)",
    )
    parser.add_argument(
        "--model",
        type=str,
        default=DEFAULT_MODEL,
        help=f"Sentence transformer model to use (default: {DEFAULT_MODEL})",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=32,
        help="Batch size for encoding (default: 32)",
    )

    args = parser.parse_args()

    # Validate inputs
    if not args.events and not (args.kalshi or args.poly):
        parser.error("Must provide either --events or at least one of --kalshi/--poly")

    # Load events
    all_events = []
    if args.events:
        all_events = load_events(args.events)
    else:
        if args.kalshi:
            kalshi_events = load_events(args.kalshi)
            all_events.extend(kalshi_events)
            print(f"Loaded {len(kalshi_events)} Kalshi events")
        if args.poly:
            poly_events = load_events(args.poly)
            all_events.extend(poly_events)
            print(f"Loaded {len(poly_events)} Poly events")

    if not all_events:
        print("ERROR: No events loaded", file=sys.stderr)
        sys.exit(1)

    print(f"Total events: {len(all_events)}")

    # Load model
    print(f"Loading model: {args.model}...")
    model = SentenceTransformer(args.model)
    dimension = model.get_sentence_embedding_dimension()
    print(f"Model loaded (dimension: {dimension})")

    # Generate embeddings
    embeddings = generate_embeddings(all_events, model, args.batch_size)

    # Create output
    output = {
        "model": args.model,
        "dimension": dimension,
        "embeddings": embeddings,
        "created_at": int(time.time()),
    }

    # Save to file
    print(f"Saving embeddings to {args.output}...")
    with open(args.output, "w") as f:
        json.dump(output, f, indent=2)

    print(f"✅ Done! Generated {len(embeddings)} embeddings")
    print(f"   Model: {args.model}")
    print(f"   Dimension: {dimension}")
    print(f"   Output: {args.output}")


if __name__ == "__main__":
    main()
