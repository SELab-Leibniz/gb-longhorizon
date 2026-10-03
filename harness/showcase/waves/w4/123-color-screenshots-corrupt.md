# 123 — Screenshots of Color games can't be decoded

**Reporter:** developer · **Component:** game library (`gb-web`)

Our thumbnail pipeline (Python) fails on screenshots of Game Boy Color
games — for example after uploading `ucity.gbc` from `roms/games-cgb/`, or
for any game with `?model=cgb`:

```
zlib.error: Error -3 while decompressing data: invalid stored block lengths
```

Screenshots of original Game Boy games decode fine. Browsers show the Color
screenshots without complaint, so nobody noticed until now.
