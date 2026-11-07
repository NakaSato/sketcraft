//! 3MF and glTF import, and choosing a reader from the file name.
use super::*;
use std::io::{Cursor, Write};

fn positions(model: &Model) -> Vec<DVec3> {
    let mut points: Vec<DVec3> = model.vertices.values().map(|v| v.position).collect();
    points.sort_by(|a, b| a.to_array().partial_cmp(&b.to_array()).unwrap());
    points
}
fn close(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-9
}

fn package(parts: &[(&str, &str)], method: zip::CompressionMethod) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default().compression_method(method))
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Target="/3D/part.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>"#;

/// A 1 cm right triangle (object 1), a component that moves it 10 cm along X (object 2),
/// and two build items: object 2 as is, and object 1 lifted 5 cm.
const MODEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="centimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/>
          <vertex x="2" y="0" z="0"/>
        </vertices>
        <triangles>
          <triangle v1="0" v2="1" v3="2"/>
          <triangle v1="0" v2="1" v3="3"/>
        </triangles>
      </mesh>
    </object>
    <object id="2" type="model">
      <components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 10 0 0"/></components>
    </object>
  </resources>
  <build>
    <item objectid="2"/>
    <item objectid="1" transform="1 0 0 0 1 0 0 0 1 0 0 5"/>
  </build>
</model>"#;

#[test]
fn threemf_follows_relationships_units_components_and_build_transforms() {
    for method in [zip::CompressionMethod::Stored, zip::CompressionMethod::Deflated] {
        let bytes = package(&[("_rels/.rels", RELS), ("3D/part.model", MODEL)], method);
        let model = import(&bytes, "3mf", "Bracket").unwrap();
        // Two placed triangles; the collinear (zero-area) one is skipped each time.
        assert_eq!(model.faces.len(), 2, "{method:?}");
        let points = positions(&model);
        assert_eq!(points.len(), 6);
        for expected in [
            DVec3::new(0.1, 0., 0.),
            DVec3::new(0.11, 0., 0.),
            DVec3::new(0.1, 0.01, 0.),
            DVec3::new(0., 0., 0.05),
            DVec3::new(0.01, 0., 0.05),
            DVec3::new(0., 0.01, 0.05),
        ] {
            assert!(points.iter().any(|p| close(*p, expected)), "missing {expected}");
        }
        assert_eq!(model.objects.values().next().unwrap().name, "Bracket");
    }
}

#[test]
fn threemf_rejects_bad_packages_without_panicking() {
    assert!(import(b"not a zip", "3mf", "x").is_err());
    let missing = package(&[("_rels/.rels", RELS)], zip::CompressionMethod::Stored);
    assert!(import(&missing, "3mf", "x").unwrap_err().contains("no model part"));
    let cycle = MODEL.replace(r#"<component objectid="1""#, r#"<component objectid="2""#);
    let looped = package(&[("_rels/.rels", RELS), ("3D/part.model", &cycle)], zip::CompressionMethod::Stored);
    assert!(import(&looped, "3mf", "x").unwrap_err().contains("nested too deeply"));
    let bad_index = MODEL.replace(r#"v3="3""#, r#"v3="9""#);
    let bad = package(&[("_rels/.rels", RELS), ("3D/part.model", &bad_index)], zip::CompressionMethod::Stored);
    assert!(import(&bad, "3mf", "x").unwrap_err().contains("missing vertex"));
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn base64_encode(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// One triangle in a node translated 2 m along glTF's −Z, which is SketCraft's +Y.
fn gltf_json(buffer_uri: Option<&str>, byte_length: usize) -> String {
    let uri = buffer_uri.map(|u| format!(r#""uri":"{u}","#)).unwrap_or_default();
    format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
        "nodes":[{{"mesh":0,"translation":[0,0,-2]}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
        "buffers":[{{{uri}"byteLength":{byte_length}}}],
        "bufferViews":[{{"buffer":0,"byteLength":36}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
            "min":[0,0,0],"max":[1,1,0]}}]}}"#
    )
}
fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut json = json.as_bytes().to_vec();
    json.resize(json.len().div_ceil(4) * 4, b' ');
    let mut bin = bin.to_vec();
    bin.resize(bin.len().div_ceil(4) * 4, 0);
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::new();
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(0x4E4F534Au32.to_le_bytes());
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(0x004E4942u32.to_le_bytes());
    out.extend(bin);
    out
}

#[test]
fn glb_converts_node_transforms_from_y_up_to_z_up() {
    let triangle = floats(&[0., 0., 0., 1., 0., 0., 0., 1., 0.]);
    let model = import(&glb(&gltf_json(None, 36), &triangle), "glb", "Chair").unwrap();
    assert_eq!(model.faces.len(), 1);
    let points = positions(&model);
    // glTF (x, y, z) becomes (x, −z, y); the node's −2 m along Z becomes +2 m along Y.
    for expected in [DVec3::new(0., 2., 0.), DVec3::new(1., 2., 0.), DVec3::new(0., 2., 1.)] {
        assert!(points.iter().any(|p| close(*p, expected)), "missing {expected}: {points:?}");
    }
}

#[test]
fn gltf_reads_embedded_buffers_and_strips_but_not_external_files() {
    // A strip of four points makes two triangles.
    let strip = floats(&[0., 0., 0., 1., 0., 0., 0., 1., 0., 1., 1., 0.]);
    let uri = format!("data:application/octet-stream;base64,{}", base64_encode(&strip));
    let json = gltf_json(Some(&uri), strip.len())
        .replace(r#""attributes":{"POSITION":0}"#, r#""attributes":{"POSITION":0},"mode":5"#)
        .replace(r#""byteLength":36}"#, r#""byteLength":48}"#)
        .replace(r#""count":3"#, r#""count":4"#);
    let model = import(json.as_bytes(), "gltf", "Strip").unwrap();
    assert_eq!(model.faces.len(), 2);

    let external = gltf_json(Some("chair.bin"), 36);
    assert!(import(external.as_bytes(), "gltf", "x").unwrap_err().contains("separate .bin"));
    assert!(import(b"{}", "gltf", "x").is_err());
}

#[test]
fn any_mode_picks_the_reader_from_the_file_extension() {
    use crate::project_files::import_extension;
    assert_eq!(import_extension("any", "/models/Chair.GLB").unwrap(), "glb");
    assert_eq!(import_extension("any", "bracket.3mf").unwrap(), "3mf");
    assert_eq!(import_extension("stl", "named.obj").unwrap(), "stl");
    assert!(import_extension("any", "notes.txt").unwrap_err().contains(".3mf"));
    assert!(import_extension("any", "no_extension").is_err());
}
