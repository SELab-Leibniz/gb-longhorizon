# 116 — An upload succeeds, but the game never shows up

**Reporter:** user · **Component:** game library (`gb-web`)

I uploaded `jam=2024.gb` (a game jam entry). The upload answered
`201 Created` with the game's details, but the game isn't in the library
list, and `GET /api/games/{id}` says `not_found`. Uploading the file again
says it's a duplicate. Other uploads from the same session are fine.
