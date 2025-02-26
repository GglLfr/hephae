
use bevy_ecs::{
    prelude::*,
    query::{QueryData, QueryItem},
    system::{SystemParam, SystemParamItem},
};
use bevy_math::{Affine3A, prelude::*};


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
