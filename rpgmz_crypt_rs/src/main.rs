// RPG Maker MZ Data File Encrypt/Decrypt Tool (Rust)
//
// Usage:
//   rpgmz_crypt decrypt <input_dir> <output_dir> [--pretty]
//   rpgmz_crypt encrypt <input_dir> <output_dir>
//   rpgmz_crypt decrypt-file <input> <output> [--pretty]
//   rpgmz_crypt encrypt-file <input> <output>
//   rpgmz_crypt restore <game_dir>
//   rpgmz_crypt revert <game_dir>
//   rpgmz_crypt patch-js <game_dir>

mod crypto;
mod commands;
mod cli;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    cli::run(args)
}
