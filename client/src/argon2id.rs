use argon2::Argon2;

pub fn hash(pwd: &[u8], salt: i64) -> argon2::Result<i64> {
    let argon2 = Argon2::default();
    let salt = salt.to_le_bytes();
    let mut buf = [0; 8];

    argon2
        .hash_password_into(pwd, &salt, &mut buf)
        .map(|()| i64::from_le_bytes(buf))
}
