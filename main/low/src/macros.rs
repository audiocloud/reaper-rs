/// Macro which gathers end exposes the static REAPER VST plug-in context.
///
/// This macro provides module entry points which gather some handles for creating
/// a REAPER VST plug-in context. The gathered handles are exposed via the function
/// `reaper_vst_plugin::static_context()` and are intended to be passed to
/// [`PluginContext::from_extension_plugin()`].
///
/// # Example
///
/// ```
/// use reaper_low::reaper_vst_plugin;
///
/// reaper_vst_plugin!();
///
/// let static_context = reaper_vst_plugin::static_context();
/// ```
///
/// [`PluginContext::from_extension_plugin()`]:
/// struct.PluginContext.html#method.from_extension_plugin
#[macro_export]
macro_rules! reaper_vst_plugin {
    () => {
        mod reaper_vst_plugin {
            // TODO-low Code here is very similar to the one in reaper-macros. Factor out.
            /// Exposes the (hopefully) obtained handles.
            pub fn static_context() -> reaper_low::StaticVstPluginContext {
                reaper_low::StaticVstPluginContext {
                    h_instance: unsafe { HINSTANCE },
                    get_swell_func: unsafe { GET_SWELL_FUNC },
                }
            }

            /// Entry point for getting hold of the module handle (HINSTANCE).
            #[cfg(target_family = "windows")]
            #[allow(non_snake_case)]
            #[no_mangle]
            extern "C" fn DllMain(
                hinstance: reaper_low::raw::HINSTANCE,
                reason: u32,
                _: *const u8,
            ) -> u32 {
                if (reason == reaper_low::raw::DLL_PROCESS_ATTACH) {
                    INIT_HINSTANCE.call_once(|| {
                        unsafe { HINSTANCE = hinstance };
                    });
                }
                1
            }
            static mut HINSTANCE: reaper_low::raw::HINSTANCE = std::ptr::null_mut();
            static INIT_HINSTANCE: std::sync::Once = std::sync::Once::new();

            // Entry point for getting hold of the SWELL function provider.
            #[allow(non_snake_case)]
            #[no_mangle]
            extern "C" fn SWELL_dllMain(
                hinstance: reaper_low::raw::HINSTANCE,
                reason: u32,
                get_func: Option<
                    unsafe extern "C" fn(
                        name: *const std::os::raw::c_char,
                    ) -> *mut std::os::raw::c_void,
                >,
            ) -> std::os::raw::c_int {
                if (reason == reaper_low::raw::DLL_PROCESS_ATTACH) {
                    INIT_GET_SWELL_FUNC.call_once(|| {
                        unsafe { GET_SWELL_FUNC = get_func };
                    });
                }
                // Give the C++ side of the plug-in the chance to initialize its SWELL function
                // pointers as well. This is only needed on Linux. On Windows we don't have SWELL
                // and on macOS the mechanism is different (SwellAPPInitializer calls this function,
                // so Objective-C side already has access to the SWELL function pointers).
                #[cfg(target_os = "linux")]
                unsafe {
                    SWELL_dllMain_called_from_rust(hinstance, reason, get_func);
                }
                1
            }
            #[cfg(target_os = "linux")]
            extern "C" {
                pub fn SWELL_dllMain_called_from_rust(
                    hinstance: reaper_low::raw::HINSTANCE,
                    reason: u32,
                    get_func: Option<
                        unsafe extern "C" fn(
                            name: *const std::os::raw::c_char,
                        ) -> *mut std::os::raw::c_void,
                    >,
                ) -> std::os::raw::c_int;
            }
            static mut GET_SWELL_FUNC: Option<
                unsafe extern "C" fn(
                    name: *const std::os::raw::c_char,
                ) -> *mut std::os::raw::c_void,
            > = None;
            static INIT_GET_SWELL_FUNC: std::sync::Once = std::sync::Once::new();
        }
    };
}
