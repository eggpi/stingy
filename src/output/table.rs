use anyhow::{anyhow, Result};
use pager::Pager;
use std::cmp::{max, min};
use std::io::Write;
use textwrap;

fn termwidth() -> usize {
    #[cfg(test)]
    return 16;
    #[allow(unreachable_code)]
    textwrap::termwidth()
}

fn setup_pager() {
    #[cfg(test)]
    return;
    #[allow(unreachable_code)]
    Pager::with_pager("less --quit-if-one-screen").setup()
}

fn textwrap_and_clone(text: &str, width: usize) -> Vec<String> {
    textwrap::wrap(text, width)
        .iter()
        .map(|l| l.to_string())
        .collect()
}

fn render_header<W>(
    writer: &mut W,
    columns: &Vec<Vec<String>>,
    table_width: usize,
    column_widths: &[usize],
) -> Result<()>
where
    W: Write,
{
    writeln!(writer, "{:=^width$}", "", width = table_width)?;
    render_row(writer, &columns, column_widths)?;
    writeln!(writer, "{:=^width$}", "", width = table_width)?;
    Ok(())
}

fn render_row<W>(
    writer: &mut W,
    row_contents: &Vec<Vec<String>>,
    column_widths: &[usize],
) -> Result<()>
where
    W: Write,
{
    let row_height_in_lines = row_contents
        .iter()
        .map(|lines| lines.len())
        .max()
        .ok_or(anyhow!("Failed to compute row height!"))?;

    for line_number in 0..row_height_in_lines {
        for (column_idx, lines_in_cell) in row_contents.iter().enumerate() {
            write!(writer, "{}", if line_number == 0 { "|" } else { ":" })?;
            if line_number < lines_in_cell.len() {
                let line = &lines_in_cell[line_number];
                let pad = column_widths[column_idx] - textwrap::core::display_width(line);
                let pad_left = pad / 2;
                let pad_right = pad - pad_left;
                write!(writer, "{:^pad_left$}{line}{:^pad_right$}", "", "")?;
            } else {
                write!(
                    writer,
                    "{:^width$.width$}",
                    "",
                    width = column_widths[column_idx]
                )?;
            }
        }
        writeln!(writer, "{}", if line_number == 0 { "|" } else { ":" })?;
    }

    Ok(())
}

pub fn render_table<W, C, R, S>(writer: &mut W, columns: &[C], rows: &[R]) -> Result<()>
where
    W: Write,
    C: AsRef<str>,
    R: AsRef<[S]>,
    S: AsRef<str>,
{
    let termwidth = termwidth();
    let n_columns = columns.len();
    // Maximum overall table width, with borders.
    let max_table_width = termwidth;
    // Maximum column width, without its borders, with padding.
    let max_column_width = (max_table_width - 1) / n_columns - 1;

    // Wrap the text in the cells at the maximum cell width.
    let rows: Vec<Vec<_>> = rows
        .iter()
        .map(|row| {
            row.as_ref()
                .iter()
                .map(|cell| textwrap_and_clone(cell.as_ref(), max_column_width))
                .collect()
        })
        .collect();
    let columns: Vec<Vec<String>> = columns
        .iter()
        .map(|column| textwrap_and_clone(column.as_ref(), max_column_width))
        .collect();

    // Compute the minimum width of each column based on the wrapped text
    // and wrapped column name.
    let mut column_widths: Vec<usize> = columns
        .iter()
        .map(|header_lines| {
            header_lines
                .iter()
                .map(|line| textwrap::core::display_width(line))
                .max()
                .unwrap_or(0)
        })
        .collect();
    for row in &rows {
        for (column_idx, cell) in row.iter().enumerate() {
            for line in cell {
                column_widths[column_idx] = max(
                    textwrap::core::display_width(line),
                    column_widths[column_idx],
                );
            }
        }
    }

    // Add a little padding, not exceeding max_column_width.
    for column_idx in 0..column_widths.len() {
        let padding = min(max_column_width - column_widths[column_idx], 4);
        column_widths[column_idx] += padding;
    }
    assert!(column_widths.iter().all(|cw| *cw <= max_column_width));

    let table_width = 1                            // the table's left border
            + n_columns                            // each column's right border
            + column_widths.iter().sum::<usize>(); // the columns themselves
    assert!(table_width <= max_table_width);
    assert!(table_width <= termwidth);

    setup_pager();
    render_header(writer, &columns, table_width, &column_widths)?;

    let mut row = 0;
    for row_contents in &rows {
        render_row(writer, row_contents, &column_widths)?;
        row += 1;
        // Output the heading again every 20 rows, and a row counter every 10,
        // except after the last row.
        if row == rows.len() {
            continue;
        }
        if row % 20 == 0 {
            render_header(writer, &columns, table_width, &column_widths)?;
        } else if row % 10 == 0 {
            let formatted_row_number = format!("{:02}", row);
            let mut col = 0;
            while col < table_width - formatted_row_number.len() {
                write!(writer, "{}", if col % 2 == 0 { "." } else { " " })?;
                col += 1;
            }
            writeln!(writer, "{formatted_row_number}")?;
        }
    }
    // Output the final row count at the bottom of the table.
    let formatted_row_number = format!("{:02}", row);
    writeln!(
        writer,
        "{:=^width$}{formatted_row_number}",
        "",
        width = table_width - formatted_row_number.len()
    )?;

    Ok(())
}

#[cfg(test)]
mod table_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn simple_cell() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["Column"];
        let rows = [["Cell"]];
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        assert_eq!(lines[0], "============");
        assert_eq!(lines[1], "|  Column  |");
        assert_eq!(lines[2], "============");
        assert_eq!(lines[3], "|   Cell   |");
        assert_eq!(lines[4], "==========01");
        assert_eq!(lines[5], "");
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn multiline_cell() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["Column"];
        let rows = [["This text doesn't fit in one column"]];
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        assert_eq!(lines[0], "================");
        assert_eq!(lines[1], "|    Column    |");
        assert_eq!(lines[2], "================");
        assert_eq!(lines[3], "|  This text   |");
        assert_eq!(lines[4], ":doesn't fit in:");
        assert_eq!(lines[5], ":  one column  :");
        assert_eq!(lines[6], "==============01");
        assert_eq!(lines[7], "");
        assert_eq!(lines.len(), 8);
    }

    #[test]
    fn row_count() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["Column"];
        let rows: Vec<_> = (0..12).map(|n| [format!("{n}")]).collect();
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        assert_eq!(lines[13], ". . . . . 10"); // The header takes 3 lines.
        assert_eq!(lines[16], "==========12");
        assert_eq!(lines[17], "");
        assert_eq!(lines.len(), 18);
    }

    #[test]
    fn no_row_count_on_last_line() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["Column"];
        let rows: Vec<_> = (0..10).map(|n| [format!("{n}")]).collect();
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        // This should be the footer already, not a row count.
        assert_eq!(lines[13], "==========10"); // The header takes 3 rows.
        assert_eq!(lines[14], "");
        assert_eq!(lines.len(), 15);
    }

    #[test]
    fn repeat_header() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["Column"];
        let rows: Vec<_> = (0..25).map(|n| [format!("{n}")]).collect();
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        assert_eq!(lines[23], "|    19    |");
        assert_eq!(lines[24], lines[0]); // Repeat the header every 20 rows.
        assert_eq!(lines[25], lines[1]);
        assert_eq!(lines[26], lines[2]);
        assert_eq!(lines[27], "|    20    |");
        assert_eq!(lines.len(), 34);
    }

    #[test]
    fn shrink_small_column_single() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["❗"];
        // We use emoji to prove that the size computation is Unicode-aware.
        let rows: Vec<_> = (0..2).map(|_| [format!("✅")]).collect();
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        // The table does not take up the whole termwidth.
        assert_eq!(lines[0], "========");
        assert_eq!(lines[1], "|  ❗  |");
        assert_eq!(lines[2], "========");
        assert_eq!(lines[3], "|  ✅  |");
        assert_eq!(lines[4], "|  ✅  |");
        assert_eq!(lines[5], "======02");
    }

    #[test]
    fn dont_shrink_small_column_large_header() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["This header is long"];
        let rows: Vec<_> = (0..2).map(|n| [format!("{n}")]).collect();
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        // The header causes the column to expand, even though the data
        // itself is short.
        assert_eq!(lines[0], "================");
        assert_eq!(lines[1], "|This header is|");
        assert_eq!(lines[2], ":     long     :");
        assert_eq!(lines[3], "================");
        assert_eq!(lines[4], "|      0       |");
        assert_eq!(lines[5], "|      1       |");
        assert_eq!(lines[6], "==============02");
    }

    #[test]
    fn shrink_small_column_multiple() {
        let mut cursor = Cursor::new(Vec::new());
        let columns = ["E", "NE"];
        // We use λ to prove that the size computation is Unicode-aware.
        let rows: Vec<_> = (0..2)
            .map(|_| ["".to_string(), format!("λλλλλλ")])
            .collect();
        render_table(cursor.get_mut(), &columns, &rows).unwrap();
        let output = String::from_utf8(cursor.get_ref().to_vec()).unwrap();
        let lines: Vec<_> = output.split("\n").collect();
        // Column E (Empty) is 4 spaces wide, NE (Not Empty) is 6.
        assert_eq!(lines[0], "==============");
        assert_eq!(lines[1], "|  E  |  NE  |");
        assert_eq!(lines[2], "==============");
        assert_eq!(lines[3], "|     |λλλλλλ|");
        assert_eq!(lines[4], "|     |λλλλλλ|");
        assert_eq!(lines[5], "============02");
    }
}
