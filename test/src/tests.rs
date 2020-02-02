use std::borrow::Cow;
use crate::api::{TestStep, step};
use reaper_rs::high_level::{Project, Reaper};
use std::rc::Rc;
use std::cell::RefCell;
// TODO Change rxRust so we don't always have to import this ... see existing trait refactoring issue
use rxrust::prelude::*;

fn share<T>(value: T) -> (Rc<RefCell<T>>, Rc<RefCell<T>>) {
    let shareable = Rc::new(RefCell::new(value));
    let mirror = shareable.clone();
    (shareable, mirror)
}



pub fn create_test_steps() -> impl IntoIterator<Item=TestStep> {
    vec!(
        step("Create empty project in new tab", |reaper| {
            // Given
            let current_project_before = reaper.get_current_project();
            let project_count_before = reaper.get_project_count();
            // When
            struct State { count: i32, event_project: Option<Project> }
            let (mut state, mirrored_state) = share(State { count: 0, event_project: None });
            reaper.project_switched().subscribe(move |p: Project| {
                state.replace(State { count: state.borrow().count + 1, event_project: Some(p) });
            });
            reaper.create_empty_project_in_new_tab();
            // Then
            check_eq!(mirrored_state.borrow().count, 1);
            Ok(())
        }),
        step("Add track", |reaper| {
            // Given
            // When
            // Then
            check_eq!("2", "5");
            Ok(())
        })
    )
}