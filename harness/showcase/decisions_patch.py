#!/usr/bin/env python3
"""Reference implementation of the product decisions that add behaviour
(P1-P3, P8, P9 in harness/HIDDEN_SPEC.md) on the pilot's gb-web, used only to
show the hidden decision checks can pass. The other decisions (P4-P7) keep the
specified behaviour and need no code.

    decisions_patch.py REPO
"""
import sys
from pathlib import Path

MAIN = [
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
    # P8 + P9: routes
    ("""        ["games", id, "save"] => api_save(app, id, request),""",
     """        ["games", id, "save"] => api_save(app, id, request),
        ["games", id, "favorite"] => api_favorite(app, id, &request.method),
        ["export"] => api_export(app, &request.method),"""),
    # P8: `favorite` list filter
    ("""    playable: Option<bool>,
    sort: Option<String>,""",
     """    playable: Option<bool>,
    favorite: Option<bool>,
    sort: Option<String>,"""),
    ("""                _ => return bad_request("`playable` must be true or false"),
            },""",
     """                _ => return bad_request("`playable` must be true or false"),
            },
            "favorite" => match value.as_str() {
                "true" => query.favorite = Some(true),
                "false" => query.favorite = Some(false),
                _ => return bad_request("`favorite` must be true or false"),
            },"""),
    ("""    if let Some(playable) = query.playable {
        if game.playable != playable {
            return false;
        }
    }""",
     """    if let Some(playable) = query.playable {
        if game.playable != playable {
            return false;
        }
    }
    if let Some(favorite) = query.favorite {
        if game.favorite != favorite {
            return false;
        }
    }"""),
    # P8 + P9: handlers
    ("""fn find_game(app: &App, id: &str) -> Option<Game> {""",
     """fn api_favorite(app: &App, id: &str, method: &str) -> Response {
    if !valid_id(id) || find_game(app, id).is_none() {
        return Response::error(404, "unknown game", "not_found");
    }
    let on = match method {
        "PUT" => true,
        "DELETE" => false,
        _ => return method_not_allowed(),
    };
    match app.store.set_favorite(id, on) {
        Ok(()) => Response::empty(204),
        Err(error) => {
            eprintln!("gb-web: favorite failed: {error}");
            Response::error(500, "could not store the favourite", "emulation_failed")
        }
    }
}

fn api_export(app: &App, method: &str) -> Response {
    if method != "GET" {
        return method_not_allowed();
    }
    let mut games = app.store.games();
    games.sort_by(|a, b| a.added.cmp(&b.added).then_with(|| a.id.cmp(&b.id)));
    let entries: Vec<String> = games
        .iter()
        .map(|g| {
            format!(
                "{{\\"id\\":\\"{}\\",\\"title\\":\\"{}\\",\\"filename\\":\\"{}\\",\\"added\\":{},\\"has_save\\":{}}}",
                json::escape(&g.id),
                json::escape(&g.title),
                json::escape(&g.filename),
                g.added,
                app.store.save(&g.id).is_some()
            )
        })
        .collect();
    let body = format!("{{\\"version\\":1,\\"games\\":[{}]}}", entries.join(","));
    Response::json(200, body).with_header("Content-Disposition", "attachment; filename=\\"library.json\\"".to_string())
}

fn find_game(app: &App, id: &str) -> Option<Game> {"""),
]

STORE = [
    ("""    pub playable: bool,
    pub added: u64,
}""",
     """    pub playable: bool,
    pub added: u64,
    pub favorite: bool,
}"""),
    ("""            playable: is_playable(cart_type),
            added,
        }""",
     """            playable: is_playable(cart_type),
            added,
            favorite: false,
        }"""),
    ("""            playable: playable?,
            added: added?,
        })""",
     """            playable: playable?,
            added: added?,
            favorite: false,
        })"""),
    ("""\\"playable\\":{},\\"added\\":{}}}",""",
     """\\"playable\\":{},\\"added\\":{},\\"favorite\\":{}}}","""),
    ("""            self.playable,
            self.added,
        )""",
     """            self.playable,
            self.added,
            self.favorite,
        )"""),
    ("""            if let Some(game) = Game::from_meta(&text) {""",
     """            if let Some(mut game) = Game::from_meta(&text) {
                game.favorite = self.fav_path(&game.id).is_file();"""),
    ("""    fn save_path(&self, id: &str) -> PathBuf {""",
     """    fn fav_path(&self, id: &str) -> PathBuf {
        self.root.join("games").join(format!("{id}.fav"))
    }

    /// Mark or unmark a game as a favourite (a marker file next to its metadata).
    pub fn set_favorite(&self, id: &str, on: bool) -> io::Result<()> {
        if on {
            write_atomic(&self.fav_path(id), b"")
        } else {
            remove_if_exists(&self.fav_path(id))
        }
    }

    fn save_path(&self, id: &str) -> PathBuf {"""),
    ("""        remove_if_exists(&self.meta_path(id))?;""",
     """        remove_if_exists(&self.fav_path(id))?;
        remove_if_exists(&self.meta_path(id))?;"""),
]


def patch(path: Path, edits):
    s = path.read_text()
    for old, new in edits:
        if s.count(old) != 1:
            raise SystemExit(f"decisions_patch: expected one match in {path.name} for {old[:70]!r}, found {s.count(old)}")
        s = s.replace(old, new)
    path.write_text(s)


if __name__ == "__main__":
    repo = Path(sys.argv[1])
    patch(repo / "gb-web/src/main.rs", MAIN)
    patch(repo / "gb-web/src/store.rs", STORE)
    print("decisions P1-P3, P8, P9 applied")
