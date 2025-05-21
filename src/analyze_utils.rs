use crate::types::Span;
use eframe::egui::{self, Color32, ScrollArea, TextEdit};
use std::collections::HashSet;
use std::rc::Rc;

/// Helper function to collect all spans in a span tree with deduplication (the same span won't appear twice).
pub fn collect_span_tree_with_deduplication(
    root_span: &Rc<Span>,
    collected_spans: &mut Vec<Rc<Span>>,
) {
    let mut seen_span_ids: HashSet<Vec<u8>> = HashSet::new();
    collect_descendant_spans_with_deduplication(root_span, collected_spans, &mut seen_span_ids);
}

fn collect_descendant_spans_with_deduplication(
    current_span: &Rc<Span>,
    collected_spans: &mut Vec<Rc<Span>>,
    seen_span_ids: &mut HashSet<Vec<u8>>,
) {
    // Only add this span if we haven't seen its ID before
    if !seen_span_ids.contains(&current_span.span_id) {
        seen_span_ids.insert(current_span.span_id.clone());
        collected_spans.push(current_span.clone());
        // Recursively add all children
        for child_span in current_span.children.borrow().iter() {
            collect_descendant_spans_with_deduplication(child_span, collected_spans, seen_span_ids);
        }
    }
}

/// Helper function to collect spans with specific name.
pub fn collect_matching_spans(
    spans: &[Rc<Span>],
    target_name: &str,
    matching_spans: &mut Vec<Rc<Span>>,
) {
    for span in spans {
        if span.name == target_name {
            matching_spans.push(span.clone());
        }
    }
}

/// Creates a search input field with a label and hint text.
pub fn span_search_ui(
    ui: &mut egui::Ui,
    search_text: &mut String,
    label: &str,
    hint_text: &str,
    width: f32,
) {
    ui.vertical(|ui| {
        ui.label(label);
        let text_edit = TextEdit::singleline(search_text)
            .hint_text(hint_text)
            .text_color(Color32::DARK_GRAY)
            .desired_width(width);
        ui.add(text_edit);
    });
}

/// Creates a scrollable list of selectable span names with search filtering.
///
/// Returns true if the user selected a different span name in this frame.
pub fn span_selection_list_ui(
    ui: &mut egui::Ui,
    unique_span_names: &[String],
    search_text: &str,
    selected_span_name: &mut Option<String>,
    height: f32,
    id_salt: &str,
) -> bool {
    let mut selection_changed = false;

    // Filter names by the search text
    let search_term = search_text.to_lowercase();
    let filtered_names: Vec<&String> = unique_span_names
        .iter()
        .filter(|name| search_term.is_empty() || name.to_lowercase().contains(&search_term))
        .collect();

    // Label with count
    ui.label(format!("Spans ({}):", filtered_names.len()));

    // Scrollable list of spans
    ScrollArea::vertical()
        .max_height(height)
        .id_salt(id_salt)
        .show_viewport(ui, |ui, _viewport| {
            for name in &filtered_names {
                let is_selected = selected_span_name.as_ref() == Some(name);
                let response = ui.selectable_label(is_selected, *name);

                if response.clicked() {
                    *selected_span_name = Some((*name).clone());
                    selection_changed = true;
                }
            }
        });

    selection_changed
}

/// Stores and calculates statistics for a collection of values.
pub struct Statistics {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub total: f64,
    pub data_points: Vec<f64>,
}

impl Statistics {
    pub fn new() -> Self {
        Self {
            count: 0,
            min: f64::MAX,
            max: f64::MIN,
            total: 0.0,
            data_points: Vec::new(),
        }
    }

    pub fn add_value(&mut self, value: f64) {
        self.count += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.total += value;
        self.data_points.push(value);
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.total / self.count as f64
    }

    pub fn median(&self) -> f64 {
        if self.data_points.is_empty() {
            return 0.0;
        }

        let mut sorted_values = self.data_points.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mid = sorted_values.len() / 2;
        if sorted_values.len() % 2 == 0 {
            (sorted_values[mid - 1] + sorted_values[mid]) / 2.0
        } else {
            sorted_values[mid]
        }
    }
}
