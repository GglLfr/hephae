use std::any::type_name;

use bevy_ecs::{
    prelude::*,
    query::{QueryData, QueryItem},
    system::{SystemParam, SystemParamItem},
};
use bevy_hierarchy::prelude::*;
use bevy_math::{Affine3A, prelude::*};
use bevy_utils::tracing::warn;

use crate::style::Node;

pub trait UiRoot: Component {
    type Param: SystemParam;
    type Item: QueryData;

    fn compute_root_transform(
        &mut self,
        param: &mut SystemParamItem<Self::Param>,
        item: QueryItem<Self::Item>,
    ) -> (Affine3A, Vec2);
}

#[derive(Component, Copy, Clone, Default, PartialEq)]
pub(crate) struct UiRootTrns {
    pub trns: Affine3A,
    pub size: Vec2,
}

pub(crate) fn compute_root_transform<T: UiRoot>(
    mut param: SystemParamItem<T::Param>,
    mut query: Query<(&mut T, &mut UiRootTrns, T::Item)>,
) {
    for (mut root, mut output, item) in &mut query {
        let (trns, size) = root.compute_root_transform(&mut param, item);
        output.set_if_neq(UiRootTrns { trns, size });
    }
}

pub(crate) fn on_parent_inserted<T: UiRoot>(
    trigger: Trigger<OnInsert, Parent>,
    mut commands: Commands,
    root_query: Query<&Parent, With<T>>,
    node_query: Query<(), With<Node>>,
) {
    let e = trigger.entity();
    if let Ok(parent) = root_query.get(e) {
        if node_query.contains(**parent) {
            warn!("`{}` in {e} contains a UI parent; removing", type_name::<T>());
            commands.entity(e).remove::<(T, UiRootTrns)>();
        }
    }
}

pub(crate) fn on_root_inserted<T: UiRoot>(
    trigger: Trigger<OnInsert, T>,
    mut commands: Commands,
    root_query: Query<&Parent>,
    node_query: Query<(), With<Node>>,
) {
    let e = trigger.entity();
    if let Ok(parent) = root_query.get(e) {
        if node_query.contains(**parent) {
            warn!("`{}` in {e} contains a UI parent; removing", type_name::<T>());
            commands.entity(e).remove::<(T, UiRootTrns)>();
        }
    }
}
