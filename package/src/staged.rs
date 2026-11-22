//! Looks at the staged files before a commit and says when they look like more than one commit.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;

/// True when a file matches a pattern: `*.css` is a file ending, anything else a path prefix.
pub fn matches(pattern: &str, path: &str) -> bool {
    match pattern.strip_prefix('*') {
        Some(ending) => path.ends_with(ending),
        None => path.starts_with(pattern),
    }
}

/// What the staged files have in common, and what they do not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Staged {
    /// Files per scope, by the path prefixes in the settings.
    pub scopes: BTreeMap<String, Vec<String>>,
    /// Files that belong to no configured scope.
    pub unscoped: Vec<String>,
    /// How many files suggest each area, by the hints in the settings.
    pub areas: BTreeMap<String, usize>,
}

impl Staged {
    /// Sorts files into scopes and counts the areas they suggest.
    pub fn analyse(files: &[String], config: &Config) -> Self {
        let mut staged = Self::default();
        for file in files {
            // The longest prefix wins, so `app/web/` beats `app/`.
            let scope = config
                .scope_paths
                .iter()
                .flat_map(|(scope, prefixes)| prefixes.iter().map(move |prefix| (scope, prefix)))
                .filter(|(_, prefix)| matches(prefix, file))
                .max_by_key(|(_, prefix)| prefix.len())
                .map(|(scope, _)| scope.clone());
            match scope {
                Some(scope) => staged.scopes.entry(scope).or_default().push(file.clone()),
                None => staged.unscoped.push(file.clone()),
            }
            let areas: BTreeSet<_> = config
                .hints
                .iter()
                .filter(|(pattern, _)| matches(pattern, file))
                .map(|(_, area)| area)
                .collect();
            for area in areas {
                *staged.areas.entry(area.clone()).or_default() += 1;
            }
        }
        staged
    }

    /// True when files from more than one scope are staged together.
    pub fn spans_scopes(&self) -> bool {
        self.scopes.len() > 1
    }

    /// A short report for the author.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.spans_scopes() {
            let parts: Vec<String> = self
                .scopes
                .iter()
                .map(|(scope, files)| format!("{scope} ({})", files.len()))
                .collect();
            lines.push(format!(
                "Staged files span {} scopes: {}. One scope per commit: could a reviewer accept one part and send the rest back? If so, commit them separately.",
                self.scopes.len(),
                parts.join(", ")
            ));
        } else if let Some((scope, files)) = self.scopes.iter().next() {
            let noun = if files.len() == 1 { "file" } else { "files" };
            lines.push(format!("Scope: {scope} ({} {noun}).", files.len()));
        }
        if !self.areas.is_empty() {
            let parts: Vec<String> = self
                .areas
                .iter()
                .map(|(area, count)| format!("{area} ({count})"))
                .collect();
            lines.push(format!(
                "The files suggest: {}. A hint, not a rule: the area is what the change is part of, not what it is made of.",
                parts.join(", ")
            ));
            if self.areas.len() > 1 {
                lines.push(
                    "More than one area is suggested. Check whether this is one decision or several."
                        .to_string(),
                );
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn config() -> Config {
        Config::from_toml(
            "[paths]\napp = ['app/']\nweb = ['app/web/']\nserver = ['server/']\n[hints]\n'*.css' = '🎨'\n'server/' = '🧩'\n",
        )
        .unwrap()
    }

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn matches_should_treat_a_star_as_a_file_ending() {
        assert!(matches("*.css", "app/styles/card.css"));
    }

    #[test]
    fn analyse_should_prefer_the_longest_prefix() {
        let staged = Staged::analyse(&files(&["app/web/index.tsx"]), &config());
        assert!(staged.scopes.contains_key("web"));
    }

    #[test]
    fn analyse_should_notice_two_scopes() {
        let staged = Staged::analyse(&files(&["app/a.tsx", "server/b.ts"]), &config());
        assert!(staged.spans_scopes());
    }

    #[test]
    fn analyse_should_count_area_hints() {
        let staged = Staged::analyse(&files(&["app/a.css", "app/b.css"]), &config());
        assert_eq!(staged.areas.get("🎨"), Some(&2));
    }

    #[test]
    fn analyse_should_keep_files_outside_every_scope() {
        let staged = Staged::analyse(&files(&["README.md"]), &config());
        assert_eq!(staged.unscoped.len(), 1);
    }
}
