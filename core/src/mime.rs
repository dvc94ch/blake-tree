use crate::Result;
use std::path::Path;
use std::str::FromStr;

static MIME_DB: &[(Mime, MimeType, &str, &str)] = &[
    (
        Mime::ApplicationOctetStream,
        MimeType::Application,
        "application/octet-stream",
        "bin",
    ),
    (
        Mime::ApplicationMsword,
        MimeType::Application,
        "application/msword",
        "doc",
    ),
    (
        Mime::ApplicationEpub,
        MimeType::Application,
        "application/epub+zip",
        "epub",
    ),
    (
        Mime::ApplicationGzip,
        MimeType::Application,
        "application/gzip",
        "gz",
    ),
    (
        Mime::ApplicationJavaArchive,
        MimeType::Application,
        "application/java-archive",
        "jar",
    ),
    (
        Mime::ApplicationJson,
        MimeType::Application,
        "application/json",
        "json",
    ),
    (
        Mime::ApplicationPdf,
        MimeType::Application,
        "application/pdf",
        "pdf",
    ),
    (
        Mime::ApplicationRtf,
        MimeType::Application,
        "application/rtf",
        "rtf",
    ),
    (
        Mime::ApplicationXhtml,
        MimeType::Application,
        "application/xhtml+xml",
        "xhtml",
    ),
    (
        Mime::ApplicationXml,
        MimeType::Application,
        "application/xml",
        "xml",
    ),
    (
        Mime::ApplicationZip,
        MimeType::Application,
        "application/zip",
        "zip",
    ),
    (
        Mime::ApplicationBzip,
        MimeType::Application,
        "application/x-bzip",
        "bz",
    ),
    (
        Mime::ApplicationBzip2,
        MimeType::Application,
        "application/x-bzip2",
        "bz2",
    ),
    (
        Mime::ApplicationTar,
        MimeType::Application,
        "application/x-tar",
        "tar",
    ),
    (Mime::AudioAac, MimeType::Audio, "audio/aac", "aac"),
    (Mime::AudioMidi, MimeType::Audio, "audio/midi", "midi"),
    (Mime::AudioMpeg, MimeType::Audio, "audio/mpeg", "mp3"),
    (Mime::AudioOgg, MimeType::Audio, "audio/ogg", "oga"),
    (Mime::AudioOpus, MimeType::Audio, "audio/opus", "opus"),
    (Mime::AudioWav, MimeType::Audio, "audio/wav", "wav"),
    (Mime::AudioWebm, MimeType::Audio, "audio/webm", "weba"),
    (Mime::FontOtf, MimeType::Font, "font/otf", "otf"),
    (Mime::FontTtf, MimeType::Font, "font/ttf", "ttf"),
    (Mime::FontWoff, MimeType::Font, "font/woff", "woff"),
    (Mime::FontWoff2, MimeType::Font, "font/woff2", "woff2"),
    (Mime::ImageAvif, MimeType::Image, "image/avif", "avif"),
    (Mime::ImageBmp, MimeType::Image, "image/bmp", "bmp"),
    (Mime::ImageGif, MimeType::Image, "image/gif", "gif"),
    (Mime::ImageJpeg, MimeType::Image, "image/jpeg", "jpg"),
    (Mime::ImagePng, MimeType::Image, "image/png", "png"),
    (Mime::ImageSvg, MimeType::Image, "image/svg+xml", "svg"),
    (Mime::ImageTiff, MimeType::Image, "image/tiff", "tiff"),
    (Mime::ImageWebp, MimeType::Image, "image/webp", "webp"),
    (Mime::TextCss, MimeType::Text, "text/css", "css"),
    (Mime::TextCsv, MimeType::Text, "text/csv", "csv"),
    (Mime::TextHtml, MimeType::Text, "text/html", "html"),
    (Mime::TextCalendar, MimeType::Text, "text/calendar", "ics"),
    (
        Mime::TextJavascript,
        MimeType::Text,
        "text/javascript",
        "js",
    ),
    (Mime::TextPlain, MimeType::Text, "text/plain", "txt"),
    (Mime::VideoMp4, MimeType::Video, "video/mp4", "mp4"),
    (Mime::VideoMpeg, MimeType::Video, "video/mpeg", "mpeg"),
    (Mime::VideoOgg, MimeType::Video, "video/ogg", "ogv"),
    (Mime::VideoMp2t, MimeType::Video, "video/mp2t", "ts"),
    (Mime::VideoWebm, MimeType::Video, "video/webm", "webm"),
    (Mime::Video3gpp, MimeType::Video, "video/3gpp", "3gp"),
    (Mime::Video3gpp2, MimeType::Video, "video/3gpp2", "3g2"),
    (
        Mime::ApplicationDash,
        MimeType::Application,
        "application/dash+xml",
        "mpd",
    ),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(u16)]
pub enum Mime {
    ApplicationOctetStream,
    ApplicationMsword,
    ApplicationEpub,
    ApplicationGzip,
    ApplicationJavaArchive,
    ApplicationJson,
    ApplicationPdf,
    ApplicationRtf,
    ApplicationXhtml,
    ApplicationXml,
    ApplicationZip,

    ApplicationBzip,
    ApplicationBzip2,
    ApplicationTar,

    AudioAac,
    AudioMidi,
    AudioMpeg,
    AudioOgg,
    AudioOpus,
    AudioWav,
    AudioWebm,

    FontOtf,
    FontTtf,
    FontWoff,
    FontWoff2,

    ImageAvif,
    ImageBmp,
    ImageGif,
    ImageJpeg,
    ImagePng,
    ImageSvg,
    ImageTiff,
    ImageWebp,

    TextCss,
    TextCsv,
    TextHtml,
    TextCalendar,
    TextJavascript,
    TextPlain,

    VideoMp4,
    VideoMpeg,
    VideoOgg,
    VideoMp2t,
    VideoWebm,
    Video3gpp,
    Video3gpp2,

    ApplicationDash,
}

impl Mime {
    pub fn from_u16(mime: u16) -> Option<Self> {
        MIME_DB.iter().find(|m| m.0 as u16 == mime).map(|m| m.0)
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        MIME_DB.iter().find(|m| m.2 == mime).map(|m| m.0)
    }

    pub fn from_ext(ext: &str) -> Option<Self> {
        MIME_DB.iter().find(|m| m.3 == ext).map(|m| m.0)
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        Self::from_ext(path.extension()?.to_str()?)
    }

    fn info(self) -> &'static (Mime, MimeType, &'static str, &'static str) {
        let info = &MIME_DB[self as usize];
        assert_eq!(self, info.0);
        info
    }

    pub fn r#type(&self) -> MimeType {
        self.info().1
    }

    pub fn mime(&self) -> &'static str {
        self.info().2
    }

    pub fn extension(&self) -> &'static str {
        self.info().3
    }
}

impl Default for Mime {
    fn default() -> Self {
        Self::ApplicationOctetStream
    }
}

impl std::fmt::Display for Mime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.mime())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum MimeType {
    Application,
    Audio,
    Font,
    Image,
    Model,
    Text,
    Video,
    Message,
    Multipart,
}

impl AsRef<str> for MimeType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Application => "application",
            Self::Audio => "audio",
            Self::Font => "font",
            Self::Image => "image",
            Self::Model => "model",
            Self::Text => "text",
            Self::Video => "video",
            Self::Message => "message",
            Self::Multipart => "multipart",
        }
    }
}

impl FromStr for MimeType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "application" => Self::Application,
            "audio" => Self::Audio,
            "font" => Self::Font,
            "image" => Self::Image,
            "model" => Self::Model,
            "text" => Self::Text,
            "video" => Self::Video,
            "message" => Self::Message,
            "multipart" => Self::Multipart,
            _ => anyhow::bail!("invalid mime type"),
        })
    }
}
