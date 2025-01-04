//! Utilities to store and keep track of [`Image`]s as [`BindGroup`]s in the render world.
//!
//! See `examples/atlas.rs` for example usage.

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

/// Extracts [`AssetEvent<Image>`]s from the main world to the render world.
#[derive(Resource, Default)]
pub struct ImageAssetEvents(Vec<AssetEvent<Image>>);

/// Stores [`BindGroup`]s for each [`Image`] on-demand via [`create`](ImageBindGroups::create).
#[derive(Resource, Default)]
pub struct ImageBindGroups(HashMap<AssetId<Image>, BindGroup>);
impl ImageBindGroups {
    /// Ensures a [`BindGroup`] for a given [`Image`] is created, returning `true` if it exists
    /// already. Should work in concert with
    /// [`Vertex::create_batch`](hephae_render::vertex::Vertex::create_batch).
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

    /// Gets the [`BindGroup`] previously created with [`create`](Self::create). Should work in
    /// concert with [`RenderCommand::render`](bevy_render::render_phase::RenderCommand::render).
    #[inline]
    pub fn get(&self, id: impl Into<AssetId<Image>>) -> Option<&BindGroup> {
        self.0.get(&id.into())
    }
}

/// Populates [`ImageAssetEvents`].
pub fn extract_image_events(
    mut events: ResMut<ImageAssetEvents>,
    mut image_events: Extract<EventReader<AssetEvent<Image>>>,
) {
    let images = &mut events.0;
    images.extend(image_events.read());
}

/// For each removed [`Image`], remove the [`BindGroup`] in [`ImageBindGroups`] too.
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
