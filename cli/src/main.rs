use anyhow::{Context, Result};
use clap::Parser;
use peershare_core::{Mime, MimeType, Range, StreamId};
use peershare_http_client::Client;
use std::path::PathBuf;
use url::Url;

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    cmd: Command,
    #[clap(long)]
    url: Option<String>,
}

#[derive(Parser)]
enum Command {
    List,
    Url(StreamOpts),
    Open(StreamOpts),
    Create(CreateOpts),
    Read(RangeOpts),
    Ranges(StreamOpts),
    MissingRanges(StreamOpts),
    Remove(StreamOpts),
}

#[derive(Parser)]
struct CreateOpts {
    file: File,
    #[clap(short)]
    quiet: bool,
    metadata: Option<PathBuf>,
    content: Option<PathBuf>,
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

impl std::fmt::Display for File {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Url(url) => url.fmt(f),
            Self::Path(path) => path.to_str().unwrap().fmt(f),
        }
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
            let client = &client;
            let streams = client
                .list()
                .await?
                .into_iter()
                .map(|s| async move { Ok((s, client.content(s).await?)) });
            let streams = futures::future::join_all(streams)
                .await
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            print_streams(streams.into_iter());
        }
        Command::Url(StreamOpts { stream }) => {
            let stream = client.content(stream).await?;
            let url = client.url(stream);
            println!("{}", url);
        }
        Command::Open(StreamOpts { stream }) => {
            let stream = client.content(stream).await?;
            let url = client.url(stream);
            // TODO: pass mime type to open
            open::that(url.to_string())?;
        }
        Command::Create(CreateOpts {
            file,
            quiet,
            metadata,
            content,
        }) => {
            let (mime, data) = match &file {
                File::Path(path) => {
                    let mime = Mime::from_path(path).unwrap_or_default();
                    let data = std::fs::read(path)?;
                    (mime, data)
                }
                File::Url(url) => {
                    let mut res = surf::get(url).await.map_err(|err| err.into_inner())?;
                    let mime = to_mime(res.content_type())?;
                    let data = res
                        .take_body()
                        .into_bytes()
                        .await
                        .map_err(|err| err.into_inner())?;
                    (mime, data)
                }
            };
            let stream = client.create(mime, &data).await?;

            let content = if let Some(content) = content {
                std::fs::read_to_string(content)?
            } else if mime.r#type() == MimeType::Text {
                std::str::from_utf8(&data)?.to_string()
            } else {
                Default::default()
            };

            let mut metadata: serde_json::Map<String, serde_json::Value> =
                if let Some(metadata) = metadata {
                    let metadata = std::fs::read_to_string(metadata)?;
                    serde_json::from_str(&metadata)?
                } else {
                    Default::default()
                };
            metadata.insert("source".into(), format!("{file}").into());
            let manifest = client.manifest(stream, metadata, content).await?;

            if quiet {
                println!("{}", manifest);
            } else {
                print_streams(std::iter::once((manifest, stream)));
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
    }
    Ok(())
}

fn to_mime(mime: Option<surf::http::Mime>) -> Result<Mime> {
    if let Some(mime) = mime {
        Mime::from_mime(mime.essence()).context("unsupported mime type")
    } else {
        Ok(Mime::default())
    }
}

fn print_streams(streams: impl Iterator<Item = (StreamId, StreamId)>) {
    println!("| {:<60} | {:<10} | {:<30} |", "stream", "length", "mime",);
    for (stream, content) in streams {
        if stream.mime() != Mime::ApplicationPeershare {
            continue;
        }
        println!(
            "| {:<60} | {:>10} | {:<30} |",
            format!("{stream}"),
            stream.length(),
            format!("{}", content.mime()),
        );
    }
}

fn print_ranges(ranges: impl Iterator<Item = Range>) {
    println!("| {:<10} | {:<10} |", "offset", "length");
    for range in ranges {
        println!("| {:>10} | {:>10} |", range.offset(), range.length());
    }
}
