//! Model-pack manifest primitives.

/// Minimal model-pack identity used before manifest parsing is implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPackRef {
    /// Stable model-pack identifier.
    pub id: String,
}

impl ModelPackRef {
    /// Create a model-pack reference.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
