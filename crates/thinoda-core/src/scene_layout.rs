//! Bounded geometry preview primitives for shared scene-container layout.
//!
//! This module defines typed bounded policies and pure geometry previews.
//! The scene hierarchy owns policy persistence and commits accepted previews;
//! routing presentation remains outside this module.

use crate::board::{MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS};
use crate::geometry::{Rect, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StackAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StackCrossAxisPolicy {
    #[default]
    Preserve,
    Fill,
}

impl StackCrossAxisPolicy {
    fn is_preserve(&self) -> bool {
        *self == Self::Preserve
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderSide {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderPolicy {
    pub side: HeaderSide,
    pub extent: f32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StackResizePolicy {
    #[default]
    Preserve,
    FillLast,
    GrowParent,
    FitParent,
    Distribute,
}

impl StackResizePolicy {
    fn is_preserve(&self) -> bool {
        *self == Self::Preserve
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ContainerLayout {
    Free,
    Stack {
        axis: StackAxis,
        spacing: f32,
        padding: f32,
        #[serde(default, skip_serializing_if = "StackCrossAxisPolicy::is_preserve")]
        cross_axis: StackCrossAxisPolicy,
        #[serde(default, skip_serializing_if = "StackResizePolicy::is_preserve")]
        resize: StackResizePolicy,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerLayoutPolicy {
    pub header: HeaderPolicy,
    pub layout: ContainerLayout,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerLayoutPreview {
    pub parent: Rect,
    pub children: Vec<Rect>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SceneLayoutError {
    #[error("scene layout geometry must be finite and within board limits")]
    InvalidGeometry,
    #[error("scene layout policy values must be finite and non-negative")]
    InvalidPolicy,
    #[error("scene layout children do not fit within the container body")]
    ExceedsBody,
    #[error("parent resizing requires the full container layout preview")]
    ParentResizeRequiresContainerPreview,
    #[error("scene layout child count exceeds canvas limits")]
    Limit,
}

/// Returns proposed sheet-space rectangles in the supplied sibling order.
///
/// Stack layout aligns children to the leading cross-axis edge and rejects
/// overflow instead of clipping. `FillLast` may grow only the final child's
/// primary extent; `Distribute` shares the available primary-axis space evenly
/// across inter-child gaps. `GrowParent` expands the parent while `FitParent`
/// sizes its primary axis to content. Their cross-axis size remains fixed when
/// children fill that axis; otherwise child sizes are preserved. Free layout
/// never changes child geometry. This primitive is intentionally one-container-
/// only: it neither mutates geometry nor routes connectors; policy persistence
/// lives in the scene hierarchy.
pub fn preview_child_layout(
    parent: Rect,
    policy: ContainerLayoutPolicy,
    children: &[Rect],
) -> Result<Vec<Rect>, SceneLayoutError> {
    let preview = preview_container_layout(parent, policy, children)?;
    if preview.parent.size != parent.size {
        return Err(SceneLayoutError::ParentResizeRequiresContainerPreview);
    }
    Ok(preview.children)
}

/// Returns the parent frame and proposed child frames for a bounded layout.
pub fn preview_container_layout(
    parent: Rect,
    policy: ContainerLayoutPolicy,
    children: &[Rect],
) -> Result<ContainerLayoutPreview, SceneLayoutError> {
    if children.len() > MAX_CANVAS_OBJECTS {
        return Err(SceneLayoutError::Limit);
    }
    validate_rect(parent)?;
    for child in children {
        validate_rect(*child)?;
    }
    if !policy.header.extent.is_finite() || policy.header.extent < 0.0 {
        return Err(SceneLayoutError::InvalidPolicy);
    }
    let parent = match policy.layout {
        ContainerLayout::Stack {
            axis,
            spacing,
            padding,
            cross_axis,
            resize: StackResizePolicy::GrowParent,
        } => grow_parent_to_fit(
            parent,
            policy.header,
            axis,
            spacing,
            padding,
            cross_axis,
            children,
        )?,
        ContainerLayout::Stack {
            axis,
            spacing,
            padding,
            cross_axis,
            resize: StackResizePolicy::FitParent,
        } => fit_parent_to_contents(
            parent,
            policy.header,
            axis,
            spacing,
            padding,
            cross_axis,
            children,
        )?,
        ContainerLayout::Free
        | ContainerLayout::Stack {
            resize:
                StackResizePolicy::Preserve
                | StackResizePolicy::FillLast
                | StackResizePolicy::Distribute,
            ..
        } => parent,
    };
    let body = body_rect(parent, policy.header)?;

    let children = match policy.layout {
        ContainerLayout::Free => children.to_vec(),
        ContainerLayout::Stack {
            axis,
            spacing,
            padding,
            cross_axis,
            resize,
        } => {
            if !spacing.is_finite() || spacing < 0.0 || !padding.is_finite() || padding < 0.0 {
                return Err(SceneLayoutError::InvalidPolicy);
            }
            let (min_x, min_y, width, height) = (
                f64::from(body.min.x) + f64::from(padding),
                f64::from(body.min.y) + f64::from(padding),
                f64::from(body.size.x) - 2.0 * f64::from(padding),
                f64::from(body.size.y) - 2.0 * f64::from(padding),
            );
            if width < 0.0 || height < 0.0 {
                return Err(SceneLayoutError::ExceedsBody);
            }

            let primary_extent = match axis {
                StackAxis::Horizontal => width,
                StackAxis::Vertical => height,
            };
            let cross_extent = match axis {
                StackAxis::Horizontal => height,
                StackAxis::Vertical => width,
            };
            let used_primary = children.iter().try_fold(0.0_f64, |used, child| {
                let primary = match axis {
                    StackAxis::Horizontal => child.size.x,
                    StackAxis::Vertical => child.size.y,
                };
                let cross = match axis {
                    StackAxis::Horizontal => child.size.y,
                    StackAxis::Vertical => child.size.x,
                };
                if cross_axis == StackCrossAxisPolicy::Preserve && f64::from(cross) > cross_extent {
                    return None;
                }
                Some(used + f64::from(primary))
            });
            let Some(used_primary) = used_primary else {
                return Err(SceneLayoutError::ExceedsBody);
            };
            let gaps = children.len().saturating_sub(1);
            let minimum_used_primary = used_primary + f64::from(spacing) * gaps as f64;
            if minimum_used_primary > primary_extent {
                return Err(SceneLayoutError::ExceedsBody);
            }
            let primary_spacing = if resize == StackResizePolicy::Distribute && gaps > 0 {
                (primary_extent - used_primary) / gaps as f64
            } else {
                f64::from(spacing)
            };

            let mut cursor = match axis {
                StackAxis::Horizontal => min_x,
                StackAxis::Vertical => min_y,
            };
            let primary_start = cursor;
            let mut result = Vec::with_capacity(children.len());
            for (index, child) in children.iter().enumerate() {
                let size = if resize == StackResizePolicy::FillLast && index + 1 == children.len() {
                    let remaining = primary_extent - (cursor - primary_start);
                    match axis {
                        StackAxis::Horizontal => Vec2::new(
                            remaining as f32,
                            if cross_axis == StackCrossAxisPolicy::Fill {
                                cross_extent as f32
                            } else {
                                child.size.y
                            },
                        ),
                        StackAxis::Vertical => Vec2::new(
                            if cross_axis == StackCrossAxisPolicy::Fill {
                                cross_extent as f32
                            } else {
                                child.size.x
                            },
                            remaining as f32,
                        ),
                    }
                } else if cross_axis == StackCrossAxisPolicy::Fill {
                    match axis {
                        StackAxis::Horizontal => Vec2::new(child.size.x, cross_extent as f32),
                        StackAxis::Vertical => Vec2::new(cross_extent as f32, child.size.y),
                    }
                } else {
                    child.size
                };
                let min = match axis {
                    StackAxis::Horizontal => Vec2::new(cursor as f32, min_y as f32),
                    StackAxis::Vertical => Vec2::new(min_x as f32, cursor as f32),
                };
                let rect = Rect::from_min_size(min, size);
                validate_rect(rect)?;
                result.push(rect);
                cursor += match axis {
                    StackAxis::Horizontal => f64::from(size.x),
                    StackAxis::Vertical => f64::from(size.y),
                } + primary_spacing;
            }
            result
        }
    };
    Ok(ContainerLayoutPreview { parent, children })
}

fn grow_parent_to_fit(
    parent: Rect,
    header: HeaderPolicy,
    axis: StackAxis,
    spacing: f32,
    padding: f32,
    cross_axis: StackCrossAxisPolicy,
    children: &[Rect],
) -> Result<Rect, SceneLayoutError> {
    let minimum =
        minimum_parent_size(parent, header, axis, spacing, padding, cross_axis, children)?;
    let grown = Rect::from_min_size(
        parent.min,
        Vec2::new(parent.size.x.max(minimum.x), parent.size.y.max(minimum.y)),
    );
    validate_rect(grown)?;
    Ok(grown)
}

fn fit_parent_to_contents(
    parent: Rect,
    header: HeaderPolicy,
    axis: StackAxis,
    spacing: f32,
    padding: f32,
    cross_axis: StackCrossAxisPolicy,
    children: &[Rect],
) -> Result<Rect, SceneLayoutError> {
    let fitted = Rect::from_min_size(
        parent.min,
        minimum_parent_size(parent, header, axis, spacing, padding, cross_axis, children)?,
    );
    validate_rect(fitted)?;
    Ok(fitted)
}

fn minimum_parent_size(
    parent: Rect,
    header: HeaderPolicy,
    axis: StackAxis,
    spacing: f32,
    padding: f32,
    cross_axis: StackCrossAxisPolicy,
    children: &[Rect],
) -> Result<Vec2, SceneLayoutError> {
    if !spacing.is_finite() || spacing < 0.0 || !padding.is_finite() || padding < 0.0 {
        return Err(SceneLayoutError::InvalidPolicy);
    }
    let gaps = children.len().saturating_sub(1);
    let total_primary = children.iter().try_fold(0.0_f64, |sum, child| {
        let extent = match axis {
            StackAxis::Horizontal => child.size.x,
            StackAxis::Vertical => child.size.y,
        };
        let extent = f64::from(extent);
        (extent.is_finite()).then_some(sum + extent)
    });
    let Some(total_primary) = total_primary else {
        return Err(SceneLayoutError::InvalidGeometry);
    };
    let total_primary = total_primary + f64::from(spacing) * gaps as f64;
    let cross_extent = children
        .iter()
        .map(|child| match axis {
            StackAxis::Horizontal => child.size.y,
            StackAxis::Vertical => child.size.x,
        })
        .fold(0.0_f32, f32::max);
    if cross_axis == StackCrossAxisPolicy::Fill {
        return Ok(match axis {
            StackAxis::Horizontal => Vec2::new(
                f32_at_least(
                    total_primary
                        + 2.0 * f64::from(padding)
                        + match header.side {
                            HeaderSide::Right | HeaderSide::Left => f64::from(header.extent),
                            HeaderSide::Top | HeaderSide::Bottom => 0.0,
                        },
                ),
                parent.size.y,
            ),
            StackAxis::Vertical => Vec2::new(
                parent.size.x,
                f32_at_least(
                    total_primary
                        + 2.0 * f64::from(padding)
                        + match header.side {
                            HeaderSide::Top | HeaderSide::Bottom => f64::from(header.extent),
                            HeaderSide::Right | HeaderSide::Left => 0.0,
                        },
                ),
            ),
        });
    }
    let (body_width, body_height) = match axis {
        StackAxis::Horizontal => (total_primary, f64::from(cross_extent)),
        StackAxis::Vertical => (f64::from(cross_extent), total_primary),
    };
    let header_width = match header.side {
        HeaderSide::Right | HeaderSide::Left => f64::from(header.extent),
        HeaderSide::Top | HeaderSide::Bottom => 0.0,
    };
    let header_height = match header.side {
        HeaderSide::Top | HeaderSide::Bottom => f64::from(header.extent),
        HeaderSide::Right | HeaderSide::Left => 0.0,
    };
    let required_width = body_width + 2.0 * f64::from(padding) + header_width;
    let required_height = body_height + 2.0 * f64::from(padding) + header_height;
    Ok(Vec2::new(
        f32_at_least(required_width),
        f32_at_least(required_height),
    ))
}

fn f32_at_least(value: f64) -> f32 {
    let rounded = value as f32;
    if f64::from(rounded) < value {
        rounded.next_up()
    } else {
        rounded
    }
}

fn body_rect(parent: Rect, header: HeaderPolicy) -> Result<Rect, SceneLayoutError> {
    let extent = header.extent;
    let mut body = parent;
    match header.side {
        HeaderSide::Top => {
            if extent > parent.size.y {
                return Err(SceneLayoutError::ExceedsBody);
            }
            body.min.y += extent;
            body.size.y -= extent;
        }
        HeaderSide::Right => {
            if extent > parent.size.x {
                return Err(SceneLayoutError::ExceedsBody);
            }
            body.size.x -= extent;
        }
        HeaderSide::Bottom => {
            if extent > parent.size.y {
                return Err(SceneLayoutError::ExceedsBody);
            }
            body.size.y -= extent;
        }
        HeaderSide::Left => {
            if extent > parent.size.x {
                return Err(SceneLayoutError::ExceedsBody);
            }
            body.min.x += extent;
            body.size.x -= extent;
        }
    }
    validate_rect(body)?;
    Ok(body)
}

fn validate_rect(rect: Rect) -> Result<(), SceneLayoutError> {
    let max = rect.max();
    if ![
        rect.min.x,
        rect.min.y,
        rect.size.x,
        rect.size.y,
        max.x,
        max.y,
    ]
    .into_iter()
    .all(|value| value.is_finite() && value.abs() <= MAX_BOARD_COORDINATE)
        || rect.size.x < 0.0
        || rect.size.y < 0.0
    {
        return Err(SceneLayoutError::InvalidGeometry);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::board::{
        CanvasConnector, CanvasLayer, CanvasObject, CanvasObjectId, CanvasObjectKind, Endpoint,
        ShapeKind,
    };
    use crate::model::{Sheet, Workbook};
    use crate::scene_hierarchy::{SceneEditKind, SceneHierarchy};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(Vec2::new(x, y), Vec2::new(width, height))
    }

    fn policy(side: HeaderSide, extent: f32, axis: StackAxis) -> ContainerLayoutPolicy {
        ContainerLayoutPolicy {
            header: HeaderPolicy { side, extent },
            layout: ContainerLayout::Stack {
                axis,
                spacing: 5.0,
                padding: 10.0,
                cross_axis: StackCrossAxisPolicy::Preserve,
                resize: StackResizePolicy::Preserve,
            },
        }
    }

    #[test]
    fn header_side_is_independent_from_stack_axis() {
        let parent = rect(100.0, 200.0, 300.0, 200.0);
        let children = [rect(0.0, 0.0, 40.0, 30.0), rect(0.0, 0.0, 50.0, 20.0)];

        let vertical_with_left_header = preview_child_layout(
            parent,
            policy(HeaderSide::Left, 35.0, StackAxis::Vertical),
            &children,
        )
        .unwrap();
        assert_eq!(vertical_with_left_header[0], rect(145.0, 210.0, 40.0, 30.0));
        assert_eq!(vertical_with_left_header[1], rect(145.0, 245.0, 50.0, 20.0));

        let horizontal_with_top_header = preview_child_layout(
            parent,
            policy(HeaderSide::Top, 25.0, StackAxis::Horizontal),
            &children,
        )
        .unwrap();
        assert_eq!(
            horizontal_with_top_header[0],
            rect(110.0, 235.0, 40.0, 30.0)
        );
        assert_eq!(
            horizontal_with_top_header[1],
            rect(155.0, 235.0, 50.0, 20.0)
        );
    }

    #[test]
    fn cross_axis_fill_resizes_children_to_the_container_body() {
        let parent = rect(100.0, 200.0, 300.0, 200.0);
        let children = [rect(0.0, 0.0, 40.0, 30.0), rect(0.0, 0.0, 50.0, 20.0)];
        let horizontal = preview_container_layout(
            parent,
            ContainerLayoutPolicy {
                header: HeaderPolicy {
                    side: HeaderSide::Top,
                    extent: 25.0,
                },
                layout: ContainerLayout::Stack {
                    axis: StackAxis::Horizontal,
                    spacing: 5.0,
                    padding: 10.0,
                    cross_axis: StackCrossAxisPolicy::Fill,
                    resize: StackResizePolicy::Preserve,
                },
            },
            &children,
        )
        .unwrap();
        assert_eq!(
            horizontal.children,
            [
                rect(110.0, 235.0, 40.0, 155.0),
                rect(155.0, 235.0, 50.0, 155.0)
            ]
        );

        let vertical = preview_container_layout(
            parent,
            ContainerLayoutPolicy {
                header: HeaderPolicy {
                    side: HeaderSide::Left,
                    extent: 35.0,
                },
                layout: ContainerLayout::Stack {
                    axis: StackAxis::Vertical,
                    spacing: 5.0,
                    padding: 10.0,
                    cross_axis: StackCrossAxisPolicy::Fill,
                    resize: StackResizePolicy::Preserve,
                },
            },
            &children,
        )
        .unwrap();
        assert_eq!(
            vertical.children,
            [
                rect(145.0, 210.0, 245.0, 30.0),
                rect(145.0, 245.0, 245.0, 20.0)
            ]
        );
    }

    #[test]
    fn cross_axis_fill_parent_resize_preserves_the_cross_axis_frame() {
        let parent = rect(10.0, 20.0, 100.0, 80.0);
        let mut policy = ContainerLayoutPolicy {
            header: HeaderPolicy {
                side: HeaderSide::Top,
                extent: 20.0,
            },
            layout: ContainerLayout::Stack {
                axis: StackAxis::Horizontal,
                spacing: 5.0,
                padding: 10.0,
                cross_axis: StackCrossAxisPolicy::Fill,
                resize: StackResizePolicy::FitParent,
            },
        };
        let preview =
            preview_container_layout(parent, policy, &[rect(0.0, 0.0, 40.0, 200.0)]).unwrap();
        assert_eq!(preview.parent, rect(10.0, 20.0, 60.0, 80.0));
        assert_eq!(preview.children, [rect(20.0, 50.0, 40.0, 40.0)]);

        let ContainerLayout::Stack { resize, .. } = &mut policy.layout else {
            unreachable!();
        };
        *resize = StackResizePolicy::GrowParent;
        let preview =
            preview_container_layout(parent, policy, &[rect(0.0, 0.0, 120.0, 200.0)]).unwrap();
        assert_eq!(preview.parent, rect(10.0, 20.0, 140.0, 80.0));
        assert_eq!(preview.children, [rect(20.0, 50.0, 120.0, 40.0)]);
    }

    #[test]
    fn free_layout_preserves_children_and_stack_rejects_overflow() {
        let parent = rect(0.0, 0.0, 100.0, 100.0);
        let children = [rect(15.0, 20.0, 120.0, 30.0)];
        let free = ContainerLayoutPolicy {
            header: HeaderPolicy {
                side: HeaderSide::Right,
                extent: 20.0,
            },
            layout: ContainerLayout::Free,
        };
        assert_eq!(
            preview_child_layout(parent, free, &children).unwrap(),
            children
        );
        assert_eq!(
            preview_child_layout(
                parent,
                policy(HeaderSide::Top, 10.0, StackAxis::Horizontal),
                &children
            ),
            Err(SceneLayoutError::ExceedsBody)
        );
    }

    #[test]
    fn fill_last_stack_grows_only_the_final_child_to_remaining_space() {
        let parent = rect(0.0, 0.0, 100.0, 60.0);
        let children = [rect(0.0, 0.0, 20.0, 10.0), rect(0.0, 0.0, 15.0, 15.0)];
        let mut fill_last = policy(HeaderSide::Top, 10.0, StackAxis::Horizontal);
        let ContainerLayout::Stack { resize, .. } = &mut fill_last.layout else {
            unreachable!();
        };
        *resize = StackResizePolicy::FillLast;

        assert_eq!(
            preview_child_layout(parent, fill_last, &children).unwrap(),
            [rect(10.0, 20.0, 20.0, 10.0), rect(35.0, 20.0, 55.0, 15.0)]
        );
        assert_eq!(
            preview_child_layout(rect(0.0, 0.0, 40.0, 60.0), fill_last, &children),
            Err(SceneLayoutError::ExceedsBody)
        );
    }

    #[test]
    fn grow_parent_preserves_children_and_accounts_for_header_and_padding() {
        let parent = rect(0.0, 0.0, 30.0, 25.0);
        let children = [rect(0.0, 0.0, 20.0, 10.0), rect(0.0, 0.0, 15.0, 15.0)];
        let mut grow_parent = policy(HeaderSide::Left, 10.0, StackAxis::Vertical);
        let ContainerLayout::Stack { resize, .. } = &mut grow_parent.layout else {
            unreachable!();
        };
        *resize = StackResizePolicy::GrowParent;

        let preview = preview_container_layout(parent, grow_parent, &children).unwrap();
        assert_eq!(preview.parent, rect(0.0, 0.0, 50.0, 50.0));
        assert_eq!(
            preview.children,
            [rect(20.0, 10.0, 20.0, 10.0), rect(20.0, 25.0, 15.0, 15.0)]
        );
        assert_eq!(
            preview_child_layout(parent, grow_parent, &children),
            Err(SceneLayoutError::ParentResizeRequiresContainerPreview)
        );
    }

    #[test]
    fn fit_parent_shrinks_to_the_exact_content_bounds() {
        let parent = rect(0.0, 0.0, 200.0, 160.0);
        let children = [rect(0.0, 0.0, 20.0, 10.0), rect(0.0, 0.0, 15.0, 15.0)];
        let mut fit_parent = policy(HeaderSide::Left, 10.0, StackAxis::Vertical);
        let ContainerLayout::Stack { resize, .. } = &mut fit_parent.layout else {
            unreachable!();
        };
        *resize = StackResizePolicy::FitParent;

        let preview = preview_container_layout(parent, fit_parent, &children).unwrap();
        assert_eq!(preview.parent, rect(0.0, 0.0, 50.0, 50.0));
        assert_eq!(
            preview.children,
            [rect(20.0, 10.0, 20.0, 10.0), rect(20.0, 25.0, 15.0, 15.0)]
        );
    }

    #[test]
    fn distribute_shares_surplus_space_between_children_without_resizing_them() {
        let parent = rect(0.0, 0.0, 100.0, 60.0);
        let children = [
            rect(0.0, 0.0, 10.0, 10.0),
            rect(0.0, 0.0, 10.0, 10.0),
            rect(0.0, 0.0, 10.0, 10.0),
        ];
        let mut distribute = policy(HeaderSide::Top, 10.0, StackAxis::Horizontal);
        let ContainerLayout::Stack { resize, .. } = &mut distribute.layout else {
            unreachable!();
        };
        *resize = StackResizePolicy::Distribute;

        assert_eq!(
            preview_child_layout(parent, distribute, &children).unwrap(),
            [
                rect(10.0, 20.0, 10.0, 10.0),
                rect(45.0, 20.0, 10.0, 10.0),
                rect(80.0, 20.0, 10.0, 10.0),
            ]
        );
        assert_eq!(
            preview_child_layout(parent, distribute, &children[..1]).unwrap(),
            [rect(10.0, 20.0, 10.0, 10.0)]
        );
    }

    #[test]
    fn nested_grow_parent_policies_are_sized_bottom_up_before_placement() {
        let parent = CanvasObjectId(1);
        let nested = CanvasObjectId(2);
        let leaf = CanvasObjectId(3);
        let mut canvas = CanvasLayer::default();
        for (id, frame) in [
            (parent, rect(0.0, 0.0, 30.0, 25.0)),
            (nested, rect(0.0, 0.0, 20.0, 15.0)),
            (leaf, rect(0.0, 0.0, 40.0, 30.0)),
        ] {
            let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
            object.id = id;
            canvas.objects.push(object);
        }
        canvas.scene.containers.extend([parent, nested]);
        canvas.scene.parents.insert(nested, parent);
        canvas.scene.parents.insert(leaf, nested);

        let mut outer = policy(HeaderSide::Top, 0.0, StackAxis::Horizontal);
        let mut inner = policy(HeaderSide::Top, 0.0, StackAxis::Vertical);
        for policy in [&mut outer, &mut inner] {
            let ContainerLayout::Stack { resize, .. } = &mut policy.layout else {
                unreachable!();
            };
            *resize = StackResizePolicy::GrowParent;
        }
        let policies = BTreeMap::from([(parent, outer), (nested, inner)]);
        let hierarchy = SceneHierarchy::from_canvas(&canvas).unwrap();
        let preview = hierarchy
            .preview_layout_tree_with_validator(&policies, |_| Ok(()))
            .unwrap();

        let frame = |id| preview.object(id).unwrap().rect;
        assert_eq!(frame(parent), rect(0.0, 0.0, 80.0, 70.0));
        assert_eq!(frame(nested), rect(10.0, 10.0, 60.0, 50.0));
        assert_eq!(frame(leaf), rect(20.0, 20.0, 40.0, 30.0));
    }

    #[test]
    fn nested_fit_parent_policies_shrink_frames_bottom_up() {
        let parent = CanvasObjectId(1);
        let nested = CanvasObjectId(2);
        let leaf = CanvasObjectId(3);
        let mut canvas = CanvasLayer::default();
        for (id, frame) in [
            (parent, rect(0.0, 0.0, 800.0, 400.0)),
            (nested, rect(0.0, 0.0, 300.0, 200.0)),
            (leaf, rect(0.0, 0.0, 40.0, 30.0)),
        ] {
            let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
            object.id = id;
            canvas.objects.push(object);
        }
        canvas.scene.containers.extend([parent, nested]);
        canvas.scene.parents.insert(nested, parent);
        canvas.scene.parents.insert(leaf, nested);

        let mut outer = policy(HeaderSide::Top, 0.0, StackAxis::Horizontal);
        let mut inner = policy(HeaderSide::Top, 0.0, StackAxis::Vertical);
        for policy in [&mut outer, &mut inner] {
            let ContainerLayout::Stack { resize, .. } = &mut policy.layout else {
                unreachable!();
            };
            *resize = StackResizePolicy::FitParent;
        }
        let policies = BTreeMap::from([(parent, outer), (nested, inner)]);
        let hierarchy = SceneHierarchy::from_canvas(&canvas).unwrap();
        let preview = hierarchy
            .preview_layout_tree_with_validator(&policies, |_| Ok(()))
            .unwrap();

        let frame = |id| preview.object(id).unwrap().rect;
        assert_eq!(frame(parent), rect(0.0, 0.0, 80.0, 70.0));
        assert_eq!(frame(nested), rect(10.0, 10.0, 60.0, 50.0));
        assert_eq!(frame(leaf), rect(20.0, 20.0, 40.0, 30.0));
    }

    #[test]
    fn legacy_stack_policy_defaults_to_preserving_child_sizes() {
        let legacy = serde_json::json!({
            "header": { "side": "top", "extent": 20.0 },
            "layout": {
                "stack": { "axis": "vertical", "spacing": 5.0, "padding": 10.0 }
            }
        });
        let decoded: ContainerLayoutPolicy = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            decoded.layout,
            ContainerLayout::Stack {
                axis: StackAxis::Vertical,
                spacing: 5.0,
                padding: 10.0,
                cross_axis: StackCrossAxisPolicy::Preserve,
                resize: StackResizePolicy::Preserve,
            }
        );
        let encoded = serde_json::to_value(decoded).unwrap();
        assert!(encoded["layout"]["stack"].get("resize").is_none());
        assert!(encoded["layout"]["stack"].get("crossAxis").is_none());
    }

    #[test]
    fn fill_last_refuses_to_resize_a_collapsed_child() {
        let parent = CanvasObjectId(1);
        let child = CanvasObjectId(2);
        let mut canvas = CanvasLayer::default();
        for (id, frame) in [
            (parent, rect(0.0, 0.0, 100.0, 60.0)),
            (child, rect(10.0, 10.0, 20.0, 10.0)),
        ] {
            let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
            object.id = id;
            canvas.objects.push(object);
        }
        canvas.scene.containers.extend([parent, child]);
        canvas.scene.parents.insert(child, parent);
        canvas
            .scene
            .collapsed
            .insert(child, rect(10.0, 10.0, 20.0, 30.0));

        let hierarchy = SceneHierarchy::from_canvas(&canvas).unwrap();
        let mut fill_last = policy(HeaderSide::Top, 0.0, StackAxis::Vertical);
        let ContainerLayout::Stack { resize, .. } = &mut fill_last.layout else {
            unreachable!();
        };
        *resize = StackResizePolicy::FillLast;
        let error = hierarchy
            .preview_layout_with_validator(parent, fill_last, |_| Ok(()))
            .unwrap_err();
        assert!(error.contains("expand collapsed children"));
    }

    #[test]
    fn invalid_policy_and_geometry_are_rejected() {
        let parent = rect(0.0, 0.0, 100.0, 100.0);
        let children = [rect(0.0, 0.0, 10.0, 10.0)];
        let invalid = ContainerLayoutPolicy {
            header: HeaderPolicy {
                side: HeaderSide::Top,
                extent: f32::NAN,
            },
            layout: ContainerLayout::Free,
        };
        assert_eq!(
            preview_child_layout(parent, invalid, &children),
            Err(SceneLayoutError::InvalidPolicy)
        );
        assert_eq!(
            preview_child_layout(rect(0.0, 0.0, f32::INFINITY, 10.0), invalid, &[]),
            Err(SceneLayoutError::InvalidGeometry)
        );
        let oversized_header = ContainerLayoutPolicy {
            header: HeaderPolicy {
                side: HeaderSide::Bottom,
                extent: 101.0,
            },
            layout: ContainerLayout::Free,
        };
        assert_eq!(
            preview_child_layout(parent, oversized_header, &[]),
            Err(SceneLayoutError::ExceedsBody)
        );
    }

    #[test]
    fn scene_layout_preview_moves_nested_subtrees_atomically() {
        let root = CanvasObjectId(1);
        let nested = CanvasObjectId(2);
        let nested_child = CanvasObjectId(3);
        let sibling = CanvasObjectId(4);
        let internal_connector = CanvasObjectId(5);
        let crossing_connector = CanvasObjectId(6);
        let mut canvas = CanvasLayer::default();
        for (id, frame) in [
            (root, rect(0.0, 0.0, 200.0, 160.0)),
            (nested, rect(100.0, 100.0, 80.0, 40.0)),
            (nested_child, rect(120.0, 130.0, 20.0, 10.0)),
            (sibling, rect(150.0, 90.0, 80.0, 20.0)),
        ] {
            let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
            object.id = id;
            canvas.objects.push(object);
        }
        let mut connector =
            CanvasConnector::new(Endpoint::Object(nested), Endpoint::Object(nested_child));
        connector.waypoints = Arc::from([Vec2::new(130.0, 150.0)]);
        let mut connector_object = CanvasObject::connector(connector);
        connector_object.id = internal_connector;
        canvas.objects.push(connector_object);
        let mut crossing =
            CanvasConnector::new(Endpoint::Object(nested), Endpoint::Object(sibling));
        crossing.waypoints = Arc::from([Vec2::new(220.0, 120.0)]);
        let mut crossing_object = CanvasObject::connector(crossing);
        crossing_object.id = crossing_connector;
        canvas.objects.push(crossing_object);
        canvas.scene.containers.extend([root, nested]);
        canvas.scene.parents = BTreeMap::from([
            (nested, root),
            (nested_child, nested),
            (internal_connector, nested),
            (sibling, root),
            (crossing_connector, root),
        ]);
        let hierarchy = SceneHierarchy::from_canvas(&canvas).unwrap();
        let policy = policy(HeaderSide::Top, 10.0, StackAxis::Vertical);

        let rejected = hierarchy.preview_layout_with_validator(root, policy, |context| {
            assert_eq!(context.kind, SceneEditKind::Layout);
            assert_eq!(
                context.geometry.object(nested).unwrap().rect,
                rect(10.0, 20.0, 80.0, 40.0)
            );
            Err("layout vetoed".to_owned())
        });
        assert_eq!(rejected.unwrap_err(), "layout vetoed");
        assert_eq!(
            canvas.object(nested).unwrap().rect,
            rect(100.0, 100.0, 80.0, 40.0)
        );

        let candidate = hierarchy
            .preview_layout_with_validator(root, policy, |_| Ok(()))
            .unwrap();
        assert_eq!(
            candidate.object(nested).unwrap().rect,
            rect(10.0, 20.0, 80.0, 40.0)
        );
        assert_eq!(
            candidate.object(nested_child).unwrap().rect,
            rect(30.0, 50.0, 20.0, 10.0)
        );
        let CanvasObjectKind::Connector(connector) =
            &candidate.object(internal_connector).unwrap().kind
        else {
            panic!("expected translated connector");
        };
        assert_eq!(connector.waypoints.as_ref(), &[Vec2::new(40.0, 70.0)]);
        let CanvasObjectKind::Connector(connector) =
            &candidate.object(crossing_connector).unwrap().kind
        else {
            panic!("expected stationary crossing connector");
        };
        assert_eq!(connector.waypoints.as_ref(), &[Vec2::new(220.0, 120.0)]);
        assert_eq!(
            candidate.object(sibling).unwrap().rect,
            rect(10.0, 65.0, 80.0, 20.0)
        );
        assert_eq!(
            candidate.scene.parents,
            BTreeMap::from([
                (nested, root),
                (nested_child, nested),
                (internal_connector, nested),
                (sibling, root),
                (crossing_connector, root),
            ])
        );
        assert_eq!(
            canvas.object(sibling).unwrap().rect,
            rect(150.0, 90.0, 80.0, 20.0)
        );
    }

    #[test]
    fn nested_layout_uses_top_down_scene_order_not_policy_map_order() {
        let root = CanvasObjectId(20);
        let nested = CanvasObjectId(10);
        let nested_child = CanvasObjectId(30);
        let sibling = CanvasObjectId(40);
        let mut canvas = CanvasLayer::default();
        for (id, frame) in [
            (root, rect(0.0, 0.0, 240.0, 200.0)),
            (nested, rect(120.0, 100.0, 80.0, 100.0)),
            (nested_child, rect(200.0, 150.0, 40.0, 20.0)),
            (sibling, rect(100.0, 50.0, 40.0, 20.0)),
        ] {
            let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
            object.id = id;
            canvas.objects.push(object);
        }
        canvas.scene.containers.extend([root, nested]);
        canvas.scene.parents =
            BTreeMap::from([(nested, root), (nested_child, nested), (sibling, root)]);
        let hierarchy = SceneHierarchy::from_canvas(&canvas).unwrap();
        let policies = BTreeMap::from([
            (nested, policy(HeaderSide::Left, 10.0, StackAxis::Vertical)),
            (root, policy(HeaderSide::Top, 10.0, StackAxis::Vertical)),
        ]);

        let candidate = hierarchy
            .preview_layout_tree_with_validator(&policies, |context| {
                assert_eq!(context.kind, SceneEditKind::Layout);
                assert_eq!(context.selection, &[root, nested]);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            candidate.object(nested).unwrap().rect,
            rect(10.0, 20.0, 80.0, 100.0)
        );
        assert_eq!(
            candidate.object(nested_child).unwrap().rect,
            rect(30.0, 30.0, 40.0, 20.0)
        );
        assert_eq!(
            candidate.object(sibling).unwrap().rect,
            rect(10.0, 125.0, 40.0, 20.0)
        );
        assert_eq!(
            canvas.object(nested_child).unwrap().rect,
            rect(200.0, 150.0, 40.0, 20.0)
        );
    }

    #[test]
    fn stored_layout_policy_applies_as_one_undoable_document_edit() {
        let mut sheet = Sheet::new("Layout", "Root");
        let container = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                rect(0.0, 0.0, 120.0, 100.0),
            ))
            .unwrap();
        sheet.set_canvas_container(container, true).unwrap();
        let child = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                rect(80.0, 70.0, 20.0, 20.0),
            ))
            .unwrap();
        sheet
            .reparent_canvas_objects(&[child], Some(container))
            .unwrap();
        let layout_policy = policy(HeaderSide::Top, 10.0, StackAxis::Vertical);
        sheet
            .set_canvas_container_layout_policy(container, Some(layout_policy))
            .unwrap();

        let mut document = Document::new(Workbook::new(sheet));
        document.layout_canvas_container(container).unwrap();
        assert_eq!(
            document.workbook().sheets[0]
                .canvas
                .object(child)
                .unwrap()
                .rect,
            rect(10.0, 20.0, 20.0, 20.0)
        );
        assert!(document.undo());
        assert_eq!(
            document.workbook().sheets[0]
                .canvas
                .object(child)
                .unwrap()
                .rect,
            rect(80.0, 70.0, 20.0, 20.0)
        );
        assert!(
            SceneHierarchy::from_canvas(&document.workbook().sheets[0].canvas)
                .unwrap()
                .preview_stored_layout_with_validator(container, |_| Ok(()))
                .is_ok()
        );
    }

    #[test]
    fn invalid_policies_are_rejected_and_policy_cleanup_tracks_container_lifecycle() {
        let mut sheet = Sheet::new("Policy lifecycle", "Root");
        let disabled = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                rect(0.0, 0.0, 120.0, 100.0),
            ))
            .unwrap();
        sheet.set_canvas_container(disabled, true).unwrap();
        let invalid = ContainerLayoutPolicy {
            header: HeaderPolicy {
                side: HeaderSide::Top,
                extent: f32::NAN,
            },
            layout: ContainerLayout::Free,
        };
        assert!(
            sheet
                .set_canvas_container_layout_policy(disabled, Some(invalid))
                .is_err()
        );
        assert!(!sheet.canvas.scene.layout_policies.contains_key(&disabled));

        let policy = ContainerLayoutPolicy {
            header: HeaderPolicy {
                side: HeaderSide::Top,
                extent: 10.0,
            },
            layout: ContainerLayout::Free,
        };
        sheet
            .set_canvas_container_layout_policy(disabled, Some(policy))
            .unwrap();
        sheet.set_canvas_container(disabled, false).unwrap();
        assert!(!sheet.canvas.scene.layout_policies.contains_key(&disabled));

        let deleted = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                rect(150.0, 0.0, 120.0, 100.0),
            ))
            .unwrap();
        sheet.set_canvas_container(deleted, true).unwrap();
        sheet
            .set_canvas_container_layout_policy(deleted, Some(policy))
            .unwrap();
        sheet.remove_canvas_objects(&[deleted]).unwrap();
        assert!(!sheet.canvas.scene.layout_policies.contains_key(&deleted));
    }

    #[test]
    fn stored_header_policy_resolves_stencil_geometry_and_label_for_each_side() {
        let cases = [
            (
                HeaderSide::Top,
                rect(0.0, 0.0, 120.0, 20.0),
                Vec2::new(120.0, 20.0),
            ),
            (
                HeaderSide::Right,
                rect(0.0, 0.0, 100.0, 100.0),
                Vec2::new(100.0, 100.0),
            ),
            (
                HeaderSide::Bottom,
                rect(0.0, 80.0, 120.0, 20.0),
                Vec2::new(120.0, 80.0),
            ),
            (
                HeaderSide::Left,
                rect(20.0, 0.0, 100.0, 100.0),
                Vec2::new(20.0, 100.0),
            ),
        ];

        for (side, expected_label, divider_point) in cases {
            let mut sheet = Sheet::new("Header policy", "Root");
            let mut object =
                CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 120.0, 100.0));
            object.stencil = Some(crate::stencil_object::StencilRef::new(
                "thinoda.general",
                "header-frame-v2",
            ));
            let container = sheet.add_canvas_object(object).unwrap();
            sheet.set_canvas_container(container, true).unwrap();
            sheet
                .set_canvas_container_layout_policy(
                    container,
                    Some(ContainerLayoutPolicy {
                        header: HeaderPolicy { side, extent: 20.0 },
                        layout: ContainerLayout::Free,
                    }),
                )
                .unwrap();

            let scene = crate::scene::build_scene(&sheet, &crate::layout::Layout::default());
            let item = scene
                .item(crate::scene::SceneId::Object(container))
                .unwrap();
            let render = item.stencil_render.as_ref().unwrap();
            assert_eq!(
                crate::stencil_object::label_rect(sheet.canvas.object(container).unwrap(), render),
                expected_label
            );
            let crate::scene::SceneGeometry::Polygon { details, .. } = &item.geometry else {
                panic!("container should have polygon geometry");
            };
            assert!(details.iter().any(|path| path.contains(&divider_point)));
        }
    }

    #[test]
    fn stored_header_extent_is_not_truncated_by_stencil_parameter_bounds() {
        let mut sheet = Sheet::new("Large header", "Root");
        let mut object =
            CanvasObject::shape(ShapeKind::Rectangle, rect(0.0, 0.0, 12_000.0, 12_000.0));
        object.stencil = Some(crate::stencil_object::StencilRef::new(
            "thinoda.general",
            "header-frame-v2",
        ));
        let container = sheet.add_canvas_object(object).unwrap();
        sheet.set_canvas_container(container, true).unwrap();
        sheet
            .set_canvas_container_layout_policy(
                container,
                Some(ContainerLayoutPolicy {
                    header: HeaderPolicy {
                        side: HeaderSide::Top,
                        extent: 10_001.0,
                    },
                    layout: ContainerLayout::Free,
                }),
            )
            .unwrap();

        let scene = crate::scene::build_scene(&sheet, &crate::layout::Layout::default());
        let item = scene
            .item(crate::scene::SceneId::Object(container))
            .unwrap();
        let render = item.stencil_render.as_ref().unwrap();
        assert_eq!(
            crate::stencil_object::label_rect(sheet.canvas.object(container).unwrap(), render)
                .size
                .y,
            10_001.0
        );
    }

    #[test]
    fn equal_sibling_translations_carry_crossing_connector_once() {
        let root = CanvasObjectId(1);
        let first = CanvasObjectId(2);
        let second = CanvasObjectId(3);
        let connector_id = CanvasObjectId(4);
        let mut canvas = CanvasLayer::default();
        for (id, frame) in [
            (root, rect(0.0, 0.0, 100.0, 100.0)),
            (first, rect(30.0, 20.0, 20.0, 20.0)),
            (second, rect(30.0, 45.0, 20.0, 20.0)),
        ] {
            let mut object = CanvasObject::shape(ShapeKind::Rectangle, frame);
            object.id = id;
            canvas.objects.push(object);
        }
        let mut connector = CanvasConnector::new(Endpoint::Object(first), Endpoint::Object(second));
        connector.waypoints = Arc::from([Vec2::new(55.0, 32.0)]);
        let mut connector_object = CanvasObject::connector(connector);
        connector_object.id = connector_id;
        canvas.objects.push(connector_object);
        canvas.scene.containers.insert(root);
        canvas.scene.parents =
            BTreeMap::from([(first, root), (second, root), (connector_id, root)]);
        let hierarchy = SceneHierarchy::from_canvas(&canvas).unwrap();

        let candidate = hierarchy
            .preview_layout_with_validator(
                root,
                policy(HeaderSide::Top, 10.0, StackAxis::Vertical),
                |_| Ok(()),
            )
            .unwrap();
        let CanvasObjectKind::Connector(connector) = &candidate.object(connector_id).unwrap().kind
        else {
            panic!("expected translated connector");
        };
        assert_eq!(connector.waypoints.as_ref(), &[Vec2::new(35.0, 32.0)]);
    }
}
