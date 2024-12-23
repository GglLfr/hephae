use std::{any::type_name, marker::PhantomData};

use bevy_app::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::ComponentId,
    prelude::*,
    query::{QueryFilter, QueryItem, ReadOnlyQueryData},
    system::{ReadOnlySystemParam, StaticSystemParam, SystemParamItem, SystemState},
    world::{unsafe_world_cell::UnsafeWorldCell, DeferredWorld, FilteredEntityRef},
};
use bevy_hierarchy::prelude::*;
use bevy_math::{prelude::*, Affine2, Affine3A};
use bevy_transform::components::Transform;
use nonmax::NonMaxUsize;

#[derive(Component, Copy, Clone, Default)]
#[require(Transform, PreferredSize, InitialLayoutSize)]
pub struct Gui {}

#[derive(Component, Copy, Clone, Default, PartialEq, Deref, DerefMut)]
pub struct PreferredSize(pub Vec2);

#[derive(Component, Copy, Clone, Default, PartialEq)]
#[require(Gui)]
pub(crate) struct GuiRootTransform(pub Vec2, pub Affine3A);

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
pub(crate) struct LayoutCache(Option<NonMaxUsize>);

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
pub(crate) struct InitialLayoutSize(pub Vec2);

pub trait GuiLayout: Component {
    type Changed: QueryFilter;

    type InitialParam: ReadOnlySystemParam;
    type InitialItem: ReadOnlyQueryData;

    type DistributeParam: ReadOnlySystemParam;
    type DistributeItem: ReadOnlyQueryData;

    fn initial_layout_size(
        param: &SystemParamItem<Self::InitialParam>,
        parent: QueryItem<Self::InitialItem>,
        children: &[Entity],
        children_layout_sizes: &[Vec2],
    ) -> Vec2;

    fn distribute_space(
        available_space: Vec2,
        param: &SystemParamItem<Self::DistributeParam>,
        parent: QueryItem<Self::DistributeItem>,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
    );
}

pub(crate) unsafe trait InitialLayoutSizeSys: Send {
    fn update_archetypes(&mut self, world: UnsafeWorldCell);

    fn apply(&mut self, world: &mut World);

    /// # Safety
    /// - `world` must be the same [`World`] that's passed to [`Self::update_archetypes`].
    /// - Within the entire span of this function call **no** write accesses to
    ///   [`GuiLayout::InitialParam`] nor [`GuiLayout::InitialItem`].
    unsafe fn execute(
        &mut self,
        parent: Entity,
        children: &[Entity],
        children_layout_sizes: &[Vec2],
        world: UnsafeWorldCell,
    ) -> Vec2;
}

unsafe impl<'w, 's, T: GuiLayout> InitialLayoutSizeSys
    for (
        SystemState<(StaticSystemParam<'w, 's, T::InitialParam>, Query<'w, 's, T::InitialItem>)>,
        PhantomData<T>,
    )
{
    #[inline]
    fn update_archetypes(&mut self, world: UnsafeWorldCell) {
        self.0.update_archetypes_unsafe_world_cell(world)
    }

    #[inline]
    fn apply(&mut self, world: &mut World) {
        self.0.apply(world)
    }

    #[inline]
    unsafe fn execute(
        &mut self,
        parent: Entity,
        children: &[Entity],
        children_layout_sizes: &[Vec2],
        world: UnsafeWorldCell,
    ) -> Vec2 {
        let (param, mut query) = self.0.get_unchecked_manual(world);
        let item = query.get_mut(parent).unwrap_or_else(|_| panic!(
            "{}::InitialItem must *always* match the GUI entities. A common escape hatch is using `Option<T>::unwrap_or_default()`",
            type_name::<T>()
        ));

        T::initial_layout_size(&param, item, children, children_layout_sizes)
    }
}

pub(crate) unsafe trait DistributeSpaceSys: Send {
    fn update_archetypes(&mut self, world: UnsafeWorldCell);

    fn apply(&mut self, world: &mut World);

    /// # Safety
    /// - `world` must be the same [`World`] that's passed to [`Self::update_archetypes`].
    /// - Within the entire span of this function call **no** write accesses to
    ///   [`GuiLayout::DistributeParam`] nor [`GuiLayout::DistributeItem`].
    unsafe fn execute(
        &mut self,
        available_space: Vec2,
        entity: Entity,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
        world: UnsafeWorldCell,
    );
}

unsafe impl<'w, 's, T: GuiLayout> DistributeSpaceSys
    for (
        SystemState<(
            StaticSystemParam<'w, 's, T::DistributeParam>,
            Query<'w, 's, T::DistributeItem>,
        )>,
        PhantomData<T>,
    )
{
    #[inline]
    fn update_archetypes(&mut self, world: UnsafeWorldCell) {
        self.0.update_archetypes_unsafe_world_cell(world)
    }

    #[inline]
    fn apply(&mut self, world: &mut World) {
        self.0.apply(world)
    }

    #[inline]
    unsafe fn execute(
        &mut self,
        available_space: Vec2,
        parent: Entity,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
        world: UnsafeWorldCell,
    ) {
        let (param, mut query) = self.0.get_unchecked_manual(world);
        let item = query.get_mut(parent).unwrap_or_else(|_| panic!(
            "{}::DistributeItem must *always* match the GUI entities. A common escape hatch is using `Option<T>::unwrap_or_default()`",
            type_name::<T>()
        ));

        T::distribute_space(available_space, &param, item, children, output)
    }
}

#[derive(Resource, Default)]
pub struct GuiLayouts(Vec<GuiLayoutData>);
impl GuiLayouts {
    #[inline]
    pub fn register<T: GuiLayout>(&mut self, world: &mut World) {
        self.0.push(GuiLayoutData {
            id: world.register_component::<T>(),
            changed: |builder| {
                builder.filter::<T::Changed>();
            },
            initial_layout_size: |world| Box::new((SystemState::new(world), PhantomData::<T>)),
            distribute_space: |world| Box::new((SystemState::new(world), PhantomData::<T>)),
        })
    }

    pub(crate) fn initial_layout_size_param(
        &self,
        world: &mut World,
    ) -> (
        Vec<ComponentId>,
        QueryState<Entity>,
        QueryState<FilteredEntityRef<'static>>,
        Vec<Box<dyn InitialLayoutSizeSys>>,
    ) {
        let len = self.0.len();
        let (layout_ids, initial_layout_sizes) = self.0.iter().fold(
            (Vec::with_capacity(len), Vec::with_capacity(len)),
            |(mut layout_ids, mut initial_layout_sizes), data| {
                layout_ids.push(data.id);
                initial_layout_sizes.push((data.initial_layout_size)(world));

                (layout_ids, initial_layout_sizes)
            },
        );

        let changed_query = QueryBuilder::new(world)
            .or(|builder| {
                builder
                    .filter::<Changed<GuiRootTransform>>()
                    .filter::<Changed<PreferredSize>>()
                    .filter::<Changed<Parent>>()
                    .filter::<Changed<Children>>();

                for data in &self.0 {
                    (data.changed)(builder);
                }
            })
            .build();

        let contains_query = QueryBuilder::new(world)
            .or(|builder| {
                for &id in &layout_ids {
                    builder.with_id(id);
                }
            })
            .build();

        (layout_ids, changed_query, contains_query, initial_layout_sizes)
    }
}

struct GuiLayoutData {
    id: ComponentId,
    changed: fn(&mut QueryBuilder),
    initial_layout_size: fn(&mut World) -> Box<dyn InitialLayoutSizeSys>,
    distribute_space: fn(&mut World) -> Box<dyn DistributeSpaceSys>,
}

pub struct GuiLayoutPlugin<T: GuiLayout>(PhantomData<fn() -> T>);
impl<T: GuiLayout> GuiLayoutPlugin<T> {
    #[inline]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: GuiLayout> Default for GuiLayoutPlugin<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: GuiLayout> Clone for GuiLayoutPlugin<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: GuiLayout> Copy for GuiLayoutPlugin<T> {}

impl<T: GuiLayout> Plugin for GuiLayoutPlugin<T> {
    fn build(&self, app: &mut App) {
        fn hook(mut world: DeferredWorld, e: Entity, _: ComponentId) {
            let mut e = world.entity_mut(e);

            // The `unwrap()` never fails here because `T` requires `LayoutCache`.
            let mut cache = e.get_mut::<LayoutCache>().unwrap();
            **cache = None
        }

        app.register_required_components::<T, Gui>()
            .register_required_components::<T, LayoutCache>();

        let world = app.world_mut();
        world.register_component_hooks::<T>().on_add(hook).on_remove(hook);
        world.resource_scope(|world, mut layouts: Mut<GuiLayouts>| layouts.register::<T>(world))
    }
}
