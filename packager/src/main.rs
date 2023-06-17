use anyhow::Result;
use clap::Parser;
use peershare_core::{Manifest, Mime, StreamId};
use peershare_http_client::Client;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
struct Opts {
    inputs: Vec<PathBuf>,
    #[clap(long)]
    url: Option<String>,
    #[clap(long)]
    key: Option<String>,
    #[clap(long)]
    metadata: Option<PathBuf>,
    #[clap(long)]
    content: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::parse();
    let url = opts
        .url
        .unwrap_or_else(|| "http://127.0.0.1:3000".to_string());
    let client = Client::new(&url)?;
    let stream = if let Some(key) = opts.key {
        mpd_drm(&client, &opts.inputs, &key).await?
    } else {
        mpd(&client, &opts.inputs).await?
    };
    let stream = manifest(
        &client,
        stream,
        opts.metadata.as_deref(),
        opts.content.as_deref(),
    )
    .await?;
    println!("{}", stream);
    Ok(())
}

async fn create_inputs<'a>(
    client: &Client,
    paths: &'a [PathBuf],
) -> Result<Vec<(&'a str, StreamId)>> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let data = std::fs::read(path)?;
        let stream = client.create(Mime::VideoWebm, &data).await?;
        inputs.push((path.file_name().unwrap().to_str().unwrap(), stream));
    }
    Ok(inputs)
}

async fn create_mpd(client: &Client, inputs: &[(&str, StreamId)], path: &Path) -> Result<StreamId> {
    let mut mpd = std::fs::read_to_string(path)?;
    for (name, stream) in inputs {
        mpd = mpd.replace(name, &format!("/streams/{stream}"));
    }
    mpd = mpd.replace(r#"codecs="av1""#, r#"codecs="av01.0.31M.08""#);
    client.create(Mime::ApplicationDash, mpd.as_bytes()).await
}

async fn mpd_drm(client: &Client, inputs: &[PathBuf], key: &str) -> Result<StreamId> {
    let output = "/tmp/output".as_ref();
    let mut outputs = Vec::with_capacity(inputs.len());
    let mut cmd = Command::new("packager");
    for (i, input) in inputs.iter().enumerate() {
        let output = format!("/tmp/output{}.webm", i);
        cmd.arg(format!(
            "in={},stream=video,output={},drm_label=key",
            input.to_str().unwrap(),
            &output,
        ));
        outputs.push(output.into());
    }
    cmd.arg("--enable_raw_key_encryption")
        .arg("--keys")
        .arg(format!(
            "label=key:key_id=00000000000000000000000000000000:key={}",
            key
        ))
        .arg("--mpd_output")
        .arg(output)
        .arg("--clear_lead")
        .arg("0")
        .arg("--protection_systems")
        .arg("Widevine");
    anyhow::ensure!(cmd.status()?.success());
    let inputs = create_inputs(client, &outputs).await?;
    create_mpd(client, &inputs, output).await
}

async fn mpd(client: &Client, inputs: &[PathBuf]) -> Result<StreamId> {
    let inputs = create_inputs(client, inputs).await?;
    let output = "/tmp/output".as_ref();
    let mut cmd = Command::new("ffmpeg");
    let mut streams = String::new();
    for (i, (path, _)) in inputs.iter().enumerate() {
        cmd.arg("-f").arg("webm_dash_manifest").arg("-i").arg(path);
        if !streams.is_empty() {
            streams.push(',');
        }
        streams.push_str(&i.to_string());
    }
    for i in 0..inputs.len() {
        cmd.arg("-map").arg(&i.to_string());
    }
    cmd.arg("-c")
        .arg("copy")
        .arg("-f")
        .arg("webm_dash_manifest")
        .arg("-adaptation_sets")
        .arg(format!("id=0,streams={}", streams))
        .arg(output);
    anyhow::ensure!(cmd.status()?.success());
    create_mpd(client, &inputs, output).await
}

async fn manifest(
    client: &Client,
    stream_id: StreamId,
    metadata: Option<&Path>,
    content: Option<&Path>,
) -> Result<StreamId> {
    let metadata = if let Some(path) = metadata {
        let metadata = std::fs::read_to_string(path)?;
        serde_json::from_str(&metadata)?
    } else {
        Default::default()
    };
    let content = if let Some(path) = content {
        std::fs::read_to_string(path)?
    } else {
        Default::default()
    };
    let manifest = Manifest {
        stream_id,
        metadata,
        content,
    };
    let manifest = serde_json::to_string(&manifest)?;
    client
        .create(Mime::ApplicationPeershare, manifest.as_bytes())
        .await
}
