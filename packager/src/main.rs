use anyhow::Result;
use clap::Parser;
use peershare_core::{Mime, StreamId};
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
    println!("{}", stream);
    Ok(())
}

async fn create_inputs<'a>(
    client: &Client,
    paths: &'a [PathBuf],
) -> Result<Vec<(&'a Path, StreamId)>> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let data = std::fs::read(path)?;
        let stream = client.create(Mime::VideoWebm, &data).await?;
        inputs.push((path.as_path(), stream));
    }
    Ok(inputs)
}

async fn create_mpd(
    client: &Client,
    inputs: &[(&Path, StreamId)],
    path: &Path,
) -> Result<StreamId> {
    let mut mpd = std::fs::read_to_string(path)?;
    for (path, stream) in inputs {
        mpd = mpd.replace(path.to_str().unwrap(), &stream.to_string());
    }
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
        let i = i.to_string();
        cmd.arg("-f")
            .arg("webm_dash_manifest")
            .arg("-i")
            .arg(path)
            .arg("-map")
            .arg(&i);
        if !streams.is_empty() {
            streams.push(',');
        }
        streams.push_str(&i);
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
