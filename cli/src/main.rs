use anyhow::{Context, Result};
use blake_tree::StreamStorage;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Opts {
    #[clap(long)]
    dir: Option<PathBuf>,
    #[clap(long)]
    url: Option<String>,
    #[clap(long)]
    mount: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    femme::start();
    let opts = Opts::parse();
    let dir = if let Some(dir) = opts.dir {
        dir
    } else {
        dirs_next::config_dir()
            .context("no config dir found")?
            .join("blake-tree-cli")
    };
    let dev_fuse = if let Some(mount_target) = opts.mount {
        log::info!("mounting fuse fs at {}", mount_target.display());
        let socket = blake_tree_fuse::mount(&mount_target)?;
        caps::clear(None, caps::CapSet::Permitted)?;
        Some(socket)
    } else {
        None
    };
    let storage = StreamStorage::new(dir)?;
    let mut joins = Vec::with_capacity(2);
    if let Some(url) = opts.url {
        joins.push(tokio::task::spawn(blake_tree_http::blake_tree_http(storage.clone(), url)));
    }
    if let Some(dev_fuse) = dev_fuse {
        joins.push(tokio::task::spawn_blocking(|| blake_tree_fuse::blake_tree_fuse(storage, dev_fuse)));
    }
    futures::future::select_all(joins).await.0??;
    Ok(())
}
