-- The story so far: conversations, and events at particular places. Maps refer to them by name.
--
-- The quest moves through `rpg.quest`: "start", until the player agrees to help the elder;
-- "accepted", until they've seen the shrine; "seen_shrine", until they tell the elder; "hunt",
-- while creatures roam Shrine Hill, until the player beats Gloamfang at the shrine; "cleared",
-- until they tell the elder; then "done".

local dialogue = require("./dialogue")
local items = require("./items")
local shop = require("./shop")
local world = require("./world")

local story = {
    -- Conversations, run when the player talks to a character, who is passed in.
    talk = {},
    -- Events, run when the player examines a place that has one, which is passed in.
    events = {},
    -- The stages of the quest, in order.
    STAGES = { "start", "accepted", "seen_shrine", "hunt", "cleared", "done" },
}

-- Whether the quest has reached `stage`, or gone past it.
function story.reached(stage: string): boolean
    local wanted = table.find(story.STAGES, stage) or error(`there's no stage called {stage}`)
    local now = table.find(story.STAGES, rpg.quest) or error(`the quest is at {rpg.quest}`)
    return now >= wanted
end

local ELDER = "Elder Rowan"
local HAZEL = "Hazel"
-- Herbs the elder gives the player for the hunt.
local HERBS = 3
-- What Hazel's shop sells, once it's open for the hunt.
local STOCK = {
    "herb",
    "tonic",
    "oak_staff",
    "wooden_shield",
    "swift_feather",
    "ward_charm",
    "iron_sword",
    "bronze_shield",
}
local chime = audio.load("sounds/chime.wav")

-- A function that says text in the dialogue box as `speaker`, waiting for the player to close
-- it.
local function voice(speaker: string?): (string) -> ()
    return function(text: string)
        dialogue.say(text, speaker)
    end
end

local elder = voice(ELDER)
local hazel = voice(HAZEL)
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
            { speaker = ELDER }
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
        elder(
            "Whatever the stone held is loose, and its brood will be crawling all over the hill "
                .. "by now."
        )
        rpg.items.herb = items.count("herb") + HERBS
        rpg.quest = "hunt"
        narrate(`Elder Rowan gives you {HERBS} herbs.`)
        elder(
            "Eat one if you're hurt. Drive those creatures off the hill, and find what broke the "
                .. "seal."
        )
        elder("And see Hazel at the shop. Her cart's back from the city, with gear for you.")
    elseif rpg.quest == "hunt" then
        elder("The creatures are on Shrine Hill. If you're hurt, drink from the well first.")
    elseif rpg.quest == "cleared" then
        elder("The chimes! I heard them from here. You did it!")
        elder("Mossbrook owes you a great deal. Rest now; you've more than earned it.")
        rpg.quest = "done"
    else
        elder("Rest while you can. Who knows what else that stone was holding back.")
    end
end

function story.talk.maple()
    if story.reached("cleared") then
        maple("Were those chimes I heard last night? It's good to hear them again.")
    elseif story.reached("seen_shrine") then
        maple("You went up the hill? You look like you've seen a ghost.")
    else
        maple("Lovely morning for a walk! Though it's been awfully quiet at night lately.")
    end
end

-- Hazel keeps Mossbrook Goods, which opens once the hunt is on.
function story.talk.hazel()
    if not story.reached("hunt") then
        hazel("Sorry, love, we're shut. My cart's still on its way back from the city.")
    else
        shop.visit(STOCK, HAZEL)
    end
end

function story.talk.pip()
    if rpg.quest == "start" then
        pip("The ducks won't go near the north end of the pond lately. I think they're scared.")
    else
        pip("Are you going up to the shrine? Grown-ups never let me go.")
    end
end

-- After the player loses a battle, and wakes up beside the elder.
function story.revived()
    elder("Easy now. Maple found you at the foot of the hill and carried you home.")
    if items.count("herb") == 0 then
        items.give("herb")
        elder("You're out of herbs, too. Here, take one of mine.")
    end
    elder("Get your strength back, then try again. The creatures won't have gone anywhere.")
end

-- After the player beats a creature of the kind `kind`.
function story.defeated(kind: string)
    if kind ~= "gloamfang" then
        return
    end
    local shrine = rpg.levels.shrine
    world.clear_foes(shrine)
    rpg.quest = "cleared"
    audio.play(chime)
    narrate("A breeze stirs the shrine's wind chimes… and, for the first time in days, they ring.")
    narrate("The other creatures scatter into the trees. Shrine Hill is quiet again.")
    -- The hill hums the village's tune again.
    shrine.music = rpg.levels.village.music
    world.play_music(shrine, 2)
end

-- Drinking from the village well restores the player.
function story.events.well()
    local max_hp = items.stats().max_hp
    if rpg.hp < max_hp then
        rpg.hp = max_hp
        audio.play(chime, { speed = 1.5 })
        narrate("You drink from the well. The cold water clears your head and restores you.")
    else
        narrate("The village well. The rope is new; the stones are very old.")
    end
end

-- The sign outside Hazel's shop, which is shut until the hunt.
function story.events.shop_sign()
    local sign = "“Mossbrook Goods: tools, rope and remedies.”"
    if story.reached("hunt") then
        narrate(`{sign} The door is propped open.`)
    else
        narrate(`{sign} It's shut today.`)
    end
end

function story.events.seal_stone()
    if story.reached("cleared") then
        narrate("The halves of the seal stone have drawn back together. A hairline crack remains.")
    elseif rpg.quest == "accepted" then
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
