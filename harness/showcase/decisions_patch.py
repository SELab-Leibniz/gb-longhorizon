#!/usr/bin/env python3
"""Reference implementation of the product decisions P1-P3 (harness/HIDDEN_SPEC.md)
on the pilot's gb-web, used only to show the hidden decision checks can pass.

    decisions_patch.py REPO
"""
import sys
from pathlib import Path

EDITS = [
    # P1: `q` also matches the mapper name
    ("""        if !game.title.to_lowercase().contains(&needle)
            && !game.filename.to_lowercase().contains(&needle)
        {""",
     """        if !game.title.to_lowercase().contains(&needle)
            && !game.filename.to_lowercase().contains(&needle)
            && !game.mapper.to_lowercase().contains(&needle)
        {"""),
    # P2: save downloads are named <filename stem>.sav
    ("""            Some(data) => Response::new(200, "application/octet-stream", data),""",
     """            Some(data) => {
                let stem = game.filename.rsplit_once('.').map_or(game.filename.as_str(), |(s, _)| s);
                Response::new(200, "application/octet-stream", data).with_header(
                    "Content-Disposition",
                    format!("attachment; filename=\\"{}\\"", attachment_name(&format!("{stem}.sav"))),
                )
            }"""),
    # P3: per-mapper counts in /api/stats
    ("""    let stats = lock(&app.stats);
    let body = format!(
        "{{\\"games\\":{},""",
     """    let stats = lock(&app.stats);
    let mut by_mapper = std::collections::BTreeMap::new();
    for game in app.store.games() {
        *by_mapper.entry(game.mapper).or_insert(0u64) += 1;
    }
    let by_mapper = by_mapper
        .iter()
        .map(|(m, n)| format!("\\"{m}\\":{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        "{{\\"by_mapper\\":{{{by_mapper}}},\\"games\\":{},"""),
]

if __name__ == "__main__":
    p = Path(sys.argv[1]) / "gb-web/src/main.rs"
    s = p.read_text()
    for old, new in EDITS:
        if s.count(old) != 1:
            raise SystemExit(f"decisions_patch: expected one match for {old[:60]!r}, found {s.count(old)}")
        s = s.replace(old, new)
    p.write_text(s)
    print("decisions P1-P3 applied")
