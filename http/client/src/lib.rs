use anyhow::Result;
use peershare_core::{Manifest, Mime, Range, StreamId};
use surf::Url;

pub struct Client {
    url: Url,
}

impl Client {
    pub fn new(url: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            url: url.as_ref().parse()?,
        })
    }

    pub async fn list(&self) -> Result<Vec<StreamId>> {
        let streams: Vec<String> = surf::get(format!("{}streams", &self.url))
            .send()
            .await
            .map_err(|e| e.into_inner())?
            .body_json()
            .await
            .map_err(|e| e.into_inner())?;
        streams.into_iter().map(|s| s.parse()).collect()
    }

    pub fn url(&self, id: StreamId) -> Url {
        let mut url = self.url.clone();
        url.set_path(&format!("/streams/{id}"));
        url
    }

    pub async fn content(&self, id: StreamId) -> Result<StreamId> {
        Ok(if id.mime() == Mime::ApplicationPeershare {
            let bytes = self.read(id, None).await?;
            let manifest: Manifest = serde_json::from_slice(&bytes)?;
            manifest.stream_id
        } else {
            id
        })
    }

    pub async fn manifest(
        &self,
        stream_id: StreamId,
        metadata: serde_json::Map<String, serde_json::Value>,
        content: String,
    ) -> Result<StreamId> {
        let manifest = Manifest {
            stream_id,
            metadata,
            content,
        };
        let manifest = serde_json::to_string(&manifest)?;
        self.create(Mime::ApplicationPeershare, manifest.as_bytes())
            .await
    }

    pub async fn create(&self, mime: Mime, data: &[u8]) -> Result<StreamId> {
        let stream_id: String = surf::post(format!("{}streams", &self.url))
            .body_bytes(data)
            .content_type(mime.to_string().as_str())
            .send()
            .await
            .map_err(|e| e.into_inner())?
            .body_json()
            .await
            .map_err(|e| e.into_inner())?;
        stream_id.parse()
    }

    pub async fn read(&self, id: StreamId, range: Option<Range>) -> Result<Vec<u8>> {
        let mut builder = surf::get(self.url(id));
        if let Some(range) = range {
            builder = builder.header(
                "Range",
                format!("bytes={}-{}", range.offset(), range.end().saturating_sub(1)),
            );
        }
        Ok(builder
            .send()
            .await
            .map_err(|e| e.into_inner())?
            .body_bytes()
            .await
            .map_err(|e| e.into_inner())?)
    }

    pub async fn ranges(&self, id: StreamId) -> Result<Vec<Range>> {
        Ok(surf::get(format!("{}streams/{}/ranges", &self.url, id))
            .send()
            .await
            .map_err(|e| e.into_inner())?
            .body_json()
            .await
            .map_err(|e| e.into_inner())?)
    }

    pub async fn missing_ranges(&self, id: StreamId) -> Result<Vec<Range>> {
        Ok(
            surf::get(format!("{}streams/{}/missing-ranges", &self.url, id))
                .send()
                .await
                .map_err(|e| e.into_inner())?
                .body_json()
                .await
                .map_err(|e| e.into_inner())?,
        )
    }

    pub async fn remove(&self, id: StreamId) -> Result<()> {
        surf::delete(format!("{}streams/{}", &self.url, id))
            .send()
            .await
            .map_err(|e| e.into_inner())?;
        Ok(())
    }
}
