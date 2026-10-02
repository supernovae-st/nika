- **The Session says what a held answer round is waiting for.**
  - When the authoring model writes the workflow itself, an answer round that finishes it
    is ready only once a judge that round can permit finds it faithful to the whole request.
  - With an authoring model, the Session asks that model as the judge. This is one bounded
    call (two when the verdict is unfaithful), counted in the receipt and admitted like any
    authoring call.
  - Without an authoring model, the round stays incomplete and the built workflow is only a
    preview. The Session now says so: "The workflow is built but not proposed…". It no longer
    says an authoring step failed on Nika's side, and no longer says the request cannot be
    built. It names why (no authoring model, or no judgment in this round settled it) and the
    way on (`/intelligence`, then state the request again · `/meaning`). Nothing is written.
  - The Serve and CLI compile doors keep a `deterministicOnly` answer round at zero calls, and
    it stays incomplete with its preview.
  - `/details` no longer says an answer round made zero calls. The knowledge line now reads
    "this answer round replayed it and presented the pack to no call", and the round's own
    receipt shows the judge's call when it made one.
