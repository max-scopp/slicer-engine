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
}

// ── Internal layer representation ──────────────────────────────────────────

/// One layer's geometry, bucketed by extrusion role.
///
/// Each `Vec<f32>` holds flat segment pairs `[x0,y0,z0, x1,y1,z1, width,height, …]`.
#[derive(Debug, Default)]
pub(super) struct InternalLayer {
    pub(super) z: f32,
    pub(super) outer_wall: Vec<f32>,
    pub(super) inner_wall: Vec<f32>,
    pub(super) infill: Vec<f32>,
    pub(super) top_surface: Vec<f32>,
    pub(super) bottom_surface: Vec<f32>,
    pub(super) travel: Vec<f32>,
    pub(super) other: Vec<f32>,
}

impl InternalLayer {
    pub(super) fn new(z: f32) -> Self {
        Self {
            z,
            ..Default::default()
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
        let buf = match role {
            Role::OuterWall => &mut self.outer_wall,
            Role::InnerWall => &mut self.inner_wall,
            Role::Infill => &mut self.infill,
            Role::TopSurface => &mut self.top_surface,
            Role::BottomSurface => &mut self.bottom_surface,
            Role::Travel => &mut self.travel,
            Role::Other => &mut self.other,
        };
        buf.extend_from_slice(&[x0, y0, z0, x1, y1, z1, width, height]);
    }
}
