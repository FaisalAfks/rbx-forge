--!strict
-- forge sync's writer in Studio: carries out a push's plan (tools/sync/src/push.rs) as one change in Studio's undo
-- history. Operations name instances by the ids the last read gave them; instances the push adds are built from the
-- files first, unparented, so references between them can be set before any of them appears in the place.
--
-- If anything fails, the whole recording is cancelled, which puts Studio back as it was.

local ChangeHistoryService = game:GetService("ChangeHistoryService")
local EncodingService = game:GetService("EncodingService")
local ScriptEditorService = game:GetService("ScriptEditorService")
local SerializationService = game:GetService("SerializationService")

local read = require(script.Parent.read)

local write = {}

type Json = any

-- Instances this push builds, by their id in the files.
type Built = { [number]: Instance }

local function num(v: Json): number
	if v == "inf" then
		return math.huge
	elseif v == "-inf" then
		return -math.huge
	elseif v == "nan" then
		return 0 / 0
	end
	return v :: number
end

local function color(v: Json): Color3
	return Color3.new(num(v[1]), num(v[2]), num(v[3]))
end

local function vector(v: Json): Vector3
	return Vector3.new(num(v[1]), num(v[2]), num(v[3]))
end

-- The instance a reference names: one Studio has, one this push builds, or one outside the tree by its names.
local function target(ref: Json, built: Built): Instance
	if type(ref) == "table" and ref.id ~= nil then
		return read.instance(ref.id)
	elseif type(ref) == "table" and ref.new ~= nil then
		local inst = built[ref.new]
		if inst == nil then
			error(`the push refers to new instance {ref.new}, which it doesn't build`)
		end
		return inst
	end
	local ok, node = pcall(game.FindService, game, ref[1])
	if not ok or node == nil then
		error(`a reference names {table.concat(ref, ".")}, which isn't in the place`)
	end
	local inst: Instance = node
	for i = 2, #ref do
		local child = inst:FindFirstChild(ref[i])
		if child == nil then
			error(`a reference names {table.concat(ref, ".")}, which isn't in the place`)
		end
		inst = child
	end
	return inst
end

-- A value in the JSON form for its disk type, as a Roblox value. `current` is what the property holds now, which an
-- enum needs for its type.
local function decode(ty: string, v: Json, current: any, built: Built): any
	if ty == "bool" or ty == "string" then
		return v
	elseif ty == "int" or ty == "float" or ty == "number" then
		return num(v)
	elseif ty == "Content" then
		return if v == "" then Content.none else Content.fromUri(v)
	elseif ty == "Enum" then
		if typeof(current) ~= "EnumItem" then
			error(`can't tell which enum {v} belongs to`)
		end
		return (current :: EnumItem).EnumType:FromName(v)
	elseif ty == "BrickColor" then
		return BrickColor.new(v)
	elseif ty == "Color3" then
		return color(v)
	elseif ty == "ColorSequence" then
		local keys = {}
		for _, k in v do
			table.insert(keys, ColorSequenceKeypoint.new(num(k[1]), color(k[2])))
		end
		return ColorSequence.new(keys)
	elseif ty == "NumberSequence" then
		local keys = {}
		for _, k in v do
			table.insert(keys, NumberSequenceKeypoint.new(num(k[1]), num(k[2]), num(k[3])))
		end
		return NumberSequence.new(keys)
	elseif ty == "NumberRange" then
		return NumberRange.new(num(v[1]), num(v[2]))
	elseif ty == "CFrame" then
		local c = {}
		for i = 1, 12 do
			c[i] = num(v[i])
		end
		return CFrame.new(table.unpack(c))
	elseif ty == "Faces" then
		local faces = {}
		for _, name in v do
			table.insert(faces, (Enum.NormalId :: any)[name])
		end
		return Faces.new(table.unpack(faces))
	elseif ty == "Axes" then
		local axes = {}
		for _, name in v do
			table.insert(axes, (Enum.Axis :: any)[name])
		end
		return Axes.new(table.unpack(axes))
	elseif ty == "Font" then
		return Font.new(v.family, (Enum.FontWeight :: any)[v.weight], (Enum.FontStyle :: any)[v.style])
	elseif ty == "PhysicalProperties" then
		return PhysicalProperties.new(
			num(v.density),
			num(v.friction),
			num(v.elasticity),
			num(v.friction_weight),
			num(v.elasticity_weight)
		)
	elseif ty == "Ray" then
		return Ray.new(vector(v.origin), vector(v.direction))
	elseif ty == "Rect" then
		return Rect.new(num(v[1]), num(v[2]), num(v[3]), num(v[4]))
	elseif ty == "Ref" then
		return target(v, built)
	elseif ty == "SecurityCapabilities" then
		local caps = {}
		for _, name in v do
			table.insert(caps, (Enum.SecurityCapability :: any)[name])
		end
		return SecurityCapabilities.new(table.unpack(caps))
	elseif ty == "UDim" then
		return UDim.new(num(v[1]), v[2])
	elseif ty == "UDim2" then
		return UDim2.new(num(v[1]), v[2], num(v[3]), v[4])
	elseif ty == "Vector2" then
		return Vector2.new(num(v[1]), num(v[2]))
	elseif ty == "Vector3" then
		return vector(v)
	elseif ty == "Vector3int16" then
		return Vector3int16.new(v[1], v[2], v[3])
	end
	error(`forge's plugin can't write a {ty}: run forge plugin install`)
end

local function set(inst: Instance, name: string, ty: string, v: Json, built: Built)
	local current = (inst :: any)[name]
	;(inst :: any)[name] = decode(ty, v, current, built)
end

-- An attribute's type as `typeof` names it, as a disk type the decoder knows.
local function attribute(inst: Instance, name: string, a: Json?, built: Built)
	if a == nil then
		inst:SetAttribute(name, nil)
		return
	end
	local ty = if a.type == "boolean" then "bool" elseif a.type == "EnumItem" then "string" else a.type
	inst:SetAttribute(name, decode(ty, a.value, inst:GetAttribute(name), built))
end

local function tags(inst: Instance, wanted: { string })
	local keep = {}
	for _, t in wanted do
		keep[t] = true
	end
	for _, t in inst:GetTags() do
		if not keep[t] then
			inst:RemoveTag(t)
		end
	end
	for _, t in wanted do
		inst:AddTag(t)
	end
end

-- A MeshPart (or any model class) from its model file, as pull wrote it (decision 0011).
local function fromModel(b64: string): Instance
	local list = SerializationService:DeserializeInstancesAsync(EncodingService:Base64Decode(buffer.fromstring(b64)))
	if #list ~= 1 then
		error(`a model file held {#list} instances, not one`)
	end
	return list[1]
end

type Source = { inst: Instance, text: string }

-- Builds a subtree from the files, unparented, noting each new instance by its id in the files. Values are set
-- afterwards, once every new instance exists, and script text once the scripts are in the place.
local function build(node: Json, built: Built, later: { () -> () }, sources: { Source }): Instance
	local inst = if node.model then fromModel(node.model) else Instance.new(node.class)
	inst.Name = node.name
	built[node.id] = inst
	if node.source ~= nil then
		table.insert(sources, { inst = inst, text = node.source })
	end
	for _, child in node.children or {} do
		build(child, built, later, sources).Parent = inst
	end
	table.insert(later, function()
		for name, p in node.properties or {} do
			set(inst, name, p.type, p.value, built)
		end
		for name, a in node.attributes or {} do
			attribute(inst, name, a, built)
		end
		tags(inst, node.tags or {})
	end)
	return inst
end

-- Swaps an instance for one built from a new model file, keeping its name, place and children.
local function remodel(id: number, b64: string)
	local old = read.instance(id)
	local new = fromModel(b64)
	new.Name = old.Name
	for _, child in old:GetChildren() do
		child.Parent = new
	end
	new.Parent = old.Parent
	old.Parent = nil
	read.replace(id, new)
end

-- Carries out every tree's operations. Errors leave the work half done; `write.apply` cancels it.
local function run(trees: { { ops: { Json } } })
	local built: Built = {}
	local later: { () -> () } = {}
	local roots: { { inst: Instance, parent: Json } } = {}
	local sources: { Source } = {}
	local all: { Json } = {}
	for _, tree in trees do
		for _, op in tree.ops do
			table.insert(all, op)
		end
	end
	-- New subtrees and replaced models first, so everything a value can point at exists.
	for _, op in all do
		if op.op == "add" then
			table.insert(roots, { inst = build(op.tree, built, later, sources), parent = op.parent })
		elseif op.op == "model" then
			remodel(op.id, op.model)
		end
	end
	for _, f in later do
		f()
	end
	for _, op in all do
		local kind = op.op
		if kind == "move" then
			local inst = read.instance(op.id)
			inst.Parent = target(op.parent, built)
			inst.Name = op.name
		elseif kind == "set" then
			set(read.instance(op.id), op.name, op.type, op.value, built)
		elseif kind == "reset" then
			local inst = read.instance(op.id)
			local fresh = Instance.new(inst.ClassName :: any)
			;(inst :: any)[op.name] = (fresh :: any)[op.name]
			fresh:Destroy()
		elseif kind == "attribute" then
			attribute(read.instance(op.id), op.name, op.value, built)
		elseif kind == "tags" then
			tags(read.instance(op.id), op.tags)
		elseif kind == "source" then
			table.insert(sources, { inst = read.instance(op.id), text = op.source })
		elseif kind ~= "add" and kind ~= "model" and kind ~= "remove" then
			error(`forge's plugin doesn't know the push operation {kind}: run forge plugin install`)
		end
	end
	for _, r in roots do
		r.inst.Parent = target(r.parent, built)
	end
	-- Script text only through ScriptEditorService (decision 0005), once the scripts are in the place.
	for _, s in sources do
		ScriptEditorService:UpdateSourceAsync(s.inst :: LuaSourceContainer, function()
			return s.text
		end)
	end
	-- Removed last: `Parent = nil` rather than Destroy, so undo can bring them back.
	for _, op in all do
		if op.op == "remove" then
			read.instance(op.id).Parent = nil
		end
	end
end

-- Applies a push as one undo step. Returns nothing; errors say why, with Studio put back as it was.
function write.apply(trees: { { ops: { Json } } })
	local recording = ChangeHistoryService:TryBeginRecording("forge sync push")
	if recording == nil then
		error("Studio is in the middle of another change (or a playtest), so the push can't be one undo step: try again")
	end
	local ok, err = pcall(run, trees)
	if not ok then
		ChangeHistoryService:FinishRecording(recording, Enum.FinishRecordingOperation.Cancel)
		error(`the push failed, and Studio is as it was: {err}`)
	end
	ChangeHistoryService:FinishRecording(recording, Enum.FinishRecordingOperation.Commit)
end

return write
