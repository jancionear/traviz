use crate::colors;
use crate::types::Span;
use crate::types::MILLISECONDS_PER_SECOND;
use eframe::egui::{
    self, Align, Align2, Color32, Context, Grid, Key, Layout, Order, RichText, ScrollArea,
    TextEdit, Ui,
};
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
        if span.original_name == target_name {
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
            .text_color(colors::DARK_GRAY)
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

    pub fn std_dev(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let mean = self.mean();
        let variance: f64 = self
            .data_points
            .iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f64>()
            / self.count as f64;
        variance.sqrt()
    }
}

impl Default for Statistics {
    fn default() -> Self {
        Self::new()
    }
}

/// Processes a slice of spans to collect a deduplicated list of all spans (including children)
/// and a sorted list of unique span names.
///
/// # Arguments
/// * `spans` - A slice of root spans to process.
///
/// # Returns
/// A tuple containing:
///  - `Vec<Rc<Span>>`: All collected spans, including children, deduplicated.
///  - `Vec<String>`: A sorted vector of unique span names.
pub fn process_spans_for_analysis(spans: &[Rc<Span>]) -> (Vec<Rc<Span>>, Vec<String>) {
    let mut all_spans_for_analysis = Vec::new();

    // Collect all spans including children
    for span in spans {
        collect_span_tree_with_deduplication(span, &mut all_spans_for_analysis);
    }

    // Create a set of unique span names
    let mut unique_span_names_set: HashSet<String> = HashSet::new();
    for span in &all_spans_for_analysis {
        unique_span_names_set.insert(span.original_name.clone());
    }

    // Convert to sorted vector
    let mut unique_span_names_vec: Vec<String> = unique_span_names_set.into_iter().collect();
    unique_span_names_vec.sort_by_key(|a| a.to_lowercase());

    (all_spans_for_analysis, unique_span_names_vec)
}

/// Calculates column widths for the analysis result table based on a total grid width and percentage array.
/// Ensures minimum widths for each column.
pub fn calculate_table_column_widths(grid_width: f32, col_percentages: &[f32; 7]) -> [f32; 7] {
    [
        (grid_width * col_percentages[0]).max(140.0), // Node/Source
        (grid_width * col_percentages[1]).max(60.0),  // Count
        (grid_width * col_percentages[2]).max(80.0),  // Min
        (grid_width * col_percentages[3]).max(80.0),  // Max
        (grid_width * col_percentages[4]).max(80.0),  // Mean
        (grid_width * col_percentages[5]).max(80.0),  // Median
        (grid_width * col_percentages[6]).max(80.0),  // Std Dev
    ]
}

/// Helper function to draw a left-aligned text cell, often used for names or labels.
pub fn draw_left_aligned_text_cell(ui: &mut Ui, width: f32, text: &str, is_strong: bool) {
    ui.scope(|cell_ui| {
        cell_ui.set_min_width(width);
        let rich_text = RichText::new(text).monospace();
        if is_strong {
            cell_ui.strong(rich_text);
        } else {
            cell_ui.label(rich_text);
        }
    });
}

/// Helper function to draw a right-aligned text cell, often used for numerical statistics,
/// optionally making it clickable.
pub fn draw_clickable_right_aligned_text_cell(
    ui: &mut Ui,
    width: f32,
    text: &str,
    is_strong: bool,
    color: Option<Color32>,
    is_clickable: bool,
) -> Option<egui::Response> {
    let mut response_opt = None;
    ui.scope(|cell_ui| {
        cell_ui.set_min_width(width);
        cell_ui.with_layout(Layout::right_to_left(Align::Center), |inner_ui| {
            let mut rich_text = RichText::new(text).monospace();
            if let Some(c) = color {
                rich_text = rich_text.color(c);
            }
            if is_strong {
                rich_text = rich_text.strong();
            }

            if is_clickable {
                let label = egui::Label::new(rich_text).sense(egui::Sense::click());
                let response = inner_ui.add(label);
                if response.hovered() {
                    inner_ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                }
                response_opt = Some(response);
            } else if is_strong {
                inner_ui.strong(rich_text);
            } else {
                inner_ui.label(rich_text);
            }
        });
    });
    response_opt
}

/// Show details of a specific span in a new window.
/// Returns true if the window was closed.
pub fn show_span_details(ctx: &Context, span: &Rc<Span>, max_width: f32, max_height: f32) -> bool {
    let mut should_close = false;

    // Create a modal dialog for the span details
    let mut open = true;
    egui::Window::new("Span Details")
        .fixed_size([max_width * 0.8, max_height * 0.6])
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .order(Order::Foreground)
        .show(ctx, |ui| {
            ui.vertical_centered_justified(|ui| {
                ui.heading(&span.name);
                ui.add_space(10.0);

                // Display timing information
                ui.strong(format!(
                    "Duration: {:.3} ms",
                    (span.end_time - span.start_time) * MILLISECONDS_PER_SECOND
                ));
                ui.label(format!(
                    "Time: {} - {}",
                    crate::types::time_point_to_utc_string(span.start_time),
                    crate::types::time_point_to_utc_string(span.end_time)
                ));

                // Display span identification
                ui.add_space(5.0);
                ui.label(format!("Node: {}", span.node.name));
                ui.label(format!("Span ID: {}", hex::encode(&span.span_id)));
                ui.label(format!(
                    "Parent Span ID: {}",
                    hex::encode(&span.parent_span_id)
                ));

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                // Display attributes
                ui.heading("Attributes");
                if span.attributes.is_empty() {
                    ui.label("No attributes");
                } else {
                    Grid::new("span_details_attributes")
                        .num_columns(2)
                        .spacing([10.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (name, value) in &span.attributes {
                                ui.strong(name);
                                ui.label(crate::types::value_to_text(value));
                                ui.end_row();
                            }
                        });
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                // Display events
                ui.heading("Events");
                ScrollArea::vertical()
                    .max_height(max_height * 0.3)
                    .show(ui, |ui| {
                        if span.events.is_empty() {
                            ui.label("No events");
                        } else {
                            for event in &span.events {
                                ui.collapsing(event.name.clone(), |ui| {
                                    ui.label(format!(
                                        "Time: {}",
                                        crate::types::time_point_to_utc_string(event.time)
                                    ));

                                    for (name, value) in &event.attributes {
                                        ui.label(format!(
                                            "{}: {}",
                                            name,
                                            crate::types::value_to_text(value)
                                        ));
                                    }
                                });
                            }
                        }
                    });

                ui.add_space(10.0);
                if ui.button("Close").clicked() {
                    should_close = true;
                }
            });
        });

    // Check for ESC key to close the span details window
    ctx.input(|i| {
        if i.key_pressed(Key::Escape) {
            should_close = true;
        }
    });

    !open || should_close
}
