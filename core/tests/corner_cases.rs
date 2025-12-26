#![allow(missing_docs)]

use c6ol_core::game::{Move, Point, Record, RecordEncodingScheme};

#[test]
fn old_place_in_corner() {
    let mut record = Record::new();

    for x in [-0x3fff, 0x3fff] {
        for y in [-0x3fff, 0x3fff] {
            let mov = Move::Place(Point::new(x, y), None);
            assert!(record.make_move(mov));
        }
    }

    let mut buf = vec![];
    let scheme = RecordEncodingScheme {
        all: false,
        delta: false,
    };
    record.encode(&mut buf, scheme);
    assert_eq!(Some(record), Record::decode(&mut &buf[..]));
}

#[test]
fn max_delta() {
    let mut record = Record::new();

    // 1. (+, +) -> (-, -)
    record.make_move(Move::Place(Point::new(0x3fff, 0x3fff), None));
    record.make_move(Move::Place(Point::new(-0x3fff, -0x3fff), None));

    // 2. (-, -) -> (+, +)
    record.make_move(Move::Place(Point::new(0x3fff, 0x3fff), None));

    // 3. (+, +) -> (-, +) (setup for next)
    record.make_move(Move::Place(Point::new(-0x3fff, 0x3fff), None));

    // 4. (-, +) -> (+, -)
    record.make_move(Move::Place(Point::new(0x3fff, -0x3fff), None));

    // 5. (+, -) -> (-, +)
    record.make_move(Move::Place(Point::new(-0x3fff, 0x3fff), None));

    let mut buf = vec![];
    record.encode(&mut buf, RecordEncodingScheme::past());
    assert_eq!(Some(record), Record::decode(&mut &buf[..]));
}
