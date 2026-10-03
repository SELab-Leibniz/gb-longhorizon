# 105 — Screenshots are rejected by some image tools

**Reporter:** developer · **Component:** game library (`gb-web`)

Our thumbnail pipeline refuses the screenshots from
`/api/games/{id}/screenshot.png`. Python's decoder says:

```
zlib.error: Error -3 while decompressing data: incorrect data check
```

The PNG chunk structure itself looks fine.
