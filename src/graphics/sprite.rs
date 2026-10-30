use super::Texture;
use crate::math::Rect;

/// A rectangular region of a texture, drawn with [`Canvas::draw_sprite`](super::Canvas::draw_sprite).
///
/// Cloning is cheap; the texture itself is shared.
#[derive(Clone, Debug)]
pub struct Sprite {
    texture: Texture,
    /// In texels, with the origin at the texture's top-left.
    region: Rect,
    flip_x: bool,
    flip_y: bool,
}

impl Sprite {
    pub fn new(texture: Texture, region: Rect) -> Self {
        Self {
            texture,
            region,
            flip_x: false,
            flip_y: false,
        }
    }

    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    /// The region of the texture this sprite shows, in texels.
    pub fn region(&self) -> Rect {
        self.region
    }

    /// Width in texels.
    pub fn width(&self) -> f32 {
        self.region.w
    }

    /// Height in texels.
    pub fn height(&self) -> f32 {
        self.region.h
    }

    /// Returns this sprite mirrored left-to-right.
    pub fn flipped_x(mut self) -> Self {
        self.flip_x = !self.flip_x;
        self
    }

    /// Returns this sprite mirrored top-to-bottom.
    pub fn flipped_y(mut self) -> Self {
        self.flip_y = !self.flip_y;
        self
    }

    /// Texture coordinates for the top-left and bottom-right corners of a quad drawing this sprite.
    pub(crate) fn uv_bounds(&self) -> ([f32; 2], [f32; 2]) {
        let size = [self.texture.width() as f32, self.texture.height() as f32];
        uv_bounds(self.region, size, self.flip_x, self.flip_y)
    }
}

fn uv_bounds(region: Rect, size: [f32; 2], flip_x: bool, flip_y: bool) -> ([f32; 2], [f32; 2]) {
    let mut min = [region.x / size[0], region.y / size[1]];
    let mut max = [region.right() / size[0], region.bottom() / size[1]];
    if flip_x {
        std::mem::swap(&mut min[0], &mut max[0]);
    }
    if flip_y {
        std::mem::swap(&mut min[1], &mut max[1]);
    }
    (min, max)
}

/// A texture divided into a grid of equally sized sprites, such as a tileset.
#[derive(Clone, Debug)]
pub struct SpriteSheet {
    texture: Texture,
    tile_width: u32,
    tile_height: u32,
    columns: u32,
    rows: u32,
}

impl SpriteSheet {
    /// Divides `texture` into tiles of `tile_width` by `tile_height` texels, starting at the
    /// top-left with no spacing between tiles. Partial tiles at the right and bottom are ignored.
    ///
    /// # Panics
    ///
    /// If either tile dimension is zero.
    pub fn new(texture: Texture, tile_width: u32, tile_height: u32) -> Self {
        Self::try_new(texture, tile_width, tile_height).unwrap_or_else(|err| panic!("{err}"))
    }

    /// [`SpriteSheet::new`], or what's wrong with the tile size.
    pub(crate) fn try_new(
        texture: Texture,
        tile_width: u32,
        tile_height: u32,
    ) -> Result<Self, String> {
        if tile_width == 0 || tile_height == 0 {
            return Err("sprite sheet tiles must be at least 1x1".into());
        }
        Ok(Self {
            columns: texture.width() / tile_width,
            rows: texture.height() / tile_height,
            texture,
            tile_width,
            tile_height,
        })
    }

    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn columns(&self) -> u32 {
        self.columns
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// Number of sprites in the sheet.
    pub fn len(&self) -> u32 {
        self.columns * self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The sprite at `column` and `row`, counting from 0 at the top-left.
    ///
    /// # Panics
    ///
    /// If the position is outside the sheet.
    pub fn sprite(&self, column: u32, row: u32) -> Sprite {
        self.try_sprite(column, row)
            .unwrap_or_else(|err| panic!("{err}"))
    }

    /// [`SpriteSheet::sprite`], or why there's no sprite there.
    pub(crate) fn try_sprite(&self, column: u32, row: u32) -> Result<Sprite, String> {
        if column >= self.columns || row >= self.rows {
            return Err(format!(
                "sprite ({column}, {row}) is outside a {}x{} sprite sheet",
                self.columns, self.rows
            ));
        }
        let region = Rect::new(
            (column * self.tile_width) as f32,
            (row * self.tile_height) as f32,
            self.tile_width as f32,
            self.tile_height as f32,
        );
        Ok(Sprite::new(self.texture.clone(), region))
    }

    /// The sprite at `index`, counting left to right, then top to bottom.
    ///
    /// # Panics
    ///
    /// If `index` is not less than [`SpriteSheet::len`].
    pub fn sprite_at(&self, index: u32) -> Sprite {
        assert!(
            index < self.len(),
            "sprite index {index} is outside a sheet of {} sprites",
            self.len()
        );
        self.sprite(index % self.columns, index / self.columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_bounds_normalize_region() {
        let region = Rect::new(16.0, 32.0, 16.0, 16.0);
        let (min, max) = uv_bounds(region, [64.0, 64.0], false, false);
        assert_eq!(min, [0.25, 0.5]);
        assert_eq!(max, [0.5, 0.75]);
    }

    #[test]
    fn uv_bounds_swap_when_flipped() {
        let region = Rect::new(0.0, 0.0, 32.0, 64.0);
        let (min, max) = uv_bounds(region, [64.0, 64.0], true, true);
        assert_eq!(min, [0.5, 1.0]);
        assert_eq!(max, [0.0, 0.0]);
    }
}
