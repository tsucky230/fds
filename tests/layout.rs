//! Layout decision (AC 11-14 / R-200 to R-203).

use fds::layout::{LayoutMode, decide, regions};

#[test]
fn boundaries_follow_evaluation_order() {
    assert_eq!(decide(80, 24), LayoutMode::Normal);
    assert_eq!(decide(80, 15), LayoutMode::Normal);
    assert_eq!(decide(80, 20), LayoutMode::Normal); // formerly a gap
    assert_eq!(decide(80, 14), LayoutMode::Compact { narrow: false });
    assert_eq!(decide(79, 24), LayoutMode::Narrow);
    assert_eq!(decide(79, 14), LayoutMode::Compact { narrow: true });
    assert_eq!(decide(40, 10), LayoutMode::Compact { narrow: true });
    assert_eq!(decide(39, 10), LayoutMode::TooSmall);
    assert_eq!(decide(40, 9), LayoutMode::TooSmall);
    assert_eq!(decide(0, 0), LayoutMode::TooSmall);
    assert_eq!(decide(u16::MAX, u16::MAX), LayoutMode::Normal);
}

#[test]
fn regions_per_mode() {
    let n = regions(LayoutMode::Normal);
    assert!(n.show_path && n.show_status && n.show_key_guide && !n.short_key_guide);

    let r = regions(LayoutMode::Narrow);
    assert!(r.show_path && r.show_status && r.show_key_guide && r.short_key_guide);

    let c = regions(LayoutMode::Compact { narrow: false });
    assert!(c.show_path && c.show_status && !c.show_key_guide && !c.short_key_guide);

    let cn = regions(LayoutMode::Compact { narrow: true });
    assert!(cn.show_path && cn.show_status && !cn.show_key_guide && cn.short_key_guide);

    let t = regions(LayoutMode::TooSmall);
    assert!(!t.show_path && !t.show_status && !t.show_key_guide && !t.short_key_guide);
}

#[test]
fn preview_split_boundaries() {
    use fds::layout::preview_split;
    // (body width, (list, preview))
    for (width, split) in [
        (0, None),
        (40, None),
        (77, None),
        (78, Some((31, 47))),
        (79, Some((31, 48))),
        (80, Some((32, 48))),
        (98, Some((39, 59))),
        (100, Some((40, 60))),
        (149, Some((59, 90))),
        (150, Some((60, 90))),
        (151, Some((60, 91))),
        (200, Some((60, 140))),
        (u16::MAX, Some((60, u16::MAX - 60))),
    ] {
        assert_eq!(preview_split(width), split, "{width}");
    }
}
