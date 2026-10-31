//! Parquet archive export/import.
//!
//! Large collections should not stay in the SQLite store with a per-ply
//! position index. `export-parquet` writes every game to a portable columnar
//! file; `import-parquet` loads it back without building the index unless it is
//! explicitly requested, so archives stay cheap to move and to browse.

use anyhow::{Context, Result};
use arrow::array::{Array, ArrayRef, Int32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use rbc_db::{ArchiveGame, Db, GameQuery};
use std::path::Path;
use std::sync::Arc;

const PAGE: u32 = 500;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("event", DataType::Utf8, true),
        Field::new("site", DataType::Utf8, true),
        Field::new("date", DataType::Utf8, true),
        Field::new("round", DataType::Utf8, true),
        Field::new("white", DataType::Utf8, false),
        Field::new("white_elo", DataType::Int32, true),
        Field::new("black", DataType::Utf8, false),
        Field::new("black_elo", DataType::Int32, true),
        Field::new("result", DataType::Utf8, false),
        Field::new("fen", DataType::Utf8, false),
        Field::new("moves", DataType::Utf8, false),
        Field::new("time_control", DataType::Utf8, true),
        Field::new("variant", DataType::Utf8, true),
    ]))
}

fn opt_strings(values: Vec<Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from_iter(values.iter().map(|value| value.as_deref())))
}

pub fn export(db_path: &str, out: &Path) -> Result<usize> {
    let db = Db::open(db_path).context("open database")?;
    let schema = schema();
    let file = std::fs::File::create(out)
        .with_context(|| format!("create {}", out.display()))?;
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None)?;
    let mut offset = 0u32;
    let mut total = 0usize;
    loop {
        let page = db.query(&GameQuery {
            limit: Some(PAGE),
            offset: Some(offset),
            sort: Some("oldest".into()),
            ..GameQuery::default()
        })?;
        if page.games.is_empty() {
            break;
        }
        let games = &page.games;
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(
                    games.iter().map(|g| g.id).collect::<Vec<_>>(),
                )),
                opt_strings(games.iter().map(|g| g.event.clone()).collect()),
                opt_strings(games.iter().map(|g| g.site.clone()).collect()),
                opt_strings(games.iter().map(|g| g.date.clone()).collect()),
                opt_strings(games.iter().map(|g| g.round.clone()).collect()),
                Arc::new(StringArray::from_iter_values(
                    games.iter().map(|g| g.white.as_str()),
                )),
                Arc::new(Int32Array::from(
                    games.iter().map(|g| g.white_elo).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from_iter_values(
                    games.iter().map(|g| g.black.as_str()),
                )),
                Arc::new(Int32Array::from(
                    games.iter().map(|g| g.black_elo).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from_iter_values(
                    games.iter().map(|g| g.result.as_str()),
                )),
                Arc::new(StringArray::from_iter_values(
                    games.iter().map(|g| g.fen.as_str()),
                )),
                Arc::new(StringArray::from_iter_values(
                    games.iter().map(|g| g.moves.as_str()),
                )),
                opt_strings(games.iter().map(|g| g.time_control.clone()).collect()),
                opt_strings(games.iter().map(|g| g.variant.clone()).collect()),
            ],
        )?;
        writer.write(&batch)?;
        total += games.len();
        offset += games.len() as u32;
    }
    writer.close()?;
    Ok(total)
}

fn string_at(batch: &RecordBatch, column: usize, row: usize) -> Option<String> {
    let array = batch.column(column);
    let array = array.as_any().downcast_ref::<StringArray>()?;
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row).to_owned())
    }
}

fn required_string(batch: &RecordBatch, column: usize, row: usize) -> String {
    string_at(batch, column, row).unwrap_or_default()
}

fn int_at(batch: &RecordBatch, column: usize, row: usize) -> Option<i32> {
    let array = batch.column(column);
    let array = array.as_any().downcast_ref::<Int32Array>()?;
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row))
    }
}

pub fn import(db_path: &str, file: &Path, index_positions: bool) -> Result<usize> {
    let db = Db::open(db_path).context("open database")?;
    let input =
        std::fs::File::open(file).with_context(|| format!("open {}", file.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(input)?;
    let reader = builder.build()?;
    let mut imported = 0usize;
    db.begin_bulk()?;
    for batch in reader {
        let batch = batch?;
        for row in 0..batch.num_rows() {
            let game = ArchiveGame {
                event: string_at(&batch, 1, row),
                site: string_at(&batch, 2, row),
                date: string_at(&batch, 3, row),
                round: string_at(&batch, 4, row),
                white: required_string(&batch, 5, row),
                white_elo: int_at(&batch, 6, row),
                black: required_string(&batch, 7, row),
                black_elo: int_at(&batch, 8, row),
                result: required_string(&batch, 9, row),
                fen: required_string(&batch, 10, row),
                moves: required_string(&batch, 11, row),
                time_control: string_at(&batch, 12, row),
                variant: string_at(&batch, 13, row),
            };
            db.insert_archive_game(&game, index_positions)?;
            imported += 1;
        }
    }
    db.commit_bulk()?;
    db.build_catalogs()?;
    if index_positions {
        db.build_opening_index(12)?;
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parquet_round_trip() {
        let dir = std::env::temp_dir().join(format!("rbc-archive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.db");
        let target = dir.join("target.db");
        let file = dir.join("games.parquet");
        let _ = std::fs::remove_file(&source);
        let _ = std::fs::remove_file(&target);

        let db = Db::open(&source.display().to_string()).unwrap();
        db.import_pgn(
            "[Event \"A\"]\n[White \"W1\"]\n[Black \"B1\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n\n\
             [Event \"B\"]\n[White \"W2\"]\n[Black \"B2\"]\n[Result \"0-1\"]\n\n1. d4 d5 0-1\n",
            None,
        )
        .unwrap();

        let exported = export(&source.display().to_string(), &file).unwrap();
        assert_eq!(exported, 2);
        let imported = import(&target.display().to_string(), &file, false).unwrap();
        assert_eq!(imported, 2);

        let target_db = Db::open(&target.display().to_string()).unwrap();
        let all = target_db.query(&GameQuery::default()).unwrap();
        assert_eq!(all.total, 2);
        assert!(all.games.iter().any(|g| g.white == "W1" && g.moves.starts_with("e4")));
        assert_eq!(target_db.schema_version().unwrap(), rbc_db::SCHEMA_VERSION);
    }
}
