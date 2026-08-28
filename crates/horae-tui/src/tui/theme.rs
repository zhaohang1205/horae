use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Theme {
    pub is_dark: bool,
    pub bg: Color,
    pub fg: Color,
    pub text_dim: Color,
    pub text_urgent: Color,
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
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            is_dark: true,
            bg: Color::Rgb(30, 30, 46),              // Base
            fg: Color::Rgb(205, 214, 244),           // Text
            text_dim: Color::Rgb(108, 112, 134),     // Overlay0
            text_urgent: Color::Rgb(243, 139, 168),  // Red
            text_success: Color::Rgb(166, 227, 161), // Green
            rrule_fg: Color::Rgb(250, 179, 135),     // Peach

            border_active: Color::Rgb(137, 180, 250), // Blue
            border_inactive: Color::Rgb(69, 71, 90),  // Surface1

            hl_bg: Color::Rgb(49, 50, 68),          // Surface0
            hl_fg: Color::Rgb(137, 180, 250),       // Blue
            row_active_bg: Color::Rgb(88, 91, 112), // Surface2，活动行（比 hl_bg 高一档，更醒目）

            status_bg: Color::Rgb(24, 24, 37),    // Mantle
            status_fg: Color::Rgb(186, 194, 222), // Subtext1

            accent: Color::Rgb(137, 180, 250), // Blue
        }
    }

    pub fn catppuccin_latte() -> Self {
        Self {
            is_dark: false,
            bg: Color::Rgb(239, 241, 245),         // Base
            fg: Color::Rgb(76, 79, 105),           // Text
            text_dim: Color::Rgb(156, 160, 176),   // Overlay0
            text_urgent: Color::Rgb(210, 15, 57),  // Red
            text_success: Color::Rgb(64, 160, 43), // Green
            rrule_fg: Color::Rgb(254, 100, 11),    // Peach

            border_active: Color::Rgb(30, 102, 245), // Blue
            border_inactive: Color::Rgb(188, 192, 204), // Surface1

            hl_bg: Color::Rgb(204, 208, 218),         // Surface0
            hl_fg: Color::Rgb(30, 102, 245),          // Blue
            row_active_bg: Color::Rgb(172, 176, 190), // Surface2，活动行（比 hl_bg 高一档）

            status_bg: Color::Rgb(230, 233, 239), // Mantle
            status_fg: Color::Rgb(92, 95, 119),   // Subtext1

            accent: Color::Rgb(30, 102, 245), // Blue
        }
    }

    pub fn toggle(&self) -> Self {
        if self.is_dark {
            Self::catppuccin_latte()
        } else {
            Self::catppuccin_mocha()
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}
