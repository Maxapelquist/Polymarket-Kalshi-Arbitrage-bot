#!/usr/bin/env python3
"""
Embedding script för bge-m3
Input: JSON via stdin med items [{id, text}]
Output: JSON via stdout med embeddings [{id, vector}]
"""

import json
import sys
import os

# Försök ladda sentence-transformers
try:
    from sentence_transformers import SentenceTransformer
    import numpy as np
    try:
        from tqdm import tqdm
        HAS_TQDM = True
    except ImportError:
        HAS_TQDM = False
except ImportError as e:
    print(f"Error importing sentence_transformers: {e}", file=sys.stderr)
    print("Please install: pip install sentence-transformers numpy tqdm", file=sys.stderr)
    sys.exit(1)

# Ladda modell (lazy loading - laddas första gången)
_model = None

def get_model():
    global _model
    if _model is None:
        print("Loading bge-m3 model...", file=sys.stderr)
        _model = SentenceTransformer('BAAI/bge-m3')
        
        # Detektera device
        import torch
        if torch.backends.mps.is_available():
            device = "mps"
        elif torch.cuda.is_available():
            device = "cuda"
        else:
            device = "cpu"
        print(f"Model loaded. Device: {device}", file=sys.stderr)
    return _model

def normalize_embedding(embedding):
    """Normalisera embedding till unit vector"""
    norm = np.linalg.norm(embedding)
    if norm == 0:
        return embedding
    return (embedding / norm).tolist()

def main():
    # Läs input från stdin
    input_data = json.load(sys.stdin)
    items = input_data.get("items", [])
    
    if not items:
        print(json.dumps({"embeddings": []}))
        return
    
    # Extrahera texts och ids
    texts = [item["text"] for item in items]
    ids = [item["id"] for item in items]
    
    total_items = len(items)
    batch_size = 32
    
    # Detektera device
    import torch
    if torch.backends.mps.is_available():
        device = "mps"
    elif torch.cuda.is_available():
        device = "cuda"
    else:
        device = "cpu"
    
    print(f"Processing {total_items} items in batches of {batch_size} (device: {device})", file=sys.stderr)
    
    # Generera embeddings (batch processing med progress)
    model = get_model()
    
    if HAS_TQDM:
        # Använd tqdm för progress bar
        embeddings = []
        for i in tqdm(range(0, len(texts), batch_size), desc="Embedding", unit="batch", file=sys.stderr):
            batch_texts = texts[i:i+batch_size]
            batch_embeddings = model.encode(
                batch_texts,
                normalize_embeddings=True,
                show_progress_bar=False,
                batch_size=batch_size
            )
            embeddings.extend(batch_embeddings)
    else:
        # Fallback utan tqdm
        embeddings = model.encode(
            texts,
            normalize_embeddings=True,
            show_progress_bar=True,  # sentence-transformers egen progress
            batch_size=batch_size
        )
    
    # Konvertera till listor och normalisera (extra säkerhet)
    results = []
    for i, (item_id, embedding) in enumerate(zip(ids, embeddings)):
        normalized = normalize_embedding(embedding)
        results.append({
            "id": item_id,
            "vector": normalized
        })
    
    # Output till stdout
    output = {"embeddings": results}
    print(json.dumps(output))

if __name__ == "__main__":
    main()
