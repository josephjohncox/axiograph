//! Evidence pointers for ingestion/discovery artifacts.
//!
//! These are used throughout the ingestion pipeline to link any extracted proposal back to
//! the concrete evidence that supports it (document chunk ids, file paths, etc).

use serde::{Deserialize, Serialize};

/// A pointer to evidence supporting a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePointer {
    /// The chunk id (see `Chunk.chunk_id`) that contains supporting evidence.
    pub chunk_id: String,
    /// Optional human-friendly locator (path, url, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// Optional span id (e.g. a section header or line-range label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
}
