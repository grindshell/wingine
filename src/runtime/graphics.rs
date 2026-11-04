//! The `graphics` table, for drawing.

use std::path::Path;

use mlua::{ErrorContext, Lua, Table, UserDataRef, Vector};

use super::{Lent, Textures};
use crate::{
    Context,
    graphics::{
        Animation, Camera, Camera3d, Canvas, Color, Font, Lighting, Mesh, MeshData, MeshVertex,
        Sprite, SpriteSheet, TextLayout, View3d,
    },
    lua::{SceneNode, check_layer, packed_color, vector3},
    math::{Affine3, Rect},
    tilemap::Tilemap,
};

/// The arguments of `graphics.draw_sprite_rotated`: the sprite, where it goes and its angle, then
/// optionally its tint and size.
type RotatedSpriteArgs = (
    UserDataRef<Sprite>,
    f32,
    f32,
    f32,
    Option<Color>,
    Option<f32>,
    Option<f32>,
);

/// The arguments of `graphics.outline_rect` and `graphics.draw_line`: four numbers, the color,
/// and optionally the thickness.
type ShapeArgs = (f32, f32, f32, f32, Color, Option<f32>);

/// The table, loading files relative to `folder`, and images through `textures`.
pub(super) fn module(
    lua: &Lua,
    lent: &Lent,
    folder: &Path,
    textures: Textures,
    default_font: Font,
) -> mlua::Result<Table> {
    let graphics = lua.create_table()?;
    graphics.set("clear", {
        let lent = lent.clone();
        lua.create_function(move |_, color: Color| lent.canvas(|canvas| canvas.clear(color)))?
    })?;
    graphics.set("fill_rect", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (x, y, width, height, color): (f32, f32, f32, f32, Color)| {
                lent.canvas(|canvas| canvas.fill_rect(Rect::new(x, y, width, height), color))
            },
        )?
    })?;
    graphics.set("outline_rect", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (x, y, width, height, color, thickness): ShapeArgs| {
                let rect = Rect::new(x, y, width, height);
                let thickness = thickness.unwrap_or(1.0);
                lent.canvas(|canvas| canvas.outline_rect(rect, thickness, color))
            },
        )?
    })?;
    graphics.set("draw_line", {
        let lent = lent.clone();
        lua.create_function(move |_, (x1, y1, x2, y2, color, thickness): ShapeArgs| {
            let thickness = thickness.unwrap_or(1.0);
            lent.canvas(|canvas| canvas.draw_line([x1, y1], [x2, y2], thickness, color))
        })?
    })?;
    graphics.set("fill_circle", {
        let lent = lent.clone();
        lua.create_function(move |_, (x, y, radius, color): (f32, f32, f32, Color)| {
            lent.canvas(|canvas| canvas.fill_circle([x, y], radius, color))
        })?
    })?;
    graphics.set("outline_circle", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (x, y, radius, color, thickness): (f32, f32, f32, Color, Option<f32>)| {
                let thickness = thickness.unwrap_or(1.0);
                lent.canvas(|canvas| canvas.outline_circle([x, y], radius, thickness, color))
            },
        )?
    })?;
    graphics.set("draw_sprite", {
        let lent = lent.clone();
        lua.create_function(
            move |_,
                  (sprite, x, y, tint, width, height): (
                UserDataRef<Sprite>,
                f32,
                f32,
                Option<Color>,
                Option<f32>,
                Option<f32>,
            )| {
                let width = width.unwrap_or(sprite.width());
                let height = height.unwrap_or(sprite.height());
                let tint = tint.unwrap_or(Color::WHITE);
                lent.canvas(|canvas| {
                    canvas.draw_sprite(&sprite, Rect::new(x, y, width, height), tint)
                })
            },
        )?
    })?;
    graphics.set("draw_sprite_rotated", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (sprite, x, y, angle, tint, width, height): RotatedSpriteArgs| {
                let width = width.unwrap_or(sprite.width());
                let height = height.unwrap_or(sprite.height());
                let tint = tint.unwrap_or(Color::WHITE);
                lent.canvas(|canvas| {
                    let dest = Rect::new(x, y, width, height);
                    canvas.draw_sprite_rotated(&sprite, dest, angle, tint)
                })
            },
        )?
    })?;
    graphics.set("draw_text", {
        let lent = lent.clone();
        lua.create_function(
            move |_,
                  (font, text, x, y, color): (
                UserDataRef<Font>,
                mlua::LuaString,
                f32,
                f32,
                Color,
            )| {
                let text = text.to_str()?;
                lent.canvas(|canvas| canvas.draw_text(&font, &text, [x, y], color))
            },
        )?
    })?;
    graphics.set("draw_text_layout", {
        let lent = lent.clone();
        lua.create_function(
            move |_,
                  (layout, x, y, color, glyphs): (
                UserDataRef<TextLayout>,
                f32,
                f32,
                Color,
                Option<f32>,
            )| {
                // Counts of revealed glyphs are often fractional; `as` rounds down, and NaN is 0.
                let shown = glyphs.map_or(usize::MAX, |glyphs| glyphs.max(0.0) as usize);
                lent.canvas(|canvas| canvas.draw_text_layout(&layout, ..shown, [x, y], color))
            },
        )?
    })?;
    graphics.set("draw_tilemap", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (map, layer): (UserDataRef<Tilemap>, Option<usize>)| {
                if let Some(layer) = layer {
                    check_layer(&map, layer)?;
                }
                lent.canvas(|canvas| match layer {
                    Some(layer) => map.draw_layer(canvas, layer),
                    None => map.draw(canvas),
                })
            },
        )?
    })?;
    graphics.set("draw_scene", {
        let lent = lent.clone();
        lua.create_function(move |_, node: UserDataRef<SceneNode>| {
            node.write(|scene, id| lent.canvas(|canvas| scene.draw_node(canvas, id)))?
        })?
    })?;
    graphics.set("set_camera", {
        let lent = lent.clone();
        lua.create_function(
            move |_,
                  (x, y, bounds, zoom): (
                Option<f32>,
                Option<f32>,
                Option<Rect>,
                Option<f32>,
            )| {
                let zoom = zoom.unwrap_or(1.0);
                // Not `zoom <= 0.0`, which lets NaN through.
                if !(zoom > 0.0 && zoom.is_finite()) {
                    let message = format!("set_camera's zoom must be above 0, not {zoom}");
                    return Err(mlua::Error::runtime(message));
                }
                let camera = match (x, y) {
                    (Some(x), Some(y)) => Some(Camera {
                        zoom,
                        bounds,
                        ..Camera::new(x, y)
                    }),
                    (None, None) => None,
                    _ => {
                        let message = "set_camera takes a position, x and y, or nil for none";
                        return Err(mlua::Error::runtime(message));
                    }
                };
                lent.canvas(|canvas| canvas.set_camera(camera))
            },
        )?
    })?;
    graphics.set("size", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| {
            let size = |canvas: &mut Canvas| (canvas.width(), canvas.height());
            match lent.with(size)? {
                Some(size) => Ok(size),
                None => lent.context(|ctx| ctx.canvas_size().into()),
            }
        })?
    })?;
    graphics.set(
        "color",
        lua.create_function(|_, color: Color| Ok(packed_color(color)))?,
    )?;
    graphics.set(
        "default_font",
        lua.create_function(move |_, ()| Ok(default_font.clone()))?,
    )?;
    graphics.set("frame_stats", {
        let lent = lent.clone();
        lua.create_function(move |lua, ()| {
            // The canvas has them while the game draws, and the context the rest of the time.
            let stats = match lent.with(|canvas: &mut Canvas| canvas.frame_stats)? {
                Some(stats) => stats,
                None => lent
                    .with(|ctx: &mut Context| ctx.frame_stats())?
                    .unwrap_or_default(),
            };
            let table = lua.create_table()?;
            table.set("frame", stats.frame)?;
            table.set("update", stats.update)?;
            table.set("draw", stats.draw)?;
            table.set("render", stats.render)?;
            table.set("quads", stats.quads)?;
            table.set("triangles", stats.triangles)?;
            table.set("draw_calls", stats.draw_calls)?;
            Ok(table)
        })?
    })?;
    add_3d(lua, &graphics, lent)?;
    graphics.set(
        "new_animation",
        lua.create_function(|_, (frames, frame_time): (Vec<UserDataRef<Sprite>>, f32)| {
            let frames = frames.iter().map(|sprite| Sprite::clone(sprite)).collect();
            Animation::try_new(frames, frame_time).map_err(mlua::Error::runtime)
        })?,
    )?;
    graphics.set("load_sprite_sheet", {
        let lent = lent.clone();
        let root = folder.to_owned();
        lua.create_function(
            move |_, (path, tile_width, tile_height): (String, u32, u32)| {
                let path = root.join(path);
                // Through the context even when loaded already, so this works at the same times
                // either way.
                let texture = lent
                    .context(|ctx| textures.load(ctx, path))?
                    .map_err(mlua::Error::external)?;
                SpriteSheet::try_new(texture, tile_width, tile_height).map_err(mlua::Error::runtime)
            },
        )?
    })?;
    graphics.set_readonly(true);
    Ok(graphics)
}

/// Adds the functions for 3D views and meshes to `graphics`.
fn add_3d(lua: &Lua, graphics: &Table, lent: &Lent) -> mlua::Result<()> {
    graphics.set("begin_3d", {
        let lent = lent.clone();
        lua.create_function(move |_, view: Table| {
            let (camera, rect, lighting) = view_parts(&view)?;
            lent.canvas(|canvas| {
                let rect = rect.unwrap_or_else(|| canvas.visible_area());
                canvas.begin_3d(View3d {
                    camera,
                    rect,
                    lighting,
                });
            })
        })?
    })?;
    graphics.set("draw_mesh", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (mesh, position, options): (UserDataRef<Mesh>, Vector, Option<Table>)| {
                let options = MeshOptions::read(options)?;
                let transform = options.transform(vector3(position));
                let sprite = options.sprite.as_deref();
                lent.canvas(|canvas| {
                    let mut view = canvas.open_view()?;
                    view.draw_mesh(&mesh, transform, sprite, options.tint);
                    Some(())
                })?
                .ok_or_else(|| no_view("draw_mesh"))
            },
        )?
    })?;
    graphics.set("draw_billboard", {
        let lent = lent.clone();
        lua.create_function(
            move |_,
                  (sprite, position, tint, width, height): (
                UserDataRef<Sprite>,
                Vector,
                Option<Color>,
                Option<f32>,
                Option<f32>,
            )| {
                let aspect = sprite.width() / sprite.height();
                let size = match (width, height) {
                    (Some(width), Some(height)) => [width, height],
                    (Some(width), None) => [width, width.abs() / aspect],
                    (None, Some(height)) => [height * aspect, height],
                    (None, None) => [aspect, 1.0],
                };
                let tint = tint.unwrap_or(Color::WHITE);
                lent.canvas(|canvas| {
                    let mut view = canvas.open_view()?;
                    view.draw_billboard(&sprite, vector3(position), size, tint);
                    Some(())
                })?
                .ok_or_else(|| no_view("draw_billboard"))
            },
        )?
    })?;
    graphics.set("project", {
        let lent = lent.clone();
        lua.create_function(move |_, (view, point): (Table, Vector)| {
            let (camera, rect, _) = view_parts(&view)?;
            let rect = view_rect(&lent, rect)?;
            let shown = camera.project(vector3(point), [rect.w, rect.h]);
            Ok(match shown {
                Some([x, y]) => (Some(rect.x + x), Some(rect.y + y)),
                None => (None, None),
            })
        })?
    })?;
    graphics.set("ray", {
        let lent = lent.clone();
        lua.create_function(move |_, (view, x, y): (Table, f32, f32)| {
            let (camera, rect, _) = view_parts(&view)?;
            let rect = view_rect(&lent, rect)?;
            let ray = camera.ray([x - rect.x, y - rect.y], [rect.w, rect.h]);
            let [origin, direction] =
                [ray.origin, ray.direction].map(|[x, y, z]| Vector::new(x, y, z));
            Ok((origin, direction))
        })?
    })?;

    graphics.set("new_mesh", {
        let lent = lent.clone();
        lua.create_function(move |_, data: Table| {
            let data = mesh_data(&data)?;
            create_mesh(&lent, &data)
        })?
    })?;
    graphics.set("new_cube", {
        let lent = lent.clone();
        lua.create_function(move |_, size: f32| {
            check_sizes(&[size])?;
            create_mesh(&lent, &MeshData::cube(size))
        })?
    })?;
    graphics.set("new_cuboid", {
        let lent = lent.clone();
        lua.create_function(move |_, (width, height, depth): (f32, f32, f32)| {
            check_sizes(&[width, height, depth])?;
            create_mesh(&lent, &MeshData::cuboid([width, height, depth]))
        })?
    })?;
    graphics.set("new_plane", {
        let lent = lent.clone();
        lua.create_function(move |_, (width, depth): (f32, f32)| {
            check_sizes(&[width, depth])?;
            create_mesh(&lent, &MeshData::plane([width, depth]))
        })?
    })?;
    graphics.set("new_sphere", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (radius, segments, rings): (f32, Option<u32>, Option<u32>)| {
                check_sizes(&[radius])?;
                let data = MeshData::sphere(radius, segments.unwrap_or(16), rings.unwrap_or(8));
                create_mesh(&lent, &data)
            },
        )?
    })?;
    Ok(())
}

/// The camera, rect (if it has one) and lighting of a table describing a 3D view.
fn view_parts(view: &Table) -> mlua::Result<(Camera3d, Option<Rect>, Lighting)> {
    let camera = view.get("camera").context("the view's camera")?;
    let rect = view.get("rect").context("the view's rect")?;
    let lighting: Option<Lighting> = view.get("lighting").context("the view's lighting")?;
    Ok((camera, rect, lighting.unwrap_or_default()))
}

/// `rect`, or without one, the whole canvas, in the coordinates drawing uses.
fn view_rect(lent: &Lent, rect: Option<Rect>) -> mlua::Result<Rect> {
    if let Some(rect) = rect {
        return Ok(rect);
    }
    match lent.with(|canvas: &mut Canvas| canvas.visible_area())? {
        Some(area) => Ok(area),
        None => {
            let [width, height] = lent.context(|ctx| ctx.canvas_size())?;
            Ok(Rect::new(0.0, 0.0, width, height))
        }
    }
}

/// The error for drawing in a 3D view when none is open.
fn no_view(function: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "{function} draws in a 3D view: call graphics.begin_3d first, then draw what's in the view \
         before anything else, which ends it"
    ))
}

/// How `graphics.draw_mesh` places and colors a mesh.
struct MeshOptions {
    yaw: f32,
    pitch: f32,
    roll: f32,
    scale: [f32; 3],
    sprite: Option<UserDataRef<Sprite>>,
    tint: Color,
}

impl MeshOptions {
    fn read(options: Option<Table>) -> mlua::Result<Self> {
        let Some(options) = options else {
            return Ok(Self {
                yaw: 0.0,
                pitch: 0.0,
                roll: 0.0,
                scale: [1.0; 3],
                sprite: None,
                tint: Color::WHITE,
            });
        };
        let angle = |name: &str| -> mlua::Result<f32> {
            Ok(options.get::<Option<f32>>(name)?.unwrap_or(0.0))
        };
        let scale = match options.get::<mlua::Value>("scale")? {
            mlua::Value::Nil => [1.0; 3],
            mlua::Value::Vector(scale) => vector3(scale),
            mlua::Value::Number(scale) => [scale as f32; 3],
            mlua::Value::Integer(scale) => [scale as f32; 3],
            _ => {
                let message = "a mesh's scale is a number, or a vector to scale each axis";
                return Err(mlua::Error::runtime(message));
            }
        };
        Ok(Self {
            yaw: angle("yaw")?,
            pitch: angle("pitch")?,
            roll: angle("roll")?,
            scale,
            sprite: options.get("sprite")?,
            tint: options
                .get::<Option<Color>>("tint")?
                .unwrap_or(Color::WHITE),
        })
    }

    /// Scales, rolls about `z`, pitches about `x` and turns about `y`, then moves to `position`.
    fn transform(&self, position: [f32; 3]) -> Affine3 {
        Affine3::from_translation(position)
            * Affine3::from_rotation_y(self.yaw)
            * Affine3::from_rotation_x(self.pitch)
            * Affine3::from_rotation_z(self.roll)
            * Affine3::from_scale(self.scale)
    }
}

/// The mesh a table `{ vertices, indices }` describes, its indices counting from 1.
fn mesh_data(data: &Table) -> mlua::Result<MeshData> {
    let vertices: Vec<Table> = data.get("vertices").context("the mesh's vertices")?;
    let vertices = vertices
        .iter()
        .enumerate()
        .map(|(i, vertex)| {
            let read = || -> mlua::Result<MeshVertex> {
                let position: Vector = vertex.get("position")?;
                let normal: Option<Vector> = vertex.get("normal")?;
                Ok(MeshVertex {
                    position: vector3(position),
                    normal: normal.map_or([0.0, 1.0, 0.0], vector3),
                    uv: [
                        vertex.get::<Option<f32>>("u")?.unwrap_or(0.0),
                        vertex.get::<Option<f32>>("v")?.unwrap_or(0.0),
                    ],
                    color: vertex
                        .get::<Option<Color>>("color")?
                        .unwrap_or(Color::WHITE),
                })
            };
            read().with_context(|_| format!("vertex {}", i + 1))
        })
        .collect::<mlua::Result<Vec<_>>>()?;
    let indices: Vec<f64> = data.get("indices").context("the mesh's indices")?;
    let indices = indices
        .into_iter()
        .map(|index| {
            // Counting from 1, as Luau's tables do.
            if index.fract() == 0.0 && index >= 1.0 && index <= vertices.len() as f64 {
                Ok(index as u32 - 1)
            } else {
                Err(mlua::Error::runtime(format!(
                    "index {index} isn't a vertex: they count from 1 to {}",
                    vertices.len()
                )))
            }
        })
        .collect::<mlua::Result<Vec<_>>>()?;
    Ok(MeshData { vertices, indices })
}

fn create_mesh(lent: &Lent, data: &MeshData) -> mlua::Result<Mesh> {
    lent.context(|ctx| ctx.create_mesh(data))?
        .map_err(|err| mlua::Error::runtime(err.to_string()))
}

/// Fails unless every size is a finite number.
fn check_sizes(sizes: &[f32]) -> mlua::Result<()> {
    match sizes.iter().find(|size| !size.is_finite()) {
        Some(size) => Err(mlua::Error::runtime(format!(
            "a mesh's sizes must be finite, not {size}"
        ))),
        None => Ok(()),
    }
}
