--!strict
-- forge sync's Studio side: answers the requests `forge sync` sends through the link. Packed into forge's plugin as
-- tools/sync (docs/link.md).

local read = require(script.Parent.read)

local sync = {}

function sync.handle(request: any): any
	if request.action == "classes" then
		return read.classes(request.trees)
	elseif request.action == "read" then
		return read.trees(request)
	end
	error(`forge's plugin has no sync action {request.action}: run forge plugin install`)
end

return sync
