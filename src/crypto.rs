//! 非对称登录使用的密码学原语。
//!
//! 协议概要（详见 `doc/api.md`）：
//!
//! * 客户端用 `PBKDF2-HMAC-SHA256(password, salt)` 派生 32 字节 X25519 私钥；
//!   公钥 = X25519 基点乘法结果（32 字节）。
//! * `/api/logintry` 用用户公钥做 ECIES：临时 X25519 密钥 + HKDF-SHA256 +
//!   AES-256-GCM，把随机挑战加密到 `challengeEncrypt`。
//! * 客户端解密挑战后，用服务器下发的 `answerEncryptingKey`（32 字节随机
//!   对称密钥）做 AES-256-GCM，得到 `encryptedAnswer`。
//! * 服务器解密并比对挑战，完成登录。
//!
//! 二进制布局（均为小端/紧凑拼接，JSON 中用标准 base64 传输）：
//!
//! ```text
//! challengeEncrypt = ephemeral_public(32) || nonce(12) || AES-GCM(challenge, tag 16)
//! encryptedAnswer  = nonce(12) || AES-GCM(challenge, tag 16)
//! ```
//!
//! 服务器只保存用户公钥，私钥永远只存在于客户端。

use ring::aead::{
    Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN,
};
use ring::agreement::{agree_ephemeral, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::hkdf::{Salt, HKDF_SHA256};
use ring::rand::{SecureRandom, SystemRandom};

/// PBKDF2 迭代次数。客户端与文档必须保持一致；登录响应里的 `kdf` 会回显。
pub const KDF_ITERATIONS: u32 = 210_000;
/// KDF 盐前缀；完整盐为 `前缀 + 小写用户名`。
pub const KDF_SALT_PREFIX: &str = "smart-campus/auth/v1:";
/// X25519 公钥 / 私钥 / 挑战 / 对称密钥长度。
pub const KEY_LEN: usize = 32;
pub const CHALLENGE_LEN: usize = 32;
pub const TAG_LEN: usize = 16;
pub const ANSWER_CIPHERTEXT_LEN: usize = NONCE_LEN + CHALLENGE_LEN + TAG_LEN; // 60
pub const CHALLENGE_ENCRYPT_LEN: usize = KEY_LEN + NONCE_LEN + CHALLENGE_LEN + TAG_LEN; // 92

/// HKDF info：挑战加密。
pub const INFO_CHALLENGE: &[u8] = b"smart-campus/challenge/v1";

/// 生成密码派生盐。用户名大小写不敏感，避免登录时因大小写不同而失败。
pub fn kdf_salt(username: &str) -> String {
    format!(
        "{KDF_SALT_PREFIX}{}",
        username.trim().to_lowercase()
    )
}

/// 用操作系统 CSPRNG 生成随机字节。
pub fn random_bytes(len: usize) -> Result<Vec<u8>, String> {
    let rng = SystemRandom::new();
    let mut out = vec![0u8; len];
    rng.fill(&mut out)
        .map_err(|_| "secure random generator unavailable".to_string())?;
    Ok(out)
}

/// HKDF-SHA256（RFC 5869），输出固定为 32 字节（一个 SHA-256 块）。
pub fn hkdf_sha256_32(ikm: &[u8], salt: &[u8], info: &[u8]) -> Result<[u8; KEY_LEN], String> {
    let salt = Salt::new(HKDF_SHA256, salt);
    let prk = salt.extract(ikm);
    let info_parts: [&[u8]; 1] = [info];
    let okm = prk
        .expand(&info_parts, HKDF_SHA256)
        .map_err(|_| "hkdf expand failed".to_string())?;
    let mut out = [0u8; KEY_LEN];
    okm.fill(&mut out)
        .map_err(|_| "hkdf fill failed".to_string())?;
    Ok(out)
}

fn aead_key(key: &[u8]) -> Result<LessSafeKey, String> {
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| "invalid AES-256-GCM key".to_string())?;
    Ok(LessSafeKey::new(unbound))
}

/// AES-256-GCM 加密，返回 `nonce(12) || ciphertext || tag(16)`。
pub fn aes_gcm_encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    if key.len() != KEY_LEN {
        return Err("invalid answer encrypting key length".to_string());
    }
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| "secure random generator unavailable".to_string())?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let key = aead_key(key)?;
    let mut in_out = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| "answer encryption failed".to_string())?;
    let mut out = Vec::with_capacity(NONCE_LEN + in_out.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&in_out);
    Ok(out)
}

/// AES-256-GCM 解密；输入为 `nonce(12) || ciphertext || tag(16)`。
pub fn aes_gcm_decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    if key.len() != KEY_LEN {
        return Err("invalid answer encrypting key length".to_string());
    }
    if data.len() < NONCE_LEN + TAG_LEN {
        return Err("encrypted answer is too short".to_string());
    }
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| "invalid AES-GCM nonce".to_string())?;
    let key = aead_key(key)?;
    let mut in_out = ciphertext.to_vec();
    let plain = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| "answer decryption failed".to_string())?;
    Ok(plain.to_vec())
}

/// 校验一个 X25519 公钥是否可用：执行一次临时 ECDH，低阶点会失败。
pub fn validate_public_key(pubkey: &[u8]) -> Result<(), String> {
    if pubkey.len() != KEY_LEN {
        return Err("public key must be 32 bytes".to_string());
    }
    let rng = SystemRandom::new();
    let probe = EphemeralPrivateKey::generate(&X25519, &rng)
        .map_err(|_| "key generation failed".to_string())?;
    agree_ephemeral(
        probe,
        &UnparsedPublicKey::new(&X25519, pubkey),
        |shared| shared.len(),
    )
    .map(|_| ())
    .map_err(|_| "invalid X25519 public key".to_string())
}

/// 把挑战加密给用户公钥（ECIES）。
pub fn encrypt_challenge(pubkey: &[u8], challenge: &[u8]) -> Result<Vec<u8>, String> {
    if pubkey.len() != KEY_LEN {
        return Err("public key must be 32 bytes".to_string());
    }
    if challenge.len() != CHALLENGE_LEN {
        return Err("challenge must be 32 bytes".to_string());
    }
    let rng = SystemRandom::new();
    let ephemeral = EphemeralPrivateKey::generate(&X25519, &rng)
        .map_err(|_| "key generation failed".to_string())?;
    let ephemeral_pub = ephemeral
        .compute_public_key()
        .map_err(|_| "public key computation failed".to_string())?;
    let ephemeral_pub = ephemeral_pub.as_ref().to_vec();
    let shared = agree_ephemeral(
        ephemeral,
        &UnparsedPublicKey::new(&X25519, pubkey),
        |secret| secret.to_vec(),
    )
    .map_err(|_| "challenge key agreement failed".to_string())?;
    let key = hkdf_sha256_32(&shared, &[], INFO_CHALLENGE)?;
    let encrypted = aes_gcm_encrypt(&key, challenge)?;
    let mut out = Vec::with_capacity(CHALLENGE_ENCRYPT_LEN);
    out.extend_from_slice(&ephemeral_pub);
    out.extend_from_slice(&encrypted);
    Ok(out)
}

/// 常数时间比较两段字节。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// base64（标准字母表，兼容 URL-safe 与缺省 padding）
// ---------------------------------------------------------------------------

const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

/// 解码 base64 文本；忽略空白，接受标准/URL-safe 字母表与缺省 `=`。
pub fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    if cleaned.len() % 4 == 1 {
        return Err("invalid base64 length".to_string());
    }
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for b in cleaned {
        let v = b64_value(b).ok_or_else(|| "invalid base64 character".to_string())? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    // 余下的 bits 必须是零；否则是非法编码。
    if bits > 0 && (acc & ((1u32 << bits) - 1)) != 0 {
        return Err("invalid base64 padding".to_string());
    }
    Ok(out)
}

/// 标准 base64 编码（带 `=` padding）。
pub fn b64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn x25519_pair() -> (EphemeralPrivateKey, Vec<u8>) {
        let rng = SystemRandom::new();
        let private = EphemeralPrivateKey::generate(&X25519, &rng).unwrap();
        let public = private.compute_public_key().unwrap().as_ref().to_vec();
        (private, public)
    }

    #[test]
    fn base64_roundtrip_and_urlsafe() {
        for len in 0..80usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = b64_encode(&data);
            assert_eq!(b64_decode(&encoded).unwrap(), data);
        }
        assert_eq!(b64_decode("SGVsbG8").unwrap(), b"Hello");
        assert_eq!(b64_decode("SGVsbG8=").unwrap(), b"Hello");
        assert_eq!(b64_decode("--__").unwrap(), vec![0xfb, 0xef, 0xff]);
        assert!(b64_decode("!!!!").is_err());
        assert!(b64_decode("A").is_err());
    }

    #[test]
    fn hkdf_rfc5869_case_1() {
        // RFC 5869 Test Case 1（仅取前 32 字节 OKM）。
        let ikm = hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = hex("000102030405060708090a0b0c");
        let info = hex("f0f1f2f3f4f5f6f7f8f9");
        let okm = hkdf_sha256_32(&ikm, &salt, &info).unwrap();
        assert_eq!(
            hex_encode(&okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf"
        );
    }

    #[test]
    fn aes_gcm_roundtrip_and_tamper() {
        let key = random_bytes(KEY_LEN).unwrap();
        let plain = b"sixteen byte msg";
        let encrypted = aes_gcm_encrypt(&key, plain).unwrap();
        assert_eq!(encrypted.len(), NONCE_LEN + plain.len() + TAG_LEN);
        assert_eq!(aes_gcm_decrypt(&key, &encrypted).unwrap(), plain);

        let mut tampered = encrypted.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(aes_gcm_decrypt(&key, &tampered).is_err());
    }

    #[test]
    fn challenge_encrypt_can_be_decrypted_by_holder() {
        let (user_private, user_public) = x25519_pair();
        let challenge = random_bytes(CHALLENGE_LEN).unwrap();
        let encrypted = encrypt_challenge(&user_public, &challenge).unwrap();
        assert_eq!(encrypted.len(), CHALLENGE_ENCRYPT_LEN);
        assert_ne!(&encrypted[..KEY_LEN], user_public.as_slice());

        // 模拟客户端：取临时公钥，与用户私钥做 ECDH，再用 HKDF + AES-GCM 解密。
        let (ephemeral_pub, rest) = encrypted.split_at(KEY_LEN);
        let shared = agree_ephemeral(
            user_private,
            &UnparsedPublicKey::new(&X25519, ephemeral_pub),
            |secret| secret.to_vec(),
        )
        .unwrap();
        let key = hkdf_sha256_32(&shared, &[], INFO_CHALLENGE).unwrap();
        let plain = aes_gcm_decrypt(&key, rest).unwrap();
        assert_eq!(plain, challenge);
    }

    #[test]
    fn validate_public_key_rejects_low_order() {
        assert!(validate_public_key(&[0u8; KEY_LEN]).is_err());
        let (_, public) = x25519_pair();
        assert!(validate_public_key(&public).is_ok());
        assert!(validate_public_key(&[1u8; 31]).is_err());
    }

    #[test]
    fn kdf_salt_is_case_insensitive() {
        assert_eq!(kdf_salt(" Student "), kdf_salt("student"));
        assert!(kdf_salt("student").starts_with(KDF_SALT_PREFIX));
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn hex_encode(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for b in bytes {
            let _ = write!(out, "{b:02x}");
        }
        out
    }
}
