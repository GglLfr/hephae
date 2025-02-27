use std::{iter::Map, ops::Index};

use bevy_ecs::{
    entity::EntityHashMap,
    prelude::*,
    query::QueryManyIter,
    system::{
        SystemParam, SystemState,
        lifetimeless::{Read, Write},
    },
};
use bevy_hierarchy::prelude::*;
use bevy_math::prelude::*;
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;
use taffy::{
    AvailableSpace, Cache, CacheTree, Layout, LayoutBlockContainer, LayoutFlexboxContainer, LayoutInput, LayoutOutput,
    LayoutPartialTree, NodeId, PrintTree, RoundTree, RunMode, Size, TraversePartialTree, TraverseTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_hidden_layout, compute_leaf_layout, compute_root_layout,
    round_layout,
};

use crate::{
    measure::{ContentSize, MeasureId, Measurements, Measurer},
    root::{UiRootSize, UiUnrounded},
    style::{Display, Ui},
};

#[derive(Component, Copy, Clone, Default)]
pub struct ComputedUi {
    /// The relative ordering of the node.
    ///
    /// Nodes with a higher order should be rendered on top of those with a lower order.
    /// This is effectively a topological sort of each tree.
    pub order: u32,
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

#[derive(Reflect, Copy, Clone, Default)]
#[reflect(Default)]
pub struct Border {
    pub left: f32,
    pub right: f32,
    pub top: f32,
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
        self.0.clear()
    }
}

#[derive(SystemParam)]
pub struct UiCaches<'w, 's> {
    pub parent_query: Query<'w, 's, Read<Parent>>,
    cache_query: Query<'w, 's, (Write<UiCache>, Has<UiRootSize>)>,
}

impl UiCaches<'_, '_> {
    #[inline]
    pub fn invalidate(&mut self, mut e: Entity) {
        loop {
            let Ok((mut cache, has_root)) = self.cache_query.get_mut(e) else {
                break
            };

            cache.clear();
            if let Some(parent) = (!has_root).then(|| self.parent_query.get(e).ok()).flatten() {
                e = **parent
            } else {
                break
            }
        }
    }
}

pub(crate) struct UiTree<'w, 's, M> {
    measurements: M,
    ui_query: Query<'w, 's, (Entity, Read<Ui>, Option<Read<ContentSize>>), Without<UiRootSize>>,
    children_query: Query<'w, 's, Read<Children>>,
    cache_query: Query<'w, 's, Write<UiCache>>,
    outputs: &'s mut EntityHashMap<(Transform, Layout)>,
}

impl<M> TraverseTree for UiTree<'_, '_, M> {}

impl<M> TraversePartialTree for UiTree<'_, '_, M> {
    type ChildIter<'a>
        = Map<
        QueryManyIter<
            'a,
            'a,
            (Entity, Read<Ui>, Option<Read<ContentSize>>),
            Without<UiRootSize>,
            std::slice::Iter<'a, Entity>,
        >,
        fn((Entity, &Ui, Option<&ContentSize>)) -> NodeId,
    >
    where
        Self: 'a;

    #[inline]
    fn child_ids(&self, parent_node_id: NodeId) -> Self::ChildIter<'_> {
        let children = self
            .children_query
            .get(Entity::from_bits(parent_node_id.into()))
            .map(|children| &**children)
            .unwrap_or(const { &[] })
            .iter();

        self.ui_query.iter_many(children).map(|(e, ..)| NodeId::from(e.to_bits()))
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
        = &'a Ui
    where
        Self: 'a;

    #[inline]
    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        self.ui_query.get(Entity::from_bits(node_id.into())).unwrap().1
    }

    #[inline]
    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.outputs
            .insert(Entity::from_bits(node_id.into()), (Transform::IDENTITY, *layout));
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
                (_, false) => compute_leaf_layout(inputs, node, |known_size, available_space| {
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
                }),
            }
        })
    }
}

impl<M> RoundTree for UiTree<'_, '_, M> {
    #[inline]
    fn get_unrounded_layout(&self, node_id: NodeId) -> &Layout {
        &self.outputs[&Entity::from_bits(node_id.into())].1
    }

    #[inline]
    fn set_final_layout(&mut self, node_id: NodeId, layout: &Layout) {
        self.outputs
            .insert(Entity::from_bits(node_id.into()), (Transform::IDENTITY, *layout));
    }
}

impl<M> PrintTree for UiTree<'_, '_, M> {
    #[inline]
    fn get_debug_label(&self, node_id: NodeId) -> &'static str {
        let node = self.ui_query.get(Entity::from_bits(node_id.into())).unwrap().1;
        match node.display {
            Display::Flexbox => "flexbox",
            Display::Block => "block",
            Display::None => "none",
        }
    }

    #[inline]
    fn get_final_layout(&self, node_id: NodeId) -> &Layout {
        &self.outputs[&Entity::from_bits(node_id.into())].1
    }
}

impl<M: Index<MeasureId, Output = dyn Measurer>> LayoutFlexboxContainer for UiTree<'_, '_, M> {
    type FlexboxContainerStyle<'a>
        = &'a Ui
    where
        Self: 'a;

    type FlexboxItemStyle<'a>
        = &'a Ui
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

impl<M: Index<MeasureId, Output = dyn Measurer>> LayoutBlockContainer for UiTree<'_, '_, M> {
    type BlockContainerStyle<'a>
        = &'a Ui
    where
        Self: 'a;

    type BlockItemStyle<'a>
        = &'a Ui
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
    compute_state: &mut SystemState<(
        Query<(Entity, &UiRootSize, Has<UiUnrounded>), Changed<UiCache>>,
        Query<(Entity, Read<Ui>, Option<Read<ContentSize>>), Without<UiRootSize>>,
        Query<Read<Children>>,
        Query<Write<UiCache>>,
    )>,
    propagate_state: &mut SystemState<(Query<Entity, With<UiRootSize>>, Query<&Children>)>,
    mut outputs: Local<EntityHashMap<(Transform, Layout)>>,
) {
    world.resource_scope(|world, mut measurers: Mut<Measurements>| {
        {
            let cell = world.as_unsafe_world_cell();

            compute_state.update_archetypes_unsafe_world_cell(cell);
            let ((root_query, ui_query, children_query, cache_query), measurements) =
                unsafe { (compute_state.get_unchecked_manual(cell), measurers.get_measurers(cell)) };

            let outputs = &mut *outputs;
            let mut tree = UiTree {
                measurements,
                ui_query,
                children_query,
                cache_query,
                outputs,
            };

            for (e, &trns, is_unrounded) in &root_query {
                let node_id = NodeId::from(e.to_bits());
                compute_root_layout(&mut tree, node_id, taffy::Size {
                    width: taffy::AvailableSpace::Definite(trns.0.x),
                    height: taffy::AvailableSpace::Definite(trns.0.y),
                });

                if !is_unrounded {
                    round_layout(&mut tree, node_id);
                }
            }
        }

        let (root_query, children_query) = propagate_state.get(world);
        for e in &root_query {
            let Some((trns, layout)) = outputs.get_mut(&e) else { continue };

            let pos = Vec3::new(layout.location.x, layout.location.y - layout.size.height, 0.);
            *trns = Transform::from_translation(pos);

            if let Ok(children) = children_query.get(e) {
                propagate(pos, children, &children_query, &mut outputs)
            }
        }

        fn propagate(
            parent: Vec3,
            entities: &[Entity],
            children_query: &Query<&Children>,
            outputs: &mut EntityHashMap<(Transform, Layout)>,
        ) {
            for &e in entities {
                let Some((trns, layout)) = outputs.get_mut(&e) else { continue };
                let pos = Vec3::new(
                    layout.location.x - parent.x,
                    layout.location.y - parent.y - layout.size.height,
                    parent.z + 0.001,
                );

                *trns = Transform::from_translation(pos);
                if let Ok(children) = children_query.get(e) {
                    propagate(pos, children, &children_query, outputs)
                }
            }
        }

        world.insert_batch(outputs.drain().map(|(e, (trns, layout))| {
            (
                e,
                (trns, ComputedUi {
                    order: layout.order,
                    size: Vec2::new(layout.size.width, layout.size.height),
                    content_size: Vec2::new(layout.content_size.width, layout.content_size.height),
                    scrollbar_size: Vec2::new(layout.scrollbar_size.width, layout.scrollbar_size.height),
                    border: layout.border.into(),
                    padding: layout.padding.into(),
                    margin: layout.margin.into(),
                }),
            )
        }));

        measurers.apply_measurers(world);
    })
}
