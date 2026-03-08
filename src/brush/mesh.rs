use std::collections::HashSet;
use std::path::Path;

use bevy::{
    asset::RenderAssetUsages,
    image::{
        CompressedImageFormats, ImageAddressMode, ImageSampler, ImageSamplerDescriptor, ImageType,
    },
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureSampleType},
};

use super::{
    BrushFaceEntity, BrushMaterialPalette, BrushMeshCache, BrushPreview, TextureCacheEntry,
    TextureMaterialCache,
};
use crate::draw_brush::DrawBrushState;
use crate::material_definition::{MaterialDefCacheEntry, MaterialDefinitionCache, MaterialLibrary};
use jackdaw_geometry::{compute_brush_geometry, compute_face_uvs, triangulate_face};

pub(super) fn setup_default_materials(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut palette: ResMut<BrushMaterialPalette>,
) {
    let defaults = [
        Color::srgb(0.7, 0.7, 0.7), // default grey (matches Cube mesh)
        Color::srgb(0.5, 0.5, 0.5), // gray
        Color::srgb(0.3, 0.3, 0.3), // dark gray
        Color::srgb(0.7, 0.3, 0.2), // brick red
        Color::srgb(0.3, 0.5, 0.7), // steel blue
        Color::srgb(0.4, 0.6, 0.3), // mossy green
        Color::srgb(0.6, 0.5, 0.3), // sandy tan
        Color::srgb(0.5, 0.3, 0.5), // purple
    ];
    for color in defaults {
        palette.materials.push(materials.add(StandardMaterial {
            base_color: color.with_alpha(1.0),
            ..default()
        }));
        palette
            .preview_materials
            .push(materials.add(StandardMaterial {
                base_color: color.with_alpha(0.75),
                alpha_mode: AlphaMode::Blend,
                ..default()
            }));
    }
}

pub(super) fn ensure_texture_materials(
    brushes: Query<&super::Brush>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<TextureMaterialCache>,
) {
    // Collect paths that need loading first to avoid borrow conflicts
    let mut paths_to_load: Vec<String> = Vec::new();
    for brush in &brushes {
        for face in &brush.faces {
            if let Some(ref path) = face.texture_path {
                if !cache.entries.contains_key(path) && !paths_to_load.contains(path) {
                    paths_to_load.push(path.clone());
                }
            }
        }
    }

    for path in paths_to_load {
        let image: Handle<Image> = asset_server.load(path.clone());
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(image.clone()),
            ..default()
        });
        cache
            .entries
            .insert(path, TextureCacheEntry { image, material });
    }
}

/// Returns true if the KTX2 file is NOT a simple 2D texture (cubemap or array texture).
fn is_ktx2_non_2d(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    use std::io::Read;
    let mut header = [0u8; 40];
    if file.read_exact(&mut header).is_err() {
        return false;
    }
    let layer_count = u32::from_le_bytes([header[32], header[33], header[34], header[35]]);
    let face_count = u32::from_le_bytes([header[36], header[37], header[38], header[39]]);
    layer_count > 1 || face_count > 1
}

/// Generate CPU-side mipmaps for uncompressed RGBA8 images via box filtering.
/// Compressed formats (e.g. KTX2 with BC7) typically include mipmaps already.
fn generate_cpu_mipmaps(image: &mut Image) {
    let format = image.texture_descriptor.format;
    if format != TextureFormat::Rgba8Unorm && format != TextureFormat::Rgba8UnormSrgb {
        return;
    }
    let w = image.texture_descriptor.size.width;
    let h = image.texture_descriptor.size.height;
    if w <= 1 && h <= 1 {
        return;
    }
    let mip_count = (w.max(h) as f32).log2().floor() as u32 + 1;
    let Some(data) = image.data.as_mut() else {
        return;
    };

    // Build mip chain by box-filtering from the previous level
    let mut prev_offset: usize = 0;
    let mut prev_w = w;
    let mut prev_h = h;
    for _ in 1..mip_count {
        let new_w = (prev_w / 2).max(1);
        let new_h = (prev_h / 2).max(1);
        let mut mip = Vec::with_capacity((new_w * new_h * 4) as usize);
        for y in 0..new_h {
            for x in 0..new_w {
                let mut rgba = [0u32; 4];
                let mut count = 0u32;
                for sy in 0..2u32 {
                    for sx in 0..2u32 {
                        let px = (x * 2 + sx).min(prev_w - 1);
                        let py = (y * 2 + sy).min(prev_h - 1);
                        let idx = prev_offset + (py * prev_w + px) as usize * 4;
                        if idx + 3 < data.len() {
                            rgba[0] += data[idx] as u32;
                            rgba[1] += data[idx + 1] as u32;
                            rgba[2] += data[idx + 2] as u32;
                            rgba[3] += data[idx + 3] as u32;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    mip.push((rgba[0] / count) as u8);
                    mip.push((rgba[1] / count) as u8);
                    mip.push((rgba[2] / count) as u8);
                    mip.push((rgba[3] / count) as u8);
                } else {
                    mip.extend_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        prev_offset = data.len();
        data.extend_from_slice(&mip);
        prev_w = new_w;
        prev_h = new_h;
    }
    image.texture_descriptor.mip_level_count = mip_count;
}

/// Load a material texture from disk, skipping cubemaps and other non-2D textures.
fn load_material_image(
    path: &str,
    assets_dir: &Path,
    images: &mut Assets<Image>,
    is_srgb: bool,
) -> Option<Handle<Image>> {
    let abs_path = assets_dir.join(path);
    // Pre-check: skip KTX2 cubemaps before decoding
    if abs_path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ktx2"))
        && is_ktx2_non_2d(&abs_path)
    {
        return None;
    }
    let bytes = std::fs::read(&abs_path).ok()?;
    let ext = abs_path.extension()?.to_str()?;
    let mut image = Image::from_buffer(
        &bytes,
        ImageType::Extension(ext),
        CompressedImageFormats::all(),
        is_srgb,
        ImageSampler::default(),
        RenderAssetUsages::default(),
    )
    .ok()?;
    // Also check decoded dimension as a safety net
    if image.texture_descriptor.dimension != TextureDimension::D2 {
        return None;
    }
    // Reject non-float formats (e.g. R16Uint) incompatible with StandardMaterial samplers
    let sample = image.texture_descriptor.format.sample_type(None, None);
    if !matches!(sample, Some(TextureSampleType::Float { .. })) {
        return None;
    }
    generate_cpu_mipmaps(&mut image);
    Some(images.add(image))
}

/// Combine standalone roughness and metallic greyscale images into a single
/// RGBA image in Bevy's expected format: G = roughness, B = metallic, R = 0, A = 255.
/// When `invert_roughness` is true, the roughness input is treated as a gloss map
/// and inverted (roughness = 255 - gloss).
fn combine_roughness_metallic(
    roughness: Option<&Image>,
    metallic: Option<&Image>,
    invert_roughness: bool,
) -> Image {
    let (width, height) = roughness
        .or(metallic)
        .map(|img| {
            (
                img.texture_descriptor.size.width,
                img.texture_descriptor.size.height,
            )
        })
        .unwrap_or((1, 1));

    let pixel_count = (width * height) as usize;
    let mut data = vec![0u8; pixel_count * 4];

    for i in 0..pixel_count {
        let r_raw = roughness
            .and_then(|img| greyscale_pixel(img, i))
            .unwrap_or(255);
        let r_val = if invert_roughness { 255 - r_raw } else { r_raw };
        let m_val = metallic
            .and_then(|img| greyscale_pixel(img, i))
            .unwrap_or(0);
        data[i * 4] = 0; // R - unused
        data[i * 4 + 1] = r_val; // G - roughness
        data[i * 4 + 2] = m_val; // B - metallic
        data[i * 4 + 3] = 255; // A
    }

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    generate_cpu_mipmaps(&mut image);
    image
}

/// Extract a greyscale value from pixel `index` of an image, regardless of format.
/// Returns the first channel (R) as a u8 approximation.
fn greyscale_pixel(img: &Image, index: usize) -> Option<u8> {
    let format = img.texture_descriptor.format;
    let block_size = format.block_copy_size(None)? as usize;
    let offset = index * block_size;
    let data = img.data.as_ref()?;
    if offset >= data.len() {
        return None;
    }
    // For common 8-bit formats, first byte is the value we want
    Some(data[offset])
}

pub(super) fn ensure_material_definitions(
    brushes: Query<&super::Brush>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<MaterialDefinitionCache>,
    library: Res<MaterialLibrary>,
    project_root: Option<Res<crate::project::ProjectRoot>>,
) {
    let assets_dir = project_root
        .map(|p| p.assets_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("assets"));

    let mut names_to_load: Vec<String> = Vec::new();

    // Pre-cache all library materials (for browser thumbnails/previews)
    for def in &library.materials {
        if !cache.entries.contains_key(&def.name) && !names_to_load.contains(&def.name) {
            names_to_load.push(def.name.clone());
        }
    }

    // Also check brush faces for materials not yet in cache
    for brush in &brushes {
        for face in &brush.faces {
            if let Some(ref name) = face.material_name {
                if !cache.entries.contains_key(name) && !names_to_load.contains(name) {
                    names_to_load.push(name.clone());
                }
            }
        }
    }

    for name in names_to_load {
        let Some(def) = library.get_by_name(&name) else {
            continue;
        };

        let base_color_image = def
            .base_color_texture
            .as_ref()
            .and_then(|p| load_material_image(p, &assets_dir, &mut images, true));
        let base_color_texture = base_color_image.clone();
        let normal_map_texture = def
            .normal_map_texture
            .as_ref()
            .and_then(|p| load_material_image(p, &assets_dir, &mut images, false));
        let mut metallic_roughness_texture = def
            .metallic_roughness_texture
            .as_ref()
            .and_then(|p| load_material_image(p, &assets_dir, &mut images, false));

        // Synthesize combined metallic-roughness from standalone textures if needed
        if metallic_roughness_texture.is_none()
            && (def.roughness_texture.is_some() || def.metallic_texture.is_some())
        {
            let roughness_img = def
                .roughness_texture
                .as_ref()
                .and_then(|p| load_material_image(p, &assets_dir, &mut images, false))
                .and_then(|h| images.get(&h).cloned());
            let metallic_img = def
                .metallic_texture
                .as_ref()
                .and_then(|p| load_material_image(p, &assets_dir, &mut images, false))
                .and_then(|h| images.get(&h).cloned());
            let combined = combine_roughness_metallic(
                roughness_img.as_ref(),
                metallic_img.as_ref(),
                def.is_gloss_map,
            );
            metallic_roughness_texture = Some(images.add(combined));
        }

        let emissive_texture = def
            .emissive_texture
            .as_ref()
            .and_then(|p| load_material_image(p, &assets_dir, &mut images, true));
        let occlusion_texture = def
            .occlusion_texture
            .as_ref()
            .and_then(|p| load_material_image(p, &assets_dir, &mut images, false));
        let depth_map = def
            .depth_texture
            .as_ref()
            .and_then(|p| load_material_image(p, &assets_dir, &mut images, false));

        let [r, g, b, a] = def.base_color;
        let has_metallic_roughness_texture = metallic_roughness_texture.is_some();
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(r, g, b, a),
            base_color_texture,
            normal_map_texture,
            metallic_roughness_texture,
            emissive_texture,
            occlusion_texture,
            depth_map,
            metallic: if has_metallic_roughness_texture { 1.0 } else { def.metallic },
            perceptual_roughness: if has_metallic_roughness_texture { 1.0 } else { def.perceptual_roughness },
            reflectance: def.reflectance,
            emissive: if def.emissive_intensity > 0.0 {
                LinearRgba::WHITE * def.emissive_intensity
            } else {
                LinearRgba::BLACK
            },
            double_sided: def.double_sided,
            flip_normal_map_y: def.flip_normal_map_y,
            ..default()
        });

        cache.entries.insert(
            name,
            MaterialDefCacheEntry {
                material,
                preview_image: None,
                base_color_image,
            },
        );
    }
}

/// Set repeat wrapping mode on material definition texture images once they finish loading.
pub(super) fn set_material_def_repeat_mode(
    cache: Res<MaterialDefinitionCache>,
    materials: Res<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut done: Local<HashSet<String>>,
) {
    for (name, entry) in &cache.entries {
        if done.contains(name) {
            continue;
        }
        let Some(mat) = materials.get(&entry.material) else {
            continue;
        };
        let mut all_loaded = true;
        for tex_handle in [
            &mat.base_color_texture,
            &mat.normal_map_texture,
            &mat.metallic_roughness_texture,
            &mat.emissive_texture,
            &mat.occlusion_texture,
            &mat.depth_map,
        ]
        .into_iter()
        .flatten()
        {
            if let Some(image) = images.get_mut(tex_handle) {
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    anisotropy_clamp: 16,
                    ..ImageSamplerDescriptor::linear()
                });
            } else {
                all_loaded = false;
            }
        }
        if all_loaded {
            done.insert(name.clone());
        }
    }
}

/// Set repeat wrapping mode on brush texture images once they finish loading.
pub(super) fn set_texture_repeat_mode(
    cache: Res<TextureMaterialCache>,
    mut images: ResMut<Assets<Image>>,
    mut done: Local<HashSet<String>>,
) {
    for (path, entry) in &cache.entries {
        if done.contains(path) {
            continue;
        }
        if let Some(image) = images.get_mut(&entry.image) {
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                anisotropy_clamp: 16,
                ..ImageSamplerDescriptor::linear()
            });
            done.insert(path.clone());
        }
    }
}

pub fn regenerate_brush_meshes(
    mut commands: Commands,
    changed_brushes: Query<
        (
            Entity,
            &super::Brush,
            Option<&Children>,
            Option<&super::BrushPreview>,
        ),
        Changed<super::Brush>,
    >,
    mesh3d_query: Query<(), With<Mesh3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<BrushMaterialPalette>,
    texture_cache: Res<TextureMaterialCache>,
    mat_def_cache: Res<MaterialDefinitionCache>,
) {
    for (entity, brush, children, preview) in &changed_brushes {
        // Despawn all Mesh3d children — covers both BrushFaceEntity children
        // from previous regen cycles and the runtime mesh child from JsnPlugin.
        if let Some(children) = children {
            for child in children.iter() {
                if mesh3d_query.get(child).is_ok() {
                    if let Ok(mut ec) = commands.get_entity(child) {
                        ec.despawn();
                    }
                }
            }
        }

        let (vertices, face_polygons) = compute_brush_geometry(&brush.faces);

        let mut face_entities = Vec::with_capacity(brush.faces.len());

        for (face_idx, face_data) in brush.faces.iter().enumerate() {
            let indices = &face_polygons[face_idx];
            if indices.len() < 3 {
                // Degenerate face, spawn nothing but track the slot
                face_entities.push(Entity::PLACEHOLDER);
                continue;
            }

            // Build per-face mesh with local vertex positions
            let positions: Vec<[f32; 3]> =
                indices.iter().map(|&vi| vertices[vi].to_array()).collect();
            let normals: Vec<[f32; 3]> = vec![face_data.plane.normal.to_array(); indices.len()];
            let uvs = compute_face_uvs(
                &vertices,
                indices,
                face_data.plane.normal,
                face_data.uv_offset,
                face_data.uv_scale,
                face_data.uv_rotation,
            );

            // Fan triangulate — local indices (0..positions.len())
            let local_tris = triangulate_face(&(0..indices.len()).collect::<Vec<_>>());
            let flat_indices: Vec<u32> =
                local_tris.iter().flat_map(|t| t.iter().copied()).collect();

            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_indices(Indices::U32(flat_indices));

            let mesh_handle = meshes.add(mesh);

            let material = match (&face_data.material_name, &face_data.texture_path) {
                (Some(name), _) => mat_def_cache
                    .entries
                    .get(name)
                    .map(|e| e.material.clone())
                    .unwrap_or_else(|| palette.materials[0].clone()),
                (None, Some(path)) => texture_cache
                    .entries
                    .get(path)
                    .map(|e| e.material.clone())
                    .unwrap_or_else(|| palette.materials[0].clone()),
                (None, None) => {
                    let mats = if preview.is_some() {
                        &palette.preview_materials
                    } else {
                        &palette.materials
                    };
                    mats.get(face_data.material_index)
                        .cloned()
                        .unwrap_or_else(|| mats[0].clone())
                }
            };

            let face_entity = commands
                .spawn((
                    BrushFaceEntity {
                        brush_entity: entity,
                        face_index: face_idx,
                    },
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material),
                    Transform::default(),
                    ChildOf(entity),
                ))
                .id();

            face_entities.push(face_entity);
        }

        commands.entity(entity).insert(BrushMeshCache {
            vertices,
            face_polygons,
            face_entities,
        });
    }
}

/// Reads interaction state each frame and inserts/removes `BrushPreview` on the
/// appropriate brush entity so downstream systems can swap materials.
pub(super) fn sync_brush_preview(
    mut commands: Commands,
    face_drag: Res<super::BrushDragState>,
    vertex_drag: Res<super::VertexDragState>,
    edge_drag: Res<super::EdgeDragState>,
    draw_state: Res<DrawBrushState>,
    selection: Res<super::BrushSelection>,
    existing: Query<Entity, With<BrushPreview>>,
) {
    let preview_entity = if face_drag.active || vertex_drag.active || edge_drag.active {
        selection.entity
    } else if let Some(ref active) = draw_state.active {
        active.append_target
    } else {
        None
    };

    for entity in &existing {
        if Some(entity) != preview_entity {
            commands.entity(entity).remove::<BrushPreview>();
        }
    }

    if let Some(entity) = preview_entity {
        if existing.get(entity).is_err() {
            commands.entity(entity).insert(BrushPreview);
        }
    }
}

/// When `BrushPreview` is added or removed, swap materials on existing face entities
/// without requiring a full mesh regeneration.
pub(super) fn apply_brush_preview_materials(
    mut commands: Commands,
    palette: Res<BrushMaterialPalette>,
    added: Query<(Entity, &BrushMeshCache), Added<BrushPreview>>,
    mut removed: RemovedComponents<BrushPreview>,
    brush_query: Query<&BrushMeshCache>,
    face_query: Query<&BrushFaceEntity>,
    brush_data: Query<&super::Brush>,
) {
    for (entity, cache) in &added {
        swap_face_materials(
            &mut commands,
            entity,
            cache,
            &palette.preview_materials,
            &face_query,
            &brush_data,
        );
    }

    for entity in removed.read() {
        if let Ok(cache) = brush_query.get(entity) {
            swap_face_materials(
                &mut commands,
                entity,
                cache,
                &palette.materials,
                &face_query,
                &brush_data,
            );
        }
    }
}

fn swap_face_materials(
    commands: &mut Commands,
    brush_entity: Entity,
    cache: &BrushMeshCache,
    target_materials: &[Handle<StandardMaterial>],
    face_query: &Query<&BrushFaceEntity>,
    brush_data: &Query<&super::Brush>,
) {
    let Ok(brush) = brush_data.get(brush_entity) else {
        return;
    };

    for &face_entity in &cache.face_entities {
        if face_entity == Entity::PLACEHOLDER {
            continue;
        }
        let Ok(face) = face_query.get(face_entity) else {
            continue;
        };
        let Some(face_data) = brush.faces.get(face.face_index) else {
            continue;
        };
        // Only swap untextured/unmaterialed faces
        if face_data.texture_path.is_some() || face_data.material_name.is_some() {
            continue;
        }
        let mat = target_materials
            .get(face_data.material_index)
            .cloned()
            .unwrap_or_else(|| target_materials[0].clone());
        commands.entity(face_entity).insert(MeshMaterial3d(mat));
    }
}
