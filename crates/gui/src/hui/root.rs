use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{lifetimeless::Read, SystemParamItem},
};
use bevy_math::{prelude::*, Affine3A};
use bevy_render::prelude::*;

use crate::root::GuiRoot;

#[derive(Component, Copy, Clone)]
#[require(Camera)]
pub struct FromCamera2d;

impl GuiRoot for FromCamera2d {
    type Param = ();
    type Item = Read<Camera>;

    #[inline]
    fn calculate(_: &mut SystemParamItem<Self::Param>, camera: QueryItem<Self::Item>) -> (Vec2, Affine3A) {
        (camera.logical_target_size().unwrap_or(Vec2::ZERO), Affine3A::IDENTITY)
    }
}
