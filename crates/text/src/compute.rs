use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;

use crate::def::{ComputedTextLayout, TextLayout};

pub fn calculate_buffers(mut root_query: Query<(Ref<TextLayout>, &mut ComputedTextLayout, Option<&Children>)>) {
    for (layout, mut computed, children) in &mut root_query {}
}
