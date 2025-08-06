use eframe::egui::{self, Button, ComboBox, Modal, ScrollArea, Ui, Vec2, Widget};

use crate::colors;
use crate::structured_modes::{
    MatchCondition, MatchOperator, SpanDecision, SpanRule, SpanSelector, StructuredMode,
};
use crate::types::DisplayLength;

pub const HIGHLIGHT_COLOR: egui::Color32 = colors::DARK_BLUE;

pub struct EditDisplayModes {
    state: EditDisplayModesState,
    editing_or_adding_mode: AddingOrEditing,
    editing_or_adding_rule: AddingOrEditing,

    all_modes: Vec<StructuredMode>,
    selected_mode_idx: usize,
    current_mode: StructuredMode,
    selected_span_rule_idx: usize,
    current_span_rule: SpanRule,
    max_width: f32,
    not_editable_message: String,
    max_scrollarea_size: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditDisplayModesState {
    Closed,
    Opened,
    DeleteModeConfirmation,
    NotEditableError,
    EditingMode,
    EditingSpanRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddingOrEditing {
    Adding,
    Editing,
}

impl EditDisplayModes {
    pub fn new() -> Self {
        EditDisplayModes {
            state: EditDisplayModesState::Closed,
            editing_or_adding_mode: AddingOrEditing::Adding,
            editing_or_adding_rule: AddingOrEditing::Adding,
            all_modes: Vec::new(),
            selected_mode_idx: 0,
            current_mode: Self::new_mode(),
            selected_span_rule_idx: 0,
            current_span_rule: Self::new_span_rule(),
            max_width: 800.0,
            not_editable_message: String::new(),
            max_scrollarea_size: Vec2::new(800.0, 400.0),
        }
    }

    pub fn open(&mut self, modes: Vec<StructuredMode>) {
        self.all_modes = modes;
        self.state = EditDisplayModesState::Opened;
    }

    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        max_width: f32,
        max_height: f32,
    ) -> Option<Vec<StructuredMode>> {
        if self.state == EditDisplayModesState::Closed {
            return None;
        }

        self.max_width = max_width;
        self.max_scrollarea_size = Vec2::new(max_width, max_height - 200.0);
        let mut result = None;
        Modal::new("edit display modes".into()).show(ctx, |ui| {
            ui.set_max_width(max_width);
            ui.set_max_height(max_height);
            match self.state {
                EditDisplayModesState::Closed => unreachable!(),
                EditDisplayModesState::Opened => result = self.draw_opened(ui, ctx),
                EditDisplayModesState::DeleteModeConfirmation => {
                    self.draw_delete_confirmation(ui, ctx)
                }
                EditDisplayModesState::NotEditableError => self.draw_not_editable_error(ui, ctx),
                EditDisplayModesState::EditingMode => self.draw_editing_mode(ui, ctx),
                EditDisplayModesState::EditingSpanRule => self.draw_editing_span_rule(ui, ctx),
            }
        });

        result
    }

    fn new_mode() -> StructuredMode {
        StructuredMode {
            name: "New Mode".to_string(),
            span_rules: vec![Self::new_span_rule()],
            is_builtin: false,
        }
    }

    fn new_span_rule() -> SpanRule {
        SpanRule {
            name: "Rule 1".to_string(),
            selector: SpanSelector {
                span_name_condition: MatchCondition {
                    operator: MatchOperator::EqualTo,
                    value: "MySpan".to_string(),
                },
                node_name_condition: MatchCondition::any(),
                attribute_conditions: Vec::new(),
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

    fn draw_short_separator(&self, ui: &mut Ui) {
        ui.set_max_width(10.0);
        ui.separator();
        ui.set_max_width(self.max_width);
    }

    fn draw_opened(&mut self, ui: &mut Ui, _ctx: &egui::Context) -> Option<Vec<StructuredMode>> {
        ui.label("Edit Display Modes");

        self.draw_short_separator(ui);

        ui.label("Modes");
        ui.allocate_ui(self.max_scrollarea_size, |ui| {
            ScrollArea::vertical()
                .id_salt("display modes")
                .show(ui, |ui| {
                    for (index, mode) in self.all_modes.iter().enumerate() {
                        let mode_name = if mode.is_builtin {
                            format!("{} (builtin)", mode.name)
                        } else {
                            mode.name.clone()
                        };

                        let button = if self.selected_mode_idx == index {
                            Button::new(mode_name).fill(HIGHLIGHT_COLOR)
                        } else {
                            Button::new(mode_name)
                        };
                        if button.ui(ui).clicked() {
                            self.selected_mode_idx = index;
                        }
                    }
                });
        });

        self.draw_short_separator(ui);

        ui.label("Actions");
        ui.horizontal(|ui| {
            if ui.button("New Mode").clicked() {
                let new_mode = Self::new_mode();
                self.current_mode = new_mode.clone();
                self.selected_span_rule_idx = 0;
                self.editing_or_adding_mode = AddingOrEditing::Adding;
                self.state = EditDisplayModesState::EditingMode;
            }
            if ui.button("Edit Mode").clicked() {
                if let Some(mode) = self.all_modes.get(self.selected_mode_idx) {
                    if mode.is_builtin {
                        self.not_editable_message =
                        "This mode is not editable! Builtin modes that are provided in traviz cannot be changed from the UI. \
                        You can clone this mode to create your own custom one and then edit the custom mode".to_string();
                        self.state = EditDisplayModesState::NotEditableError;
                    } else {
                        self.current_mode = mode.clone();
                        self.selected_span_rule_idx = 0;
                        self.editing_or_adding_mode = AddingOrEditing::Editing;
                        self.state = EditDisplayModesState::EditingMode;
                    }
                }
            }
            if ui.button("Clone Mode").clicked() {
                let mut new_mode = self.all_modes[self.selected_mode_idx].clone();
                new_mode.name = format!("{} Clone", new_mode.name);
                new_mode.is_builtin = false;
                self.all_modes.push(new_mode);
                self.selected_mode_idx = self.all_modes.len() - 1;
            }
            if ui.button("Delete Mode").clicked() {
                if let Some(mode) = self.all_modes.get(self.selected_mode_idx) {
                    if mode.is_builtin {
                        self.not_editable_message = "Builtin modes can not be deleted".to_string();
                        self.state = EditDisplayModesState::NotEditableError;
                    } else {
                        self.state = EditDisplayModesState::DeleteModeConfirmation;
                    }
                }
            }
        });

        let mut result = None;

        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.state = EditDisplayModesState::Closed;
                result = Some(std::mem::take(&mut self.all_modes));
            }
            if ui.button("Cancel").clicked() {
                self.state = EditDisplayModesState::Closed;
            }
        });

        result
    }

    fn draw_delete_confirmation(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Are you sure you want to delete this mode?");
        self.draw_short_separator(ui);

        if let Some(mode) = self.all_modes.get(self.selected_mode_idx) {
            ui.label(format!("Mode Name: {}", mode.name));
        }

        self.draw_short_separator(ui);
        if ui.button("Yes, Delete").clicked() {
            self.all_modes.remove(self.selected_mode_idx);
            self.selected_mode_idx = 0;
            self.state = EditDisplayModesState::Opened;
        }
        if ui.button("No, Cancel").clicked() {
            self.state = EditDisplayModesState::Opened;
        }
    }

    fn draw_not_editable_error(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label(&self.not_editable_message);
        self.draw_short_separator(ui);
        if ui.button("Ok").clicked() {
            self.state = EditDisplayModesState::Opened;
        }
    }

    fn draw_editing_mode(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Editing Mode");
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            ui.label("Mode Name:");
            ui.text_edit_singleline(&mut self.current_mode.name);
        });
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Span rules");
                ui.allocate_ui(self.max_scrollarea_size, |ui| {
                    ScrollArea::vertical().id_salt("span rules").show(ui, |ui| {
                        for (index, rule) in self.current_mode.span_rules.iter().enumerate() {
                            let button = if self.selected_span_rule_idx == index {
                                Button::new(rule.name.to_string()).fill(HIGHLIGHT_COLOR)
                            } else {
                                Button::new(rule.name.to_string())
                            };
                            if button.ui(ui).clicked() {
                                self.selected_span_rule_idx = index;
                            }
                        }
                        if self.current_mode.span_rules.is_empty() {
                            ui.label("<empty>");
                        }
                    });
                });
            });
        });
        self.draw_short_separator(ui);
        ui.label("Actions");
        ui.horizontal(|ui| {
            if ui.button("New rule").clicked() {
                self.current_span_rule = Self::new_span_rule();
                self.state = EditDisplayModesState::EditingSpanRule;
                self.editing_or_adding_rule = AddingOrEditing::Adding;
            };
            if ui.button("Edit rule").clicked() {
                if let Some(span_rule) = self
                    .current_mode
                    .span_rules
                    .get(self.selected_span_rule_idx)
                {
                    self.current_span_rule = span_rule.clone();
                    self.state = EditDisplayModesState::EditingSpanRule;
                    self.editing_or_adding_rule = AddingOrEditing::Editing;
                }
            }
            if ui.button("Delete rule").clicked()
                && self.selected_span_rule_idx < self.current_mode.span_rules.len()
            {
                self.current_mode
                    .span_rules
                    .remove(self.selected_span_rule_idx);
                if self.selected_span_rule_idx >= self.current_mode.span_rules.len() {
                    if self.current_mode.span_rules.is_empty() {
                        self.selected_span_rule_idx = 0;
                    } else {
                        self.selected_span_rule_idx = self.current_mode.span_rules.len() - 1;
                    }
                }
            }
            if ui.button("Clone rule").clicked()
                && self.selected_span_rule_idx < self.current_mode.span_rules.len()
            {
                let mut new_rule =
                    self.current_mode.span_rules[self.selected_span_rule_idx].clone();
                new_rule.name = format!("{} Clone", new_rule.name);
                self.current_mode.span_rules.push(new_rule);
                self.selected_span_rule_idx = self.current_mode.span_rules.len() - 1;
            }
            if ui.button("Move up").clicked() && self.selected_span_rule_idx > 0 {
                self.current_mode
                    .span_rules
                    .swap(self.selected_span_rule_idx, self.selected_span_rule_idx - 1);
                self.selected_span_rule_idx -= 1;
            }
            if ui.button("Move down").clicked()
                && self.selected_span_rule_idx + 1 < self.current_mode.span_rules.len()
            {
                self.current_mode
                    .span_rules
                    .swap(self.selected_span_rule_idx, self.selected_span_rule_idx + 1);
                self.selected_span_rule_idx += 1;
            }
        });
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("Ok").clicked() {
                match self.editing_or_adding_mode {
                    AddingOrEditing::Adding => {
                        self.all_modes.push(self.current_mode.clone());
                        self.selected_mode_idx = self.all_modes.len() - 1;
                        self.state = EditDisplayModesState::Opened;
                    }
                    AddingOrEditing::Editing => {
                        *self.all_modes.get_mut(self.selected_mode_idx).unwrap() =
                            self.current_mode.clone();
                        self.state = EditDisplayModesState::Opened;
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                self.state = EditDisplayModesState::Opened;
            };
        });
    }

    fn draw_editing_span_rule(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Editing Span Rule");
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            ui.label("Rule Name:");
            ui.text_edit_singleline(&mut self.current_span_rule.name);
        });
        self.draw_short_separator(ui);
        Self::draw_edit_span_selector(
            &mut self.current_span_rule.selector,
            ui,
            self.max_width,
            "span rule selector",
        );
        self.draw_short_separator(ui);
        ui.label("Decision");
        ui.horizontal(|ui| {
            ui.label("Visibility:");
            ComboBox::new("span visible or hidden", "")
                .selected_text(if self.current_span_rule.decision.visible {
                    "Show"
                } else {
                    "Hide"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.current_span_rule.decision.visible, true, "Show");
                    ui.selectable_value(
                        &mut self.current_span_rule.decision.visible,
                        false,
                        "Hide",
                    );
                });
        });
        ui.horizontal(|ui| {
            ui.label("Display length:");
            ComboBox::new("display length", "")
                .selected_text(format!(
                    "{:?}",
                    self.current_span_rule.decision.display_length
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.current_span_rule.decision.display_length,
                        DisplayLength::Text,
                        "Text",
                    );
                    ui.selectable_value(
                        &mut self.current_span_rule.decision.display_length,
                        DisplayLength::Time,
                        "Time",
                    );
                });
        });
        ui.horizontal(|ui| {
            ui.label("Replace span name with:");
            ui.text_edit_singleline(&mut self.current_span_rule.decision.replace_name);
        });
        ui.checkbox(
            &mut self.current_span_rule.decision.add_height_to_name,
            "Add Height to Name",
        );
        ui.checkbox(
            &mut self.current_span_rule.decision.add_shard_id_to_name,
            "Add Shard ID to Name",
        );
        ui.checkbox(
            &mut self.current_span_rule.decision.group,
            "Group spans with same Name and Height",
        );
        self.draw_short_separator(ui);

        ui.horizontal(|ui| {
            if ui.button("Ok").clicked() {
                match self.editing_or_adding_rule {
                    AddingOrEditing::Adding => {
                        self.current_mode
                            .span_rules
                            .push(self.current_span_rule.clone());
                        self.selected_span_rule_idx = self.current_mode.span_rules.len() - 1;
                        self.state = EditDisplayModesState::EditingMode;
                    }
                    AddingOrEditing::Editing => {
                        self.current_mode.span_rules[self.selected_span_rule_idx] =
                            self.current_span_rule.clone();
                        self.state = EditDisplayModesState::EditingMode;
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                self.state = EditDisplayModesState::EditingMode;
            }
        });
    }

    pub fn draw_edit_match_condition(ui: &mut Ui, condition: &mut MatchCondition, id_salt: &str) {
        ComboBox::new(id_salt, "")
            .selected_text(format!("{:?}", condition.operator))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut condition.operator, MatchOperator::Any, "Any");
                ui.selectable_value(&mut condition.operator, MatchOperator::None, "None");
                ui.selectable_value(&mut condition.operator, MatchOperator::EqualTo, "Equal To");
                ui.selectable_value(
                    &mut condition.operator,
                    MatchOperator::NotEqualTo,
                    "Not Equal To",
                );
                ui.selectable_value(&mut condition.operator, MatchOperator::Contains, "Contains");
            });
        match condition.operator {
            MatchOperator::Any | MatchOperator::None => {}
            MatchOperator::EqualTo | MatchOperator::NotEqualTo | MatchOperator::Contains => {
                ui.horizontal(|ui| {
                    ui.label("Value:");
                    ui.text_edit_singleline(&mut condition.value);
                });
            }
        }
    }

    pub fn draw_edit_span_selector(
        selector: &mut SpanSelector,
        ui: &mut Ui,
        max_width: f32,
        ui_seed: &str,
    ) {
        let draw_short_separator = |ui: &mut Ui| {
            ui.set_max_width(10.0);
            ui.separator();
            ui.set_max_width(max_width);
        };

        ui.label("Span name condition");
        Self::draw_edit_match_condition(
            ui,
            &mut selector.span_name_condition,
            &format!("span name condition {ui_seed}"),
        );
        draw_short_separator(ui);
        ui.label("Node name condition");
        Self::draw_edit_match_condition(
            ui,
            &mut selector.node_name_condition,
            &format!("node name condition {ui_seed}"),
        );
        draw_short_separator(ui);
        ui.label("Attribute Conditions");
        let mut attribute_condition_to_remove = None;
        for (i, attr_condition) in &mut selector.attribute_conditions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label("Attribute Name:");
                ui.text_edit_singleline(&mut attr_condition.0);
                Self::draw_edit_match_condition(
                    ui,
                    &mut attr_condition.1,
                    format!("attribute condition {} {}", ui_seed, i).as_str(),
                );
                if ui.button("Remove").clicked() {
                    attribute_condition_to_remove = Some(i);
                }
            });
        }
        if let Some(idx) = attribute_condition_to_remove {
            selector.attribute_conditions.remove(idx);
        }
        if ui.button("New Attribute Condition").clicked() {
            selector.attribute_conditions.push((
                "<attribute name>".to_string(),
                MatchCondition {
                    operator: MatchOperator::EqualTo,
                    value: "val".to_string(),
                },
            ));
        }
    }
}

impl Default for EditDisplayModes {
    fn default() -> Self {
        EditDisplayModes::new()
    }
}
