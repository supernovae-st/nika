- **nika:fetch can observe named HTTP statuses.** Explicit response.accept
  returns status_code, a sanitized final URL (or null) and the extracted
  body. The exact set contains 1–16 distinct integer statuses with no
  implicit successful statuses. Unlisted statuses and transport/security
  failures still fail. Malformed policies refuse before sending. Traverse
  rejects response and headers at check and run; the new static header
  rejection requires the next minor release.
