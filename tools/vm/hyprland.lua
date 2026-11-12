-- The guest's Hyprland: the host's look, from ~/.config/hypr/configs when
-- vm.sh found it there, on the VM's one display. Monitors, keybinds, autostart
-- and window rules belong to the host's machine and stay behind.
for _, look in ipairs({ "settings", "decoration", "animation" }) do
	pcall(require, "configs/" .. look)
end

hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
-- Like every compositor in the suite: the fake mouse's moves are pixels. And
-- no "Hyprland Updated" window on top of the suite's.
hl.config({
	input = { accel_profile = "flat" },
	ecosystem = { no_update_news = true, no_donation_nag = true },
})
