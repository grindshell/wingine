use thiserror::Error;

/// Errors that can stop the engine from starting or running.
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
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
