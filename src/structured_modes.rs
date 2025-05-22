//! This module contains the definitions for structured display modes.
//! "Structured" means that the mode is declared using a list of standardized rules, without having to write custom code.
//! Each mode has a list of rules, each rule has a selector and a decision. The selector decides
//! whether a rule matches a particular span, and if it does then the decision specifies how to
//! display the span in this mode.

use crate::types::{value_to_text, DisplayLength, Span};

#[derive(Debug, Clone)]
pub struct StructuredMode {
    pub name: String,
    /// A list of rules that define how to display spans.
    /// For each span, the first rule that matches the span will be used to determine how to display it.
    /// If no rule matches, the span will not be visible.
    pub span_rules: Vec<SpanRule>,
    /// Defines which nodes will be displayed.
    /// If a node doesn't match any condition on this list, its spans will not be visible.
    pub show_nodes: Vec<MatchCondition>,
}

/// A rule that defines how to display a span that matches the selector.
#[derive(Debug, Clone)]
pub struct SpanRule {
    #[allow(unused)]
    name: String,
    /// A span that matches this selector
    selector: SpanSelector,
    /// Will be displayed like this
    decision: SpanDecision,
}

/// A selector used to determine whether a span matches a rule.
#[derive(Debug, Clone)]
pub struct SpanSelector {
    /// Span's name must match this condition
    name_condition: MatchCondition,
    /// Span's attributes must match these conditions.
    /// If the attribute is not present, the span doesn't match the selector.
    attribute_conditions: Vec<(String, MatchCondition)>,
}

/// Defines how to display a span that matches some rule.
#[derive(Debug, Clone)]
pub struct SpanDecision {
    /// Whether the span should be visible or not.
    pub visible: bool,
    /// Should the span's length be defined by time or length of the name text?
    pub display_length: DisplayLength,
    /// If a replacement name is provided, the span's name will be replaced with this name.
    pub replace_name: Option<String>,
    /// Add height (e.g H=123) to the span's name, the height is read from the attributes.
    pub add_height_to_name: bool,
    /// Add shard id (e.g s=123) to the span's name, the shard id is read from the attributes.
    pub add_shard_id_to_name: bool,
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum MatchCondition {
    /// Always matches
    Any,
    /// Never matches
    None,
    /// Matches if the value is equal to the given string
    EqualTo(String),
    /// Matches if the value is not equal to the given string
    NotEqualTo(String),
    /// Matches if the value contains the given substring
    Contains(String),
}

impl SpanSelector {
    pub fn matches(&self, span: &Span) -> bool {
        if !self.name_condition.matches(&span.name) {
            return false;
        }

        for (attr_name, attr_condition) in &self.attribute_conditions {
            if let Some(attr_value) = span.attributes.get(attr_name) {
                if !attr_condition.matches(&value_to_text(attr_value)) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

impl MatchCondition {
    pub fn matches(&self, value: &str) -> bool {
        match self {
            MatchCondition::Any => true,
            MatchCondition::None => false,
            MatchCondition::EqualTo(expected) => value == expected,
            MatchCondition::NotEqualTo(expected) => value != expected,
            MatchCondition::Contains(substring) => value.contains(substring),
        }
    }
}

impl StructuredMode {
    pub fn get_decision_for_span(&self, span: &Span) -> SpanDecision {
        let hide_decision = SpanDecision {
            visible: false,
            display_length: DisplayLength::Time,
            replace_name: None,
            add_height_to_name: false,
            add_shard_id_to_name: false,
        };

        let mut node_matched = false;
        for node_condition in &self.show_nodes {
            if node_condition.matches(&span.node.name) {
                node_matched = true;
                break;
            }
        }
        if !node_matched {
            return hide_decision;
        }

        for rule in &self.span_rules {
            if rule.selector.matches(span) {
                return rule.decision.clone();
            }
        }

        hide_decision
    }
}

/// Chain mode
pub fn chain_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Chain".to_string(),
        span_rules: vec![
            show_span("validate_chunk_state_witness"),
            show_span("apply_new_chunk"),
            show_span("preprocess_optimistic_block"),
            show_span("process_optimistic_block"),
            show_span("postprocess_ready_block"),
            show_span("postprocess_optimistic_block"),
            show_span("preprocess_block"),
            show_span("apply_new_chunk"),
            show_span("apply_old_chunk"),
            show_span("produce_chunk_internal"),
            show_span("produce_block_on"),
            show_span("receive_optimistic_block"),
            show_span("validate_chunk_state_witness"),
            show_span("send_chunk_state_witness"),
            show_span("produce_optimistic_block_on_head"),
            show_span("validate_chunk_endorsement"),
            show_span("on_approval_message"),
        ],
        show_nodes: vec![MatchCondition::Any],
    }
}

/// Everything mode
pub fn everything_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Everything".to_string(),
        span_rules: vec![
            // Show "verify_chunk_endorsement" as "VCE", helps with performance.
            SpanRule {
                name: "Shorter verify_chunk_endorsement".to_string(),
                selector: SpanSelector {
                    name_condition: MatchCondition::EqualTo("verify_chunk_endorsement".to_string()),
                    attribute_conditions: vec![],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Time,
                    replace_name: Some("VCE".to_string()),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                },
            },
            // All spans should be visible, their length should be based on time.
            SpanRule {
                name: "Show all".to_string(),
                selector: SpanSelector {
                    name_condition: MatchCondition::Any,
                    attribute_conditions: vec![],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Time,
                    replace_name: None,
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                },
            },
        ],
        show_nodes: vec![MatchCondition::Any],
    }
}

fn show_span(name: &str) -> SpanRule {
    SpanRule {
        name: format!("Show {}", name),
        selector: SpanSelector {
            name_condition: MatchCondition::EqualTo(name.to_string()),
            attribute_conditions: vec![],
        },
        decision: SpanDecision {
            visible: true,
            display_length: DisplayLength::Text,
            replace_name: None,
            add_height_to_name: true,
            add_shard_id_to_name: true,
        },
    }
}

/// List of all modes
pub fn get_all_structured_modes() -> Vec<StructuredMode> {
    vec![chain_structured_mode(), everything_structured_mode()]
}
