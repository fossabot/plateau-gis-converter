//! GeoPackage sink

use std::path::PathBuf;
use url::Url;

use indexmap::IndexMap;

use rayon::prelude::*;

use crate::parameters::Parameters;
use crate::parameters::*;
use crate::pipeline::{Feedback, PipelineError, Receiver, Result};
use crate::sink::{DataSink, DataSinkProvider, SinkInfo};
use crate::{get_parameter_value, transformer};

use nusamai_citygml::object::{ObjectStereotype, Value};
use nusamai_citygml::schema::{Schema, TypeDef, TypeRef};
use nusamai_citygml::GeometryType;
use nusamai_gpkg::geometry::write_indexed_multipolygon;
use nusamai_gpkg::GpkgHandler;

pub struct GpkgSinkProvider {}

impl DataSinkProvider for GpkgSinkProvider {
    fn info(&self) -> SinkInfo {
        SinkInfo {
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

impl GpkgSink {
    pub async fn run_async(
        &mut self,
        upstream: Receiver,
        feedback: &Feedback,
        schema: &Schema,
    ) -> Result<()> {
        let mut handler = if self.output_path.to_string_lossy().starts_with("sqlite:") {
            GpkgHandler::from_url(&Url::parse(self.output_path.to_str().unwrap()).unwrap())
                .await
                .unwrap()
        } else {
            GpkgHandler::from_url(
                &Url::parse(&format!("sqlite://{}", self.output_path.to_str().unwrap())).unwrap(),
            )
            .await
            .unwrap()
        };

        // add attribute columns
        let attribute_columns = schema_to_columns(schema);
        handler.add_columns(attribute_columns).await.unwrap();

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
                        let ObjectStereotype::Feature { id: _, geometries } = &obj.stereotype
                        else {
                            return Ok(());
                        };

                        let mut mpoly = nusamai_geometry::MultiPolygon::new();

                        geometries.iter().for_each(|entry| match entry.ty {
                            GeometryType::Solid
                            | GeometryType::Surface
                            | GeometryType::Triangle => {
                                for idx_poly in geom_store.multipolygon.iter_range(
                                    entry.pos as usize..(entry.pos + entry.len) as usize,
                                ) {
                                    mpoly.push(idx_poly);
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

                        // Prepare attributes
                        let mut n_skipped_attributes = 0;
                        let mut attributes = IndexMap::<String, String>::new();
                        for (attr_name, attr_value) in &obj.attributes {
                            match attr_value {
                                Value::String(s) => {
                                    attributes.insert(attr_name.into(), s.into());
                                }
                                Value::Integer(i) => {
                                    attributes.insert(attr_name.into(), i.to_string());
                                }
                                Value::Double(d) => {
                                    attributes.insert(attr_name.into(), d.to_string());
                                }
                                Value::Boolean(b) => {
                                    // 0 for false and 1 for true in SQLite
                                    attributes.insert(
                                        attr_name.into(),
                                        if *b { "1".into() } else { "0".into() },
                                    );
                                }
                                _ => {
                                    // TODO: implement
                                    n_skipped_attributes += 1;
                                }
                            };
                        }
                        let n_unskipped_attributes = obj.attributes.len() - n_skipped_attributes;
                        if n_unskipped_attributes > 0 {
                            log::info!(
                                "Entity - {:?} unskipped attributes in result",
                                n_unskipped_attributes
                            );
                        }

                        if sender.blocking_send((bytes, attributes)).is_err() {
                            return Err(PipelineError::Canceled);
                        };

                        Ok(())
                    })
            })
        };

        let mut tx = handler.begin().await.unwrap();
        while let Some((gpkg_bin, attributes)) = receiver.recv().await {
            feedback.ensure_not_canceled()?;
            tx.insert_feature(&gpkg_bin, &attributes).await.unwrap();
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
        // use transformer::RequirementItem;

        transformer::Requirements {
            ..Default::default()
        }
    }

    fn run(&mut self, upstream: Receiver, feedback: &Feedback, schema: &Schema) -> Result<()> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(self.run_async(upstream, feedback, schema))
    }
}

/// Check the schema, and prepare attribute column information for the SQLite table
fn schema_to_columns(schema: &Schema) -> IndexMap<String, String> {
    let mut attribute_columns = IndexMap::<String, String>::new();
    schema.types.iter().for_each(|(_name, ty)| match ty {
        TypeDef::Feature(feat_td) => {
            // Note: consider `feat_td.additional_attributes` ?
            feat_td.attributes.iter().for_each(|(attr_name, attr)| {
                // Note: consider  `attr.{min_occurs,max_occurs}` ?
                match &attr.type_ref {
                    TypeRef::String | TypeRef::JsonString => {
                        attribute_columns.insert(attr_name.into(), "TEXT".into());
                    }
                    TypeRef::Integer | TypeRef::NonNegativeInteger => {
                        attribute_columns.insert(attr_name.into(), "INTEGER".into());
                    }
                    TypeRef::Double => {
                        attribute_columns.insert(attr_name.into(), "REAL".into());
                    }
                    TypeRef::Boolean => {
                        attribute_columns.insert(attr_name.into(), "BOOLEAN".into());
                    }
                    _ => {
                        log::warn!(
                            "TypeDef::Feature - Unsupported attribute type: {:?} ('{}')",
                            attr.type_ref,
                            attr_name
                        );
                    }
                }
            });
        }
        TypeDef::Data(data_td) => {
            // TODO: implement
            log::warn!(
                "TypeDef::Data - Not supported yet: {:?}",
                data_td.attributes.values()
            );
        }
        TypeDef::Property(prop_td) => {
            // TODO: implement
            log::warn!(
                "TypeDef::Property - Not supported yet: {} members ({:?}, etc.)",
                prop_td.members.len(),
                prop_td
                    .members
                    .iter()
                    .map(|m| &m.type_ref)
                    .take(3)
                    .collect::<Vec<_>>()
            );
        }
    });

    attribute_columns
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {}
}
