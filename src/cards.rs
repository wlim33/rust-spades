extern crate rand;

use self::rand::{thread_rng, Rng};
use std::fmt;
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Suit {
    Blank = 0,
    Club = 1,
    Diamond = 2,
    Heart = 3,
    Spade = 4,
}

impl fmt::Debug for Suit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Suit::Blank => write!(f, " "),
            Suit::Club => write!(f, "\u{2667}"),
            Suit::Diamond => write!(f, "\u{2662}"),
            Suit::Heart => write!(f, "\u{2661}"),
            Suit::Spade => write!(f, "\u{2664}"),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Rank {
    Blank = 0,
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
    Ace = 14
}
impl fmt::Debug for Rank {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Rank::Blank => write!(f, " "),
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

/// Intuitive card struct. Comparisons are made according to alphabetical order, ascending.
#[derive(Clone, PartialEq, Eq)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} {:?}",self.suit , self.rank)
    }
}

impl Ord for Card {
    fn cmp(&self, other: &Card) -> Ordering {
        ((self.suit as u64) * 15 + (self.rank as u64)).cmp(&(((other.suit as u64)* 15) + (other.rank as u64)))
    }
}

impl PartialOrd for Card {
    fn partial_cmp(&self, other: &Card) -> Option<Ordering> {
        Some(((self.suit as u64) * 15 + (self.rank as u64)).cmp(&(((other.suit as u64)* 15) + (other.rank as u64))))
    }
}




/// Given four cards and a starting card, returns the winner of a trick.
/// 
/// The rules used to determine the winner of a trick are as follows: 
/// * Spades trump all other suits
/// * The suit the first player (given by index) plays sets the suit of the trick
/// * The highest ranking spades card or card of suit of first player's card wins the trick.
pub fn get_trick_winner(index: usize, others: &[Card ; 4]) -> usize {
    let mut winning_index = index;
    let mut max_card = &others[index];

    for i in 0..4 {
        let other = &others[i];
        if other.suit == max_card.suit {
            if other.rank as u8  > max_card.rank as u8 {
                max_card = &other;
                winning_index = i;
            }
        } else if other.suit == Suit::Spade {
            max_card = &other;
            winning_index = i;
        }
    }
    return winning_index;
}



/// Returns a shuffled deck of [`deck::Card`](struct.Card.html)'s, with 52 elements.
pub fn new_deck() -> Vec<Card> {
    let ranks: Vec<Rank> = vec![
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
        Rank::Ace
    ];
    let suits: Vec<Suit> = vec![
        Suit::Club,
        Suit::Diamond,
        Suit::Heart,
        Suit::Spade,
    ];

    let mut cards = Vec::new();
    for s in &suits {
        for r in &ranks {
            cards.push(Card {suit:s.clone(), rank:r.clone()});
        }
    }
    shuffle(&mut cards);

    return cards;
}

/// Returns an array of `Blank` suited and ranked cards.
pub fn new_pot() -> [Card; 4] {
    [
        Card { suit: Suit::Blank, rank: Rank::Blank}, 
        Card { suit: Suit::Blank, rank: Rank::Blank},
        Card { suit: Suit::Blank, rank: Rank::Blank},
        Card { suit: Suit::Blank, rank: Rank::Blank}
    ]
}

/// Shuffles a `Vector` of cards in place, see [`rand::thread_rng::shuffle`](https://docs.rs/rand/0.5.4/rand/trait.Rng.html#method.shuffle).
pub fn shuffle(cards: &mut Vec<Card>) {
    let mut rng = thread_rng();
    rng.shuffle(cards);
}

/// Used to reshuffle a deck of cards, panics if the `cards` does not have 52 elements (should only be used on a "full" deck).
pub fn deal_four_players(cards: &mut Vec<Card>) -> Vec<Vec<Card>> {
    assert_eq!(cards.len(), 52);
    shuffle(cards);
    let mut hands = vec![vec![], vec![], vec![], vec![]];

    let mut i = 0;
    while cards.len() > 0 {
        &hands[i].push(cards.pop().unwrap());
        i = (i + 1) % 4;
    }

    return hands;
}

