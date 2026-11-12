--!strict
-- forge's confirm window in Studio (decision 0015): shows what a command wants to change, and waits for Apply or
-- Cancel. Nothing a push plans happens until Apply is clicked here, so the choice is made where the owner sees it,
-- not by whoever runs the command. Colours come from Studio's theme, so it looks like Studio's own windows.

local confirm = {}

export type Ask = {
	title: string,
	summary: string,
	lines: { string },
	-- Shown in a warning colour above the list (scripts whose draft a push writes over).
	warnings: { string },
	timeoutSeconds: number,
}

-- Lines shown at most; the rest are counted, so a huge push can't make the window slow.
local MAX_LINES = 300

local widget: DockWidgetPluginGui? = nil
local busy = false

local function theme(color: Enum.StudioStyleGuideColor, modifier: Enum.StudioStyleGuideModifier?): Color3
	return (settings() :: any).Studio.Theme:GetColor(color, modifier or Enum.StudioStyleGuideModifier.Default)
end

local function label(parent: Instance, text: string, order: number, color: Color3?, font: Enum.Font?): TextLabel
	local l = Instance.new("TextLabel")
	l.BackgroundTransparency = 1
	l.Size = UDim2.new(1, -8, 0, 0)
	l.AutomaticSize = Enum.AutomaticSize.Y
	l.TextXAlignment = Enum.TextXAlignment.Left
	l.TextWrapped = true
	l.RichText = false
	l.Font = font or Enum.Font.SourceSans
	l.TextSize = 15
	l.TextColor3 = color or theme(Enum.StudioStyleGuideColor.MainText)
	l.Text = text
	l.LayoutOrder = order
	l.Parent = parent
	return l
end

local function button(parent: Instance, text: string, main: boolean, x: number): TextButton
	local b = Instance.new("TextButton")
	b.Size = UDim2.fromOffset(96, 28)
	b.Position = UDim2.new(1, x, 0.5, -14)
	b.Font = Enum.Font.SourceSansBold
	b.TextSize = 15
	b.AutoButtonColor = true
	b.Text = text
	local color = if main then Enum.StudioStyleGuideColor.DialogMainButton else Enum.StudioStyleGuideColor.DialogButton
	local textColor = if main
		then Enum.StudioStyleGuideColor.DialogMainButtonText
		else Enum.StudioStyleGuideColor.DialogButtonText
	b.BackgroundColor3 = theme(color)
	b.TextColor3 = theme(textColor)
	b.BorderSizePixel = 0
	local corner = Instance.new("UICorner")
	corner.CornerRadius = UDim.new(0, 4)
	corner.Parent = b
	b.Parent = parent
	return b
end

local function window(plugin: Plugin): DockWidgetPluginGui
	if widget then
		return widget
	end
	local info = DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Float, false, true, 520, 420, 360, 240)
	local w = plugin:CreateDockWidgetPluginGuiAsync("forgeConfirm", info)
	w.Title = "forge"
	widget = w
	return w
end

-- Shows the window and waits. Returns "apply", "cancel" (Cancel, or the window closed) or "timeout".
function confirm.ask(plugin: Plugin, ask: Ask): string
	if busy then
		error("forge's window is already waiting for Apply or Cancel on another change")
	end
	busy = true
	local w = window(plugin)
	w:ClearAllChildren()
	w.Title = ask.title

	local root = Instance.new("Frame")
	root.Size = UDim2.fromScale(1, 1)
	root.BackgroundColor3 = theme(Enum.StudioStyleGuideColor.MainBackground)
	root.BorderSizePixel = 0
	root.Parent = w

	local list = Instance.new("ScrollingFrame")
	list.Position = UDim2.fromOffset(8, 8)
	list.Size = UDim2.new(1, -16, 1, -60)
	list.BackgroundColor3 = theme(Enum.StudioStyleGuideColor.InputFieldBackground)
	list.BorderSizePixel = 0
	list.CanvasSize = UDim2.new()
	list.AutomaticCanvasSize = Enum.AutomaticSize.Y
	list.ScrollBarThickness = 8
	list.Parent = root
	local layout = Instance.new("UIListLayout")
	layout.SortOrder = Enum.SortOrder.LayoutOrder
	layout.Padding = UDim.new(0, 2)
	layout.Parent = list
	local pad = Instance.new("UIPadding")
	pad.PaddingLeft = UDim.new(0, 6)
	pad.PaddingTop = UDim.new(0, 6)
	pad.Parent = list

	label(list, ask.summary, 1, nil, Enum.Font.SourceSansBold)
	local order = 2
	for _, warning in ask.warnings do
		label(list, warning, order, theme(Enum.StudioStyleGuideColor.WarningText))
		order += 1
	end
	for i, line in ask.lines do
		if i > MAX_LINES then
			label(list, `... and {#ask.lines - MAX_LINES} more lines (the command's output has them all)`, order)
			break
		end
		label(list, line, order, nil, Enum.Font.Code)
		order += 1
	end

	local bar = Instance.new("Frame")
	bar.Position = UDim2.new(0, 0, 1, -48)
	bar.Size = UDim2.new(1, 0, 0, 48)
	bar.BackgroundTransparency = 1
	bar.Parent = root
	local apply = button(bar, "Apply", true, -112)
	local cancel = button(bar, "Cancel", false, -216)

	local result: string? = nil
	local thread = coroutine.running()
	local function finish(choice: string)
		if result == nil then
			result = choice
			task.spawn(thread)
		end
	end
	local connections = {
		apply.Activated:Connect(function()
			finish("apply")
		end),
		cancel.Activated:Connect(function()
			finish("cancel")
		end),
		w:GetPropertyChangedSignal("Enabled"):Connect(function()
			if not w.Enabled then
				finish("cancel")
			end
		end),
	}
	task.delay(ask.timeoutSeconds, function()
		finish("timeout")
	end)
	w.Enabled = true
	coroutine.yield()
	for _, c in connections do
		c:Disconnect()
	end
	w.Enabled = false
	w:ClearAllChildren()
	busy = false
	return result :: string
end

return confirm
