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

/// Escape a tag value for emission: `"` -> `\"`, `\` -> `\\`. Nothing else.
pub(super) fn escape_tag_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out
}

/// Unescape a tag value. Returns None on any unrecognized backslash sequence
/// or trailing backslash.
pub(super) fn unescape_tag_value(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                _ => return None,
            }
        } else if ch == '"' {
            // bare unescaped quote inside value: invalid
            return None;
        } else {
            out.push(ch);
        }
    }
    Some(out)
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

    #[test]
    fn escape_passthrough_for_safe_strings() {
        assert_eq!(escape_tag_value("Alice"), "Alice");
        assert_eq!(escape_tag_value(""), "");
        assert_eq!(escape_tag_value("hello world"), "hello world");
    }

    #[test]
    fn escape_handles_quotes_and_backslash() {
        assert_eq!(escape_tag_value("a\"b"), "a\\\"b");
        assert_eq!(escape_tag_value("a\\b"), "a\\\\b");
        assert_eq!(escape_tag_value("\\\""), "\\\\\\\"");
    }

    #[test]
    fn escape_unescape_round_trip() {
        for s in [
            "",
            "Alice",
            "a\"b",
            "a\\b",
            "\\",
            "\"",
            "a\"b\\c\"d",
            "Carol \"the queen\" Q",
        ] {
            let esc = escape_tag_value(s);
            assert_eq!(unescape_tag_value(&esc).as_deref(), Some(s), "round trip {:?}", s);
        }
    }

    #[test]
    fn unescape_rejects_bad_sequences() {
        assert_eq!(unescape_tag_value("\\n"), None, "\\n not allowed");
        assert_eq!(unescape_tag_value("\\t"), None);
        assert_eq!(unescape_tag_value("\\"), None, "trailing backslash");
        assert_eq!(unescape_tag_value("\"bare"), None, "bare quote inside value");
        assert_eq!(unescape_tag_value("safe\""), None);
    }
}
