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
}

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

function link.start(settings: Settings): Connection
	local running = true
	local current: WebStreamClient? = nil
	-- A server error is shown once, not on every try (an old plugin keeps getting the same one).
	local lastError: string? = nil
	local session = HttpService:GenerateGUID(false)
	local url = `ws://127.0.0.1:{settings.port}`

	local function handle(raw: string)
		local ok, message = pcall(HttpService.JSONDecode, HttpService, raw)
		if not ok or type(message) ~= "table" then
			return
		end
		if message.op == "welcome" then
			lastError = nil
			print(`forge: linked to forge serve {message.version} on port {settings.port}`)
		elseif message.op == "error" and message.message ~= lastError then
			lastError = message.message
			warn(`forge: {message.message}`)
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
		client.MessageReceived:Connect(handle)
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
		while running do
			if try() and running then
				print("forge: lost the link to forge serve, trying again in the background")
			end
			if running then
				task.wait(settings.retrySeconds)
			end
		end
	end)

	return {
		stop = function()
			running = false
			local client = current
			if client then
				pcall(client.Close, client)
			end
		end,
	}
end

return link
