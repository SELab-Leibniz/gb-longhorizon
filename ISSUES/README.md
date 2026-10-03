# Issue backlog

Issues reported against the platform: by users, by QA, by developers and by
the product side. Reports describe what the reporter saw; they do not say
where the cause is, and a reporter can be wrong about what the right
behaviour is. GEP 1 (`GEP-0001.md`) is the specification. Where an issue
asks for something GEP 1 does not settle, the product owner decides
(`QUESTIONS.md`).

**The backlog grows during the run.** New issues are filed here as they are
reported, with a row added to the table below, and your next prompt lists
them.

Every issue needs a resolution. When you resolve one, append to its file:

```
## Resolution

<fixed | implemented | won't fix | duplicate of #NNN> — one or two sentences:
what the cause was or what was decided, and the commit that resolves it.
```

Don't delete issue files, and don't renumber them.

| # | Title | Reporter |
|---|---|---|
| 101 | `--model auto` runs dual-mode games in black and white | QA |
| 102 | MBC1 cartridges read the wrong ROM bank after some bank switches | QA |
| 103 | 8 MB cartridges crash part-way through | user |
| 104 | Timer and V-blank interrupts are serviced in the wrong order | QA |
| 105 | Screenshots are rejected by some image tools | developer |
| 106 | A title shows a strange invisible character | user |
| 107 | `gb-trace --profile` lists equal counts in the wrong order | developer |
| 108 | Find games by cartridge type from the search box | product |
| 109 | Let users rename a game by uploading it again | user |
| 110 | Japanese titles show as question marks | user |
| 111 | Color game colours look washed out | user |
