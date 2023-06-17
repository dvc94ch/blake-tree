use crate::StreamId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Stream ID of the source file.
    pub stream_id: StreamId,
    /// Metadata to show in search results.
    pub metadata: serde_json::Map<String, serde_json::Value>,
    /// Scraped content to use for search.
    pub content: String,
}
