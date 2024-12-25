use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{lifetimeless::Read, SystemParamItem},
};
use bevy_math::{prelude::*, Affine3A};
use bevy_render::prelude::*;

use crate::space::GuiRoot;

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
    ) -> (Vec2, Affine3A) {
        (
            area.size(),
            Affine3A::from_translation((area.size() * -viewport_origin).extend(near + f32::EPSILON)),
        )
    }
}
