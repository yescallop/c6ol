use super::{nibble::*, *};

#[ignore]
#[test]
fn test_pairing() {
    for z in 0..=u32::MAX {
        let (x, y) = szudzik_unpair(z);
        assert_eq!(z, szudzik_pair(x, y));
    }
}

#[ignore]
#[test]
fn test_d4_index() {
    for z in 0..128 {
        let p = Point::from_d4_index(z).unwrap();
        println!("{z}: {p:?}");
    }
    for z in 0..0x7fff * 0x7fff {
        let p = Point::from_d4_index(z).unwrap();
        assert_eq!(z, p.d4_index());
    }
}

#[ignore]
#[test]
fn test_d4_centrosymmetric_index() {
    for z in 0..128 {
        let p = Point::from_d4_centrosymmetric_index(z).unwrap();
        println!("{z}: {p:?}");
    }
    for z in 0..=2 * 0x8000 * 0x8000 - 2 * 0x8000 {
        let p = Point::from_d4_centrosymmetric_index(z).unwrap();
        assert_eq!(z, p.d4_centrosymmetric_index());
    }
}

#[test]
fn test_nibble_reader_overflow() {
    let mut buf = Vec::new();
    {
        let mut writer = NibbleWriter::new(&mut buf);
        writer.write_u32_varint(u32::MAX);
    }
    {
        let mut slice = buf.as_slice();
        let mut reader = NibbleReader::new(&mut slice);
        assert_eq!(reader.read_u32_varint(), Some(u32::MAX));
    }

    buf.clear();

    // 10 chunks of 0xF (payload 7, cont 1).
    buf.extend(iter::repeat_n(0xFF, 5));
    // 11th chunk.
    // If we write 4 (payload 4, cont 0), it should be overflow (bit 32 set).
    buf.push(0x84);

    {
        let mut slice = buf.as_slice();
        let mut reader = NibbleReader::new(&mut slice);
        assert_eq!(reader.read_u32_varint(), None);
    }

    buf.clear();

    // 10 chunks of 0xF (payload 7, cont 1).
    buf.extend(iter::repeat_n(0xFF, 5));
    // 11th chunk.
    // If we write 0 (payload 0, cont 0), it should be valid.
    buf.push(0x80);

    {
        let mut slice = buf.as_slice();
        let mut reader = NibbleReader::new(&mut slice);
        // 30 bits of 1s = 0x3FFFFFFF.
        assert_eq!(reader.read_u32_varint(), Some(0x3FFFFFFF));
    }
}

#[test]
fn test_special_case_encoding() {
    let mut record = Record::new();
    record.make_move(Move::Place(Point::ZERO, None));

    // Test with scheme::past()
    let encoded = record.encode_to_vec(RecordEncodingScheme::past().delta());
    assert_eq!(encoded, vec![0x02]);
    let decoded = Record::decode(&mut encoded.as_slice()).unwrap();
    assert_eq!(record.moves(), decoded.moves());

    // Test with scheme::all()
    let encoded = record.encode_to_vec(RecordEncodingScheme::all().delta());
    // scheme=3 (0011). index=1 (0001). special=0 (0000).
    // put_u4(3) -> p=3
    // put_u32_varint(1) -> put_u4(1) -> byte 0x13 (0001 0011). p=None.
    // put_u4(0) -> p=0.
    // drop -> byte 0xF0 (1000 0000).
    assert_eq!(encoded, vec![0x13, 0xF0]);
    let decoded = Record::decode(&mut encoded.as_slice()).unwrap();
    assert_eq!(record.moves(), decoded.moves());
}

#[test]
fn test_nibble_padding_and_varint() {
    let mut buf = Vec::new();
    {
        let mut writer = NibbleWriter::new(&mut buf);
        // Write 1 nibble.
        writer.write_u3(1);
        // Drop writer. Should add padding nibble F.
    }
    assert_eq!(buf, vec![0xF1]); // Low 1, High F.

    {
        let mut slice = buf.as_slice();
        let mut reader = NibbleReader::new(&mut slice);
        assert_eq!(reader.read_u3(), Some(1));
        assert_eq!(reader.read_u3(), None); // Should see F and return None.
    }

    buf.clear();
    {
        let mut writer = NibbleWriter::new(&mut buf);
        // Write 2 nibbles.
        writer.write_u3(1);
        writer.write_u3(2);
        // Drop writer. No padding needed.
    }
    assert_eq!(buf, vec![0x21]); // Low 1, High 2.

    {
        let mut slice = buf.as_slice();
        let mut reader = NibbleReader::new(&mut slice);
        assert_eq!(reader.read_u3(), Some(1));
        assert_eq!(reader.read_u3(), Some(2));
        assert_eq!(reader.read_u3(), None);
    }
}

#[test]
fn test_nibble_varint_boundaries() {
    let mut buf = Vec::new();
    {
        let mut writer = NibbleWriter::new(&mut buf);
        // 0 -> 0 (1 nibble)
        writer.write_u32_varint(0);
        // 7 -> 7 (1 nibble)
        writer.write_u32_varint(7);
        // 8 -> 8 (cont), 1 (payload) -> 0 | 8, 1 -> 8, 1 (2 nibbles)
        writer.write_u32_varint(8);
    }
    // 0, 7, 8, 1.
    // Byte 0: Low 0, High 7 -> 0x70.
    // Byte 1: Low 8, High 1 -> 0x18.
    assert_eq!(buf, vec![0x70, 0x18]);

    {
        let mut slice = buf.as_slice();
        let mut reader = NibbleReader::new(&mut slice);
        assert_eq!(reader.read_u32_varint(), Some(0));
        assert_eq!(reader.read_u32_varint(), Some(7));
        assert_eq!(reader.read_u32_varint(), Some(8));
        assert_eq!(reader.read_u32_varint(), None);
    }
}
