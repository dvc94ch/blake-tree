use anyhow::{Context, Result};
use clap::Parser;
use peershare_core::StreamStorage;
use std::path::PathBuf;

#[derive(Parser)]
struct Opts {
    #[clap(long)]
    dir: Option<PathBuf>,
    #[clap(long)]
    url: Option<String>,
    #[cfg(feature = "fuse")]
    #[clap(long)]
    mount: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::parse();
    let url = opts
        .url
        .unwrap_or_else(|| "http://127.0.0.1:3000".to_string());
    let dir = if let Some(dir) = opts.dir {
        dir
    } else {
        dirs_next::config_dir()
            .context("no config dir found")?
            .join("peershare")
    };
    let storage = StreamStorage::new(dir)?;

    #[cfg(feature = "fuse")]
    let dev_fuse = if let Some(mount_target) = opts.mount {
        log::info!("mounting fuse fs at {}", mount_target.display());
        let socket = peershare_fuse::mount(&mount_target)?;
        caps::clear(None, caps::CapSet::Permitted)?;
        Some(socket)
    } else {
        None
    };
    let mut joins = Vec::with_capacity(2);
    joins.push(tokio::task::spawn(peershare_http::http(
        storage.clone(),
        url.clone(),
    )));
    #[cfg(feature = "fuse")]
    if let Some(dev_fuse) = dev_fuse {
        joins.push(tokio::task::spawn_blocking(|| {
            peershare_fuse::fuse(storage, dev_fuse)
        }));
    }
    futures::future::select_all(joins).await.0??;
    Ok(())
}
