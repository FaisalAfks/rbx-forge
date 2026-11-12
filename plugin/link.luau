--!strict
-- The plugin's side of forge's link (decisions 0012 and 0015, docs/link.md): a WebSocket to `forge serve` on
-- 127.0.0.1. It tries again and again in the background, so the server can start and stop at any time. It never
-- prompts and never blocks Studio, and failed tries print nothing: Studio reports a refused connection as Error then
-- Closed after about 2 s, and logs nothing of its own.

local HttpService = game:GetService("HttpService")

export type Settings = {
	version: string,
	protocol: number,
	port: number,
	retrySeconds: number,
	openTimeoutSeconds: number,
}

export type Connection = {
	stop: () -> (),
	-- Sends an event to forge serve, if the link is up (it's dropped otherwise: nobody would hear it).
	notify: (event: any) -> (),
}

-- Answers one request the server passes on from a forge command, or errors with why it can't.
export type Handler = (request: any) -> any

-- Told when the link's state changes: "linked", "trying" or "off", and a detail line.
export type OnStatus = (state: string, detail: string) -> ()

local link = {}

-- What the plugin tells the server about this Studio.
local function hello(settings: Settings, session: string): string
	return HttpService:JSONEncode({
		op = "hello",
		role = "studio",
		protocol = settings.protocol,
		plugin = settings.version,
		session = session,
		place = game.PlaceId,
		game = game.GameId,
		name = game.Name,
	})
end

function link.start(settings: Settings, handler: Handler, onStatus: OnStatus): Connection
	local running = true
	local shown = ""
	local function status(state: string, detail: string)
		if `{state}|{detail}` ~= shown then
			shown = `{state}|{detail}`
			onStatus(state, detail)
		end
	end
	local current: WebStreamClient? = nil
	-- A server error is shown once, not on every try (an old plugin keeps getting the same one).
	local lastError: string? = nil
	local session = HttpService:GenerateGUID(false)
	local url = `ws://127.0.0.1:{settings.port}`

	-- Runs a request apart from the link, so a long one (a whole pull) never holds up other messages, and sends the
	-- answer back on the connection it came in on.
	local function answer(client: WebStreamClient, message: any)
		task.spawn(function()
			local ok, result = pcall(handler, message.request)
			local reply = if ok
				then { op = "reply", id = message.id, reply = result }
				else { op = "failed", id = message.id, message = tostring(result) }
			local encoded, text = pcall(HttpService.JSONEncode, HttpService, reply)
			if not encoded then
				text = HttpService:JSONEncode({
					op = "failed",
					id = message.id,
					message = `can't send the answer: {text}`,
				})
			end
			pcall(client.Send, client, text)
		end)
	end

	local function handle(client: WebStreamClient, raw: string)
		local ok, message = pcall(HttpService.JSONDecode, HttpService, raw)
		if not ok or type(message) ~= "table" then
			return
		end
		if message.op == "request" then
			answer(client, message)
		elseif message.op == "welcome" then
			lastError = nil
			print(`forge: linked to forge serve {message.version} on port {settings.port}`)
			status("linked", `forge serve {message.version}, port {settings.port}`)
		elseif message.op == "error" and message.message ~= lastError then
			lastError = message.message
			warn(`forge: {message.message}`)
			status("trying", message.message)
		end
	end

	-- One try: open, say hello, and hold the connection until it closes. Returns whether it opened.
	local function try(): boolean
		local ok, client =
			pcall(HttpService.CreateWebStreamClient, HttpService, Enum.WebStreamClientType.WebSocket, { Url = url })
		if not ok then
			return false
		end
		current = client
		local thread = coroutine.running()
		local opened = false
		local over = false
		-- Error and Closed both fire when a try fails, and the timeout may too: only the first resumes the loop.
		local function finish()
			if not over then
				over = true
				task.spawn(thread)
			end
		end
		client.Opened:Connect(function()
			opened = true
			client:Send(hello(settings, session))
		end)
		client.MessageReceived:Connect(function(raw)
			handle(client, raw)
		end)
		client.Error:Connect(finish)
		client.Closed:Connect(finish)
		task.delay(settings.openTimeoutSeconds, function()
			if not opened then
				finish()
			end
		end)
		coroutine.yield()
		current = nil
		-- Closing frees the client: Studio allows only four at a time.
		pcall(client.Close, client)
		return opened
	end

	task.spawn(function()
		status("trying", `port {settings.port}`)
		while running do
			if try() and running then
				print("forge: lost the link to forge serve, trying again in the background")
				status("trying", `lost the link; trying port {settings.port} again`)
			end
			if running then
				task.wait(settings.retrySeconds)
			end
		end
	end)

	return {
		notify = function(event)
			local client = current
			if client then
				pcall(client.Send, client, HttpService:JSONEncode({ op = "notify", event = event }))
			end
		end,
		stop = function()
			running = false
			status("off", "")
			local client = current
			if client then
				pcall(client.Close, client)
			end
		end,
	}
end

return link
