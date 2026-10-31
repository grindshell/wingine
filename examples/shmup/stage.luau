-- The stage: waves of fairies, then the boss. `stage.run` is a script the game resumes once an
-- update, so the waves read in the order they come.

local art = require("./art")
local boss = require("./boss")
local enemies = require("./enemies")
local field = require("./field")
local patterns = require("./patterns")

local stage = {}

local pellet, orb, rice = art.kinds.pellet, art.kinds.orb, art.kinds.rice

-- Points for shooting down each kind of fairy, and their hit points.
local SMALL_POINTS, SMALL_HP = 1000, 6
local LARGE_POINTS, LARGE_HP = 10000, 120

-- A ghost that glides down to `stop_y`, fires at the player twice, and glides off to the side
-- it's facing.
local function ghost(stop_y: number, away: number)
    return function(enemy)
        enemies.move_to(enemy, enemy.x, stop_y, 1)
        for _ = 1, 2 do
            patterns.aimed(pellet.cyan, enemy.x, enemy.y, 3, 0.4, 130)
            wait(0.6)
        end
        enemies.drift(enemy, away * 90, -30, 4)
    end
end

-- A bat that swoops down across the field, bursting into a ring of pellets halfway.
local function bat(vx: number)
    return function(enemy)
        enemies.drift(enemy, vx, 110, 1)
        patterns.ring(pellet.red, enemy.x, enemy.y, 12, patterns.aim(enemy.x, enemy.y), 100)
        enemies.drift(enemy, vx, 50, 1)
        patterns.ring(pellet.orange, enemy.x, enemy.y, 12, patterns.aim(enemy.x, enemy.y), 80)
        enemies.drift(enemy, vx * 1.5, -40, 4)
    end
end

-- A large fairy that comes to a stop and fires a spiral of rice, then leaves.
local function octopus(enemy)
    enemies.move_to(enemy, enemy.x, 90, 1.5)
    local angle = 0
    for volley = 1, 70 do
        for arm = 0, 2 do
            shmup.bullets.spawn(rice.green, enemy.x, enemy.y, angle + arm * 2 * math.pi / 3, 100)
        end
        angle += 0.23
        if volley % 14 == 0 then
            patterns.aimed(orb.yellow, enemy.x, enemy.y, 3, 0.5, 120)
        end
        wait(0.07)
    end
    enemies.drift(enemy, 0, -60, 3)
end

-- Waits until every enemy has left or been shot down, or `seconds` have passed.
local function wait_clear(seconds: number)
    local time = 0
    while #shmup.enemies > 0 and time < seconds do
        time += enemies.frame()
    end
end

-- The fairies' waves.
local function waves()
    wait(1.5)
    -- Ghosts down the left, then the right.
    for i = 0, 4 do
        enemies.spawn("ghost", 40 + i * 24, -12, SMALL_HP, SMALL_POINTS, ghost(70 + i * 10, -1))
        wait(0.35)
    end
    wait(2)
    for i = 0, 4 do
        local x = field.WIDTH - 40 - i * 24
        enemies.spawn("ghost", x, -12, SMALL_HP, SMALL_POINTS, ghost(70 + i * 10, 1))
        wait(0.35)
    end
    wait(2.5)
    -- Bats swooping across from both sides.
    for i = 0, 5 do
        local from_left = i % 2 == 0
        local x = if from_left then -12 else field.WIDTH + 12
        enemies.spawn("bat", x, 20 + i * 8, SMALL_HP, SMALL_POINTS, bat(if from_left then 120 else -120))
        wait(0.5)
    end
    wait_clear(4)
    -- A large fairy, with ghosts on either side.
    enemies.spawn("octopus", field.WIDTH / 2, -16, LARGE_HP, LARGE_POINTS, octopus)
    wait(1.5)
    for i = 0, 3 do
        enemies.spawn("ghost", 30, -12, SMALL_HP, SMALL_POINTS, ghost(50 + i * 25, -1))
        enemies.spawn("ghost", field.WIDTH - 30, -12, SMALL_HP, SMALL_POINTS, ghost(50 + i * 25, 1))
        wait(0.8)
    end
    wait_clear(8)
    wait(1)
end

-- Runs the stage: the waves, unless `practice` is "boss", then the boss.
function stage.run(practice: string?)
    if practice ~= "boss" then
        waves()
    end
    shmup.show("Warning!", "A powerful presence approaches")
    wait(2)
    local the_boss = enemies.spawn("boss", field.WIDTH / 2, -24, 1, 0, boss.script)
    shmup.boss = the_boss
    while table.find(shmup.enemies, the_boss) do
        wait()
    end
    shmup.boss = nil
    wait(1.5)
    shmup.finish()
end

return stage
