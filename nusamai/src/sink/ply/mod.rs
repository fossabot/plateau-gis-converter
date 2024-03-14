//! Stanford PLY sink

use std::io::Write;
use std::path::PathBuf;

use ahash::RandomState;
use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use earcut_rs::{utils3d::project3d_to_2d, Earcut};
use indexmap::IndexSet;
use nusamai_citygml::{schema::Schema, GeometryType};

use nusamai_projection::cartesian::geographic_to_geocentric;
use rayon::prelude::*;

use crate::get_parameter_value;
use crate::parameters::*;
use crate::pipeline::{Feedback, PipelineError, Receiver};
use crate::sink::{DataRequirements, DataSink, DataSinkProvider, SinkInfo};

use nusamai_citygml::object::{ObjectStereotype, Value};

const PLY_HEADER_TEMPLATE: &str = r##"ply
format binary_little_endian 1.0
element vertex {n_verts}
property double x
property double y
property double z
element face {n_faces}
property list uchar uint vertex_indices
end_header
"##;

// comment crs: GEODCRS["WGS 84",ENSEMBLE["World Geodetic System 1984 ensemble",MEMBER["World Geodetic System 1984 (Transit)"],MEMBER["World Geodetic System 1984 (G730)"],MEMBER["World Geodetic System 1984 (G873)"],MEMBER["World Geodetic System 1984 (G1150)"],MEMBER["World Geodetic System 1984 (G1674)"],MEMBER["World Geodetic System 1984 (G1762)"],ELLIPSOID["WGS 84",6378137,298.257223563,LENGTHUNIT["metre",1]],ENSEMBLEACCURACY[2.0]],PRIMEM["Greenwich",0,ANGLEUNIT["degree",0.0174532925199433]],CS[Cartesian,3],AXIS["(X)",geocentricX,ORDER[1],LENGTHUNIT["metre",1]],AXIS["(Y)",geocentricY,ORDER[2],LENGTHUNIT["metre",1]],AXIS["(Z)",geocentricZ,ORDER[3],LENGTHUNIT["metre",1]],USAGE[SCOPE["Geodesy. Navigation and positioning using GPS satellite system."],AREA["World."],BBOX[-90,-180,90,180]],ID["EPSG",4978]]

pub struct StanfordPlySinkProvider {}

impl DataSinkProvider for StanfordPlySinkProvider {
    fn info(&self) -> SinkInfo {
        SinkInfo {
            id_name: "ply".to_string(),
            name: "Stanford PLY".to_string(),
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

        Box::<StanfordPlySink>::new(StanfordPlySink {
            output_path: output_path.as_ref().unwrap().into(),
        })
    }
}

pub struct StanfordPlySink {
    output_path: PathBuf,
}

impl DataSink for StanfordPlySink {
    fn make_requirements(&self) -> DataRequirements {
        DataRequirements {
            ..Default::default()
        }
    }

    fn run(
        &mut self,
        upstream: Receiver,
        feedback: &Feedback,
        _schema: &Schema,
    ) -> Result<(), PipelineError> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1000);

        let (ra, rb) = rayon::join(
            || {
                let ellipsoid = nusamai_projection::ellipsoid::wgs84();

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
                        let ObjectStereotype::Feature { geometries, .. } = &obj.stereotype else {
                            return Ok(());
                        };

                        let mut earcutter = Earcut::new();
                        let mut buf3d: Vec<f64> = Vec::new();
                        let mut buf2d: Vec<f64> = Vec::new();
                        let mut triangles_buf: Vec<u32> = Vec::new();
                        let mut triangles = Vec::new();

                        geometries.iter().for_each(|entry| match entry.ty {
                            GeometryType::Solid
                            | GeometryType::Surface
                            | GeometryType::Triangle => {
                                for idx_poly in geom_store.multipolygon.iter_range(
                                    entry.pos as usize..(entry.pos + entry.len) as usize,
                                ) {
                                    let poly = idx_poly.transform(|idx| {
                                        let [lng, lat, height] =
                                            geom_store.vertices[idx[0] as usize];
                                        // Convert to geocentric (x, y, z) coordinate.
                                        // (Earcut do not work in geographic space)
                                        let (x, y, z) =
                                            geographic_to_geocentric(&ellipsoid, lng, lat, height);
                                        [x, y, z]
                                    });
                                    let num_outer = match poly.hole_indices().first() {
                                        Some(&v) => v as usize,
                                        None => poly.coords().len() / 3,
                                    };

                                    buf3d.clear();
                                    buf3d.extend(poly.coords());

                                    if project3d_to_2d(&buf3d, num_outer, 3, &mut buf2d) {
                                        // earcut
                                        earcutter.earcut(
                                            &buf2d,
                                            poly.hole_indices(),
                                            2,
                                            &mut triangles_buf,
                                        );
                                        triangles.extend(triangles_buf.iter().map(|idx| {
                                            [
                                                buf3d[*idx as usize * 3],
                                                buf3d[*idx as usize * 3 + 1],
                                                buf3d[*idx as usize * 3 + 2],
                                            ]
                                        }));
                                    }
                                }
                            }
                            GeometryType::Curve | GeometryType::Point => {
                                // not supported in PLY sink
                            }
                        });

                        if sender.send(triangles).is_err() {
                            return Err(PipelineError::Canceled);
                        };

                        Ok(())
                    })
            },
            || {
                // calculate the centroid
                let mut mu_x = 0.;
                let mut mu_y = 0.;
                let mut mu_z = 0.;
                let mut all_vertices = Vec::new();
                for (i, triangles) in receiver.into_iter().enumerate() {
                    if i % 10000 == 0 {
                        feedback.ensure_not_canceled()?;
                    }

                    for [x, y, z] in triangles {
                        mu_x += x;
                        mu_y += y;
                        mu_z += z;
                        all_vertices.push([x, y, z]);
                    }
                }
                mu_x /= all_vertices.len() as f64;
                mu_y /= all_vertices.len() as f64;
                mu_z /= all_vertices.len() as f64;

                // make vertices and indices
                let mut vertices: IndexSet<[u64; 3], RandomState> = IndexSet::default();
                let indices: Vec<_> = all_vertices
                    .iter()
                    .map(|[x, y, z]| {
                        let vbits = [
                            (x - mu_x).to_bits(),
                            (y - mu_y).to_bits(),
                            (z - mu_z).to_bits(),
                        ];
                        let (index, _) = vertices.insert_full(vbits);
                        index as u32
                    })
                    .collect();

                feedback.ensure_not_canceled()?;

                // write to file
                println!("{:?} {:?}", vertices.len(), indices.len());
                let file = std::fs::File::create(&self.output_path)?;
                let mut writer = std::io::BufWriter::new(file);
                writer.write_all(
                    PLY_HEADER_TEMPLATE
                        .replace("{n_verts}", &vertices.len().to_string())
                        .replace("{n_faces}", &(indices.len() / 3).to_string())
                        .as_ref(),
                )?;

                let mut buf = [0; 24];
                for v in &vertices {
                    LittleEndian::write_u64_into(v, &mut buf);
                    writer.write_all(&buf)?;
                }
                let mut buf = [0; 12];
                for idx in indices.chunks_exact(3) {
                    writer.write_u8(3)?;
                    LittleEndian::write_u32_into(idx, &mut buf);
                    writer.write_all(&buf)?;
                }

                writer.flush()?;

                Ok(())
            },
        );

        match ra {
            Ok(_) | Err(PipelineError::Canceled) => {}
            Err(error) => feedback.fatal_error(error),
        }
        match rb {
            Ok(_) | Err(PipelineError::Canceled) => {}
            Err(error) => feedback.fatal_error(error),
        }
        Ok(())
    }
}
