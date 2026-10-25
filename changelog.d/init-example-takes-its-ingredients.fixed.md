- **`nika init --example` founds a project its lesson can run in.** The example lane copied the
  workflow alone: 19 lessons that read files under `examples/fixtures/` (`csv-chart-report`,
  `meeting-actions`, …) failed their first suggested run with « file not found », and
  `10-compose-pipeline` founded without the child it invokes, so its own audit refused
  NIKA-COMP-001. Init now stages what `nika try` stages: the fixtures under the project root, where
  runs resolve them, and composition children beside the lesson in `workflows/`. Each staged file
  is a `created` row, and an existing file is never overwritten.
