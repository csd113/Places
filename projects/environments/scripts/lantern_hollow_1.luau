--!strict
-- Each neighbor has an independent three-line conversation cycle.
local next_line = {}
local residents = {
    ["editor_prop_145"] = {name="ADA", lines={
        "Come in. There is always room for one more in my garden.",
        "You make this little street feel brighter just by visiting.",
        "Take your time today. Even flowers need a quiet afternoon.",
    }},
    ["editor_prop_146"] = {name="BEN", lines={
        "The best part of baking is having someone to share it with.",
        "Whatever brought you here, I am glad you knocked.",
        "You deserve something warm after a long walk.",
    }},
    ["editor_prop_147"] = {name="MIRA", lines={
        "You do not need a good story to be welcome here.",
        "I hope someone listens to you as kindly as you listen to others.",
        "Sit for a moment. Your company is enough.",
    }},
    ["editor_prop_148"] = {name="THEO", lines={
        "If your day has come apart, we can mend one small piece.",
        "Thank you for stopping by. Little visits matter more than you know.",
        "You have already made a difference somewhere today.",
    }},
    ["editor_prop_149"] = {name="ELIAS", lines={
        "There is a spare chair, and no hurry to turn the next page.",
        "Every person brings a story worth making time for.",
        "May your next chapter have a little more peace in it.",
    }},
    ["editor_prop_150"] = {name="NORA", lines={
        "I was looking for a cheerful colour. Your visit will do.",
        "You notice lovely things. I hope you notice them in yourself too.",
        "There is room in the picture for a day that goes your way.",
    }},
    ["editor_prop_151"] = {name="OWEN", lines={
        "The birds are singing again. I saved you the quiet window.",
        "Thank you for walking gently through our little corner of the world.",
        "I hope something small and wonderful finds you on the way home.",
    }},
    ["editor_prop_152"] = {name="LENA", lines={
        "The kettle is on. You can leave your worries at the door.",
        "You never have to earn a friendly welcome here.",
        "I hope the rest of your day is as pleasant as your visit.",
    }},
    ["editor_prop_153"] = {name="RUTH", lines={
        "A stitch at a time is plenty. You are allowed to go slowly.",
        "I am glad our paths crossed. You have a kind presence.",
        "May you always find a warm place when the world feels cold.",
    }},
    ["editor_prop_154"] = {name="SAM", lines={
        "A familiar street feels new when you meet someone kind.",
        "You are welcome to walk beside me whenever you need company.",
        "I hope the road ahead brings you safely to someone you love.",
    }},
}
return {
    resident = function(api, event)
        local entry = residents[event.subject]
        if entry then
            local index = (next_line[event.subject] or 0) % 3 + 1
            next_line[event.subject] = index
            api.message(entry.name, {entry.lines[index]})
        end
    end,
}
