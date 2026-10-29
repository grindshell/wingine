use std::path::PathBuf;

use thiserror::Error;

/// Errors from starting the engine, loading or saving files, audio, headless rendering, Luau
/// scripts, or scripted runs.
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
    #[error("failed to save image {}: {source}", .path.display())]
    SaveImage {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("failed to load audio {}: {source}", .path.display())]
    LoadAudio {
        path: PathBuf,
        source: kira::sound::FromFileError,
    },
    #[error("failed to play audio: {0}")]
    PlayAudio(String),
    #[error("failed to read {}: {source}", .path.display())]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to read a rendered frame back from the GPU: {0}")]
    ReadFrame(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A Luau script failed to compile or raised an error.
    #[error("Luau: {0}")]
    Lua(#[from] mlua::Error),
    #[error("script {}: {message}", .path.display())]
    Script { path: PathBuf, message: String },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
