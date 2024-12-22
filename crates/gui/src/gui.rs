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
use bevy_hierarchy::{Children, Parent};
use bevy_math::Vec2;
use bevy_transform::components::Transform;
use nonmax::NonMaxUsize;

#[derive(Component, Copy, Clone, Default)]
#[require(Transform, PreferredSize, PreferredLayoutSize)]
pub struct Gui {}

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
pub struct PreferredSize(pub Vec2);

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
pub(crate) struct LayoutCache(Option<NonMaxUsize>);

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
pub(crate) struct PreferredLayoutSize(pub Vec2);

pub trait GuiLayout: Component {
    type Changed: QueryFilter;

    type PreferredParam: ReadOnlySystemParam;
    type PreferredItem: ReadOnlyQueryData;

    fn preferred_layout_size(
        param: &SystemParamItem<Self::PreferredParam>,
        parent: (Entity, QueryItem<Self::PreferredItem>),
        children: &[Entity],
        children_layout_sizes: &[Vec2],
    ) -> Vec2;
}

pub(crate) unsafe trait PreferredLayoutSizeSys: Send {
    fn update_archetypes(&mut self, world: UnsafeWorldCell);

    fn apply(&mut self, world: &mut World);

    /// # Safety
    /// - `world` must be the same [`World`] that's passed to [`Self::update_archetypes`].
    /// - Within the entire span of this function call **no** write accesses to
    ///   [`GuiLayout::PreferredParam`] nor [`GuiLayout::PreferredItem`].
    unsafe fn execute(
        &mut self,
        entity: Entity,
        children: &[Entity],
        children_layout_sizes: &[Vec2],
        world: UnsafeWorldCell,
    ) -> Vec2;
}

unsafe impl<'w, 's, T: GuiLayout> PreferredLayoutSizeSys
    for (
        SystemState<(StaticSystemParam<'w, 's, T::PreferredParam>, Query<'w, 's, T::PreferredItem>)>,
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
            "{}::PreferredItem must *always* match the GUI entities. A common escape hatch is using `Option<T>::unwrap_or_default()`",
            type_name::<T>()
        ));

        T::preferred_layout_size(&param, (parent, item), children, children_layout_sizes)
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
            preferred_layout_size: |world| Box::new((SystemState::new(world), PhantomData::<T>)),
        })
    }

    pub(crate) fn preferred_layout_size_param(
        &self,
        world: &mut World,
    ) -> (
        Vec<ComponentId>,
        QueryState<Entity>,
        QueryState<FilteredEntityRef<'static>>,
        Vec<Box<dyn PreferredLayoutSizeSys>>,
    ) {
        let len = self.0.len();
        let (layout_ids, preferred_layout_sizes) = self.0.iter().fold(
            (Vec::with_capacity(len), Vec::with_capacity(len)),
            |(mut layout_ids, mut preferred_layout_sizes), data| {
                layout_ids.push(data.id);
                preferred_layout_sizes.push((data.preferred_layout_size)(world));

                (layout_ids, preferred_layout_sizes)
            },
        );

        let changed_query = QueryBuilder::new(world)
            .or(|builder| {
                builder
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

        (layout_ids, changed_query, contains_query, preferred_layout_sizes)
    }
}

struct GuiLayoutData {
    id: ComponentId,
    changed: fn(&mut QueryBuilder),
    preferred_layout_size: fn(&mut World) -> Box<dyn PreferredLayoutSizeSys>,
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
