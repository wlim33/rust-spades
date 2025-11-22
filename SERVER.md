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

### Logging

The server uses the `env_logger` crate for logging. You can control the log level using the `RUST_LOG` environment variable:

```bash
# Run with info-level logging (default)
RUST_LOG=info cargo run --features server --bin spades-server

# Run with debug-level logging for more detailed output
RUST_LOG=debug cargo run --features server --bin spades-server

# Run with trace-level logging for maximum verbosity
RUST_LOG=trace cargo run --features server --bin spades-server
```

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

### Without Persistence

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

### With SQLite Persistence

```rust
use spades::game_manager::GameManager;
use spades::GameTransition;

fn main() {
    // Create a manager with SQLite storage
    let manager = GameManager::with_storage("games.db").unwrap();
    
    // Create a game (automatically saved to database)
    let response = manager.create_game(500).unwrap();
    let game_id = response.game_id;
    
    // Start the game (state changes are automatically persisted)
    manager.make_transition(game_id, GameTransition::Start).unwrap();
    
    // Games are automatically loaded from storage when the manager is created
    // So games persist across server restarts
}
```

## SQLite Storage

The server now supports optional SQLite persistence for games. When enabled:

- **Automatic Persistence**: All game state changes are automatically saved to the database
- **Crash Recovery**: Games are automatically restored when the server restarts
- **Thread-Safe**: Storage operations are protected by mutexes for concurrent access
- **Zero Configuration**: Just specify a database file path

To use storage in your own code:

```rust
use spades::game_manager::GameManager;

// Create manager with persistence
let manager = GameManager::with_storage("path/to/games.db").unwrap();

// All operations automatically save to database
let game = manager.create_game(500).unwrap();
manager.make_transition(game.game_id, GameTransition::Start).unwrap();

// Games persist across restarts
drop(manager);

// Create a new manager - games are automatically loaded
let manager2 = GameManager::with_storage("path/to/games.db").unwrap();
let games = manager2.list_games().unwrap();
// games vector will contain the previously created game
```

You can also use the storage module directly:

```rust
use spades::storage::GameStorage;
use spades::Game;
use uuid::Uuid;

fn main() {
    let storage = GameStorage::new("games.db").unwrap();
    
    // Create and save a game
    let game = Game::new(
        Uuid::new_v4(),
        [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
        500
    );
    storage.save_game(&game).unwrap();
    
    // Load a game
    let loaded_game = storage.load_game(*game.get_id()).unwrap();
    
    // List all games
    let game_ids = storage.list_games().unwrap();
}
```

## Concurrent Games

The server is designed to handle multiple concurrent games efficiently. Each game is stored in a thread-safe data structure (`Arc<RwLock<HashMap<Uuid, Arc<RwLock<Game>>>>>`), allowing multiple games to be played simultaneously without interference.

When persistence is enabled, storage operations use mutexes to ensure thread-safe database access across concurrent requests.

## Architecture

- **GameManager**: Manages multiple game instances with thread-safe access and optional SQLite persistence
- **GameStorage**: Optional SQLite-based persistence layer for game state
- **REST API**: Built with Axum web framework for high performance
- **Logging**: Comprehensive logging using the `log` crate with `env_logger`
- **Serialization**: All game types support JSON serialization via Serde
- **Backward Compatibility**: The original library API remains unchanged; server mode is completely optional

## Robustness & Error Handling

The server and library include comprehensive error handling and logging:

- **Logging**: All operations log at appropriate levels (info, warn, error, debug)
- **Lock Management**: Proper handling of RwLock/Mutex poisoning
- **Error Types**: Strongly-typed errors with descriptive messages
- **Input Validation**: Invalid game transitions, card plays, and bets are caught and logged
- **Concurrent Access**: Thread-safe data structures prevent race conditions
- **Storage Errors**: Database errors are caught and properly propagated
- **Recovery**: Games can be recovered from storage after crashes or restarts

## Dependencies

The server mode adds the following dependencies (only when the `server` feature is enabled):
- `tokio` - Async runtime
- `axum` - Web framework
- `tower` - Service abstractions
- `tower-http` - HTTP middleware (CORS support)
- `serde` and `serde_json` - JSON serialization
- `log` - Logging facade
- `env_logger` - Logger implementation
- `rusqlite` - SQLite database support (with bundled feature for easy deployment)
