# 112 — A "press any key" pause only continues when the key is released

**Reporter:** developer · **Component:** emulator

My homebrew pauses with the usual low-power loop: select the buttons in
`P1`, enable only the joypad interrupt, `HALT`, and continue when it wakes
up. On this emulator the game continues only when I *let go* of the
button, not when I press it. On hardware (and in other emulators) it
continues on the press.
