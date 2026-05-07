use crate::kernel::ids::PaneId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PaneTree {
    root: PaneNode,
}

impl PaneTree {
    pub fn single(pane_id: PaneId) -> Self {
        Self {
            root: PaneNode::Leaf(pane_id),
        }
    }

    #[allow(dead_code)]
    pub fn root(&self) -> &PaneNode {
        &self.root
    }

    pub fn contains_pane(&self, pane_id: PaneId) -> bool {
        self.root.contains_pane(pane_id)
    }

    pub fn leaves(&self) -> Vec<PaneId> {
        let mut leaves = Vec::new();
        self.root.collect_leaves(&mut leaves);
        leaves
    }

    pub fn split_leaf(&mut self, target: PaneId, new_pane: PaneId, axis: SplitAxis) -> bool {
        self.root.split_leaf(target, new_pane, axis)
    }

    pub fn adjacent_pane(&self, target: PaneId, direction: PaneDirection) -> Option<PaneId> {
        self.root.adjacent_pane(target, direction)
    }

    pub fn resize_pane(
        &mut self,
        target: PaneId,
        direction: PaneDirection,
        delta_percent: u8,
    ) -> bool {
        self.root.resize_pane(target, direction, delta_percent)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PaneNode {
    Leaf(PaneId),
    Split {
        axis: SplitAxis,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
        ratio_percent: u8,
    },
}

impl PaneNode {
    fn resize_pane(&mut self, target: PaneId, direction: PaneDirection, delta_percent: u8) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                axis,
                first,
                second,
                ratio_percent,
            } => {
                if first.contains_pane(target) {
                    if first.resize_pane(target, direction, delta_percent) {
                        return true;
                    }

                    if direction.exits_first(*axis) {
                        *ratio_percent = ratio_percent.saturating_add(delta_percent).clamp(10, 90);
                        return true;
                    }
                } else if second.contains_pane(target) {
                    if second.resize_pane(target, direction, delta_percent) {
                        return true;
                    }

                    if direction.exits_second(*axis) {
                        *ratio_percent = ratio_percent.saturating_sub(delta_percent).clamp(10, 90);
                        return true;
                    }
                }

                false
            }
        }
    }

    fn adjacent_pane(&self, target: PaneId, direction: PaneDirection) -> Option<PaneId> {
        match self {
            Self::Leaf(_) => None,
            Self::Split {
                axis,
                first,
                second,
                ..
            } => {
                if first.contains_pane(target) {
                    if let Some(pane_id) = first.adjacent_pane(target, direction) {
                        return Some(pane_id);
                    }

                    if direction.exits_first(*axis) {
                        return Some(second.entry_leaf(direction));
                    }
                } else if second.contains_pane(target) {
                    if let Some(pane_id) = second.adjacent_pane(target, direction) {
                        return Some(pane_id);
                    }

                    if direction.exits_second(*axis) {
                        return Some(first.entry_leaf(direction));
                    }
                }

                None
            }
        }
    }

    fn contains_pane(&self, pane_id: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == pane_id,
            Self::Split { first, second, .. } => {
                first.contains_pane(pane_id) || second.contains_pane(pane_id)
            }
        }
    }

    fn collect_leaves(&self, leaves: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(id) => leaves.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_leaves(leaves);
                second.collect_leaves(leaves);
            }
        }
    }

    fn split_leaf(&mut self, target: PaneId, new_pane: PaneId, axis: SplitAxis) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Split {
                    axis,
                    first: Box::new(Self::Leaf(target)),
                    second: Box::new(Self::Leaf(new_pane)),
                    ratio_percent: 50,
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split_leaf(target, new_pane, axis)
                    || second.split_leaf(target, new_pane, axis)
            }
        }
    }

    fn entry_leaf(&self, direction: PaneDirection) -> PaneId {
        match self {
            Self::Leaf(id) => *id,
            Self::Split { first, second, .. } => match direction {
                PaneDirection::Right | PaneDirection::Down => first.entry_leaf(direction),
                PaneDirection::Left | PaneDirection::Up => second.entry_leaf(direction),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

impl PaneDirection {
    fn exits_first(self, axis: SplitAxis) -> bool {
        matches!(
            (self, axis),
            (Self::Right, SplitAxis::Vertical) | (Self::Down, SplitAxis::Horizontal)
        )
    }

    fn exits_second(self, axis: SplitAxis) -> bool {
        matches!(
            (self, axis),
            (Self::Left, SplitAxis::Vertical) | (Self::Up, SplitAxis::Horizontal)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_leaf_adds_a_second_pane_without_nesting_panes() {
        let mut tree = PaneTree::single(PaneId::new(1));

        assert!(tree.split_leaf(PaneId::new(1), PaneId::new(2), SplitAxis::Horizontal));

        assert!(tree.contains_pane(PaneId::new(1)));
        assert!(tree.contains_pane(PaneId::new(2)));
        assert_eq!(tree.leaves(), vec![PaneId::new(1), PaneId::new(2)]);
    }

    #[test]
    fn adjacent_pane_uses_split_axes_for_directional_navigation() {
        let mut tree = PaneTree::single(PaneId::new(1));
        assert!(tree.split_leaf(PaneId::new(1), PaneId::new(2), SplitAxis::Vertical));
        assert!(tree.split_leaf(PaneId::new(2), PaneId::new(3), SplitAxis::Horizontal));

        assert_eq!(
            tree.adjacent_pane(PaneId::new(1), PaneDirection::Right),
            Some(PaneId::new(2))
        );
        assert_eq!(
            tree.adjacent_pane(PaneId::new(2), PaneDirection::Left),
            Some(PaneId::new(1))
        );
        assert_eq!(
            tree.adjacent_pane(PaneId::new(2), PaneDirection::Down),
            Some(PaneId::new(3))
        );
        assert_eq!(
            tree.adjacent_pane(PaneId::new(3), PaneDirection::Up),
            Some(PaneId::new(2))
        );
        assert_eq!(
            tree.adjacent_pane(PaneId::new(1), PaneDirection::Left),
            None
        );
    }

    #[test]
    fn resize_pane_adjusts_matching_split_ratio() {
        let mut tree = PaneTree::single(PaneId::new(1));
        assert!(tree.split_leaf(PaneId::new(1), PaneId::new(2), SplitAxis::Vertical));

        assert!(tree.resize_pane(PaneId::new(1), PaneDirection::Right, 10));

        assert!(matches!(
            tree.root(),
            PaneNode::Split {
                axis: SplitAxis::Vertical,
                ratio_percent: 60,
                ..
            }
        ));
    }

    #[test]
    fn resize_pane_clamps_ratio_within_bounds() {
        let mut tree = PaneTree::single(PaneId::new(1));
        assert!(tree.split_leaf(PaneId::new(1), PaneId::new(2), SplitAxis::Vertical));

        assert!(tree.resize_pane(PaneId::new(1), PaneDirection::Right, 50));

        assert!(matches!(
            tree.root(),
            PaneNode::Split {
                ratio_percent: 90,
                ..
            }
        ));
    }
}
