mod conversion;

pub use conversion::nusamai_to_geojson_geometry;

/// An intermediate function to create a "geojson feature" from a "geojson geometry"
// TODO: Handle properties
pub fn geojson_geometry_to_feature(geojson_geom: geojson::Geometry) -> geojson::Feature {
    geojson::Feature {
        bbox: None,
        geometry: Some(geojson_geom),
        id: None,
        properties: None,
        foreign_members: None,
    }
}
