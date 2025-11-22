#!/bin/bash
# Demo script for Spades Server API
# This script demonstrates creating and playing a game through the REST API

set -e

API_URL="${API_URL:-http://localhost:3000}"

echo "=== Spades Server Demo ==="
echo "API URL: $API_URL"
echo ""

# Check if server is running
echo "Checking server health..."
if ! curl -sf "$API_URL/" > /dev/null; then
    echo "Error: Server not responding at $API_URL"
    echo "Please start the server with: cargo run --features server --bin spades-server"
    exit 1
fi
echo "✓ Server is running"
echo ""

# Create a new game
echo "Creating a new game..."
GAME_RESPONSE=$(curl -s -X POST "$API_URL/games" \
    -H "Content-Type: application/json" \
    -d '{"max_points": 500}')
echo "$GAME_RESPONSE" | jq .
echo ""

GAME_ID=$(echo "$GAME_RESPONSE" | jq -r '.game_id')
PLAYER_0=$(echo "$GAME_RESPONSE" | jq -r '.player_ids[0]')
PLAYER_1=$(echo "$GAME_RESPONSE" | jq -r '.player_ids[1]')
PLAYER_2=$(echo "$GAME_RESPONSE" | jq -r '.player_ids[2]')
PLAYER_3=$(echo "$GAME_RESPONSE" | jq -r '.player_ids[3]')

echo "Game ID: $GAME_ID"
echo "Player IDs:"
echo "  Player 0 (Team A): $PLAYER_0"
echo "  Player 1 (Team B): $PLAYER_1"
echo "  Player 2 (Team A): $PLAYER_2"
echo "  Player 3 (Team B): $PLAYER_3"
echo ""

# Start the game
echo "Starting the game..."
curl -s -X POST "$API_URL/games/$GAME_ID/transition" \
    -H "Content-Type: application/json" \
    -d '{"type": "start"}' | jq .
echo ""

# Get game state
echo "Getting game state..."
curl -s "$API_URL/games/$GAME_ID" | jq .
echo ""

# Get player 0's hand
echo "Getting Player 0's hand..."
HAND=$(curl -s "$API_URL/games/$GAME_ID/players/$PLAYER_0/hand")
echo "$HAND" | jq .
echo ""

# Place bets for all players
echo "Placing bets for all players..."
for i in 0 1 2 3; do
    BET=$((3 + i % 2))  # Alternate between 3 and 4
    echo "  Player $i bets: $BET"
    curl -s -X POST "$API_URL/games/$GAME_ID/transition" \
        -H "Content-Type: application/json" \
        -d "{\"type\": \"bet\", \"amount\": $BET}" | jq -c .
done
echo ""

# Get updated game state
echo "Game state after betting:"
curl -s "$API_URL/games/$GAME_ID" | jq .
echo ""

# Play first trick (4 cards)
echo "Playing first trick..."
for i in 0 1 2 3; do
    PLAYER_VAR="PLAYER_$i"
    PLAYER_ID="${!PLAYER_VAR}"
    
    # Get player's hand
    HAND=$(curl -s "$API_URL/games/$GAME_ID/players/$PLAYER_ID/hand")
    
    # Play the first card in their hand
    CARD=$(echo "$HAND" | jq -c '.cards[0]')
    
    echo "  Player $i plays: $CARD"
    curl -s -X POST "$API_URL/games/$GAME_ID/transition" \
        -H "Content-Type: application/json" \
        -d "{\"type\": \"card\", \"card\": $CARD}" | jq -c .
done
echo ""

# Final game state
echo "Game state after first trick:"
curl -s "$API_URL/games/$GAME_ID" | jq .
echo ""

# List all games
echo "Listing all active games:"
curl -s "$API_URL/games" | jq .
echo ""

echo "=== Demo Complete ==="
echo "Game ID: $GAME_ID"
echo ""
echo "You can continue playing by making more transition requests:"
echo "  curl -X POST $API_URL/games/$GAME_ID/transition \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"type\": \"card\", \"card\": {\"suit\": \"Spade\", \"rank\": \"Ace\"}}'"
echo ""
echo "Or delete the game:"
echo "  curl -X DELETE $API_URL/games/$GAME_ID"
