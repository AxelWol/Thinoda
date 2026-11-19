//! Per-process crash recovery with advisory locks and coalesced snapshot writes.

use crate::{
    Error, Paths, Result,
    fs::{self, EntryKind, Lock, atomic_write},
    unix_seconds,
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error as _, IgnoredAny, MapAccess, SeqAccess, Visitor, value::MapAccessDeserializer},
    ser::SerializeTuple,
};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use thinoda_core::Workbook;

/// Document identity and recovery timestamp (Unix seconds).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotMeta {
    pub title: String,
    pub original_path: Option<PathBuf>,
    pub import_source: Option<PathBuf>,
    pub dirty: bool,
    pub saved_at: u64,
}

/// On-disk envelope; callers can serialize off the UI thread and queue the bytes.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub workbook: Workbook,
    /// Application-owned, versioned pending-ink data, if present.
    pub pending_ink: Option<String>,
}

const INLINE_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INLINE-1";
const WAYPOINT_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-WAYPOINTS-1";
const INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-1";
const MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-1";
const MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-1";
const EXTENDED_MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-2";
const EXTENDED_MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-2";
const SIZED_MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-3";
const SIZED_MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-3";
const FILLED_MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-4";
const FILLED_MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-4";
const DIAMOND_MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-5";
const DIAMOND_MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-5";
const OVAL_MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-6";
const OVAL_MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-6";
const HALF_CIRCLE_MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-7";
const HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-7";
const BAR_MARKER_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKERS-8";
const BAR_MARKER_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKERS-8";
const ROUNDING_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-ROUNDING-1";
const ROUNDING_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-ROUNDING-1";
const RECTANGLE_SIDES_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-RECTANGLE-SIDES-1";
const RECTANGLE_SIDES_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-RECTANGLE-SIDES-1";
const DOUBLE_ELLIPSE_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-DOUBLE-ELLIPSE-1";
const DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-DOUBLE-ELLIPSE-1";
const FIXED_PORT_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-FIXED-PORTS-1";
const FIXED_PORT_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-FIXED-PORTS-1";
const DRAWIO_ROUNDING_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-DRAWIO-ROUNDING-1";
const DRAWIO_ROUNDING_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-DRAWIO-ROUNDING-1";
const FINAL_STATE_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-FINAL-STATE-1";
const FINAL_STATE_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-FINAL-STATE-1";
const SANKEY_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SANKEY-1";
const SANKEY_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-SANKEY-1";
const STENCIL_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-STENCIL-1";
const STENCIL_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-STENCIL-1";
const SCENE_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SCENE-1";
const SCENE_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-SCENE-1";
const PINNED_STENCIL_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-STENCIL-2";
const PINNED_STENCIL_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-STENCIL-2";
const CLOSED_STENCIL_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-STENCIL-3";
const CLOSED_STENCIL_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-STENCIL-3";
const SCENE_STENCIL_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SCENE-STENCIL-1";
const SCENE_STENCIL_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-SCENE-STENCIL-1";
const COLLAPSE_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-COLLAPSE-1";
const COLLAPSE_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-COLLAPSE-1";
const SCENE_LAYOUT_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SCENE-LAYOUT-1";
const SCENE_LAYOUT_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-SCENE-LAYOUT-1";
const SCENE_STENCIL_LAYOUT_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SCENE-STENCIL-LAYOUT-1";
const SCENE_STENCIL_LAYOUT_INK_WORKBOOK_VERSION: &str =
    "THINODA-RECOVERY-INK-SCENE-STENCIL-LAYOUT-1";
const COLLAPSE_LAYOUT_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-COLLAPSE-LAYOUT-1";
const COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-COLLAPSE-LAYOUT-1";
const SEMANTIC_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SEMANTIC-1";
const SEMANTIC_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-SEMANTIC-1";
const MARKER_LIBRARY_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-MARKER-LIBRARY-1";
const MARKER_LIBRARY_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-MARKER-LIBRARY-1";
const LINE_JUMPS_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-LINE-JUMPS-1";
const LINE_JUMPS_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-LINE-JUMPS-1";
const EDGE_ROUTES_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-EDGE-ROUTES-1";
const EDGE_ROUTES_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-EDGE-ROUTES-1";
const SEGMENT_ROUTES_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SEGMENT-ROUTES-1";
const SEGMENT_ROUTES_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-SEGMENT-ROUTES-1";
const EDGE_SHAPES_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-EDGE-SHAPES-1";
const EDGE_SHAPES_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-EDGE-SHAPES-1";
const CONNECTOR_LABELS_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-CONNECTOR-LABELS-1";
const CONNECTOR_LABELS_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-CONNECTOR-LABELS-1";
const OBJECT_DATA_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-OBJECT-DATA-1";
const OBJECT_DATA_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-OBJECT-DATA-1";
const LAYERS_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-LAYERS-1";
const LAYERS_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-LAYERS-1";
const TABLES_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-TABLES-1";
const TABLES_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-TABLES-1";
const SHAPE_STYLES_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-SHAPE-STYLES-1";
const SHAPE_STYLES_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-SHAPE-STYLES-1";
const LABEL_STYLES_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-LABEL-STYLES-1";
const LABEL_STYLES_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-LABEL-STYLES-1";
const RICH_LABELS_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-RICH-LABELS-1";
const RICH_LABELS_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-RICH-LABELS-1";
const RICH_CONNECTOR_LABELS_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-RICH-CONNECTOR-LABELS-1";
const RICH_CONNECTOR_LABELS_INK_WORKBOOK_VERSION: &str =
    "THINODA-RECOVERY-INK-RICH-CONNECTOR-LABELS-1";
const RICH_EXTRA_CONNECTOR_LABELS_WORKBOOK_VERSION: &str =
    "THINODA-RECOVERY-RICH-EXTRA-CONNECTOR-LABELS-1";
const RICH_EXTRA_CONNECTOR_LABELS_INK_WORKBOOK_VERSION: &str =
    "THINODA-RECOVERY-INK-RICH-EXTRA-CONNECTOR-LABELS-1";
const CARD_BASELINE_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-CARD-BASELINE-1";
const CARD_BASELINE_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-CARD-BASELINE-1";
const DRAWIO_DOCUMENT_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-DRAWIO-DOCUMENT-1";
const DRAWIO_DOCUMENT_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-DRAWIO-DOCUMENT-1";
const DRAWIO_ZONE_IDS_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-DRAWIO-ZONE-IDS-1";
const DRAWIO_ZONE_IDS_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-DRAWIO-ZONE-IDS-1";
const DRAWIO_TOPIC_IDS_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-DRAWIO-TOPIC-IDS-1";
const DRAWIO_TOPIC_IDS_INK_WORKBOOK_VERSION: &str = "THINODA-RECOVERY-INK-DRAWIO-TOPIC-IDS-1";

/// Bound the transient input envelope separately from the workbook.
pub const MAX_PENDING_INK_BYTES: usize = 16 * 1024 * 1024;

struct BorrowedWorkbook<'a> {
    workbook: &'a Workbook,
    pending_ink: Option<&'a str>,
}

impl Serialize for BorrowedWorkbook<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let Some(pending_ink) = self.pending_ink else {
            return serialize_workbook(self.workbook, serializer);
        };
        if pending_ink.is_empty() || pending_ink.len() > MAX_PENDING_INK_BYTES {
            return Err(serde::ser::Error::custom("invalid pending ink size"));
        }
        let mut tuple = serializer.serialize_tuple(3)?;
        tuple.serialize_element(if self.workbook.has_drawio_topic_source_ids() {
            DRAWIO_TOPIC_IDS_INK_WORKBOOK_VERSION
        } else if self.workbook.has_drawio_zone_source_ids() {
            DRAWIO_ZONE_IDS_INK_WORKBOOK_VERSION
        } else if self.workbook.drawio.is_some() {
            DRAWIO_DOCUMENT_INK_WORKBOOK_VERSION
        } else if self.workbook.has_card_baseline_overrides() {
            CARD_BASELINE_INK_WORKBOOK_VERSION
        } else if self.workbook.has_rich_extra_connector_labels() {
            RICH_EXTRA_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
        } else if self.workbook.has_rich_connector_labels() {
            RICH_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
        } else if self.workbook.has_rich_labels() {
            RICH_LABELS_INK_WORKBOOK_VERSION
        } else if self.workbook.has_label_styles() {
            LABEL_STYLES_INK_WORKBOOK_VERSION
        } else if self.workbook.has_shape_styles() {
            SHAPE_STYLES_INK_WORKBOOK_VERSION
        } else if self.workbook.has_tables() {
            TABLES_INK_WORKBOOK_VERSION
        } else if self.workbook.has_layers() {
            LAYERS_INK_WORKBOOK_VERSION
        } else if self.workbook.has_object_data() {
            OBJECT_DATA_INK_WORKBOOK_VERSION
        } else if self.workbook.has_connector_labels() {
            CONNECTOR_LABELS_INK_WORKBOOK_VERSION
        } else if self.workbook.has_connector_edge_shapes() {
            EDGE_SHAPES_INK_WORKBOOK_VERSION
        } else if self.workbook.has_drawio_segment_routes() {
            SEGMENT_ROUTES_INK_WORKBOOK_VERSION
        } else if self.workbook.has_drawio_edge_routes() {
            EDGE_ROUTES_INK_WORKBOOK_VERSION
        } else if self.workbook.has_connector_line_jumps() {
            LINE_JUMPS_INK_WORKBOOK_VERSION
        } else if self.workbook.has_library_connector_markers() {
            MARKER_LIBRARY_INK_WORKBOOK_VERSION
        } else if self.workbook.has_semantic_model() {
            SEMANTIC_INK_WORKBOOK_VERSION
        } else if self.workbook.has_scene_collapse() && self.workbook.has_scene_layout_policies() {
            COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION
        } else if self.workbook.has_scene_collapse() {
            COLLAPSE_INK_WORKBOOK_VERSION
        } else if self.workbook.has_scene_hierarchy()
            && self.workbook.has_scene_layout_policies()
            && (self.workbook.has_stencil_definition_hashes()
                || self.workbook.has_stencil_definition_closures())
        {
            SCENE_STENCIL_LAYOUT_INK_WORKBOOK_VERSION
        } else if self.workbook.has_scene_hierarchy() && self.workbook.has_scene_layout_policies() {
            SCENE_LAYOUT_INK_WORKBOOK_VERSION
        } else if self.workbook.has_scene_hierarchy()
            && (self.workbook.has_stencil_definition_hashes()
                || self.workbook.has_stencil_definition_closures())
        {
            SCENE_STENCIL_INK_WORKBOOK_VERSION
        } else if self.workbook.has_scene_hierarchy() {
            SCENE_INK_WORKBOOK_VERSION
        } else if self.workbook.has_stencil_definition_closures() {
            CLOSED_STENCIL_INK_WORKBOOK_VERSION
        } else if self.workbook.has_stencil_definition_hashes() {
            PINNED_STENCIL_INK_WORKBOOK_VERSION
        } else if self.workbook.has_stencils() {
            STENCIL_INK_WORKBOOK_VERSION
        } else if self.workbook.has_sankey() {
            SANKEY_INK_WORKBOOK_VERSION
        } else if self.workbook.has_final_states() {
            FINAL_STATE_INK_WORKBOOK_VERSION
        } else if self.workbook.has_drawio_rectangle_rounding() {
            DRAWIO_ROUNDING_INK_WORKBOOK_VERSION
        } else if self.workbook.has_fixed_connector_ports() {
            FIXED_PORT_INK_WORKBOOK_VERSION
        } else if self.workbook.has_double_ellipses() {
            DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION
        } else if self.workbook.has_rectangle_sides() {
            RECTANGLE_SIDES_INK_WORKBOOK_VERSION
        } else if self.workbook.has_shape_rounding() {
            ROUNDING_INK_WORKBOOK_VERSION
        } else if self.workbook.has_bar_connector_markers() {
            BAR_MARKER_INK_WORKBOOK_VERSION
        } else if self.workbook.has_half_circle_connector_markers() {
            HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
        } else if self.workbook.has_oval_connector_markers() {
            OVAL_MARKER_INK_WORKBOOK_VERSION
        } else if self.workbook.has_diamond_connector_markers() {
            DIAMOND_MARKER_INK_WORKBOOK_VERSION
        } else if self.workbook.has_connector_marker_fills() {
            FILLED_MARKER_INK_WORKBOOK_VERSION
        } else if self.workbook.has_connector_marker_sizes() {
            SIZED_MARKER_INK_WORKBOOK_VERSION
        } else if self.workbook.has_extended_connector_markers() {
            EXTENDED_MARKER_INK_WORKBOOK_VERSION
        } else if self.workbook.has_connector_markers() {
            MARKER_INK_WORKBOOK_VERSION
        } else {
            INK_WORKBOOK_VERSION
        })?;
        tuple.serialize_element(self.workbook)?;
        tuple.serialize_element(pending_ink)?;
        tuple.end()
    }
}

#[derive(Serialize)]
struct BorrowedSnapshot<'a> {
    meta: &'a SnapshotMeta,
    workbook: BorrowedWorkbook<'a>,
}

impl Serialize for Snapshot {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        BorrowedSnapshot {
            meta: &self.meta,
            workbook: BorrowedWorkbook {
                workbook: &self.workbook,
                pending_ink: self.pending_ink.as_deref(),
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Snapshot {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StoredSnapshot {
            meta: SnapshotMeta,
            #[serde(deserialize_with = "deserialize_workbook")]
            workbook: SnapshotWorkbook,
        }

        let stored = StoredSnapshot::deserialize(deserializer)?;
        Ok(Self {
            meta: stored.meta,
            workbook: stored.workbook.workbook,
            pending_ink: stored.workbook.pending_ink,
        })
    }
}

struct SnapshotWorkbook {
    workbook: Workbook,
    pending_ink: Option<String>,
}

fn finish_recovery_workbook<'de, A: SeqAccess<'de>>(
    mut seq: A,
    mut workbook: Workbook,
    has_pending_ink: bool,
) -> std::result::Result<SnapshotWorkbook, A::Error> {
    let pending_ink = if has_pending_ink {
        let pending_ink: String = seq
            .next_element()?
            .ok_or_else(|| A::Error::custom("missing pending ink"))?;
        if pending_ink.is_empty() || pending_ink.len() > MAX_PENDING_INK_BYTES {
            return Err(A::Error::custom("invalid pending ink size"));
        }
        Some(pending_ink)
    } else {
        None
    };
    if seq.next_element::<IgnoredAny>()?.is_some() {
        return Err(A::Error::custom("unexpected recovery workbook data"));
    }
    workbook
        .hydrate_stencil_packages()
        .map_err(A::Error::custom)?;
    Ok(SnapshotWorkbook {
        workbook,
        pending_ink,
    })
}

fn serialize_workbook<S>(workbook: &Workbook, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let version = if workbook.has_drawio_topic_source_ids() {
        DRAWIO_TOPIC_IDS_WORKBOOK_VERSION
    } else if workbook.has_drawio_zone_source_ids() {
        DRAWIO_ZONE_IDS_WORKBOOK_VERSION
    } else if workbook.drawio.is_some() {
        DRAWIO_DOCUMENT_WORKBOOK_VERSION
    } else if workbook.has_card_baseline_overrides() {
        CARD_BASELINE_WORKBOOK_VERSION
    } else if workbook.has_rich_extra_connector_labels() {
        RICH_EXTRA_CONNECTOR_LABELS_WORKBOOK_VERSION
    } else if workbook.has_rich_connector_labels() {
        RICH_CONNECTOR_LABELS_WORKBOOK_VERSION
    } else if workbook.has_rich_labels() {
        RICH_LABELS_WORKBOOK_VERSION
    } else if workbook.has_label_styles() {
        LABEL_STYLES_WORKBOOK_VERSION
    } else if workbook.has_shape_styles() {
        SHAPE_STYLES_WORKBOOK_VERSION
    } else if workbook.has_tables() {
        TABLES_WORKBOOK_VERSION
    } else if workbook.has_layers() {
        LAYERS_WORKBOOK_VERSION
    } else if workbook.has_object_data() {
        OBJECT_DATA_WORKBOOK_VERSION
    } else if workbook.has_connector_labels() {
        CONNECTOR_LABELS_WORKBOOK_VERSION
    } else if workbook.has_connector_edge_shapes() {
        EDGE_SHAPES_WORKBOOK_VERSION
    } else if workbook.has_drawio_segment_routes() {
        SEGMENT_ROUTES_WORKBOOK_VERSION
    } else if workbook.has_drawio_edge_routes() {
        EDGE_ROUTES_WORKBOOK_VERSION
    } else if workbook.has_connector_line_jumps() {
        LINE_JUMPS_WORKBOOK_VERSION
    } else if workbook.has_library_connector_markers() {
        MARKER_LIBRARY_WORKBOOK_VERSION
    } else if workbook.has_semantic_model() {
        SEMANTIC_WORKBOOK_VERSION
    } else if workbook.has_scene_collapse() && workbook.has_scene_layout_policies() {
        COLLAPSE_LAYOUT_WORKBOOK_VERSION
    } else if workbook.has_scene_collapse() {
        COLLAPSE_WORKBOOK_VERSION
    } else if workbook.has_scene_hierarchy()
        && workbook.has_scene_layout_policies()
        && (workbook.has_stencil_definition_hashes() || workbook.has_stencil_definition_closures())
    {
        SCENE_STENCIL_LAYOUT_WORKBOOK_VERSION
    } else if workbook.has_scene_hierarchy() && workbook.has_scene_layout_policies() {
        SCENE_LAYOUT_WORKBOOK_VERSION
    } else if workbook.has_scene_hierarchy()
        && (workbook.has_stencil_definition_hashes() || workbook.has_stencil_definition_closures())
    {
        SCENE_STENCIL_WORKBOOK_VERSION
    } else if workbook.has_scene_hierarchy() {
        SCENE_WORKBOOK_VERSION
    } else if workbook.has_stencil_definition_closures() {
        CLOSED_STENCIL_WORKBOOK_VERSION
    } else if workbook.has_stencil_definition_hashes() {
        PINNED_STENCIL_WORKBOOK_VERSION
    } else if workbook.has_stencils() {
        STENCIL_WORKBOOK_VERSION
    } else if workbook.has_sankey() {
        SANKEY_WORKBOOK_VERSION
    } else if workbook.has_final_states() {
        FINAL_STATE_WORKBOOK_VERSION
    } else if workbook.has_drawio_rectangle_rounding() {
        DRAWIO_ROUNDING_WORKBOOK_VERSION
    } else if workbook.has_fixed_connector_ports() {
        FIXED_PORT_WORKBOOK_VERSION
    } else if workbook.has_double_ellipses() {
        DOUBLE_ELLIPSE_WORKBOOK_VERSION
    } else if workbook.has_rectangle_sides() {
        RECTANGLE_SIDES_WORKBOOK_VERSION
    } else if workbook.has_shape_rounding() {
        ROUNDING_WORKBOOK_VERSION
    } else if workbook.has_bar_connector_markers() {
        BAR_MARKER_WORKBOOK_VERSION
    } else if workbook.has_half_circle_connector_markers() {
        HALF_CIRCLE_MARKER_WORKBOOK_VERSION
    } else if workbook.has_oval_connector_markers() {
        OVAL_MARKER_WORKBOOK_VERSION
    } else if workbook.has_diamond_connector_markers() {
        DIAMOND_MARKER_WORKBOOK_VERSION
    } else if workbook.has_connector_marker_fills() {
        FILLED_MARKER_WORKBOOK_VERSION
    } else if workbook.has_connector_marker_sizes() {
        SIZED_MARKER_WORKBOOK_VERSION
    } else if workbook.has_extended_connector_markers() {
        EXTENDED_MARKER_WORKBOOK_VERSION
    } else if workbook.has_connector_markers() {
        MARKER_WORKBOOK_VERSION
    } else if workbook.has_connector_waypoints() {
        WAYPOINT_WORKBOOK_VERSION
    } else if workbook.has_inline_text() {
        INLINE_WORKBOOK_VERSION
    } else {
        return workbook.serialize(serializer);
    };
    let mut tuple = serializer.serialize_tuple(2)?;
    tuple.serialize_element(version)?;
    tuple.serialize_element(workbook)?;
    tuple.end()
}

fn deserialize_workbook<'de, D>(deserializer: D) -> std::result::Result<SnapshotWorkbook, D::Error>
where
    D: Deserializer<'de>,
{
    struct WorkbookVisitor;
    impl<'de> Visitor<'de> for WorkbookVisitor {
        type Value = SnapshotWorkbook;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a legacy workbook or a versioned recovery workbook")
        }

        fn visit_map<A>(self, map: A) -> std::result::Result<SnapshotWorkbook, A::Error>
        where
            A: MapAccess<'de>,
        {
            let workbook = Workbook::deserialize(MapAccessDeserializer::new(map))?;
            if workbook.has_drawio_topic_source_ids() {
                return Err(A::Error::custom(
                    "draw.io topic source IDs require a topic-ID-aware recovery workbook",
                ));
            }
            if workbook.has_drawio_zone_source_ids() {
                return Err(A::Error::custom(
                    "draw.io zone source IDs require a zone-ID-aware recovery workbook",
                ));
            }
            if workbook.drawio.is_some() {
                return Err(A::Error::custom(
                    "draw.io document metadata requires a document-aware recovery workbook",
                ));
            }
            if workbook.has_card_baseline_overrides() {
                return Err(A::Error::custom(
                    "card baseline overrides require a card-baseline-aware recovery workbook",
                ));
            }
            if workbook.has_rich_labels() {
                return Err(A::Error::custom(
                    "rich labels require a rich-label-aware recovery workbook",
                ));
            }
            if workbook.has_label_styles() {
                return Err(A::Error::custom(
                    "label styles require a label-style-aware recovery workbook",
                ));
            }
            if workbook.has_shape_styles() {
                return Err(A::Error::custom(
                    "shape styles require a shape-style-aware recovery workbook",
                ));
            }
            if workbook.has_tables() {
                return Err(A::Error::custom(
                    "tables require a table-aware recovery workbook",
                ));
            }
            if workbook.has_layers() {
                return Err(A::Error::custom(
                    "layers require a layer-aware recovery workbook",
                ));
            }
            if workbook.has_object_data() {
                return Err(A::Error::custom(
                    "object data requires an object-data-aware recovery workbook",
                ));
            }
            if workbook.has_connector_labels() {
                return Err(A::Error::custom(
                    "connector labels require a connector-label-aware recovery workbook",
                ));
            }
            if workbook.has_connector_edge_shapes() {
                return Err(A::Error::custom(
                    "connector edge shapes require an edge-shape-aware recovery workbook",
                ));
            }
            if workbook.has_drawio_segment_routes() {
                return Err(A::Error::custom(
                    "draw.io segment routes require a segment-route-aware recovery workbook",
                ));
            }
            if workbook.has_drawio_edge_routes() {
                return Err(A::Error::custom(
                    "draw.io edge routes require an edge-route-aware recovery workbook",
                ));
            }
            if workbook.has_connector_line_jumps() {
                return Err(A::Error::custom(
                    "connector line jumps require a line-jump-aware recovery workbook",
                ));
            }
            if workbook.has_library_connector_markers() {
                return Err(A::Error::custom(
                    "draw.io library connector markers require a marker-library-aware recovery workbook",
                ));
            }
            if workbook.has_scene_hierarchy() {
                return Err(A::Error::custom(
                    "scene hierarchy requires scene-aware recovery",
                ));
            }
            if workbook.has_semantic_model() {
                return Err(A::Error::custom(
                    "socio-technical data requires a semantic-aware recovery workbook",
                ));
            }
            if workbook.has_stencils() {
                return Err(A::Error::custom(
                    "stencil shapes require a versioned recovery workbook",
                ));
            }
            if workbook.has_sankey() {
                return Err(A::Error::custom(
                    "Sankey flows require a versioned recovery workbook",
                ));
            }
            if workbook.has_final_states() {
                return Err(A::Error::custom(
                    "final states require a versioned recovery workbook",
                ));
            }
            if workbook.has_fixed_connector_ports() {
                return Err(A::Error::custom(
                    "fixed connector ports require a versioned recovery workbook",
                ));
            }
            if workbook.has_double_ellipses() {
                return Err(A::Error::custom(
                    "double ellipses require a versioned recovery workbook",
                ));
            }
            if workbook.has_rectangle_sides() {
                return Err(A::Error::custom(
                    "rectangle outline sides require a versioned recovery workbook",
                ));
            }
            if workbook.has_shape_rounding() {
                return Err(A::Error::custom(
                    "shape rounding requires a versioned recovery workbook",
                ));
            }
            if workbook.has_connector_markers() {
                return Err(A::Error::custom(
                    "connector markers require a versioned recovery workbook",
                ));
            }
            if workbook.has_connector_waypoints() {
                return Err(A::Error::custom(
                    "connector waypoints require a versioned recovery workbook",
                ));
            }
            Ok(SnapshotWorkbook {
                workbook,
                pending_ink: None,
            })
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<SnapshotWorkbook, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let version: String = seq
                .next_element()?
                .ok_or_else(|| A::Error::custom("missing recovery workbook version"))?;
            if version != INLINE_WORKBOOK_VERSION
                && version != WAYPOINT_WORKBOOK_VERSION
                && version != INK_WORKBOOK_VERSION
                && version != MARKER_WORKBOOK_VERSION
                && version != MARKER_INK_WORKBOOK_VERSION
                && version != EXTENDED_MARKER_WORKBOOK_VERSION
                && version != EXTENDED_MARKER_INK_WORKBOOK_VERSION
                && version != SIZED_MARKER_WORKBOOK_VERSION
                && version != SIZED_MARKER_INK_WORKBOOK_VERSION
                && version != FILLED_MARKER_WORKBOOK_VERSION
                && version != FILLED_MARKER_INK_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_INK_WORKBOOK_VERSION
                && version != OVAL_MARKER_WORKBOOK_VERSION
                && version != OVAL_MARKER_INK_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && version != RECTANGLE_SIDES_WORKBOOK_VERSION
                && version != RECTANGLE_SIDES_INK_WORKBOOK_VERSION
                && version != DOUBLE_ELLIPSE_WORKBOOK_VERSION
                && version != DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION
                && version != FIXED_PORT_WORKBOOK_VERSION
                && version != FIXED_PORT_INK_WORKBOOK_VERSION
                && version != DRAWIO_ROUNDING_WORKBOOK_VERSION
                && version != DRAWIO_ROUNDING_INK_WORKBOOK_VERSION
                && version != FINAL_STATE_WORKBOOK_VERSION
                && version != FINAL_STATE_INK_WORKBOOK_VERSION
                && version != SANKEY_WORKBOOK_VERSION
                && version != SANKEY_INK_WORKBOOK_VERSION
                && version != STENCIL_WORKBOOK_VERSION
                && version != STENCIL_INK_WORKBOOK_VERSION
                && version != SCENE_WORKBOOK_VERSION
                && version != SCENE_INK_WORKBOOK_VERSION
                && version != PINNED_STENCIL_WORKBOOK_VERSION
                && version != PINNED_STENCIL_INK_WORKBOOK_VERSION
                && version != CLOSED_STENCIL_WORKBOOK_VERSION
                && version != CLOSED_STENCIL_INK_WORKBOOK_VERSION
                && version != SCENE_STENCIL_WORKBOOK_VERSION
                && version != SCENE_STENCIL_INK_WORKBOOK_VERSION
                && version != COLLAPSE_WORKBOOK_VERSION
                && version != COLLAPSE_INK_WORKBOOK_VERSION
                && version != SCENE_LAYOUT_WORKBOOK_VERSION
                && version != SCENE_LAYOUT_INK_WORKBOOK_VERSION
                && version != SCENE_STENCIL_LAYOUT_WORKBOOK_VERSION
                && version != SCENE_STENCIL_LAYOUT_INK_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION
                && version != SEMANTIC_WORKBOOK_VERSION
                && version != SEMANTIC_INK_WORKBOOK_VERSION
                && version != MARKER_LIBRARY_WORKBOOK_VERSION
                && version != MARKER_LIBRARY_INK_WORKBOOK_VERSION
                && version != LINE_JUMPS_WORKBOOK_VERSION
                && version != LINE_JUMPS_INK_WORKBOOK_VERSION
                && version != EDGE_ROUTES_WORKBOOK_VERSION
                && version != EDGE_ROUTES_INK_WORKBOOK_VERSION
                && version != SEGMENT_ROUTES_WORKBOOK_VERSION
                && version != SEGMENT_ROUTES_INK_WORKBOOK_VERSION
                && version != EDGE_SHAPES_WORKBOOK_VERSION
                && version != EDGE_SHAPES_INK_WORKBOOK_VERSION
                && version != CONNECTOR_LABELS_WORKBOOK_VERSION
                && version != CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                && version != OBJECT_DATA_WORKBOOK_VERSION
                && version != OBJECT_DATA_INK_WORKBOOK_VERSION
                && version != LAYERS_WORKBOOK_VERSION
                && version != LAYERS_INK_WORKBOOK_VERSION
                && version != TABLES_WORKBOOK_VERSION
                && version != TABLES_INK_WORKBOOK_VERSION
                && version != SHAPE_STYLES_WORKBOOK_VERSION
                && version != SHAPE_STYLES_INK_WORKBOOK_VERSION
                && version != LABEL_STYLES_WORKBOOK_VERSION
                && version != LABEL_STYLES_INK_WORKBOOK_VERSION
                && version != RICH_LABELS_WORKBOOK_VERSION
                && version != RICH_LABELS_INK_WORKBOOK_VERSION
                && version != RICH_CONNECTOR_LABELS_WORKBOOK_VERSION
                && version != RICH_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                && version != RICH_EXTRA_CONNECTOR_LABELS_WORKBOOK_VERSION
                && version != RICH_EXTRA_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                && version != CARD_BASELINE_WORKBOOK_VERSION
                && version != CARD_BASELINE_INK_WORKBOOK_VERSION
                && version != DRAWIO_DOCUMENT_WORKBOOK_VERSION
                && version != DRAWIO_DOCUMENT_INK_WORKBOOK_VERSION
                && version != DRAWIO_ZONE_IDS_WORKBOOK_VERSION
                && version != DRAWIO_ZONE_IDS_INK_WORKBOOK_VERSION
                && version != DRAWIO_TOPIC_IDS_WORKBOOK_VERSION
                && version != DRAWIO_TOPIC_IDS_INK_WORKBOOK_VERSION
            {
                return Err(A::Error::custom(format!(
                    "unsupported recovery workbook version {version}"
                )));
            }
            let has_pending_ink = version.contains("-INK-");
            let workbook: Workbook = seq
                .next_element()?
                .ok_or_else(|| A::Error::custom("missing recovery workbook"))?;
            let topic_envelope = version == DRAWIO_TOPIC_IDS_WORKBOOK_VERSION
                || version == DRAWIO_TOPIC_IDS_INK_WORKBOOK_VERSION;
            if workbook.has_drawio_topic_source_ids() != topic_envelope {
                return Err(A::Error::custom(
                    "draw.io topic source IDs require a topic-ID-aware recovery workbook",
                ));
            }
            if topic_envelope {
                return finish_recovery_workbook(seq, workbook, has_pending_ink);
            }
            let zone_envelope = version == DRAWIO_ZONE_IDS_WORKBOOK_VERSION
                || version == DRAWIO_ZONE_IDS_INK_WORKBOOK_VERSION;
            if workbook.has_drawio_zone_source_ids() != zone_envelope {
                return Err(A::Error::custom(
                    "draw.io zone source IDs require a zone-ID-aware recovery workbook",
                ));
            }
            if zone_envelope {
                return finish_recovery_workbook(seq, workbook, has_pending_ink);
            }
            let document_envelope = version == DRAWIO_DOCUMENT_WORKBOOK_VERSION
                || version == DRAWIO_DOCUMENT_INK_WORKBOOK_VERSION;
            if workbook.drawio.is_some() != document_envelope {
                return Err(A::Error::custom(
                    "draw.io document metadata requires a document-aware recovery workbook",
                ));
            }
            if document_envelope {
                return finish_recovery_workbook(seq, workbook, has_pending_ink);
            }
            let card_baseline_envelope = version == CARD_BASELINE_WORKBOOK_VERSION
                || version == CARD_BASELINE_INK_WORKBOOK_VERSION;
            if workbook.has_card_baseline_overrides() != card_baseline_envelope {
                return Err(A::Error::custom(
                    "card baseline overrides require a card-baseline-aware recovery workbook",
                ));
            }
            let rich_extra_connector_labels_envelope = version
                == RICH_EXTRA_CONNECTOR_LABELS_WORKBOOK_VERSION
                || version == RICH_EXTRA_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                || card_baseline_envelope && workbook.has_rich_extra_connector_labels();
            if workbook.has_rich_extra_connector_labels() != rich_extra_connector_labels_envelope {
                return Err(A::Error::custom(
                    "rich extra connector labels require a rich-extra-connector-label-aware recovery workbook",
                ));
            }
            let rich_connector_labels_envelope = version == RICH_CONNECTOR_LABELS_WORKBOOK_VERSION
                || version == RICH_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                || (rich_extra_connector_labels_envelope || card_baseline_envelope)
                    && workbook.has_rich_connector_labels();
            if workbook.has_rich_connector_labels() != rich_connector_labels_envelope {
                return Err(A::Error::custom(
                    "rich connector labels require a rich-connector-label-aware recovery workbook",
                ));
            }
            let rich_labels_envelope = version == RICH_LABELS_WORKBOOK_VERSION
                || version == RICH_LABELS_INK_WORKBOOK_VERSION
                || (rich_connector_labels_envelope
                    || rich_extra_connector_labels_envelope
                    || card_baseline_envelope)
                    && workbook.has_rich_labels();
            if workbook.has_rich_labels() != rich_labels_envelope {
                return Err(A::Error::custom(
                    "rich labels require a rich-label-aware recovery workbook",
                ));
            }
            let label_styles_envelope = version == LABEL_STYLES_WORKBOOK_VERSION
                || version == LABEL_STYLES_INK_WORKBOOK_VERSION
                || (rich_labels_envelope
                    || rich_connector_labels_envelope
                    || rich_extra_connector_labels_envelope
                    || card_baseline_envelope)
                    && workbook.has_label_styles();
            if workbook.has_label_styles() != label_styles_envelope {
                return Err(A::Error::custom(
                    "label styles require a label-style-aware recovery workbook",
                ));
            }
            let version = if !label_styles_envelope
                && !rich_labels_envelope
                && !rich_connector_labels_envelope
                && !rich_extra_connector_labels_envelope
                && !card_baseline_envelope
            {
                version
            } else {
                let (plain, ink) = if workbook.has_shape_styles() {
                    (
                        SHAPE_STYLES_WORKBOOK_VERSION,
                        SHAPE_STYLES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_tables() {
                    (TABLES_WORKBOOK_VERSION, TABLES_INK_WORKBOOK_VERSION)
                } else if workbook.has_layers() {
                    (LAYERS_WORKBOOK_VERSION, LAYERS_INK_WORKBOOK_VERSION)
                } else if workbook.has_object_data() {
                    (
                        OBJECT_DATA_WORKBOOK_VERSION,
                        OBJECT_DATA_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_labels() {
                    (
                        CONNECTOR_LABELS_WORKBOOK_VERSION,
                        CONNECTOR_LABELS_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_edge_shapes() {
                    (
                        EDGE_SHAPES_WORKBOOK_VERSION,
                        EDGE_SHAPES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_segment_routes() {
                    (
                        SEGMENT_ROUTES_WORKBOOK_VERSION,
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let shape_styles_envelope = version == SHAPE_STYLES_WORKBOOK_VERSION
                || version == SHAPE_STYLES_INK_WORKBOOK_VERSION;
            if workbook.has_shape_styles() != shape_styles_envelope {
                return Err(A::Error::custom(
                    "shape styles require a shape-style-aware recovery workbook",
                ));
            }
            let version = if !shape_styles_envelope {
                version
            } else {
                let (plain, ink) = if workbook.has_tables() {
                    (TABLES_WORKBOOK_VERSION, TABLES_INK_WORKBOOK_VERSION)
                } else if workbook.has_layers() {
                    (LAYERS_WORKBOOK_VERSION, LAYERS_INK_WORKBOOK_VERSION)
                } else if workbook.has_object_data() {
                    (
                        OBJECT_DATA_WORKBOOK_VERSION,
                        OBJECT_DATA_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_labels() {
                    (
                        CONNECTOR_LABELS_WORKBOOK_VERSION,
                        CONNECTOR_LABELS_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_edge_shapes() {
                    (
                        EDGE_SHAPES_WORKBOOK_VERSION,
                        EDGE_SHAPES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_segment_routes() {
                    (
                        SEGMENT_ROUTES_WORKBOOK_VERSION,
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let tables_envelope =
                version == TABLES_WORKBOOK_VERSION || version == TABLES_INK_WORKBOOK_VERSION;
            if workbook.has_tables() != tables_envelope {
                return Err(A::Error::custom(
                    "tables require a table-aware recovery workbook",
                ));
            }
            let version = if !tables_envelope {
                version
            } else {
                let (plain, ink) = if workbook.has_layers() {
                    (LAYERS_WORKBOOK_VERSION, LAYERS_INK_WORKBOOK_VERSION)
                } else if workbook.has_object_data() {
                    (
                        OBJECT_DATA_WORKBOOK_VERSION,
                        OBJECT_DATA_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_labels() {
                    (
                        CONNECTOR_LABELS_WORKBOOK_VERSION,
                        CONNECTOR_LABELS_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_edge_shapes() {
                    (
                        EDGE_SHAPES_WORKBOOK_VERSION,
                        EDGE_SHAPES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_segment_routes() {
                    (
                        SEGMENT_ROUTES_WORKBOOK_VERSION,
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let layers_envelope =
                version == LAYERS_WORKBOOK_VERSION || version == LAYERS_INK_WORKBOOK_VERSION;
            if workbook.has_layers() != layers_envelope {
                return Err(A::Error::custom(
                    "layers require a layer-aware recovery workbook",
                ));
            }
            let version = if !layers_envelope {
                version
            } else {
                let (plain, ink) = if workbook.has_object_data() {
                    (
                        OBJECT_DATA_WORKBOOK_VERSION,
                        OBJECT_DATA_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_labels() {
                    (
                        CONNECTOR_LABELS_WORKBOOK_VERSION,
                        CONNECTOR_LABELS_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_edge_shapes() {
                    (
                        EDGE_SHAPES_WORKBOOK_VERSION,
                        EDGE_SHAPES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_segment_routes() {
                    (
                        SEGMENT_ROUTES_WORKBOOK_VERSION,
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let object_data_envelope = version == OBJECT_DATA_WORKBOOK_VERSION
                || version == OBJECT_DATA_INK_WORKBOOK_VERSION;
            if workbook.has_object_data() != object_data_envelope {
                return Err(A::Error::custom(
                    "object data requires an object-data-aware recovery workbook",
                ));
            }
            let version = if !object_data_envelope {
                version
            } else {
                let (plain, ink) = if workbook.has_connector_labels() {
                    (
                        CONNECTOR_LABELS_WORKBOOK_VERSION,
                        CONNECTOR_LABELS_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_edge_shapes() {
                    (
                        EDGE_SHAPES_WORKBOOK_VERSION,
                        EDGE_SHAPES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_segment_routes() {
                    (
                        SEGMENT_ROUTES_WORKBOOK_VERSION,
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let connector_labels_envelope = version == CONNECTOR_LABELS_WORKBOOK_VERSION
                || version == CONNECTOR_LABELS_INK_WORKBOOK_VERSION;
            if workbook.has_connector_labels() != connector_labels_envelope {
                return Err(A::Error::custom(
                    "connector labels require a connector-label-aware recovery workbook",
                ));
            }
            let version = if !connector_labels_envelope {
                version
            } else {
                let (plain, ink) = if workbook.has_connector_edge_shapes() {
                    (
                        EDGE_SHAPES_WORKBOOK_VERSION,
                        EDGE_SHAPES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_segment_routes() {
                    (
                        SEGMENT_ROUTES_WORKBOOK_VERSION,
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let edge_shapes_envelope = version == EDGE_SHAPES_WORKBOOK_VERSION
                || version == EDGE_SHAPES_INK_WORKBOOK_VERSION;
            if workbook.has_connector_edge_shapes() != edge_shapes_envelope {
                return Err(A::Error::custom(
                    "connector edge shapes require an edge-shape-aware recovery workbook",
                ));
            }
            // Map the edge-shape envelope down to the next older one, so the
            // older checks still run.
            let version = if !edge_shapes_envelope {
                version
            } else {
                let (plain, ink) = if workbook.has_drawio_segment_routes() {
                    (
                        SEGMENT_ROUTES_WORKBOOK_VERSION,
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let segment_routes_envelope = version == SEGMENT_ROUTES_WORKBOOK_VERSION
                || version == SEGMENT_ROUTES_INK_WORKBOOK_VERSION;
            if workbook.has_drawio_segment_routes() != segment_routes_envelope {
                return Err(A::Error::custom(
                    "draw.io segment routes require a segment-route-aware recovery workbook",
                ));
            }
            // Map the segment envelope down to the next older one the
            // workbook's other content selects, so the older checks still run.
            let version = if !segment_routes_envelope {
                version
            } else {
                let (plain, ink) = if workbook.has_drawio_edge_routes() {
                    (
                        EDGE_ROUTES_WORKBOOK_VERSION,
                        EDGE_ROUTES_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_connector_line_jumps() {
                    (LINE_JUMPS_WORKBOOK_VERSION, LINE_JUMPS_INK_WORKBOOK_VERSION)
                } else if workbook.has_library_connector_markers() {
                    (
                        MARKER_LIBRARY_WORKBOOK_VERSION,
                        MARKER_LIBRARY_INK_WORKBOOK_VERSION,
                    )
                } else if workbook.has_semantic_model() {
                    (SEMANTIC_WORKBOOK_VERSION, SEMANTIC_INK_WORKBOOK_VERSION)
                } else {
                    (
                        COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                        COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                    )
                };
                if has_pending_ink { ink } else { plain }.to_owned()
            };
            let edge_routes_envelope = version == EDGE_ROUTES_WORKBOOK_VERSION
                || version == EDGE_ROUTES_INK_WORKBOOK_VERSION;
            if workbook.has_drawio_edge_routes() != edge_routes_envelope {
                return Err(A::Error::custom(
                    "draw.io edge routes require an edge-route-aware recovery workbook",
                ));
            }
            let version = match (
                edge_routes_envelope,
                workbook.has_connector_line_jumps(),
                workbook.has_library_connector_markers(),
                workbook.has_semantic_model(),
                has_pending_ink,
            ) {
                (false, ..) => version,
                (true, true, _, _, true) => LINE_JUMPS_INK_WORKBOOK_VERSION.to_owned(),
                (true, true, _, _, false) => LINE_JUMPS_WORKBOOK_VERSION.to_owned(),
                (true, false, true, _, true) => MARKER_LIBRARY_INK_WORKBOOK_VERSION.to_owned(),
                (true, false, true, _, false) => MARKER_LIBRARY_WORKBOOK_VERSION.to_owned(),
                (true, false, false, true, true) => SEMANTIC_INK_WORKBOOK_VERSION.to_owned(),
                (true, false, false, true, false) => SEMANTIC_WORKBOOK_VERSION.to_owned(),
                (true, false, false, false, true) => {
                    COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION.to_owned()
                }
                (true, false, false, false, false) => COLLAPSE_LAYOUT_WORKBOOK_VERSION.to_owned(),
            };
            let line_jumps_envelope = version == LINE_JUMPS_WORKBOOK_VERSION
                || version == LINE_JUMPS_INK_WORKBOOK_VERSION;
            if workbook.has_connector_line_jumps() != line_jumps_envelope {
                return Err(A::Error::custom(
                    "connector line jumps require a line-jump-aware recovery workbook",
                ));
            }
            let version = match (
                line_jumps_envelope,
                workbook.has_library_connector_markers(),
                workbook.has_semantic_model(),
                has_pending_ink,
            ) {
                (false, _, _, _) => version,
                (true, true, _, true) => MARKER_LIBRARY_INK_WORKBOOK_VERSION.to_owned(),
                (true, true, _, false) => MARKER_LIBRARY_WORKBOOK_VERSION.to_owned(),
                (true, false, true, true) => SEMANTIC_INK_WORKBOOK_VERSION.to_owned(),
                (true, false, true, false) => SEMANTIC_WORKBOOK_VERSION.to_owned(),
                (true, false, false, true) => COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION.to_owned(),
                (true, false, false, false) => COLLAPSE_LAYOUT_WORKBOOK_VERSION.to_owned(),
            };
            let marker_library_envelope = version == MARKER_LIBRARY_WORKBOOK_VERSION
                || version == MARKER_LIBRARY_INK_WORKBOOK_VERSION;
            if workbook.has_library_connector_markers() != marker_library_envelope {
                return Err(A::Error::custom(
                    "draw.io library connector markers require a marker-library-aware recovery workbook",
                ));
            }
            let version = match (
                marker_library_envelope,
                workbook.has_semantic_model(),
                has_pending_ink,
            ) {
                (false, _, _) => version,
                (true, true, true) => SEMANTIC_INK_WORKBOOK_VERSION.to_owned(),
                (true, true, false) => SEMANTIC_WORKBOOK_VERSION.to_owned(),
                (true, false, true) => COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION.to_owned(),
                (true, false, false) => COLLAPSE_LAYOUT_WORKBOOK_VERSION.to_owned(),
            };
            let semantic_envelope =
                version == SEMANTIC_WORKBOOK_VERSION || version == SEMANTIC_INK_WORKBOOK_VERSION;
            if workbook.has_semantic_model() != semantic_envelope {
                return Err(A::Error::custom(
                    "socio-technical data requires a semantic-aware recovery workbook",
                ));
            }
            let version = if semantic_envelope {
                if has_pending_ink {
                    COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION.to_owned()
                } else {
                    COLLAPSE_LAYOUT_WORKBOOK_VERSION.to_owned()
                }
            } else {
                version
            };
            if version != COLLAPSE_WORKBOOK_VERSION
                && version != COLLAPSE_INK_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION
                && workbook.has_scene_collapse()
            {
                return Err(A::Error::custom(
                    "collapsed containers require collapse-aware recovery",
                ));
            }
            if version != SCENE_WORKBOOK_VERSION
                && version != SCENE_INK_WORKBOOK_VERSION
                && version != SCENE_STENCIL_WORKBOOK_VERSION
                && version != SCENE_STENCIL_INK_WORKBOOK_VERSION
                && version != COLLAPSE_WORKBOOK_VERSION
                && version != COLLAPSE_INK_WORKBOOK_VERSION
                && version != SCENE_LAYOUT_WORKBOOK_VERSION
                && version != SCENE_LAYOUT_INK_WORKBOOK_VERSION
                && version != SCENE_STENCIL_LAYOUT_WORKBOOK_VERSION
                && version != SCENE_STENCIL_LAYOUT_INK_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION
                && workbook.has_scene_hierarchy()
            {
                return Err(A::Error::custom(
                    "scene hierarchy requires scene-aware recovery",
                ));
            }
            if version != SCENE_LAYOUT_WORKBOOK_VERSION
                && version != SCENE_LAYOUT_INK_WORKBOOK_VERSION
                && version != SCENE_STENCIL_LAYOUT_WORKBOOK_VERSION
                && version != SCENE_STENCIL_LAYOUT_INK_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_WORKBOOK_VERSION
                && version != COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION
                && workbook.has_scene_layout_policies()
            {
                return Err(A::Error::custom(
                    "scene layout policies require layout-aware recovery",
                ));
            }
            let version = match version.as_str() {
                SCENE_WORKBOOK_VERSION | SCENE_LAYOUT_WORKBOOK_VERSION => {
                    STENCIL_WORKBOOK_VERSION.to_owned()
                }
                SCENE_INK_WORKBOOK_VERSION | SCENE_LAYOUT_INK_WORKBOOK_VERSION => {
                    STENCIL_INK_WORKBOOK_VERSION.to_owned()
                }
                SCENE_STENCIL_WORKBOOK_VERSION
                | COLLAPSE_WORKBOOK_VERSION
                | SCENE_STENCIL_LAYOUT_WORKBOOK_VERSION
                | COLLAPSE_LAYOUT_WORKBOOK_VERSION => CLOSED_STENCIL_WORKBOOK_VERSION.to_owned(),
                SCENE_STENCIL_INK_WORKBOOK_VERSION
                | COLLAPSE_INK_WORKBOOK_VERSION
                | SCENE_STENCIL_LAYOUT_INK_WORKBOOK_VERSION
                | COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION => {
                    CLOSED_STENCIL_INK_WORKBOOK_VERSION.to_owned()
                }
                _ => version,
            };
            if version != STENCIL_WORKBOOK_VERSION
                && version != STENCIL_INK_WORKBOOK_VERSION
                && version != PINNED_STENCIL_WORKBOOK_VERSION
                && version != PINNED_STENCIL_INK_WORKBOOK_VERSION
                && version != CLOSED_STENCIL_WORKBOOK_VERSION
                && version != CLOSED_STENCIL_INK_WORKBOOK_VERSION
                && workbook.has_stencils()
            {
                return Err(A::Error::custom(
                    "stencil shapes require stencil-aware recovery",
                ));
            }
            if version != PINNED_STENCIL_WORKBOOK_VERSION
                && version != PINNED_STENCIL_INK_WORKBOOK_VERSION
                && version != CLOSED_STENCIL_WORKBOOK_VERSION
                && version != CLOSED_STENCIL_INK_WORKBOOK_VERSION
                && workbook.has_stencil_definition_hashes()
            {
                return Err(A::Error::custom(
                    "pinned stencil definitions require pinned-stencil-aware recovery",
                ));
            }
            if version != CLOSED_STENCIL_WORKBOOK_VERSION
                && version != CLOSED_STENCIL_INK_WORKBOOK_VERSION
                && workbook.has_stencil_definition_closures()
            {
                return Err(A::Error::custom(
                    "stencil definition closures require closure-aware recovery",
                ));
            }
            let version = match version.as_str() {
                STENCIL_WORKBOOK_VERSION
                | PINNED_STENCIL_WORKBOOK_VERSION
                | CLOSED_STENCIL_WORKBOOK_VERSION => SANKEY_WORKBOOK_VERSION.to_owned(),
                STENCIL_INK_WORKBOOK_VERSION
                | PINNED_STENCIL_INK_WORKBOOK_VERSION
                | CLOSED_STENCIL_INK_WORKBOOK_VERSION => SANKEY_INK_WORKBOOK_VERSION.to_owned(),
                _ => version,
            };
            if version != SANKEY_WORKBOOK_VERSION
                && version != SANKEY_INK_WORKBOOK_VERSION
                && workbook.has_sankey()
            {
                return Err(A::Error::custom(
                    "Sankey flows require Sankey-aware recovery",
                ));
            }
            let version = match version.as_str() {
                SANKEY_WORKBOOK_VERSION => FINAL_STATE_WORKBOOK_VERSION.to_owned(),
                SANKEY_INK_WORKBOOK_VERSION => FINAL_STATE_INK_WORKBOOK_VERSION.to_owned(),
                _ => version,
            };
            if version != FINAL_STATE_WORKBOOK_VERSION
                && version != FINAL_STATE_INK_WORKBOOK_VERSION
                && workbook.has_final_states()
            {
                return Err(A::Error::custom(
                    "final states require a final-state-aware recovery workbook",
                ));
            }
            let version = match version.as_str() {
                FINAL_STATE_WORKBOOK_VERSION => DRAWIO_ROUNDING_WORKBOOK_VERSION,
                FINAL_STATE_INK_WORKBOOK_VERSION => DRAWIO_ROUNDING_INK_WORKBOOK_VERSION,
                version => version,
            };
            if version != DRAWIO_ROUNDING_WORKBOOK_VERSION
                && version != DRAWIO_ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_drawio_rectangle_rounding()
            {
                return Err(A::Error::custom(
                    "standard draw.io rectangle rounding requires a rounding-aware recovery workbook",
                ));
            }
            let version = match version {
                DRAWIO_ROUNDING_WORKBOOK_VERSION => FIXED_PORT_WORKBOOK_VERSION,
                DRAWIO_ROUNDING_INK_WORKBOOK_VERSION => FIXED_PORT_INK_WORKBOOK_VERSION,
                version => version,
            };
            if version != FIXED_PORT_WORKBOOK_VERSION
                && version != FIXED_PORT_INK_WORKBOOK_VERSION
                && workbook.has_fixed_connector_ports()
            {
                return Err(A::Error::custom(
                    "fixed connector ports require a port-aware recovery workbook",
                ));
            }
            let version = match version {
                FIXED_PORT_WORKBOOK_VERSION => DOUBLE_ELLIPSE_WORKBOOK_VERSION,
                FIXED_PORT_INK_WORKBOOK_VERSION => DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION,
                version => version,
            };
            if version != DOUBLE_ELLIPSE_WORKBOOK_VERSION
                && version != DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION
                && workbook.has_double_ellipses()
            {
                return Err(A::Error::custom(
                    "double ellipses require a double-ellipse-aware recovery workbook",
                ));
            }
            let version = match version {
                DOUBLE_ELLIPSE_WORKBOOK_VERSION => RECTANGLE_SIDES_WORKBOOK_VERSION,
                DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION => RECTANGLE_SIDES_INK_WORKBOOK_VERSION,
                version => version,
            };
            if version != RECTANGLE_SIDES_WORKBOOK_VERSION
                && version != RECTANGLE_SIDES_INK_WORKBOOK_VERSION
                && workbook.has_rectangle_sides()
            {
                return Err(A::Error::custom(
                    "rectangle outline sides require a partial-rectangle-aware recovery workbook",
                ));
            }
            // These envelopes also support every earlier rounding-era feature.
            let version = match version {
                RECTANGLE_SIDES_WORKBOOK_VERSION => ROUNDING_WORKBOOK_VERSION,
                RECTANGLE_SIDES_INK_WORKBOOK_VERSION => ROUNDING_INK_WORKBOOK_VERSION,
                version => version,
            };
            if version != MARKER_WORKBOOK_VERSION
                && version != MARKER_INK_WORKBOOK_VERSION
                && version != EXTENDED_MARKER_WORKBOOK_VERSION
                && version != EXTENDED_MARKER_INK_WORKBOOK_VERSION
                && version != SIZED_MARKER_WORKBOOK_VERSION
                && version != SIZED_MARKER_INK_WORKBOOK_VERSION
                && version != FILLED_MARKER_WORKBOOK_VERSION
                && version != FILLED_MARKER_INK_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_INK_WORKBOOK_VERSION
                && version != OVAL_MARKER_WORKBOOK_VERSION
                && version != OVAL_MARKER_INK_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_connector_markers()
            {
                return Err(A::Error::custom(
                    "connector markers require a marker-aware recovery workbook",
                ));
            }
            if version != EXTENDED_MARKER_WORKBOOK_VERSION
                && version != EXTENDED_MARKER_INK_WORKBOOK_VERSION
                && version != SIZED_MARKER_WORKBOOK_VERSION
                && version != SIZED_MARKER_INK_WORKBOOK_VERSION
                && version != FILLED_MARKER_WORKBOOK_VERSION
                && version != FILLED_MARKER_INK_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_INK_WORKBOOK_VERSION
                && version != OVAL_MARKER_WORKBOOK_VERSION
                && version != OVAL_MARKER_INK_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_extended_connector_markers()
            {
                return Err(A::Error::custom(
                    "classic and thin connector markers require an extended-marker recovery workbook",
                ));
            }
            if version != SIZED_MARKER_WORKBOOK_VERSION
                && version != SIZED_MARKER_INK_WORKBOOK_VERSION
                && version != FILLED_MARKER_WORKBOOK_VERSION
                && version != FILLED_MARKER_INK_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_INK_WORKBOOK_VERSION
                && version != OVAL_MARKER_WORKBOOK_VERSION
                && version != OVAL_MARKER_INK_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_connector_marker_sizes()
            {
                return Err(A::Error::custom(
                    "connector marker sizes require a size-aware recovery workbook",
                ));
            }
            if version != FILLED_MARKER_WORKBOOK_VERSION
                && version != FILLED_MARKER_INK_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_INK_WORKBOOK_VERSION
                && version != OVAL_MARKER_WORKBOOK_VERSION
                && version != OVAL_MARKER_INK_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_connector_marker_fills()
            {
                return Err(A::Error::custom(
                    "connector marker fills require a fill-aware recovery workbook",
                ));
            }
            if version != DIAMOND_MARKER_WORKBOOK_VERSION
                && version != DIAMOND_MARKER_INK_WORKBOOK_VERSION
                && version != OVAL_MARKER_WORKBOOK_VERSION
                && version != OVAL_MARKER_INK_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_diamond_connector_markers()
            {
                return Err(A::Error::custom(
                    "diamond connector markers require a diamond-aware recovery workbook",
                ));
            }
            if version != OVAL_MARKER_WORKBOOK_VERSION
                && version != OVAL_MARKER_INK_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_oval_connector_markers()
            {
                return Err(A::Error::custom(
                    "oval connector markers require an oval-aware recovery workbook",
                ));
            }
            if version != HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                && version != HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                && version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_half_circle_connector_markers()
            {
                return Err(A::Error::custom(
                    "half-circle connector markers require a half-circle-aware recovery workbook",
                ));
            }
            if version != BAR_MARKER_WORKBOOK_VERSION
                && version != BAR_MARKER_INK_WORKBOOK_VERSION
                && version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_bar_connector_markers()
            {
                return Err(A::Error::custom(
                    "bar connector markers require a bar-aware recovery workbook",
                ));
            }
            if version != ROUNDING_WORKBOOK_VERSION
                && version != ROUNDING_INK_WORKBOOK_VERSION
                && workbook.has_shape_rounding()
            {
                return Err(A::Error::custom(
                    "shape rounding requires a rounding-aware recovery workbook",
                ));
            }
            if version == INLINE_WORKBOOK_VERSION && workbook.has_connector_waypoints() {
                return Err(A::Error::custom(
                    "connector waypoints require a waypoint-aware recovery workbook",
                ));
            }
            finish_recovery_workbook(seq, workbook, has_pending_ink)
        }
    }

    deserializer.deserialize_any(WorkbookVisitor)
}

impl Snapshot {
    /// Serialize a recovery envelope, without writing or taking a recovery lock.
    pub fn to_bytes(meta: SnapshotMeta, workbook: &Workbook) -> Result<Vec<u8>> {
        Self::to_bytes_with_optional_ink(meta, workbook, None)
    }

    /// Serialize a snapshot whose application-owned ink data must not be discarded
    /// by readers that understand only older workbook versions.
    pub fn to_bytes_with_pending_ink(
        meta: SnapshotMeta,
        workbook: &Workbook,
        pending_ink: &str,
    ) -> Result<Vec<u8>> {
        Self::to_bytes_with_optional_ink(meta, workbook, Some(pending_ink))
    }

    fn to_bytes_with_optional_ink(
        meta: SnapshotMeta,
        workbook: &Workbook,
        pending_ink: Option<&str>,
    ) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&BorrowedSnapshot {
            meta: &meta,
            workbook: BorrowedWorkbook {
                workbook,
                pending_ink,
            },
        })?)
    }
}

#[derive(Serialize)]
struct SessionInfo {
    pid: u32,
    started_at: u64,
    app_version: &'static str,
}

struct State {
    lock: Option<Lock>,
    finished: bool,
}

/// A live recovery session. Share an `Arc` with the panic hook and explicitly
/// call `finish` after all documents have closed cleanly. Dropping normally also
/// cleans up; a panic hook marks the session for preservation during unwinding.
pub struct RecoveryManager {
    session_id: String,
    session_dir: PathBuf,
    pending: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    state: Mutex<State>,
    unflushed: AtomicBool,
    flush_failed: AtomicBool,
    preserve: AtomicBool,
    closed: AtomicBool,
}

/// Claimable sessions and whether some recovery data could not be inspected.
#[derive(Default)]
pub struct OrphanDiscovery {
    pub sessions: Vec<OrphanSession>,
    pub incomplete: bool,
}

fn check_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 180
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(Error::InvalidDocKey(key.into()));
    }
    Ok(())
}

fn retire_session(dir: &Path) -> Result<()> {
    match fs::remove_file(dir.join("session.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn cleanup_retired_session(dir: &Path) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Only remove empty directories; never delete a newly resumed owner's data.
        for (path, file) in [
            (dir.join("docs"), false),
            (dir.join("lock"), true),
            (dir.to_path_buf(), false),
        ] {
            let result = if file {
                std::fs::remove_file(&path)
            } else {
                std::fs::remove_dir(&path)
            };
            if let Err(error) = result
                && error.kind() != io::ErrorKind::NotFound
            {
                log::warn!("Recovery session cleanup failed: {error}");
                break;
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = dir;
}

impl RecoveryManager {
    /// Create and exclusively lock a fresh UUID-named recovery session.
    pub fn start(paths: &Paths) -> Result<Self> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session_dir = paths.data_dir.join("recovery").join(&session_id);
        fs::create_dir_all(session_dir.join("docs"))?;
        let lock = Lock::try_acquire(&session_dir.join("lock"))?;
        let manager = Self {
            session_id,
            session_dir,
            pending: Arc::default(),
            state: Mutex::new(State {
                lock: Some(lock),
                finished: false,
            }),
            unflushed: AtomicBool::new(false),
            flush_failed: AtomicBool::new(false),
            preserve: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        };
        let info = SessionInfo {
            pid: if cfg!(target_arch = "wasm32") {
                0
            } else {
                std::process::id()
            },
            started_at: unix_seconds(),
            app_version: crate::system::app_version(),
        };
        atomic_write(
            manager.session_dir.join("session.json"),
            &serde_json::to_vec(&info)?,
            false,
        )?;
        Ok(manager)
    }

    /// Session UUID used to group recovered documents.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// On-disk session directory, useful for diagnostics.
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// Whether a queued snapshot has not yet been persisted or canceled.
    pub fn has_unflushed_snapshots(&self) -> bool {
        self.unflushed.load(Ordering::Acquire)
    }

    /// Whether the last flush or suspension write attempt failed.
    pub fn last_flush_failed(&self) -> bool {
        self.flush_failed.load(Ordering::Acquire)
    }

    /// Serialize and synchronously persist a document; replaces an older queued snapshot.
    pub fn snapshot(&self, doc_key: &str, meta: SnapshotMeta, workbook: &Workbook) -> Result<()> {
        self.queue_snapshot(doc_key, Snapshot::to_bytes(meta, workbook)?)?;
        self.flush()
    }

    /// Persist a handoff without queuing failed work that the caller may abandon.
    pub fn persist_snapshot(
        &self,
        doc_key: &str,
        meta: SnapshotMeta,
        workbook: &Workbook,
    ) -> Result<()> {
        let bytes = Snapshot::to_bytes(meta, workbook)?;
        self.persist_snapshot_bytes(doc_key, &bytes)
    }

    /// Persist a complete ink-aware handoff without queuing an abandoned write.
    pub fn persist_snapshot_with_pending_ink(
        &self,
        doc_key: &str,
        meta: SnapshotMeta,
        workbook: &Workbook,
        pending_ink: &str,
    ) -> Result<()> {
        let bytes = Snapshot::to_bytes_with_pending_ink(meta, workbook, pending_ink)?;
        self.persist_snapshot_bytes(doc_key, &bytes)
    }

    fn persist_snapshot_bytes(&self, doc_key: &str, bytes: &[u8]) -> Result<()> {
        check_key(doc_key)?;
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh()?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        atomic_write(
            self.session_dir
                .join("docs")
                .join(format!("{doc_key}.json")),
            bytes,
            false,
        )?;
        pending.remove(doc_key);
        self.unflushed.store(!pending.is_empty(), Ordering::Release);
        Ok(())
    }

    /// Coalesce a serialized `Snapshot` by document key without doing filesystem I/O.
    /// Serialize with `Snapshot::to_bytes`; the latest queued value wins.
    pub fn queue_snapshot(&self, doc_key: &str, bytes: Vec<u8>) -> Result<()> {
        check_key(doc_key)?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Finished);
        }
        pending.insert(doc_key.into(), bytes);
        self.unflushed.store(true, Ordering::Release);
        Ok(())
    }

    fn write_pending(
        &self,
        pending: &mut BTreeMap<String, Vec<u8>>,
        mut refresh: impl FnMut() -> io::Result<()>,
    ) -> Result<()> {
        // Remove only successful writes, retaining failed and unattempted snapshots.
        while let Some((key, bytes)) = pending.first_key_value() {
            refresh()?;
            atomic_write(
                self.session_dir.join("docs").join(format!("{key}.json")),
                bytes,
                false,
            )?;
            let key = key.clone();
            pending.remove(&key);
        }
        Ok(())
    }

    /// Persist pending snapshots and release the session lock, e.g. when a browser
    /// page is hidden or unloaded. Another tab (or the reloaded page) can then offer
    /// the snapshots for recovery. [`Self::resume`] takes the lock again.
    /// Failed persistence retains the live lock and pending snapshots for retry.
    pub fn suspend(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        if state.lock.is_none() {
            return Ok(());
        }
        let result = self.flush_locked(&state);
        self.flush_failed.store(result.is_err(), Ordering::Release);
        result?;
        state.lock.take();
        Ok(())
    }

    /// Re-acquire the session lock after [`Self::suspend`]. Fails if another
    /// process or tab claimed the session in the meantime.
    pub fn resume(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        if let Some(lock) = &state.lock {
            return Ok(lock.refresh()?);
        }
        let lock = Lock::try_resume(&self.session_dir.join("lock"))?;
        fs::read_bounded(self.session_dir.join("session.json"), 4096)?;
        state.lock = Some(lock);
        Ok(())
    }

    /// Persist pending snapshots synchronously. Failed writes remain queued for retry.
    /// A suspended session must resume ownership before writing again.
    pub fn flush(&self) -> Result<()> {
        match self.state.lock() {
            Ok(state) => {
                let result = self.flush_locked(&state);
                self.flush_failed.store(result.is_err(), Ordering::Release);
                result
            }
            Err(_) => {
                self.flush_failed.store(true, Ordering::Release);
                Err(Error::Poisoned)
            }
        }
    }

    fn flush_locked(&self, state: &State) -> Result<()> {
        if state.finished {
            return Err(Error::Finished);
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh()?;
        self.flush_pending(|| lock.refresh())
    }

    fn flush_pending(&self, refresh: impl FnMut() -> io::Result<()>) -> Result<()> {
        let mut pending = {
            let mut shared = self.pending.lock().map_err(|_| Error::Poisoned)?;
            std::mem::take(&mut *shared)
        };
        let result = self.write_pending(&mut pending, refresh);
        let mut shared = self.pending.lock().map_err(|_| Error::Poisoned)?;
        for (key, bytes) in pending {
            shared.entry(key).or_insert(bytes);
        }
        self.unflushed.store(!shared.is_empty(), Ordering::Release);
        result
    }

    /// Mark this session for crash preservation and flush without waiting on locks.
    /// Returns `false` when a lock was busy/poisoned or a disk write failed.
    pub fn try_flush_for_crash(&self) -> bool {
        self.preserve.store(true, Ordering::Release);
        let Ok(state) = self.state.try_lock() else {
            return false;
        };
        if state.finished {
            return true;
        }
        let Some(lock) = state.lock.as_ref() else {
            return false;
        };
        if lock.refresh().is_err() {
            return false;
        }
        let Ok(mut pending) = self.pending.try_lock() else {
            return false;
        };
        let result = self.write_pending(&mut pending, || lock.refresh());
        self.unflushed.store(!pending.is_empty(), Ordering::Release);
        result.is_ok()
    }

    /// Remove a saved/closed document and cancel its pending snapshot.
    pub fn remove(&self, doc_key: &str) -> Result<()> {
        check_key(doc_key)?;
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Err(Error::Finished);
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh()?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        match fs::remove_file(
            self.session_dir
                .join("docs")
                .join(format!("{doc_key}.json")),
        ) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        pending.remove(doc_key);
        self.unflushed.store(!pending.is_empty(), Ordering::Release);
        Ok(())
    }

    /// Clean shutdown: discard pending snapshots, release the lock and remove the session.
    /// Call only after documents are saved or the user explicitly discards them.
    /// A suspended session must be resumed explicitly before it can be removed.
    pub fn finish(&self) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.finished {
            return Ok(());
        }
        let lock = state.lock.as_ref().ok_or(Error::Suspended)?;
        lock.refresh()?;
        let mut pending = self.pending.lock().map_err(|_| Error::Poisoned)?;
        // Remove docs while the lock is held; no scanner can claim them in between.
        match fs::remove_dir_all(self.session_dir.join("docs")) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        retire_session(&self.session_dir)?;
        cleanup_retired_session(&self.session_dir);
        state.finished = true;
        self.closed.store(true, Ordering::Release);
        pending.clear();
        self.unflushed.store(false, Ordering::Release);
        state.lock.take();
        Ok(())
    }

    /// Discover unlocked sessions containing snapshots. Active sessions are skipped.
    /// Returned handles retain the orphan lock so concurrent processes cannot claim it.
    pub fn find_orphans(paths: &Paths) -> Vec<OrphanSession> {
        Self::discover_orphans(paths).sessions
    }

    /// Discover claimable sessions and report unreadable or corrupt recovery data.
    pub fn discover_orphans(paths: &Paths) -> OrphanDiscovery {
        let root = paths.data_dir.join("recovery");
        let entries = match fs::list_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return OrphanDiscovery::default();
            }
            Err(error) => {
                log::warn!("Recovery discovery failed: {error}");
                return OrphanDiscovery {
                    sessions: Vec::new(),
                    incomplete: true,
                };
            }
        };
        let mut sessions = Vec::new();
        let mut incomplete = false;
        for (dir, kind) in entries {
            if kind != EntryKind::Dir {
                continue;
            }
            let Some(session_id) = dir.file_name().and_then(|s| s.to_str()).map(str::to_owned)
            else {
                continue;
            };
            let lock = match Lock::try_acquire(&dir.join("lock")) {
                Ok(lock) => lock,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(error) => {
                    log::warn!("Could not inspect a recovery session: {error}");
                    incomplete = true;
                    continue;
                }
            };
            let mut documents = Vec::new();
            let mut can_cleanup = true;
            match fs::list_dir(dir.join("docs")) {
                Ok(entries) => {
                    for (path, kind) in entries {
                        if path.extension().is_none_or(|e| e != "json") {
                            continue;
                        }
                        if kind != EntryKind::File {
                            can_cleanup = false;
                            incomplete = true;
                            continue;
                        }
                        let Some(key) = path.file_stem().and_then(|s| s.to_str()) else {
                            can_cleanup = false;
                            incomplete = true;
                            continue;
                        };
                        let meta = match fs::read(&path).and_then(|bytes| {
                            serde_json::from_slice::<Snapshot>(&bytes).map_err(io::Error::other)
                        }) {
                            Ok(snapshot) => snapshot.meta,
                            Err(error) => {
                                log::warn!("Could not inspect a recovery snapshot: {error}");
                                incomplete = true;
                                SnapshotMeta {
                                    title: key.into(),
                                    ..SnapshotMeta::default()
                                }
                            }
                        };
                        documents.push((key.to_owned(), meta, path));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    log::warn!("Could not list recovery snapshots: {error}");
                    can_cleanup = false;
                    incomplete = true;
                }
            }
            if documents.is_empty() {
                // A suspended live page can still resume an empty session.
                // Keep its marker; only clean abandoned directories without one.
                let marker_missing = match fs::read_bounded(dir.join("session.json"), 4096) {
                    Ok(_) => false,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => true,
                    Err(error) => {
                        log::warn!("Could not inspect a recovery session marker: {error}");
                        incomplete = true;
                        false
                    }
                };
                if can_cleanup && marker_missing {
                    if let Err(error) = lock.refresh() {
                        log::warn!("Recovery session cleanup failed: {error}");
                        incomplete = true;
                        continue;
                    }
                    if let Err(error) = retire_session(&dir) {
                        log::warn!("Recovery session cleanup failed: {error}");
                        incomplete = true;
                        continue;
                    }
                    cleanup_retired_session(&dir);
                }
                drop(lock);
                continue;
            }
            if let Err(error) = lock.refresh() {
                log::warn!("Recovery session claim was lost: {error}");
                incomplete = true;
                continue;
            }
            let lease = Arc::new(OrphanLease {
                lock: Mutex::new(Some(lock)),
                dir,
            });
            let mut docs: Vec<_> = documents
                .into_iter()
                .map(|(doc_key, meta, path)| OrphanDoc {
                    session_id: session_id.clone(),
                    doc_key,
                    meta,
                    path,
                    lease: Arc::clone(&lease),
                })
                .collect();
            docs.sort_by(|a, b| a.doc_key.cmp(&b.doc_key));
            sessions.push(OrphanSession { session_id, docs });
        }
        sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        OrphanDiscovery {
            sessions,
            incomplete,
        }
    }

    /// Delete all currently claimable orphan snapshots, never a live session.
    pub fn discard_all_orphans(paths: &Paths) -> Result<()> {
        let discovery = Self::discover_orphans(paths);
        if discovery.incomplete {
            return Err(io::Error::other("some recovery sessions could not be inspected").into());
        }
        for session in discovery.sessions {
            session.discard()?;
        }
        Ok(())
    }
}

impl Drop for RecoveryManager {
    fn drop(&mut self) {
        if !self.preserve.load(Ordering::Acquire)
            && !std::thread::panicking()
            && let Err(error) = self.finish()
        {
            log::warn!("Recovery cleanup failed: {error}");
        }
    }
}

struct OrphanLease {
    lock: Mutex<Option<Lock>>,
    dir: PathBuf,
}
impl OrphanLease {
    fn discard(&self, path: &Path) -> Result<()> {
        let mut lock = self.lock.lock().map_err(|_| Error::Poisoned)?;
        let Some(held) = lock.as_ref() else {
            return Ok(());
        };
        held.refresh()?;
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let has_docs = match fs::list_dir(self.dir.join("docs")) {
            Ok(entries) => entries
                .iter()
                .any(|(path, _)| path.extension().is_some_and(|s| s == "json")),
            // The browser filesystem has no empty directories.
            Err(e) if cfg!(target_arch = "wasm32") && e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        if !has_docs {
            retire_session(&self.dir)?;
            cleanup_retired_session(&self.dir);
            lock.take();
        }
        Ok(())
    }
}

/// A dead process's recoverable documents; holding these handles claims the session.
#[derive(Clone)]
pub struct OrphanSession {
    pub session_id: String,
    pub docs: Vec<OrphanDoc>,
}
impl OrphanSession {
    /// Discard every document in this orphan session.
    pub fn discard(&self) -> Result<()> {
        for doc in &self.docs {
            doc.discard()?;
        }
        Ok(())
    }
}

/// A recoverable document. Corrupt snapshots remain visible and can be discarded.
#[derive(Clone)]
pub struct OrphanDoc {
    pub session_id: String,
    pub doc_key: String,
    pub meta: SnapshotMeta,
    pub path: PathBuf,
    lease: Arc<OrphanLease>,
}
impl OrphanDoc {
    /// Deserialize and validate before exposing a workbook to application code.
    pub fn load(&self) -> Result<Workbook> {
        let snapshot = self.load_snapshot()?;
        if snapshot.pending_ink.is_some() {
            return Err(Error::InvalidRecovery(
                "pending ink requires an ink-aware recovery consumer".into(),
            ));
        }
        Ok(snapshot.workbook)
    }

    /// Load the complete envelope without silently discarding pending ink.
    pub fn load_snapshot(&self) -> Result<Snapshot> {
        let guard = self.lease.lock.lock().map_err(|_| Error::Poisoned)?;
        guard.as_ref().ok_or(Error::Finished)?.refresh()?;
        let snapshot: Snapshot = serde_json::from_slice(&fs::read(&self.path)?)?;
        snapshot
            .workbook
            .validate()
            .map_err(Error::InvalidWorkbook)?;
        Ok(snapshot)
    }

    /// Copy the raw browser storage record while this session is still claimed.
    /// The UTF-16LE blob is not a validated snapshot or a `.thinoda` document.
    #[cfg(target_arch = "wasm32")]
    pub fn raw_browser_record(&self) -> Result<web_sys::Blob> {
        // Discovery represents storage keys as UTF-8 paths. Reject noncanonical
        // IDs so lossy conversion or path normalization cannot select another
        // record; recovery-generated session and document IDs are ASCII.
        check_key(&self.session_id)?;
        check_key(&self.doc_key)?;
        let guard = self.lease.lock.lock().map_err(|_| Error::Poisoned)?;
        guard.as_ref().ok_or(Error::Finished)?.refresh()?;
        Ok(crate::vfs::raw_browser_record(&self.path)?)
    }

    /// Permanently discard this snapshot, cleaning the session when it becomes empty.
    pub fn discard(&self) -> Result<()> {
        self.lease.discard(&self.path)
    }
}

// Make accidental changes to the panic hook's sharing contract a compile error.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RecoveryManager>();
};

#[cfg(test)]
mod lock_tests {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct LegacySnapshot {
        meta: SnapshotMeta,
        workbook: Workbook,
    }

    #[derive(Debug, Deserialize)]
    struct PreviousVersionedSnapshot {
        #[serde(rename = "workbook", deserialize_with = "previous_versioned_workbook")]
        _workbook: (),
    }

    fn previous_versioned_workbook<'de, D>(deserializer: D) -> std::result::Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PreviousVisitor;
        impl<'de> Visitor<'de> for PreviousVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a previously supported recovery workbook")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<(), A::Error>
            where
                A: SeqAccess<'de>,
            {
                let version: String = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::custom("missing version"))?;
                if version != INLINE_WORKBOOK_VERSION && version != WAYPOINT_WORKBOOK_VERSION {
                    return Err(A::Error::custom(format!(
                        "unsupported recovery workbook version {version}"
                    )));
                }
                let _: IgnoredAny = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::custom("missing workbook"))?;
                if seq.next_element::<IgnoredAny>()?.is_some() {
                    return Err(A::Error::custom("unexpected recovery workbook data"));
                }
                Ok(())
            }
        }
        deserializer.deserialize_any(PreviousVisitor)
    }

    fn formatted_workbook(kind: thinoda_core::CardKind) -> Workbook {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let title = "A🦀é\n第二";
        let card = sheet
            .add_card(
                title,
                thinoda_core::Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(kind, thinoda_core::Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let topic = sheet.topic_mut(card).unwrap();
        topic
            .inline_text
            .update_range(title, 0..3, |style| {
                style.bold = Some(true);
                style.font_family = Some("Sans".into());
            })
            .unwrap();
        topic
            .inline_text
            .paragraphs
            .push(thinoda_core::InlineParagraph {
                index: 1,
                list: thinoda_core::ListKind::Numbered,
            });
        book.validate().unwrap();
        book
    }

    #[test]
    fn recovery_workbook_versions_protect_rich_data_and_keep_legacy_snapshots() {
        let meta = SnapshotMeta {
            title: "Recovery".into(),
            dirty: true,
            ..SnapshotMeta::default()
        };
        let plain = Workbook::default();
        let legacy_plain = LegacySnapshot {
            meta: meta.clone(),
            workbook: plain.clone(),
        };
        let plain_bytes = Snapshot::to_bytes(meta.clone(), &plain).unwrap();
        assert_eq!(plain_bytes, serde_json::to_vec(&legacy_plain).unwrap());
        assert_eq!(
            serde_json::from_slice::<LegacySnapshot>(&plain_bytes)
                .unwrap()
                .workbook,
            plain
        );

        for kind in [thinoda_core::CardKind::Sticky, thinoda_core::CardKind::Text] {
            let book = formatted_workbook(kind);
            let bytes = Snapshot::to_bytes(meta.clone(), &book).unwrap();
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.meta, meta);
            assert_eq!(snapshot.workbook, book);
            assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
            assert!(serde_json::from_slice::<LegacySnapshot>(&bytes).is_err());

            let historical = serde_json::to_vec(&LegacySnapshot {
                meta: meta.clone(),
                workbook: book.clone(),
            })
            .unwrap();
            assert_eq!(
                serde_json::from_slice::<Snapshot>(&historical)
                    .unwrap()
                    .workbook,
                book
            );

            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["workbook"][0] = serde_json::json!("THINODA-RECOVERY-INLINE-999");
            assert!(serde_json::from_value::<Snapshot>(value).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["workbook"].as_array_mut().unwrap().pop();
            assert!(serde_json::from_value::<Snapshot>(value).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["workbook"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!(null));
            assert!(serde_json::from_value::<Snapshot>(value).is_err());
        }
    }

    #[test]
    fn recovery_workbook_versions_protect_waypoints_on_inactive_sheets() {
        use thinoda_core::{CanvasConnector, CanvasObject, Endpoint, Sheet, Vec2};

        let mut sheet = Sheet::new("Routes", "Root");
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.waypoints = vec![Vec2::new(50.0, 50.0)].into();
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let meta = SnapshotMeta::default();
        for mut book in [
            Workbook::default(),
            formatted_workbook(thinoda_core::CardKind::Text),
        ] {
            book.sheets.push(sheet.clone());
            book.validate().unwrap();
            let bytes = Snapshot::to_bytes(meta.clone(), &book).unwrap();
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
            assert!(serde_json::from_slice::<LegacySnapshot>(&bytes).is_err());

            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["workbook"][0], WAYPOINT_WORKBOOK_VERSION);
            let mut inline_only = value.clone();
            inline_only["workbook"][0] = serde_json::json!(INLINE_WORKBOOK_VERSION);
            assert!(
                serde_json::from_value::<Snapshot>(inline_only)
                    .unwrap_err()
                    .to_string()
                    .contains("connector waypoints require a waypoint-aware recovery workbook")
            );
            let mut unversioned = value.clone();
            unversioned["workbook"] = value["workbook"][1].clone();
            assert!(
                serde_json::from_value::<Snapshot>(unversioned)
                    .unwrap_err()
                    .to_string()
                    .contains("connector waypoints require a versioned recovery workbook")
            );
        }
    }

    #[test]
    fn connector_markers_require_marker_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Sheet,
            Vec2,
        };

        let mut sheet = Sheet::new("Markers", "Root");
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Block,
        });
        connector.waypoints = vec![Vec2::new(50.0, 20.0)].into();
        sheet
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        book.sheets.push(sheet);
        book.validate().unwrap();
        let meta = SnapshotMeta::default();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => Snapshot::to_bytes_with_pending_ink(meta.clone(), &book, ink).unwrap(),
                None => Snapshot::to_bytes(meta.clone(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
            assert!(serde_json::from_slice::<LegacySnapshot>(&bytes).is_err());
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());

            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    MARKER_INK_WORKBOOK_VERSION
                } else {
                    MARKER_WORKBOOK_VERSION
                }
            );
            for old_version in [
                INLINE_WORKBOOK_VERSION,
                WAYPOINT_WORKBOOK_VERSION,
                INK_WORKBOOK_VERSION,
            ] {
                let mut forged = value.clone();
                forged["workbook"][0] = serde_json::json!(old_version);
                assert!(
                    serde_json::from_value::<Snapshot>(forged)
                        .unwrap_err()
                        .to_string()
                        .contains("connector markers require a marker-aware recovery workbook")
                );
            }
            let mut unversioned = value.clone();
            unversioned["workbook"] = value["workbook"][1].clone();
            assert!(
                serde_json::from_value::<Snapshot>(unversioned)
                    .unwrap_err()
                    .to_string()
                    .contains("connector markers require a versioned recovery workbook")
            );
        }
    }

    #[test]
    fn classic_and_thin_markers_require_extended_recovery_envelopes() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [
            ConnectorMarkerKind::Classic,
            ConnectorMarkerKind::ClassicThin,
            ConnectorMarkerKind::OpenThin,
            ConnectorMarkerKind::BlockThin,
        ] {
            let mut book = formatted_workbook(thinoda_core::CardKind::Text);
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.end_marker = Some(ConnectorMarker { kind });
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
                let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    value["workbook"][0],
                    if ink.is_some() {
                        EXTENDED_MARKER_INK_WORKBOOK_VERSION
                    } else {
                        EXTENDED_MARKER_WORKBOOK_VERSION
                    }
                );
                value["workbook"][0] = serde_json::json!(if ink.is_some() {
                    MARKER_INK_WORKBOOK_VERSION
                } else {
                    MARKER_WORKBOOK_VERSION
                });
                assert!(
                    serde_json::from_value::<Snapshot>(value)
                        .unwrap_err()
                        .to_string()
                        .contains("classic and thin connector markers require an extended-marker recovery workbook")
                );
            }
        }
    }

    #[test]
    fn marker_sizes_require_size_aware_recovery_envelopes() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Open,
        });
        connector.end_marker_size = Some(12.5);
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    SIZED_MARKER_INK_WORKBOOK_VERSION
                } else {
                    SIZED_MARKER_WORKBOOK_VERSION
                }
            );
            value["workbook"][0] = serde_json::json!(if ink.is_some() {
                EXTENDED_MARKER_INK_WORKBOOK_VERSION
            } else {
                EXTENDED_MARKER_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(value)
                    .unwrap_err()
                    .to_string()
                    .contains("connector marker sizes require a size-aware recovery workbook")
            );
        }
    }

    #[test]
    fn marker_fills_require_fill_aware_recovery_envelopes() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Classic,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    FILLED_MARKER_INK_WORKBOOK_VERSION
                } else {
                    FILLED_MARKER_WORKBOOK_VERSION
                }
            );
            value["workbook"][0] = serde_json::json!(if ink.is_some() {
                SIZED_MARKER_INK_WORKBOOK_VERSION
            } else {
                SIZED_MARKER_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(value)
                    .unwrap_err()
                    .to_string()
                    .contains("connector marker fills require a fill-aware recovery workbook")
            );
        }
    }

    #[test]
    fn diamond_markers_require_diamond_aware_recovery_envelopes() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [
            ConnectorMarkerKind::Diamond,
            ConnectorMarkerKind::DiamondThin,
        ] {
            let mut book = formatted_workbook(thinoda_core::CardKind::Text);
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.end_marker = Some(ConnectorMarker { kind });
            connector.end_marker_size = Some(12.5);
            connector.end_marker_unfilled = true;
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
                let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    value["workbook"][0],
                    if ink.is_some() {
                        DIAMOND_MARKER_INK_WORKBOOK_VERSION
                    } else {
                        DIAMOND_MARKER_WORKBOOK_VERSION
                    }
                );
                value["workbook"][0] = serde_json::json!(if ink.is_some() {
                    FILLED_MARKER_INK_WORKBOOK_VERSION
                } else {
                    FILLED_MARKER_WORKBOOK_VERSION
                });
                assert!(
                    serde_json::from_value::<Snapshot>(value)
                        .unwrap_err()
                        .to_string()
                        .contains(
                            "diamond connector markers require a diamond-aware recovery workbook"
                        )
                );
            }
        }
    }

    #[test]
    fn oval_markers_require_oval_aware_recovery_envelopes() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Diamond,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Oval,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    OVAL_MARKER_INK_WORKBOOK_VERSION
                } else {
                    OVAL_MARKER_WORKBOOK_VERSION
                }
            );
            value["workbook"][0] = serde_json::json!(if ink.is_some() {
                DIAMOND_MARKER_INK_WORKBOOK_VERSION
            } else {
                DIAMOND_MARKER_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(value)
                    .unwrap_err()
                    .to_string()
                    .contains("oval connector markers require an oval-aware recovery workbook")
            );
        }
    }

    #[test]
    fn half_circle_markers_require_aware_recovery_envelopes() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.arrow_start = true;
        connector.start_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Oval,
        });
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::HalfCircle,
        });
        connector.end_marker_size = Some(12.5);
        connector.end_marker_unfilled = true;
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                } else {
                    HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                }
            );
            value["workbook"][0] = serde_json::json!(if ink.is_some() {
                OVAL_MARKER_INK_WORKBOOK_VERSION
            } else {
                OVAL_MARKER_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(value)
                    .unwrap_err()
                    .to_string()
                    .contains("half-circle connector markers require a half-circle-aware recovery workbook")
            );
        }
    }

    #[test]
    fn bar_markers_require_aware_recovery_envelopes() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        for kind in [ConnectorMarkerKind::BaseDash, ConnectorMarkerKind::Dash] {
            for bar_at_start in [false, true] {
                let mut book = formatted_workbook(thinoda_core::CardKind::Text);
                let mut connector = CanvasConnector::new(
                    Endpoint::Point(Vec2::ZERO),
                    Endpoint::Point(Vec2::new(100.0, 0.0)),
                );
                connector.arrow_start = true;
                let bar = Some(ConnectorMarker { kind });
                let half_circle = Some(ConnectorMarker {
                    kind: ConnectorMarkerKind::HalfCircle,
                });
                (connector.start_marker, connector.end_marker) = if bar_at_start {
                    (bar, half_circle)
                } else {
                    (half_circle, bar)
                };
                connector.start_marker_size = Some(0.0);
                connector.end_marker_size = Some(12.5);
                connector.start_marker_unfilled = true;
                connector.end_marker_unfilled = true;
                book.active_mut()
                    .add_canvas_object(CanvasObject::connector(connector))
                    .unwrap();
                for ink in [None, Some("pending ink")] {
                    let bytes = match ink {
                        Some(ink) => {
                            Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                                .unwrap()
                        }
                        None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                    };
                    let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(snapshot.workbook, book);
                    assert_eq!(snapshot.pending_ink.as_deref(), ink);
                    assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
                    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(
                        value["workbook"][0],
                        if ink.is_some() {
                            BAR_MARKER_INK_WORKBOOK_VERSION
                        } else {
                            BAR_MARKER_WORKBOOK_VERSION
                        }
                    );
                    value["workbook"][0] = serde_json::json!(if ink.is_some() {
                        HALF_CIRCLE_MARKER_INK_WORKBOOK_VERSION
                    } else {
                        HALF_CIRCLE_MARKER_WORKBOOK_VERSION
                    });
                    assert!(
                        serde_json::from_value::<Snapshot>(value)
                            .unwrap_err()
                            .to_string()
                            .contains(
                                "bar connector markers require a bar-aware recovery workbook"
                            )
                    );
                }
            }
        }
    }

    #[test]
    fn shape_rounding_requires_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind,
            DrawioRectangleStencil, Endpoint, Rect, ShapeKind, ShapeRounding, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut shape = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rounding = Some(ShapeRounding {
            radius: 16.0,
            stencil: DrawioRectangleStencil::Ios,
            ..Default::default()
        });
        book.active_mut().add_canvas_object(shape).unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::Dash,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    ROUNDING_INK_WORKBOOK_VERSION
                } else {
                    ROUNDING_WORKBOOK_VERSION
                }
            );
            let mut older = value.clone();
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                BAR_MARKER_INK_WORKBOOK_VERSION
            } else {
                BAR_MARKER_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(older)
                    .unwrap_err()
                    .to_string()
                    .contains("shape rounding requires a rounding-aware recovery workbook")
            );
            let mut unversioned = value;
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(
                serde_json::from_value::<Snapshot>(unversioned)
                    .unwrap_err()
                    .to_string()
                    .contains("shape rounding requires a versioned recovery workbook")
            );
        }
    }

    #[test]
    fn rectangle_sides_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasObject, DrawioRectangleStencil, Rect, RectangleSides, ShapeKind, ShapeRounding,
            Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rectangle_sides = Some(RectangleSides {
            top: false,
            right: true,
            bottom: true,
            left: false,
        });
        book.active_mut().add_canvas_object(shape).unwrap();
        let mut rounded = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::new(110.0, 0.0), Vec2::new(100.0, 60.0)),
        );
        rounded.rounding = Some(ShapeRounding {
            radius: 16.0,
            stencil: DrawioRectangleStencil::Ios,
            ..Default::default()
        });
        book.active_mut().add_canvas_object(rounded).unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    RECTANGLE_SIDES_INK_WORKBOOK_VERSION
                } else {
                    RECTANGLE_SIDES_WORKBOOK_VERSION
                }
            );
            let mut older = value.clone();
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                ROUNDING_INK_WORKBOOK_VERSION
            } else {
                ROUNDING_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(older)
                    .unwrap_err()
                    .to_string()
                    .contains("rectangle outline sides require a partial-rectangle-aware")
            );
            let mut unversioned = value;
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(
                serde_json::from_value::<Snapshot>(unversioned)
                    .unwrap_err()
                    .to_string()
                    .contains("rectangle outline sides require a versioned recovery workbook")
            );
        }
    }

    #[test]
    fn fixed_connector_ports_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorPort, Endpoint, Rect, ShapeKind, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let shape = book
            .active_mut()
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
            ))
            .unwrap();
        let mut connector = CanvasConnector::new(
            Endpoint::Object(shape),
            Endpoint::Point(Vec2::new(200.0, 0.0)),
        );
        connector.from_port = Some(ConnectorPort {
            position: Vec2::new(1.0, 0.25),
            offset: Vec2::ZERO,
            perimeter: true,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    FIXED_PORT_INK_WORKBOOK_VERSION
                } else {
                    FIXED_PORT_WORKBOOK_VERSION
                }
            );
            let mut older = value.clone();
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION
            } else {
                DOUBLE_ELLIPSE_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(older)
                    .unwrap_err()
                    .to_string()
                    .contains("fixed connector ports require a port-aware")
            );
            let mut unversioned = value;
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
        }
    }

    #[test]
    fn socio_technical_data_requires_aware_recovery_with_and_without_pending_ink() {
        let mut book = Workbook::default();
        let mut semantic = thinoda_core::semantic::SemanticModel::default();
        semantic.entities.insert(
            thinoda_core::semantic::SemanticEntityId(2),
            thinoda_core::semantic::SemanticEntity::Participant(
                thinoda_core::semantic::Participant {
                    kind: thinoda_core::semantic::ParticipantKind::Software,
                    name: "Order service".into(),
                    description: String::new(),
                },
            ),
        );
        book.active_mut().semantic = Some(semantic);

        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION
            } else {
                COLLAPSE_LAYOUT_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
        }
    }

    #[test]
    fn rich_labels_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::board::{CanvasObject, CanvasObjectKind, ShapeKind};
        use thinoda_core::{Rect, Vec2};

        for label_style in [false, true] {
            let mut book = Workbook::default();
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
            );
            let CanvasObjectKind::Shape { label, .. } = &mut object.kind else {
                unreachable!()
            };
            *label = "A🙂B".into();
            let mut rich = thinoda_core::InlineText::default();
            rich.update_range(label, 1..2, |style| style.italic = Some(true))
                .unwrap();
            object.label_rich_text = Some(std::sync::Arc::new(rich));
            if label_style {
                object.label_style =
                    Some(std::sync::Arc::new(thinoda_core::label_style::LabelStyle {
                        bold: true,
                        ..Default::default()
                    }));
            }
            book.active_mut().add_canvas_object(object).unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        RICH_LABELS_INK_WORKBOOK_VERSION
                    } else {
                        RICH_LABELS_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    LABEL_STYLES_INK_WORKBOOK_VERSION
                } else {
                    LABEL_STYLES_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn rich_connector_labels_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::board::{CanvasConnector, CanvasObject};
        use thinoda_core::{Endpoint, Vec2};

        let mut book = Workbook::default();
        let mut object = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        ));
        object.set_label("A🙂B").unwrap();
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range(object.label(), 1..2, |style| style.italic = Some(true))
            .unwrap();
        object.label_rich_text = Some(std::sync::Arc::new(rich));
        book.active_mut().add_canvas_object(object).unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    RICH_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                } else {
                    RICH_CONNECTOR_LABELS_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                RICH_LABELS_INK_WORKBOOK_VERSION
            } else {
                RICH_LABELS_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
            assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
        }
    }

    #[test]
    fn rich_extra_connector_labels_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::board::{CanvasConnector, CanvasObject, ConnectorLabel, ShapeKind};
        use thinoda_core::{Endpoint, Vec2};

        for primary in [false, true] {
            for (label_style, shape_rich) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let mut book = Workbook::default();
                let mut connector = CanvasConnector::new(
                    Endpoint::Point(Vec2::ZERO),
                    Endpoint::Point(Vec2::new(100.0, 0.0)),
                );
                let mut rich = thinoda_core::InlineText::default();
                rich.update_range("A🙂B", 1..2, |style| style.italic = Some(true))
                    .unwrap();
                connector.extra_labels.push(ConnectorLabel {
                    text: "A🙂B".into(),
                    rich_text: Some(Arc::new(rich.clone())),
                    ..Default::default()
                });
                let mut object = CanvasObject::connector(connector);
                if primary {
                    object.set_label("A🙂B").unwrap();
                    object.label_rich_text = Some(Arc::new(rich));
                }
                book.active_mut().add_canvas_object(object).unwrap();
                if label_style || shape_rich {
                    let mut shape = CanvasObject::shape(
                        ShapeKind::Rectangle,
                        thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
                    );
                    if label_style {
                        shape.label_style = Some(Arc::new(thinoda_core::label_style::LabelStyle {
                            bold: true,
                            ..Default::default()
                        }));
                    }
                    if shape_rich {
                        shape.set_label("shape").unwrap();
                        let mut rich = thinoda_core::InlineText::default();
                        rich.update_range("shape", 0..5, |style| style.bold = Some(true))
                            .unwrap();
                        shape.label_rich_text = Some(Arc::new(rich));
                    }
                    book.active_mut().add_canvas_object(shape).unwrap();
                }
                for ink in [None, Some("pending ink")] {
                    let bytes = match ink {
                        Some(ink) => {
                            Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                                .unwrap()
                        }
                        None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                    };
                    let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(snapshot.workbook, book);
                    assert_eq!(snapshot.pending_ink.as_deref(), ink);
                    let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(
                        older["workbook"][0],
                        if ink.is_some() {
                            RICH_EXTRA_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                        } else {
                            RICH_EXTRA_CONNECTOR_LABELS_WORKBOOK_VERSION
                        }
                    );
                    older["workbook"][0] = serde_json::json!(if ink.is_some() {
                        RICH_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                    } else {
                        RICH_CONNECTOR_LABELS_WORKBOOK_VERSION
                    });
                    assert!(serde_json::from_value::<Snapshot>(older).is_err());
                    let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                    assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
                }
            }
        }
    }

    #[test]
    fn drawio_zone_ids_require_aware_recovery_with_and_without_ink() {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        sheet
            .create_board_frame(
                "Lane",
                thinoda_core::Rect::from_min_size(
                    thinoda_core::Vec2::ZERO,
                    thinoda_core::Vec2::new(300.0, 200.0),
                ),
            )
            .unwrap();
        sheet.zones[0].drawio_source_id = Some("source-zone".into());
        book.drawio = Some(Arc::new(
            thinoda_core::drawio_passthrough::DrawioDocumentPassthrough {
                attributes: vec![("owner".into(), "team".into())],
            },
        ));
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    DRAWIO_ZONE_IDS_INK_WORKBOOK_VERSION
                } else {
                    DRAWIO_ZONE_IDS_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                DRAWIO_DOCUMENT_INK_WORKBOOK_VERSION
            } else {
                DRAWIO_DOCUMENT_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
            let mut missing: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            missing["workbook"][1]["sheets"][0]["zones"][0]["drawio_source_id"] =
                serde_json::Value::Null;
            assert!(serde_json::from_value::<Snapshot>(missing).is_err());
        }
    }

    #[test]
    fn drawio_topic_ids_require_aware_recovery_with_and_without_ink() {
        let mut book = Workbook::default();
        let root = book.active().root;
        book.active_mut().topic_mut(root).unwrap().drawio_source_id = Some("source-topic".into());
        book.active_mut()
            .create_board_frame(
                "Lane",
                thinoda_core::Rect::from_min_size(
                    thinoda_core::Vec2::ZERO,
                    thinoda_core::Vec2::new(300.0, 200.0),
                ),
            )
            .unwrap();
        book.active_mut().zones[0].drawio_source_id = Some("source-zone".into());
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    DRAWIO_TOPIC_IDS_INK_WORKBOOK_VERSION
                } else {
                    DRAWIO_TOPIC_IDS_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                DRAWIO_ZONE_IDS_INK_WORKBOOK_VERSION
            } else {
                DRAWIO_ZONE_IDS_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
            let mut missing: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            missing["workbook"][1]["sheets"][0]["topics"][root.0.to_string()]["drawio_source_id"] =
                serde_json::Value::Null;
            assert!(serde_json::from_value::<Snapshot>(missing).is_err());
        }
    }

    #[test]
    fn drawio_document_metadata_requires_aware_recovery_with_and_without_ink() {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let id = sheet
            .add_card(
                "Text",
                thinoda_core::Vec2::ZERO,
                thinoda_core::Card::new(
                    thinoda_core::CardKind::Sticky,
                    thinoda_core::Vec2::new(160.0, 100.0),
                ),
            )
            .unwrap();
        sheet
            .topic_mut(id)
            .unwrap()
            .inline_text
            .update_range("Text", 0..1, |style| {
                style.baseline = Some(thinoda_core::InlineBaseline::Subscript);
            })
            .unwrap();
        book.drawio = Some(Arc::new(
            thinoda_core::drawio_passthrough::DrawioDocumentPassthrough {
                attributes: vec![("owner".into(), "team".into())],
            },
        ));
        book.validate().unwrap();
        let plain = Workbook {
            drawio: book.drawio.clone(),
            ..Workbook::default()
        };
        let bytes = Snapshot::to_bytes(SnapshotMeta::default(), &plain).unwrap();
        assert_eq!(
            serde_json::from_slice::<Snapshot>(&bytes).unwrap().workbook,
            plain
        );
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    DRAWIO_DOCUMENT_INK_WORKBOOK_VERSION
                } else {
                    DRAWIO_DOCUMENT_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                CARD_BASELINE_INK_WORKBOOK_VERSION
            } else {
                CARD_BASELINE_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
            let mut missing: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            missing["workbook"][1]["drawio"] = serde_json::Value::Null;
            assert!(serde_json::from_value::<Snapshot>(missing).is_err());
            if ink.is_some() {
                let mut empty: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                empty["workbook"][2] = serde_json::json!("");
                assert!(serde_json::from_value::<Snapshot>(empty).is_err());
            }
        }
    }

    #[test]
    fn card_baseline_requires_aware_recovery_with_and_without_pending_ink() {
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let id = sheet
            .add_card(
                "A🙂B",
                thinoda_core::Vec2::ZERO,
                thinoda_core::Card::new(
                    thinoda_core::CardKind::Sticky,
                    thinoda_core::Vec2::new(160.0, 100.0),
                ),
            )
            .unwrap();
        sheet
            .topic_mut(id)
            .unwrap()
            .inline_text
            .update_range("A🙂B", 1..2, |style| {
                style.baseline = Some(thinoda_core::InlineBaseline::Subscript);
            })
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    CARD_BASELINE_INK_WORKBOOK_VERSION
                } else {
                    CARD_BASELINE_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                RICH_EXTRA_CONNECTOR_LABELS_INK_WORKBOOK_VERSION
            } else {
                RICH_EXTRA_CONNECTOR_LABELS_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
        }
        let mut connector = thinoda_core::board::CanvasConnector::new(
            thinoda_core::Endpoint::Point(thinoda_core::Vec2::ZERO),
            thinoda_core::Endpoint::Point(thinoda_core::Vec2::new(100.0, 0.0)),
        );
        let mut rich = thinoda_core::InlineText::default();
        rich.update_range("label", 0..5, |style| style.bold = Some(true))
            .unwrap();
        connector
            .extra_labels
            .push(thinoda_core::board::ConnectorLabel {
                text: "label".into(),
                rich_text: Some(Arc::new(rich.clone())),
                ..Default::default()
            });
        let mut object = thinoda_core::board::CanvasObject::connector(connector);
        object.set_label("label").unwrap();
        object.label_rich_text = Some(Arc::new(rich));
        book.active_mut().add_canvas_object(object).unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
        }
    }

    #[test]
    fn label_styles_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::Vec2;
        use thinoda_core::board::{CanvasObject, ShapeKind};
        use thinoda_core::label_style::LabelStyle;

        for shape_style in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
            );
            object.label_style = Some(std::sync::Arc::new(LabelStyle {
                italic: true,
                ..LabelStyle::default()
            }));
            object.style.shadow = shape_style;
            sheet.add_canvas_object(object).unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        LABEL_STYLES_INK_WORKBOOK_VERSION
                    } else {
                        LABEL_STYLES_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    SHAPE_STYLES_INK_WORKBOOK_VERSION
                } else {
                    SHAPE_STYLES_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn shape_styles_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::Vec2;
        use thinoda_core::board::{CanvasObject, ShapeKind};

        for table in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                thinoda_core::Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 30.0)),
            );
            object.style.shadow = true;
            sheet.add_canvas_object(object).unwrap();
            if table {
                sheet
                    .insert_canvas_table(Vec2::new(100.0, 0.0), 2, 2, None)
                    .unwrap();
            }
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        SHAPE_STYLES_INK_WORKBOOK_VERSION
                    } else {
                        SHAPE_STYLES_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    TABLES_INK_WORKBOOK_VERSION
                } else {
                    TABLES_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn tables_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::Vec2;

        for layered in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let table = sheet.insert_canvas_table(Vec2::ZERO, 2, 3, None).unwrap();
            if layered {
                let layer = sheet.add_board_layer("Tables").unwrap();
                sheet
                    .move_canvas_objects_to_layer(&[table], Some(layer))
                    .unwrap();
            }
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        TABLES_INK_WORKBOOK_VERSION
                    } else {
                        TABLES_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    LAYERS_INK_WORKBOOK_VERSION
                } else {
                    LAYERS_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn layers_require_aware_recovery_with_and_without_pending_ink() {
        use std::sync::Arc;
        use thinoda_core::object_data::ObjectData;
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};

        for data in [false, true] {
            let mut book = Workbook::default();
            let sheet = book.active_mut();
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 20.0)),
            );
            if data {
                object.data = Some(Arc::new(ObjectData {
                    tags: vec!["n".into()],
                    ..Default::default()
                }));
            }
            let id = sheet.add_canvas_object(object).unwrap();
            let layer = sheet.add_board_layer("Hidden notes").unwrap();
            sheet
                .move_canvas_objects_to_layer(&[id], Some(layer))
                .unwrap();
            sheet.set_board_layer_visible(layer, false).unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        LAYERS_INK_WORKBOOK_VERSION
                    } else {
                        LAYERS_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    OBJECT_DATA_INK_WORKBOOK_VERSION
                } else {
                    OBJECT_DATA_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn object_data_requires_aware_recovery_with_and_without_pending_ink() {
        use std::sync::Arc;
        use thinoda_core::object_data::ObjectData;
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorLabel, ConnectorLabelPlacement, Endpoint, Vec2,
        };

        for labels in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            if labels {
                connector.extra_labels.push(ConnectorLabel {
                    text: "n".into(),
                    placement: ConnectorLabelPlacement::default(),
                    rich_text: None,
                });
            }
            let mut object = CanvasObject::connector(connector);
            object.data = Some(Arc::new(ObjectData {
                link: Some("data:page/id,p2".into()),
                ..Default::default()
            }));
            book.active_mut().add_canvas_object(object).unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        OBJECT_DATA_INK_WORKBOOK_VERSION
                    } else {
                        OBJECT_DATA_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                } else {
                    CONNECTOR_LABELS_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn connector_labels_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorLabel, ConnectorLabelPlacement, EdgeShape,
            EdgeShapeKind, Endpoint, Vec2,
        };

        for edge_shape in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.extra_labels.push(ConnectorLabel {
                text: "n".into(),
                placement: ConnectorLabelPlacement::default(),
                rich_text: None,
            });
            if edge_shape {
                connector.edge_shape = Some(EdgeShape::new(EdgeShapeKind::Pipe));
            }
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        CONNECTOR_LABELS_INK_WORKBOOK_VERSION
                    } else {
                        CONNECTOR_LABELS_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    EDGE_SHAPES_INK_WORKBOOK_VERSION
                } else {
                    EDGE_SHAPES_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn edge_shapes_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorRoute, EdgeShape, EdgeShapeKind, Endpoint, Vec2,
        };

        for segment_route in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.edge_shape = Some(EdgeShape::new(EdgeShapeKind::TaperedArrow));
            if segment_route {
                connector.route = ConnectorRoute::Segment;
            }
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        EDGE_SHAPES_INK_WORKBOOK_VERSION
                    } else {
                        EDGE_SHAPES_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    SEGMENT_ROUTES_INK_WORKBOOK_VERSION
                } else {
                    SEGMENT_ROUTES_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn segment_routes_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{CanvasConnector, CanvasObject, ConnectorRoute, Endpoint, Vec2};

        for edge_route in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.route = ConnectorRoute::Segment;
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            if edge_route {
                let mut other = CanvasConnector::new(
                    Endpoint::Point(Vec2::ZERO),
                    Endpoint::Point(Vec2::new(0.0, 80.0)),
                );
                other.route = ConnectorRoute::ElbowVertical;
                book.active_mut()
                    .add_canvas_object(CanvasObject::connector(other))
                    .unwrap();
            }
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        SEGMENT_ROUTES_INK_WORKBOOK_VERSION
                    } else {
                        SEGMENT_ROUTES_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    EDGE_ROUTES_INK_WORKBOOK_VERSION
                } else {
                    EDGE_ROUTES_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn edge_routes_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorRoute, Endpoint, LineJumpStyle, LineJumps, Vec2,
        };

        for jumps in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 40.0)),
            );
            connector.route = ConnectorRoute::TopToBottom;
            if jumps {
                connector.line_jumps = Some(LineJumps::new(LineJumpStyle::Gap));
            }
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        EDGE_ROUTES_INK_WORKBOOK_VERSION
                    } else {
                        EDGE_ROUTES_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    LINE_JUMPS_INK_WORKBOOK_VERSION
                } else {
                    LINE_JUMPS_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn line_jumps_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint,
            LineJumpStyle, LineJumps, Vec2,
        };

        for library in [false, true] {
            let mut book = Workbook::default();
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(100.0, 0.0)),
            );
            connector.line_jumps = Some(LineJumps::new(LineJumpStyle::Gap));
            if library {
                connector.end_marker = Some(ConnectorMarker {
                    kind: ConnectorMarkerKind::Box,
                });
            }
            book.active_mut()
                .add_canvas_object(CanvasObject::connector(connector))
                .unwrap();
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        LINE_JUMPS_INK_WORKBOOK_VERSION
                    } else {
                        LINE_JUMPS_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    MARKER_LIBRARY_INK_WORKBOOK_VERSION
                } else {
                    MARKER_LIBRARY_WORKBOOK_VERSION
                });
                assert!(serde_json::from_value::<Snapshot>(older).is_err());
                let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
                let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
                assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
            }
        }
    }

    #[test]
    fn library_markers_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorMarker, ConnectorMarkerKind, Endpoint, Vec2,
        };

        let mut book = Workbook::default();
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.end_marker = Some(ConnectorMarker {
            kind: ConnectorMarkerKind::DoubleBlock,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    MARKER_LIBRARY_INK_WORKBOOK_VERSION
                } else {
                    MARKER_LIBRARY_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION
            } else {
                COLLAPSE_LAYOUT_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
            let mut mismatched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            mismatched["workbook"][1] = serde_json::to_value(Workbook::default()).unwrap();
            assert!(serde_json::from_value::<Snapshot>(mismatched).is_err());
        }
    }

    #[test]
    fn sankey_requires_aware_recovery_with_and_without_pending_ink() {
        let book = thinoda_core::templates::instantiate("sankey-probability");
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }

                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    SANKEY_INK_WORKBOOK_VERSION
                } else {
                    SANKEY_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                FINAL_STATE_INK_WORKBOOK_VERSION
            } else {
                FINAL_STATE_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
        }
    }

    #[test]
    fn stencil_closures_require_aware_recovery_and_restore_offline_with_pending_ink() {
        use thinoda_core::stencil_object::{find_package, new_object};
        let mut package = find_package("thinoda.general").unwrap().clone();
        package.id = "vendor.recovery-offline".into();
        let definition = package.definition("message").unwrap();
        let mut book = Workbook::default();
        let object = new_object(&package, definition, thinoda_core::Vec2::new(10.0, 10.0));
        book.active_mut().add_canvas_object(object).unwrap();
        assert!(book.has_stencil_definition_closures());
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(
                thinoda_core::stencil_object::render(
                    &snapshot.workbook.sheets[0].canvas.objects[0]
                )
                .is_some(),
                "recovery should restore package definitions without the installed library"
            );
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    CLOSED_STENCIL_INK_WORKBOOK_VERSION
                } else {
                    CLOSED_STENCIL_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                PINNED_STENCIL_INK_WORKBOOK_VERSION
            } else {
                PINNED_STENCIL_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(older)
                    .unwrap_err()
                    .to_string()
                    .contains("stencil definition closures")
            );
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
        }
    }

    #[test]
    fn scene_hierarchy_requires_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::stencil_object::{find_package, new_object};
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};
        let mut book = Workbook::default();
        let pool = book
            .active_mut()
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
            ))
            .unwrap();
        book.active_mut().set_canvas_container(pool, true).unwrap();
        let package = find_package("thinoda.general").unwrap();
        book.active_mut()
            .add_canvas_object(new_object(
                package,
                package.definition("message").unwrap(),
                Vec2::new(20.0, 20.0),
            ))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let restored: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(restored.workbook, book);
            assert_eq!(restored.pending_ink.as_deref(), ink);
            let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                older["workbook"][0],
                if ink.is_some() {
                    SCENE_STENCIL_INK_WORKBOOK_VERSION
                } else {
                    SCENE_STENCIL_WORKBOOK_VERSION
                }
            );
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                STENCIL_INK_WORKBOOK_VERSION
            } else {
                STENCIL_WORKBOOK_VERSION
            });
            assert!(serde_json::from_value::<Snapshot>(older).is_err());
            let mut unversioned: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
        }
    }

    #[test]
    fn scene_layout_policies_require_layout_aware_recovery() {
        use thinoda_core::{
            CanvasObject, Rect, ShapeKind, Vec2,
            scene_layout::{
                ContainerLayout, ContainerLayoutPolicy, HeaderPolicy, HeaderSide, StackAxis,
                StackCrossAxisPolicy, StackResizePolicy,
            },
            stencil_object::{find_package, new_object},
        };

        for (collapsed, with_stencil, expected_version, old_version) in [
            (
                false,
                false,
                SCENE_LAYOUT_WORKBOOK_VERSION,
                SCENE_WORKBOOK_VERSION,
            ),
            (
                false,
                true,
                SCENE_STENCIL_LAYOUT_WORKBOOK_VERSION,
                SCENE_STENCIL_WORKBOOK_VERSION,
            ),
            (
                true,
                true,
                COLLAPSE_LAYOUT_WORKBOOK_VERSION,
                COLLAPSE_WORKBOOK_VERSION,
            ),
        ] {
            let mut book = Workbook::default();
            let pool = book
                .active_mut()
                .add_canvas_object(CanvasObject::shape(
                    ShapeKind::Rectangle,
                    Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
                ))
                .unwrap();
            book.active_mut().set_canvas_container(pool, true).unwrap();
            book.active_mut()
                .set_canvas_container_layout_policy(
                    pool,
                    Some(ContainerLayoutPolicy {
                        header: HeaderPolicy {
                            side: HeaderSide::Top,
                            extent: 24.0,
                        },
                        layout: ContainerLayout::Stack {
                            axis: StackAxis::Vertical,
                            spacing: 8.0,
                            padding: 8.0,
                            cross_axis: StackCrossAxisPolicy::Fill,
                            resize: StackResizePolicy::Preserve,
                        },
                    }),
                )
                .unwrap();
            if with_stencil {
                let package = find_package("thinoda.general").unwrap();
                book.active_mut()
                    .add_canvas_object(new_object(
                        package,
                        package.definition("message").unwrap(),
                        Vec2::new(250.0, 0.0),
                    ))
                    .unwrap();
            }
            if collapsed {
                book.active_mut()
                    .set_canvas_container_collapsed(
                        pool,
                        Some(Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 30.0))),
                    )
                    .unwrap();
            }

            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let restored: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(restored.workbook, book);
                assert_eq!(restored.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let layout_version = if ink.is_some() {
                    match (collapsed, with_stencil) {
                        (true, _) => COLLAPSE_LAYOUT_INK_WORKBOOK_VERSION,
                        (false, true) => SCENE_STENCIL_LAYOUT_INK_WORKBOOK_VERSION,
                        (false, false) => SCENE_LAYOUT_INK_WORKBOOK_VERSION,
                    }
                } else {
                    expected_version
                };
                assert_eq!(older["workbook"][0], layout_version);
                let old_version = if ink.is_some() {
                    if collapsed {
                        COLLAPSE_INK_WORKBOOK_VERSION
                    } else if with_stencil {
                        SCENE_STENCIL_INK_WORKBOOK_VERSION
                    } else {
                        SCENE_INK_WORKBOOK_VERSION
                    }
                } else {
                    old_version
                };
                older["workbook"][0] = serde_json::json!(old_version);
                assert!(
                    serde_json::from_value::<Snapshot>(older)
                        .unwrap_err()
                        .to_string()
                        .contains("scene layout policies")
                );
            }
        }
    }

    #[test]
    fn collapsed_containers_require_aware_recovery_including_stencils_and_pending_ink() {
        use thinoda_core::{CanvasObject, Rect, ShapeKind, Vec2};
        let mut book = Workbook::default();
        let pool = book
            .active_mut()
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 100.0)),
            ))
            .unwrap();
        book.active_mut().set_canvas_container(pool, true).unwrap();
        book.active_mut()
            .set_canvas_container_collapsed(
                pool,
                Some(Rect::from_min_size(Vec2::ZERO, Vec2::new(200.0, 30.0))),
            )
            .unwrap();
        for with_stencil in [false, true] {
            if with_stencil {
                let package =
                    thinoda_core::stencil_object::find_package("thinoda.general").unwrap();
                book.active_mut()
                    .add_canvas_object(thinoda_core::stencil_object::new_object(
                        package,
                        package.definition("message").unwrap(),
                        Vec2::new(250.0, 0.0),
                    ))
                    .unwrap();
            }
            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let restored: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(restored.workbook, book);
                assert_eq!(restored.pending_ink.as_deref(), ink);
                let mut older: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    older["workbook"][0],
                    if ink.is_some() {
                        COLLAPSE_INK_WORKBOOK_VERSION
                    } else {
                        COLLAPSE_WORKBOOK_VERSION
                    }
                );
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    SCENE_STENCIL_INK_WORKBOOK_VERSION
                } else {
                    SCENE_STENCIL_WORKBOOK_VERSION
                });
                assert!(
                    serde_json::from_value::<Snapshot>(older)
                        .unwrap_err()
                        .to_string()
                        .contains("collapsed")
                );
            }
        }
    }

    #[test]
    fn final_states_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasObject, DrawioRectangleStencil, Rect, ShapeKind, ShapeRounding,
            ShapeRoundingMode, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut rounded = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        rounded.rounding = Some(ShapeRounding {
            radius: 15.5,
            stencil: DrawioRectangleStencil::Standard,
            mode: ShapeRoundingMode::Relative,
            rounded_perimeter: true,
        });
        book.active_mut().add_canvas_object(rounded).unwrap();
        book.active_mut()
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::FinalState,
                Rect::from_min_size(Vec2::new(120.0, 0.0), Vec2::new(30.0, 30.0)),
            ))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    FINAL_STATE_INK_WORKBOOK_VERSION
                } else {
                    FINAL_STATE_WORKBOOK_VERSION
                }
            );
            let mut older = value.clone();
            older["workbook"][0] = serde_json::json!(if ink.is_some() {
                DRAWIO_ROUNDING_INK_WORKBOOK_VERSION
            } else {
                DRAWIO_ROUNDING_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(older)
                    .unwrap_err()
                    .to_string()
                    .contains("final states require a final-state-aware")
            );
            let mut unversioned = value;
            unversioned["workbook"] = serde_json::to_value(&book).unwrap();
            assert!(serde_json::from_value::<Snapshot>(unversioned).is_err());
        }
    }

    #[test]
    fn drawio_rounding_requires_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, ConnectorPort, DrawioRectangleStencil, Endpoint, Rect,
            ShapeKind, ShapeRounding, ShapeRoundingMode, Vec2,
        };

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut shape = CanvasObject::shape(
            ShapeKind::RoundedRectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
        );
        shape.rounding = Some(ShapeRounding {
            radius: 15.5,
            stencil: DrawioRectangleStencil::Standard,
            mode: ShapeRoundingMode::Relative,
            rounded_perimeter: true,
        });
        let id = book.active_mut().add_canvas_object(shape).unwrap();
        let mut connector =
            CanvasConnector::new(Endpoint::Object(id), Endpoint::Point(Vec2::new(200.0, 0.0)));
        connector.from_port = Some(ConnectorPort {
            position: Vec2::new(1.0, 0.0),
            offset: Vec2::ZERO,
            perimeter: true,
        });
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        for ink in [None, Some("pending ink")] {
            let bytes = match ink {
                Some(ink) => {
                    Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                        .unwrap()
                }
                None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
            };
            let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(snapshot.workbook, book);
            assert_eq!(snapshot.pending_ink.as_deref(), ink);
            assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value["workbook"][0],
                if ink.is_some() {
                    DRAWIO_ROUNDING_INK_WORKBOOK_VERSION
                } else {
                    DRAWIO_ROUNDING_WORKBOOK_VERSION
                }
            );
            value["workbook"][0] = serde_json::json!(if ink.is_some() {
                FIXED_PORT_INK_WORKBOOK_VERSION
            } else {
                FIXED_PORT_WORKBOOK_VERSION
            });
            assert!(
                serde_json::from_value::<Snapshot>(value)
                    .unwrap_err()
                    .to_string()
                    .contains("standard draw.io rectangle rounding requires")
            );
        }
    }

    #[test]
    fn double_ellipses_require_aware_recovery_with_and_without_pending_ink() {
        use thinoda_core::{
            CanvasObject, DrawioRectangleStencil, Rect, RectangleSides, ShapeKind, ShapeRounding,
            Vec2,
        };

        for margin in [None, Some(8.5)] {
            let mut book = formatted_workbook(thinoda_core::CardKind::Text);
            let mut ellipse = CanvasObject::shape(
                ShapeKind::DoubleEllipse,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 60.0)),
            );
            ellipse.double_ellipse_margin = margin;
            book.active_mut().add_canvas_object(ellipse).unwrap();
            let mut rectangle = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(120.0, 0.0), Vec2::new(100.0, 60.0)),
            );
            rectangle.rectangle_sides = Some(RectangleSides::default());
            book.active_mut().add_canvas_object(rectangle).unwrap();
            let mut rounded = CanvasObject::shape(
                ShapeKind::RoundedRectangle,
                Rect::from_min_size(Vec2::new(240.0, 0.0), Vec2::new(100.0, 60.0)),
            );
            rounded.rounding = Some(ShapeRounding {
                radius: 15.0,
                stencil: DrawioRectangleStencil::Forms,
                ..Default::default()
            });
            book.active_mut().add_canvas_object(rounded).unwrap();

            for ink in [None, Some("pending ink")] {
                let bytes = match ink {
                    Some(ink) => {
                        Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, ink)
                            .unwrap()
                    }
                    None => Snapshot::to_bytes(SnapshotMeta::default(), &book).unwrap(),
                };
                let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(snapshot.workbook, book);
                assert_eq!(snapshot.pending_ink.as_deref(), ink);
                assert!(serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes).is_err());
                let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    value["workbook"][0],
                    if ink.is_some() {
                        DOUBLE_ELLIPSE_INK_WORKBOOK_VERSION
                    } else {
                        DOUBLE_ELLIPSE_WORKBOOK_VERSION
                    }
                );
                let mut older = value.clone();
                older["workbook"][0] = serde_json::json!(if ink.is_some() {
                    RECTANGLE_SIDES_INK_WORKBOOK_VERSION
                } else {
                    RECTANGLE_SIDES_WORKBOOK_VERSION
                });
                assert!(
                    serde_json::from_value::<Snapshot>(older)
                        .unwrap_err()
                        .to_string()
                        .contains("double ellipses require a double-ellipse-aware")
                );
                let mut unversioned = value.clone();
                unversioned["workbook"] = serde_json::to_value(&book).unwrap();
                assert!(
                    serde_json::from_value::<Snapshot>(unversioned)
                        .unwrap_err()
                        .to_string()
                        .contains("double ellipses require a versioned recovery workbook")
                );
                let mut malformed = value;
                malformed["workbook"][1]["sheets"][0]["canvas"]["objects"][0]["double_ellipse_margin"] =
                    serde_json::json!("invalid");
                assert!(serde_json::from_value::<Snapshot>(malformed).is_err());
            }
        }
    }

    #[test]
    fn pending_ink_envelope_round_trips_and_rejects_old_readers() {
        use thinoda_core::{CanvasConnector, CanvasObject, Endpoint, Vec2};

        let mut book = formatted_workbook(thinoda_core::CardKind::Text);
        let mut connector = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        );
        connector.waypoints = vec![Vec2::new(50.0, 50.0)].into();
        book.active_mut()
            .add_canvas_object(CanvasObject::connector(connector))
            .unwrap();
        book.validate().unwrap();
        assert!(book.has_inline_text());
        assert!(book.has_connector_waypoints());
        let meta = SnapshotMeta {
            title: "Pending ink".into(),
            dirty: true,
            ..SnapshotMeta::default()
        };
        let payload = r#"{"version":1,"samples":[[0,4294967295]],"feedback":"NaN"}"#;
        let bytes = Snapshot::to_bytes_with_pending_ink(meta.clone(), &book, payload).unwrap();
        let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(snapshot.meta, meta);
        assert_eq!(snapshot.workbook, book);
        assert_eq!(snapshot.pending_ink.as_deref(), Some(payload));
        assert_eq!(serde_json::to_vec(&snapshot).unwrap(), bytes);
        assert!(serde_json::from_slice::<LegacySnapshot>(&bytes).is_err());
        let old_error = serde_json::from_slice::<PreviousVersionedSnapshot>(&bytes)
            .unwrap_err()
            .to_string();
        assert!(old_error.contains("unsupported recovery workbook version"));

        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["workbook"][0], INK_WORKBOOK_VERSION);
        let mut future = value.clone();
        future["workbook"][0] = serde_json::json!("THINODA-RECOVERY-INK-999");
        assert!(serde_json::from_value::<Snapshot>(future).is_err());
        let mut missing = value.clone();
        missing["workbook"].as_array_mut().unwrap().pop();
        assert!(serde_json::from_value::<Snapshot>(missing).is_err());
        let mut extra = value.clone();
        extra["workbook"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(null));
        assert!(serde_json::from_value::<Snapshot>(extra).is_err());
        let mut empty = value.clone();
        empty["workbook"][2] = serde_json::json!("");
        assert!(serde_json::from_value::<Snapshot>(empty).is_err());
        let mut wrong_type = value;
        wrong_type["workbook"][2] = serde_json::json!({});
        assert!(serde_json::from_value::<Snapshot>(wrong_type).is_err());
        assert!(Snapshot::to_bytes_with_pending_ink(meta, &book, "").is_err());
    }

    #[test]
    fn pending_ink_size_limit_is_enforced_on_read_and_write() {
        let book = Workbook::default();
        let payload = "a".repeat(MAX_PENDING_INK_BYTES);
        let bytes =
            Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, &payload).unwrap();
        let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(snapshot.pending_ink.as_deref(), Some(payload.as_str()));

        let oversized = "a".repeat(MAX_PENDING_INK_BYTES + 1);
        assert!(
            Snapshot::to_bytes_with_pending_ink(SnapshotMeta::default(), &book, &oversized)
                .is_err()
        );
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["workbook"][2] = serde_json::json!(oversized);
        assert!(serde_json::from_value::<Snapshot>(value).is_err());
    }

    #[test]
    fn pending_ink_orphan_requires_a_complete_consumer_and_survives_handoff() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let source = RecoveryManager::start(&paths).unwrap();
        let book = Workbook::default();
        let meta = SnapshotMeta {
            title: "Interrupted".into(),
            dirty: true,
            ..SnapshotMeta::default()
        };
        source
            .persist_snapshot_with_pending_ink("doc", meta.clone(), &book, "pending")
            .unwrap();
        let path = source.session_dir().join("docs").join("doc.json");
        let original = std::fs::read(&path).unwrap();
        assert!(
            source
                .persist_snapshot_with_pending_ink("doc", meta.clone(), &book, "")
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(source.try_flush_for_crash());
        source.suspend().unwrap();
        drop(source);

        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans.len(), 1);
        let document = &orphans[0].docs[0];
        assert!(matches!(document.load(), Err(Error::InvalidRecovery(_))));
        let snapshot = document.load_snapshot().unwrap();
        assert_eq!(snapshot.workbook, book);
        assert_eq!(snapshot.pending_ink.as_deref(), Some("pending"));

        let replacement = RecoveryManager::start(&paths).unwrap();
        replacement
            .persist_snapshot_with_pending_ink(
                "replacement",
                snapshot.meta,
                &snapshot.workbook,
                snapshot.pending_ink.as_deref().unwrap(),
            )
            .unwrap();
        orphans[0].discard().unwrap();
        drop(orphans);
        assert!(replacement.try_flush_for_crash());
        replacement.suspend().unwrap();
        drop(replacement);
        let recovered = RecoveryManager::find_orphans(&paths);
        assert_eq!(recovered.len(), 1);
        let snapshot = recovered[0].docs[0].load_snapshot().unwrap();
        assert_eq!(snapshot.workbook, book);
        assert_eq!(snapshot.pending_ink.as_deref(), Some("pending"));
        recovered[0].discard().unwrap();
        drop(recovered);
    }

    #[test]
    fn recovery_load_restores_formatted_cards_and_rejects_malformed_runs() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let manager = RecoveryManager::start(&paths).unwrap();
        let mut book = formatted_workbook(thinoda_core::CardKind::Sticky);
        manager
            .snapshot("rich", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans[0].docs[0].load().unwrap(), book);
        drop(orphans);
        manager.resume().unwrap();
        let sheet = book.active_mut();
        let card = sheet.floating[0];
        sheet.topic_mut(card).unwrap().inline_text.runs[0].end = 100;
        manager
            .snapshot("rich", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert!(orphans[0].docs[0].load().is_err());
        drop(orphans);
        manager.resume().unwrap();
        manager.finish().unwrap();
    }

    #[test]
    fn recovery_restores_embedded_bytes_and_rejects_missing_resources() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let manager = RecoveryManager::start(&paths).unwrap();
        let mut book = Workbook::default();
        let sheet = book.active_mut();
        let reference = sheet
            .resources
            .insert(
                thinoda_core::resources::Resource::new(
                    "evidence.txt",
                    "text/plain",
                    b"evidence".to_vec(),
                )
                .unwrap(),
            )
            .unwrap();
        sheet.topic_mut(sheet.root).unwrap().image = Some(reference);
        sheet.topic_mut(sheet.root).unwrap().image_size =
            Some(thinoda_core::Vec2::new(240.0, 120.0));
        sheet.topic_mut(sheet.root).unwrap().image_below = true;
        thinoda_core::attachments::insert(
            sheet,
            sheet.root,
            "opaque.docm",
            b"\0PK\x03\x04macro bytes\xff".to_vec(),
        )
        .unwrap();
        manager
            .snapshot("resource", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        let recovered = orphans[0].docs[0].load().unwrap();
        assert_eq!(recovered, book);
        drop(orphans);
        manager.resume().unwrap();
        book.active_mut().resources.entries.clear();
        manager
            .snapshot("resource", SnapshotMeta::default(), &book)
            .unwrap();
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert!(orphans[0].docs[0].load().is_err());
        drop(orphans);
        manager.resume().unwrap();
        manager.finish().unwrap();
    }

    #[test]
    fn crash_flush_never_waits_for_busy_mutexes() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let manager = RecoveryManager::start(&Paths::from_root(dir.path())).unwrap();
        let pending = manager.pending.lock().unwrap();
        assert!(!manager.try_flush_for_crash());
        drop(pending);
        let state = manager.state.lock().unwrap();
        assert!(!manager.try_flush_for_crash());
        drop(state);
        assert!(manager.try_flush_for_crash());
        manager.finish().unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_snapshot_queued_during_a_flush_remains_unflushed() {
        use std::{sync::mpsc, time::Duration};

        for fail in [false, true] {
            let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
            let manager = RecoveryManager::start(&Paths::from_root(dir.path())).unwrap();
            manager.queue_snapshot("doc", b"earlier".to_vec()).unwrap();
            let (started_tx, started_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            std::thread::scope(|scope| {
                let manager = &manager;
                let writer = scope.spawn(move || {
                    manager.flush_pending(|| {
                        started_tx.send(()).unwrap();
                        resume_rx
                            .recv_timeout(Duration::from_secs(5))
                            .map_err(io::Error::other)?;
                        if fail {
                            Err(io::Error::other("injected write failure"))
                        } else {
                            Ok(())
                        }
                    })
                });
                started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(manager.has_unflushed_snapshots());
                manager.queue_snapshot("doc", b"newer".to_vec()).unwrap();
                resume_tx.send(()).unwrap();
                assert_eq!(writer.join().unwrap().is_err(), fail);
            });
            assert!(manager.has_unflushed_snapshots());
            let path = manager.session_dir.join("docs").join("doc.json");
            if fail {
                assert!(!fs::exists(&path));
            } else {
                assert_eq!(fs::read(&path).unwrap(), b"earlier");
            }
            manager.flush().unwrap();
            assert!(!manager.has_unflushed_snapshots());
            assert_eq!(fs::read(path).unwrap(), b"newer");
            manager.finish().unwrap();
        }
    }

    #[test]
    fn pending_flush_rechecks_ownership_before_each_document() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let manager = RecoveryManager::start(&Paths::from_root(dir.path())).unwrap();
        let first = b"first snapshot".to_vec();
        let second = b"second snapshot".to_vec();
        let mut pending = BTreeMap::from([
            ("a".to_owned(), first.clone()),
            ("b".to_owned(), second.clone()),
        ]);
        let mut checks = 0;
        let result = manager.write_pending(&mut pending, || {
            checks += 1;
            if checks == 2 {
                Err(io::Error::new(io::ErrorKind::WouldBlock, "claim lost"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_eq!(checks, 2);
        assert_eq!(
            fs::read(manager.session_dir.join("docs").join("a.json")).unwrap(),
            first
        );
        assert!(!fs::exists(manager.session_dir.join("docs").join("b.json")));
        assert_eq!(pending, BTreeMap::from([("b".to_owned(), second.clone())]));
        manager.write_pending(&mut pending, || Ok(())).unwrap();
        assert_eq!(
            fs::read(manager.session_dir.join("docs").join("b.json")).unwrap(),
            second
        );
        assert!(pending.is_empty());
        manager.finish().unwrap();
    }

    #[test]
    fn suspended_sessions_are_recoverable_until_resumed() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let manager = RecoveryManager::start(&paths).unwrap();
        manager
            .queue_snapshot(
                "doc",
                Snapshot::to_bytes(SnapshotMeta::default(), &Workbook::default()).unwrap(),
            )
            .unwrap();
        assert!(RecoveryManager::find_orphans(&paths).is_empty());
        manager.suspend().unwrap();
        let orphans = RecoveryManager::find_orphans(&paths);
        assert_eq!(orphans.len(), 1);
        assert!(
            manager.resume().is_err(),
            "a claimed session must not be re-locked"
        );
        drop(orphans);
        manager.resume().unwrap();
        assert!(RecoveryManager::find_orphans(&paths).is_empty());
        manager.finish().unwrap();
    }
}
