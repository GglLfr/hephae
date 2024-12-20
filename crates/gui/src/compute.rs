use bevy_ecs::{prelude::*, system::StaticSystemParam};
use bevy_math::prelude::*;

use crate::def::{AvailableSize, GuiRootSize};

pub(crate) fn compute_available_size<T: GuiRootSize>(
    param: StaticSystemParam<T::Param>,
    mut query: Query<(&mut AvailableSize, T::Query), (T::Filter, With<T>)>,
    mut stale: Query<&mut AvailableSize, Without<T>>,
) {
    let param = &mut param.into_inner();
    for (mut size, mut query) in &mut query {
        size.0 = T::compute(param, &mut query);
    }

    for mut size in &mut stale {
        size.0 = Vec2::ZERO;
    }
}

pub fn compute_transforms(changed: Query<Entity>) {}
