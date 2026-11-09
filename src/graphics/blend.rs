/// How what's drawn mixes with what's under it. Blending is in linear space, as all drawing is.
///
/// Set with [`Canvas::set_blend`](super::Canvas::set_blend), which applies to everything drawn
/// after it, until the frame ends or it's set again. Meshes in 3D views don't blend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Blend {
    /// Covers what's under it as much as it's opaque.
    #[default]
    Normal,
    /// Adds its color, scaled by its alpha, to what's under it, so it only ever brightens: for
    /// fire, sparks and glows.
    Add,
    /// Multiplies what's under it by its color, where it's opaque, so it only ever darkens: for
    /// shadows and night.
    Multiply,
    /// Brightens what's under it as much as its color is bright, like two pictures projected on
    /// one screen, never past white: for soft light.
    Screen,
}

impl Blend {
    pub const ALL: [Self; 4] = [Self::Normal, Self::Add, Self::Multiply, Self::Screen];

    /// Its name in scene files and scripts, such as `"add"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Add => "add",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
        }
    }

    /// The blend mode named `name`, if there is one.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|blend| blend.name() == name)
    }

    /// How a renderer blends a premultiplied color into the target in this mode.
    pub(crate) fn state(self) -> wgpu::BlendState {
        use wgpu::{BlendComponent, BlendFactor, BlendOperation};
        let color = |src_factor, dst_factor| BlendComponent {
            src_factor,
            dst_factor,
            operation: BlendOperation::Add,
        };
        // Alpha covers as it does in normal blending, whatever the mode.
        let alpha = color(BlendFactor::One, BlendFactor::OneMinusSrcAlpha);
        let color = match self {
            Self::Normal => alpha,
            Self::Add => color(BlendFactor::One, BlendFactor::One),
            Self::Multiply => color(BlendFactor::Dst, BlendFactor::OneMinusSrcAlpha),
            Self::Screen => color(BlendFactor::One, BlendFactor::OneMinusSrc),
        };
        wgpu::BlendState { color, alpha }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_modes_are_found_by_name() {
        for blend in Blend::ALL {
            assert_eq!(Blend::from_name(blend.name()), Some(blend));
        }
        assert_eq!(Blend::from_name("alpha"), None);
    }

    #[test]
    fn normal_blending_is_premultiplied_alpha_blending() {
        assert_eq!(
            Blend::Normal.state(),
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
        );
    }
}
