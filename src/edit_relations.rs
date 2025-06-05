use eframe::egui::{self, Button, ComboBox, Modal, ScrollArea, Ui, Vec2, Widget};
use std::collections::HashMap;
use uuid::Uuid;

use crate::edit_modes::{AddingOrEditing, EditDisplayModes, HIGHLIGHT_COLOR};
use crate::relation::{
    AttributeRelation, AttributeRelationOp, MatchType, Relation, RelationNodesConfig, RelationView,
};
use crate::structured_modes::SpanSelector;

#[derive(Clone, Debug)]
pub struct EditRelations {
    state: EditRelationsState,
    relations: Vec<Relation>,
    relation_views: Vec<RelationView>,
    selected_relation_idx: usize,
    current_relation: Relation,
    editing_or_adding_relation: AddingOrEditing,
    not_editable_message: String,
    min_time_difference_string: String,
    max_time_difference_string: String,
    max_width: f32,
    max_scrollarea_size: egui::Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditRelationsState {
    Closed,
    Open,
    NotEditableError,
    ConfirmDeletion,
    EditingRelation,
}

impl EditRelations {
    pub fn new() -> EditRelations {
        EditRelations {
            state: EditRelationsState::Closed,
            relations: Vec::new(),
            relation_views: Vec::new(),
            selected_relation_idx: 0,
            current_relation: Self::new_relation(),
            editing_or_adding_relation: AddingOrEditing::Editing,
            not_editable_message: String::new(),
            min_time_difference_string: "0.0".to_string(),
            max_time_difference_string: String::new(),
            max_width: 0.0,
            max_scrollarea_size: egui::Vec2::ZERO,
        }
    }

    pub fn open(&mut self, relations: Vec<Relation>, relation_views: Vec<RelationView>) {
        self.relations = relations;
        self.relation_views = relation_views;
        self.selected_relation_idx = 0;
        self.state = EditRelationsState::Open;
    }

    pub fn draw(
        &mut self,
        max_width: f32,
        max_height: f32,
        ctx: &egui::Context,
    ) -> Option<(Vec<Relation>, Vec<RelationView>)> {
        if self.state == EditRelationsState::Closed {
            return None;
        }

        self.max_width = max_width;
        self.max_scrollarea_size = Vec2::new(max_width, max_height - 200.0);
        let mut result = None;
        Modal::new("edit relations".into()).show(ctx, |ui| {
            ui.set_max_width(max_width);
            ui.set_max_height(max_height);
            match self.state {
                EditRelationsState::Closed => unreachable!(),
                EditRelationsState::Open => result = self.draw_open(ui, ctx),
                EditRelationsState::ConfirmDeletion => self.draw_delete_confirmation(ui, ctx),
                EditRelationsState::NotEditableError => self.draw_not_editable_error(ui, ctx),
                EditRelationsState::EditingRelation => self.draw_editing_relation(ui, ctx),
            }
        });

        result
    }

    fn draw_open(
        &mut self,
        ui: &mut Ui,
        _ctx: &egui::Context,
    ) -> Option<(Vec<Relation>, Vec<RelationView>)> {
        ui.label("Edit releations");

        self.draw_short_separator(ui);

        ui.label("Relations");
        ui.allocate_ui(self.max_scrollarea_size, |ui| {
            ScrollArea::vertical().id_salt("relations").show(ui, |ui| {
                let mut sorted_relations: Vec<(usize, &Relation)> =
                    self.relations.iter().enumerate().collect::<Vec<_>>();
                sorted_relations.sort_by_key(|r| &r.1.name);

                for (index, relation) in sorted_relations {
                    let relation_name = if relation.is_builtin {
                        format!("{} (builtin)", relation.name)
                    } else {
                        relation.name.clone()
                    };

                    let button = if self.selected_relation_idx == index {
                        Button::new(relation_name).fill(HIGHLIGHT_COLOR)
                    } else {
                        Button::new(relation_name)
                    };
                    if button.ui(ui).clicked() {
                        self.selected_relation_idx = index;
                    }
                }
            });
        });

        self.draw_short_separator(ui);

        ui.label("Actions");
        ui.horizontal(|ui| {
            if ui.button("New Relation").clicked() {
                let new_relation = Self::new_relation();
                self.current_relation = new_relation.clone();
                self.set_time_difference_strings();
                self.selected_relation_idx = 0;
                self.editing_or_adding_relation = AddingOrEditing::Adding;
                self.state = EditRelationsState::EditingRelation;
            }
            if ui.button("Edit Relation").clicked() {
                if let Some(relation) = self.relations.get(self.selected_relation_idx) {
                    if relation.is_builtin {
                        self.not_editable_message =
                        "This relation is not editable! Builtin relations that are provided in traviz cannot be changed from the UI. \
                        You can clone this relation to create your own custom one and then edit the custom relation".to_string();
                        self.state = EditRelationsState::NotEditableError;
                    } else {
                        self.current_relation = relation.clone();
                        self.set_time_difference_strings();
                        self.editing_or_adding_relation = AddingOrEditing::Editing;
                        self.state = EditRelationsState::EditingRelation;
                    }
                }
            }
            if ui.button("Clone Relation").clicked() {
                let mut new_relation = self.relations[self.selected_relation_idx].clone();
                new_relation.id = Uuid::new_v4();
                new_relation.name = format!("{} Clone", new_relation.name);
                new_relation.is_builtin = false;
                self.relations.push(new_relation);
                self.selected_relation_idx = self.relations.len() - 1;
            }
            if ui.button("Delete Relation").clicked() {
                if let Some(relation) = self.relations.get(self.selected_relation_idx) {
                    if relation.is_builtin {
                        self.not_editable_message = "Builtin relations can not be deleted".to_string();
                        self.state = EditRelationsState::NotEditableError;
                    } else {
                        self.state = EditRelationsState::ConfirmDeletion;
                    }
                }
            }
        });

        let mut result = None;

        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.state = EditRelationsState::Closed;
                result = Some((
                    std::mem::take(&mut self.relations),
                    std::mem::take(&mut self.relation_views),
                ));
            }
            if ui.button("Cancel").clicked() {
                self.state = EditRelationsState::Closed;
            }
        });

        result
    }

    fn draw_delete_confirmation(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        let Some(relation) = self.relations.get(self.selected_relation_idx) else {
            self.state = EditRelationsState::Open;
            return;
        };

        ui.label("Are you sure you want to delete this relation?");
        self.draw_short_separator(ui);
        ui.label(format!("Relation Name: {}", relation.name));
        self.draw_short_separator(ui);
        if ui.button("Yes, Delete").clicked() {
            // First remove it from all relation views
            for relation_view in &mut self.relation_views {
                relation_view
                    .enabled_relations
                    .retain(|relation_id| *relation_id != relation.id);
            }

            self.relations.remove(self.selected_relation_idx);
            self.selected_relation_idx = 0;
            self.state = EditRelationsState::Open;
        }
        if ui.button("No, Cancel").clicked() {
            self.state = EditRelationsState::Open;
        }
    }

    fn draw_not_editable_error(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label(&self.not_editable_message);
        self.draw_short_separator(ui);
        if ui.button("Ok").clicked() {
            self.state = EditRelationsState::Open;
        }
    }

    fn draw_editing_relation(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.heading("Editing Relation");
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            ui.label("Relation name:");
            ui.text_edit_singleline(&mut self.current_relation.name);
        });
        // TODO - description
        self.draw_short_separator(ui);
        ui.strong("From span selector");
        EditDisplayModes::draw_edit_span_selector(
            &mut self.current_relation.from_span_selector,
            ui,
            self.max_width,
            "from span selector",
        );
        ui.add_space(20.0);
        self.draw_short_separator(ui);
        ui.strong("To span selector");
        EditDisplayModes::draw_edit_span_selector(
            &mut self.current_relation.to_span_selector,
            ui,
            self.max_width,
            "to span selector",
        );
        ui.add_space(20.0);
        self.draw_short_separator(ui);
        ui.strong("Attribute Relations");
        let mut attribute_relation_to_remove = None;
        for (i, attr_condition) in &mut self
            .current_relation
            .attribute_relations
            .iter_mut()
            .enumerate()
        {
            ui.horizontal(|ui| {
                ui.label("From attribute:");
                ui.text_edit_singleline(&mut attr_condition.from_attribute);
                ui.label("To attribute:");
                ui.text_edit_singleline(&mut attr_condition.to_attribute);
                ui.label("Relation:");
                ComboBox::new(format!("relation operator {i}"), "")
                    .selected_text(match attr_condition.relation {
                        AttributeRelationOp::Equal => "Equal",
                        AttributeRelationOp::OneGreater => "One greater",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut attr_condition.relation,
                            AttributeRelationOp::Equal,
                            "Equal",
                        );
                        ui.selectable_value(
                            &mut attr_condition.relation,
                            AttributeRelationOp::OneGreater,
                            "One greater",
                        );
                    });
                if ui.button("Remove").clicked() {
                    attribute_relation_to_remove = Some(i);
                }
            });
        }
        if let Some(idx) = attribute_relation_to_remove {
            self.current_relation.attribute_relations.remove(idx);
        }
        if ui.button("New Attribute relation").clicked() {
            self.current_relation
                .attribute_relations
                .push(AttributeRelation {
                    from_attribute: "from attribute".to_string(),
                    to_attribute: "to attribute".to_string(),
                    relation: AttributeRelationOp::Equal,
                });
        }

        self.draw_short_separator(ui);
        ui.strong("Other options");
        ui.horizontal(|ui| {
            ui.horizontal(|ui| {
                ui.label("Min time difference (seconds) (can be negative to support backward relations):");
                ui.text_edit_singleline(&mut self.min_time_difference_string);
                if self.min_time_difference_string.is_empty() {
                    self.current_relation.min_time_diff = 0.0;
                } else if let Ok(value) = self.min_time_difference_string.parse::<f64>() {
                    self.current_relation.min_time_diff = value;
                } else {
                    ui.label("Can't parse value!");
                    self.current_relation.min_time_diff = 0.0;
                }
            })
        });
        ui.horizontal(|ui| {
            ui.label("Max time difference (seconds):");
            ui.text_edit_singleline(&mut self.max_time_difference_string);
            if self.max_time_difference_string.is_empty() {
                self.current_relation.max_time_diff = None;
            } else if let Ok(value) = self.max_time_difference_string.parse::<f64>() {
                self.current_relation.max_time_diff = Some(value);
            } else {
                ui.label("Can't parse value!");
                self.current_relation.max_time_diff = None;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Nodes config:");
            ComboBox::new("nodes config", "")
                .selected_text(match self.current_relation.nodes_config {
                    RelationNodesConfig::AllNodes => "All nodes",
                    RelationNodesConfig::SameNode => "Same node",
                    RelationNodesConfig::DifferentNode => "Different node",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.current_relation.nodes_config,
                        RelationNodesConfig::AllNodes,
                        "All nodes",
                    );
                    ui.selectable_value(
                        &mut self.current_relation.nodes_config,
                        RelationNodesConfig::SameNode,
                        "Same Node",
                    );
                    ui.selectable_value(
                        &mut self.current_relation.nodes_config,
                        RelationNodesConfig::DifferentNode,
                        "Different node",
                    );
                });
        });

        ui.horizontal(|ui| {
            ui.label("Match type:");
            ComboBox::new("match type", "")
                .selected_text(match self.current_relation.match_type {
                    MatchType::MatchAll => "Match all",
                    MatchType::MatchClosest => "Match closest",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.current_relation.match_type,
                        MatchType::MatchAll,
                        "Match all",
                    );
                    ui.selectable_value(
                        &mut self.current_relation.match_type,
                        MatchType::MatchClosest,
                        "Match closest",
                    );
                });
        });

        ui.horizontal(|ui| {
            if ui.button("Ok").clicked() {
                match self.editing_or_adding_relation {
                    AddingOrEditing::Adding => {
                        self.relations.push(self.current_relation.clone());
                        self.selected_relation_idx = self.relations.len() - 1;
                        self.state = EditRelationsState::Open;
                    }
                    AddingOrEditing::Editing => {
                        self.relations[self.selected_relation_idx] = self.current_relation.clone();
                        self.state = EditRelationsState::Open;
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                self.state = EditRelationsState::Open;
            }
        });
    }

    fn draw_short_separator(&self, ui: &mut Ui) {
        ui.set_max_width(10.0);
        ui.separator();
        ui.set_max_width(self.max_width);
    }

    fn new_relation() -> Relation {
        Relation {
            id: Uuid::new_v4(),
            name: "New relation".to_string(),
            description: String::new(),
            from_span_selector: SpanSelector::new_equal_name("from span"),
            to_span_selector: SpanSelector::new_equal_name("to span"),
            attribute_relations: vec![],
            max_time_diff: Some(10.0),
            nodes_config: RelationNodesConfig::AllNodes,
            match_type: MatchType::MatchAll,
            min_time_diff: 0.0,
            is_builtin: false,
        }
    }

    fn set_time_difference_strings(&mut self) {
        if let Some(max_time_diff) = self.current_relation.max_time_diff {
            self.max_time_difference_string = format!("{:.6}", max_time_diff.to_string());
        } else {
            self.max_time_difference_string = String::new();
        }
        self.min_time_difference_string =
            format!("{:.6}", self.current_relation.min_time_diff.to_string());
    }
}

#[derive(Clone, Debug)]
pub struct EditRelationViews {
    state: EditRelationViewsState,
    relations: HashMap<Uuid, Relation>,
    relation_views: Vec<RelationView>,
    selected_relation_view_idx: usize,
    current_relation_view: RelationView,
    editing_or_adding_view: AddingOrEditing,
    not_editable_message: String,
    max_width: f32,
    max_scrollarea_size: egui::Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditRelationViewsState {
    Closed,
    Open,
    EditingRelationView,
    NotEditableError,
    ConfirmDeletion,
}

impl EditRelationViews {
    pub fn new() -> EditRelationViews {
        EditRelationViews {
            state: EditRelationViewsState::Closed,
            relations: HashMap::new(),
            relation_views: Vec::new(),
            selected_relation_view_idx: 0,
            current_relation_view: RelationView {
                enabled_relations: Vec::new(),
                name: String::new(),
                is_builtin: false,
            },
            editing_or_adding_view: AddingOrEditing::Editing,
            not_editable_message: String::new(),
            max_width: 0.0,
            max_scrollarea_size: egui::Vec2::ZERO,
        }
    }

    pub fn open(&mut self, relations: Vec<Relation>, relation_views: Vec<RelationView>) {
        self.relations = relations
            .into_iter()
            .map(|relation| (relation.id, relation))
            .collect();
        self.relation_views = relation_views;
        self.selected_relation_view_idx = 0;
        self.state = EditRelationViewsState::Open;
    }

    pub fn draw(
        &mut self,
        max_width: f32,
        max_height: f32,
        ctx: &egui::Context,
    ) -> Option<Vec<RelationView>> {
        if self.state == EditRelationViewsState::Closed {
            return None;
        }

        self.max_width = max_width;
        self.max_scrollarea_size = Vec2::new(max_width, max_height - 200.0);
        let mut result = None;
        Modal::new("edit relation views".into()).show(ctx, |ui| {
            ui.set_max_width(max_width);
            ui.set_max_height(max_height);
            match self.state {
                EditRelationViewsState::Closed => unreachable!(),
                EditRelationViewsState::Open => result = self.draw_open(ui, ctx),
                EditRelationViewsState::ConfirmDeletion => self.draw_delete_confirmation(ui, ctx),
                EditRelationViewsState::NotEditableError => self.draw_not_editable_error(ui, ctx),
                EditRelationViewsState::EditingRelationView => {
                    self.draw_editing_relation_view(ui, ctx)
                }
            }
        });

        result
    }

    fn draw_open(&mut self, ui: &mut Ui, _ctx: &egui::Context) -> Option<Vec<RelationView>> {
        ui.label("Edit relation views");

        self.draw_short_separator(ui);

        ui.label("Relation views");
        ui.allocate_ui(self.max_scrollarea_size, |ui| {
            ScrollArea::vertical().id_salt("relations").show(ui, |ui| {
                for (index, relation_view) in self.relation_views.iter().enumerate() {
                    let view_name = if relation_view.is_builtin {
                        format!("{} (builtin)", relation_view.name)
                    } else {
                        relation_view.name.clone()
                    };

                    let button = if self.selected_relation_view_idx == index {
                        Button::new(view_name).fill(HIGHLIGHT_COLOR)
                    } else {
                        Button::new(view_name)
                    };
                    if button.ui(ui).clicked() {
                        self.selected_relation_view_idx = index;
                    }
                }
            });
        });

        self.draw_short_separator(ui);

        ui.label("Actions");
        ui.horizontal(|ui| {
            if ui.button("New Relation view").clicked() {
                let new_relation_view = Self::new_view();
                self.current_relation_view = new_relation_view.clone();
                self.selected_relation_view_idx = 0;
                self.editing_or_adding_view = AddingOrEditing::Adding;
                self.state = EditRelationViewsState::EditingRelationView;
            }
            if ui.button("Edit Relation view").clicked() {
                if let Some(relation_view) = self.relation_views.get(self.selected_relation_view_idx) {
                    if relation_view.is_builtin {
                        self.not_editable_message =
                        "This relation view is not editable! Builtin relation views that are provided in traviz cannot be changed from the UI. \
                        You can clone this relation view to create your own custom one and then edit the custom relation view".to_string();
                        self.state = EditRelationViewsState::NotEditableError;
                    } else {
                        self.current_relation_view = relation_view.clone();
                        self.editing_or_adding_view = AddingOrEditing::Editing;
                        self.state = EditRelationViewsState::EditingRelationView;
                    }
                }
            }
            if ui.button("Clone Relation view").clicked() {
                let mut new_relation_view = self.relation_views[self.selected_relation_view_idx].clone();
                new_relation_view.name = format!("{} Clone", new_relation_view.name);
                new_relation_view.is_builtin = false;
                self.relation_views.push(new_relation_view);
                self.selected_relation_view_idx = self.relation_views.len() - 1;
            }
            if ui.button("Delete Relation view").clicked() {
                if let Some(view) = self.relation_views.get(self.selected_relation_view_idx) {
                    if view.is_builtin {
                        self.not_editable_message = "Builtin relation views can not be deleted".to_string();
                        self.state = EditRelationViewsState::NotEditableError;
                    } else {
                        self.state = EditRelationViewsState::ConfirmDeletion;
                    }
                }
            }
        });

        let mut result = None;

        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.state = EditRelationViewsState::Closed;
                result = Some(std::mem::take(&mut self.relation_views));
            }
            if ui.button("Cancel").clicked() {
                self.state = EditRelationViewsState::Closed;
            }
        });

        result
    }

    fn draw_delete_confirmation(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        let Some(relation_view) = self.relation_views.get(self.selected_relation_view_idx) else {
            self.state = EditRelationViewsState::Open;
            return;
        };

        ui.label("Are you sure you want to delete this relation view?");
        self.draw_short_separator(ui);
        ui.label(format!("Relation view name: {}", relation_view.name));
        self.draw_short_separator(ui);
        if ui.button("Yes, Delete").clicked() {
            self.relation_views.remove(self.selected_relation_view_idx);
            self.selected_relation_view_idx = 0;
            self.state = EditRelationViewsState::Open;
        }
        if ui.button("No, Cancel").clicked() {
            self.state = EditRelationViewsState::Open;
        }
    }

    fn draw_not_editable_error(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label(&self.not_editable_message);
        self.draw_short_separator(ui);
        if ui.button("Ok").clicked() {
            self.state = EditRelationViewsState::Open;
        }
    }

    fn draw_editing_relation_view(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.label("Editing Relation View");
        self.draw_short_separator(ui);
        ui.horizontal(|ui| {
            ui.label("Relation view name:");
            ui.text_edit_singleline(&mut self.current_relation_view.name);
        });
        self.draw_short_separator(ui);

        let mut all_relations: Vec<(String, Uuid, bool)> = self
            .relations
            .iter()
            .map(|(relation_id, relation)| {
                let is_enabled = self
                    .current_relation_view
                    .enabled_relations
                    .contains(relation_id);
                (relation.name.clone(), *relation_id, is_enabled)
            })
            .collect::<Vec<_>>();
        for enabled_relation_id in &self.current_relation_view.enabled_relations {
            if !self.relations.contains_key(enabled_relation_id) {
                all_relations.push(("Unknown Relation".to_string(), *enabled_relation_id, true));
            }
        }
        all_relations.sort_by_key(|(name, _, is_enabled)| (!is_enabled, name.clone()));

        ui.allocate_ui(self.max_scrollarea_size, |ui| {
            ScrollArea::vertical()
                .id_salt("enabled relations")
                .show(ui, |ui| {
                    ui.label("Enabled relations");
                    let mut first_disabled = true;
                    for (relation_name, relation_id, mut enabled) in all_relations {
                        if !enabled && first_disabled {
                            ui.label("Disabled relations");
                            first_disabled = false;
                        }

                        let was_enabled = enabled;

                        ui.horizontal(|ui| {
                            ui.checkbox(&mut enabled, "");
                            ui.label(relation_name);
                        });

                        if was_enabled != enabled {
                            if enabled {
                                self.current_relation_view
                                    .enabled_relations
                                    .push(relation_id);
                            } else {
                                self.current_relation_view
                                    .enabled_relations
                                    .retain(|id| *id != relation_id);
                            }
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            if ui.button("Ok").clicked() {
                match self.editing_or_adding_view {
                    AddingOrEditing::Adding => {
                        self.relation_views.push(self.current_relation_view.clone());
                        self.selected_relation_view_idx = self.relation_views.len() - 1;
                        self.state = EditRelationViewsState::Open;
                    }
                    AddingOrEditing::Editing => {
                        self.relation_views[self.selected_relation_view_idx] =
                            self.current_relation_view.clone();
                        self.state = EditRelationViewsState::Open;
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                self.state = EditRelationViewsState::Open;
            }
        });
    }

    fn draw_short_separator(&self, ui: &mut Ui) {
        ui.set_max_width(10.0);
        ui.separator();
        ui.set_max_width(self.max_width);
    }

    fn new_view() -> RelationView {
        RelationView {
            enabled_relations: Vec::new(),
            name: "New Relation View".to_string(),
            is_builtin: false,
        }
    }
}
