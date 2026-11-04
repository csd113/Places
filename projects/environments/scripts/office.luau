--!strict
local active, done = false, false
local received = { false, false, false }
local papers = { "office_scrap_1", "office_scrap_2", "office_scrap_3" }
local groups = { "office_page_1", "office_page_2", "office_page_3" }
local reported = -1
local function refresh(api)
    local count = 0
    for i, id in papers do
        local page = api.actor(id)
        -- A unique filing receipt survives later bumps or removal of that page.
        received[i] = received[i] or (page.physical and not page.held and page.sleeping
            and page.x > -1.46 and page.x < -0.94
            and page.z > 9.89 and page.z < 10.31
            and page.y > 7.25 and page.y < 7.64)
        if received[i] then count += 1 end
    end
    return count
end
local function talk(api)
    if done then
        api.message("THE NIGHT ARCHIVIST", {
            "Three less things to carry into tomorrow.",
            "Office favor complete. The Hub has your stamp.",
            "Return using the Hub elevator in the entry foyer.",
        })
        return
    end
    active = true
    reported = refresh(api)
    api.message("THE NIGHT ARCHIVIST", {
        "Three loose pages: one on each floor, including the basement.",
        "Upstairs: first desk. Middle: tilted desk by the west landing.",
        "Basement: abandoned desk in the eastern workroom. F helps there.",
        "Bring each to the wide recycling crate in the entry foyer.",
        "E sets a held page down. Let it settle inside; leave good things.",
        "Filed: " .. tostring(reported) .. " of 3. F1 gives the next location.",
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
        reported = -1
    end,
    start = function(api)
        done = api.completed("office")
        if done then api.animate("office_archivist", "pose_arms_up", true) end
    end,
    talk = talk,
    hint = function(api)
        if done then talk(api) return end
        local count = refresh(api)
        local nextPage = "Upstairs: first desk north of reception."
        if received[1] then
            nextPage = "Middle floor: tilted desk beside the northwest landing."
            if received[2] then
                nextPage = "Basement: abandoned desk in the eastern workroom."
            end
        end
        api.message("OFFICE - " .. tostring(count) .. " OF 3 FILED", {
            "Speak to the night archivist beside reception upstairs.",
            nextPage,
            "East ramp: upstairs to middle. West ramp: middle to basement.",
            "Return pages to the wide crate in the top-floor entry foyer.",
            "E picks up or sets down. The crate button recovers unfiled pages.",
            "Recovery keeps filed pages. Mugs, books and other items do not count.",
        })
    end,
    recover = function(api)
        if done then return end
        reported = refresh(api)
        for i, group in groups do
            if not received[i] then api.physics(group, false) end
        end
        api.message("PAGE RECOVERY", {
            "Requested unfiled pages back at their original desks.",
            "Already-filed pages keep their credit.",
            "If a desk is blocked, step clear and press the button again.",
        })
    end,
    update = function(api)
        if not active or done then return end
        local count = refresh(api)
        if count == 3 then
            done = true
            api.complete()
            api.animate("office_archivist", "pose_arms_up", true)
            api.message("THE NIGHT ARCHIVIST", {
                "All three are in. Thank you for leaving the good things.",
                "You brought the last page up from the basement, too.",
                "Office favor complete. The Hub will remember.",
                "Return using the Hub elevator beside the foyer crate.",
            })
        elseif count ~= reported then
            reported = count
            api.message("OFFICE RECYCLING", {
                "Filed: " .. tostring(count) .. " of 3 pages.",
                "There is one page on each floor. F1 gives the next location.",
                "The crate button recovers unfiled pages without losing these.",
            })
        end
    end,
}
