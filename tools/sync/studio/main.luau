--!strict
-- forge sync's Studio side: answers the requests `forge sync` sends through the link. Packed into forge's plugin as
-- tools/sync (docs/link.md).

local read = require(script.Parent.read)
local write = require(script.Parent.write)

local sync = {}

export type Context = {
	-- Shows forge's confirm window and waits: "apply", "cancel" or "timeout".
	confirm: (ask: any) -> string,
}

-- A push waits for Apply in Studio before anything changes (decision 0015).
local function push(request: any, context: Context): any
	local warnings = {}
	if #request.drafts > 0 then
		table.insert(warnings, "These scripts have an uncommitted draft, which the push writes over:")
		for _, name in request.drafts do
			table.insert(warnings, `  {name}`)
		end
	end
	local choice = context.confirm({
		title = "forge sync push",
		summary = `Push from the files: {request.summary}`,
		lines = request.lines,
		warnings = warnings,
		timeoutSeconds = request.timeoutMs / 1000,
	})
	if choice == "apply" then
		write.apply(request.pushTrees)
		return { result = "applied" }
	end
	return { result = if choice == "timeout" then "timeout" else "cancelled" }
end

function sync.handle(request: any, context: Context): any
	if request.action == "classes" then
		return read.classes(request.trees)
	elseif request.action == "read" then
		return read.trees(request)
	elseif request.action == "drafts" then
		return read.drafts(request.trees)
	elseif request.action == "push" then
		return push(request, context)
	end
	error(`forge's plugin has no sync action {request.action}: run forge plugin install`)
end

return sync
