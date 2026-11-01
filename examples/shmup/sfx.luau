-- Sound effects, by what they're for, and the music.

local sounds = {}

local MUSIC_VOLUME = 0.4

local effects = {
    graze = { sound = audio.load("sounds/blip.wav"), settings = { volume = 0.25, speed = 1.6 } },
    kill = { sound = audio.load("sounds/blip.wav"), settings = { volume = 0.5, speed = 0.7 } },
    hit = { sound = audio.load("sounds/bump.wav"), settings = { volume = 1 } },
    bomb = { sound = audio.load("sounds/chime.wav"), settings = { volume = 0.8, speed = 0.6 } },
    bonus = { sound = audio.load("sounds/chime.wav"), settings = { volume = 0.8 } },
}

-- Seconds between graze sounds, which would otherwise come every frame of a long graze.
local GRAZE_GAP = 0.08
local next_graze = 0

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

function sounds.start_music()
    audio.set_music_volume(MUSIC_VOLUME)
    audio.play_music("sounds/music.ogg", 1)
end

return sounds
