use bevy_core_pipeline::core_2d::*;
use bevy_ecs::{
    prelude::*,
    query::{QueryData, QueryItem},
    system::{StaticSystemParam, SystemParam, SystemParamItem, lifetimeless::Read},
};
use bevy_math::prelude::*;
use bevy_render::prelude::*;
use bevy_transform::prelude::*;

pub trait UiRoot: Component {
    type Param: SystemParam;
    type Item: QueryData;

    fn compute_root_transform(
        &mut self,
        param: &mut SystemParamItem<Self::Param>,
        item: QueryItem<Self::Item>,
    ) -> (Transform, Vec2);
}

#[derive(Component, Copy, Clone, Default)]
pub struct UiUnrounded;

#[derive(Component, Copy, Clone, Default)]
pub(crate) struct UiRootTrns {
    pub transform: Transform,
    pub size: Vec2,
}

pub(crate) fn compute_root_transform<T: UiRoot>(
    mut param: StaticSystemParam<T::Param>,
    mut query: Query<(&mut T, &mut UiRootTrns, T::Item)>,
) {
    for (mut root, mut output, item) in &mut query {
        let (transform, size) = root.compute_root_transform(&mut param, item);

        output.bypass_change_detection().transform = transform;
        output.map_unchanged(|trns| &mut trns.size).set_if_neq(size);
    }
}

#[derive(Component, Copy, Clone)]
#[require(Camera2d)]
pub struct Camera2dRoot {
    pub scale: f32,
    pub offset: f32,
}

impl Default for Camera2dRoot {
    #[inline]
    fn default() -> Self {
        Self {
            scale: 1.,
            offset: -100.,
        }
    }
}

impl UiRoot for Camera2dRoot {
    type Param = ();
    type Item = (Read<Camera>, Read<OrthographicProjection>, Has<UiUnrounded>);

    #[inline]
    fn compute_root_transform(
        &mut self,
        _: &mut SystemParamItem<Self::Param>,
        (camera, projection, is_unrounded): QueryItem<Self::Item>,
    ) -> (Transform, Vec2) {
        let area = projection.area;
        let size = (camera.physical_viewport_size().unwrap_or_default().as_vec2() / self.scale)
            .map(|value| if !is_unrounded { value.round() } else { value });

        (
            Transform {
                // UI transforms originate on bottom-left instead of center. This simplifies projecting points in space to
                // get the box vertices for UI nodes.
                translation: area.min.extend(self.offset),
                rotation: Quat::IDENTITY,
                // UI nodes assume the physical viewport size as available space, so scale them back to logical size in order
                // to fit in the camera projection.
                scale: (area.size() / size).extend(1.),
            },
            size,
        )
    }
}
