//! Fixed aperture and one moving coordinate layer. No routing or execution.
use std::time::{Duration, Instant};

use crate::model::{Item, TreeVariant};
use crate::state::{State, Status};

// Studio moving-tree-r1 geometry (styles.css/app.js): 96px rows, 264px nodes,
// 90px aperture (124px for the final preview), one 420ms ease-out translation.
pub const PITCH: f32 = 96.0;
pub const NODE_WIDTH: f32 = 264.0;
pub const APERTURE: f32 = 90.0;
pub const APERTURE_PREVIEW: f32 = 124.0;
pub const MOTION_MS: u64 = 420;

/// Tree stage width for a display: wide enough for query, fork and child
/// columns (three steps plus a node), never wider than the mockup's 1440px.
pub fn stage_width(bar_width: f32, display_width: f32) -> f32 {
    (display_width - 64.0).min(1440.0).max(bar_width)
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}
impl Position {
    pub fn plus(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

pub struct Node {
    pub item: Item,
    pub position: Position,
    pub index: Option<usize>,
    pub selected: bool,
    pub prior: bool,
}
pub struct Edge {
    pub from: Position,
    pub to: Position,
    pub chosen: bool,
}
pub struct Plan {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub target: Position,
    pub center: Position,
    pub node_width: f32,
    pub depth: usize,
    pub query: Option<Position>,
}

pub fn visible_range(count: usize, selected: usize) -> std::ops::Range<usize> {
    let start = selected.saturating_sub(2).min(count.saturating_sub(5));
    start..(start + 5).min(count)
}

pub fn slot(index: usize) -> f32 {
    match index {
        0 => 0.0,
        1 => -1.0,
        2 => 1.0,
        3 => 2.0,
        4 => -2.0,
        _ => index.div_ceil(2) as f32 * if index.is_multiple_of(2) { -1.0 } else { 1.0 },
    }
}
fn visual_order(count: usize) -> Vec<usize> {
    let mut order: Vec<_> = (0..count).collect();
    order.sort_by(|a, b| slot(*a).total_cmp(&slot(*b)));
    order
}
pub fn visible_indices(count: usize, selected: usize) -> Vec<usize> {
    let order = visual_order(count);
    let position = order
        .iter()
        .position(|index| *index == selected)
        .unwrap_or(0);
    order[visible_range(count, position)].to_vec()
}
pub fn neighbor(count: usize, selected: usize, delta: isize) -> usize {
    let order = visual_order(count);
    let position = order
        .iter()
        .position(|index| *index == selected)
        .unwrap_or(0);
    order
        .get((position as isize + delta).clamp(0, count.saturating_sub(1) as isize) as usize)
        .copied()
        .unwrap_or(0)
}

impl Plan {
    pub fn new(state: &State, variant: TreeVariant, width: f32, height: f32) -> Self {
        let depth = state
            .history
            .iter()
            .filter(|scope| !scope.items.is_empty())
            .count();
        let step = (width * 0.36).clamp(268.0, 380.0);
        let center = Position {
            x: width / 2.0,
            y: height / 2.0,
        };
        let mut plan = Self {
            nodes: vec![],
            edges: vec![],
            target: center,
            center,
            node_width: NODE_WIDTH,
            depth,
            query: (depth == 0 || variant != TreeVariant::Crumbs)
                .then_some(Position { x: -step, y: 0.0 }),
        };
        let base = plan.ancestors(state, variant, step);
        let focus_y = plan.current(
            state,
            Position {
                x: depth as f32 * step,
                y: base,
            },
        );
        plan.target = Position {
            x: center.x - depth as f32 * step,
            y: center.y - focus_y,
        };
        plan.connections(
            variant,
            Position {
                x: (depth as f32 - 1.0) * step,
                y: base,
            },
        );
        if variant == TreeVariant::Trail {
            plan.horizontal_trail(focus_y);
        }
        plan
    }
    fn ancestors(&mut self, state: &State, variant: TreeVariant, step: f32) -> f32 {
        let mut base = 0.0;
        for (column, scope) in state
            .history
            .iter()
            .filter(|scope| !scope.items.is_empty())
            .enumerate()
        {
            let indices = match variant {
                TreeVariant::Crumbs => vec![],
                TreeVariant::Trail => vec![scope.selected],
                TreeVariant::Faint => visible_indices(scope.items.len(), scope.selected),
            };
            for index in indices {
                self.nodes.push(Node {
                    item: scope.items[index].clone(),
                    position: Position {
                        x: column as f32 * step,
                        y: base + slot(index) * PITCH,
                    },
                    index: None,
                    selected: index == scope.selected,
                    prior: true,
                });
            }
            base += slot(scope.selected) * PITCH;
        }
        base
    }
    fn current(&mut self, state: &State, base: Position) -> f32 {
        if matches!(state.status, Status::Preview(_)) {
            if let Some(receipt) = &state.receipt {
                self.nodes.push(Node {
                    item: receipt.item.clone(),
                    position: base,
                    index: None,
                    selected: true,
                    prior: false,
                });
            }
            return base.y;
        }
        for index in visible_indices(state.items.len(), state.selected) {
            self.nodes.push(Node {
                item: state.items[index].clone(),
                position: base.plus(Position {
                    x: 0.0,
                    y: slot(index) * PITCH,
                }),
                index: Some(index),
                selected: index == state.selected,
                prior: false,
            });
        }
        base.y + slot(state.selected) * PITCH
    }
    fn connections(&mut self, variant: TreeVariant, parent: Position) {
        let parent = if self.depth == 0 {
            self.query
        } else {
            (variant != TreeVariant::Crumbs).then_some(parent)
        };
        if let Some(parent) = parent {
            self.edges.extend(
                self.nodes
                    .iter()
                    .filter(|node| !node.prior)
                    .map(|node| Edge {
                        from: parent,
                        to: node.position,
                        chosen: node.selected,
                    }),
            );
        }
    }
    fn horizontal_trail(&mut self, y: f32) {
        for node in self.nodes.iter_mut().filter(|node| node.prior) {
            node.position.y = y;
        }
        for edge in &mut self.edges {
            edge.from.y = y;
        }
        if let Some(query) = &mut self.query {
            query.y = y;
        }
    }
    pub fn selected_center(&self) -> Option<Position> {
        self.nodes
            .iter()
            .find(|node| node.selected && !node.prior)
            .map(|node| node.position.plus(self.target))
    }
}

/// Retarget from the currently rendered position, not an interrupted old target.
pub struct Motion {
    from: Position,
    target: Position,
    started: Instant,
    initialized: bool,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            from: Position::default(),
            target: Position::default(),
            started: Instant::now(),
            initialized: false,
        }
    }
}
impl Motion {
    pub fn retarget(&mut self, target: Position, now: Instant, reduced: bool) {
        if !self.initialized || reduced {
            self.from = target;
            self.target = target;
            self.initialized = true;
            self.started = now - Duration::from_millis(MOTION_MS);
        } else if target != self.target {
            self.from = self.position(now);
            self.target = target;
            self.started = now;
        }
    }
    pub fn settled(&self, now: Instant) -> bool {
        self.from == self.target
            || now.duration_since(self.started) >= Duration::from_millis(MOTION_MS)
    }
    pub fn position(&self, now: Instant) -> Position {
        let t = (now.duration_since(self.started).as_secs_f32() / (MOTION_MS as f32 / 1000.0))
            .clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        Position {
            x: self.from.x + (self.target.x - self.from.x) * eased,
            y: self.from.y + (self.target.y - self.from.y) * eased,
        }
    }
}
