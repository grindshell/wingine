-- Gameplay scripts for the sprites example. The scene runs `bat` as a task from the start, and
-- `read_note` when the hero reads the note on the house's door.
--
-- Besides the engine's built-ins (`wait`, `wait_until`, `spawn` and `print`), the scene gives
-- scripts these functions:
--
--   say(text)           shows text in the dialogue box, and waits until the player closes it
--   bat_fly(direction)  starts the bat flying one tile "up", "down", "left" or "right"
--   play_sound(name)    plays a sound effect: "chime"

function read_note()
    play_sound("chime")
    say("A note on the door reads: “Gone to find the key. Don’t wait up!”")
    say("A bat circles overhead… perhaps it knows where the key went.")
end

-- The bat circles the house, resting between flaps.
function bat()
    local loop = { "right", "right", "down", "down", "left", "left", "up", "up" }
    while true do
        for _, direction in loop do
            bat_fly(direction)
            wait(0.5)
        end
    end
end
