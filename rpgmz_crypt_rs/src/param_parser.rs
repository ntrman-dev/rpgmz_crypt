use anyhow::{anyhow, Result};
use regex::Regex;

use crate::crypto::CryptoParams;

pub(crate) fn parse_crypto_params(content: &str) -> Result<CryptoParams> {
    let k_re = Regex::new(r"window\._K\s*=\s*\(Math\.sqrt\((\d+)\)\|0\)")?;
    let lower_re = Regex::new(
        r"var\s+n\s*=\s*src\.split\(/.*?/\)\.pop\(\)\.replace\('\.json',\s*''\)(\.toLowerCase\(\))?",
    )?;
    let loop_re = Regex::new(
        r"var\s+_c\s*=\s*\(fk\|(\d+)\)&~\(fk&\d+\),\s*_m\s*=\s*\(i%128\),\s*_p\s*=\s*\(\(ls<<(\d+)\)\|\(ls>>>(\d+)\)\)&~\(\(ls<<\d+\)&\(ls>>>\d+\)\);\s*var\s+_k\s*=\s*\(\(\(\(_c\+_m\+_p\)\|(\d+)\)&~\(\(\(_c\+_m\+_p\)&\d+\)\)\)\+(\d+)\)&255;",
    )?;

    let k_caps = k_re
        .captures(content)
        .ok_or_else(|| anyhow!("Cannot find window._K assignment"))?;
    let k_source: i32 = k_caps[1].parse()?;
    let k_value = (k_source as f64).sqrt() as i32;
    if k_value * k_value != k_source {
        return Err(anyhow!("Math.sqrt({k_source}) is not a perfect square"));
    }

    let lower_caps = lower_re
        .captures(content)
        .ok_or_else(|| anyhow!("Cannot find filename normalisation pattern"))?;

    let loop_caps = loop_re
        .captures(content)
        .ok_or_else(|| anyhow!("Cannot find decryption loop pattern"))?;

    Ok(CryptoParams {
        k_value,
        xor_c: loop_caps[1].parse()?,
        left_shift_p: loop_caps[2].parse()?,
        right_shift_p: loop_caps[3].parse()?,
        xor_k: loop_caps[4].parse()?,
        add_k: loop_caps[5].parse()?,
        lowercase_filename: lower_caps.get(1).is_some(),
    })
}
