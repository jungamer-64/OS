use super::*;
use crate::constants::{FEATURES, SERIAL_HINTS};
use crate::vga_buffer::ColorCode;
use std::string::String;
use std::vec::Vec;

struct MockOutput {
    writes: Vec<(String, ColorCode)>,
}

impl MockOutput {
    fn new() -> Self {
        Self { writes: Vec::new() }
    }
}

impl Output for MockOutput {
    fn write(&mut self, text: &str, color: ColorCode) {
        self.writes.push((text.to_string(), color));
    }
}

#[test]
fn feature_list_emits_all_entries() {
    let mut mock = MockOutput::new();
    display_feature_list_with(&mut mock);

    assert!(!mock.writes.is_empty());
    let expected = FEATURES.len() + 2; // header + newline + entries
    assert_eq!(mock.writes.len(), expected);
    assert!(mock
        .writes
        .iter()
        .any(|(text, _)| text.contains("Major Improvements")));
}

#[test]
fn usage_note_prints_hints() {
    let mut mock = MockOutput::new();
    display_usage_note_with(&mut mock);

    for hint in SERIAL_HINTS {
        assert!(
            mock.writes.iter().any(|(text, _)| text.contains(hint)),
            "missing hint: {}",
            hint
        );
    }
}
