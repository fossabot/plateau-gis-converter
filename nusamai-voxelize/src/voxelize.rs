use byteorder::{LittleEndian, WriteBytesExt};
use nalgebra::{distance, Point3, Vector3};
use nusamai_geometry::MultiPolygon;
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Write,
};

#[derive(Eq, PartialEq, Hash, Debug)]
struct Voxel {
    x: i32,
    y: i32,
    z: i32,
}

fn draw_line(voxels: &mut HashSet<Voxel>, start: [f64; 3], end: [f64; 3], voxel_size: f64) {
    // 始点と終点が既知なので方向ベクトルが算出できる
    let direction = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    // 方向ベクトルのXYZ方向の最大移動距離を取得
    // 移動距離なので、絶対値
    let max_dist = direction
        .iter()
        .fold(0.0_f64, |acc, &val| acc.max(val.abs()));
    // 距離をボクセルサイズで割り、切り上げることでステップ数を算出
    // エッジを何ステップに分割するかを計算
    let steps = (max_dist / voxel_size).ceil() as i32;
    // XYZ方向へ1ステップで進む距離を算出
    let step_size = [
        direction[0] / steps as f64,
        direction[1] / steps as f64,
        direction[2] / steps as f64,
    ];

    let mut current = start;
    // ステップの数だけ繰り返し、各ステップで通過するボクセルを計算
    for _ in 0..=steps {
        // ボクセルの座標計算
        // 現在の座標をボクセルのサイズで割り、切り捨てることでボクセルの格子座標（整数値）を算出
        let voxel = Voxel {
            x: (current[0] / voxel_size).floor() as i32,
            y: (current[1] / voxel_size).floor() as i32,
            z: (current[2] / voxel_size).floor() as i32,
        };
        voxels.insert(voxel);
        // 現在の座標を更新
        current[0] += step_size[0];
        current[1] += step_size[1];
        current[2] += step_size[2];
    }
}

fn triangle_to_voxel(triangles: &[[f64; 3]], voxel_size: f64) -> HashSet<Voxel> {
    // 占有されたボクセルを格納する
    // HashSetは重複を許さない
    let mut occupied_voxels = HashSet::new();

    // todo: 以下の実装だとエッジしか操作できないので、三角形を走査して行くように実装を変更する必要がある
    // for tri in triangles.windows(3) {
    //     // indicesの要素を2つずつ取り出すのがwindows(2)メソッド
    //     for window in tri.windows(2) {
    //         // 隣り合った2つの頂点を取り出し、これを線分（エッジ）の始点と終点とする
    //         let start = window[0];
    //         let end = window[1];
    //         // すべてのエッジを走査して、occupied_voxelsに格納していく)
    //         draw_line(&mut occupied_voxels, start, end, voxel_size);
    //     }
    // }

    // todo: 三角形を走査していく関数を実装
    // todo: 色は後で考える
    // indicesの要素を3つずつ取り出して三角形を構築
    for t in triangles.windows(3) {
        fill_triangle(&mut occupied_voxels, voxel_size, t);
    }

    occupied_voxels
}

fn fill_triangle(voxels: &mut HashSet<Voxel>, voxel_size: f64, triangle: &[[f64; 3]]) {
    if triangle.len() != 3 {
        panic!("The number of vertices is not 3")
    }

    // 全ての三角形は反時計回りを表面とする
    let p1 = Point3::from(triangle[0]);
    let p2 = Point3::from(triangle[1]);
    let p3 = Point3::from(triangle[2]);
    println!("p1={:?}, p2={:?}, p3={:?}", p1, p2, p3);

    // 3辺の長さを算出し、三角形が小さい（すべての辺がvoxel_size未満）場合は、面を走査せずvoxelを一つだけ塗りつぶす
    if is_small_triangle(&p1, &p2, &p3, voxel_size) {
        println!("Triangles too small!");

        let p1_floor = p1.map(|x| x.floor());
        let p2_floor = p2.map(|y| y.floor());
        let p3_floor = p3.map(|z| z.floor());

        voxels.insert(Voxel {
            x: p1_floor.x as i32,
            y: p1_floor.y as i32,
            z: p1_floor.z as i32,
        });
        voxels.insert(Voxel {
            x: p2_floor.x as i32,
            y: p2_floor.y as i32,
            z: p2_floor.z as i32,
        });
        voxels.insert(Voxel {
            x: p3_floor.x as i32,
            y: p3_floor.y as i32,
            z: p3_floor.z as i32,
        });
    }

    // p1からp2に伸びるベクトルと、p1からp3に伸びるベクトルを考える
    let v1 = p2 - p1;
    let v2 = p3 - p1;

    // 法線ベクトルを計算
    let mut norm = v1.cross(&v2);
    let d = norm.norm();

    if d.is_nan() || d == 0.0 {
        return;
    }

    // 正規化し、法線ベクトルを単位ベクトルに変換
    norm /= d;

    // 最大長の軸を求める
    // norm_axis=0 (x) --> yz-plane
    // norm_axis=1 (y) --> zx-plane
    // norm_axis=2 (z) --> xy-plane
    let norm_axis = norm
        .abs()
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    println!("norm_axis: {}", norm_axis);
}

fn is_small_triangle(p1: &Point3<f64>, p2: &Point3<f64>, p3: &Point3<f64>, size: f64) -> bool {
    let d12 = distance(p1, p2);
    let d23 = distance(p2, p3);
    let d31 = distance(p3, p1);

    d12 <= size && d23 <= size && d31 <= size
}

#[cfg(test)]
mod tests {
    use earcut::{utils3d::project3d_to_2d, Earcut};

    use super::*;

    #[test]
    fn test_voxelize() {
        let vertices: Vec<[f64; 3]> = vec![
            // Large box Exterior
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [10.0, 10.0, 10.0],
            [0.0, 10.0, 10.0],
            // Large box Interior
            [1.0, 1.0, 1.0],
            [9.0, 1.0, 1.0],
            [9.0, 9.0, 1.0],
            [1.0, 9.0, 1.0],
            [1.0, 1.0, 9.0],
            [9.0, 1.0, 9.0],
            [9.0, 9.0, 9.0],
            [1.0, 9.0, 9.0],
            // Medium box Exterior
            [15.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
            [20.0, 5.0, 0.0],
            [15.0, 5.0, 0.0],
            [15.0, 0.0, 5.0],
            [20.0, 0.0, 5.0],
            [20.0, 5.0, 5.0],
            [15.0, 5.0, 5.0],
            // Medium box Interior
            [16.0, 1.0, 1.0],
            [19.0, 1.0, 1.0],
            [19.0, 4.0, 1.0],
            [16.0, 4.0, 1.0],
            [16.0, 1.0, 4.0],
            [19.0, 1.0, 4.0],
            [19.0, 4.0, 4.0],
            [16.0, 4.0, 4.0],
            // Small box Exterior
            [25.0, 0.0, 0.0],
            [28.0, 0.0, 0.0],
            [28.0, 3.0, 0.0],
            [25.0, 3.0, 0.0],
            [25.0, 0.0, 3.0],
            [28.0, 0.0, 3.0],
            [28.0, 3.0, 3.0],
            [25.0, 3.0, 3.0],
        ];

        let mut mpoly = MultiPolygon::<u32>::new();

        // Large box
        mpoly.add_exterior([0, 1, 2, 3, 0]);
        mpoly.add_exterior([4, 5, 6, 7, 4]);
        mpoly.add_exterior([0, 1, 5, 4, 0]);
        mpoly.add_exterior([1, 2, 6, 5, 1]);
        mpoly.add_exterior([2, 3, 7, 6, 2]);
        mpoly.add_exterior([3, 0, 4, 7, 3]);
        mpoly.add_interior([8, 9, 10, 11, 8]);
        mpoly.add_interior([12, 13, 14, 15, 12]);
        mpoly.add_interior([8, 9, 13, 12, 8]);
        mpoly.add_interior([9, 10, 14, 13, 9]);
        mpoly.add_interior([10, 11, 15, 14, 10]);
        mpoly.add_interior([11, 8, 12, 15, 11]);

        // Medium box
        mpoly.add_exterior([16, 17, 18, 19, 16]);
        mpoly.add_exterior([20, 21, 22, 23, 20]);
        mpoly.add_exterior([16, 17, 21, 20, 16]);
        mpoly.add_exterior([17, 18, 22, 21, 17]);
        mpoly.add_exterior([18, 19, 23, 22, 18]);
        mpoly.add_exterior([19, 16, 20, 23, 19]);
        mpoly.add_interior([24, 25, 26, 27, 24]);
        mpoly.add_interior([28, 29, 30, 31, 28]);
        mpoly.add_interior([24, 25, 29, 28, 24]);
        mpoly.add_interior([25, 26, 30, 29, 25]);
        mpoly.add_interior([26, 27, 31, 30, 26]);
        mpoly.add_interior([27, 24, 28, 31, 27]);

        // Small box
        mpoly.add_exterior([32, 33, 34, 35, 32]);
        mpoly.add_exterior([36, 37, 38, 39, 36]);
        mpoly.add_exterior([32, 33, 37, 36, 32]);
        mpoly.add_exterior([33, 34, 38, 37, 33]);
        mpoly.add_exterior([34, 35, 39, 38, 34]);
        mpoly.add_exterior([35, 32, 36, 39, 35]);

        // triangulation
        let mut earcutter = Earcut::new();
        let mut buf3d: Vec<[f64; 3]> = Vec::new();
        let mut buf2d: Vec<[f64; 2]> = Vec::new();
        let mut index_buf: Vec<u32> = Vec::new();

        let mut triangles: Vec<[f64; 3]> = Vec::new();

        // ポリゴンを取り出す
        for idx_poly in mpoly.iter() {
            // インデックスを座標に変換
            let poly = idx_poly.transform(|idx| vertices[*idx as usize]);
            // holeがあるか確認
            let num_outer = match poly.hole_indices().first() {
                Some(&v) => v as usize,
                None => poly.raw_coords().len(),
            };

            // 3次元での座標を格納
            buf3d.clear();
            buf3d.extend(poly.raw_coords().iter());

            // 3次元座標を2次元座標に変換
            if project3d_to_2d(&buf3d, num_outer, &mut buf2d) {
                // earcut
                earcutter.earcut(buf2d.iter().cloned(), poly.hole_indices(), &mut index_buf);
                triangles.extend(index_buf.iter().map(|&idx| buf3d[idx as usize]));
            }
        }

        let voxel_size = 1.0;

        let occupied_voxels = triangle_to_voxel(&triangles, voxel_size);
        let points_count = occupied_voxels.len();

        // -------------------gltfの作成-------------------

        // voxelは整数値だが、accessorsのcomponentTypeは5126（浮動小数点数）であり、primitivesの制約でINTEGER型は使用できない
        let mut min_point = [f32::MAX; 3];
        let mut max_point = [f32::MIN; 3];

        let mut bin_file = File::create("data/output.bin").unwrap();
        for v in occupied_voxels.iter() {
            let [x, y, z] = [
                (v.x as f32) * voxel_size as f32,
                (v.y as f32) * voxel_size as f32,
                (v.z as f32) * voxel_size as f32,
            ];
            min_point = [
                f32::min(min_point[0], x),
                f32::min(min_point[1], y),
                f32::min(min_point[2], z),
            ];
            max_point = [
                f32::max(max_point[0], x),
                f32::max(max_point[1], y),
                f32::max(max_point[2], z),
            ];

            bin_file.write_f32::<LittleEndian>(x).unwrap();
            bin_file.write_f32::<LittleEndian>(y).unwrap();
            bin_file.write_f32::<LittleEndian>(z).unwrap();
        }

        // voxelの数 × 頂点の座標数（3） × 4バイト（f32）
        let byte_length = points_count * 3 * 4;

        // GLTFファイルの作成
        let gltf_json = json!( {
            "asset": {
                "version": "2.0",
            },
            "scene": 0,
            "scenes": [
                {
                    "nodes": [0],
                },
            ],
            "nodes": [
                {"mesh": 0},
            ],
            "meshes": [
                {
                    "primitives": [
                        {
                            "attributes": {"POSITION": 0},
                            "mode": 0,
                        },
                    ],
                },
            ],
            "buffers": [
                {
                    "uri": "./output.bin",
                    "byteLength": byte_length,
                },
            ],
            "bufferViews": [
                {
                    "buffer": 0,
                    "byteOffset": 0,
                    "byteLength": byte_length,
                    "target": 34962,
                },
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "byteOffset": 0,
                    "componentType": 5126,
                    "count": points_count,
                    "type": "VEC3",
                    "min": [min_point[0], min_point[1], min_point[2]],
                    "max": [max_point[0], max_point[1], max_point[2]],
                },
            ],
        });

        // gltfファイルを出力
        let mut gltf_file = File::create("data/output.gltf").unwrap();
        let _ = gltf_file.write_all(gltf_json.to_string().as_bytes());
    }
}
