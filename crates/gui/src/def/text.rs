use bevy_ecs::prelude::*;
use hephae_text::prelude::*;

#[derive(Component, Copy, Clone, Default)]
#[require(Text)]
pub struct UiText;
