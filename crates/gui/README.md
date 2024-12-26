# `hephae-gui`

Hephae's GUI abstract layout module, with a sensible default. If you'd like to create your own layout system, refer to
these:

- `GuiLayout`: The main component that handles the affine transform (which includes offset, scale, and rotation) and
  size for its direct children. `Cont` is a built-in GUI layout that arranges its children either horizontally or
  vertically without wrapping.
- `GuiRoot`: The component that's placed on root GUI entities only, specifying how its GUI tree should be projected to
  the world space. `FromCamera2d` is a built-in GUI root that projects its tree to the 2D camera's near space.
- `GuiLayoutPlugin<T: GuiLayout>`: Attaches layout systems generic over `T` to the application.
- `GuiRootPlugin<T: GuiRoot>`: Attaches root-transform systems generic over `T` to the application.
