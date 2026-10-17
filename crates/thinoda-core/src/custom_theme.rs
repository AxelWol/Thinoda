//! Embedded custom themes remain usable without the author's local profile.

use crate::{
    Color, TopicStyle,
    quick_styles::validate_named_style,
    theme::{Level, Theme, ThemeSettings},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RoleStyles {
    pub central: TopicStyle,
    pub main: TopicStyle,
    pub sub: TopicStyle,
    pub floating: TopicStyle,
    pub summary: TopicStyle,
}

impl RoleStyles {
    pub fn get(&self, level: Level) -> &TopicStyle {
        match level {
            Level::Central => &self.central,
            Level::Main => &self.main,
            Level::Sub => &self.sub,
            Level::Floating => &self.floating,
            Level::Summary => &self.summary,
        }
    }

    pub fn get_mut(&mut self, level: Level) -> &mut TopicStyle {
        match level {
            Level::Central => &mut self.central,
            Level::Main => &mut self.main,
            Level::Sub => &mut self.sub,
            Level::Floating => &mut self.floating,
            Level::Summary => &mut self.summary,
        }
    }

    pub fn fonts(&self) -> impl Iterator<Item = &String> {
        [
            &self.central,
            &self.main,
            &self.sub,
            &self.floating,
            &self.summary,
        ]
        .into_iter()
        .filter_map(|style| style.font_family.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTheme {
    pub name: String,
    pub base_theme_id: String,
    pub palette: Vec<Color>,
    pub light_background: Color,
    pub dark_background: Color,
    pub font_family: Option<String>,
    pub roles: RoleStyles,
}

impl CustomTheme {
    pub fn from_settings(settings: &ThemeSettings) -> Self {
        if let Some(custom) = &settings.custom_theme {
            return custom.clone();
        }
        let theme = Theme::builtin(&settings.theme_id);
        Self {
            name: "My Theme".into(),
            base_theme_id: theme.id.into(),
            palette: theme.palette.clone(),
            light_background: theme.background(settings, false),
            dark_background: theme.background(settings, true),
            font_family: settings.font_family.clone(),
            roles: RoleStyles::default(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_named_style(&self.name, &TopicStyle::default(), "custom theme")?;
        if !Theme::IDS.contains(&self.base_theme_id.as_str()) {
            return Err("custom theme base must be a built-in theme".into());
        }
        if self.palette.is_empty() || self.palette.len() > 16 {
            return Err("custom theme palette must contain 1-16 colors".into());
        }
        for role in [
            Level::Central,
            Level::Main,
            Level::Sub,
            Level::Floating,
            Level::Summary,
        ] {
            validate_named_style(&self.name, self.roles.get(role), "custom theme")?;
        }
        Ok(())
    }
}
