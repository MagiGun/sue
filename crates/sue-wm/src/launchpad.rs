//! The Launchpad: the runtime registry of applets, and [`Applets`],
//! the live instances the shell drives.

use sue_core::Entity;
use sue_rice::RiceDeclaration;

use crate::{AppletId, AppletManifest, AppletRole, SueApplet, WmError};

/// Builds a fresh applet.
pub type AppletFactory = Box<dyn Fn() -> Box<dyn SueApplet> + Send + Sync>;

struct Registration {
    manifest: AppletManifest,
    factory: AppletFactory,
}

/// Every installed applet, registered at runtime.
#[derive(Default)]
pub struct Launchpad {
    registrations: Vec<Registration>,
}

impl std::fmt::Debug for Launchpad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.manifests().map(|manifest| manifest.id.as_str()))
            .finish()
    }
}

impl Launchpad {
    /// An empty Launchpad.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an applet factory.
    ///
    /// # Errors
    /// [`WmError::DuplicateApplet`] if the id is taken.
    pub fn register<F>(&mut self, factory: F) -> Result<AppletId, WmError>
    where
        F: Fn() -> Box<dyn SueApplet> + Send + Sync + 'static,
    {
        let manifest = factory().manifest();
        if self.manifest(&manifest.id).is_some() {
            return Err(WmError::DuplicateApplet(manifest.id));
        }
        let id = manifest.id.clone();
        self.registrations.push(Registration {
            manifest,
            factory: Box::new(factory),
        });
        Ok(id)
    }

    /// Removes an applet. Returns `false` if it was not installed.
    pub fn unregister(&mut self, id: &AppletId) -> bool {
        let before = self.registrations.len();
        self.registrations
            .retain(|registration| &registration.manifest.id != id);
        before != self.registrations.len()
    }

    /// The manifest of `id`.
    #[must_use]
    pub fn manifest(&self, id: &AppletId) -> Option<&AppletManifest> {
        self.manifests().find(|manifest| &manifest.id == id)
    }

    /// Every manifest, in registration order.
    pub fn manifests(&self) -> impl Iterator<Item = &AppletManifest> {
        self.registrations
            .iter()
            .map(|registration| &registration.manifest)
    }

    /// Every riceable the installed applets declare, and each studio's and
    /// tool's language (`<section>.language`).
    #[must_use]
    pub fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        let mut declarations: Vec<RiceDeclaration> = self
            .registrations
            .iter()
            .flat_map(|registration| (registration.factory)().rice_declarations())
            .collect();
        declarations.extend(
            self.manifests()
                .filter(|manifest| manifest.role.is_page())
                .map(|manifest| crate::language_declaration(&manifest.id)),
        );
        declarations
    }

    /// One live instance of every applet.
    #[must_use]
    pub fn instantiate(&self) -> Applets {
        let mut list: Vec<(AppletManifest, Box<dyn SueApplet>)> = self
            .registrations
            .iter()
            .map(|registration| (registration.manifest.clone(), (registration.factory)()))
            .collect();
        list.sort_by_key(|(manifest, _)| manifest.order);
        Applets { list }
    }

    /// Number of installed applets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// `true` with nothing installed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }
}

/// The live applets, sorted by their manifest order.
#[derive(Default)]
pub struct Applets {
    list: Vec<(AppletManifest, Box<dyn SueApplet>)>,
}

impl std::fmt::Debug for Applets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.list.iter().map(|(manifest, _)| manifest.id.as_str()))
            .finish()
    }
}

impl Applets {
    /// Every manifest, sorted.
    pub fn manifests(&self) -> impl Iterator<Item = &AppletManifest> {
        self.list.iter().map(|(manifest, _)| manifest)
    }

    /// Manifests with `role`, sorted.
    pub fn with_role(&self, role: AppletRole) -> impl Iterator<Item = &AppletManifest> {
        self.manifests()
            .filter(move |manifest| manifest.role == role)
    }

    /// The manifest of `id`.
    #[must_use]
    pub fn manifest(&self, id: &AppletId) -> Option<&AppletManifest> {
        self.manifests().find(|manifest| &manifest.id == id)
    }

    /// The applet `id`.
    pub fn get_mut(&mut self, id: &AppletId) -> Option<&mut Box<dyn SueApplet>> {
        self.list
            .iter_mut()
            .find(|(manifest, _)| &manifest.id == id)
            .map(|(_, applet)| applet)
    }

    /// The applet `id`.
    #[must_use]
    pub fn get(&self, id: &AppletId) -> Option<&dyn SueApplet> {
        self.list
            .iter()
            .find(|(manifest, _)| &manifest.id == id)
            .map(|(_, applet)| applet.as_ref())
    }

    /// The first studio or tool that works on `entity`, studios first.
    #[must_use]
    pub fn page_for(&self, entity: &Entity) -> Option<AppletId> {
        [AppletRole::Studio, AppletRole::Tool]
            .into_iter()
            .find_map(|role| {
                self.list
                    .iter()
                    .find(|(manifest, applet)| manifest.role == role && applet.accepts(entity))
            })
            .map(|(manifest, _)| manifest.id.clone())
    }
    /// Every applet with its manifest.
    pub fn iter(&self) -> impl Iterator<Item = (&AppletManifest, &dyn SueApplet)> {
        self.list
            .iter()
            .map(|(manifest, applet)| (manifest, applet.as_ref()))
    }

    /// Every applet, mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&AppletManifest, &mut Box<dyn SueApplet>)> {
        self.list
            .iter_mut()
            .map(|(manifest, applet)| (&*manifest, applet))
    }

    /// `true` while some applet needs every key.
    #[must_use]
    pub fn captures_keys(&self) -> bool {
        self.list.iter().any(|(_, applet)| applet.captures_keys())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Arena, StringBuffer};

    /// A studio or tool for tests that takes entities named after it.
    struct Probe(&'static str, AppletRole, i32);

    impl SueApplet for Probe {
        fn manifest(&self) -> AppletManifest {
            let mut manifest = AppletManifest::new(self.0, self.0, self.1, "*");
            manifest.order = self.2;
            manifest
        }

        fn accepts(&self, entity: &Entity) -> bool {
            entity.name() == "shared" || entity.name() == self.0
        }
    }

    #[test]
    fn applets_register_once_and_come_back_sorted() {
        let mut launchpad = Launchpad::new();
        launchpad
            .register(|| Box::new(Probe("paint", AppletRole::Studio, 20)))
            .unwrap();
        launchpad
            .register(|| Box::new(Probe("writer", AppletRole::Studio, 10)))
            .unwrap();
        assert!(matches!(
            launchpad.register(|| Box::new(Probe("paint", AppletRole::Tool, 1))),
            Err(WmError::DuplicateApplet(_))
        ));
        let applets = launchpad.instantiate();
        let order: Vec<&str> = applets.manifests().map(|m| m.id.as_str()).collect();
        assert_eq!(order, ["writer", "paint"]);
        assert!(launchpad.unregister(&AppletId::new("paint").unwrap()));
        assert_eq!(launchpad.len(), 1);
    }

    #[test]
    fn an_entity_opens_in_the_first_studio_that_takes_it_then_a_tool() {
        let mut launchpad = Launchpad::new();
        for (id, role, order) in [
            ("viewer", AppletRole::Tool, 1),
            ("paint", AppletRole::Studio, 20),
            ("settings", AppletRole::Window, 0),
        ] {
            launchpad
                .register(move || Box::new(Probe(id, role, order)))
                .unwrap();
        }
        let applets = launchpad.instantiate();
        let mut arena = Arena::new();
        let mut page_for = |name: &str| {
            let handle = arena.spawn(name, StringBuffer::new());
            let entity = arena.get(handle).unwrap();
            applets.page_for(entity).map(|id| id.as_str().to_owned())
        };
        assert_eq!(
            page_for("shared").as_deref(),
            Some("paint"),
            "studios first"
        );
        assert_eq!(page_for("viewer").as_deref(), Some("viewer"));
        assert_eq!(page_for("settings"), None, "a window is not a page");
        assert_eq!(page_for("nobody"), None);
        assert!(AppletRole::Tool.is_page() && !AppletRole::Window.is_page());
    }

    #[test]
    fn ids_are_validated() {
        assert!(AppletId::new("hex-view2").is_ok());
        assert!(AppletId::new("Hex").is_err());
        assert!(AppletId::new("2hex").is_err());
    }
}
