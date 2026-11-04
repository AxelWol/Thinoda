use crate::{PresentationSource, Sheet, SheetRootMode};

impl Sheet {
    /// Whether this sheet uses content that native-v6-only export formats preserve.
    pub fn uses_board_semantics(&self) -> bool {
        self.root_mode == SheetRootMode::Hidden
            || !self.canvas.is_empty()
            || !self.board.is_default()
            || self.topics.values().any(|topic| topic.card.is_some())
            || self.zones.iter().any(|zone| {
                zone.color.is_some()
                    || zone.background_color.is_some()
                    || zone.clip
                    || zone.keep_empty
            })
            || self
                .presentation
                .slides
                .iter()
                .any(|slide| matches!(slide.source, PresentationSource::Viewport))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Rect, Vec2};

    #[test]
    fn plain_sheet_does_not_use_board_semantics() {
        let sheet = Sheet::new("Map", "Root");
        assert!(!sheet.uses_board_semantics());
    }

    #[test]
    fn hidden_root_and_nondefault_board_settings_use_board_semantics() {
        let mut hidden = Sheet::new("Board", "");
        hidden.root_mode = SheetRootMode::Hidden;
        assert!(hidden.uses_board_semantics());

        let mut gridded = Sheet::new("Map", "Root");
        gridded.board.grid = crate::GridKind::Dots;
        assert!(gridded.uses_board_semantics());
    }

    #[test]
    fn independent_zone_background_is_board_semantics() {
        let mut sheet = Sheet::new("Map", "Root");
        sheet
            .create_board_frame(
                "Frame",
                Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 100.0)),
            )
            .unwrap();
        sheet.zones[0].color = None;
        sheet.zones[0].background_color = Some(Color::rgb(10, 20, 30));

        assert!(sheet.uses_board_semantics());
    }
}
