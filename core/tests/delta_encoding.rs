#![allow(missing_docs)]

use c6ol_core::game::{Direction, Move, Point, Record, RecordEncodingScheme, Stone};
use rand::prelude::*;

#[test]
fn test_delta_encoding_roundtrip() {
    let mut rng = rand::rng();

    for _ in 0..1000 {
        let mut record = Record::new();

        // Always start with a single stone move (black)
        let p1 = Point::new(rng.random_range(-10..10), rng.random_range(-10..10));
        assert!(record.make_move(Move::Place(p1, None)));

        for _ in 0..50 {
            let p1 = Point::new(rng.random_range(-100..100), rng.random_range(-100..100));
            let p2 = Point::new(rng.random_range(-100..100), rng.random_range(-100..100));

            record.make_move(Move::Place(p1, Some(p2)));
        }

        // Encode
        let encoded = record.encode_to_vec(RecordEncodingScheme::all());

        // Decode
        let decoded = Record::decode(&mut encoded.as_slice()).expect("Failed to decode");

        assert_eq!(record.moves(), decoded.moves());
    }
}

#[test]
fn test_delta_encoding_all_types() {
    let mut record = Record::new();

    // 1. Place single stone (First move)
    assert!(record.make_move(Move::Place(Point::new(0, 0), None)));

    // 2. Place two stones
    assert!(record.make_move(Move::Place(Point::new(1, 1), Some(Point::new(2, 2)))));

    // 3. Pass
    assert!(record.make_move(Move::Pass));

    // 4. Draw
    assert!(record.make_move(Move::Draw));

    // Encode/Decode
    let encoded = record.encode_to_vec(RecordEncodingScheme::all());
    let decoded = Record::decode(&mut encoded.as_slice()).expect("Failed to decode");
    assert_eq!(record.moves(), decoded.moves());

    // Reset and try Resign
    let mut record = Record::new();
    assert!(record.make_move(Move::Place(Point::new(0, 0), None)));
    assert!(record.make_move(Move::Resign(Stone::White)));

    let encoded = record.encode_to_vec(RecordEncodingScheme::all());
    let decoded = Record::decode(&mut encoded.as_slice()).expect("Failed to decode");
    assert_eq!(record.moves(), decoded.moves());
}

#[test]
fn test_delta_encoding_single_place_mid_game() {
    let mut record = Record::new();
    assert!(record.make_move(Move::Place(Point::new(0, 0), None)));

    // Try placing a single stone as second move
    let m = Move::Place(Point::new(1, 1), None);
    assert!(record.make_move(m));

    let m = Move::Place(Point::new(2, 2), Some(Point::new(2, 3)));
    assert!(record.make_move(m));

    let encoded = record.encode_to_vec(RecordEncodingScheme::all());
    let decoded = Record::decode(&mut encoded.as_slice()).expect("Failed to decode");
    assert_eq!(record.moves(), decoded.moves());
}

#[test]
fn test_delta_encoding_win_all_directions() {
    for dir in 0..8 {
        let dir = Direction::from_u8(dir).unwrap();

        let mut record = Record::new();
        // Black starts at (0,0)
        assert!(record.make_move(Move::Place(Point::ZERO, None)));

        // We need 5 more stones for Black: offset(1)..offset(5)
        let stones: Vec<Point> = (1..=5)
            .map(|i| Point::ZERO + dir.offset(i as i16))
            .collect();

        // White dummy moves base
        let mut white_y = 100;

        for chunk in stones.chunks(2) {
            // White dummy move
            assert!(record.make_move(Move::Place(
                Point::new(0, white_y),
                Some(Point::new(1, white_y))
            )));
            white_y += 1;

            if chunk.len() == 2 {
                assert!(record.make_move(Move::Place(chunk[0], Some(chunk[1]))));
            } else {
                // Last stone and a dummy stone
                assert!(record.make_move(Move::Place(chunk[0], Some(Point::new(-10, -10)))));
            }
        }

        // Now Black has 6 stones in a row (0..5).
        // Check win.
        let win = Move::Win(Point::ZERO, dir);
        assert!(
            record.make_move(win),
            "Failed to make win move for direction {:?}",
            dir
        );

        let encoded = record.encode_to_vec(RecordEncodingScheme::all());
        let decoded = Record::decode(&mut encoded.as_slice()).expect("Failed to decode");

        assert_eq!(
            record.moves(),
            decoded.moves(),
            "Mismatch for direction {:?}",
            dir
        );
    }
}
