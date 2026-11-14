--!strict
-- What `require("@wingine")` returns, for luau-lsp: the engine's API, whose types `wingine.d.luau`
-- declares. The engine never runs this file: it answers `@wingine` itself, before looking at any
-- `.luaurc`, so a game aliases `wingine` to this file, or a copy of it, only for its language
-- server.
return (nil :: any) :: Wingine
