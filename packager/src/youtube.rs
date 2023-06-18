use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
pub struct YoutubeOpts {
    id: String,
}

#[derive(Deserialize, Serialize)]
struct Metadata {
    title: String,
    description: String,
    thumbnail: String,
    channel: String,
    categories: Vec<String>,
    tags: Vec<String>,
}

fn youtube_dl(id: &str) -> Result<PathBuf> {
    let mut cmd = Command::new("youtube-dl");
    cmd.arg("--id")
        .arg("--write-info")
        .arg("--merge-output-format")
        .arg("mkv")
        .arg(format!("https://www.youtube.com/watch?v={id}"));
    anyhow::ensure!(cmd.status()?.success());
    let info = std::fs::read_to_string(format!("{id}.info.json"))?;
    let metadata: Metadata = serde_json::from_str(&info)?;
    let metadata = serde_json::to_string(&metadata)?;
    std::fs::write("metadata.json", metadata.as_bytes())?;
    Ok(format!("{id}.mkv").into())
}

pub fn youtube(opts: YoutubeOpts) -> Result<()> {
    let mkv = youtube_dl(&opts.id)?;
    crate::prepare_audio(&mkv, "audio.webm")?;
    crate::prepare_video(&mkv, "video.webm")?;
    //crate::transcribe("audio.weba", "content.txt")?;
    Ok(())
}
