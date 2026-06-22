use trick_notation::{from_text, to_text};

/// A canonical doc must parse and re-encode byte-identically (stable canonical form).
fn assert_canonical(text: &str) {
    let model = from_text(text).unwrap_or_else(|e| panic!("parse failed: {e}\n---\n{text}"));
    assert_eq!(to_text(&model), text, "re-encode differed");
}

#[test]
fn hearts_with_pass_phase() {
    let text = "\
% trick-notation v1
[Game \"hearts\"]
[Deck \"french52\"]
[Seats \"N E S W\"]
[Dealer \"N\"]
[Caps \"exchange\"]

D N:AK.T9.-.-
X N>E: 2C 7D KH
P S 2C 5C 9C KC
";
    assert_canonical(text);
}

#[test]
fn euchre_with_kitty_and_turnup() {
    let text = "\
% trick-notation v1
[Game \"euchre\"]
[Deck \"euchre24\"]
[Seats \"N E S W\"]
[Dealer \"N\"]
[Partnerships \"NS EW\"]
[Caps \"piles\"]

D E:AK.Q.-.JT S:JT.-.A.KQ W:9.AK.9.9 N:-.JT9.KJ.- @kitty:Q.-.QT.A
U @kitty:QD
C E: pass pass make N
P E AS KS QH JC
";
    assert_canonical(text);
}
