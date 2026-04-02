"""Type stubs for nika_sdk."""

from typing import Optional

class NikaError(Exception):
    """Base exception for Nika SDK errors."""
    ...

class JobInfo:
    """Status information for a job."""
    job_id: str
    status: str
    workflow: str
    created_at: str
    started_at: Optional[str]
    completed_at: Optional[str]
    exit_code: Optional[int]
    output: Optional[str]

class JobResult:
    """Final result of a completed job."""
    job_id: str
    output: Optional[str]

class ArtifactInfo:
    """Artifact metadata."""
    name: str
    size: int
    content_type: str
    checksum: Optional[str]

class Job:
    """Handle to a submitted workflow job."""
    @property
    def job_id(self) -> str: ...
    def status(self) -> JobInfo: ...
    def cancel(self) -> JobInfo: ...
    def wait(self) -> JobResult: ...
    def artifacts(self) -> list[ArtifactInfo]: ...
    def download_artifact(self, name: str) -> bytes: ...
    def events(self) -> list[dict[str, object]]: ...

class Client:
    """Nika SDK client — connects to a remote nika serve instance."""
    def __init__(self, base_url: str, token: str) -> None: ...
    def submit(
        self,
        workflow: str,
        inputs: Optional[dict[str, object]] = None,
        resume_from: Optional[str] = None,
    ) -> Job: ...
    def run_sync(
        self,
        workflow: str,
        inputs: Optional[dict[str, object]] = None,
    ) -> JobResult: ...
    def health(self) -> bool: ...
