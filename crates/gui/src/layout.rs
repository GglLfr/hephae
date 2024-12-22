use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::ComponentId,
    prelude::*,
    system::{SystemBuffer, SystemMeta, SystemState},
    world::{unsafe_world_cell::UnsafeWorldCell, FilteredEntityRef},
};
use bevy_hierarchy::prelude::*;
use bevy_math::prelude::*;
use fixedbitset::FixedBitSet;
use nonmax::NonMaxUsize;

use crate::gui::{Gui, GuiLayouts, LayoutCache, PreferredLayoutSize, PreferredLayoutSizeSys, PreferredSize};

#[derive(Default, Deref, DerefMut)]
pub(crate) struct InvalidCaches(Vec<Entity>);
impl SystemBuffer for InvalidCaches {
    fn apply(&mut self, _: &SystemMeta, world: &mut World) {
        let id = world.register_component::<LayoutCache>();
        for e in self.drain(..) {
            if let Ok(mut e) = world.get_entity_mut(e) {
                e.remove_by_id(id);
            }
        }
    }
}

pub(crate) fn calculate_preferred_layout_size(
    world: &mut World,
    (mut layout_ids, mut changed_query, mut contains_query, mut preferred_layout_size, caches_query): (
        Local<Vec<ComponentId>>,
        Local<QueryState<Entity>>,
        Local<QueryState<FilteredEntityRef>>,
        Local<Vec<Box<dyn PreferredLayoutSizeSys>>>,
        &mut QueryState<&mut LayoutCache>,
    ),
    (mut root_changed, mut root_iterated, root_gui_query, has_gui_query, ancestor_query): (
        Local<Vec<Entity>>,
        Local<FixedBitSet>,
        &mut QueryState<Entity, (With<Gui>, Without<Parent>)>,
        &mut QueryState<(), With<Gui>>,
        &mut QueryState<&Parent>,
    ),
    propagate_state: &mut SystemState<(
        Deferred<InvalidCaches>,
        Query<(Option<&mut LayoutCache>, &mut PreferredLayoutSize)>,
        Query<(Entity, &Parent), With<Gui>>,
        Query<&Children>,
        Query<&PreferredSize>,
    )>,
    (mut children_stack, mut children_size_stack): (Local<Vec<Entity>>, Local<Vec<Vec2>>),
) {
    let mut all_changed = false;
    world.resource_scope(|world, layouts: Mut<GuiLayouts>| {
        if layouts.is_changed() {
            let (layout_ids_sys, changed_query_sys, contains_query_sys, preferred_layout_size_sys) =
                layouts.preferred_layout_size_param(world);
            *layout_ids = layout_ids_sys;
            *changed_query = changed_query_sys;
            *contains_query = contains_query_sys;
            *preferred_layout_size = preferred_layout_size_sys;

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

        'node: for mut e in changed_query.iter(world) {
            root_iterated.grow((e.index() + 1) as usize);
            if root_iterated.put(e.index() as usize) {
                continue 'node
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
        }
    }

    if root_changed.is_empty() {
        return
    }

    unsafe fn propagate(
        invalid_caches: &mut InvalidCaches,
        world: UnsafeWorldCell,
        node: Entity,
        layout_ids: &[ComponentId],
        preferred_layout_size: &mut [Box<dyn PreferredLayoutSizeSys>],
        contains_query: &mut QueryState<FilteredEntityRef>,
        layout_query: &mut Query<(Option<&mut LayoutCache>, &mut PreferredLayoutSize)>,
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

                let size = propagate(
                    invalid_caches,
                    world,
                    child,
                    layout_ids,
                    preferred_layout_size,
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
            preferred_layout_size[id].execute(
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
    for sys in &mut preferred_layout_size {
        sys.update_archetypes(cell);
    }

    // Safety:
    // - `GuiLayout::PreferredParam` and `GuiLayout::PreferredItem` aren't accessed mutably. This is
    //   enforced by the read-only constrain in `GuiLayout`'s associated type definitions.
    // - We only ever write to `GuiCache` and `PreferredLayoutSize`, which is guaranteed to be unique
    //   because of the type's restricted visibility.
    propagate_state.update_archetypes_unsafe_world_cell(cell);
    let (mut invalid_caches, mut layout_query, parent_query, children_query, preferred_size_query) =
        unsafe { propagate_state.get_unchecked_manual(cell) };

    for root in root_changed.drain(..) {
        unsafe {
            propagate(
                &mut invalid_caches,
                cell,
                root,
                &layout_ids,
                &mut preferred_layout_size,
                &mut contains_query,
                &mut layout_query,
                &parent_query,
                &children_query,
                &preferred_size_query,
                &mut children_stack,
                &mut children_size_stack,
            );
        }
    }

    propagate_state.apply(world);
    for sys in &mut preferred_layout_size {
        sys.apply(world);
    }
}
