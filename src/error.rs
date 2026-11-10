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
    /// An image was read, but can't be a texture: it's larger than the GPU takes.
    #[error("image {}: {message}", .path.display())]
    Image { path: PathBuf, message: String },
    /// A window's configuration asks for what the GPU can't do, such as a fixed resolution larger
    /// than it renders.
    #[error("invalid window configuration: {0}")]
    WindowConfig(String),
    /// An image can't be a window's icon.
    #[error("invalid icon: {0}")]
    Icon(String),
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
    /// A font file couldn't be read, or what it describes couldn't be drawn.
    #[error("font {}: {message}", .path.display())]
    Font { path: PathBuf, message: String },
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
    /// A model file couldn't be read, or what it holds couldn't be drawn.
    #[error("model {}: {message}", .path.display())]
    Model { path: PathBuf, message: String },
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
    /// A game couldn't be exported to ship.
    #[error("can't export {}: {message}", .game.display())]
    Export { game: PathBuf, message: String },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An error from a library whose types the engine doesn't expose.
pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;
