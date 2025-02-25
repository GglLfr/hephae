use std::{iter::Map, ops::IndexMut};

use bevy_ecs::{
    entity::{EntityHashMap, VisitEntitiesMut},
    prelude::*,
    reflect::{ReflectMapEntities, ReflectVisitEntities, ReflectVisitEntitiesMut},
    system::{
        SystemState,
        lifetimeless::{Read, Write},
    },
};
use bevy_math::prelude::*;
use bevy_reflect::prelude::*;
use smallvec::SmallVec;
use taffy::{
    AvailableSpace, Cache, CacheTree, Layout, LayoutBlockContainer, LayoutFlexboxContainer, LayoutInput, LayoutOutput,
    LayoutPartialTree, NodeId, PrintTree, RoundTree, RunMode, Size, TraversePartialTree, TraverseTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_hidden_layout, compute_leaf_layout, compute_root_layout,
};

use crate::{
    measure::{MeasureId, Measurements, Measurer},
    root::UiRootTrns,
    style::{Display, Node},
};

#[derive(Component, Reflect, Clone, VisitEntitiesMut, Default)]
#[reflect(Component, VisitEntities, VisitEntitiesMut, MapEntities, Default)]
pub struct NodeChildren(SmallVec<[Entity; 8]>);
impl<'a> IntoIterator for &'a NodeChildren {
    type Item = &'a Entity;
    type IntoIter = std::slice::Iter<'a, Entity>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

//TODO trigger for parent addition for source of truth

#[derive(Component, Copy, Clone, Default)]
pub struct ComputedNode {
    /// The relative ordering of the node.
    ///
    /// Nodes with a higher order should be rendered on top of those with a lower order.
    /// This is effectively a topological sort of each tree.
    pub order: u32,
    /// The top-left corner of the node.
    pub location: Vec2,
    /// The width and height of the node.
    pub size: Size<f32>,
    /// The width and height of the content inside the node. This may be larger than the size of the
    /// node in the case of overflowing content and is useful for computing a "scroll width/height"
    /// for scrollable nodes.
    pub content_size: Vec2,
    /// The size of the scrollbars in each dimension. If there is no scrollbar then the size will be
    /// zero.
    pub scrollbar_size: Vec2,
    /// The size of the borders of the node.
    pub border: Rect,
    /// The size of the padding of the node.
    pub padding: Rect,
    /// The size of the margin of the node.
    pub margin: Rect,
}

#[derive(Component, Copy, Clone)]
pub struct ContentSize(MeasureId);
impl ContentSize {
    #[inline]
    pub const fn get(self) -> MeasureId {
        self.0
    }
}

impl Default for ContentSize {
    #[inline]
    fn default() -> Self {
        Self(MeasureId::INVALID)
    }
}

#[derive(Component, Default)]
pub(crate) struct NodeCache(Cache);

pub(crate) struct UiTree<'w, 's, M> {
    measurements: M,
    node_query: Query<'w, 's, (Read<Node>, Option<Read<ContentSize>>)>,
    children_query: Query<'w, 's, Read<NodeChildren>>,
    cache_query: Query<'w, 's, Write<NodeCache>>,
    outputs: Local<'s, EntityHashMap<Layout>>,
}

impl<M> TraverseTree for UiTree<'_, '_, M> {}

impl<M> TraversePartialTree for UiTree<'_, '_, M> {
    type ChildIter<'a>
        = Map<std::slice::Iter<'a, Entity>, fn(&'a Entity) -> NodeId>
    where
        Self: 'a;

    #[inline]
    fn child_ids(&self, parent_node_id: NodeId) -> Self::ChildIter<'_> {
        self.children_query
            .get(Entity::from_bits(parent_node_id.into()))
            .unwrap_or(const { &NodeChildren(SmallVec::new_const()) })
            .into_iter()
            .map(|&e| NodeId::from(e.to_bits()))
    }

    #[inline]
    fn child_count(&self, parent_node_id: NodeId) -> usize {
        self.child_ids(parent_node_id).len()
    }

    #[inline]
    fn get_child_id(&self, parent_node_id: NodeId, child_index: usize) -> NodeId {
        self.child_ids(parent_node_id).nth(child_index).unwrap()
    }
}

impl<M: IndexMut<MeasureId, Output = dyn Measurer>> LayoutPartialTree for UiTree<'_, '_, M> {
    type CoreContainerStyle<'a>
        = &'a Node
    where
        Self: 'a;

    #[inline]
    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.node_query.get(Entity::from_bits(node_id.into())).unwrap().0
    }

    #[inline]
    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.outputs.insert(Entity::from_bits(node_id.into()), layout.clone());
    }

    #[inline]
    fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let e = Entity::from_bits(node_id.into());
            let (node, measure) = tree.node_query.get(e).unwrap();
            let has_children = tree.child_count(node_id) != 0;

            match (node.display, has_children) {
                (Display::Flexbox, true) => compute_flexbox_layout(tree, node_id, inputs),
                (Display::Block, true) => compute_block_layout(tree, node_id, inputs),
                (Display::None, _) => compute_hidden_layout(tree, node_id),
                (_, false) => compute_leaf_layout(inputs, node, |known_size, available_space| {
                    if let Some(measure) = measure {
                        let Vec2 { x: width, y: height } = unsafe {
                            tree.measurements[measure.get()].measure(
                                (known_size.width, known_size.height),
                                (available_space.width.into(), available_space.height.into()),
                                e,
                            )
                        };

                        Size { width, height }
                    } else {
                        Size::ZERO
                    }
                }),
            }
        })
    }
}

impl<M> RoundTree for UiTree<'_, '_, M> {
    #[inline]
    fn get_unrounded_layout(&self, node_id: NodeId) -> &Layout {
        &self.outputs[&Entity::from_bits(node_id.into())]
    }

    #[inline]
    fn set_final_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.outputs.insert(Entity::from_bits(node_id.into()), layout.clone());
    }
}

impl<M> PrintTree for UiTree<'_, '_, M> {
    #[inline]
    fn get_debug_label(&self, _: NodeId) -> &'static str {
        "something"
    }

    #[inline]
    fn get_final_layout(&self, node_id: NodeId) -> &Layout {
        &self.outputs[&Entity::from_bits(node_id.into())]
    }
}

impl<M: IndexMut<MeasureId, Output = dyn Measurer>> LayoutFlexboxContainer for UiTree<'_, '_, M> {
    type FlexboxContainerStyle<'a>
        = &'a Node
    where
        Self: 'a;

    type FlexboxItemStyle<'a>
        = &'a Node
    where
        Self: 'a;

    #[inline]
    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    #[inline]
    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl<M: IndexMut<MeasureId, Output = dyn Measurer>> LayoutBlockContainer for UiTree<'_, '_, M> {
    type BlockContainerStyle<'a>
        = &'a Node
    where
        Self: 'a;

    type BlockItemStyle<'a>
        = &'a Node
    where
        Self: 'a;

    #[inline]
    fn get_block_container_style(&self, node_id: NodeId) -> Self::BlockContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    #[inline]
    fn get_block_child_style(&self, child_node_id: NodeId) -> Self::BlockItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl<M> CacheTree for UiTree<'_, '_, M> {
    #[inline]
    fn cache_get(
        &self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<LayoutOutput> {
        self.cache_query
            .get(Entity::from_bits(node_id.into()))
            .ok()
            .and_then(|cache| cache.0.get(known_dimensions, available_space, run_mode))
    }

    #[inline]
    fn cache_store(
        &mut self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: LayoutOutput,
    ) {
        if let Ok(mut cache) = self.cache_query.get_mut(Entity::from_bits(node_id.into())) {
            cache.0.store(known_dimensions, available_space, run_mode, layout_output)
        }
    }

    #[inline]
    fn cache_clear(&mut self, node_id: NodeId) {
        if let Ok(mut cache) = self.cache_query.get_mut(Entity::from_bits(node_id.into())) {
            cache.0.clear()
        }
    }
}

pub(crate) fn compute_ui_tree(
    world: &mut World,
    root_query: &mut SystemState<(
        Query<(Entity, &UiRootTrns)>,
        // Parameters for `UiTree`.
        Query<(Read<Node>, Option<Read<ContentSize>>)>,
        Query<Read<NodeChildren>>,
        Query<Write<NodeCache>>,
        Local<EntityHashMap<Layout>>,
    )>,
) {
    world.resource_scope(|world, mut measurements: Mut<Measurements>| {
        let cell = world.as_unsafe_world_cell();

        root_query.update_archetypes_unsafe_world_cell(cell);
        let ((root_query, node_query, children_query, cache_query, outputs), measurements) =
            unsafe { (root_query.get_unchecked_manual(cell), measurements.get_measurers(cell)) };

        let mut tree = UiTree {
            measurements,
            node_query,
            children_query,
            cache_query,
            outputs,
        };

        for (e, &trns) in &root_query {
            compute_root_layout(&mut tree, NodeId::from(e.to_bits()), taffy::Size {
                width: taffy::AvailableSpace::Definite(trns.size.x),
                height: taffy::AvailableSpace::Definite(trns.size.y),
            });
        }
    })
}
