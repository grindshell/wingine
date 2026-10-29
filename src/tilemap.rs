//! Tile maps: grids of tiles from a tileset, in layers drawn one over another, where solid tiles
//! block movement.

use std::ops::Range;

use crate::{
    graphics::{Canvas, Color, Sprite},
    math::Rect,
};

/// One kind of tile in a [`Tileset`]: how it looks and whether it blocks movement.
///
/// Set the fields that differ from [`Tile::new`]'s defaults with struct update syntax:
/// `Tile { solid: true, ..Tile::new(sprite) }`.
#[derive(Clone, Debug)]
pub struct Tile {
    /// Drawn stretched to fill the tile's cell.
    pub sprite: Sprite,
    /// Multiplies the sprite's colors. [`Color::WHITE`] leaves them unchanged.
    pub tint: Color,
    /// Whether a cell holding this tile, on any layer, blocks movement.
    pub solid: bool,
}

impl Tile {
    /// A tile drawn with `sprite` unchanged, which doesn't block movement.
    pub fn new(sprite: Sprite) -> Self {
        Self {
            sprite,
            tint: Color::WHITE,
            solid: false,
        }
    }
}

/// Identifies a tile in a [`Tileset`], as returned by [`Tileset::add`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileId(u32);

/// The kinds of tile a [`Tilemap`] is built from, all the same size.
#[derive(Clone, Debug)]
pub struct Tileset {
    tile_width: u32,
    tile_height: u32,
    tiles: Vec<Tile>,
}

impl Tileset {
    /// An empty tileset of tiles `tile_width` by `tile_height` world units, which are canvas
    /// pixels at a camera zoom of 1.
    ///
    /// # Panics
    ///
    /// If either dimension is zero.
    pub fn new(tile_width: u32, tile_height: u32) -> Self {
        assert!(
            tile_width > 0 && tile_height > 0,
            "tiles must be at least 1x1"
        );
        Self {
            tile_width,
            tile_height,
            tiles: Vec::new(),
        }
    }

    /// Adds a kind of tile and returns its ID, for placing it with [`Tilemap::set`].
    pub fn add(&mut self, tile: Tile) -> TileId {
        let id = u32::try_from(self.tiles.len()).expect("a tileset holds at most 2^32 tiles");
        self.tiles.push(tile);
        TileId(id)
    }

    /// # Panics
    ///
    /// If `id` came from a different tileset with more tiles.
    pub fn tile(&self, id: TileId) -> &Tile {
        &self.tiles[id.0 as usize]
    }

    pub fn tile_width(&self) -> u32 {
        self.tile_width
    }

    pub fn tile_height(&self) -> u32 {
        self.tile_height
    }

    /// Number of kinds of tile.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

/// A grid of tiles in layers, drawn in order with later layers on top.
///
/// Cells are addressed as `[column, row]`, from `[0, 0]` at the top-left. The map's top-left
/// corner is at the world origin, and each cell is one tile of the [`Tileset`] in size.
#[derive(Clone, Debug)]
pub struct Tilemap {
    tileset: Tileset,
    columns: u32,
    rows: u32,
    /// Each layer's cells, row by row.
    layers: Vec<Vec<Option<TileId>>>,
}

impl Tilemap {
    /// A map `columns` by `rows` tiles in size, with no layers yet.
    pub fn new(tileset: Tileset, columns: u32, rows: u32) -> Self {
        Self {
            tileset,
            columns,
            rows,
            layers: Vec::new(),
        }
    }

    pub fn tileset(&self) -> &Tileset {
        &self.tileset
    }

    pub fn columns(&self) -> u32 {
        self.columns
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// Adds an empty layer on top of the others and returns its index.
    pub fn add_layer(&mut self) -> usize {
        let cells = self.columns as usize * self.rows as usize;
        self.layers.push(vec![None; cells]);
        self.layers.len() - 1
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// The tile in cell `tile` on `layer`: `None` if the cell is empty or outside the map.
    ///
    /// # Panics
    ///
    /// If there is no layer `layer`.
    pub fn get(&self, layer: usize, tile: [i32; 2]) -> Option<TileId> {
        let cells = &self.layers[layer];
        self.cell_index(tile).and_then(|index| cells[index])
    }

    /// Puts `id` in cell `tile` on `layer`, or with `None`, empties the cell.
    ///
    /// # Panics
    ///
    /// If there is no layer `layer`, or `tile` is outside the map.
    pub fn set(&mut self, layer: usize, tile: [i32; 2], id: Option<TileId>) {
        let index = self.cell_index(tile).unwrap_or_else(|| {
            panic!(
                "tile {tile:?} is outside a {}x{} map",
                self.columns, self.rows
            )
        });
        self.layers[layer][index] = id;
    }

    /// Whether cell `tile` is inside the map.
    pub fn contains(&self, tile: [i32; 2]) -> bool {
        self.cell_index(tile).is_some()
    }

    /// Whether cell `tile` blocks movement: it's outside the map, or holds a solid tile on any
    /// layer.
    pub fn is_solid(&self, tile: [i32; 2]) -> bool {
        let Some(index) = self.cell_index(tile) else {
            return true;
        };
        self.layers
            .iter()
            .filter_map(|cells| cells[index])
            .any(|id| self.tileset.tile(id).solid)
    }

    /// The world area the map covers, such as for [`Camera::bounds`](crate::graphics::Camera::bounds).
    pub fn bounds(&self) -> Rect {
        let [width, height] = self.tile_size();
        Rect::new(
            0.0,
            0.0,
            self.columns as f32 * width,
            self.rows as f32 * height,
        )
    }

    /// The world area of cell `tile`.
    pub fn tile_rect(&self, tile: [i32; 2]) -> Rect {
        let [x, y] = self.tile_to_world(tile.map(|v| v as f32));
        let [width, height] = self.tile_size();
        Rect::new(x, y, width, height)
    }

    /// The world position of `position`, measured in tiles. It may fall between cells, as for a
    /// [`GridMover`](crate::grid::GridMover) partway through a step.
    pub fn tile_to_world(&self, position: [f32; 2]) -> [f32; 2] {
        let size = self.tile_size();
        [0, 1].map(|i| position[i] * size[i])
    }

    /// The cell containing world position `point`, which may be outside the map.
    pub fn world_to_tile(&self, point: [f32; 2]) -> [i32; 2] {
        let size = self.tile_size();
        [0, 1].map(|i| (point[i] / size[i]).floor() as i32)
    }

    /// Draws every layer, in order.
    pub fn draw(&self, canvas: &mut Canvas) {
        for layer in 0..self.layers.len() {
            self.draw_layer(canvas, layer);
        }
    }

    /// Draws one layer, so other things can be drawn between layers: characters above the ground
    /// but below treetops, say. Only the cells the canvas shows are drawn.
    ///
    /// # Panics
    ///
    /// If there is no layer `layer`.
    pub fn draw_layer(&self, canvas: &mut Canvas, layer: usize) {
        let cells = &self.layers[layer];
        let visible = canvas.visible_area();
        let [width, height] = self.tile_size();
        let columns = cell_range(visible.x, visible.right(), width, self.columns);
        let rows = cell_range(visible.y, visible.bottom(), height, self.rows);
        for row in rows {
            for column in columns.clone() {
                let index = row as usize * self.columns as usize + column as usize;
                if let Some(id) = cells[index] {
                    let tile = self.tileset.tile(id);
                    canvas.draw_sprite(&tile.sprite, self.tile_rect([column, row]), tile.tint);
                }
            }
        }
    }

    fn tile_size(&self) -> [f32; 2] {
        [
            self.tileset.tile_width as f32,
            self.tileset.tile_height as f32,
        ]
    }

    /// Where cell `tile` is in each layer's cells, if it's inside the map.
    fn cell_index(&self, [column, row]: [i32; 2]) -> Option<usize> {
        let column = u32::try_from(column).ok().filter(|&c| c < self.columns)?;
        let row = u32::try_from(row).ok().filter(|&r| r < self.rows)?;
        Some(row as usize * self.columns as usize + column as usize)
    }
}

/// The cells along one axis, each `size` long and `count` in all, that overlap `start..end`.
fn cell_range(start: f32, end: f32, size: f32, count: u32) -> Range<i32> {
    let first = (start / size).floor() as i32;
    let last = (end / size).ceil() as i32;
    first.max(0)..last.min(count as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        graphics::{Camera, Texture},
        headless::Headless,
    };

    /// A 1x1 white texture. Creating one needs the GPU.
    fn texture() -> Texture {
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let ctx = headless.context();
        ctx.renderer
            .create_texture(&ctx.gpu.device, &ctx.gpu.queue, 1, 1, &[255; 4])
    }

    /// A 3x2 map of 8x4 tiles: grass everywhere on the ground layer, and a wall at [1, 0] on
    /// the layer above.
    fn map() -> (Tilemap, [TileId; 2]) {
        let sprite = texture().sprite();
        let mut tileset = Tileset::new(8, 4);
        let grass = tileset.add(Tile::new(sprite.clone()));
        let wall = tileset.add(Tile {
            solid: true,
            ..Tile::new(sprite)
        });
        let mut map = Tilemap::new(tileset, 3, 2);
        let ground = map.add_layer();
        let walls = map.add_layer();
        for row in 0..2 {
            for column in 0..3 {
                map.set(ground, [column, row], Some(grass));
            }
        }
        map.set(walls, [1, 0], Some(wall));
        (map, [grass, wall])
    }

    #[test]
    fn cells_hold_tiles_per_layer() {
        let (mut map, [grass, wall]) = map();
        assert_eq!(map.get(0, [1, 0]), Some(grass));
        assert_eq!(map.get(1, [1, 0]), Some(wall));
        assert_eq!(map.get(1, [0, 0]), None);
        assert_eq!(map.get(0, [3, 0]), None, "outside the map");
        map.set(1, [1, 0], None);
        assert_eq!(map.get(1, [1, 0]), None);
    }

    #[test]
    #[should_panic(expected = "outside a 3x2 map")]
    fn setting_outside_the_map_panics() {
        let (mut map, [grass, _]) = map();
        map.set(0, [0, -1], Some(grass));
    }

    #[test]
    fn solid_tiles_and_the_outside_block_movement() {
        let (map, _) = map();
        assert!(map.is_solid([1, 0]), "a wall above the grass");
        assert!(!map.is_solid([0, 0]));
        assert!(!map.is_solid([2, 1]));
        for outside in [[-1, 0], [3, 0], [0, -1], [0, 2]] {
            assert!(map.is_solid(outside), "{outside:?}");
        }
    }

    #[test]
    fn converts_between_tiles_and_the_world() {
        let (map, _) = map();
        assert_eq!(map.bounds(), Rect::new(0.0, 0.0, 24.0, 8.0));
        assert_eq!(map.tile_rect([2, 1]), Rect::new(16.0, 4.0, 8.0, 4.0));
        assert_eq!(map.tile_to_world([0.5, 1.5]), [4.0, 6.0]);
        assert_eq!(map.world_to_tile([15.9, 4.0]), [1, 1]);
        assert_eq!(map.world_to_tile([-0.1, 8.0]), [-1, 2]);
    }

    #[test]
    fn cell_ranges_cover_partly_visible_cells() {
        assert_eq!(cell_range(0.0, 32.0, 16.0, 10), 0..2);
        assert_eq!(cell_range(4.0, 36.0, 16.0, 10), 0..3);
        assert_eq!(cell_range(-20.0, 500.0, 16.0, 10), 0..10);
        assert!(cell_range(200.0, 240.0, 16.0, 10).is_empty());
    }

    #[test]
    fn draws_only_visible_cells() {
        let (map, _) = map();
        let mut canvas = Canvas::new();
        canvas.begin(16.0, 8.0);
        // Shows x 4..20 and y 0..8: columns 0 to 2 on both rows.
        canvas.set_camera(Some(Camera::new(12.0, 4.0)));
        map.draw_layer(&mut canvas, 0);
        assert_eq!(canvas.vertices.len(), 6 * 4);

        // Shows x 0..8 and y 0..8 at a zoom of 2: only the first column.
        canvas.begin(16.0, 16.0);
        canvas.set_camera(Some(Camera {
            zoom: 2.0,
            ..Camera::new(4.0, 4.0)
        }));
        map.draw(&mut canvas);
        // Grass in the first column; the wall is in the second.
        assert_eq!(canvas.vertices.len(), 2 * 4);
    }
}
