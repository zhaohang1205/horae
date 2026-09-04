//! 农历与二十四节气静态数据表（1900–2100）

/// 1900-2100 各年农历编码表（经天文台校对的标准表）。
/// 每个 u32 格式：
/// - bits 0..3: 闰月月份 (0 表示无闰月，1-12 表示闰几月)
/// - bits 4..15: 1~12 月每月大小 (1 为大月 30 天，0 为小月 29 天)
/// - bit 16: 闰月大小 (1 为大月 30 天，0 为小月 29 天)
pub const LUNAR_INFO: [u32; 201] = [
    0x04BD8, 0x04AE0, 0x0A570, 0x054D5, 0x0D260, 0x0D950, 0x16554, 0x056A0, 0x09AD0,
    0x055D2, // 1900-1909
    0x04AE0, 0x0A5B6, 0x0A4D0, 0x0D250, 0x1D255, 0x0B540, 0x0D6A0, 0x0ADA2, 0x095B0,
    0x14977, // 1910-1919
    0x04970, 0x0A4B0, 0x0B4B5, 0x06A50, 0x06D40, 0x1AB54, 0x02B60, 0x09570, 0x052F2,
    0x04970, // 1920-1929
    0x06566, 0x0D4A0, 0x0EA50, 0x16A95, 0x05AD0, 0x02B60, 0x186E3, 0x092E0, 0x1C8D7,
    0x0C950, // 1930-1939
    0x0D4A0, 0x1D8A6, 0x0B550, 0x056A0, 0x1A5B4, 0x025D0, 0x092D0, 0x0D2B2, 0x0A950,
    0x0B557, // 1940-1949
    0x06CA0, 0x0B550, 0x15355, 0x04DA0, 0x0A5B0, 0x14573, 0x052B0, 0x0A9A8, 0x0E950,
    0x06AA0, // 1950-1959
    0x0AEA6, 0x0AB50, 0x04B60, 0x0AAE4, 0x0A570, 0x05260, 0x0F263, 0x0D950, 0x05B57,
    0x056A0, // 1960-1969
    0x096D0, 0x04DD5, 0x04AD0, 0x0A4D0, 0x0D4D4, 0x0D250, 0x0D558, 0x0B540, 0x0B6A0,
    0x195A6, // 1970-1979
    0x095B0, 0x049B0, 0x0A974, 0x0A4B0, 0x0B27A, 0x06A50, 0x06D40, 0x0AF46, 0x0AB60,
    0x09570, // 1980-1989
    0x04AF5, 0x04970, 0x064B0, 0x074A3, 0x0EA50, 0x06B58, 0x05AC0, 0x0AB60, 0x096D5,
    0x092E0, // 1990-1999
    0x0C960, 0x0D954, 0x0D4A0, 0x0DA50, 0x07552, 0x056A0, 0x0ABB7, 0x025D0, 0x092D0,
    0x0CAB5, // 2000-2009
    0x0A950, 0x0B4A0, 0x0BAA4, 0x0AD50, 0x055D9, 0x04BA0, 0x0A5B0, 0x15176, 0x052B0,
    0x0A930, // 2010-2019
    0x07954, 0x06AA0, 0x0AD50, 0x05B52, 0x04B60, 0x0A6E6, 0x0A4E0, 0x0D260, 0x0EA65,
    0x0D530, // 2020-2029
    0x05AA0, 0x076A3, 0x096D0, 0x04AFB, 0x04AD0, 0x0A4D0, 0x1D0B6, 0x0D250, 0x0D520,
    0x0DD45, // 2030-2039
    0x0B5A0, 0x056D0, 0x055B2, 0x049B0, 0x0A577, 0x0A4B0, 0x0AA50, 0x1B255, 0x06D20,
    0x0ADA0, // 2040-2049
    0x14B63, 0x09370, 0x049F8, 0x04970, 0x064B0, 0x168A6, 0x0EA50, 0x06AA0, 0x1A6C4,
    0x0AAE0, // 2050-2059
    0x092E0, 0x0D2E3, 0x0C960, 0x0D557, 0x0D4A0, 0x0DA50, 0x05D55, 0x056A0, 0x0A6D0,
    0x055D4, // 2060-2069
    0x052D0, 0x0A9B8, 0x0A950, 0x0B4A0, 0x0B6A6, 0x0AD50, 0x055A0, 0x0ABA4, 0x0A5B0,
    0x052B0, // 2070-2079
    0x0B273, 0x06930, 0x07337, 0x06AA0, 0x0AD50, 0x14B55, 0x04B60, 0x0A570, 0x054E4,
    0x0D160, // 2080-2089
    0x0E968, 0x0D520, 0x0DAA0, 0x16AA6, 0x056D0, 0x04AE0, 0x0A9D4, 0x0A2D0, 0x0D150,
    0x0F252, // 2090-2099
    0x0D520, // 2100
];

/// 农历算法起点年份与公历日期：1900-01-31 为农历庚子年正月初一。
pub const LUNAR_START_YEAR: i32 = 1900;
pub const LUNAR_END_YEAR: i32 = 2100;

/// 24节气时间分钟偏移量（从小寒为 0 开始算起）。
pub const S_TERM_INFO: [i64; 24] = [
    0, 21208, 42467, 63836, 85337, 107014, 128867, 150921, 173149, 195551, 218072, 240693, 263343,
    285989, 308563, 331033, 353350, 375494, 397447, 419210, 440795, 462224, 483532, 504758,
];

/// 回归年毫秒常数 (365.2421990741 * 24 * 3600 * 1000)
pub const TROPICAL_YEAR_MS: f64 = 31556925974.7;

/// 1900-01-06 02:05:00 UTC 对应的时间戳（毫秒）—— 1900年小寒基准点。
pub const BASE_S_TERM_UTC_MS: i64 = -2208942900000;

/// 节气索引对应的名称（从 0: 小寒 到 23: 冬至）。
pub const SOLAR_TERM_NAMES: [&str; 24] = [
    "小寒", "大寒", "立春", "雨水", "惊蛰", "春分", "清明", "谷雨", "立夏", "小满", "芒种", "夏至",
    "小暑", "大暑", "立秋", "处暑", "白露", "秋分", "寒露", "霜降", "立冬", "小雪", "大雪", "冬至",
];

/// 二十四节气物候/时令描述
pub const SOLAR_TERM_DESCS: [&str; 24] = [
    "雁北乡，鹊始巢，雉雊",                   // 小寒
    "鸡乳，征鸟厉疾，水泽腹坚",               // 大寒
    "东风解冻，蛰虫始振，鱼陟负冰",           // 立春
    "獭祭鱼，候雁北，草木萌动",               // 雨水
    "桃始华，仓庚鸣，鹰化为鸠",               // 惊蛰
    "玄鸟至，雷乃发声，始电",                 // 春分
    "桐始华，田鼠化为鴽，虹始见",             // 清明
    "萍始生，鸣鸠拂其羽，戴胜降于桑",         // 谷雨
    "蝼蝈鸣，蚯蚓出，王瓜生",                 // 立夏
    "苦菜秀，靡草死，麦秋至",                 // 小满
    "螳螂生，鵙始鸣，反舌无声",               // 芒种
    "鹿角解，蝉始鸣，半夏生",                 // 夏至
    "温风至，蟋蟀居壁，鹰始挚",               // 小暑
    "腐草为萤，土润溽暑，大雨时行",           // 大暑
    "凉风至，白露降，寒蝉鸣",                 // 立秋
    "鹰乃祭鸟，天地始肃，禾乃登",             // 处暑
    "鸿雁来，玄鸟归，群鸟养羞",               // 白露
    "雷始收声，蛰虫坯户，水始涸",             // 秋分
    "鸿雁来宾，雀入大水为蛤，菊有黄华",       // 寒露
    "豺乃祭兽，草木黄落，蛰虫咸俯",           // 霜降
    "水始冰，地始冻，雉入大水为蜃",           // 立冬
    "虹藏不见，天气上升地气下降，闭塞而成冬", // 小雪
    "鹖鴠不鸣，虎始交，荔挺出",               // 大雪
    "蚯蚓结，麋角解，水泉动",                 // 冬至
];

/// 天干
pub const GANZHI_HEAVEN: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
/// 地支
pub const GANZHI_EARTH: [&str; 12] = [
    "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥",
];
/// 十二生肖
pub const ZODIAC_ANIMALS: [&str; 12] = [
    "鼠", "牛", "虎", "兔", "龙", "蛇", "马", "羊", "猴", "鸡", "狗", "猪",
];

/// 农历月中文名
pub const LUNAR_MONTH_NAMES: [&str; 12] = [
    "正月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "冬月", "腊月",
];

/// 农历日中文名
pub const LUNAR_DAY_NAMES: [&str; 30] = [
    "初一", "初二", "初三", "初四", "初五", "初六", "初七", "初八", "初九", "初十", "十一", "十二",
    "十三", "十四", "十五", "十六", "十七", "十八", "十九", "二十", "廿一", "廿二", "廿三", "廿四",
    "廿五", "廿六", "廿七", "廿八", "廿九", "三十",
];

/// 节日定义
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HolidayDef {
    pub name: &'static str,
    /// 是否为农历节日（true 为农历月日，false 为公历月日）
    pub is_lunar: bool,
    pub month: u32,
    pub day: u32,
    /// 是否为重大节日（提前 3 天与 1 天自动提醒）
    pub is_major: bool,
    /// 节日备忘/问候提示
    pub hint: &'static str,
}

pub const HOLIDAYS: &[HolidayDef] = &[
    // ── 农历传统重大节日 ──
    HolidayDef {
        name: "春节",
        is_lunar: true,
        month: 1,
        day: 1,
        is_major: true,
        hint: "岁首迎春，阖家欢聚，建议提前备齐年货与规划探亲返乡行程",
    },
    HolidayDef {
        name: "元宵节",
        is_lunar: true,
        month: 1,
        day: 15,
        is_major: true,
        hint: "正月十五上元佳节，赏花灯品元宵",
    },
    HolidayDef {
        name: "端午节",
        is_lunar: true,
        month: 5,
        day: 5,
        is_major: true,
        hint: "端午粽香，龙舟竞渡，建议提前准备粽子与短假出行安排",
    },
    HolidayDef {
        name: "中秋节",
        is_lunar: true,
        month: 8,
        day: 15,
        is_major: true,
        hint: "月满人团圆，建议提前准备月饼与拜访礼品",
    },
    // ── 农历传统常规节日 ──
    HolidayDef {
        name: "龙抬头",
        is_lunar: true,
        month: 2,
        day: 2,
        is_major: false,
        hint: "二月二龙抬头，万物复苏祈丰年",
    },
    HolidayDef {
        name: "七夕节",
        is_lunar: true,
        month: 7,
        day: 7,
        is_major: false,
        hint: "迢迢牵牛星，皎皎河汉女，愿天下有情人终成眷属",
    },
    HolidayDef {
        name: "中元节",
        is_lunar: true,
        month: 7,
        day: 15,
        is_major: false,
        hint: "七月半敬祖祈安",
    },
    HolidayDef {
        name: "重阳节",
        is_lunar: true,
        month: 9,
        day: 9,
        is_major: false,
        hint: "九九重阳，登高赏菊，敬老祈寿",
    },
    HolidayDef {
        name: "腊八节",
        is_lunar: true,
        month: 12,
        day: 8,
        is_major: false,
        hint: "腊七腊八冻掉下巴，温粥暖心迎新岁",
    },
    HolidayDef {
        name: "小年",
        is_lunar: true,
        month: 12,
        day: 23,
        is_major: false,
        hint: "祭灶扫尘辞旧岁",
    },
    // ── 公历法定重大长假 ──
    HolidayDef {
        name: "元旦",
        is_lunar: false,
        month: 1,
        day: 1,
        is_major: true,
        hint: "新年伊始，辞旧迎新，建议做好新年规划与假期安排",
    },
    HolidayDef {
        name: "劳动节",
        is_lunar: false,
        month: 5,
        day: 1,
        is_major: true,
        hint: "五一劳动节，致敬劳动者，建议提前规划假期行程",
    },
    HolidayDef {
        name: "国庆节",
        is_lunar: false,
        month: 10,
        day: 1,
        is_major: true,
        hint: "盛世华诞，举国欢庆，黄金周建议提前安排出行车票与酒店",
    },
    // ── 公历常规节日 ──
    HolidayDef {
        name: "妇女节",
        is_lunar: false,
        month: 3,
        day: 8,
        is_major: false,
        hint: "致敬女性力量",
    },
    HolidayDef {
        name: "植树节",
        is_lunar: false,
        month: 3,
        day: 12,
        is_major: false,
        hint: "春满人间，植绿护青山",
    },
    HolidayDef {
        name: "青年节",
        is_lunar: false,
        month: 5,
        day: 4,
        is_major: false,
        hint: "青春向阳，勇毅前行",
    },
    HolidayDef {
        name: "儿童节",
        is_lunar: false,
        month: 6,
        day: 1,
        is_major: false,
        hint: "童心未泯，快乐成长",
    },
    HolidayDef {
        name: "建军节",
        is_lunar: false,
        month: 8,
        day: 1,
        is_major: false,
        hint: "致敬最可爱的人",
    },
    HolidayDef {
        name: "教师节",
        is_lunar: false,
        month: 9,
        day: 10,
        is_major: false,
        hint: "甘为人梯，师恩难忘",
    },
    HolidayDef {
        name: "程序员节",
        is_lunar: false,
        month: 10,
        day: 24,
        is_major: false,
        hint: "1024 程序员节，代码无 bug，幸福每一天",
    },
];
