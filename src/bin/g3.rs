use atty::Stream;
use clap::Parser;
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

const SPRING_LENGTH: f32 = 200.0; // Increased even more for better spacing
const SPRING_K: f32 = 0.05; // Reduced spring force for more stability
const REPULSION_K: f32 = 50000.0; // Increased repulsion significantly
const DAMPING: f32 = 0.7; // Increased damping to prevent oscillation
const MAX_VELOCITY: f32 = 20.0; // Increased max velocity
const MIN_MOVEMENT: f32 = 0.5; // Increased minimum movement threshold
const COMPONENT_SPACING: f32 = 400.0; // Minimum spacing between components

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,
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
    graph_data: Arc<Mutex<Graph<String, (), Undirected>>>,
    positions: Arc<Mutex<HashMap<NodeIndex, Pos2>>>,
    velocities: HashMap<NodeIndex, Vec2>,
    is_dragging: Option<NodeIndex>,
    running_simulation: bool,
    components: Vec<Vec<NodeIndex>>,
    initialized: bool,
    zoom_level: f32,
    pan_offset: Vec2,
    viewport_bounds: Option<(Pos2, Pos2)>,
    interaction_mode: InteractionMode,
    selection_state: SelectionState,
}

impl Default for GraphVisualizerApp {
    fn default() -> Self {
        Self {
            graph_data: Arc::new(Mutex::new(Graph::new_undirected())),
            positions: Arc::new(Mutex::new(HashMap::new())),
            velocities: HashMap::new(),
            is_dragging: None,
            running_simulation: true,
            components: Vec::new(),
            initialized: false,
            zoom_level: 1.0,
            pan_offset: Vec2::ZERO,
            viewport_bounds: None,
            interaction_mode: InteractionMode::Pan,
            selection_state: SelectionState::default(),
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
                self.reset_layout();
                self.initialized = true;
            }
        }

        if self.running_simulation {
            self.update_layout();
            ctx.request_repaint();
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
                    self.reset_layout();
                }
                if ui.button("🔍 Fit to View").clicked() {
                    self.fit_to_view(window_size);
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

                ui.label(format!("Zoom: {:.1}x", self.zoom_level));
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
                } else if response.drag_released() {
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
                } else if response.drag_released() {
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
                            positions.insert(node_idx, pos);
                            self.velocities.insert(node_idx, Vec2::ZERO);
                        } else {
                            let positions = self.positions.lock().unwrap();
                            for (idx, &node_pos) in positions.iter() {
                                if node_pos.distance(pos) < 15.0 {
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
                    if let (Some(&src_pos), Some(&tgt_pos)) =
                        (positions.get(&source), positions.get(&target))
                    {
                        let screen_src = self.graph_to_screen_pos(src_pos);
                        let screen_tgt = self.graph_to_screen_pos(tgt_pos);
                        painter.line_segment(
                            [screen_src, screen_tgt],
                            Stroke::new(2.0 * self.zoom_level, Color32::GRAY),
                        );
                    }
                }

                // Draw nodes
                for node_idx in graph.node_indices() {
                    if let Some(&position) = positions.get(&node_idx) {
                        let screen_pos = self.graph_to_screen_pos(position);
                        if let Some(node) = graph.node_weight(node_idx) {
                            let node_radius = 12.0 * self.zoom_level;
                            let stroke_width = 2.0 * self.zoom_level;

                            // Determine node color based on selection and preview state
                            let node_color =
                                if self.selection_state.selected_nodes.contains(&node_idx) {
                                    Color32::from_rgb(0, 255, 0) // Green for selected nodes
                                } else if self.selection_state.preview_nodes.contains(&node_idx) {
                                    Color32::from_rgb(0, 200, 100) // Light green for preview selection
                                } else {
                                    Color32::from_rgb(0, 100, 255) // Original blue for unselected nodes
                                };

                            painter.circle_stroke(
                                screen_pos,
                                node_radius,
                                Stroke::new(stroke_width, Color32::WHITE),
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
            }
        });
    }
}
impl GraphVisualizerApp {
    fn update_preview_selection(&mut self, modifiers: egui::Modifiers) {
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

        // Update zoom level with tighter clamping
        self.zoom_level = (self.zoom_level * zoom_factor).clamp(0.5, 5.0); // Increased minimum zoom

        // If the zoom level didn't change (due to clamping), don't update the pan offset
        if (self.zoom_level - old_zoom).abs() < f32::EPSILON {
            return;
        }

        // Adjust pan offset to keep the point under the cursor stationary
        let center_vec = Vec2::new(center_pos.x, center_pos.y);
        self.pan_offset =
            center_vec + (self.pan_offset - center_vec) * (self.zoom_level / old_zoom);
    }

    fn fit_to_view(&mut self, available_size: Vec2) {
        let positions = self.positions.lock().unwrap();
        if positions.is_empty() {
            return;
        }

        // Calculate bounds
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for &pos in positions.values() {
            min_x = min_x.min(pos.x);
            min_y = min_y.min(pos.y);
            max_x = max_x.max(pos.x);
            max_y = max_y.max(pos.y);
        }

        // Ensure we have valid dimensions
        if min_x.is_infinite() || min_y.is_infinite() || max_x.is_infinite() || max_y.is_infinite()
        {
            println!("Invalid bounds detected");
            return;
        }

        // Add percentage-based padding
        let width = max_x - min_x;
        let height = max_y - min_y;

        if width <= 0.0 || height <= 0.0 {
            println!("Invalid dimensions: width={}, height={}", width, height);
            return;
        }

        let padding_percent = 0.1; // 10% padding
        let padded_width = width * (1.0 + 2.0 * padding_percent);
        let padded_height = height * (1.0 + 2.0 * padding_percent);

        // Calculate the required zoom level
        let zoom_x = available_size.x / padded_width;
        let zoom_y = available_size.y / padded_height;

        // Use the smaller zoom factor and ensure it's within tighter bounds
        let new_zoom = zoom_x.min(zoom_y).clamp(0.5, 5.0); // Increased minimum zoom

        if new_zoom.is_finite() && new_zoom > 0.0 {
            self.zoom_level = new_zoom;

            // Calculate the center points
            let graph_center_x = (min_x + max_x) / 2.0;
            let graph_center_y = (min_y + max_y) / 2.0;
            let screen_center_x = available_size.x / 2.0;
            let screen_center_y = available_size.y / 2.0;

            // Update pan offset to center the graph
            self.pan_offset = Vec2::new(
                screen_center_x - (graph_center_x * self.zoom_level),
                screen_center_y - (graph_center_y * self.zoom_level),
            );
        }
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

    fn reset_layout(&mut self) {
        self.find_components();
        let mut positions = self.positions.lock().unwrap();

        // Calculate grid layout for components
        let components_per_row = (self.components.len() as f32).sqrt().ceil() as usize;

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

        self.running_simulation = true;
    }

    fn update_layout(&mut self) {
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

                            let force = delta.normalized() * (distance - SPRING_LENGTH) * -spring_k;

                            if self.is_dragging != Some(node1) {
                                *forces.get_mut(&node1).unwrap() += force;
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
                let force = forces[&node_idx];

                // Update velocity with damping
                *velocity = (*velocity + force) * DAMPING;

                // Subtract average component velocity to prevent drift
                *velocity -= avg_velocity;

                // Limit velocity
                if velocity.length() > MAX_VELOCITY {
                    *velocity = velocity.normalized() * MAX_VELOCITY;
                }

                // Update position
                if let Some(pos) = positions.get_mut(&node_idx) {
                    let old_pos = *pos;
                    *pos = old_pos + *velocity;

                    // Keep nodes within reasonable bounds
                    let max_x = COMPONENT_SPACING * (self.components.len() as f32);
                    let max_y = COMPONENT_SPACING * (self.components.len() as f32);
                    pos.x = pos.x.clamp(100.0, max_x);
                    pos.y = pos.y.clamp(100.0, max_y);

                    max_movement = max_movement.max((*velocity).length());
                }
            }
        }

        // Stop simulation if movement is very small
        if max_movement < MIN_MOVEMENT {
            self.running_simulation = false;
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
fn parse_input(
    graph_ref: &Arc<Mutex<Graph<String, (), Undirected>>>,
    positions_ref: &Arc<Mutex<HashMap<NodeIndex, Pos2>>>,
    input: &str,
) {
    // First, collect all unique nodes and edges
    let mut unique_nodes = HashSet::new();
    let mut edges = Vec::new();

    for line in input.lines() {
        let (node1, node2) = parse_edge(line);
        unique_nodes.insert(node1.clone());
        unique_nodes.insert(node2.clone());
        edges.push((node1, node2));
    }

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
    for (node1, node2) in edges {
        let node1_index = node_indices[&node1];
        let node2_index = node_indices[&node2];
        // Only add edge if it doesn't already exist
        if !graph.contains_edge(node1_index, node2_index) {
            graph.add_edge(node1_index, node2_index, ());
        }
    }
}

fn parse_edge(line: &str) -> (String, String) {
    let mut nodes = line.split_whitespace();
    let node1 = nodes.next().unwrap_or_default().to_string();
    let node2 = nodes.next().unwrap_or_default().to_string();
    (node1, node2)
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();
    let graph_app = GraphVisualizerApp::default();
    let graph_ref = graph_app.graph_data.clone();
    let positions_ref = graph_app.positions.clone();

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
            parse_input(&graph_ref, &positions_ref, &content);
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
            parse_input(&graph_ref, &positions_ref, &content);
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Graph Visualizer",
        options,
        Box::new(|_cc| Ok(Box::new(graph_app))),
    )
}
