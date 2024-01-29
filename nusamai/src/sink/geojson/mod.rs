//! GeoJSON sink

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use nusamai_citygml::schema::Schema;
use nusamai_citygml::GeometryType;
use rayon::prelude::*;

use crate::parameters::*;
use crate::pipeline::{Feedback, Receiver};
use crate::sink::{DataSink, DataSinkProvider, SinkInfo};
use crate::{get_parameter_value, transformer};

use nusamai_citygml::object::{Entity, ObjectStereotype, Value};
use nusamai_geojson::conversion::{
    indexed_linestring_to_value, indexed_point_to_value, indexed_polygon_to_value,
};

pub struct GeoJsonSinkProvider {}

impl DataSinkProvider for GeoJsonSinkProvider {
    fn info(&self) -> SinkInfo {
        SinkInfo {
            name: "GeoJSON".to_string(),
        }
    }

    fn parameters(&self) -> Parameters {
        let mut params = Parameters::new();
        params.define(
            "@output".into(),
            ParameterEntry {
                description: "Output file path".into(),
                required: true,
                parameter: ParameterType::FileSystemPath(FileSystemPathParameter {
                    value: None,
                    must_exist: false,
                }),
            },
        );
        params
    }

    fn create(&self, params: &Parameters) -> Box<dyn DataSink> {
        let output_path = get_parameter_value!(params, "@output", FileSystemPath);

        Box::<GeoJsonSink>::new(GeoJsonSink {
            output_path: output_path.as_ref().unwrap().into(),
        })
    }
}

pub struct GeoJsonSink {
    output_path: PathBuf,
}

impl DataSink for GeoJsonSink {
    fn make_transform_requirements(&self) -> transformer::Requirements {
        use transformer::RequirementItem;

        transformer::Requirements {
            mergedown: RequirementItem::Required(transformer::Mergedown::Geometry),
            ..Default::default()
        }
    }

    fn run(&mut self, upstream: Receiver, feedback: &Feedback, _schema: &Schema) {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1000);

        rayon::join(
            || {
                // Convert CityObjects to GeoJSON objects

                let _ = upstream.into_iter().par_bridge().try_for_each_with(
                    sender,
                    |sender, parcel| {
                        if feedback.is_cancelled() {
                            return Err(());
                        }

                        let features = toplevel_cityobj_to_geojson_features(&parcel.entity);
                        for feature in features {
                            let Ok(bytes) = serde_json::to_vec(&feature) else {
                                // TODO: fatal error
                                return Err(());
                            };
                            if sender.send(bytes).is_err() {
                                log::info!("sink cancelled");
                                return Err(());
                            };
                        }
                        Ok(())
                    },
                );
            },
            || {
                // Write GeoJSON to a file

                // TODO: Handle output file path
                let mut file = File::create(&self.output_path).unwrap();
                let mut writer = BufWriter::with_capacity(1024 * 1024, &mut file);

                // Write the FeatureCollection header
                writer
                    .write_all(b"{\"type\":\"FeatureCollection\",\"features\":[")
                    .unwrap();

                // Write each Feature
                let mut iter = receiver.into_iter().peekable();
                while let Some(bytes) = iter.next() {
                    writer.write_all(&bytes).unwrap();
                    if iter.peek().is_some() {
                        writer.write_all(b",").unwrap();
                    };
                }

                // Write the FeautureCollection footer and EOL
                writer.write_all(b"]}\n").unwrap();
            },
        );
    }
}

fn extract_properties(value: &nusamai_citygml::object::Value) -> Option<geojson::JsonObject> {
    match &value {
        obj @ nusamai_citygml::Value::Object(_) => match obj.to_attribute_json() {
            serde_json::Value::Object(map) => Some(map),
            _ => unreachable!(),
        },
        _ => panic!("Root value type must be Feature, but found {:?}", value),
    }
}

/// Create GeoJSON features from a TopLevelCityObject
/// Each feature for MultiPolygon, MultiLineString, and MultiPoint will be created (if it exists)
pub fn toplevel_cityobj_to_geojson_features(entity: &Entity) -> Vec<geojson::Feature> {
    let mut geojson_features: Vec<geojson::Feature> = Vec::with_capacity(1);
    let properties = extract_properties(&entity.root);
    let geom_store = entity.geometry_store.read().unwrap();

    let Value::Object(obj) = &entity.root else {
        return Vec::default();
    };
    let ObjectStereotype::Feature { id, geometries } = &obj.stereotype else {
        return Vec::default();
    };

    let mut polygons = Vec::new();
    let mut linestrings = Vec::new();
    let mut points = Vec::new();

    geometries.iter().for_each(|entry| match entry.ty {
        GeometryType::Solid | GeometryType::Surface | GeometryType::Triangle => {
            for idx_poly in geom_store
                .multipolygon
                .iter_range(entry.pos as usize..(entry.pos + entry.len) as usize)
            {
                let value = indexed_polygon_to_value(&geom_store.vertices, &idx_poly);
                let geojson::Value::Polygon(poly) = value else {
                    unreachable!()
                };
                polygons.push(poly);
            }
        }
        GeometryType::Curve => {
            for idx_ls in geom_store
                .multilinestring
                .iter_range(entry.pos as usize..(entry.pos + entry.len) as usize)
            {
                let value = indexed_linestring_to_value(&geom_store.vertices, &idx_ls);
                let geojson::Value::LineString(ls) = value else {
                    unreachable!()
                };
                linestrings.push(ls);
            }
        }
        GeometryType::Point => {
            for idx_point in geom_store
                .multipoint
                .iter_range(entry.pos as usize..(entry.pos + entry.len) as usize)
            {
                let value = indexed_point_to_value(&geom_store.vertices, idx_point[0]);
                let geojson::Value::Point(point) = value else {
                    unreachable!()
                };
                points.push(point);
            }
        }
    });

    if !polygons.is_empty() {
        let mpoly_geojson_feat = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Value::MultiPolygon(polygons).into()),
            id: Some(geojson::feature::Id::String(id.clone())),
            properties: properties.clone(),
            foreign_members: None,
        };
        geojson_features.push(mpoly_geojson_feat);
    }

    if !linestrings.is_empty() {
        let mpoly_geojson_feat = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Value::MultiLineString(linestrings).into()),
            id: Some(geojson::feature::Id::String(id.clone())),
            properties: properties.clone(),
            foreign_members: None,
        };
        geojson_features.push(mpoly_geojson_feat);
    }

    if !points.is_empty() {
        let mpoly_geojson_feat = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Value::MultiPoint(points).into()),
            id: Some(geojson::feature::Id::String(id.clone())),
            properties: properties.clone(),
            foreign_members: None,
        };
        geojson_features.push(mpoly_geojson_feat);
    }

    geojson_features
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use super::*;
    use nusamai_citygml::{object::Object, GeometryRefEntry, Value};
    use nusamai_geometry::MultiPolygon;
    use nusamai_projection::crs::EPSG_JGD2011_GEOGRAPHIC_3D;

    #[test]
    fn test_toplevel_cityobj_multipolygon() {
        let vertices: Vec<[f64; 3]> = vec![
            [0., 0., 111.],
            [5., 0., 111.],
            [5., 5., 111.],
            [0., 5., 111.],
        ];
        let mut mpoly = MultiPolygon::<1, u32>::new();
        mpoly.add_exterior([[0], [1], [2], [3], [0]]);
        let geometries = nusamai_citygml::GeometryStore {
            epsg: EPSG_JGD2011_GEOGRAPHIC_3D,
            vertices,
            multipolygon: mpoly,
            multilinestring: Default::default(),
            multipoint: Default::default(),
        };

        let obj = Entity {
            root: Value::Object(Object {
                typename: "dummy".into(),
                attributes: Default::default(),
                stereotype: nusamai_citygml::object::ObjectStereotype::Feature {
                    id: "dummy".into(),
                    geometries: vec![GeometryRefEntry {
                        ty: GeometryType::Solid,
                        pos: 0,
                        len: 1,
                        lod: 1,
                    }],
                },
            }),
            geometry_store: RwLock::new(geometries).into(),
        };

        let geojson_features = toplevel_cityobj_to_geojson_features(&obj);
        assert_eq!(geojson_features.len(), 1);

        let mpoly_geojson = geojson_features.first().unwrap();
        assert!(mpoly_geojson.bbox.is_none());
        assert!(mpoly_geojson.foreign_members.is_none());
        if let geojson::Value::MultiPolygon(rings_list) =
            mpoly_geojson.geometry.clone().unwrap().value
        {
            for (i, rings) in rings_list.iter().enumerate() {
                match i {
                    0 => {
                        assert_eq!(rings.len(), 1);
                        assert_eq!(
                            rings[0],
                            vec![
                                [0., 0., 111.],
                                [5., 0., 111.],
                                [5., 5., 111.],
                                [0., 5., 111.],
                                [0., 0., 111.]
                            ]
                        );
                    }
                    _ => unreachable!("Unexpected number of polygons"),
                }
            }
        } else {
            unreachable!("The result is not a GeoJSON MultiPolygon");
        };
    }
}
