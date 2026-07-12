//! Count-noun agreement for visible strings — the operator surface
//! reads as prose, so it never prints `1 tasks` nor the lazy `task(s)`.

/// `count(3, "task")` → `3 tasks` · `count(1, "task")` → `1 task`.
///
/// Regular plurals only — every counted noun on the surface (task ·
/// wave · run · finding · hint · edge · key) pluralizes with a plain
/// `s`; an irregular noun would earn its own arm the day one exists.
pub(crate) fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn one_reads_singular_and_the_rest_plural() {
        assert_eq!(count(1, "task"), "1 task");
        assert_eq!(count(0, "task"), "0 tasks");
        assert_eq!(count(3, "wave"), "3 waves");
        // A compound noun pluralizes at its tail — the caller can pass
        // a qualified noun and the agreement stays right.
        assert_eq!(count(1, "more hint"), "1 more hint");
        assert_eq!(count(4, "downstream task"), "4 downstream tasks");
    }
}
