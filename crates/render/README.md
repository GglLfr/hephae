# `hephae-render`

Hephae's core rendering module. This library provides the following for you to build your framework on:

- `Vertex`: The heart of Hephae. Defines the vertex buffer layout, rendering pipeline specialization, batching
  parameters, and draw commands.
- `Drawer`: A render-world `Component` extracted from entities with `HasDrawer<T>`, acting as the "commander" to push
  out vertices and indices according to their logic-world entity parameters.
- `VertexCommand`: A "draw command" issued by `Drawer`, cached and sorted in the pipeline and modifies the GPU buffers
  directly when dispatched by camera views.
- `HephaeRenderPlugin<T: Vertex>`: Attaches Hephae vertex systems generic over `T` to the application.
- `DrawerPlugin<T: Drawer>`: Attaches Hephae vertex-drawer systems generic over `T` to the application.

The five of these are enough to build a sprite-less colorful 2D rendering system (see `examples/quad.rs`). Please refer
to the item-level documentations for more in-depth explanations and usage guides.
