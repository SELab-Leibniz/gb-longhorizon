# 120 — Uploads from our branch office are rejected

**Reporter:** operations · **Component:** game library (`gb-web`)

Our branch office uploads games over a slow, bursty connection. There,
uploads of bigger ROMs often fail with `400 not_a_rom`, although the files
are fine: the same files upload without problems from the main office, and
small ROMs usually go through from the branch office too.
