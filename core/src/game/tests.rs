use super::{bit::*, *};

/// Maps two natural numbers to one.
fn pair(x: u16, y: u16) -> u32 {
    let (x, y) = (x as u32, y as u32);
    if x <= y {
        y * y + 2 * x
    } else {
        x * x + 2 * y + 1
    }
}

/// Maps one natural number to two (undoes `pair`).
fn unpair(z: u32) -> (u16, u16) {
    let s = z.isqrt();
    let r = z - s * s;
    let h = r / 2;
    if r & 1 == 0 {
        (h as u16, s as u16)
    } else {
        (s as u16, h as u16)
    }
}

#[ignore]
#[test]
fn test_pairing() {
    for z in 0..=u32::MAX {
        let (x, y) = szudzik_unpair(z);
        assert_eq!(z, szudzik_pair(x, y));

        let (x, y) = unpair(z);
        assert_eq!(z, pair(x, y));
    }
}

#[ignore]
#[test]
fn test_point_index() {
    for z in 0..128 {
        let p = Point::from_new_index(z).unwrap();
        println!("{z}: {p:?}");
    }
    for z in 0..0xffff * 0xffff {
        let p = Point::from_new_index(z).unwrap();
        assert_eq!(Some(z), p.new_index());
    }
}

#[ignore]
#[test]
fn test_point_sym_index() {
    for z in 0..128 {
        let p = Point::from_sym_index(z).unwrap();
        println!("{z}: {p:?}");
    }
    for z in 0..=2 * 0x8000 * 0x8000 - 2 * 0x8000 {
        let p = Point::from_sym_index(z).unwrap();
        assert_eq!(Some(z), p.sym_index());
    }
}

#[test]
fn test_bit_writer_reader_basic() {
    fn test_width(n_bits: u8) {
        let mut buf = Vec::new();
        {
            let mut writer = BitWriter::new(&mut buf);
            for i in 0..100 {
                writer.write(i as u8, n_bits);
            }
        }

        let mut slice = buf.as_slice();
        let mut reader = BitReader::new(&mut slice);
        for i in 0..100 {
            let mask = if n_bits == 8 {
                0xFF
            } else {
                (1u8 << n_bits) - 1
            };
            let expected = (i as u8) & mask;
            assert_eq!(
                reader.read(n_bits),
                Some(expected),
                "width: {}, i: {}",
                n_bits,
                i
            );
        }
        assert!(!reader.has_remaining());
    }

    for width in 1..=8 {
        test_width(width);
    }
}

#[test]
fn test_bit_writer_reader_mixed() {
    let mut buf = Vec::new();
    {
        let mut writer = BitWriter::new(&mut buf);
        writer.write(1, 1);
        writer.write(3, 2);
        writer.write(5, 3);
        writer.write(10, 4);
        writer.write(20, 5);
        writer.write(40, 6);
        writer.write(80, 7);
        writer.write(150, 8);
    }

    let mut slice = buf.as_slice();
    let mut reader = BitReader::new(&mut slice);
    assert_eq!(reader.read(1), Some(1));
    assert_eq!(reader.read(2), Some(3));
    assert_eq!(reader.read(3), Some(5));
    assert_eq!(reader.read(4), Some(10));
    assert_eq!(reader.read(5), Some(20));
    assert_eq!(reader.read(6), Some(40));
    assert_eq!(reader.read(7), Some(80));
    assert_eq!(reader.read(8), Some(150));
    assert!(!reader.has_remaining());
}

#[test]
fn test_varint_u4() {
    let mut buf = Vec::new();
    {
        let mut writer = BitWriter::new(&mut buf);
        writer.write_u32_varint(0, 4);
        writer.write_u32_varint(7, 4);
        writer.write_u32_varint(8, 4);
        writer.write_u32_varint(127, 4);
        writer.write_u32_varint(128, 4);
        writer.write_u32_varint(u32::MAX, 4);
    }

    let mut slice = buf.as_slice();
    let mut reader = BitReader::new(&mut slice);
    assert_eq!(reader.read_u32_varint(4), Some(0));
    assert_eq!(reader.read_u32_varint(4), Some(7));
    assert_eq!(reader.read_u32_varint(4), Some(8));
    assert_eq!(reader.read_u32_varint(4), Some(127));
    assert_eq!(reader.read_u32_varint(4), Some(128));
    assert_eq!(reader.read_u32_varint(4), Some(u32::MAX));
    assert!(!reader.has_remaining());
}

#[test]
fn test_varint_u2() {
    let mut buf = Vec::new();
    {
        let mut writer = BitWriter::new(&mut buf);
        writer.write_u32_varint(0, 2);
        writer.write_u32_varint(1, 2);
        writer.write_u32_varint(2, 2);
        writer.write_u32_varint(u32::MAX, 2);
    }

    let mut slice = buf.as_slice();
    let mut reader = BitReader::new(&mut slice);
    assert_eq!(reader.read_u32_varint(2), Some(0));
    assert_eq!(reader.read_u32_varint(2), Some(1));
    assert_eq!(reader.read_u32_varint(2), Some(2));
    assert_eq!(reader.read_u32_varint(2), Some(u32::MAX));
    assert!(!reader.has_remaining());
}

#[test]
fn test_varint_u8() {
    let mut buf = Vec::new();
    {
        let mut writer = BitWriter::new(&mut buf);
        writer.write_u32_varint(0, 8);
        writer.write_u32_varint(127, 8);
        writer.write_u32_varint(128, 8);
        writer.write_u32_varint(u32::MAX, 8);
    }

    let mut slice = buf.as_slice();
    let mut reader = BitReader::new(&mut slice);
    assert_eq!(reader.read_u32_varint(8), Some(0));
    assert_eq!(reader.read_u32_varint(8), Some(127));
    assert_eq!(reader.read_u32_varint(8), Some(128));
    assert_eq!(reader.read_u32_varint(8), Some(u32::MAX));
    assert!(!reader.has_remaining());
}

#[test]
fn test_padding() {
    let mut buf = Vec::new();
    {
        let mut writer = BitWriter::new(&mut buf);
        writer.write(0, 1);
        // 7 bits padding (all 1s)
    }
    assert_eq!(buf.len(), 1);
    assert_eq!(buf[0], 0xFE); // 0 (1 bit) | 1111111 (7 bits) << 1 = 0 | 254 = 254 (0xFE)

    let mut slice = buf.as_slice();
    let mut reader = BitReader::new(&mut slice);
    assert_eq!(reader.read(1), Some(0));
    assert!(!reader.has_remaining());
}

#[test]
fn test_varint_overflow() {
    // Construct overflow.
    // 10 chunks of F (1111). Each is 3 bits payload (111) + 1 bit cont (1).
    // Total 30 bits of 1s.
    let mut buf = vec![0xFF; 5];

    // 11th chunk: 0x4 (0100) -> payload 0 (000), cont 1 (1).
    // This means we have 30 bits, then 0 bits, but more coming.
    // shift becomes 33. Next read should fail.
    // We need to pack this into bytes.
    // 10 nibbles of F.
    // 11th nibble: 4.
    // 12th nibble: F (padding).
    // Byte: 0xF4.
    buf.push(0xF4);

    let mut slice = buf.as_slice();
    let mut reader = BitReader::new(&mut slice);
    assert_eq!(reader.read_u32_varint(4), None);
}

#[test]
fn test_special_case_encoding() {
    let mut record = Record::new();
    record.make_move(Move::Place(Point::ZERO, None));

    // Test with scheme::past()
    let encoded = record.encode_to_vec(RecordEncodingScheme::past());
    assert_eq!(encoded, vec![0x82]);
    let decoded = Record::decode(&mut encoded.as_slice()).unwrap();
    assert_eq!(record.moves(), decoded.moves());

    // Test with scheme::all()
    let encoded = record.encode_to_vec(RecordEncodingScheme::all());
    assert_eq!(encoded, vec![0x13, 0xF8]);
    let decoded = Record::decode(&mut encoded.as_slice()).unwrap();
    assert_eq!(record.moves(), decoded.moves());
}
