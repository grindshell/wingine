//! Reading model files, glTF 2.0, into what a [`Model`](super::Model) is made of, without a GPU.
//!
//! The engine draws what a model's meshes look like, not how they move: it reads the default
//! scene's nodes and their meshes' triangles, with their positions, normals, texture coordinates
//! and vertex colors, and each material's base color, PNG texture (how it wraps, and its
//! `KHR_texture_transform`) and whether it's double-sided. Skins, morph targets, animations,
//! cameras, lights and the rest of materials are left out, and files that require an extension
//! are refused.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use ::gltf::{
    buffer, image as gltf_image,
    material::AlphaMode,
    mesh::Mode,
    texture::{self, WrappingMode},
};
use image::RgbaImage;

use super::{Color, MeshData, MeshVertex};
use crate::math::{Affine3, cross3, normalize3, sub3};

/// How deeply a file's nodes may nest, so one whose nodes loop is an error, not a crash.
const MAX_DEPTH: u32 = 256;

/// What a model file holds that the engine draws.
#[derive(Debug, Default)]
pub(crate) struct ModelData {
    /// The parts' meshes, one for each primitive of the file's meshes that its nodes place.
    pub meshes: Vec<MeshData>,
    /// The parts' textures, decoded.
    pub textures: Vec<TextureData>,
    pub parts: Vec<PartData>,
    /// The files beside the model file that it read, such as a `.gltf`'s buffers and images.
    pub files: Vec<PathBuf>,
}

/// A texture of a model: its image, and how texture coordinates outside it pick its texels.
#[derive(Debug)]
pub(crate) struct TextureData {
    pub image: RgbaImage,
    /// Across, then down.
    pub wrap: [Wrap; 2],
}

/// How texture coordinates outside 0 to 1, along one of a texture's axes, pick its texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Wrap {
    /// The texels at its edge.
    Clamp,
    /// The texture again, as tiles.
    Repeat,
    /// The texture again, every other time mirrored.
    Mirror,
}

/// A mesh placed in a model.
#[derive(Debug)]
pub(crate) struct PartData {
    /// Which of the model's meshes it is: parts placing the same one share it.
    pub mesh: usize,
    /// Which of its textures it shows, if any.
    pub texture: Option<usize>,
    pub transform: Affine3,
    /// Whether its material is double-sided, showing its triangles from behind too.
    pub double_sided: bool,
}

/// Reads a model file's `bytes`, `.gltf` or `.glb`, finding the files it names in `folder`.
pub(crate) fn read(bytes: &[u8], folder: &Path) -> Result<ModelData, String> {
    // Parsed once, and checked for what it needs before it's validated, which would fail on what
    // the engine doesn't know with a less helpful error.
    let ::gltf::Gltf {
        document: unchecked,
        mut blob,
    } = ::gltf::Gltf::from_slice_without_validation(bytes).map_err(|err| err.to_string())?;
    let required: Vec<&str> = unchecked.extensions_required().collect();
    if !required.is_empty() {
        return Err(format!(
            "it needs {}, which the engine doesn't read: export it without",
            required.join(", ")
        ));
    }
    let document =
        ::gltf::Document::from_json(unchecked.into_json()).map_err(|err| err.to_string())?;
    let mut files = Vec::new();
    let buffers = document
        .buffers()
        .map(|buffer| {
            let data = match buffer.source() {
                buffer::Source::Bin => blob.take().ok_or("its binary chunk is missing")?,
                buffer::Source::Uri(uri) => load_uri(uri, folder, &mut files)?,
            };
            if data.len() < buffer.length() {
                return Err(format!(
                    "buffer {} has {} bytes, fewer than the {} it says",
                    buffer.index(),
                    data.len(),
                    buffer.length()
                ));
            }
            Ok(data)
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut reader = Reader {
        buffers,
        folder,
        meshes: HashMap::new(),
        textures: HashMap::new(),
        data: ModelData {
            files,
            ..ModelData::default()
        },
    };
    match document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        Some(scene) => {
            for node in scene.nodes() {
                reader.node(&node, Affine3::IDENTITY, 0)?;
            }
        }
        // Without a scene, every node that isn't another's child.
        None => {
            let children: HashSet<usize> = document
                .nodes()
                .flat_map(|node| node.children().map(|child| child.index()))
                .collect();
            for node in document.nodes() {
                if !children.contains(&node.index()) {
                    reader.node(&node, Affine3::IDENTITY, 0)?;
                }
            }
        }
    }
    Ok(reader.data)
}

/// Reads a file's nodes into a model.
struct Reader<'a> {
    buffers: Vec<Vec<u8>>,
    folder: &'a Path,
    /// The meshes read so far, in `data.meshes`, by the file's mesh and the primitive of it.
    meshes: HashMap<(usize, usize), usize>,
    /// The textures made so far, in `data.textures`, by the file's image, whether its alpha is
    /// ignored, as an opaque material's is, and how it wraps.
    textures: HashMap<(usize, bool, [Wrap; 2]), usize>,
    data: ModelData,
}

impl Reader<'_> {
    /// Adds the parts of `node` and the nodes inside it, placed in `parent`.
    fn node(&mut self, node: &::gltf::Node, parent: Affine3, depth: u32) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("its nodes nest too deeply, or in a loop".to_owned());
        }
        let transform = parent * affine(node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let mesh_index = self.mesh(&mesh, &primitive).map_err(|err| {
                    format!(
                        "{}, part {}: {err}",
                        named("mesh", mesh.index(), mesh.name()),
                        primitive.index()
                    )
                })?;
                let material = primitive.material();
                let texture = self.texture(&material, mesh_index)?;
                self.data.parts.push(PartData {
                    mesh: mesh_index,
                    texture,
                    transform,
                    double_sided: material.double_sided(),
                });
            }
        }
        for child in node.children() {
            self.node(&child, transform, depth + 1)?;
        }
        Ok(())
    }

    /// The index in `data.meshes` of `primitive` of `mesh`, reading it if it hasn't been.
    fn mesh(
        &mut self,
        mesh: &::gltf::Mesh,
        primitive: &::gltf::Primitive,
    ) -> Result<usize, String> {
        let key = (mesh.index(), primitive.index());
        if let Some(&index) = self.meshes.get(&key) {
            return Ok(index);
        }
        let data = mesh_data(primitive, &self.buffers)?;
        self.data.meshes.push(data);
        let index = self.data.meshes.len() - 1;
        self.meshes.insert(key, index);
        Ok(index)
    }

    /// The index in `data.textures` of `material`'s base color texture, if it has one, as the
    /// mesh `mesh` of `data.meshes` shows it, decoding it if it hasn't been.
    fn texture(
        &mut self,
        material: &::gltf::Material,
        mesh: usize,
    ) -> Result<Option<usize>, String> {
        let Some(info) = material.pbr_metallic_roughness().base_color_texture() else {
            return Ok(None);
        };
        let image = info.texture().source();
        let opaque = material.alpha_mode() == AlphaMode::Opaque;
        // How it wraps matters only along the axes where the mesh's coordinates leave the texture,
        // and clamping keeps the edges of those that don't from picking up the far side's texels.
        let sampler = info.texture().sampler();
        let modes = [sampler.wrap_s(), sampler.wrap_t()];
        let vertices = &self.data.meshes[mesh].vertices;
        let wrap = [0, 1].map(|axis| {
            let leaves = vertices
                .iter()
                .any(|vertex| !(0.0..=1.0).contains(&vertex.uv[axis]));
            match modes[axis] {
                WrappingMode::Repeat if leaves => Wrap::Repeat,
                WrappingMode::MirroredRepeat if leaves => Wrap::Mirror,
                _ => Wrap::Clamp,
            }
        });
        let key = (image.index(), opaque, wrap);
        if let Some(&index) = self.textures.get(&key) {
            return Ok(Some(index));
        }
        let mut decoded = self
            .decode(&image)
            .map_err(|err| format!("{}: {err}", named("image", image.index(), image.name())))?;
        if opaque {
            for pixel in decoded.pixels_mut() {
                pixel.0[3] = u8::MAX;
            }
        }
        self.data.textures.push(TextureData {
            image: decoded,
            wrap,
        });
        let index = self.data.textures.len() - 1;
        self.textures.insert(key, index);
        Ok(Some(index))
    }

    fn decode(&mut self, image: &::gltf::Image) -> Result<RgbaImage, String> {
        let bytes = match image.source() {
            gltf_image::Source::View { view, .. } => {
                let buffer = &self.buffers[view.buffer().index()];
                let range = view.offset()..view.offset() + view.length();
                Cow::Borrowed(
                    buffer
                        .get(range)
                        .ok_or("it runs past the end of its buffer")?,
                )
            }
            gltf_image::Source::Uri { uri, .. } => {
                Cow::Owned(load_uri(uri, self.folder, &mut self.data.files)?)
            }
        };
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            return Err(
                "it's a JPEG, which the engine doesn't read: export textures as PNG".to_owned(),
            );
        }
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("it isn't a PNG, the only images the engine reads".to_owned());
        }
        image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map(|image| image.into_rgba8())
            .map_err(|err| err.to_string())
    }
}

/// The triangles of `primitive`, its material's base color made part of its vertices' colors.
fn mesh_data(primitive: &::gltf::Primitive, buffers: &[Vec<u8>]) -> Result<MeshData, String> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or("it has no positions")?
        .collect();
    let count = positions.len();
    let indices: Vec<u32> = match reader.read_indices() {
        Some(indices) => indices.into_u32().collect(),
        None => (0..count as u32).collect(),
    };
    let triangles = triangles(primitive.mode(), &indices)?;
    if let Some(index) = triangles.iter().find(|&&index| index as usize >= count) {
        return Err(format!("index {index} is past its {count} vertices"));
    }

    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();
    let factor = pbr.base_color_factor();
    let opaque = material.alpha_mode() == AlphaMode::Opaque;
    let info = pbr.base_color_texture();
    let transform = info.as_ref().and_then(texture::Info::texture_transform);
    // A transform may pick other texture coordinates.
    let set = info.as_ref().map_or(0, |info| {
        let picked = transform
            .as_ref()
            .and_then(|transform| transform.tex_coord());
        picked.unwrap_or(info.tex_coord())
    });
    let uvs: Vec<[f32; 2]> = reader
        .read_tex_coords(set)
        .map(|uvs| match &transform {
            Some(transform) => {
                let [offset, scale] = [transform.offset(), transform.scale()];
                let rotation = transform.rotation();
                uvs.into_f32()
                    .map(|uv| transform_uv(uv, offset, rotation, scale))
                    .collect()
            }
            None => uvs.into_f32().collect(),
        })
        .unwrap_or_default();
    let colors: Vec<[f32; 4]> = reader
        .read_colors(0)
        .map(|colors| colors.into_rgba_f32().collect())
        .unwrap_or_default();
    let vertex = |i: usize, normal: [f32; 3]| {
        // Both are linear, so they multiply as they are.
        let color = colors.get(i).copied().unwrap_or([1.0; 4]);
        let mut linear = [0, 1, 2, 3].map(|c| color[c] * factor[c]);
        if opaque {
            linear[3] = 1.0;
        }
        MeshVertex {
            position: positions[i],
            normal,
            uv: uvs.get(i).copied().unwrap_or([0.0; 2]),
            color: Color::from_linear(linear),
        }
    };

    Ok(match reader.read_normals() {
        Some(normals) => {
            let normals: Vec<[f32; 3]> = normals.collect();
            MeshData {
                vertices: (0..count)
                    .map(|i| vertex(i, normals.get(i).copied().unwrap_or([0.0; 3])))
                    .collect(),
                indices: triangles,
            }
        }
        // Flat, as glTF asks of a file without normals: each triangle's corners are copies of
        // their own, facing as it does.
        None => {
            let mut data = MeshData::default();
            for triangle in triangles.as_chunks::<3>().0 {
                let [a, b, c] = triangle.map(|index| positions[index as usize]);
                let normal = normalize3(cross3(sub3(b, a), sub3(c, a))).unwrap_or([0.0; 3]);
                for &index in triangle {
                    data.indices.push(data.vertices.len() as u32);
                    data.vertices.push(vertex(index as usize, normal));
                }
            }
            data
        }
    })
}

/// `uv` moved as `KHR_texture_transform` moves it: scaled, turned `rotation` radians, then
/// moved by `offset`, as its specification's matrix `translation * rotation * scale` does.
fn transform_uv(uv: [f32; 2], offset: [f32; 2], rotation: f32, scale: [f32; 2]) -> [f32; 2] {
    let [x, y] = [uv[0] * scale[0], uv[1] * scale[1]];
    let (sin, cos) = rotation.sin_cos();
    [cos * x - sin * y + offset[0], sin * x + cos * y + offset[1]]
}

/// Three of `indices` for each triangle `mode` makes of them, each turning as the first does.
fn triangles(mode: Mode, indices: &[u32]) -> Result<Vec<u32>, String> {
    let corners = |i: usize| indices[i];
    Ok(match mode {
        Mode::Triangles => indices[..indices.len() / 3 * 3].to_vec(),
        // Every other triangle of a strip turns the other way round its corners.
        Mode::TriangleStrip => (0..indices.len().saturating_sub(2))
            .flat_map(|i| {
                if i % 2 == 0 {
                    [corners(i), corners(i + 1), corners(i + 2)]
                } else {
                    [corners(i), corners(i + 2), corners(i + 1)]
                }
            })
            .collect(),
        Mode::TriangleFan => (1..indices.len().saturating_sub(1))
            .flat_map(|i| [corners(0), corners(i), corners(i + 1)])
            .collect(),
        Mode::Points => return Err("it's points, not triangles".to_owned()),
        Mode::Lines | Mode::LineLoop | Mode::LineStrip => {
            return Err("it's lines, not triangles".to_owned());
        }
    })
}

/// The transform of a node's matrix, whose columns are its axes and its translation.
fn affine(m: [[f32; 4]; 4]) -> Affine3 {
    let column = |i: usize| [m[i][0], m[i][1], m[i][2]];
    Affine3 {
        x_axis: column(0),
        y_axis: column(1),
        z_axis: column(2),
        translation: column(3),
    }
}

/// How an error names something of a file: by its name, if it has one, and its index.
fn named(what: &str, index: usize, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{what} {index}, `{name}`"),
        None => format!("{what} {index}"),
    }
}

/// The data a URI in a file names: in itself, base64 after `data:`, or in a file in `folder`,
/// whose path it adds to `files`.
fn load_uri(uri: &str, folder: &Path, files: &mut Vec<PathBuf>) -> Result<Vec<u8>, String> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (kind, payload) = data.split_once(',').ok_or("a data URI holds no data")?;
        if !kind.ends_with(";base64") {
            return Err("a data URI's data isn't base64".to_owned());
        }
        return base64(payload).ok_or_else(|| "a data URI's base64 is broken".to_owned());
    }
    if uri.contains("://") {
        return Err(format!("`{uri}` isn't a file beside the model"));
    }
    let path = folder.join(percent_decoded(uri)?);
    let data = crate::files::read(&path).map_err(|err| format!("{}: {err}", path.display()));
    if !files.contains(&path) {
        files.push(path);
    }
    data
}

/// The bytes `text` encodes in base64, or `None` if it isn't base64.
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    let [mut bits, mut held] = [0u32, 0u32];
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => return None,
        };
        bits = bits << 6 | u32::from(value);
        held += 6;
        if held >= 8 {
            held -= 8;
            bytes.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    Some(bytes)
}

/// `uri` with its `%` escapes decoded, as a path.
fn percent_decoded(uri: &str) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(uri.len());
    let mut rest = uri.as_bytes();
    while let Some((&c, after)) = rest.split_first() {
        if c == b'%' {
            let byte = after
                .get(..2)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| format!("`{uri}` has a broken escape"))?;
            bytes.push(byte);
            rest = &after[2..];
        } else {
            bytes.push(c);
            rest = after;
        }
    }
    String::from_utf8(bytes).map_err(|_| format!("`{uri}` isn't UTF-8"))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::save::TemporaryFolder;

    fn encode_base64(bytes: &[u8]) -> String {
        const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut text = String::new();
        for chunk in bytes.chunks(3) {
            let bits = chunk
                .iter()
                .enumerate()
                .fold(0u32, |bits, (i, &b)| bits | u32::from(b) << (16 - 8 * i));
            for i in 0..4 {
                if i <= chunk.len() {
                    text.push(DIGITS[(bits >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    text.push('=');
                }
            }
        }
        text
    }

    /// A buffer of little-endian floats and then `u16` indices, and its accessors' JSON.
    struct Buffer {
        bytes: Vec<u8>,
        views: Vec<String>,
        accessors: Vec<String>,
    }

    impl Buffer {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                views: Vec::new(),
                accessors: Vec::new(),
            }
        }

        /// Adds an accessor of floats, `width` to each element, such as 3 for `VEC3`.
        fn floats(&mut self, values: &[f32], kind: &str, width: usize) -> usize {
            let offset = self.bytes.len();
            for value in values {
                self.bytes.extend(value.to_le_bytes());
            }
            let count = values.len() / width;
            let bounds = if kind == "VEC3" {
                let axis = |i: usize, pick: fn(f32, f32) -> f32| {
                    values.chunks(3).map(|v| v[i]).reduce(pick).unwrap()
                };
                format!(
                    r#","min":[{},{},{}],"max":[{},{},{}]"#,
                    axis(0, f32::min),
                    axis(1, f32::min),
                    axis(2, f32::min),
                    axis(0, f32::max),
                    axis(1, f32::max),
                    axis(2, f32::max)
                )
            } else {
                String::new()
            };
            self.add(
                offset,
                format!(r#""componentType":5126,"count":{count},"type":"{kind}"{bounds}"#),
            )
        }

        fn indices(&mut self, indices: &[u16]) -> usize {
            let offset = self.bytes.len();
            for index in indices {
                self.bytes.extend(index.to_le_bytes());
            }
            while !self.bytes.len().is_multiple_of(4) {
                self.bytes.push(0);
            }
            let count = indices.len();
            self.add(
                offset,
                format!(r#""componentType":5123,"count":{count},"type":"SCALAR""#),
            )
        }

        fn add(&mut self, offset: usize, accessor: String) -> usize {
            let length = self.bytes.len() - offset;
            self.views.push(format!(
                r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{length}}}"#
            ));
            let view = self.views.len() - 1;
            self.accessors
                .push(format!(r#"{{"bufferView":{view},{accessor}}}"#));
            self.accessors.len() - 1
        }

        /// The file's JSON, its buffer in a data URI, with `rest` among the top-level fields.
        fn gltf(&self, rest: &str) -> String {
            format!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{},"uri":"data:application/octet-stream;base64,{}"}}],"bufferViews":[{}],"accessors":[{}],{rest}}}"#,
                self.bytes.len(),
                encode_base64(&self.bytes),
                self.views.join(","),
                self.accessors.join(","),
            )
        }
    }

    /// A triangle facing `+z`: its positions' accessor and its indices'.
    fn triangle(buffer: &mut Buffer) -> (usize, usize) {
        let positions = buffer.floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], "VEC3", 3);
        (positions, buffer.indices(&[0, 1, 2]))
    }

    fn read_str(json: &str) -> Result<ModelData, String> {
        read(json.as_bytes(), Path::new("."))
    }

    fn png(image: &RgbaImage) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn nodes_place_their_meshes_and_share_them() {
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        let json = buffer.gltf(&format!(
            r#""meshes":[{{"name":"tri","primitives":[{{"attributes":{{"POSITION":{positions}}},"indices":{indices}}}]}}],
            "nodes":[
                {{"translation":[10,0,0],"children":[1]}},
                {{"mesh":0,"scale":[2,2,2]}},
                {{"mesh":0,"translation":[0,5,0]}},
                {{"mesh":0,"translation":[0,0,99]}}
            ],
            "scenes":[{{"nodes":[0,2]}}],"scene":0"#
        ));
        let model = read_str(&json).unwrap();
        // The node no scene places isn't part of it.
        assert_eq!(model.parts.len(), 2);
        assert_eq!(model.meshes.len(), 1, "both place the same mesh");
        let child = &model.parts[0];
        assert_eq!(child.transform.apply([1.0, 0.0, 0.0]), [12.0, 0.0, 0.0]);
        assert_eq!(model.parts[1].transform.translation, [0.0, 5.0, 0.0]);
        assert!(
            model
                .parts
                .iter()
                .all(|part| part.mesh == 0 && part.texture.is_none() && !part.double_sided)
        );
        assert!(model.files.is_empty(), "its buffer is in the file itself");
        let mesh = &model.meshes[0];
        assert_eq!(mesh.indices, [0, 1, 2]);
        // Without normals, its corners face as the triangle does.
        assert!(mesh.vertices.iter().all(|v| v.normal == [0.0, 0.0, 1.0]));
        assert!(mesh.vertices.iter().all(|v| v.color.to_rgba8() == [255; 4]));
    }

    #[test]
    fn without_a_scene_every_top_node_is_placed() {
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        let json = buffer.gltf(&format!(
            r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":{positions}}},"indices":{indices}}}]}}],
            "nodes":[{{"mesh":0,"children":[1]}},{{"mesh":0}},{{"mesh":0}}]"#
        ));
        assert_eq!(read_str(&json).unwrap().parts.len(), 3);
    }

    #[test]
    fn colors_are_made_srgb_and_multiplied_by_the_material() {
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        let normals = buffer.floats(&[0.0, 0.0, 1.0].repeat(3), "VEC3", 3);
        let colors = buffer.floats(
            &[
                1.0, 1.0, 1.0, 1.0, 0.214, 0.0, 0.0, 0.5, 0.0, 0.0, 1.0, 0.25,
            ],
            "VEC4",
            4,
        );
        let primitive = |material: usize| {
            format!(
                r#"{{"attributes":{{"POSITION":{positions},"NORMAL":{normals},"COLOR_0":{colors}}},"indices":{indices},"material":{material}}}"#
            )
        };
        let json = buffer.gltf(&format!(
            r#""materials":[
                {{"pbrMetallicRoughness":{{"baseColorFactor":[1,0.214,1,1]}}}},
                {{"alphaMode":"MASK"}}
            ],
            "meshes":[{{"primitives":[{},{}]}}],
            "nodes":[{{"mesh":0}}]"#,
            primitive(0),
            primitive(1)
        ));
        let model = read_str(&json).unwrap();
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        // Opaque: halfway in linear light is about 0.5 in sRGB, and alpha is ignored.
        let [white, red, blue] = [0, 1, 2].map(|i| model.meshes[0].vertices[i].color);
        assert!(
            near(white.r, 1.0) && near(white.g, 0.5) && near(white.b, 1.0),
            "{white:?}"
        );
        assert!(near(red.r, 0.5) && red.a == 1.0, "{red:?}");
        assert!(near(blue.b, 1.0) && blue.a == 1.0, "{blue:?}");
        // Masked, alpha stays.
        let masked = &model.meshes[1].vertices;
        assert!(near(masked[1].color.a, 0.5) && near(masked[2].color.a, 0.25));
        assert!(masked.iter().all(|v| v.normal == [0.0, 0.0, 1.0]));
    }

    #[test]
    fn strips_and_fans_become_triangles_facing_the_same_way() {
        assert_eq!(
            triangles(Mode::TriangleStrip, &[0, 1, 2, 3, 4]).unwrap(),
            [0, 1, 2, 1, 3, 2, 2, 3, 4]
        );
        assert_eq!(
            triangles(Mode::TriangleFan, &[0, 1, 2, 3]).unwrap(),
            [0, 1, 2, 0, 2, 3]
        );
        assert_eq!(
            triangles(Mode::Triangles, &[0, 1, 2, 3]).unwrap(),
            [0, 1, 2]
        );
        assert!(
            triangles(Mode::Lines, &[0, 1])
                .unwrap_err()
                .contains("lines")
        );
        assert!(
            triangles(Mode::Points, &[0])
                .unwrap_err()
                .contains("points")
        );
    }

    #[test]
    fn textures_are_pngs_shared_between_materials() {
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        let uvs = buffer.floats(&[0.0, 1.0, 1.0, 1.0, 0.0, 0.0], "VEC2", 2);
        let image = RgbaImage::from_fn(2, 1, |x, _| image::Rgba([x as u8 * 255, 0, 0, 100]));
        let uri = format!("data:image/png;base64,{}", encode_base64(&png(&image)));
        let primitive = |material: usize| {
            format!(
                r#"{{"attributes":{{"POSITION":{positions},"TEXCOORD_0":{uvs}}},"indices":{indices},"material":{material}}}"#
            )
        };
        let json = buffer.gltf(&format!(
            r#""images":[{{"uri":"{uri}"}}],"textures":[{{"source":0}}],
            "materials":[
                {{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}},
                {{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}},
                {{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}},"alphaMode":"MASK"}}
            ],
            "meshes":[{{"primitives":[{},{},{}]}}],
            "nodes":[{{"mesh":0}}]"#,
            primitive(0),
            primitive(1),
            primitive(2)
        ));
        let model = read_str(&json).unwrap();
        let textures: Vec<_> = model.parts.iter().map(|part| part.texture).collect();
        assert_eq!(textures, [Some(0), Some(0), Some(1)]);
        // Opaque materials ignore its alpha; masked ones keep it.
        let image = |i: usize| &model.textures[i].image;
        assert_eq!(image(0).get_pixel(1, 0).0, [255, 0, 0, 255]);
        assert_eq!(image(1).get_pixel(1, 0).0, [255, 0, 0, 100]);
        assert_eq!(model.meshes[0].vertices[0].uv, [0.0, 1.0]);
        // Its coordinates stay on the texture, so however its sampler wraps, it clamps.
        assert!(
            model
                .textures
                .iter()
                .all(|texture| texture.wrap == [Wrap::Clamp; 2])
        );
    }

    /// A triangle showing a texture with its coordinates `uvs`, sampled by `sampler`'s JSON if
    /// it has one, with `material`'s JSON in its material.
    fn textured(uvs: &[f32], sampler: Option<&str>, material: &str) -> String {
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        let uvs = buffer.floats(uvs, "VEC2", 2);
        let image = RgbaImage::from_pixel(1, 1, image::Rgba([9, 9, 9, 255]));
        let uri = format!("data:image/png;base64,{}", encode_base64(&png(&image)));
        let (samplers, texture) = match sampler {
            Some(sampler) => (format!(r#""samplers":[{sampler}],"#), r#","sampler":0"#),
            None => (String::new(), ""),
        };
        buffer.gltf(&format!(
            r#"{samplers}"images":[{{"uri":"{uri}"}}],"textures":[{{"source":0{texture}}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}{material}}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{positions},"TEXCOORD_0":{uvs}}},"indices":{indices},"material":0}}]}}],
            "nodes":[{{"mesh":0}}]"#
        ))
    }

    #[test]
    fn textures_wrap_as_their_samplers_say_where_coordinates_leave_them() {
        // Repeating across, as glTF's samplers do by default, but not down, where it stays on.
        let tiled = read_str(&textured(&[0.0, 0.0, 3.0, 0.0, 0.0, 1.0], None, "")).unwrap();
        assert_eq!(tiled.textures[0].wrap, [Wrap::Repeat, Wrap::Clamp]);

        // Mirrored across, and clamped down, though it leaves the texture that way too.
        let sampler = r#"{"wrapS":33648,"wrapT":33071}"#;
        let json = textured(&[-1.0, 0.0, 2.0, 0.0, 0.0, 2.0], Some(sampler), "");
        let mirrored = read_str(&json).unwrap();
        assert_eq!(mirrored.textures[0].wrap, [Wrap::Mirror, Wrap::Clamp]);
    }

    #[test]
    fn texture_transforms_move_coordinates() {
        let transform = r#","extensions":{"KHR_texture_transform":{"offset":[0.5,0],"rotation":1.5707964,"scale":[2,3]}}"#;
        let json = textured(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0], None, "")
            .replace(r#"{"index":0}"#, &format!(r#"{{"index":0{transform}}}"#))
            .replace(
                r#""asset""#,
                r#""extensionsUsed":["KHR_texture_transform"],"asset""#,
            );
        let model = read_str(&json).unwrap();
        let uvs: Vec<_> = model.meshes[0].vertices.iter().map(|v| v.uv).collect();
        // Scaled to (2, 0) and (0, 3), turned a quarter turn to (0, 2) and (-3, 0), then moved.
        let expected = [[0.5, 0.0], [0.5, 2.0], [-2.5, 0.0]];
        for (uv, expected) in uvs.iter().zip(expected) {
            assert!(
                (uv[0] - expected[0]).abs() < 1e-5 && (uv[1] - expected[1]).abs() < 1e-5,
                "{uvs:?}"
            );
        }
        // Off the texture both ways now, and repeating as the sampler's default.
        assert_eq!(model.textures[0].wrap, [Wrap::Repeat; 2]);
    }

    #[test]
    fn double_sided_materials_say_so() {
        let json = textured(&[0.0; 6], None, r#","doubleSided":true"#);
        assert!(read_str(&json).unwrap().parts[0].double_sided);
        let json = textured(&[0.0; 6], None, "");
        assert!(!read_str(&json).unwrap().parts[0].double_sided);
    }

    #[test]
    fn binary_files_hold_their_buffer() {
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],"bufferViews":[{}],"accessors":[{}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{positions}}},"indices":{indices}}}]}}],
            "nodes":[{{"mesh":0}}]}}"#,
            buffer.bytes.len(),
            buffer.views.join(","),
            buffer.accessors.join(",")
        );
        let pad = |mut bytes: Vec<u8>, with: u8| {
            while !bytes.len().is_multiple_of(4) {
                bytes.push(with);
            }
            bytes
        };
        let [json, bin] = [pad(json.into_bytes(), b' '), pad(buffer.bytes.clone(), 0)];
        let mut glb = b"glTF".to_vec();
        glb.extend(2u32.to_le_bytes());
        glb.extend(((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
        for (chunk, kind) in [(&json, b"JSON"), (&bin, b"BIN\0")] {
            glb.extend((chunk.len() as u32).to_le_bytes());
            glb.extend(kind);
            glb.extend(chunk);
        }
        let model = read(&glb, Path::new(".")).unwrap();
        assert_eq!(model.meshes[0].vertices[1].position, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn files_beside_the_model_are_read_by_their_escaped_names() {
        let folder = TemporaryFolder::new();
        std::fs::create_dir_all(folder.path()).unwrap();
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        std::fs::write(folder.path().join("tri data.bin"), &buffer.bytes).unwrap();
        let image = RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
        std::fs::write(folder.path().join("skin.png"), png(&image)).unwrap();
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{},"uri":"tri%20data.bin"}}],"bufferViews":[{}],"accessors":[{}],
            "images":[{{"uri":"skin.png"}}],"textures":[{{"source":0}}],
            "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":{positions}}},"indices":{indices},"material":0}}]}}],
            "nodes":[{{"mesh":0}}]}}"#,
            buffer.bytes.len(),
            buffer.views.join(","),
            buffer.accessors.join(",")
        );
        let model = read(json.as_bytes(), folder.path()).unwrap();
        assert_eq!(model.meshes[0].vertices.len(), 3);
        assert_eq!(model.textures[0].image.get_pixel(0, 0).0, [1, 2, 3, 255]);
        // Each read once, and listed, so tools know to read the model again when they change.
        let files = ["tri data.bin", "skin.png"].map(|name| folder.path().join(name));
        assert_eq!(model.files, files);

        let missing = json.replace("skin.png", "gone.png");
        let err = read(missing.as_bytes(), folder.path()).unwrap_err();
        assert!(err.contains("image 0") && err.contains("gone.png"), "{err}");
    }

    #[test]
    fn what_the_engine_doesnt_read_is_an_error_saying_so() {
        let mut buffer = Buffer::new();
        let (positions, indices) = triangle(&mut buffer);
        let mesh = |rest: &str| {
            buffer.gltf(&format!(
                r#""meshes":[{{"name":"rope","primitives":[{{"attributes":{{"POSITION":{positions}}},"indices":{indices}{rest}}}]}}],"nodes":[{{"mesh":0}}]"#
            ))
        };
        let err = read_str(&mesh(r#","mode":1"#)).unwrap_err();
        assert_eq!(err, "mesh 0, `rope`, part 0: it's lines, not triangles");

        let required = mesh("").replace(
            r#""asset""#,
            r#""extensionsUsed":["KHR_draco_mesh_compression"],"extensionsRequired":["KHR_draco_mesh_compression"],"asset""#,
        );
        let err = read_str(&required).unwrap_err();
        assert!(err.contains("KHR_draco_mesh_compression"), "{err}");

        let jpeg = format!(
            "data:image/jpeg;base64,{}",
            encode_base64(&[0xff, 0xd8, 0xff, 0xe0, 0, 0])
        );
        let textured = mesh(r#","material":0"#).replace(
            r#""meshes""#,
            &format!(
                r#""images":[{{"uri":"{jpeg}"}}],"textures":[{{"source":0}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"meshes""#
            ),
        );
        let err = read_str(&textured).unwrap_err();
        assert!(err.contains("JPEG") && err.contains("PNG"), "{err}");

        let past = buffer.gltf(&format!(
            r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":{positions}}}}}]}}],"nodes":[{{"mesh":0}}]"#
        ));
        assert!(
            read_str(&past).is_ok(),
            "without indices, each three vertices are a triangle"
        );
        assert!(read_str("not a model").is_err());
    }

    #[test]
    fn base64_and_escapes_decode() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGk").unwrap(), b"hi");
        assert_eq!(base64("a b"), None);
        assert_eq!(percent_decoded("a%20b%2Fc").unwrap(), "a b/c");
        assert!(percent_decoded("a%2").is_err());
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(base64(&encode_base64(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn the_example_props_read_as_blender_made_them() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/models");
        let prop = |name: &str| {
            let bytes = std::fs::read(folder.join(name)).unwrap();
            read(&bytes, &folder).unwrap_or_else(|err| panic!("{name}: {err}"))
        };

        // Colored by its vertices: staves of two browns and iron hoops.
        let barrel = prop("barrel.glb");
        assert_eq!((barrel.parts.len(), barrel.textures.len()), (1, 0));
        let colors: HashSet<[u8; 4]> = barrel.meshes[0]
            .vertices
            .iter()
            .map(|vertex| vertex.color.to_rgba8())
            .collect();
        assert!(colors.contains(&[0x4c, 0x4a, 0x54, 255]), "{colors:?}");
        assert!(colors.len() >= 5, "{colors:?}");

        // Textured, the texture held in the file. Its sampler repeats, but its coordinates stay on
        // the texture, so it clamps; and like the other props, it shows only from outside.
        let crate_ = prop("crate.glb");
        assert_eq!(crate_.parts[0].texture, Some(0));
        assert_eq!(crate_.textures[0].image.dimensions(), (12, 12));
        assert_eq!(crate_.textures[0].wrap, [Wrap::Clamp; 2]);
        assert!(crate_.files.is_empty());
        let props = [&barrel, &crate_];
        assert!(
            props
                .iter()
                .all(|prop| prop.parts.iter().all(|p| !p.double_sided))
        );

        // A top, and four legs that share a mesh.
        let table = prop("table.glb");
        assert_eq!((table.parts.len(), table.meshes.len()), (5, 2));
        let legs = table
            .parts
            .iter()
            .filter(|part| part.mesh == table.parts[1].mesh);
        assert_eq!(legs.count(), 4);
    }
}
