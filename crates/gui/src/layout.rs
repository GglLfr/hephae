use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::ComponentId,
    prelude::*,
    system::{SystemBuffer, SystemMeta, SystemState},
    world::{unsafe_world_cell::UnsafeWorldCell, FilteredEntityRef},
};
use bevy_hierarchy::prelude::*;
use bevy_math::{prelude::*, Affine2};
use fixedbitset::FixedBitSet;
use nonmax::NonMaxUsize;

use crate::gui::{
    ChangedQuery, DistributeSpaceSys, DistributedSpace, Gui, GuiLayouts, GuiRootTransform, InitialLayoutSize,
    InitialLayoutSizeSys, LayoutCache, PreferredSize,
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
        Query<&PreferredSize>,
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
        preferred_size_query: &Query<&PreferredSize>,
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
                    preferred_size_query,
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
            preferred_size_query.get(node).unwrap().0
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
    let (mut invalid_caches, mut layout_query, parent_query, children_query, preferred_size_query) =
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
                &preferred_size_query,
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
                &cache_query,
                &initial_layout_size_query,
                &children_query,
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
