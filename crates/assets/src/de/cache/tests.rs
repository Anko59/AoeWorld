use super::*;
use crate::sld::Frame;
use std::cell::Cell;

fn frame(index: u16) -> Frame {
    Frame {
        width: 1,
        height: 1,
        anchor_x: 0,
        anchor_y: 0,
        index,
        main: None,
        shadow: None,
        player: None,
    }
}

#[test]
fn only_current_source_is_retained_and_returned_frames_keep_order() {
    let calls = Cell::new(0);
    let mut cache = Cache::new(|source| {
        calls.set(calls.get() + 1);
        Ok(vec![frame(source as u16), frame(source as u16 + 10)])
    });
    assert_eq!(cache.get(0).unwrap()[1].index, 10);
    assert_eq!(cache.get(0).unwrap()[0].index, 0);
    assert_eq!(calls.get(), 1);
    assert_eq!(cache.get(1).unwrap()[1].index, 11);
    assert_eq!(calls.get(), 2);
    assert_eq!(cache.get(0).unwrap()[0].index, 0);
    assert_eq!(calls.get(), 3);
}

#[test]
fn failed_source_load_does_not_return_an_earlier_sources_frames() {
    let mut cache = Cache::new(|source| {
        if source == 1 {
            Err(crate::invalid("DE", 0, "synthetic source failure"))
        } else {
            Ok(vec![frame(source as u16)])
        }
    });
    assert_eq!(cache.get(0).unwrap()[0].index, 0);
    assert!(cache.get(1).is_err());
    assert_eq!(cache.get(2).unwrap()[0].index, 2);
}
