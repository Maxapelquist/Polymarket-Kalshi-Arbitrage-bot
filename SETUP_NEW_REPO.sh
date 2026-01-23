#!/bin/bash
# Script för att skapa nytt GitHub-repo och pusha kod

set -e

echo "🚀 Skapar nytt GitHub-repo..."
echo ""
echo "Steg 1: Skapa repo på GitHub"
echo "=============================="
echo "Gå till: https://github.com/new"
echo "Eller kör: gh repo create Polymarket-Kalshi-Arbitrage-bot --private"
echo ""
read -p "Har du skapat repo? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Skapa repo först och kör scriptet igen."
    exit 1
fi

echo ""
echo "Steg 2: Ange ditt nya repo URL"
echo "=============================="
read -p "GitHub repo URL (t.ex. https://github.com/ditt-användarnamn/repo-namn.git): " REPO_URL

if [ -z "$REPO_URL" ]; then
    echo "❌ Ingen URL angiven"
    exit 1
fi

echo ""
echo "Steg 3: Uppdaterar git remote..."
echo "=============================="

# Ta bort gamla remote om den finns
if git remote get-url origin > /dev/null 2>&1; then
    echo "Tar bort gamla remote..."
    git remote remove origin
fi

# Lägg till ny remote
echo "Lägger till ny remote: $REPO_URL"
git remote add origin "$REPO_URL"

# Verifiera
echo ""
echo "Verifierar remote:"
git remote -v

echo ""
echo "Steg 4: Stage och commit..."
echo "=============================="

# Stage alla ändringar
git add .

# Commit
echo "Skapar commit..."
git commit -m "Initial commit: LLM-driven event matching pipeline for Kalshi ↔ Polymarket arbitrage

- Full data ingestion pipeline
- LLM-based event classification and matching
- Market-level contract matching
- Local AI inference (Ollama/LM Studio/llama.cpp)
- Checkpoint/resume support
- Comprehensive debugging and statistics"

echo ""
echo "Steg 5: Push till nytt repo..."
echo "=============================="

# Hämta nuvarande branch
CURRENT_BRANCH=$(git branch --show-current)
echo "Nuvarande branch: $CURRENT_BRANCH"

# Push
git push -u origin "$CURRENT_BRANCH"

echo ""
echo "✅ Klart! Ditt nya repo är skapat och kod är pushad."
echo "Gå till: $REPO_URL"
