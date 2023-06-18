use anyhow::Result;
use std::path::Path;
use std::process::Command;

mod package;
mod youtube;

pub use crate::package::*;
pub use crate::youtube::*;

pub fn prepare_audio(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i")
        .arg(input.as_ref())
        .arg("-c:a")
        .arg("libopus")
        .arg("-b:a")
        .arg("128k")
        .arg("-vn")
        .arg("-f")
        .arg("webm")
        .arg("-dash")
        .arg("1")
        .arg(output.as_ref());
    anyhow::ensure!(cmd.status()?.success());
    Ok(())
}

pub fn prepare_video(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<()> {
    let mut cmd = Command::new("av1an");
    cmd.arg("-i")
        .arg(input.as_ref())
        .arg("-e")
        .arg("rav1e")
        //.arg("--target-quality")
        //.arg(vmaf.to_string())
        .arg("-o")
        .arg(output.as_ref());
    anyhow::ensure!(cmd.status()?.success());
    Ok(())
}

pub fn transcribe(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<()> {
    let input = input.as_ref().to_str().unwrap();
    let output = output.as_ref().to_str().unwrap();
    let python = format!(
        r#"from transformers import pipeline
    transcriber = pipeline(model='openai/whisper-base')
    transcript = transcriber('{input}')
    with open('{output}', 'w') as f:
        f.write(transcript['text'])
    "#
    );
    let mut cmd = Command::new("python");
    cmd.arg("-c").arg(python);
    anyhow::ensure!(cmd.status()?.success());
    Ok(())
}
