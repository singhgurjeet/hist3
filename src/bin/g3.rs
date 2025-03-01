use atty::Stream;
use clap::Parser;
use core::f64;
use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::Undirected;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{fs::File, thread};

const SPRING_LENGTH: f32 = 12.5; // Increase for more spacing
const SPRING_K: f32 = 0.05; // Reduced for more stability
const REPULSION_K: f32 = 100_000.0; // Increase to encourage planarity
const DAMPING: f32 = 0.8; // Increase to prevent oscillation
const MAX_VELOCITY: f32 = 20.0;
const MIN_MOVEMENT: f32 = 10.0; // Movement threshold to prevent nodes from jiggling
const COMPONENT_SPACING: f32 = 500.0; // Minimum spacing between components

const NODE_RADIUS: f32 = 12.0;

mod colors {
    use egui::Color32;

    // Forest High Contrast
    pub mod forest_bold {
        use super::*;
        pub const NODE_DEFAULT: Color32 = Color32::from_rgb(47, 95, 61); // Deep forest green
        pub const NODE_SELECTED: Color32 = Color32::from_rgb(255, 166, 0); // Bright amber
        pub const NODE_PREVIEW: Color32 = Color32::from_rgb(141, 227, 135); // Bright leaf green
        pub const NODE_NEIGHBOR: Color32 = Color32::from_rgb(255, 89, 94); // Wild mushroom red
        pub const STROKE_DEFAULT: Color32 = Color32::from_rgb(240, 250, 240); // Bright moss
        pub const EDGE: Color32 = Color32::from_rgb(140, 140, 140); // Neutral grey
    }

    // Mountain Lake High Contrast
    pub mod mountain_bold {
        use super::*;
        pub const NODE_DEFAULT: Color32 = Color32::from_rgb(43, 101, 136); // Deep lake blue
        pub const NODE_SELECTED: Color32 = Color32::from_rgb(255, 198, 30); // Bright sunlight
        pub const NODE_PREVIEW: Color32 = Color32::from_rgb(110, 206, 255); // Sky reflection
        pub const NODE_NEIGHBOR: Color32 = Color32::from_rgb(255, 117, 143); // Alpine rose
        pub const STROKE_DEFAULT: Color32 = Color32::from_rgb(235, 245, 255); // Snow white
        pub const EDGE: Color32 = Color32::from_rgb(150, 150, 150); // Neutral grey
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ColorTheme {
    ForestBold,
    MountainBold,
}

impl ColorTheme {
    fn node_default(&self) -> Color32 {
        match self {
            ColorTheme::ForestBold => colors::forest_bold::NODE_DEFAULT,
            ColorTheme::MountainBold => colors::mountain_bold::NODE_DEFAULT,
        }
    }

    fn node_selected(&self) -> Color32 {
        match self {
            ColorTheme::ForestBold => colors::forest_bold::NODE_SELECTED,
            ColorTheme::MountainBold => colors::mountain_bold::NODE_SELECTED,
        }
    }

    fn node_preview(&self) -> Color32 {
        match self {
            ColorTheme::ForestBold => colors::forest_bold::NODE_PREVIEW,
            ColorTheme::MountainBold => colors::mountain_bold::NODE_PREVIEW,
        }
    }

    fn node_neighbor(&self) -> Color32 {
        match self {
            ColorTheme::ForestBold => colors::forest_bold::NODE_NEIGHBOR,
            ColorTheme::MountainBold => colors::mountain_bold::NODE_NEIGHBOR,
        }
    }

    fn stroke_default(&self) -> Color32 {
        match self {
            ColorTheme::ForestBold => colors::forest_bold::STROKE_DEFAULT,
            ColorTheme::MountainBold => colors::mountain_bold::STROKE_DEFAULT,
        }
    }

    fn edge(&self) -> Color32 {
        match self {
            ColorTheme::ForestBold => colors::forest_bold::EDGE,
            ColorTheme::MountainBold => colors::mountain_bold::EDGE,
        }
    }
}

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,

    /// Is the graph weighted? If yes, input is like node_1\tnode_2\tweight
    #[arg(long, short)]
    weighted: bool,
}

#[derive(PartialEq)]
enum InteractionMode {
    Pan,
    Select,
}

struct SelectionState {
    selected_nodes: HashSet<NodeIndex>,
    preview_nodes: HashSet<NodeIndex>, // New: tracks nodes currently in selection rectangle
    drag_start: Option<Pos2>,
    drag_end: Option<Pos2>,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selected_nodes: HashSet::new(),
            preview_nodes: HashSet::new(),
            drag_start: None,
            drag_end: None,
        }
    }
}

struct GraphVisualizerApp {
    graph_data: Arc<Mutex<Graph<String, f64, Undirected>>>,
    weight_histogram: Arc<Mutex<Histogram>>,
    weighted: bool,
    min_weight: f64,
    positions: Arc<Mutex<HashMap<NodeIndex, Pos2>>>,
    velocities: HashMap<NodeIndex, Vec2>,
    is_dragging: Option<NodeIndex>,
    running_simulation: bool,
    components: Vec<Vec<NodeIndex>>,
    initialized: bool,
    zoom_level: f32,
    pan_offset: Vec2,
    interaction_mode: InteractionMode,
    selection_state: SelectionState,
    initial_layout_complete: bool,
    min_zoom_level: f32,
    simulation_start_time: Option<std::time::Instant>,
    frame_count: u64,
    color_theme: ColorTheme,
}

impl Default for GraphVisualizerApp {
    fn default() -> Self {
        Self {
            graph_data: Arc::new(Mutex::new(Graph::new_undirected())),
            weight_histogram: Arc::new(Mutex::new(Histogram {
                bins: Vec::new(),
                min: 0.0,
                max: 0.0,
                bin_width: 0.0,
            })),
            weighted: false,
            min_weight: f64::NEG_INFINITY,
            positions: Arc::new(Mutex::new(HashMap::new())),
            velocities: HashMap::new(),
            is_dragging: None,
            running_simulation: true,
            components: Vec::new(),
            initialized: false,
            zoom_level: 1.0,
            pan_offset: Vec2::ZERO,
            interaction_mode: InteractionMode::Pan,
            selection_state: SelectionState::default(),
            initial_layout_complete: false,
            min_zoom_level: 0.5,
            simulation_start_time: None,
            frame_count: 0,
            color_theme: ColorTheme::ForestBold,
        }
    }
}

impl eframe::App for GraphVisualizerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let window_size = ctx.available_rect().size();

        if ctx.input(|i| i.key_pressed(egui::Key::H)) {
            self.fit_to_view(window_size);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.running_simulation = !self.running_simulation;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::S)) {
            self.interaction_mode = if self.interaction_mode == InteractionMode::Select {
                InteractionMode::Pan
            } else {
                InteractionMode::Select
            };
        }

        if !self.initialized {
            let graph = self.graph_data.lock().unwrap();
            if !graph.node_indices().next().is_none() {
                drop(graph);
                self.reset_layout(ctx.available_rect().size());
                self.initialized = true;
            }
        }

        if self.running_simulation {
            self.update_layout();
            if !self.running_simulation && !self.initial_layout_complete {
                // Only fit on first settle
                self.fit_to_view(window_size);
                self.initial_layout_complete = true;
            }
            ctx.request_repaint();
        }

        // Add this section for keyboard panning
        let pan_speed = 10.0 / self.zoom_level;
        if ctx.input(|i| i.key_down(egui::Key::ArrowRight)) {
            self.pan_offset.x += pan_speed;
        }
        if ctx.input(|i| i.key_down(egui::Key::ArrowLeft)) {
            self.pan_offset.x -= pan_speed;
        }
        if ctx.input(|i| i.key_down(egui::Key::ArrowDown)) {
            self.pan_offset.y += pan_speed;
        }
        if ctx.input(|i| i.key_down(egui::Key::ArrowUp)) {
            self.pan_offset.y -= pan_speed;
        }

        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(if self.running_simulation {
                        "⏸ Pause"
                    } else {
                        "▶ Resume"
                    })
                    .clicked()
                {
                    self.running_simulation = !self.running_simulation;
                }
                if ui.button("🔄 Reset Layout").clicked() {
                    self.reset_layout(ctx.available_rect().size());
                }
                if ui.button("🔍 Fit to View").clicked() {
                    self.fit_to_view(window_size);
                }

                if self.weighted {
                    ui.horizontal(|ui| {
                        let (min, max) = {
                            let histogram = self.weight_histogram.lock().unwrap();
                            (histogram.min, histogram.max)
                        };

                        ui.label("Weight Filter");
                        let mut weight_value = if self.min_weight == f64::NEG_INFINITY {
                            min
                        } else {
                            self.min_weight
                        };
                        if ui
                            .add(egui::Slider::new(&mut weight_value, min..=max).show_value(true))
                            .changed()
                        {
                            self.min_weight = weight_value;
                        }
                    });
                }

                // Add selection mode toggle button
                if ui
                    .selectable_label(
                        self.interaction_mode == InteractionMode::Select,
                        "✋ Select Mode",
                    )
                    .clicked()
                {
                    self.interaction_mode = if self.interaction_mode == InteractionMode::Select {
                        InteractionMode::Pan
                    } else {
                        InteractionMode::Select
                    };
                }

                // Add theme selection combo box
                egui::ComboBox::from_label("Theme")
                    .selected_text(match self.color_theme {
                        ColorTheme::ForestBold => "Forest",
                        ColorTheme::MountainBold => "Mountain",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.color_theme,
                            ColorTheme::ForestBold,
                            "Forest",
                        );
                        ui.selectable_value(
                            &mut self.color_theme,
                            ColorTheme::MountainBold,
                            "Mountain",
                        );
                    });

                ui.label(format!("Zoom: {:.1}x", self.zoom_level));
                let graph = self.graph_data.lock().unwrap();
                let names: Vec<String> = self
                    .selection_state
                    .selected_nodes
                    .iter()
                    .filter_map(|&idx| graph.node_weight(idx).cloned())
                    .collect();
                if !names.is_empty() {
                    ctx.copy_text(names.join("\n"));
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

            // Handle zoom
            if let Some(hover_pos) = response.hover_pos() {
                let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
                if scroll_delta != 0.0 {
                    self.handle_zoom(scroll_delta, hover_pos);
                }
            }

            let pointer_pos = response
                .hover_pos()
                .map(|pos| self.screen_to_graph_pos(pos));

            // Handle selection and dragging
            let modifiers = ui.input(|i| i.modifiers);

            if self.interaction_mode == InteractionMode::Select {
                if response.dragged() {
                    if let Some(pos) = response.hover_pos() {
                        if self.selection_state.drag_start.is_none() {
                            self.selection_state.drag_start = Some(pos);
                        }
                        self.selection_state.drag_end = Some(pos);

                        // Update preview selection in real-time
                        self.update_preview_selection(modifiers);
                    }
                } else if response.clicked() {
                    if let Some(pos) = pointer_pos {
                        let positions = self.positions.lock().unwrap();
                        let mut found_node = false;

                        for (idx, &node_pos) in positions.iter() {
                            if node_pos.distance(pos) < 15.0 {
                                if modifiers.ctrl {
                                    // Toggle single node
                                    if !self.selection_state.selected_nodes.remove(idx) {
                                        self.selection_state.selected_nodes.insert(*idx);
                                    }
                                } else if modifiers.shift {
                                    // Add to selection
                                    self.selection_state.selected_nodes.insert(*idx);
                                } else {
                                    // New selection
                                    self.selection_state.selected_nodes.clear();
                                    self.selection_state.selected_nodes.insert(*idx);
                                }
                                found_node = true;
                                break;
                            }
                        }

                        if !found_node && !modifiers.ctrl && !modifiers.shift {
                            self.selection_state.selected_nodes.clear();
                        }
                    }
                } else if response.drag_stopped() {
                    // Finalize selection
                    if !modifiers.ctrl && !modifiers.shift {
                        self.selection_state.selected_nodes.clear();
                    }

                    for node_idx in &self.selection_state.preview_nodes {
                        if modifiers.ctrl {
                            if !self.selection_state.selected_nodes.remove(node_idx) {
                                self.selection_state.selected_nodes.insert(*node_idx);
                            }
                        } else {
                            self.selection_state.selected_nodes.insert(*node_idx);
                        }
                    }

                    // Clear preview state
                    self.selection_state.preview_nodes.clear();
                    self.selection_state.drag_start = None;
                    self.selection_state.drag_end = None;
                } else if response.clicked() {
                    if let Some(pos) = pointer_pos {
                        let positions = self.positions.lock().unwrap();
                        let mut found_node = false;

                        for (idx, &node_pos) in positions.iter() {
                            if node_pos.distance(pos) < 15.0 {
                                if modifiers.ctrl {
                                    // Toggle single node
                                    if !self.selection_state.selected_nodes.remove(idx) {
                                        self.selection_state.selected_nodes.insert(*idx);
                                    }
                                } else if modifiers.shift {
                                    // Add to selection
                                    self.selection_state.selected_nodes.insert(*idx);
                                } else {
                                    // New selection
                                    self.selection_state.selected_nodes.clear();
                                    self.selection_state.selected_nodes.insert(*idx);
                                }
                                found_node = true;
                                break;
                            }
                        }

                        if !found_node && !modifiers.ctrl && !modifiers.shift {
                            self.selection_state.selected_nodes.clear();
                        }
                    }
                } else if response.drag_stopped() {
                    if let (Some(start), Some(end)) = (
                        self.selection_state.drag_start,
                        self.selection_state.drag_end,
                    ) {
                        let selection_rect = Rect::from_two_pos(start, end);
                        let positions = self.positions.lock().unwrap();

                        for (idx, &pos) in positions.iter() {
                            let screen_pos = self.graph_to_screen_pos(pos);
                            if selection_rect.contains(screen_pos) {
                                if modifiers.ctrl {
                                    if !self.selection_state.selected_nodes.remove(idx) {
                                        self.selection_state.selected_nodes.insert(*idx);
                                    }
                                } else if modifiers.shift {
                                    self.selection_state.selected_nodes.insert(*idx);
                                } else {
                                    self.selection_state.selected_nodes.insert(*idx);
                                }
                            }
                        }
                    }

                    self.selection_state.drag_start = None;
                    self.selection_state.drag_end = None;
                }
            } else {
                // Pan mode dragging logic
                if response.dragged() {
                    if let Some(pos) = pointer_pos {
                        if let Some(node_idx) = self.is_dragging {
                            let mut positions = self.positions.lock().unwrap();

                            // If dragging a selected node, move all selected nodes
                            if self.selection_state.selected_nodes.contains(&node_idx) {
                                // Calculate the movement delta
                                let old_pos = positions[&node_idx];
                                let delta = pos - old_pos;

                                // Move all selected nodes by the same delta
                                for &selected_idx in &self.selection_state.selected_nodes {
                                    if let Some(selected_pos) = positions.get_mut(&selected_idx) {
                                        *selected_pos = *selected_pos + delta;
                                    }
                                    self.velocities.insert(selected_idx, Vec2::ZERO);
                                }
                            } else {
                                // If dragging an unselected node, move just that node
                                positions.insert(node_idx, pos);
                                self.velocities.insert(node_idx, Vec2::ZERO);
                            }
                        } else {
                            let positions = self.positions.lock().unwrap();
                            for (idx, &node_pos) in positions.iter() {
                                if node_pos.distance(pos) < NODE_RADIUS / self.zoom_level {
                                    self.is_dragging = Some(*idx);
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    self.is_dragging = None;
                }
            }

            // Draw selection rectangle if in select mode and dragging
            if self.interaction_mode == InteractionMode::Select {
                if let (Some(start), Some(end)) = (
                    self.selection_state.drag_start,
                    self.selection_state.drag_end,
                ) {
                    painter.rect_stroke(
                        Rect::from_two_pos(start, end),
                        0.0,
                        Stroke::new(1.0, Color32::WHITE),
                        egui::StrokeKind::Middle,
                    );
                }
            }

            // Draw the graph
            {
                let graph = self.graph_data.lock().unwrap();
                let positions = self.positions.lock().unwrap();

                // Draw edges
                for edge in graph.edge_indices() {
                    let (source, target) = graph.edge_endpoints(edge).unwrap();
                    let weight = *graph.edge_weight(edge).unwrap_or(&0.0);

                    if self.weighted && weight <= self.min_weight {
                        continue; // Skip drawing this edge
                    }

                    if let (Some(&src_pos), Some(&tgt_pos)) =
                        (positions.get(&source), positions.get(&target))
                    {
                        let screen_src = self.graph_to_screen_pos(src_pos);
                        let screen_tgt = self.graph_to_screen_pos(tgt_pos);

                        // Base thickness that increases as we zoom out
                        let base_thickness = 1.5 / self.zoom_level.powf(0.7);
                        // Clamp to reasonable limits
                        let thickness = base_thickness.clamp(0.5, 1.5);

                        painter.line_segment(
                            [screen_src, screen_tgt],
                            Stroke::new(thickness, self.color_theme.edge()),
                        );
                    }
                }

                // Draw nodes
                for node_idx in graph.node_indices() {
                    if let Some(&position) = positions.get(&node_idx) {
                        let screen_pos = self.graph_to_screen_pos(position);
                        if let Some(node) = graph.node_weight(node_idx) {
                            let node_radius = NODE_RADIUS * self.zoom_level;
                            let stroke_width = 2.0 * self.zoom_level;

                            // Check if this node is a neighbor of the dragged node
                            let is_neighbor = self
                                .is_dragging
                                .map(|dragged_idx| {
                                    graph.neighbors(dragged_idx).any(|n| {
                                        n == node_idx
                                            && (!self.weighted
                                                || graph
                                                    .edge_weight(
                                                        graph.find_edge(dragged_idx, n).unwrap(),
                                                    )
                                                    .unwrap_or(&f64::INFINITY)
                                                    >= &self.min_weight)
                                    })
                                })
                                .unwrap_or(false);

                            // Determine node color
                            let node_color =
                                if self.selection_state.selected_nodes.contains(&node_idx) {
                                    self.color_theme.node_selected()
                                } else if self.selection_state.preview_nodes.contains(&node_idx) {
                                    self.color_theme.node_preview()
                                } else {
                                    self.color_theme.node_default()
                                };

                            // Draw outer circle with highlight for neighbors
                            let stroke_color = if is_neighbor {
                                self.color_theme.node_neighbor()
                            } else {
                                self.color_theme.stroke_default()
                            };

                            painter.circle_stroke(
                                screen_pos,
                                node_radius,
                                Stroke::new(stroke_width, stroke_color),
                            );
                            painter.circle_filled(
                                screen_pos,
                                node_radius - stroke_width,
                                node_color,
                            );

                            // Node label rendering remains the same
                            let font_size = 14.0 * self.zoom_level;
                            let font = egui::FontId::proportional(font_size);
                            let text_padding = 4.0 * self.zoom_level;
                            let circle_spacing = 10.0 * self.zoom_level;

                            let galley = painter.layout_no_wrap(
                                node.to_string(),
                                font.clone(),
                                Color32::WHITE,
                            );

                            let text_pos = Pos2::new(
                                screen_pos.x + node_radius + stroke_width + circle_spacing,
                                screen_pos.y,
                            );
                            let rect = egui::Rect::from_center_size(
                                Pos2::new(text_pos.x + galley.size().x / 2.0, text_pos.y),
                                egui::Vec2::new(
                                    galley.size().x + text_padding * 2.0,
                                    galley.size().y + text_padding * 2.0,
                                ),
                            );

                            painter.rect_filled(rect, 4.0, Color32::from_black_alpha(200));

                            painter.text(
                                text_pos,
                                egui::Align2::LEFT_CENTER,
                                node,
                                font,
                                Color32::WHITE,
                            );
                        }
                    }
                }

                // Add stats at the bottom
                let node_count = graph.node_count();
                let edge_count = graph.edge_count();
                let component_count = self.components.len();
                let selected_count = self.selection_state.selected_nodes.len();

                egui::TopBottomPanel::bottom("stats").show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(format!(
                            "Nodes: {} | Edges: {} | Components: {} | Selected: {}",
                            node_count, edge_count, component_count, selected_count
                        ));
                    });
                });
            }
        });
    }
}
impl GraphVisualizerApp {
    fn update_preview_selection(&mut self, _modifiers: egui::Modifiers) {
        if let (Some(start), Some(end)) = (
            self.selection_state.drag_start,
            self.selection_state.drag_end,
        ) {
            let selection_rect = Rect::from_two_pos(start, end);
            let positions = self.positions.lock().unwrap();

            // Clear previous preview
            self.selection_state.preview_nodes.clear();

            // Update preview selection based on current drag rectangle
            for (idx, &pos) in positions.iter() {
                let screen_pos = self.graph_to_screen_pos(pos);
                if selection_rect.contains(screen_pos) {
                    self.selection_state.preview_nodes.insert(*idx);
                }
            }
        }
    }

    fn handle_zoom(&mut self, scroll_delta: f32, center_pos: Pos2) {
        let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 0.9 };
        let old_zoom = self.zoom_level;

        // Use the calculated min_zoom_level instead of a hard-coded value
        self.zoom_level = (self.zoom_level * zoom_factor).clamp(self.min_zoom_level, 5.0);

        if (self.zoom_level - old_zoom).abs() < f32::EPSILON {
            return;
        }

        let center_vec = Vec2::new(center_pos.x, center_pos.y);
        self.pan_offset =
            center_vec + (self.pan_offset - center_vec) * (self.zoom_level / old_zoom);
    }

    fn fit_to_view(&mut self, available_size: Vec2) {
        let positions = self.positions.lock().unwrap();
        if positions.is_empty() {
            return;
        }

        let is_selection = !self.selection_state.selected_nodes.is_empty();
        let nodes_to_fit: Box<dyn Iterator<Item = &NodeIndex>> = if is_selection {
            Box::new(self.selection_state.selected_nodes.iter())
        } else {
            Box::new(positions.keys())
        };

        // Calculate bounds
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for node_idx in nodes_to_fit {
            if let Some(&pos) = positions.get(node_idx) {
                min_x = min_x.min(pos.x);
                min_y = min_y.min(pos.y);
                max_x = max_x.max(pos.x);
                max_y = max_y.max(pos.y);
            }
        }

        let width = max_x - min_x;
        let height = max_y - min_y;

        // Use different padding for selection vs full graph
        let padding_factor = if is_selection { 1.0 } else { 0.4 }; // More padding for selection
        let padded_width = width * (1.0 + padding_factor);
        let padded_height = height * (1.0 + padding_factor);

        let zoom_x = available_size.x / padded_width;
        let zoom_y = available_size.y / padded_height;

        // Only update min_zoom_level when viewing the full graph
        if !is_selection {
            self.min_zoom_level = zoom_x.min(zoom_y) * 0.8;
        }

        // For selection, allow zooming in more than the global minimum
        let zoom_bounds = if is_selection {
            (self.min_zoom_level * 0.1, 5.0) // Allow much more zoom for selections
        } else {
            (self.min_zoom_level, 5.0)
        };

        let new_zoom = zoom_x.min(zoom_y).clamp(zoom_bounds.0, zoom_bounds.1);
        self.zoom_level = new_zoom;

        // Center the view
        let graph_center_x = (min_x + max_x) / 2.0;
        let graph_center_y = (min_y + max_y) / 2.0;
        let screen_center_x = available_size.x / 2.0;
        let screen_center_y = available_size.y / 2.0;

        self.pan_offset = Vec2::new(
            screen_center_x - (graph_center_x * self.zoom_level),
            screen_center_y - (graph_center_y * self.zoom_level),
        );
    }

    fn graph_to_screen_pos(&self, pos: Pos2) -> Pos2 {
        Pos2::new(
            pos.x * self.zoom_level + self.pan_offset.x,
            pos.y * self.zoom_level + self.pan_offset.y,
        )
    }

    fn screen_to_graph_pos(&self, pos: Pos2) -> Pos2 {
        Pos2::new(
            (pos.x - self.pan_offset.x) / self.zoom_level,
            (pos.y - self.pan_offset.y) / self.zoom_level,
        )
    }

    fn find_components(&mut self) {
        let graph = self.graph_data.lock().unwrap();
        // Use kosaraju_scc which returns Vec<Vec<NodeIndex>>
        self.components = kosaraju_scc(&*graph);
    }

    fn reset_layout(&mut self, window_size: Vec2) {
        self.simulation_start_time = None;
        self.frame_count = 0;

        self.find_components();

        // Calculate grid layout for components
        let components_per_row = (self.components.len() as f32).sqrt().ceil() as usize;

        {
            let mut positions = self.positions.lock().unwrap();

            for (comp_idx, component) in self.components.iter().enumerate() {
                let row = comp_idx / components_per_row;
                let col = comp_idx % components_per_row;

                // Calculate component center position
                let center_x = col as f32 * COMPONENT_SPACING + COMPONENT_SPACING / 2.0;
                let center_y = row as f32 * COMPONENT_SPACING + COMPONENT_SPACING / 2.0;

                // Place nodes in a circle within their component
                let node_count = component.len();
                for (i, &node_idx) in component.iter().enumerate() {
                    let angle = 2.0 * std::f32::consts::PI * (i as f32) / (node_count as f32);
                    let radius = 150.0; // Radius for each component's circle
                    let x = radius * angle.cos() + center_x;
                    let y = radius * angle.sin() + center_y;
                    positions.insert(node_idx, Pos2::new(x, y));

                    // Add random initial velocity
                    let random_angle = (node_idx.index() as f32) * 0.1;
                    let random_velocity = Vec2::new(random_angle.cos(), random_angle.sin()) * 2.0;
                    self.velocities.insert(node_idx, random_velocity);
                }
            }
        }

        self.running_simulation = true;
        self.fit_to_view(window_size);
    }

    fn update_layout(&mut self) {
        // Initialize simulation start time if not set
        if self.simulation_start_time.is_none() {
            self.simulation_start_time = Some(std::time::Instant::now());
        }
        self.frame_count += 1;

        let graph = self.graph_data.lock().unwrap();
        let mut positions = self.positions.lock().unwrap();
        let mut forces: HashMap<NodeIndex, Vec2> = HashMap::new();

        // Update forces for each component separately
        for component in &self.components {
            let component_center = self.calculate_component_center(component, &positions);

            // Initialize forces for this component
            for &node_idx in component {
                let random_angle = (node_idx.index() as f32) * std::f32::consts::PI * 0.1;
                let random_force = Vec2::new(random_angle.cos(), random_angle.sin()) * 0.1;
                forces.insert(node_idx, random_force);
            }

            // Calculate forces within component
            for &node1 in component {
                if self.is_dragging == Some(node1) {
                    continue;
                }

                let mut total_force = Vec2::ZERO;

                // Repulsive forces from other nodes in the same component
                for &node2 in component {
                    if node1 == node2 {
                        continue;
                    }

                    if let (Some(&pos1), Some(&pos2)) =
                        (positions.get(&node1), positions.get(&node2))
                    {
                        let delta = pos1 - pos2;
                        let distance = delta.length().max(1.0);

                        let repulsion_strength = if distance < SPRING_LENGTH {
                            REPULSION_K * 2.0
                        } else {
                            REPULSION_K
                        };

                        let force = delta.normalized() * (repulsion_strength / distance.powi(2));
                        total_force += force;
                    }
                }

                // Add centering force towards component center
                if let Some(&pos) = positions.get(&node1) {
                    let to_center = component_center - Vec2::new(pos.x, pos.y);
                    let center_distance = to_center.length();
                    let center_force = to_center * (0.05 * (center_distance / 300.0).powi(2));
                    total_force += center_force;
                }

                *forces.get_mut(&node1).unwrap() += total_force;
            }

            // Calculate attractive forces along edges within component
            for &node1 in component {
                for neighbor in graph.neighbors(node1) {
                    if component.contains(&neighbor) {
                        // Add check for edge weight if graph is weighted
                        let edge_weight_valid = !self.weighted
                            || graph
                                .find_edge(node1, neighbor)
                                .and_then(|edge| graph.edge_weight(edge))
                                .map_or(false, |&weight| weight >= self.min_weight);

                        if edge_weight_valid {
                            if let (Some(&pos1), Some(&pos2)) =
                                (positions.get(&node1), positions.get(&neighbor))
                            {
                                let delta = pos1 - pos2;
                                let distance = delta.length().max(1.0);

                                let spring_k = if distance > SPRING_LENGTH * 2.0 {
                                    SPRING_K * 2.0
                                } else {
                                    SPRING_K
                                };

                                let force =
                                    delta.normalized() * (distance - SPRING_LENGTH) * -spring_k;

                                if self.is_dragging != Some(node1) {
                                    *forces.get_mut(&node1).unwrap() += force;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Update velocities and positions
        let mut max_movement = 0.0_f32;
        for component in &self.components {
            // Calculate average velocity for the component
            let mut avg_velocity = Vec2::ZERO;
            let mut node_count = 0;

            for &node_idx in component {
                if self.is_dragging != Some(node_idx) {
                    if let Some(&velocity) = self.velocities.get(&node_idx) {
                        avg_velocity += velocity;
                        node_count += 1;
                    }
                }
            }

            if node_count > 0 {
                avg_velocity /= node_count as f32;
            }

            // Update velocities and positions with correction
            for &node_idx in component {
                if self.is_dragging == Some(node_idx) {
                    continue;
                }

                let velocity = self.velocities.entry(node_idx).or_insert(Vec2::ZERO);

                // First subtract average component velocity to prevent drift
                *velocity -= avg_velocity;

                // Then apply new forces and damping
                let force = forces[&node_idx];
                *velocity = (*velocity + force) * DAMPING;

                // Limit velocity
                if velocity.length() > MAX_VELOCITY {
                    *velocity = velocity.normalized() * MAX_VELOCITY;
                }

                // Update position
                if let Some(pos) = positions.get_mut(&node_idx) {
                    let old_pos = *pos;
                    *pos = old_pos + *velocity;

                    max_movement = max_movement.max((*velocity).length());
                }
            }
        }

        // Calculate adaptive movement threshold
        let base_threshold = MIN_MOVEMENT;
        let elapsed = self.simulation_start_time.unwrap().elapsed().as_secs_f32();
        let frame_factor = (self.frame_count as f32 / 100.0).min(1.0); // Ramp up over first 100 frames

        // Increase threshold based on time and frames
        let time_factor = (elapsed / 2.0).min(3.0); // Max 3x increase over 2 seconds
        let adaptive_threshold = base_threshold * (1.0 + time_factor * frame_factor);

        // Stop simulation if movement is very small
        if max_movement < adaptive_threshold {
            self.running_simulation = false;
            self.simulation_start_time = None;
            self.frame_count = 0;
        }
    }

    fn calculate_component_center(
        &self,
        component: &[NodeIndex],
        positions: &HashMap<NodeIndex, Pos2>,
    ) -> Vec2 {
        let mut center = Vec2::ZERO;
        let mut count = 0;

        for &node_idx in component {
            if let Some(&pos) = positions.get(&node_idx) {
                center += Vec2::new(pos.x, pos.y);
                count += 1;
            }
        }

        if count > 0 {
            center /= count as f32;
        }

        center
    }
}

/// Simple histogram struct to help with rendering histograms
struct Histogram {
    bins: Vec<usize>,
    min: f64,
    max: f64,
    bin_width: f64,
}

fn parse_input(
    graph_ref: &Arc<Mutex<Graph<String, f64, Undirected>>>,
    positions_ref: &Arc<Mutex<HashMap<NodeIndex, Pos2>>>,
    histograph_ref: &Arc<Mutex<Histogram>>,
    input: &str,
    weighted: bool,
) {
    // First, collect all unique nodes and edges
    let mut unique_nodes = HashSet::new();
    let mut edges = Vec::new();
    let num_bins = 20;
    let mut bins: Vec<usize> = vec![0; num_bins];
    let mut min_weight = f64::INFINITY;
    let mut max_weight = f64::NEG_INFINITY;

    for line in input.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue; // Skip empty lines
        }

        // Insert all nodes into unique_nodes
        if weighted {
            unique_nodes.insert(parts[0].to_string());
            unique_nodes.insert(parts[1].to_string());
        } else {
            for part in &parts {
                unique_nodes.insert(part.to_string());
            }
        }

        if weighted {
            let node1 = parts[0].to_string();
            let node2 = parts[1].to_string();
            let weight: f64 = parts[2]
                .parse()
                .unwrap_or_else(|_| panic!("Invalid weight value: {}", parts[2]));
            if weight > max_weight {
                max_weight = weight;
            }
            if weight < min_weight {
                min_weight = weight;
            }
            edges.push((node1, node2, weight));
        } else if parts.len() >= 2 {
            for i in 0..parts.len() {
                for j in (i + 1)..parts.len() {
                    let node1 = parts[i].to_string();
                    let node2 = parts[j].to_string();
                    edges.push((node1, node2, 1.0)); // Default weight for unweighted edges
                }
            }
        }
    }

    let bin_width = (max_weight - min_weight) / num_bins as f64;

    let mut graph = graph_ref.lock().unwrap();
    let mut positions = positions_ref.lock().unwrap();
    let mut node_indices = HashMap::new();

    // Create nodes in a circular layout
    let node_count = unique_nodes.len();
    let angle_step = 2.0 * std::f32::consts::PI / (node_count.max(1) as f32);
    let mut angle = 0.0_f32;
    let radius = 200.0;
    let center_x = 400.0;
    let center_y = 300.0;

    // First pass: create all nodes
    for node_name in unique_nodes {
        let index = graph.add_node(node_name.clone());
        let x = radius * angle.cos() + center_x;
        let y = radius * angle.sin() + center_y;
        positions.insert(index, Pos2::new(x, y));
        node_indices.insert(node_name, index);
        angle += angle_step;
    }

    // Second pass: create all edges
    for (node1, node2, weight) in edges {
        let bin_index = ((weight - min_weight) / bin_width).floor() as usize;
        let bin_index = if bin_index >= num_bins {
            num_bins - 1 // Clamp the last bin
        } else {
            bin_index
        };

        bins[bin_index] += 1;

        let node1_index = node_indices[&node1];
        let node2_index = node_indices[&node2];
        // Only add edge if it doesn't already exist
        if !graph.contains_edge(node1_index, node2_index) {
            graph.add_edge(node1_index, node2_index, weight);
        }
    }
    if weighted {
        let mut histogram = histograph_ref.lock().unwrap();
        histogram.bin_width = bin_width;
        histogram.min = min_weight;
        histogram.max = max_weight;
        histogram.bins = bins;
    }
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    println!("{:?}", args);
    let graph_app = GraphVisualizerApp {
        weighted: args.weighted,
        ..GraphVisualizerApp::default()
    };
    let graph_ref = graph_app.graph_data.clone();
    let positions_ref = graph_app.positions.clone();
    let histogram_ref = graph_app.weight_histogram.clone();

    thread::spawn(move || {
        let input = if !atty::is(Stream::Stdin) {
            "stdin".to_string()
        } else {
            args.input
                .unwrap_or_else(|| panic!("Input must either be piped in or provided as a file"))
        };

        if input == "stdin" {
            let stdin = io::stdin();
            let reader = stdin.lock();
            let content = reader
                .lines()
                .filter_map(Result::ok)
                .collect::<Vec<String>>()
                .join("\n");
            parse_input(
                &graph_ref,
                &positions_ref,
                &histogram_ref,
                &content,
                args.weighted,
            );
        } else {
            if !Path::new(&input).exists() {
                panic!("File does not exist");
            }
            let file = File::open(input).unwrap();
            let reader = io::BufReader::new(file);
            let content = reader
                .lines()
                .filter_map(Result::ok)
                .collect::<Vec<String>>()
                .join("\n");
            parse_input(
                &graph_ref,
                &positions_ref,
                &histogram_ref,
                &content,
                args.weighted,
            );
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0]) // Wider default window
            .with_min_inner_size([400.0, 300.0]), // Set minimum size
        ..Default::default()
    };

    eframe::run_native(
        "Graph Visualizer",
        options,
        Box::new(|_cc| Ok(Box::new(graph_app))),
    )
}
