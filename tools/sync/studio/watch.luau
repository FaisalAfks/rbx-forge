--!strict
-- forge sync's watcher in Studio (decision 0013). While `forge sync watch` runs, it listens on each synced tree for
-- changes to the properties forge syncs, attributes, instances added, removed or moved, and script text (open tabs
-- as they're typed in, and a poll for edits nothing announces). When a tree has been quiet for a moment it tells forge
-- "this tree changed", once, so a drag becomes one change at the end. forge then reads the tree and merges.

local ScriptEditorService = game:GetService("ScriptEditorService")

local read = require(script.Parent.read)

local watch = {}

export type Request = {
	trees: { { string } },
	properties: { [string]: { [string]: string } },
	methods: { [string]: { [string]: string } },
	quietMs: number,
	pollMs: number,
}

-- How often the watcher looks for trees that have gone quiet.
local TICK = 0.05

local active: { stop: () -> () }? = nil

function watch.stop()
	local a = active
	if a then
		active = nil
		a.stop()
	end
end

-- Starts watching, replacing any watch already running. `notify` sends an event to forge.
function watch.start(request: Request, notify: (event: any) -> ())
	watch.stop()
	local roots: { Instance } = {}
	for _, path in request.trees do
		table.insert(roots, read.root(path))
	end
	local running = true
	local connections: { RBXScriptConnection } = {}
	local lastChange: { [number]: number } = {}
	local scriptText: { [Instance]: string } = {}

	-- The properties whose changes matter, per class: the ones pull reads, plus where an instance is and its name.
	local watched: { [string]: { [string]: boolean } } = {}
	local function matters(class: string, property: string): boolean
		local set = watched[class]
		if set == nil then
			set = { Name = true, Parent = true }
			local props: { [string]: string } = request.properties[class] or {}
			for name in props do
				set[name] = true
			end
			local byMethod: { [string]: string } = request.methods[class] or {}
			for name in byMethod do
				set[name] = true
			end
			watched[class] = set
		end
		return set[property] == true
	end

	local function mark(tree: number)
		lastChange[tree] = os.clock()
	end

	local function follow(inst: Instance, tree: number)
		table.insert(
			connections,
			inst.Changed:Connect(function(property: string)
				if matters(inst.ClassName, property) then
					read.forgetModel(inst)
					mark(tree)
				end
			end)
		)
		table.insert(
			connections,
			inst.AttributeChanged:Connect(function()
				mark(tree)
			end)
		)
		if inst:IsA("LuaSourceContainer") then
			scriptText[inst] = ScriptEditorService:GetEditorSource(inst :: LuaSourceContainer)
		end
	end

	local function treeOf(inst: Instance): number?
		for i, root in roots do
			if inst == root or inst:IsDescendantOf(root) then
				return i
			end
		end
		return nil
	end

	for i, root in roots do
		follow(root, i)
		for _, d in root:GetDescendants() do
			follow(d, i)
		end
		table.insert(
			connections,
			root.DescendantAdded:Connect(function(d)
				follow(d, i)
				mark(i)
			end)
		)
		table.insert(
			connections,
			root.DescendantRemoving:Connect(function()
				mark(i)
			end)
		)
	end
	table.insert(
		connections,
		ScriptEditorService.TextDocumentDidChange:Connect(function(doc)
			local s = doc:GetScript()
			local tree = if s then treeOf(s) else nil
			if tree then
				mark(tree)
			end
		end)
	)
	read.setModelCaching(true)

	-- Edits nothing announces (another plugin's UpdateSourceAsync, a collaborator's commit) show up in the poll.
	task.spawn(function()
		while running do
			task.wait(request.pollMs / 1000)
			for inst, text in scriptText do
				if inst.Parent == nil then
					scriptText[inst] = nil
					continue
				end
				local now = ScriptEditorService:GetEditorSource(inst :: LuaSourceContainer)
				if now ~= text then
					scriptText[inst] = now
					local tree = treeOf(inst)
					if tree then
						mark(tree)
					end
				end
			end
		end
	end)

	task.spawn(function()
		while running do
			task.wait(TICK)
			local now = os.clock()
			for tree, at in lastChange do
				if now - at >= request.quietMs / 1000 then
					lastChange[tree] = nil
					notify({ tool = "sync", kind = "changed", tree = tree - 1 })
				end
			end
		end
	end)

	active = {
		stop = function()
			running = false
			for _, c in connections do
				c:Disconnect()
			end
			read.setModelCaching(false)
		end,
	}
end

return watch
