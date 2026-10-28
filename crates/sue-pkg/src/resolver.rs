//! Dependency resolution over a verified package index.

use std::collections::{BTreeMap, BTreeSet};

use semver::VersionReq;

use crate::{Dependency, PackageManifest, PkgError, SignedManifest, TrustStore};

/// Every known version of every package. Remote manifests can only enter
/// through [`add_verified`](Self::add_verified).
#[derive(Clone, Debug, Default)]
pub struct PackageIndex {
    packages: BTreeMap<String, Vec<PackageManifest>>,
}

impl PackageIndex {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a signed manifest after verifying it against `trust`.
    ///
    /// # Errors
    /// Signature errors, or [`PkgError::DuplicateVersion`].
    pub fn add_verified(
        &mut self,
        signed: SignedManifest,
        trust: &TrustStore,
    ) -> Result<(), PkgError> {
        trust.verify(&signed)?;
        self.insert(signed.manifest)
    }

    /// Adds a manifest built locally by the user, which needs no signature.
    ///
    /// # Errors
    /// [`PkgError::DuplicateVersion`].
    pub fn add_local(&mut self, manifest: PackageManifest) -> Result<(), PkgError> {
        self.insert(manifest)
    }

    fn insert(&mut self, manifest: PackageManifest) -> Result<(), PkgError> {
        let versions = self.packages.entry(manifest.id.clone()).or_default();
        if versions
            .iter()
            .any(|known| known.version == manifest.version)
        {
            return Err(PkgError::DuplicateVersion {
                id: manifest.id,
                version: manifest.version.to_string(),
            });
        }
        versions.push(manifest);
        versions.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(())
    }

    /// Versions of `id`, newest first.
    #[must_use]
    pub fn versions(&self, id: &str) -> &[PackageManifest] {
        self.packages.get(id).map_or(&[], Vec::as_slice)
    }

    /// Total number of manifests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.packages.values().map(Vec::len).sum()
    }

    /// `true` if the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

/// A consistent set of packages, in install order (dependencies first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// Packages to install, dependencies before dependents.
    pub install_order: Vec<PackageManifest>,
}

impl Resolution {
    /// The chosen manifest for `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&PackageManifest> {
        self.install_order.iter().find(|manifest| manifest.id == id)
    }

    /// Number of packages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.install_order.len()
    }

    /// `true` if nothing needs installing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.install_order.is_empty()
    }
}

type Selection<'a> = BTreeMap<String, &'a PackageManifest>;

/// Resolves `requests` to the newest consistent set of versions.
///
/// The search is complete: candidates are tried newest first, every
/// requirement on a package is checked against its chosen version, and a
/// conflict backtracks to the most recent choice.
///
/// # Errors
/// [`PkgError::UnknownPackage`], [`PkgError::NoMatchingVersion`] or
/// [`PkgError::DependencyCycle`].
pub fn resolve(index: &PackageIndex, requests: &[Dependency]) -> Result<Resolution, PkgError> {
    let selection = search(index, &Selection::new(), &BTreeMap::new(), requests)?;
    order(&selection)
}

fn no_match(id: &str, requirements: &[VersionReq]) -> PkgError {
    PkgError::NoMatchingVersion {
        id: id.to_owned(),
        requirements: requirements.iter().map(ToString::to_string).collect(),
    }
}

fn search<'a>(
    index: &'a PackageIndex,
    selection: &Selection<'a>,
    constraints: &BTreeMap<String, Vec<VersionReq>>,
    pending: &[Dependency],
) -> Result<Selection<'a>, PkgError> {
    let Some((next, rest)) = pending.split_first() else {
        return Ok(selection.clone());
    };
    let mut constraints = constraints.clone();
    let requirements = constraints.entry(next.id.clone()).or_default();
    requirements.push(next.requirement.clone());
    let requirements = requirements.clone();

    if let Some(chosen) = selection.get(&next.id) {
        return if next.requirement.matches(&chosen.version) {
            search(index, selection, &constraints, rest)
        } else {
            Err(no_match(&next.id, &requirements))
        };
    }

    let candidates = index.versions(&next.id);
    if candidates.is_empty() {
        return Err(PkgError::UnknownPackage(next.id.clone()));
    }
    let mut last_error = None;
    for candidate in candidates.iter().filter(|candidate| {
        requirements
            .iter()
            .all(|requirement| requirement.matches(&candidate.version))
    }) {
        let mut selection = selection.clone();
        selection.insert(next.id.clone(), candidate);
        let mut queue = rest.to_vec();
        queue.extend(candidate.dependencies.iter().cloned());
        match search(index, &selection, &constraints, &queue) {
            Ok(done) => return Ok(done),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| no_match(&next.id, &requirements)))
}

/// Topological order with deterministic (alphabetical) ties.
fn order(selection: &Selection<'_>) -> Result<Resolution, PkgError> {
    let mut waiting_on: BTreeMap<&str, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, manifest) in selection {
        waiting_on.insert(id, manifest.dependencies.len());
        for dependency in &manifest.dependencies {
            dependents
                .entry(dependency.id.as_str())
                .or_default()
                .push(id);
        }
    }
    let mut ready: BTreeSet<&str> = waiting_on
        .iter()
        .filter(|&(_, &count)| count == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut install_order = Vec::with_capacity(selection.len());
    while let Some(id) = ready.pop_first() {
        install_order.push(selection[id].clone());
        for &dependent in dependents.get(id).into_iter().flatten() {
            let count = waiting_on
                .get_mut(dependent)
                .expect("every dependent is selected");
            *count -= 1;
            if *count == 0 {
                ready.insert(dependent);
            }
        }
    }
    if install_order.len() < selection.len() {
        let stuck = waiting_on
            .into_iter()
            .filter(|&(_, count)| count > 0)
            .map(|(id, _)| id.to_owned())
            .collect();
        return Err(PkgError::DependencyCycle(stuck));
    }
    Ok(Resolution { install_order })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackageKind;

    fn package(id: &str, version: &str, dependencies: &[(&str, &str)]) -> PackageManifest {
        let mut manifest =
            PackageManifest::new(id, version, PackageKind::Applet, id.as_bytes()).unwrap();
        for &(dependency, requirement) in dependencies {
            manifest = manifest.with_dependency(dependency, requirement).unwrap();
        }
        manifest
    }

    fn index(packages: Vec<PackageManifest>) -> PackageIndex {
        let mut index = PackageIndex::new();
        for manifest in packages {
            index.add_local(manifest).unwrap();
        }
        index
    }

    fn request(id: &str, requirement: &str) -> Vec<Dependency> {
        vec![Dependency::parse(id, requirement).unwrap()]
    }

    fn chosen(resolution: &Resolution) -> Vec<String> {
        resolution
            .install_order
            .iter()
            .map(|manifest| format!("{}@{}", manifest.id, manifest.version))
            .collect()
    }

    #[test]
    fn picks_newest_compatible_versions_in_dependency_order() {
        let index = index(vec![
            package("daw", "1.0.0", &[("codec", "^1"), ("ui", "^2")]),
            package("codec", "1.0.0", &[]),
            package("codec", "1.4.0", &[]),
            package("codec", "2.0.0", &[]),
            package("ui", "2.1.0", &[("codec", ">=1.2")]),
        ]);
        let resolution = resolve(&index, &request("daw", "*")).unwrap();
        assert_eq!(
            chosen(&resolution),
            ["codec@1.4.0", "ui@2.1.0", "daw@1.0.0"]
        );
        assert_eq!(
            resolution.get("codec").unwrap().version.to_string(),
            "1.4.0"
        );
        assert_eq!(index.versions("codec")[0].version.to_string(), "2.0.0");
    }

    #[test]
    fn backtracks_to_older_versions_when_needed() {
        let index = index(vec![
            package("studio", "2.0.0", &[("engine", "^2")]),
            package("studio", "1.0.0", &[("engine", "^1")]),
            package("engine", "1.5.0", &[]),
        ]);
        let resolution = resolve(&index, &request("studio", "*")).unwrap();
        assert_eq!(chosen(&resolution), ["engine@1.5.0", "studio@1.0.0"]);
    }

    #[test]
    fn reports_conflicts_unknowns_and_cycles() {
        let conflict = index(vec![
            package("a", "1.0.0", &[("c", "^1")]),
            package("b", "1.0.0", &[("c", "^2")]),
            package("c", "1.0.0", &[]),
            package("c", "2.0.0", &[]),
        ]);
        let mut requests = request("a", "*");
        requests.extend(request("b", "*"));
        assert!(matches!(
            resolve(&conflict, &requests),
            Err(PkgError::NoMatchingVersion { .. })
        ));

        assert_eq!(
            resolve(&PackageIndex::new(), &request("ghost", "*")),
            Err(PkgError::UnknownPackage("ghost".into()))
        );

        let cycle = index(vec![
            package("x", "1.0.0", &[("y", "*")]),
            package("y", "1.0.0", &[("x", "*")]),
        ]);
        assert_eq!(
            resolve(&cycle, &request("x", "*")),
            Err(PkgError::DependencyCycle(vec!["x".into(), "y".into()]))
        );
    }

    #[test]
    fn duplicate_versions_are_rejected() {
        let mut index = PackageIndex::new();
        index.add_local(package("a", "1.0.0", &[])).unwrap();
        assert!(matches!(
            index.add_local(package("a", "1.0.0", &[])),
            Err(PkgError::DuplicateVersion { .. })
        ));
        assert_eq!(index.len(), 1);
    }
}
