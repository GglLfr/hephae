use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::ComponentId,
    prelude::*,
    system::{StaticSystemParam, SystemBuffer, SystemMeta, SystemState},
    world::{unsafe_world_cell::UnsafeWorldCell, FilteredEntityRef},
};
use bevy_hierarchy::prelude::*;
use bevy_math::{prelude::*, vec2, Affine2, Affine3A};
use bevy_transform::components::GlobalTransform;
use fixedbitset::FixedBitSet;
use nonmax::NonMaxUsize;

use crate::gui::{
    ChangedQuery, DistributeSpaceSys, DistributedSpace, Gui, GuiDepth, GuiLayouts, GuiRoot, GuiRootTransform, GuiRoots,
    InitialLayoutSize, InitialLayoutSizeSys, LayoutCache,
};

#[derive(Default, Deref, DerefMut)]
pub(crate) struct InvalidCaches(Vec<Entity>);
impl SystemBuffer for InvalidCaches {
    #[inline]
    fn apply(&mut self, _: &SystemMeta, world: &mut World) {
        let id = world.register_component::<LayoutCache>();
        for e in self.drain(..) {
            if let Ok(mut e) = world.get_entity_mut(e) {
                e.remove_by_id(id);
            }
        }
    }
}

pub(crate) fn calculate_root<T: GuiRoot>(
    param: StaticSystemParam<T::Param>,
    mut query: Query<(&mut GuiRootTransform, T::Item), With<T>>,
) {
    let param = &mut param.into_inner();
    for (mut size, item) in &mut query {
        size.set_if_neq(T::calculate(param, item));
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

pub(crate) fn propagate_layout(
    world: &mut World,
    (mut layout_ids, mut changed_queries, mut contains_query, mut initial_layout_size, mut distribute_space, caches_query): (
        Local<Vec<ComponentId>>,
        Local<Vec<Box<dyn ChangedQuery>>>,
        Local<QueryState<FilteredEntityRef>>,
        Local<Vec<Box<dyn InitialLayoutSizeSys>>>,
        Local<Vec<Box<dyn DistributeSpaceSys>>>,
        &mut QueryState<&mut LayoutCache>,
    ),
    (mut root_changed, mut root_iterated, root_gui_query, has_gui_query, ancestor_query): (
        Local<Vec<Entity>>,
        Local<FixedBitSet>,
        &mut QueryState<Entity, With<GuiRootTransform>>,
        &mut QueryState<(), With<Gui>>,
        &mut QueryState<&Parent>,
    ),
    initial_size_state: &mut SystemState<(
        Deferred<InvalidCaches>,
        Query<(Option<&mut LayoutCache>, &mut InitialLayoutSize)>,
        Query<(Entity, &Parent), With<Gui>>,
        Query<&Children>,
    )>,
    (mut initial_stack, mut initial_size_stack): (Local<Vec<Entity>>, Local<Vec<Vec2>>),
    distribute_space_state: &mut SystemState<(
        Query<&mut DistributedSpace>,
        Query<&GuiRootTransform>,
        Query<&LayoutCache>,
        Query<(Entity, &InitialLayoutSize)>,
        Query<&Children>,
    )>,
    (mut distribute_stack, mut distribute_output_stack): (Local<Vec<Entity>>, Local<Vec<(Affine2, Vec2)>>),
) {
    let mut all_changed = false;
    world.resource_scope(|world, layouts: Mut<GuiLayouts>| {
        if layouts.is_changed() {
            let (layout_ids_sys, changed_queries_sys, contains_query_sys, initial_layout_size_sys, distribute_space_sys) =
                layouts.initial_layout_size_param(world);
            *layout_ids = layout_ids_sys;
            *changed_queries = changed_queries_sys;
            *contains_query = contains_query_sys;
            *initial_layout_size = initial_layout_size_sys;
            *distribute_space = distribute_space_sys;

            all_changed = true;
            for mut cache in caches_query.iter_mut(world) {
                **cache = None;
            }
        }
    });

    has_gui_query.update_archetypes(world);
    ancestor_query.update_archetypes(world);

    if all_changed {
        root_changed.extend(root_gui_query.iter(world));
    } else {
        root_iterated.clear();
        changed_queries.iter_mut().for_each(|query| {
            query.for_each(world, &mut |mut e| {
                root_iterated.grow((e.index() + 1) as usize);
                if root_iterated.put(e.index() as usize) {
                    return
                }

                root_changed.push(loop {
                    let Ok(parent) = ancestor_query.get_manual(world, e) else {
                        break e
                    };

                    if has_gui_query.get_manual(world, parent.get()).is_ok() {
                        e = parent.get()
                    } else {
                        break e
                    }
                });
            })
        });
    }

    if root_changed.is_empty() {
        return
    }

    // Phase 1: Calculate initial layout size, children-to-parent.
    unsafe fn propagate_initial_size(
        invalid_caches: &mut InvalidCaches,
        world: UnsafeWorldCell,
        node: Entity,
        layout_ids: &[ComponentId],
        initial_layout_size: &mut [Box<dyn InitialLayoutSizeSys>],
        contains_query: &mut QueryState<FilteredEntityRef>,
        layout_query: &mut Query<(Option<&mut LayoutCache>, &mut InitialLayoutSize)>,
        parent_query: &Query<(Entity, &Parent), With<Gui>>,
        children_query: &Query<&Children>,
        children_stack: &mut Vec<Entity>,
        children_size_stack: &mut Vec<Vec2>,
    ) -> Vec2 {
        let children_offset = children_stack.len();
        if let Ok(children) = children_query.get(node) {
            for (child, actual_parent) in parent_query.iter_many(children) {
                assert_eq!(
                    actual_parent.get(), node,
                    "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
                );

                let size = propagate_initial_size(
                    invalid_caches,
                    world,
                    child,
                    layout_ids,
                    initial_layout_size,
                    contains_query,
                    layout_query,
                    parent_query,
                    children_query,
                    children_stack,
                    children_size_stack,
                );

                children_stack.push(child);
                children_size_stack.push(size);
            }
        }

        let (cache, mut size) = layout_query.get_mut(node).unwrap();
        let id = cache.and_then(|mut cache| match **cache {
            Some(id) => Some(id.get()),
            // Safety: This code is the only one having exclusive access to the components.
            None => match contains_query.get_unchecked(world, node) {
                Ok(layouts) => {
                    for (i, &id) in layout_ids.iter().enumerate() {
                        if layouts.contains_id(id) {
                            **cache = Some(NonMaxUsize::new(i).unwrap());
                            break
                        }
                    }

                    match **cache {
                        Some(id) => Some(id.get()),
                        None => {
                            invalid_caches.push(node);
                            None
                        }
                    }
                }
                Err(..) => {
                    invalid_caches.push(node);
                    None
                }
            },
        });

        let result = if let Some(id) = id {
            // Safety: See below.
            initial_layout_size[id].execute(
                node,
                &children_stack[children_offset..],
                &children_size_stack[children_offset..],
                world,
            )
        } else {
            Vec2::ZERO
        };

        children_stack.truncate(children_offset);
        children_size_stack.truncate(children_offset);

        **size = result;
        result
    }

    let cell = world.as_unsafe_world_cell();

    contains_query.update_archetypes_unsafe_world_cell(cell);
    for sys in &mut initial_layout_size {
        sys.update_archetypes(cell);
    }

    // Safety:
    // - `GuiLayout::PreferredParam` and `GuiLayout::PreferredItem` aren't accessed mutably. This is
    //   enforced by the read-only constrain in `GuiLayout`'s associated type definitions.
    // - We only ever write to `GuiCache` and `PreferredLayoutSize`, which is guaranteed to be unique
    //   because of the type's restricted visibility.
    initial_size_state.update_archetypes_unsafe_world_cell(cell);
    let (mut invalid_caches, mut layout_query, parent_query, children_query) =
        unsafe { initial_size_state.get_unchecked_manual(cell) };

    for &root in &root_changed {
        unsafe {
            propagate_initial_size(
                &mut invalid_caches,
                cell,
                root,
                &layout_ids,
                &mut initial_layout_size,
                &mut contains_query,
                &mut layout_query,
                &parent_query,
                &children_query,
                &mut initial_stack,
                &mut initial_size_stack,
            );
        }
    }

    initial_size_state.apply(world);

    // Phase 2: Calculate and distribute space, parent-to-children.
    unsafe fn propagate_distribute_space(
        world: UnsafeWorldCell,
        available_space: Vec2,
        node: Entity,
        distributed_space_query: &mut Query<&mut DistributedSpace>,
        cache_query: &Query<&LayoutCache>,
        initial_layout_size_query: &Query<(Entity, &InitialLayoutSize)>,
        children_query: &Query<&Children>,
        distribute_space: &mut [Box<dyn DistributeSpaceSys>],
        children_stack: &mut Vec<Entity>,
        children_output_stack: &mut Vec<(Affine2, Vec2)>,
    ) {
        let from = children_stack.len();
        if let Ok(children) = children_query.get(node) {
            // Hierarchy has been validated in `propagate_initial_size`, so don't bother wasting time.
            for (child, &initial_layout_size) in initial_layout_size_query.iter_many(children) {
                children_stack.push(child);
                children_output_stack.push((Affine2::IDENTITY, *initial_layout_size));
            }
        }
        let to = children_stack.len();

        // Similarly, `LayoutCache` has just been updated.
        if let Some(distribute_space) = cache_query
            .get(node)
            .ok()
            .and_then(|&cache| cache.and_then(|id| distribute_space.get_mut(id.get())))
        {
            // Safety: See below.
            distribute_space.execute(
                available_space,
                node,
                &children_stack[from..to],
                &mut children_output_stack[from..to],
                world,
            )
        }

        for i in from..to {
            let child = children_stack[i];
            let (transform, size) = children_output_stack[i];

            distributed_space_query
                .get_mut(child)
                .unwrap()
                .set_if_neq(DistributedSpace { transform, size });

            propagate_distribute_space(
                world,
                size,
                child,
                distributed_space_query,
                cache_query,
                initial_layout_size_query,
                children_query,
                distribute_space,
                children_stack,
                children_output_stack,
            );
        }

        children_stack.truncate(from);
        children_output_stack.truncate(from);
    }

    let cell = world.as_unsafe_world_cell();
    for sys in &mut distribute_space {
        sys.update_archetypes(cell);
    }

    // Safety:
    // - `GuiLayout::DistributeParam` and `GuiLayout::DistributeItem` aren't accessed mutably. This is
    //   enforced by the read-only constrain in `GuiLayout`'s associated type definitions.
    // - We only ever write to `DistributedSpace`, which is guaranteed to be unique because of the
    //   type's restricted visibility.
    // - The deferred effects of `InvalidCaches` don't invalidate the hierarchy tree, since there is no
    //   way an end-user could somehow transform the tree based on removals of the `LayoutCache`
    //   component.
    distribute_space_state.update_archetypes_unsafe_world_cell(cell);
    let (mut distributed_space_query, root_query, cache_query, initial_layout_size_query, children_query) =
        unsafe { distribute_space_state.get_unchecked_manual(cell) };

    for root in root_changed.drain(..) {
        let available_space = match root_query.get(root) {
            Ok(&root_transform) => root_transform.available_space,
            Err(..) => Vec2::ZERO,
        };

        distributed_space_query.get_mut(root).unwrap().set_if_neq(DistributedSpace {
            transform: Affine2::IDENTITY,
            size: available_space,
        });

        unsafe {
            propagate_distribute_space(
                cell,
                available_space,
                root,
                &mut distributed_space_query,
                &cache_query,
                &initial_layout_size_query,
                &children_query,
                &mut distribute_space,
                &mut distribute_stack,
                &mut distribute_output_stack,
            )
        }
    }

    for sys in &mut initial_layout_size {
        sys.apply(world);
    }

    for sys in &mut distribute_space {
        sys.apply(world);
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

        if depth_index.is_none() {
            let push = (*total, std::mem::take(list));
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
