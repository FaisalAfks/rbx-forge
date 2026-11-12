--!strict
-- forge's window in Studio (#59), opened from its button in the Plugins tab. A short home page: the link to forge
-- serve, a push waiting for Apply, and recent activity. Settings and a push's full plan open as their own views in the
-- same window. Colours come from Studio's theme, so it follows Studio's light or dark look.
--
-- The window is drawn again from its state whenever something changes; it holds little, so that stays cheap.

local settingsStore = require(script.Parent.settings)

local window = {}

export type Ask = {
	title: string,
	-- How many instances the push changes, adds, removes or moves (the "Apply pushes" setting compares it).
	changes: number,
	summary: string,
	lines: { string },
	-- Shown in a warning colour (scripts whose draft a push writes over).
	warnings: { string },
	timeoutSeconds: number,
}

export type LinkState = "linked" | "trying" | "off"

-- Something a tool keeps on the home page until it's settled (sync's held clashes): a bold title, one line per item,
-- and a hint on what to do.
export type Notice = {
	title: string,
	lines: { string },
	hint: string,
}

export type Window = {
	-- The link's state, and a detail line (the server's version, or an error).
	setLink: (state: string, detail: string) -> (),
	-- One line in Recent, newest first.
	log: (text: string) -> (),
	-- Shows a push's plan and waits: "apply", "cancel" or "timeout".
	ask: (ask: Ask) -> string,
	-- A tool's notice on the home page; nil takes it away.
	setNotice: (tool: string, notice: Notice?) -> (),
	toggle: () -> (),
}

-- Lines shown at most in a plan; the rest are counted, so a huge push can't make the window slow.
local MAX_LINES = 300
-- How many lines Recent keeps.
local RECENT = 8

local function theme(color: Enum.StudioStyleGuideColor): Color3
	return (settings() :: any).Studio.Theme:GetColor(color)
end

local function stack(parent: Instance, order: number, gap: number, pad: number?): Frame
	local f = Instance.new("Frame")
	f.BackgroundTransparency = 1
	f.Size = UDim2.fromScale(1, 0)
	f.AutomaticSize = Enum.AutomaticSize.Y
	f.LayoutOrder = order
	local layout = Instance.new("UIListLayout")
	layout.SortOrder = Enum.SortOrder.LayoutOrder
	layout.Padding = UDim.new(0, gap)
	layout.Parent = f
	if pad then
		local p = Instance.new("UIPadding")
		p.PaddingLeft = UDim.new(0, pad)
		p.PaddingRight = UDim.new(0, pad)
		p.PaddingTop = UDim.new(0, pad)
		p.PaddingBottom = UDim.new(0, pad)
		p.Parent = f
	end
	f.Parent = parent
	return f
end

local function card(parent: Instance, order: number, border: Color3?): Frame
	local f = stack(parent, order, 6, 12)
	f.BackgroundTransparency = 0
	f.BackgroundColor3 = theme(Enum.StudioStyleGuideColor.InputFieldBackground)
	local corner = Instance.new("UICorner")
	corner.CornerRadius = UDim.new(0, 6)
	corner.Parent = f
	local stroke = Instance.new("UIStroke")
	stroke.Color = border or theme(Enum.StudioStyleGuideColor.Border)
	stroke.Parent = f
	return f
end

type TextOpts = { mono: boolean?, bold: boolean?, size: number?, color: Color3? }

local function text(parent: Instance, s: string, order: number, opts: TextOpts?): TextLabel
	local o: TextOpts = opts or {}
	local l = Instance.new("TextLabel")
	l.BackgroundTransparency = 1
	l.Size = UDim2.fromScale(1, 0)
	l.AutomaticSize = Enum.AutomaticSize.Y
	l.TextXAlignment = Enum.TextXAlignment.Left
	l.TextWrapped = true
	l.Font = if o.mono then Enum.Font.Code elseif o.bold then Enum.Font.SourceSansBold else Enum.Font.SourceSans
	l.TextSize = o.size or 15
	l.TextColor3 = o.color or theme(Enum.StudioStyleGuideColor.MainText)
	l.Text = s
	l.LayoutOrder = order
	l.Parent = parent
	return l
end

local function button(parent: Instance, label: string, order: number, main: boolean, onClick: () -> ()): TextButton
	local b = Instance.new("TextButton")
	b.AutomaticSize = Enum.AutomaticSize.X
	b.Size = UDim2.fromOffset(0, 28)
	b.Font = if main then Enum.Font.SourceSansBold else Enum.Font.SourceSans
	b.TextSize = 15
	b.Text = label
	b.AutoButtonColor = true
	b.BorderSizePixel = 0
	b.LayoutOrder = order
	b.BackgroundColor3 =
		theme(if main then Enum.StudioStyleGuideColor.DialogMainButton else Enum.StudioStyleGuideColor.DialogButton)
	b.TextColor3 = theme(
		if main then Enum.StudioStyleGuideColor.DialogMainButtonText else Enum.StudioStyleGuideColor.DialogButtonText
	)
	local pad = Instance.new("UIPadding")
	pad.PaddingLeft = UDim.new(0, 14)
	pad.PaddingRight = UDim.new(0, 14)
	pad.Parent = b
	local corner = Instance.new("UICorner")
	corner.CornerRadius = UDim.new(0, 4)
	corner.Parent = b
	b.Activated:Connect(onClick)
	b.Parent = parent
	return b
end

-- A row of buttons, right-aligned.
local function row(parent: Instance, order: number): Frame
	local f = Instance.new("Frame")
	f.BackgroundTransparency = 1
	f.Size = UDim2.new(1, 0, 0, 28)
	f.LayoutOrder = order
	local layout = Instance.new("UIListLayout")
	layout.FillDirection = Enum.FillDirection.Horizontal
	layout.HorizontalAlignment = Enum.HorizontalAlignment.Right
	layout.SortOrder = Enum.SortOrder.LayoutOrder
	layout.Padding = UDim.new(0, 8)
	layout.Parent = f
	f.Parent = parent
	return f
end

function window.new(plugin: Plugin, actions: { connect: () -> (), disconnect: () -> () }): Window
	local info = DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Float, false, false, 380, 560, 320, 260)
	local widget = plugin:CreateDockWidgetPluginGuiAsync("forge", info)
	widget.Title = "forge"
	widget.Name = "forge"

	local values = settingsStore.load(plugin)
	local view: "home" | "plan" | "settings" = "home"
	local link: { state: LinkState, detail: string } = { state = "off", detail = "" }
	local recent: { string } = {}
	local pending: { ask: Ask, finish: (string) -> () }? = nil
	-- Each tool's notice; a tool with none has no entry.
	local notices: { [string]: Notice? } = {}

	local render: () -> ()

	local function setValues(change: (settingsStore.Values) -> ())
		change(values)
		settingsStore.save(plugin, values)
		render()
	end

	local function home(body: Frame)
		local c = card(body, 1)
		local dot = if link.state == "linked"
			then Color3.fromRGB(63, 185, 80)
			elseif link.state == "trying" then theme(Enum.StudioStyleGuideColor.WarningText)
			else theme(Enum.StudioStyleGuideColor.DimmedText)
		local title = if link.state == "linked"
			then "Linked to forge serve"
			elseif link.state == "trying" then "Waiting for forge serve"
			else "Not connected"
		text(c, title, 1, { bold = true, color = dot, size = 16 })
		text(c, `place {game.PlaceId}`, 2, { color = theme(Enum.StudioStyleGuideColor.DimmedText) })
		if link.detail ~= "" then
			text(c, link.detail, 3, { color = theme(Enum.StudioStyleGuideColor.DimmedText) })
		end
		local buttons = row(c, 4)
		if link.state == "off" then
			button(buttons, "Connect", 1, false, actions.connect)
		else
			button(buttons, "Disconnect", 1, false, actions.disconnect)
		end

		local p = pending
		if p then
			local w = card(body, 2, theme(Enum.StudioStyleGuideColor.WarningText))
			text(w, "A push is waiting for you", 1, { bold = true })
			text(w, p.ask.summary, 2)
			if #p.ask.warnings > 0 then
				text(w, `{#p.ask.warnings - 1} scripts with a draft would be written over`, 3, {
					color = theme(Enum.StudioStyleGuideColor.WarningText),
				})
			end
			local buttons2 = row(w, 4)
			button(buttons2, "Show what changes", 1, false, function()
				view = "plan"
				render()
			end)
			button(buttons2, "Cancel", 2, false, function()
				p.finish("cancel")
			end)
			button(buttons2, "Apply", 3, true, function()
				p.finish("apply")
			end)
		end

		local tools = {}
		for tool in notices do
			table.insert(tools, tool)
		end
		table.sort(tools)
		for _, tool in tools do
			local n = notices[tool]
			if n == nil then
				continue
			end
			local k = card(body, 3, theme(Enum.StudioStyleGuideColor.ErrorText))
			text(k, n.title, 1, { bold = true })
			for i, line in n.lines do
				text(k, line, 1 + i, { mono = true, size = 14 })
			end
			text(k, n.hint, 99, {
				color = theme(Enum.StudioStyleGuideColor.DimmedText),
				size = 13,
			})
		end

		local r = stack(body, 4, 4)
		text(r, "Recent", 1, { color = theme(Enum.StudioStyleGuideColor.DimmedText), size = 13 })
		if #recent == 0 then
			text(r, "Nothing yet", 2, { color = theme(Enum.StudioStyleGuideColor.DimmedText) })
		end
		for i, line in recent do
			text(r, line, i + 1)
		end
	end

	local function plan(body: Frame)
		local p = pending
		if p == nil then
			view = "home"
			home(body)
			return
		end
		text(body, p.ask.summary, 1, { bold = true })
		for i, w in p.ask.warnings do
			text(body, w, 1 + i, { color = theme(Enum.StudioStyleGuideColor.WarningText) })
		end
		local list = card(body, 100)
		for i, line in p.ask.lines do
			if i > MAX_LINES then
				text(list, `... and {#p.ask.lines - MAX_LINES} more lines (the command's output has them all)`, i)
				break
			end
			text(list, line, i, { mono = true, size = 14 })
		end
		local buttons = row(body, 101)
		button(buttons, "Cancel", 1, false, function()
			p.finish("cancel")
		end)
		button(buttons, "Apply", 2, true, function()
			p.finish("apply")
		end)
	end

	local function settingsView(body: Frame)
		text(body, "Apply pushes", 1, { bold = true })
		local choices: { { mode: settingsStore.ApplyPushes, label: string } } = {
			{ mode = "ask", label = "Ask every time" },
			{ mode = "large", label = `Ask for large pushes (over {values.largePush} changes)` },
			{ mode = "never", label = "Never ask: apply as soon as a push comes" },
		}
		for i, choice in choices do
			local picked = values.applyPushes == choice.mode
			local b = button(body, `{if picked then "(x)" else "( )"}  {choice.label}`, 1 + i, false, function()
				setValues(function(v)
					v.applyPushes = choice.mode
				end)
			end)
			b.TextXAlignment = Enum.TextXAlignment.Left
			b.Size = UDim2.new(1, 0, 0, 28)
			b.AutomaticSize = Enum.AutomaticSize.None
		end
		if values.applyPushes == "large" then
			local box = Instance.new("TextBox")
			box.Size = UDim2.fromOffset(120, 28)
			box.LayoutOrder = 5
			box.Font = Enum.Font.SourceSans
			box.TextSize = 15
			box.Text = tostring(values.largePush)
			box.PlaceholderText = "changes"
			box.BackgroundColor3 = theme(Enum.StudioStyleGuideColor.InputFieldBackground)
			box.TextColor3 = theme(Enum.StudioStyleGuideColor.MainText)
			box.FocusLost:Connect(function()
				local n = tonumber(box.Text)
				if n and n >= 1 then
					setValues(function(v)
						v.largePush = math.floor(n)
					end)
				else
					box.Text = tostring(values.largePush)
				end
			end)
			box.Parent = body
		end
		text(
			body,
			"Without a click, the plan is still printed by the command, and what a push replaces is still backed up.",
			6,
			{
				color = theme(Enum.StudioStyleGuideColor.DimmedText),
				size = 13,
			}
		)
		text(body, "Link", 7, { bold = true })
		local b = button(
			body,
			`{if values.connectAutomatically then "[x]" else "[ ]"}  Connect automatically when the place opens`,
			8,
			false,
			function()
				setValues(function(v)
					v.connectAutomatically = not v.connectAutomatically
				end)
			end
		)
		b.TextXAlignment = Enum.TextXAlignment.Left
		b.Size = UDim2.new(1, 0, 0, 28)
		b.AutomaticSize = Enum.AutomaticSize.None
		text(body, "These settings change only here in Studio: no command or agent can change them.", 9, {
			color = theme(Enum.StudioStyleGuideColor.DimmedText),
			size = 13,
		})
	end

	render = function()
		widget:ClearAllChildren()
		local root = Instance.new("ScrollingFrame")
		root.Size = UDim2.fromScale(1, 1)
		root.CanvasSize = UDim2.new()
		root.AutomaticCanvasSize = Enum.AutomaticSize.Y
		root.ScrollBarThickness = 8
		root.BorderSizePixel = 0
		root.BackgroundColor3 = theme(Enum.StudioStyleGuideColor.MainBackground)
		root.Parent = widget
		local page = stack(root, 1, 12, 12)

		local header = Instance.new("Frame")
		header.BackgroundTransparency = 1
		header.Size = UDim2.new(1, 0, 0, 28)
		header.LayoutOrder = 0
		header.Parent = page
		local title =
			text(header, if view == "settings" then "Settings" elseif view == "plan" then "Push" else "forge", 1, {
				bold = true,
				size = 18,
			})
		title.Size = UDim2.fromScale(0.6, 1)
		local nav = row(header, 1)
		nav.Size = UDim2.fromScale(1, 1)
		if view == "home" then
			button(nav, "Settings", 1, false, function()
				view = "settings"
				render()
			end)
		else
			button(nav, "Back", 1, false, function()
				view = "home"
				render()
			end)
		end

		local body = stack(page, 1, 10)
		if view == "plan" then
			plan(body)
		elseif view == "settings" then
			settingsView(body)
		else
			home(body)
		end
	end

	pcall(function()
		(settings() :: any).Studio.ThemeChanged:Connect(render)
	end)
	render()

	local self: Window = {
		setLink = function(state, detail)
			local known: LinkState = if state == "linked" then "linked" elseif state == "trying" then "trying" else "off"
			link = { state = known, detail = detail }
			render()
		end,
		log = function(line)
			table.insert(recent, 1, `{os.date("%H:%M")}  {line}`)
			if #recent > RECENT then
				table.remove(recent)
			end
			render()
		end,
		ask = function(ask)
			if pending then
				error("forge's window is already waiting for Apply or Cancel on another push")
			end
			local thread = coroutine.running()
			local result: string? = nil
			local function finish(choice: string)
				if result == nil then
					result = choice
					task.spawn(thread)
				end
			end
			pending = { ask = ask, finish = finish }
			view = "home"
			widget.Enabled = true
			render()
			task.delay(ask.timeoutSeconds, function()
				finish("timeout")
			end)
			coroutine.yield()
			pending = nil
			if view == "plan" then
				view = "home"
			end
			render()
			return result :: string
		end,
		setNotice = function(tool, notice)
			notices[tool] = notice
			render()
		end,
		toggle = function()
			widget.Enabled = not widget.Enabled
		end,
	}
	return self
end

-- The settings the window last saved, read fresh (a push decides with these).
function window.settings(plugin: Plugin): settingsStore.Values
	return settingsStore.load(plugin)
end

return window
