// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_camel_case_types)]

pub use self::FileMatch::*;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use session::search_paths::{SearchPaths, PathKind};
use util::fs as rustcfs;

#[derive(Copy, Clone)]
pub enum FileMatch {
    FileMatches,
    FileDoesntMatch,
}

// A module for searching for libraries
// FIXME (#2658): I'm not happy how this module turned out. Should
// probably just be folded into cstore.

pub struct FileSearch<'a> {
    pub sysroot: &'a Path,
    pub search_paths: &'a SearchPaths,
    pub triple: &'a str,
    pub kind: PathKind,
}

impl<'a> FileSearch<'a> {
    pub fn for_each_lib_search_path<F>(&self, mut f: F) where
        F: FnMut(&Path, PathKind) -> FileMatch,
    {
        let mut visited_dirs = HashSet::new();
        let mut found = false;

        for (path, kind) in self.search_paths.iter(self.kind) {
            match f(path, kind) {
                FileMatches => found = true,
                FileDoesntMatch => ()
            }
            visited_dirs.insert(path.to_path_buf());
        }

        debug!("filesearch: searching lib path");
        let tlib_path = make_target_lib_path(self.sysroot,
                                             self.triple);
        if !visited_dirs.contains(&tlib_path) {
            match f(&tlib_path, PathKind::All) {
                FileMatches => found = true,
                FileDoesntMatch => ()
            }
        }

        visited_dirs.insert(tlib_path);
        // Try RUST_PATH
        if !found {
            let rustpath = rust_path();
            for path in &rustpath {
                let tlib_path = make_rustpkg_lib_path(path, self.triple);
                debug!("is {} in visited_dirs? {}", tlib_path.display(),
                        visited_dirs.contains(&tlib_path));

                if !visited_dirs.contains(&tlib_path) {
                    visited_dirs.insert(tlib_path.clone());
                    // Don't keep searching the RUST_PATH if one match turns up --
                    // if we did, we'd get a "multiple matching crates" error
                    match f(&tlib_path, PathKind::All) {
                       FileMatches => {
                           break;
                       }
                       FileDoesntMatch => ()
                    }
                }
            }
        }
    }

    pub fn get_lib_path(&self) -> PathBuf {
        make_target_lib_path(self.sysroot, self.triple)
    }

    pub fn search<F>(&self, mut pick: F)
        where F: FnMut(&Path, PathKind) -> FileMatch
    {
        self.for_each_lib_search_path(|lib_search_path, kind| {
            info!("searching {}", lib_search_path.display());
            match fs::read_dir(lib_search_path) {
                Ok(files) => {
                    let files = files.filter_map(|p| p.ok().map(|s| s.path()))
                                     .collect::<Vec<_>>();
                    let mut rslt = FileDoesntMatch;
                    fn is_rlib(p: &Path) -> bool {
                        p.extension().and_then(|s| s.to_str()) == Some("rlib")
                    }
                    // Reading metadata out of rlibs is faster, and if we find both
                    // an rlib and a dylib we only read one of the files of
                    // metadata, so in the name of speed, bring all rlib files to
                    // the front of the search list.
                    let files1 = files.iter().filter(|p| is_rlib(p));
                    let files2 = files.iter().filter(|p| !is_rlib(p));
                    for path in files1.chain(files2) {
                        debug!("testing {}", path.display());
                        let maybe_picked = pick(path, kind);
                        match maybe_picked {
                            FileMatches => {
                                debug!("picked {}", path.display());
                                rslt = FileMatches;
                            }
                            FileDoesntMatch => {
                                debug!("rejected {}", path.display());
                            }
                        }
                    }
                    rslt
                }
                Err(..) => FileDoesntMatch,
            }
        });
    }

    pub fn new(sysroot: &'a Path,
               triple: &'a str,
               search_paths: &'a SearchPaths,
               kind: PathKind) -> FileSearch<'a> {
        debug!("using sysroot = {}, triple = {}", sysroot.display(), triple);
        FileSearch {
            sysroot: sysroot,
            search_paths: search_paths,
            triple: triple,
            kind: kind,
        }
    }

    // Returns a list of directories where target-specific dylibs might be located.
    pub fn get_dylib_search_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        self.for_each_lib_search_path(|lib_search_path, _| {
            paths.push(lib_search_path.to_path_buf());
            FileDoesntMatch
        });
        paths
    }

    // Returns a list of directories where target-specific tool binaries are located.
    pub fn get_tools_search_paths(&self) -> Vec<PathBuf> {
        let mut p = PathBuf::from(self.sysroot);
        p.push(libdir_str());
        p.push(&rustlibdir());
        p.push(&self.triple);
        p.push("bin");
        vec![p]
    }
}

pub fn relative_target_lib_path(target_triple: &str) -> PathBuf {
    let mut p = PathBuf::from(&libdir_str());
    assert!(p.is_relative());
    p.push(&rustlibdir());
    p.push(target_triple);
    p.push("lib");
    p
}

fn make_target_lib_path(sysroot: &Path,
                        target_triple: &str) -> PathBuf {
    sysroot.join(&relative_target_lib_path(target_triple))
}

fn make_rustpkg_lib_path(dir: &Path,
                         triple: &str) -> PathBuf {
    let mut p = dir.join(libdir_str());
    p.push(triple);
    p
}

pub fn bindir_relative_str() -> &'static str {
    env!("CFG_BINDIR_RELATIVE")
}

pub fn bindir_relative_path() -> PathBuf {
    PathBuf::from(bindir_relative_str())
}

pub fn libdir_str() -> &'static str {
    env!("CFG_LIBDIR_RELATIVE")
}

pub fn get_or_default_sysroot() -> PathBuf {
    // Follow symlinks.  If the resolved path is relative, make it absolute.
    fn canonicalize(path: Option<PathBuf>) -> Option<PathBuf> {
        path.and_then(|path| {
            match fs::canonicalize(&path) {
                // See comments on this target function, but the gist is that
                // gcc chokes on verbatim paths which fs::canonicalize generates
                // so we try to avoid those kinds of paths.
                Ok(canon) => Some(rustcfs::fix_windows_verbatim_for_gcc(&canon)),
                Err(e) => panic!("failed to get realpath: {}", e),
            }
        })
    }

    match canonicalize(env::current_exe().ok()) {
        Some(mut p) => {
            // Remove the exe name
            p.pop();
            let mut rel = bindir_relative_path();

            // Remove a number of elements equal to the number of elements in the bindir relative
            // path
            while rel.pop() {
                p.pop();
            }
            p
        }
        None => panic!("can't determine value for sysroot")
    }
}

#[cfg(windows)]
const PATH_ENTRY_SEPARATOR: char = ';';
#[cfg(not(windows))]
const PATH_ENTRY_SEPARATOR: char = ':';

/// Returns RUST_PATH as a string, without default paths added
pub fn get_rust_path() -> Option<String> {
    env::var("RUST_PATH").ok()
}

/// Returns the value of RUST_PATH, as a list
/// of Paths. Includes default entries for, if they exist:
/// $HOME/.rust
/// DIR/.rust for any DIR that's the current working directory
/// or an ancestor of it
pub fn rust_path() -> Vec<PathBuf> {
    let mut env_rust_path: Vec<PathBuf> = match get_rust_path() {
        Some(env_path) => {
            let env_path_components =
                env_path.split(PATH_ENTRY_SEPARATOR);
            env_path_components.map(|s| PathBuf::from(s)).collect()
        }
        None => Vec::new()
    };
    let cwd = env::current_dir().unwrap();
    // now add in default entries
    let cwd_dot_rust = cwd.join(".rust");
    if !env_rust_path.contains(&cwd_dot_rust) {
        env_rust_path.push(cwd_dot_rust);
    }
    if !env_rust_path.contains(&cwd) {
        env_rust_path.push(cwd.clone());
    }
    let mut cur = &*cwd;
    while let Some(parent) = cur.parent() {
        let candidate = parent.join(".rust");
        if !env_rust_path.contains(&candidate) && candidate.exists() {
            env_rust_path.push(candidate.clone());
        }
        cur = parent;
    }
    if let Some(h) = env::home_dir() {
        let p = h.join(".rust");
        if !env_rust_path.contains(&p) && p.exists() {
            env_rust_path.push(p);
        }
    }
    env_rust_path
}

// The name of rustc's own place to organize libraries.
// Used to be "rustc", now the default is "rustlib"
pub fn rustlibdir() -> String {
    "rustlib".to_string()
}
