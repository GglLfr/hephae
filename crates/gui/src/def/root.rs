use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{lifetimeless::Read, SystemParamItem},
};
use bevy_math::Affine3A;
use bevy_render::prelude::*;

use crate::gui::{GuiRoot, GuiRootTransform};

/// A [`GuiRoot`] implementation that projects the GUI node tree to the viewport of the camera.
#[derive(Component, Copy, Clone)]
#[require(Camera)]
pub struct FromCamera2d;

impl GuiRoot for FromCamera2d {
    type Param = ();
    type Item = Read<OrthographicProjection>;

    #[inline]
    fn calculate(
        _: &mut SystemParamItem<Self::Param>,
        &OrthographicProjection {
            area,
            near,
            viewport_origin,
            ..
        }: QueryItem<Self::Item>,
    ) -> GuiRootTransform {
        GuiRootTransform {
            available_space: area.size(),
            transform: Affine3A::from_translation((area.size() * -viewport_origin).extend(near + f32::EPSILON)),
        }
    }
}
