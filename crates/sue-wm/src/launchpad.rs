//! The runtime applet registry behind the Hub, and pin-to-boot.

use std::fmt;

use sue_rice::RiceDeclaration;

use crate::{AppletCategory, AppletId, AppletManifest, SueApplet, WmError};

/// Builds a fresh applet instance. Factories must be cheap and side-effect
/// free; heavy work belongs in [`SueApplet::mount`].
pub type AppletFactory = Box<dyn Fn() -> Box<dyn SueApplet> + Send + Sync>;

struct Registration {
    manifest: AppletManifest,
    factory: AppletFactory,
}

/// What Sue opens at launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootTarget {
    /// The Hub, listing every installed applet.
    Hub,
    /// A pinned applet.
    Applet(AppletId),
}

/// The Hub's listing, sorted by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HubView<'a> {
    /// Full creative environments.
    pub studios: Vec<&'a AppletManifest>,
    /// Everyday tools.
    pub utilities: Vec<&'a AppletManifest>,
}

/// The registry of installed applets. It knows applets only through their
/// factories and manifests; nothing here lists applets by name (LAW 1).
#[derive(Default)]
pub struct Launchpad {
    registrations: Vec<Registration>,
    pinned_boot_applet: Option<AppletId>,
}

impl fmt::Debug for Launchpad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Launchpad")
            .field(
                "applets",
                &self
                    .manifests()
                    .map(|manifest| manifest.id.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("pinned_boot_applet", &self.pinned_boot_applet)
            .finish()
    }
}

impl Launchpad {
    /// An empty launchpad.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs an applet. The factory is called once to read the manifest.
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

    /// Uninstalls an applet. Returns `true` if it was installed.
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

    /// Every installed applet, in installation order.
    pub fn manifests(&self) -> impl Iterator<Item = &AppletManifest> {
        self.registrations
            .iter()
            .map(|registration| &registration.manifest)
    }

    /// The Hub listing: studios and utilities, each sorted by name.
    #[must_use]
    pub fn hub(&self) -> HubView<'_> {
        let sorted = |category: AppletCategory| {
            let mut listed: Vec<&AppletManifest> = self
                .manifests()
                .filter(|manifest| manifest.category == category)
                .collect();
            listed.sort_by(|a, b| a.name.cmp(&b.name));
            listed
        };
        HubView {
            studios: sorted(AppletCategory::Studio),
            utilities: sorted(AppletCategory::Utility),
        }
    }

    /// Pins `id` so Sue boots straight into it.
    ///
    /// # Errors
    /// [`WmError::UnknownApplet`] if it is not installed.
    pub fn pin_boot(&mut self, id: &AppletId) -> Result<(), WmError> {
        if self.manifest(id).is_none() {
            return Err(WmError::UnknownApplet(id.clone()));
        }
        self.pinned_boot_applet = Some(id.clone());
        Ok(())
    }

    /// Boots into the Hub again.
    pub fn unpin_boot(&mut self) {
        self.pinned_boot_applet = None;
    }

    /// The pinned applet, if any.
    #[must_use]
    pub fn pinned_boot_applet(&self) -> Option<&AppletId> {
        self.pinned_boot_applet.as_ref()
    }

    /// What to open at launch. A pinned applet that has since been uninstalled
    /// falls back to the Hub.
    #[must_use]
    pub fn boot_target(&self) -> BootTarget {
        match &self.pinned_boot_applet {
            Some(id) if self.manifest(id).is_some() => BootTarget::Applet(id.clone()),
            _ => BootTarget::Hub,
        }
    }

    /// A fresh instance of `id`.
    ///
    /// # Errors
    /// [`WmError::UnknownApplet`].
    pub fn instantiate(&self, id: &AppletId) -> Result<Box<dyn SueApplet>, WmError> {
        self.registrations
            .iter()
            .find(|registration| &registration.manifest.id == id)
            .map(|registration| (registration.factory)())
            .ok_or_else(|| WmError::UnknownApplet(id.clone()))
    }

    /// Every riceable declared by installed applets.
    #[must_use]
    pub fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        self.registrations
            .iter()
            .flat_map(|registration| (registration.factory)().rice_declarations())
            .collect()
    }

    /// Number of installed applets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// `true` if no applet is installed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{AppletContext, AppletError, InputResponse, Zone};
    use sue_platform::InputEvent;
    use sue_rice::RiceParameter;

    /// A minimal applet for tests.
    pub(crate) struct Fixture {
        pub(crate) id: &'static str,
        pub(crate) name: &'static str,
        pub(crate) category: AppletCategory,
        pub(crate) uses_timeline: bool,
    }

    impl SueApplet for Fixture {
        fn manifest(&self) -> AppletManifest {
            AppletManifest {
                id: AppletId::new(self.id).unwrap(),
                name: self.name.to_owned(),
                category: self.category,
                home_zone: Zone::CenterStage,
                uses_timeline: self.uses_timeline,
                summary: String::new(),
                opens: Vec::new(),
                accepts_drops: false,
            }
        }

        fn rice_declarations(&self) -> Vec<RiceDeclaration> {
            vec![RiceDeclaration::new(
                format!("{}.enabled", self.id.replace('-', "_")),
                RiceParameter::Bool { value: true },
                "Fixture toggle",
            )]
        }

        fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
            Ok(())
        }

        fn handle_input(
            &mut self,
            _event: &InputEvent,
            _ctx: &mut AppletContext<'_>,
        ) -> Result<InputResponse, AppletError> {
            Ok(InputResponse::Ignored)
        }
    }

    pub(crate) fn fixture(
        id: &'static str,
        name: &'static str,
        category: AppletCategory,
        uses_timeline: bool,
    ) -> impl Fn() -> Box<dyn SueApplet> + Send + Sync + 'static {
        move || {
            Box::new(Fixture {
                id,
                name,
                category,
                uses_timeline,
            })
        }
    }

    fn launchpad() -> Launchpad {
        let mut launchpad = Launchpad::new();
        launchpad
            .register(fixture(
                "video",
                "Video Editor",
                AppletCategory::Studio,
                true,
            ))
            .unwrap();
        launchpad
            .register(fixture(
                "player",
                "Media Player",
                AppletCategory::Utility,
                false,
            ))
            .unwrap();
        launchpad
            .register(fixture("cad", "CAD Studio", AppletCategory::Studio, false))
            .unwrap();
        launchpad
    }

    #[test]
    fn groups_and_sorts_the_hub() {
        let launchpad = launchpad();
        let hub = launchpad.hub();
        let names =
            |list: &[&AppletManifest]| list.iter().map(|m| m.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&hub.studios), ["CAD Studio", "Video Editor"]);
        assert_eq!(names(&hub.utilities), ["Media Player"]);
        assert_eq!(launchpad.len(), 3);
        assert_eq!(launchpad.rice_declarations().len(), 3);
    }

    #[test]
    fn rejects_duplicates() {
        let mut launchpad = launchpad();
        assert!(matches!(
            launchpad.register(fixture("cad", "CAD Again", AppletCategory::Studio, false)),
            Err(WmError::DuplicateApplet(_))
        ));
    }

    #[test]
    fn pins_and_falls_back_to_the_hub() {
        let mut launchpad = launchpad();
        assert_eq!(launchpad.boot_target(), BootTarget::Hub);
        let cad = AppletId::new("cad").unwrap();
        launchpad.pin_boot(&cad).unwrap();
        assert_eq!(launchpad.boot_target(), BootTarget::Applet(cad.clone()));
        assert!(launchpad.unregister(&cad));
        assert_eq!(launchpad.boot_target(), BootTarget::Hub);
        assert_eq!(launchpad.pinned_boot_applet(), Some(&cad));
        assert!(
            launchpad
                .pin_boot(&AppletId::new("ghost").unwrap())
                .is_err()
        );
        launchpad.unpin_boot();
        assert!(launchpad.pinned_boot_applet().is_none());
        assert!(launchpad.instantiate(&cad).is_err());
        assert!(
            launchpad
                .instantiate(&AppletId::new("video").unwrap())
                .is_ok()
        );
    }
}
