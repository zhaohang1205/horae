use horae_core::model::task::Status;
use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    CatppuccinMocha,
    CatppuccinLatte,
    TokyoNight,
    Gruvbox,
    Retro82,
    Ethereal,
    Everforest,
    FlexokiLight,
    Hackerman,
    JadeHorse,
    Kanagawa,
    LastHorizon,
    Lumon,
    Lupine,
    MatteBlack,
    Miasma,
    Nord,
    OsakaJade,
    Ristretto,
    RosePine,
    Solitude,
    Starship,
    Vantablack,
    White,
}

impl ThemeId {
    pub const ALL: &'static [Self] = &[
        Self::CatppuccinMocha,
        Self::CatppuccinLatte,
        Self::TokyoNight,
        Self::Gruvbox,
        Self::Retro82,
        Self::Ethereal,
        Self::Everforest,
        Self::FlexokiLight,
        Self::Hackerman,
        Self::JadeHorse,
        Self::Kanagawa,
        Self::LastHorizon,
        Self::Lumon,
        Self::Lupine,
        Self::MatteBlack,
        Self::Miasma,
        Self::Nord,
        Self::OsakaJade,
        Self::Ristretto,
        Self::RosePine,
        Self::Solitude,
        Self::Starship,
        Self::Vantablack,
        Self::White,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            Self::CatppuccinMocha => "mocha",
            Self::CatppuccinLatte => "latte",
            Self::TokyoNight => "tokyo-night",
            Self::Gruvbox => "gruvbox",
            Self::Retro82 => "retro-82",
            Self::Ethereal => "ethereal",
            Self::Everforest => "everforest",
            Self::FlexokiLight => "flexoki-light",
            Self::Hackerman => "hackerman",
            Self::JadeHorse => "jade-horse",
            Self::Kanagawa => "kanagawa",
            Self::LastHorizon => "last-horizon",
            Self::Lumon => "lumon",
            Self::Lupine => "lupine",
            Self::MatteBlack => "matte-black",
            Self::Miasma => "miasma",
            Self::Nord => "nord",
            Self::OsakaJade => "osaka-jade",
            Self::Ristretto => "ristretto",
            Self::RosePine => "rose-pine",
            Self::Solitude => "solitude",
            Self::Starship => "starship",
            Self::Vantablack => "vantablack",
            Self::White => "white",
        }
    }

    pub fn display_name(&self, lang: horae_core::i18n::Lang) -> &'static str {
        match (self, lang) {
            (Self::CatppuccinMocha, horae_core::i18n::Lang::Zh) => "Catppuccin 摩卡 (深色)",
            (Self::CatppuccinMocha, horae_core::i18n::Lang::En) => "Catppuccin Mocha (Dark)",
            (Self::CatppuccinLatte, horae_core::i18n::Lang::Zh) => "Catppuccin 拿铁 (亮色)",
            (Self::CatppuccinLatte, horae_core::i18n::Lang::En) => "Catppuccin Latte (Light)",
            (Self::TokyoNight, horae_core::i18n::Lang::Zh) => "Tokyo Night 东京之夜 (深色)",
            (Self::TokyoNight, horae_core::i18n::Lang::En) => "Tokyo Night (Dark)",
            (Self::Gruvbox, horae_core::i18n::Lang::Zh) => "Gruvbox 复古暖色 (深色)",
            (Self::Gruvbox, horae_core::i18n::Lang::En) => "Gruvbox (Dark)",
            (Self::Retro82, horae_core::i18n::Lang::Zh) => "Retro 82 复古未来 (深色)",
            (Self::Retro82, horae_core::i18n::Lang::En) => "Retro 82 (Dark)",
            (Self::Ethereal, horae_core::i18n::Lang::Zh) => "Ethereal 空灵 (深色)",
            (Self::Ethereal, horae_core::i18n::Lang::En) => "Ethereal (Dark)",
            (Self::Everforest, horae_core::i18n::Lang::Zh) => "Everforest 常青森林 (深色)",
            (Self::Everforest, horae_core::i18n::Lang::En) => "Everforest (Dark)",
            (Self::FlexokiLight, horae_core::i18n::Lang::Zh) => "Flexoki 暖纸 (亮色)",
            (Self::FlexokiLight, horae_core::i18n::Lang::En) => "Flexoki Light",
            (Self::Hackerman, horae_core::i18n::Lang::Zh) => "Hackerman 黑客绿 (深色)",
            (Self::Hackerman, horae_core::i18n::Lang::En) => "Hackerman (Dark)",
            (Self::JadeHorse, horae_core::i18n::Lang::Zh) => "Jade Horse 翡翠玉马 (深色)",
            (Self::JadeHorse, horae_core::i18n::Lang::En) => "Jade Horse (Dark)",
            (Self::Kanagawa, horae_core::i18n::Lang::Zh) => "Kanagawa 神奈川 (深色)",
            (Self::Kanagawa, horae_core::i18n::Lang::En) => "Kanagawa (Dark)",
            (Self::LastHorizon, horae_core::i18n::Lang::Zh) => "Last Horizon 暮色地平线 (深色)",
            (Self::LastHorizon, horae_core::i18n::Lang::En) => "Last Horizon (Dark)",
            (Self::Lumon, horae_core::i18n::Lang::Zh) => "Lumon 卢蒙冷调 (深色)",
            (Self::Lumon, horae_core::i18n::Lang::En) => "Lumon (Dark)",
            (Self::Lupine, horae_core::i18n::Lang::Zh) => "Lupine 鲁冰花 (亮色)",
            (Self::Lupine, horae_core::i18n::Lang::En) => "Lupine (Light)",
            (Self::MatteBlack, horae_core::i18n::Lang::Zh) => "Matte Black 哑光黑 (深色)",
            (Self::MatteBlack, horae_core::i18n::Lang::En) => "Matte Black (Dark)",
            (Self::Miasma, horae_core::i18n::Lang::Zh) => "Miasma 沼泽复古 (深色)",
            (Self::Miasma, horae_core::i18n::Lang::En) => "Miasma (Dark)",
            (Self::Nord, horae_core::i18n::Lang::Zh) => "Nord 极光北欧 (深色)",
            (Self::Nord, horae_core::i18n::Lang::En) => "Nord (Dark)",
            (Self::OsakaJade, horae_core::i18n::Lang::Zh) => "Osaka Jade 大阪翡翠 (深色)",
            (Self::OsakaJade, horae_core::i18n::Lang::En) => "Osaka Jade (Dark)",
            (Self::Ristretto, horae_core::i18n::Lang::Zh) => "Ristretto 浓缩咖啡 (深色)",
            (Self::Ristretto, horae_core::i18n::Lang::En) => "Ristretto (Dark)",
            (Self::RosePine, horae_core::i18n::Lang::Zh) => "Rosé Pine 玫瑰松木 (亮色)",
            (Self::RosePine, horae_core::i18n::Lang::En) => "Rosé Pine (Light)",
            (Self::Solitude, horae_core::i18n::Lang::Zh) => "Solitude 孤寂冷灰 (深色)",
            (Self::Solitude, horae_core::i18n::Lang::En) => "Solitude (Dark)",
            (Self::Starship, horae_core::i18n::Lang::Zh) => "Starship 星舰 (深色)",
            (Self::Starship, horae_core::i18n::Lang::En) => "Starship (Dark)",
            (Self::Vantablack, horae_core::i18n::Lang::Zh) => "Vantablack 纯粹暗黑 (深色)",
            (Self::Vantablack, horae_core::i18n::Lang::En) => "Vantablack (Dark)",
            (Self::White, horae_core::i18n::Lang::Zh) => "White 极简纯白 (亮色)",
            (Self::White, horae_core::i18n::Lang::En) => "White (Light)",
        }
    }

    pub fn next(&self) -> Self {
        let all = Self::ALL;
        if let Some(pos) = all.iter().position(|id| id == self) {
            all[(pos + 1) % all.len()]
        } else {
            Self::CatppuccinMocha
        }
    }

    pub fn is_last(&self) -> bool {
        *self == Self::White
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub id: ThemeId,
    pub is_dark: bool,
    pub bg: Color,
    pub fg: Color,
    pub text_dim: Color,
    pub text_urgent: Color,
    pub text_warn: Color,
    pub text_success: Color,
    pub rrule_fg: Color,

    // Borders
    pub border_active: Color,
    pub border_inactive: Color,

    // Highlight
    pub hl_bg: Color,
    pub hl_fg: Color,
    pub row_active_bg: Color,

    // Status Bar
    pub status_bg: Color,
    pub status_fg: Color,

    // Specific elements
    pub accent: Color,

    // Semantic status colors
    pub status_inbox: Color,
    pub status_next: Color,
    pub status_waiting: Color,
    pub status_scheduled: Color,
    pub status_someday: Color,
    pub status_reference: Color,
    pub status_done: Color,

    // Priority colors
    pub priority_high: Color,
    pub priority_medium: Color,
    pub priority_low: Color,
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            id: ThemeId::CatppuccinMocha,
            is_dark: true,
            bg: Color::Rgb(30, 30, 46),              // Base #1e1e2e
            fg: Color::Rgb(205, 214, 244),           // Text #cdd6f4
            text_dim: Color::Rgb(108, 112, 134),     // Overlay0 #6c7086
            text_urgent: Color::Rgb(243, 139, 168),  // Red #f38ba8
            text_warn: Color::Rgb(249, 226, 175),    // Yellow #f9e2af
            text_success: Color::Rgb(166, 227, 161), // Green #a6e3a1
            rrule_fg: Color::Rgb(250, 179, 135),     // Peach #fab387

            border_active: Color::Rgb(137, 180, 250), // Blue #89b4fa
            border_inactive: Color::Rgb(69, 71, 90),  // Surface1 #45475a

            hl_bg: Color::Rgb(49, 50, 68),          // Surface0 #313244
            hl_fg: Color::Rgb(137, 180, 250),       // Blue #89b4fa
            row_active_bg: Color::Rgb(88, 91, 112), // Surface2 #585b70

            status_bg: Color::Rgb(24, 24, 37),    // Mantle #181825
            status_fg: Color::Rgb(186, 194, 222), // Subtext1 #bac2de

            accent: Color::Rgb(137, 180, 250), // Blue #89b4fa

            // Status colors
            status_inbox: Color::Rgb(147, 153, 178), // Overlay2 #9399b2
            status_next: Color::Rgb(166, 227, 161),  // Green #a6e3a1
            status_waiting: Color::Rgb(249, 226, 175), // Yellow #f9e2af
            status_scheduled: Color::Rgb(137, 180, 250), // Blue #89b4fa
            status_someday: Color::Rgb(203, 166, 247), // Mauve #cba6f7
            status_reference: Color::Rgb(148, 226, 213), // Teal #94e2d5
            status_done: Color::Rgb(127, 132, 156),  // Overlay1 #7f849c

            // Priority colors
            priority_high: Color::Rgb(243, 139, 168), // Red #f38ba8
            priority_medium: Color::Rgb(249, 226, 175), // Yellow #f9e2af
            priority_low: Color::Rgb(137, 180, 250),  // Blue #89b4fa
        }
    }

    pub fn catppuccin_latte() -> Self {
        Self {
            id: ThemeId::CatppuccinLatte,
            is_dark: false,
            bg: Color::Rgb(239, 241, 245),         // Base #eff1f5
            fg: Color::Rgb(76, 79, 105),           // Text #4c4f69
            text_dim: Color::Rgb(108, 111, 133),   // Subtext0 #6c6f85
            text_urgent: Color::Rgb(210, 15, 57),  // Red #d20f39
            text_warn: Color::Rgb(223, 142, 29),   // Yellow #df8e1d
            text_success: Color::Rgb(64, 160, 43), // Green #40a02b
            rrule_fg: Color::Rgb(254, 100, 11),    // Peach #fe640b

            border_active: Color::Rgb(30, 102, 245), // Blue #1e66f5
            border_inactive: Color::Rgb(156, 160, 176), // Overlay0 #9ca0b0

            hl_bg: Color::Rgb(204, 208, 218), // Surface0 #ccd0da
            hl_fg: Color::Rgb(30, 102, 245),  // Blue #1e66f5
            row_active_bg: Color::Rgb(172, 176, 190), // Surface2 #acb0be

            status_bg: Color::Rgb(230, 233, 239), // Mantle #e6e9ef
            status_fg: Color::Rgb(76, 79, 105),   // Text #4c4f69

            accent: Color::Rgb(30, 102, 245), // Blue #1e66f5

            // Status colors
            status_inbox: Color::Rgb(138, 143, 161), // Overlay1 #8a8fa1
            status_next: Color::Rgb(64, 160, 43),    // Green #40a02b
            status_waiting: Color::Rgb(223, 142, 29), // Yellow #df8e1d
            status_scheduled: Color::Rgb(30, 102, 245), // Blue #1e66f5
            status_someday: Color::Rgb(136, 57, 239), // Mauve #8839ef
            status_reference: Color::Rgb(23, 146, 153), // Teal #179299
            status_done: Color::Rgb(138, 143, 161),  // Overlay1 #8a8fa1

            // Priority colors
            priority_high: Color::Rgb(210, 15, 57), // Red #d20f39
            priority_medium: Color::Rgb(223, 142, 29), // Yellow #df8e1d
            priority_low: Color::Rgb(30, 102, 245), // Blue #1e66f5
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            id: ThemeId::TokyoNight,
            is_dark: true,
            bg: Color::Rgb(26, 27, 38),              // #1a1b26
            fg: Color::Rgb(192, 202, 245),           // #c0caf5
            text_dim: Color::Rgb(86, 95, 137),       // #565f89
            text_urgent: Color::Rgb(247, 118, 142),  // Red #f7768e
            text_warn: Color::Rgb(224, 175, 104),    // Yellow #e0af68
            text_success: Color::Rgb(158, 206, 106), // Green #9ece6a
            rrule_fg: Color::Rgb(255, 158, 100),     // Orange #ff9e64

            border_active: Color::Rgb(122, 162, 247), // Blue #7aa2f7
            border_inactive: Color::Rgb(65, 72, 104), // Muted #414868

            hl_bg: Color::Rgb(41, 46, 66),         // Selection #292e42
            hl_fg: Color::Rgb(122, 162, 247),      // Blue #7aa2f7
            row_active_bg: Color::Rgb(54, 60, 86), // #363c56

            status_bg: Color::Rgb(19, 20, 28), // Dark background #13141c
            status_fg: Color::Rgb(169, 177, 214), // Light fg #a9b1d6

            accent: Color::Rgb(122, 162, 247), // Blue #7aa2f7

            // Status colors
            status_inbox: Color::Rgb(169, 177, 214), // Light fg #a9b1d6
            status_next: Color::Rgb(158, 206, 106),  // Green #9ece6a
            status_waiting: Color::Rgb(224, 175, 104), // Yellow #e0af68
            status_scheduled: Color::Rgb(122, 162, 247), // Blue #7aa2f7
            status_someday: Color::Rgb(187, 154, 247), // Bright magenta #bb9af7
            status_reference: Color::Rgb(13, 185, 215), // Bright cyan #0db9d7
            status_done: Color::Rgb(86, 95, 137),    // Dark fg #565f89

            // Priority colors
            priority_high: Color::Rgb(247, 118, 142), // Red #f7768e
            priority_medium: Color::Rgb(224, 175, 104), // Yellow #e0af68
            priority_low: Color::Rgb(122, 162, 247),  // Blue #7aa2f7
        }
    }

    pub fn gruvbox() -> Self {
        Self {
            id: ThemeId::Gruvbox,
            is_dark: true,
            bg: Color::Rgb(40, 40, 40),              // Background #282828
            fg: Color::Rgb(212, 190, 152),           // Foreground #d4be98
            text_dim: Color::Rgb(124, 111, 100),     // Dark fg #7c6f64
            text_urgent: Color::Rgb(234, 105, 98),   // Red #ea6962
            text_warn: Color::Rgb(216, 166, 87),     // Yellow #d8a657
            text_success: Color::Rgb(169, 182, 101), // Green #a9b665
            rrule_fg: Color::Rgb(225, 135, 92),      // Orange #e1875c

            border_active: Color::Rgb(125, 174, 163), // Aqua/Blue #7daea3
            border_inactive: Color::Rgb(102, 92, 84), // Muted #665c54

            hl_bg: Color::Rgb(60, 56, 54),         // Lighter bg #3c3836
            hl_fg: Color::Rgb(216, 166, 87),       // Yellow #d8a657
            row_active_bg: Color::Rgb(80, 73, 69), // Selection #504945

            status_bg: Color::Rgb(30, 30, 30),    // Dark bg #1e1e1e
            status_fg: Color::Rgb(189, 174, 147), // Light fg #bdae93

            accent: Color::Rgb(125, 174, 163), // Aqua #7daea3

            // Status colors
            status_inbox: Color::Rgb(189, 174, 147), // Light fg #bdae93
            status_next: Color::Rgb(169, 182, 101),  // Green #a9b665
            status_waiting: Color::Rgb(216, 166, 87), // Yellow #d8a657
            status_scheduled: Color::Rgb(125, 174, 163), // Blue #7daea3
            status_someday: Color::Rgb(211, 134, 155), // Magenta #d3869b
            status_reference: Color::Rgb(137, 180, 130), // Cyan/Green #89b482
            status_done: Color::Rgb(124, 111, 100),  // Muted #7c6f64

            // Priority colors
            priority_high: Color::Rgb(234, 105, 98), // Red #ea6962
            priority_medium: Color::Rgb(216, 166, 87), // Yellow #d8a657
            priority_low: Color::Rgb(125, 174, 163), // Aqua #7daea3
        }
    }

    pub fn retro_82() -> Self {
        Self {
            id: ThemeId::Retro82,
            is_dark: true,
            bg: Color::Rgb(5, 24, 46),              // Deep navy #05182e
            fg: Color::Rgb(246, 220, 172),          // Amber cream #f6dcac
            text_dim: Color::Rgb(63, 143, 138),     // Teal #3f8f8a
            text_urgent: Color::Rgb(248, 85, 37),   // Neon red/orange #f85525
            text_warn: Color::Rgb(233, 123, 60),    // Warm orange #e97b3c
            text_success: Color::Rgb(53, 162, 167), // Bright teal #35a2a7
            rrule_fg: Color::Rgb(250, 169, 104),    // Amber accent #faa968

            border_active: Color::Rgb(250, 169, 104), // Amber accent #faa968
            border_inactive: Color::Rgb(42, 107, 120), // Muted teal #2a6b78

            hl_bg: Color::Rgb(19, 78, 90),          // Selection #134e5a
            hl_fg: Color::Rgb(250, 169, 104),       // Amber #faa968
            row_active_bg: Color::Rgb(30, 95, 108), // #1e5f6c

            status_bg: Color::Rgb(3, 18, 34),     // Dark bg #031222
            status_fg: Color::Rgb(167, 201, 198), // Light fg #a7c9c6

            accent: Color::Rgb(250, 169, 104), // Warm amber #faa968

            // Status colors
            status_inbox: Color::Rgb(167, 201, 198), // Light fg #a7c9c6
            status_next: Color::Rgb(53, 162, 167),   // Bright teal #35a2a7
            status_waiting: Color::Rgb(233, 123, 60), // Orange #e97b3c
            status_scheduled: Color::Rgb(250, 169, 104), // Amber #faa968
            status_someday: Color::Rgb(101, 165, 161), // Bright magenta/cyan #65a5a1
            status_reference: Color::Rgb(140, 191, 184), // Bright cyan #8cbfb8
            status_done: Color::Rgb(63, 143, 138),   // Dim teal #3f8f8a

            // Priority colors
            priority_high: Color::Rgb(248, 85, 37), // Neon red #f85525
            priority_medium: Color::Rgb(233, 123, 60), // Amber #e97b3c
            priority_low: Color::Rgb(53, 162, 167), // Teal #35a2a7
        }
    }

    pub fn ethereal() -> Self {
        Self {
            id: ThemeId::Ethereal,
            is_dark: true,
            bg: Color::Rgb(6, 11, 30),               // #060B1E
            fg: Color::Rgb(255, 206, 173),           // #ffcead
            text_dim: Color::Rgb(109, 125, 182),     // #6d7db6
            text_urgent: Color::Rgb(237, 91, 90),    // #ED5B5A
            text_warn: Color::Rgb(233, 187, 79),     // #E9BB4F
            text_success: Color::Rgb(146, 165, 147), // #92a593
            rrule_fg: Color::Rgb(235, 139, 84),      // #eb8b54

            border_active: Color::Rgb(125, 130, 217), // #7d82d9
            border_inactive: Color::Rgb(37, 46, 86),  // #252e56

            hl_bg: Color::Rgb(19, 26, 58),            // #131a3a
            hl_fg: Color::Rgb(125, 130, 217),         // #7d82d9
            row_active_bg: Color::Rgb(109, 125, 182), // #6d7db6

            status_bg: Color::Rgb(4, 8, 22),      // #040816
            status_fg: Color::Rgb(201, 184, 166), // #c9b8a6

            accent: Color::Rgb(125, 130, 217), // #7d82d9

            // Status colors
            status_inbox: Color::Rgb(201, 184, 166), // #c9b8a6
            status_next: Color::Rgb(146, 165, 147),  // #92a593
            status_waiting: Color::Rgb(233, 187, 79), // #E9BB4F
            status_scheduled: Color::Rgb(125, 130, 217), // #7d82d9
            status_someday: Color::Rgb(200, 157, 193), // #c89dc1
            status_reference: Color::Rgb(163, 191, 209), // #a3bfd1
            status_done: Color::Rgb(109, 125, 182),  // #6d7db6

            // Priority colors
            priority_high: Color::Rgb(237, 91, 90), // #ED5B5A
            priority_medium: Color::Rgb(233, 187, 79), // #E9BB4F
            priority_low: Color::Rgb(125, 130, 217), // #7d82d9
        }
    }

    pub fn everforest() -> Self {
        Self {
            id: ThemeId::Everforest,
            is_dark: true,
            bg: Color::Rgb(45, 53, 59),              // #2d353b
            fg: Color::Rgb(211, 198, 170),           // #d3c6aa
            text_dim: Color::Rgb(79, 88, 94),        // #4f585e
            text_urgent: Color::Rgb(230, 126, 128),  // #e67e80
            text_warn: Color::Rgb(219, 188, 127),    // #dbbc7f
            text_success: Color::Rgb(167, 192, 128), // #a7c080
            rrule_fg: Color::Rgb(224, 157, 127),     // #e09d7f

            border_active: Color::Rgb(127, 187, 179), // #7fbbb3
            border_inactive: Color::Rgb(61, 72, 77),  // #3d484d

            hl_bg: Color::Rgb(52, 63, 68),         // #343f44
            hl_fg: Color::Rgb(127, 187, 179),      // #7fbbb3
            row_active_bg: Color::Rgb(71, 82, 88), // #475258

            status_bg: Color::Rgb(33, 39, 44),    // #21272c
            status_fg: Color::Rgb(157, 169, 160), // #9da9a0

            accent: Color::Rgb(127, 187, 179), // #7fbbb3

            // Status colors
            status_inbox: Color::Rgb(157, 169, 160), // #9da9a0
            status_next: Color::Rgb(167, 192, 128),  // #a7c080
            status_waiting: Color::Rgb(219, 188, 127), // #dbbc7f
            status_scheduled: Color::Rgb(127, 187, 179), // #7fbbb3
            status_someday: Color::Rgb(214, 153, 182), // #d699b6
            status_reference: Color::Rgb(131, 192, 146), // #83c092
            status_done: Color::Rgb(79, 88, 94),     // #4f585e

            // Priority colors
            priority_high: Color::Rgb(230, 126, 128), // #e67e80
            priority_medium: Color::Rgb(219, 188, 127), // #dbbc7f
            priority_low: Color::Rgb(127, 187, 179),  // #7fbbb3
        }
    }

    pub fn flexoki_light() -> Self {
        Self {
            id: ThemeId::FlexokiLight,
            is_dark: false,
            bg: Color::Rgb(255, 252, 240),          // #FFFCF0
            fg: Color::Rgb(16, 15, 15),             // #100F0F
            text_dim: Color::Rgb(135, 133, 128),    // #878580
            text_urgent: Color::Rgb(209, 77, 65),   // #D14D41
            text_warn: Color::Rgb(208, 162, 21),    // #D0A215
            text_success: Color::Rgb(135, 154, 57), // #879A39
            rrule_fg: Color::Rgb(208, 119, 43),     // #d0772b

            border_active: Color::Rgb(32, 94, 166), // #205EA6
            border_inactive: Color::Rgb(206, 205, 195), // #CECDC3

            hl_bg: Color::Rgb(230, 228, 217),         // #E6E4D9
            hl_fg: Color::Rgb(32, 94, 166),           // #205EA6
            row_active_bg: Color::Rgb(206, 205, 195), // #CECDC3

            status_bg: Color::Rgb(242, 239, 228), // #f2efe4
            status_fg: Color::Rgb(64, 62, 60),    // #403E3C

            accent: Color::Rgb(32, 94, 166), // #205EA6

            // Status colors
            status_inbox: Color::Rgb(64, 62, 60),      // #403E3C
            status_next: Color::Rgb(135, 154, 57),     // #879A39
            status_waiting: Color::Rgb(208, 162, 21),  // #D0A215
            status_scheduled: Color::Rgb(32, 94, 166), // #205EA6
            status_someday: Color::Rgb(206, 93, 151),  // #CE5D97
            status_reference: Color::Rgb(58, 169, 159), // #3AA99F
            status_done: Color::Rgb(135, 133, 128),    // #878580

            // Priority colors
            priority_high: Color::Rgb(209, 77, 65), // #D14D41
            priority_medium: Color::Rgb(208, 162, 21), // #D0A215
            priority_low: Color::Rgb(32, 94, 166),  // #205EA6
        }
    }

    pub fn hackerman() -> Self {
        Self {
            id: ThemeId::Hackerman,
            is_dark: true,
            bg: Color::Rgb(11, 12, 22),             // #0B0C16
            fg: Color::Rgb(221, 247, 255),          // #ddf7ff
            text_dim: Color::Rgb(106, 110, 149),    // #6a6e95
            text_urgent: Color::Rgb(80, 248, 114),  // #50f872
            text_warn: Color::Rgb(80, 247, 212),    // #50f7d4
            text_success: Color::Rgb(79, 232, 143), // #4fe88f
            rrule_fg: Color::Rgb(80, 247, 163),     // #50f7a3

            border_active: Color::Rgb(130, 251, 156), // #82FB9C
            border_inactive: Color::Rgb(31, 37, 58),  // #1f253a

            hl_bg: Color::Rgb(21, 24, 40),         // #151828
            hl_fg: Color::Rgb(130, 251, 156),      // #82FB9C
            row_active_bg: Color::Rgb(45, 52, 80), // #2d3450

            status_bg: Color::Rgb(8, 9, 16),      // #080910
            status_fg: Color::Rgb(181, 197, 219), // #b5c5db

            accent: Color::Rgb(130, 251, 156), // #82FB9C

            // Status colors
            status_inbox: Color::Rgb(181, 197, 219), // #b5c5db
            status_next: Color::Rgb(79, 232, 143),   // #4fe88f
            status_waiting: Color::Rgb(80, 247, 212), // #50f7d4
            status_scheduled: Color::Rgb(130, 251, 156), // #82FB9C
            status_someday: Color::Rgb(134, 167, 223), // #86a7df
            status_reference: Color::Rgb(124, 248, 247), // #7cf8f7
            status_done: Color::Rgb(106, 110, 149),  // #6a6e95

            // Priority colors
            priority_high: Color::Rgb(80, 248, 114), // #50f872
            priority_medium: Color::Rgb(80, 247, 212), // #50f7d4
            priority_low: Color::Rgb(130, 251, 156), // #82FB9C
        }
    }

    pub fn jade_horse() -> Self {
        Self {
            id: ThemeId::JadeHorse,
            is_dark: true,
            bg: Color::Rgb(20, 38, 32),              // #142620
            fg: Color::Rgb(223, 237, 229),           // #dfede5
            text_dim: Color::Rgb(145, 173, 159),     // #91ad9f
            text_urgent: Color::Rgb(216, 141, 145),  // #d88d91
            text_warn: Color::Rgb(197, 204, 145),    // #c5cc91
            text_success: Color::Rgb(139, 203, 177), // #8bcbb1
            rrule_fg: Color::Rgb(210, 171, 141),     // #d2ab8d

            border_active: Color::Rgb(139, 203, 177), // #8bcbb1
            border_inactive: Color::Rgb(43, 81, 71),  // #2b5147

            hl_bg: Color::Rgb(32, 59, 49),            // #203b31
            hl_fg: Color::Rgb(139, 203, 177),         // #8bcbb1
            row_active_bg: Color::Rgb(100, 136, 123), // #64887b

            status_bg: Color::Rgb(15, 30, 25),    // #0f1e19
            status_fg: Color::Rgb(190, 214, 201), // #bed6c9

            accent: Color::Rgb(139, 203, 177), // #8bcbb1

            // Status colors
            status_inbox: Color::Rgb(190, 214, 201), // #bed6c9
            status_next: Color::Rgb(139, 203, 177),  // #8bcbb1
            status_waiting: Color::Rgb(197, 204, 145), // #c5cc91
            status_scheduled: Color::Rgb(150, 183, 208), // #96b7d0
            status_someday: Color::Rgb(183, 161, 200), // #b7a1c8
            status_reference: Color::Rgb(134, 199, 197), // #86c7c5
            status_done: Color::Rgb(145, 173, 159),  // #91ad9f

            // Priority colors
            priority_high: Color::Rgb(216, 141, 145), // #d88d91
            priority_medium: Color::Rgb(197, 204, 145), // #c5cc91
            priority_low: Color::Rgb(150, 183, 208),  // #96b7d0
        }
    }

    pub fn kanagawa() -> Self {
        Self {
            id: ThemeId::Kanagawa,
            is_dark: true,
            bg: Color::Rgb(31, 31, 40),              // #1f1f28
            fg: Color::Rgb(220, 215, 186),           // #dcd7ba
            text_dim: Color::Rgb(114, 113, 105),     // #727169
            text_urgent: Color::Rgb(195, 64, 67),    // #c34043
            text_warn: Color::Rgb(192, 163, 110),    // #c0a36e
            text_success: Color::Rgb(118, 148, 106), // #76946a
            rrule_fg: Color::Rgb(193, 113, 88),      // #c17158

            border_active: Color::Rgb(220, 215, 186), // #dcd7ba
            border_inactive: Color::Rgb(54, 54, 70),  // #363646

            hl_bg: Color::Rgb(34, 50, 73),          // #223249
            hl_fg: Color::Rgb(220, 215, 186),       // #dcd7ba
            row_active_bg: Color::Rgb(84, 84, 109), // #54546D

            status_bg: Color::Rgb(23, 23, 30),    // #17171e
            status_fg: Color::Rgb(200, 192, 147), // #c8c093

            accent: Color::Rgb(220, 215, 186), // #dcd7ba

            // Status colors
            status_inbox: Color::Rgb(200, 192, 147), // #c8c093
            status_next: Color::Rgb(118, 148, 106),  // #76946a
            status_waiting: Color::Rgb(192, 163, 110), // #c0a36e
            status_scheduled: Color::Rgb(126, 156, 216), // #7e9cd8
            status_someday: Color::Rgb(149, 127, 184), // #957fb8
            status_reference: Color::Rgb(106, 149, 137), // #6a9589
            status_done: Color::Rgb(114, 113, 105),  // #727169

            // Priority colors
            priority_high: Color::Rgb(195, 64, 67), // #c34043
            priority_medium: Color::Rgb(192, 163, 110), // #c0a36e
            priority_low: Color::Rgb(126, 156, 216), // #7e9cd8
        }
    }

    pub fn last_horizon() -> Self {
        Self {
            id: ThemeId::LastHorizon,
            is_dark: true,
            bg: Color::Rgb(12, 11, 12),              // #0c0b0c
            fg: Color::Rgb(250, 252, 251),           // #FAFCFB
            text_dim: Color::Rgb(88, 78, 81),        // #584e51
            text_urgent: Color::Rgb(195, 139, 123),  // #c38b7b
            text_warn: Color::Rgb(107, 94, 115),     // #6B5E73
            text_success: Color::Rgb(135, 169, 176), // #87a9b0
            rrule_fg: Color::Rgb(181, 151, 144),     // #b59790

            border_active: Color::Rgb(181, 151, 144), // #b59790
            border_inactive: Color::Rgb(88, 78, 81),  // #584e51

            hl_bg: Color::Rgb(30, 27, 30),         // surface
            hl_fg: Color::Rgb(181, 151, 144),      // #b59790
            row_active_bg: Color::Rgb(88, 78, 81), // #584e51

            status_bg: Color::Rgb(9, 8, 9),       // #090809
            status_fg: Color::Rgb(207, 211, 205), // #cfd3cd

            accent: Color::Rgb(181, 151, 144), // #b59790

            // Status colors
            status_inbox: Color::Rgb(207, 211, 205), // #cfd3cd
            status_next: Color::Rgb(135, 169, 176),  // #87a9b0
            status_waiting: Color::Rgb(107, 94, 115), // #6B5E73
            status_scheduled: Color::Rgb(181, 151, 144), // #b59790
            status_someday: Color::Rgb(196, 216, 226), // #c4d8e2
            status_reference: Color::Rgb(165, 160, 182), // #a5a0b6
            status_done: Color::Rgb(88, 78, 81),     // #584e51

            // Priority colors
            priority_high: Color::Rgb(195, 139, 123), // #c38b7b
            priority_medium: Color::Rgb(107, 94, 115), // #6B5E73
            priority_low: Color::Rgb(181, 151, 144),  // #b59790
        }
    }

    pub fn lumon() -> Self {
        Self {
            id: ThemeId::Lumon,
            is_dark: true,
            bg: Color::Rgb(22, 36, 45),             // #16242d
            fg: Color::Rgb(214, 226, 238),          // #d6e2ee
            text_dim: Color::Rgb(77, 134, 176),     // #4d86b0
            text_urgent: Color::Rgb(77, 134, 176),  // #4d86b0
            text_warn: Color::Rgb(111, 164, 201),   // #6fa4c9
            text_success: Color::Rgb(94, 149, 188), // #5e95bc
            rrule_fg: Color::Rgb(139, 201, 235),    // #8bc9eb

            border_active: Color::Rgb(139, 201, 235), // #8bc9eb
            border_inactive: Color::Rgb(36, 61, 86),  // #243d56

            hl_bg: Color::Rgb(27, 45, 64),         // #1b2d40
            hl_fg: Color::Rgb(139, 201, 235),      // #8bc9eb
            row_active_bg: Color::Rgb(48, 72, 96), // #304860

            status_bg: Color::Rgb(16, 27, 33),    // #101b21
            status_fg: Color::Rgb(214, 226, 238), // #d6e2ee

            accent: Color::Rgb(139, 201, 235), // #8bc9eb

            // Status colors
            status_inbox: Color::Rgb(214, 226, 238), // #d6e2ee
            status_next: Color::Rgb(94, 149, 188),   // #5e95bc
            status_waiting: Color::Rgb(111, 164, 201), // #6fa4c9
            status_scheduled: Color::Rgb(139, 201, 235), // #8bc9eb
            status_someday: Color::Rgb(139, 201, 235), // #8bc9eb
            status_reference: Color::Rgb(180, 228, 246), // #b4e4f6
            status_done: Color::Rgb(77, 134, 176),   // #4d86b0

            // Priority colors
            priority_high: Color::Rgb(77, 134, 176), // #4d86b0
            priority_medium: Color::Rgb(111, 164, 201), // #6fa4c9
            priority_low: Color::Rgb(139, 201, 235), // #8bc9eb
        }
    }

    pub fn lupine() -> Self {
        Self {
            id: ThemeId::Lupine,
            is_dark: false,
            bg: Color::Rgb(250, 250, 250),         // #fafafa
            fg: Color::Rgb(33, 33, 33),            // #212121
            text_dim: Color::Rgb(117, 117, 117),   // #757575
            text_urgent: Color::Rgb(201, 0, 196),  // #c900c4
            text_warn: Color::Rgb(2, 111, 222),    // #026fde
            text_success: Color::Rgb(74, 47, 208), // #4a2fd0
            rrule_fg: Color::Rgb(2, 111, 222),     // #026fde

            border_active: Color::Rgb(50, 100, 235), // #3264eb
            border_inactive: Color::Rgb(208, 208, 208), // #d0d0d0

            hl_bg: Color::Rgb(245, 245, 245),         // #f5f5f5
            hl_fg: Color::Rgb(50, 100, 235),          // #3264eb
            row_active_bg: Color::Rgb(208, 208, 208), // #d0d0d0

            status_bg: Color::Rgb(236, 236, 236), // #ececec
            status_fg: Color::Rgb(66, 66, 66),    // #424242

            accent: Color::Rgb(50, 100, 235), // #3264eb

            // Status colors
            status_inbox: Color::Rgb(66, 66, 66),    // #424242
            status_next: Color::Rgb(74, 47, 208),    // #4a2fd0
            status_waiting: Color::Rgb(2, 111, 222), // #026fde
            status_scheduled: Color::Rgb(50, 100, 235), // #3264eb
            status_someday: Color::Rgb(138, 74, 215), // #8a4ad7
            status_reference: Color::Rgb(12, 103, 222), // #0c67de
            status_done: Color::Rgb(117, 117, 117),  // #757575

            // Priority colors
            priority_high: Color::Rgb(201, 0, 196), // #c900c4
            priority_medium: Color::Rgb(2, 111, 222), // #026fde
            priority_low: Color::Rgb(50, 100, 235), // #3264eb
        }
    }

    pub fn matte_black() -> Self {
        Self {
            id: ThemeId::MatteBlack,
            is_dark: true,
            bg: Color::Rgb(18, 18, 18),            // #121212
            fg: Color::Rgb(190, 190, 190),         // #bebebe
            text_dim: Color::Rgb(85, 85, 85),      // #555555
            text_urgent: Color::Rgb(211, 95, 95),  // #D35F5F
            text_warn: Color::Rgb(185, 28, 28),    // #b91c1c
            text_success: Color::Rgb(255, 193, 7), // #FFC107
            rrule_fg: Color::Rgb(198, 61, 61),     // #c63d3d

            border_active: Color::Rgb(230, 142, 13), // #e68e0d
            border_inactive: Color::Rgb(42, 42, 42), // #2a2a2a

            hl_bg: Color::Rgb(30, 30, 30),         // #1e1e1e
            hl_fg: Color::Rgb(230, 142, 13),       // #e68e0d
            row_active_bg: Color::Rgb(51, 51, 51), // #333333

            status_bg: Color::Rgb(13, 13, 13),    // #0d0d0d
            status_fg: Color::Rgb(138, 138, 141), // #8a8a8d

            accent: Color::Rgb(230, 142, 13), // #e68e0d

            // Status colors
            status_inbox: Color::Rgb(138, 138, 141), // #8a8a8d
            status_next: Color::Rgb(255, 193, 7),    // #FFC107
            status_waiting: Color::Rgb(185, 28, 28), // #b91c1c
            status_scheduled: Color::Rgb(230, 142, 13), // #e68e0d
            status_someday: Color::Rgb(211, 95, 95), // #D35F5F
            status_reference: Color::Rgb(190, 190, 190), // #bebebe
            status_done: Color::Rgb(85, 85, 85),     // #555555

            // Priority colors
            priority_high: Color::Rgb(211, 95, 95), // #D35F5F
            priority_medium: Color::Rgb(185, 28, 28), // #b91c1c
            priority_low: Color::Rgb(230, 142, 13), // #e68e0d
        }
    }

    pub fn miasma() -> Self {
        Self {
            id: ThemeId::Miasma,
            is_dark: true,
            bg: Color::Rgb(34, 34, 34),            // #222222
            fg: Color::Rgb(194, 194, 176),         // #c2c2b0
            text_dim: Color::Rgb(102, 102, 102),   // #666666
            text_urgent: Color::Rgb(104, 87, 66),  // #685742
            text_warn: Color::Rgb(179, 109, 67),   // #b36d43
            text_success: Color::Rgb(95, 135, 95), // #5f875f
            rrule_fg: Color::Rgb(141, 98, 66),     // #8d6242

            border_active: Color::Rgb(120, 130, 75), // #78824b
            border_inactive: Color::Rgb(56, 56, 56), // #383838

            hl_bg: Color::Rgb(44, 44, 44),            // #2c2c2c
            hl_fg: Color::Rgb(120, 130, 75),          // #78824b
            row_active_bg: Color::Rgb(102, 102, 102), // #666666

            status_bg: Color::Rgb(25, 25, 25),    // #191919
            status_fg: Color::Rgb(138, 138, 126), // #8a8a8d

            accent: Color::Rgb(120, 130, 75), // #78824b

            // Status colors
            status_inbox: Color::Rgb(138, 138, 126), // #8a8a8d
            status_next: Color::Rgb(95, 135, 95),    // #5f875f
            status_waiting: Color::Rgb(179, 109, 67), // #b36d43
            status_scheduled: Color::Rgb(120, 130, 75), // #78824b
            status_someday: Color::Rgb(187, 119, 68), // #bb7744
            status_reference: Color::Rgb(201, 165, 84), // #c9a554
            status_done: Color::Rgb(102, 102, 102),  // #666666

            // Priority colors
            priority_high: Color::Rgb(104, 87, 66), // #685742
            priority_medium: Color::Rgb(179, 109, 67), // #b36d43
            priority_low: Color::Rgb(120, 130, 75), // #78824b
        }
    }

    pub fn nord() -> Self {
        Self {
            id: ThemeId::Nord,
            is_dark: true,
            bg: Color::Rgb(46, 52, 64),              // #2e3440
            fg: Color::Rgb(216, 222, 233),           // #d8dee9
            text_dim: Color::Rgb(102, 112, 128),     // #667080
            text_urgent: Color::Rgb(191, 97, 106),   // #bf616a
            text_warn: Color::Rgb(235, 203, 139),    // #ebcb8b
            text_success: Color::Rgb(163, 190, 140), // #a3be8c
            rrule_fg: Color::Rgb(213, 150, 122),     // #d5967a

            border_active: Color::Rgb(129, 161, 193), // #81a1c1
            border_inactive: Color::Rgb(67, 76, 94),  // #434c5e

            hl_bg: Color::Rgb(59, 66, 82),          // #3b4252
            hl_fg: Color::Rgb(129, 161, 193),       // #81a1c1
            row_active_bg: Color::Rgb(76, 86, 106), // #4c566a

            status_bg: Color::Rgb(34, 39, 48),    // #222730
            status_fg: Color::Rgb(173, 181, 196), // #adb5c4

            accent: Color::Rgb(129, 161, 193), // #81a1c1

            // Status colors
            status_inbox: Color::Rgb(173, 181, 196), // #adb5c4
            status_next: Color::Rgb(163, 190, 140),  // #a3be8c
            status_waiting: Color::Rgb(235, 203, 139), // #ebcb8b
            status_scheduled: Color::Rgb(129, 161, 193), // #81a1c1
            status_someday: Color::Rgb(180, 142, 173), // #b48ead
            status_reference: Color::Rgb(136, 192, 208), // #88c0d0
            status_done: Color::Rgb(102, 112, 128),  // #667080

            // Priority colors
            priority_high: Color::Rgb(191, 97, 106), // #bf616a
            priority_medium: Color::Rgb(235, 203, 139), // #ebcb8b
            priority_low: Color::Rgb(129, 161, 193), // #81a1c1
        }
    }

    pub fn osaka_jade() -> Self {
        Self {
            id: ThemeId::OsakaJade,
            is_dark: true,
            bg: Color::Rgb(17, 28, 24),             // #111c18
            fg: Color::Rgb(193, 196, 151),          // #C1C497
            text_dim: Color::Rgb(129, 184, 168),    // #81B8A8
            text_urgent: Color::Rgb(255, 83, 69),   // #FF5345
            text_warn: Color::Rgb(69, 148, 81),     // #459451
            text_success: Color::Rgb(84, 158, 106), // #549e6a
            rrule_fg: Color::Rgb(162, 115, 75),     // #a2734b

            border_active: Color::Rgb(80, 148, 117), // #509475
            border_inactive: Color::Rgb(50, 71, 59), // #32473B

            hl_bg: Color::Rgb(35, 55, 43),          // #23372B
            hl_fg: Color::Rgb(80, 148, 117),        // #509475
            row_active_bg: Color::Rgb(83, 104, 91), // #53685B

            status_bg: Color::Rgb(12, 21, 18),    // #0c1512
            status_fg: Color::Rgb(214, 213, 188), // #D6D5BC

            accent: Color::Rgb(80, 148, 117), // #509475

            // Status colors
            status_inbox: Color::Rgb(214, 213, 188), // #D6D5BC
            status_next: Color::Rgb(84, 158, 106),   // #549e6a
            status_waiting: Color::Rgb(69, 148, 81), // #459451
            status_scheduled: Color::Rgb(80, 148, 117), // #509475
            status_someday: Color::Rgb(210, 104, 156), // #D2689C
            status_reference: Color::Rgb(45, 213, 183), // #2DD5B7
            status_done: Color::Rgb(129, 184, 168),  // #81B8A8

            // Priority colors
            priority_high: Color::Rgb(255, 83, 69), // #FF5345
            priority_medium: Color::Rgb(69, 148, 81), // #459451
            priority_low: Color::Rgb(80, 148, 117), // #509475
        }
    }

    pub fn ristretto() -> Self {
        Self {
            id: ThemeId::Ristretto,
            is_dark: true,
            bg: Color::Rgb(44, 37, 37),              // #2c2525
            fg: Color::Rgb(230, 217, 219),           // #e6d9db
            text_dim: Color::Rgb(114, 105, 106),     // #72696a
            text_urgent: Color::Rgb(253, 104, 131),  // #fd6883
            text_warn: Color::Rgb(249, 204, 108),    // #f9cc6c
            text_success: Color::Rgb(173, 218, 120), // #adda78
            rrule_fg: Color::Rgb(251, 154, 119),     // #fb9a77

            border_active: Color::Rgb(243, 141, 112), // #f38d70
            border_inactive: Color::Rgb(64, 62, 65),  // #403e41

            hl_bg: Color::Rgb(61, 47, 42),            // #3d2f2a
            hl_fg: Color::Rgb(243, 141, 112),         // #f38d70
            row_active_bg: Color::Rgb(114, 105, 106), // #72696a

            status_bg: Color::Rgb(33, 27, 27),    // #211b1b
            status_fg: Color::Rgb(195, 183, 184), // #c3b7b8

            accent: Color::Rgb(243, 141, 112), // #f38d70

            // Status colors
            status_inbox: Color::Rgb(195, 183, 184), // #c3b7b8
            status_next: Color::Rgb(173, 218, 120),  // #adda78
            status_waiting: Color::Rgb(249, 204, 108), // #f9cc6c
            status_scheduled: Color::Rgb(243, 141, 112), // #f38d70
            status_someday: Color::Rgb(168, 169, 235), // #a8a9eb
            status_reference: Color::Rgb(133, 218, 204), // #85dacc
            status_done: Color::Rgb(114, 105, 106),  // #72696a

            // Priority colors
            priority_high: Color::Rgb(253, 104, 131), // #fd6883
            priority_medium: Color::Rgb(249, 204, 108), // #f9cc6c
            priority_low: Color::Rgb(243, 141, 112),  // #f38d70
        }
    }

    pub fn rose_pine() -> Self {
        Self {
            id: ThemeId::RosePine,
            is_dark: false,
            bg: Color::Rgb(250, 244, 237),          // #faf4ed
            fg: Color::Rgb(87, 82, 121),            // #575279
            text_dim: Color::Rgb(152, 147, 165),    // #9893a5
            text_urgent: Color::Rgb(180, 99, 122),  // #b4637a
            text_warn: Color::Rgb(234, 157, 52),    // #ea9d34
            text_success: Color::Rgb(40, 105, 131), // #286983
            rrule_fg: Color::Rgb(207, 128, 87),     // #cf8057

            border_active: Color::Rgb(86, 148, 159), // #56949f
            border_inactive: Color::Rgb(223, 218, 217), // #dfdad9

            hl_bg: Color::Rgb(242, 233, 225),         // #f2e9e1
            hl_fg: Color::Rgb(86, 148, 159),          // #56949f
            row_active_bg: Color::Rgb(223, 218, 217), // #dfdad9

            status_bg: Color::Rgb(237, 231, 225), // #ede7e1
            status_fg: Color::Rgb(110, 106, 134), // #6e6a86

            accent: Color::Rgb(86, 148, 159), // #56949f

            // Status colors
            status_inbox: Color::Rgb(110, 106, 134), // #6e6a86
            status_next: Color::Rgb(40, 105, 131),   // #286983
            status_waiting: Color::Rgb(234, 157, 52), // #ea9d34
            status_scheduled: Color::Rgb(86, 148, 159), // #56949f
            status_someday: Color::Rgb(144, 122, 169), // #907aa9
            status_reference: Color::Rgb(215, 130, 126), // #d7827e
            status_done: Color::Rgb(152, 147, 165),  // #9893a5

            // Priority colors
            priority_high: Color::Rgb(180, 99, 122), // #b4637a
            priority_medium: Color::Rgb(234, 157, 52), // #ea9d34
            priority_low: Color::Rgb(86, 148, 159),  // #56949f
        }
    }

    pub fn solitude() -> Self {
        Self {
            id: ThemeId::Solitude,
            is_dark: true,
            bg: Color::Rgb(16, 19, 21),              // #101315
            fg: Color::Rgb(202, 204, 204),           // #cacccc
            text_dim: Color::Rgb(75, 78, 85),        // #4b4e55
            text_urgent: Color::Rgb(222, 97, 69),    // bright_red #de6145
            text_warn: Color::Rgb(217, 219, 220),    // #d9dbdc
            text_success: Color::Rgb(159, 165, 169), // #9fa5a9
            rrule_fg: Color::Rgb(222, 97, 69),       // bright_red #de6145

            border_active: Color::Rgb(121, 129, 134), // #798186
            border_inactive: Color::Rgb(52, 61, 65),  // #343d41

            hl_bg: Color::Rgb(28, 33, 36),         // surface
            hl_fg: Color::Rgb(121, 129, 134),      // #798186
            row_active_bg: Color::Rgb(75, 78, 85), // #4b4e55

            status_bg: Color::Rgb(12, 14, 16),    // #0c0e10
            status_fg: Color::Rgb(203, 194, 190), // #cbc2be

            accent: Color::Rgb(121, 129, 134), // #798186

            // Status colors
            status_inbox: Color::Rgb(203, 194, 190), // #cbc2be
            status_next: Color::Rgb(159, 165, 169),  // #9fa5a9
            status_waiting: Color::Rgb(217, 219, 220), // #d9dbdc
            status_scheduled: Color::Rgb(121, 129, 134), // #798186
            status_someday: Color::Rgb(174, 174, 174), // #aeaeae
            status_reference: Color::Rgb(112, 112, 112), // #707070
            status_done: Color::Rgb(75, 78, 85),     // #4b4e55

            // Priority colors
            priority_high: Color::Rgb(222, 97, 69), // #de6145
            priority_medium: Color::Rgb(217, 219, 220), // #d9dbdc
            priority_low: Color::Rgb(121, 129, 134), // #798186
        }
    }

    pub fn starship() -> Self {
        Self {
            id: ThemeId::Starship,
            is_dark: true,
            bg: Color::Rgb(28, 28, 30),             // #1c1c1e
            fg: Color::Rgb(245, 245, 247),          // #f5f5f7
            text_dim: Color::Rgb(152, 152, 157),    // #98989d
            text_urgent: Color::Rgb(255, 95, 74),   // #ff5f4a
            text_warn: Color::Rgb(255, 194, 122),   // #ffc27a
            text_success: Color::Rgb(255, 159, 10), // #ff9f0a
            rrule_fg: Color::Rgb(255, 106, 61),     // #ff6a3d

            border_active: Color::Rgb(255, 159, 10), // #ff9f0a
            border_inactive: Color::Rgb(58, 58, 60), // #3a3a3c

            hl_bg: Color::Rgb(44, 44, 46),            // #2c2c2e
            hl_fg: Color::Rgb(255, 159, 10),          // #ff9f0a
            row_active_bg: Color::Rgb(142, 142, 147), // #8e8e93

            status_bg: Color::Rgb(22, 22, 24),    // #161618
            status_fg: Color::Rgb(251, 251, 253), // #fbfbfd

            accent: Color::Rgb(255, 159, 10), // #ff9f0a

            // Status colors
            status_inbox: Color::Rgb(251, 251, 253), // #fbfbfd
            status_next: Color::Rgb(255, 159, 10),   // #ff9f0a
            status_waiting: Color::Rgb(255, 194, 122), // #ffc27a
            status_scheduled: Color::Rgb(255, 159, 10), // #ff9f0a
            status_someday: Color::Rgb(255, 143, 163), // #ff8fa3
            status_reference: Color::Rgb(166, 220, 239), // #a6dcef
            status_done: Color::Rgb(152, 152, 157),  // #98989d

            // Priority colors
            priority_high: Color::Rgb(255, 95, 74), // #ff5f4a
            priority_medium: Color::Rgb(255, 194, 122), // #ffc27a
            priority_low: Color::Rgb(255, 159, 10), // #ff9f0a
        }
    }

    pub fn vantablack() -> Self {
        Self {
            id: ThemeId::Vantablack,
            is_dark: true,
            bg: Color::Rgb(0, 0, 0),                 // #000000
            fg: Color::Rgb(255, 255, 255),           // #ffffff
            text_dim: Color::Rgb(80, 80, 80),        // #505050
            text_urgent: Color::Rgb(164, 164, 164),  // #a4a4a4
            text_warn: Color::Rgb(206, 206, 206),    // #cecece
            text_success: Color::Rgb(182, 182, 182), // #b6b6b6
            rrule_fg: Color::Rgb(185, 185, 185),     // #b9b9b9

            border_active: Color::Rgb(141, 141, 141), // #8d8d8d
            border_inactive: Color::Rgb(26, 26, 26),  // #1a1a1a

            hl_bg: Color::Rgb(26, 26, 26),            // #1a1a1a
            hl_fg: Color::Rgb(141, 141, 141),         // #8d8d8d
            row_active_bg: Color::Rgb(122, 122, 122), // #7a7a7a

            status_bg: Color::Rgb(9, 9, 9),       // #090909
            status_fg: Color::Rgb(236, 236, 236), // #ececec

            accent: Color::Rgb(141, 141, 141), // #8d8d8d

            // Status colors
            status_inbox: Color::Rgb(236, 236, 236), // #ececec
            status_next: Color::Rgb(182, 182, 182),  // #b6b6b6
            status_waiting: Color::Rgb(206, 206, 206), // #cecece
            status_scheduled: Color::Rgb(141, 141, 141), // #8d8d8d
            status_someday: Color::Rgb(155, 155, 155), // #9b9b9b
            status_reference: Color::Rgb(176, 176, 176), // #b0b0b0
            status_done: Color::Rgb(80, 80, 80),     // #505050

            // Priority colors
            priority_high: Color::Rgb(164, 164, 164), // #a4a4a4
            priority_medium: Color::Rgb(206, 206, 206), // #cecece
            priority_low: Color::Rgb(141, 141, 141),  // #8d8d8d
        }
    }

    pub fn white() -> Self {
        Self {
            id: ThemeId::White,
            is_dark: false,
            bg: Color::Rgb(255, 255, 255),        // #ffffff
            fg: Color::Rgb(0, 0, 0),              // #000000
            text_dim: Color::Rgb(192, 192, 192),  // #c0c0c0
            text_urgent: Color::Rgb(42, 42, 42),  // #2a2a2a
            text_warn: Color::Rgb(74, 74, 74),    // #4a4a4a
            text_success: Color::Rgb(58, 58, 58), // #3a3a3a
            rrule_fg: Color::Rgb(110, 110, 110),  // #6e6e6e

            border_active: Color::Rgb(110, 110, 110), // #6e6e6e
            border_inactive: Color::Rgb(192, 192, 192), // #c0c0c0

            hl_bg: Color::Rgb(230, 230, 230),         // #e6e6e6
            hl_fg: Color::Rgb(110, 110, 110),         // #6e6e6e
            row_active_bg: Color::Rgb(192, 192, 192), // #c0c0c0

            status_bg: Color::Rgb(245, 245, 245), // #f5f5f5
            status_fg: Color::Rgb(0, 0, 0),       // #000000

            accent: Color::Rgb(110, 110, 110), // #6e6e6e

            // Status colors
            status_inbox: Color::Rgb(0, 0, 0),           // #000000
            status_next: Color::Rgb(58, 58, 58),         // #3a3a3a
            status_waiting: Color::Rgb(74, 74, 74),      // #4a4a4a
            status_scheduled: Color::Rgb(110, 110, 110), // #6e6e6e
            status_someday: Color::Rgb(46, 46, 46),      // #2e2e2e
            status_reference: Color::Rgb(62, 62, 62),    // #3e3e3e
            status_done: Color::Rgb(192, 192, 192),      // #c0c0c0

            // Priority colors
            priority_high: Color::Rgb(42, 42, 42),   // #2a2a2a
            priority_medium: Color::Rgb(74, 74, 74), // #4a4a4a
            priority_low: Color::Rgb(110, 110, 110), // #6e6e6e
        }
    }

    pub fn from_id(id: ThemeId) -> Self {
        match id {
            ThemeId::CatppuccinMocha => Self::catppuccin_mocha(),
            ThemeId::CatppuccinLatte => Self::catppuccin_latte(),
            ThemeId::TokyoNight => Self::tokyo_night(),
            ThemeId::Gruvbox => Self::gruvbox(),
            ThemeId::Retro82 => Self::retro_82(),
            ThemeId::Ethereal => Self::ethereal(),
            ThemeId::Everforest => Self::everforest(),
            ThemeId::FlexokiLight => Self::flexoki_light(),
            ThemeId::Hackerman => Self::hackerman(),
            ThemeId::JadeHorse => Self::jade_horse(),
            ThemeId::Kanagawa => Self::kanagawa(),
            ThemeId::LastHorizon => Self::last_horizon(),
            ThemeId::Lumon => Self::lumon(),
            ThemeId::Lupine => Self::lupine(),
            ThemeId::MatteBlack => Self::matte_black(),
            ThemeId::Miasma => Self::miasma(),
            ThemeId::Nord => Self::nord(),
            ThemeId::OsakaJade => Self::osaka_jade(),
            ThemeId::Ristretto => Self::ristretto(),
            ThemeId::RosePine => Self::rose_pine(),
            ThemeId::Solitude => Self::solitude(),
            ThemeId::Starship => Self::starship(),
            ThemeId::Vantablack => Self::vantablack(),
            ThemeId::White => Self::white(),
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().replace(['_', ' '], "-").as_str() {
            "catppuccin" | "catppuccin-mocha" | "mocha" => Some(Self::catppuccin_mocha()),
            "catppuccin-latte" | "latte" => Some(Self::catppuccin_latte()),
            "tokyo-night" | "tokyonight" => Some(Self::tokyo_night()),
            "gruvbox" => Some(Self::gruvbox()),
            "retro-82" | "retro82" => Some(Self::retro_82()),
            "ethereal" => Some(Self::ethereal()),
            "everforest" => Some(Self::everforest()),
            "flexoki-light" | "flexoki" | "flexokilight" => Some(Self::flexoki_light()),
            "hackerman" => Some(Self::hackerman()),
            "jade-horse" | "jadehorse" => Some(Self::jade_horse()),
            "kanagawa" => Some(Self::kanagawa()),
            "last-horizon" | "lasthorizon" => Some(Self::last_horizon()),
            "lumon" => Some(Self::lumon()),
            "lupine" => Some(Self::lupine()),
            "matte-black" | "matteblack" => Some(Self::matte_black()),
            "miasma" => Some(Self::miasma()),
            "nord" => Some(Self::nord()),
            "osaka-jade" | "osakajade" => Some(Self::osaka_jade()),
            "ristretto" => Some(Self::ristretto()),
            "rose-pine" | "rosepine" => Some(Self::rose_pine()),
            "solitude" => Some(Self::solitude()),
            "starship" => Some(Self::starship()),
            "vantablack" => Some(Self::vantablack()),
            "white" => Some(Self::white()),
            _ => None,
        }
    }

    pub fn status_color(&self, s: &Status) -> Color {
        match s {
            Status::Inbox => self.status_inbox,
            Status::Next => self.status_next,
            Status::Waiting => self.status_waiting,
            Status::Scheduled => self.status_scheduled,
            Status::Someday => self.status_someday,
            Status::Reference => self.status_reference,
            Status::Done => self.status_done,
        }
    }

    pub fn priority_color(&self, p: &str) -> Option<Color> {
        match p {
            "high" | "h" | "1" | "高" => Some(self.priority_high),
            "medium" | "m" | "2" | "中" => Some(self.priority_medium),
            "low" | "l" | "3" | "低" => Some(self.priority_low),
            _ => None,
        }
    }

    pub fn next(&self) -> Self {
        Self::from_id(self.id.next())
    }

    pub fn toggle(&self) -> Self {
        self.next()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}

/// 解析 6 位十六进制 RGB 颜色 (如 "#7aa2f7")。
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let clean = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    if clean.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
    let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
    let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// 检查当前环境是否为 Omarchy 桌面系统。
pub fn is_omarchy_system() -> bool {
    if horae_core::repo::state::is_test_override() {
        return false;
    }
    let Some(home) = horae_core::config::home_dir() else {
        return false;
    };
    home.join(".local/state/omarchy").exists() || home.join(".config/omarchy").exists()
}

/// 从 Omarchy 当前激活的主题探测并返回对应主题配置。
pub fn detect_omarchy_theme() -> Option<Theme> {
    if horae_core::repo::state::is_test_override() {
        return None;
    }
    let home = horae_core::config::home_dir()?;

    // 1. 读取 theme.name 文本文件
    let name_paths = [
        home.join(".local/state/omarchy/current/theme.name"),
        home.join(".config/omarchy/current/theme.name"),
    ];
    for p in &name_paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            let name = content.trim();
            if let Some(t) = Theme::from_name(name) {
                return Some(t);
            }
        }
    }

    // 2. 检查软链接指向的主题目录名称
    let symlink_paths = [
        home.join(".local/state/omarchy/current/theme"),
        home.join(".config/omarchy/current/theme"),
    ];
    for p in &symlink_paths {
        if let Ok(target) = std::fs::read_link(p) {
            if let Some(name) = target.file_name().and_then(|n| n.to_str()) {
                if let Some(t) = Theme::from_name(name) {
                    return Some(t);
                }
            }
        }
    }

    None
}

/// 循环切换主题（支持 Omarchy 自动跟随逻辑）。
/// 返回 (新主题, 是否为 omarchy_auto, 保存到 settings 的 key)
pub fn cycle_theme(current: &Theme, omarchy_auto: bool) -> (Theme, bool, &'static str) {
    let is_omarchy = is_omarchy_system();
    if is_omarchy && omarchy_auto {
        let next = Theme::catppuccin_latte();
        let key = next.id.key();
        (next, false, key)
    } else if is_omarchy && current.id.is_last() {
        let detected = detect_omarchy_theme().unwrap_or_else(Theme::catppuccin_mocha);
        (detected, true, "auto")
    } else {
        let next = current.next();
        let key = next.id.key();
        (next, false, key)
    }
}
