pub mod data;
pub mod terms_table;

use chrono::{Datelike, Duration, NaiveDate};
use data::*;
use std::fmt;

/// 二十四节气枚举（从 0: 小寒 到 23: 冬至）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SolarTerm {
    Xiaohan,     // 小寒 (0)
    Dahan,       // 大寒 (1)
    Lichun,      // 立春 (2)
    Yushui,      // 雨水 (3)
    Jingzhe,     // 惊蛰 (4)
    Chunfen,     // 春分 (5)
    Qingming,    // 清明 (6)
    Guyu,        // 谷雨 (7)
    Lixia,       // 立夏 (8)
    Xiaoman,     // 小满 (9)
    Mangzhong,   // 芒种 (10)
    Xiazhi,      // 夏至 (11)
    Xiaoshu,     // 小暑 (12)
    Dashu,       // 大暑 (13)
    Liqiu,       // 立秋 (14)
    Chushu,      // 处暑 (15)
    Bailu,       // 白露 (16)
    Qiufen,      // 秋分 (17)
    Hanlu,       // 寒露 (18)
    Shuangjiang, // 霜降 (19)
    Lidong,      // 立冬 (20)
    Xiaoxue,     // 小雪 (21)
    Daxue,       // 大雪 (22)
    Dongzhi,     // 冬至 (23)
}

impl SolarTerm {
    pub const ALL: [SolarTerm; 24] = [
        SolarTerm::Xiaohan,
        SolarTerm::Dahan,
        SolarTerm::Lichun,
        SolarTerm::Yushui,
        SolarTerm::Jingzhe,
        SolarTerm::Chunfen,
        SolarTerm::Qingming,
        SolarTerm::Guyu,
        SolarTerm::Lixia,
        SolarTerm::Xiaoman,
        SolarTerm::Mangzhong,
        SolarTerm::Xiazhi,
        SolarTerm::Xiaoshu,
        SolarTerm::Dashu,
        SolarTerm::Liqiu,
        SolarTerm::Chushu,
        SolarTerm::Bailu,
        SolarTerm::Qiufen,
        SolarTerm::Hanlu,
        SolarTerm::Shuangjiang,
        SolarTerm::Lidong,
        SolarTerm::Xiaoxue,
        SolarTerm::Daxue,
        SolarTerm::Dongzhi,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(idx: usize) -> Option<Self> {
        Self::ALL.get(idx % 24).copied()
    }

    pub fn name(self) -> &'static str {
        SOLAR_TERM_NAMES[self.index()]
    }

    pub fn desc(self) -> &'static str {
        SOLAR_TERM_DESCS[self.index()]
    }

    /// 计算给定年份中该节气的公历日期（天文台校准）
    pub fn date_in_year(self, year: i32) -> Option<NaiveDate> {
        if !(LUNAR_START_YEAR..=LUNAR_END_YEAR).contains(&year) {
            return None;
        }
        let y_idx = (year - LUNAR_START_YEAR) as usize;
        let t_idx = self.index();
        let day = terms_table::SOLAR_TERMS_DAYS[y_idx][t_idx] as u32;
        let month = (t_idx as u32 / 2) + 1;
        NaiveDate::from_ymd_opt(year, month, day)
    }
}

impl fmt::Display for SolarTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// 农历日期对象
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LunarDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub is_leap: bool,
    pub ganzhi_year: &'static str,
    pub zodiac: &'static str,
    pub month_name: &'static str,
    pub day_name: &'static str,
}

impl fmt::Display for LunarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let leap = if self.is_leap { "闰" } else { "" };
        write!(
            f,
            "{}年({}) {}{}{}",
            self.ganzhi_year, self.zodiac, leap, self.month_name, self.day_name
        )
    }
}

impl LunarDate {
    /// 简要格式，如 "七月廿四" 或 "闰六月初一"
    pub fn short_format(&self) -> String {
        let leap = if self.is_leap { "闰" } else { "" };
        format!("{}{}{}", leap, self.month_name, self.day_name)
    }
}

/// 节日信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holiday {
    pub name: String,
    pub is_major: bool,
    pub hint: String,
}

/// 重大节日提前提醒（3天与1天）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvanceWarning {
    pub holiday_name: String,
    pub days_left: i64,
    pub target_date: NaiveDate,
    pub hint: String,
}

impl AdvanceWarning {
    pub fn message(&self) -> String {
        if self.days_left == 1 {
            format!(
                "明天是{} ({}) · {}",
                self.holiday_name,
                self.target_date.format("%m-%d"),
                self.hint
            )
        } else {
            format!(
                "距{}还有 {} 天 ({}) · {}",
                self.holiday_name,
                self.days_left,
                self.target_date.format("%m-%d"),
                self.hint
            )
        }
    }
}

/// 某日聚合历法与节气信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarDayInfo {
    pub date: NaiveDate,
    pub lunar: LunarDate,
    pub solar_term: Option<SolarTerm>,
    pub holidays: Vec<Holiday>,
    pub next_solar_term: (SolarTerm, NaiveDate, i64),
    pub next_major_holiday: Option<(Holiday, NaiveDate, i64)>,
    pub warnings: Vec<AdvanceWarning>,
}

impl CalendarDayInfo {
    /// 紧凑的一行状态摘要，适合放置在标题栏或状态栏
    /// 例："七月廿四 · 处暑 · 距白露3天 · 距中秋21天"
    pub fn status_line(&self) -> String {
        let mut parts = Vec::new();
        parts.push(self.lunar.short_format());
        if let Some(term) = self.solar_term {
            parts.push(format!("🌟 今日节气: {}", term.name()));
        } else {
            let (next_term, _, days) = self.next_solar_term;
            if days <= 7 {
                parts.push(format!("距{}还有{}天", next_term.name(), days));
            }
        }
        for h in &self.holidays {
            if h.is_major {
                parts.push(format!("🎉 今日{}", h.name));
            } else {
                parts.push(format!("今日{}", h.name));
            }
        }
        if let Some((ref mh, _, days)) = self.next_major_holiday {
            if days > 0 && days <= 30 {
                parts.push(format!("距{}还有{}天", mh.name, days));
            }
        }
        parts.join(" · ")
    }
}

/// 将公历日期转为农历日期
pub fn lunar_from_date(date: NaiveDate) -> Option<LunarDate> {
    let base = NaiveDate::from_ymd_opt(LUNAR_START_YEAR, 1, 31)?;
    let mut offset = (date - base).num_days();
    if offset < 0 {
        return None;
    }

    let mut year = LUNAR_START_YEAR;
    while year <= LUNAR_END_YEAR {
        let idx = (year - LUNAR_START_YEAR) as usize;
        let info = *LUNAR_INFO.get(idx)?;
        let days_in_year = days_in_lunar_year(info);
        if offset < days_in_year {
            break;
        }
        offset -= days_in_year;
        year += 1;
    }

    if year > LUNAR_END_YEAR {
        return None;
    }

    let idx = (year - LUNAR_START_YEAR) as usize;
    let info = LUNAR_INFO[idx];
    let leap_month = info & 0x0f;
    let mut month = 1u32;
    let mut is_leap = false;

    while month <= 12 {
        let month_days = if (info & (0x10000 >> month)) != 0 {
            30
        } else {
            29
        };
        if offset < month_days {
            break;
        }
        offset -= month_days;

        if leap_month == month {
            let leap_days = if (info & 0x10000) != 0 { 30 } else { 29 };
            if offset < leap_days {
                is_leap = true;
                break;
            }
            offset -= leap_days;
        }
        month += 1;
    }

    let day = (offset + 1) as u32;
    let ganzhi_year = ganzhi_of_year(year);
    let zodiac = zodiac_of_year(year);
    let month_name = LUNAR_MONTH_NAMES
        .get((month.saturating_sub(1)) as usize)
        .copied()
        .unwrap_or("正");
    let day_name = LUNAR_DAY_NAMES
        .get((day.saturating_sub(1)) as usize)
        .copied()
        .unwrap_or("初一");

    Some(LunarDate {
        year,
        month,
        day,
        is_leap,
        ganzhi_year,
        zodiac,
        month_name,
        day_name,
    })
}

fn days_in_lunar_year(info: u32) -> i64 {
    let mut sum = 0;
    // 12 个普通月
    for m in 1..=12 {
        if (info & (0x10000 >> m)) != 0 {
            sum += 30;
        } else {
            sum += 29;
        }
    }
    // 闰月
    let leap_m = info & 0x0f;
    if leap_m > 0 {
        if (info & 0x10000) != 0 {
            sum += 30;
        } else {
            sum += 29;
        }
    }
    sum
}

fn ganzhi_of_year(year: i32) -> &'static str {
    let h_idx = ((year - 4).rem_euclid(10)) as usize;
    let e_idx = ((year - 4).rem_euclid(12)) as usize;
    static GANZHI_CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let cache = GANZHI_CACHE.get_or_init(|| {
        let mut v = Vec::new();
        for h in GANZHI_HEAVEN {
            for e in GANZHI_EARTH {
                v.push(format!("{}{}", h, e));
            }
        }
        v
    });
    // 查找对应干支组合
    let target = format!("{}{}", GANZHI_HEAVEN[h_idx], GANZHI_EARTH[e_idx]);
    cache
        .iter()
        .find(|s| *s == &target)
        .map(|s| s.as_str())
        .unwrap_or("丙午")
}

fn zodiac_of_year(year: i32) -> &'static str {
    let idx = ((year - 4).rem_euclid(12)) as usize;
    ZODIAC_ANIMALS[idx]
}

/// 获取指定公历日期对应的节气（如果有）
pub fn solar_term_of(date: NaiveDate) -> Option<SolarTerm> {
    let year = date.year();
    for term in SolarTerm::ALL {
        if let Some(d) = term.date_in_year(year) {
            if d == date {
                return Some(term);
            }
        }
    }
    None
}

/// 计算从 `from` 开始的下一个节气及剩余天数
pub fn next_solar_term(from: NaiveDate) -> (SolarTerm, NaiveDate, i64) {
    let year = from.year();
    // 遍历当前年及下一年所有的节气
    let mut candidates = Vec::new();
    for y in [year, year + 1] {
        for term in SolarTerm::ALL {
            if let Some(d) = term.date_in_year(y) {
                if d >= from {
                    candidates.push((term, d, (d - from).num_days()));
                }
            }
        }
    }
    candidates.sort_by_key(|(_, d, _)| *d);
    candidates
        .into_iter()
        .next()
        .unwrap_or((SolarTerm::Lichun, from, 0))
}

/// 判断指定日期是否为除夕（大年三十/廿九，即明天是农历正月初一）
pub fn is_new_years_eve(date: NaiveDate) -> bool {
    if let Some(tomorrow) = date.succ_opt() {
        if let Some(lunar_tomorrow) = lunar_from_date(tomorrow) {
            return lunar_tomorrow.month == 1 && lunar_tomorrow.day == 1;
        }
    }
    false
}

/// 获取指定公历日期当天的所有节日列表
pub fn holidays_of(date: NaiveDate) -> Vec<Holiday> {
    let mut list = Vec::new();
    let lunar_opt = lunar_from_date(date);
    let term_opt = solar_term_of(date);

    // 1. 除夕特判
    if is_new_years_eve(date) {
        list.push(Holiday {
            name: "除夕".to_string(),
            is_major: true,
            hint: "岁末除夕夜，团圆守岁迎新春，建议提前准备年夜饭与家庭聚会".to_string(),
        });
    }

    // 2. 清明节气特判（清明既是节气也是重大节日）
    if let Some(SolarTerm::Qingming) = term_opt {
        list.push(Holiday {
            name: "清明节".to_string(),
            is_major: true,
            hint: "清明时节，慎终追远，祭祖踏青，建议提前安排扫墓行程".to_string(),
        });
    }

    // 3. 冬至节气特判（冬至兼具传统大节）
    if let Some(SolarTerm::Dongzhi) = term_opt {
        list.push(Holiday {
            name: "冬至".to_string(),
            is_major: true,
            hint: "冬至大如年，人间小团圆，饺子汤圆暖心头".to_string(),
        });
    }

    // 4. 表格配置节日
    for def in HOLIDAYS {
        if def.is_lunar {
            if let Some(ref l) = lunar_opt {
                if !l.is_leap && l.month == def.month && l.day == def.day {
                    list.push(Holiday {
                        name: def.name.to_string(),
                        is_major: def.is_major,
                        hint: def.hint.to_string(),
                    });
                }
            }
        } else if date.month() == def.month && date.day() == def.day {
            list.push(Holiday {
                name: def.name.to_string(),
                is_major: def.is_major,
                hint: def.hint.to_string(),
            });
        }
    }

    list
}

/// 查找距离 `from` 最近的下一个重大节日
pub fn next_major_holiday(from: NaiveDate) -> Option<(Holiday, NaiveDate, i64)> {
    // 扫描未来 366 天
    for days in 1..=366 {
        let cur = from + Duration::days(days);
        for h in holidays_of(cur) {
            if h.is_major {
                return Some((h, cur, days));
            }
        }
    }
    None
}

/// 获取当日针对重大节日激活的提前 3 天与提前 1 天自动提醒
pub fn active_advance_warnings(date: NaiveDate) -> Vec<AdvanceWarning> {
    let mut warnings = Vec::new();

    // 检查 3 天后的节日提醒
    if let Some(d3) = date.checked_add_signed(Duration::days(3)) {
        for h in holidays_of(d3) {
            if h.is_major {
                warnings.push(AdvanceWarning {
                    holiday_name: h.name,
                    days_left: 3,
                    target_date: d3,
                    hint: h.hint,
                });
            }
        }
    }

    // 检查 1 天后（明天）的节日提醒
    if let Some(d1) = date.checked_add_signed(Duration::days(1)) {
        for h in holidays_of(d1) {
            if h.is_major {
                warnings.push(AdvanceWarning {
                    holiday_name: h.name,
                    days_left: 1,
                    target_date: d1,
                    hint: h.hint,
                });
            }
        }
    }

    warnings
}

/// 获取当日完整的聚合日历与节气信息
pub fn day_calendar_info(date: NaiveDate) -> Option<CalendarDayInfo> {
    let lunar = lunar_from_date(date)?;
    let solar_term = solar_term_of(date);
    let holidays = holidays_of(date);
    let next_solar_term = next_solar_term(date);
    let next_major_holiday = next_major_holiday(date);
    let warnings = active_advance_warnings(date);

    Some(CalendarDayInfo {
        date,
        lunar,
        solar_term,
        holidays,
        next_solar_term,
        next_major_holiday,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_lunar_dates() {
        // 2024-02-10 甲辰龙年正月初一
        let d = NaiveDate::from_ymd_opt(2024, 2, 10).unwrap();
        let l = lunar_from_date(d).unwrap();
        assert_eq!(l.year, 2024);
        assert_eq!(l.month, 1);
        assert_eq!(l.day, 1);
        assert_eq!(l.zodiac, "龙");
        assert_eq!(l.ganzhi_year, "甲辰");

        // 2026-09-04 丙午马年七月廿三
        let d = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let l = lunar_from_date(d).unwrap();
        assert_eq!(l.year, 2026);
        assert_eq!(l.month, 7);
        assert_eq!(l.day, 23);
        assert_eq!(l.zodiac, "马");
        assert_eq!(l.ganzhi_year, "丙午");
        assert_eq!(l.short_format(), "七月廿三");
    }

    #[test]
    fn test_solar_terms_dates() {
        // 2026 年白露在 2026-09-07
        let term_date = SolarTerm::Bailu.date_in_year(2026).unwrap();
        assert_eq!(term_date, NaiveDate::from_ymd_opt(2026, 9, 7).unwrap());

        // 2026-09-07 当天应检测出白露
        assert_eq!(solar_term_of(term_date), Some(SolarTerm::Bailu));

        // 2026-09-04 的下一个节气是白露，相距 3 天
        let from = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let (next_t, next_d, days) = next_solar_term(from);
        assert_eq!(next_t, SolarTerm::Bailu);
        assert_eq!(next_d, NaiveDate::from_ymd_opt(2026, 9, 7).unwrap());
        assert_eq!(days, 3);
    }

    #[test]
    fn test_major_holidays_and_advance_warnings() {
        // 2026 年中秋节是八月十五，公历 2026-09-25
        let mid_autumn = NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
        let hs = holidays_of(mid_autumn);
        assert!(hs.iter().any(|h| h.name == "中秋节" && h.is_major));

        // 中秋前 3 天：2026-09-22 应触发 3 天提醒
        let warn_3d = NaiveDate::from_ymd_opt(2026, 9, 22).unwrap();
        let warnings = active_advance_warnings(warn_3d);
        assert!(warnings
            .iter()
            .any(|w| w.holiday_name == "中秋节" && w.days_left == 3));

        // 中秋前 1 天：2026-09-24 应触发 1 天提醒
        let warn_1d = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        let warnings = active_advance_warnings(warn_1d);
        assert!(warnings
            .iter()
            .any(|w| w.holiday_name == "中秋节" && w.days_left == 1));

        // 清明与除夕判定
        let qingming_2026 = SolarTerm::Qingming.date_in_year(2026).unwrap();
        let qingming_hs = holidays_of(qingming_2026);
        assert!(qingming_hs.iter().any(|h| h.name == "清明节" && h.is_major));

        // 2026 春节前一天是除夕 (2026-02-16)
        let eve = NaiveDate::from_ymd_opt(2026, 2, 16).unwrap();
        assert!(is_new_years_eve(eve));
        let eve_hs = holidays_of(eve);
        assert!(eve_hs.iter().any(|h| h.name == "除夕" && h.is_major));
    }

    #[test]
    fn test_day_calendar_info_status_line() {
        let d = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let info = day_calendar_info(d).unwrap();
        let status = info.status_line();
        assert!(status.contains("七月廿三"));
        assert!(status.contains("白露"));
    }

    #[test]
    fn test_lunar_year_boundaries() {
        // 边界起始：1900-01-31 (农历 1900 正月初一)
        let base = NaiveDate::from_ymd_opt(1900, 1, 31).unwrap();
        let l_base = lunar_from_date(base).unwrap();
        assert_eq!(l_base.year, 1900);
        assert_eq!(l_base.month, 1);
        assert_eq!(l_base.day, 1);

        // 边界前一日 (1900-01-30) 超出范围返回 None
        let before = NaiveDate::from_ymd_opt(1900, 1, 30).unwrap();
        assert!(lunar_from_date(before).is_none());

        // 边界结束：2100-12-31 在有效范围内
        let end = NaiveDate::from_ymd_opt(2100, 12, 31).unwrap();
        assert!(lunar_from_date(end).is_some());

        // 超出 2100 年返回 None
        let after = NaiveDate::from_ymd_opt(2101, 3, 1).unwrap();
        assert!(lunar_from_date(after).is_none());
    }

    #[test]
    fn test_all_24_solar_terms_monotonic_order() {
        // 验证 24 节气在 2026 年皆可成功计算，且日期严格递增
        let mut prev_date = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap();
        for i in 0..24 {
            let term = SolarTerm::from_index(i).unwrap();
            let date = term.date_in_year(2026).unwrap();
            assert!(
                date > prev_date,
                "节气 {} ({}) 日期应晚于上一节气",
                term.name(),
                date
            );
            assert_eq!(solar_term_of(date), Some(term));
            prev_date = date;
        }
    }

    #[test]
    fn test_leap_month_handling() {
        // 2023 年农历闰二月：公历 2023-03-22 为闰二月初一
        let d = NaiveDate::from_ymd_opt(2023, 3, 22).unwrap();
        let l = lunar_from_date(d).unwrap();
        assert_eq!(l.month, 2);
        assert_eq!(l.day, 1);
        assert!(l.is_leap, "2023年应为闰二月");
        assert_eq!(l.short_format(), "闰二月初一");
    }

    #[test]
    fn test_all_major_holidays_defined_and_hints_valid() {
        for h in data::HOLIDAYS {
            assert!(!h.name.is_empty(), "节日名称不能为空");
            assert!(!h.hint.is_empty(), "节日行动提示不能为空");
        }

        // 静态表中的固定重大节日数量为 7 个 (春节, 元宵, 端午, 中秋, 元旦, 劳动节, 国庆节)
        let static_major_count = data::HOLIDAYS.iter().filter(|h| h.is_major).count();
        assert_eq!(static_major_count, 7);

        // 全年（如 2026 年）涵盖静态与动态重大节日（含清明节、除夕、冬至等），总数应 >= 9
        let mut year_major_holidays = std::collections::HashSet::new();
        let mut cur = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for _ in 0..365 {
            for h in holidays_of(cur) {
                if h.is_major {
                    year_major_holidays.insert(h.name);
                }
            }
            cur += Duration::days(1);
        }
        assert!(
            year_major_holidays.len() >= 9,
            "全年至少应覆盖 9 个核心重大节日: {:?}",
            year_major_holidays
        );
        assert!(year_major_holidays.contains("清明节"));
        assert!(year_major_holidays.contains("除夕"));
        assert!(year_major_holidays.contains("中秋节"));
        assert!(year_major_holidays.contains("国庆节"));
    }
}
