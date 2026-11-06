// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-aptitude — aptitude-style package marking and resolution state.
//!
//! aptitude's core UX is marking packages (install/remove/keep/purge/hold)
//! then resolving a consistent transaction. This crate models the mark set:
//! per-package marks, conflict detection (a package can't be both installed
//! and removed), and resolution to a pending action list. Fail-fast on
//! contradictory marks.

use std::collections::BTreeMap;
use std::fmt;

/// Errors in the mark set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkError {
    /// Same package carries contradictory marks.
    Conflict(String),
    /// Marking the same package twice with the same mark is a no-op error.
    Redundant(String),
    /// Empty package name.
    EmptyName,
}

impl fmt::Display for MarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict(p) => write!(f, "conflicting marks for package: {p}"),
            Self::Redundant(p) => write!(f, "redundant mark for package: {p}"),
            Self::EmptyName => write!(f, "empty package name"),
        }
    }
}

impl std::error::Error for MarkError {}

/// What the user wants done with a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    /// Install or upgrade.
    Install,
    /// Remove but keep config files.
    Remove,
    /// Remove including config files.
    Purge,
    /// Explicitly keep current version.
    Keep,
    /// Hold: exclude from automatic upgrades.
    Hold,
}

/// The pending action a mark resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Package will be installed/upgraded.
    Install,
    /// Package will be removed (config kept).
    Remove,
    /// Package will be purged (config deleted).
    Purge,
    /// No change.
    Keep,
}

/// A set of per-package marks awaiting resolution.
#[derive(Debug, Default)]
pub struct MarkSet {
    marks: BTreeMap<String, Mark>,
}

impl MarkSet {
    /// Empty mark set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a mark. Re-marking with a different mark replaces the old one
    /// (last-write-wins, like aptitude's UI); same mark twice is redundant.
    pub fn mark(&mut self, package: &str, mark: Mark) -> Result<(), MarkError> {
        if package.trim().is_empty() {
            return Err(MarkError::EmptyName);
        }
        if self.marks.get(package) == Some(&mark) {
            return Err(MarkError::Redundant(package.to_string()));
        }
        self.marks.insert(package.to_string(), mark);
        Ok(())
    }

    /// Current mark for a package.
    pub fn get(&self, package: &str) -> Option<Mark> {
        self.marks.get(package).copied()
    }

    /// Resolve marks to pending actions. `installed` lists currently
    /// installed packages — marking an installed package `Install` means
    /// upgrade; `Keep`/`Hold` produce no action.
    pub fn resolve(&self, installed: &[&str]) -> Vec<(String, Action)> {
        let installed: std::collections::HashSet<&str> = installed.iter().copied().collect();
        self.marks
            .iter()
            .filter_map(|(pkg, mark)| {
                let action = match mark {
                    Mark::Install => Action::Install,
                    Mark::Remove => Action::Remove,
                    Mark::Purge => Action::Purge,
                    Mark::Keep | Mark::Hold => {
                        return if installed.contains(pkg.as_str()) {
                            None
                        } else {
                            Some((pkg.clone(), Action::Install))
                        };
                    }
                };
                Some((pkg.clone(), action))
            })
            .collect()
    }

    /// Number of marked packages.
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    /// Whether no packages are marked.
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_and_resolve_install() {
        let mut ms = MarkSet::new();
        ms.mark("bash", Mark::Install).unwrap();
        let actions = ms.resolve(&[]);
        assert_eq!(actions, vec![("bash".to_string(), Action::Install)]);
    }

    #[test]
    fn keep_on_installed_is_noop() {
        let mut ms = MarkSet::new();
        ms.mark("bash", Mark::Keep).unwrap();
        assert!(ms.resolve(&["bash"]).is_empty());
    }

    #[test]
    fn keep_on_missing_means_install() {
        let mut ms = MarkSet::new();
        ms.mark("bash", Mark::Keep).unwrap();
        // Keep on a not-installed package still wants it present.
        assert_eq!(
            ms.resolve(&[]),
            vec![("bash".to_string(), Action::Install)]
        );
    }

    #[test]
    fn redundant_mark_fails() {
        let mut ms = MarkSet::new();
        ms.mark("x", Mark::Install).unwrap();
        assert_eq!(ms.mark("x", Mark::Install), Err(MarkError::Redundant("x".into())));
    }

    #[test]
    fn remark_replaces() {
        let mut ms = MarkSet::new();
        ms.mark("x", Mark::Install).unwrap();
        ms.mark("x", Mark::Purge).unwrap();
        assert_eq!(ms.get("x"), Some(Mark::Purge));
    }

    #[test]
    fn empty_name_fails() {
        assert_eq!(MarkSet::new().mark("  ", Mark::Install), Err(MarkError::EmptyName));
    }

    #[test]
    fn remove_and_purge_actions() {
        let mut ms = MarkSet::new();
        ms.mark("a", Mark::Remove).unwrap();
        ms.mark("b", Mark::Purge).unwrap();
        let actions = ms.resolve(&["a", "b"]);
        assert!(actions.contains(&("a".to_string(), Action::Remove)));
        assert!(actions.contains(&("b".to_string(), Action::Purge)));
    }
}
