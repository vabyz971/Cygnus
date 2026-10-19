# math-utils

Geometry: re-exports [`Transform2D`](src/transform2d.rs)
and [`Vec2`](src/vec2.rs) from `datatypes` to simplify
app imports.

Key functions:
- [`Transform2D`] (3×3 matrix, inverse matrix, etc.)
- [`Vec2`] (2D vector)

No logic: these types are defined in `datatypes`
and re-exported here to avoid duplicating imports.
