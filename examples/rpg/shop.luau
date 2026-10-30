-- Shops, where the player buys items and gear for coins, and sells what they carry for half
-- what it cost. `shop.visit` runs a visit as part of a task, through menus in the dialogue box.

local dialogue = require("./dialogue")
local items = require("./items")

local shop = {}

local sale = audio.load("sounds/chime.wav")

-- A menu of the items `names`, each with `detail` beside it, for the shopkeeper `keeper` to ask
-- about. It ends with "Back".
local function menu(names: { string }, detail: (string) -> string, keeper: string)
    local choices, options = items.menu(names, detail)
    table.insert(choices, "Back")
    options.speaker = keeper
    return choices, options
end

local function buy(stock: { string }, keeper: string)
    while true do
        local choices, options = menu(stock, function(name)
            return tostring(items.kind(name).price)
        end, keeper)
        local question = `What would you like? You have {rpg.coins} coins.`
        local picked = dialogue.ask(question, choices, options)
        if picked > #stock then
            return
        end
        local name = stock[picked]
        local item = items.kind(name)
        if rpg.coins < item.price then
            dialogue.say("Oh dear, you're a little short for that one.", keeper)
        else
            local question = `The {item.name}? That'll be {item.price} coins.`
            local yes = dialogue.ask(question, { "Buy it", "No thanks" }, { speaker = keeper })
            if yes == 1 then
                rpg.coins -= item.price
                items.give(name)
                audio.play(sale, { speed = 2 })
                dialogue.say("Thank you kindly!", keeper)
                if item.slot then
                    local offer = `Equip the {item.name} now?`
                    if dialogue.ask(offer, { "Equip it", "Not now" }) == 1 then
                        items.equip(name)
                        dialogue.say(`You equip the {item.name}. Change gear from your bag.`)
                    end
                end
            end
        end
    end
end

local function sell(keeper: string)
    while true do
        local owned = items.owned()
        if #owned == 0 then
            dialogue.say("You've nothing left to sell, dear.", keeper)
            return
        end
        local choices, options = menu(owned, function(name)
            return tostring(items.sell_price(name))
        end, keeper)
        local picked = dialogue.ask("What are you selling?", choices, options)
        if picked > #owned then
            return
        end
        local name = owned[picked]
        local item = items.kind(name)
        local price = items.sell_price(name)
        if items.in_use(name) then
            local refusal = `You're using that {item.name}! Take it off first to sell it.`
            dialogue.say(refusal, keeper)
        else
            local question = `I'll give you {price} coins for your {item.name}.`
            local yes = dialogue.ask(question, { "Sell it", "Keep it" }, { speaker = keeper })
            if yes == 1 then
                items.take(name)
                rpg.coins += price
                audio.play(sale, { speed = 2 })
                dialogue.say("Pleasure doing business.", keeper)
            end
        end
    end
end

-- A visit to `keeper`'s shop, which sells the items `stock`, until the player leaves.
function shop.visit(stock: { string }, keeper: string)
    dialogue.say("Welcome to Mossbrook Goods! Have a look around.", keeper)
    while true do
        local choice = dialogue.ask(
            `What can I do for you? You have {rpg.coins} coins.`,
            { "Buy", "Sell", "Leave" },
            { speaker = keeper }
        )
        if choice == 1 then
            buy(stock, keeper)
        elseif choice == 2 then
            sell(keeper)
        else
            dialogue.say("Come again! And do be careful up on that hill.", keeper)
            return
        end
    end
end

return shop
