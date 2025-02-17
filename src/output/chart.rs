use anyhow::{bail, Result};
use charming;
use std::collections;
use std::io::Write;
use std::process;

use crate::database;
use crate::output::format::ToOutputFormat;
use crate::output::{Output, OutputForTesting};

const FONT_SIZE: f64 = 25.0;
const TITLE_FONT_SIZE: f64 = FONT_SIZE * 1.5;

fn chart_to_sixel<W>(writer: &mut W, chart: &charming::Chart) -> Result<()>
where
    W: Write,
{
    let mut renderer = charming::ImageRenderer::new(2048, 1024);
    let bytes = renderer.render_format(charming::ImageFormat::Png, &chart)?;

    let mut child = process::Command::new("sh")
        .arg("-c")
        .arg("magick png:- sixel:-")
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || stdin.write_all(&bytes));
        let output = child.wait_with_output()?;
        Ok(writer.write_all(&output.stdout)?)
    } else {
        child.kill()?;
        bail!("failed to take handle to child process stdin")
    }
}

fn default_chart() -> charming::Chart {
    charming::Chart::new().background_color("#efefef")
}

fn default_legend() -> charming::component::Legend {
    charming::component::Legend::new()
        .padding((20, 0))
        .text_style(charming::element::TextStyle::new().font_size(FONT_SIZE))
}

fn default_title() -> charming::component::Title {
    charming::component::Title::new()
        .padding((20, 0))
        .text_style(charming::element::TextStyle::new().font_size(TITLE_FONT_SIZE))
        .text_align(charming::element::TextAlign::Center)
}

fn default_label() -> charming::element::Label {
    charming::element::Label::new().font_size(FONT_SIZE)
}

fn default_label_line() -> charming::element::LabelLine {
    charming::element::LabelLine::new().line_style(charming::element::LineStyle::new().width(3))
}

fn category_axis(data: &Vec<String>) -> charming::component::Axis {
    charming::component::Axis::new()
        .type_(charming::element::AxisType::Category)
        .data(data.to_vec())
        .axis_label(charming::element::AxisLabel::new().font_size(FONT_SIZE))
}

fn value_axis(name: Option<&str>) -> charming::component::Axis {
    let axis = charming::component::Axis::new()
        .type_(charming::element::AxisType::Value)
        .axis_label(charming::element::AxisLabel::new().font_size(FONT_SIZE))
        .axis_line(charming::element::axis_line::AxisLine::new().show(true));
    if let Some(name) = name {
        axis.name(name)
            .name_text_style(charming::element::TextStyle::new().font_size(FONT_SIZE))
            .name_location(charming::element::name_location::NameLocation::End)
            .name_gap(50)
    } else {
        axis
    }
}

fn by_month_rows_to_chart(
    rows: &[database::ByMonthRow],
    show_balance: bool,
) -> Result<charming::Chart> {
    let mut accounts: Vec<_> = rows.iter().map(|r| r.account_name.clone()).collect();
    accounts.sort();
    accounts.dedup();

    let mut account_to_balance_series = collections::HashMap::new();
    let mut account_to_debits_series = collections::HashMap::new();
    let mut account_to_credits_series = collections::HashMap::new();

    let mut rows = rows.to_vec();
    rows.sort_by_key(|row| row.month);

    let mut months = vec![];
    let mut i = 0;
    while i < rows.len() {
        let mut row = &rows[i];
        let month = row.month;
        months.push(month);

        // Initialize all series for this month.
        // Use 0.0 for the debits and credits series, and the last month's
        // balance (if available) for the balance.
        for account_name in &accounts {
            for series in [
                &mut account_to_debits_series,
                &mut account_to_credits_series,
            ] {
                series
                    .entry(account_name)
                    .and_modify(|v: &mut Vec<_>| v.push(0.0))
                    .or_insert(vec![0.0]);
            }
            account_to_balance_series
                .entry(account_name)
                .and_modify(|v: &mut Vec<_>| v.push(*v.last().unwrap_or(&f64::NAN)))
                .or_insert(vec![f64::NAN]);
        }

        while i < rows.len() {
            row = &rows[i];
            if row.month != month {
                break;
            }

            // Replace the defaults with real information, if the account actually
            // has any for this month.
            *account_to_balance_series
                .get_mut(&row.account_name)
                .unwrap()
                .last_mut()
                .unwrap() = row.balance;
            *account_to_debits_series
                .get_mut(&row.account_name)
                .unwrap()
                .last_mut()
                .unwrap() = row.debit_amount;
            *account_to_credits_series
                .get_mut(&row.account_name)
                .unwrap()
                .last_mut()
                .unwrap() = row.credit_amount;
            i += 1;
        }
    }

    months.dedup();
    let months = months
        .iter()
        .map(|m| m.format("%b/%Y").to_string())
        .collect();

    let mut chart = default_chart()
        .legend(default_legend())
        .x_axis(category_axis(&months))
        .y_axis(value_axis(Some("Credits (+) / Debits (-)")));
    if show_balance {
        chart = chart
            .grid(charming::component::Grid::new().height("30%").bottom("7%"))
            .grid(charming::component::Grid::new().height("30%").top("17%"))
            .x_axis(category_axis(&months).grid_index(1))
            .y_axis(value_axis(Some("Balance (b)")).grid_index(1));
    } else {
        chart = chart.grid(charming::component::Grid::new().top("17%"));
    }
    for account_name in &accounts {
        // Add debits series.
        chart = chart
            .series(
                charming::series::Bar::new()
                    .name(format!("{account_name} (-)"))
                    .stack(format!("{account_name}"))
                    .data(
                        account_to_debits_series
                            .remove(account_name)
                            .unwrap()
                            .into_iter()
                            .map(|d| -d)
                            .collect(),
                    )
                    .label(default_label()),
            )
            // Add credits series.
            .series(
                charming::series::Bar::new()
                    .name(format!("{account_name} (+)"))
                    .stack(format!("{account_name}"))
                    .data(account_to_credits_series.remove(account_name).unwrap())
                    .label(default_label()),
            );
        if show_balance {
            // Add balance series.
            chart = chart.series(
                charming::series::Line::new()
                    .name(format!("{account_name} (b)"))
                    .line_style(charming::element::LineStyle::new().width(4))
                    .symbol_size(16)
                    .x_axis_index(1)
                    .y_axis_index(1)
                    .data(account_to_balance_series.remove(account_name).unwrap()),
            )
        }
    }
    Ok(chart)
}

fn by_tag_rows_to_chart(rows: &[database::ByTagRow]) -> Result<charming::Chart> {
    let make_pie_chart = |name, center_x, data| {
        charming::series::Pie::new()
            .name(name)
            .radius("55%")
            .center(vec![center_x, "50%"])
            .label(default_label())
            .label_line(default_label_line())
            .data(data)
    };
    let filter_row_by_pct = |pct: f64, amount: f64, tag: &str| {
        let normalized_tag = if tag == "" { "(untagged)" } else { tag };
        let normalized_tag = format!("{normalized_tag}\n({})", amount.to_output_format());
        if pct >= 2.0 {
            Some((amount, normalized_tag))
        } else {
            None
        }
    };
    let debits: Vec<_> = rows
        .iter()
        .filter_map(|r| filter_row_by_pct(r.tag_debit_pct, r.tag_debit, &r.tag))
        .collect();
    let credits: Vec<_> = rows
        .iter()
        .filter_map(|r| filter_row_by_pct(r.tag_credit_pct, r.tag_credit, &r.tag))
        .collect();
    Ok(default_chart()
        .title(default_title().text("Debits").top("10%").left("25%"))
        .title(default_title().text("Credits").top("10%").left("75%"))
        .series(make_pie_chart("Debits", "25%", debits))
        .series(make_pie_chart("Credits", "75%", credits)))
}

pub struct ChartOutput<W> {
    writer: W,
}

impl<W> Output<W> for ChartOutput<W>
where
    W: Write,
{
    fn new(writer: W, _: Option<usize>) -> ChartOutput<W> {
        ChartOutput { writer: writer }
    }

    fn render_by_month(
        &mut self,
        rows: &[database::ByMonthRow],
        show_balance: bool,
    ) -> Result<OutputForTesting> {
        let chart = by_month_rows_to_chart(rows, show_balance)?;
        chart_to_sixel(&mut self.writer, &chart)?;
        Ok(OutputForTesting::Chart(chart.to_string()))
    }

    fn render_by_tag(&mut self, rows: &[database::ByTagRow]) -> Result<OutputForTesting> {
        let chart = by_tag_rows_to_chart(rows)?;
        chart_to_sixel(&mut self.writer, &chart)?;
        Ok(OutputForTesting::Chart(chart.to_string()))
    }

    fn render_debits(&mut self, _: &[database::DebitsRow], _: bool) -> Result<OutputForTesting> {
        unimplemented!();
    }

    fn render_credits(&mut self, _: &[database::CreditsRow], _: bool) -> Result<OutputForTesting> {
        unimplemented!();
    }
}
