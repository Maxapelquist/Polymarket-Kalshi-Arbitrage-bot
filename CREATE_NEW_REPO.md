# Skapa nytt GitHub-repo - Snabbguide

## Steg 1: Skapa repo på GitHub

Gå till: **https://github.com/new**

Fyll i:
- **Repository name**: `Polymarket-Kalshi-Arbitrage-bot`
- **Description**: `AI-based event and contract matching system for Kalshi ↔ Polymarket arbitrage`
- **Visibility**: Välj Private eller Public
- **VIKTIGT**: Checka INTE "Add a README file" eller "Initialize with .gitignore"
- Klicka **"Create repository"**

## Steg 2: Kopiera repo URL

Efter att repot är skapat, kopiera URL:en (t.ex. `https://github.com/ditt-användarnamn/Polymarket-Kalshi-Arbitrage-bot.git`)

## Steg 3: Kör dessa kommandon

```bash
# Ta bort gamla remote
git remote remove origin

# Lägg till ny remote (ersätt med din URL från steg 2)
git remote add origin https://github.com/DITT_ANVÄNDARNAMN/Polymarket-Kalshi-Arbitrage-bot.git

# Stage alla ändringar
git add .

# Commit
git commit -m "Initial commit: LLM-driven event matching pipeline

- Full data ingestion pipeline (Kalshi + Polymarket)
- LLM-based event classification and matching
- Market-level contract matching
- Local AI inference (Ollama/LM Studio/llama.cpp)
- Checkpoint/resume support
- Comprehensive debugging and statistics"

# Push till nytt repo
git push -u origin raw-api-rebuild

# Eller skapa main branch:
git checkout -b main
git push -u origin main
```

## Klart! 🎉

Ditt nya repo är nu skapat och all kod är pushad.
