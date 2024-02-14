pub mod parameters;
pub mod pipeline;
pub mod sink;
pub mod source;
pub mod transformer;

pub static BUILTIN_SINKS: &[&dyn sink::DataSinkProvider] = &[
    &sink::cesiumtiles::CesiumTilesSinkProvider {},
    &sink::gpkg::GpkgSinkProvider {},
    &sink::mvt::MVTSinkProvider {},
    &sink::geojson::GeoJsonSinkProvider {},
    &sink::geojson_transform_exp::GeoJsonTransformExpSinkProvider {},
    &sink::czml::CzmlSinkProvider {},
    &sink::gltf_poc::GltfPocSinkProvider {},
    &sink::kml::KmlSinkProvider {},
    &sink::ply::StanfordPlySinkProvider {},
    &sink::serde::SerdeSinkProvider {},
    &sink::shapefile::ShapefileSinkProvider {},
    &sink::noop::NoopSinkProvider {},
];
