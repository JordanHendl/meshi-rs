use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DockSplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockTab {
    pub id: u64,
    pub title: String,
}

impl DockTab {
    pub fn new(id: u64, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockTabGroup {
    pub id: u64,
    pub tabs: Vec<DockTab>,
    pub active: usize,
}

impl DockTabGroup {
    pub fn new(id: u64, tabs: Vec<DockTab>) -> Self {
        Self {
            id,
            tabs,
            active: 0,
        }
    }

    pub fn active_index(&self) -> Option<usize> {
        if self.tabs.is_empty() {
            None
        } else {
            Some(self.active.min(self.tabs.len().saturating_sub(1)))
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DockNode {
    Split {
        direction: DockSplitDirection,
        ratio: f32,
        first: Box<DockNode>,
        second: Box<DockNode>,
    },
    Tabs {
        group: DockTabGroup,
    },
}

impl PartialEq for DockNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                DockNode::Split {
                    direction: d1,
                    first: f1,
                    second: s1,
                    ..
                },
                DockNode::Split {
                    direction: d2,
                    first: f2,
                    second: s2,
                    ..
                },
            ) => d1 == d2 && f1 == f2 && s1 == s2,

            (
                DockNode::Tabs { group: g1 },
                DockNode::Tabs { group: g2 },
            ) => g1 == g2,

            _ => false,
        }
    }
}

impl Eq for DockNode {}

impl DockNode {
    pub fn tabs(group: DockTabGroup) -> Self {
        Self::Tabs { group }
    }

    pub fn split(
        direction: DockSplitDirection,
        ratio: f32,
        first: DockNode,
        second: DockNode,
    ) -> Self {
        Self::Split {
            direction,
            ratio: ratio.clamp(0.05, 0.95),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub fn swap_groups(&mut self, first_id: u64, second_id: u64) -> bool {
        if first_id == second_id {
            return false;
        }

        let placeholder = DockTabGroup::new(first_id, Vec::new());
        let first_group = self.replace_group(first_id, placeholder);
        let second_group = self.replace_group(second_id, DockTabGroup::new(second_id, Vec::new()));

        match (first_group, second_group) {
            (Some(first_group), Some(second_group)) => {
                self.replace_group(first_id, second_group);
                self.replace_group(second_id, first_group);
                true
            }
            (Some(first_group), None) => {
                self.replace_group(first_id, first_group);
                false
            }
            (None, Some(second_group)) => {
                self.replace_group(second_id, second_group);
                false
            }
            (None, None) => false,
        }
    }

    fn replace_group(&mut self, target_id: u64, replacement: DockTabGroup) -> Option<DockTabGroup> {
        match self {
            DockNode::Tabs { group } if group.id == target_id => {
                let old = group.clone();
                *group = replacement;
                Some(old)
            }
            DockNode::Split { first, second, .. } => {
                if let Some(found) = first.replace_group(target_id, replacement.clone()) {
                    return Some(found);
                }
                second.replace_group(target_id, replacement)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DockLayoutTree {
    pub root: DockNode,
}

impl DockLayoutTree {
    pub fn new(root: DockNode) -> Self {
        Self { root }
    }

    pub fn serialize(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn deserialize(data: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(data)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockRect {
    pub min: Vec2,
    pub max: Vec2,
}

impl DockRect {
    pub fn from_position_size(position: Vec2, size: Vec2) -> Self {
        Self {
            min: position,
            max: position + size,
        }
    }

    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockTabLayout {
    pub tab_id: u64,
    pub rect: DockRect,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockGroupLayout {
    pub group_id: u64,
    pub tab_bar: DockRect,
    pub content: DockRect,
    pub tabs: Vec<DockTabLayout>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct DockLayout {
    pub groups: Vec<DockGroupLayout>,
}

impl DockLayout {
    pub fn group_at_tab_bar(&self, point: Vec2) -> Option<u64> {
        self.groups
            .iter()
            .find(|group| group.tab_bar.contains(point))
            .map(|group| group.group_id)
    }

    pub fn group_layout(&self, group_id: u64) -> Option<&DockGroupLayout> {
        self.groups.iter().find(|group| group.group_id == group_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DockTabDragState {
    group_id: u64,
    start: Vec2,
}

pub struct DockArea {
    pub tree: DockLayoutTree,
    pub tab_bar_height: f32,
    drag_state: Option<DockTabDragState>,
}

impl DockArea {
    pub fn new(tree: DockLayoutTree) -> Self {
        Self {
            tree,
            tab_bar_height: 28.0,
            drag_state: None,
        }
    }

    pub fn layout(&self, rect: DockRect) -> DockLayout {
        let mut layout = DockLayout::default();
        layout_node(
            &self.tree.root,
            rect,
            self.tab_bar_height,
            &mut layout.groups,
        );
        layout
    }

    pub fn begin_tab_drag(&mut self, cursor: Vec2, layout: &DockLayout) -> bool {
        if let Some(group_id) = layout.group_at_tab_bar(cursor) {
            self.drag_state = Some(DockTabDragState {
                group_id,
                start: cursor,
            });
            return true;
        }
        false
    }

    pub fn is_dragging_tabs(&self) -> bool {
        self.drag_state.is_some()
    }

    pub fn cancel_tab_drag(&mut self) {
        self.drag_state = None;
    }

    pub fn end_tab_drag(&mut self, cursor: Vec2, layout: &DockLayout) -> bool {
        let Some(drag_state) = self.drag_state.take() else {
            return false;
        };

        let Some(target_group) = layout.group_at_tab_bar(cursor) else {
            return false;
        };

        if target_group == drag_state.group_id {
            return false;
        }

        self.tree
            .root
            .swap_groups(drag_state.group_id, target_group)
    }
}

fn layout_node(node: &DockNode, rect: DockRect, tab_bar_height: f32, output: &mut Vec<DockGroupLayout>) {
    match node {
        DockNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let size = rect.size();
            match direction {
                DockSplitDirection::Horizontal => {
                    let first_width = size.x * ratio.clamp(0.05, 0.95);
                    let first_rect = DockRect::from_position_size(rect.min, vec2(first_width, size.y));
                    let second_rect = DockRect::from_position_size(
                        vec2(rect.min.x + first_width, rect.min.y),
                        vec2(size.x - first_width, size.y),
                    );
                    layout_node(first, first_rect, tab_bar_height, output);
                    layout_node(second, second_rect, tab_bar_height, output);
                }
                DockSplitDirection::Vertical => {
                    let first_height = size.y * ratio.clamp(0.05, 0.95);
                    let first_rect = DockRect::from_position_size(rect.min, vec2(size.x, first_height));
                    let second_rect = DockRect::from_position_size(
                        vec2(rect.min.x, rect.min.y + first_height),
                        vec2(size.x, size.y - first_height),
                    );
                    layout_node(first, first_rect, tab_bar_height, output);
                    layout_node(second, second_rect, tab_bar_height, output);
                }
            }
        }
        DockNode::Tabs { group } => {
            let size = rect.size();
            let tab_height = tab_bar_height.min(size.y.max(0.0));
            let tab_bar = DockRect::from_position_size(rect.min, vec2(size.x, tab_height));
            let content = DockRect::from_position_size(
                vec2(rect.min.x, rect.min.y + tab_height),
                vec2(size.x, (size.y - tab_height).max(0.0)),
            );

            let mut tabs = Vec::with_capacity(group.tabs.len());
            let tab_count = group.tabs.len().max(1) as f32;
            let tab_width = (size.x / tab_count).max(1.0);
            let active = group.active_index();
            for (index, tab) in group.tabs.iter().enumerate() {
                let tab_rect = DockRect::from_position_size(
                    vec2(rect.min.x + tab_width * index as f32, rect.min.y),
                    vec2(tab_width, tab_height),
                );
                tabs.push(DockTabLayout {
                    tab_id: tab.id,
                    rect: tab_rect,
                    selected: active.map_or(false, |active| active == index),
                });
            }

            output.push(DockGroupLayout {
                group_id: group.id,
                tab_bar,
                content,
                tabs,
            });
        }
    }
}

fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}
