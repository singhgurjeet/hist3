use atty::Stream;
use clap::Parser;
use eframe::egui;
use egui::{Color32, Pos2, Stroke, Vec2};
use petgraph::graph::{Graph, NodeIndex};
use petgraph::Undirected;
use std::collections::HashMap;
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

#[derive(clap::Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Input file
    input: Option<String>,
}

struct GraphVisualizerApp {
    graph_data: Arc<Mutex<Graph<String, (), Undirected>>>,
    positions: Arc<Mutex<HashMap<NodeIndex, Pos2>>>,
    velocities: HashMap<NodeIndex, Vec2>,
    is_dragging: Option<NodeIndex>,
    running_simulation: bool,
}

impl Default for GraphVisualizerApp {
    fn default() -> Self {
        Self {
            graph_data: Arc::new(Mutex::new(Graph::new_undirected())),
            positions: Arc::new(Mutex::new(HashMap::new())),
            velocities: HashMap::new(),
            is_dragging: None,
            running_simulation: true,
        }
    }
}

impl eframe::App for GraphVisualizerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.running_simulation {
            self.update_layout();
            ctx.request_repaint();
        }

        // Add top panel for controls
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
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

            let pointer_pos = response.hover_pos();

            // Handle dragging
            if response.dragged() {
                if let Some(pos) = pointer_pos {
                    if let Some(node_idx) = self.is_dragging {
                        let mut positions = self.positions.lock().unwrap();
                        positions.insert(node_idx, pos);
                        self.velocities.insert(node_idx, Vec2::ZERO);
                    } else {
                        // Check if we started dragging a node
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

            // Draw the graph
            {
                let graph = self.graph_data.lock().unwrap();
                let positions = self.positions.lock().unwrap();

                // Draw edges first (behind nodes)
                for edge in graph.edge_indices() {
                    let (source, target) = graph.edge_endpoints(edge).unwrap();
                    if let (Some(&src_pos), Some(&tgt_pos)) =
                        (positions.get(&source), positions.get(&target))
                    {
                        painter.line_segment([src_pos, tgt_pos], Stroke::new(2.0, Color32::GRAY));
                    }
                }

                // Draw nodes
                for node_idx in graph.node_indices() {
                    if let Some(&position) = positions.get(&node_idx) {
                        if let Some(node) = graph.node_weight(node_idx) {
                            // Draw circle with border
                            painter.circle_stroke(position, 12.0, Stroke::new(2.0, Color32::WHITE));
                            painter.circle_filled(position, 10.0, Color32::from_rgb(0, 100, 255));

                            // Draw text
                            painter.text(
                                position,
                                egui::Align2::CENTER_CENTER,
                                node,
                                egui::FontId::proportional(14.0),
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
    fn reset_layout(&mut self) {
        let mut positions = self.positions.lock().unwrap();
        let graph = self.graph_data.lock().unwrap();
        let node_count = graph.node_count();

        // Place nodes in a larger circle
        for (i, node_idx) in graph.node_indices().enumerate() {
            let angle = 2.0 * std::f32::consts::PI * (i as f32) / (node_count as f32);
            let radius = 350.0; // Increased initial radius
            let center_x = 400.0;
            let center_y = 300.0;
            let x = radius * angle.cos() + center_x;
            let y = radius * angle.sin() + center_y;
            positions.insert(node_idx, Pos2::new(x, y));
        }

        // Add some random initial velocity to break symmetry
        self.velocities.clear();
        for node_idx in graph.node_indices() {
            let random_angle = (node_idx.index() as f32) * 0.1;
            let random_velocity = Vec2::new(random_angle.cos(), random_angle.sin()) * 2.0;
            self.velocities.insert(node_idx, random_velocity);
        }

        self.running_simulation = true;
    }

    fn update_layout(&mut self) {
        let graph = self.graph_data.lock().unwrap();
        let mut positions = self.positions.lock().unwrap();
        let mut forces: HashMap<NodeIndex, Vec2> = HashMap::new();
        let center = Vec2::new(400.0, 300.0);

        // Initialize forces with a small random component to break symmetry
        for node_idx in graph.node_indices() {
            let random_angle = (node_idx.index() as f32) * std::f32::consts::PI * 0.1;
            let random_force = Vec2::new(random_angle.cos(), random_angle.sin()) * 0.1;
            forces.insert(node_idx, random_force);
        }

        // Calculate repulsive forces between all nodes
        for node1 in graph.node_indices() {
            if self.is_dragging == Some(node1) {
                continue;
            }

            let mut total_force = Vec2::ZERO;

            // Repulsive forces from other nodes
            for node2 in graph.node_indices() {
                if node1 == node2 {
                    continue;
                }

                if let (Some(&pos1), Some(&pos2)) = (positions.get(&node1), positions.get(&node2)) {
                    let delta = pos1 - pos2;
                    let distance = delta.length().max(1.0);

                    // Stronger repulsion at close distances
                    let repulsion_strength = if distance < SPRING_LENGTH {
                        REPULSION_K * 2.0
                    } else {
                        REPULSION_K
                    };

                    let force = delta.normalized() * (repulsion_strength / distance.powi(2));
                    total_force += force;
                }
            }

            // Add centering force
            if let Some(&pos) = positions.get(&node1) {
                let to_center = center - Vec2::new(pos.x, pos.y);
                let center_distance = to_center.length();
                let center_force = to_center * (0.05 * (center_distance / 300.0).powi(2));
                total_force += center_force;
            }

            *forces.get_mut(&node1).unwrap() += total_force;
        }

        // Calculate attractive forces along edges
        for edge in graph.edge_indices() {
            let (node1, node2) = graph.edge_endpoints(edge).unwrap();
            if let (Some(&pos1), Some(&pos2)) = (positions.get(&node1), positions.get(&node2)) {
                let delta = pos1 - pos2;
                let distance = delta.length().max(1.0);

                // Stronger attraction for distant nodes
                let spring_k = if distance > SPRING_LENGTH * 2.0 {
                    SPRING_K * 2.0
                } else {
                    SPRING_K
                };

                let force = delta.normalized() * (distance - SPRING_LENGTH) * -spring_k;

                if self.is_dragging != Some(node1) {
                    *forces.get_mut(&node1).unwrap() += force;
                }
                if self.is_dragging != Some(node2) {
                    *forces.get_mut(&node2).unwrap() -= force;
                }
            }
        }

        // Update velocities and positions
        let mut max_movement = 0.0_f32;
        for node_idx in graph.node_indices() {
            if self.is_dragging == Some(node_idx) {
                continue;
            }

            let velocity = self.velocities.entry(node_idx).or_insert(Vec2::ZERO);
            let force = forces[&node_idx];

            // Update velocity with damping
            *velocity = (*velocity + force) * DAMPING;

            // Limit velocity
            if velocity.length() > MAX_VELOCITY {
                *velocity = velocity.normalized() * MAX_VELOCITY;
            }

            // Update position
            if let Some(pos) = positions.get_mut(&node_idx) {
                let old_pos = *pos;
                *pos = old_pos + *velocity;

                // Keep nodes within bounds with more padding
                pos.x = pos.x.clamp(150.0, 650.0);
                pos.y = pos.y.clamp(150.0, 450.0);

                max_movement = max_movement.max((*velocity).length());
            }
        }

        // Stop simulation if movement is very small
        if max_movement < MIN_MOVEMENT {
            self.running_simulation = false;
        }
    }
}

fn parse_input(
    graph_ref: &Arc<Mutex<Graph<String, (), Undirected>>>,
    positions_ref: &Arc<Mutex<HashMap<NodeIndex, Pos2>>>,
    input: &str,
) {
    let edges = input.lines().map(parse_edge).collect::<Vec<_>>();
    let mut graph = graph_ref.lock().unwrap();
    let mut positions = positions_ref.lock().unwrap();
    let mut node_indices = HashMap::new();

    let angle_step = 2.0 * std::f32::consts::PI / (edges.len().max(1) as f32);
    let mut angle = 0.0_f32;

    for (node1, node2) in edges {
        let node1_index = *node_indices.entry(node1.clone()).or_insert_with(|| {
            let index = graph.add_node(node1);
            let position = Pos2::new(angle.cos() * 200.0 + 400.0, angle.sin() * 200.0 + 300.0);
            positions.insert(index, position);
            angle += angle_step;
            index
        });

        let node2_index = *node_indices.entry(node2.clone()).or_insert_with(|| {
            let index = graph.add_node(node2);
            let position = Pos2::new(angle.cos() * 200.0 + 400.0, angle.sin() * 200.0 + 300.0);
            positions.insert(index, position);
            angle += angle_step;
            index
        });

        graph.add_edge(node1_index, node2_index, ());
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
            for line in reader.lines() {
                if let Ok(line) = line {
                    parse_input(&graph_ref, &positions_ref, &line);
                }
            }
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
