//! Defines components and plugins required to create GUI layout and root components.

use std::{any::type_name, marker::PhantomData};

use bevy_app::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::ComponentId,
    prelude::*,
    query::{QueryData, QueryFilter, QueryItem, ReadOnlyQueryData},
    system::{ReadOnlySystemParam, StaticSystemParam, SystemParam, SystemParamItem, SystemState},
    world::{unsafe_world_cell::UnsafeWorldCell, DeferredWorld, FilteredEntityRef},
};
use bevy_hierarchy::prelude::*;
use bevy_math::{prelude::*, Affine2, Affine3A, Vec3A};
use bevy_transform::components::Transform;
use nonmax::NonMaxUsize;

use crate::{layout::calculate_root, HephaeGuiSystems};

/// The heart of Hephae GUI. All GUI entities must have this component (which is usually done
/// automatically by required components).
///
/// Values stored in this component are the four rectangle corners projected into 3D world-space,
/// ready to be rendered as-is. They're calculated in
/// [HephaeGuiSystems::CalculateCorners].
#[derive(Component, Copy, Clone, PartialEq, Default)]
#[require(Transform, GuiDepth, InitialLayoutSize, DistributedSpace)]
pub struct Gui {
    /// The bottom-left corner of this GUI entity, in world-space.
    pub bottom_left: Vec3,
    /// The bottom-right corner of this GUI entity, in world-space.
    pub bottom_right: Vec3,
    /// The top-right corner of this GUI entity, in world-space.
    pub top_right: Vec3,
    /// The top-left corner of this GUI entity, in world-space.
    pub top_left: Vec3,
}

impl Gui {
    /// Computes corners based on global 3D transform, local 2D transform, and local 2D corners.
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

/// Stores the depth of this GUI entity node in the GUI tree.
#[derive(Component, Copy, Clone, Default, PartialEq)]
pub struct GuiDepth {
    /// The depth of the node, based on its position in the hierarchy. Root nodes are always `0`.
    pub depth: usize,
    /// The total depth of the tree, i.e., the value of `depth` stored by the deepest leaf nodes.
    pub total_depth: usize,
}

/// GUI root node transform computation results from [`GuiRoot::calculate`]. Contains the local
/// transform relative to its parent, e.g., inverse of the camera's viewport origin.
#[derive(Component, Copy, Clone, Default, PartialEq, Deref, DerefMut)]
#[require(Gui)]
pub struct GuiRootTransform(pub Affine3A);

/// GUI root node available space computation results from [`GuiRoot::calculate`]. Contains the
/// available space for its children, e.g., area of the camera viewport.
#[derive(Component, Copy, Clone, Default, PartialEq, Deref, DerefMut)]
#[require(Gui)]
pub struct GuiRootSpace(pub Vec2);

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

/// The main component that handles the affine transform (which includes offset, scale, and
/// rotation) and size for its direct children.
pub trait GuiLayout: Component {
    /// Query filter to observe changes that may affect this GUI layout. Should be
    /// [`Or<Changed<T>>`].
    type Changed: QueryFilter;

    /// System param to fetch for [`Self::initial_layout_size`].
    type InitialParam: ReadOnlySystemParam;
    /// Query item to fetch for [`Self::initial_layout_size`]. **Must** match the GUI entity with
    /// this layout component, otherwise a panic will occur.
    type InitialItem: ReadOnlyQueryData;

    /// System param to fetch for [`Self::distribute_space`].
    type DistributeParam: ReadOnlySystemParam;
    /// Query item to fetch for [`Self::distribute_space`]. **Must** match the GUI entity with
    /// this layout component, otherwise a panic will occur.
    type DistributeItem: ReadOnlyQueryData;

    /// Computes the initial layout size of each nodes, based on their children. In most cases, this
    /// should be "minimum size to fit all children".
    fn initial_layout_size(
        param: &SystemParamItem<Self::InitialParam>,
        parent: QueryItem<Self::InitialItem>,
        children: &[Entity],
        children_layout_sizes: &[Vec2],
    ) -> Vec2;

    /// Distributes the actual available size for each children node, based on their parent. Each
    /// `children[i]` is associated with `output[i]`, where initially `output[i].1` is the size
    /// calculated from [`Self::initial_layout_size`].
    fn distribute_space(
        this: (&mut Affine2, &mut Vec2),
        param: &SystemParamItem<Self::DistributeParam>,
        parent: QueryItem<Self::DistributeItem>,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
    );
}

/// # Safety
/// In [`Self::execute`], implementors may only have read accesses to [`GuiLayout::InitialParam`]
/// and [`GuiLayout::InitialItem`].
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

/// # Safety
/// In [`Self::execute`], implementors may only have read accesses to [`GuiLayout::DistributeParam`]
/// and [`GuiLayout::DistributeItem`].
pub(crate) unsafe trait DistributeSpaceSys: Send {
    fn update_archetypes(&mut self, world: UnsafeWorldCell);

    fn apply(&mut self, world: &mut World);

    /// # Safety
    /// - `world` must be the same [`World`] that's passed to [`Self::update_archetypes`].
    /// - Within the entire span of this function call **no** write accesses to
    ///   [`GuiLayout::DistributeParam`] nor [`GuiLayout::DistributeItem`].
    unsafe fn execute(
        &mut self,
        this: (&mut Affine2, &mut Vec2),
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
        this: (&mut Affine2, &mut Vec2),
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

        T::distribute_space(this, &param, item, children, output)
    }
}

#[derive(Resource, Default)]
pub(crate) struct GuiLayouts(Vec<GuiLayoutData>);
impl GuiLayouts {
    #[inline]
    pub fn register<T: GuiLayout>(&mut self, world: &mut World) {
        self.0.push(GuiLayoutData {
            id: world.register_component::<T>(),
            changed: |world| {
                Box::new(QueryState::<
                    Entity,
                    Or<(
                        Changed<GuiRootSpace>,
                        Changed<Parent>,
                        Changed<Children>,
                        Changed<T>,
                        T::Changed,
                    )>,
                >::new(world))
            },
            initial_layout_size: |world| Box::new((SystemState::new(world), PhantomData::<T>)),
            distribute_space: |world| Box::new((SystemState::new(world), PhantomData::<T>)),
        })
    }

    pub fn initial_layout_size_param(
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

/// Registers `T` GUI layout and all of the systems associates with it to the application.
pub struct GuiLayoutPlugin<T: GuiLayout>(PhantomData<fn() -> T>);
impl<T: GuiLayout> GuiLayoutPlugin<T> {
    /// Creates a new [`GuiLayoutPlugin`].
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

/// A GUI root component, responsible for how its children are recursively projected into the 3D
/// space for the camera to pick up. A GUI root component may not have a parent with a [`Gui`]
/// component.
pub trait GuiRoot: Component {
    /// System param to fetch for [`Self::calculate`]. Must **not** have write access to
    /// [`GuiRootTransform`] and [`GuiRootSpace`], otherwise a panic will arise.
    type Param: SystemParam;
    /// World query to fetch for [`Self::calculate`].
    type Item: QueryData;

    /// Calculates [`GuiRootTransform`] and [`GuiRootSpace`] for each GUI root nodes.
    fn calculate(param: &mut SystemParamItem<Self::Param>, item: QueryItem<Self::Item>) -> (Vec2, Affine3A);
}

#[derive(Resource, Default)]
pub(crate) struct GuiRoots(pub Vec<ComponentId>);
impl GuiRoots {
    #[inline]
    pub fn register<T: GuiRoot>(&mut self, world: &mut World) {
        self.0.push(world.register_component::<T>())
    }
}

/// Registers `T` GUI root and all of the systems associates with it to the application.
pub struct GuiRootPlugin<T: GuiRoot>(PhantomData<fn() -> T>);
impl<T: GuiRoot> GuiRootPlugin<T> {
    /// Creates a new [`GuiRootPlugin`].
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
            .register_required_components::<T, GuiRootSpace>()
            .add_systems(PostUpdate, calculate_root::<T>.in_set(HephaeGuiSystems::CalculateRoot))
            .world_mut()
            .resource_scope(|world, mut roots: Mut<GuiRoots>| roots.register::<T>(world))
    }
}
