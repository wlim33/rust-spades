use super::super::cards::{Card, Suit, Rank, get_trick_winner, deal_four_players, new_pot};
use super::super::cards;
use ntest::test_case;
use std::cmp::Ordering;

#[test]
fn new_deck() {
    let x = cards::new_deck();
    assert_eq!(x.len(), 52);
}

#[test]
fn deal_deck_four_players() {
    let mut x = cards::new_deck();
    let hands = deal_four_players(&mut x);
    for hand in &hands {
        assert_eq!(hand.len(), 13);
    }
}

#[test_case(0, 1)]
#[test_case(1, 1)]
#[test_case(2, 1)]
#[test_case(3, 1)]
fn trick_winner_same_suit(start: usize, expected: usize) {
    let trick = [
        Card { suit: Suit::Club, rank: Rank::Two },
        Card { suit: Suit::Club, rank: Rank::Ace },
        Card { suit: Suit::Club, rank: Rank::King },
        Card { suit: Suit::Club, rank: Rank::Nine },
    ];
    assert_eq!(expected, get_trick_winner(start, &trick));
}

#[test_case(0, 3)]
#[test_case(1, 1)]
#[test_case(2, 1)]
#[test_case(3, 3)]
fn trick_winner_no_spades(start: usize, expected: usize) {
    let trick = [
        Card { suit: Suit::Diamond, rank: Rank::Two },
        Card { suit: Suit::Heart, rank: Rank::Ace },
        Card { suit: Suit::Heart, rank: Rank::King },
        Card { suit: Suit::Diamond, rank: Rank::Nine },
    ];
    assert_eq!(expected, get_trick_winner(start, &trick));
}

#[test_case(0, 2)]
#[test_case(1, 2)]
#[test_case(2, 2)]
#[test_case(3, 2)]
fn trick_winner_spades(start: usize, expected: usize) {
    let trick = [
        Card { suit: Suit::Diamond, rank: Rank::Two },
        Card { suit: Suit::Heart, rank: Rank::Ace },
        Card { suit: Suit::Spade, rank: Rank::Two },
        Card { suit: Suit::Diamond, rank: Rank::Nine },
    ];
    assert_eq!(expected, get_trick_winner(start, &trick));
}

#[test]
fn test_new_pot_returns_blank_cards() {
    let pot = new_pot();
    for card in &pot {
        assert_eq!(card.suit, Suit::Blank);
        assert_eq!(card.rank, Rank::Blank);
    }
}

#[test]
fn test_card_ord_comparison() {
    let low = Card { suit: Suit::Club, rank: Rank::Two };
    let high = Card { suit: Suit::Spade, rank: Rank::Ace };
    assert_eq!(low.cmp(&high), Ordering::Less);
    assert_eq!(high.cmp(&low), Ordering::Greater);

    let same = Card { suit: Suit::Heart, rank: Rank::King };
    let same2 = Card { suit: Suit::Heart, rank: Rank::King };
    assert_eq!(same.cmp(&same2), Ordering::Equal);
    assert_eq!(same.partial_cmp(&same2), Some(Ordering::Equal));
}

#[test]
fn test_get_trick_winner_multiple_spades() {
    let trick = [
        Card { suit: Suit::Club, rank: Rank::Ace },
        Card { suit: Suit::Spade, rank: Rank::Two },
        Card { suit: Suit::Spade, rank: Rank::King },
        Card { suit: Suit::Heart, rank: Rank::Ace },
    ];
    assert_eq!(2, get_trick_winner(0, &trick));
}

#[test]
fn test_get_trick_winner_leader_wins() {
    let trick = [
        Card { suit: Suit::Diamond, rank: Rank::Ace },
        Card { suit: Suit::Diamond, rank: Rank::King },
        Card { suit: Suit::Diamond, rank: Rank::Queen },
        Card { suit: Suit::Diamond, rank: Rank::Jack },
    ];
    assert_eq!(0, get_trick_winner(0, &trick));
}

#[test]
fn test_card_debug_format() {
    let card = Card { suit: Suit::Spade, rank: Rank::Ace };
    let debug = format!("{:?}", card);
    assert!(debug.contains("A"));

    let suit_debug = format!("{:?}", Suit::Heart);
    assert!(suit_debug.contains("\u{2661}"));

    let rank_debug = format!("{:?}", Rank::Ten);
    assert_eq!(rank_debug, "10");

    assert_eq!(format!("{:?}", Suit::Blank), " ");
    assert_eq!(format!("{:?}", Rank::Blank), " ");
}
