//! GeoPackage sink

use std::path::PathBuf;
use url::Url;

use rayon::prelude::*;

use crate::parameters::Parameters;
use crate::parameters::*;
use crate::pipeline::{Feedback, PipelineError, Receiver, Result};
use crate::sink::{DataSink, DataSinkProvider, SinkInfo};
use crate::{get_parameter_value, transformer};

use nusamai_citygml::object::{ObjectStereotype, Value};
use nusamai_citygml::schema::Schema;
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
    pub async fn run_async(&mut self, upstream: Receiver, feedback: &Feedback) -> Result<()> {
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

                        if sender.blocking_send(bytes).is_err() {
                            return Err(PipelineError::Canceled);
                        };

                        Ok(())
                    })
            })
        };

        let mut tx = handler.begin().await.unwrap();
        while let Some(gpkg_bin) = receiver.recv().await {
            feedback.ensure_not_canceled()?;
            tx.insert_feature(&gpkg_bin).await;
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

    fn run(&mut self, upstream: Receiver, feedback: &Feedback, _schema: &Schema) -> Result<()> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(self.run_async(upstream, feedback))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {}
}
