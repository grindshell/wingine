//! [`WindowIcon`], the picture a game's window and taskbar button show, and its executable once
//! exported for Windows.

use std::{fmt, path::Path, sync::Arc};

use image::{
    RgbaImage,
    imageops::{self, FilterType},
};

use crate::{Error, Result};

/// How many pixels across the largest icons are that systems show: Windows' go up to 256.
pub(crate) const LARGEST: u32 = 256;

/// The picture a game's window shows in its title bar and taskbar button, as
/// [`WindowConfig::icon`](crate::WindowConfig::icon) sets it: a square image, often pixel art 16,
/// 32 or 48 pixels across. An exported game's executable shows it too, on Windows (see
/// [`runtime::export`](crate::runtime::export)).
///
/// Systems show icons at sizes of their own. So that pixel art stays crisp, the window gets its
/// icon scaled up by the largest whole number that keeps it within 256 pixels, nearest
/// neighbor, so systems only ever shrink it. Windows and X11 show a window's icon; Wayland and
/// macOS show the application's instead, and the web the page's.
#[derive(Clone, PartialEq, Eq)]
pub struct WindowIcon {
    /// Its pixels, row by row from the top, as RGBA bytes.
    rgba: Arc<[u8]>,
    /// How many pixels across and down it is.
    size: u32,
}

impl WindowIcon {
    /// Loads an icon from a PNG file, which must be square.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = crate::files::read(path).map_err(|source| Error::ReadFile {
            path: path.to_owned(),
            source,
        })?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|source| Error::LoadImage {
                path: path.to_owned(),
                source,
            })?
            .into_rgba8();
        let (width, height) = image.dimensions();
        Self::from_rgba(image.into_raw(), width, height).map_err(|err| match err {
            Error::Icon(problem) => Error::Icon(format!("{}: {problem}", path.display())),
            other => other,
        })
    }

    /// An icon of `width` by `height` pixels, which must be the same, from their RGBA bytes, row
    /// by row from the top, colors in sRGB.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self> {
        if width != height {
            return Err(Error::Icon(format!(
                "an icon must be square, not {width}x{height}"
            )));
        }
        if width == 0 {
            return Err(Error::Icon("an icon must have pixels".into()));
        }
        let expected = u64::from(width) * u64::from(height) * 4;
        if rgba.len() as u64 != expected {
            return Err(Error::Icon(format!(
                "a {width}x{height} icon is {expected} bytes of RGBA, not {}",
                rgba.len()
            )));
        }
        Ok(Self {
            rgba: rgba.into(),
            size: width,
        })
    }

    /// How many pixels across, and down, the icon is.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The icon scaled up by the largest whole number that keeps it within `largest` pixels
    /// across, or as it is if it's that big already, nearest neighbor.
    pub(crate) fn scaled_within(&self, largest: u32) -> RgbaImage {
        let factor = (largest / self.size).max(1);
        self.resized(self.size * factor)
    }

    /// The icon at `size` pixels across: nearest neighbor, if that's a whole number of times its
    /// own size, and otherwise scaled up by whole numbers past `size` first, then smoothly down
    /// to it, so pixel art's edges stay as sharp as they can.
    pub(crate) fn resized(&self, size: u32) -> RgbaImage {
        let image = RgbaImage::from_raw(self.size, self.size, self.rgba.to_vec())
            .expect("an icon's bytes are its pixels");
        if size.is_multiple_of(self.size) {
            if size == self.size {
                return image;
            }
            return imageops::resize(&image, size, size, FilterType::Nearest);
        }
        let past = self.size * size.div_ceil(self.size);
        let sharp = imageops::resize(&image, past, past, FilterType::Nearest);
        imageops::resize(&sharp, size, size, FilterType::Triangle)
    }
}

impl fmt::Debug for WindowIcon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowIcon")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 icon: red, green, blue and clear.
    fn checks() -> WindowIcon {
        let rgba = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [0; 4]];
        WindowIcon::from_rgba(rgba.concat(), 2, 2).unwrap()
    }

    #[test]
    fn icons_are_square_with_a_pixel_for_every_four_bytes() {
        assert_eq!(checks().size(), 2);
        for (rgba, width, height, problem) in [
            (vec![0; 24], 3, 2, "must be square, not 3x2"),
            (Vec::new(), 0, 0, "must have pixels"),
            (vec![0; 15], 2, 2, "16 bytes of RGBA, not 15"),
        ] {
            let err = WindowIcon::from_rgba(rgba, width, height).unwrap_err();
            assert!(err.to_string().contains(problem), "{err}");
        }
    }

    #[test]
    fn icons_scale_up_by_whole_numbers_to_stay_crisp() {
        let icon = checks();
        let scaled = icon.scaled_within(LARGEST);
        assert_eq!(scaled.dimensions(), (256, 256));
        assert_eq!(scaled.get_pixel(127, 127).0, [255, 0, 0, 255]);
        assert_eq!(scaled.get_pixel(128, 0).0, [0, 255, 0, 255]);
        assert_eq!(scaled.get_pixel(0, 255).0, [0, 0, 255, 255]);
        assert_eq!(icon.scaled_within(7).dimensions(), (6, 6));
        assert_eq!(icon.scaled_within(1).dimensions(), (2, 2), "never shrunk");

        // Between whole numbers of times, only the pixels along the edges between them blend.
        let three = icon.resized(3);
        assert_eq!(three.dimensions(), (3, 3));
        assert_eq!(three.get_pixel(0, 0).0, [255, 0, 0, 255]);
        let middle = three.get_pixel(1, 0).0;
        assert!(middle[0] > 0 && middle[1] > 0, "{middle:?}");
    }

    #[test]
    fn missing_and_unreadable_icons_name_their_paths() {
        let err = WindowIcon::load("no/such/icon.png").unwrap_err();
        assert!(err.to_string().contains("icon.png"), "{err}");
        let sounds = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sounds/blip.wav");
        let err = WindowIcon::load(sounds).unwrap_err();
        assert!(err.to_string().contains("blip.wav"), "{err}");
    }
}
