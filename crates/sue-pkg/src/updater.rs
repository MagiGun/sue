//! Immutable install generations and the rollback-safe update state machine.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::{ContentStore, PackageManifest, PkgError, Resolution};

/// File name of the persisted active-generation pointer.
const POINTER_FILE: &str = "active-generation";

/// One immutable set of installed packages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generation {
    number: u64,
    packages: BTreeMap<String, PackageManifest>,
}

impl Generation {
    /// Generation number; numbers only grow.
    #[must_use]
    pub fn number(&self) -> u64 {
        self.number
    }

    /// The installed manifest of `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&PackageManifest> {
        self.packages.get(id)
    }

    /// Every installed manifest, by id.
    pub fn packages(&self) -> impl Iterator<Item = &PackageManifest> {
        self.packages.values()
    }

    /// Number of installed packages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// `true` if nothing is installed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

/// Where an update is in its lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdatePhase {
    /// No update in progress.
    Idle,
    /// A candidate generation is assembled.
    Staged,
    /// Every payload in the candidate re-hashed correctly.
    Verified,
    /// The candidate is live but not yet confirmed healthy.
    Activating,
}

/// The result of [`AtomicUpdater::apply_with_health_check`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// The new generation passed its health check and is now permanent.
    Committed {
        /// The new generation.
        generation: u64,
    },
    /// The new generation failed its health check; the previous one is back.
    RolledBack {
        /// The generation that failed.
        failed: u64,
        /// The generation that is active again.
        restored: u64,
    },
}

/// The update state machine: `Idle → Staged → Verified → Activating → Idle`.
/// The live generation is never edited; activation and rollback each move a
/// single pointer.
#[derive(Clone, Debug)]
pub struct AtomicUpdater {
    generations: Vec<Generation>,
    active: usize,
    phase: UpdatePhase,
    candidate: Option<Generation>,
    previous: Option<usize>,
}

impl Default for AtomicUpdater {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicUpdater {
    /// An updater whose active generation 0 is empty.
    #[must_use]
    pub fn new() -> Self {
        Self {
            generations: vec![Generation {
                number: 0,
                packages: BTreeMap::new(),
            }],
            active: 0,
            phase: UpdatePhase::Idle,
            candidate: None,
            previous: None,
        }
    }

    /// The live generation.
    #[must_use]
    pub fn active(&self) -> &Generation {
        &self.generations[self.active]
    }

    /// Every generation ever activated, oldest first.
    #[must_use]
    pub fn generations(&self) -> &[Generation] {
        &self.generations
    }

    /// The current phase.
    #[must_use]
    pub fn phase(&self) -> UpdatePhase {
        self.phase
    }

    fn expect_phase(&self, phase: UpdatePhase, action: &'static str) -> Result<(), PkgError> {
        if self.phase == phase {
            Ok(())
        } else {
            Err(PkgError::InvalidTransition {
                phase: self.phase,
                action,
            })
        }
    }

    fn discard_candidate(&mut self) {
        self.candidate = None;
        self.phase = UpdatePhase::Idle;
    }

    /// Builds the candidate generation: the active packages updated with
    /// `plan`. Every payload must already be in `store`.
    ///
    /// # Errors
    /// [`PkgError::InvalidTransition`] unless idle, or
    /// [`PkgError::MissingPayload`].
    pub fn stage(&mut self, plan: &Resolution, store: &ContentStore) -> Result<(), PkgError> {
        self.expect_phase(UpdatePhase::Idle, "stage")?;
        if let Some(missing) = plan
            .install_order
            .iter()
            .find(|manifest| !store.contains(&manifest.payload))
        {
            return Err(PkgError::MissingPayload {
                id: missing.id.clone(),
            });
        }
        let mut packages = self.active().packages.clone();
        for manifest in &plan.install_order {
            packages.insert(manifest.id.clone(), manifest.clone());
        }
        let number = self
            .generations
            .iter()
            .map(Generation::number)
            .max()
            .unwrap_or(0)
            + 1;
        self.candidate = Some(Generation { number, packages });
        self.phase = UpdatePhase::Staged;
        Ok(())
    }

    /// Re-hashes every payload of the candidate. On any mismatch the candidate
    /// is discarded and the live generation is untouched.
    ///
    /// # Errors
    /// [`PkgError::InvalidTransition`] unless staged, or the first payload error.
    pub fn verify(&mut self, store: &ContentStore) -> Result<(), PkgError> {
        self.expect_phase(UpdatePhase::Staged, "verify")?;
        let result = self.candidate.as_ref().map_or(Ok(()), |candidate| {
            candidate.packages().try_for_each(|manifest| {
                let bytes =
                    store
                        .get(&manifest.payload)
                        .ok_or_else(|| PkgError::MissingPayload {
                            id: manifest.id.clone(),
                        })?;
                manifest.verify_payload(bytes)
            })
        });
        match result {
            Ok(()) => {
                self.phase = UpdatePhase::Verified;
                Ok(())
            }
            Err(error) => {
                self.discard_candidate();
                Err(error)
            }
        }
    }

    /// Makes the verified candidate live by moving the active pointer.
    /// Returns the new generation number.
    ///
    /// # Errors
    /// [`PkgError::InvalidTransition`] unless verified.
    pub fn activate(&mut self) -> Result<u64, PkgError> {
        self.expect_phase(UpdatePhase::Verified, "activate")?;
        let candidate = self.candidate.take().ok_or(PkgError::InvalidTransition {
            phase: self.phase,
            action: "activate",
        })?;
        let number = candidate.number;
        self.generations.push(candidate);
        self.previous = Some(self.active);
        self.active = self.generations.len() - 1;
        self.phase = UpdatePhase::Activating;
        Ok(number)
    }

    /// Keeps the activated generation. Returns its number.
    ///
    /// # Errors
    /// [`PkgError::InvalidTransition`] unless activating.
    pub fn confirm(&mut self) -> Result<u64, PkgError> {
        self.expect_phase(UpdatePhase::Activating, "confirm")?;
        self.previous = None;
        self.phase = UpdatePhase::Idle;
        Ok(self.active().number)
    }

    /// Abandons the update: restores the previous generation if the candidate
    /// was activated, or discards the candidate otherwise. Returns the active
    /// generation number.
    ///
    /// # Errors
    /// [`PkgError::InvalidTransition`] when idle.
    pub fn rollback(&mut self) -> Result<u64, PkgError> {
        match self.phase {
            UpdatePhase::Idle => Err(PkgError::InvalidTransition {
                phase: UpdatePhase::Idle,
                action: "roll back",
            }),
            UpdatePhase::Staged | UpdatePhase::Verified => {
                self.discard_candidate();
                Ok(self.active().number)
            }
            UpdatePhase::Activating => {
                if let Some(previous) = self.previous.take() {
                    self.active = previous;
                }
                self.phase = UpdatePhase::Idle;
                Ok(self.active().number)
            }
        }
    }

    /// Runs the whole pipeline (stage, verify, activate), then keeps the new
    /// generation only if `health_check` approves it, rolling back otherwise.
    ///
    /// # Errors
    /// Any stage or verify error; the live generation is unchanged then.
    pub fn apply_with_health_check<F>(
        &mut self,
        plan: &Resolution,
        store: &ContentStore,
        health_check: F,
    ) -> Result<UpdateOutcome, PkgError>
    where
        F: FnOnce(&Generation) -> bool,
    {
        self.stage(plan, store)?;
        self.verify(store)?;
        let generation = self.activate()?;
        if health_check(self.active()) {
            self.confirm()?;
            Ok(UpdateOutcome::Committed { generation })
        } else {
            let restored = self.rollback()?;
            Ok(UpdateOutcome::RolledBack {
                failed: generation,
                restored,
            })
        }
    }
}

/// Persists the active generation number in `dir` atomically: write a
/// temporary file, flush it to disk, then rename it over the pointer. A crash
/// leaves either the old pointer or the new one, never a torn one.
///
/// # Errors
/// Any I/O error.
pub fn persist_active_pointer(dir: &Path, generation: u64) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!(".{POINTER_FILE}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(generation.to_string().as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, dir.join(POINTER_FILE))
    })();
    if result.is_err() {
        // Best effort: the temporary file is garbage either way.
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Reads the pointer written by [`persist_active_pointer`]. `Ok(None)` if none
/// has been written yet.
///
/// # Errors
/// I/O errors, or [`io::ErrorKind::InvalidData`] for a corrupt pointer.
pub fn read_active_pointer(dir: &Path) -> io::Result<Option<u64>> {
    match fs::read_to_string(dir.join(POINTER_FILE)) {
        Ok(text) => text
            .trim()
            .parse()
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dependency, PackageIndex, PackageKind, resolve};

    fn setup() -> (PackageIndex, ContentStore) {
        let mut index = PackageIndex::new();
        let mut store = ContentStore::new();
        for (id, version, payload) in [
            ("sue.codec.flac", "1.0.0", b"flac v1".as_slice()),
            ("sue.codec.flac", "1.1.0", b"flac v1.1".as_slice()),
            ("sue.studio.daw", "1.0.0", b"daw".as_slice()),
        ] {
            let mut manifest =
                PackageManifest::new(id, version, PackageKind::Codec, payload).unwrap();
            if id == "sue.studio.daw" {
                manifest = manifest.with_dependency("sue.codec.flac", "^1").unwrap();
            }
            store.put(payload.to_vec());
            index.add_local(manifest).unwrap();
        }
        (index, store)
    }

    fn plan(index: &PackageIndex, id: &str, requirement: &str) -> Resolution {
        resolve(index, &[Dependency::parse(id, requirement).unwrap()]).unwrap()
    }

    #[test]
    fn healthy_updates_commit() {
        let (index, store) = setup();
        let mut updater = AtomicUpdater::new();
        let outcome = updater
            .apply_with_health_check(&plan(&index, "sue.studio.daw", "*"), &store, |generation| {
                generation.len() == 2
            })
            .unwrap();
        assert_eq!(outcome, UpdateOutcome::Committed { generation: 1 });
        assert_eq!(
            updater
                .active()
                .get("sue.codec.flac")
                .unwrap()
                .version
                .to_string(),
            "1.1.0"
        );
        assert_eq!(updater.phase(), UpdatePhase::Idle);
    }

    #[test]
    fn failed_health_checks_roll_back() {
        let (index, store) = setup();
        let mut updater = AtomicUpdater::new();
        updater
            .apply_with_health_check(&plan(&index, "sue.codec.flac", "=1.0.0"), &store, |_| true)
            .unwrap();
        let outcome = updater
            .apply_with_health_check(&plan(&index, "sue.codec.flac", "=1.1.0"), &store, |_| false)
            .unwrap();
        assert_eq!(
            outcome,
            UpdateOutcome::RolledBack {
                failed: 2,
                restored: 1
            }
        );
        assert_eq!(
            updater
                .active()
                .get("sue.codec.flac")
                .unwrap()
                .version
                .to_string(),
            "1.0.0"
        );
        assert_eq!(updater.generations().len(), 3);
    }

    #[test]
    fn corrupt_payloads_never_go_live() {
        let (index, mut store) = setup();
        let resolution = plan(&index, "sue.studio.daw", "*");
        let daw = resolution.get("sue.studio.daw").unwrap().payload;
        store.corrupt(&daw, b"dAw".to_vec());
        let mut updater = AtomicUpdater::new();
        assert!(matches!(
            updater.apply_with_health_check(&resolution, &store, |_| true),
            Err(PkgError::HashMismatch { .. })
        ));
        assert_eq!(updater.active().number(), 0);
        assert!(updater.active().is_empty());
        assert_eq!(updater.phase(), UpdatePhase::Idle);
    }

    #[test]
    fn missing_payloads_and_bad_transitions_are_errors() {
        let (index, _) = setup();
        let mut updater = AtomicUpdater::new();
        assert!(matches!(
            updater.stage(&plan(&index, "sue.studio.daw", "*"), &ContentStore::new()),
            Err(PkgError::MissingPayload { .. })
        ));
        assert!(matches!(
            updater.confirm(),
            Err(PkgError::InvalidTransition { .. })
        ));
        assert!(matches!(
            updater.activate(),
            Err(PkgError::InvalidTransition { .. })
        ));
        assert!(updater.rollback().is_err());
    }

    #[test]
    fn manual_rollback_restores_the_previous_generation() {
        let (index, store) = setup();
        let mut updater = AtomicUpdater::new();
        updater
            .stage(&plan(&index, "sue.codec.flac", "*"), &store)
            .unwrap();
        updater.verify(&store).unwrap();
        assert_eq!(updater.activate(), Ok(1));
        assert_eq!(updater.rollback(), Ok(0));
        updater
            .stage(&plan(&index, "sue.codec.flac", "*"), &store)
            .unwrap();
        assert_eq!(updater.rollback(), Ok(0));
        assert_eq!(updater.phase(), UpdatePhase::Idle);
    }

    #[test]
    fn active_pointer_persists_atomically() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("sue-pkg-test-{}-{unique}", std::process::id()));
        assert_eq!(read_active_pointer(&dir).unwrap(), None);
        persist_active_pointer(&dir, 4).unwrap();
        persist_active_pointer(&dir, 5).unwrap();
        assert_eq!(read_active_pointer(&dir).unwrap(), Some(5));
        let leftovers = fs::read_dir(&dir).unwrap().count();
        fs::write(dir.join(POINTER_FILE), "garbage").unwrap();
        let corrupt = read_active_pointer(&dir).unwrap_err();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(leftovers, 1);
        assert_eq!(corrupt.kind(), io::ErrorKind::InvalidData);
    }
}
