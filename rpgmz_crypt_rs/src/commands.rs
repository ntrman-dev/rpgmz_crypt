// ── File operations & high-level commands ──────────────────────────────

use crate::crypto;
use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
// (no io imports needed)
use std::path::{Path, PathBuf};

// ── JSON wrapper for encrypted files ───────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedWrapper {
    uid: String,
    bid: String,
    data: String,
}

// ── JS engine patching constants ───────────────────────────────────────

const MANAGERS_JS: &str = "js/rmmz_managers.js";
const MANAGERS_JS_BAK: &str = "js/rmmz_managers.js.bak";
const DATA_BAK: &str = "data.encrypted";

/// Two targeted replacements on line 107 of rmmz_managers.js.
/// Use `c.bid` (not `c.data`) because map JSON also has a `data` field (tile array).
const PATCH_REPLACEMENTS: &[(&str, &str)] = &[
    (
        "var b = Buffer.from(c.data, 'base64');",
        "if(c.bid){var b = Buffer.from(c.data, 'base64');",
    ),
    (
        "window[name] = JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));   _t.onLoad(window[name]);",
        "window[name] = JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));}else{window[name] = c;}   _t.onLoad(window[name]);",
    ),
];

// ── Per-file operations ────────────────────────────────────────────────

/// Decrypt a single encrypted .json file.
pub fn decrypt_file(input: &Path, output: &Path, pretty: bool) -> Result<()> {
    let text = fs::read_to_string(input)
        .with_context(|| format!("cannot read {}", input.display()))?;

    let wrapper: EncryptedWrapper = serde_json::from_str(&text)
        .with_context(|| format!("{} is not an encrypted RPG Maker MZ data file", input.display()))?;

    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&wrapper.data)
        .context("base64 decode failed")?;

    let filename = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.json");
    let plaintext = crypto::decrypt(&ciphertext, filename);
    let mut text = String::from_utf8(plaintext).context("decrypted data is not valid UTF-8")?;

    // Strip BOM if present
    if text.starts_with('\u{FEFF}') {
        text.remove(0);
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    if pretty {
        let parsed: serde_json::Value = serde_json::from_str(&text)
            .context("decrypted data is not valid JSON")?;
        let formatted = serde_json::to_string_pretty(&parsed)?;
        fs::write(output, formatted)?;
    } else {
        fs::write(output, text)?;
    }
    Ok(())
}

/// Encrypt a single plain .json file into RPG Maker MZ format.
pub fn encrypt_file(input: &Path, output: &Path) -> Result<()> {
    let mut text = fs::read_to_string(input)
        .with_context(|| format!("cannot read {}", input.display()))?;

    // Remove BOM if present
    if text.starts_with('\u{FEFF}') {
        text.remove(0);
    }

    let plaintext = text.as_bytes();
    let filename = output
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.json");
    let ciphertext = crypto::encrypt(plaintext, filename);
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(&ciphertext);

    let wrapper = EncryptedWrapper {
        uid: String::new(),
        bid: "1.9.0".to_string(),
        data: data_b64,
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(&wrapper)?;
    fs::write(output, json)?;
    Ok(())
}

/// Process all .json files in a directory.
pub fn process_directory(
    input_dir: &Path,
    output_dir: &Path,
    decrypting: bool,
    pretty: bool,
) -> Result<Vec<String>> {
    fs::create_dir_all(output_dir)?;

    let mut entries: Vec<PathBuf> = fs::read_dir(input_dir)
        .context("cannot read input directory")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    entries.sort();

    let mut processed = Vec::new();
    for src in &entries {
        let fname = src.file_name().unwrap().to_str().unwrap().to_string();
        let dst = output_dir.join(&fname);
        let result = if decrypting {
            decrypt_file(src, &dst, pretty)
        } else {
            encrypt_file(src, &dst)
        };
        match result {
            Ok(()) => processed.push(fname),
            Err(e) => eprintln!("  ERROR processing {}: {}", src.display(), e),
        }
    }
    Ok(processed)
}

// ── JS patching ────────────────────────────────────────────────────────

/// Patch `rmmz_managers.js` to support plain JSON in addition to encrypted.
/// Returns true if newly patched, false if already patched.
pub fn patch_managers_js(game_dir: &Path) -> Result<bool> {
    let js_path = game_dir.join(MANAGERS_JS);
    if !js_path.is_file() {
        anyhow::bail!(
            "{} not found — is this an RPG Maker MZ game?",
            js_path.display()
        );
    }

    let content = fs::read_to_string(&js_path)?;

    // Check already patched
    if content.contains("if(c.bid){var b = Buffer.from(c.data, 'base64');") {
        println!("  JS already patched (plain JSON support detected).");
        return Ok(false);
    }

    // Verify all patterns exist
    for (old, _) in PATCH_REPLACEMENTS {
        if !content.contains(old) {
            anyhow::bail!(
                "Expected pattern not found in {}\n\
                 This game may use a different engine version.",
                js_path.display()
            );
        }
    }

    // Apply replacements
    let mut new_content = content;
    for (old, new) in PATCH_REPLACEMENTS {
        new_content = new_content.replace(old, new);
    }

    fs::write(&js_path, new_content)?;
    Ok(true)
}

// ── High-level commands ────────────────────────────────────────────────

pub fn cmd_restore(game_dir: &Path) -> Result<()> {
    let data_dir = game_dir.join("data");
    let data_bak = game_dir.join(DATA_BAK);
    let js_file = game_dir.join(MANAGERS_JS);
    let js_bak = game_dir.join(MANAGERS_JS_BAK);

    if !data_dir.is_dir() {
        anyhow::bail!(
            "{} not found — is this an RPG Maker MZ game?",
            data_dir.display()
        );
    }
    if !js_file.is_file() {
        anyhow::bail!(
            "{} not found — is this an RPG Maker MZ game?",
            js_file.display()
        );
    }
    if data_bak.exists() {
        anyhow::bail!(
            "Backup already exists at {}/\n\
             Run 'revert' first if you want to undo a previous restore.",
            data_bak.display()
        );
    }

    println!("{}", "=".repeat(60));
    println!("RPG Maker MZ — One-Click Restore");
    println!("{}", "=".repeat(60));
    println!("Game directory: {}", game_dir.canonicalize()?.display());
    println!();

    // Step 1: backup data/
    println!("[1/3] Backing up encrypted data/ ...");
    let file_count = fs::read_dir(&data_dir)?.count();
    fs::rename(&data_dir, &data_bak)?;
    println!("  → {}/ ({} files)", data_bak.file_name().unwrap().to_str().unwrap(), file_count);

    // Step 2: decrypt in place
    println!("[2/3] Decrypting data files ...");
    fs::create_dir(&data_dir)?;
    let processed = process_directory(&data_bak, &data_dir, true, false)?;
    println!("  → {} files decrypted", processed.len());

    // Step 3: backup + patch JS
    println!("[3/3] Patching JS engine ...");
    fs::copy(&js_file, &js_bak)?;
    println!("  → backup: {}", js_bak.file_name().unwrap().to_str().unwrap());

    let patched = patch_managers_js(game_dir)?;
    if patched {
        println!("  → rmmz_managers.js patched: plain JSON support enabled");
    }

    println!();
    println!("Done! The game now runs with decrypted (editable) data files.");
    println!("  Encrypted backup: {}/", DATA_BAK);
    println!("  JS backup:        {}", MANAGERS_JS_BAK);
    println!();
    println!("To undo, run:  rpgmz_crypt revert {}", game_dir.display());
    Ok(())
}

pub fn cmd_revert(game_dir: &Path) -> Result<()> {
    let data_dir = game_dir.join("data");
    let data_bak = game_dir.join(DATA_BAK);
    let js_file = game_dir.join(MANAGERS_JS);
    let js_bak = game_dir.join(MANAGERS_JS_BAK);

    if !data_bak.is_dir() && !js_bak.is_file() {
        anyhow::bail!("No backups found. Nothing to revert.");
    }

    println!("Reverting restore...");

    if data_bak.is_dir() {
        if data_dir.exists() {
            fs::remove_dir_all(&data_dir)?;
        }
        fs::rename(&data_bak, &data_dir)?;
        let count = fs::read_dir(&data_dir)?.count();
        println!("  → data/ restored ({} files)", count);
    }

    if js_bak.is_file() {
        fs::copy(&js_bak, &js_file)?;
        fs::remove_file(&js_bak)?;
        println!("  → {} restored", MANAGERS_JS);
    }

    println!("Revert complete. Game is back to its original (encrypted) state.");
    Ok(())
}

pub fn cmd_patch_js(game_dir: &Path) -> Result<()> {
    let js_file = game_dir.join(MANAGERS_JS);
    let js_bak = game_dir.join(MANAGERS_JS_BAK);

    if !js_file.is_file() {
        anyhow::bail!("{} not found.", js_file.display());
    }
    if js_bak.exists() {
        println!("Note: backup already exists at {} (not overwriting)", js_bak.display());
    }

    println!("Patching JS engine...");
    fs::copy(&js_file, &js_bak)?;
    println!(
        "  → backup: {}",
        js_bak.file_name().unwrap().to_str().unwrap()
    );

    let patched = patch_managers_js(game_dir)?;
    if patched {
        println!("  → rmmz_managers.js patched successfully");
    }
    println!();
    println!("The engine now accepts both encrypted and plain JSON data files.");
    Ok(())
}
