use chrono::NaiveDate;

pub trait ToOutputFormat {
    fn to_output_format(&self) -> String;
}

impl ToOutputFormat for String {
    fn to_output_format(&self) -> String {
        self.to_string()
    }
}

impl ToOutputFormat for NaiveDate {
    fn to_output_format(&self) -> String {
        format!("{}", self.format("%Y/%m/%d"))
    }
}

impl ToOutputFormat for i64 {
    fn to_output_format(&self) -> String {
        format!("{:3}", self)
    }
}

impl ToOutputFormat for f64 {
    fn to_output_format(&self) -> String {
        format!("{:.02}", self)
    }
}

impl ToOutputFormat for str {
    fn to_output_format(&self) -> String {
        self.to_string()
    }
}

impl ToOutputFormat for Vec<String> {
    fn to_output_format(&self) -> String {
        self.join("\n")
    }
}
