/// Extrusion role for a GCode move.
///
/// Derived from `;TYPE:` comment lines emitted by our slicer and by
/// OrcaSlicer-compatible slicers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    OuterWall,
    InnerWall,
    Infill,
    TopSurface,
    BottomSurface,
    Travel,
    Other,
}

impl Role {
    pub(super) fn from_type_comment(s: &str) -> Self {
        let lower = s.to_ascii_lowercase();
        if lower.contains("outer") || lower.contains("perimeter") && !lower.contains("inner") {
            return Self::OuterWall;
        }
        if lower.contains("inner") || lower.contains("inner perimeter") {
            return Self::InnerWall;
        }
        if lower.contains("top") {
            return Self::TopSurface;
        }
        if lower.contains("bottom") {
            return Self::BottomSurface;
        }
        if lower.contains("infill") || lower.contains("sparse") || lower.contains("solid infill") {
            return Self::Infill;
        }
        Self::Other
    }

    pub(super) fn id(self) -> u8 {
        match self {
            Role::OuterWall => 0,
            Role::InnerWall => 1,
            Role::Infill => 2,
            Role::TopSurface => 3,
            Role::BottomSurface => 4,
            Role::Travel => 5,
            Role::Other => 6,
        }
    }
}

// ── Internal layer representation ──────────────────────────────────────────

/// Geometric kind of segments stored in a [`Block`].
///
/// `Line` blocks store 8 floats per segment: `[x0, y0, z0, x1, y1, z1, w, h]`.
/// `Arc` blocks store 11 floats per segment: `[x0, y0, z0, x1, y1, z1, cx, cy, is_cw, w, h]`
/// where `(cx, cy)` is the absolute centre on the layer's Z plane and `is_cw`
/// is `1.0` for G2 and `0.0` for G3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SegmentKind {
    Line,
    Arc,
}

impl SegmentKind {
    pub(super) fn id(self) -> u8 {
        match self {
            SegmentKind::Line => 0,
            SegmentKind::Arc => 1,
        }
    }

    pub(super) fn stride(self) -> usize {
        match self {
            SegmentKind::Line => 8,
            SegmentKind::Arc => 11,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Block {
    pub(super) role: Role,
    pub(super) kind: SegmentKind,
    pub(super) data: Vec<f32>,
}

/// One layer's geometry, composed of sequential segment blocks to preserve timeline order.
#[derive(Debug, Default)]
pub(super) struct InternalLayer {
    pub(super) z: f32,
    pub(super) blocks: Vec<Block>,
}

impl InternalLayer {
    pub(super) fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub(super) fn new(z: f32) -> Self {
        Self {
            z,
            blocks: Vec::new(),
        }
    }

    pub(super) fn push_segment(
        &mut self,
        role: Role,
        x0: f32,
        y0: f32,
        z0: f32,
        x1: f32,
        y1: f32,
        z1: f32,
        width: f32,
        height: f32,
    ) {
        let segment_data = [x0, y0, z0, x1, y1, z1, width, height];
        if let Some(last) = self.blocks.last_mut() {
            if last.role == role && last.kind == SegmentKind::Line {
                last.data.extend_from_slice(&segment_data);
                return;
            }
        }
        self.blocks.push(Block {
            role,
            kind: SegmentKind::Line,
            data: segment_data.to_vec(),
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_arc(
        &mut self,
        role: Role,
        x0: f32,
        y0: f32,
        z0: f32,
        x1: f32,
        y1: f32,
        z1: f32,
        cx: f32,
        cy: f32,
        is_cw: bool,
        width: f32,
        height: f32,
    ) {
        let arc_data = [
            x0,
            y0,
            z0,
            x1,
            y1,
            z1,
            cx,
            cy,
            if is_cw { 1.0 } else { 0.0 },
            width,
            height,
        ];
        if let Some(last) = self.blocks.last_mut() {
            if last.role == role && last.kind == SegmentKind::Arc {
                last.data.extend_from_slice(&arc_data);
                return;
            }
        }
        self.blocks.push(Block {
            role,
            kind: SegmentKind::Arc,
            data: arc_data.to_vec(),
        });
    }
}
