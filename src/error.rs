use std::path::PathBuf;

use thiserror::Error;

/// Errors from starting the engine or loading assets.
#[derive(Debug, Error)]
pub enum Error {
    #[error("event loop error: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("failed to create window: {0}")]
    CreateWindow(#[from] winit::error::OsError),
    #[error("failed to create rendering surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),
    #[error("no suitable GPU adapter: {0}")]
    RequestAdapter(#[from] wgpu::RequestAdapterError),
    #[error("failed to open GPU device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    #[error("failed to load image {}: {source}", .path.display())]
    LoadImage {
        path: PathBuf,
        source: image::ImageError,
    },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
