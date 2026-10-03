# 115 — The server accepts save files of the wrong size

**Reporter:** QA · **Component:** game library (`gb-web`)

`PUT /api/games/{id}/save` with a body longer than the game's `ram_size`
is accepted (`204`), and the stored save is then longer than the
cartridge RAM. A body that is too short is correctly rejected.
