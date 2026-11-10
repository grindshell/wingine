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
    /// A fixed size in pixels, scaled to the window as its [`Scaling`] says: by default, up by the
    /// largest whole number that fits, with black bars filling the space left over. This keeps
    /// pixel art crisp and shows the same area on every screen.
    Fixed { width: u32, height: u32 },
}

/// How a canvas of a fixed [`Resolution`] fills a window, or a screen, whose size isn't a whole
/// number of times its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scaling {
    /// Scaled up by the largest whole number that fits, centered, with black bars filling the
    /// space left over, so every texel is a square of the same pixels. A window smaller than the
    /// canvas shows it scaled down to fit, as `Fit` does.
    #[default]
    Whole,
    /// Scaled as large as fits, keeping its shape, so it fills the window's height or its width,
    /// centered with black bars along the other. Texels stay square, but differ by a pixel in
    /// size, so a texel 1 wide is 4 pixels wide in places and 5 in others at 4.5 times.
    Fit,
    /// Stretched to fill the window both ways, with no bars, so texels are as much wider than
    /// they're tall as the window's shape is wider than the canvas's.
    Stretch,
}

impl Scaling {
    pub const ALL: [Self; 3] = [Self::Whole, Self::Fit, Self::Stretch];

    /// The scaling's name in Luau and in `conf.luau`: `"whole"`, `"fit"` or `"stretch"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Fit => "fit",
            Self::Stretch => "stretch",
        }
    }
}

impl Resolution {
    /// Why the GPU of `device` can't render a canvas at this resolution, if it can't.
    pub(crate) fn problem(self, device: &wgpu::Device) -> Option<String> {
        match self {
            Self::Window => None,
            Self::Fixed { width, height } => {
                super::gpu::texture_size_problem(device, "a fixed resolution", [width, height])
            }
        }
    }
}

/// Draws finished canvases onto an output (the window, or a headless frame) at a [`Resolution`],
/// scaled as its [`Scaling`] says.
pub(crate) struct Screen {
    resolution: Resolution,
    scaling: Scaling,
    /// For a fixed resolution: the canvas renders here, then is scaled onto the output.
    target: Option<RenderTarget>,
    /// Draws `target` onto the output.
    present: Canvas,
}

impl Screen {
    pub fn new(resolution: Resolution) -> Self {
        let mut screen = Self {
            resolution: Resolution::Window,
            scaling: Scaling::default(),
            target: None,
            present: Canvas::new(),
        };
        screen.set_resolution(resolution);
        screen
    }

    /// Draws later canvases at `resolution`, keeping the scaling.
    pub fn set_resolution(&mut self, resolution: Resolution) {
        if let Resolution::Fixed { width, height } = resolution {
            assert!(
                width > 0 && height > 0,
                "a fixed resolution must be at least 1x1"
            );
        }
        if resolution != self.resolution {
            self.resolution = resolution;
            self.target = None;
        }
    }

    pub fn scaling(&self) -> Scaling {
        self.scaling
    }

    /// Scales later canvases of a fixed resolution by `scaling`. It changes nothing without one.
    pub fn set_scaling(&mut self, scaling: Scaling) {
        self.scaling = scaling;
    }

    /// The canvas size for an output of `output` physical pixels at `scale_factor`.
    pub fn canvas_size(&self, output: [u32; 2], scale_factor: f64) -> [f32; 2] {
        match self.resolution {
            Resolution::Window => output.map(|v| (f64::from(v) / scale_factor) as f32),
            Resolution::Fixed { width, height } => [width as f32, height as f32],
        }
    }

    /// Converts a point on an output of `output` physical pixels at `scale_factor`, such as the
    /// cursor position, to canvas pixels.
    pub fn to_canvas(&self, point: [f64; 2], output: [u32; 2], scale_factor: f64) -> [f32; 2] {
        match self.resolution {
            Resolution::Window => point.map(|v| (v / scale_factor) as f32),
            Resolution::Fixed { width, height } => {
                let viewport = viewport([width, height], output, self.scaling);
                [
                    (point[0] as f32 - viewport.x) * width as f32 / viewport.w,
                    (point[1] as f32 - viewport.y) * height as f32 / viewport.h,
                ]
            }
        }
    }

    /// Converts a point in canvas pixels to a point on an output of `output` physical pixels at
    /// `scale_factor`: the inverse of [`Screen::to_canvas`].
    pub fn to_output(&self, point: [f32; 2], output: [u32; 2], scale_factor: f64) -> [f64; 2] {
        match self.resolution {
            Resolution::Window => point.map(|v| f64::from(v) * scale_factor),
            Resolution::Fixed { width, height } => {
                let viewport = viewport([width, height], output, self.scaling);
                [
                    f64::from(viewport.x + point[0] * viewport.w / width as f32),
                    f64::from(viewport.y + point[1] * viewport.h / height as f32),
                ]
            }
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
            renderer.render(device, queue, canvas, output, output_size);
            return;
        };
        let target = self
            .target
            .get_or_insert_with(|| renderer.create_render_target(device, width, height));
        renderer.render(device, queue, canvas, &target.view, [width, height]);

        // `begin` clears to black, which fills the bars around the scaled canvas.
        self.present
            .begin(output_size[0] as f32, output_size[1] as f32);
        let dest = viewport([width, height], output_size, self.scaling);
        self.present
            .draw_sprite(&target.texture.sprite(), dest, Color::WHITE);
        renderer.render(device, queue, &self.present, output, output_size);
    }
}

/// Where a `resolution`-sized canvas appears on an `output`-sized target, scaled by `scaling`:
/// filling it, or as large as fits keeping its shape, by a whole number unless it's
/// [`Scaling::Fit`] (or even 1x doesn't fit), and centered on whole pixels.
fn viewport(resolution: [u32; 2], output: [u32; 2], scaling: Scaling) -> Rect {
    let [width, height] = resolution.map(|v| v as f32);
    let [output_width, output_height] = output.map(|v| v as f32);
    if scaling == Scaling::Stretch {
        return Rect::new(0.0, 0.0, output_width, output_height);
    }
    let fit = (output_width / width).min(output_height / height);
    let scale = if scaling == Scaling::Whole && fit >= 1.0 {
        fit.floor()
    } else {
        fit
    };
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
        for scaling in Scaling::ALL {
            assert_eq!(
                viewport([256, 144], [1280, 720], scaling),
                Rect::new(0.0, 0.0, 1280.0, 720.0),
                "{scaling:?}"
            );
        }
    }

    #[test]
    fn scales_by_whole_numbers_and_centers() {
        // 3x fits (96x48); 4x would be too tall.
        assert_eq!(
            viewport([32, 16], [100, 60], Scaling::Whole),
            Rect::new(2.0, 6.0, 96.0, 48.0)
        );
    }

    #[test]
    fn scales_down_when_the_output_is_smaller() {
        for scaling in [Scaling::Whole, Scaling::Fit] {
            assert_eq!(
                viewport([200, 100], [100, 100], scaling),
                Rect::new(0.0, 25.0, 100.0, 50.0),
                "{scaling:?}"
            );
        }
    }

    #[test]
    fn fitting_fills_one_way_keeping_the_shape() {
        // 3.125x across would be too tall; 3.75x down fits.
        assert_eq!(
            viewport([32, 16], [100, 60], Scaling::Fit),
            Rect::new(0.0, 5.0, 100.0, 50.0)
        );
        // 320x180 on a 1366x768 laptop screen, filling its height at 4.27x.
        assert_eq!(
            viewport([320, 180], [1366, 768], Scaling::Fit),
            Rect::new(0.0, 0.0, 1365.0, 768.0)
        );
        // On a 4:3 screen, filling its width, with bars above and below.
        assert_eq!(
            viewport([320, 180], [1024, 768], Scaling::Fit),
            Rect::new(0.0, 96.0, 1024.0, 576.0)
        );
    }

    #[test]
    fn stretching_fills_both_ways() {
        assert_eq!(
            viewport([32, 16], [100, 60], Scaling::Stretch),
            Rect::new(0.0, 0.0, 100.0, 60.0)
        );
        assert_eq!(
            viewport([200, 100], [50, 70], Scaling::Stretch),
            Rect::new(0.0, 0.0, 50.0, 70.0)
        );
    }

    #[test]
    fn output_points_map_through_each_scaling() {
        let mut screen = Screen::new(Resolution::Fixed {
            width: 32,
            height: 16,
        });
        screen.set_scaling(Scaling::Fit);
        // The canvas fills (0, 5) to (100, 55) of a 100x60 output, at 3.125x.
        assert_eq!(screen.to_canvas([0.0, 5.0], [100, 60], 1.0), [0.0, 0.0]);
        assert_eq!(screen.to_canvas([50.0, 30.0], [100, 60], 1.0), [16.0, 8.0]);
        assert_eq!(
            screen.to_output([32.0, 16.0], [100, 60], 1.0),
            [100.0, 55.0]
        );

        // Stretched, 3.125x across and 3.75x down.
        screen.set_scaling(Scaling::Stretch);
        assert_eq!(screen.to_canvas([50.0, 30.0], [100, 60], 1.0), [16.0, 8.0]);
        assert_eq!(screen.to_canvas([25.0, 15.0], [100, 60], 1.0), [8.0, 4.0]);
        assert_eq!(
            screen.to_output([32.0, 16.0], [100, 60], 1.0),
            [100.0, 60.0]
        );
    }

    #[test]
    fn scaling_changes_nothing_without_a_fixed_resolution() {
        let mut screen = Screen::new(Resolution::Window);
        screen.set_scaling(Scaling::Stretch);
        assert_eq!(screen.canvas_size([800, 600], 1.0), [800.0, 600.0]);
        assert_eq!(
            screen.to_canvas([300.0, 150.0], [800, 600], 1.0),
            [300.0, 150.0]
        );
    }

    #[test]
    fn changing_the_resolution_keeps_the_scaling() {
        let mut screen = Screen::new(Resolution::Window);
        screen.set_scaling(Scaling::Fit);
        screen.set_resolution(Resolution::Fixed {
            width: 32,
            height: 16,
        });
        assert_eq!(screen.scaling(), Scaling::Fit);
        assert_eq!(screen.canvas_size([100, 60], 1.0), [32.0, 16.0]);
    }

    #[test]
    fn window_resolution_uses_logical_pixels() {
        let screen = Screen::new(Resolution::Window);
        assert_eq!(screen.canvas_size([2560, 1440], 2.0), [1280.0, 720.0]);
    }

    #[test]
    fn output_points_map_through_the_scaling() {
        let fixed = Screen::new(Resolution::Fixed {
            width: 32,
            height: 16,
        });
        // The canvas fills (2, 6) to (98, 54) of a 100x60 output, at 3x.
        assert_eq!(fixed.to_canvas([2.0, 6.0], [100, 60], 1.0), [0.0, 0.0]);
        assert_eq!(fixed.to_canvas([50.0, 30.0], [100, 60], 1.0), [16.0, 8.0]);
        assert_eq!(fixed.to_canvas([0.0, 0.0], [100, 60], 1.0)[0], -2.0 / 3.0);

        let window = Screen::new(Resolution::Window);
        assert_eq!(
            window.to_canvas([300.0, 150.0], [800, 600], 1.5),
            [200.0, 100.0]
        );
    }

    #[test]
    fn canvas_points_map_back_to_the_output() {
        let fixed = Screen::new(Resolution::Fixed {
            width: 32,
            height: 16,
        });
        assert_eq!(fixed.to_output([0.0, 0.0], [100, 60], 1.0), [2.0, 6.0]);
        assert_eq!(fixed.to_output([16.0, 8.0], [100, 60], 1.0), [50.0, 30.0]);

        let window = Screen::new(Resolution::Window);
        assert_eq!(
            window.to_output([200.0, 100.0], [800, 600], 1.5),
            [300.0, 150.0]
        );
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
