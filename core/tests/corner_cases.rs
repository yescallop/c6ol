use c6ol_core::game::{Move, Point, Record};

#[test]
#[should_panic]
fn place_in_corner() {
    let mut buf = vec![];

    let mov = Move::Place(Point::new(i16::MIN, i16::MIN), None);
    mov.encode(&mut buf, true);
    assert_eq!(Some(mov), Move::decode(&mut &buf[..], true));
    buf.clear();

    let mut record = Record::new();
    assert!(record.make_move(mov));
    record.encode(&mut buf, false);
    assert_eq!(Some(record), Record::decode(&mut &buf[..], false));
}
