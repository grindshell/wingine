//! Finding the modules scripts `require` among the page's [files](crate::files) on the web, where
//! mlua's own requirer, which reads the file system, finds none. It finds them as that one does:
//! by their paths relative to the script requiring them, as a file with a `.luau` or `.lua`
//! extension, or a folder with an `init.luau` or `init.lua`, with aliases from `.luaurc` and
//! `.config.luau` files.

use std::path::{Component, Path, PathBuf};

use crate::{
    files,
    lua::mlua::{
        self,
        luau::{NavigateError, Require},
    },
};

/// Navigates the page's files for Luau's `require`, as mlua's `FsRequirer` navigates the file
/// system. Chunk names are paths from `/`, as the page's files are.
#[derive(Debug, Default)]
pub(super) struct FilesRequirer {
    /// The module's path, which needn't be a file.
    path: PathBuf,
    /// The file or folder with an init file that the path leads to, if any.
    resolved: Option<PathBuf>,
}

impl FilesRequirer {
    /// Chunk names starting with this are paths, which may require.
    const CHUNK_PREFIX: &str = "@";
    const FILE_EXTENSIONS: &[&str] = &["luau", "lua"];
    const LUAURC_CONFIG: &str = ".luaurc";
    const LUAU_CONFIG: &str = ".config.luau";

    pub(super) fn new() -> Self {
        Self::default()
    }

    /// `chunk_name` without the line number a traceback gives it.
    fn without_line(chunk_name: &str) -> &str {
        match chunk_name.rsplit_once(':') {
            Some((path, line)) if line.parse::<u32>().is_ok() => path,
            _ => chunk_name,
        }
    }

    /// `path` from `/`, without `.` or `..`.
    fn normalize(path: &Path) -> PathBuf {
        let mut normal = PathBuf::from("/");
        for component in path.components() {
            match component {
                Component::Normal(name) => normal.push(name),
                Component::ParentDir => {
                    normal.pop();
                }
                Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            }
        }
        normal
    }

    /// The file a module's `path` leads to: the path with an extension, or a folder's init
    /// file. A folder without one leads nowhere yet, but may hold modules.
    fn resolve(path: &Path) -> Result<Option<PathBuf>, NavigateError> {
        let mut found = None;
        if path.file_name() != Some("init".as_ref()) {
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| format!("{extension}."))
                .unwrap_or_default();
            for candidate_extension in Self::FILE_EXTENSIONS {
                let candidate = path.with_extension(format!("{extension}{candidate_extension}"));
                if files::is_file(&candidate) && found.replace(candidate).is_some() {
                    return Err(NavigateError::Ambiguous);
                }
            }
        }
        if files::is_dir(path) {
            for extension in Self::FILE_EXTENSIONS {
                let candidate = path.join(format!("init.{extension}"));
                if files::is_file(&candidate) && found.replace(candidate).is_some() {
                    return Err(NavigateError::Ambiguous);
                }
            }
            if found.is_none() {
                return Ok(None);
            }
        }
        found.map(Some).ok_or(NavigateError::NotFound)
    }

    fn go_to(&mut self, path: PathBuf) -> Result<(), NavigateError> {
        self.resolved = Self::resolve(&path)?;
        self.path = path;
        Ok(())
    }
}

impl Require for FilesRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name.starts_with(Self::CHUNK_PREFIX)
    }

    fn reset(&mut self, chunk_name: &str) -> Result<(), NavigateError> {
        let Some(name) = chunk_name.strip_prefix(Self::CHUNK_PREFIX) else {
            return Err(NavigateError::NotFound);
        };
        let path = Self::normalize(Self::without_line(name).as_ref());
        // A chunk's name is its file's, extension and all, unlike a module's.
        self.resolved = match Self::resolve(&path) {
            Err(NavigateError::NotFound) if files::is_file(&path) => Some(path.clone()),
            result => result?,
        };
        self.path = path;
        Ok(())
    }

    fn jump_to_alias(&mut self, path: &str) -> Result<(), NavigateError> {
        self.go_to(Self::normalize(path.as_ref()))
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        let mut path = self.path.clone();
        // Going above `/` is a mistake Luau recovers from, as it looks for aliases.
        if !path.pop() {
            return Err(NavigateError::NotFound);
        }
        self.go_to(path)
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        self.go_to(self.path.join(name))
    }

    fn has_module(&self) -> bool {
        self.resolved.as_deref().is_some_and(files::is_file)
    }

    fn cache_key(&self) -> String {
        let resolved = self.resolved.as_deref();
        resolved.unwrap_or(&self.path).display().to_string()
    }

    fn has_config(&self) -> bool {
        files::is_dir(&self.path)
            && (files::is_file(&self.path.join(Self::LUAURC_CONFIG))
                || files::is_file(&self.path.join(Self::LUAU_CONFIG)))
    }

    fn config(&self) -> std::io::Result<Vec<u8>> {
        let luaurc = self.path.join(Self::LUAURC_CONFIG);
        if files::is_file(&luaurc) {
            return files::read(&luaurc);
        }
        files::read(&self.path.join(Self::LUAU_CONFIG))
    }

    fn loader(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Function> {
        let path = self
            .resolved
            .as_deref()
            .ok_or_else(|| mlua::Error::runtime("there's no module here"))?;
        let source = files::read(path).map_err(mlua::Error::external)?;
        lua.load(source)
            .set_name(format!("@{}", self.path.display()))
            .into_function()
    }
}
