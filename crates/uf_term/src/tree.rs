//! Trees built from paths, drawn with box-drawing branches.

use crate::render::Renderer;
use crate::text::push_spaces;

/// A node in a rendered tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree<'a> {
    label: &'a str,
    children: Vec<Tree<'a>>,
}

impl<'a> Tree<'a> {
    /// A leaf.
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            children: Vec::new(),
        }
    }

    /// This node's label.
    pub fn label(&self) -> &'a str {
        self.label
    }

    /// This node's children.
    pub fn children(&self) -> &[Tree<'a>] {
        &self.children
    }

    /// Find or create the child named `label`.
    pub fn child(&mut self, label: &'a str) -> &mut Tree<'a> {
        let index = match self.children.iter().position(|child| child.label == label) {
            Some(index) => index,
            None => {
                self.children.push(Tree::new(label));
                self.children.len() - 1
            }
        };
        &mut self.children[index]
    }

    /// Insert a `/`-separated path, creating intermediate nodes.
    ///
    /// Empty segments are skipped, so a leading or doubled separator cannot
    /// produce a nameless node.
    pub fn insert_path(&mut self, path: &'a str) {
        let mut node = self;
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            node = node.child(segment);
        }
    }

    /// Build a tree from `/`-separated paths under one root label.
    pub fn from_paths(label: &'a str, paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut root = Tree::new(label);
        for path in paths {
            root.insert_path(path);
        }
        root.sort();
        root
    }

    /// Sort every level by the bytes of its labels, which is code-point order
    /// and the order `LC_ALL=C ls` lists a directory in.
    ///
    /// Directories and files are interleaved as that order puts them, rather
    /// than directories first. A route file is spelled `$page.js` so that `$`
    /// (U+0024, below every digit and letter) lists it first in its directory;
    /// grouping directories first would put `settings/` above `$layout.js` and
    /// undo that. The same rule holds the trees drawn in the documentation
    /// (`tools/docs/trees.js`), so what `uf new` prints matches both the
    /// manual and the reader's own `ls`.
    ///
    /// `str`'s `Ord` is byte order, so this is a plain comparison: no locale,
    /// no case folding, and a label that is a prefix of another (`app` beside
    /// `app.js`) comes first. The sort is stable, so equal labels keep their
    /// insertion order, though [`Tree::child`] never creates two.
    pub fn sort(&mut self) {
        self.children
            .sort_by(|left, right| left.label.cmp(right.label));
        for child in &mut self.children {
            child.sort();
        }
    }
}

impl Renderer {
    /// Append a tree, starting from its root label.
    ///
    /// The branch prefix is one buffer pushed and truncated as the walk
    /// descends, rather than a fresh `String` per node.
    pub fn tree(&self, out: &mut String, indent: usize, tree: &Tree<'_>) {
        push_spaces(out, indent);
        self.line(out, self.theme().title, tree.label());
        let mut prefix = String::new();
        self.tree_children(out, indent, &mut prefix, tree);
    }

    fn tree_children(&self, out: &mut String, indent: usize, prefix: &mut String, node: &Tree<'_>) {
        let glyphs = self.glyphs();
        let last = node.children().len().saturating_sub(1);
        for (index, child) in node.children().iter().enumerate() {
            let is_last = index == last;
            push_spaces(out, indent);
            self.theme().rule.open(self.color(), out);
            out.push_str(prefix);
            out.push_str(if is_last {
                glyphs.last_branch
            } else {
                glyphs.branch
            });
            self.theme().rule.close(self.color(), out);
            let style = if child.children().is_empty() {
                self.theme().value
            } else {
                self.theme().path
            };
            style.paint(self.color(), child.label(), out);
            out.push('\n');

            let restore = prefix.len();
            prefix.push_str(if is_last { glyphs.gap } else { glyphs.trunk });
            self.tree_children(out, indent, prefix, child);
            prefix.truncate(restore);
        }
    }
}

#[cfg(test)]
mod tests;
