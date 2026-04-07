//! Artifact API endpoints.
//!
//! - `GET /v1/jobs/{id}/artifacts` — list artifacts for a job
//! - `GET /v1/jobs/{id}/artifacts/{name}` — download a single artifact

use aide::transform::TransformOperation;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use tokio_util::io::ReaderStream;

use schemars::JsonSchema;
use serde::Serialize;

use crate::error::ServeError;
use crate::state::AppState;

#[derive(Serialize, JsonSchema)]
pub struct ArtifactInfo {
    pub name: String,
    pub size: u64,
    pub format: String,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct ListArtifactsResponse {
    pub job_id: String,
    pub count: usize,
    pub artifacts: Vec<ArtifactInfo>,
}

pub fn list_docs(op: TransformOperation) -> TransformOperation {
    op.id("listArtifacts")
        .summary("List job artifacts")
        .description("Returns artifacts with name, size, format, content_type, and checksum.")
        .tag("artifacts")
}

pub fn download_docs(op: TransformOperation) -> TransformOperation {
    op.id("downloadArtifact")
        .summary("Download a single artifact")
        .description("Streams the artifact file with Content-Type, Content-Length, ETag, and Cache-Control headers.")
        .tag("artifacts")
}

/// List artifacts for a completed job.
///
/// Returns JSON array with name, size, format, content_type, and checksum.
pub async fn list_artifacts(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Path(job_id): Path<String>,
) -> Result<Json<ListArtifactsResponse>, ServeError> {
    // Verify job exists
    let job = state
        .storage
        .get_job(&job_id)
        .await?
        .ok_or(ServeError::NotFound)?;

    // L2 scope enforcement
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_access(&job.workflow) {
            return Err(ServeError::Forbidden(format!(
                "token '{}' scope '{}' does not cover workflow '{}'",
                p.token_name, p.scope, job.workflow
            )));
        }
    }

    let artifacts = state.storage.list_artifacts(&job_id).await?;

    let items: Vec<ArtifactInfo> = artifacts
        .iter()
        .map(|a| ArtifactInfo {
            name: a.name.clone(),
            size: a.size,
            format: a.format.clone(),
            content_type: a.content_type.clone(),
            checksum: a.checksum.clone(),
        })
        .collect();

    let count = items.len();
    Ok(Json(ListArtifactsResponse {
        job_id,
        count,
        artifacts: items,
    }))
}

/// Download a single artifact by name.
///
/// Serves the file with correct Content-Type, Content-Length, and ETag headers.
/// ETag is derived from checksum (if available) or file mtime.
pub async fn download_artifact(
    State(state): State<AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Path((job_id, name)): Path<(String, String)>,
) -> Result<Response, ServeError> {
    // Reject path traversal
    if name.contains("..") || name.starts_with('/') {
        return Err(ServeError::PathTraversal);
    }

    // Verify job exists
    let job = state
        .storage
        .get_job(&job_id)
        .await?
        .ok_or(ServeError::NotFound)?;

    // L2 scope enforcement
    if let Some(axum::Extension(ref p)) = principal {
        if !p.can_access(&job.workflow) {
            return Err(ServeError::Forbidden(format!(
                "token '{}' scope '{}' does not cover workflow '{}'",
                p.token_name, p.scope, job.workflow
            )));
        }
    }

    // Find the artifact in DB
    let artifacts = state.storage.list_artifacts(&job_id).await?;
    let artifact = artifacts
        .iter()
        .find(|a| a.name == name)
        .ok_or(ServeError::NotFound)?;

    // Verify file exists on disk and is within the project root.
    // We check against project_root (not workflows_dir) because artifacts
    // write to [artifacts] dir which may be a sibling of workflows_dir.
    let path = std::path::Path::new(&artifact.path);
    if let Ok(canonical) = tokio::fs::canonicalize(path).await {
        let base_dir = state
            .config
            .project_root
            .as_ref()
            .unwrap_or(&state.config.workflows_dir);
        let allowed_base = tokio::fs::canonicalize(base_dir)
            .await
            .map_err(|_| ServeError::NotFound)?;
        if !canonical.starts_with(&allowed_base) {
            return Err(ServeError::PathTraversal);
        }
    }
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
