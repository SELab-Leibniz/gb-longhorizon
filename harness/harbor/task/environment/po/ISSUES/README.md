# Issue backlog

Open issues inherited from the previous team, as they were reported: by
users, by QA, and by the product side. Reports describe what the reporter
saw; they do not say where the cause is, and a reporter can be wrong about
what the right behaviour is. GEP 1 (`GEP-0001.md`) is the specification.
Where an issue asks for something GEP 1 does not settle, the product owner
decides (`QUESTIONS.md`).

Every issue needs a resolution. When you resolve one, append to its file:

```
## Resolution

<fixed | implemented | won't fix | duplicate of #NNN> — one or two sentences:
what the cause was or what was decided, and the commit that resolves it.
```

Don't delete issue files, and don't renumber them.

| # | Title | Reporter |
|---|---|---|
| 101 | Writing all eight Color background palettes leaves half of them unset | QA |
| 102 | Library statistics: games per cartridge type | product |
| 103 | 8 MB cartridges crash part-way through | user |
| 104 | Japanese titles show as question marks | user |
| 105 | Two-digit counters skip from 98 to garbage | user |
| 106 | Browser player: A and B are swapped | user |
| 107 | Find games by cartridge type from the search box | product |
| 108 | Screenshots are rejected by some image tools | developer |
| 109 | `--model auto` runs dual-mode games in black and white | QA |
| 110 | Let users rename a game by uploading it again | user |
| 111 | `cpu_instrs` 04, 09 and 11 fail | QA |
| 112 | Debugger memory reads repeat a byte at the top of memory | developer |
| 113 | Color game colours look washed out | user |
| 114 | Timer and V-blank interrupts are serviced in the wrong order | QA |
| 115 | The server accepts save files of the wrong size | QA |
| 116 | Show newest uploads first | user |
| 117 | A title shows a strange invisible character | user |
| 118 | MBC1 cartridges read the wrong ROM bank after some bank switches | QA |
| 119 | Downloaded save files have a useless name | user |
| 120 | `gb-trace --profile` lists equal counts in the wrong order | developer |
| 121 | Homebrew ROMs over 8 MB are rejected | user |
