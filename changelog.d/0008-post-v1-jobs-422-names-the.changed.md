- **POST `/v1/jobs` 422 names the capture diagnosis.** A parse-fatal or
  check-fatal world returns `{error:{code,message}}` with the NIKA code
  when the engine stamped one, including analysis codes (AUTH/SEC)
  that live on `finding.code`. Symlink and other capture refuses stay
  `admission_refused`. Paths stay dropped.
