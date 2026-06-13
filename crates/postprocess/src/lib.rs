//! Deterministic transcript post-processing.

/// One custom transcript replacement rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementRule {
    /// Text to replace after punctuation and whitespace cleanup.
    pub pattern: String,
    /// Replacement text.
    pub replacement: String,
}

impl ReplacementRule {
    /// Create a replacement rule.
    #[must_use]
    pub fn new(pattern: impl Into<String>, replacement: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            replacement: replacement.into(),
        }
    }

    /// Return `true` when this rule cannot affect transcript text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }
}

/// Deterministic post-processing configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostProcessor {
    replacements: Vec<ReplacementRule>,
}

impl PostProcessor {
    /// Create a processor with no custom replacements.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a processor with ordered custom replacement rules.
    #[must_use]
    pub fn with_replacements(replacements: Vec<ReplacementRule>) -> Self {
        Self { replacements }
    }

    /// Replace all configured replacement rules.
    pub fn set_replacements(&mut self, replacements: Vec<ReplacementRule>) {
        self.replacements = replacements;
    }

    /// Return the configured replacement rules.
    #[must_use]
    pub fn replacements(&self) -> &[ReplacementRule] {
        &self.replacements
    }

    /// Apply whitespace cleanup, punctuation cleanup, and custom replacements.
    #[must_use]
    pub fn process(&self, input: &str) -> String {
        let mut output = cleanup_punctuation(&normalize_whitespace(input));

        for rule in &self.replacements {
            if !rule.is_empty() {
                output = output.replace(&rule.pattern, &rule.replacement);
            }
        }

        cleanup_punctuation(&normalize_whitespace(&output))
    }
}

/// Normalize whitespace in transcript text.
#[must_use]
pub fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalize spacing around punctuation marks.
#[must_use]
pub fn cleanup_punctuation(input: &str) -> String {
    let without_space_before = input
        .replace(" .", ".")
        .replace(" ,", ",")
        .replace(" !", "!")
        .replace(" ?", "?")
        .replace(" ;", ";")
        .replace(" :", ":");

    let mut output = String::with_capacity(without_space_before.len());
    let mut chars = without_space_before.chars().peekable();

    while let Some(current) = chars.next() {
        output.push(current);

        if matches!(current, '.' | ',' | '!' | '?' | ';' | ':') {
            while matches!(chars.peek(), Some(next) if next.is_whitespace()) {
                let _ = chars.next();
            }

            if matches!(chars.peek(), Some(next) if should_insert_space_after_punctuation(*next)) {
                output.push(' ');
            }
        }
    }

    output.trim().to_owned()
}

fn should_insert_space_after_punctuation(next: char) -> bool {
    !matches!(next, '.' | ',' | '!' | '?' | ';' | ':' | ')' | ']' | '}')
}

#[cfg(test)]
mod tests {
    use super::{PostProcessor, ReplacementRule, cleanup_punctuation, normalize_whitespace};

    #[test]
    fn collapses_whitespace() {
        assert_eq!(normalize_whitespace(" hello\n  world\t"), "hello world");
    }

    #[test]
    fn normalizes_punctuation_spacing() {
        assert_eq!(
            cleanup_punctuation("hello ,world !  Again"),
            "hello, world! Again"
        );
    }

    #[test]
    fn applies_replacements_after_cleanup() {
        let processor = PostProcessor::with_replacements(vec![ReplacementRule::new(
            "parakeet dictation",
            "Canary dictation",
        )]);

        assert_eq!(
            processor.process(" fake  parakeet dictation ,done"),
            "fake Canary dictation, done"
        );
    }
}
