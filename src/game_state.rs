#[derive(Debug, PartialEq)]
pub enum State {
    NotStarted,
    Betting(usize),
    Trick(usize),
    Completed
}
