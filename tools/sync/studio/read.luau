--!strict
-- forge sync's reader in Studio (decisions 0006 and 0011). Two calls, both from `forge sync pull` through the link:
-- `classes` lists every class under the trees with Studio's reflection data, and forge chooses the properties; `read`
-- reads those properties, attributes, tags, script text (only through ScriptEditorService) and, for model classes, a
-- model file of the instance alone. Values are written in the JSON forms tools/sync/src/value.rs reads. Instances
-- Studio doesn't save (Archivable off) are left out with everything under them.

local EncodingService = game:GetService("EncodingService")
local ReflectionService = game:GetService("ReflectionService")
local ScriptEditorService = game:GetService("ScriptEditorService")
local SerializationService = game:GetService("SerializationService")

type Json = any

local read = {}

-- The instances the last read sent, by id, so a push planned from that read can name them.
local lastRead: { [number]: Instance } = {}

-- The instance the last read sent as `id`.
function read.instance(id: number): Instance
	local inst = lastRead[id]
	if inst == nil or inst.Parent == nil then
		error(`instance {id} from forge's last read is gone from Studio: the tree changed, run the command again`)
	end
	return inst
end

-- Points `id` at a new instance that took the old one's place (a MeshPart rebuilt from its model file).
function read.replace(id: number, inst: Instance)
	lastRead[id] = inst
end

-- The instance a forge.toml path names: a service, then one child per name. Two children with the same name would
-- make the path mean either, so that's an error.
local function resolve(path: { string }): Instance
	local ok, service = pcall(game.FindService, game, path[1])
	if not ok or service == nil then
		error(`{path[1]} isn't a service in this place`)
	end
	local node: Instance = service
	for i = 2, #path do
		local found: Instance? = nil
		for _, child in node:GetChildren() do
			if child.Name == path[i] then
				if found then
					error(
						`two children of {node:GetFullName()} are named {path[i]}: rename one so forge.toml names one`
					)
				end
				found = child
			end
		end
		if found == nil then
			error(`{node:GetFullName()} has no child named {path[i]}`)
		end
		node = found :: Instance
	end
	return node
end

-- Every saved instance under `root`, root first, parents before children.
local function saved(root: Instance): { Instance }
	local out = {}
	local function visit(inst: Instance)
		if not inst.Archivable then
			return
		end
		table.insert(out, inst)
		for _, child in inst:GetChildren() do
			visit(child)
		end
	end
	visit(root)
	return out
end

-- Whether a script has an uncommitted draft: with collaborative editing, `Source` keeps the last committed text while
-- the editor has the draft, which plugins can't read or commit (decision 0009, #20).
local function hasDraft(script: LuaSourceContainer, editorText: string): boolean
	local ok, committed = pcall(function()
		return (script :: any).Source
	end)
	return ok and committed ~= editorText
end

-- Every script under the trees with an uncommitted draft, by full name, per tree.
function read.drafts(trees: { { string } }): Json
	local out = {}
	for _, path in trees do
		local list = {}
		for _, inst in saved(resolve(path)) do
			if inst:IsA("LuaSourceContainer") then
				local script = inst :: LuaSourceContainer
				if hasDraft(script, ScriptEditorService:GetEditorSource(script)) then
					table.insert(list, inst:GetFullName())
				end
			end
		end
		table.insert(out, list)
	end
	return { drafts = out }
end

function read.classes(trees: { { string } }): Json
	local classes = {}
	for _, path in trees do
		for _, inst in saved(resolve(path)) do
			local class = inst.ClassName
			if classes[class] == nil then
				local list = {}
				for _, p in ReflectionService:GetPropertiesOfClass(class) or {} do
					local permits = p.Permits or {}
					table.insert(list, {
						name = p.Name,
						owner = p.Owner,
						engine = p.Type.EngineType,
						serialized = p.Serialized == true,
						read = permits.Read ~= nil,
						write = permits.Write ~= nil,
						deprecated = p.Display ~= nil and p.Display.DeprecationMessage ~= nil,
					})
				end
				classes[class] = list
			end
		end
	end
	return { classes = classes }
end

-- JSON has no infinity or NaN, so those go as strings (decision 0014).
local function num(x: number): Json
	if x ~= x then
		return "nan"
	elseif x == math.huge then
		return "inf"
	elseif x == -math.huge then
		return "-inf"
	end
	return x
end

local function xyz(v: Vector3): Json
	return { num(v.X), num(v.Y), num(v.Z) }
end

local function rgb(c: Color3): Json
	return { num(c.R), num(c.G), num(c.B) }
end

-- The names from `game` down to an instance outside the tree.
local function names(inst: Instance): { string }?
	local out = {}
	local node: Instance? = inst
	while node and node ~= game do
		table.insert(out, 1, node.Name)
		node = node.Parent
	end
	return if node == game then out else nil
end

-- A value in the JSON form for its type. `ids` numbers the instances being sent, so a reference inside the trees
-- is an id and one outside is a list of names.
local function encode(v: any, ids: { [Instance]: number }, where: string): Json
	local t = typeof(v)
	if t == "number" then
		return num(v)
	elseif t == "boolean" or t == "string" then
		return v
	elseif t == "EnumItem" then
		return v.Name
	elseif t == "Vector3" then
		return xyz(v)
	elseif t == "Vector2" then
		return { num(v.X), num(v.Y) }
	elseif t == "Vector3int16" then
		return { v.X, v.Y, v.Z }
	elseif t == "CFrame" then
		local out = {}
		for _, c in { v:GetComponents() } do
			table.insert(out, num(c))
		end
		return out
	elseif t == "Color3" then
		return rgb(v)
	elseif t == "UDim" then
		return { num(v.Scale), v.Offset }
	elseif t == "UDim2" then
		return { num(v.X.Scale), v.X.Offset, num(v.Y.Scale), v.Y.Offset }
	elseif t == "BrickColor" then
		return v.Number
	elseif t == "NumberRange" then
		return { num(v.Min), num(v.Max) }
	elseif t == "Rect" then
		return { num(v.Min.X), num(v.Min.Y), num(v.Max.X), num(v.Max.Y) }
	elseif t == "Font" then
		return { family = v.Family, weight = v.Weight.Name, style = v.Style.Name }
	elseif t == "Content" then
		return v.Uri or ""
	elseif t == "NumberSequence" then
		local out = {}
		for _, k in v.Keypoints do
			table.insert(out, { num(k.Time), num(k.Value), num(k.Envelope) })
		end
		return out
	elseif t == "ColorSequence" then
		local out = {}
		for _, k in v.Keypoints do
			table.insert(out, { num(k.Time), rgb(k.Value) })
		end
		return out
	elseif t == "PhysicalProperties" then
		local out = {
			density = num(v.Density),
			friction = num(v.Friction),
			elasticity = num(v.Elasticity),
			friction_weight = num(v.FrictionWeight),
			elasticity_weight = num(v.ElasticityWeight),
		}
		local ok, absorption = pcall(function()
			return (v :: any).AcousticAbsorption
		end)
		if ok and type(absorption) == "number" then
			out.acoustic_absorption = num(absorption)
		end
		return out
	elseif t == "Ray" then
		return { origin = xyz(v.Origin), direction = xyz(v.Direction) }
	elseif t == "Faces" then
		local out = {}
		for _, face in Enum.NormalId:GetEnumItems() do
			if (v :: any)[face.Name] then
				table.insert(out, face.Name)
			end
		end
		return out
	elseif t == "Axes" then
		local out = {}
		for _, axis in { "X", "Y", "Z" } do
			if (v :: any)[axis] then
				table.insert(out, axis)
			end
		end
		return out
	elseif t == "SecurityCapabilities" then
		local out = {}
		for _, item in Enum.SecurityCapability:GetEnumItems() do
			if v:Contains(item) then
				table.insert(out, item.Name)
			end
		end
		return out
	elseif t == "Instance" then
		local id = ids[v]
		if id then
			return id
		end
		local path = names(v)
		if path == nil then
			error(`{where} points at {v:GetFullName()}, which isn't in the place`)
		end
		return path
	end
	error(`{where} has a {t}, which forge can't write`)
end

-- A model file of `inst` alone, without its children, as base64 (decision 0011). SerializeInstancesAsync has been
-- seen never to answer, so it has a time limit.
local function model(inst: Instance, timeoutSeconds: number): string
	-- Clone gives nil for an instance that can't be copied, which its type doesn't say.
	local cloned = inst:Clone() :: Instance?
	if cloned == nil then
		error(`{inst:GetFullName()} can't be copied for its model file`)
	end
	local copy: Instance = cloned
	copy:ClearAllChildren()
	local result: { any }? = nil
	task.spawn(function()
		result = { pcall(SerializationService.SerializeInstancesAsync, SerializationService, { copy }) }
	end)
	local deadline = os.clock() + timeoutSeconds
	while result == nil and os.clock() < deadline do
		task.wait()
	end
	if result == nil then
		error(
			`{inst:GetFullName()}: its model file didn't come back within {timeoutSeconds * 1000} ms (sync.model_timeout_ms)`
		)
	end
	copy:Destroy()
	local done = result :: { any }
	if not done[1] then
		error(`{inst:GetFullName()}: its model file failed: {done[2]}`)
	end
	return buffer.tostring(EncodingService:Base64Encode(done[2]))
end

export type ReadRequest = {
	trees: { { string } },
	classes: { string },
	properties: { [string]: { [string]: string } },
	methods: { [string]: { [string]: string } },
	models: { string },
	modelTimeoutMs: number,
	yieldEvery: number,
}

-- Values read through a method instead of a property, by `Class.Property`, with the value they have when unset.
local METHODS: { [string]: { get: (Instance) -> any, default: any } } = {
	["Model.ScaleFactor"] = {
		get = function(inst)
			return (inst :: Model):GetScale()
		end,
		default = 1,
	},
}

function read.trees(request: ReadRequest): Json
	local known = {}
	for _, class in request.classes do
		known[class] = true
	end
	local isModel = {}
	for _, class in request.models do
		isModel[class] = true
	end
	local nextId = 0
	local sinceYield = 0
	local out = {}
	lastRead = {}
	for _, path in request.trees do
		local started = os.clock()
		local list = saved(resolve(path))
		local ids: { [Instance]: number } = {}
		for _, inst in list do
			nextId += 1
			ids[inst] = nextId
			lastRead[nextId] = inst
		end
		local nodes: { [Instance]: Json } = {}
		local drafts = {}
		for _, inst in list do
			local class = inst.ClassName
			if not known[class] then
				error(
					`{inst:GetFullName()} is a {class}, which wasn't there a moment ago: the tree changed during pull, run it again`
				)
			end
			local where = inst:GetFullName()
			local properties = {}
			local toRead: { [string]: string } = request.properties[class] or {}
			for name, ty in toRead do
				if inst:IsPropertyModified(name) then
					local ok, value = pcall(function()
						return (inst :: any)[name]
					end)
					if ok and value ~= nil then
						properties[name] = { type = ty, value = encode(value, ids, `{where}.{name}`) }
					end
				end
			end
			local byMethod: { [string]: string } = request.methods[class] or {}
			for name, ty in byMethod do
				local method = METHODS[`{class}.{name}`]
				if method == nil then
					error(`forge's plugin has no reader for {class}.{name}: run forge plugin install`)
				end
				local value = method.get(inst)
				if value ~= method.default then
					properties[name] = { type = ty, value = encode(value, ids, `{where}.{name}`) }
				end
			end
			local attributes = {}
			for name, value in inst:GetAttributes() do
				attributes[name] = { type = typeof(value), value = encode(value, ids, `{where} attribute {name}`) }
			end
			local node: Json = {
				id = ids[inst],
				class = class,
				name = inst.Name,
				properties = properties,
				attributes = attributes,
				tags = inst:GetTags(),
				children = {},
			}
			if inst:IsA("LuaSourceContainer") then
				local script = inst :: LuaSourceContainer
				node.source = ScriptEditorService:GetEditorSource(script)
				if hasDraft(script, node.source) then
					table.insert(drafts, where)
				end
			end
			if isModel[class] then
				node.model = model(inst, request.modelTimeoutMs / 1000)
			end
			nodes[inst] = node
			local parent = inst.Parent
			if parent and nodes[parent] and inst ~= list[1] then
				table.insert(nodes[parent].children, node)
			end
			sinceYield += 1
			if sinceYield >= request.yieldEvery then
				sinceYield = 0
				task.wait()
			end
		end
		table.insert(out, { tree = nodes[list[1]], ms = (os.clock() - started) * 1000, drafts = drafts })
	end
	return { trees = out }
end

return read
