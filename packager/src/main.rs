use anyhow::Result;
use clap::Parser;
use peershare_packager::{package, youtube, PackageOpts, YoutubeOpts};

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(Parser)]
enum Cmd {
    Youtube(YoutubeOpts),
    Package(PackageOpts),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::parse();
    match opts.cmd {
        Cmd::Youtube(opts) => youtube(opts)?,
        Cmd::Package(opts) => package(opts).await?,
    }
    Ok(())
}
