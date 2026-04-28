// ── JS 32-bit integer emulation & core crypto ──────────────────────────

use std::path::Path;

/// Emulate JavaScript >>> (unsigned right shift).
#[inline]
fn js_ushift_r(v: i32, n: u32) -> i32 {
    ((v as u32) >> n) as i32
}

/// Emulate JavaScript XOR expression (a|b)&~(a&b) in 32-bit signed.
#[inline]
fn js_xor(a: i32, b: i32) -> i32 {
    (a | b) & !(a & b)
}

/// Compute the key byte for position `i`.
/// Exact replication of the engine's obfuscated computation.
fn compute_k(ls: i32, idx: usize, fk: i32) -> u8 {
    let _c = js_xor(fk, 82);                         // fk XOR 82
    let _m = (idx % 128) as i32;                     // i % 128
    let _p = js_xor(ls << 2, js_ushift_r(ls, 4));    // (ls<<2) XOR (ls>>>4)
    let sum = _c + _m + _p;
    let k = js_xor(sum, 146);                        // sum XOR 146
    ((k + 46) & 255) as u8                           // (result + 46) & 255
}

/// Constant: sqrt(61009) | 0 in the engine (247 * 247 == 61009).
const FK_CONST: i32 = 247;

/// Compute JS-style string hash: t = ((t << 5) - t + charCode) | 0.
fn filename_hash(name: &str) -> i32 {
    let mut t: i32 = 0;
    for ch in name.chars() {
        t = t.wrapping_shl(5).wrapping_sub(t).wrapping_add(ch as i32);
    }
    t
}

/// Derive the initial feedback key from filename.
/// Basename is extracted (stem, no .json), lowercased, hashed, XOR with 247.
pub fn get_fk(filename: &str) -> u8 {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let hash = filename_hash(&stem);
    (js_xor(FK_CONST, hash & 255) & 255) as u8
}

// ── Core crypto ────────────────────────────────────────────────────────

/// Decrypt RPG Maker MZ encrypted data (ciphertext → plaintext).
/// Runs backwards; key for position i depends on plaintext[i+1].
pub fn decrypt(ciphertext: &[u8], filename: &str) -> Vec<u8> {
    let fk = get_fk(filename) as i32;
    let n = ciphertext.len();
    let mut result = vec![0u8; n];
    let mut ls = fk;

    for i in (0..n).rev() {
        let k = compute_k(ls, i, fk) as i32;
        let v = js_xor(ciphertext[i] as i32, k) & 255;
        result[i] = v as u8;
        ls = v; // plaintext feedback
    }
    result
}

/// Encrypt data into RPG Maker MZ format (plaintext → ciphertext).
/// Mirror of decrypt(): same direction, same plaintext feedback.
pub fn encrypt(plaintext: &[u8], filename: &str) -> Vec<u8> {
    let fk = get_fk(filename) as i32;
    let n = plaintext.len();
    let mut result = vec![0u8; n];
    let mut ls = fk;

    for i in (0..n).rev() {
        let k = compute_k(ls, i, fk) as i32;
        let v = js_xor(plaintext[i] as i32, k) & 255;
        result[i] = v as u8;
        ls = plaintext[i] as i32; // plaintext feedback (NOT ciphertext)
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_small() {
        let data = b"Hello, World! Test 123";
        let enc = encrypt(data, "Test.json");
        let dec = decrypt(&enc, "Test.json");
        assert_eq!(data.to_vec(), dec);
    }

    #[test]
    fn test_filename_hash_known() {
        // "map100" hash verified against Python/JS
        assert_eq!(filename_hash("map100"), -1081423083);
    }

    #[test]
    fn test_get_fk_known() {
        let fk = get_fk("Map100.json");
        assert_eq!(fk, 226); // 247 XOR (0x15) = 0xE2 = 226
    }
}
