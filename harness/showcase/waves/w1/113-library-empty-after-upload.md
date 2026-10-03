# 113 — After one upload the library page is empty

**Reporter:** user · **Component:** game library (`gb-web`)

I uploaded my homebrew `say-hi.gb` (its title is `SAY "HI"`). The upload
went through, but since then the library page doesn't show any games at
all, and `GET /api/games` returns something our scripts can't parse as
JSON. When I delete that game through the API, everything is back to
normal.
