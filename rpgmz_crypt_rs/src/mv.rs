use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use crate::crypto::CryptoParams;
use crate::param_parser::parse_crypto_params;

pub const MANAGERS_JS: &str = "js/rpg_managers.js";
pub const MANAGERS_JS_BAK: &str = "js/rpg_managers.js.bak";

pub fn extract_mv_params_from_str(content: &str) -> Result<CryptoParams> {
    parse_crypto_params(content)
}

pub fn extract_mv_params_from_path(path: &Path) -> Result<CryptoParams> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    extract_mv_params_from_str(&content)
}

pub fn patch_managers_js(game_dir: &Path) -> Result<bool> {
    let js_path = game_dir.join(MANAGERS_JS);
    if !js_path.is_file() {
        bail!(
            "{} not found — is this an RPG Maker MV-custom game?",
            js_path.display()
        );
    }

    let content = fs::read_to_string(&js_path)?;
    if content.contains("if(c.bid){var b=Buffer.from(c.data,'base64');") {
        println!("  JS already patched (plain JSON support detected).");
        return Ok(false);
    }

    if !content.contains("var c=JSON.parse(xhr.responseText);var b=Buffer.from(c.data,'base64');") {
        bail!(
            "Expected MV patch pattern not found in {}\nThis game may use a different engine version.",
            js_path.display()
        );
    }

    let mut new_content = content.replace(
        "var c=JSON.parse(xhr.responseText);var b=Buffer.from(c.data,'base64');",
        "var c=JSON.parse(xhr.responseText);if(c.bid){var b=Buffer.from(c.data,'base64');",
    );

    if new_content.contains("window[name]=JSON.parse(b.toString('utf8').replace(/^﻿/, ''));") {
        new_content = new_content.replace(
            "window[name]=JSON.parse(b.toString('utf8').replace(/^﻿/, ''));",
            "window[name]=JSON.parse(b.toString('utf8').replace(/^﻿/, ''));}else{window[name]=c;}",
        );
    } else if new_content.contains("window[name]=JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));") {
        new_content = new_content.replace(
            "window[name]=JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));",
            "window[name]=JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));}else{window[name]=c;}",
        );
    } else {
        bail!(
            "Expected MV JSON load pattern not found in {}\nThis game may use a different engine version.",
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
    fn extracts_mv_params_from_fixture() {
        let fixture = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("tests/fixtures/mv_rpg_managers.js"),
        )
        .unwrap();

        let params = extract_mv_params_from_str(&fixture).unwrap();
        assert_eq!(
            params,
            CryptoParams {
                k_value: 152,
                xor_c: 85,
                left_shift_p: 2,
                right_shift_p: 4,
                xor_k: 180,
                add_k: 36,
                lowercase_filename: false,
            }
        );
    }
}
