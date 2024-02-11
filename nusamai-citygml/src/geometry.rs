use nusamai_geometry::{MultiLineString, MultiPoint, MultiPolygon};
use nusamai_projection::crs::*;

use crate::LocalId;

#[derive(Debug, Clone, Copy)]
pub enum GeometryParseType {
    Geometry,
    Solid,
    MultiSurface,
    MultiCurve,
    MultiPoint,
    Surface,
    Point,
    Triangulated,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum GeometryType {
    /// Polygons (solids)
    Solid,
    /// Polygons (surfaces)
    Surface,
    /// Polygons (triangles)
    Triangle,
    /// Line-strings
    Curve,
    /// Points
    Point,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GeometryRef {
    #[serde(rename = "type")]
    pub ty: GeometryType,
    pub lod: u8,
    pub pos: u32,
    pub len: u32,
}

pub type GeometryRefs = Vec<GeometryRef>;

/// Geometries in a single city object and all its children.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Default)]
pub struct GeometryStore {
    /// EPSG code of the Coordinate Reference System (CRS) for this geometry
    pub epsg: EPSGCode,

    /// Shared vertex buffer for all geometries
    pub vertices: Vec<[f64; 3]>,

    /// All polygons, referenced by `GeometryRefs`
    pub multipolygon: MultiPolygon<'static, 1, u32>,
    /// All line-strings, referenced by `GeometryRefs`
    pub multilinestring: MultiLineString<'static, 1, u32>,
    /// All points, referenced by `GeometryRefs`
    pub multipoint: MultiPoint<'static, 1, u32>,

    /// Ring ids of the all polygons
    pub ring_ids: Vec<Option<LocalId>>,
    /// List of surface ids and their spans in `multipolygon`
    pub surface_spans: Vec<SurfaceSpan>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct SurfaceSpan {
    pub id: LocalId,
    pub start: u32,
    pub end: u32,
}

/// Temporary storage for the parser to collect geometries.
#[derive(Default)]
pub(crate) struct GeometryCollector {
    pub vertices: indexmap::IndexSet<[u64; 3], ahash::RandomState>,
    pub multipolygon: MultiPolygon<'static, 1, u32>,
    pub multilinestring: MultiLineString<'static, 1, u32>,
    pub multipoint: MultiPoint<'static, 1, u32>,

    /// ring ids of the all polygons
    pub ring_ids: Vec<Option<LocalId>>,

    /// surface polygon spans in `multipolygon`
    pub surface_spans: Vec<SurfaceSpan>,
}

impl GeometryCollector {
    pub fn add_exterior_ring(
        &mut self,
        iter: impl IntoIterator<Item = [f64; 3]>,
        ring_id: Option<LocalId>,
    ) -> usize {
        self.ring_ids.push(ring_id);
        self.multipolygon.add_exterior(iter.into_iter().map(|v| {
            let vbits = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];
            let (index, _) = self.vertices.insert_full(vbits);
            [index as u32]
        }));

        self.multipolygon.len() - 1
    }

    pub fn add_interior_ring(
        &mut self,
        iter: impl IntoIterator<Item = [f64; 3]>,
        ring_id: Option<LocalId>,
    ) {
        self.ring_ids.push(ring_id);
        self.multipolygon.add_interior(iter.into_iter().map(|v| {
            let vbits = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];
            let (index, _) = self.vertices.insert_full(vbits);
            [index as u32]
        }));
    }

    pub fn into_geometries(self) -> GeometryStore {
        let mut vertices = Vec::with_capacity(self.vertices.len());
        for vbits in &self.vertices {
            vertices.push([
                f64::from_bits(vbits[0]),
                f64::from_bits(vbits[1]),
                f64::from_bits(vbits[2]),
            ]);
        }

        GeometryStore {
            epsg: EPSG_JGD2011_GEOGRAPHIC_3D,
            vertices,
            multipolygon: self.multipolygon,
            multilinestring: self.multilinestring,
            multipoint: self.multipoint,
            ring_ids: self.ring_ids,
            surface_spans: self.surface_spans,
        }
    }
}
