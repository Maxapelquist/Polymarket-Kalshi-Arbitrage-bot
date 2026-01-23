# Embedding Setup - Fixa Python-miljön

## Problem

Felet visar att Python-paketen i `.venv` är kompilerade för fel arkitektur:
```
mach-o file, but is an incompatible architecture (have 'arm64', need 'x86_64')
```

## Lösningar

### Alternativ 1: Använd systemets Python (Rekommenderat)

1. Installera sentence-transformers i systemets Python:
```bash
/Library/Frameworks/Python.framework/Versions/3.10/bin/python3 -m pip install sentence-transformers numpy
```

2. Sätt miljövariabel för att använda systemets Python:
```bash
export PYTHON=/Library/Frameworks/Python.framework/Versions/3.10/bin/python3
cargo run match_events_embeddings
```

### Alternativ 2: Fixa .venv

1. Ta bort gamla .venv:
```bash
rm -rf .venv
```

2. Skapa ny .venv med rätt arkitektur:
```bash
python3 -m venv .venv
source .venv/bin/activate
pip install sentence-transformers numpy
```

3. Verifiera arkitektur:
```bash
python3 -c "import platform; print(platform.machine())"
# Borde visa: arm64
```

### Alternativ 3: Använd Conda (om du har det)

```bash
conda create -n embeddings python=3.10
conda activate embeddings
pip install sentence-transformers numpy
export PYTHON=$(which python)
cargo run match_events_embeddings
```

## Verifiering

Testa att embedding-scriptet fungerar:
```bash
echo '{"items": [{"id": "test", "text": "Hello world"}]}' | python3 scripts/embed.py
```

Om det fungerar bör du se JSON med en embedding-vektor.
