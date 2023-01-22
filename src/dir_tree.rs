use crate::node::fs;
use crate::node::path::Path;
use crate::Error;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use async_recursion::async_recursion;
use async_trait::async_trait;
use beef::Cow;
use simple_path_match::PathMatch;

pub const ROOT_NAME: &str = ".";

#[derive(Debug, Default, Clone)]
pub struct Ignores {
    map: BTreeMap<usize, BTreeSet<String>>,
}

impl Ignores {
    pub fn add(&mut self, depth: usize, name: &str) {
        self.map.entry(depth).or_default().insert(name.to_string());
    }

    pub fn should_ignore(&self, name: &str, depth: usize) -> bool {
        self.map.get(&depth).map_or(false, |names| names.contains(name))
    }
}

#[async_trait(?Send)]
pub trait Visitor {
    async fn should_enter(&self, _path: &Path) -> Result<bool, Error> {
        Ok(true)
    }
    async fn enter_folder(&mut self, path: &Path) -> Result<(), Error>;
    async fn visit_entry(&mut self, name: &Path, is_file: bool) -> Result<(), Error>;
    async fn exit_folder(&mut self, path: &Path) -> Result<(), Error>;
}

pub async fn apply_visitor<V>(folder_path: &Path, ignores: &Ignores, visitor: &mut V) -> Result<(), Error>
where
    V: Visitor,
{
    apply_visitor_impl(0, folder_path, ignores, visitor).await
}

#[async_recursion(?Send)]
async fn apply_visitor_impl(
    depth: usize,
    path: &Path,
    ignores: &Ignores,
    visitor: &mut dyn Visitor,
) -> Result<(), Error> {
    let file_name: Cow<str> = if depth == 0 {
        ROOT_NAME.into()
    } else {
        path.file_name().into()
    };
    if ignores.should_ignore(&file_name, depth) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path).await?;
    if metadata.is_directory() {
        if visitor.should_enter(path).await? {
            visitor.enter_folder(path).await?;
            let depth = depth + 1;
            let dir = fs::read_dir(path).await?;
            for entry in dir {
                let path = entry.path();
                apply_visitor_impl(depth, &path, ignores, visitor).await?;
            }
            visitor.exit_folder(path).await?;
        } else {
            visitor.visit_entry(path, false).await?;
        }
    } else {
        visitor.visit_entry(path, true).await?;
    }
    Ok(())
}

#[derive(Debug)]
struct PathMatchVisitor<'a> {
    matching_paths: Vec<Path>,
    matcher: &'a PathMatch,
    path_stack: VecDeque<Path>,
    output_relative: bool,
}

impl<'a> PathMatchVisitor<'a> {
    fn full_path_to_relative(&self, full_path: &Path) -> Path {
        self.path_stack
            .back()
            .map_or_else(|| Path::from("."), |p| p.join(&full_path.file_name()))
    }

    fn visit_path(&mut self, absolute: &Path, relative: &Path) {
        if self.matcher.matches(relative.to_string()) {
            let path = if self.output_relative { relative } else { absolute }.clone();
            self.matching_paths.push(path);
        }
    }
}

#[async_trait(?Send)]
impl<'a> Visitor for PathMatchVisitor<'a> {
    async fn should_enter(&self, full: &Path) -> Result<bool, Error> {
        let result = if self.path_stack.len() >= self.matcher.max_depth() {
            false
        } else {
            let relative = self.full_path_to_relative(full);
            self.matcher.matches_prefix(relative.to_string())
        };
        Ok(result)
    }

    async fn enter_folder(&mut self, full: &Path) -> Result<(), Error> {
        let relative = self.full_path_to_relative(full);
        self.visit_path(full, &relative);
        self.path_stack.push_back(relative);
        Ok(())
    }

    async fn visit_entry(&mut self, full: &Path, _is_file: bool) -> Result<(), Error> {
        let relative = self.full_path_to_relative(full);
        self.visit_path(full, &relative);
        Ok(())
    }

    async fn exit_folder(&mut self, _: &Path) -> Result<(), Error> {
        self.path_stack.pop_back();
        Ok(())
    }
}

pub async fn match_relative_paths(path: &Path, matcher: &PathMatch, output_relative: bool) -> Result<Vec<Path>, Error> {
    let mut visitor = PathMatchVisitor {
        matching_paths: Vec::new(),
        matcher,
        path_stack: VecDeque::new(),
        output_relative,
    };
    let ignores = Ignores::default();
    apply_visitor(path, &ignores, &mut visitor).await?;
    Ok(visitor.matching_paths)
}
