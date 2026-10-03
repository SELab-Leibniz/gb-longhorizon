# 121 — Searching for "C++" finds nothing

**Reporter:** developer · **Component:** game library (`gb-web`)

Our chat bot searches the library through the API. We have a game titled
`QUEST C++`. `GET /api/games?q=QUEST` finds it, but searching for `C++`
(sent as `q=C%2B%2B`, the way every HTTP client encodes it) returns no
games at all.
