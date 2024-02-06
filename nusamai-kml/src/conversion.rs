use kml::types::{Coord, Geometry, LinearRing, MultiGeometry, Point, Polygon as KmlPolygon};
use nusamai_geometry::{CoordNum, MultiPoint, Polygon};
use std::{collections::HashMap, vec};

fn polygon_to_kml_outer_boundary_with_mapping<const D: usize, T: CoordNum>(
    poly: Polygon<D, T>,
    mapping: impl Fn([T; D]) -> [f64; 3],
) -> LinearRing {
    let outer_coords: Vec<Coord> = poly
        .exterior()
        .iter_closed()
        .map(&mapping)
        .map(|coords| Coord {
            x: coords[0],
            y: coords[1],
            z: Some(coords[2]),
        })
        .collect();

    LinearRing {
        coords: outer_coords,
        extrude: false,
        tessellate: false,
        altitude_mode: Default::default(),
        attrs: HashMap::new(),
    }
}

fn polygon_to_kml_inner_boundary_with_mapping<const D: usize, T: CoordNum>(
    poly: Polygon<D, T>,
    mapping: impl Fn([T; D]) -> [f64; 3],
) -> Vec<LinearRing> {
    poly.interiors()
        .map(|ring| {
            ring.iter_closed()
                .map(&mapping)
                .map(|coords| Coord {
                    x: coords[0],
                    y: coords[1],
                    z: Some(coords[2]),
                })
                .collect::<Vec<_>>()
        })
        .map(|coords| LinearRing {
            coords,
            extrude: false,
            tessellate: false,
            altitude_mode: Default::default(),
            attrs: HashMap::new(),
        })
        .collect()
}

fn polygon_to_kml_polygon_with_mapping<const D: usize, T: CoordNum>(
    poly: Polygon<D, T>,
    mapping: impl Fn([T; D]) -> [f64; 3],
) -> KmlPolygon {
    let outer = polygon_to_kml_outer_boundary_with_mapping(poly.clone(), &mapping);
    let inner = polygon_to_kml_inner_boundary_with_mapping(poly, &mapping);

    KmlPolygon {
        outer,
        inner,
        extrude: false,
        tessellate: false,
        altitude_mode: Default::default(),
        attrs: HashMap::new(),
    }
}

/// Create a kml::MultiGeometry with Polygon from `nusamai_geometry::MultiPoint` with a mapping function.
pub fn polygon_to_kml_with_mapping<const D: usize, T: CoordNum>(
    poly: Polygon<D, T>,
    mapping: impl Fn([T; D]) -> [f64; 3],
) -> MultiGeometry {
    let polygons = vec![polygon_to_kml_polygon_with_mapping(poly, mapping)];
    MultiGeometry {
        geometries: polygons
            .into_iter()
            .map(|poly: KmlPolygon| Geometry::Polygon(poly))
            .collect(),
        attrs: HashMap::new(),
    }
}

/// Create a kml::MultiGeometry from a nusamai_geometry::MultiPolygon
pub fn polygon_to_kml(poly: &Polygon<3>) -> MultiGeometry {
    polygon_to_kml_with_mapping(poly.clone(), |c| c)
}

/// Create a kml::MultiGeometry with Points from `nusamai_geometry::MultiPoint` with a mapping function.
pub fn multipoint_to_kml_with_mapping<const D: usize, T: CoordNum>(
    mpoint: &MultiPoint<D, T>,
    mapping: impl Fn([T; D]) -> [f64; 3],
) -> MultiGeometry {
    let points = mpoint
        .iter()
        .map(&mapping)
        .map(|coords| Point::new(coords[0], coords[1], Some(coords[2])))
        .collect::<Vec<_>>();
    MultiGeometry {
        geometries: points
            .into_iter()
            .map(|pt: Point| Geometry::Point(pt))
            .collect(),
        attrs: HashMap::new(),
    }
}

/// Create a kml::MultiGeometry with Points vertices and indices.
pub fn indexed_multipoint_to_kml(
    vertices: &[[f64; 3]],
    mpoint_idx: &MultiPoint<1, u32>,
) -> MultiGeometry {
    multipoint_to_kml_with_mapping(mpoint_idx, |idx| vertices[idx[0] as usize])
}

/// Create a kml::MultiGeometry from a nusamai_geometry::MultiPoint
pub fn multipoint_to_kml(mpoint: &MultiPoint<3>) -> MultiGeometry {
    multipoint_to_kml_with_mapping(mpoint, |c| c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kml::types::{Geometry, Point};
    use nusamai_geometry::{MultiPoint, Polygon3};

    #[test]
    fn test_multipoint_to_kml() {
        let mut mpoint = MultiPoint::<3>::new();
        mpoint.push(&[11., 12., 13.]);
        mpoint.push(&[21., 22., 23.]);
        mpoint.push(&[31., 32., 33.]);

        let multi_geom = multipoint_to_kml(&mpoint);

        assert_eq!(&multi_geom.geometries.len(), &3);

        assert_eq!(
            &multi_geom.geometries,
            &vec![
                Geometry::Point(Point::new(11., 12., Some(13.))),
                Geometry::Point(Point::new(21., 22., Some(23.))),
                Geometry::Point(Point::new(31., 32., Some(33.)))
            ]
        );
    }

    #[test]
    fn test_indexed_multipoint_to_kml() {
        let vertices = vec![[11., 12., 13.], [21., 22., 23.], [31., 32., 33.]];
        let mut mpoint_idx = MultiPoint::<1, u32>::new();
        mpoint_idx.push(&[0]);
        mpoint_idx.push(&[1]);
        mpoint_idx.push(&[2]);

        let multi_geom = indexed_multipoint_to_kml(&vertices, &mpoint_idx);

        assert_eq!(&multi_geom.geometries.len(), &3);

        assert_eq!(
            &multi_geom.geometries,
            &vec![
                Geometry::Point(Point::new(11., 12., Some(13.))),
                Geometry::Point(Point::new(21., 22., Some(23.))),
                Geometry::Point(Point::new(31., 32., Some(33.)))
            ]
        );
    }

    #[test]
    fn test_polygon_to_kml() {
        let mut poly = Polygon3::new();
        poly.add_ring([
            [10., 10., 0.],
            [10., 20., 0.],
            [20., 20., 0.],
            [20., 10., 0.], // not closed
        ]);
        poly.add_ring([
            [15., 15., 0.],
            [18., 10., 0.],
            [18., 18., 0.],
            [15., 18., 0.],
        ]);

        let multi_geom = polygon_to_kml(&poly);

        assert_eq!(&multi_geom.geometries.len(), &1);

        assert_eq!(
            &multi_geom.geometries[0],
            &Geometry::Polygon(KmlPolygon {
                outer: LinearRing {
                    coords: vec![
                        Coord {
                            x: 10.,
                            y: 10.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 10.,
                            y: 20.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 20.,
                            y: 20.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 20.,
                            y: 10.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 10.0,
                            y: 10.0,
                            z: Some(0.0)
                        }
                    ],
                    extrude: false,
                    tessellate: false,
                    altitude_mode: Default::default(),
                    attrs: HashMap::new(),
                },
                inner: vec![LinearRing {
                    coords: vec![
                        Coord {
                            x: 15.,
                            y: 15.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 18.,
                            y: 10.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 18.,
                            y: 18.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 15.,
                            y: 18.,
                            z: Some(0.),
                        },
                        Coord {
                            x: 15.0,
                            y: 15.0,
                            z: Some(0.0)
                        }
                    ],
                    extrude: false,
                    tessellate: false,
                    altitude_mode: Default::default(),
                    attrs: HashMap::new(),
                }],
                extrude: false,
                tessellate: false,
                altitude_mode: Default::default(),
                attrs: HashMap::new(),
            })
        );
    }
}
