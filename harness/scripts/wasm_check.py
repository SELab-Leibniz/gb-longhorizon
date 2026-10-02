#!/usr/bin/env python3
"""CR-4 check: the WebAssembly build produces the same frames as the native build.

    wasm_check.py GB_WASM.wasm --gb GB_CLI --roms ROMS [--staged STAGED]

For each case (ROM, model, frames, held buttons) runs the module in Node.js
through the ABI in docs/specs/portability.md and the native `gb` CLI, and
compares the FNV-1a-64 frame hashes. Prints one JSON line:
{"score", "instantiates", "cases": {...}}.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

JS = r"""
const fs = require("fs");
const [wasmPath, casesPath] = process.argv.slice(2);
const cases = JSON.parse(fs.readFileSync(casesPath, "utf8"));
function fnv(bytes) {
  let h = 0xcbf29ce484222325n;
  for (const b of bytes) { h ^= BigInt(b); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
  return h.toString(16).padStart(16, "0");
}
(async () => {
  const out = {};
  let mod;
  try { mod = await WebAssembly.compile(fs.readFileSync(wasmPath)); }
  catch (e) { console.log(JSON.stringify({ instantiates: false, error: "compile: " + e.message })); return; }
  const imports = WebAssembly.Module.imports(mod);
  for (const c of cases) {
    try {
      const { exports: e } = await WebAssembly.instantiate(mod, {});
      const rom = fs.readFileSync(c.rom);
      const p = e.gb_alloc(rom.length);
      new Uint8Array(e.memory.buffer, p, rom.length).set(rom);
      const st = e.gb_load(p, rom.length, c.model === "cgb" ? 1 : 0);
      if (st !== 0) { out[c.name] = { error: "gb_load returned " + st }; continue; }
      e.gb_set_buttons(c.mask);
      e.gb_run_frames(c.frames);
      const len = e.gb_frame_len();
      out[c.name] = { hash: fnv(new Uint8Array(e.memory.buffer, e.gb_frame_ptr(), len)), len };
    } catch (err) { out[c.name] = { error: String(err && err.message || err) }; }
  }
  console.log(JSON.stringify({ instantiates: true, imports: imports.length, cases: out }));
})();
"""

MASK = {"RIGHT": 1, "LEFT": 2, "UP": 4, "DOWN": 8, "A": 16, "B": 32, "SELECT": 64, "START": 128}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wasm")
    ap.add_argument("--gb", required=True)
    ap.add_argument("--roms", type=Path, required=True)
    ap.add_argument("--staged", type=Path)
    a = ap.parse_args()
    cases = [
        {"name": "dmg-acid2", "rom": a.roms / "test/acid2/dmg-acid2.gb", "model": "dmg", "frames": 120, "buttons": []},
        {"name": "tobudx-dmg", "rom": a.roms / "games/tobudx.gb", "model": "dmg", "frames": 400, "buttons": []},
        {"name": "2048-start", "rom": a.roms / "games/2048.gb", "model": "dmg", "frames": 300, "buttons": ["START"]},
    ]
    if a.staged:
        cases += [
            {"name": "cgb-acid2", "rom": a.staged / "CR-1/roms/test/cgb-acid2/cgb-acid2.gbc", "model": "cgb", "frames": 60, "buttons": []},
            {"name": "ucity-cgb", "rom": a.staged / "CR-1/roms/games-cgb/ucity.gbc", "model": "cgb", "frames": 400, "buttons": []},
        ]
    cases = [c for c in cases if Path(c["rom"]).exists()]
    native = {}
    with tempfile.TemporaryDirectory() as td:
        for c in cases:
            script = Path(td) / f"{c['name']}.input"
            script.write_text(f"0 {','.join(c['buttons'])}\n" if c["buttons"] else "")
            p = subprocess.run([a.gb, "--rom", str(c["rom"]), "--frames", str(c["frames"]), "--model", c["model"],
                                "--hash", "--input-script", str(script)], capture_output=True, text=True, timeout=600)
            m = re.search(r"final ([0-9a-f]{16})", p.stdout)
            native[c["name"]] = m.group(1) if m else None
        js = Path(td) / "check.js"
        js.write_text(JS)
        cj = Path(td) / "cases.json"
        cj.write_text(json.dumps([{"name": c["name"], "rom": str(c["rom"]), "model": c["model"], "frames": c["frames"],
                                   "mask": sum(MASK[b] for b in c["buttons"])} for c in cases]))
        p = subprocess.run(["node", str(js), a.wasm, str(cj)], capture_output=True, text=True, timeout=1800)
    try:
        res = json.loads(p.stdout.strip().splitlines()[-1])
    except Exception:
        print(json.dumps({"score": 0.0, "instantiates": False, "detail": (p.stdout + p.stderr)[-800:]}))
        return 0
    results = {}
    for c in cases:
        w = res.get("cases", {}).get(c["name"], {})
        ok = native[c["name"]] is not None and w.get("hash") == native[c["name"]]
        results[c["name"]] = {"ok": ok, "wasm": w.get("hash") or w.get("error"), "native": native[c["name"]]}
    no_imports = res.get("instantiates") and res.get("imports") == 0
    score = (sum(r["ok"] for r in results.values()) / len(results)) if results and no_imports else 0.0
    print(json.dumps({"score": round(score, 4), "instantiates": res.get("instantiates"), "imports": res.get("imports"),
                      "cases": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
