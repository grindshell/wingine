use crate::math::Rect;

/// A view onto the game world, applied to drawing with [`Canvas::set_camera`](super::Canvas::set_camera).
///
/// World coordinates use the same units as canvas pixels at a zoom of 1: `y` grows downward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World position shown at the center of the canvas.
    pub x: f32,
    /// World position shown at the center of the canvas.
    pub y: f32,
    /// Canvas pixels per world unit. Whole numbers keep pixel art crisp.
    pub zoom: f32,
    /// World area the view stays inside, such as the map. On an axis where the view is larger
    /// than the bounds, the bounds are centered instead.
    pub bounds: Option<Rect>,
}

impl Camera {
    /// A camera centered on `x`, `y` at a zoom of 1, with no bounds.
    pub const fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            zoom: 1.0,
            bounds: None,
        }
    }

    /// The world area visible on a canvas of `width` by `height` pixels, after applying bounds.
    pub fn visible_area(&self, width: f32, height: f32) -> Rect {
        self.transform(width, height).visible_area(width, height)
    }

    /// The world position under `point` on a canvas of `width` by `height` pixels, such as the
    /// position under the cursor.
    pub fn canvas_to_world(&self, point: [f32; 2], width: f32, height: f32) -> [f32; 2] {
        let transform = self.transform(width, height);
        [0, 1].map(|i| (point[i] + transform.offset[i]) / transform.scale)
    }

    /// The world-to-canvas transform for a canvas of `width` by `height` pixels.
    pub(crate) fn transform(&self, width: f32, height: f32) -> Transform {
        let half_view = [width / self.zoom / 2.0, height / self.zoom / 2.0];
        let mut center = [self.x, self.y];
        if let Some(bounds) = self.bounds {
            center[0] = clamp_axis(center[0], bounds.x, bounds.w, half_view[0]);
            center[1] = clamp_axis(center[1], bounds.y, bounds.h, half_view[1]);
        }
        // Snapped to whole canvas pixels, so the world doesn't shimmer as the camera moves by
        // fractions of a pixel.
        Transform {
            scale: self.zoom,
            offset: [
                (center[0] * self.zoom - width / 2.0).round(),
                (center[1] * self.zoom - height / 2.0).round(),
            ],
        }
    }
}

/// Keeps a view `2 * half_view` long, centered on `center`, inside `start..start + length`.
fn clamp_axis(center: f32, start: f32, length: f32, half_view: f32) -> f32 {
    if length <= half_view * 2.0 {
        start + length / 2.0
    } else {
        // Not `clamp`, which panics if a bound is NaN.
        center
            .max(start + half_view)
            .min(start + length - half_view)
    }
}

/// Maps world coordinates to canvas pixels: `canvas = world * scale - offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Transform {
    pub scale: f32,
    pub offset: [f32; 2],
}

impl Transform {
    pub const IDENTITY: Self = Self {
        scale: 1.0,
        offset: [0.0, 0.0],
    };

    pub fn apply(&self, rect: Rect) -> Rect {
        Rect::new(
            rect.x * self.scale - self.offset[0],
            rect.y * self.scale - self.offset[1],
            rect.w * self.scale,
            rect.h * self.scale,
        )
    }

    /// The area that lands on a canvas of `width` by `height` pixels, in the coordinates this
    /// transform maps from.
    pub fn visible_area(&self, width: f32, height: f32) -> Rect {
        Rect::new(
            self.offset[0] / self.scale,
            self.offset[1] / self.scale,
            width / self.scale,
            height / self.scale,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_the_camera_position() {
        let transform = Camera::new(100.0, 50.0).transform(64.0, 32.0);
        assert_eq!(transform.apply(Rect::new(100.0, 50.0, 1.0, 1.0)).x, 32.0);
        assert_eq!(transform.apply(Rect::new(100.0, 50.0, 1.0, 1.0)).y, 16.0);
    }

    #[test]
    fn zoom_scales_around_the_center() {
        let camera = Camera {
            zoom: 2.0,
            ..Camera::new(10.0, 10.0)
        };
        let rect = camera
            .transform(40.0, 40.0)
            .apply(Rect::new(10.0, 10.0, 4.0, 4.0));
        assert_eq!(rect, Rect::new(20.0, 20.0, 8.0, 8.0));
    }

    #[test]
    fn canvas_points_map_back_to_the_world() {
        let camera = Camera {
            zoom: 2.0,
            ..Camera::new(10.0, 10.0)
        };
        let world = Rect::new(13.0, 7.0, 1.0, 1.0);
        let canvas = camera.transform(40.0, 40.0).apply(world);
        assert_eq!(
            camera.canvas_to_world([canvas.x, canvas.y], 40.0, 40.0),
            [13.0, 7.0]
        );
    }

    #[test]
    fn offset_snaps_to_whole_pixels() {
        let transform = Camera::new(10.3, 10.6).transform(20.0, 20.0);
        assert_eq!(transform.offset, [0.0, 1.0]);
    }

    #[test]
    fn bounds_keep_the_view_inside() {
        let camera = Camera {
            bounds: Some(Rect::new(0.0, 0.0, 100.0, 100.0)),
            ..Camera::new(5.0, 95.0)
        };
        assert_eq!(
            camera.visible_area(20.0, 20.0),
            Rect::new(0.0, 80.0, 20.0, 20.0)
        );
    }

    #[test]
    fn bounds_smaller_than_the_view_are_centered() {
        let camera = Camera {
            bounds: Some(Rect::new(0.0, 0.0, 10.0, 100.0)),
            ..Camera::new(0.0, 50.0)
        };
        let area = camera.visible_area(30.0, 20.0);
        assert_eq!((area.x, area.w), (-10.0, 30.0));
        assert_eq!((area.y, area.h), (40.0, 20.0));
    }

    #[test]
    fn nan_bounds_do_not_panic() {
        let camera = Camera {
            bounds: Some(Rect::new(0.0, 0.0, f32::NAN, 100.0)),
            ..Camera::new(50.0, 50.0)
        };
        assert_eq!(camera.visible_area(20.0, 20.0).y, 40.0);
    }

    #[test]
    fn bounds_account_for_zoom() {
        let camera = Camera {
            zoom: 2.0,
            bounds: Some(Rect::new(0.0, 0.0, 100.0, 100.0)),
            ..Camera::new(0.0, 0.0)
        };
        assert_eq!(
            camera.visible_area(40.0, 40.0),
            Rect::new(0.0, 0.0, 20.0, 20.0)
        );
    }
}
