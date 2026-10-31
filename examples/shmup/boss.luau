-- The boss, and its attacks: an opening, then three spell cards. Each has hit points and a time
-- limit, and ends when either runs out, clearing the field of bullets, each worth points. A spell
-- card shot down without the player being hit or bombing pays its bonus, which shrinks as time
-- passes.

local art = require("./art")
local enemies = require("./enemies")
local field = require("./field")
local patterns = require("./patterns")
local player = require("./player")
local sounds = require("./sounds")

local boss = {}

local pellet, orb, rice = art.kinds.pellet, art.kinds.orb, art.kinds.rice

-- Seconds the boss can't be hurt at the start of each attack, and rests between them.
local SHIELD_SECONDS = 1.5
local REST_SECONDS = 1
-- Where the boss attacks from.
local HOME_X, HOME_Y = field.WIDTH / 2, 80

-- An aimed fan of rice and turning rings of orbs.
local function opening(enemy)
    local angle = 0
    while true do
        patterns.ring(orb.blue, enemy.x, enemy.y, 20, angle, 90)
        angle += 0.15
        for _ = 1, 3 do
            patterns.aimed(rice.red, enemy.x, enemy.y, 5, 0.6, 170)
            wait(0.3)
        end
        enemies.move_to(enemy, HOME_X + patterns.random(-60, 60), HOME_Y + patterns.random(-20, 20), 0.6)
    end
end

-- Rings that burst out fast and slow to a crawl, curling one way and then the other like petals.
local function petals(enemy)
    local colors = { pellet.red, pellet.orange, pellet.yellow, pellet.purple }
    local angle = 0
    local ring = 0
    while true do
        ring += 1
        local curl = if ring % 2 == 0 then 0.35 else -0.35
        patterns.ring(colors[ring % #colors + 1], enemy.x, enemy.y, 28, angle, 180, -150, curl, 40)
        angle += 0.2
        wait(0.3)
        if ring % 8 == 0 then
            patterns.aimed(orb.white, enemy.x, enemy.y, 3, 0.3, 120)
        end
    end
end

-- Two arms of rice spiraling out each way, the bullets bending as they go, and aimed orbs.
local function helix(enemy)
    local angle = 0
    local volley = 0
    while true do
        volley += 1
        for arm = 0, 1 do
            local a = angle + arm * math.pi
            shmup.bullets.spawn(rice.cyan, enemy.x, enemy.y, a, 110, 1, 0, 0, 0.5)
            shmup.bullets.spawn(rice.blue, enemy.x, enemy.y, -a, 110, 1, 0, 0, -0.5)
        end
        angle += 0.19
        if volley % 40 == 0 then
            patterns.aimed(orb.green, enemy.x, enemy.y, 5, 0.8, 100)
        end
        wait(0.05)
    end
end

-- Needles thrown up that fall back as rain, while orbs fan out at the player.
local function rain(enemy)
    local volley = 0
    while true do
        volley += 1
        for _ = 1, 6 do
            local x = enemy.x + patterns.random(-70, 70)
            -- Pointing down but moving up, they slow, then fall faster and faster.
            local angle = math.pi / 2 + patterns.random(-0.3, 0.3)
            local speed = -patterns.random(110, 160)
            shmup.bullets.spawn(rice.white, x, enemy.y, angle, speed, 1, 0, 240, 0, -math.huge, 150)
        end
        if volley % 10 == 0 then
            patterns.aimed(orb.purple, enemy.x, enemy.y, 7, 0.9, 110)
        end
        wait(0.12)
        if volley % 30 == 0 then
            enemies.move_to(enemy, HOME_X + patterns.random(-70, 70), HOME_Y + patterns.random(-10, 30), 0.8)
        end
    end
end

export type Card = {
    -- A spell card's name; the opening has none.
    name: string?,
    hp: number,
    -- Seconds before it times out.
    time: number,
    -- The bonus for shooting it down at once.
    bonus: number?,
    attack: (any) -> (),
}

local CARDS: { Card } = {
    { hp = 800, time = 30, attack = opening },
    { name = 'Bloom Sign "Hundred-Petal Ring"', hp = 1200, time = 45, bonus = 1000000, attack = petals },
    { name = 'Helix Sign "Twin Spiral"', hp = 1400, time = 45, bonus = 1500000, attack = helix },
    { name = 'Rain Sign "Falling Needles"', hp = 1600, time = 50, bonus = 2000000, attack = rain },
}
boss.CARDS = CARDS

-- The bonus a spell card pays now: all of it at the start, half by the time it times out.
function boss.bonus(card): number
    local left = math.max(card.time_left, 0) / card.time
    return math.floor(card.bonus * (0.5 + 0.5 * left) / 10) * 10
end

-- Runs `card` until the boss's hit points or the time run out, then clears the field.
local function run_card(enemy, card: Card, number: number)
    enemy.hp, enemy.max_hp = card.hp, card.hp
    enemy.shielded = SHIELD_SECONDS
    local state = {
        number = number,
        name = card.name,
        time = card.time,
        time_left = card.time,
        bonus = card.bonus,
        -- Whether the player was hit or bombed, which forfeits the bonus.
        failed = false,
    }
    shmup.card = state
    local attack = coroutine.create(card.attack)
    local running = enemies.resume(attack, enemy)
    while enemy.hp > 0 and state.time_left > 0 do
        local dt = enemies.frame()
        state.time_left -= dt
        if running then
            running = enemies.resume(attack, dt)
        end
    end
    shmup.score += shmup.bullets.clear() * player.CLEAR_POINTS
    if state.bonus and enemy.hp <= 0 and not state.failed then
        local bonus = boss.bonus(state)
        shmup.score += bonus
        shmup.show("Spell Card Bonus!", string.format("%d", bonus))
        sounds.play("bonus")
    end
    shmup.card = nil
end

-- The boss's script: it flies in, runs its cards in turn, and is gone when the last ends.
function boss.script(enemy)
    enemies.move_to(enemy, HOME_X, HOME_Y, 2)
    for number, card in boss.CARDS do
        run_card(enemy, card, number)
        wait(REST_SECONDS)
        if number < #boss.CARDS then
            enemies.move_to(enemy, HOME_X, HOME_Y, 0.5)
        end
    end
    sounds.play("kill")
end

return boss
