use anyhow::{Context, Result};
use chrono::{Datelike, Local, NaiveDate, Weekday};
use horae_core::lunar::{self, CalendarDayInfo};
use serde::Serialize;

#[derive(Serialize)]
struct CalendarJson {
    date: String,
    weekday: String,
    lunar: LunarJson,
    solar_term: Option<String>,
    holidays: Vec<HolidayJson>,
    next_solar_term: NextSolarTermJson,
    next_major_holiday: Option<NextMajorHolidayJson>,
    warnings: Vec<WarningJson>,
    summary: String,
}

#[derive(Serialize)]
struct LunarJson {
    year: i32,
    month: u32,
    day: u32,
    is_leap: bool,
    ganzhi_year: String,
    zodiac: String,
    month_name: String,
    day_name: String,
    formatted: String,
}

#[derive(Serialize)]
struct HolidayJson {
    name: String,
    is_major: bool,
    hint: String,
}

#[derive(Serialize)]
struct NextSolarTermJson {
    name: String,
    date: String,
    days_left: i64,
}

#[derive(Serialize)]
struct NextMajorHolidayJson {
    name: String,
    date: String,
    days_left: i64,
    hint: String,
}

#[derive(Serialize)]
struct WarningJson {
    holiday_name: String,
    days_left: i64,
    target_date: String,
    hint: String,
    message: String,
}

fn weekday_cn(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "星期一",
        Weekday::Tue => "星期二",
        Weekday::Wed => "星期三",
        Weekday::Thu => "星期四",
        Weekday::Fri => "星期五",
        Weekday::Sat => "星期六",
        Weekday::Sun => "星期日",
    }
}

pub fn run(date_str: Option<&str>, short: bool, json: bool) -> Result<()> {
    let target_date = if let Some(s) = date_str {
        if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            d
        } else if let Ok(ms) = horae_core::time::parse_time(s) {
            chrono::DateTime::from_timestamp_millis(ms)
                .map(|dt| dt.with_timezone(&Local).naive_local().date())
                .context("invalid timestamp")?
        } else {
            anyhow::bail!(
                "无法解析日期: '{}'，请使用 YYYY-MM-DD 格式 (例如 2026-09-04)",
                s
            );
        }
    } else {
        Local::now().naive_local().date()
    };

    let cal = lunar::day_calendar_info(target_date)
        .with_context(|| format!("日期 {} 超出支持的农历年份范围 (1900-2100)", target_date))?;

    if short {
        println!("{}", cal.status_line());
        return Ok(());
    }

    if json {
        let (nst, nst_date, nst_days) = cal.next_solar_term;
        let nmh = cal
            .next_major_holiday
            .as_ref()
            .map(|(h, d, days)| NextMajorHolidayJson {
                name: h.name.clone(),
                date: d.format("%Y-%m-%d").to_string(),
                days_left: *days,
                hint: h.hint.clone(),
            });
        let warnings = cal
            .warnings
            .iter()
            .map(|w| WarningJson {
                holiday_name: w.holiday_name.clone(),
                days_left: w.days_left,
                target_date: w.target_date.format("%Y-%m-%d").to_string(),
                hint: w.hint.clone(),
                message: w.message(),
            })
            .collect();
        let holidays = cal
            .holidays
            .iter()
            .map(|h| HolidayJson {
                name: h.name.clone(),
                is_major: h.is_major,
                hint: h.hint.clone(),
            })
            .collect();

        let data = CalendarJson {
            date: target_date.format("%Y-%m-%d").to_string(),
            weekday: weekday_cn(target_date.weekday()).to_string(),
            lunar: LunarJson {
                year: cal.lunar.year,
                month: cal.lunar.month,
                day: cal.lunar.day,
                is_leap: cal.lunar.is_leap,
                ganzhi_year: cal.lunar.ganzhi_year.to_string(),
                zodiac: cal.lunar.zodiac.to_string(),
                month_name: cal.lunar.month_name.to_string(),
                day_name: cal.lunar.day_name.to_string(),
                formatted: cal.lunar.to_string(),
            },
            solar_term: cal.solar_term.map(|st| st.name().to_string()),
            holidays,
            next_solar_term: NextSolarTermJson {
                name: nst.name().to_string(),
                date: nst_date.format("%Y-%m-%d").to_string(),
                days_left: nst_days,
            },
            next_major_holiday: nmh,
            warnings,
            summary: cal.status_line(),
        };
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    print_card(&cal, target_date);
    Ok(())
}

fn print_card(cal: &CalendarDayInfo, date: NaiveDate) {
    let weekday = weekday_cn(date.weekday());
    println!("📅 公历日期: {} ({})", date.format("%Y-%m-%d"), weekday);
    println!("🌙 农历干支: {}", cal.lunar);
    println!("🏷  农历简述: {}", cal.lunar.short_format());

    if let Some(term) = cal.solar_term {
        println!("🌱 今日节气: {} · {}", term.name(), term.desc());
    } else {
        let (next_term, next_date, days) = cal.next_solar_term;
        println!(
            "🌱 下一节气: {} (距今 {} 天 · {}) · {}",
            next_term.name(),
            days,
            next_date.format("%Y-%m-%d"),
            next_term.desc()
        );
    }

    if !cal.holidays.is_empty() {
        println!("🎉 今日节日:");
        for h in &cal.holidays {
            if h.is_major {
                println!("   • [重大节日] {} — {}", h.name, h.hint);
            } else {
                println!("   • {} — {}", h.name, h.hint);
            }
        }
    }

    if !cal.warnings.is_empty() {
        println!("🏮 节日提醒 (重大节日规划):");
        for w in &cal.warnings {
            println!("   🔔  {}", w.message());
        }
    }

    if let Some((ref mh, m_date, days)) = cal.next_major_holiday {
        if days > 0 {
            println!(
                "📌 即将到来: 距{}还有 {} 天 ({}) · {}",
                mh.name,
                days,
                m_date.format("%Y-%m-%d"),
                mh.hint
            );
        }
    }

    println!("✨ 摘要一览: {}", cal.status_line());
}
