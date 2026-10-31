//! Layout decision by terminal size (design prompt §16 / R-200 to R-203). Pure logic.

/// Layout kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// `width < 40 || height < 10`. No list; only a warning.
    TooSmall,
    /// `height < 15`. `narrow` is `width < 80` (Narrow's omission rules also apply, R-200-4).
    Compact { narrow: bool },
    /// `width < 80` (and height >= 15).
    Narrow,
    /// None of the above.
    Normal,
}

/// Regions to draw. The file list is always drawn (except TooSmall).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regions {
    /// The band at the top with the name, version and clock (R-200-12).
    pub show_title: bool,
    pub show_path: bool,
    pub show_status: bool,
    pub show_key_guide: bool,
    /// Use the short key guide (narrow variants).
    pub short_key_guide: bool,
}

/// Decides the layout from the terminal size (R-200-1).
///
/// Behavior: evaluate in this order and return the first match.
/// 1. `width < 40 || height < 10` -> TooSmall
/// 2. `height < 15` -> Compact { narrow: width < 80 }
/// 3. `width < 80` -> Narrow
/// 4. otherwise -> Normal
pub fn decide(width: u16, height: u16) -> LayoutMode {
    if width < 40 || height < 10 {
        LayoutMode::TooSmall
    } else if height < 15 {
        LayoutMode::Compact { narrow: width < 80 }
    } else if width < 80 {
        LayoutMode::Narrow
    } else {
        LayoutMode::Normal
    }
}

/// Narrowest body that gets the preview pane (R-200-9): an 80-column terminal with the frame.
pub const PREVIEW_MIN_WIDTH: u16 = 78;
/// The list never gets wider than this while the preview is shown.
pub const PREVIEW_LIST_MAX: u16 = 60;

/// Splits the body width into (list, preview) for the preview pane (R-200-9).
///
/// Behavior:
/// - `width < PREVIEW_MIN_WIDTH` -> None (the list keeps the whole width).
/// - Otherwise list = min(width * 2 / 5, PREVIEW_LIST_MAX) (integer division) and
///   preview = width - list.
pub fn preview_split(width: u16) -> Option<(u16, u16)> {
    if width < PREVIEW_MIN_WIDTH {
        return None;
    }
    // u32: width * 2 overflows u16 for wide terminals.
    let list = (u32::from(width) * 2 / 5).min(u32::from(PREVIEW_LIST_MAX)) as u16;
    Some((list, width - list))
}

/// Returns the regions for a layout (R-201-2 priority: filename/list -> path -> status -> help).
///
/// Behavior:
/// - Normal: title, path, status and key guide all shown. short_key_guide = false
/// - Narrow: title, path, status and key guide all shown. short_key_guide = true
/// - Compact { narrow }: path and status shown, title and key guide hidden.
///   short_key_guide = narrow
/// - TooSmall: all false
pub fn regions(mode: LayoutMode) -> Regions {
    match mode {
        LayoutMode::Normal => Regions {
            show_title: true,
            show_path: true,
            show_status: true,
            show_key_guide: true,
            short_key_guide: false,
        },
        LayoutMode::Narrow => Regions {
            show_title: true,
            show_path: true,
            show_status: true,
            show_key_guide: true,
            short_key_guide: true,
        },
        LayoutMode::Compact { narrow } => Regions {
            show_title: false,
            show_path: true,
            show_status: true,
            show_key_guide: false,
            short_key_guide: narrow,
        },
        LayoutMode::TooSmall => Regions {
            show_title: false,
            show_path: false,
            show_status: false,
            show_key_guide: false,
            short_key_guide: false,
        },
    }
}
