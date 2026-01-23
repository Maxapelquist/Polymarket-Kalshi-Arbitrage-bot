#!/bin/bash
# Snabbguide för att skapa nytt GitHub-repo

echo "🚀 Skapar nytt GitHub-repo för Polymarket-Kalshi-Arbitrage-bot"
echo ""
echo "Steg 1: Skapa repo på GitHub"
echo "=============================="
echo "1. Öppna: https://github.com/new"
echo "2. Repository name: Polymarket-Kalshi-Arbitrage-bot"
echo "3. Description: AI-based event and contract matching system"
echo "4. Välj Private eller Public"
echo "5. Viktigt: Checka INTE 'Add a README file'"
echo "6. Klicka 'Create repository'"
echo ""
read -p "Har du skapat repot? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Skapa repot först på https://github.com/new"
    exit 1
fi

echo ""
echo "Steg 2: Ange repo URL"
echo "=============================="
read -p "Klistra in din repo URL (t.ex. https://github.com/ditt-användarnamn/Polymarket-Kalshi-Arbitrage-bot.git): " REPO_URL

if [ -z "$REPO_URL" ]; then
    echo "❌ Ingen URL angiven"
    exit 1
fi

echo ""
echo "Steg 3: Uppdaterar git remote..."
git remote remove origin 2>/dev/null || true
git remote add origin "$REPO_URL"
echo "✅ Remote uppdaterad"

echo ""
echo "Steg 4: Stage och commit..."
git add -A
git commit -m "Initial commit: LLM-driven event matching pipeline

Features:
- Full data ingestion (Kalshi + Polymarket)
- LLM-based event classification and matching
- Market-level contract matching  
- Local AI inference (Ollama/LM Studio/llama.cpp)
- Checkpoint/resume support
- Comprehensive debugging and statistics

Architecture:
- PHASE 1: Full data ingestion
- PHASE 2: Semantic categorization (LLM)
- PHASE 3: Event-level matching (LLM)
- PHASE 4: Contract-level matching
- PHASE 5: Runtime scanning"

echo "✅ Commit skapad"

echo ""
echo "Steg 5: Push till nytt repo..."
CURRENT_BRANCH=$(git branch --show-current)
echo "Pusher branch: $CURRENT_BRANCH"

# Skapa main branch om vi är på annan branch
if [ "$CURRENT_BRANCH" != "main" ]; then
    read -p "Vill du skapa 'main' branch? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        git checkout -b main
        CURRENT_BRANCH="main"
    fi
fi

git push -u origin "$CURRENT_BRANCH"

echo ""
echo "✅ Klart! Ditt nya repo är skapat och kod är pushad."
echo "🌐 Gå till: $REPO_URL"
