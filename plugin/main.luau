--!strict
-- forge's Studio plugin: starts the link to `forge serve` in Studio's edit session. Packed into forge.rbxmx by
-- `forge plugin install`, which also writes the settings below as this script's attributes (docs/link.md).

local RunService = game:GetService("RunService")

local link = require(script.link)

-- Plugins also load into each playtest's client and server. Only the edit session is synced.
if not RunService:IsEdit() then
	return
end

local function setting(name: string): any
	local value = script:GetAttribute(name)
	assert(value ~= nil, `forge's plugin file has no {name}: run forge plugin install again`)
	return value
end

local connection = link.start({
	version = setting("version"),
	protocol = setting("protocol"),
	port = setting("port"),
	retrySeconds = setting("retryMs") / 1000,
	openTimeoutSeconds = setting("openTimeoutMs") / 1000,
})

plugin.Unloading:Connect(connection.stop)
