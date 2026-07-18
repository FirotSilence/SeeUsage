#[cfg(target_os = "windows")]
mod platform {
    use crate::{
        i18n::{self, key},
        settings,
    };
    use std::{
        mem::size_of,
        ptr::{null, null_mut},
        sync::OnceLock,
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_WINDOW, CreateFontW, DEFAULT_CHARSET,
            DEFAULT_PITCH, DeleteObject, FF_DONTCARE, GetMonitorInfoW, HFONT,
            MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, OUT_DEFAULT_PRECIS,
            UpdateWindow,
        },
        System::{LibraryLoader::GetModuleHandleW, SystemServices::SS_SIMPLE},
        UI::{
            Controls::{EM_SETLIMITTEXT, EM_SETSEL},
            HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem},
            Input::KeyboardAndMouse::{SetActiveWindow, SetFocus},
            WindowsAndMessaging::{
                BS_DEFPUSHBUTTON, BS_PUSHBUTTON, BringWindowToTop, CREATESTRUCTW, CreateWindowExW,
                DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL, GWLP_USERDATA,
                GetClientRect, GetCursorPos, GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW,
                GetWindowTextW, IDC_ARROW, IDCANCEL, IDOK, IsDialogMessageW, IsWindow,
                IsWindowVisible, LoadCursorW, MA_ACTIVATE, MB_ICONERROR, MB_OK, MB_SETFOREGROUND,
                MB_TOPMOST, MSG, MessageBoxW, PostMessageW, RegisterClassW, SW_HIDE, SW_SHOW,
                SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetForegroundWindow, SetWindowLongPtrW,
                SetWindowPos, ShowWindow, TranslateMessage, WM_CLOSE, WM_COMMAND, WM_CREATE,
                WM_DPICHANGED, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_SETFONT,
                WM_SIZE, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_CLIENTEDGE,
                WS_EX_CONTROLPARENT, WS_EX_DLGMODALFRAME, WS_EX_TOOLWINDOW, WS_SYSMENU, WS_TABSTOP,
                WS_VISIBLE,
            },
        },
    };

    const CLASS_NAME: &str = "SeeUsage.InputDialog";
    const CLIENT_WIDTH_DIP: i32 = 440;
    const CLIENT_HEIGHT_DIP: i32 = 174;
    const DUAL_CLIENT_HEIGHT_DIP: i32 = 246;

    struct SecondaryInput {
        prompt: String,
        default: String,
        max_chars: usize,
        label: HWND,
        edit: HWND,
    }

    struct InputState {
        prompt: String,
        default: String,
        max_chars: usize,
        cancel_text: String,
        confirm_text: String,
        secondary: Option<SecondaryInput>,
        label: HWND,
        edit: HWND,
        cancel: HWND,
        confirm: HWND,
        font: HFONT,
        done: bool,
        result: Option<(String, Option<String>)>,
    }

    #[derive(Clone, Copy)]
    struct DialogLayout {
        label: RECT,
        edit: RECT,
        secondary_label: Option<RECT>,
        secondary_edit: Option<RECT>,
        cancel: RECT,
        confirm: RECT,
    }

    pub fn prompt_text(
        title: &str,
        prompt: &str,
        default: &str,
        max_chars: usize,
    ) -> Option<String> {
        prompt_fields(title, prompt, default, max_chars, None).map(|result| result.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prompt_reminder(
        title: &str,
        text_prompt: &str,
        text_default: &str,
        text_max_chars: usize,
        schedule_prompt: &str,
        schedule_default: &str,
        schedule_max_chars: usize,
    ) -> Option<(String, String)> {
        let (text, schedule) = prompt_fields(
            title,
            text_prompt,
            text_default,
            text_max_chars,
            Some((schedule_prompt, schedule_default, schedule_max_chars)),
        )?;
        Some((text, schedule?))
    }

    fn prompt_fields(
        title: &str,
        prompt: &str,
        default: &str,
        max_chars: usize,
        secondary: Option<(&str, &str, usize)>,
    ) -> Option<(String, Option<String>)> {
        if !register_class() {
            show_error("Could not open the input window.");
            return None;
        }

        let instance = unsafe { GetModuleHandleW(null()) };
        if instance.is_null() {
            return None;
        }
        let dpi = unsafe { GetDpiForSystem() }.max(96);
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
        let extended_style = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT | WS_EX_TOOLWINDOW;
        let client_height = if secondary.is_some() {
            DUAL_CLIENT_HEIGHT_DIP
        } else {
            CLIENT_HEIGHT_DIP
        };
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: dip(CLIENT_WIDTH_DIP, dpi),
            bottom: dip(client_height, dpi),
        };
        unsafe { AdjustWindowRectExForDpi(&mut outer, style, 0, extended_style, dpi) };
        let width = outer.right - outer.left;
        let height = outer.bottom - outer.top;
        let (x, y) = centered_position(width, height);
        let language = settings::language().effective();

        let mut state = Box::new(InputState {
            prompt: prompt.into(),
            default: default.into(),
            max_chars,
            cancel_text: i18n::text(language, key::CANCEL).into(),
            confirm_text: i18n::text(language, key::ADD).into(),
            secondary: secondary.map(|(prompt, default, max_chars)| SecondaryInput {
                prompt: prompt.into(),
                default: default.into(),
                max_chars,
                label: null_mut(),
                edit: null_mut(),
            }),
            label: null_mut(),
            edit: null_mut(),
            cancel: null_mut(),
            confirm: null_mut(),
            font: null_mut(),
            done: false,
            result: None,
        });
        let hwnd = unsafe {
            CreateWindowExW(
                extended_style,
                wide(CLASS_NAME).as_ptr(),
                wide(title).as_ptr(),
                style,
                x,
                y,
                width,
                height,
                null_mut(),
                null_mut(),
                instance,
                (&mut *state as *mut InputState).cast(),
            )
        };
        if hwnd.is_null() {
            return None;
        }

        unsafe {
            resize_dialog(hwnd, None, state.secondary.is_some());
            ShowWindow(hwnd, SW_SHOW);
            if IsWindowVisible(hwnd) == 0 {
                ShowWindow(hwnd, SW_SHOW);
            }
            UpdateWindow(hwnd);
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            SetActiveWindow(hwnd);
            if !state.edit.is_null() {
                SetFocus(state.edit);
                SendMessageW(state.edit, EM_SETSEL, 0, -1);
            }
        }

        let mut message = MSG::default();
        loop {
            if state.done {
                break;
            }
            let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
            if result <= 0 {
                break;
            }
            if unsafe { IsDialogMessageW(hwnd, &message) } == 0 {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        if unsafe { IsWindow(hwnd) } != 0 {
            unsafe { DestroyWindow(hwnd) };
        }
        state.result.take()
    }

    pub fn show_error(message: &str) {
        let title = wide("SeeUsage");
        let message = wide(message);
        unsafe {
            MessageBoxW(
                null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR | MB_TOPMOST | MB_SETFOREGROUND,
            );
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCCREATE => {
                let create = lparam as *const CREATESTRUCTW;
                if !create.is_null() {
                    let state = unsafe { (*create).lpCreateParams as *mut InputState };
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                }
            }
            WM_CREATE => {
                if let Some(state) = window_state(hwnd) {
                    create_controls(hwnd, state);
                    layout_controls(hwnd, state);
                    return 0;
                }
            }
            WM_SIZE => {
                if let Some(state) = window_state(hwnd) {
                    layout_controls(hwnd, state);
                }
                return 0;
            }
            WM_DPICHANGED => {
                let recommended = lparam as *const RECT;
                let position = unsafe { recommended.as_ref() }.map(|rect| (rect.left, rect.top));
                let has_secondary =
                    window_state(hwnd).is_some_and(|state| state.secondary.is_some());
                resize_dialog(hwnd, position, has_secondary);
                if let Some(state) = window_state(hwnd) {
                    apply_font(hwnd, state);
                    layout_controls(hwnd, state);
                }
                return 0;
            }
            WM_MOUSEACTIVATE => return MA_ACTIVATE as LRESULT,
            WM_COMMAND => {
                let id = (wparam & 0xffff) as i32;
                if id == IDOK || id == IDCANCEL {
                    if let Some(state) = window_state(hwnd) {
                        if id == IDOK {
                            state.result = read_text(state.edit).and_then(|text| {
                                let secondary = match state.secondary.as_ref() {
                                    Some(secondary) => Some(read_text(secondary.edit)?),
                                    None => None,
                                };
                                Some((text, secondary))
                            });
                        }
                        state.done = true;
                    }
                    unsafe {
                        ShowWindow(hwnd, SW_HIDE);
                        PostMessageW(hwnd, WM_NULL, 0, 0);
                    }
                    return 0;
                }
            }
            WM_CLOSE => {
                if let Some(state) = window_state(hwnd) {
                    state.done = true;
                }
                unsafe {
                    ShowWindow(hwnd, SW_HIDE);
                    PostMessageW(hwnd, WM_NULL, 0, 0);
                }
                return 0;
            }
            WM_NCDESTROY => {
                if let Some(state) = window_state(hwnd) {
                    state.done = true;
                    state.edit = null_mut();
                    if let Some(secondary) = state.secondary.as_mut() {
                        secondary.edit = null_mut();
                    }
                    if !state.font.is_null() {
                        unsafe { DeleteObject(state.font) };
                        state.font = null_mut();
                    }
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            }
            _ => {}
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn register_class() -> bool {
        static REGISTERED: OnceLock<bool> = OnceLock::new();
        *REGISTERED.get_or_init(|| {
            let instance = unsafe { GetModuleHandleW(null()) };
            let class_name = wide(CLASS_NAME);
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
                hbrBackground: (COLOR_WINDOW as usize + 1) as _,
                lpszClassName: class_name.as_ptr(),
                ..WNDCLASSW::default()
            };
            (unsafe { RegisterClassW(&class) } != 0) || unsafe { GetLastError() } == 1410
        })
    }

    fn create_controls(hwnd: HWND, state: &mut InputState) {
        state.label = create_control(
            hwnd,
            "STATIC",
            &state.prompt,
            0,
            WS_CHILD | WS_VISIBLE | SS_SIMPLE,
            100,
        );
        state.edit = create_control(
            hwnd,
            "EDIT",
            &state.default,
            WS_EX_CLIENTEDGE,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            101,
        );
        if let Some(secondary) = state.secondary.as_mut() {
            secondary.label = create_control(
                hwnd,
                "STATIC",
                &secondary.prompt,
                0,
                WS_CHILD | WS_VISIBLE | SS_SIMPLE,
                102,
            );
            secondary.edit = create_control(
                hwnd,
                "EDIT",
                &secondary.default,
                WS_EX_CLIENTEDGE,
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                103,
            );
            unsafe { SendMessageW(secondary.edit, EM_SETLIMITTEXT, secondary.max_chars, 0) };
        }
        state.cancel = create_control(
            hwnd,
            "BUTTON",
            &state.cancel_text,
            0,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
            IDCANCEL,
        );
        state.confirm = create_control(
            hwnd,
            "BUTTON",
            &state.confirm_text,
            0,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            IDOK,
        );
        unsafe { SendMessageW(state.edit, EM_SETLIMITTEXT, state.max_chars, 0) };
        apply_font(hwnd, state);
    }

    fn create_control(
        parent: HWND,
        class: &str,
        text: &str,
        extended_style: u32,
        style: u32,
        id: i32,
    ) -> HWND {
        unsafe {
            CreateWindowExW(
                extended_style,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                style,
                0,
                0,
                1,
                1,
                parent,
                id as usize as _,
                GetModuleHandleW(null()),
                null_mut(),
            )
        }
    }

    fn resize_dialog(hwnd: HWND, position: Option<(i32, i32)>, has_secondary: bool) {
        let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
        let style = WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
        let extended_style = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT | WS_EX_TOOLWINDOW;
        let client_height = if has_secondary {
            DUAL_CLIENT_HEIGHT_DIP
        } else {
            CLIENT_HEIGHT_DIP
        };
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: dip(CLIENT_WIDTH_DIP, dpi),
            bottom: dip(client_height, dpi),
        };
        unsafe { AdjustWindowRectExForDpi(&mut outer, style, 0, extended_style, dpi) };
        let width = outer.right - outer.left;
        let height = outer.bottom - outer.top;
        let (x, y) = position.unwrap_or_else(|| centered_position(width, height));
        unsafe {
            SetWindowPos(
                hwnd,
                null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }

    fn layout_controls(hwnd: HWND, state: &InputState) {
        let mut client = RECT::default();
        unsafe { GetClientRect(hwnd, &mut client) };
        let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
        let layout = dialog_layout(client.right, client.bottom, dpi, state.secondary.is_some());
        for (control, bounds) in [
            (state.label, layout.label),
            (state.edit, layout.edit),
            (state.cancel, layout.cancel),
            (state.confirm, layout.confirm),
        ] {
            unsafe {
                SetWindowPos(
                    control,
                    null_mut(),
                    bounds.left,
                    bounds.top,
                    bounds.right - bounds.left,
                    bounds.bottom - bounds.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
        if let (Some(secondary), Some(label), Some(edit)) = (
            state.secondary.as_ref(),
            layout.secondary_label,
            layout.secondary_edit,
        ) {
            for (control, bounds) in [(secondary.label, label), (secondary.edit, edit)] {
                unsafe {
                    SetWindowPos(
                        control,
                        null_mut(),
                        bounds.left,
                        bounds.top,
                        bounds.right - bounds.left,
                        bounds.bottom - bounds.top,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                }
            }
        }
    }

    fn dialog_layout(
        client_width: i32,
        client_height: i32,
        dpi: u32,
        has_secondary: bool,
    ) -> DialogLayout {
        let margin = dip(20, dpi);
        let button_height = dip(34, dpi);
        let button_top = client_height - margin - button_height;
        let confirm_width = dip(88, dpi);
        let cancel_width = dip(84, dpi);
        let gap = dip(10, dpi);
        let confirm = RECT {
            left: client_width - margin - confirm_width,
            top: button_top,
            right: client_width - margin,
            bottom: button_top + button_height,
        };
        DialogLayout {
            label: RECT {
                left: margin,
                top: dip(18, dpi),
                right: client_width - margin,
                bottom: dip(42, dpi),
            },
            edit: RECT {
                left: margin,
                top: dip(50, dpi),
                right: client_width - margin,
                bottom: dip(82, dpi),
            },
            secondary_label: has_secondary.then_some(RECT {
                left: margin,
                top: dip(98, dpi),
                right: client_width - margin,
                bottom: dip(122, dpi),
            }),
            secondary_edit: has_secondary.then_some(RECT {
                left: margin,
                top: dip(130, dpi),
                right: client_width - margin,
                bottom: dip(162, dpi),
            }),
            cancel: RECT {
                left: confirm.left - gap - cancel_width,
                top: button_top,
                right: confirm.left - gap,
                bottom: button_top + button_height,
            },
            confirm,
        }
    }

    fn apply_font(hwnd: HWND, state: &mut InputState) {
        let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
        let face = wide("Segoe UI");
        let font = unsafe {
            CreateFontW(
                -dip(10, dpi),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                (DEFAULT_PITCH | FF_DONTCARE) as u32,
                face.as_ptr(),
            )
        };
        if font.is_null() {
            return;
        }
        for control in [state.label, state.edit, state.cancel, state.confirm] {
            unsafe { SendMessageW(control, WM_SETFONT, font as usize, 1) };
        }
        if let Some(secondary) = state.secondary.as_ref() {
            for control in [secondary.label, secondary.edit] {
                unsafe { SendMessageW(control, WM_SETFONT, font as usize, 1) };
            }
        }
        if !state.font.is_null() {
            unsafe { DeleteObject(state.font) };
        }
        state.font = font;
    }

    fn read_text(hwnd: HWND) -> Option<String> {
        if hwnd.is_null() {
            return None;
        }
        let length = unsafe { GetWindowTextLengthW(hwnd) };
        if length < 0 {
            return None;
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        Some(String::from_utf16_lossy(&buffer[..copied as usize]))
    }

    fn centered_position(width: i32, height: i32) -> (i32, i32) {
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) };
        let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return (0, 0);
        }
        (
            info.rcWork.left + (info.rcWork.right - info.rcWork.left - width) / 2,
            info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - height) / 2,
        )
    }

    fn window_state(hwnd: HWND) -> Option<&'static mut InputState> {
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut InputState;
        unsafe { pointer.as_mut() }
    }

    fn dip(value: i32, dpi: u32) -> i32 {
        (value * dpi as i32 + 48) / 96
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain([0]).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn dialog_controls_fit_at_the_monitor_dpi() {
            let dpi = 144;
            let width = dip(CLIENT_WIDTH_DIP, dpi);
            let height = dip(CLIENT_HEIGHT_DIP, dpi);
            let layout = dialog_layout(width, height, dpi, false);

            for bounds in [layout.label, layout.edit, layout.cancel, layout.confirm] {
                assert!(bounds.left >= 0);
                assert!(bounds.top >= 0);
                assert!(bounds.right <= width);
                assert!(bounds.bottom <= height);
            }
        }

        #[test]
        fn reminder_dialog_fields_and_buttons_fit_at_the_monitor_dpi() {
            let dpi = 144;
            let width = dip(CLIENT_WIDTH_DIP, dpi);
            let height = dip(DUAL_CLIENT_HEIGHT_DIP, dpi);
            let layout = dialog_layout(width, height, dpi, true);
            let secondary_label = layout.secondary_label.expect("secondary label");
            let secondary_edit = layout.secondary_edit.expect("secondary edit");

            for bounds in [
                layout.label,
                layout.edit,
                secondary_label,
                secondary_edit,
                layout.cancel,
                layout.confirm,
            ] {
                assert!(bounds.left >= 0);
                assert!(bounds.top >= 0);
                assert!(bounds.right <= width);
                assert!(bounds.bottom <= height);
            }
            assert!(layout.edit.bottom < secondary_label.top);
            assert!(secondary_edit.bottom < layout.confirm.top);
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use crate::{
        i18n::{self, key},
        settings,
    };
    use std::process::Command;

    const PROMPT_SCRIPT: &str = r#"on run argv
try
set cancelLabel to item 4 of argv
set addLabel to item 5 of argv
set answer to display dialog (item 1 of argv) with title (item 2 of argv) default answer (item 3 of argv) buttons {cancelLabel, addLabel} default button addLabel cancel button cancelLabel
return text returned of answer
on error number -128
return "__SEEUSAGE_CANCEL__"
end try
end run"#;

    pub fn prompt_text(
        title: &str,
        prompt: &str,
        default: &str,
        max_chars: usize,
    ) -> Option<String> {
        let language = settings::language().effective();
        let output = Command::new("/usr/bin/osascript")
            .args([
                "-e",
                PROMPT_SCRIPT,
                "--",
                prompt,
                title,
                default,
                i18n::text(language, key::CANCEL),
                i18n::text(language, key::ADD),
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\r', '\n'])
            .to_string();
        if !output.status.success() || text == "__SEEUSAGE_CANCEL__" {
            return None;
        }
        Some(text.chars().take(max_chars.saturating_add(1)).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prompt_reminder(
        title: &str,
        text_prompt: &str,
        text_default: &str,
        text_max_chars: usize,
        schedule_prompt: &str,
        schedule_default: &str,
        schedule_max_chars: usize,
    ) -> Option<(String, String)> {
        let text = prompt_text(title, text_prompt, text_default, text_max_chars)?;
        let schedule = prompt_text(title, schedule_prompt, schedule_default, schedule_max_chars)?;
        Some((text, schedule))
    }

    pub fn show_error(message: &str) {
        const SCRIPT: &str = r#"on run argv
set okLabel to item 2 of argv
display dialog (item 1 of argv) with title "SeeUsage" buttons {okLabel} default button okLabel with icon stop
end run"#;
        let language = settings::language().effective();
        let _ = Command::new("/usr/bin/osascript")
            .args(["-e", SCRIPT, "--", message, i18n::text(language, key::OK)])
            .status();
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    pub fn prompt_text(_: &str, _: &str, _: &str, _: usize) -> Option<String> {
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prompt_reminder(
        _: &str,
        _: &str,
        _: &str,
        _: usize,
        _: &str,
        _: &str,
        _: usize,
    ) -> Option<(String, String)> {
        None
    }

    pub fn show_error(message: &str) {
        eprintln!("SeeUsage: {message}");
    }
}

pub use platform::{prompt_reminder, prompt_text, show_error};
