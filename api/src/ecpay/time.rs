//! 綠界的日期字串一律台北時間（UTC+8）；DB 一律 UTC（規格 §8）
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};

/// `MerchantTradeDate`、`PaymentDate`、超商 `ExpireDate` 的格式
pub const DATETIME_FMT: &str = "%Y/%m/%d %H:%M:%S";
/// ATM `ExpireDate` 的格式（只有日期）
pub const DATE_FMT: &str = "%Y/%m/%d";

pub fn taipei() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("UTC+8 是合法的時區位移")
}

/// UTC → `yyyy/MM/dd HH:mm:ss`（台北）
pub fn format_datetime(at: DateTime<Utc>) -> String {
    at.with_timezone(&taipei()).format(DATETIME_FMT).to_string()
}

/// 用指定格式把台北時間字串轉成 UTC；格式不符回 None。發票的 InvoiceDate 是 `%Y-%m-%d %H:%M:%S`
pub fn parse_taipei(s: &str, fmt: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s.trim(), fmt).ok()?;
    naive
        .and_local_timezone(taipei())
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

/// `yyyy/MM/dd HH:mm:ss` → UTC
pub fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    parse_taipei(s, DATETIME_FMT)
}

/// `yyyy/MM/dd` → 該日台北時間 23:59:59（ATM 的虛擬帳號到最後一天 23:59:59 有效，規格 §5）
pub fn parse_date_end_of_day(s: &str) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(s.trim(), DATE_FMT).ok()?;
    let naive = date.and_time(NaiveTime::from_hms_opt(23, 59, 59)?);
    naive
        .and_local_timezone(taipei())
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

/// 繳費期限：先試完整時間（超商），再試只有日期（ATM）
pub fn parse_expire(s: &str) -> Option<DateTime<Utc>> {
    parse_datetime(s).or_else(|| parse_date_end_of_day(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn formats_in_taipei() {
        let utc = Utc.with_ymd_and_hms(2023, 3, 12, 7, 30, 23).unwrap();
        assert_eq!(format_datetime(utc), "2023/03/12 15:30:23");
    }

    #[test]
    fn parses_back_to_utc() {
        assert_eq!(
            parse_datetime("2023/03/12 15:30:23"),
            Some(Utc.with_ymd_and_hms(2023, 3, 12, 7, 30, 23).unwrap())
        );
        assert_eq!(
            parse_date_end_of_day("2023/03/12"),
            Some(Utc.with_ymd_and_hms(2023, 3, 12, 15, 59, 59).unwrap())
        );
        assert_eq!(
            parse_expire("2023/03/12"),
            parse_date_end_of_day("2023/03/12")
        );
        assert_eq!(
            parse_expire(" 2023/03/12 08:00:00 "),
            Some(Utc.with_ymd_and_hms(2023, 3, 12, 0, 0, 0).unwrap())
        );
        assert_eq!(
            parse_taipei("2026-09-06 15:30:23", "%Y-%m-%d %H:%M:%S"),
            Some(Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap())
        );
        assert_eq!(parse_expire("not a date"), None);
        assert_eq!(parse_datetime("2023/03/12"), None);
    }
}
