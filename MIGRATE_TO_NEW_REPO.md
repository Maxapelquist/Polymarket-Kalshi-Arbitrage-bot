# Migrera till nytt GitHub-repo

## Steg 1: Skapa nytt GitHub-repository

### Alternativ A: Via GitHub web
1. Gå till https://github.com/new
2. Fyll i:
   - Repository name: `Polymarket-Kalshi-Arbitrage-bot` (eller ditt val)
   - Description: "AI-based event and contract matching system for Kalshi ↔ Polymarket arbitrage"
   - Visibility: Private eller Public
   - **VIKTIGT**: Checka INTE "Initialize with README" (vi har redan kod)
3. Klicka "Create repository"

### Alternativ B: Via GitHub CLI
```bash
gh repo create Polymarket-Kalshi-Arbitrage-bot --private --description "AI-based event and contract matching system"
```

## Steg 2: Uppdatera git remote

Efter att du skapat det nya repo, kör dessa kommandon:

```bash
# Ta bort gamla remote
git remote remove origin

# Lägg till ny remote (ersätt med ditt nya repo URL)
git remote add origin https://github.com/DITT_ANVÄNDARNAMN/Polymarket-Kalshi-Arbitrage-bot.git

# Verifiera
git remote -v
```

## Steg 3: Commit och push

```bash
# Stage alla ändringar
git add .

# Commit
git commit -m "Initial commit: LLM-driven event matching pipeline"

# Push till nytt repo
git push -u origin main
# eller om du är på annan branch:
git push -u origin raw-api-rebuild
```

## Steg 4: Skapa .gitignore (om saknas)

Se till att du har en .gitignore som exkluderar:
- `.venv/`
- `data/cache/`
- `secrets/`
- `*.tmp`
- `__pycache__/`
- `target/`

## Steg 5: Verifiera

Gå till ditt nya GitHub-repo och kontrollera att allt är där.
