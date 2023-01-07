use anyhow::{Context, Result};
use blake_tree::StreamStorage;
use clap::Parser;
use std::path::PathBuf;
use tide::security::{CorsMiddleware, Origin};

#[derive(Parser)]
struct Opts {
    #[clap(long)]
    dir: Option<PathBuf>,
    #[clap(long)]
    url: Option<String>,
    //#[clap(long)]
    //mount: Option<PathBuf>,
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
    let storage = StreamStorage::new(dir)?;

    if let Some(url) = opts.url {
        let server = blake_tree_http::server(storage).await;

        let cors = CorsMiddleware::new()
            .allow_origin(Origin::from("*"))
            .allow_credentials(false);

        let mut app = tide::new();
        app.with(tide::log::LogMiddleware::new());
        app.with(cors);
        app.at("/").nest(server);
        app.listen(&url)
            .await
            .with_context(|| format!("listening on {}", url))?;
    }

    Ok(())
}
