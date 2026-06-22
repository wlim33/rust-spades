use trick_notation::{Card, Deck, Event, Meta, Model, from_text, to_text};

fn card(rank: &str, suit: &str) -> Card {
    Card::Suited { suit: suit.into(), rank: rank.into() }
}

fn sample_model() -> Model {
    Model {
        meta: Meta {
            version: 1,
            game_hint: Some("hearts".into()),
            seats: vec!["N".into(), "E".into(), "S".into(), "W".into()],
            dealer: Some("N".into()),
            players: vec![Some("Ann".into()), None, Some("Cy".into()), None],
            partnerships: None,
            caps: vec!["exchange".into()],
            extra: vec![],
        },
        deck: Deck::french52(),
        events: vec![
            Event::Deal { hands: vec![("N".into(), vec![card("A", "S"), card("K", "H")])] },
            Event::Exchange { from: "N".into(), to: "E".into(), cards: vec![card("2", "C")] },
            Event::Play { leader: "S".into(), cards: vec![card("2", "C"), card("5", "C")] },
        ],
    }
}

#[test]
fn text_to_model_to_text_is_stable() {
    let m = sample_model();
    let text1 = to_text(&m);
    let parsed = from_text(&text1).expect("parse");
    assert_eq!(parsed, m);
    let text2 = to_text(&parsed);
    assert_eq!(text1, text2);
}

#[test]
fn fuzz_garbage_never_panics() {
    let inputs = [
        "",
        "garbage",
        "% trick-notation v1\n[Deck \"nope\"]\n",
        "% trick-notation v1\n[Seats \"N E S W\"]\nP\n",
        "% trick-notation v1\n[Deck \"french52\"]\n[Seats \"N E\"]\nD N:ZZ.-.-.-\n",
        "% trick-notation v1\n[Deck \"french52\"]\n[Seats \"N E S W\"]\nQ foo\n",
    ];
    for inp in inputs {
        // Must return Err, not panic.
        let _ = from_text(inp);
    }
}
