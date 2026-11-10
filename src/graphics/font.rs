use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use ab_glyph::{Font as _, FontArc, GlyphId, OutlinedGlyph, PxScale, ScaleFont as _};
use image::RgbaImage;

use super::{
    Texture, default_font,
    gpu::texture_size_problem,
    renderer::{GlyphPages, Renderer},
};
use crate::{math::Rect, scene::BitmapFontDef};

/// A typeface ready to draw: the built-in pixel font, a bitmap font whose glyphs are drawn in an
/// image, or a TrueType or OpenType font at a size, whose glyphs are rasterized as text first
/// needs them.
///
/// Draw text with [`Canvas::draw_text`](super::Canvas::draw_text), or lay it out first with
/// [`Font::layout`] to wrap it or reveal it a little at a time. Cloning is cheap; clones share
/// the glyphs. The built-in font comes from
/// [`Context::default_font`](crate::Context::default_font), and others from font files, with
/// [`Context::load_font`](crate::Context::load_font) and
/// [`Context::load_vector_font`](crate::Context::load_vector_font).
#[derive(Clone)]
pub struct Font {
    data: Arc<FontData>,
    /// How many times its glyphs' size they're drawn. Always 1 for a vector font, which is
    /// rasterized at the size it's drawn instead.
    scale: f32,
}

enum FontData {
    /// Glyphs known from the start, in one texture: the built-in font, and bitmap fonts.
    Fixed {
        texture: Texture,
        metrics: Metrics,
        files: Vec<PathBuf>,
    },
    /// A TrueType or OpenType font at one size.
    Vector {
        face: Arc<Face>,
        sized: Arc<FaceSize>,
    },
}

/// How a TrueType or OpenType font is drawn, given when it's loaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSettings {
    /// How many pixels its em is, as CSS's `font-size` counts them. A pixel font made as a
    /// TrueType font is drawn at the size it was made for, or a whole number of times it, to fit
    /// its pixels.
    pub size: f32,
    /// Whether pixels its outlines partly cover are partly opaque, as text usually is, or each
    /// opaque or clear, whichever is nearer, so a pixel font stays crisp.
    pub smooth: bool,
}

impl FontSettings {
    /// A smooth font `size` pixels to the em.
    pub fn new(size: f32) -> Self {
        Self { size, smooth: true }
    }
}

/// Everything about a font whose glyphs are known from the start that layout needs, apart from
/// the texture.
#[derive(Debug)]
struct Metrics {
    glyphs: HashMap<char, Glyph>,
    /// Drawn for characters the font lacks.
    fallback: Option<Glyph>,
    line_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Glyph {
    /// Where the glyph's image goes, relative to the pen at the top of the line, in unscaled
    /// pixels. Empty for glyphs with nothing to draw, such as a space.
    bounds: Rect,
    uv: ([f32; 2], [f32; 2]),
    /// How far the pen moves on after this glyph.
    advance: f32,
    /// Which of the font's textures its image is in.
    page: u32,
}

impl Glyph {
    /// A glyph with nothing to draw, which moves the pen `advance`.
    fn blank(advance: f32) -> Self {
        Self {
            bounds: Rect::default(),
            uv: ([0.0; 2], [0.0; 2]),
            advance,
            page: 0,
        }
    }

    fn has_ink(&self) -> bool {
        self.bounds.w > 0.0 && self.bounds.h > 0.0
    }
}

/// A glyph placed by layout, relative to the text's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacedGlyph {
    pub rect: Rect,
    pub uv: ([f32; 2], [f32; 2]),
    /// Which of the font's textures it's drawn from ([`Font::page`]).
    pub page: u32,
}

impl Font {
    /// The built-in pixel font, from [`default_font`].
    pub(crate) fn built_in(
        renderer: &Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {
        let glyphs: Vec<(char, Vec<&str>)> = default_font::GLYPHS
            .iter()
            .map(|&(c, rows)| (c, rows.split(' ').collect()))
            .collect();

        // Pack the glyphs in a grid of equal cells, a pixel apart so none bleeds into another.
        const COLUMNS: usize = 16;
        let cell_width = glyphs
            .iter()
            .map(|(_, rows)| rows[0].len())
            .max()
            .unwrap_or(0)
            + 1;
        let cell_height = glyphs.iter().map(|(_, rows)| rows.len()).max().unwrap_or(0) + 1;
        let width = COLUMNS * cell_width;
        let height = glyphs.len().div_ceil(COLUMNS) * cell_height;
        let mut rgba = vec![0; width * height * 4];
        let mut metrics = Metrics {
            glyphs: HashMap::new(),
            fallback: None,
            line_height: default_font::LINE_HEIGHT,
        };
        for (i, (c, rows)) in glyphs.iter().enumerate() {
            let origin = [i % COLUMNS * cell_width, i / COLUMNS * cell_height];
            for (y, row) in rows.iter().enumerate() {
                for (x, pixel) in row.bytes().enumerate() {
                    if pixel == b'#' {
                        let offset = ((origin[1] + y) * width + origin[0] + x) * 4;
                        rgba[offset..offset + 4].fill(255);
                    }
                }
            }
            let [glyph_width, glyph_height] = [rows[0].len() as f32, rows.len() as f32];
            let size = [width as f32, height as f32];
            let min = [0, 1].map(|i| origin[i] as f32 / size[i]);
            let max = [
                min[0] + glyph_width / size[0],
                min[1] + glyph_height / size[1],
            ];
            let glyph = Glyph {
                bounds: Rect::new(0.0, 0.0, glyph_width, glyph_height),
                uv: (min, max),
                advance: glyph_width + 1.0,
                page: 0,
            };
            metrics.glyphs.insert(*c, glyph);
        }
        for &(alias, original) in default_font::ALIASES {
            let glyph = metrics.glyphs[&original];
            metrics.glyphs.insert(alias, glyph);
        }
        metrics
            .glyphs
            .insert(' ', Glyph::blank(default_font::SPACE_ADVANCE));
        metrics.fallback = metrics.glyphs.get(&'?').copied();

        let texture = renderer.create_texture(device, queue, width as u32, height as u32, &rgba);
        Self::fixed(texture, metrics, Vec::new())
    }

    /// A bitmap font: its glyphs cut from `image`, as `def` describes them, which becomes its
    /// texture. `files` are those it was read from.
    pub(crate) fn bitmap(
        renderer: &Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        def: &BitmapFontDef,
        image: &RgbaImage,
        files: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let metrics = bitmap_metrics(def, image)?;
        let (width, height) = image.dimensions();
        if let Some(problem) = texture_size_problem(device, "its image", [width, height]) {
            return Err(problem);
        }
        let texture = renderer.create_texture(device, queue, width, height, image.as_raw());
        Ok(Self::fixed(texture, metrics, files))
    }

    fn fixed(texture: Texture, metrics: Metrics, files: Vec<PathBuf>) -> Self {
        Self {
            data: Arc::new(FontData::Fixed {
                texture,
                metrics,
                files,
            }),
            scale: 1.0,
        }
    }

    /// A TrueType or OpenType font, from the bytes of its file, drawn as `settings` say, its
    /// glyphs put in pages `pages` makes. `files` are those it was read from.
    pub(crate) fn vector(
        bytes: Vec<u8>,
        settings: FontSettings,
        files: Vec<PathBuf>,
        pages: GlyphPages,
    ) -> Result<Self, String> {
        let font = FontArc::try_from_vec(bytes)
            .map_err(|_| "this isn't a TrueType or OpenType font".to_owned())?;
        let face = Arc::new(Face {
            font,
            smooth: settings.smooth,
            files,
            pages,
            sizes: Mutex::default(),
        });
        face.font(settings.size)
    }

    /// The distance from one line of text to the next.
    pub fn line_height(&self) -> f32 {
        match &*self.data {
            FontData::Fixed { metrics, .. } => metrics.line_height * self.scale,
            FontData::Vector { sized, .. } => sized.line_height,
        }
    }

    /// This font drawn `scale` times as large, such as for a title. Whole numbers keep pixel
    /// fonts crisp. A TrueType or OpenType font is rasterized at the new size, so it stays sharp
    /// at any.
    ///
    /// # Panics
    ///
    /// If `scale` isn't a positive number.
    pub fn scaled(&self, scale: f32) -> Self {
        self.try_scaled(scale).unwrap_or_else(|err| panic!("{err}"))
    }

    /// [`Font::scaled`], or what's wrong with `scale`.
    pub(crate) fn try_scaled(&self, scale: f32) -> Result<Self, String> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(format!("font scale must be a positive number, not {scale}"));
        }
        match &*self.data {
            FontData::Fixed { .. } => Ok(Self {
                data: Arc::clone(&self.data),
                scale: self.scale * scale,
            }),
            FontData::Vector { face, sized } => face.font(sized.size * scale),
        }
    }

    /// This font at `size` pixels to the em, if it's a TrueType or OpenType font, sharing the
    /// file it was read from.
    pub(crate) fn at_size(&self, size: f32) -> Option<Result<Self, String>> {
        match &*self.data {
            FontData::Fixed { .. } => None,
            FontData::Vector { face, .. } => Some(face.font(size)),
        }
    }

    /// The files the font was read from: a font file, and the image or font it names. The
    /// built-in font has none.
    pub fn files(&self) -> &[PathBuf] {
        match &*self.data {
            FontData::Fixed { files, .. } => files,
            FontData::Vector { face, .. } => &face.files,
        }
    }

    /// Lays out `text` to draw with [`Canvas::draw_text_layout`](super::Canvas::draw_text_layout).
    /// With a `max_width`, lines wrap between words to fit it, and words too long for a line of
    /// their own break between characters. `\n` always starts a new line.
    pub fn layout(&self, text: &str, max_width: Option<f32>) -> TextLayout {
        self.layout_aligned(text, max_width, Align::Left)
    }

    /// [`Font::layout`], with the lines lined up by `align` across the width of the widest. Lines
    /// move by whole pixels, so pixel fonts stay crisp.
    pub fn layout_aligned(&self, text: &str, max_width: Option<f32>, align: Align) -> TextLayout {
        let (glyphs, size, lines) = self
            .glyphs()
            .lay_out_aligned(self.scale, text, max_width, align);
        TextLayout {
            font: self.clone(),
            glyphs,
            size,
            lines,
            align,
        }
    }

    /// The width and height of `text` drawn without wrapping. The width runs to the right edge of
    /// the last glyph's ink on the longest line; the height is a whole number of lines.
    pub fn measure(&self, text: &str) -> [f32; 2] {
        self.glyphs().lay_out(self.scale, text, None, |_, _| {}).0
    }

    /// The texture glyphs on `page` are drawn from.
    pub(crate) fn page(&self, page: u32) -> Texture {
        match &*self.data {
            FontData::Fixed { texture, .. } => texture.clone(),
            FontData::Vector { sized, .. } => {
                lock(&sized.atlas).pages[page as usize].texture.clone()
            }
        }
    }

    /// Places each glyph of `text` without wrapping, passing it to `place`.
    pub(crate) fn place_glyphs(&self, text: &str, mut place: impl FnMut(PlacedGlyph)) {
        self.glyphs()
            .lay_out(self.scale, text, None, |glyph, _| place(glyph));
    }

    fn glyphs(&self) -> Source<'_> {
        match &*self.data {
            FontData::Fixed { metrics, .. } => Source::Fixed(metrics),
            FontData::Vector { face, sized } => Source::Vector(face, sized),
        }
    }
}

impl fmt::Debug for Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("Font");
        if let FontData::Vector { sized, .. } = &*self.data {
            debug.field("size", &sized.size);
        }
        debug
            .field("scale", &self.scale)
            .field("line_height", &self.line_height())
            .field("files", &self.files())
            .finish_non_exhaustive()
    }
}

/// The glyphs of a bitmap font, cut from `image` as `def` describes, each as wide as its ink
/// unless the font is monospace.
fn bitmap_metrics(def: &BitmapFontDef, image: &RgbaImage) -> Result<Metrics, String> {
    let [cell_width, cell_height] = def.cell;
    let (width, height) = image.dimensions();
    let [columns, rows] = [width / cell_width, height / cell_height];
    if columns == 0 || rows == 0 {
        return Err(format!(
            "its image, {width}x{height}, is smaller than a cell, {cell_width}x{cell_height}"
        ));
    }
    let size = [width as f32, height as f32];
    let mut glyphs = HashMap::new();
    for (c, [column, row]) in def.characters.cells(columns) {
        if column >= columns || row >= rows {
            return Err(format!(
                "`{c}` would be in column {column} and row {row} of cells, counting from 0, but \
                 the image has {columns} columns and {rows} rows of them"
            ));
        }
        let [left, top] = [column * cell_width, row * cell_height];
        let inked = |x: u32| (top..top + cell_height).any(|y| image.get_pixel(left + x, y)[3] > 0);
        let Some(first) = (0..cell_width).find(|&x| inked(x)) else {
            return Err(format!(
                "the cell for `{c}`, column {column} and row {row} counting from 0, is empty"
            ));
        };
        let last = (0..cell_width).rev().find(|&x| inked(x)).unwrap_or(first);
        let (from, glyph_width) = if def.monospace {
            (0, cell_width)
        } else {
            (first, last - first + 1)
        };
        let min = [(left + from) as f32 / size[0], top as f32 / size[1]];
        let max = [
            (left + from + glyph_width) as f32 / size[0],
            (top + cell_height) as f32 / size[1],
        ];
        let glyph = Glyph {
            bounds: Rect::new(0.0, 0.0, glyph_width as f32, cell_height as f32),
            uv: (min, max),
            advance: glyph_width as f32 + def.spacing as f32,
            page: 0,
        };
        glyphs.insert(c, glyph);
    }
    glyphs.insert(' ', Glyph::blank(def.space as f32));
    Ok(Metrics {
        fallback: glyphs.get(&'?').copied(),
        glyphs,
        line_height: def.line_height as f32,
    })
}

/// How many sizes of a TrueType font are kept rasterized, besides those fonts in use hold, so a
/// font scaled the same way in every frame's drawing doesn't rasterize its glyphs each time.
const KEPT_SIZES: usize = 8;

/// The largest page of glyphs: the largest texture every GPU wgpu runs on takes by default.
const MAX_PAGE: u32 = 8192;

/// A TrueType or OpenType font file, parsed, with the sizes it's been drawn at lately.
struct Face {
    font: FontArc,
    smooth: bool,
    files: Vec<PathBuf>,
    pages: GlyphPages,
    /// The most recently used last, [`KEPT_SIZES`] at most.
    sizes: Mutex<Vec<Arc<FaceSize>>>,
}

impl Face {
    /// The font at `size` pixels to the em.
    fn font(self: &Arc<Self>, size: f32) -> Result<Font, String> {
        if !(size.is_finite() && size > 0.0) {
            return Err(format!(
                "a font's size must be a positive number, not {size}"
            ));
        }
        Ok(Font {
            data: Arc::new(FontData::Vector {
                face: Arc::clone(self),
                sized: self.sized(size),
            }),
            scale: 1.0,
        })
    }

    /// Its glyphs at `size`, rasterized already if it's been drawn at that size lately.
    fn sized(&self, size: f32) -> Arc<FaceSize> {
        let mut sizes = lock(&self.sizes);
        if let Some(i) = sizes.iter().position(|sized| sized.size == size) {
            let sized = sizes.remove(i);
            sizes.push(Arc::clone(&sized));
            return sized;
        }
        let sized = Arc::new(FaceSize::new(&self.font, size));
        if sizes.len() >= KEPT_SIZES {
            sizes.remove(0);
        }
        sizes.push(Arc::clone(&sized));
        sized
    }
}

/// A TrueType or OpenType font at one size: its lines, and the glyphs rasterized so far.
struct FaceSize {
    /// Pixels to the em.
    size: f32,
    /// What `ab_glyph` scales its outlines by.
    scale: PxScale,
    /// How far below the top of a line its baseline is, a whole number of pixels.
    baseline: f32,
    line_height: f32,
    /// How many pixels across and down its pages of glyphs are.
    page_size: u32,
    atlas: Mutex<Atlas>,
}

impl FaceSize {
    fn new(font: &FontArc, size: f32) -> Self {
        let units_per_em = font.units_per_em().unwrap_or(1000.0);
        let scale = PxScale::from(size * font.height_unscaled() / units_per_em);
        let scaled = font.as_scaled(scale);
        let line_height = (scaled.ascent() - scaled.descent() + scaled.line_gap())
            .round()
            .max(1.0);
        Self {
            size,
            scale,
            baseline: scaled.ascent().round(),
            line_height,
            page_size: page_size(line_height),
            atlas: Mutex::default(),
        }
    }

    /// The glyph for `c`, rasterized and put in a page the first time, its `?` if the font lacks
    /// it, or `None` if it lacks that too.
    fn glyph(&self, face: &Face, c: char) -> Option<Glyph> {
        let mut atlas = lock(&self.atlas);
        if let Some(glyph) = atlas.glyphs.get(&c) {
            return *glyph;
        }
        let id = face.font.glyph_id(c);
        let glyph = if id.0 != 0 {
            Some(self.rasterize(face, &mut atlas, id))
        } else if c != '?' {
            drop(atlas);
            let fallback = self.glyph(face, '?');
            atlas = lock(&self.atlas);
            fallback
        } else {
            None
        };
        atlas.glyphs.insert(c, glyph);
        glyph
    }

    /// Rasterizes the glyph `id` into a page, with its baseline on a whole pixel.
    fn rasterize(&self, face: &Face, atlas: &mut Atlas, id: GlyphId) -> Glyph {
        let advance = face.font.as_scaled(self.scale).h_advance(id);
        let blank = Glyph::blank(advance);
        let positioned =
            id.with_scale_and_position(self.scale, ab_glyph::point(0.0, self.baseline));
        let Some(outlined) = face.font.outline_glyph(positioned) else {
            return blank;
        };
        let bounds = outlined.px_bounds();
        let [width, height] = [bounds.width(), bounds.height()];
        // Glyphs too big for any page, or without a pixel, draw nothing.
        let largest = (MAX_PAGE - 1) as f32;
        if !(1.0..=largest).contains(&width) || !(1.0..=largest).contains(&height) {
            return blank;
        }
        let size = [width as u32, height as u32];
        let Some((page, at)) = atlas.place(size, self.page_size, &face.pages) else {
            return blank;
        };
        let Page { raw, shelves, .. } = &atlas.pages[page];
        face.pages
            .write(raw, at, size, &glyph_pixels(&outlined, size, face.smooth));
        let across = shelves.size as f32;
        let min = [at[0] as f32 / across, at[1] as f32 / across];
        Glyph {
            bounds: Rect::new(bounds.min.x, bounds.min.y, width, height),
            uv: (min, [min[0] + width / across, min[1] + height / across]),
            advance,
            page: page as u32,
        }
    }
}

/// How many pixels across a font's pages of glyphs are, with lines `line_height` apart: room for
/// a couple of hundred glyphs at least.
fn page_size(line_height: f32) -> u32 {
    let wanted = (line_height * 16.0).clamp(1.0, 2048.0) as u32;
    wanted.next_power_of_two().clamp(256, 2048)
}

/// The pixels of `outlined`, `size` of them, white with its coverage as their opacity: partly
/// opaque where it's partly covered if `smooth`, and otherwise opaque where it's at least half.
fn glyph_pixels(outlined: &OutlinedGlyph, [width, height]: [u32; 2], smooth: bool) -> Vec<u8> {
    let mut rgba = [255, 255, 255, 0].repeat((width * height) as usize);
    outlined.draw(|x, y, coverage| {
        if x < width && y < height {
            rgba[((y * width + x) * 4 + 3) as usize] = opacity(coverage, smooth);
        }
    });
    rgba
}

/// The opacity of a pixel its glyph covers `coverage` of, from 0 to 1.
fn opacity(coverage: f32, smooth: bool) -> u8 {
    let coverage = coverage.clamp(0.0, 1.0);
    if smooth {
        (coverage * 255.0).round() as u8
    } else if coverage >= 0.5 {
        255
    } else {
        0
    }
}

/// A font size's glyphs: what each character draws, and the pages they're in.
#[derive(Default)]
struct Atlas {
    /// `None` for characters the font has nothing for, not even a `?`.
    glyphs: HashMap<char, Option<Glyph>>,
    pages: Vec<Page>,
}

struct Page {
    raw: wgpu::Texture,
    texture: Texture,
    /// Where the next glyph goes.
    shelves: Shelves,
}

impl Atlas {
    /// Where a glyph `size` pixels across and down goes: which page and where in it, in a new
    /// page `page_size` across if the last one is full, or one of its own if it's bigger than
    /// that.
    fn place(
        &mut self,
        size: [u32; 2],
        page_size: u32,
        pages: &GlyphPages,
    ) -> Option<(usize, [u32; 2])> {
        if let Some(page) = self.pages.last_mut()
            && let Some(at) = page.shelves.place(size)
        {
            return Some((self.pages.len() - 1, at));
        }
        let needed = size[0].max(size[1]) + 1;
        let page_size = if needed > page_size {
            needed.next_power_of_two()
        } else {
            page_size
        };
        if page_size > MAX_PAGE {
            return None;
        }
        let mut shelves = Shelves::new(page_size);
        let at = shelves.place(size)?;
        let (raw, texture) = pages.create(page_size);
        self.pages.push(Page {
            raw,
            texture,
            shelves,
        });
        Some((self.pages.len() - 1, at))
    }
}

/// Glyphs packed into a square page in rows, each as tall as its tallest glyph, left to right
/// and then down, with a pixel to spare to the right of and below each.
#[derive(Debug)]
struct Shelves {
    /// How many pixels across and down the page is.
    size: u32,
    /// Where the next glyph goes on the row.
    x: u32,
    /// The top of the row.
    y: u32,
    /// How tall the row is so far, with the pixel below.
    height: u32,
}

impl Shelves {
    fn new(size: u32) -> Self {
        Self {
            size,
            x: 0,
            y: 0,
            height: 0,
        }
    }

    /// Where a glyph `[width, height]` goes, if there's room.
    fn place(&mut self, [width, height]: [u32; 2]) -> Option<[u32; 2]> {
        if self.x + width > self.size {
            self.x = 0;
            self.y += self.height;
            self.height = 0;
        }
        if self.x + width > self.size || self.y + height > self.size {
            return None;
        }
        let at = [self.x, self.y];
        self.x += width + 1;
        self.height = self.height.max(height + 1);
        Some(at)
    }
}

/// `mutex`'s value, even if a thread panicked holding it: a font's glyphs stay usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a font's glyphs come from, for laying text out.
enum Source<'a> {
    Fixed(&'a Metrics),
    Vector(&'a Face, &'a FaceSize),
}

impl Glyphs for Source<'_> {
    fn glyph(&self, c: char) -> Option<Glyph> {
        match self {
            Self::Fixed(metrics) => metrics.glyph(c),
            Self::Vector(face, sized) => sized.glyph(face, c),
        }
    }

    fn kern(&self, before: char, c: char) -> f32 {
        match self {
            Self::Fixed(_) => 0.0,
            Self::Vector(face, sized) => {
                let font = &face.font;
                let scaled = font.as_scaled(sized.scale);
                scaled.kern(font.glyph_id(before), font.glyph_id(c))
            }
        }
    }

    fn line_height(&self) -> f32 {
        match self {
            Self::Fixed(metrics) => metrics.line_height,
            Self::Vector(_, sized) => sized.line_height,
        }
    }
}

impl Glyphs for Metrics {
    fn glyph(&self, c: char) -> Option<Glyph> {
        self.glyphs.get(&c).or(self.fallback.as_ref()).copied()
    }

    fn kern(&self, _: char, _: char) -> f32 {
        0.0
    }

    fn line_height(&self) -> f32 {
        self.line_height
    }
}

/// How lines lay out: everything about a font that it needs.
trait Glyphs {
    /// The glyph for `c`, or the one drawn for characters the font lacks.
    fn glyph(&self, c: char) -> Option<Glyph>;

    /// How far the pen moves between `before` and `c`, beside its advance, in unscaled pixels.
    fn kern(&self, before: char, c: char) -> f32;

    fn line_height(&self) -> f32;

    /// Lays out `text` at `scale`, passing each glyph with something to draw to `place`, with the
    /// line it's on, counting from 0, and returns the text's size and how many lines it takes.
    /// Glyphs go where the pen is, rounded to whole pixels.
    fn lay_out(
        &self,
        scale: f32,
        text: &str,
        max_width: Option<f32>,
        mut place: impl FnMut(PlacedGlyph, usize),
    ) -> ([f32; 2], usize) {
        let max_width = max_width.unwrap_or(f32::INFINITY);
        let space = self.glyph(' ').map_or(0.0, |glyph| glyph.advance * scale);
        let mut cursor = Cursor {
            line_height: self.line_height() * scale,
            ..Cursor::default()
        };
        for (i, paragraph) in text.split('\n').enumerate() {
            if i > 0 {
                cursor.new_line();
            }
            // The width of the spaces since the last word, kept unless the line wraps there.
            let mut spaces = 0.0;
            for (i, word) in paragraph.split(' ').enumerate() {
                if i > 0 {
                    spaces += space;
                }
                let word_ink = self.ink_width(scale, word);
                if word_ink == 0.0 {
                    continue;
                }
                if !cursor.line_is_empty() && (cursor.pen + spaces).round() + word_ink > max_width {
                    cursor.new_line();
                } else {
                    cursor.pen += spaces;
                }
                spaces = 0.0;
                // Only a word that doesn't fit even where a line starts breaks between
                // characters: one too long for any line, or one after leading spaces.
                let breakable = cursor.pen.round() + word_ink > max_width;
                let mut before = None;
                for c in word.chars().filter(|c| !c.is_control()) {
                    let Some(glyph) = self.glyph(c) else {
                        continue;
                    };
                    if let Some(before) = before {
                        cursor.pen += self.kern(before, c) * scale;
                    }
                    before = Some(c);
                    let bounds = scale_rect(glyph.bounds, scale);
                    // Something is on the line, leading spaces at least, so it can wrap.
                    if breakable
                        && cursor.pen > 0.0
                        && cursor.pen.round() + bounds.right() > max_width
                    {
                        cursor.new_line();
                    }
                    if glyph.has_ink() {
                        let pen = cursor.pen.round();
                        let rect =
                            Rect::new(pen + bounds.x, cursor.top() + bounds.y, bounds.w, bounds.h);
                        let placed = PlacedGlyph {
                            rect,
                            uv: glyph.uv,
                            page: glyph.page,
                        };
                        place(placed, cursor.line);
                        cursor.ink = Some(cursor.ink.unwrap_or(0.0).max(pen + bounds.right()));
                    }
                    cursor.pen += glyph.advance * scale;
                }
            }
        }
        cursor.finish()
    }

    /// Lays out `text` as [`Glyphs::lay_out`] does, then moves each line by whole pixels to line
    /// up by `align` across the width of the widest, and returns the glyphs, the text's size and
    /// how many lines it takes.
    fn lay_out_aligned(
        &self,
        scale: f32,
        text: &str,
        max_width: Option<f32>,
        align: Align,
    ) -> (Vec<PlacedGlyph>, [f32; 2], usize) {
        let mut glyphs = Vec::new();
        let mut lines_of = Vec::new();
        let (size, lines) = self.lay_out(scale, text, max_width, |glyph, line| {
            glyphs.push(glyph);
            lines_of.push(line);
        });
        if align != Align::Left {
            // Each line's ink, which ends where its last glyph's does.
            let mut widths = vec![0.0f32; lines];
            for (glyph, &line) in glyphs.iter().zip(&lines_of) {
                widths[line] = widths[line].max(glyph.rect.right());
            }
            for (glyph, &line) in glyphs.iter_mut().zip(&lines_of) {
                glyph.rect.x += ((size[0] - widths[line]) * align.fraction()).round();
            }
        }
        (glyphs, size, lines)
    }

    /// How far the ink of `word` reaches from where it starts, as [`Glyphs::lay_out`] places it.
    fn ink_width(&self, scale: f32, word: &str) -> f32 {
        let mut pen = 0.0f32;
        let mut ink: f32 = 0.0;
        let mut before = None;
        for c in word.chars().filter(|c| !c.is_control()) {
            if let Some(glyph) = self.glyph(c) {
                if let Some(before) = before {
                    pen += self.kern(before, c) * scale;
                }
                before = Some(c);
                if glyph.has_ink() {
                    ink = ink.max(pen.round() + scale_rect(glyph.bounds, scale).right());
                }
                pen += glyph.advance * scale;
            }
        }
        ink
    }
}

/// How the lines of laid-out text line up: along their left edges, about their middles, or along
/// their right edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    /// Its name in scene files and scripts, such as `"center"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    /// The alignment named `name`, if there is one.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    /// How far across a line's width the point it lines up by is: 0, 0.5 or 1.
    pub fn fraction(self) -> f32 {
        match self {
            Self::Left => 0.0,
            Self::Center => 0.5,
            Self::Right => 1.0,
        }
    }
}

/// Where layout has got to.
#[derive(Default)]
struct Cursor {
    line_height: f32,
    line: usize,
    /// Where the next glyph goes on the line, before it's rounded to a whole pixel.
    pen: f32,
    /// The right edge of the ink on the line, once anything is drawn on it.
    ink: Option<f32>,
    /// The widest line's ink so far.
    width: f32,
}

impl Cursor {
    fn top(&self) -> f32 {
        self.line as f32 * self.line_height
    }

    fn line_is_empty(&self) -> bool {
        self.ink.is_none()
    }

    fn new_line(&mut self) {
        self.width = self.width.max(self.ink.unwrap_or(0.0));
        self.line += 1;
        self.pen = 0.0;
        self.ink = None;
    }

    fn finish(mut self) -> ([f32; 2], usize) {
        self.new_line();
        ([self.width, self.top()], self.line)
    }
}

fn scale_rect(rect: Rect, scale: f32) -> Rect {
    Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.w * scale,
        rect.h * scale,
    )
}

/// Text laid out by [`Font::layout`], ready to draw with
/// [`Canvas::draw_text_layout`](super::Canvas::draw_text_layout).
///
/// Laying text out once and drawing a growing number of its glyphs reveals it a little at a time,
/// as in RPG dialogue, without words jumping between lines as they appear.
#[derive(Clone, Debug)]
pub struct TextLayout {
    font: Font,
    glyphs: Vec<PlacedGlyph>,
    size: [f32; 2],
    lines: usize,
    align: Align,
}

impl TextLayout {
    /// The width and height the text takes up, measured as for [`Font::measure`].
    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    /// How its lines line up, across the width of the widest.
    pub fn align(&self) -> Align {
        self.align
    }

    /// How many glyphs there are to draw. Spaces and other characters without ink don't count.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    pub fn line_count(&self) -> usize {
        self.lines
    }

    pub(crate) fn font(&self) -> &Font {
        &self.font
    }

    pub(crate) fn glyphs(&self) -> &[PlacedGlyph] {
        &self.glyphs
    }
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;
    use crate::scene::FontCharacters;

    /// Metrics for a font whose glyphs are each 3x4 with a 1px gap, except `i` (1 wide) and a
    /// descending `g`; a space is 2 wide. Lines are 6 apart. `?` stands in for anything else.
    fn metrics() -> Metrics {
        let glyph = |w, h| Glyph {
            bounds: Rect::new(0.0, 0.0, w, h),
            uv: ([0.0; 2], [1.0; 2]),
            advance: w + 1.0,
            page: 0,
        };
        let mut glyphs: HashMap<char, Glyph> = ('a'..='f').map(|c| (c, glyph(3.0, 4.0))).collect();
        glyphs.insert('i', glyph(1.0, 4.0));
        glyphs.insert('g', glyph(3.0, 6.0));
        glyphs.insert(' ', Glyph::blank(2.0));
        Metrics {
            glyphs,
            fallback: Some(glyph(3.0, 4.0)),
            line_height: 6.0,
        }
    }

    fn lay_out(text: &str, max_width: Option<f32>) -> (Vec<Rect>, [f32; 2], usize) {
        let mut rects = Vec::new();
        let (size, lines) = metrics().lay_out(1.0, text, max_width, |g, _| rects.push(g.rect));
        (rects, size, lines)
    }

    /// Where each glyph of `text` starts across, with its lines lined up by `align`.
    fn aligned(text: &str, max_width: Option<f32>, align: Align) -> Vec<f32> {
        let (glyphs, _, _) = metrics().lay_out_aligned(1.0, text, max_width, align);
        glyphs.iter().map(|g| g.rect.x).collect()
    }

    #[test]
    fn lines_line_up_across_the_widest() {
        // "abc" is 11 wide, "a" 3 and "ai" 5.
        assert_eq!(
            aligned("abc\na\nai", None, Align::Left),
            [0., 4., 8., 0., 0., 4.]
        );
        assert_eq!(
            aligned("abc\na\nai", None, Align::Right),
            [0., 4., 8., 8., 6., 10.]
        );
        assert_eq!(
            aligned("abc\na\nai", None, Align::Center),
            [0., 4., 8., 4., 3., 7.]
        );
        // Wrapped lines line up the same way.
        assert_eq!(aligned("ab c", Some(8.0), Align::Right), [0., 4., 4.]);
        let (_, size, _) = metrics().lay_out_aligned(1.0, "abc\na", None, Align::Center);
        assert_eq!(size, [11.0, 12.0], "the same size");
    }

    #[test]
    fn alignments_have_names() {
        for align in [Align::Left, Align::Center, Align::Right] {
            assert_eq!(Align::from_name(align.name()), Some(align));
        }
        assert_eq!(Align::from_name("middle"), None);
    }

    #[test]
    fn glyphs_advance_along_the_line() {
        let (rects, size, lines) = lay_out("aib", None);
        let xs: Vec<f32> = rects.iter().map(|r| r.x).collect();
        assert_eq!(xs, [0.0, 4.0, 6.0]);
        assert_eq!(size, [9.0, 6.0], "to the end of the ink, one line high");
        assert_eq!(lines, 1);
    }

    #[test]
    fn spaces_move_the_pen_but_draw_nothing() {
        let (rects, size, _) = lay_out("a  b ", None);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[1].x, 8.0);
        assert_eq!(size[0], 11.0, "trailing spaces don't count");
    }

    #[test]
    fn newlines_start_new_lines() {
        let (rects, size, lines) = lay_out("ab\n\nc", None);
        assert_eq!(rects[2], Rect::new(0.0, 12.0, 3.0, 4.0));
        assert_eq!(size, [7.0, 18.0]);
        assert_eq!(lines, 3);
    }

    #[test]
    fn lines_wrap_between_words() {
        // "ab cd" is 17 wide: it fits 17, but not 16.
        assert_eq!(lay_out("ab cd", Some(17.0)).2, 1);
        let (rects, size, lines) = lay_out("ab cd", Some(16.0));
        assert_eq!(lines, 2);
        assert_eq!(
            rects[2],
            Rect::new(0.0, 6.0, 3.0, 4.0),
            "the space is dropped"
        );
        assert_eq!(size, [7.0, 12.0]);
    }

    #[test]
    fn leading_spaces_stay() {
        let (rects, _, _) = lay_out("  a", Some(100.0));
        assert_eq!(rects[0].x, 4.0);
    }

    #[test]
    fn words_longer_than_a_line_break_between_characters() {
        let (rects, size, lines) = lay_out("abcdef", Some(10.0));
        let positions: Vec<[f32; 2]> = rects.iter().map(|r| [r.x, r.y]).collect();
        assert_eq!(
            positions,
            [
                [0.0, 0.0],
                [4.0, 0.0],
                [0.0, 6.0],
                [4.0, 6.0],
                [0.0, 12.0],
                [4.0, 12.0]
            ]
        );
        assert_eq!(size, [7.0, 18.0]);
        assert_eq!(lines, 3);
    }

    #[test]
    fn leading_spaces_do_not_push_words_past_the_width() {
        // "ab" is 7 wide, but 11 after the leading spaces.
        let (rects, size, lines) = lay_out("  ab", Some(8.0));
        let positions: Vec<[f32; 2]> = rects.iter().map(|r| [r.x, r.y]).collect();
        assert_eq!(positions, [[4.0, 0.0], [0.0, 6.0]]);
        assert_eq!(size, [7.0, 12.0]);
        assert_eq!(lines, 2);
    }

    #[test]
    fn unknown_characters_use_the_fallback() {
        let (rects, _, _) = lay_out("a\u{263a}b", None);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[2].x, 8.0);
    }

    #[test]
    fn control_characters_are_skipped() {
        let (rects, _, _) = lay_out("a\tb\r", None);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[1].x, 4.0);
    }

    #[test]
    fn descenders_hang_below_the_glyphs_around_them() {
        let (rects, _, _) = lay_out("ag", None);
        assert_eq!(rects[1], Rect::new(4.0, 0.0, 3.0, 6.0));
    }

    #[test]
    fn scaling_scales_everything() {
        let mut rects = Vec::new();
        let (size, _) = metrics().lay_out(2.0, "a b", Some(18.0), |g, _| rects.push(g.rect));
        assert_eq!(rects[1], Rect::new(12.0, 0.0, 6.0, 8.0));
        assert_eq!(size, [18.0, 12.0]);
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        let (rects, size, lines) = lay_out("", Some(10.0));
        assert!(rects.is_empty());
        assert_eq!(size, [0.0, 6.0]);
        assert_eq!(lines, 1);
    }

    /// [`metrics`] with `a` and `b` advancing 3.4, and `b` kerned 1.5 closer after `a`.
    struct Kerned(Metrics);

    impl Glyphs for Kerned {
        fn glyph(&self, c: char) -> Option<Glyph> {
            let mut glyph = self.0.glyph(c)?;
            if c == 'a' || c == 'b' {
                glyph.advance = 3.4;
            }
            Some(glyph)
        }

        fn kern(&self, before: char, c: char) -> f32 {
            if (before, c) == ('a', 'b') { -1.5 } else { 0.0 }
        }

        fn line_height(&self) -> f32 {
            self.0.line_height
        }
    }

    #[test]
    fn fractional_advances_and_kerning_place_glyphs_on_whole_pixels() {
        let font = Kerned(metrics());
        let mut xs = Vec::new();
        // Pens at 0, 3.4, 6.8 - 1.5 = 5.3 for the kerned `b`, then 8.7.
        let (size, _) = font.lay_out(1.0, "aabb", None, |g, _| xs.push(g.rect.x));
        assert_eq!(xs, [0.0, 3.0, 5.0, 9.0]);
        assert_eq!(size[0], 12.0, "to the last glyph's ink, where it's drawn");
        assert_eq!(font.ink_width(1.0, "aabb"), 12.0, "measured as it's placed");
        // Words don't kern across spaces.
        let mut xs = Vec::new();
        font.lay_out(1.0, "a b", None, |g, _| xs.push(g.rect.x));
        assert_eq!(xs, [0.0, 5.0]);
    }

    /// A bitmap font's image: cells 4x3, two across and two down, with ink in each column set
    /// in `inked`, by cell, `[column, row]`.
    fn image(inked: &[([u32; 2], &[u32])]) -> RgbaImage {
        let mut image = RgbaImage::new(8, 6);
        for &([column, row], xs) in inked {
            for &x in xs {
                image.put_pixel(column * 4 + x, row * 3 + 1, Rgba([255, 255, 255, 128]));
            }
        }
        image
    }

    fn def(characters: FontCharacters) -> BitmapFontDef {
        BitmapFontDef {
            image: "font.png".into(),
            cell: [4, 3],
            characters,
            spacing: 1,
            space: 2,
            line_height: 4,
            monospace: false,
        }
    }

    #[test]
    fn bitmap_glyphs_are_as_wide_as_their_ink() {
        let image = image(&[([0, 0], &[1, 2]), ([1, 0], &[0]), ([0, 1], &[3])]);
        let metrics = bitmap_metrics(&def(FontCharacters::Cells("ab?".into())), &image).unwrap();
        let a = metrics.glyph('a').unwrap();
        assert_eq!(a.bounds, Rect::new(0.0, 0.0, 2.0, 3.0));
        assert_eq!(
            a.uv,
            ([1.0 / 8.0, 0.0], [3.0 / 8.0, 0.5]),
            "its inked columns"
        );
        assert_eq!(a.advance, 3.0, "and the spacing");
        assert_eq!(metrics.glyph('b').unwrap().bounds.w, 1.0);
        let question = metrics.glyph('?').unwrap();
        assert_eq!(question.uv.0, [3.0 / 8.0, 0.5], "on the next row of cells");
        assert_eq!(
            metrics.glyph('z'),
            Some(question),
            "drawn for what it lacks"
        );
        assert_eq!(metrics.glyph(' ').unwrap().advance, 2.0);
        assert_eq!(metrics.line_height, 4.0);

        let monospace = BitmapFontDef {
            monospace: true,
            ..def(FontCharacters::Cells("ab?".into()))
        };
        let metrics = bitmap_metrics(&monospace, &image).unwrap();
        assert_eq!(metrics.glyph('b').unwrap().bounds.w, 4.0, "a whole cell");
    }

    #[test]
    fn bitmap_rows_list_their_cells_and_spaces_skip_them() {
        let image = image(&[([1, 0], &[0]), ([0, 1], &[1])]);
        let rows = FontCharacters::Rows(vec![" a".into(), "b".into()]);
        let metrics = bitmap_metrics(&def(rows), &image).unwrap();
        assert_eq!(metrics.glyph('a').unwrap().uv.0, [4.0 / 8.0, 0.0]);
        assert_eq!(metrics.glyph('b').unwrap().uv.0, [1.0 / 8.0, 0.5]);
        assert_eq!(metrics.fallback, None, "without a `?`");
    }

    #[test]
    fn bitmap_fonts_that_cannot_be_cut_out_are_errors() {
        let gap = image(&[([0, 0], &[0]), ([1, 0], &[0]), ([0, 1], &[0])]);
        let err = bitmap_metrics(&def(FontCharacters::Cells("abcd".into())), &gap).unwrap_err();
        assert!(
            err.contains("the cell for `d`, column 1 and row 1"),
            "{err}"
        );

        let full = image(&[
            ([0, 0], &[0]),
            ([1, 0], &[0]),
            ([0, 1], &[0]),
            ([1, 1], &[0]),
        ]);
        for (characters, problem) in [
            (
                FontCharacters::Cells("abcde".into()),
                "`e` would be in column 0 and row 2",
            ),
            (
                FontCharacters::Rows(vec!["abc".into()]),
                "`c` would be in column 2 and row 0",
            ),
        ] {
            let err = bitmap_metrics(&def(characters), &full).unwrap_err();
            assert!(err.contains(problem), "{err}");
        }
        let small = BitmapFontDef {
            cell: [16, 16],
            ..def(FontCharacters::Cells("a".into()))
        };
        let err = bitmap_metrics(&small, &gap).unwrap_err();
        assert!(err.contains("smaller than a cell"), "{err}");
    }

    #[test]
    fn shelves_pack_glyphs_a_pixel_apart_in_rows() {
        let mut shelves = Shelves::new(10);
        assert_eq!(shelves.place([4, 3]), Some([0, 0]));
        assert_eq!(shelves.place([4, 5]), Some([5, 0]));
        assert_eq!(shelves.place([2, 2]), Some([0, 6]), "below the tallest");
        assert_eq!(shelves.place([10, 3]), None, "no room left below");
        assert_eq!(Shelves::new(4).place([5, 1]), None, "wider than the page");
    }

    #[test]
    fn pages_hold_hundreds_of_glyphs() {
        assert_eq!(page_size(10.0), 256);
        assert_eq!(page_size(40.0), 1024);
        assert_eq!(page_size(1e9), 2048);
        assert_eq!(page_size(f32::NAN), 256);
    }

    /// Hack, the TrueType font the lettering example shows.
    const HACK: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/lettering/fonts/Hack-Regular.ttf"
    ));

    #[test]
    fn vector_glyphs_sit_on_the_baseline_smooth_or_crisp() {
        let font = FontArc::try_from_vec(HACK.to_vec()).unwrap();
        let sized = FaceSize::new(&font, 13.0);
        // Its ascent and descent are 2384 of its 2048 units to the em.
        assert_eq!(sized.line_height, 15.0);
        assert_eq!(sized.page_size, 256);
        let at = ab_glyph::point(0.0, sized.baseline);
        let outlined = |c| {
            let glyph = font.glyph_id(c).with_scale_and_position(sized.scale, at);
            font.outline_glyph(glyph).unwrap()
        };
        let x = outlined('x').px_bounds();
        assert_eq!(x.max.y, sized.baseline, "ending on the baseline");
        assert!(
            outlined('p').px_bounds().max.y > sized.baseline,
            "descending"
        );
        assert!(outlined('d').px_bounds().min.y < x.min.y, "ascending");

        let a = outlined('a');
        let bounds = a.px_bounds();
        let size = [bounds.width() as u32, bounds.height() as u32];
        let opacities = |smooth| {
            let pixels = glyph_pixels(&a, size, smooth);
            pixels.chunks(4).map(|pixel| pixel[3]).collect::<Vec<u8>>()
        };
        let smooth = opacities(true);
        assert!(smooth.iter().any(|&a| a > 0 && a < 255), "partly covered");
        let crisp = opacities(false);
        assert!(crisp.iter().all(|&a| a == 0 || a == 255));
        assert!(crisp.iter().filter(|&&a| a == 255).count() > 10);
    }

    #[test]
    fn crisp_glyphs_are_opaque_or_clear() {
        assert_eq!(opacity(0.25, true), 64);
        assert_eq!(opacity(1.2, true), 255);
        assert_eq!(opacity(0.49, false), 0);
        assert_eq!(opacity(0.5, false), 255);
    }
}
