use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use crate::crypto::CryptoParams;
use crate::param_parser::parse_crypto_params;

pub const MANAGERS_JS: &str = "js/rmmz_managers.js";
pub const MANAGERS_JS_BAK: &str = "js/rmmz_managers.js.bak";

const PATCH_REPLACEMENTS: &[(&str, &str)] = &[(
    "var b = Buffer.from(c.data, 'base64');",
    "if(c.bid){var b = Buffer.from(c.data, 'base64');",
)];

pub fn extract_mz_params_from_str(content: &str) -> Result<CryptoParams> {
    parse_crypto_params(content)
}

pub fn extract_mz_params_from_path(path: &Path) -> Result<CryptoParams> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    extract_mz_params_from_str(&content)
}

pub fn patch_managers_js(game_dir: &Path) -> Result<bool> {
    let js_path = game_dir.join(MANAGERS_JS);
    if !js_path.is_file() {
        bail!("{} not found — is this an RPG Maker MZ game?", js_path.display());
    }

    let content = fs::read_to_string(&js_path)?;
    if content.contains("if(c.bid){var b = Buffer.from(c.data, 'base64');") {
        println!("  JS already patched (plain JSON support detected).");
        return Ok(false);
    }

    let mut new_content = content;
    for (old, new) in PATCH_REPLACEMENTS {
        if !new_content.contains(old) {
            bail!(
                "Expected MZ patch pattern not found in {}\nThis game may use a different engine version.",
                js_path.display()
            );
        }
        new_content = new_content.replace(old, new);
    }

    if new_content.contains("window[name] = JSON.parse(b.toString('utf8').replace(/^﻿/, ''));") {
        new_content = new_content.replace(
            "window[name] = JSON.parse(b.toString('utf8').replace(/^﻿/, ''));",
            "window[name] = JSON.parse(b.toString('utf8').replace(/^﻿/, ''));}else{window[name] = c;}",
        );
    } else if new_content.contains("window[name] = JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));   _t.onLoad(window[name]);") {
        new_content = new_content.replace(
            "window[name] = JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));   _t.onLoad(window[name]);",
            "window[name] = JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));}else{window[name] = c;}   _t.onLoad(window[name]);",
        );
    } else {
        bail!(
            "Expected MZ JSON load pattern not found in {}\nThis game may use a different engine version.",
            js_path.display()
        );
    }

    fs::write(&js_path, new_content)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_mz_params_from_fixture() {
        let fixture = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("tests/fixtures/mz_rmmz_managers.js"),
        )
        .unwrap();

        let params = extract_mz_params_from_str(&fixture).unwrap();
        assert_eq!(
            params,
            CryptoParams {
                k_value: 247,
                xor_c: 82,
                left_shift_p: 2,
                right_shift_p: 4,
                xor_k: 146,
                add_k: 46,
                lowercase_filename: true,
            }
        );
    }
}
