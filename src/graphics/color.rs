/// An RGBA color with components in `0.0..=1.0`.
///
/// Components are in sRGB space, the space colors are normally authored in (color pickers, hex
/// codes, image files). The renderer converts to linear space before blending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba8(r, g, b, 255)
    }

    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// The components as bytes, still in sRGB, each clamped to `0.0..=1.0` and rounded.
    pub(crate) fn to_rgba8(self) -> [u8; 4] {
        // Adding a half and truncating rounds, without `f32::round`, which is a function call
        // where the CPU has no instruction for it.
        [self.r, self.g, self.b, self.a].map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
    }

    /// Converts to linear-space components for the GPU. Alpha is already linear.
    pub(crate) fn to_linear(self) -> [f32; 4] {
        [
            srgb_to_linear(self.r),
            srgb_to_linear(self.g),
            srgb_to_linear(self.b),
            self.a,
        ]
    }

    /// The color of linear-space components, as model files give them. Alpha is already linear.
    #[cfg(feature = "models")]
    pub(crate) fn from_linear([r, g, b, a]: [f32; 4]) -> Self {
        Self::rgba(linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(b), a)
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(feature = "models")]
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_conversion_keeps_endpoints_and_alpha() {
        assert_eq!(
            Color::rgba(0.0, 1.0, 0.0, 0.5).to_linear(),
            [0.0, 1.0, 0.0, 0.5]
        );
    }

    #[test]
    fn bytes_round_and_clamp() {
        assert_eq!(
            Color::rgba(0.5, 1.5, -1.0, 0.25).to_rgba8(),
            [128, 255, 0, 64]
        );
        assert_eq!(
            Color::rgb8(0xf2, 0xc1, 0x4e).to_rgba8(),
            [0xf2, 0xc1, 0x4e, 0xff]
        );
    }

    #[test]
    fn linear_conversion_darkens_midtones() {
        let [r, ..] = Color::rgb(0.5, 0.0, 0.0).to_linear();
        assert!((r - 0.214).abs() < 0.001);
    }

    #[cfg(feature = "models")]
    #[test]
    fn linear_conversion_goes_both_ways() {
        for c in [0.0, 0.002, 0.04, 0.25, 0.5, 0.8, 1.0] {
            let color = Color::rgba(c, c / 2.0, 1.0 - c, c);
            let back = Color::from_linear(color.to_linear());
            let [r, g, b, a] = [back.r, back.g, back.b, back.a];
            assert!(
                (r - c).abs() < 1e-5 && (g - c / 2.0).abs() < 1e-5,
                "{back:?}"
            );
            assert!((b - (1.0 - c)).abs() < 1e-5 && a == c, "{back:?}");
        }
    }
}
