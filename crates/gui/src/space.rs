use std::marker::PhantomData;

use bevy_app::prelude::*;
use bevy_ecs::{
    component::ComponentId,
    prelude::*,
    query::{QueryData, QueryItem},
    system::{StaticSystemParam, SystemParam, SystemParamItem},
    world::FilteredEntityRef,
};
use bevy_hierarchy::prelude::*;
use bevy_math::{prelude::*, vec2, Affine2, Affine3A};
use bevy_transform::prelude::GlobalTransform;
use fixedbitset::FixedBitSet;

use crate::{
    gui::{DistributedSpace, Gui, GuiDepth, GuiRootTransform},
    HephaeGuiSystems,
};

pub trait GuiRoot: Component {
    type Param: SystemParam;
    type Item: QueryData;

    fn calculate(param: &mut SystemParamItem<Self::Param>, item: QueryItem<Self::Item>) -> (Vec2, Affine3A);
}

#[derive(Resource, Default)]
pub struct GuiRoots(Vec<ComponentId>);
impl GuiRoots {
    #[inline]
    pub fn register<T: GuiRoot>(&mut self, world: &mut World) {
        self.0.push(world.register_component::<T>())
    }
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

pub(crate) fn validate_root(
    world: &mut World,
    test_query: &mut QueryState<(Entity, Option<&Parent>), With<GuiRootTransform>>,
    has_gui_query: &mut QueryState<(), With<Gui>>,
    mut root_query: Local<QueryState<FilteredEntityRef>>,
    mut to_remove: Local<Vec<Entity>>,
) {
    world.resource_scope(|world, roots: Mut<GuiRoots>| {
        if roots.is_changed() {
            *root_query = QueryBuilder::new(world)
                .or(|builder| {
                    for &id in &roots.0 {
                        builder.with_id(id);
                    }
                })
                .build()
        }
    });

    has_gui_query.update_archetypes(world);
    root_query.update_archetypes(world);

    for (e, parent) in test_query.iter(world) {
        if parent.and_then(|e| has_gui_query.get_manual(world, e.get()).ok()).is_some() {
            to_remove.push(e);
            continue
        }

        if root_query.get_manual(world, e).is_err() {
            to_remove.push(e);
            continue
        }
    }

    world.resource_scope(|world, roots: Mut<GuiRoots>| {
        let root_id = world.register_component::<GuiRootTransform>();
        for e in to_remove.drain(..) {
            let mut e = world.entity_mut(e);
            e.remove_by_id(root_id);

            for &id in &roots.0 {
                e.remove_by_id(id);
            }
        }
    });
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
            .add_systems(PostUpdate, calculate_root::<T>.in_set(HephaeGuiSystems::CalculateRoot))
            .world_mut()
            .resource_scope(|world, mut roots: Mut<GuiRoots>| roots.register::<T>(world))
    }
}

pub(crate) fn calculate_corners(
    root_query: Query<(Entity, Ref<GlobalTransform>, Ref<GuiRootTransform>)>,
    children_query: Query<&Children>,
    mut gui_query: Query<(&mut Gui, Ref<DistributedSpace>)>,
    mut gui_depth_query: Query<&mut GuiDepth>,
    mut orphaned: RemovedComponents<Parent>,
    mut orphaned_entities: Local<FixedBitSet>,
    mut depth_list: Local<Vec<(usize, Vec<(usize, Entity)>)>>,
) {
    orphaned_entities.clear();
    orphaned_entities.extend(orphaned.read().map(|e| e.index() as usize));

    fn propagate(
        node: Entity,
        global_trns: Affine3A,
        mut parent_trns: Affine2,
        children_query: &Query<&Children>,
        gui_query: &mut Query<(&mut Gui, Ref<DistributedSpace>)>,
        mut changed: bool,
        current_depth: usize,
        depth_list: &mut Vec<(usize, Entity)>,
    ) -> usize {
        let Ok((mut gui, space)) = gui_query.get_mut(node) else {
            return current_depth - 1
        };

        depth_list.push((current_depth, node));
        changed |= gui.is_added() || space.is_changed();
        parent_trns *= space.transform;

        if changed {
            gui.set_if_neq(Gui::from_transform(
                global_trns,
                parent_trns,
                Vec2::ZERO,
                vec2(space.size.x, 0.),
                space.size,
                vec2(0., space.size.y),
            ));
        }

        let mut max_depth = current_depth;
        if let Ok(children) = children_query.get(node) {
            for &child in children {
                max_depth = max_depth.max(propagate(
                    child,
                    global_trns,
                    parent_trns,
                    children_query,
                    gui_query,
                    changed,
                    current_depth + 1,
                    depth_list,
                ));
            }
        }

        max_depth
    }

    let mut depth_index = Some(0);
    for (root, trns, root_trns) in &root_query {
        let (gui, space) = gui_query.get_mut(root).unwrap();
        let changed = space.is_changed() ||
            trns.is_changed() ||
            root_trns.is_changed() ||
            gui.is_added() ||
            orphaned_entities.contains(root.index() as usize);

        let global_trns = trns.affine() * root_trns.transform;

        let (mut total, mut list) = (&mut 0, &mut Vec::new());
        if let Some((total_src, list_src)) = depth_index.and_then(|i| depth_list.get_mut(i)) {
            depth_index = Some(depth_index.unwrap() + 1);
            total = total_src;
            list = list_src;
        } else {
            depth_index = None;
        }

        *total = propagate(
            root,
            global_trns,
            space.transform,
            &children_query,
            &mut gui_query,
            changed,
            0,
            list,
        );

        if let None = depth_index {
            let push = (*total, std::mem::replace(list, Vec::new()));
            depth_list.push(push);
        }
    }

    for (total, ref mut list) in &mut depth_list {
        for (depth, e) in list.drain(..) {
            gui_depth_query.get_mut(e).unwrap().set_if_neq(GuiDepth {
                depth,
                total_depth: *total,
            });
        }

        *total = 0;
    }
}
