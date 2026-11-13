//! What a commit keeps of a row as it was, when it is long: kept in pieces
//! (`specboard://SpecBoard/Docs/Design:DesignBranches` §11). A long text is cut where what it says
//! decides, each piece is held once for the project (`history_chunks`), and the row keeps the list of
//! its pieces. A document changed a little then adds only the pieces around the change, and every
//! row still reads whole by itself: no row leans on another, nor on the document as it is now.

use super::db;
use rusqlite::types::{FromSql, FromSqlResult, Value as Sql, ValueRef};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// A row shorter than this is kept as its text: cut in pieces, it would take more than it saves.
pub(crate) const INLINE: usize = 4096;

/// No piece but a text's last is shorter than this: a piece costs its row and its place in a list.
const SHORTEST: usize = 1024;

/// No piece is longer than this, whatever the text says: a change costs at most this much anew.
const LONGEST: usize = 16 * 1024;

/// Past the shortest, a piece ends where these bits of the value run over the text are all clear:
/// one place in 2,048, so a piece is about 3 KB. The highest bits, which the 64 bytes before the
/// place all reach (a lower bit is of the last few bytes alone).
const EDGE: u64 = 0xFFE0_0000_0000_0000;

/// The longest a row of the store is: a list that says more in all is no row's.
const WHOLE: usize = 1_000_000_000;

/// The form of a list of pieces, its first byte: what follows is pairs of whole numbers, a piece's
/// number and its length, seven bits to a byte, the lowest first.
const FORM: u8 = 1;

/// How many bytes of a piece's digest are kept to find it by.
const DIGEST: usize = 16;

/// What each byte adds to the value run over a text (`edge`): fixed, so that every build cuts a
/// text at the same places and finds the pieces an earlier one kept.
const GEAR: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut i = 0;
    while i < 256 {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        table[i] = z ^ (z >> 31);
        i += 1;
    }
    table
};

fn unreadable() -> String {
    text!("history.row.unreadable")
}

/// What a commit's row keeps of the row as it was (`commit_rows.before`): nothing (the row was not
/// there), its text, or the list of the pieces it is kept in.
pub(crate) enum Before {
    None,
    Text(String),
    Pieces(Vec<u8>),
}

impl FromSql for Before {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(match value {
            ValueRef::Null => Before::None,
            ValueRef::Blob(list) => Before::Pieces(list.to_vec()),
            other => Before::Text(String::column_result(other)?),
        })
    }
}

impl Before {
    /// The row as it was, whole; none where it was not there. A list that names a piece that is
    /// gone, or one not as long as the list says, is refused: never read short.
    pub(crate) fn text(self, conn: &Connection) -> Result<Option<String>, String> {
        let list = match self {
            Before::None => return Ok(None),
            Before::Text(text) => return Ok(Some(text)),
            Before::Pieces(list) => list,
        };
        let pieces = listed(&list).ok_or_else(unreadable)?;
        let total = pieces
            .iter()
            .try_fold(0usize, |sum, (_, length)| sum.checked_add(*length))
            .ok_or_else(unreadable)?;
        // Before a piece is read: a short list naming one piece over and over builds no text.
        if total > WHOLE {
            return Err(unreadable());
        }
        // No more room taken ahead than a row may well be: a list is not trusted to say how long
        // it is before its pieces are read.
        let mut whole = Vec::with_capacity(total.min(1 << 22));
        let mut read = conn
            .prepare_cached("SELECT data FROM history_chunks WHERE id = ?")
            .map_err(db)?;
        for (id, length) in pieces {
            let found = read
                .query_row([id], |r| {
                    let data = r.get_ref(0)?.as_blob()?;
                    Ok((data.len() == length).then(|| whole.extend_from_slice(data)))
                })
                .optional()
                .map_err(db)?;
            if found.flatten().is_none() {
                return Err(unreadable());
            }
        }
        String::from_utf8(whole).map(Some).map_err(|_| unreadable())
    }
}

/// The pieces a list points at, in order: (the piece's number, its length). None for what is no
/// list of the form — one that says a piece longer than any is cut among them: where lengths are
/// added up, none is taken at its word past what a piece can be.
pub(crate) fn listed(list: &[u8]) -> Option<Vec<(i64, usize)>> {
    let (form, rest) = list.split_first()?;
    if *form != FORM {
        return None;
    }
    let mut numbers: Vec<u64> = Vec::new();
    let (mut value, mut shift) = (0u64, 0u32);
    for byte in rest {
        if shift > 56 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            numbers.push(value);
            (value, shift) = (0, 0);
        } else {
            shift += 7;
        }
    }
    if shift != 0 || !numbers.len().is_multiple_of(2) {
        return None;
    }
    numbers
        .chunks(2)
        .map(|pair| {
            let length = usize::try_from(pair[1]).ok().filter(|l| *l <= LONGEST)?;
            Some((i64::try_from(pair[0]).ok()?, length))
        })
        .collect()
}

fn number(list: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        list.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    list.push(value as u8);
}

/// Where the first piece of `text` ends: at the first place past the shortest where the value run
/// over the 64 bytes before it has the edge's bits clear, else at the longest (or the text's end).
/// What decides is the text just before the place alone, so a change elsewhere leaves it where it was.
fn edge(text: &[u8]) -> usize {
    if text.len() <= SHORTEST {
        return text.len();
    }
    let longest = text.len().min(LONGEST);
    let mut value: u64 = 0;
    for (at, byte) in text[..longest].iter().enumerate().skip(SHORTEST - 64) {
        value = (value << 1).wrapping_add(GEAR[usize::from(*byte)]);
        if at + 1 >= SHORTEST && value & EDGE == 0 {
            return at + 1;
        }
    }
    longest
}

/// `text` cut into its pieces, in order.
pub(crate) fn cut(text: &[u8]) -> Vec<&[u8]> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (piece, after) = rest.split_at(edge(rest));
        pieces.push(piece);
        rest = after;
    }
    pieces
}

/// What `commit_rows.before` holds for the row `text` of the project: the text itself when it is
/// short, else the list of its pieces, each put in the project's pieces unless it is there.
pub(crate) fn pack(conn: &Connection, project: &str, text: &str) -> Result<Sql, String> {
    if text.len() < INLINE {
        return Ok(Sql::Text(text.to_string()));
    }
    // Found by the index alone: the pieces themselves are not read to keep a row.
    let mut find = conn
        .prepare_cached("SELECT id FROM history_chunks WHERE project_id = ? AND hash = ?")
        .map_err(db)?;
    let mut add = conn
        .prepare_cached("INSERT INTO history_chunks (project_id, hash, data) VALUES (?, ?, ?)")
        .map_err(db)?;
    let mut list = vec![FORM];
    for piece in cut(text.as_bytes()) {
        let digest = Sha256::digest(piece);
        let hash = &digest[..DIGEST];
        let there: Option<i64> = find
            .query_row(params![project, hash], |r| r.get(0))
            .optional()
            .map_err(db)?;
        let id = match there {
            Some(id) => id,
            None => {
                add.execute(params![project, hash, piece]).map_err(db)?;
                conn.last_insert_rowid()
            }
        };
        number(&mut list, id as u64);
        number(&mut list, piece.len() as u64);
    }
    Ok(Sql::Blob(list))
}

/// The pieces a history row points at, with the tree and the commit the row is of.
type Listed = (String, i64, Vec<(i64, usize)>);

/// The lists the history rows of a project keep, each with the tree and the commit it is of: what
/// the sizes and the letting go read. The pieces themselves are not read — a list says how long
/// each is. `unread`: whether a row keeps what is no list of the form — what that row points at is
/// not known.
struct Lists {
    read: Vec<Listed>,
    unread: bool,
}

fn lists(conn: &Connection, project: &str) -> Result<Lists, String> {
    let mut stmt = conn
        .prepare(
            "SELECT c.branch_id, c.seq, r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id
              WHERE c.project_id = ? AND typeof(r.before) = 'blob'",
        )
        .map_err(db)?;
    let mut rows = stmt.query([project]).map_err(db)?;
    let mut out = Lists {
        read: Vec::new(),
        unread: false,
    };
    while let Some(row) = rows.next().map_err(db)? {
        let list = row
            .get_ref(2)
            .map_err(db)?
            .as_blob()
            .map_err(|e| e.to_string())?;
        match listed(list) {
            Some(pieces) => {
                out.read
                    .push((row.get(0).map_err(db)?, row.get(1).map_err(db)?, pieces))
            }
            None => out.unread = true,
        }
    }
    Ok(out)
}

/// The pieces the rows read by `query` (their `before`, lists alone) point at. A row whose list
/// cannot be read adds none: what it points at is not known.
fn pointed(
    conn: &Connection,
    query: &str,
    values: &[&dyn rusqlite::ToSql],
    into: &mut HashSet<i64>,
) -> Result<(), String> {
    let mut stmt = conn.prepare_cached(query).map_err(db)?;
    let mut rows = stmt.query(values).map_err(db)?;
    while let Some(row) = rows.next().map_err(db)? {
        let list = row
            .get_ref(0)
            .map_err(db)?
            .as_blob()
            .map_err(|e| e.to_string())?;
        into.extend(listed(list).into_iter().flatten().map(|(id, _)| id));
    }
    Ok(())
}

/// The pieces the history rows of the commits `commits` point at: what may be let go of once
/// those commits are gone (`let_go`).
pub(crate) fn pointed_by(conn: &Connection, commits: &[String]) -> Result<HashSet<i64>, String> {
    let mut out = HashSet::new();
    for commit in commits {
        pointed(
            conn,
            "SELECT before FROM commit_rows WHERE commit_id = ? AND typeof(before) = 'blob'",
            &[commit],
            &mut out,
        )?;
    }
    Ok(out)
}

/// The pieces the history rows of a tree point at — of its commits before `before`, or of all.
pub(crate) fn pointed_in(
    conn: &Connection,
    project: &str,
    tree: &str,
    before: Option<i64>,
) -> Result<HashSet<i64>, String> {
    let mut out = HashSet::new();
    pointed(
        conn,
        "SELECT r.before FROM commit_rows r JOIN commits c ON c.id = r.commit_id
          WHERE c.project_id = ?1 AND c.branch_id = ?2 AND (?3 IS NULL OR c.seq < ?3)
            AND typeof(r.before) = 'blob'",
        &[&project, &tree, &before],
        &mut out,
    )?;
    Ok(out)
}

/// Of `pieces` — what rows that are gone pointed at — those no history row of the project points
/// at any more, let go of: how many went. In the write that took the rows. Nothing is read where
/// the rows gone pointed at none; and none goes while a row keeps a list that cannot be read,
/// which may point at any of them.
pub(crate) fn let_go(
    conn: &Connection,
    project: &str,
    pieces: &HashSet<i64>,
) -> Result<usize, String> {
    if pieces.is_empty() {
        return Ok(0);
    }
    let lists = lists(conn, project)?;
    if lists.unread {
        return Ok(0);
    }
    let staying: HashSet<i64> = lists
        .read
        .into_iter()
        .flat_map(|(_, _, pieces)| pieces)
        .map(|(id, _)| id)
        .collect();
    let mut gone = 0;
    let mut delete = conn
        .prepare_cached("DELETE FROM history_chunks WHERE id = ? AND project_id = ?")
        .map_err(db)?;
    for id in pieces.iter().filter(|id| !staying.contains(id)) {
        gone += delete.execute(params![id, project]).map_err(db)?;
    }
    Ok(gone)
}

/// What the history of each of `trees` takes, in their order: what its rows hold themselves (a
/// text, or a list) and how long the pieces they point at are — each piece once, for the first of
/// the trees (main first) whose rows point at it. A row whose list cannot be read is counted by
/// what it holds alone: it is refused to who reads it, not to who asks how much the project takes.
///
/// One pass over the project's rows, the pieces not read (a list says how long each is), and no
/// text read either: a length is asked of the row's header.
pub(crate) fn sizes(conn: &Connection, project: &str, trees: &[&str]) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT c.branch_id, octet_length(r.before), CASE WHEN typeof(r.before) = 'blob' THEN r.before END
               FROM commit_rows r JOIN commits c ON c.id = r.commit_id WHERE c.project_id = ?",
        )
        .map_err(db)?;
    let mut out = vec![0i64; trees.len()];
    // Each piece, with the first of the trees that points at it and how long it is.
    let mut of: HashMap<i64, (usize, usize)> = HashMap::new();
    let mut rows = stmt.query([project]).map_err(db)?;
    while let Some(row) = rows.next().map_err(db)? {
        let tree = row.get_ref(0).map_err(db)?;
        let tree = tree.as_str().map_err(|e| e.to_string())?;
        let Some(at) = trees.iter().position(|t| *t == tree) else {
            continue;
        };
        out[at] += row.get::<_, Option<i64>>(1).map_err(db)?.unwrap_or(0);
        let ValueRef::Blob(list) = row.get_ref(2).map_err(db)? else {
            continue;
        };
        for (id, length) in listed(list).into_iter().flatten() {
            of.entry(id)
                .and_modify(|first| {
                    if first.0 > at {
                        *first = (at, length);
                    }
                })
                .or_insert((at, length));
        }
    }
    for (at, length) in of.into_values() {
        out[at] += length as i64;
    }
    Ok(out)
}

/// How long the pieces are that the commits of `tree` before `before` point at and no row that
/// stays does, of the tree itself or of the trees `listed`: what cutting them takes from the
/// history's size as `sizes` counts it for those trees, besides what the rows hold themselves. A
/// piece a tree not listed still points at (a branch set aside) is no longer counted either.
pub(crate) fn freed(
    conn: &Connection,
    project: &str,
    tree: &str,
    before: i64,
    listed: &[&str],
) -> Result<i64, String> {
    let (mut going, mut staying): (HashMap<i64, usize>, HashSet<i64>) = Default::default();
    for (of, seq, pieces) in lists(conn, project)?.read {
        if of == tree && seq < before {
            going.extend(pieces);
        } else if of == tree || listed.contains(&of.as_str()) {
            staying.extend(pieces.into_iter().map(|(id, _)| id));
        }
    }
    Ok(going
        .into_iter()
        .filter(|(id, _)| !staying.contains(id))
        .map(|(_, length)| length as i64)
        .sum())
}

/// Every long row the history keeps as its text cut into pieces, where a store kept them all so
/// (before `core_0030_history_chunks`): how many were. Each is put in place only once its pieces
/// read back as the text it was; one that does not stays its text, which reads as before.
pub(crate) fn pack_all(conn: &Connection) -> Result<usize, String> {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT r.rowid, c.project_id FROM commit_rows r JOIN commits c ON c.id = r.commit_id
                  WHERE typeof(r.before) = 'text' AND octet_length(r.before) >= ?",
            )
            .map_err(db)?;
        stmt.query_map([INLINE as i64], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db)?
            .collect::<Result<_, _>>()
            .map_err(db)?
    };
    let mut packed = 0;
    // The projects with a row left as its text though pieces were made for it.
    let mut left: HashSet<String> = HashSet::new();
    for (row, project) in rows {
        let text: Vec<u8> = conn
            .query_row(
                "SELECT before FROM commit_rows WHERE rowid = ?",
                [row],
                |r| Ok(r.get_ref(0)?.as_bytes()?.to_vec()),
            )
            .map_err(db)?;
        // A text that is not one (written by no core) is left as it is.
        let Ok(text) = String::from_utf8(text) else {
            continue;
        };
        let Sql::Blob(list) = pack(conn, &project, &text)? else {
            continue;
        };
        if Before::Pieces(list.clone())
            .text(conn)
            .ok()
            .flatten()
            .as_deref()
            != Some(text.as_str())
        {
            left.insert(project);
            continue;
        }
        conn.execute(
            "UPDATE commit_rows SET before = ? WHERE rowid = ?",
            params![list, row],
        )
        .map_err(db)?;
        packed += 1;
    }
    // The pieces made for a row that stayed its text, where no other row points at them.
    for project in left {
        let_go(conn, &project, &held(conn, &project)?)?;
    }
    Ok(packed)
}

/// The numbers of every piece the project holds.
fn held(conn: &Connection, project: &str) -> Result<HashSet<i64>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM history_chunks WHERE project_id = ?")
        .map_err(db)?;
    stmt.query_map([project], |r| r.get(0))
        .map_err(db)?
        .collect::<Result<_, _>>()
        .map_err(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A store with the pieces' table and one project to keep them for.
    fn store() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY);
             INSERT INTO projects (id) VALUES ('p');",
        )
        .unwrap();
        conn.execute_batch(crate::schema::HISTORY_CHUNKS_TABLE)
            .unwrap();
        conn
    }

    /// About 200 KB whose lines all differ.
    fn lines() -> Vec<String> {
        (1..=4000)
            .map(|n| {
                format!(
                    "{n} 行目の文。札 {} の説明を足して長くする",
                    n * 7919 % 1000
                )
            })
            .collect()
    }

    fn pieces_held(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM history_chunks", [], |r| r.get(0))
            .unwrap()
    }

    fn read(conn: &Connection, kept: Sql) -> Result<Option<String>, String> {
        let before = match kept {
            Sql::Null => Before::None,
            Sql::Text(text) => Before::Text(text),
            Sql::Blob(list) => Before::Pieces(list),
            other => panic!("{other:?}"),
        };
        before.text(conn)
    }

    #[test]
    fn a_change_in_the_middle_leaves_the_pieces_around_it_as_they_were() {
        let mut lines = lines();
        let text = lines.join("\n");
        assert!(text.len() > 200_000);
        let was: HashSet<&[u8]> = cut(text.as_bytes()).into_iter().collect();
        assert!(was.len() > 20, "{} 個にしか切れていない", was.len());

        lines[2000] = "2001 行目を直した".to_string();
        let middle = lines.join("\n");
        let added = format!("頭に足した 1 行\n{middle}");
        for now in [&middle, &added] {
            let pieces = cut(now.as_bytes());
            let new = pieces.iter().filter(|piece| !was.contains(*piece)).count();
            assert!(new <= 3, "{new} 個が新しい");
            assert_eq!(pieces.concat(), now.as_bytes());
            let (last, rest) = pieces.split_last().unwrap();
            assert!(last.len() <= 16 * 1024);
            for piece in rest {
                assert!(
                    (1024..=16 * 1024).contains(&piece.len()),
                    "{} バイトの塊",
                    piece.len()
                );
            }
        }
    }

    #[test]
    fn what_is_packed_reads_back_as_it_was() {
        let conn = store();
        let wide = "全角の文字と絵文字 📎 を並べた文。".repeat(7000);
        assert!(wide.len() > 300_000);
        let same = "あ".repeat(34_000);
        for text in [
            String::new(),
            "a".repeat(INLINE - 1),
            "a".repeat(INLINE),
            wide,
            same.clone(),
        ] {
            let kept = pack(&conn, "p", &text).unwrap();
            match &kept {
                Sql::Text(as_text) => {
                    assert!(text.len() < INLINE, "{} バイトが文のまま", text.len());
                    assert_eq!(as_text, &text);
                }
                Sql::Blob(_) => assert!(text.len() >= INLINE, "{} バイトが並び", text.len()),
                other => panic!("{other:?}"),
            }
            assert_eq!(read(&conn, kept).unwrap().as_deref(), Some(text.as_str()));
        }
        // A text with no edge to find is cut by the longest a piece may be.
        let longest = cut(same.as_bytes())
            .iter()
            .map(|piece| piece.len())
            .max()
            .unwrap();
        assert_eq!(longest, 16 * 1024);
        // Kept again, nothing is added.
        let pieces = pieces_held(&conn);
        pack(&conn, "p", &same).unwrap();
        assert_eq!(pieces_held(&conn), pieces);
    }

    #[test]
    fn a_list_that_says_more_than_can_be_held_is_refused() {
        let conn = store();
        // One piece, said to be as long as a number can say; then two whose lengths add past it.
        let mut one = vec![FORM];
        number(&mut one, 1);
        number(&mut one, u64::MAX >> 1);
        let mut two = one.clone();
        number(&mut two, 2);
        number(&mut two, u64::MAX >> 1);
        for list in [one, two] {
            assert!(Before::Pieces(list).text(&conn).is_err());
        }
    }

    #[test]
    fn a_list_naming_one_piece_over_and_over_is_refused_before_it_is_built() {
        let conn = store();
        let kept = pack(&conn, "p", &"a".repeat(LONGEST)).unwrap();
        let Sql::Blob(list) = kept else {
            panic!("{kept:?}")
        };
        let (id, length) = listed(&list).unwrap()[0];
        assert_eq!(length, LONGEST);
        // Past a gigabyte in all, of one piece: more than any row is.
        let mut over = vec![FORM];
        for _ in 0..70_000 {
            number(&mut over, id as u64);
            number(&mut over, length as u64);
        }
        assert!(Before::Pieces(over).text(&conn).is_err());
    }

    #[test]
    fn a_list_that_says_a_piece_longer_than_any_is_cut_is_no_list() {
        let conn = store();
        conn.execute_batch(
            "CREATE TABLE commits (id TEXT PRIMARY KEY, project_id TEXT, branch_id TEXT, seq INTEGER);
             CREATE TABLE commit_rows (commit_id TEXT, tbl TEXT, row_id TEXT, before);
             INSERT INTO commits VALUES ('k1', 'p', '', 1);",
        )
        .unwrap();
        let mut held = 0;
        for piece in 1..=3u64 {
            let mut list = vec![FORM];
            number(&mut list, piece);
            number(&mut list, u64::MAX >> 2);
            assert!(listed(&list).is_none());
            held += list.len() as i64;
            conn.execute(
                "INSERT INTO commit_rows VALUES ('k1', 'card_docs', ?, ?)",
                params![format!("c{piece}"), list],
            )
            .unwrap();
        }
        let mut list = vec![FORM];
        number(&mut list, 4);
        number(&mut list, LONGEST as u64 + 1);
        assert!(listed(&list).is_none());
        // Counted by what the rows hold themselves: a length no piece has is not added to another.
        assert_eq!(sizes(&conn, "p", &[""]).unwrap(), [held]);
        assert_eq!(freed(&conn, "p", "", 2, &[""]).unwrap(), 0);
    }

    #[test]
    fn a_list_that_cannot_be_read_is_passed_by_the_sizes_and_lets_no_piece_go() {
        let conn = store();
        conn.execute_batch(
            "CREATE TABLE commits (id TEXT PRIMARY KEY, project_id TEXT, branch_id TEXT, seq INTEGER);
             CREATE TABLE commit_rows (commit_id TEXT, tbl TEXT, row_id TEXT, before);
             INSERT INTO commits VALUES ('k1', 'p', '', 1), ('k2', 'p', '', 2);",
        )
        .unwrap();
        let text = lines().join("\n");
        let kept = pack(&conn, "p", &text).unwrap();
        conn.execute(
            "INSERT INTO commit_rows VALUES ('k1', 'card_docs', 'c1', ?)",
            [&kept],
        )
        .unwrap();
        // Not a list of the form: what it points at cannot be known.
        conn.execute(
            "INSERT INTO commit_rows VALUES ('k2', 'card_docs', 'c2', x'09ffff')",
            [],
        )
        .unwrap();
        let Sql::Blob(list) = &kept else {
            panic!("{kept:?}")
        };
        // What the rows hold themselves — the list, and the three bytes that are none — and the
        // pieces the list points at.
        assert_eq!(
            sizes(&conn, "p", &[""]).unwrap(),
            [(list.len() + 3 + text.len()) as i64]
        );
        assert_eq!(freed(&conn, "p", "", 2, &[""]).unwrap(), text.len() as i64);

        let pieces = pieces_held(&conn);
        let going = pointed_by(&conn, &["k1".to_string()]).unwrap();
        assert_eq!(going.len() as i64, pieces);
        conn.execute("DELETE FROM commit_rows WHERE commit_id = 'k1'", [])
            .unwrap();
        assert_eq!(let_go(&conn, "p", &going).unwrap(), 0);
        assert_eq!(pieces_held(&conn), pieces);
        // The row that cannot be read gone, what no row points at goes.
        conn.execute("DELETE FROM commit_rows WHERE commit_id = 'k2'", [])
            .unwrap();
        assert_eq!(let_go(&conn, "p", &going).unwrap() as i64, pieces);
        assert_eq!(pieces_held(&conn), 0);
    }

    #[test]
    fn a_row_whose_piece_is_gone_is_refused() {
        let conn = store();
        let text = lines().join("\n");
        let kept = pack(&conn, "p", &text).unwrap();
        assert_eq!(
            read(&conn, kept.clone()).unwrap().as_deref(),
            Some(text.as_str())
        );

        conn.execute(
            "UPDATE history_chunks SET data = x'41' WHERE id = (SELECT max(id) FROM history_chunks)",
            [],
        )
        .unwrap();
        assert!(read(&conn, kept.clone()).is_err(), "短い塊のまま読めた");

        conn.execute(
            "DELETE FROM history_chunks WHERE id = (SELECT max(id) FROM history_chunks)",
            [],
        )
        .unwrap();
        assert!(read(&conn, kept).is_err(), "塊が無いのに読めた");
    }
}
