use anyhow::{Context, Result};
use clap::Parser;
use peershare_core::{Mime, Range, StreamId, StreamStorage};
use peershare_http_client::Client;
use std::path::PathBuf;
use url::Url;

#[derive(Parser)]
struct Opts {
    #[clap(long)]
    dir: Option<PathBuf>,
    #[clap(subcommand)]
    cmd: Command,
    #[clap(long)]
    url: Option<String>,
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
    file: File,
    #[clap(short)]
    quiet: bool,
}

#[derive(Clone)]
enum File {
    Url(Url),
    Path(PathBuf),
}

impl std::str::FromStr for File {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(if s.starts_with("http://") || s.starts_with("https://") {
            Self::Url(s.parse()?)
        } else {
            Self::Path(s.parse()?)
        })
    }
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
    mount: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::parse();
    let url = opts
        .url
        .unwrap_or_else(|| "http://127.0.0.1:3000".to_string());
    let client = Client::new(&url)?;
    match opts.cmd {
        Command::List => {
            print_streams(client.list().await?.into_iter());
        }
        Command::Create(CreateOpts { file, quiet }) => {
            let (mime, data) = match file {
                File::Path(path) => {
                    let mime = Mime::from_path(&path).unwrap_or_default();
                    let data = std::fs::read(path)?;
                    (mime, data)
                }
                File::Url(url) => {
                    let mut res = surf::get(url).await.map_err(|err| err.into_inner())?;
                    let mime = peershare_http::to_mime(res.content_type())?;
                    let data = res
                        .take_body()
                        .into_bytes()
                        .await
                        .map_err(|err| err.into_inner())?;
                    (mime, data)
                }
            };
            let stream = client.create(mime, &data).await?;
            if quiet {
                println!("{}", stream);
            } else {
                print_streams(std::iter::once(stream));
            }
        }
        Command::Read(RangeOpts { stream }) => {
            let data = client.read(stream, None).await?;
            std::io::copy(&mut &data[..], &mut std::io::stdout())?;
        }
        Command::Ranges(StreamOpts { stream }) => {
            let ranges = client.ranges(stream).await?;
            print_ranges(ranges.into_iter());
        }
        Command::MissingRanges(StreamOpts { stream }) => {
            let ranges = client.missing_ranges(stream).await?;
            print_ranges(ranges.into_iter());
        }
        Command::Remove(StreamOpts { stream }) => {
            client.remove(stream).await?;
        }
        Command::Spawn(SpawnOpts { mount }) => {
            let dir = if let Some(dir) = opts.dir {
                dir
            } else {
                dirs_next::config_dir()
                    .context("no config dir found")?
                    .join("peershare")
            };
            let storage = StreamStorage::new(dir)?;

            let dev_fuse = if let Some(mount_target) = mount {
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
                url,
            )));
            if let Some(dev_fuse) = dev_fuse {
                joins.push(tokio::task::spawn_blocking(|| {
                    peershare_fuse::fuse(storage, dev_fuse)
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
