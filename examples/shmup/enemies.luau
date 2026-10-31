-- Enemies: fairies and the boss. Each runs a script of its own, a coroutine the game resumes once
-- an update with the update's `dt`, so scripts read as steps that `wait` between them. An enemy
-- whose script ends has left; one shot down is gone at once.

local art = require("./art")
local field = require("./field")
local sounds = require("./sounds")

local enemies = {}

type Look = {
    sprite: any,
    tint: number,
    -- The sprite's size on the field, and how close a shot must come to hit.
    size: number,
    radius: number,
    boss: boolean?,
}

local LOOKS: { [string]: Look } = {
    ghost = { sprite = art.ghost, tint = graphics.color("#9be8ff"), size = 16, radius = 7 },
    bat = { sprite = art.bat, tint = graphics.color("#ffb3e6"), size = 16, radius = 7 },
    octopus = { sprite = art.octopus, tint = graphics.color("#c8a2ff"), size = 16, radius = 8 },
    boss = {
        sprite = art.boss,
        tint = graphics.color("#ff7b9c"),
        size = 32,
        radius = 14,
        boss = true,
    },
}

-- Seconds an enemy flashes when hit.
local FLASH_SECONDS = 0.05
local FLASH = graphics.color("#ffffff")
-- How far from a shot's center an enemy is hit.
local SHOT_RADIUS = 4

export type Enemy = {
    look: Look,
    x: number,
    y: number,
    hp: number,
    max_hp: number,
    -- Points for shooting it down.
    points: number,
    script: thread,
    -- Seconds left of flashing, and of shots passing through it harmlessly.
    flash: number,
    shielded: number,
    boss: boolean,
}

-- Resumes `routine`, raising its error with a traceback if it fails. Returns whether it's still
-- running.
function enemies.resume(routine: thread, ...): boolean
    local ok, err = coroutine.resume(routine, ...)
    if not ok then
        error(debug.traceback(routine, tostring(err)), 0)
    end
    return coroutine.status(routine) ~= "dead"
end

-- Puts an enemy that looks like `look` at `x`, `y` with `hp` hit points, worth `points`, and starts
-- its `script`, which is passed the enemy.
function enemies.spawn(
    look: string,
    x: number,
    y: number,
    hp: number,
    points: number,
    script: (Enemy) -> ()
): Enemy
    local enemy: Enemy = {
        look = LOOKS[look],
        x = x,
        y = y,
        hp = hp,
        max_hp = hp,
        points = points,
        script = coroutine.create(script),
        flash = 0,
        shielded = 0,
        boss = LOOKS[look].boss == true,
    }
    table.insert(shmup.enemies, enemy)
    if not enemies.resume(enemy.script, enemy) then
        table.remove(shmup.enemies, table.find(shmup.enemies, enemy))
    end
    return enemy
end

-- Runs each enemy's script on by `dt`, removing enemies whose scripts have ended.
function enemies.update(dt: number)
    local list = shmup.enemies
    local i = 1
    while i <= #list do
        local enemy = list[i]
        enemy.flash = math.max(enemy.flash - dt, 0)
        enemy.shielded = math.max(enemy.shielded - dt, 0)
        if enemies.resume(enemy.script, dt) then
            i += 1
        else
            table.remove(list, i)
        end
    end
end

-- Takes `amount` hit points from `enemy`. A fairy with none left is shot down; the boss's spell
-- cards watch its hit points themselves.
local function damage(enemy: Enemy, amount: number)
    if enemy.shielded > 0 or enemy.hp <= 0 then
        return
    end
    enemy.hp = math.max(enemy.hp - amount * (shmup.cheats.damage or 1), 0)
    enemy.flash = FLASH_SECONDS
    if enemy.hp <= 0 and not enemy.boss then
        shmup.score += enemy.points
        table.remove(shmup.enemies, table.find(shmup.enemies, enemy))
        sounds.play("kill")
    end
end

-- Hits the first enemy a shot at `x`, `y` touches, for `amount` damage. Returns whether it hit one.
function enemies.shoot(x: number, y: number, amount: number): boolean
    for _, enemy in shmup.enemies do
        local reach = enemy.look.radius + SHOT_RADIUS
        local dx, dy = x - enemy.x, y - enemy.y
        if dx * dx + dy * dy < reach * reach then
            damage(enemy, amount)
            return true
        end
    end
    return false
end

-- Takes `amount` hit points from every enemy on the field, as a bomb does.
function enemies.damage_all(amount: number)
    -- A copy, since shooting enemies down removes them.
    for _, enemy in table.clone(shmup.enemies) do
        if field.contains(enemy.x, enemy.y) then
            damage(enemy, amount)
        end
    end
end

-- Whether an enemy's body touches the circle at `x`, `y` of `radius`.
function enemies.touching(x: number, y: number, radius: number): boolean
    for _, enemy in shmup.enemies do
        local reach = enemy.look.radius * 0.6 + radius
        local dx, dy = x - enemy.x, y - enemy.y
        if dx * dx + dy * dy < reach * reach then
            return true
        end
    end
    return false
end

-- For scripts: waits for the next update, and returns its `dt`.
function enemies.frame(): number
    return coroutine.yield()
end

-- For scripts: moves `enemy` to `x`, `y` over `seconds`, easing in and out.
function enemies.move_to(enemy: Enemy, x: number, y: number, seconds: number)
    local start_x, start_y = enemy.x, enemy.y
    local time = 0
    while time < seconds do
        time += enemies.frame()
        local t = math.min(time / seconds, 1)
        local eased = t * t * (3 - 2 * t)
        enemy.x = start_x + (x - start_x) * eased
        enemy.y = start_y + (y - start_y) * eased
    end
end

-- For scripts: moves `enemy` at `vx`, `vy` pixels a second for `seconds`.
function enemies.drift(enemy: Enemy, vx: number, vy: number, seconds: number)
    local time = 0
    while time < seconds do
        local dt = enemies.frame()
        time += dt
        enemy.x += vx * dt
        enemy.y += vy * dt
    end
end

function enemies.draw()
    -- Under steady fire, enemies flicker rather than stay white.
    local flicker = math.floor(shmup.time / FLASH_SECONDS) % 2 == 0
    for _, enemy in shmup.enemies do
        local look = enemy.look
        local tint = if enemy.flash > 0 and flicker then FLASH else look.tint
        local half = look.size / 2
        graphics.draw_sprite(look.sprite, enemy.x - half, enemy.y - half, tint, look.size, look.size)
    end
end

return enemies
