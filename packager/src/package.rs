use anyhow::Result;
use clap::Parser;
use peershare_core::{Manifest, Mime, StreamId};
use peershare_http_client::Client;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
pub struct PackageOpts {
    #[clap(short, long)]
    video: Vec<PathBuf>,
    #[clap(short, long)]
    audio: Option<PathBuf>,
    #[clap(short, long)]
    url: Option<String>,
    #[clap(short, long)]
    key: Option<String>,
    #[clap(short, long)]
    metadata: Option<PathBuf>,
    #[clap(short, long)]
    content: Option<PathBuf>,
}

pub async fn package(opts: PackageOpts) -> Result<()> {
    let url = opts
        .url
        .unwrap_or_else(|| "http://127.0.0.1:3000".to_string());
    let client = Client::new(&url)?;
    let stream = if let Some(key) = opts.key {
        mpd_drm(&client, &opts.video, opts.audio.as_deref(), &key).await?
    } else {
        mpd(&client, &opts.video, opts.audio.as_deref()).await?
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

async fn create_input<'a>(
    client: &Client,
    path: &'a Path,
    mime: Mime,
) -> Result<(&'a str, StreamId)> {
    let data = std::fs::read(path)?;
    let stream = client.create(mime, &data).await?;
    Ok((path.file_name().unwrap().to_str().unwrap(), stream))
}

async fn create_inputs<'a>(
    client: &Client,
    video: &'a [PathBuf],
    audio: Option<&'a Path>,
) -> Result<Vec<(&'a str, StreamId)>> {
    let mut inputs = Vec::with_capacity(video.len() + 1);
    for path in video {
        inputs.push(create_input(client, path, Mime::VideoWebm).await?);
    }
    if let Some(path) = audio {
        inputs.push(create_input(client, path, Mime::AudioWebm).await?);
    }
    Ok(inputs)
}

async fn rewrite_mpd(
    client: &Client,
    inputs: &[(&str, StreamId)],
    path: &Path,
) -> Result<StreamId> {
    let mut mpd = std::fs::read_to_string(path)?;
    for (name, stream) in inputs {
        mpd = mpd.replace(name, &format!("/streams/{stream}"));
    }
    mpd = mpd.replace(r#"codecs="av1""#, r#"codecs="av01.0.31M.08""#);
    client.create(Mime::ApplicationDash, mpd.as_bytes()).await
}

async fn mpd_drm(
    client: &Client,
    video: &[PathBuf],
    audio: Option<&Path>,
    key: &str,
) -> Result<StreamId> {
    let mut cmd = Command::new("packager");
    let mut voutputs = Vec::with_capacity(video.len() + 1);
    for (i, input) in video.iter().enumerate() {
        let output = format!("/tmp/video{}.webm", i);
        cmd.arg(format!(
            "in={},stream=video,output={},drm_label=key",
            input.to_str().unwrap(),
            &output,
        ));
        voutputs.push(output.into());
    }
    let mut aoutput = None;
    if let Some(input) = audio {
        let output = "/tmp/audio.webm";
        cmd.arg(format!(
            "in={},stream=audio,output={},drm_label=key",
            input.to_str().unwrap(),
            &output,
        ));
        aoutput = Some(output.as_ref());
    }
    let mpd = "/tmp/output.mpd".as_ref();
    cmd.arg("--enable_raw_key_encryption")
        .arg("--keys")
        .arg(format!(
            "label=key:key_id=00000000000000000000000000000000:key={}",
            key
        ))
        .arg("--mpd_output")
        .arg(mpd)
        .arg("--clear_lead")
        .arg("0")
        .arg("--protection_systems")
        .arg("Widevine");
    anyhow::ensure!(cmd.status()?.success());
    let inputs = create_inputs(client, &voutputs, aoutput).await?;
    rewrite_mpd(client, &inputs, mpd).await
}

async fn mpd(client: &Client, video: &[PathBuf], audio: Option<&Path>) -> Result<StreamId> {
    let inputs = create_inputs(client, video, audio).await?;
    let output = "/tmp/output.mpd".as_ref();
    let mut cmd = Command::new("ffmpeg");
    for (path, _) in &inputs {
        cmd.arg("-f").arg("webm_dash_manifest").arg("-i").arg(path);
    }
    cmd.arg("-c").arg("copy");
    for i in 0..inputs.len() {
        cmd.arg("-map").arg(&i.to_string());
    }
    let mut adaptation_sets = String::new();
    if !video.is_empty() {
        adaptation_sets.push_str("id=0,streams=0");
        for i in 1..video.len() {
            adaptation_sets.push(',');
            adaptation_sets.push_str(&i.to_string())
        }
    }
    if audio.is_some() {
        adaptation_sets.push_str(" id=1,streams=");
        adaptation_sets.push_str(&video.len().to_string());
    }
    cmd.arg("-f")
        .arg("webm_dash_manifest")
        .arg("-adaptation_sets")
        .arg(adaptation_sets)
        .arg("-y")
        .arg(output);
    anyhow::ensure!(cmd.status()?.success());
    rewrite_mpd(client, &inputs, output).await
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
