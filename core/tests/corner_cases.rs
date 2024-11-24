use c6ol_core::game::{Move, Point, Record};

#[test]
fn place_in_corner() {
    let mut buf = vec![];

    for x in [i16::MIN, i16::MAX] {
        for y in [i16::MIN, i16::MAX] {
            let mov = Move::Place(Point::new(x, y), None);
            mov.encode(&mut buf, true);
            assert_eq!(Some(mov), Move::decode(&mut &buf[..], true));
            buf.clear();

            let mut record = Record::new();
            assert!(record.make_move(mov));
            record.encode(&mut buf, false);
            assert_eq!(Some(record), Record::decode(&mut &buf[..], false));
            buf.clear();
        }
    }
}
