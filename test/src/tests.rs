use std::borrow::Cow;
use crate::api::{TestStep, step};
use reaper_rs::high_level::{Project, Reaper, Track, ActionKind, get_media_track_guid, Guid, InputMonitoringMode, MidiRecordingInput, RecordingInput, MidiInputDevice, Volume, Pan, AutomationMode, ActionCharacter, ParameterType, toggleable};
use std::rc::Rc;
use std::cell::{RefCell, Ref, Cell};
// TODO Change rxRust so we don't always have to import this ... see existing trait refactoring issue
use rxrust::prelude::*;
use rxrust::ops::TakeUntil;
use std::ops::{Deref, DerefMut};
use c_str_macro::c_str;
use std::ffi::{CStr, CString};
use std::convert::TryFrom;
use super::mock::observe_invocations;
use std::ptr::null_mut;

pub fn create_test_steps() -> impl IntoIterator<Item=TestStep> {
    vec!(
        step("Create empty project in new tab", |reaper, step| {
            // Given
            let current_project_before = reaper.get_current_project();
            let project_count_before = reaper.get_project_count();
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.project_switched().take_until(step.finished).subscribe(move |p| {
                    mock.invoke(p);
                });
            });
            let new_project = reaper.create_empty_project_in_new_tab();
            // Then
            check_eq!(current_project_before, current_project_before);
            check_eq!(reaper.get_project_count(), project_count_before + 1);
            check_eq!(reaper.get_projects().count() as u32, project_count_before + 1);
            check_ne!(reaper.get_current_project(), current_project_before);
            check_eq!(reaper.get_current_project(), new_project);
            check_ne!(reaper.get_projects().nth(0), Some(new_project));
            //            assertTrue(Reaper::instance().projectsWithCurrentOneFirst().as_blocking().first() == newProject);
//            assertTrue(Reaper::instance().projectsWithCurrentOneFirst().as_blocking().count() == projectCountBefore + 1);
            check_eq!(new_project.get_track_count(), 0);
            check!(new_project.get_index() > 0);
            check_eq!(new_project.get_file_path(), None);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), new_project);
            Ok(())
        }),
        step("Add track", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            // When
            #[derive(Default)]
            struct State { count: i32, track: Option<Track> }
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_added().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            let new_track = project.add_track();
            // Then
            check_eq!(project.get_track_count(), 1);
            check_eq!(new_track.get_index(), 0);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), new_track.into());
            Ok(())
        }),
        step("FnMut action", |reaper, step| {
            // TODO Add this as new test
            return Ok(());
            let mut i = 0;
            let action1 = reaper.register_action(
                c_str!("reaperRsCounter"),
                c_str!("reaper-rs counter"),
                move || {
                    let owned = format!("Hello from Rust number {}\0", i);
                    let reaper = Reaper::instance();
                    reaper.show_console_msg(CStr::from_bytes_with_nul(owned.as_bytes()).unwrap());
                    i += 1;
                },
                ActionKind::NotToggleable,
            );
            Ok(())
        }),
        step("Query master track", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            // When
            let master_track = project.get_master_track();
            // Then
            check!(master_track.is_master_track());
            Ok(())
        }),
        step("Query all tracks", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            project.add_track();
            // When
            let tracks = project.get_tracks();
            // Then
            check_eq!(tracks.count(), 2);
            Ok(())
        }),
        step("Query track by GUID", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            let first_track = get_first_track()?;
            let new_track = project.add_track();
            // When
            let found_track = project.get_track_by_guid(new_track.get_guid());
            // Then
            check!(found_track.is_available());
            check_eq!(&found_track, &new_track);
            check_ne!(&found_track, &first_track);
            check_eq!(new_track.get_guid(), &get_media_track_guid(new_track.get_media_track()));
            Ok(())
        }),
        step("Query non-existent track by GUID", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            // When
            let guid = Guid::try_from(c_str!("{E64BB283-FB17-4702-ACFA-2DDB7E38F14F}"))?;
            let found_track = project.get_track_by_guid(&guid);
            // Then
            check!(!found_track.is_available());
            Ok(())
        }),
        step("Query track project", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            let track = get_first_track()?;
            // When
            let track_project = track.get_project();
            // Then
            check_eq!(track_project, project);
            Ok(())
        }),
        step("Query track name", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let track_name = track.get_name();
            // Then
            check_eq!(track_name.as_bytes().len(), 0);
            Ok(())
        }),
        step("Set track name", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            // TODO Factor this state pattern out
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_name_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.set_name(c_str!("Foo Bla"));
            // Then
            check_eq!(track.get_name(), c_str!("Foo Bla").to_owned());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Query track input monitoring", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let mode = track.get_input_monitoring_mode();
            // Then
            check_eq!(mode, InputMonitoringMode::Normal);
            Ok(())
        }),
        step("Set track input monitoring", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            #[derive(Default)]
            struct State { count: i32, track: Option<Track> }
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_input_monitoring_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.set_input_monitoring_mode(InputMonitoringMode::NotWhenPlaying);
            // Then
            check_eq!(track.get_input_monitoring_mode(), InputMonitoringMode::NotWhenPlaying);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Query track recording input", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let input = track.get_recording_input();
            // Then
            match input {
                RecordingInput::Mono => Ok(()),
                _ => Err("Expected MidiRecordingInput".into())
            }
        }),
        step("Set track recording input MIDI all/all", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_input_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.set_recording_input(MidiRecordingInput::from_all_devices_and_channels());
            // Then
            let input = track.get_recording_input();
            let input_data = match input {
                RecordingInput::Midi(d) => d,
                _ => return Err("Expected MIDI input".into())
            };
            check_eq!(input_data.get_channel(), None);
            check_eq!(input_data.get_device(), None);
            check_eq!(input_data.get_rec_input_index(), 6112);
            check_eq!(RecordingInput::from_rec_input_index(6112), input);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Set track recording input MIDI 4/5", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            track.set_recording_input(MidiRecordingInput::from_device_and_channel(MidiInputDevice::new(4), 5));
            // Then
            let input = track.get_recording_input();
            let input_data = match input {
                RecordingInput::Midi(d) => d,
                _ => return Err("Expected MIDI input".into())
            };
            check_eq!(input_data.get_channel(), Some(5));
            check_eq!(input_data.get_device().ok_or("Expected device")?.get_id(), 4);
            Ok(())
        }),
        step("Set track recording input MIDI 7/all", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            track.set_recording_input(MidiRecordingInput::from_all_channels_of_device(MidiInputDevice::new(7)));
            // Then
            let input = track.get_recording_input();
            let input_data = match input {
                RecordingInput::Midi(d) => d,
                _ => return Err("Expected MIDI input".into())
            };
            check_eq!(input_data.get_channel(), None);
            check_eq!(input_data.get_device(), Some(MidiInputDevice::new(7)));
            Ok(())
        }),
        step("Set track recording input MIDI all/15", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            track.set_recording_input(MidiRecordingInput::from_all_devices_with_channel(15));
            // Then
            let input = track.get_recording_input();
            let input_data = match input {
                RecordingInput::Midi(d) => d,
                _ => return Err("Expected MIDI input".into())
            };
            check_eq!(input_data.get_channel(), Some(15));
            check_eq!(input_data.get_device(), None);
            Ok(())
        }),
        step("Query track volume", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let volume = track.get_volume();
            // Then
            check_eq!(volume.get_reaper_value(), 1.0);
            check_eq!(volume.get_db(), 0.0);
            check_eq!(volume.get_normalized_value(), 0.71599999999999997);
            Ok(())
        }),
        step("Set track volume", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_volume_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.set_volume(Volume::of_normalized_value(0.25));
            // Then
            let volume = track.get_volume();
            check_eq!(volume.get_reaper_value(), 0.031588093366685013);
            check_eq!(volume.get_db(), -30.009531739774296);
            check_eq!(volume.get_normalized_value(), 0.25000000000003497);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Query track pan", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let pan = track.get_pan();
            // Then
            check_eq!(pan.get_reaper_value(), 0.0);
            check_eq!(pan.get_normalized_value(), 0.5);
            Ok(())
        }),
        step("Set track pan", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_pan_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.set_pan(Pan::of_normalized_value(0.25));
            // Then
            let pan = track.get_pan();
            check_eq!(pan.get_reaper_value(), -0.5);
            check_eq!(pan.get_normalized_value(), 0.25);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Query track selection state", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            let track = get_first_track()?;
            // When
            let is_selected = track.is_selected();
            // Then
            check!(!is_selected);
            check_eq!(project.get_selected_track_count(false), 0);
            Ok(())
        }),
        step("Select track", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track = get_first_track()?;
            let track2 = project.get_track_by_index(2).ok_or("No track at index 2")?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_selected_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.select();
            track2.select();
            // Then
            check!(track.is_selected());
            check!(track2.is_selected());
            check_eq!(project.get_selected_track_count(false), 2);
            let first_selected_track = project.get_first_selected_track(false)
                .ok_or("Couldn't get first selected track")?;
            check_eq!(first_selected_track.get_index(), 0);
            check_eq!(project.get_selected_tracks(false).count(), 2);
            check_eq!(mock.invocation_count(), 2);
            check_eq!(mock.last_arg(), track2.into());
            Ok(())
        }),
        step("Unselect track", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_selected_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.unselect();
            // Then
            check!(!track.is_selected());
            check_eq!(project.get_selected_track_count(false), 1);
            let first_selected_track = project.get_first_selected_track(false)
                .ok_or("Couldn't get first selected track")?;
            check_eq!(first_selected_track.get_index(), 2);
            check_eq!(project.get_selected_tracks(false).count(), 1);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Select master track", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let master_track = project.get_master_track();
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_selected_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            project.unselect_all_tracks();
            master_track.select();
            // Then
            check!(master_track.is_selected());
            check_eq!(project.get_selected_track_count(true), 1);
            let first_selected_track = project.get_first_selected_track(true)
                .ok_or("Couldn't get first selected track")?;
            check!(first_selected_track.is_master_track());
            check_eq!(project.get_selected_tracks(true).count(), 1);
            // TODO REAPER doesn't notify us about master track selection currently
            check_eq!(mock.invocation_count(), 1);
            let last_arg: Track = mock.last_arg().into();
            check_eq!(last_arg.get_index(), 2);
            Ok(())
        }),
        step("Query track auto arm mode", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let is_in_auto_arm_mode = track.has_auto_arm_enabled();
            // Then
            check!(!is_in_auto_arm_mode);
            Ok(())
        }),
        step("Query track arm state", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let is_armed = track.is_armed(true);
            let is_armed_ignoring_auto_arm = track.is_armed(false);
            // Then
            check!(!is_armed);
            check!(!is_armed_ignoring_auto_arm);
            Ok(())
        }),
        step("Arm track in normal mode", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_arm_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.arm(true);
            // Then
            check!(track.is_armed(true));
            check!(track.is_armed(false));
            check!(!track.has_auto_arm_enabled());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Disarm track in normal mode", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_arm_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.disarm(true);
            // Then
            check!(!track.is_armed(true));
            check!(!track.is_armed(false));
            check!(!track.has_auto_arm_enabled());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Enable track auto-arm mode", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            track.enable_auto_arm();
            // Then
            check!(track.has_auto_arm_enabled());
            check!(!track.is_armed(true));
            check!(!track.is_armed(false));
            Ok(())
        }),
        step("Arm track in auto-arm mode", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_arm_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.arm(true);
            // Then
            check!(track.is_armed(true));
            // TODO Interesting! GetMediaTrackInfo_Value read with I_RECARM seems to support auto-arm already!
            // So maybe we should remove the chunk check and the parameter supportAutoArm
            check!(track.is_armed(false));
            check!(track.has_auto_arm_enabled());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Disarm track in auto-arm mode", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_arm_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.disarm(true);
            // Then
            check!(!track.is_armed(true));
            check!(!track.is_armed(false));
            check!(track.has_auto_arm_enabled());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Disable track auto-arm mode", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            track.disable_auto_arm();
            // Then
            check!(!track.has_auto_arm_enabled());
            check!(!track.is_armed(true));
            check!(!track.is_armed(false));
            Ok(())
        }),
        step("Switch to normal track mode while armed", |reaper, _| {
            // Given
            let track = get_first_track()?;
            track.arm(true);
            check!(track.is_armed(true));
            // When
            track.disable_auto_arm();
            // Then
            check!(!track.has_auto_arm_enabled());
            check!(track.is_armed(true));
            check!(track.is_armed(false));
            Ok(())
        }),
        step("Switch track to auto-arm mode while armed", |reaper, _| {
            // Given
            let track = get_first_track()?;
            track.unselect();
            // When
            track.enable_auto_arm();
            // Then
            check!(track.has_auto_arm_enabled());
            check!(track.is_armed(true));
            check!(track.is_armed(false));
            Ok(())
        }),
        step("Disarm track in auto-arm mode (ignoring auto-arm)", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_arm_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.disarm(false);
            // Then
            check!(!track.is_armed(true));
            check!(!track.is_armed(false));
            check!(!track.has_auto_arm_enabled());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Arm track in auto-arm mode (ignoring auto-arm)", |reaper, step| {
            // Given
            let track = get_first_track()?;
            track.enable_auto_arm();
            check!(track.has_auto_arm_enabled());
            check!(!track.is_armed(true));
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_arm_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.arm(false);
            // Then
            check!(track.is_armed(true));
            check!(track.is_armed(false));
            check!(!track.has_auto_arm_enabled());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track.into());
            Ok(())
        }),
        step("Select track exclusively", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track_1 = project.get_track_by_index(0).ok_or("Missing track 1")?;
            let track_2 = project.get_track_by_index(1).ok_or("Missing track 2")?;
            let track_3 = project.get_track_by_index(2).ok_or("Missing track 3")?;
            track_1.unselect();
            track_2.select();
            track_3.select();
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_selected_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track_1.select_exclusively();
            // Then
            check!(track_1.is_selected());
            check!(!track_2.is_selected());
            check!(!track_3.is_selected());
            check_eq!(project.get_selected_track_count(false), 1);
            check!(project.get_first_selected_track(false).is_some());
            check_eq!(project.get_selected_tracks(false).count(), 1);
            check_eq!(mock.invocation_count(), 3);
            Ok(())
        }),
        step("Remove track", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track_count_before = project.get_track_count();
            let track_1 = project.get_track_by_number(1).ok_or("Missing track 1")?;
            let track_2 = project.get_track_by_number(2).ok_or("Missing track 2")?;
            let track_2_guid = track_2.get_guid();
            check!(track_1.is_available());
            check_eq!(track_2.get_index(), 1);
            check!(track_2.is_available());
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_removed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            project.remove_track(&track_1);
            // Then
            check_eq!(project.get_track_count(), track_count_before - 1);
            check!(!track_1.is_available());
            check_eq!(track_2.get_index(), 0);
            check_eq!(track_2.get_guid(), track_2_guid);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track_1.into());
            Ok(())
        }),
        step("Query track automation mode", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let automation_mode = track.get_automation_mode();
            let global_automation_override = reaper.get_global_automation_override();
            let effective_automation_mode = track.get_effective_automation_mode();
            // Then
            check_eq!(automation_mode, AutomationMode::TrimRead);
            check_eq!(global_automation_override, AutomationMode::NoOverride);
            check_eq!(effective_automation_mode, AutomationMode::TrimRead);
            Ok(())
        }),
        step("Query track send count", |reaper, _| {
            // Given
            let track = get_first_track()?;
            // When
            let send_count = track.get_send_count();
            // Then
            check_eq!(send_count, 0);
            check_eq!(track.get_send_by_index(0), None);
            check!(!track.get_send_by_target_track(track.clone()).is_available());
            check!(!track.get_index_based_send_by_index(0).is_available());
            check_eq!(track.get_sends().count(), 0);
            Ok(())
        }),
        step("Add track send", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            let track_1 = project.get_track_by_index(0).ok_or("Missing track 1")?;
            let track_2 = project.get_track_by_index(1).ok_or("Missing track 2")?;
            // When
            let send = track_1.add_send_to(track_2.clone());
            // Then
            check_eq!(track_1.get_send_count(), 1);
            check_eq!(track_1.get_send_by_index(0), Some(send));
            check!(track_1.get_send_by_target_track(track_2.clone()).is_available());
            check!(!track_2.get_send_by_target_track(track_1.clone()).is_available());
            check!(track_1.get_index_based_send_by_index(0).is_available());
            check_eq!(track_1.get_sends().count(), 1);
            Ok(())
        }),
        step("Query track send", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            let track_1 = project.get_track_by_index(0).ok_or("Missing track 1")?;
            let track_2 = project.get_track_by_index(1).ok_or("Missing track 2")?;
            let track_3 = project.add_track();
            // When
            let send_to_track_2 = track_1.get_send_by_target_track(track_2.clone());
            let send_to_track_3 = track_1.add_send_to(track_3.clone());
            // Then
            check!(send_to_track_2.is_available());
            check!(send_to_track_3.is_available());
            check_eq!(send_to_track_2.get_index(), 0);
            check_eq!(send_to_track_3.get_index(), 1);
            check_eq!(send_to_track_2.get_source_track(), track_1);
            check_eq!(send_to_track_3.get_source_track(), track_1);
            check_eq!(send_to_track_2.get_target_track(), track_2);
            check_eq!(send_to_track_3.get_target_track(), track_3);
            check_eq!(send_to_track_2.get_volume().get_db(), 0.0);
            check_eq!(send_to_track_3.get_volume().get_db(), 0.0);
            Ok(())
        }),
        step("Set track send volume", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track_1 = project.get_track_by_index(0).ok_or("Missing track 1")?;
            let track_3 = project.get_track_by_index(2).ok_or("Missing track 3")?;
            let send = track_1.get_send_by_target_track(track_3);
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_send_volume_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            send.set_volume(Volume::of_normalized_value(0.25));
            // Then
            check_eq!(send.get_volume().get_db(), -30.009531739774296);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), send.into());
            Ok(())
        }),
        step("Set track send pan", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track_1 = project.get_track_by_index(0).ok_or("Missing track 1")?;
            let track_3 = project.get_track_by_index(2).ok_or("Missing track 3")?;
            let send = track_1.get_send_by_target_track(track_3);
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_send_pan_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            send.set_pan(Pan::of_normalized_value(0.25));
            // Then
            check_eq!(send.get_pan().get_reaper_value(), -0.5);
            check_eq!(send.get_pan().get_normalized_value(), 0.25);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), send.into());
            Ok(())
        }),
        step("Query action", |reaper, _| {
            // Given
            let track = get_first_track()?;
            track.select_exclusively();
            check!(!track.is_muted());
            // When
            let toggle_action = reaper.get_main_section().get_action_by_command_id(6);
            let normal_action = reaper.get_main_section().get_action_by_command_id(41075);
            let normal_action_by_index = reaper.get_main_section().get_action_by_index(normal_action.get_index());
            // Then
            check!(toggle_action.is_available());
            check!(normal_action.is_available());
            check_eq!(toggle_action.get_character(), ActionCharacter::Toggle);
            check_eq!(normal_action.get_character(), ActionCharacter::Trigger);
            check!(!toggle_action.is_on());
            check!(!normal_action.is_on());
            check_eq!(toggle_action.get_parameter_type(), ParameterType::Action);
            check_eq!(toggle_action.clone(), toggle_action);
            check_eq!(toggle_action.get_command_id(), 6);
            check_eq!(toggle_action.get_command_name(), None);
            check_eq!(toggle_action.get_name(), Some(c_str!("Track: Toggle mute for selected tracks")));
            check!(toggle_action.get_index() > 0);
            check_eq!(toggle_action.get_section(), reaper.get_main_section());
            check_eq!(normal_action_by_index, normal_action);
            Ok(())
        }),
        step("Invoke action", |reaper, step| {
            // Given
            let action = reaper.get_main_section().get_action_by_command_id(6);
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.action_invoked().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            action.invoke_as_trigger(None);
            // Then
            check!(action.is_on());
            check!(track.is_muted());
            // TODO Actually it would be nice if the actionInvoked event would be raised but it isn't
            check_eq!(mock.invocation_count(), 0);
            Ok(())
        }),
        step("Test actionInvoked event", |reaper, step| {
            // Given
            let action = reaper.get_main_section().get_action_by_command_id(1582);
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.action_invoked().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            reaper.medium.main_on_command_ex(action.get_command_id() as i32, 0, null_mut());
            // Then
            check_eq!(mock.invocation_count(), 1);
            check_eq!(*mock.last_arg(), action);
            Ok(())
        }),
        step("Unmute track", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_mute_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.unmute();
            // Then
            check!(!track.is_muted());
            // For some reason REAPER doesn't call SetSurfaceMute on control surfaces when an action
            // caused the muting. So HelperControlSurface still thinks the track was unmuted and
            // therefore will not fire a change event!
            check_eq!(mock.invocation_count(), 0);
            Ok(())
        }),
        step("Mute track", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_mute_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.mute();
            // Then
            check!(track.is_muted());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track);
            Ok(())
        }),
        step("Solo track", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_solo_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.solo();
            // Then
            check!(track.is_solo());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track);
            Ok(())
        }),
        step("Unsolo track", |reaper, step| {
            // Given
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_solo_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            track.unsolo();
            // Then
            check!(!track.is_solo());
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track);
            Ok(())
        }),
        step("Generate GUID", |reaper, _| {
            // Given
            // When
            let guid = reaper.generate_guid();
            // Then
            check_eq!(guid.to_string_with_braces().len(), 38);
            Ok(())
        }),
        step("Main section functions", |reaper, _| {
            // Given
            let section = reaper.get_main_section();
            // When
            let actions = section.get_actions();
            // Then
            check_eq!(actions.count() as u32, section.get_action_count());
            Ok(())
        }),
        step("Register and unregister action", |reaper, _| {
            // Given
            // When
            // TODO Rename RegisteredAction to ActionRegistration or something like that
            let (mock, reg) = observe_invocations(|mock| {
                reaper.register_action(
                    c_str!("reaperRsTest"),
                    c_str!("reaper-rs test action"),
                    move || {
                        mock.invoke(42);
                    },
                    ActionKind::NotToggleable
                )
            });
            let action = reaper.get_action_by_command_name(c_str!("reaperRsTest").into());
            // Then
            check!(action.is_available());
            check_eq!(mock.invocation_count(), 0);
            action.invoke_as_trigger(None);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), 42);
            check_eq!(action.get_character(), ActionCharacter::Trigger);
            check!(action.get_command_id() > 0);
            check_eq!(action.get_command_name(), Some(c_str!("reaperRsTest")));
            check!(action.get_index() >= 0);
            check!(!action.is_on());
            check_eq!(action.get_name(), Some(c_str!("reaper-rs test action")));
            reg.unregister();
            check!(!action.is_available());
            Ok(())
        }),
        step("Register and unregister toggle action", |reaper, _| {
            // Given
            // When
            let (mock, reg) = observe_invocations(|mock| {
                let cloned_mock = mock.clone();
                reaper.register_action(
                    c_str!("reaperRsTest2"),
                    c_str!("reaper-rs test toggle action"),
                    move || {
                        mock.invoke(43);
                    },
                    toggleable(move || {
                        cloned_mock.invocation_count() % 2 == 1
                    })
                )
            });
            let action = reaper.get_action_by_command_name(c_str!("reaperRsTest2").into());
            // Then
            check!(action.is_available());
            check_eq!(mock.invocation_count(), 0);
            check!(!action.is_on());
            action.invoke_as_trigger(None);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), 43);
            check!(action.is_on());
            check_eq!(action.get_character(), ActionCharacter::Toggle);
            check!(action.get_command_id() > 0);
            check_eq!(action.get_command_name(), Some(c_str!("reaperRsTest2")));
            check!(action.get_index() >= 0);
            check_eq!(action.get_name(), Some(c_str!("reaper-rs test toggle action")));
            reg.unregister();
            check!(!action.is_available());
            Ok(())
        }),
        // TODO Insert FX tests HERE!
        step("Insert track at", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track_1 = project.get_track_by_index(0).ok_or("Missing track 1")?;
            let track_2 = project.get_track_by_index(1).ok_or("Missing track 2")?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_added().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            let new_track = project.insert_track_at(1);
            new_track.set_name(c_str!("Inserted track"));
            // Then
            check_eq!(project.get_track_count(), 4);
            check_eq!(new_track.get_index(), 1);
            check_eq!(new_track.get_name(), c_str!("Inserted track").into());
            check_eq!(track_2.get_index(), 2);
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), new_track);
            Ok(())
        }),
        step("Query MIDI input devices", |reaper, _| {
            // Given
            // When
            let devs = reaper.get_midi_input_devices();
            let dev_0 = reaper.get_midi_input_device_by_id(0);
            // Then
            check_ne!(devs.count(), 0);
            check!(dev_0.is_available());
            Ok(())
        }),
        step("Query MIDI output devices", |reaper, _| {
            // Given
            // When
            let devs = reaper.get_midi_output_devices();
            let dev_0 = reaper.get_midi_output_device_by_id(0);
            // Then
            check_ne!(devs.count(), 0);
            check!(dev_0.is_available());
            Ok(())
        }),
        // TODO Insert test "Stuff MIDI messages"
        step("Use undoable", |reaper, step| {
            // Given
            let project = reaper.get_current_project();
            let track = get_first_track()?;
            // When
            let (mock, _) = observe_invocations(|mock| {
                reaper.track_name_changed().take_until(step.finished).subscribe(move |t| {
                    mock.invoke(t);
                });
            });
            let track_mirror = track.clone();
            project.undoable(c_str!("ReaPlus integration test operation"), move || {
                track_mirror.set_name(c_str!("Renamed"));
            });
            let label = project.get_label_of_last_undoable_action();
            // Then
            check_eq!(track.get_name(), c_str!("Renamed").into());
            check_eq!(label, Some(c_str!("ReaPlus integration test operation")));
            check_eq!(mock.invocation_count(), 1);
            check_eq!(mock.last_arg(), track);
            Ok(())
        }),
        step("Undo", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            let track = get_first_track()?;
            // When
            let successful = project.undo();
            let label = project.get_label_of_last_redoable_action();
            // Then
            check!(successful);
            check_eq!(track.get_name().as_bytes().len(), 0);
            check_eq!(label, Some(c_str!("ReaPlus integration test operation")));
            Ok(())
        }),
        step("Redo", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            let track = get_first_track()?;
            // When
            let successful = project.redo();
            let label = project.get_label_of_last_undoable_action();
            // Then
            check!(successful);
            check_eq!(track.get_name(), c_str!("Renamed").into());
            check_eq!(label, Some(c_str!("ReaPlus integration test operation")));
            Ok(())
        }),
        step("Get REAPER window", |reaper, _| {
            // Given
            // When
            let window = reaper.get_main_window();
            // Then
            check!(!window.is_null());
            Ok(())
        }),
        step("Mark project as dirty", |reaper, _| {
            // Given
            let project = reaper.get_current_project();
            // When
            project.mark_as_dirty();
            // Then
            // TODO Doesn't say very much because it has been dirty before already. Save before!?
            check!(project.is_dirty());
            Ok(())
        }),
    )
}

fn get_first_track() -> Result<Track, &'static str> {
    Reaper::instance().get_current_project().get_first_track().ok_or("First track not found")
}