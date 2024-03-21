//! CityGMLファイル (.gml) からポリゴンを読み込んで .glTF 形式で出力するデモ
//! nusamai-geometry/examples/citygml_polygons.rs を元にしています。
//!
//! 使用例:
//!
//! ```bash
//! cargo run --example geometry_to_gltf --release -- ~/path/to/PLATEAU/22203_numazu-shi_2021_citygml_4_op/udx/*/52385628_*_6697_op.gml
//! ```
//!
//! このXMLのパース方法は本格的なパーザで使うことを意図していません。

use std::{
    clone::Clone,
    collections::HashMap,
    default::Default,
    fs,
    io::{BufWriter, Write},
    usize,
};

use byteorder::{LittleEndian, WriteBytesExt};
use clap::Parser;
use earcut_rs::{utils3d::project3d_to_2d, Earcut};
use indexmap::IndexSet;
use nusamai_geometry::MultiPolygon3;
use nusamai_gltf_json::*;
use quick_xml::{
    events::Event,
    name::{Namespace, ResolveResult::Bound},
    reader::NsReader,
};
use thiserror::Error;

const GML_NS: Namespace = Namespace(b"http://www.opengis.net/gml");
const BUILDING_NS: Namespace = Namespace(b"http://www.opengis.net/citygml/building/2.0");
const CITYFURNITURE_NS: Namespace = Namespace(b"http://www.opengis.net/citygml/cityfurniture/2.0");
const TRANSPORTATION_NS: Namespace =
    Namespace(b"http://www.opengis.net/citygml/transportation/2.0");
const BRIDGE_NS: Namespace = Namespace(b"http://www.opengis.net/citygml/bridge/2.0");
const VEGETATION_NS: Namespace = Namespace(b"http://www.opengis.net/citygml/vegetation/2.0");

// 暫定で構造体を定義
#[derive(Debug, Clone, Default)]
struct Triangles {
    pub indices: Vec<u32>,
    pub vertices: IndexSet<[u32; 3]>,
    // pub face_normals: Option<Vec<f32>>,
    // pub vertex_normals: Option<Vec<f32>>,
    // pub vertex_colors: Option<Vec<f32>>,
    // pub vertex_ids: Option<Vec<u64>>,
    // pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl Triangles {
    pub fn new(indices: Vec<u32>, vertices: IndexSet<[u32; 3]>) -> Self {
        Self { indices, vertices }
    }
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("XML error: {0}")]
    XmlError(quick_xml::Error),
}

fn parse_polygon(
    reader: &mut NsReader<&[u8]>,
    mpoly: &mut MultiPolygon3,
    buf: &mut Vec<f64>,
) -> Result<(), ParseError> {
    let mut is_interior = false;
    let mut in_poslist = false;
    loop {
        match reader.read_resolved_event() {
            Ok((Bound(GML_NS), Event::Start(e))) => match e.local_name().as_ref() {
                b"posList" => in_poslist = true,
                b"exterior" => is_interior = false,
                b"interior" => is_interior = true,
                _ => (),
            },
            Ok((Bound(GML_NS), Event::End(e))) => match e.local_name().as_ref() {
                b"Polygon" => return Ok(()),
                b"posList" => in_poslist = false,
                _ => (),
            },
            Ok((_, Event::Text(e))) => {
                if !in_poslist {
                    continue;
                }
                let text = e.unescape().unwrap();
                buf.clear();
                buf.extend(
                    text.split_ascii_whitespace()
                        .map(|v| v.parse::<f64>().unwrap()),
                );
                if is_interior {
                    mpoly.add_interior(buf.chunks_exact(3).map(|c| [c[1], c[0], c[2]]));
                // lon, lat, height
                } else {
                    mpoly.add_exterior(buf.chunks_exact(3).map(|c| [c[1], c[0], c[2]]));
                    // lon, lat, height
                }
            }
            Ok(_) => (),
            Err(e) => return Err(ParseError::XmlError(e)),
        }
    }
}

fn parse_lod_geometry(
    reader: &mut NsReader<&[u8]>,
    mpoly: &mut MultiPolygon3,
    buf: &mut Vec<f64>,
) -> Result<(), ParseError> {
    let mut depth = 0;
    loop {
        match reader.read_resolved_event() {
            Ok((Bound(GML_NS), Event::Start(e))) => match e.local_name().as_ref() {
                b"Polygon" => parse_polygon(reader, mpoly, buf)?,
                _ => depth += 1,
            },
            Ok((_, Event::Start(_))) => depth += 1,
            Ok((_, Event::End(_))) => match depth {
                0 => return Ok(()),
                _ => depth -= 1,
            },
            Ok(_) => (),
            Err(e) => return Err(ParseError::XmlError(e)),
        }
    }
}

fn parse_cityobj(
    reader: &mut NsReader<&[u8]>,
    buf: &mut Vec<f64>,
) -> Result<MultiPolygon3<'static>, ParseError> {
    let mut mpoly = MultiPolygon3::new();
    let mut depth = 0;
    let mut max_lod = 0;
    loop {
        let ev = reader.read_resolved_event();
        match ev {
            Ok((
                Bound(
                    BUILDING_NS | CITYFURNITURE_NS | TRANSPORTATION_NS | VEGETATION_NS | BRIDGE_NS,
                ),
                Event::Start(e),
            )) => match e.local_name().as_ref() {
                b"lod4Geometry" | b"lod4MultiSurface" => {
                    if max_lod < 4 {
                        max_lod = 4;
                        mpoly.clear();
                    }
                    if max_lod == 4 {
                        parse_lod_geometry(reader, &mut mpoly, buf)?;
                    } else {
                        depth += 1;
                    }
                }
                b"lod3Geometry" | b"lod3MultiSurface" => {
                    if max_lod < 3 {
                        max_lod = 3;
                        mpoly.clear();
                    }
                    if max_lod == 3 {
                        parse_lod_geometry(reader, &mut mpoly, buf)?;
                    } else {
                        depth += 1;
                    }
                }
                b"lod2Geometry" | b"lod2MultiSurface" => {
                    if max_lod < 2 {
                        max_lod = 2;
                        mpoly.clear();
                    }
                    if max_lod == 2 {
                        parse_lod_geometry(reader, &mut mpoly, buf)?;
                    } else {
                        depth += 1;
                    }
                }
                b"lod1Solid" | b"lod1MultiSurface" => {
                    if max_lod < 1 {
                        max_lod = 1;
                        mpoly.clear();
                    }
                    if max_lod == 1 {
                        parse_lod_geometry(reader, &mut mpoly, buf)?;
                    } else {
                        depth += 1;
                    }
                }
                _ => depth += 1,
            },
            Ok((_, Event::Start(_))) => depth += 1,
            Ok((_, Event::End(_))) => match depth {
                0 => return Ok(mpoly),
                _ => depth -= 1,
            },
            Ok(_) => (),
            Err(e) => return Err(ParseError::XmlError(e)),
        }
    }
}

fn parse_body(reader: &mut NsReader<&[u8]>) -> Result<Vec<MultiPolygon3<'static>>, ParseError> {
    let mut mpolys: Vec<MultiPolygon3> = Vec::new();
    let mut buf: Vec<f64> = Vec::new();
    loop {
        match reader.read_resolved_event() {
            Ok((_, Event::Eof)) => return Ok(mpolys),
            Ok((
                Bound(
                    BUILDING_NS | CITYFURNITURE_NS | TRANSPORTATION_NS | VEGETATION_NS | BRIDGE_NS,
                ),
                Event::Start(e),
            )) => match e.local_name().as_ref() {
                b"Building"
                | b"CityFurniture"
                | b"Road"
                | b"Bridge"
                | b"SolitaryVegetationObject"
                | b"PlantCover" => mpolys.push(parse_cityobj(reader, &mut buf)?),
                _ => (),
            },
            Ok(_) => (),
            Err(e) => return Err(ParseError::XmlError(e)),
        }
    }
}

fn tessellate(
    mpolys: &[MultiPolygon3],
    mu_lng: f64,
    mu_lat: f64,
) -> Result<Triangles, Box<dyn std::error::Error>> {
    let mut earcutter = Earcut::new();
    let mut buf3d: Vec<f64> = Vec::new();
    let mut buf2d: Vec<f64> = Vec::new();
    let mut triangles_out: Vec<u32> = Vec::new();

    let mut indices: Vec<u32> = Vec::new();
    let mut vertices: IndexSet<[u32; 3]> = IndexSet::new();

    for mpoly in mpolys.iter() {
        for poly in mpoly {
            let num_outer = match poly.hole_indices().first() {
                Some(&v) => v as usize,
                None => poly.raw_coords().len() / 3,
            };

            buf3d.clear();
            buf3d.extend(poly.raw_coords().chunks_exact(3).flat_map(|v| {
                let (lat, lng) = (v[0], v[1]);
                [
                    (lng - mu_lng) * (10000000. * lat.to_radians().cos() / 90.),
                    (lat - mu_lat) * (10000000. / 90.),
                    v[2],
                ]
            }));

            if project3d_to_2d(&buf3d, num_outer, 3, &mut buf2d) {
                // earcut
                earcutter.earcut(&buf2d, poly.hole_indices(), 2, &mut triangles_out);
                // indices and vertices
                indices.extend(triangles_out.iter().map(|idx| {
                    let vbits = [
                        (buf3d[*idx as usize * 3] as f32).to_bits(),
                        (buf3d[*idx as usize * 3 + 1] as f32).to_bits(),
                        (buf3d[*idx as usize * 3 + 2] as f32).to_bits(),
                    ];
                    let (index, _) = vertices.insert_full(vbits);
                    index as u32
                }));
            } else {
                println!("WARN: polygon does not have normal");
            }
        }
    }

    Ok(Triangles::new(indices, vertices))
}

#[derive(Parser)]
struct Args {
    #[clap(required = true)]
    filenames: Vec<String>,
}

fn make_glb(gltf_string: String, binary_buffer: Vec<u8>) -> Vec<u8> {
    // JSONチャンクをバイナリに変換し、4の倍数に調整
    let json_chunk = gltf_string.as_bytes();
    let json_chunk_len = json_chunk.len();
    let json_chunk_padded = {
        let mut v = json_chunk.to_vec();
        while v.len() % 4 != 0 {
            v.push(0); // 4バイト境界に合わせるために0でパディング
        }
        v
    };

    // JSONチャンクヘッダー
    // この長さはパディングを含まない元のJSONデータの長さ
    let json_chunk_header = [
        json_chunk_len as u32, // パディングなしの長さ
        0x4E4F534A,            // JSON (リトルエンディアンで "JSON")
    ];

    let binary_len = binary_buffer.len();

    // バイナリチャンクヘッダー
    let bin_chunk_header = [
        binary_len as u32,
        0x004E4942, // BIN (リトルエンディアンで "BIN")
    ];

    // ファイル全体の長さ
    // この長さはパディングを含む
    let total_length = 12 + 8 + json_chunk_padded.len() + 8 + binary_len;

    // GLBヘッダー
    let glb_header = [
        0x46546C67, // glTF (リトルエンディアンで "glTF")
        2,
        total_length as u32, // ファイル全体の長さ
    ];

    // ファイル作成前にバイナリを作成
    let mut glb = Vec::new();

    // ヘッダーの書き込み
    let _ = glb.write_all(&glb_header[0].to_le_bytes());
    let _ = glb.write_all(&glb_header[1].to_le_bytes());
    let _ = glb.write_all(&glb_header[2].to_le_bytes());

    // JSONチャンクの書き込み
    let _ = glb.write_u32::<LittleEndian>(json_chunk_header[0]);
    let _ = glb.write_u32::<LittleEndian>(json_chunk_header[1]);
    let _ = glb.write_all(&json_chunk_padded);

    // バイナリチャンクの書き込み
    let _ = glb.write_u32::<LittleEndian>(bin_chunk_header[0]);
    let _ = glb.write_u32::<LittleEndian>(bin_chunk_header[1]);
    let _ = glb.write_all(&binary_buffer);

    glb
}

fn make_gltf_json(triangles: &Triangles) -> String {
    let indices = &triangles.indices;
    let vertices = &triangles.vertices;

    // glTF のモデルを作成
    let mut gltf = Gltf::default();
    assert_eq!(gltf.asset.version, "2.0");
    assert_eq!(gltf.asset.generator, Some("nusamai-gltf".into()));

    // glTF のバッファを作成
    let indices_byte_length = indices.len() as u32 * 4;
    let vertices_byte_length = vertices.len() as u32 * 12;
    let buffer = Buffer {
        byte_length: indices_byte_length + vertices_byte_length,
        uri: Some("data.bin".to_string()),
        ..Default::default()
    };
    // indicesはu32なので4バイト、verticesはf32x3なので12バイト

    gltf.buffers = vec![buffer];

    // glTF のバッファビューを作成
    let buffer_view1 = BufferView {
        byte_length: indices_byte_length,
        target: Some(BufferViewTarget::ElementArrayBuffer),
        ..Default::default()
    };

    let buffer_view2 = BufferView {
        byte_length: vertices_byte_length,
        byte_offset: indices_byte_length,
        target: Some(BufferViewTarget::ArrayBuffer),
        ..Default::default()
    };

    gltf.buffer_views = vec![buffer_view1, buffer_view2];

    // glTF のアクセサを作成
    let accessor1 = Accessor {
        buffer_view: Some(0),
        byte_offset: 0,
        component_type: ComponentType::UnsignedInt,
        count: indices.len() as u32,
        type_: AccessorType::Scalar,
        ..Default::default()
    };

    let mut accessor2 = Accessor {
        buffer_view: Some(1),
        byte_offset: 0,
        component_type: ComponentType::Float,
        count: vertices.len() as u32,
        type_: AccessorType::Vec3,
        ..Default::default()
    };
    let mut max_vertex: [f64; 3] = [f64::MIN; 3];
    let mut min_vertex: [f64; 3] = [f64::MAX; 3];
    for vertex in vertices {
        for (i, v) in vertex.iter().enumerate() {
            let v = f32::from_bits(*v);
            max_vertex[i] = max_vertex[i].max(v as f64);
            min_vertex[i] = min_vertex[i].min(v as f64);
        }
    }
    accessor2.max = Some(max_vertex.to_vec());
    accessor2.min = Some(min_vertex.to_vec());

    gltf.accessors = vec![accessor1, accessor2];

    // glTF のメッシュを作成
    let primitive1 = MeshPrimitive {
        indices: Some(0),
        mode: PrimitiveMode::Triangles,
        attributes: {
            let mut map = HashMap::new();
            map.insert("POSITION".to_string(), 1);
            map
        },
        ..Default::default()
    };

    let mesh = Mesh {
        primitives: vec![primitive1],
        ..Default::default()
    };

    gltf.meshes = vec![mesh];

    // glTF のシーンを作成
    let scene = Scene {
        nodes: Some(vec![0]),
        ..Default::default()
    };

    gltf.scenes = vec![scene];

    // glTF のノードを作成
    let node = Node {
        mesh: Some(0),
        ..Default::default()
    };

    gltf.nodes = vec![node];

    // glTF のシーンを設定
    gltf.scene = Some(0);

    gltf.to_string().unwrap()
}

fn calc_center(all_mpolys: &Vec<nusamai_geometry::MultiPolygon<3>>) -> (f64, f64) {
    // 中心の経緯度を求める
    let (mu_lat, mu_lng) = {
        let (mut mu_lat, mut mu_lng) = (0.0, 0.0);
        let mut num_features = 0;
        for mpoly in all_mpolys {
            let (mut feat_mu_lng, mut feat_mu_lat) = (0.0, 0.0);
            let mut num_verts = 0;
            for poly in mpoly {
                for v in poly.raw_coords().chunks_exact(3) {
                    num_verts += 1;
                    feat_mu_lng += v[0];
                    feat_mu_lat += v[1];
                }
            }
            if num_verts > 0 {
                num_features += 1;
                mu_lat += feat_mu_lng / num_verts as f64;
                mu_lng += feat_mu_lat / num_verts as f64;
            }
        }
        (mu_lat / num_features as f64, mu_lng / num_features as f64)
    };
    println!("{} {}", mu_lat, mu_lng);
    (mu_lat, mu_lng)
}

fn make_binary_buffer(triangles: &Triangles) -> Vec<u8> {
    let indices = &triangles.indices;
    let vertices = &triangles.vertices;

    let mut indices_buf = Vec::new();
    let mut vertices_buf = Vec::new();

    // glTFのバイナリはリトルエンディアン
    for index in indices {
        indices_buf.write_u32::<LittleEndian>(*index).unwrap();
    }

    for vertex in vertices {
        for v in vertex {
            vertices_buf
                .write_f32::<LittleEndian>(f32::from_bits(*v))
                .unwrap();
        }
    }

    [&indices_buf[..], &vertices_buf[..]].concat()
}
fn main() {
    let args = Args::parse();

    let mut all_mpolys = Vec::new();

    for filename in args.filenames {
        let xml = fs::read_to_string(filename).unwrap();
        let mut reader = NsReader::from_str(&xml);
        reader.trim_text(true);
        match parse_body(&mut reader) {
            Ok(mpolys) => {
                println!(
                    "features={features} polygons={polygons}",
                    features = mpolys.len(),
                    polygons = mpolys.iter().flatten().count()
                );
                all_mpolys.extend(mpolys);
            }
            Err(e) => match e {
                ParseError::XmlError(e) => {
                    println!("Error at position {}: {:?}", reader.buffer_position(), e)
                }
            },
        };
    }

    // NOTE: この時点で MultiPolygon にジオメトリデータが詰め込まれている状態
    //
    // ここから先は glb 形式での出力を行う。

    // 中心の経緯度を求める
    let (mu_lat, mu_lng) = calc_center(&all_mpolys);

    // 三角分割
    let triangles = tessellate(&all_mpolys, mu_lng, mu_lat).unwrap();

    // バイナリバッファを作成
    let binary_buffer = make_binary_buffer(&triangles);
    std::fs::create_dir("./data/").unwrap();
    fs::write("./data/data.bin", &binary_buffer).unwrap();

    // glTFのJSON文字列を作成
    let gltf_string = make_gltf_json(&triangles);
    fs::write("./data/data.gltf", &gltf_string).unwrap();

    // glbを作成
    let glb = make_glb(gltf_string, binary_buffer);

    // ファイルを作成
    let mut file = BufWriter::new(fs::File::create("./data/data.glb").unwrap());

    // ファイルの書き込み
    let _ = file.write_all(glb.as_slice());
    let _ = file.flush();
}
