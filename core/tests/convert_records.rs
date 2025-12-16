#![allow(missing_docs)]

use std::fs;

use base64::prelude::*;
use c6ol_core::game::{Record, RecordEncodingScheme};

// Baseline: 1231 -> 1205

#[test]
fn convert_records() {
    let recs = fs::read_to_string("records/00.txt").unwrap();
    let recs_new = fs::read_to_string("records/02.txt").unwrap();

    let mut total_moves = 0;
    let mut total_bytes = 0;
    let mut total_bytes_alt = 0;

    for (i, (rec_str, rec_new_str)) in recs.lines().zip(recs_new.lines()).enumerate() {
        let rec_bytes = BASE64_STANDARD.decode(rec_str).unwrap();
        let rec = Record::decode(&mut &rec_bytes[..]).unwrap();

        let mut rec_bytes_new = Vec::new();
        rec.encode(&mut rec_bytes_new, RecordEncodingScheme::past());

        // assert_eq!(rec_new_str, BASE64_STANDARD_NO_PAD.encode(&rec_bytes_new));

        let rec_new = Record::decode(&mut &rec_bytes_new[..]).unwrap();
        assert_eq!(rec, rec_new);

        println!(
            "{i}: {} moves, {} -> {} ({:+}) bytes",
            rec.moves().len(),
            rec_bytes.len(),
            rec_bytes_new.len(),
            rec_bytes_new.len() as isize - rec_bytes.len() as isize
        );

        total_moves += rec.moves().len();
        total_bytes += rec_bytes.len();
        total_bytes_alt += rec_bytes_new.len();
    }

    println!("total: {total_moves} moves, {total_bytes} -> {total_bytes_alt} bytes");
}
