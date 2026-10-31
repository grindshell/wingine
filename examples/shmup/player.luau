-- The player: moving, focusing, shooting and bombing, and being hit or grazing bullets.

local art = require("./art")
local enemies = require("./enemies")
local field = require("./field")
local sounds = require("./sounds")

local player = {}

-- Pixels a second the player moves, and while focused.
player.SPEED = 170
player.FOCUS_SPEED = 70
-- How close to the field's edges the player goes.
local MARGIN = 10
-- How close a bullet's edge must come to the player's center to hit them, and to graze them.
player.HITBOX = 2
player.GRAZE = 16
player.START_X = field.WIDTH / 2
player.START_Y = field.HEIGHT - 48
-- The extra lives a game starts with, and the bombs each life starts with.
player.LIVES = 2
player.BOMBS = 3
-- Seconds between volleys of shots, pixels a second shots fly, and the damage each does.
local SHOT_INTERVAL = 0.05
local SHOT_SPEED = 600
local SHOT_DAMAGE = 1
-- How far apart the three shots of a volley start, and the angles they fan out at.
local SHOT_GAP = 6
local FOCUSED_SHOT_GAP = 4
local SHOT_FAN = 0.12
-- Seconds the player is gone after being hit, and then can't be hit again.
local DEATH_SECONDS = 1
local RESPAWN_SECONDS = 3
-- Seconds a bomb lasts, clearing bullets and keeping the player safe, and the damage it does to
-- every enemy on the field.
player.BOMB_SECONDS = 2
local BOMB_DAMAGE = 40
-- Points for each bullet a bomb clears, each shot that hits, and each graze.
player.CLEAR_POINTS = 10
local HIT_POINTS = 10
player.GRAZE_POINTS = 100
-- Seconds between blinks while the player can't be hit.
local BLINK = 0.1

local TINT = graphics.color("#ffe6a8")
local FLASH = graphics.color("#ffffff")

export type Player = {
    x: number,
    y: number,
    lives: number,
    bombs: number,
    -- Seconds left of being gone after a hit, of not being hittable, of a bomb, and until the
    -- next volley.
    dead: number,
    invincible: number,
    bombing: number,
    reload: number,
    focused: boolean,
}

function player.new(): Player
    return {
        x = player.START_X,
        y = player.START_Y,
        lives = player.LIVES,
        bombs = player.BOMBS,
        dead = 0,
        invincible = 0,
        bombing = 0,
        reload = 0,
        focused = false,
    }
end

-- A volley of three shots, close together when focused and fanned out when not.
local function fire(p: Player)
    local gap = if p.focused then FOCUSED_SHOT_GAP else SHOT_GAP
    local fan = if p.focused then 0 else SHOT_FAN
    for i = -1, 1 do
        local angle = -math.pi / 2 + i * fan
        table.insert(shmup.shots, {
            x = p.x + i * gap,
            y = p.y - 8,
            vx = math.cos(angle) * SHOT_SPEED,
            vy = math.sin(angle) * SHOT_SPEED,
        })
    end
end

-- Uses a bomb, if the player has one and isn't using one already.
function player.bomb()
    local p = shmup.player
    if p.bombs <= 0 or p.bombing > 0 or p.dead > 0 then
        return
    end
    p.bombs -= 1
    p.bombing = player.BOMB_SECONDS
    p.invincible = math.max(p.invincible, player.BOMB_SECONDS)
    enemies.damage_all(BOMB_DAMAGE)
    if shmup.card then
        shmup.card.failed = true
    end
    sounds.play("bomb")
end

-- Moves the player with the keys, fires while shoot is held, and bombs when bomb is pressed.
function player.update(dt: number)
    local p = shmup.player
    p.invincible = math.max(p.invincible - dt, 0)
    p.bombing = math.max(p.bombing - dt, 0)
    if p.dead > 0 then
        p.dead -= dt
        if p.dead <= 0 then
            if p.lives < 0 then
                shmup.mode = "over"
            else
                p.x, p.y = player.START_X, player.START_Y
                p.invincible = RESPAWN_SECONDS
            end
        end
        return
    end

    local dx, dy = 0, 0
    if input.held("left") then
        dx -= 1
    end
    if input.held("right") then
        dx += 1
    end
    if input.held("up") then
        dy -= 1
    end
    if input.held("down") then
        dy += 1
    end
    p.focused = input.held("focus")
    local speed = if p.focused then player.FOCUS_SPEED else player.SPEED
    if dx ~= 0 and dy ~= 0 then
        speed /= math.sqrt(2)
    end
    p.x = math.clamp(p.x + dx * speed * dt, MARGIN, field.WIDTH - MARGIN)
    p.y = math.clamp(p.y + dy * speed * dt, MARGIN, field.HEIGHT - MARGIN)

    p.reload -= dt
    if input.held("shoot") and p.reload <= 0 then
        fire(p)
        p.reload = SHOT_INTERVAL
    end
    if input.pressed("bomb") then
        player.bomb()
    end
    -- A bomb keeps clearing bullets while it lasts.
    if p.bombing > 0 then
        shmup.score += shmup.bullets.clear() * player.CLEAR_POINTS
    end
end

-- Moves the player's shots, which hit the first enemy they touch.
function player.update_shots(dt: number)
    local shots = shmup.shots
    local i = 1
    while i <= #shots do
        local shot = shots[i]
        shot.x += shot.vx * dt
        shot.y += shot.vy * dt
        if not field.contains(shot.x, shot.y, 16) then
            table.remove(shots, i)
        elseif enemies.shoot(shot.x, shot.y, SHOT_DAMAGE) then
            shmup.score += HIT_POINTS
            table.remove(shots, i)
        else
            i += 1
        end
    end
end

-- Loses a life, clearing the bullets. Bombs come back for the next life.
function player.die()
    local p = shmup.player
    p.lives -= 1
    p.dead = DEATH_SECONDS
    p.bombs = player.BOMBS
    p.bombing = 0
    shmup.bullets.clear()
    shmup.shots = {}
    if shmup.card then
        shmup.card.failed = true
    end
    sounds.play("hit")
end

-- Checks the player against the bullets and enemies: grazes score, and a hit costs a life.
function player.collide()
    local p = shmup.player
    if p.dead > 0 then
        return
    end
    local hit, grazes = shmup.bullets.collide(p.x, p.y, player.HITBOX, player.GRAZE)
    if grazes > 0 then
        shmup.graze += grazes
        shmup.score += grazes * player.GRAZE_POINTS
        sounds.play("graze")
    end
    hit = hit or enemies.touching(p.x, p.y, player.HITBOX)
    if hit and p.invincible <= 0 and not shmup.cheats.invincible then
        player.die()
    end
end

function player.draw()
    for _, shot in shmup.shots do
        graphics.draw_sprite(art.shot, shot.x - 8, shot.y - 8)
    end
    local p = shmup.player
    if p.dead > 0 or p.lives < 0 then
        return
    end
    -- Blinks while it can't be hit.
    if p.invincible > 0 and math.floor(p.invincible / BLINK) % 2 == 0 then
        return
    end
    local tint = if p.bombing > 0 then FLASH else TINT
    graphics.draw_sprite(art.player, p.x - 8, p.y - 8, tint)
    if p.focused then
        graphics.draw_sprite(art.hitbox, p.x - 4, p.y - 4)
    end
end

return player
