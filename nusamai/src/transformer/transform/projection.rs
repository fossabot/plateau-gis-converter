use std::sync::Arc;

use crate::transformer::Transform;

use nusamai_citygml::schema::Schema;
use nusamai_plateau::Entity;
use nusamai_projection::crs::*;
use nusamai_projection::etmerc::ExtendedTransverseMercatorProjection;
use nusamai_projection::jprect::JPRZone;
use nusamai_projection::vshift::Jgd2011ToWgs84;

/// Coordinate transformation
#[derive(Clone)]
pub struct ProjectionTransform {
    jgd2wgs: Arc<Jgd2011ToWgs84>,
    output_epsg: EpsgCode,
    jpr_zone_proj: Option<ExtendedTransverseMercatorProjection>,
}

impl Transform for ProjectionTransform {
    fn transform(&mut self, entity: Entity, out: &mut Vec<Entity>) {
        let input_epsg = {
            let geom_store = entity.geometry_store.read().unwrap();
            geom_store.epsg
        };

        match input_epsg {
            EPSG_JGD2011_GEOGRAPHIC_3D => match self.output_epsg {
                EPSG_WGS84_GEOGRAPHIC_3D => {
                    let mut geom_store = entity.geometry_store.write().unwrap();
                    geom_store.vertices.iter_mut().for_each(|v| {
                        // Swap x and y (lat, lng -> lng, lat)
                        let (lng, lat, height) = (v[1], v[0], v[2]);
                        // JGD2011 to WGS 84 (elevation to ellipsoidal height)
                        (v[0], v[1], v[2]) = self.jgd2wgs.convert(lng, lat, height);
                    });
                    geom_store.epsg = self.output_epsg;
                }
                EPSG_JGD2011_JPRECT_I_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_II_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_III_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_IV_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_V_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_VI_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_VII_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_VIII_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_IX_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_X_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_XI_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_XII_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_XIII_JGD2011_HEIGHT => {
                    // To Japan Plane Rectangular CS
                    let proj = self.jpr_zone_proj.as_ref().unwrap();
                    let mut geom_store = entity.geometry_store.write().unwrap();
                    geom_store.vertices.iter_mut().for_each(|v| {
                        let (lng, lat) = (v[1], v[0]);
                        // Change x and y; keep the height
                        // TODO: error handling
                        (v[0], v[1], _) = proj.project_forward(lng, lat, 0.).unwrap();
                    });
                    geom_store.epsg = self.output_epsg;
                }
                _ => {
                    panic!("Unsupported output CRS: {}", self.output_epsg);
                }
            },
            EPSG_WGS84_GEOGRAPHIC_3D => match self.output_epsg {
                EPSG_WGS84_GEOGRAPHIC_3D => {
                    // Do nothing
                }
                EPSG_JGD2011_JPRECT_I_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_II_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_III_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_IV_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_V_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_VI_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_VII_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_VIII_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_IX_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_X_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_XI_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_XII_JGD2011_HEIGHT
                | EPSG_JGD2011_JPRECT_XIII_JGD2011_HEIGHT => {
                    // TODO: implement
                    unimplemented!("WGS84 to EPSG:{} not supported yet", self.output_epsg);
                }
                _ => {
                    panic!("Unsupported output CRS: {}", self.output_epsg);
                }
            },
            _ => {
                panic!("Unsupported input CRS: {}", input_epsg);
            }
        }

        out.push(entity);
    }

    fn transform_schema(&self, schema: &mut Schema) {
        schema.epsg = Some(self.output_epsg);
    }
}

impl ProjectionTransform {
    pub fn new(jgd2wgs: Arc<Jgd2011ToWgs84>, output_epsg: EpsgCode) -> Self {
        // For Japan Plane Rectangular CS
        let jpr_zone_proj = JPRZone::from_epsg(output_epsg).map(|zone| zone.projection());

        Self {
            jgd2wgs,
            output_epsg,
            jpr_zone_proj,
        }
    }
}
