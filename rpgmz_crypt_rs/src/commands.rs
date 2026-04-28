// ── File operations & high-level commands ──────────────────────────────

use crate::crypto::{self, CryptoParams};
use crate::detect::{auto_detect_game_context, detect_game_context, EngineKind, GameContext};
use crate::{mv, mz};
use anyhow::{bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedWrapper {
    uid: String,
    bid: String,
    data: String,
}

const DATA_BAK: &str = "data.encrypted";

fn resolve_game_context(explicit_game: Option<&Path>, paths: &[&Path]) -> Result<GameContext> {
    match explicit_game {
        Some(game_root) => detect_game_context(game_root)
            .with_context(|| format!("Failed to detect game context from {}", game_root.display())),
        None => auto_detect_game_context(paths),
    }
}

fn params_for_context(ctx: &GameContext) -> Result<CryptoParams> {
    match ctx.engine {
        EngineKind::Mz => mz::extract_mz_params_from_path(&ctx.manager_js),
        EngineKind::MvCustom => mv::extract_mv_params_from_path(&ctx.manager_js),
    }
    .with_context(|| {
        format!(
            "Failed to extract crypto parameters from {}",
            ctx.manager_js.display()
        )
    })
}

fn decrypt_file_with_params(
    input: &Path,
    output: &Path,
    pretty: bool,
    params: &CryptoParams,
) -> Result<()> {
    let text = fs::read_to_string(input)
        .with_context(|| format!("cannot read {}", input.display()))?;

    let wrapper: EncryptedWrapper = serde_json::from_str(&text).with_context(|| {
        format!(
            "{} is not an encrypted RPG Maker data file",
            input.display()
        )
    })?;

    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&wrapper.data)
        .context("base64 decode failed")?;

    let filename = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.json");
    let plaintext = crypto::decrypt(&ciphertext, filename, params);
    let mut text = String::from_utf8(plaintext).context("decrypted data is not valid UTF-8")?;

    if text.starts_with('\u{FEFF}') {
        text.remove(0);
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    if pretty {
        let parsed: serde_json::Value =
            serde_json::from_str(&text).context("decrypted data is not valid JSON")?;
        fs::write(output, serde_json::to_string_pretty(&parsed)?)?;
    } else {
        fs::write(output, text)?;
    }
    Ok(())
}

fn encrypt_file_with_params(input: &Path, output: &Path, params: &CryptoParams) -> Result<()> {
    let mut text = fs::read_to_string(input)
        .with_context(|| format!("cannot read {}", input.display()))?;

    if text.starts_with('\u{FEFF}') {
        text.remove(0);
    }

    let filename = output
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.json");
    let ciphertext = crypto::encrypt(text.as_bytes(), filename, params);
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(&ciphertext);

    let wrapper = EncryptedWrapper {
        uid: String::new(),
        bid: "1.9.0".to_string(),
        data: data_b64,
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_string(&wrapper)?)?;
    Ok(())
}

pub fn decrypt_file(input: &Path, output: &Path, pretty: bool, game: Option<&Path>) -> Result<()> {
    let ctx = resolve_game_context(game, &[input, output])?;
    let params = params_for_context(&ctx)?;
    decrypt_file_with_params(input, output, pretty, &params)
}

pub fn encrypt_file(input: &Path, output: &Path, game: Option<&Path>) -> Result<()> {
    let ctx = resolve_game_context(game, &[input, output])?;
    let params = params_for_context(&ctx)?;
    encrypt_file_with_params(input, output, &params)
}

fn collect_json_entries<I>(entries: I) -> Result<Vec<PathBuf>>
where
    I: IntoIterator<Item = std::io::Result<fs::DirEntry>>,
{
    let mut paths = entries
        .into_iter()
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| path.extension().map(|ext| ext == "json").unwrap_or(false));
    paths.sort();
    Ok(paths)
}

pub fn process_directory(
    input_dir: &Path,
    output_dir: &Path,
    decrypting: bool,
    pretty: bool,
    game: Option<&Path>,
) -> Result<Vec<String>> {
    fs::create_dir_all(output_dir)?;

    let entries = fs::read_dir(input_dir).context("cannot read input directory")?;
    let entries = collect_json_entries(entries).context("cannot iterate input directory")?;
    let ctx = resolve_game_context(game, &[input_dir, output_dir])?;
    let params = params_for_context(&ctx)?;

    let mut processed = Vec::new();
    for src in &entries {
        let fname = src.file_name().unwrap().to_str().unwrap().to_string();
        let dst = output_dir.join(&fname);
        if decrypting {
            decrypt_file_with_params(src, &dst, pretty, &params)
        } else {
            encrypt_file_with_params(src, &dst, &params)
        }
        .with_context(|| format!("failed to process {}", src.display()))?;
        processed.push(fname);
    }
    Ok(processed)
}

fn js_backup_path(ctx: &GameContext) -> PathBuf {
    match ctx.engine {
        EngineKind::Mz => ctx.root.join(mz::MANAGERS_JS_BAK),
        EngineKind::MvCustom => ctx.root.join(mv::MANAGERS_JS_BAK),
    }
}

fn patch_managers_js(ctx: &GameContext) -> Result<bool> {
    match ctx.engine {
        EngineKind::Mz => mz::patch_managers_js(&ctx.root),
        EngineKind::MvCustom => mv::patch_managers_js(&ctx.root),
    }
}

pub fn cmd_restore(game_dir: &Path) -> Result<()> {
    let ctx = detect_game_context(game_dir)?;
    let data_dir = ctx.root.join("data");
    let data_bak = ctx.root.join(DATA_BAK);
    let js_file = ctx.manager_js.clone();
    let js_bak = js_backup_path(&ctx);

    if data_bak.exists() {
        bail!(
            "Backup already exists at {}\nRun 'revert' first if you want to undo a previous restore.",
            data_bak.display()
        );
    }

    let label = match ctx.engine {
        EngineKind::Mz => "RPG Maker MZ",
        EngineKind::MvCustom => "RPG Maker MV-custom",
    };

    println!("{}", "=".repeat(60));
    println!("{} — One-Click Restore", label);
    println!("{}", "=".repeat(60));
    println!("Game directory: {}", ctx.root.canonicalize()?.display());
    println!();

    println!("[1/3] Backing up encrypted data/ ...");
    let file_count = fs::read_dir(&data_dir)?.count();
    fs::rename(&data_dir, &data_bak)?;
    println!(
        "  → {}/ ({} files)",
        data_bak.file_name().unwrap().to_str().unwrap(),
        file_count
    );

    println!("[2/3] Decrypting data files ...");
    fs::create_dir(&data_dir)?;
    let params = params_for_context(&ctx)?;
    let entries = fs::read_dir(&data_bak)?;
    let entries = collect_json_entries(entries).context("cannot iterate backup data directory")?;
    let mut processed = Vec::new();
    for src in &entries {
        let fname = src.file_name().unwrap().to_str().unwrap().to_string();
        let dst = data_dir.join(&fname);
        decrypt_file_with_params(src, &dst, false, &params)
            .with_context(|| format!("failed to process {}", src.display()))?;
        processed.push(fname);
    }
    println!("  → {} files decrypted", processed.len());

    println!("[3/3] Patching JS engine ...");
    fs::copy(&js_file, &js_bak)?;
    println!("  → backup: {}", js_bak.file_name().unwrap().to_str().unwrap());

    let patched = patch_managers_js(&ctx)?;
    if patched {
        println!("  → {} patched: plain JSON support enabled", js_file.file_name().unwrap().to_str().unwrap());
    }

    println!();
    println!("Done! The game now runs with decrypted (editable) data files.");
    println!("  Encrypted backup: {}/", DATA_BAK);
    println!("  JS backup:        {}", js_bak.strip_prefix(&ctx.root).unwrap().display());
    println!();
    println!("To undo, run:  rpgmz_crypt revert {}", ctx.root.display());
    Ok(())
}

pub fn cmd_revert(game_dir: &Path) -> Result<()> {
    let ctx = detect_game_context(game_dir)?;
    let data_dir = ctx.root.join("data");
    let data_bak = ctx.root.join(DATA_BAK);
    let js_file = ctx.manager_js.clone();
    let js_bak = js_backup_path(&ctx);

    if !data_bak.is_dir() && !js_bak.is_file() {
        bail!("No backups found. Nothing to revert.");
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
        println!(
            "  → {} restored",
            js_file.strip_prefix(&ctx.root).unwrap().display()
        );
    }

    println!("Revert complete. Game is back to its original (encrypted) state.");
    Ok(())
}

pub fn cmd_patch_js(game_dir: &Path) -> Result<()> {
    let ctx = detect_game_context(game_dir)?;
    let js_file = ctx.manager_js.clone();
    let js_bak = js_backup_path(&ctx);

    println!("Patching JS engine...");
    if js_bak.exists() {
        println!(
            "Note: backup already exists at {} (not overwriting)",
            js_bak.display()
        );
    } else {
        fs::copy(&js_file, &js_bak)?;
        println!("  → backup: {}", js_bak.file_name().unwrap().to_str().unwrap());
    }

    let patched = patch_managers_js(&ctx)?;
    if patched {
        println!("  → {} patched successfully", js_file.file_name().unwrap().to_str().unwrap());
    }
    println!();
    println!("The engine now accepts both encrypted and plain JSON data files.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn process_directory_fails_when_read_dir_entry_errors() {
        let entry_error = io::Error::new(io::ErrorKind::Other, "broken entry");
        let result = collect_json_entries(vec![Err(entry_error)].into_iter());

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("broken entry"));
    }
}
