use blake2::{digest::consts::U8, Blake2b, Digest};
use std::num::NonZero;

const ONE: NonZero<i64> = NonZero::new(1).unwrap();

pub fn hash(input: &[u8]) -> NonZero<i64> {
    let mut hasher = Blake2b::<U8>::new();
    hasher.update(input);

    let hash = i64::from_le_bytes(hasher.finalize().into());
    NonZero::new(hash).unwrap_or(ONE)
}
