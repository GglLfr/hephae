//! Defines everything related to UI tree computations.

use std::{iter::FilterMap, ops::Index};

use bevy::{
    ecs::{
        entity::EntityHashMap,
        query::QueryManyIter,
        system::{
            SystemParam, SystemState,
            lifetimeless::{Read, Write},
        },
    },
    prelude::*,
};
use taffy::{
    AvailableSpace, Cache, CacheTree, Layout, LayoutBlockContainer, LayoutFlexboxContainer, LayoutInput, LayoutOutput,
    LayoutPartialTree, NodeId, PrintTree, RoundTree, RunMode, Size, TraversePartialTree, TraverseTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_hidden_layout, compute_leaf_layout, compute_root_layout,
    round_layout,
};

use crate::{
    measure::{ContentSize, MeasureId, Measurements, Measurer},
    root::{UiRootTrns, UiUnrounded},
    style::{Display, Ui, WithCtx},
};

/// The computed layout values of a [`Ui`] node.
#[derive(Component, Copy, Clone, Default)]
#[require(IntermediateUi, UiCache)]
pub struct ComputedUi {
    /// The relative ordering of the node.
    ///
    /// Nodes with a higher order should be rendered on top of those with a lower order.
    /// This is effectively a topological sort of each tree.
    pub order: u32,
    /// The top-left corner of the node.
    location: Vec2,
    /// The width and height of the node.
    pub size: Vec2,
    /// The width and height of the content inside the node. This may be larger than the size of the
    /// node in the case of overflowing content and is useful for computing a "scroll width/height"
    /// for scrollable nodes.
    pub content_size: Vec2,
    /// The size of the scrollbars in each dimension. If there is no scrollbar then the size will be
    /// zero.
    pub scrollbar_size: Vec2,
    /// The size of the borders of the node.
    pub border: Border,
    /// The size of the padding of the node.
    pub padding: Border,
    /// The size of the margin of the node.
    pub margin: Border,
}

#[derive(Component, Copy, Clone, Default)]
pub(crate) struct IntermediateUi(Layout);

/// A border rectangle.
#[derive(Reflect, Copy, Clone, Default)]
#[reflect(Default)]
pub struct Border {
    /// X-coordinate or padding width on the left side.
    pub left: f32,
    /// X-coordinate or padding width on the right side.
    pub right: f32,
    /// Y-coordinate or padding height on the top side.
    pub top: f32,
    /// Y-coordinate or padding height on the bottom side.
    pub bottom: f32,
}

impl From<taffy::Rect<f32>> for Border {
    #[inline]
    fn from(
        taffy::Rect {
            left,
            right,
            top,
            bottom,
        }: taffy::Rect<f32>,
    ) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }
}

#[derive(Component, Default)]
pub(crate) struct UiCache(Cache);
impl UiCache {
    #[inline]
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// A system parameter used for invalidating UI caches. Use if you need a [`Ui`] node to be
/// recomputed.
#[derive(SystemParam)]
pub struct UiCaches<'w, 's>(Query<'w, 's, (Write<UiCache>, Option<Read<ChildOf>>)>);

impl UiCaches<'_, '_> {
    /// Invalidates a [`Ui`] node's cache recursively to its root.
    #[inline]
    pub fn invalidate(&mut self, mut e: Entity) {
        loop {
            let Ok((mut cache, parent)) = self.0.get_mut(e) else { break };
            cache.clear();

            if let Some(parent) = parent { e = parent.parent() } else { break }
        }
    }
}

pub(crate) struct UiTree<'w, 's, M> {
    measurements: M,
    viewport_size: Vec2,
    ui_query: Query<'w, 's, (Entity, Has<UiRootTrns>, Read<Ui>, Option<Read<ContentSize>>)>,
    children_query: Query<'w, 's, Read<Children>>,
    intermediate_query: Query<'w, 's, Write<IntermediateUi>>,
    cache_query: Query<'w, 's, Write<UiCache>>,
    outputs: &'s mut EntityHashMap<Layout>,
}

impl<M> TraverseTree for UiTree<'_, '_, M> {}

impl<M> TraversePartialTree for UiTree<'_, '_, M> {
    type ChildIter<'a>
        = FilterMap<
        QueryManyIter<
            'a,
            'a,
            (Entity, Has<UiRootTrns>, Read<Ui>, Option<Read<ContentSize>>),
            (),
            std::slice::Iter<'a, Entity>,
        >,
        fn((Entity, bool, &Ui, Option<&ContentSize>)) -> Option<NodeId>,
    >
    where Self: 'a;

    #[inline]
    fn child_ids(&self, parent_node_id: NodeId) -> Self::ChildIter<'_> {
        let children = self
            .children_query
            .get(Entity::from_bits(parent_node_id.into()))
            .map(|children| &**children)
            .unwrap_or(&[])
            .iter();

        self.ui_query
            .iter_many(children)
            .filter_map(|(e, is_root, ..)| (!is_root).then_some(NodeId::from(e.to_bits())))
    }

    #[inline]
    fn child_count(&self, parent_node_id: NodeId) -> usize {
        self.child_ids(parent_node_id).count()
    }

    #[inline]
    fn get_child_id(&self, parent_node_id: NodeId, child_index: usize) -> NodeId {
        self.child_ids(parent_node_id).nth(child_index).unwrap()
    }
}

impl<M: Index<MeasureId, Output = dyn Measurer>> LayoutPartialTree for UiTree<'_, '_, M> {
    type CoreContainerStyle<'a>
        = WithCtx<&'a Ui>
    where Self: 'a;

    #[inline]
    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        let e = Entity::from_bits(node_id.into());
        WithCtx {
            width: self.viewport_size.x,
            height: self.viewport_size.y,
            item: self.ui_query.get(e).unwrap().2,
        }
    }

    #[inline]
    fn resolve_calc_value(&self, _val: *const (), _basis: f32) -> f32 {
        0.
    }

    #[inline]
    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.intermediate_query.get_mut(Entity::from_bits(node_id.into())).unwrap().0 = *layout
    }

    #[inline]
    fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let e = Entity::from_bits(node_id.into());
            let (.., node, measure) = tree.ui_query.get(e).unwrap();
            let has_children = tree.child_count(node_id) != 0;

            match (node.display, has_children) {
                (Display::Flexbox, true) => compute_flexbox_layout(tree, node_id, inputs),
                (Display::Block, true) => compute_block_layout(tree, node_id, inputs),
                (Display::None, _) => compute_hidden_layout(tree, node_id),
                (_, false) => compute_leaf_layout(
                    inputs,
                    &WithCtx {
                        width: tree.viewport_size.x,
                        height: tree.viewport_size.y,
                        item: node,
                    },
                    |val, basis| tree.resolve_calc_value(val, basis),
                    |known_size, available_space| {
                        if let Some(measure) = measure.and_then(|id| match id.get() {
                            MeasureId::INVALID => None,
                            id => Some(id),
                        }) {
                            let Vec2 { x: width, y: height } = tree.measurements[measure].measure(
                                (known_size.width, known_size.height),
                                (available_space.width.into(), available_space.height.into()),
                                e,
                            );

                            Size { width, height }
                        } else {
                            Size::ZERO
                        }
                    },
                ),
            }
        })
    }
}

impl<M> RoundTree for UiTree<'_, '_, M> {
    #[inline]
    fn get_unrounded_layout(&self, node_id: NodeId) -> &Layout {
        &self.intermediate_query.get(Entity::from_bits(node_id.into())).unwrap().0
    }

    #[inline]
    fn set_final_layout(&mut self, node_id: NodeId, layout: &Layout) {
        let e = Entity::from_bits(node_id.into());
        self.outputs.insert(e, *layout);
    }
}

impl<M> PrintTree for UiTree<'_, '_, M> {
    #[inline]
    fn get_debug_label(&self, node_id: NodeId) -> &'static str {
        let node = self.ui_query.get(Entity::from_bits(node_id.into())).unwrap().2;
        match node.display {
            Display::Flexbox => "flexbox",
            Display::Block => "block",
            Display::None => "none",
        }
    }

    #[inline]
    fn get_final_layout(&self, node_id: NodeId) -> &Layout {
        &self.outputs[&Entity::from_bits(node_id.into())]
    }
}

impl<M: Index<MeasureId, Output = dyn Measurer>> LayoutFlexboxContainer for UiTree<'_, '_, M> {
    type FlexboxContainerStyle<'a>
        = WithCtx<&'a Ui>
    where Self: 'a;

    type FlexboxItemStyle<'a>
        = WithCtx<&'a Ui>
    where Self: 'a;

    #[inline]
    fn get_flexbox_container_style(&self, node_id: NodeId) -> Self::FlexboxContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    #[inline]
    fn get_flexbox_child_style(&self, child_node_id: NodeId) -> Self::FlexboxItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl<M: Index<MeasureId, Output = dyn Measurer>> LayoutBlockContainer for UiTree<'_, '_, M> {
    type BlockContainerStyle<'a>
        = WithCtx<&'a Ui>
    where Self: 'a;

    type BlockItemStyle<'a>
        = WithCtx<&'a Ui>
    where Self: 'a;

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
        let e = Entity::from_bits(node_id.into());
        if let Ok(mut cache) = self.cache_query.get_mut(e) {
            cache.0.store(known_dimensions, available_space, run_mode, layout_output)
        }
    }

    #[inline]
    fn cache_clear(&mut self, node_id: NodeId) {
        if let Ok(mut cache) = self.cache_query.get_mut(Entity::from_bits(node_id.into())) {
            cache.0.clear();
        }
    }
}

pub(crate) fn compute_ui_tree(
    world: &mut World,
    compute_state: &mut SystemState<(
        Query<(Ref<UiRootTrns>, &Children, Has<UiUnrounded>)>,
        Query<(Entity, Has<UiRootTrns>, Read<Ui>, Option<Read<ContentSize>>)>,
        Query<Read<Children>>,
        Query<Write<IntermediateUi>>,
        Query<Write<UiCache>>,
    )>,
    propagate_state: &mut SystemState<(
        Query<(&UiRootTrns, &Children)>,
        Query<(&mut Transform, &ComputedUi)>,
        Query<&Children>,
    )>,
    mut outputs: Local<EntityHashMap<Layout>>,
) {
    world.resource_scope(|world, mut measurers: Mut<Measurements>| {
        {
            let cell = world.as_unsafe_world_cell();

            compute_state.update_archetypes_unsafe_world_cell(cell);
            let ((root_query, ui_query, children_query, intermediate_query, cache_query), measurements) =
                unsafe { (compute_state.get_unchecked_manual(cell), measurers.get_measurers(cell)) };

            let mut tree = UiTree {
                measurements,
                viewport_size: Vec2::ZERO,
                ui_query,
                children_query,
                intermediate_query,
                cache_query,
                outputs: &mut outputs,
            };

            for (trns, roots, is_unrounded) in &root_query {
                tree.viewport_size = trns.size;
                for &root in roots {
                    if trns.is_changed() || tree.cache_query.get_mut(root).is_ok_and(|cache| cache.is_changed()) {
                        let node_id = NodeId::from(root.to_bits());
                        compute_root_layout(&mut tree, node_id, Size {
                            width: AvailableSpace::Definite(trns.size.x),
                            height: AvailableSpace::Definite(trns.size.y),
                        });

                        if !is_unrounded {
                            round_layout(&mut tree, node_id)
                        }
                    }
                }
            }
        }

        world.insert_batch(outputs.drain().map(|(e, layout)| {
            (e, ComputedUi {
                order: layout.order,
                location: Vec2::new(layout.location.x, layout.location.y),
                size: Vec2::new(layout.size.width, layout.size.height),
                content_size: Vec2::new(layout.content_size.width, layout.content_size.height),
                scrollbar_size: Vec2::new(layout.scrollbar_size.width, layout.scrollbar_size.height),
                border: layout.border.into(),
                padding: layout.padding.into(),
                margin: layout.margin.into(),
            })
        }));

        measurers.apply_measurers(world);

        let (root_query, mut query, children_query) = propagate_state.get_mut(world);
        for (trns, roots) in &root_query {
            propagate(trns.transform, trns.size.y, roots, &mut query, &children_query);
        }

        fn propagate(
            root_transform: Transform,
            parent_height: f32,
            entities: &[Entity],
            query: &mut Query<(&mut Transform, &ComputedUi)>,
            children_query: &Query<&Children>,
        ) {
            for &e in entities {
                let Ok((mut trns, layout)) = query.get_mut(e) else { continue };
                let pos = Vec3::new(layout.location.x, parent_height - layout.location.y - layout.size.y, 0.001);

                trns.set_if_neq(root_transform * Transform::from_translation(pos));
                if let Ok(children) = children_query.get(e) {
                    propagate(Transform::IDENTITY, layout.size.y, children, query, children_query)
                }
            }
        }
    })
}
