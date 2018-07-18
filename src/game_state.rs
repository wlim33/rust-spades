/// Current game stage, field of `Game`. 
/// 
/// The `Betting` and `Trick` variants have a `usize` value between 0
/// and 3, inclusive, that refers to the number of players that have placed bets or played cards in the trick, 
/// respectively.
/// 
/// **Example:** `State::Trick(2)` means the game is in the card playing stage, and two players have played their cards.
#[derive(Debug, PartialEq)]
pub enum State {
    NotStarted,
    Betting(usize),
    Trick(usize),
    Completed
}
