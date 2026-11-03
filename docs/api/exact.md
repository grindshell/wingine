# `exact`

Math that gives the same bits on every platform, for games whose state must play out the same
everywhere, such as to replay recorded inputs, check a speedrun, or keep two players' games in
step over a network.

`math.sin`, `math.cos` and the rest of `math`'s functions below come from the C library Luau is
built with, and Windows' and Linux's round some results differently in their last bits. Those
bits add up: the same game, given the same inputs, can drift apart from one platform to another.
`exact`'s functions are written in Rust (the [`libm`](https://crates.io/crates/libm) crate) and
come out the same everywhere.

| Function | Takes |
|---|---|
| `exact.sin(x)`, `exact.cos(x)`, `exact.tan(x)` | An angle in radians |
| `exact.asin(x)`, `exact.acos(x)` | A number from -1 to 1 |
| `exact.atan(x)` | Any number |
| `exact.atan2(y, x)` | A point's coordinates, for the angle to it |
| `exact.sinh(x)`, `exact.cosh(x)`, `exact.tanh(x)` | Any number |
| `exact.exp(x)` | Any number |
| `exact.log(x, base)` | A positive number, and optionally a base (by default, e) |
| `exact.log10(x)` | A positive number |
| `exact.pow(x, y)` | `x` to the power of `y` |

Each takes what `math`'s function of the same name takes.

```lua
-- A launch angle turned by the player's stick, the same on every platform.
local radians = math.rad(angle)
fighter.vx = exact.cos(radians) * speed
fighter.vy = -exact.sin(radians) * speed
```

## When to use them

Use `exact` for what the game's state depends on, and `math` for what's only for show, such as a
shake, a bob or a particle's wobble. `math`'s are quicker: Luau calls them without leaving its VM,
while each call to one of `exact`'s costs about five times as much, some 30 nanoseconds more. That
only matters in code that calls them thousands of times a frame.

The rest of Luau's arithmetic is the same on every platform already, because IEEE 754 defines it
exactly: `+`, `-`, `*`, `/`, `math.sqrt`, `math.floor`, `math.ceil`, `math.round`, `math.abs`,
`math.min`, `math.max`, `math.clamp`, `math.rad` and `math.deg`, and `//` and `%`, which are
built from them (`a % b` is `a - math.floor(a / b) * b`). So is `math.random`, given the same
seed. The exception is `x ^ y`, which uses the C library's `pow`: write `exact.pow(x, y)`
instead. `x ^ 2`, `x ^ 3` and `x ^ 0.5` with those numbers written in are fine as they are, since
Luau turns them into multiplying and `math.sqrt`.

[Pong](../../examples/pong) has nothing random in it, so the same inputs play the same match, and
it takes its ball's angles from `exact`, so it plays the same match on every platform: its tests
play a whole match and check it to the bit on Windows and Linux.
