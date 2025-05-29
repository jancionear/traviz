//! This module contains the definitions for structured display modes.
//! "Structured" means that the mode is declared using a list of standardized rules, without having to write custom code.
//! Each mode has a list of rules, each rule has a selector and a decision. The selector decides
//! whether a rule matches a particular span, and if it does then the decision specifies how to
//! display the span in this mode.

use crate::types::{value_to_text, DisplayLength, Span};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StructuredMode {
    pub name: String,
    /// A list of rules that define how to display spans.
    /// For each span, the first rule that matches the span will be used to determine how to display it.
    /// If no rule matches, the span will not be visible.
    pub span_rules: Vec<SpanRule>,
    /// Built-in modes (chain, everything, etc.) are not editable and are not saved in persistent data.
    pub is_builtin: bool,
}

/// A rule that defines how to display a span that matches the selector.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanRule {
    pub name: String,
    /// A span that matches this selector
    pub selector: SpanSelector,
    /// Will be displayed like this
    pub decision: SpanDecision,
}

/// A selector used to determine whether a span matches a rule.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanSelector {
    /// Span's name must match this condition
    pub span_name_condition: MatchCondition,
    /// Node's name must match this condition
    pub node_name_condition: MatchCondition,
    /// Span's attributes must match these conditions.
    /// If the attribute is not present, the span doesn't match the selector.
    pub attribute_conditions: Vec<(String, MatchCondition)>,
}

/// Defines how to display a span that matches some rule.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpanDecision {
    /// Whether the span should be visible or not.
    pub visible: bool,
    /// Should the span's length be defined by time or length of the name text?
    pub display_length: DisplayLength,
    /// If a replacement name is provided, the span's name will be replaced with this name.
    pub replace_name: String,
    /// Add height (e.g H=123) to the span's name, the height is read from the attributes.
    pub add_height_to_name: bool,
    /// Add shard id (e.g s=123) to the span's name, the shard id is read from the attributes.
    pub add_shard_id_to_name: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MatchCondition {
    pub operator: MatchOperator,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MatchOperator {
    /// Always matches
    Any,
    /// Never matches
    None,
    /// Matches if the value is equal to the given string
    EqualTo,
    /// Matches if the value is not equal to the given string
    NotEqualTo,
    /// Matches if the value contains the given substring
    Contains,
}

impl SpanSelector {
    pub fn matches(&self, span: &Span) -> bool {
        if !self.span_name_condition.matches(&span.name) {
            return false;
        }

        if !self.node_name_condition.matches(&span.node.name) {
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
    pub fn any() -> MatchCondition {
        MatchCondition {
            operator: MatchOperator::Any,
            value: String::new(),
        }
    }

    pub fn matches(&self, value: &str) -> bool {
        match self.operator {
            MatchOperator::Any => true,
            MatchOperator::None => false,
            MatchOperator::EqualTo => value == self.value,
            MatchOperator::NotEqualTo => value != self.value,
            MatchOperator::Contains => value.contains(self.value.as_str()),
        }
    }
}

impl StructuredMode {
    pub fn get_decision_for_span(&self, span: &Span) -> SpanDecision {
        for rule in &self.span_rules {
            if rule.selector.matches(span) {
                return rule.decision.clone();
            }
        }

        SpanDecision {
            visible: false,
            display_length: DisplayLength::Time,
            replace_name: String::new(),
            add_height_to_name: false,
            add_shard_id_to_name: false,
        }
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
            show_span("send_chunk_endorsement"),
        ],
        is_builtin: true,
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
                    span_name_condition: MatchCondition {
                        operator: MatchOperator::EqualTo,
                        value: "verify_chunk_endorsement".to_string(),
                    },
                    node_name_condition: MatchCondition::any(),
                    attribute_conditions: vec![],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Time,
                    replace_name: "VCE".to_string(),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                },
            },
            // All spans should be visible, their length should be based on time.
            SpanRule {
                name: "Show all".to_string(),
                selector: SpanSelector {
                    span_name_condition: MatchCondition::any(),
                    node_name_condition: MatchCondition::any(),
                    attribute_conditions: vec![],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Time,
                    replace_name: String::new(),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                },
            },
        ],
        is_builtin: true,
    }
}

/// Block Production mode
pub fn block_production_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Block Production".to_string(),
        span_rules: vec![
            // Show "validate_chunk_state_witness" as "VCSW", helps with performance and visual clutter.
            SpanRule {
                name: "Shorter validate_chunk_state_witness".to_string(),
                selector: SpanSelector {
                    span_name_condition: MatchCondition {
                        operator: MatchOperator::EqualTo,
                        value: "validate_chunk_state_witness".to_string(),
                    },
                    node_name_condition: MatchCondition::any(),
                    attribute_conditions: vec![],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Text,
                    replace_name: "VCSW".to_string(),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                },
            },
            // Show "validate_chunk_endorsement" as "VCE", helps with performance and visual clutter.
            SpanRule {
                name: "Shorter validate_chunk_endorsement".to_string(),
                selector: SpanSelector {
                    span_name_condition: MatchCondition {
                        operator: MatchOperator::EqualTo,
                        value: "validate_chunk_endorsement".to_string(),
                    },
                    node_name_condition: MatchCondition::any(),
                    attribute_conditions: vec![],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Text,
                    replace_name: "VCE".to_string(),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                },
            },
            // All spans with 'block_production' tag should be visible.
            SpanRule {
                name: "Show block_production spans".to_string(),
                selector: SpanSelector {
                    span_name_condition: MatchCondition::any(),
                    node_name_condition: MatchCondition::any(),
                    attribute_conditions: vec![(
                        "tag_block_production".to_string(),
                        MatchCondition {
                            operator: MatchOperator::EqualTo,
                            value: "true".to_string(),
                        },
                    )],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Text,
                    replace_name: String::new(),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                },
            },
        ],
        is_builtin: true,
    }
}

fn show_span(name: &str) -> SpanRule {
    SpanRule {
        name: format!("Show {}", name),
        selector: SpanSelector {
            span_name_condition: MatchCondition {
                operator: MatchOperator::EqualTo,
                value: name.to_string(),
            },
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![],
        },
        decision: SpanDecision {
            visible: true,
            display_length: DisplayLength::Text,
            replace_name: String::new(),
            add_height_to_name: true,
            add_shard_id_to_name: true,
        },
    }
}

/// List of all modes
pub fn builtin_structured_modes() -> Vec<StructuredMode> {
    vec![
        chain_structured_mode(),
        everything_structured_mode(),
        block_production_structured_mode(),
    ]
}
