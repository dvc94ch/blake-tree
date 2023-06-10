use anyhow::{Context, Result};
use blake_tree::{Range, StreamId, StreamStorage};
use clap::Parser;
use std::path::PathBuf;
use url::Url;

#[derive(Parser)]
struct Opts {
    #[clap(long)]
    dir: Option<PathBuf>,
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Parser)]
enum Command {
    List,
    Create(CreateOpts),
    Read(RangeOpts),
    Ranges(StreamOpts),
    MissingRanges(StreamOpts),
    Remove(StreamOpts),
    Spawn(SpawnOpts),
}

#[derive(Parser)]
struct CreateOpts {
    url: Url,
}

#[derive(Parser)]
struct RangeOpts {
    stream: StreamId,
    //range: Option<Range>,
}

#[derive(Parser)]
struct StreamOpts {
    stream: StreamId,
}

#[derive(Parser)]
struct SpawnOpts {
    #[clap(long)]
    url: Option<String>,
    #[clap(long)]
    mount: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::parse();
    let dir = if let Some(dir) = opts.dir {
        dir
    } else {
        dirs_next::config_dir()
            .context("no config dir found")?
            .join("blake-tree-cli")
    };
    let storage = StreamStorage::new(dir)?;
    match opts.cmd {
        Command::List => {
            print_streams(storage.streams());
        }
        Command::Create(CreateOpts { url }) => {
            let stream = if url.scheme() == "file" {
                storage.insert_path(&url.path())?
            } else {
                let mut res = surf::get(url).await.map_err(|err| err.into_inner())?;
                let mime = blake_tree_http::to_mime(res.content_type())?;
                let reader = res
                    .take_body()
                    .into_bytes()
                    .await
                    .map_err(|err| err.into_inner())?;
                storage.insert(mime, &mut &reader[..])?
            };
            print_streams(std::iter::once(*stream.id()));
        }
        Command::Read(RangeOpts { stream }) => {
            let range = stream.range();
            let mut reader = storage.get(&stream)?.read_range(range)?;
            std::io::copy(&mut reader, &mut std::io::stdout())?;
        }
        Command::Ranges(StreamOpts { stream }) => {
            print_ranges(storage.get(&stream)?.ranges()?.into_iter());
        }
        Command::MissingRanges(StreamOpts { stream }) => {
            print_ranges(storage.get(&stream)?.missing_ranges()?.into_iter());
        }
        Command::Remove(StreamOpts { stream }) => {
            storage.remove(&stream)?;
        }
        Command::Spawn(SpawnOpts { url, mount }) => {
            let dev_fuse = if let Some(mount_target) = mount {
                log::info!("mounting fuse fs at {}", mount_target.display());
                let socket = blake_tree_fuse::mount(&mount_target)?;
                caps::clear(None, caps::CapSet::Permitted)?;
                Some(socket)
            } else {
                None
            };
            let mut joins = Vec::with_capacity(2);
            if let Some(url) = url {
                joins.push(tokio::task::spawn(blake_tree_http::blake_tree_http(
                    storage.clone(),
                    url,
                )));
            }
            if let Some(dev_fuse) = dev_fuse {
                joins.push(tokio::task::spawn_blocking(|| {
                    blake_tree_fuse::blake_tree_fuse(storage, dev_fuse)
                }));
            }
            futures::future::select_all(joins).await.0??;
        }
    }
    Ok(())
}

fn print_streams(streams: impl Iterator<Item = StreamId>) {
    println!("| {:<60} | {:<10} | {:<30} |", "stream", "length", "mime");
    for stream in streams {
        println!(
            "| {:<60} | {:>10} | {:<30} |",
            stream,
            stream.length(),
            format!("{}", stream.mime()),
        );
    }
}

fn print_ranges(ranges: impl Iterator<Item = Range>) {
    println!("| {:<10} | {:<10} |", "offset", "length");
    for range in ranges {
        println!("| {:>10} | {:>10} |", range.offset(), range.length());
    }
}
