-- The story so far: conversations, and events at particular places. Maps refer to them by name.
--
-- The quest moves through `rpg.quest`: "start", until the player agrees to help the elder;
-- "accepted", until they've seen the shrine; "seen_shrine", until they tell the elder; then
-- "reported".

local dialogue = require("./dialogue")
local world = require("./world")

local story = {
    -- Conversations, run when the player talks to a character, who is passed in.
    talk = {},
    -- Events, run when the player examines a place that has one, which is passed in.
    events = {},
}

local ELDER = "Elder Rowan"

local chime = audio.load("sounds/chime.wav")

-- A function that says text in the dialogue box as `speaker`, waiting for the player to close
-- it.
local function voice(speaker: string?): (string) -> ()
    return function(text: string)
        dialogue.say(text, speaker)
    end
end

local elder = voice(ELDER)
local maple = voice("Maple")
local pip = voice("Pip")
local narrate = voice(nil)

function story.talk.elder()
    if rpg.quest == "start" then
        elder("Ah, there you are. I was hoping you'd pass this way.")
        elder(
            "Three nights ago, the wind chimes at the Stone Shrine fell silent. They've rung "
                .. "every night since before I was born."
        )
        local answer = dialogue.ask(
            "Would you climb the hill and see what's become of the shrine?",
            { "I'll go.", "Not yet." },
            ELDER
        )
        if answer == 1 then
            rpg.quest = "accepted"
            world.replace(rpg.levels.village, "gate", "gate_open")
            audio.play(chime)
            elder(
                "Thank you. I've unlatched the north gate. Follow the path up the hill, and go "
                    .. "carefully."
            )
        else
            elder("I understand. Come and find me when you're ready.")
        end
    elseif rpg.quest == "accepted" then
        elder("The shrine is up the path, through the north gate. Go carefully.")
    elseif rpg.quest == "seen_shrine" then
        elder("You're back! What did you find?")
        elder("The seal stone… split in two? Then it's as I feared.")
        elder("Whatever the stone held beneath the hill is stirring. We'll need to be ready for it.")
        rpg.quest = "reported"
        elder("Rest tonight. Tomorrow, we'll talk about what comes next.")
    else
        elder("Rest while you can. Tomorrow, we prepare.")
    end
end

function story.talk.maple()
    if rpg.quest == "seen_shrine" or rpg.quest == "reported" then
        maple("You went up the hill? You look like you've seen a ghost.")
    else
        maple("Lovely morning for a walk! Though it's been awfully quiet at night lately.")
    end
end

function story.talk.pip()
    if rpg.quest == "start" then
        pip("The ducks won't go near the north end of the pond lately. I think they're scared.")
    else
        pip("Are you going up to the shrine? Grown-ups never let me go.")
    end
end

function story.events.seal_stone()
    if rpg.quest == "accepted" then
        audio.play(chime, { speed = 0.5 })
        narrate(
            "The seal stone stands at the heart of the shrine… split clean in two, from crown to "
                .. "root."
        )
        narrate("Deep scratches score the ground around it, as if something clawed its way out.")
        rpg.quest = "seen_shrine"
        narrate("Elder Rowan needs to hear about this.")
    else
        narrate(
            "The two halves of the seal stone lean apart. Cold air seeps from the crack between "
                .. "them."
        )
    end
end

return story
