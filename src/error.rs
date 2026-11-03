use std::path::PathBuf;

use thiserror::Error;

/// Errors from starting the engine, loading or saving files, saves, audio, meshes, headless
/// rendering, Luau scripts, native libraries, or scripted runs.
///
/// Errors from winit and wgpu are boxed, so their types stay out of the engine's API.
#[derive(Debug, Error)]
pub enum Error {
    #[error("event loop error: {0}")]
    EventLoop(#[source] BoxedError),
    #[error("failed to create window: {0}")]
    CreateWindow(#[source] BoxedError),
    #[error("failed to create rendering surface: {0}")]
    CreateSurface(#[source] BoxedError),
    #[error("no suitable GPU adapter: {0}")]
    RequestAdapter(#[source] BoxedError),
    #[error("failed to open GPU device: {0}")]
    RequestDevice(#[source] BoxedError),
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
    #[error("failed to write {}: {source}", .path.display())]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A scene file couldn't be read, or what it describes couldn't be loaded.
    #[error("scene {}: {message}", .path.display())]
    Scene { path: PathBuf, message: String },
    /// A tileset file couldn't be read, or what it describes couldn't be loaded.
    #[error("tileset {}: {message}", .path.display())]
    Tileset { path: PathBuf, message: String },
    /// A scene or tileset couldn't be written as a file.
    #[error("failed to write a scene: {0}")]
    WriteScene(String),
    /// A save couldn't be read or written, other than by failing to read or write its file: its
    /// name isn't one, its data isn't plain, or there's nowhere to keep it.
    #[error("save `{name}`: {message}")]
    Save { name: String, message: String },
    /// A mesh's data couldn't be drawn.
    #[error("invalid mesh: {0}")]
    Mesh(String),
    #[error("failed to read a rendered frame back from the GPU: {0}")]
    ReadFrame(#[source] BoxedError),
    /// A Luau script failed to compile or raised an error.
    #[error("Luau: {0}")]
    Lua(#[from] mlua::Error),
    #[error("script {}: {message}", .path.display())]
    Script { path: PathBuf, message: String },
    /// A native library failed to load, or one of its hooks failed.
    #[error("native library `{name}`: {message}")]
    NativeLibrary { name: String, message: String },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An error from a library whose types the engine doesn't expose.
pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;
