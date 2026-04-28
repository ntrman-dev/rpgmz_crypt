// ── CLI argument parsing (clap derive) ─────────────────────────────────

use crate::commands;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// RPG Maker data file encrypt/decrypt tool
#[derive(Parser)]
#[command(name = "rpgdata_crypt")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Decrypt all .json files in a directory
    Decrypt {
        /// Directory containing encrypted .json files
        input_dir: PathBuf,
        /// Directory to write decrypted .json files
        output_dir: PathBuf,
        /// Pretty-print JSON with indent=2
        #[arg(long)]
        pretty: bool,
        /// Game root used to resolve engine and crypto parameters
        #[arg(long)]
        game: Option<PathBuf>,
    },

    /// Encrypt all .json files in a directory
    Encrypt {
        /// Directory containing plain .json files
        input_dir: PathBuf,
        /// Directory to write encrypted .json files
        output_dir: PathBuf,
        /// Game root used to resolve engine and crypto parameters
        #[arg(long)]
        game: Option<PathBuf>,
    },

    /// Decrypt a single file
    DecryptFile {
        /// Encrypted .json file
        input: PathBuf,
        /// Output path for decrypted .json
        output: PathBuf,
        /// Pretty-print JSON with indent=2
        #[arg(long)]
        pretty: bool,
        /// Game root used to resolve engine and crypto parameters
        #[arg(long)]
        game: Option<PathBuf>,
    },

    /// Encrypt a single file
    EncryptFile {
        /// Plain .json file
        input: PathBuf,
        /// Output path for encrypted .json
        output: PathBuf,
        /// Game root used to resolve engine and crypto parameters
        #[arg(long)]
        game: Option<PathBuf>,
    },

    /// One-click: decrypt all data + patch JS so the game runs with plain JSON
    Restore {
        /// Root directory of the RPG Maker game (contains data/ and js/)
        game_dir: PathBuf,
    },

    /// Undo a previous restore — re-encrypt data and restore original JS
    Revert {
        /// Root directory of the RPG Maker game (contains data/ and js/)
        game_dir: PathBuf,
    },

    /// Patch only the JS engine to support plain JSON (without touching data)
    PatchJs {
        /// Root directory of the RPG Maker game (contains data/ and js/)
        game_dir: PathBuf,
    },
}

pub fn run(args: Cli) -> anyhow::Result<()> {
    match args.command {
        Command::Decrypt {
            input_dir,
            output_dir,
            pretty,
            game,
        } => {
            let processed =
                commands::process_directory(&input_dir, &output_dir, true, pretty, game.as_deref())?;
            println!("Decrypted {} files:", processed.len());
            for name in &processed {
                println!("  {}", name);
            }
        }

        Command::Encrypt {
            input_dir,
            output_dir,
            game,
        } => {
            let processed =
                commands::process_directory(&input_dir, &output_dir, false, false, game.as_deref())?;
            println!("Encrypted {} files:", processed.len());
            for name in &processed {
                println!("  {}", name);
            }
        }

        Command::DecryptFile {
            input,
            output,
            pretty,
            game,
        } => {
            commands::decrypt_file(&input, &output, pretty, game.as_deref())?;
            println!("Decrypted: {} → {}", input.display(), output.display());
        }

        Command::EncryptFile {
            input,
            output,
            game,
        } => {
            commands::encrypt_file(&input, &output, game.as_deref())?;
            println!("Encrypted: {} → {}", input.display(), output.display());
        }

        Command::Restore { game_dir } => {
            commands::cmd_restore(&game_dir)?;
        }

        Command::Revert { game_dir } => {
            commands::cmd_revert(&game_dir)?;
        }

        Command::PatchJs { game_dir } => {
            commands::cmd_patch_js(&game_dir)?;
        }
    }
    Ok(())
}
