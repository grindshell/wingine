//! Finding the modules scripts `require` in the file system, as mlua's own `FsRequirer` finds them,
//! whose navigation this follows step for step (mlua is MIT-licensed): by their paths relative to
//! the script requiring them, as a file with a `.luau` or `.lua` extension, or a folder with an
//! `init.luau` or `init.lua`, with aliases from `.luaurc` and `.config.luau` files.
//!
//! Unlike mlua's, it remembers what it learned of the file system ([`Lookups`]): every `require`
//! navigates from the requiring script to the module again, even one already loaded, asking
//! whether the same few paths are files or folders, and asking the file system each time took a
//! third of a large game's load.

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    env,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

use crate::{
    files,
    lua::{
        Bytecode, load_script,
        mlua::{
            self,
            luau::{NavigateError, Require},
        },
    },
};

/// Whether each path `require` asked about is a file, and whether it's a folder, as the file
/// system answered the first time. A game's load keeps one, so a path is asked about once a load,
/// and a reload, which loads the game again, asks again; a run of Luau tests keeps one for all its
/// games, whose files don't change under it. A file added after a path was asked about is found
/// only by the next load.
#[derive(Clone, Default)]
pub(crate) struct Lookups(Rc<RefCell<HashMap<PathBuf, Lookup>>>);

/// What a path is, as far as anything has asked.
#[derive(Clone, Copy, Default)]
struct Lookup {
    file: Option<bool>,
    dir: Option<bool>,
}

impl Lookups {
    fn is_file(&self, path: &Path) -> bool {
        self.ask(path, |lookup| &mut lookup.file, files::is_file)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.ask(path, |lookup| &mut lookup.dir, files::is_dir)
    }

    fn ask(
        &self,
        path: &Path,
        answer: impl Fn(&mut Lookup) -> &mut Option<bool>,
        look: impl FnOnce(&Path) -> bool,
    ) -> bool {
        let known = self
            .0
            .borrow_mut()
            .get_mut(path)
            .and_then(|lookup| *answer(lookup));
        if let Some(known) = known {
            return known;
        }
        let found = look(path);
        let mut lookups = self.0.borrow_mut();
        *answer(lookups.entry(path.to_owned()).or_default()) = Some(found);
        found
    }
}

/// Navigates the file system for Luau's `require`, as mlua's `FsRequirer` does, but with what it
/// learns kept in [`Lookups`], and scripts loaded through [`Bytecode`] if it has one.
pub(super) struct FsRequirer {
    lookups: Lookups,
    bytecode: Option<Bytecode>,
    /// The module's path, made absolute, which needn't be a file.
    abs_path: PathBuf,
    /// The module's path as the requiring chunk's name gave it, which names the module's chunk.
    rel_path: PathBuf,
    /// The file or folder with an init file that the path leads to, if any.
    resolved_path: Option<PathBuf>,
}

impl FsRequirer {
    /// Chunk names starting with this are paths, which may require.
    const CHUNK_PREFIX: &str = "@";
    const FILE_EXTENSIONS: &[&str] = &["luau", "lua"];
    const LUAURC_CONFIG: &str = ".luaurc";
    const LUAU_CONFIG: &str = ".config.luau";

    pub(super) fn new(lookups: Lookups, bytecode: Option<Bytecode>) -> Self {
        Self {
            lookups,
            bytecode,
            abs_path: PathBuf::new(),
            rel_path: PathBuf::new(),
            resolved_path: None,
        }
    }

    /// `chunk_name` without the line number a traceback gives it.
    fn without_line(chunk_name: &str) -> &str {
        match chunk_name.rsplit_once(':') {
            Some((path, line)) if line.parse::<u32>().is_ok() => path,
            _ => chunk_name,
        }
    }

    /// `path` without `.` or `..` where it can go without them, and from `.` if it's relative.
    fn normalize(path: &Path) -> PathBuf {
        let mut components = VecDeque::new();
        for component in path.components() {
            match component {
                Component::Prefix(..) | Component::RootDir => components.push_back(component),
                Component::CurDir => {}
                Component::ParentDir => {
                    if matches!(components.back(), None | Some(Component::ParentDir)) {
                        components.push_back(Component::ParentDir);
                    } else if matches!(components.back(), Some(Component::Normal(..))) {
                        components.pop_back();
                    }
                }
                Component::Normal(..) => components.push_back(component),
            }
        }
        if matches!(components.front(), None | Some(Component::Normal(..))) {
            components.push_front(Component::CurDir);
        }
        components.into_iter().collect()
    }

    /// The file a module's `path` leads to: the path with an extension, or a folder's init file.
    /// A folder without one leads nowhere yet, but may hold modules.
    fn resolve(&self, path: &Path) -> Result<Option<PathBuf>, NavigateError> {
        let mut found = None;
        if path.components().next_back() != Some(Component::Normal("init".as_ref())) {
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| format!("{extension}."))
                .unwrap_or_default();
            for candidate_extension in Self::FILE_EXTENSIONS {
                let candidate = path.with_extension(format!("{extension}{candidate_extension}"));
                if self.lookups.is_file(&candidate) && found.replace(candidate).is_some() {
                    return Err(NavigateError::Ambiguous);
                }
            }
        }
        if self.lookups.is_dir(path) {
            for extension in Self::FILE_EXTENSIONS {
                let candidate = path.join(format!("init.{extension}"));
                if self.lookups.is_file(&candidate) && found.replace(candidate).is_some() {
                    return Err(NavigateError::Ambiguous);
                }
            }
            if found.is_none() {
                return Ok(None);
            }
        }
        found.map(Some).ok_or(NavigateError::NotFound)
    }

    fn go_to(&mut self, abs_path: PathBuf, rel_path: PathBuf) -> Result<(), NavigateError> {
        self.resolved_path = self.resolve(&abs_path)?;
        self.abs_path = abs_path;
        self.rel_path = rel_path;
        Ok(())
    }
}

impl Require for FsRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name.starts_with(Self::CHUNK_PREFIX)
    }

    fn reset(&mut self, chunk_name: &str) -> Result<(), NavigateError> {
        let Some(name) = chunk_name.strip_prefix(Self::CHUNK_PREFIX) else {
            return Err(NavigateError::NotFound);
        };
        let chunk_path = Self::normalize(Self::without_line(name).as_ref());
        let cwd = || env::current_dir().map_err(|_| NavigateError::NotFound);
        if chunk_path.extension() == Some("rs".as_ref()) {
            // A chunk named for Rust source, as mlua names one by where it's loaded, requires
            // from the current folder.
            let file_name = chunk_path.file_name().ok_or(NavigateError::NotFound)?;
            self.abs_path = Self::normalize(&cwd()?.join(file_name));
            self.rel_path = [Component::CurDir, Component::Normal(file_name)]
                .into_iter()
                .collect();
            self.resolved_path = None;
            return Ok(());
        }
        let abs_path = if chunk_path.is_absolute() {
            chunk_path.clone()
        } else {
            Self::normalize(&cwd()?.join(&chunk_path))
        };
        // A chunk's name is its file's, extension and all, unlike a module's.
        self.resolved_path = match self.resolve(&abs_path) {
            Err(NavigateError::NotFound) if self.lookups.is_file(&abs_path) => {
                Some(abs_path.clone())
            }
            result => result?,
        };
        self.abs_path = abs_path;
        self.rel_path = chunk_path;
        Ok(())
    }

    fn jump_to_alias(&mut self, path: &str) -> Result<(), NavigateError> {
        let path = Self::normalize(path.as_ref());
        self.go_to(path.clone(), path)
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        let mut abs_path = self.abs_path.clone();
        // Going above the root is a mistake Luau recovers from, as it looks for aliases.
        if !abs_path.pop() {
            return Err(NavigateError::NotFound);
        }
        let mut rel_path = self.rel_path.clone();
        rel_path.pop();
        self.go_to(abs_path, Self::normalize(&rel_path))
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        self.go_to(self.abs_path.join(name), self.rel_path.join(name))
    }

    fn has_module(&self) -> bool {
        (self.resolved_path.as_deref()).is_some_and(|path| self.lookups.is_file(path))
    }

    fn cache_key(&self) -> String {
        let resolved = self.resolved_path.as_deref();
        resolved.unwrap_or(&self.abs_path).display().to_string()
    }

    fn has_config(&self) -> bool {
        self.lookups.is_dir(&self.abs_path)
            && (self
                .lookups
                .is_file(&self.abs_path.join(Self::LUAURC_CONFIG))
                || self.lookups.is_file(&self.abs_path.join(Self::LUAU_CONFIG)))
    }

    fn config(&self) -> std::io::Result<Vec<u8>> {
        let luaurc = self.abs_path.join(Self::LUAURC_CONFIG);
        if self.lookups.is_file(&luaurc) {
            return files::read(&luaurc);
        }
        files::read(&self.abs_path.join(Self::LUAU_CONFIG))
    }

    fn loader(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Function> {
        let path = self
            .resolved_path
            .as_deref()
            .ok_or_else(|| mlua::Error::runtime("there's no module here"))?;
        let source = files::read(path).map_err(mlua::Error::external)?;
        let name = format!("@{}", self.rel_path.display());
        load_script(lua, &source, name, None, self.bytecode.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::save::TemporaryFolder;

    /// A folder of modules: files of either extension, folders with and without an init file, a
    /// module both ways, one that fails, and an alias.
    fn modules() -> TemporaryFolder {
        let folder = TemporaryFolder::new();
        for (path, contents) in [
            ("main.luau", ""),
            ("lib/init.luau", "return 'lib'"),
            (
                "lib/numbers.luau",
                "return 'numbers and ' .. require('../util')",
            ),
            ("util.lua", "return 'util'"),
            ("twice.luau", "return 1"),
            ("twice.lua", "return 2"),
            (
                "deep/er/most.luau",
                "return 'most, ' .. require('../../lib/numbers')",
            ),
            ("empty/nothing.txt", ""),
            ("broken.luau", "error('broken at the top')"),
            (".luaurc", r#"{ "aliases": { "kit": "./lib" } }"#),
        ] {
            let path = folder.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        folder
    }

    /// What `require(module)` returns from `folder`'s `main.luau`, or the error it raises, with
    /// `requirer` finding modules.
    fn required(folder: &Path, requirer: impl Require + 'static, module: &str) -> String {
        let lua = mlua::Lua::new();
        let require = lua.create_require_function(requirer).unwrap();
        lua.globals().set("require", require).unwrap();
        let main = folder.join("main.luau");
        lua.load(format!(
            "local _, got = pcall(require, '{module}') return tostring(got)"
        ))
        .set_name(format!("@{}", main.display()))
        .eval()
        .unwrap()
    }

    #[test]
    fn modules_are_found_as_mluas_requirer_finds_them() {
        let folder = modules();
        for module in [
            "./lib",
            "./lib/numbers",
            "./lib/init",
            "./util",
            "./deep/er/most",
            "./twice",
            "./missing",
            "./empty",
            "./broken",
            "../outside",
            "@kit/numbers",
            "@nothing/here",
            "lib",
        ] {
            let ours = FsRequirer::new(Lookups::default(), None);
            let ours = required(folder.path(), ours, module);
            let mluas = required(folder.path(), mlua::luau::FsRequirer::new(), module);
            assert_eq!(ours, mluas, "require('{module}')");
        }
        let found = FsRequirer::new(Lookups::default(), Some(Bytecode::default()));
        assert_eq!(
            required(folder.path(), found, "./deep/er/most"),
            "most, numbers and util",
            "and through kept bytecode"
        );
    }

    #[test]
    fn what_a_path_was_stays_what_it_was_until_asked_afresh() {
        let folder = modules();
        let lookups = Lookups::default();
        let numbers = folder.path().join("lib/numbers.luau");
        assert!(lookups.is_file(&numbers));
        assert!(!lookups.is_dir(&numbers));
        assert!(lookups.is_dir(&folder.path().join("lib")));
        fs::remove_file(&numbers).unwrap();
        assert!(lookups.is_file(&numbers), "kept");
        let shared = lookups.clone();
        assert!(shared.is_file(&numbers), "shared with its clones");
        assert!(
            !Lookups::default().is_file(&numbers),
            "a new one asks again"
        );
    }
}
