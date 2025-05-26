use eframe::egui::{self, Button, Modal, ScrollArea, Ui, Widget};

use crate::edit_modes::{AddingOrEditing, EditDisplayModes, HIGHLIGHT_COLOR};
use crate::structured_modes::{MatchCondition, MatchOperator};

#[derive(Debug, Clone)]
pub struct NodeFilter {
    pub name: String,
    pub rules: Vec<NodeRule>,
    /// Built-in filters (everything, etc.) are not editable.
    pub is_editable: bool,
}

#[derive(Debug, Clone)]
pub struct NodeRule {
    pub name: String,
    pub condition: MatchCondition,
    pub visible: bool,
}

impl NodeFilter {
    pub fn show_all() -> NodeFilter {
        NodeFilter {
            name: "Show all".to_string(),
            rules: vec![NodeRule {
                name: "Show all".to_string(),
                condition: MatchCondition::any(),
                visible: true,
            }],
            is_editable: false,
        }
    }

    pub fn should_show_span(&self, node_name: &str) -> bool {
        for rule in &self.rules {
            if rule.condition.matches(node_name) {
                return rule.visible;
            }
        }
        false
    }
}

#[derive(Debug)]
pub struct EditNodeFilters {
    state: EditNodeFiltersState,
    filters: Vec<NodeFilter>,
    max_width: f32,
    selected_filter_idx: usize,
    selected_rule_idx: usize,
    current_filter: NodeFilter,
    current_rule: NodeRule,
    editing_or_adding_filter: AddingOrEditing,
    editing_or_adding_rule: AddingOrEditing,
    not_editable_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditNodeFiltersState {
    Closed,
    Open,
    CloningFilter,
    DeleteFilterConfirmation,
    NotEditableError,
    EditingFilter,
    EditingFilterRule,
}

impl EditNodeFilters {
    pub fn new() -> EditNodeFilters {
        EditNodeFilters {
            state: EditNodeFiltersState::Closed,
            filters: vec![],
            max_width: 800.0,
            selected_rule_idx: 0,
            selected_filter_idx: 0,
            current_filter: Self::new_filter(),
            current_rule: Self::new_rule(),
            editing_or_adding_filter: AddingOrEditing::Adding,
            editing_or_adding_rule: AddingOrEditing::Adding,
            not_editable_message: String::new(),
        }
    }

    pub fn open(&mut self, filters: Vec<NodeFilter>) {
        self.filters = filters;
        self.state = EditNodeFiltersState::Open;
    }

    pub fn draw(
        &mut self,
        _ui: &mut Ui,
        ctx: &egui::Context,
        max_width: f32,
        max_height: f32,
    ) -> Option<Vec<NodeFilter>> {
        if self.state == EditNodeFiltersState::Closed {
            return None;
        }

        let mut result = None;
        self.max_width = max_width;
        Modal::new("edit node filters".into()).show(ctx, |ui| {
            ui.set_max_width(max_width);
            ui.set_max_height(max_height);
            match self.state {
                EditNodeFiltersState::Closed => unreachable!(),
                EditNodeFiltersState::Open => result = self.draw_open(ui, ctx),
                EditNodeFiltersState::CloningFilter => self.draw_cloning_filter(ui, ctx),
                EditNodeFiltersState::DeleteFilterConfirmation => {
                    self.draw_delete_confirmation(ui, ctx)
                }
                EditNodeFiltersState::NotEditableError => self.draw_not_editable_error(ui, ctx),
                EditNodeFiltersState::EditingFilter => self.draw_edit_filter(ui, ctx),
                EditNodeFiltersState::EditingFilterRule => self.draw_edit_filter_rule(ui, ctx),
            }
        });

        result
    }

    fn draw_open(&mut self, ui: &mut Ui, _ctx: &egui::Context) -> Option<Vec<NodeFilter>> {
        ui.label("Node filters");
        ScrollArea::vertical()
            .id_salt("node filters")
            .show(ui, |ui| {
                for (index, filter) in self.filters.iter().enumerate() {
                    let button = if self.selected_filter_idx == index {
                        Button::new(filter.name.clone()).fill(HIGHLIGHT_COLOR)
                    } else {
                        Button::new(filter.name.clone())
                    };
                    if button.ui(ui).clicked() {
                        self.selected_filter_idx = index;
                    }
                }
            });

        self.draw_short_separator(ui);

        ui.horizontal(|ui| {
            if ui.button("New Filter").clicked() {
                self.current_filter = Self::new_filter();
                self.selected_rule_idx = 0;
                self.editing_or_adding_filter = AddingOrEditing::Adding;
                self.state = EditNodeFiltersState::EditingFilter;
            }
            if ui.button("Edit Filter").clicked() {
                if let Some(filter) = self.filters.get(self.selected_filter_idx) {
                    if filter.is_editable {
                        self.current_filter = filter.clone();
                        self.selected_rule_idx = 0;
                        self.editing_or_adding_filter = AddingOrEditing::Editing;
                        self.state = EditNodeFiltersState::EditingFilter;
                    } else {
                        self.not_editable_message =
                        "This filter is not editable! Builtin filters that are provided in traviz cannot be changed from the UI. \
                        You can clone this filter to create your own custom one and then edit the custom filter".to_string();
                        self.state = EditNodeFiltersState::NotEditableError;
                    }
                }
            }
            if ui.button("Clone Filter").clicked() {
                let mut new_filter = self.filters[self.selected_filter_idx].clone();
                new_filter.name = format!("{} Clone", new_filter.name);
                new_filter.is_editable = true;
                self.state = EditNodeFiltersState::CloningFilter;
                self.current_filter = new_filter;
            }
            if ui.button("Delete Filter").clicked() {
                if let Some(filter) = self.filters.get(self.selected_filter_idx) {
                    if filter.is_editable {
                        self.state = EditNodeFiltersState::DeleteFilterConfirmation;
                    } else {
                        self.not_editable_message = "Builtin filters can not be deleted".to_string();
                        self.state = EditNodeFiltersState::NotEditableError;
                    }
                }
            }
        });

        let mut result = None;

        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.state = EditNodeFiltersState::Closed;
                result = Some(std::mem::take(&mut self.filters));
            }
            if ui.button("Cancel").clicked() {
                self.state = EditNodeFiltersState::Closed;
            }
        });

        result
    }

    fn draw_cloning_filter(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Clone Filter");
        self.draw_short_separator(ui);

        ui.horizontal(|ui| {
            ui.label("Filter Name:");
            ui.text_edit_singleline(&mut self.current_filter.name);
        });

        self.draw_short_separator(ui);
        if ui.button("Clone").clicked() {
            self.filters.push(self.current_filter.clone());
            self.selected_filter_idx = self.filters.len() - 1;
            self.state = EditNodeFiltersState::Open;
        }
        if ui.button("Cancel").clicked() {
            self.state = EditNodeFiltersState::Open;
        }
    }

    fn draw_delete_confirmation(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Are you sure you want to delete this filter?");
        self.draw_short_separator(ui);

        if let Some(filter) = self.filters.get(self.selected_filter_idx) {
            ui.label(format!("Filter Name: {}", filter.name));
        }

        self.draw_short_separator(ui);
        if ui.button("Yes, Delete").clicked() {
            self.filters.remove(self.selected_filter_idx);
            self.selected_filter_idx = 0;
            self.state = EditNodeFiltersState::Open;
        }
        if ui.button("No, Cancel").clicked() {
            self.state = EditNodeFiltersState::Open;
        }
    }

    fn draw_not_editable_error(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label(&self.not_editable_message);
        self.draw_short_separator(ui);
        if ui.button("Ok").clicked() {
            self.state = EditNodeFiltersState::Open;
        }
    }

    fn draw_edit_filter(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Editing Filter");
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            ui.label("Filter Name:");
            ui.text_edit_singleline(&mut self.current_filter.name);
        });
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Filter Rules");
                ScrollArea::vertical()
                    .id_salt("filter rules")
                    .show(ui, |ui| {
                        for (index, rule) in self.current_filter.rules.iter().enumerate() {
                            let button = if self.selected_rule_idx == index {
                                Button::new(rule.name.to_string()).fill(HIGHLIGHT_COLOR)
                            } else {
                                Button::new(rule.name.to_string())
                            };
                            if button.ui(ui).clicked() {
                                self.selected_rule_idx = index;
                            }
                        }
                    });
            });
        });
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("New Rule").clicked() {
                self.current_rule = Self::new_rule();
                self.state = EditNodeFiltersState::EditingFilterRule;
                self.editing_or_adding_rule = AddingOrEditing::Adding;
            };
            if ui.button("Edit").clicked() {
                if let Some(rule) = self.current_filter.rules.get(self.selected_rule_idx) {
                    self.current_rule = rule.clone();
                    self.state = EditNodeFiltersState::EditingFilterRule;
                    self.editing_or_adding_rule = AddingOrEditing::Editing;
                }
            }
            if ui.button("Delete").clicked()
                && self.selected_rule_idx < self.current_filter.rules.len()
            {
                self.current_filter.rules.remove(self.selected_rule_idx);
                if self.selected_rule_idx >= self.current_filter.rules.len() {
                    if self.current_filter.rules.is_empty() {
                        self.selected_rule_idx = 0;
                    } else {
                        self.selected_rule_idx = self.current_filter.rules.len() - 1;
                    }
                }
            }
            if ui.button("Move up").clicked() && self.selected_rule_idx > 0 {
                self.current_filter
                    .rules
                    .swap(self.selected_rule_idx, self.selected_rule_idx - 1);
                self.selected_rule_idx -= 1;
            }
            if ui.button("Move down").clicked()
                && self.selected_rule_idx + 1 < self.current_filter.rules.len()
            {
                self.current_filter
                    .rules
                    .swap(self.selected_rule_idx, self.selected_rule_idx + 1);
                self.selected_rule_idx += 1;
            }
        });
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("Ok").clicked() {
                match self.editing_or_adding_filter {
                    AddingOrEditing::Adding => {
                        self.filters.push(self.current_filter.clone());
                        self.selected_filter_idx = self.filters.len() - 1;
                        self.state = EditNodeFiltersState::Open;
                    }
                    AddingOrEditing::Editing => {
                        *self.filters.get_mut(self.selected_filter_idx).unwrap() =
                            self.current_filter.clone();
                        self.state = EditNodeFiltersState::Open;
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                self.state = EditNodeFiltersState::Open;
            };
        });
    }

    fn draw_edit_filter_rule(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Editing Rule");
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            ui.label("Rule Name:");
            ui.text_edit_singleline(&mut self.current_rule.name);
        });
        self.draw_short_separator(ui);
        ui.label("Node name condition:");
        EditDisplayModes::draw_edit_match_condition(
            ui,
            &mut self.current_rule.condition,
            "node name condition",
        );
        self.draw_short_separator(ui);
        ui.label("Decision");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.current_rule.visible, "Visible");
        });
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("Ok").clicked() {
                match self.editing_or_adding_rule {
                    AddingOrEditing::Adding => {
                        self.current_filter.rules.push(self.current_rule.clone());
                        self.selected_rule_idx = self.current_filter.rules.len() - 1;
                        self.state = EditNodeFiltersState::EditingFilter;
                    }
                    AddingOrEditing::Editing => {
                        self.current_filter.rules[self.selected_rule_idx] =
                            self.current_rule.clone();
                        self.state = EditNodeFiltersState::EditingFilter;
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                self.state = EditNodeFiltersState::EditingFilter;
            }
        });
    }

    fn new_filter() -> NodeFilter {
        NodeFilter {
            name: "New Filter".to_string(),
            rules: vec![Self::new_rule()],
            is_editable: true,
        }
    }

    fn new_rule() -> NodeRule {
        NodeRule {
            name: "Show my_node".to_string(),
            condition: MatchCondition {
                operator: MatchOperator::EqualTo,
                value: "my_node".to_string(),
            },
            visible: true,
        }
    }

    fn draw_short_separator(&self, ui: &mut Ui) {
        ui.set_max_width(10.0);
        ui.separator();
        ui.set_max_width(self.max_width);
    }
}
