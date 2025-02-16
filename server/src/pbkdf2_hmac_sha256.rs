use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha256;

pub fn hash(password: &[u8], salt: i64) -> i64 {
    let salt = salt.to_le_bytes();
    let hash = pbkdf2_hmac_array::<Sha256, 8>(password, &salt, 600_000);
    i64::from_le_bytes(hash)
}
