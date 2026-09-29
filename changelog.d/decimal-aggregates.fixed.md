- **Totals and averages are exact, or the run says why not.**
  - A sum of `0.1` and `0.2` is written `0.3`, not `0.30000000000000004`; an average of `0.1`
    and `0.2` is `0.15`; totals of large integers keep every digit.
  - An average with no finite decimal expansion (`4` divided by `3`) stops the run and asks for
    a rounding; a rounding the request states is applied exactly, half away from zero.
  - Exact arithmetic is bounded: a value needing more than 1000 digits stops the run naming it,
    instead of silently dropping a tiny part (`1e-2000` beside `1` used to give `1.0`).
  - A result, or a column the request writes as a JSON number, that no JSON number carries
    exactly (`0.12345678901234567891`, or an integer past 2^64) stops the run naming the
    value and what it would have become, instead of writing a rounded number.
