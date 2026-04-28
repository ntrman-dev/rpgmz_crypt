use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoParams {
    pub k_value: i32,
    pub xor_c: i32,
    pub left_shift_p: u32,
    pub right_shift_p: u32,
    pub xor_k: i32,
    pub add_k: i32,
    pub lowercase_filename: bool,
}

impl Default for CryptoParams {
    fn default() -> Self {
        Self {
            k_value: 247,
            xor_c: 82,
            left_shift_p: 2,
            right_shift_p: 4,
            xor_k: 146,
            add_k: 46,
            lowercase_filename: true,
        }
    }
}

#[inline]
fn js_signed32(v: i32) -> i32 {
    v
}

#[inline]
fn js_ushift_r(v: i32, n: u32) -> i32 {
    ((v as u32) >> n) as i32
}

#[inline]
fn js_xor(a: i32, b: i32) -> i32 {
    (a | b) & !(a & b)
}

pub fn compute_k(ls: i32, idx: usize, fk: i32, params: &CryptoParams) -> u8 {
    let c = js_xor(fk, params.xor_c);
    let m = (idx % 128) as i32;
    let p = js_xor(
        ls.wrapping_shl(params.left_shift_p),
        js_ushift_r(ls, params.right_shift_p),
    );
    ((js_xor(c.wrapping_add(m).wrapping_add(p), params.xor_k).wrapping_add(params.add_k)) & 255)
        as u8
}

fn filename_hash(name: &str) -> i32 {
    let mut t: i32 = 0;
    for ch in name.chars() {
        t = js_signed32(
            t.wrapping_shl(5)
                .wrapping_sub(t)
                .wrapping_add(ch as i32),
        );
    }
    t
}

pub fn get_fk(filename: &str, params: &CryptoParams) -> u8 {
    let mut stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    if params.lowercase_filename {
        stem = stem.to_lowercase();
    }
    let hash = filename_hash(&stem);
    (js_xor(params.k_value, hash & 255) & 255) as u8
}

pub fn decrypt(ciphertext: &[u8], filename: &str, params: &CryptoParams) -> Vec<u8> {
    let fk = get_fk(filename, params) as i32;
    let n = ciphertext.len();
    let mut result = vec![0u8; n];
    let mut ls = fk;

    for i in (0..n).rev() {
        let k = compute_k(ls, i, fk, params) as i32;
        let v = js_xor(ciphertext[i] as i32, k) & 255;
        result[i] = v as u8;
        ls = v;
    }
    result
}

pub fn encrypt(plaintext: &[u8], filename: &str, params: &CryptoParams) -> Vec<u8> {
    let fk = get_fk(filename, params) as i32;
    let n = plaintext.len();
    let mut result = vec![0u8; n];
    let mut ls = fk;

    for i in (0..n).rev() {
        let k = compute_k(ls, i, fk, params) as i32;
        let v = js_xor(plaintext[i] as i32, k) & 255;
        result[i] = v as u8;
        ls = plaintext[i] as i32;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_small_with_default_params() {
        let params = CryptoParams::default();
        let data = b"Hello, World! Test 123";
        let enc = encrypt(data, "Test.json", &params);
        let dec = decrypt(&enc, "Test.json", &params);
        assert_eq!(data.to_vec(), dec);
    }

    #[test]
    fn test_roundtrip_small_with_custom_params() {
        let params = CryptoParams {
            k_value: 152,
            xor_c: 85,
            left_shift_p: 2,
            right_shift_p: 4,
            xor_k: 180,
            add_k: 36,
            lowercase_filename: false,
        };
        let data = b"Custom MV style params";
        let enc = encrypt(data, "Map001.json", &params);
        let dec = decrypt(&enc, "Map001.json", &params);
        assert_eq!(data.to_vec(), dec);
    }

    #[test]
    fn test_filename_hash_known() {
        assert_eq!(filename_hash("map100"), -1081423083);
    }

    #[test]
    fn test_get_fk_known() {
        let params = CryptoParams::default();
        let fk = get_fk("Map100.json", &params);
        assert_eq!(fk, 226);
    }
}
