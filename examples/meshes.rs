//! Meshes in a 3D view: crates, a post and a bouncing ball on a field of grass, with a table and
//! barrels loaded from model files, lit by the sun and seen through a camera circling them. The
//! view renders at the canvas's 320x180 pixels, so it keeps the pixel art's look, and a 2D HUD
//! goes over it, with a label that follows the ball.
//!
//! The post flies a flag, a mesh made once and written again every update with its corners where
//! the wind has them (`Context::write_mesh`), which costs less than making a mesh of them anew.
//! Its colors are a texture the example works out, pixel by pixel (`Context::create_texture`),
//! where the grass's is read from a file.
//!
//! Under the ball is a puddle, the one mesh here that blends with what's behind it
//! (`Canvas3d::set_blend`): the grass shows through it, where every other mesh is opaque.
//!
//! The field is a mesh built by hand, a quad for each tile, shaded a little lighter or darker by
//! its corners' colors. It's drawn twice: plain, as the ground, then a little above it showing the
//! grass sprite, whose transparent background shows the ground through. The table and the barrels
//! are models made in Blender (`examples/models`), drawn a draw call for each of their parts.
//!
//! Left and Right (or A and D) turn the camera round faster either way. Run with
//! `cargo run --example meshes`.

use std::f32::consts::TAU;

use wingine::{
    Context, FrameStats, Game, WindowConfig,
    graphics::{
        Blend, Camera3d, Canvas, Canvas3d, Color, Font, Mesh, MeshData, MeshVertex, Model,
        Resolution, Sprite, SpriteSheet, View3d,
    },
    input::Key,
    math::{Affine3, Rect},
};

const RESOLUTION: Resolution = Resolution::Fixed {
    width: 320,
    height: 180,
};
const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);
const TABLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/models/table.glb");
const BARREL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/models/barrel.glb");
/// Where the barrels stand, and how far each is turned.
const BARRELS: [([f32; 3], f32); 2] = [([-2.6, 0.0, 0.2], 0.0), ([-3.0, 0.0, -0.6], 0.7)];
/// How many tiles the field is across, each 1 world unit.
const FIELD: u32 = 8;
/// How fast the camera circles, in radians a second, and how much faster it goes turning.
const ORBIT_SPEED: f32 = 0.3;
const TURN_SPEED: f32 = 1.5;
/// How far the camera is from the middle, and how high.
const CAMERA_DISTANCE: f32 = 6.0;
const CAMERA_HEIGHT: f32 = 3.5;
/// How many times a second the ball bounces, and how high.
const BOUNCES: f32 = 0.8;
const BOUNCE_HEIGHT: f32 = 1.2;
const BALL_RADIUS: f32 = 0.4;
/// How wide and deep the puddle under the ball is.
const PUDDLE_SIZE: [f32; 2] = [1.5, 1.1];
/// Where the post stands, by its middle, and how thick and tall it is.
const POST: [f32; 3] = [-2.0, 1.0, 1.5];
const POST_SIZE: [f32; 3] = [0.25, 2.0, 0.25];
/// The flag: how many quads it is across and down, how long and tall it is, how far its fly
/// ripples either way, how many waves it holds, and how many pass along it a second.
const FLAG_QUADS: [u32; 2] = [8, 3];
const FLAG_SIZE: [f32; 2] = [1.2, 0.7];
const FLAG_RIPPLE: f32 = 0.14;
const FLAG_WAVES: f32 = 1.25;
const FLAG_SPEED: f32 = 0.9;

struct Meshes {
    field: Mesh,
    cube: Mesh,
    post: Mesh,
    /// Written every update, as it flies.
    flag: Mesh,
    ball: Mesh,
    /// Flat on the grass under the ball, and drawn blended.
    puddle: Mesh,
    table: Model,
    barrel: Model,
    grass: Sprite,
    /// What the flag shows, made of pixels worked out here.
    banner: Sprite,
    font: Font,
    /// The camera's angle round the middle, in radians.
    angle: f32,
    /// Seconds since the game began, which bounce the ball.
    time: f32,
    /// The frame before's, to show in the HUD.
    stats: FrameStats,
}

impl Meshes {
    fn new(ctx: &mut Context) -> wingine::Result<Self> {
        let input = ctx.input_mut();
        input.bind("left", [Key::Left, Key::A]);
        input.bind("right", [Key::Right, Key::D]);
        let sheet = SpriteSheet::new(ctx.load_texture(TILESET)?, 16, 16);
        let ([width, height], pixels) = banner();
        let banner = ctx.create_texture(width, height, &pixels)?.sprite();
        Ok(Self {
            field: ctx.create_mesh(&field())?,
            cube: ctx.create_mesh(&MeshData::cube(1.0))?,
            post: ctx.create_mesh(&MeshData::cuboid(POST_SIZE))?,
            flag: ctx.create_mesh(&flag(0.0))?,
            ball: ctx.create_mesh(&MeshData::sphere(BALL_RADIUS, 16, 8))?,
            puddle: ctx.create_mesh(&MeshData::plane(PUDDLE_SIZE))?,
            table: ctx.load_model(TABLE)?,
            barrel: ctx.load_model(BARREL)?,
            grass: sheet.sprite(5, 0),
            banner,
            font: ctx.default_font().clone(),
            angle: 0.6,
            time: 0.0,
            stats: FrameStats::default(),
        })
    }

    fn camera(&self) -> Camera3d {
        let (sin, cos) = self.angle.sin_cos();
        let position = [sin * CAMERA_DISTANCE, CAMERA_HEIGHT, cos * CAMERA_DISTANCE];
        Camera3d::perspective(position, [0.0, 0.5, 0.0], 0.8)
    }

    fn draw_world(&self, view: &mut Canvas3d) {
        let ground = Color::rgb8(0x4f, 0x8a, 0x3c);
        view.draw_mesh(&self.field, Affine3::IDENTITY, None, ground);
        let above = Affine3::from_translation([0.0, 0.01, 0.0]);
        let grass = Color::rgb8(0x8c, 0xd0, 0x5c);
        view.draw_mesh(&self.field, above, Some(&self.grass), grass);

        // Two crates side by side, one on top turned a little, and a post beside them.
        let wood = Color::rgb8(0xb0, 0x7a, 0x48);
        for transform in [
            Affine3::from_translation([-1.0, 0.5, -0.5]),
            Affine3::from_translation([0.1, 0.5, -0.7]) * Affine3::from_rotation_y(0.3),
            Affine3::from_translation([-0.5, 1.5, -0.6]) * Affine3::from_rotation_y(0.8),
        ] {
            view.draw_mesh(&self.cube, transform, None, wood);
        }
        let post = Affine3::from_translation(POST);
        view.draw_mesh(&self.post, post, None, Color::rgb8(0xe0, 0xe0, 0xd8));
        let hoist = Affine3::from_translation(flag_hoist());
        view.draw_mesh(&self.flag, hoist, Some(&self.banner), Color::WHITE);

        // The models, as they were made: a table across from the crates, and barrels by the post.
        let table = Affine3::from_translation([2.0, 0.0, -1.6]) * Affine3::from_rotation_y(-0.4);
        view.draw_model(&self.table, table, Color::WHITE);
        for (position, turn) in BARRELS {
            let barrel = Affine3::from_translation(position) * Affine3::from_rotation_y(turn);
            view.draw_model(&self.barrel, barrel, Color::WHITE);
        }

        let ball = Affine3::from_translation(self.ball_position());
        view.draw_mesh(&self.ball, ball, None, Color::rgb8(0xe0, 0x50, 0x40));

        // The puddle under it, as see-through as its tint: blended, it's drawn after everything
        // opaque, wherever it comes here, and hides nothing.
        view.set_blend(Some(Blend::Normal));
        let [x, _, z] = self.ball_position();
        let puddle = Affine3::from_translation([x, 0.02, z]);
        view.draw_mesh(
            &self.puddle,
            puddle,
            None,
            Color::rgba(0.2, 0.42, 0.85, 0.55),
        );
        view.set_blend(None);
    }

    /// Where the ball's middle is: bouncing on the spot, slowing toward the top of each bounce.
    fn ball_position(&self) -> [f32; 3] {
        let phase = (self.time * BOUNCES).fract();
        let height = 4.0 * phase * (1.0 - phase) * BOUNCE_HEIGHT;
        [1.5, BALL_RADIUS + height, 1.0]
    }
}

/// The field: a quad for each tile, facing up, each showing the whole sprite, alternately a
/// little lighter and darker.
fn field() -> MeshData {
    let mut mesh = MeshData::default();
    let half = FIELD as f32 / 2.0;
    for row in 0..FIELD {
        for column in 0..FIELD {
            let [x, z] = [column as f32 - half, row as f32 - half];
            let color = if (row + column) % 2 == 0 {
                Color::WHITE
            } else {
                Color::rgb(0.85, 0.85, 0.85)
            };
            let first = mesh.vertices.len() as u32;
            // From the corner showing the sprite's bottom-left, counter-clockwise seen from above.
            for (corner, uv) in [
                ([x, 0.0, z + 1.0], [0.0, 1.0]),
                ([x + 1.0, 0.0, z + 1.0], [1.0, 1.0]),
                ([x + 1.0, 0.0, z], [1.0, 0.0]),
                ([x, 0.0, z], [0.0, 0.0]),
            ] {
                mesh.vertices.push(MeshVertex {
                    color,
                    ..MeshVertex::new(corner, [0.0, 1.0, 0.0], uv)
                });
            }
            mesh.indices
                .extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
        }
    }
    mesh
}

/// Where the top of the flag's hoist is: up the side of the post, just under its top.
fn flag_hoist() -> [f32; 3] {
    let [x, y, z] = POST;
    [x + POST_SIZE[0] / 2.0, y + POST_SIZE[1] / 2.0 - 0.05, z]
}

/// What the flag shows: its size in texels, a texel for each tenth of a unit of the flag, and
/// their sRGB RGBA bytes, row by row from the top. A field of gold with a cross of blue, its
/// upright nearer the hoist.
fn banner() -> ([u32; 2], Vec<u8>) {
    const SIZE: [u32; 2] = [12, 7];
    let [gold, blue] = [[0xe8, 0xc0, 0x40, 0xff], [0x2c, 0x5a, 0xa0, 0xff]];
    let mut pixels = Vec::new();
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            let crossed = y == SIZE[1] / 2 || x == 3 || x == 4;
            pixels.extend(if crossed { blue } else { gold });
        }
    }
    (SIZE, pixels)
}

/// The flag as it flies at `time`, from the top of its hoist: quads facing `+z` and the same
/// again facing `-z`, since a mesh shows from one side, each corner pushed along `z` by a wave
/// running from the hoist, which stays on the post, and growing toward the fly. Its triangles
/// are between the same corners whenever it is, so one mesh is written with each in turn.
fn flag(time: f32) -> MeshData {
    let [columns, rows] = FLAG_QUADS;
    let [length, height] = FLAG_SIZE;
    let mut mesh = MeshData::default();
    for facing in [1.0, -1.0] {
        let first = mesh.vertices.len() as u32;
        for row in 0..=rows {
            for column in 0..=columns {
                let [along, down] = [column as f32 / columns as f32, row as f32 / rows as f32];
                let wave = TAU * (FLAG_WAVES * along - FLAG_SPEED * time);
                let (sin, cos) = wave.sin_cos();
                // How steeply the flag crosses `z` here, which its surface leans away from.
                let slope = FLAG_RIPPLE / length * (sin + TAU * FLAG_WAVES * along * cos);
                mesh.vertices.push(MeshVertex::new(
                    [along * length, -down * height, FLAG_RIPPLE * along * sin],
                    [-slope * facing, 0.0, facing],
                    [along, down],
                ));
            }
        }
        let across = columns + 1;
        for row in 0..rows {
            for column in 0..columns {
                let corner = first + row * across + column;
                // From the quad's bottom-left, counter-clockwise seen from `+z`.
                let quad = [corner + across, corner + across + 1, corner + 1, corner];
                let order = if facing > 0.0 {
                    [0, 1, 2, 0, 2, 3]
                } else {
                    [0, 2, 1, 0, 3, 2]
                };
                mesh.indices.extend(order.map(|i| quad[i]));
            }
        }
    }
    mesh
}

impl Game for Meshes {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        let turn = ctx.input().axis("left", "right");
        self.angle += (ORBIT_SPEED + turn * TURN_SPEED) * dt;
        self.time += dt;
        ctx.write_mesh(&self.flag, &flag(self.time).vertices)
            .expect("the flag has as many corners however it flies");
        self.stats = ctx.frame_stats();
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(Color::rgb8(0x8c, 0xc4, 0xe8));
        let rect = Rect::new(0.0, 0.0, canvas.width(), canvas.height());
        let camera = self.camera();
        self.draw_world(&mut canvas.begin_3d(View3d::new(camera, rect)));

        // The HUD, over the view: its shade first, so the text after it is drawn in one call.
        let shade = Color::rgba(0.0, 0.0, 0.0, 0.5);
        canvas.fill_rect(Rect::new(0.0, 0.0, canvas.width(), 14.0), shade);

        // A label over the ball, wherever it shows.
        let [x, y, z] = self.ball_position();
        let above = [x, y + BALL_RADIUS + 0.2, z];
        if let Some([x, y]) = camera.project(above, [rect.w, rect.h]) {
            let label = "ball";
            let width = self.font.measure(label)[0];
            canvas.draw_text(&self.font, label, [x - width / 2.0, y - 10.0], Color::WHITE);
        }

        let stats = format!(
            "{} triangles, {} draw calls",
            self.stats.triangles, self.stats.draw_calls
        );
        canvas.draw_text(&self.font, &stats, [4.0, 3.0], Color::WHITE);
        let hint = "Left/Right: turn";
        let width = self.font.measure(hint)[0];
        let corner = [canvas.width() - width - 4.0, canvas.height() - 12.0];
        canvas.draw_text(&self.font, hint, corner, Color::WHITE);
    }
}

fn config() -> WindowConfig {
    WindowConfig {
        title: "Wingine - meshes".into(),
        width: 960,
        height: 540,
        resolution: RESOLUTION,
        ..Default::default()
    }
}

fn main() -> wingine::Result<()> {
    env_logger::init();
    wingine::run(config(), Meshes::new)
}

/// Drives the example without a window.
#[cfg(test)]
mod tests {
    use wingine::headless::Simulation;

    use super::*;

    fn simulation() -> Simulation<Meshes> {
        Simulation::new(&config(), Meshes::new).expect("a GPU adapter is available")
    }

    #[test]
    fn draws_each_run_of_a_mesh_in_one_call() {
        let mut simulation = simulation();
        simulation.run_frames(2);
        let stats = simulation.game().stats;
        let game = simulation.game();
        let triangles = 2 * field().triangle_count()
            + 3 * MeshData::cube(1.0).triangle_count()
            + MeshData::cuboid(POST_SIZE).triangle_count()
            + flag(0.0).triangle_count()
            + MeshData::sphere(BALL_RADIUS, 16, 8).triangle_count()
            + MeshData::plane(PUDDLE_SIZE).triangle_count()
            + game.table.triangle_count() as usize
            + BARRELS.len() * game.barrel.triangle_count() as usize;
        assert_eq!(stats.triangles as usize, triangles);
        // The field, then the field again showing grass, the three crates, one mesh, together,
        // the post, the flag, the table's top, then its four legs together, the two barrels
        // together, the ball, and the puddle, blended, after them all, then the HUD: its
        // shade, then its text.
        assert_eq!(stats.draw_calls, 1 + 1 + 1 + 1 + 1 + 2 + 1 + 1 + 1 + 2);
    }

    #[test]
    fn the_flag_is_written_as_it_flies() {
        let mut simulation = simulation();
        simulation.run_frames(1);
        let early = simulation.game().flag.bounds().unwrap();
        simulation.run_frames(20);
        let game = simulation.game();
        let later = game.flag.bounds().unwrap();
        assert_eq!(Some(later), flag(game.time).bounds(), "as it flies now");
        assert_ne!(early, later, "it flew on");
        // Its hoist stays on the post, and its fly within its ripple.
        for bounds in [early, later] {
            assert_eq!([bounds.min[0], bounds.max[0]], [0.0, FLAG_SIZE[0]]);
            assert!(bounds.min[2] >= -FLAG_RIPPLE && bounds.max[2] <= FLAG_RIPPLE);
        }
        // Both faces lean as the surface does, each away from the other.
        let data = flag(game.time);
        let half = data.vertices.len() / 2;
        for (front, back) in data.vertices[..half].iter().zip(&data.vertices[half..]) {
            assert_eq!(front.position, back.position);
            assert_eq!(front.normal.map(|v| -v), back.normal);
        }
    }

    #[test]
    fn the_flag_shows_colors_worked_out_here() {
        let ([width, height], pixels) = banner();
        assert_eq!(pixels.len() as u32, width * height * 4);
        let simulation = simulation();
        let shown = &simulation.game().banner;
        assert_eq!(
            [shown.width(), shown.height()],
            [width as f32, height as f32]
        );
    }

    #[test]
    fn the_models_are_the_props_made_for_them() {
        let simulation = simulation();
        let game = simulation.game();
        // A top and four legs that share a mesh, each a box.
        assert_eq!(game.table.parts().len(), 5);
        assert_eq!(game.table.triangle_count(), 5 * 12);
        let bounds = game.barrel.bounds().unwrap();
        assert_eq!(bounds.min[1], 0.0, "standing on its origin");
        assert!((bounds.max[1] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn turning_moves_the_camera_round_faster() {
        let mut simulation = simulation();
        let start = simulation.game().angle;
        simulation.hold(Key::Right, 1.0);
        let right = simulation.game().angle - start;
        assert!(
            (right - (ORBIT_SPEED + TURN_SPEED)).abs() < 0.05,
            "turned {right}"
        );

        let start = simulation.game().angle;
        simulation.hold(Key::A, 1.0);
        let left = simulation.game().angle - start;
        assert!(
            (left - (ORBIT_SPEED - TURN_SPEED)).abs() < 0.05,
            "turned {left}"
        );
    }

    #[test]
    fn the_ball_bounces_on_the_ground() {
        let mut simulation = simulation();
        let mut lowest = f32::INFINITY;
        let mut highest = f32::NEG_INFINITY;
        for _ in 0..90 {
            simulation.run_frames(1);
            let [_, y, _] = simulation.game().ball_position();
            lowest = lowest.min(y);
            highest = highest.max(y);
        }
        assert!((lowest - BALL_RADIUS).abs() < 0.1, "lowest {lowest}");
        assert!(
            (highest - BALL_RADIUS - BOUNCE_HEIGHT).abs() < 0.05,
            "highest {highest}"
        );
    }

    #[test]
    fn the_label_follows_the_ball() {
        let simulation = simulation();
        let game = simulation.game();
        let [x, y, z] = game.ball_position();
        let size = [320.0, 180.0];
        let middle = game.camera().project([x, y, z], size).unwrap();
        let above = game.camera().project([x, y + 1.0, z], size).unwrap();
        assert!(middle[0] > 0.0 && middle[0] < size[0] && middle[1] < size[1]);
        assert!(
            above[1] < middle[1],
            "higher in the world is higher on screen"
        );
    }
}
