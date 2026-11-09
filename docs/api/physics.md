# `physics`

Physics among a scene's nodes, by [Rapier](https://rapier.rs). In 2D: nodes made solid where
they're drawn and tile maps where their tiles are, characters seen from above that slide along
what they bump into, as the player walks around the [garden](../../examples/garden), and bodies
that fall, roll and stack, with characters seen from the side walking among them, as the player
climbs out of the [quarry](../../examples/quarry). In [3D views](scene.md), bodies that fall,
tumble and come to rest, and characters that walk up stairs and slopes among them, pushing what's
in their way, as the player runs around the [course](../../examples/course). In both, rays,
overlaps and sensors tell where things are. Everything here works any time.

Physics is the same to the bit on every platform, so a game that replays inputs, as
[`exact`](exact.md) lets one, plays out the same everywhere.

## In 2D

A physics world holds nodes of one [scene](scene.md), each solid where it's drawn, turned,
stretched and mirrored as it is, the area `node:contains` tests, or only along its edges. A tile
map is solid where its tiles are [solid](scene.md), on any of its layers, and a tile layer where
its own are. Colliders follow their nodes: a node that moves, turns or changes size is solid
where it is now, a tile map whose tiles a script sets is solid where they are now, and a node
removed from its scene is solid nowhere. Whether a node is shown doesn't matter.

Seen from above, as in the garden, a character slides among them with `world:move`:

```lua
local level = scene.load("level.scene.luau")
local player = level:find("player")

-- Everything tagged `solid` is in the way, and the edges of the ground keep the player on it.
local world = physics.world()
for _, wall in level:tagged("solid") do
    world:add(wall)
end
world:add(level:find("ground"), { edges = true })

-- What of the player bumps into things: its feet, around its position.
local FEET = { x = -4, y = -2, width = 8, height = 2 }
local SPEED = 64

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })

function update(dt: number)
    local dx = input.axis("left", "right")
    local dy = input.axis("up", "down")
    if dx ~= 0 or dy ~= 0 then
        -- As fast diagonally as straight.
        local step = SPEED * dt / math.sqrt(dx * dx + dy * dy)
        world:move(player, dx * step, dy * step, FEET)
    end
end

function draw()
    graphics.clear("#000000")
    graphics.draw_scene(level)
end
```

### Bodies and characters in 2D

Nodes can have bodies, as in 3D: fixed, the default, dynamic or kinematic.

- **Dynamic** bodies fall, slide, roll, bounce and stack, as `world:step` moves the world on, and
  they move and turn their nodes. Each is a box, what its node covers, stretched as it is, or a
  circle inside that, as wide as its widest side. A script that moves a dynamic body's node puts
  the body there.
- **Kinematic** bodies go where their nodes are put, pushing dynamic bodies out of the way and
  carrying what stands on them, as a lift does.
- A **character** is a kinematic box walked by `world:walk`, standing on its node's position. It
  walks along the ground, up steps and slopes it can climb, down them without leaving the ground,
  and along with what it stands on, a lift or a crate, as that moves, pushing dynamic bodies it
  walks into. The game keeps how fast it's falling, and makes it jump.

`world:move` slides a node without pushing anything, bodies included, as a character seen from
above walks; `world:walk` is for characters seen from the side, which stand on things and push
them.

A scene's units are usually pixels, which Rapier, tuned for metres, needs told about: a world's
`metre` is how many of them make one, 16 by default, so a crate 16 pixels across is a metre
across and, at a density of 1, weighs 1. Gravity pulls down the scene at 9.81 metres a second
each second, 157 pixels at 16 to a metre, unless the world says otherwise; games usually want
more, and those seen from above, none.

```lua
local level = scene.load("level.scene.luau")
local player = level:find("player")

local GRAVITY = 600
local world = physics.world({ gravity = GRAVITY })
-- The level's tile map is solid where its tiles are.
world:add(level:find("map"))
for _, crate in level:tagged("crate") do
    world:add(crate, { body = "dynamic", friction = 0.6 })
end
world:add(level:find("boulder"), { body = "dynamic", shape = "circle" })
world:add(level:find("lift"), { body = "kinematic" })
world:add(player, { character = { width = 8, height = 14, step = 4 } })

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("jump", { "Space" })

local SPEED = 90
local JUMP = 230
-- How fast the player is falling, in pixels a second: rising while it's less than 0.
local falling = 0
local grounded = false

function update(dt: number)
    if grounded and input.pressed("jump") then
        falling = -JUMP
    end
    falling += GRAVITY * dt
    local across = input.axis("left", "right") * SPEED * dt
    local _, _, on_ground = world:walk(player, across, falling * dt)
    grounded = on_ground
    if grounded then
        falling = 0
    end
    world:step(dt)
end

function draw()
    graphics.clear("#000000")
    graphics.draw_scene(level)
end
```

A body that moves may be inside nodes that move it, but not ones that turn or stretch it, so it's
placed only by where it is and how it's turned. Fixed nodes can be anywhere. Bodies go when their
nodes are removed from their scene.

### Where things are in 2D

The world answers questions about where things are, by its colliders, in the scene's
coordinates:

- [`world:ray`](#worldray) finds the collider a ray hits first, for line of sight, or how far
  something can be thrown.
- [`world:overlap_circle`](#worldoverlap_circle) and [`world:overlap_box`](#worldoverlap_box) find
  what an area touches.
- A **sensor**, a node added with `sensor = true`, is solid nowhere, so nothing bumps into it,
  but [`world:inside`](#worldinside) tells which nodes' colliders are in it: a doorway, a trap,
  the area beside a sign.

They find only nodes in the world, so a character a sensor should notice is added too, solid
where it's drawn; its own collider isn't in its way as it moves. They find nodes where they are
now, moved by scripts since the last step too, but a kinematic body that isn't a sensor, a
character's among them, where the last step took it until the next takes it to its node. They
never find sensors, and return nodes in the order they were added. The garden throws seeds this
way:

```lua
world:add(player)
for _, area in level:tagged("note") do
    world:add(area, { sensor = true })
end

-- A seed thrown from the player's feet lands 24 pixels ahead, or 6 short of what's in its way,
-- and grows only if nothing solid is within 5 of it.
local x, y = player:position()
local hit = world:ray(x, y, facing_x, facing_y, { distance = 24, except = player })
local reach = if hit then math.max(hit.distance - 6, 0) else 24
local seed_x, seed_y = x + facing_x * reach, y + facing_y * reach
if #world:overlap_circle(seed_x, seed_y, 5, { except = player }) == 0 then
    plant(seed_x, seed_y)
end

-- The note of the area the player is in.
for _, area in level:tagged("note") do
    for _, node in world:inside(area) do
        if node == player then
            show(area:props().text)
        end
    end
end
```

## In 3D

A 3D physics world holds nodes of one 3D view, each with a body shaped like what it shows: a
mesh's triangles, or a model's. Bodies are fixed, dynamic or kinematic:

- **Fixed** bodies don't move: the ground, walls, stairs. They're solid along their triangles, by
  default, so stairs are stairs and an arch can be walked under, and they follow their nodes if a
  script moves them.
- **Dynamic** bodies fall, tumble, bounce and come to rest, as `world:step` moves the world on,
  and they move their nodes: a script reads where a crate is from its node. They're convex: the
  shape around their triangles, by default, or the box around them, or a ball. A script that moves
  a dynamic body's node puts the body there.
- **Kinematic** bodies go where their nodes are put, pushing dynamic bodies out of the way, as a
  lift or a door does.

A **character** is a kinematic body walked by `world:walk`: an upright cylinder, standing on its
flat feet. It walks across the ground, up stairs and slopes it can climb, down them without
leaving the ground, and along walls, pushing dynamic bodies it walks into. The game keeps how fast
it's falling, and makes it jump. A script that moves a character's node itself, rather than
walking it, puts its body there at once, as when it starts over, without sweeping through what's
between.

Units are the view's, which Rapier is tuned for as metres: a crate half a unit across is half a
metre, and gravity pulls down at 9.81 units a second each second unless the world says otherwise.
Bodies go when their nodes are removed from their scene. Whether a node is shown doesn't matter.

```lua
local level = scene.load("level.scene.luau")
local view = level:find("view")
local player = view:find("player")

local world = physics.world_3d()
for _, node in view:tagged("solid") do
    world:add(node)
end
for _, crate in view:tagged("crate") do
    world:add(crate, { body = "dynamic", shape = "box" })
end
world:add(player, { character = { radius = 0.2, height = 0.7, step = 0.3 } })

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })
input.bind("jump", { "Space" })

local SPEED = 2.5
local JUMP = 5
local GRAVITY = 15
-- How fast the player is rising, or falling, in units a second.
local rising = 0
local grounded = false

function update(dt: number)
    if grounded and input.pressed("jump") then
        rising = JUMP
    end
    rising -= GRAVITY * dt
    local across = input.axis("left", "right") * SPEED * dt
    local along = input.axis("up", "down") * SPEED * dt
    local _, on_ground = world:walk(player, vector.create(across, rising * dt, along))
    grounded = on_ground
    if grounded then
        rising = 0
    end
    world:step(dt)
end

function draw()
    graphics.clear("#a8c8e8")
    local feet = player:position_3d()
    view:set_camera({ position = feet + vector.create(0, 4, 5), target = feet })
    graphics.draw_scene(level)
end
```

A dynamic body's node, and a kinematic one's, may be inside nodes that move it, but not ones that
turn or stretch it, so a body that moves is placed only by where it is and how it's turned. Fixed
bodies can be anywhere.

### Where things are in 3D

The world answers questions about where things are, by bodies' shapes, a mesh's or a model's
triangles where they are, so the ground under a character on a staircase is the step it's over,
not the box around the stairs:

- [`world:ray`](#worldray-1) finds what a ray hits first, for line of sight, shooting, or what's
  under something or the mouse ([`view:ray`](scene.md#noderay) starts one there).
- [`world:overlap_ball`](#worldoverlap_ball) and [`world:overlap_box`](#worldoverlap_box-1) find
  what an area touches, for a blast or a pickup.
- A **sensor** is a body nothing bumps into, an area such as a doorway or a pit, which
  [`world:inside`](#worldinside-1) tells what's in. Dynamic bodies fall through it, and characters
  walk through it.

Queries find bodies where their nodes are now, moved by scripts since the last step too, but a
kinematic body that isn't a sensor, a character's among them, is where the last step took it
until the next takes it to its node. They never find sensors, and return nodes in the order they
were added to the world.

```lua
local DOWN = vector.create(0, -1, 0)
local pit = view:find("pit")
world:add(pit, { sensor = true, shape = "box" })

function update(dt: number)
    -- ... walking the player, then:
    world:step(dt)
    -- The player's shadow, on whatever is below it.
    local hit = world:ray(player:position_3d() + vector.create(0, 0.5, 0), DOWN, { except = player })
    shadow:set_visible(hit ~= nil)
    if hit then
        shadow:set_position_3d(hit.point + hit.normal * 0.01)
    end
    -- Into the pit, the player starts again.
    for _, node in world:inside(pit) do
        if node == player then
            player:set_position_3d(start)
        end
    end
end
```

## Functions

### `physics.world`

```lua
physics.world(options: { metre: number?, gravity: number? }?): PhysicsWorld
```

Returns an empty 2D physics world, with `metre` of the scene's units to a metre, 16 by default,
which Rapier tunes itself for and densities are of a square of, and whose dynamic bodies fall
down the scene with `gravity`, in its units a second each second: by default 9.81 metres' worth,
as Earth's gravity does. `metre` must be more than 0. The first node given to it, to `add` or
`move`, makes it a world of that node's scene, and giving it a node of another scene is an error.
Nodes in 3D views have no place in one: they go in a world from `physics.world_3d`.

### `physics.world_3d`

```lua
physics.world_3d(options: { gravity: vector? }?): PhysicsWorld3d
```

Returns an empty 3D physics world, whose dynamic bodies fall with `gravity`, 9.81 down by default.
The first node given to it makes it a world of the 3D view that node is in, and giving it a node
of another view, or one that isn't in a view, is an error.

An engine built without its `physics` feature has no physics worlds: both functions fail.

## `PhysicsWorld`

### `world:add`

```lua
world:add(node: SceneNode, options: ColliderOptions?)
```

Makes `node` solid where it's drawn: all of what it covers, or with `edges = true`, only its edges,
which keep what's inside in and what's outside out, as a fence around a field does. With
`sensor = true`, none of it is solid: it's a sensor, all of whose area
[`world:inside`](#worldinside) tells what's in, which can't also be only its edges. Only the node
is in the world, not the nodes inside it, and a plain node covers nothing, so it's nowhere.

A tile map is solid where its solid tiles are, on any of its layers, and a tile layer where its
own are, merged into as few boxes as its rows make, so what slides along a floor has no seams to
catch on. They take no options.

`options`, all of them optional:

| Option | |
|---|---|
| `edges` | Only its edges are solid when `true`. Only for a fixed box. |
| `sensor` | Makes it a sensor when `true`: an area nothing bumps into. A sensor is fixed or kinematic, not dynamic. |
| `body` | `"fixed"` (the default), `"dynamic"` or `"kinematic"`. |
| `shape` | `"box"`, what it covers, stretched as it is, the default; or `"circle"`, the circle inside that as wide as its widest side. A fixed box is turned with its node as it's drawn; other bodies are turned by the body. |
| `density` | How heavy each square metre of it is: 1 by default. With its shape, it makes a dynamic body's mass. |
| `friction` | How much it grips what slides along it, from 0 for ice: 0.5 by default. |
| `bounce` | How much it bounces, from 0, not at all, the default, to 1, as fast as it hit. |
| `character` | Makes it a character, which `world:walk` walks, of a kinematic box: a table of how it's sized and walks, below. |

A character's table, all of it optional:

| Option | |
|---|---|
| `width` | How wide its box is: by default, as wide as what its node covers. |
| `height` | How tall it is, up from its node's position, its feet: by default, as tall as what its node covers. |
| `step` | The highest it steps up, as onto a stair: a third of its height by default. |
| `slope` | The steepest slope it walks up and stands on, in radians: 45 degrees (`math.pi / 4`) by default. Steeper slopes are walls to it. |
| `mass` | How heavy it is, for how hard it pushes dynamic bodies: 1 by default. |

A character's box is never turned, so a character can waddle or mirror without its feet changing.
A plain node covers nothing to shape a body by, but can be a character with a `width` and a
`height`.

Adding a node again changes how solid it is, or gives it a new body.

### `world:remove`

```lua
world:remove(node: SceneNode)
```

Makes `node` solid nowhere, if it was. Removing a node from its scene does this too.

### `world:step`

```lua
world:step(dt: number)
```

Moves the world on by `dt` seconds, a frame's `update` time: dynamic bodies fall, roll, push
each other and come to rest, moving and turning their nodes, and kinematic bodies move to where
their nodes are. Colliders first follow their nodes, if scripts have moved them or the nodes
they're in. Steps are taken as in 3D: those longer than a sixtieth of a second in equal parts,
and the same every update with an `update_rate` of 60.

### `world:walk`

```lua
world:walk(node: SceneNode, dx: number, dy: number): (number, number, boolean)
```

Walks the character `node` by up to `dx`, `dy`, and returns how far it moved and whether it's on
the ground. `dy` is down the scene, so a character falls by a `dy` more than 0. On the ground,
and not rising, it's first carried along with what it stands on, as far as that moved where it
stands in the last step, across, up or down, as a lift carries it, then walks across by `dx`: up
steps no higher than its `step`, up and down slopes no steeper than its `slope`, staying on the
ground as it goes down them, and sliding along walls and steeper slopes; or off a ledge, falling
by `dy`. Off the ground, or rising, it moves as `dx` and `dy` say, landing on what it can stand
on, sliding down what's too steep, and stopping at ceilings. It stops a hundredth of a unit short
of what it hits, and its own body isn't in its way.

Dynamic bodies it walks into are pushed along at the speed it walks, taken over the last step's
time, so call it in `update`, with `world:step` after. A script that moves a character's node
itself, rather than walking it, puts its body there at once.

### `world:velocity`

```lua
world:velocity(node: SceneNode): (number, number)
```

Returns how fast `node`'s body is moving, in units a second, across and down. A fixed node's
isn't.

### `world:set_velocity`

```lua
world:set_velocity(node: SceneNode, vx: number, vy: number)
```

Sets how fast `node`'s dynamic body is moving. Bodies of other kinds don't change.

### `world:push`

```lua
world:push(node: SceneNode, x: number, y: number)
```

Pushes `node`'s dynamic body through its middle, changing its velocity by the impulse `x`, `y`
over its mass, as a kick does. Bodies of other kinds don't change.

### `world:is_resting`

```lua
world:is_resting(node: SceneNode): boolean
```

Returns whether `node`'s body is at rest: always for a fixed one, for a kinematic one while it
isn't moving or turning, and for a dynamic one once it has been still for a couple of seconds,
after which the world leaves it alone until something touches it.

Using `walk`, `velocity`, `set_velocity`, `push`, `is_resting` or `inside` on a node that isn't in
the world is an error, as is walking a node that isn't a character.

### `world:move`

```lua
world:move(node: SceneNode, dx: number, dy: number, box: Rect): (number, number)
```

Moves `node` by up to `dx`, `dy` in the scene's coordinates, as if it were `box`, a rectangle
around its position, and returns how far it moved. It moves all the way unless `box` hits
something solid, where it stops, then slides along what it hit as far as the move goes that way,
so a player walking into a wall at a slant carries on along it. It stops a hundredth of a unit
short of what it hits.

`box` is in the scene's units, from the node's position, and doesn't turn or stretch with the
node, so a character can waddle or mirror without its feet changing. It must be wider and taller
than 0, and the move finite. The node moves within its parent by however much the parent turns
and stretches it, so it moves `dx`, `dy` in the scene wherever it is.

A box that starts inside something solid moves only out of it or along it. The node itself isn't
in its way, so a character can be solid, for others to bump into and sensors to find, and still
move. Sensors aren't in its way either. Bodies are, and it doesn't push them: a character seen
from the side, which does, walks with [`world:walk`](#worldwalk).

### `world:ray`

```lua
world:ray(x: number, y: number, dx: number, dy: number, options: RayOptions?): RayHit?
```

Casts a ray from `x`, `y` going `dx`, `dy` in the scene's coordinates, and returns where it first
hits a collider, or `nil` if it hits nothing:

| Field | |
|---|---|
| `node` | The node whose collider it hit. |
| `distance` | How far along the ray, in lengths of `dx`, `dy`, so in the scene's units for a direction of length 1. |
| `x`, `y` | Where it hit. |
| `normal_x`, `normal_y` | The way out of the collider there, of length 1, facing back along the ray. |

A ray that starts inside a node solid all over hits it at once, at a `distance` of 0, with a
normal of zero, and edges are hit from either side, so a ray inside a fence hits it. `options` are
the 3D world's: `distance`, how far it goes, and `except`, a node or a list of them whose
colliders it goes through. The direction can't be zero. Sensors are never hit.

### `world:overlap_circle`

```lua
world:overlap_circle(x: number, y: number, radius: number, options: { except: (SceneNode | { SceneNode })? }?): { SceneNode }
```

Returns the nodes whose colliders the circle around `x`, `y` touches, even by an edge, in the
order they were added, but those `except` names and sensors. Edges count only where the circle
touches them, not if it's inside them. `radius` must be more than 0.

### `world:overlap_box`

```lua
world:overlap_box(box: Rect, options: { except: (SceneNode | { SceneNode })? }?): { SceneNode }
```

Returns the nodes whose colliders `box`, a rectangle in the scene's coordinates, touches, as
[`world:overlap_circle`](#worldoverlap_circle) does. It must be wider and taller than 0.

### `world:inside`

```lua
world:inside(sensor: SceneNode): { SceneNode }
```

Returns the nodes whose colliders are in the sensor `sensor`'s area, even partly, other than
sensors, in the order they were added. It's asked when it's called, so a node a script puts in it
is in it at once. Asking what's in a node that isn't in the world, or isn't a sensor, is an error.

## `PhysicsWorld3d`

### `world:add`

```lua
world:add(node: SceneNode, options: BodyOptions?)
```

Gives `node` a body, or a new one in place of the one it has. `options`, all of them optional:

| Option | |
|---|---|
| `body` | `"fixed"` (the default), `"dynamic"` or `"kinematic"`. |
| `shape` | What the body is shaped as, from what the node shows, stretched as the node is: `"triangles"`, solid along them, the default for fixed bodies and only for them; `"hull"`, the convex shape around them, the default for the rest; `"box"`, the box around them; or `"ball"`, the ball inside that box as wide as its widest side. |
| `density` | How heavy each unit of its volume is: 1 by default. With its shape, it makes a dynamic body's mass. |
| `friction` | How much it grips what slides along it, from 0 for ice: 0.5 by default. |
| `bounce` | How much it bounces, from 0, not at all, the default, to 1, as fast as it hit. |
| `character` | Makes it a character, which `world:walk` walks, of a kinematic body: a table of how it's sized and walks, below. |
| `sensor` | Makes it a sensor when `true`: an area nothing bumps into, which [`world:inside`](#worldinside-1) tells what's in. A sensor is fixed or kinematic, not dynamic, and solid all through, so its shape is `"hull"` by default, or `"box"` or `"ball"`, but not `"triangles"`. A kinematic sensor goes where its node is put at once, since it pushes nothing. |

A character's table, all of it optional:

| Option | |
|---|---|
| `radius` | How far it reaches out from its middle: by default, half the widest the box around what its node shows is, across the ground. |
| `height` | How tall it is: by default, as tall as that box. |
| `step` | The highest it steps up, as onto a stair: a third of its height by default. |
| `slope` | The steepest slope it walks up and stands on, in radians: 45 degrees (`math.pi / 4`) by default. Steeper slopes are walls to it. |
| `mass` | How heavy it is, for how hard it pushes dynamic bodies: 1 by default. A heavy character pushes a light crate along at its own speed, and a light one a heavy crate slowly. |

Only meshes and models show something to shape a body by. A plain node, or a billboard, can be a
character with a `radius` and a `height`.

### `world:remove`

```lua
world:remove(node: SceneNode)
```

Takes away `node`'s body, if it has one. Removing a node from its scene does this too.

### `world:step`

```lua
world:step(dt: number)
```

Moves the world on by `dt` seconds, a frame's `update` time: dynamic bodies fall, tumble, push
each other and come to rest, moving their nodes, and kinematic bodies move to where their nodes
are. Bodies first follow their nodes, if scripts have moved them or the nodes they're in. Steps
longer than a sixtieth of a second are taken in equal parts, so bodies don't pass through each
other, and kinematic bodies go an equal part of the way in each; a game whose
`conf.luau` sets an `update_rate` of 60 steps the same each update, so a run replays exactly.

### `world:walk`

```lua
world:walk(node: SceneNode, motion: vector): (vector, boolean)
```

Walks the character `node` by up to `motion`, and returns how far it moved and whether it's on
the ground. On the ground, and not rising, it walks across by `motion.x` and `motion.z`: up stairs
no higher than its `step`, up and down slopes no steeper than its `slope`, staying on the ground
as it goes down them, and with what it stands on as that goes down, as a lift does, and sliding
along walls and steeper slopes; or off a ledge, falling by `motion.y`. Off the ground, or rising, it moves as `motion` says, landing on what it can stand on,
sliding down what's too steep, and stopping at ceilings. It stops a five-hundredth of a unit
short of what it hits, and its own body isn't in its way.

Dynamic bodies it walks into are pushed along at the speed it walks, taken over the last step's
time, so call it in `update`, with `world:step` after.

### `world:velocity`

```lua
world:velocity(node: SceneNode): vector
```

Returns how fast `node`'s body is moving, in units a second.

### `world:set_velocity`

```lua
world:set_velocity(node: SceneNode, velocity: vector)
```

Sets how fast `node`'s dynamic body is moving. Bodies of other kinds don't change.

### `world:push`

```lua
world:push(node: SceneNode, impulse: vector)
```

Pushes `node`'s dynamic body through its middle, changing its velocity by `impulse` over its mass,
as a kick does. Bodies of other kinds don't change.

### `world:is_resting`

```lua
world:is_resting(node: SceneNode): boolean
```

Returns whether `node`'s body is at rest: always for a fixed body, for a kinematic one while it
isn't moving or turning, and for a dynamic one once it has been still for a couple of seconds, after which
the world leaves it alone until something touches it.

### `world:ray`

```lua
world:ray(origin: vector, direction: vector, options: RayOptions?): RayHit3d?
```

Casts a ray from `origin` going `direction`, and returns where it first hits a body, or `nil` if
it hits nothing:

| Field | |
|---|---|
| `node` | The node whose body it hit. |
| `distance` | How far along the ray, in lengths of `direction`, so in world units for a direction of length 1, as [`view:ray`](scene.md#noderay) gives. |
| `point` | Where it hit, in the view's world. |
| `normal` | The way out of the body there, of length 1, facing back along the ray. |

It hits a body by its shape: triangles where they are, from either side, and a hull, a box or a
ball anywhere on or inside it, so a ray that starts inside one hits it at once, at a `distance` of
0, with a `normal` of zero. `options`, both optional:

| Option | |
|---|---|
| `distance` | How far it goes, in lengths of `direction`: as far as there is, by default. |
| `except` | A node, or a list of them, whose bodies it goes through, such as the one looking. |

`direction` can't be zero. Sensors are never hit.

### `world:overlap_ball`

```lua
world:overlap_ball(center: vector, radius: number, options: { except: (SceneNode | { SceneNode })? }?): { SceneNode }
```

Returns the nodes whose bodies the ball around `center` touches, even by an edge, in the order they
were added, but those `except` names and sensors. Triangles count only where the ball touches
them, not if it's inside what they're around. `radius` must be more than 0.

```lua
-- A blast, pushing every crate near it away.
for _, node in world:overlap_ball(at, 2) do
    if node:has_tag("crate") then
        world:push(node, vector.normalize(node:position_3d() - at) * 3)
    end
end
```

### `world:overlap_box`

```lua
world:overlap_box(min: vector, max: vector, options: { except: (SceneNode | { SceneNode })? }?): { SceneNode }
```

Returns the nodes whose bodies the box between the corners `min` and `max` touches, along the
world's axes, as [`world:overlap_ball`](#worldoverlap_ball) does. The corners can be given either
way round.

### `world:inside`

```lua
world:inside(sensor: SceneNode): { SceneNode }
```

Returns the nodes whose bodies are in the sensor `sensor`'s, even partly, other than sensors, in
the order they were added. It's asked when it's called, so a node put in a sensor by a script is
in it at once, and one that passed through it between two calls never was.

Using `velocity`, `set_velocity`, `push`, `is_resting`, `walk` or `inside` on a node with no body
in the world is an error, as is walking a node that isn't a character, and asking what's inside
a body that isn't a sensor.
