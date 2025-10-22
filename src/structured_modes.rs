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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Whether to group spans with the same name into a single aggregated span.
    #[serde(default)]
    pub group: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        if !self.span_name_condition.matches(span.original_name()) {
            return false;
        }

        if !self.node_name_condition.matches(&span.node.name) {
            return false;
        }

        for (attr_name, attr_condition) in &self.attribute_conditions {
            if attr_name.is_empty() || attr_name == "<attribute name>" {
                // Ignore empty attribute conditions that have an empty or default name.
                // They were probably added accidentally, it doesn't make much sense to enforce them.
                continue;
            }

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

    pub fn new_equal_name(name: &str) -> SpanSelector {
        SpanSelector {
            span_name_condition: MatchCondition {
                operator: MatchOperator::EqualTo,
                value: name.to_string(),
            },
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![],
        }
    }

    pub fn new_name_contains(name: &str) -> SpanSelector {
        SpanSelector {
            span_name_condition: MatchCondition {
                operator: MatchOperator::Contains,
                value: name.to_string(),
            },
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![],
        }
    }
}

impl MatchCondition {
    pub fn any() -> MatchCondition {
        MatchCondition {
            operator: MatchOperator::Any,
            value: String::new(),
        }
    }

    pub fn equal_to(value: &str) -> MatchCondition {
        MatchCondition {
            operator: MatchOperator::EqualTo,
            value: value.to_string(),
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
            group: false,
        }
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
                    group: false,
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
                    group: false,
                },
            },
        ],
        is_builtin: true,
    }
}

/// tag_block_production
pub fn block_production_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "tag_block_production".to_string(),
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
                    group: false,
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
                    group: false,
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
                    group: false,
                },
            },
        ],
        is_builtin: true,
    }
}

/// tag_block_production without spam
fn block_production_no_spam() -> StructuredMode {
    let mut mode = block_production_structured_mode();
    mode.name = "tag_block_production less spammy".to_string();

    mode.span_rules = [
        hide_span("validate_chunk_endorsement"),
        hide_span("on_chunk_completed"),
        hide_span("send_chunk_endorsement"),
    ]
    .into_iter()
    .chain(mode.span_rules)
    .collect();

    mode
}

fn witness_distribution_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Witness Distribution".to_string(),
        span_rules: vec![SpanRule {
            name: "Show witness distribution spans".to_string(),
            selector: SpanSelector {
                span_name_condition: MatchCondition::any(),
                node_name_condition: MatchCondition::any(),
                attribute_conditions: vec![(
                    "tag_witness_distribution".to_string(),
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
                group: false,
            },
        }],
        is_builtin: true,
    }
}

fn witness_distribution_shard_0_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Witness distribution (shard 0)".to_string(),
        span_rules: vec![SpanRule {
            name: "Show witness distribution spans from shard0".to_string(),
            selector: SpanSelector {
                span_name_condition: MatchCondition::any(),
                node_name_condition: MatchCondition::any(),
                attribute_conditions: vec![
                    (
                        "tag_witness_distribution".to_string(),
                        MatchCondition {
                            operator: MatchOperator::EqualTo,
                            value: "true".to_string(),
                        },
                    ),
                    (
                        "shard_id".to_string(),
                        MatchCondition {
                            operator: MatchOperator::EqualTo,
                            value: "0".to_string(),
                        },
                    ),
                ],
            },
            decision: SpanDecision {
                visible: true,
                display_length: DisplayLength::Text,
                replace_name: String::new(),
                add_height_to_name: true,
                add_shard_id_to_name: true,
                group: false,
            },
        }],
        is_builtin: true,
    }
}

fn chunk_distribution_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Chunk distribution".to_string(),
        span_rules: vec![SpanRule {
            name: "Show chunk distribution spans".to_string(),
            selector: SpanSelector {
                span_name_condition: MatchCondition::any(),
                node_name_condition: MatchCondition::any(),
                attribute_conditions: vec![(
                    "tag_chunk_distribution".to_string(),
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
                group: false,
            },
        }],
        is_builtin: true,
    }
}

fn chunk_distribution_shard_0_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Chunk distribution (shard 0)".to_string(),
        span_rules: vec![SpanRule {
            name: "Show chunk distribution spans from shard 0".to_string(),
            selector: SpanSelector {
                span_name_condition: MatchCondition::any(),
                node_name_condition: MatchCondition::any(),
                attribute_conditions: vec![
                    (
                        "tag_chunk_distribution".to_string(),
                        MatchCondition {
                            operator: MatchOperator::EqualTo,
                            value: "true".to_string(),
                        },
                    ),
                    (
                        "shard_id".to_string(),
                        MatchCondition {
                            operator: MatchOperator::EqualTo,
                            value: "0".to_string(),
                        },
                    ),
                ],
            },
            decision: SpanDecision {
                visible: true,
                display_length: DisplayLength::Text,
                replace_name: String::new(),
                add_height_to_name: true,
                add_shard_id_to_name: true,
                group: false,
            },
        }],
        is_builtin: true,
    }
}

fn critical_path_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Critical Path".to_string(),
        span_rules: vec![
            show_span("send_witness_to_client"),
            show_span("produce_chunk"),
            // show_span("produce_chunks"),
            // show_span("persist_and_distribute_encoded_chunk"),
            // show_span_grouped("chunk_completed")
            // show_span_grouped("process_optimistic_block")
            // show_span("produce_optimistic_block_on_head")
            // show_span("receive_witness")
            show_span("produce_chunk_internal"),
            show_span("produce_block_on_head"),
            show_span_grouped("validate_chunk_state_witness"),
            show_span("start_process_block_async"),
            show_span("do_apply_chunks"),
            //show_span("apply_new_chunk"),
            show_span("send_chunk_state_witness"),
            show_span_grouped("decode_state_witness"),
            show_span_grouped("validate_chunk_endorsement"),
            show_span("generate_state_witness_parts"),
            show_span("postprocess_ready_block"),
            show_span("distribute_chunk_state_witness"),
            show_span("send partial_encoded_state_witnesses"),
            show_span_grouped("decode_witness_parts"),
        ],
        is_builtin: true,
    }
}

fn min_critical_path_structured_mode() -> StructuredMode {
    StructuredMode {
        name: "Smaller critical path".to_string(),
        span_rules: vec![
            show_span("produce_block_on_head"),
            show_span_grouped("validate_chunk_state_witness"),
            show_span("start_process_block_async"),
            show_span("postprocess_ready_block"),
            show_span("produce_chunks"),
            show_span_grouped("chunk_completed"),
            SpanRule {
                name: "apply_new_chunk normal".to_string(),
                selector: SpanSelector {
                    span_name_condition: MatchCondition {
                        operator: MatchOperator::EqualTo,
                        value: "apply_new_chunk".to_string(),
                    },
                    node_name_condition: MatchCondition::any(),
                    attribute_conditions: vec![
                        ("block_type".to_string(), MatchCondition::equal_to("Normal")),
                        (
                            "apply_reason".to_string(),
                            MatchCondition::equal_to("UpdateTrackedShard"),
                        ),
                    ],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Text,
                    replace_name: "apply_new_chunk (normal)".to_string(),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                    group: false,
                },
            },
            SpanRule {
                name: "apply_new_chunk opt".to_string(),
                selector: SpanSelector {
                    span_name_condition: MatchCondition {
                        operator: MatchOperator::EqualTo,
                        value: "apply_new_chunk".to_string(),
                    },
                    node_name_condition: MatchCondition::any(),
                    attribute_conditions: vec![
                        (
                            "block_type".to_string(),
                            MatchCondition::equal_to("Optimistic"),
                        ),
                        (
                            "apply_reason".to_string(),
                            MatchCondition::equal_to("UpdateTrackedShard"),
                        ),
                    ],
                },
                decision: SpanDecision {
                    visible: true,
                    display_length: DisplayLength::Text,
                    replace_name: "apply_new_chunk (opt)".to_string(),
                    add_height_to_name: true,
                    add_shard_id_to_name: true,
                    group: false,
                },
            },
        ],
        is_builtin: true,
    }
}

fn show_span(name: &str) -> SpanRule {
    SpanRule {
        name: format!("Show {name}"),
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
            group: false,
        },
    }
}

fn show_span_grouped(name: &str) -> SpanRule {
    SpanRule {
        name: format!("Show {name}"),
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
            group: true,
        },
    }
}

fn hide_span(name: &str) -> SpanRule {
    SpanRule {
        name: format!("Hide {}", name),
        selector: SpanSelector {
            span_name_condition: MatchCondition {
                operator: MatchOperator::EqualTo,
                value: name.to_string(),
            },
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![],
        },
        decision: SpanDecision {
            visible: false,
            display_length: DisplayLength::Time,
            replace_name: String::new(),
            add_height_to_name: false,
            add_shard_id_to_name: false,
            group: false,
        },
    }
}

/// List of all modes
pub fn builtin_structured_modes() -> Vec<StructuredMode> {
    vec![
        everything_structured_mode(),
        critical_path_structured_mode(),
        min_critical_path_structured_mode(),
        block_production_structured_mode(),
        block_production_no_spam(),
        witness_distribution_structured_mode(),
        witness_distribution_shard_0_structured_mode(),
        chunk_distribution_structured_mode(),
        chunk_distribution_shard_0_structured_mode(),
    ]
}
