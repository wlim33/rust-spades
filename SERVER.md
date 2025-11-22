# Spades Server Mode

The rust-spades library now includes an optional server mode that allows you to run multiple concurrent games via a REST API.

## Building the Server

To build the server, enable the `server` feature:

```bash
cargo build --features server --bin spades-server
```

## Running the Server

Start the server with:

```bash
cargo run --features server --bin spades-server
```

The server will listen on `0.0.0.0:3000` by default.

## API Endpoints

### Root
- **GET /** - Get API information
  ```bash
  curl http://localhost:3000/
  ```

### Create a New Game
- **POST /games** - Create a new game
  - Request body: `{"max_points": 500}`
  - Returns: `{"game_id": "<uuid>", "player_ids": ["<uuid>", ...]}`
  
  ```bash
  curl -X POST http://localhost:3000/games \
    -H "Content-Type: application/json" \
    -d '{"max_points": 500}'
  ```

### List Games
- **GET /games** - List all active games
  - Returns: Array of game UUIDs
  
  ```bash
  curl http://localhost:3000/games
  ```

### Get Game State
- **GET /games/:game_id** - Get the current state of a game
  - Returns: Game state including scores, bags, and current player
  
  ```bash
  curl http://localhost:3000/games/<game_id>
  ```

### Make a Game Transition
- **POST /games/:game_id/transition** - Make a move (start, bet, or play card)
  - Request body examples:
    - Start game: `{"type": "start"}`
    - Place bet: `{"type": "bet", "amount": 3}`
    - Play card: `{"type": "card", "card": {"suit": "Spade", "rank": "Ace"}}`
  
  ```bash
  # Start a game
  curl -X POST http://localhost:3000/games/<game_id>/transition \
    -H "Content-Type: application/json" \
    -d '{"type": "start"}'
  
  # Place a bet
  curl -X POST http://localhost:3000/games/<game_id>/transition \
    -H "Content-Type: application/json" \
    -d '{"type": "bet", "amount": 3}'
  
  # Play a card
  curl -X POST http://localhost:3000/games/<game_id>/transition \
    -H "Content-Type: application/json" \
    -d '{"type": "card", "card": {"suit": "Spade", "rank": "Ace"}}'
  ```

### Get Player's Hand
- **GET /games/:game_id/players/:player_id/hand** - Get a player's current hand
  - Returns: Array of cards
  
  ```bash
  curl http://localhost:3000/games/<game_id>/players/<player_id>/hand
  ```

### Delete a Game
- **DELETE /games/:game_id** - Remove a game from the server
  
  ```bash
  curl -X DELETE http://localhost:3000/games/<game_id>
  ```

## Example Workflow

```bash
# 1. Create a new game
RESPONSE=$(curl -s -X POST http://localhost:3000/games \
  -H "Content-Type: application/json" \
  -d '{"max_points": 500}')

GAME_ID=$(echo $RESPONSE | jq -r '.game_id')
PLAYER_1=$(echo $RESPONSE | jq -r '.player_ids[0]')

# 2. Start the game
curl -X POST http://localhost:3000/games/$GAME_ID/transition \
  -H "Content-Type: application/json" \
  -d '{"type": "start"}'

# 3. Check game state
curl http://localhost:3000/games/$GAME_ID | jq .

# 4. Get player's hand
curl http://localhost:3000/games/$GAME_ID/players/$PLAYER_1/hand | jq .

# 5. Place bets for all 4 players
for i in {0..3}; do
  curl -X POST http://localhost:3000/games/$GAME_ID/transition \
    -H "Content-Type: application/json" \
    -d '{"type": "bet", "amount": 3}'
done

# 6. Play cards...
# (Continue playing the game by making card transitions)
```

## Using the GameManager in Your Code

You can also use the `GameManager` directly in your Rust code (requires the `server` feature):

```rust
use spades::game_manager::GameManager;
use spades::GameTransition;

fn main() {
    let manager = GameManager::new();
    
    // Create a game
    let response = manager.create_game(500).unwrap();
    let game_id = response.game_id;
    
    // Start the game
    manager.make_transition(game_id, GameTransition::Start).unwrap();
    
    // Get game state
    let state = manager.get_game_state(game_id).unwrap();
    println!("Game state: {:?}", state);
    
    // Get a player's hand
    let player_id = response.player_ids[0];
    let hand = manager.get_hand(game_id, player_id).unwrap();
    println!("Player's hand: {:?}", hand);
}
```

## Concurrent Games

The server is designed to handle multiple concurrent games efficiently. Each game is stored in a thread-safe data structure (`Arc<RwLock<HashMap<Uuid, Arc<RwLock<Game>>>>>`), allowing multiple games to be played simultaneously without interference.

## Architecture

- **GameManager**: Manages multiple game instances with thread-safe access
- **REST API**: Built with Axum web framework for high performance
- **Serialization**: All game types support JSON serialization via Serde
- **Backward Compatibility**: The original library API remains unchanged; server mode is completely optional

## Dependencies

The server mode adds the following dependencies (only when the `server` feature is enabled):
- `tokio` - Async runtime
- `axum` - Web framework
- `tower` - Service abstractions
- `tower-http` - HTTP middleware (CORS support)
- `serde` and `serde_json` - JSON serialization
