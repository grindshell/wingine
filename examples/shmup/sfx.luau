-- Sound effects, by what they're for, and the music.

local sounds = {}

local MUSIC_VOLUME = 0.4

local blip = audio.load("sounds/blip.wav")
local effects = {
    graze = { sound = blip, settings = { volume = 0.25, speed = 1.6 } },
    kill = { sound = blip, settings = { volume = 0.5, speed = 0.7 } },
    hit = { sound = audio.load("sounds/bump.wav"), settings = { volume = 1 } },
    bomb = { sound = audio.load("sounds/chime.wav"), settings = { volume = 0.8, speed = 0.6 } },
    bonus = { sound = audio.load("sounds/chime.wav"), settings = { volume = 0.8 } },
}

-- Seconds between graze sounds, which would otherwise come every frame of a long graze.
local GRAZE_GAP = 0.08
local next_graze = 0

-- The shot's sound: the blip looping, slowed so it ticks once a volley, 20 times a second, for
-- as long as the player shoots.
local VOLLEYS_PER_SECOND = 20
local SHOT_SETTINGS = {
    volume = 0.15,
    speed = blip:duration() * VOLLEYS_PER_SECOND,
    looping = true,
}
-- Seconds the shot's sound takes to fade once the player stops shooting.
local SHOT_FADE = 0.05
-- The shot's sound, while it plays.
sounds.shot = nil :: Voice?

-- Plays the effect `name`.
function sounds.play(name: string)
    local effect = effects[name]
    if name == "graze" then
        if shmup.time < next_graze then
            return
        end
        next_graze = shmup.time + GRAZE_GAP
    end
    audio.play(effect.sound, effect.settings)
end

-- Keeps the shot's sound ticking while `shooting`, starting it if it isn't, and stops it when not.
function sounds.shoot(shooting: boolean)
    local shot = sounds.shot
    if shot and not shot:is_playing() then
        -- Stopped with every other sound.
        shot = nil
    end
    if shooting and not shot then
        shot = audio.play(blip, SHOT_SETTINGS)
    elseif not shooting and shot then
        shot:stop(SHOT_FADE)
        shot = nil
    end
    sounds.shot = shot
end

-- Pauses every sound playing as the game pauses, or with `paused` false, resumes them.
function sounds.pause(paused: boolean)
    if paused then
        audio.pause_sounds()
    else
        audio.resume_sounds()
    end
end

function sounds.start_music()
    audio.set_music_volume(MUSIC_VOLUME)
    audio.play_music("sounds/music.ogg", 1)
end

return sounds
