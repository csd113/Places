--!strict
local active, done = false, false
local received = { false, false, false }
local papers = { "office_scrap_1", "office_scrap_2", "office_scrap_3" }
local function talk(api)
    if done then
        api.message("THE NIGHT ARCHIVIST", {"Three less things to carry into tomorrow.", "At last, I can take my break."})
        return
    end
    active = true
    local count = 0
    for _, present in received do if present then count += 1 end end
    api.message("THE NIGHT ARCHIVIST", {
        "The upstairs desks are littered with three loose pages.",
        "Drop them into the open waste crate beside reception.",
        "Let each page settle inside; mugs and books stay out.",
        "Filed for recycling: " .. tostring(count) .. " of 3.",
        "The crate's restore button recovers the three pages.",
    })
end
return {
    checkpoint_state = function()
        return {active=active, done=done, page1=received[1], page2=received[2], page3=received[3]}
    end,
    restore_state = function(saved)
        for _, key in {"active", "done", "page1", "page2", "page3"} do
            if type(saved[key]) ~= "boolean" then error("Invalid office checkpoint state") end
        end
        active, done = saved.active, saved.done
        received = {saved.page1, saved.page2, saved.page3}
    end,
    start = function(api) done = api.completed("office") end,
    talk = talk,
    hint = function(api) api.message("OFFICE", { "The night archivist waits by reception on the top floor.", "The three loose pages belong in the open waste crate." }) end,
    recover = function(api)
        if done then return end
        received = { false, false, false }
        api.physics("office_scraps", false)
        api.message("THE NIGHT ARCHIVIST", {"All three pages are back on their desks.", "The recycling count has been cleared for this attempt."})
    end,
    update = function(api)
        if not active or done then return end
        local count = 0
        for i, id in papers do
            local page = api.actor(id)
            -- The physical crate has an open mouth and a real bottom.
            received[i] = page.physical and not page.held and page.sleeping
                and page.x > 6.65 and page.x < 7.15
                and page.z > 6.80 and page.z < 7.20
                and page.y > 7.21 and page.y < 7.48
            if received[i] then count += 1 end
        end
        if count == 3 then
            done = true
            api.complete()
            api.animate("office_archivist", "pose_arms_up", true)
            api.message("THE NIGHT ARCHIVIST", {
                "All three are in. Thank you for leaving the good things.",
                "Even an endless office can have an end to its day.",
                "Office favor complete. The Hub will remember.",
            })
        end
    end,
}
