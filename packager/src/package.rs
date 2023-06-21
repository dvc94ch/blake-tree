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
    audio: Vec<PathBuf>,
    #[clap(short, long)]
    url: Option<String>,
    #[clap(short, long)]
    key: Option<String>,
    #[clap(short, long)]
    metadata: Option<PathBuf>,
    #[clap(short, long)]
    content: Option<PathBuf>,
}

struct Package {
    video: Vec<PathBuf>,
    audio: Vec<PathBuf>,
    mpd: PathBuf,
}

pub async fn package(opts: PackageOpts) -> Result<()> {
    let url = opts
        .url
        .unwrap_or_else(|| "http://127.0.0.1:3000".to_string());
    let client = Client::new(&url)?;
    let package = if let Some(key) = opts.key {
        mpd_drm(opts.video, opts.audio, &key)?
    } else {
        mpd(opts.video, opts.audio)?
    };
    let stream = create_package(&client, &package).await?;
    println!("mpd: {}", stream);
    let stream = manifest(
        &client,
        stream,
        opts.metadata.as_deref(),
        opts.content.as_deref(),
    )
    .await?;
    println!("manifest: {}", stream);
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

async fn create_package<'a>(client: &Client, package: &Package) -> Result<StreamId> {
    let mut mpd = std::fs::read_to_string(&package.mpd)?;
    for path in &package.video {
        let (name, stream) = create_input(client, path, Mime::VideoWebm).await?;
        println!("video: {stream}");
        mpd = mpd.replace(name, &format!("/streams/{stream}"));
    }
    for path in &package.audio {
        let (name, stream) = create_input(client, path, Mime::AudioWebm).await?;
        println!("audio: {stream}");
        mpd = mpd.replace(name, &format!("/streams/{stream}"));
    }
    mpd = mpd.replace(r#"codecs="av1""#, r#"codecs="av01.0.31M.08""#);
    client.create(Mime::ApplicationDash, mpd.as_bytes()).await
}

fn mpd_drm(video: Vec<PathBuf>, audio: Vec<PathBuf>, key: &str) -> Result<Package> {
    let mut cmd = Command::new("packager");
    let mut voutputs = Vec::with_capacity(video.len());
    for (i, input) in video.iter().enumerate() {
        let output = format!("/tmp/video{}.webm", i);
        cmd.arg(format!(
            "in={},stream=video,output={},drm_label=key",
            input.to_str().unwrap(),
            &output,
        ));
        voutputs.push(output.into());
    }
    let mut aoutputs = Vec::with_capacity(audio.len());
    for (i, input) in audio.iter().enumerate() {
        let output = format!("/tmp/audio{}.webm", i);
        cmd.arg(format!(
            "in={},stream=audio,output={},drm_label=key",
            input.to_str().unwrap(),
            &output,
        ));
        aoutputs.push(output.into());
    }
    let mpd = "/tmp/output.mpd";
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
    Ok(Package {
        video: voutputs,
        audio: aoutputs,
        mpd: mpd.into(),
    })
}

fn mpd(video: Vec<PathBuf>, audio: Vec<PathBuf>) -> Result<Package> {
    let mut cmd = Command::new("ffmpeg");
    for path in &video {
        cmd.arg("-f").arg("webm_dash_manifest").arg("-i").arg(path);
    }
    for path in &audio {
        cmd.arg("-f").arg("webm_dash_manifest").arg("-i").arg(path);
    }
    cmd.arg("-c").arg("copy");
    for i in 0..video.len() {
        cmd.arg("-map").arg(&i.to_string());
    }
    for i in 0..audio.len() {
        let i = i + video.len();
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
    if !audio.is_empty() {
        adaptation_sets.push_str(" id=1,streams=");
        adaptation_sets.push_str(&video.len().to_string());
        for i in 1..audio.len() {
            let i = i + video.len();
            adaptation_sets.push(',');
            adaptation_sets.push_str(&i.to_string())
        }
    }
    let mpd = "/tmp/output.mpd";
    cmd.arg("-f")
        .arg("webm_dash_manifest")
        .arg("-adaptation_sets")
        .arg(adaptation_sets)
        .arg("-y")
        .arg(mpd);
    anyhow::ensure!(cmd.status()?.success());
    Ok(Package {
        video,
        audio,
        mpd: mpd.into(),
    })
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
