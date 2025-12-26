#![allow(missing_docs)]

use std::fs;

use base64::prelude::*;
use c6ol_core::game::{Record, RecordEncodingScheme};

#[test]
fn convert_records() {
    let recs = fs::read_to_string("records/00.txt").unwrap();
    let recs_new = fs::read_to_string("records/02.txt").unwrap();

    let rec_lines: Vec<_> = recs.lines().collect();
    let rec_new_lines: Vec<_> = recs_new.lines().collect();
    assert_eq!(rec_lines.len(), rec_new_lines.len());

    let mut total_moves = 0;
    let mut total_bytes = 0;
    let mut total_bytes_new = 0;

    for (i, (rec_str, rec_new_str)) in rec_lines.into_iter().zip(rec_new_lines).enumerate() {
        let rec_bytes = BASE64_STANDARD_NO_PAD.decode(rec_str).unwrap();
        let rec = Record::decode(&mut &rec_bytes[..]).unwrap();

        let mut rec_bytes_new = Vec::new();
        rec.encode(&mut rec_bytes_new, RecordEncodingScheme::past());

        assert_eq!(rec_new_str, BASE64_URL_SAFE_NO_PAD.encode(&rec_bytes_new));

        let rec_new = Record::decode(&mut &rec_bytes_new[..]).unwrap();
        assert_eq!(rec, rec_new);

        println!(
            "{i}: {} moves, {} -> {} ({:+}) bytes, bpm: {:.2}",
            rec.moves().len(),
            rec_bytes.len(),
            rec_bytes_new.len(),
            rec_bytes_new.len() as isize - rec_bytes.len() as isize,
            rec_bytes_new.len() as f64 / rec.moves().len() as f64,
        );

        total_moves += rec.moves().len();
        total_bytes += rec_bytes.len();
        total_bytes_new += rec_bytes_new.len();
    }

    println!(
        "total: {} moves, {} -> {} bytes, bpm: {:.2}",
        total_moves,
        total_bytes,
        total_bytes_new,
        total_bytes_new as f64 / total_moves as f64,
    );

    assert_eq!(total_moves, 1411);
    assert_eq!(total_bytes, 3266);
    assert_eq!(total_bytes_new, 2358);
}
