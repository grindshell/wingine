use std::{collections::HashMap, sync::Arc};

use super::{Texture, default_font, renderer::Renderer};
use crate::math::Rect;

/// A typeface ready to draw: its glyph images packed into one texture, and their measurements.
///
/// Draw text with [`Canvas::draw_text`](super::Canvas::draw_text), or lay it out first with
/// [`Font::layout`] to wrap it or reveal it a little at a time. Cloning is cheap; clones share
/// the glyphs. The engine has a built-in pixel font, from
/// [`Context::default_font`](crate::Context::default_font).
#[derive(Clone, Debug)]
pub struct Font {
    data: Arc<FontData>,
    scale: f32,
}

#[derive(Debug)]
struct FontData {
    texture: Texture,
    metrics: Metrics,
}

/// Everything about a font that layout needs, apart from the texture.
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
}

impl Glyph {
    fn has_ink(&self) -> bool {
        self.bounds.w > 0.0 && self.bounds.h > 0.0
    }
}

/// A glyph placed by layout, relative to the text's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacedGlyph {
    pub rect: Rect,
    pub uv: ([f32; 2], [f32; 2]),
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
            };
            metrics.glyphs.insert(*c, glyph);
        }
        for &(alias, original) in default_font::ALIASES {
            let glyph = metrics.glyphs[&original];
            metrics.glyphs.insert(alias, glyph);
        }
        metrics.glyphs.insert(
            ' ',
            Glyph {
                bounds: Rect::default(),
                uv: ([0.0; 2], [0.0; 2]),
                advance: default_font::SPACE_ADVANCE,
            },
        );
        metrics.fallback = metrics.glyphs.get(&'?').copied();

        let texture = renderer.create_texture(device, queue, width as u32, height as u32, &rgba);
        Self {
            data: Arc::new(FontData { texture, metrics }),
            scale: 1.0,
        }
    }

    /// The distance from one line of text to the next.
    pub fn line_height(&self) -> f32 {
        self.data.metrics.line_height * self.scale
    }

    /// This font drawn `scale` times as large, such as for a title. Whole numbers keep pixel
    /// fonts crisp.
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
        Ok(Self {
            data: Arc::clone(&self.data),
            scale: self.scale * scale,
        })
    }

    /// Lays out `text` to draw with [`Canvas::draw_text_layout`](super::Canvas::draw_text_layout).
    /// With a `max_width`, lines wrap between words to fit it, and words too long for a line of
    /// their own break between characters. `\n` always starts a new line.
    pub fn layout(&self, text: &str, max_width: Option<f32>) -> TextLayout {
        let mut glyphs = Vec::new();
        let (size, lines) = self
            .data
            .metrics
            .lay_out(self.scale, text, max_width, |glyph| glyphs.push(glyph));
        TextLayout {
            font: self.clone(),
            glyphs,
            size,
            lines,
        }
    }

    /// The width and height of `text` drawn without wrapping. The width runs to the right edge of
    /// the last glyph's ink on the longest line; the height is a whole number of lines.
    pub fn measure(&self, text: &str) -> [f32; 2] {
        self.data.metrics.lay_out(self.scale, text, None, |_| {}).0
    }

    pub(crate) fn texture(&self) -> &Texture {
        &self.data.texture
    }

    /// Places each glyph of `text` without wrapping, passing it to `place`.
    pub(crate) fn place_glyphs(&self, text: &str, place: impl FnMut(PlacedGlyph)) {
        self.data.metrics.lay_out(self.scale, text, None, place);
    }
}

impl Metrics {
    fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyphs.get(&c).or(self.fallback.as_ref())
    }

    /// Lays out `text` at `scale`, passing each glyph with something to draw to `place`, and
    /// returns the text's size and how many lines it takes.
    fn lay_out(
        &self,
        scale: f32,
        text: &str,
        max_width: Option<f32>,
        mut place: impl FnMut(PlacedGlyph),
    ) -> ([f32; 2], usize) {
        let max_width = max_width.unwrap_or(f32::INFINITY);
        let space = self.glyph(' ').map_or(0.0, |glyph| glyph.advance * scale);
        let mut cursor = Cursor {
            line_height: self.line_height * scale,
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
                if !cursor.line_is_empty() && cursor.pen + spaces + word_ink > max_width {
                    cursor.new_line();
                } else {
                    cursor.pen += spaces;
                }
                spaces = 0.0;
                // Only a word that doesn't fit even where a line starts breaks between
                // characters: one too long for any line, or one after leading spaces.
                let breakable = cursor.pen + word_ink > max_width;
                for c in word.chars().filter(|c| !c.is_control()) {
                    let Some(glyph) = self.glyph(c) else {
                        continue;
                    };
                    let bounds = scale_rect(glyph.bounds, scale);
                    // Something is on the line, leading spaces at least, so it can wrap.
                    if breakable && cursor.pen > 0.0 && cursor.pen + bounds.right() > max_width {
                        cursor.new_line();
                    }
                    if glyph.has_ink() {
                        let rect = Rect::new(
                            cursor.pen + bounds.x,
                            cursor.top() + bounds.y,
                            bounds.w,
                            bounds.h,
                        );
                        place(PlacedGlyph { rect, uv: glyph.uv });
                        cursor.ink = Some(cursor.pen + bounds.right());
                    }
                    cursor.pen += glyph.advance * scale;
                }
            }
        }
        cursor.finish()
    }

    /// How far the ink of `word` reaches from where it starts.
    fn ink_width(&self, scale: f32, word: &str) -> f32 {
        let mut pen = 0.0;
        let mut ink: f32 = 0.0;
        for c in word.chars().filter(|c| !c.is_control()) {
            if let Some(glyph) = self.glyph(c) {
                if glyph.has_ink() {
                    ink = ink.max(pen + scale_rect(glyph.bounds, scale).right());
                }
                pen += glyph.advance * scale;
            }
        }
        ink
    }
}

/// Where layout has got to.
#[derive(Default)]
struct Cursor {
    line_height: f32,
    line: usize,
    /// Where the next glyph goes on the line.
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
}

impl TextLayout {
    /// The width and height the text takes up, measured as for [`Font::measure`].
    pub fn size(&self) -> [f32; 2] {
        self.size
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
    use super::*;

    /// Metrics for a font whose glyphs are each 3x4 with a 1px gap, except `i` (1 wide) and a
    /// descending `g`; a space is 2 wide. Lines are 6 apart. `?` stands in for anything else.
    fn metrics() -> Metrics {
        let glyph = |w, h| Glyph {
            bounds: Rect::new(0.0, 0.0, w, h),
            uv: ([0.0; 2], [1.0; 2]),
            advance: w + 1.0,
        };
        let mut glyphs: HashMap<char, Glyph> = ('a'..='f').map(|c| (c, glyph(3.0, 4.0))).collect();
        glyphs.insert('i', glyph(1.0, 4.0));
        glyphs.insert('g', glyph(3.0, 6.0));
        glyphs.insert(
            ' ',
            Glyph {
                bounds: Rect::default(),
                uv: ([0.0; 2], [0.0; 2]),
                advance: 2.0,
            },
        );
        Metrics {
            glyphs,
            fallback: Some(glyph(3.0, 4.0)),
            line_height: 6.0,
        }
    }

    fn lay_out(text: &str, max_width: Option<f32>) -> (Vec<Rect>, [f32; 2], usize) {
        let mut rects = Vec::new();
        let (size, lines) = metrics().lay_out(1.0, text, max_width, |g| rects.push(g.rect));
        (rects, size, lines)
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
        let (size, _) = metrics().lay_out(2.0, "a b", Some(18.0), |g| rects.push(g.rect));
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
}
