--!strict
-- forge's Studio plugin: a button in the Plugins tab that opens forge's window, and the link to `forge serve` in
-- Studio's edit session. Packed into forge.rbxmx by `forge plugin install`, which also writes the link's settings as
-- this script's attributes (docs/link.md). The settings people change live in Studio (plugin/settings.luau).

local RunService = game:GetService("RunService")

local link = require(script.link)
local settingsStore = require(script.settings)
local window = require(script.window)

-- What a tool's Studio side (tools/<name>/studio/main.luau) gives the plugin: it answers its requests, and may say
-- what a request did, as a line in the window's Recent list.
type ToolSide = {
	handle: (request: any, context: any) -> any,
	describe: ((request: any, result: any) -> string?)?,
}

-- Every tool's Studio side, by the name commands ask for (decision 0019). A new tool is one more line here.
local tools: { [string]: ToolSide } = {
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

-- What the plugin gives each tool: the parts that need the plugin or its window.
local function contextFor(name: string)
	return {
		-- Shows a push's plan and waits for Apply, unless the "Apply pushes" setting says not to ask (decision 0018).
		confirm = function(ask: any): string
			if not settingsStore.asks(settingsStore.load(plugin), ask.changes or math.huge) then
				return "apply"
			end
			return ui.ask(ask)
		end,
		notify = function(event: any)
			local c = connection
			if c then
				c.notify(event)
			end
		end,
		-- Keeps the tool's notice on the home page until it passes nil.
		showNotice = function(notice: window.Notice?)
			ui.setNotice(name, notice)
		end,
	}
end

local function handle(request: any): any
	local tool = tools[request.tool]
	if tool == nil then
		error(`forge's plugin has no tool {request.tool}: run forge plugin install`)
	end
	local ok, result = pcall(tool.handle, request, contextFor(request.tool))
	if not ok then
		ui.log(`{request.action} failed: {result}`)
		error(result, 0)
	end
	local line = if tool.describe then tool.describe(request, result) else nil
	if line then
		ui.log(line)
	end
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
