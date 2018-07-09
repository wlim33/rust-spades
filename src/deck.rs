extern crate rand;

use self::rand::{thread_rng, Rng};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Suit {
    Blank = 0,
    Club = 1,
    Diamond = 2,
    Heart = 3,
    Spade = 4,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rank {
    Blank = 1,
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

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank
}

pub fn get_trick_winner(index: usize, others: &[Card ; 4]) -> usize {
    let mut winning_index = index;
    let mut max_card = &others[index];

    for index in 0..4 {
        let other = &others[index];
        if other.suit == max_card.suit {
            if other.rank as u8  > max_card.rank as u8 {
                max_card = &other;
                winning_index = index;
            }
        } else if other.suit == Suit::Spade {
            max_card = &other;
            winning_index = index;
        }
    }
    return winning_index;
}




pub fn new() -> Vec<Card> {
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

pub fn new_pot() -> [Card; 4] {
    [
        Card { suit: Suit::Blank, rank: Rank::Blank}, 
        Card { suit: Suit::Blank, rank: Rank::Blank},
        Card { suit: Suit::Blank, rank: Rank::Blank},
        Card { suit: Suit::Blank, rank: Rank::Blank}
    ]
}

pub fn shuffle(cards: &mut Vec<Card>) {
    let mut rng = thread_rng();
    rng.shuffle(cards);
}

pub fn deal_four_players(cards: &mut Vec<Card>) -> Vec<Vec<Card>> {
    shuffle(cards);
    let mut hands = vec![vec![], vec![], vec![], vec![]];

    let mut i = 0;
    while cards.len() > 0 {
        &hands[i].push(cards.pop().unwrap());
        i = (i + 1) % 4;
    }

    return hands;
}

