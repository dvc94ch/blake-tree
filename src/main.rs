use anyhow::{Context, Result};
use clap::Parser;
use peershare_core::{Manifest, Mime, StreamEvent, StreamId, StreamStorage};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use surf::http::Method;
use surf::RequestBuilder;

#[derive(Parser)]
struct Opts {
    #[clap(long)]
    dir: Option<PathBuf>,
    #[clap(long)]
    url: Option<String>,
    #[cfg(feature = "fuse")]
    #[clap(long)]
    mount: Option<PathBuf>,
    #[clap(long)]
    meili_url: Option<String>,
    #[clap(long)]
    meili_key: Option<String>,
}

#[async_std::main]
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
    let mut storage = StreamStorage::new(dir)?;
    if let Some(meili_url) = opts.meili_url {
        let meili = Arc::new(Meili::new(meili_url, opts.meili_key));
        meili.initialize().await.map_err(|e| e.into_inner())?;
        storage.set_callback(move |storage, event| match event {
            StreamEvent::Insert(stream) if stream.mime() == Mime::ApplicationPeershare => {
                let storage = storage.clone();
                let meili = meili.clone();
                async_std::task::spawn(async move {
                    let bytes = storage.get(&stream)?.to_vec()?;
                    meili
                        .add_manifest(stream, serde_json::from_slice(&bytes)?)
                        .await
                });
            }
            StreamEvent::Remove(stream) if stream.mime() == Mime::ApplicationPeershare => {
                let meili = meili.clone();
                async_std::task::spawn(async move { meili.remove_manifest(stream).await });
            }
            _ => {}
        });
    }
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
    joins.push(async_std::task::spawn(peershare_http::http(
        storage.clone(),
        url.clone(),
    )));
    #[cfg(feature = "fuse")]
    if let Some(dev_fuse) = dev_fuse {
        joins.push(tokio::task::spawn_blocking(|| {
            peershare_fuse::fuse(storage, dev_fuse)
        }));
    }
    futures::future::select_all(joins).await.0?;
    Ok(())
}

struct Meili {
    url: String,
    key: Option<String>,
}

impl Meili {
    pub fn new(url: String, key: Option<String>) -> Self {
        Self { url, key }
    }

    pub async fn initialize(&self) -> surf::Result<()> {
        self.builder(Method::Post, "/indexes")
            .body_json(&json!({ "uid": "content", "primaryKey": "streamId" }))?
            .send()
            .await?;
        self.builder(Method::Patch, "/indexes/content/settings")
            .body_json(&json!({
               "displayedAttributes": ["*"],
            }))?
            .send()
            .await?;
        Ok(())
    }

    pub async fn add_manifest(&self, stream: StreamId, manifest: Manifest) -> surf::Result<()> {
        let mut document = manifest.metadata;
        document.insert("streamId".into(), stream.to_string().into());
        document.insert("content".into(), manifest.content.into());
        self.builder(Method::Post, "/indexes/content/documents")
            .body_json(&[document])?
            .send()
            .await?;
        Ok(())
    }

    pub async fn remove_manifest(&self, stream: StreamId) -> surf::Result<()> {
        self.builder(
            Method::Delete,
            &format!("/indexes/content/documents/{stream}"),
        )
        .send()
        .await?;
        Ok(())
    }

    fn builder(&self, method: Method, path: &str) -> RequestBuilder {
        let mut builder =
            RequestBuilder::new(method, format!("{}{}", self.url, path).parse().unwrap());
        if let Some(master_key) = self.key.as_ref() {
            builder = builder.header("Authorization", format!("Bearer {}", master_key));
        }
        builder
    }
}
