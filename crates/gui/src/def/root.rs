use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{SystemParamItem, lifetimeless::Read},
};
use bevy_math::{Affine3A, Quat, UVec2, Vec2};
use bevy_render::prelude::*;

use crate::gui::GuiRoot;

/// A [`GuiRoot`] implementation that projects the GUI node tree to the viewport of the camera.
#[derive(Component, Copy, Clone)]
#[require(Camera)]
pub struct FromCamera2d;

impl GuiRoot for FromCamera2d {
    type Param = ();
    type Item = (Read<Camera>, Read<OrthographicProjection>);

    #[inline]
    fn calculate(_: &mut SystemParamItem<Self::Param>, (camera, projection): QueryItem<Self::Item>) -> (Vec2, Affine3A) {
        let size = camera.physical_viewport_size().unwrap_or(UVec2::ZERO).as_vec2();
        let size = size * camera.target_scaling_factor().unwrap_or(1.);

        let area = projection.area.size();
        (
            size,
            Affine3A::from_scale_rotation_translation(
                (area / size).extend(1.),
                Quat::IDENTITY,
                (area * -projection.viewport_origin).extend(projection.near),
            ),
        )
    }
}
