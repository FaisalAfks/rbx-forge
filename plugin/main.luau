--!strict
-- forge's Studio plugin: a button in the Plugins tab that opens forge's window, and the link to `forge serve` in
-- Studio's edit session. Packed into forge.rbxmx by `forge plugin install`, which also writes the link's settings as
-- this script's attributes (docs/link.md). The settings people change live in Studio (plugin/settings.luau).

local RunService = game:GetService("RunService")

local link = require(script.link)
local settingsStore = require(script.settings)
local window = require(script.window)

-- Each tool's Studio side (tools/<name>/studio/, packed under tools), by the name commands ask for.
local tools: { [string]: { handle: (request: any, context: any) -> any } } = {
	sync = require(script.tools.sync.main),
}

-- Plugins also load into each playtest's client and server. Only the edit session is synced.
if not RunService:IsEdit() then
	return
end

local function setting(name: string): any
	local value = script:GetAttribute(name)
	assert(value ~= nil, `forge's plugin file has no {name}: run forge plugin install again`)
	return value
end

local linkSettings = {
	version = setting("version"),
	protocol = setting("protocol"),
	port = setting("port"),
	retrySeconds = setting("retryMs") / 1000,
	openTimeoutSeconds = setting("openTimeoutMs") / 1000,
}

local connection: link.Connection? = nil
local ui: window.Window

-- What the plugin gives every tool: the parts that need the plugin or its window.
local context = {
	-- Shows a push's plan and waits for Apply, unless the "Apply pushes" setting says not to ask (decision 0018).
	confirm = function(ask: any): string
		if not settingsStore.asks(settingsStore.load(plugin), ask.changes or math.huge) then
			return "apply"
		end
		return ui.ask(ask)
	end,
}

-- A line in the window's Recent list for each request a command sends.
local function note(request: any, result: any)
	if request.action == "read" then
		ui.log(`Read {#request.trees} trees for forge`)
	elseif request.action == "drafts" then
		ui.log("Listed scripts with drafts")
	elseif request.action == "check" then
		ui.log(`Round-trip check of {#request.trees} trees`)
	elseif request.action == "push" then
		local outcome = if type(result) == "table" then result.result else "?"
		ui.log(`Push {outcome}: {request.summary}`)
	end
end

local function handle(request: any): any
	local tool = tools[request.tool]
	if tool == nil then
		error(`forge's plugin has no tool {request.tool}: run forge plugin install`)
	end
	local ok, result = pcall(tool.handle, request, context)
	if not ok then
		ui.log(`{request.action} failed: {result}`)
		error(result, 0)
	end
	note(request, result)
	return result
end

local function connect()
	if connection == nil then
		connection = link.start(linkSettings, handle, ui.setLink)
	end
end

local function disconnect()
	local c = connection
	if c then
		connection = nil
		c.stop()
	end
end

ui = window.new(plugin, { connect = connect, disconnect = disconnect })

local toolbar = plugin:CreateToolbar("forge")
local button = toolbar:CreateButton("forge", "Open forge's window: the link, a waiting push, and settings", "")
button.ClickableWhenViewportHidden = true
button.Click:Connect(ui.toggle)

if settingsStore.load(plugin).connectAutomatically then
	connect()
end

plugin.Unloading:Connect(disconnect)
