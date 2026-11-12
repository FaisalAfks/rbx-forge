--!strict
-- forge's settings in Studio (decision 0018), stored with plugin:SetSetting so they last across Studio restarts and
-- can only be changed here, in forge's window: no command, agent or file reaches them.

local settings = {}

export type ApplyPushes = "ask" | "large" | "never"

export type Values = {
	-- When a push waits for Apply: every time, only above `largePush` changes, or never.
	applyPushes: ApplyPushes,
	largePush: number,
	-- Whether the link to forge serve starts when Studio opens the place.
	connectAutomatically: boolean,
}

local DEFAULTS: Values = {
	applyPushes = "ask",
	largePush = 20,
	connectAutomatically = true,
}

local KEY = "forge.settings"

-- The stored settings, with a default for any that's missing or unreadable.
function settings.load(plugin: Plugin): Values
	local ok, stored = pcall(plugin.GetSetting, plugin, KEY)
	local values = table.clone(DEFAULTS)
	if ok and type(stored) == "table" then
		if stored.applyPushes == "ask" or stored.applyPushes == "large" or stored.applyPushes == "never" then
			values.applyPushes = stored.applyPushes
		end
		if type(stored.largePush) == "number" and stored.largePush >= 1 then
			values.largePush = math.floor(stored.largePush)
		end
		if type(stored.connectAutomatically) == "boolean" then
			values.connectAutomatically = stored.connectAutomatically
		end
	end
	return values
end

function settings.save(plugin: Plugin, values: Values)
	plugin:SetSetting(KEY, values)
end

-- Whether a push of `changes` changes waits for Apply under these settings.
function settings.asks(values: Values, changes: number): boolean
	if values.applyPushes == "never" then
		return false
	elseif values.applyPushes == "large" then
		return changes > values.largePush
	end
	return true
end

return settings
