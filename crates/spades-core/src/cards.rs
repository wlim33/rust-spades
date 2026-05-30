use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub enum Suit {
    Club = 1,
    Diamond = 2,
    Heart = 3,
    Spade = 4,
}

impl fmt::Debug for Suit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Suit::Club => write!(f, "\u{2667}"),
            Suit::Diamond => write!(f, "\u{2662}"),
            Suit::Heart => write!(f, "\u{2661}"),
            Suit::Spade => write!(f, "\u{2664}"),
        }
    }
}

impl Suit {
    /// Every suit in canonical (ascending) order.
    pub const ALL: [Suit; 4] = [Suit::Club, Suit::Diamond, Suit::Heart, Suit::Spade];
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
}

impl fmt::Debug for Rank {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Rank::Two => write!(f, "2"),
            Rank::Three => write!(f, "3"),
            Rank::Four => write!(f, "4"),
            Rank::Five => write!(f, "5"),
            Rank::Six => write!(f, "6"),
            Rank::Seven => write!(f, "7"),
            Rank::Eight => write!(f, "8"),
            Rank::Nine => write!(f, "9"),
            Rank::Ten => write!(f, "10"),
            Rank::Jack => write!(f, "J"),
            Rank::Queen => write!(f, "Q"),
            Rank::King => write!(f, "K"),
            Rank::Ace => write!(f, "A"),
        }
    }
}

impl Rank {
    /// Every rank in ascending order (`Two` … `Ace`).
    pub const ALL: [Rank; 13] = [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ];
}

/// Intuitive card struct. Ordered by suit (Club < Diamond < Heart < Spade),
/// then rank ascending — the derived lexicographic order over the fields.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} {:?}", self.suit, self.rank)
    }
}

/// Given four cards and a starting card, returns the winner of a trick.
///
/// The rules used to determine the winner of a trick are as follows:
/// * Spades trump all other suits
/// * The suit the first player (given by index) plays sets the suit of the trick
/// * The highest ranking spades card or card of suit of first player's card wins the trick.
pub fn get_trick_winner(index: usize, others: &[Card; 4]) -> usize {
    let mut winning_index = index;
    let mut max_card = &others[index];

    for (i, other) in others.iter().enumerate() {
        if other.suit == max_card.suit {
            if (other.rank as u8) > max_card.rank as u8 {
                max_card = other;
                winning_index = i;
            }
        } else if other.suit == Suit::Spade {
            max_card = other;
            winning_index = i;
        }
    }
    winning_index
}

/// Returns a full 52-card deck in canonical order (by suit, then rank).
/// Call [`shuffle`] before dealing.
pub fn new_deck() -> Vec<Card> {
    Suit::ALL
        .into_iter()
        .flat_map(|suit| Rank::ALL.into_iter().map(move |rank| Card { suit, rank }))
        .collect()
}

/// Shuffles a slice of cards in place, see [`rand::seq::SliceRandom::shuffle`].
pub fn shuffle(cards: &mut [Card]) {
    let mut rng = rand::rng();
    cards.shuffle(&mut rng);
}

/// Deals a 52-card deck out to four hands. Panics if `cards` does not have 52 elements.
/// Caller is responsible for shuffling first.
pub fn deal_four_players(cards: &mut Vec<Card>) -> Vec<Vec<Card>> {
    assert_eq!(cards.len(), 52);
    let mut hands = vec![vec![], vec![], vec![], vec![]];

    let mut i = 0;
    while let Some(element) = cards.pop() {
        hands[i].push(element);
        i = (i + 1) % 4;
    }

    hands
}
