use crate::cards::{Card, Rank, Suit};

pub(super) fn card_to_str(c: Card) -> [u8; 2] {
    [rank_byte(c.rank), suit_byte(c.suit)]
}

fn rank_byte(r: Rank) -> u8 {
    match r {
        Rank::Two => b'2',
        Rank::Three => b'3',
        Rank::Four => b'4',
        Rank::Five => b'5',
        Rank::Six => b'6',
        Rank::Seven => b'7',
        Rank::Eight => b'8',
        Rank::Nine => b'9',
        Rank::Ten => b'T',
        Rank::Jack => b'J',
        Rank::Queen => b'Q',
        Rank::King => b'K',
        Rank::Ace => b'A',
    }
}

fn suit_byte(s: Suit) -> u8 {
    match s {
        Suit::Club => b'C',
        Suit::Diamond => b'D',
        Suit::Heart => b'H',
        Suit::Spade => b'S',
    }
}

pub(super) fn parse_card(token: &str) -> Option<Card> {
    let bytes = token.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let rank = match bytes[0] {
        b'2' => Rank::Two,
        b'3' => Rank::Three,
        b'4' => Rank::Four,
        b'5' => Rank::Five,
        b'6' => Rank::Six,
        b'7' => Rank::Seven,
        b'8' => Rank::Eight,
        b'9' => Rank::Nine,
        b'T' => Rank::Ten,
        b'J' => Rank::Jack,
        b'Q' => Rank::Queen,
        b'K' => Rank::King,
        b'A' => Rank::Ace,
        _ => return None,
    };
    let suit = match bytes[1] {
        b'C' => Suit::Club,
        b'D' => Suit::Diamond,
        b'H' => Suit::Heart,
        b'S' => Suit::Spade,
        _ => return None,
    };
    Some(Card { rank, suit })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(c: Card) -> String {
        let b = card_to_str(c);
        String::from_utf8(b.to_vec()).unwrap()
    }

    #[test]
    fn every_card_round_trips() {
        for suit in [Suit::Club, Suit::Diamond, Suit::Heart, Suit::Spade] {
            for rank in [
                Rank::Two, Rank::Three, Rank::Four, Rank::Five, Rank::Six,
                Rank::Seven, Rank::Eight, Rank::Nine, Rank::Ten, Rank::Jack,
                Rank::Queen, Rank::King, Rank::Ace,
            ] {
                let c = Card { suit, rank };
                let txt = s(c);
                assert_eq!(parse_card(&txt), Some(c), "round trip {}", txt);
            }
        }
    }

    #[test]
    fn known_examples() {
        assert_eq!(s(Card { suit: Suit::Club, rank: Rank::Two }), "2C");
        assert_eq!(s(Card { suit: Suit::Club, rank: Rank::Ten }), "TC");
        assert_eq!(s(Card { suit: Suit::Spade, rank: Rank::Ace }), "AS");
        assert_eq!(s(Card { suit: Suit::Diamond, rank: Rank::King }), "KD");
        assert_eq!(s(Card { suit: Suit::Heart, rank: Rank::Jack }), "JH");
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(parse_card("").is_none());
        assert!(parse_card("A").is_none());
        assert!(parse_card("AKS").is_none());
        assert!(parse_card("1C").is_none(), "1 is not a valid rank");
        assert!(parse_card("AX").is_none(), "X is not a valid suit");
        assert!(parse_card("aC").is_none(), "lowercase rank not accepted");
        assert!(parse_card("Ac").is_none(), "lowercase suit not accepted");
        assert!(parse_card("0S").is_none());
        assert!(parse_card("TT").is_none());
    }
}
