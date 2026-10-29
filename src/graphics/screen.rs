use super::{
    Canvas, Color,
    renderer::{RenderTarget, Renderer},
};
use crate::math::Rect;

/// The size of the canvas a game draws on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Resolution {
    /// The window's size in logical pixels, so the view grows with the window.
    #[default]
    Window,
    /// A fixed size in pixels, scaled up to the window by the largest whole number that fits,
    /// with black bars filling the space left over. This keeps pixel art crisp and shows the same
    /// area on every screen. A window smaller than this size shows the canvas scaled down to fit.
    Fixed { width: u32, height: u32 },
}

/// Draws finished canvases onto an output (the window, or a headless frame) at a [`Resolution`].
pub(crate) struct Screen {
    resolution: Resolution,
    /// For a fixed resolution: the canvas renders here, then is scaled onto the output.
    target: Option<RenderTarget>,
    /// Draws `target` onto the output.
    present: Canvas,
}

impl Screen {
    pub fn new(resolution: Resolution) -> Self {
        if let Resolution::Fixed { width, height } = resolution {
            assert!(
                width > 0 && height > 0,
                "a fixed resolution must be at least 1x1"
            );
        }
        Self {
            resolution,
            target: None,
            present: Canvas::new(),
        }
    }

    /// The canvas size for an output of `output` physical pixels at `scale_factor`.
    pub fn canvas_size(&self, output: [u32; 2], scale_factor: f64) -> [f32; 2] {
        match self.resolution {
            Resolution::Window => output.map(|v| (f64::from(v) / scale_factor) as f32),
            Resolution::Fixed { width, height } => [width as f32, height as f32],
        }
    }

    /// Renders `canvas` onto `output`, which is `output_size` physical pixels.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut Renderer,
        canvas: &Canvas,
        output: &wgpu::TextureView,
        output_size: [u32; 2],
    ) {
        let Resolution::Fixed { width, height } = self.resolution else {
            renderer.render(device, queue, canvas, output);
            return;
        };
        let target = self
            .target
            .get_or_insert_with(|| renderer.create_render_target(device, width, height));
        renderer.render(device, queue, canvas, &target.view);

        // `begin` clears to black, which fills the bars around the scaled canvas.
        self.present
            .begin(output_size[0] as f32, output_size[1] as f32);
        let dest = viewport([width, height], output_size);
        self.present
            .draw_sprite(&target.texture.sprite(), dest, Color::WHITE);
        renderer.render(device, queue, &self.present, output);
    }
}

/// Where a `resolution`-sized canvas appears on an `output`-sized target: scaled by the largest
/// whole number that fits (or scaled down to fit, if even 1x doesn't) and centered on whole pixels.
fn viewport(resolution: [u32; 2], output: [u32; 2]) -> Rect {
    let [width, height] = resolution.map(|v| v as f32);
    let [output_width, output_height] = output.map(|v| v as f32);
    let fit = (output_width / width).min(output_height / height);
    let scale = if fit >= 1.0 { fit.floor() } else { fit };
    let scaled = [(width * scale).round(), (height * scale).round()];
    Rect::new(
        ((output_width - scaled[0]) / 2.0).floor(),
        ((output_height - scaled[1]) / 2.0).floor(),
        scaled[0],
        scaled[1],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_multiple_fills_the_output() {
        assert_eq!(
            viewport([256, 144], [1280, 720]),
            Rect::new(0.0, 0.0, 1280.0, 720.0)
        );
    }

    #[test]
    fn scales_by_whole_numbers_and_centers() {
        // 3x fits (96x48); 4x would be too tall.
        assert_eq!(
            viewport([32, 16], [100, 60]),
            Rect::new(2.0, 6.0, 96.0, 48.0)
        );
    }

    #[test]
    fn scales_down_when_the_output_is_smaller() {
        assert_eq!(
            viewport([200, 100], [100, 100]),
            Rect::new(0.0, 25.0, 100.0, 50.0)
        );
    }

    #[test]
    fn window_resolution_uses_logical_pixels() {
        let screen = Screen::new(Resolution::Window);
        assert_eq!(screen.canvas_size([2560, 1440], 2.0), [1280.0, 720.0]);
    }

    #[test]
    fn fixed_resolution_ignores_the_output() {
        let screen = Screen::new(Resolution::Fixed {
            width: 320,
            height: 180,
        });
        assert_eq!(screen.canvas_size([2560, 1440], 2.0), [320.0, 180.0]);
    }
}
