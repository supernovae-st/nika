- **Cost review details show the host evidence as its public view.**
  `details` on an unknown-cost decision printed the host cap evidence as
  a Rust debug dump. It now prints the same view the machine surfaces
  expose: whether the host allows the choice, and for each layer its
  class, origin and cap.
