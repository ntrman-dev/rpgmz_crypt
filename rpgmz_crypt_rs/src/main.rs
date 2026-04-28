use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = rpgdata_crypt::cli::Cli::parse();
    rpgdata_crypt::cli::run(args)
}
