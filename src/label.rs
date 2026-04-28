//! `Label` — taxonomy markers attached to issues.

use serde::{Deserialize, Serialize};

/// One label, identified by `name`. `color` is six hex chars without the
/// leading `#` (GitHub convention); `description` is a one-line tooltip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// Natural key — e.g. `"type:bug"`, `"area:graph"`, `"good first issue"`.
    pub name: String,
    /// Hex colour, e.g. `"d73a4a"` for red. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Label {
    /// Construct a label with just the name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: None,
            description: None,
        }
    }

    /// Builder: set the colour (six hex chars, no `#`).
    #[must_use]
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Builder: set the description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// `true` when `color` is `Some` and looks like 6 hex characters.
    #[must_use]
    pub fn has_valid_color(&self) -> bool {
        self.color
            .as_deref()
            .is_some_and(|c| c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn new_carries_name_only() {
        let l = Label::new("bug");
        assert_eq!(l.name, "bug");
        assert!(l.color.is_none());
        assert!(l.description.is_none());
    }

    #[test]
    fn builders_chain() {
        let l = Label::new("type:bug")
            .with_color("d73a4a")
            .with_description("Something is broken");
        assert_eq!(l.color.as_deref(), Some("d73a4a"));
        assert_eq!(l.description.as_deref(), Some("Something is broken"));
    }

    #[test]
    fn has_valid_color_checks_format() {
        assert!(Label::new("x").with_color("d73a4a").has_valid_color());
        assert!(Label::new("x").with_color("DEADBE").has_valid_color());
        assert!(!Label::new("x").with_color("xyz123").has_valid_color());
        assert!(!Label::new("x").with_color("00").has_valid_color());
        assert!(!Label::new("x").has_valid_color());
    }

    #[test]
    fn serde_omits_empty_optionals() {
        let l = Label::new("bug");
        let json = serde_json::to_string(&l).expect("serialize");
        assert!(!json.contains("color"), "color absent: {json}");
        assert!(!json.contains("description"), "desc absent: {json}");
    }

    #[test]
    fn serde_roundtrip() {
        let l = Label::new("type:bug")
            .with_color("d73a4a")
            .with_description("oops");
        let json = serde_json::to_string(&l).expect("serialize");
        let back: Label = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(l, back);
    }
}
