--!strict
-- Requires that only resolve through the sourcemap: a module in another tree, and a Wally package.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Counter = require(ReplicatedStorage.Shared.Util.Counter)
local Lib = require(ReplicatedStorage.Packages.Lib)

local total: number = Counter.add(Lib.answer, 1)
print(total)
