use std::marker::PhantomData;

use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::{
    prelude::*,
    query::{QueryData, QueryItem},
    system::{StaticSystemParam, SystemParam, SystemParamItem},
};
use bevy_math::{prelude::*, Affine3A};

use crate::{gui::GuiRootTransform, HephaeGuiSystems};

pub trait GuiRoot: Component {
    type Param: SystemParam;
    type Item: QueryData;

    fn calculate(param: &mut SystemParamItem<Self::Param>, item: QueryItem<Self::Item>) -> (Vec2, Affine3A);
}

pub(crate) fn calculate_root<T: GuiRoot>(
    param: StaticSystemParam<T::Param>,
    mut query: Query<(&mut GuiRootTransform, T::Item), With<T>>,
) {
    let param = &mut param.into_inner();
    for (mut size, item) in &mut query {
        let (available_space, transform) = T::calculate(param, item);
        size.set_if_neq(GuiRootTransform {
            available_space,
            transform,
        });
    }
}

pub struct GuiRootPlugin<T: GuiRoot>(PhantomData<fn() -> T>);
impl<T: GuiRoot> GuiRootPlugin<T> {
    #[inline]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: GuiRoot> Default for GuiRootPlugin<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: GuiRoot> Clone for GuiRootPlugin<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: GuiRoot> Copy for GuiRootPlugin<T> {}

impl<T: GuiRoot> Plugin for GuiRootPlugin<T> {
    fn build(&self, app: &mut App) {
        app.register_required_components::<T, GuiRootTransform>()
            .add_systems(PostUpdate, calculate_root::<T>.in_set(HephaeGuiSystems::CalculateRoot));
    }
}
