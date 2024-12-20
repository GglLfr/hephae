use std::{any::type_name, marker::PhantomData};

use bevy_app::prelude::*;
use bevy_ecs::{
    component::RequiredComponentsError,
    prelude::*,
    query::{QueryData, QueryFilter, QueryItem},
    system::{SystemParam, SystemParamItem},
};
use bevy_math::prelude::*;
use bevy_transform::prelude::*;

#[derive(Component, Copy, Clone, Default)]
#[require(Transform, AvailableSize)]
pub struct Gui;

#[derive(Component, Copy, Clone, Default)]
pub(crate) struct AvailableSize(pub Vec2);

pub trait GuiRootSize: Component {
    type Param: SystemParam;
    type Query: QueryData;
    type Filter: QueryFilter;

    fn compute(param: &mut SystemParamItem<Self::Param>, query: &mut QueryItem<Self::Query>) -> Vec2;
}

pub struct GuiRootSizePlugin<T: GuiRootSize>(PhantomData<fn() -> T>);
impl<T: GuiRootSize> GuiRootSizePlugin<T> {
    #[inline]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: GuiRootSize> Default for GuiRootSizePlugin<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: GuiRootSize> Clone for GuiRootSizePlugin<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: GuiRootSize> Copy for GuiRootSizePlugin<T> {}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, SystemSet)]
pub enum HephaeGuiSystems {
    ComputeAvailableSize,
}

impl<T: GuiRootSize> Plugin for GuiRootSizePlugin<T> {
    fn build(&self, app: &mut App) {
        assert!(
            !matches!(
                app.try_register_required_components::<T, Gui>(),
                Err(RequiredComponentsError::ArchetypeExists(..))
            ),
            "entity with component '{}' without a '{}' already exists",
            type_name::<T>(),
            type_name::<Gui>()
        );

        app.configure_sets(
            PostUpdate,
            HephaeGuiSystems::ComputeAvailableSize.after_ignore_deferred(TransformSystem::TransformPropagate),
        );
    }
}

pub trait GuiLayout {
    type Change: QueryFilter;
}
