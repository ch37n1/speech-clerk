//! Deterministic transcript post-processing.

/// Normalize whitespace in transcript text.
#[must_use]
pub fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize_whitespace;

    #[test]
    fn collapses_whitespace() {
        assert_eq!(normalize_whitespace(" hello\n  world\t"), "hello world");
    }
}
