//! SimHash on recorded press releases (Bureau §3.6 step 2). The fixtures are Federal Reserve FOMC
//! statements (US government works): the September text as the Board published it, the same text as a
//! wire carried it under its own headline, and the June statement, which reads much alike but is
//! another event. A wire that adds a dateline or a footer can drift past 3 bits (7 in a trial); such
//! copies are the embedding step's, not SimHash's.

#![allow(clippy::unwrap_used)]

use news_clerk::simhash::{distance, simhash};
use news_taster::config;

const BOARD: &str = include_str!("fixtures/fomc-2025-09-17-board.txt");
const WIRE: &str = include_str!("fixtures/fomc-2025-09-17-wire.txt");
const JUNE: &str = include_str!("fixtures/fomc-2025-06-18-board.txt");

#[test]
fn the_same_release_from_two_wires_is_within_the_threshold_and_another_event_is_not() {
    let max = config::v1().unwrap().simhash_distance;
    let (b, w, j) = (simhash(BOARD), simhash(WIRE), simhash(JUNE));
    assert!(distance(b, w) <= max, "Board vs wire: {}", distance(b, w));
    assert!(
        distance(b, j) > max,
        "September vs June: {}",
        distance(b, j)
    );
    assert!(distance(w, j) > max, "wire vs June: {}", distance(w, j));
}

#[test]
fn simhash_does_not_depend_on_case_or_spacing() {
    let squashed = BOARD.to_uppercase().replace("\n\n", "\n   ");
    assert_eq!(simhash(BOARD), simhash(&squashed));
}
