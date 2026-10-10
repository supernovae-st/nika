- **A Session's Stop stops its run over Serve too.** The run door Serve
  lends a Session never armed a Stop: over HTTP, a Stop during a run
  answered `run_underway` and the run went on. It now asks the resident's
  own job cancellation (the one `POST /v1/jobs/{id}/cancel` takes) once:
  the run is stopping and ends interrupted, a second Stop sends nothing
  more, and a Stop before the job's admission admits nothing. A resident
  that cannot stop a job says so.
