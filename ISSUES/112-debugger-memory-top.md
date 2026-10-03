# 112 — Debugger memory reads repeat a byte at the top of memory

**Reporter:** developer · **Component:** `gb-server`

Reading a range that runs past the top of memory returns the last byte over
and over, e.g. `GET /memory?addr=0xFFFE&len=4` gives four bytes whose last
three are the same as the byte at `$FFFF`. Reads elsewhere are fine.
