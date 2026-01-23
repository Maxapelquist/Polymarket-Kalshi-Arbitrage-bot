# Arkitektur-fixar

## ARM64/x86_64-problemet

### Problem
På Apple Silicon (arm64) byggdes `market-observer` först som x86_64 (Rosetta) vilket gjorde att `sentence_transformers` (och framförallt `regex`-extensionen) inte kunde laddas:

```
dlopen ... incompatible architecture (have arm64, need x86_64)
```

### Diagnos
- `file target/debug/market-observer` visade `x86_64`
- `rustc -vV | grep host` visade `x86_64-apple-darwin`
- Python i `.venv` är `arm64`

### Fix
Bygga/köra som ARM64:
```bash
rustup target add aarch64-apple-darwin
cargo build --target aarch64-apple-darwin
arch -arm64 target/aarch64-apple-darwin/debug/market-observer ...
```

Eller använd systemets Python:
```bash
export PYTHON=/Library/Frameworks/Python.framework/Versions/3.10/bin/python3
cargo run match_events_embeddings
```
