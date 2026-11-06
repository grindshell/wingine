# `rect`

Functions of rectangles: tables `{ x = ..., y = ..., width = ..., height = ... }`, where `x`, `y`
is the top-left corner (see [`Rect`](README.md#rect)), such as
[`node:bounds()`](scene.md#nodebounds) and [`map:bounds()`](tilemap.md#mapbounds) return. Their
edges follow the engine's own rectangles, so a game that tests its own rectangles agrees with the
scenes it tests them against.

They're written in Luau, so calling one costs what calling a script's own function does.

## `rect.overlaps`

```lua
rect.overlaps(a: Rect, b: Rect): boolean
```

Returns whether `a` and `b` overlap. Rectangles that only touch, side by side or at a corner,
don't.

```lua
-- Whether the sword's swing reaches the slime.
local swing = { x = hero.x + 8, y = hero.y, width = 12, height = 16 }
if rect.overlaps(swing, slime:bounds()) then
    hurt(slime)
end
```

## `rect.contains`

```lua
rect.contains(r: Rect, x: number, y: number): boolean
```

Returns whether the point `x`, `y` is in `r`. Its top and left edges count as in it, and its bottom
and right edges don't, so a point on the edge between two rectangles side by side is in exactly
one of them, and a rectangle with no width or height contains nothing.

```lua
-- Whether the fighter has left the stage, by where their middle is.
if not rect.contains(stage.blast_zone, fighter.x, fighter.y - fighter.height / 2) then
    knock_out(fighter)
end
```

[`node:contains(x, y)`](scene.md#nodecontains) tests a point against what a node covers in the
same way.
