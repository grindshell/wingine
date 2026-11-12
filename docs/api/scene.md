# `scene`

Scenes loaded from scene files, and the nodes they're made of. [Scenes](../scenes.md) describes
the files.

- [Loading](#loading): [`scene.load`](#sceneload), [`scene.clips`](#sceneclips)
- [Nodes](#nodes): [`name`](#nodename), [`kind`](#nodekind), [`path`](#nodepath),
  [`find`](#nodefind), [`tagged`](#nodetagged), [`parent`](#nodeparent),
  [`children`](#nodechildren), [`tags`](#nodetags), [`has_tag`](#nodehas_tag),
  [`props`](#nodeprops)
- [Placing nodes](#placing-nodes): [`position`](#nodeposition),
  [`set_position`](#nodeset_position), [`anchor`](#nodeanchor), [`set_anchor`](#nodeset_anchor),
  [`rotation`](#noderotation), [`set_rotation`](#nodeset_rotation), [`scale`](#nodescale),
  [`set_scale`](#nodeset_scale), [`world_position`](#nodeworld_position),
  [`is_visible`](#nodeis_visible), [`set_visible`](#nodeset_visible)
- [What nodes cover](#what-nodes-cover): [`size`](#nodesize), [`set_size`](#nodeset_size),
  [`bounds`](#nodebounds), [`contains`](#nodecontains)
- [What nodes show](#what-nodes-show): [`set_sprite`](#nodeset_sprite),
  [`set_cell`](#nodeset_cell), [`set_tint`](#nodeset_tint), [`set_color`](#nodeset_color),
  [`text`](#nodetext), [`set_text`](#nodeset_text), [`blend`](#nodeblend),
  [`set_blend`](#nodeset_blend)
- [Particles](#particles): [`advance_particles`](#nodeadvance_particles),
  [`particle_count`](#nodeparticle_count), [`clear_particles`](#nodeclear_particles),
  [`emit`](#nodeemit), [`is_emitting`](#nodeis_emitting), [`set_emitting`](#nodeset_emitting),
  [`particle_settings`](#nodeparticle_settings),
  [`set_particle_settings`](#nodeset_particle_settings)
- [Nodes in 3D views](#nodes-in-3d-views): [`position_3d`](#nodeposition_3d),
  [`set_position_3d`](#nodeset_position_3d), [`orientation`](#nodeorientation),
  [`set_orientation`](#nodeset_orientation), [`scale_3d`](#nodescale_3d),
  [`set_scale_3d`](#nodeset_scale_3d), [`world_position_3d`](#nodeworld_position_3d),
  [`bounds_3d`](#nodebounds_3d), [`light`](#nodelight), [`set_light`](#nodeset_light),
  [`casts_shadow`](#nodecasts_shadow), [`set_casts_shadow`](#nodeset_casts_shadow)
- [3D views](#3d-views): [`camera`](#nodecamera), [`set_camera`](#nodeset_camera),
  [`lighting`](#nodelighting), [`set_lighting`](#nodeset_lighting), [`fog`](#nodefog),
  [`set_fog`](#nodeset_fog), [`pick`](#nodepick),
  [`project`](#nodeproject), [`ray`](#noderay)
- [Posing](#posing): [`pose`](#nodepose), [`Clip`](#clip)
- [Adding and removing nodes](#adding-and-removing-nodes): [`add`](#nodeadd),
  [`remove`](#noderemove), [`is_removed`](#nodeis_removed)
- [Tilemaps](#tilemaps): [`is_solid`](#nodeis_solid), [`tile_to_world`](#nodetile_to_world),
  [`world_to_tile`](#nodeworld_to_tile), [`grid_size`](#nodegrid_size),
  [`tile_size`](#nodetile_size), [`tile_props`](#nodetile_props), [`tile`](#nodetile),
  [`set_tile`](#nodeset_tile)

Draw a scene with [`graphics.draw_scene`](graphics.md#graphicsdraw_scene).

## Loading

### `scene.load`

```lua
scene.load(path: string): SceneNode
```

Loads the scene file at `path`, relative to the game's folder, and the scene files its instances
place, the tileset files its tilemaps use and the images its sheets name, and returns the
scene's root: a plain node holding the scene's `props`, with the file's nodes as its children.
The scene is laid out on the canvas, as its [anchors](../scenes.md#laying-out-screens) say, and
laid out again each time it's drawn.

Each call returns a scene of its own, so loading a file twice gives two scenes that change
independently, such as two of the same enemy. Each file is read once, however many times it's
loaded or placed, and each image once, however many scenes and sprite sheets use it.

A file that can't be read, or that isn't a scene, is an error naming the file and the node where
the problem is. Works while the game loads and in `update`.

### `scene.clips`

```lua
scene.clips(path: string): { [string]: Clip }
```

Returns the [clips](../scenes.md#clips) of the scene file at `path`, by name, for posing nodes
with [`node:pose`](#nodepose). The file is read once, whether it's loaded, placed or asked for
its clips. A file that can't be read, or that isn't a scene, is an error. Works at any time.

```lua
local clips = scene.clips("hero.scene.luau")
hero:pose(clips.walk, walk_time)
```

## Nodes

A `SceneNode` is a node of a loaded scene. Every node of a scene shares the scene: changing a node
through one `SceneNode` changes it for every other that's the same node, and two `SceneNode`s of
the same node are `==`.

### `node:name`

```lua
node:name(): string
```

Returns the node's name, unique among its siblings. The root's is `""`.

### `node:kind`

```lua
node:kind(): string
```

Returns `"node"`, `"sprite"`, `"rect"`, `"text"`, `"area"`, `"stack"`, `"panel"`, `"particles"`,
`"tilemap"`, `"tile_layer"`, `"view3d"`, `"mesh"`, `"model"`, `"billboard"` or `"light"`. An
instance is a `"node"` once loaded.

### `node:path`

```lua
node:path(): string
```

Returns the names from the root to the node, separated by `/`, such as `"mill/sails"`, which
[`find`](#nodefind) finds from the root.

### `node:find`

```lua
node:find(path: string): SceneNode?
```

Returns the node `path` leads to from this one, such as `"mill/sails"` for the `sails` inside the
`mill` inside it, or `nil` if there's none.

### `node:tagged`

```lua
node:tagged(tag: string): { SceneNode }
```

Returns this node and every node inside it with the tag `tag`, in the order they draw.

```lua
for _, coin in level:tagged("coin") do
    coins += coin:props().value
end
```

### `node:parent`

```lua
node:parent(): SceneNode?
```

Returns the node it's inside, or `nil` for the root.

### `node:children`

```lua
node:children(): { SceneNode }
```

Returns the nodes directly inside it, in the order they draw.

### `node:tags`

```lua
node:tags(): { string }
```

### `node:has_tag`

```lua
node:has_tag(tag: string): boolean
```

### `node:props`

```lua
node:props(): { [string]: any }
```

Returns the node's props as a new table, a copy: changing it doesn't change the node. Reading props
makes a table each time, so read them once, not every frame.

## Placing nodes

A node is placed within its parent: its position, rotation and scale are in its parent's
coordinates, and its children move, turn and stretch with it. A node in a 3D view is placed in 3D
instead (see [Nodes in 3D views](#nodes-in-3d-views)): `position` and `set_position` are its `x`
and `y`, and `scale` and `set_scale` its `scale_x` and `scale_y`, but it has no rotation or anchor,
so `rotation`, `set_rotation` and `set_anchor` are errors.

### `node:position`

```lua
node:position(): (number, number)
```

Returns where its origin is within its parent: from the parent's origin, or from its
[anchor](#nodeanchor) if it has one.

### `node:set_position`

```lua
node:set_position(x: number, y: number)
```

### `node:anchor`

```lua
node:anchor(): (number?, number?)
```

Returns the point of its parent's rectangle its position is from, as fractions of the
rectangle's width and height from its top-left corner, or `nil` if it's placed from its
parent's origin. See [Laying out screens](../scenes.md#laying-out-screens).

### `node:set_anchor`

```lua
node:set_anchor(x: number?, y: number?)
```

Places it from the point `x`, `y` of its parent's rectangle, such as `0.5, 0.5` for its middle,
keeping its position, or with `nil`, from its parent's origin. The root's anchor does nothing.

### `node:rotation`

```lua
node:rotation(): number
```

Returns how far it's turned about its origin, in radians, clockwise.

### `node:set_rotation`

```lua
node:set_rotation(angle: number)
```

### `node:scale`

```lua
node:scale(): (number, number)
```

Returns how much it's stretched along its own axes about its origin. Negative scales mirror it.

### `node:set_scale`

```lua
node:set_scale(x: number, y: number?)
```

Stretches it by `x` and `y`, or by `x` both ways. A character whose pivot is the middle of its feet
faces the other way with `node:set_scale(-1, 1)`.

### `node:world_position`

```lua
node:world_position(): (number, number)
```

Returns where its origin is in the scene, after every node it's inside has moved, turned and
stretched it: in canvas pixels, or the world's with a camera, as the scene is drawn. It's an error
for a node in a 3D view, which is somewhere in the view's world instead: see
[`node:world_position_3d`](#nodeworld_position_3d).

### `node:is_visible`

```lua
node:is_visible(): boolean
```

### `node:set_visible`

```lua
node:set_visible(visible: boolean)
```

Shows or hides it and everything inside it.

Positions, anchors, rotations and scales must be finite numbers.

## What nodes cover

A sprite, rect, text, area or 3D view covers a rectangle, with its pivot at the node's origin, as
the rectangle particles start in does,
and a tilemap, or a layer of one, covers its map. A plain node covers nothing, and nor do nodes in
3D views, which have [boxes](#nodebounds_3d) instead.

### `node:size`

```lua
node:size(): (number, number)
```

Returns the width and height it covers, before any scaling: a sprite's size, a rect's, an
area's or a 3D view's, the size of a text's text, or the size of a tilemap's map. For a node that
[stretches](../scenes.md#laying-out-screens) with its parent, it's the size it has before
stretching, as [`set_size`](#nodeset_size) sets it; [`bounds`](#nodebounds) is where it is. A
billboard's is its width and height in its view's world. A [stack's or a
panel's](../scenes.md#stacks-and-panels) is its own size along the axes it has one, and what it
holds, with its padding, along those it fits it.

### `node:set_size`

```lua
node:set_size(width: number, height: number)
```

Changes the size of a sprite, a rect, an area, a stack, a panel, a 3D view, a billboard, or the
rectangle particles start in. A
sprite stretches to it, a billboard with a negative width is mirrored, and a stack or a panel
then has a size of its own both ways, rather than fitting what it holds.

### `node:bounds`

```lua
node:bounds(): Rect?
```

Returns the smallest [`Rect`](README.md#rect) in the scene's coordinates around what it covers, as
it's turned and stretched, or `nil` for a plain node. For a scene drawn without a camera, that's
where it is on the canvas, in canvas pixels.

### `node:contains`

```lua
node:contains(x: number, y: number): boolean
```

Returns whether the point `x`, `y`, in the scene's coordinates, is inside what it covers, as it's
turned and stretched. Points on its top and left edges are inside, and on its bottom and right
edges outside, so a point between two areas side by side is in exactly one. Nothing is inside a
plain node.

```lua
-- Whether the player stands in the doorway.
if level:find("house/door"):contains(player_x, player_y) then
    enter_house()
end
```

A scene drawn without a camera is in canvas pixels, as the mouse's position is, so `contains`
tells whether the mouse is over a node:

```lua
local x, y = input.mouse_position()
if x and y and menu:find("quit"):contains(x, y) and input.mouse_pressed("left") then
    quit_to_title()
end
```

## What nodes show

Changing what a node shows is an error for a node of a kind that doesn't show it, such as the
tint of a rect.

### `node:set_sprite`

```lua
node:set_sprite(sprite: Sprite)
```

Changes the sprite a sprite node shows, such as to a frame of an
[animation](graphics.md#animation). It keeps its size: a sprite of another size stretches to it.
On a particles node, it changes what its particles show.

### `node:set_cell`

```lua
node:set_cell(column: number, row: number)
```

Shows the sprite in another cell of the sheet the scene file gave a sprite, billboard, mesh or
particles node, counting from 0 at the top-left. A cell outside the sheet is an error. Clips change cells
too, with a `cell` track.

```lua
-- A face that frowns.
hero:find("head/face"):set_cell(4, 0)
```

### `node:set_tint`

```lua
node:set_tint(tint: Color)
```

Changes the color the colors of a sprite, mesh, model or billboard node are multiplied by.

### `node:set_color`

```lua
node:set_color(color: Color)
```

Changes the color of a rect or text node, or the fill of a panel that isn't showing a sprite.

### `node:text`

```lua
node:text(): string
```

Returns a text node's text.

### `node:set_text`

```lua
node:set_text(text: string)
```

Changes a text node's text, laying it out again if it's different, so setting the same text
every frame costs little.

### `node:blend`

```lua
node:blend(): Blend
```

Returns how a sprite, rect, text, tile layer or particles node mixes with what's under it: one of
the [blend modes](graphics.md#blend-modes).

### `node:set_blend`

```lua
node:set_blend(blend: Blend)
```

Changes how a sprite, rect, text, tile layer or particles node mixes with what's under it.

## Particles

A [particles node](../scenes.md#particles) sends particles out as it's advanced. Its methods are
an error on a node of another kind, apart from `advance_particles`, `particle_count` and
`clear_particles`, which act on every particles node inside the node too, on any kind of node.

### `node:advance_particles`

```lua
node:advance_particles(dt: number)
```

Moves on by `dt` seconds the particles of the node and of every particles node inside it, sending
new ones out from where each node is laid out now, as
[`particles:advance`](graphics.md#particlesadvance) does. Scenes keep no time, so a game advances
its scene's particles in `update`:

```lua
function update(dt: number)
    level:advance_particles(dt)
end
```

Particles advance whether their nodes are visible or not; a hidden node's particles don't draw.

### `node:particle_count`

```lua
node:particle_count(): number
```

Returns how many particles the node and every particles node inside it have alive.

### `node:clear_particles`

```lua
node:clear_particles()
```

Ends the lives of the particles of the node and every particles node inside it at once, as when a
level starts over.

### `node:emit`

```lua
node:emit(count: number)
```

Sends `count` of a particles node's particles out at once from where it's laid out, emitting or
not, as many as its `limit` leaves room for: a burst, such as dust where something lands.

### `node:is_emitting`

```lua
node:is_emitting(): boolean
```

Returns whether a particles node sends particles out at its rate.

### `node:set_emitting`

```lua
node:set_emitting(emitting: boolean)
```

Starts or stops a particles node sending particles out at its rate. Those it sent live out their
lives, so a torch put out dies down.

### `node:particle_settings`

```lua
node:particle_settings(): ParticleSettings
```

Returns a particles node's [settings](graphics.md#particle-settings), every one of them.

### `node:set_particle_settings`

```lua
node:set_particle_settings(settings: ParticleSettings)
```

Changes the settings of a particles node that `settings` has, keeping the rest, as
[`particles:set_settings`](graphics.md#particlesset_settings) does. Its sprite changes with
[`set_sprite`](#nodeset_sprite) and [`set_cell`](#nodeset_cell), and the rectangle particles
start in with [`set_size`](#nodeset_size).

## Nodes in 3D views

The nodes of a [3D view](../scenes.md#3d-views) are placed in 3D, in its world: by `x`, `y` and
`z`, turned by `yaw`, `pitch` and `roll`, and stretched by `scale_x`, `scale_y` and `scale_z`.
These are errors for a node that isn't in a 3D view. Positions, angles and scales must be finite.

### `node:position_3d`

```lua
node:position_3d(): vector
```

Returns where its origin is within its parent.

### `node:set_position_3d`

```lua
node:set_position_3d(position: vector)
```

```lua
-- The hero, where the grid mover has them.
local column, row = mover:position()
hero:set_position_3d(vector.create(column + 0.5, 0, row + 0.5))
```

### `node:orientation`

```lua
node:orientation(): (number, number, number)
```

Returns its yaw, pitch and roll, in radians.

### `node:set_orientation`

```lua
node:set_orientation(yaw: number, pitch: number?, roll: number?)
```

Turns it about its origin: by `roll` about `z`, then `pitch` about `x`, then `yaw` about `y`, each
counter-clockwise seen from that axis's end, so a yaw turns it as seen from above. `pitch` and
`roll` are 0 if left out.

### `node:scale_3d`

```lua
node:scale_3d(): vector
```

### `node:set_scale_3d`

```lua
node:set_scale_3d(scale: number | vector)
```

Stretches it along its own axes by a vector, or by a number along all three.

### `node:world_position_3d`

```lua
node:world_position_3d(): vector
```

Returns where its origin is in its view's world, after every node it's inside has placed it.

### `node:bounds_3d`

```lua
node:bounds_3d(): (vector?, vector?)
```

Returns the least and greatest corners of the box in its view's world around what a mesh or
model shows, moved, turned and stretched as it's drawn, or around a billboard, standing as it faces
the camera; or nothing for a plain node or a light.

### `node:light`

```lua
node:light(): LightSettings
```

Returns the settings of a [light node](../scenes.md#3d-views), as a table: its `color`, a number
`0xRRGGBBAA`, its `range`, `energy`, `edge` and `shadows`, and for a spot light, its `cone`,
which a point light's table doesn't have. An error for a node that isn't a light.

### `node:set_light`

```lua
node:set_light(settings: LightSettings)
```

Changes those of a light node's settings that `settings` gives, any of `color`, `range`, `energy`,
`cone`, `edge` and `shadows`, keeping the rest. A `cone` makes a point light a spot light; a cone
can be widened, up to pi, but not taken away. Where a light is and the way a spot light shines are
its node's, which [`set_position_3d`](#nodeset_position_3d) and
[`set_orientation`](#nodeset_orientation) change.

```lua
-- A torch that flickers.
torch:set_light({ energy = 0.9 + 0.1 * math.sin(time * 13) })
```

Settings a light can't have, such as a `range` of 0 or an `edge` without a `cone`, are an error,
and the light keeps those it had. A node that isn't a light is an error too.

`shadows` is whether the view's meshes and models in the light's way shade what's behind them,
as [`graphics.begin_3d`](graphics.md#graphicsbegin_3d) describes, with what shadows cost.

### `node:casts_shadow`

```lua
node:casts_shadow(): boolean
```

Returns whether a mesh or model node shades what's behind it from lights with `shadows`: `true`
unless its file's `casts_shadow` says not, or [`set_casts_shadow`](#nodeset_casts_shadow) has. An
error for a node of another kind.

### `node:set_casts_shadow`

```lua
node:set_casts_shadow(casts: boolean)
```

Changes whether a mesh or model node shades what's behind it from lights with `shadows`. An error
for a node of another kind.

## 3D views

These work on a `view3d` node.

### `node:camera`

```lua
node:camera(): Camera3d
```

Returns the view's camera, a table as [`graphics.begin_3d`](graphics.md#graphicsbegin_3d) takes,
with every field: `position`, `target`, `up`, `fov` or `height`, `near` and `far`.

### `node:set_camera`

```lua
node:set_camera(camera: Camera3d)
```

Changes the view's camera, whose fields are the defaults where they're left out, as in
[`graphics.begin_3d`](graphics.md#graphicsbegin_3d).

```lua
-- Following the hero from above and behind.
local feet = hero:position_3d()
view:set_camera({ position = feet + vector.create(0, 7, 3.5), target = feet })
```

### `node:lighting`

```lua
node:lighting(): Lighting
```

Returns the view's lighting: `direction`, and `color` and `ambient` as numbers `0xRRGGBBAA`.

### `node:set_lighting`

```lua
node:set_lighting(lighting: Lighting)
```

Changes the view's lighting, its sun and ambient light, whose fields are the defaults where
they're left out, as in [`graphics.begin_3d`](graphics.md#graphicsbegin_3d). A scene's view is lit
at places by its [light nodes](#nodelight), so `lights` here is an error.

### `node:fog`

```lua
node:fog(): Fog?
```

Returns the view's fog, `color` as a number `0xRRGGBBAA`, `near` and `far`, or nothing if it has
none.

### `node:set_fog`

```lua
node:set_fog(fog: Fog?)
```

Changes the view's fog, whole: `color` is white and `near` 0 where they're left out, and it must
have a `far`, more than `near`, as in [`graphics.begin_3d`](graphics.md#graphicsbegin_3d). With
`nil`, the view has none.

```lua
-- The fog closes in as night falls.
view:set_fog({ color = sky, near = 8, far = 48 - 24 * night })
```

### `node:pick`

```lua
node:pick(x: number, y: number): { SceneNode }
```

Returns the nodes of the view under the point `x`, `y` in the scene's coordinates, nearest the
camera first: those that show whose [boxes](#nodebounds_3d) the ray through the point goes into.
A light has no box, so it's never among them. Nothing is under a point outside the view. A scene
drawn without a camera is in canvas pixels, as the mouse is.

```lua
local x, y = input.mouse_position()
local under = if x and y then view:pick(x, y)[1] else nil
```

### `node:project`

```lua
node:project(point: vector): (number?, number?)
```

Returns where `point` in the view's world shows in the scene, or nothing if it's behind the camera
or beyond what it sees, as [`graphics.project`](graphics.md#graphicsproject) does for a view
drawn by hand.

### `node:ray`

```lua
node:ray(x: number, y: number): (vector?, vector?)
```

Returns the ray into the view's world through the point `x`, `y` in the scene's coordinates: where
it starts, on the camera's near plane, and the way it goes, of length 1, as
[`graphics.ray`](graphics.md#graphicsray) does for a view drawn by hand; or nothing for a point
outside the view. Everything that shows at that point lies along it, and a physics world's
[`world:ray`](physics.md#worldray-1) finds what it hits first by the shapes of bodies, where
[`node:pick`](#nodepick) goes by the boxes around what shows:

```lua
-- A click kicks the crate under the mouse.
local x, y = input.mouse_position()
local origin, direction
if x and y then
    origin, direction = view:ray(x, y)
end
if origin and direction and input.pressed("click") then
    local hit = world:ray(origin, direction)
    if hit and hit.node:has_tag("crate") then
        world:push(hit.node, direction * 2)
    end
end
```

## Posing

### `node:pose`

```lua
node:pose(clip: Clip, time: number, weight: number?)
```

Poses the node and the nodes inside it as `clip` has them `time` seconds after it started: each
of the clip's tracks sets a property of the node its path leads to from this one to the value its
keys give then. A clip that loops wraps `time` to its duration; one that doesn't holds its first
pose before it starts and its last after it ends. Properties the clip has no track for keep what
they have.

`weight`, from 0 to 1 and 1 if left out, is how much the clip counts: numbers move only that
fraction of the way from what they are to the clip's, and `visible` and `cell` change only from a
weight of a half. Posing with one clip, then another at a weight rising from 0 to 1, crossfades
from the first to the second:

```lua
-- Every frame: standing, walking as much as the hero is, and blinking now and then.
hero:pose(clips.idle, time)
hero:pose(clips.walk, walk_time, walking)
hero:pose(clips.blink, time)
```

A track whose node isn't there, a `cell` track of a node that isn't a sprite, billboard or mesh
from a sheet with those cells, a `rotation` track of a node in a 3D view, or a `z`, `yaw`,
`pitch`, `roll` or `scale_z` track of one that isn't, is an error, and the pose changes nothing.
`time` and `weight` must be finite.

### `Clip`

A clip of a scene file, from [`scene.clips`](#sceneclips): tracks of keyframes on nodes'
properties, described in [Clips](../scenes.md#clips). Like an [`Animation`](graphics.md#animation),
it doesn't keep time; the game keeps how long it has played. One clip poses any number of nodes.

#### `clip:duration`

```lua
clip:duration(): number
```

Returns how many seconds it lasts.

#### `clip:is_looping`

```lua
clip:is_looping(): boolean
```

Returns whether it starts over at its end, rather than holding its last pose.

#### `clip:is_finished`

```lua
clip:is_finished(time: number): boolean
```

Returns whether a clip that doesn't loop has reached its end, `time` seconds after it started.
One that loops never has.

```lua
if clips.wave:is_finished(time - waved_at) then
    waving = false
end
```

## Adding and removing nodes

### `node:add`

```lua
node:add(source: SceneNode, name: string?): SceneNode
```

Adds a copy of `source` and everything inside it as the last child of `node`, named `name`, or
the name `source` has, and returns the copy. `source` can be from any scene, this one included,
such as a scene's root from [`scene.load`](#sceneload), whose props come with it. A root has no
name, so copying one takes a `name`.

```lua
-- A bat, from a scene of its own, in the level's creatures.
local bat = level:find("creatures"):add(scene.load("bat.scene.luau"), "bat_1")
bat:set_position(48, 96)
```

A name that's empty, holds `/`, or is one of `node`'s children's already is an error, as is
copying a tile layer without its tilemap, a node of a 3D view anywhere but into one, or a node of
the 2D scene into one. Nodes can't nest more than 200 levels below a scene's root, so a copy that
would place one deeper is an error too.

### `node:remove`

```lua
node:remove()
```

Takes the node and everything inside it out of its scene. Using a removed node afterwards is an
error, apart from [`is_removed`](#nodeis_removed) and `tostring`. A scene's root can't be
removed.

### `node:is_removed`

```lua
node:is_removed(): boolean
```

## Tilemaps

These work on a tilemap node, or one of its layers, which stands for its tilemap. Cells are a
column and a row, counting from 0 at the map's top-left corner, which is at the tilemap node's
origin.

### `node:is_solid`

```lua
node:is_solid(column: number, row: number): boolean
```

Returns whether a cell blocks movement: whether it holds a solid tile on any layer, or is outside
the map, as [`map:is_solid`](tilemap.md#mapis_solid) does.

### `node:tile_to_world`

```lua
node:tile_to_world(column: number, row: number): (number, number)
```

Returns the position in the scene of the top-left corner of a cell, which may be a fraction of
the way between cells, as for a [grid mover](grid.md#moverposition) partway through a step.

### `node:world_to_tile`

```lua
node:world_to_tile(x: number, y: number): (number, number)
```

Returns the cell containing the position `x`, `y` in the scene, which may be outside the map.

### `node:grid_size`

```lua
node:grid_size(): (number, number)
```

Returns how many columns and rows the map has. [`size`](#nodesize) is its size in the scene.

### `node:tile_size`

```lua
node:tile_size(): (number, number)
```

Returns the width and height of the map's tiles.

### `node:tile_props`

```lua
node:tile_props(name: string): { [string]: any }
```

Returns the props of the tile named `name` in the map's tileset, as a new table. A name the
tileset doesn't have is an error.

### `node:tile`

```lua
node:tile(column: number, row: number): string?
```

Returns the name of the tile in a cell of this tile layer, or `nil` if the cell is empty or
outside the map.

### `node:set_tile`

```lua
node:set_tile(column: number, row: number, name: string?)
```

Puts the tile named `name` in a cell of this tile layer, or with `nil`, empties it. A cell
outside the map, or a name the tileset doesn't have, is an error.

```lua
-- The gate opens.
level:find("map/objects"):set_tile(20, 2, "gate_open")
```