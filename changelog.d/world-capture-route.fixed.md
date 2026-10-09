- **A stated endpoint route no longer stops a trial as a path outside the
  project.** Before a candidate rehearses, the Session observes the files
  the request names. A destination the request states as the route of an
  endpoint it names (`POST /notifications/stock` to the stated local sink),
  which the candidate sends to, is that endpoint's route and no file: it is
  left out of that observation, and the trial room is still handed it. An
  unstated route, a rooted or `..` file outside the project, and a route the
  candidate also writes as a file are still refused in the same words.
