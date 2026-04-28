use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = rpgmz_crypt::cli::Cli::parse();
    rpgmz_crypt::cli::run(args)
}
