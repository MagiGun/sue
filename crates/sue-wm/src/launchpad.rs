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

    /// Every riceable the installed applets declare.
    #[must_use]
    pub fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        self.registrations
            .iter()
            .flat_map(|registration| (registration.factory)().rice_declarations())
            .collect()
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

    /// The first node applet that accepts `entity`.
    #[must_use]
    pub fn node_applet_for(&self, entity: &Entity) -> Option<AppletId> {
        self.list
            .iter()
            .find(|(manifest, applet)| manifest.role == AppletRole::Node && applet.accepts(entity))
            .map(|(manifest, _)| manifest.id.clone())
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
pub(crate) mod tests {
    use super::*;
    use crate::{AppletContext, FileTemplate, ModeSpec};

    /// A node applet for tests.
    pub(crate) struct Probe(pub &'static str, pub AppletRole, pub i32);

    impl SueApplet for Probe {
        fn manifest(&self) -> AppletManifest {
            let mut manifest = AppletManifest::new(self.0, self.0, self.1, "*");
            manifest.order = self.2;
            if self.1 == AppletRole::Node {
                manifest.mode = Some(ModeSpec {
                    label: self.0.to_owned(),
                    in_pill: true,
                });
                manifest.lane = Some(format!("{} Track", self.0));
            }
            manifest
        }

        fn templates(&self) -> Vec<FileTemplate> {
            if self.1 == AppletRole::Node {
                vec![FileTemplate::new("Probe", self.0, "plain")]
            } else {
                Vec::new()
            }
        }

        fn new_entity(&mut self, _cx: &mut AppletContext<'_>) -> Option<crate::NewEntity> {
            Some(crate::NewEntity {
                name: self.0.to_owned(),
                base: sue_core::StringBuffer::new().into(),
                meta: Vec::new(),
                size: egui::vec2(200.0, 120.0),
                props: Vec::new(),
            })
        }
    }

    #[test]
    fn applets_register_once_and_come_back_sorted() {
        let mut launchpad = Launchpad::new();
        launchpad
            .register(|| Box::new(Probe("paint", AppletRole::Node, 20)))
            .unwrap();
        launchpad
            .register(|| Box::new(Probe("document", AppletRole::Node, 10)))
            .unwrap();
        assert!(matches!(
            launchpad.register(|| Box::new(Probe("paint", AppletRole::Node, 1))),
            Err(WmError::DuplicateApplet(_))
        ));
        let applets = launchpad.instantiate();
        let order: Vec<&str> = applets.manifests().map(|m| m.id.as_str()).collect();
        assert_eq!(order, ["document", "paint"]);
        assert!(launchpad.unregister(&AppletId::new("paint").unwrap()));
        assert_eq!(launchpad.len(), 1);
    }

    #[test]
    fn ids_are_validated() {
        assert!(AppletId::new("hex-view2").is_ok());
        assert!(AppletId::new("Hex").is_err());
        assert!(AppletId::new("2hex").is_err());
    }
}
