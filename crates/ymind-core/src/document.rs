//! [`Document`]: a workbook plus snapshot based undo/redo and dirty tracking.

use crate::edit::{EditError, EditResult};
use crate::model::{Sheet, Workbook};

const MAX_HISTORY: usize = 300;

#[derive(Debug, Clone)]
enum Snapshot {
    Sheet { index: usize, sheet: Box<Sheet> },
    Book { sheets: Vec<Sheet>, active: usize },
}

#[derive(Debug, Clone)]
struct Entry {
    label: String,
    snapshot: Snapshot,
    state: u64,
}

/// Editable document with undo history.
#[derive(Debug, Clone)]
pub struct Document {
    workbook: Workbook,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    state: u64,
    next_state: u64,
    saved_state: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new(Workbook::default())
    }
}

impl Document {
    pub fn new(workbook: Workbook) -> Self {
        Self {
            workbook,
            undo: Vec::new(),
            redo: Vec::new(),
            state: 0,
            next_state: 1,
            saved_state: 0,
        }
    }

    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    pub fn sheet(&self) -> &Sheet {
        self.workbook.active()
    }

    pub fn active_index(&self) -> usize {
        self.workbook
            .active_sheet
            .min(self.workbook.sheets.len() - 1)
    }

    /// Switching sheets is a view change and therefore not undoable.
    pub fn set_active(&mut self, index: usize) {
        if index < self.workbook.sheets.len() {
            self.workbook.active_sheet = index;
        }
    }

    /// Opaque id of the current content state (changes on every edit/undo/redo).
    pub fn state_id(&self) -> u64 {
        self.state
    }

    pub fn is_dirty(&self) -> bool {
        self.state != self.saved_state
    }

    pub fn mark_saved(&mut self) {
        self.saved_state = self.state;
    }

    /// Marks the document as modified without an undo entry (e.g. recovered documents).
    pub fn mark_dirty(&mut self) {
        self.state = self.bump();
    }

    fn bump(&mut self) -> u64 {
        let s = self.next_state;
        self.next_state += 1;
        s
    }

    fn push(&mut self, label: &str, snapshot: Snapshot) {
        self.undo.push(Entry {
            label: label.to_string(),
            snapshot,
            state: self.state,
        });
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.state = self.bump();
    }

    /// Edits the active sheet. If `f` fails, the sheet is restored and no history is recorded.
    pub fn edit_sheet<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Sheet) -> EditResult<T>,
    ) -> EditResult<T> {
        let index = self.active_index();
        let before = self.workbook.sheets[index].clone();
        match f(&mut self.workbook.sheets[index]) {
            Ok(v) => {
                if self.workbook.sheets[index] != before {
                    self.push(
                        label,
                        Snapshot::Sheet {
                            index,
                            sheet: Box::new(before),
                        },
                    );
                }
                Ok(v)
            }
            Err(e) => {
                self.workbook.sheets[index] = before;
                Err(e)
            }
        }
    }

    /// Edits the whole workbook (sheet add/remove/reorder, metadata).
    pub fn edit_book<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Workbook) -> EditResult<T>,
    ) -> EditResult<T> {
        let sheets = self.workbook.sheets.clone();
        let active = self.workbook.active_sheet;
        match f(&mut self.workbook) {
            Ok(v) => {
                if self.workbook.sheets.is_empty() {
                    self.workbook.sheets = sheets;
                    self.workbook.active_sheet = active;
                    return Err(EditError::Invalid(
                        "a workbook needs at least one sheet".into(),
                    ));
                }
                self.workbook.active_sheet = self
                    .workbook
                    .active_sheet
                    .min(self.workbook.sheets.len() - 1);
                if self.workbook.sheets != sheets {
                    self.push(label, Snapshot::Book { sheets, active });
                }
                Ok(v)
            }
            Err(e) => {
                self.workbook.sheets = sheets;
                self.workbook.active_sheet = active;
                Err(e)
            }
        }
    }

    fn swap(&mut self, snapshot: Snapshot) -> Snapshot {
        match snapshot {
            Snapshot::Sheet { index, sheet } => {
                let index = index.min(self.workbook.sheets.len() - 1);
                let cur = std::mem::replace(&mut self.workbook.sheets[index], *sheet);
                self.workbook.active_sheet = index;
                Snapshot::Sheet {
                    index,
                    sheet: Box::new(cur),
                }
            }
            Snapshot::Book { sheets, active } => {
                let cur = std::mem::replace(&mut self.workbook.sheets, sheets);
                let cur_active = self.workbook.active_sheet;
                self.workbook.active_sheet = active.min(self.workbook.sheets.len() - 1);
                Snapshot::Book {
                    sheets: cur,
                    active: cur_active,
                }
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|e| e.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }

    pub fn undo(&mut self) -> bool {
        let Some(e) = self.undo.pop() else {
            return false;
        };
        let snap = self.swap(e.snapshot);
        self.redo.push(Entry {
            label: e.label,
            snapshot: snap,
            state: self.state,
        });
        self.state = e.state;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(e) = self.redo.pop() else {
            return false;
        };
        let snap = self.swap(e.snapshot);
        self.undo.push(Entry {
            label: e.label,
            snapshot: snap,
            state: self.state,
        });
        self.state = e.state;
        true
    }

    /// Replaces the whole workbook (e.g. after reload) and clears history.
    pub fn replace(&mut self, workbook: Workbook) {
        *self = Document::new(workbook);
    }

    pub fn add_sheet(&mut self, sheet: Sheet) -> usize {
        self.edit_book("Add sheet", |wb| {
            wb.sheets.push(sheet);
            wb.active_sheet = wb.sheets.len() - 1;
            Ok(wb.active_sheet)
        })
        .expect("adding a sheet cannot fail")
    }

    pub fn remove_sheet(&mut self, index: usize) -> EditResult<()> {
        self.edit_book("Delete sheet", |wb| {
            if index >= wb.sheets.len() {
                return Err(EditError::NotFound);
            }
            wb.sheets.remove(index);
            if wb.active_sheet >= index && wb.active_sheet > 0 {
                wb.active_sheet -= 1;
            }
            Ok(())
        })
    }

    pub fn duplicate_sheet(&mut self, index: usize) -> EditResult<usize> {
        self.edit_book("Duplicate sheet", |wb| {
            let mut s = wb.sheets.get(index).cloned().ok_or(EditError::NotFound)?;
            s.title = format!("{} (copy)", s.title);
            wb.sheets.insert(index + 1, s);
            wb.active_sheet = index + 1;
            Ok(index + 1)
        })
    }

    pub fn move_sheet(&mut self, from: usize, to: usize) -> EditResult<()> {
        self.edit_book("Move sheet", |wb| {
            if from >= wb.sheets.len() || to >= wb.sheets.len() {
                return Err(EditError::NotFound);
            }
            let s = wb.sheets.remove(from);
            wb.sheets.insert(to, s);
            wb.active_sheet = to;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_undo_redo_and_dirty() {
        let mut d = Document::default();
        assert!(!d.is_dirty());
        let root = d.sheet().root;
        let n = d.sheet().topic_count();
        let id = d.edit_sheet("Add", |s| s.insert_child(root, "X")).unwrap();
        assert!(d.is_dirty());
        assert_eq!(d.sheet().topic_count(), n + 1);
        assert_eq!(d.undo_label(), Some("Add"));
        assert!(d.undo());
        assert!(!d.is_dirty());
        assert!(!d.sheet().contains(id));
        assert!(d.redo());
        assert!(d.sheet().contains(id));
        assert!(d.is_dirty());
        d.mark_saved();
        assert!(!d.is_dirty());
        d.undo();
        assert!(d.is_dirty());
    }

    #[test]
    fn failed_edit_rolls_back() {
        let mut d = Document::default();
        let root = d.sheet().root;
        let before = d.sheet().clone();
        let r = d.edit_sheet("Bad", |s| {
            s.insert_child(root, "temp")?;
            s.remove_topics(&[root])
        });
        assert!(r.is_err());
        assert_eq!(d.sheet(), &before);
        assert!(!d.can_undo());
    }

    #[test]
    fn noop_edit_records_nothing() {
        let mut d = Document::default();
        let root = d.sheet().root;
        let title = d.sheet().root_topic().title.clone();
        d.edit_sheet("Rename", |s| s.set_title(root, &title))
            .unwrap();
        assert!(!d.can_undo());
        assert!(!d.is_dirty());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut d = Document::default();
        let root = d.sheet().root;
        d.edit_sheet("A", |s| s.insert_child(root, "A")).unwrap();
        d.undo();
        assert!(d.can_redo());
        d.edit_sheet("B", |s| s.insert_child(root, "B")).unwrap();
        assert!(!d.can_redo());
    }

    #[test]
    fn sheet_management_is_undoable() {
        let mut d = Document::default();
        d.add_sheet(Sheet::new("Two", "R2"));
        assert_eq!(d.workbook().sheets.len(), 2);
        assert_eq!(d.active_index(), 1);
        d.duplicate_sheet(1).unwrap();
        assert_eq!(d.workbook().sheets.len(), 3);
        d.move_sheet(2, 0).unwrap();
        assert_eq!(d.workbook().sheets[0].title, "Two (copy)");
        d.remove_sheet(0).unwrap();
        d.remove_sheet(0).unwrap();
        assert!(d.remove_sheet(0).is_err()); // last sheet stays
        assert_eq!(d.workbook().sheets.len(), 1);
        while d.undo() {}
        assert_eq!(d.workbook().sheets.len(), 1);
        assert_eq!(d.workbook().sheets[0].title, "Map 1");
        assert!(!d.is_dirty());
    }

    #[test]
    fn undo_restores_edited_sheet_even_if_other_active() {
        let mut d = Document::default();
        let root = d.sheet().root;
        d.edit_sheet("A", |s| s.set_title(root, "changed")).unwrap();
        d.add_sheet(Sheet::new("Two", "R2"));
        d.undo(); // removes sheet two
        d.undo(); // restores title on sheet 0
        assert_eq!(d.active_index(), 0);
        assert_ne!(d.sheet().root_topic().title, "changed");
    }

    #[test]
    fn history_is_capped() {
        let mut d = Document::default();
        let root = d.sheet().root;
        for i in 0..(MAX_HISTORY + 20) {
            d.edit_sheet("t", |s| s.set_title(root, &i.to_string()))
                .unwrap();
        }
        let mut n = 0;
        while d.undo() {
            n += 1;
        }
        assert_eq!(n, MAX_HISTORY);
    }
}
