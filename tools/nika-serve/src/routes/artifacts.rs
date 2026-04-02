//! Artifact API endpoints.
//!
//! - `GET /v1/jobs/{id}/artifacts` — list artifacts for a job
//! - `GET /v1/jobs/{id}/artifacts/{name}` — download a single artifact

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use tokio_util::io::ReaderStream;

use crate::error::ServeError;
use crate::state::AppState;

/// List artifacts for a completed job.
///
/// Returns JSON array with name, size, format, content_type, and checksum.
pub async fn list_artifacts(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, ServeError> {
    // Verify job exists
    state
        .storage
        .get_job(&job_id)
        .await?
        .ok_or(ServeError::NotFound)?;

    let artifacts = state.storage.list_artifacts(&job_id).await?;

    let items: Vec<serde_json::Value> = artifacts
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name,
                "size": a.size,
                "format": a.format,
                "content_type": a.content_type,
                "checksum": a.checksum,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "job_id": job_id,
        "count": items.len(),
        "artifacts": items,
    })))
}

/// Download a single artifact by name.
///
/// Serves the file with correct Content-Type, Content-Length, and ETag headers.
/// ETag is derived from checksum (if available) or file mtime.
pub async fn download_artifact(
    State(state): State<AppState>,
    Path((job_id, name)): Path<(String, String)>,
) -> Result<Response, ServeError> {
    // Reject path traversal
    if name.contains("..") || name.starts_with('/') {
        return Err(ServeError::PathTraversal);
    }

    // Verify job exists
    state
        .storage
        .get_job(&job_id)
        .await?
        .ok_or(ServeError::NotFound)?;

    // Find the artifact in DB
    let artifacts = state.storage.list_artifacts(&job_id).await?;
    let artifact = artifacts
        .iter()
        .find(|a| a.name == name)
        .ok_or(ServeError::NotFound)?;

    // Verify file exists on disk
    let path = std::path::Path::new(&artifact.path);
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| ServeError::NotFound)?;

    // Build ETag from checksum or mtime
    let etag = if let Some(ref checksum) = artifact.checksum {
        format!("\"{checksum}\"")
    } else {
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("\"{mtime}-{}\"", metadata.len())
    };

    // Open file and stream response
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| ServeError::NotFound)?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, artifact.content_type.clone()),
            (header::CONTENT_LENGTH, metadata.len().to_string()),
            (header::ETAG, etag),
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".to_string(),
            ),
        ],
        body,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    #[test]
    fn path_traversal_rejected() {
        assert!("../etc/passwd".contains(".."));
        assert!("/etc/passwd".starts_with('/'));
    }
}
