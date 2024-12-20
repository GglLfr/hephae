use bevy_asset::prelude::*;
use bevy_ecs::{
    event::EventReader,
    prelude::{Resource, *},
};
use bevy_image::prelude::*;
use bevy_render::{
    render_resource::{BindGroup, BindGroupEntry, BindGroupLayout},
    renderer::RenderDevice,
    Extract,
};
use bevy_utils::{Entry, HashMap};

#[derive(Resource, Default)]
pub struct ImageAssetEvents(Vec<AssetEvent<Image>>);

#[derive(Resource, Default)]
pub struct ImageBindGroups(HashMap<AssetId<Image>, BindGroup>);
impl ImageBindGroups {
    #[inline]
    pub fn create(
        &mut self,
        id: impl Into<AssetId<Image>>,
        device: &RenderDevice,
        layout: &BindGroupLayout,
        entries: &[BindGroupEntry],
    ) -> bool {
        match self.0.entry(id.into()) {
            Entry::Vacant(e) => {
                e.insert(device.create_bind_group("hephae_atlas_page", layout, entries));
                true
            }
            Entry::Occupied(..) => false,
        }
    }

    #[inline]
    pub fn get(&self, id: impl Into<AssetId<Image>>) -> Option<&BindGroup> {
        self.0.get(&id.into())
    }
}

pub fn extract_image_events(
    mut events: ResMut<ImageAssetEvents>,
    mut image_events: Extract<EventReader<AssetEvent<Image>>>,
) {
    let images = &mut events.0;
    images.extend(image_events.read());
}

pub fn validate_image_bind_groups(mut image_bind_groups: ResMut<ImageBindGroups>, mut events: ResMut<ImageAssetEvents>) {
    for event in events.0.drain(..) {
        match event {
            AssetEvent::Added { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
            AssetEvent::Modified { id } | AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                image_bind_groups.0.remove(&id);
            }
        }
    }
}
