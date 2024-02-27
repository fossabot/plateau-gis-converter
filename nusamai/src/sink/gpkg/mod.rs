//! GeoPackage sink

mod attributes;
mod bbox;
mod table;

use std::collections::HashSet;
use std::path::PathBuf;
use url::Url;

use indexmap::IndexMap;

use rayon::prelude::*;

use crate::parameters::*;
use crate::pipeline::{Feedback, PipelineError, Receiver, Result};
use crate::sink::{DataSink, DataSinkProvider, SinkInfo};
use crate::{get_parameter_value, transformer};
use attributes::prepare_object_attributes;
use bbox::{get_indexed_multipolygon_bbox, Bbox};
use table::schema_to_table_infos;

use nusamai_citygml::object::{ObjectStereotype, Value};
use nusamai_citygml::schema::Schema;
use nusamai_citygml::GeometryType;
use nusamai_gpkg::geometry::write_indexed_multipolygon;
use nusamai_gpkg::GpkgHandler;

pub struct GpkgSinkProvider {}

impl DataSinkProvider for GpkgSinkProvider {
    fn info(&self) -> SinkInfo {
        SinkInfo {
            id_name: "gpkg".to_string(),
            name: "GeoPackage".to_string(),
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

        Box::<GpkgSink>::new(GpkgSink {
            output_path: output_path.as_ref().unwrap().into(),
        })
    }
}

pub struct GpkgSink {
    output_path: PathBuf,
}

// An ephimeral container to wrap and pass the data in the pipeline
// Corresponds to a record in the features/attributes table of GeoPackage
enum Record {
    Feature {
        obj_id: String,
        geometry: Vec<u8>,
        bbox: Bbox,
        attributes: IndexMap<String, String>,
    },
    Attribute {
        attributes: IndexMap<String, String>,
    },
}

impl GpkgSink {
    pub async fn run_async(
        &mut self,
        upstream: Receiver,
        feedback: &Feedback,
        schema: &Schema,
    ) -> Result<()> {
        let mut handler = if self.output_path.to_string_lossy().starts_with("sqlite:") {
            // note: unlike the case of the file system path, the database is not cleared even if it already exists
            // this is mainly expected to be used with `sqlite::memory:` for the testing purpose
            GpkgHandler::from_url(&Url::parse(self.output_path.to_str().unwrap()).unwrap())
                .await
                .unwrap()
        } else {
            // delete the db file first is already exists
            if self.output_path.exists() {
                std::fs::remove_file(&self.output_path).unwrap();
            };

            GpkgHandler::from_url(
                &Url::parse(&format!("sqlite://{}", self.output_path.to_str().unwrap())).unwrap(),
            )
            .await
            .unwrap()
        };

        let table_infos = schema_to_table_infos(schema);
        let mut created_tables = HashSet::<String>::new();

        let mut table_bboxes = IndexMap::<String, Bbox>::new();

        let (sender, mut receiver) = tokio::sync::mpsc::channel(100);

        let producers = {
            let feedback = feedback.clone();
            tokio::task::spawn_blocking(move || {
                upstream
                    .into_iter()
                    .par_bridge()
                    .try_for_each_with(sender, |sender, parcel| {
                        feedback.ensure_not_canceled()?;

                        let entity = parcel.entity;
                        let geom_store = entity.geometry_store.read().unwrap();

                        let Value::Object(obj) = &entity.root else {
                            return Ok(());
                        };

                        match &obj.stereotype {
                            ObjectStereotype::Feature {
                                id: obj_id,
                                geometries,
                            } => {
                                let mut mpoly = nusamai_geometry::MultiPolygon::new();

                                geometries.iter().for_each(|entry| match entry.ty {
                                    GeometryType::Solid
                                    | GeometryType::Surface
                                    | GeometryType::Triangle => {
                                        for idx_poly in geom_store.multipolygon.iter_range(
                                            entry.pos as usize..(entry.pos + entry.len) as usize,
                                        ) {
                                            mpoly.push(&idx_poly);
                                        }
                                    }
                                    GeometryType::Curve => unimplemented!(),
                                    GeometryType::Point => unimplemented!(),
                                });

                                if mpoly.is_empty() {
                                    return Ok(());
                                }

                                let mut bytes = Vec::new();
                                if write_indexed_multipolygon(
                                    &mut bytes,
                                    &geom_store.vertices,
                                    &mpoly,
                                    4326,
                                )
                                .is_err()
                                {
                                    // TODO: fatal error
                                }

                                let table_name = obj.typename.to_string();
                                let record = Record::Feature {
                                    obj_id: obj_id.clone(),
                                    geometry: bytes,
                                    bbox: get_indexed_multipolygon_bbox(
                                        &geom_store.vertices,
                                        &mpoly,
                                    ),
                                    attributes: prepare_object_attributes(obj),
                                };
                                if sender.blocking_send((table_name, record)).is_err() {
                                    return Err(PipelineError::Canceled);
                                };
                            }
                            ObjectStereotype::Data => {
                                let table_name = obj.typename.to_string();
                                let record = Record::Attribute {
                                    attributes: prepare_object_attributes(obj),
                                };
                                if sender.blocking_send((table_name, record)).is_err() {
                                    return Err(PipelineError::Canceled);
                                };
                            }
                            ObjectStereotype::Object { id: obj_id } => {
                                // TODO: implement (you will also need the corresponding TypeDef::Object in the schema)
                                log::warn!(
                                    "ObjectStereotype::Object is not supported yet: id = {}",
                                    obj_id
                                );
                            }
                        }

                        Ok(())
                    })
            })
        };

        let mut tx = handler.begin().await.unwrap();
        while let Some((table_name, record)) = receiver.recv().await {
            feedback.ensure_not_canceled()?;

            if !created_tables.contains(&table_name) {
                let tf = table_infos.get(&table_name).unwrap();
                tx.add_table(tf).await.unwrap();
                created_tables.insert(table_name.clone());
            }

            match record {
                Record::Feature {
                    obj_id,
                    geometry,
                    bbox,
                    attributes,
                } => {
                    tx.insert_feature(&table_name, &obj_id, &geometry, &attributes)
                        .await
                        .unwrap();
                    table_bboxes.entry(table_name).or_default().merge(&bbox);
                }
                Record::Attribute { attributes } => {
                    tx.insert_attribute(&table_name, &attributes).await.unwrap();
                }
            }
        }

        for (table_name, bbox) in table_bboxes {
            tx.update_bbox(&table_name, bbox.to_tuple()).await.unwrap();
        }

        tx.commit().await.unwrap();

        match producers.await.unwrap() {
            Ok(_) | Err(PipelineError::Canceled) => Ok(()),
            error @ Err(_) => error,
        }
    }
}

impl DataSink for GpkgSink {
    fn make_transform_requirements(&self) -> transformer::Requirements {
        transformer::Requirements {
            tree_flattening: transformer::TreeFlatteningSpec::Flatten {
                feature: transformer::FeatureFlatteningOption::AllExceptThematicSurfaces,
                data: transformer::DataFlatteningOption::TopLevelOnly,
                object: transformer::ObjectFlatteningOption::None,
            },
            ..Default::default()
        }
    }

    fn run(&mut self, upstream: Receiver, feedback: &Feedback, schema: &Schema) -> Result<()> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(self.run_async(upstream, feedback, schema))
    }
}
