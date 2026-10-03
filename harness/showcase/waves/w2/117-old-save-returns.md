# 117 — A deleted game's save comes back

**Reporter:** user · **Component:** game library (`gb-web`)

I deleted a game I had played, and some days later uploaded the same ROM
again. Its old save was back (`GET /api/games/{id}/save` returns it), as if
the game had never been deleted. I expected a clean start.
