# `physics`

Collision among a scene's nodes, by [Rapier](https://rapier.rs): nodes made solid where they're
drawn, and characters that walk among them, stopping at what's solid and sliding along it, as
the player walks around the [garden](../../examples/garden). Everything here works any time.

A physics world holds nodes of one [scene](scene.md), each solid where it's drawn, turned,
stretched and mirrored as it is, the area `node:contains` tests, or only along its edges.
Colliders follow their nodes: a node that moves, turns or changes size is solid where it is now,
and one removed from its scene is solid nowhere. Whether a node is shown doesn't matter.

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

Collision is the same to the bit on every platform, so a game that replays inputs, as
[`exact`](exact.md) lets one, plays out the same everywhere.

## Functions

### `physics.world`

```lua
physics.world(): PhysicsWorld
```

Returns an empty physics world. The first node given to it, to `add` or `move`, makes it a world
of that node's scene, and giving it a node of another scene is an error. Nodes in
[3D views](scene.md) have no place in one.

An engine built without its `physics` feature has no physics worlds: `physics.world` fails.

## `PhysicsWorld`

### `world:add`

```lua
world:add(node: SceneNode, options: { edges: boolean? }?)
```

Makes `node` solid where it's drawn: all of what it covers, or with `edges = true`, only its edges,
which keep what's inside in and what's outside out, as a fence around a field does. Only the node
is solid, not the nodes inside it, and a plain node covers nothing, so it's solid nowhere.

Adding a node again changes how solid it is.

### `world:remove`

```lua
world:remove(node: SceneNode)
```

Makes `node` solid nowhere, if it was. Removing a node from its scene does this too.

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
in its way, so a character can be solid, for others to bump into, and still move.
