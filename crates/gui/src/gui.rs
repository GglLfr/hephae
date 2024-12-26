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
use bevy_math::{prelude::*, Affine2, Affine3A, Vec3A};
use bevy_transform::components::Transform;
use nonmax::NonMaxUsize;

#[derive(Component, Copy, Clone, PartialEq, Default)]
#[require(Transform, GuiDepth, PreferredSize, InitialLayoutSize, DistributedSpace)]
pub struct Gui {
    pub bottom_left: Vec3,
    pub bottom_right: Vec3,
    pub top_right: Vec3,
    pub top_left: Vec3,
}

impl Gui {
    #[inline]
    pub fn from_transform(
        global_trns: Affine3A,
        local_trns: Affine2,
        bottom_left: Vec2,
        bottom_right: Vec2,
        top_right: Vec2,
        top_left: Vec2,
    ) -> Self {
        let trns = global_trns *
            Affine3A::from_cols(
                local_trns.x_axis.extend(0.).into(),
                local_trns.y_axis.extend(0.).into(),
                Vec3A::Z,
                local_trns.z_axis.extend(0.).into(),
            );

        Self {
            bottom_left: trns.transform_point3(bottom_left.extend(0.)),
            bottom_right: trns.transform_point3(bottom_right.extend(0.)),
            top_right: trns.transform_point3(top_right.extend(0.)),
            top_left: trns.transform_point3(top_left.extend(0.)),
        }
    }
}

#[derive(Component, Copy, Clone, Default, PartialEq)]
pub struct GuiDepth {
    pub depth: usize,
    pub total_depth: usize,
}

#[derive(Component, Copy, Clone, Default, PartialEq, Deref, DerefMut)]
pub struct PreferredSize(pub Vec2);

#[derive(Component, Copy, Clone, Default, PartialEq)]
#[require(Gui)]
pub struct GuiRootTransform {
    pub available_space: Vec2,
    pub transform: Affine3A,
}

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub(crate) struct LayoutCache(Option<NonMaxUsize>);

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
pub(crate) struct InitialLayoutSize(pub Vec2);

#[derive(Component, Copy, Clone, PartialEq, Default)]
pub(crate) struct DistributedSpace {
    pub transform: Affine2,
    pub size: Vec2,
}

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
        parent: Entity,
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
            changed: |world| {
                Box::new(QueryState::<
                    Entity,
                    Or<(
                        Changed<GuiRootTransform>,
                        Changed<PreferredSize>,
                        Changed<Parent>,
                        Changed<Children>,
                        T::Changed,
                    )>,
                >::new(world))
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
        Vec<Box<dyn ChangedQuery>>,
        QueryState<FilteredEntityRef<'static>>,
        Vec<Box<dyn InitialLayoutSizeSys>>,
        Vec<Box<dyn DistributeSpaceSys>>,
    ) {
        let len = self.0.len();
        let (layout_ids, changed_queries, initial_layout_size, distribute_space) = self.0.iter().fold(
            (
                Vec::with_capacity(len),
                Vec::with_capacity(len),
                Vec::with_capacity(len),
                Vec::with_capacity(len),
            ),
            |(mut layout_ids, mut changed_queries, mut initial_layout_sizes, mut distribute_space), data| {
                layout_ids.push(data.id);
                changed_queries.push((data.changed)(world));
                initial_layout_sizes.push((data.initial_layout_size)(world));
                distribute_space.push((data.distribute_space)(world));

                (layout_ids, changed_queries, initial_layout_sizes, distribute_space)
            },
        );

        let contains_query = QueryBuilder::new(world)
            .or(|builder| {
                for &id in &layout_ids {
                    builder.with_id(id);
                }
            })
            .build();

        (
            layout_ids,
            changed_queries,
            contains_query,
            initial_layout_size,
            distribute_space,
        )
    }
}

struct GuiLayoutData {
    id: ComponentId,
    changed: fn(&mut World) -> Box<dyn ChangedQuery>,
    initial_layout_size: fn(&mut World) -> Box<dyn InitialLayoutSizeSys>,
    distribute_space: fn(&mut World) -> Box<dyn DistributeSpaceSys>,
}

pub(crate) trait ChangedQuery: Send {
    fn for_each(&mut self, world: &World, callback: &mut dyn FnMut(Entity));
}

impl<F: QueryFilter> ChangedQuery for QueryState<Entity, F> {
    #[inline]
    fn for_each(&mut self, world: &World, callback: &mut dyn FnMut(Entity)) {
        self.iter(world).for_each(callback)
    }
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

        app.register_required_components::<T, LayoutCache>();

        let world = app.world_mut();
        world.register_component_hooks::<T>().on_add(hook).on_remove(hook);
        world.resource_scope(|world, mut layouts: Mut<GuiLayouts>| layouts.register::<T>(world))
    }
}
