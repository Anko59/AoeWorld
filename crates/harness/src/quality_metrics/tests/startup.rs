use crate::quality_metrics::march::{self, OrderPath};
use aoe_map::{
    EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider,
    ResourceKind,
};
use aoe_simulation::{GameWorld, UnitKind};
use std::sync::Arc;

#[derive(Debug)]
struct Pages(Vec<Arc<EnvironmentPage>>);
impl EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.0
            .iter()
            .find(|page| page.key() == key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}
fn bare_map() -> GameWorld {
    let fixture = aoe_simulation::terrain_fixtures::build("river");
    let pages = fixture
        .elevation
        .into_iter()
        .map(EnvironmentPage::Elevation)
        .chain(fixture.water.into_iter().map(EnvironmentPage::Water))
        .chain(
            fixture
                .vegetation
                .into_iter()
                .map(EnvironmentPage::Vegetation),
        )
        .chain(
            fixture
                .hydrology
                .into_iter()
                .map(EnvironmentPage::HydrologyEvidence),
        )
        .chain(
            fixture
                .cover
                .into_iter()
                .map(EnvironmentPage::ModernLandCover),
        )
        .map(Arc::new)
        .collect();
    GameWorld::from_page_provider(fixture.package, Arc::new(Pages(pages))).unwrap()
}
#[test]
fn bare_clearing_gets_the_same_normal_starting_resources_for_every_order_path() {
    let mut reference = None;
    for path in OrderPath::ALL {
        let scene = march::populate(
            bare_map(),
            (35, 43),
            (145, 48),
            path,
            UnitKind::Militia,
            8000,
        )
        .unwrap();
        let resources = scene.world.starting_resources();
        assert!(
            resources
                .iter()
                .filter(|node| node.kind == ResourceKind::Wood)
                .count()
                >= 8
        );
        let identity: Vec<_> = resources.iter().map(|node| (node.id, node.tile)).collect();
        if let Some(reference) = &reference {
            assert_eq!(reference, &identity);
        } else {
            reference = Some(identity);
        }
        if path == OrderPath::Steward {
            assert!(
                scene
                    .ids
                    .iter()
                    .all(|id| scene.world.steward_controller(*id) == scene.controller)
            );
        }
    }
}
