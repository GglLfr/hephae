# `hephae-atlas`

Provides texture atlas functionality.

A texture atlas contains atlas pages, i.e. lists of textures packed into one large texture in order to reduce the amount
of bind groups necessary to hold the information passed to shaders. This means integrating a texture atlas into `Vertex`
rendering will significantly increase batching potential, leading to fewer GPU render calls.

This module provides the `Atlas` type, which also derives `Asset` and has an associated asset loader with it.
Refer to `AtlasFile` for the specific format of `.atlas` files.

This module also provides `AtlasEntry` and `AtlasCache` components; the former being the atlas lookup key, and the
latter being the cached sprite index. The dedicated update_atlas_index system listens to changes/additions to texture
atlas assets and updates the `AtlasCache` of entities accordingly.

Ideally, you'd want to associate each atlas pages with a `BindGroup`, define a texture and sampler layout in the
specialized pipeline, somehow store a reference to this bind group into the batch entities, and finally set the render
pass' bind group to the atlas page bind group accordingly with the layout you defined earlier.

See the `examples/sprite.rs` for a full example.
