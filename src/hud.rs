use crate::format::{DisplayState, ProviderVisibility};

#[cfg(target_os = "windows")]
mod platform {
    use super::{DisplayState, ProviderVisibility};
    use crate::{
        personal::{HudItem, HudItemKind},
        settings::{self, HudPosition},
    };
    use std::{
        cell::{Cell, RefCell},
        ffi::c_void,
        mem::size_of,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{COLORREF, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
        Graphics::{
            Dwm::{
                DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
                DWMWCP_DONOTROUND, DwmSetWindowAttribute,
            },
            Gdi::{
                AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO,
                BITMAPINFOHEADER, BLENDFUNCTION, BeginPaint, CLIP_DEFAULT_PRECIS,
                CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateRoundRectRgn,
                CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_END_ELLIPSIS,
                DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
                DrawTextW, EndPaint, FF_DONTCARE, FillRgn, GRADIENT_FILL_RECT_V, GRADIENT_RECT,
                GetDC, GetMonitorInfoW, GradientFill, HDC, HFONT, InvalidateRect,
                MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromRect,
                MonitorFromWindow, OUT_DEFAULT_PRECIS, PAINTSTRUCT, ReleaseDC, RestoreDC, SaveDC,
                SelectObject, SetBkMode, SetTextColor, SetViewportOrgEx, TRANSPARENT, TRIVERTEX,
                UpdateWindow,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::WM_MOUSELEAVE,
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent},
            WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE,
                GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, GetWindowRect, HTCAPTION,
                HTCLIENT, HWND_NOTOPMOST, HWND_TOPMOST, IDC_HAND, IDC_SIZEALL, LoadCursorW,
                MA_NOACTIVATE, RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
                SetCursor, SetWindowLongPtrW, SetWindowPos, ShowWindow, ULW_ALPHA,
                UpdateLayeredWindow, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENTERSIZEMOVE,
                WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
                WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
                WM_NCRBUTTONUP, WM_PAINT, WM_PRINTCLIENT, WM_RBUTTONUP, WM_SETCURSOR,
                WM_SETTINGCHANGE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
                WS_EX_TOPMOST, WS_POPUP,
            },
        },
    };

    const CLASS_NAME: &str = "SeeUsage.DesktopMeter";
    const BASE_WIDTH_DIP: i32 = 332;
    const HEADER_HEIGHT_DIP: i32 = 58;
    const BOTTOM_PADDING_DIP: i32 = 12;
    const DEFAULT_ROW_HEIGHT_DIP: i32 = 26;
    const MIN_ROW_HEIGHT_DIP: i32 = 18;
    const COLUMN_WIDTH_DIP: i32 = 220;
    const MIN_COLUMN_WIDTH_DIP: i32 = 140;
    const COLUMN_GAP_DIP: i32 = 12;
    const CONTENT_PADDING_DIP: i32 = 12;
    const MARGIN_DIP: i32 = 10;
    const SHADOW_PADDING_DIP: i32 = 8;
    const SURFACE_RADIUS_DIP: i32 = 24;
    const RENDER_SCALE: i32 = 2;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct PanelLayout {
        width: i32,
        height: i32,
        columns: usize,
        rows: usize,
        row_height: i32,
    }

    struct HudState {
        codex: Cell<Option<u8>>,
        minimax: Cell<Option<u8>>,
        codex_stale: Cell<bool>,
        minimax_stale: Cell<bool>,
        show_codex: Cell<bool>,
        show_minimax: Cell<bool>,
        position: Cell<Option<HudPosition>>,
        pinned: Cell<bool>,
        layout: Cell<PanelLayout>,
        hovered: Cell<bool>,
        dragging: Cell<bool>,
        items: RefCell<Vec<HudItem>>,
        clock: RefCell<String>,
        show_context_menu: Box<dyn Fn(Option<u64>)>,
        acknowledge_reminder: Box<dyn Fn(u64)>,
    }

    pub struct Hud {
        hwnd: HWND,
        _state: Box<HudState>,
    }

    impl Hud {
        pub fn new(
            display: &DisplayState,
            visibility: ProviderVisibility,
            show_context_menu: impl Fn(Option<u64>) + 'static,
            acknowledge_reminder: impl Fn(u64) + 'static,
        ) -> Result<Self, String> {
            let instance = unsafe { GetModuleHandleW(null()) };
            if instance.is_null() {
                return Err(last_error("could not get the application module"));
            }

            let class_name = wide(CLASS_NAME);
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                hCursor: unsafe { LoadCursorW(null_mut(), IDC_SIZEALL) },
                lpszClassName: class_name.as_ptr(),
                ..WNDCLASSW::default()
            };
            if unsafe { RegisterClassW(&window_class) } == 0 {
                return Err(last_error("could not register the desktop meter"));
            }

            let mut state = Box::new(HudState {
                codex: Cell::new(display.codex_remaining),
                minimax: Cell::new(display.minimax_remaining),
                codex_stale: Cell::new(display.codex_stale),
                minimax_stale: Cell::new(display.minimax_stale),
                show_codex: Cell::new(visibility.codex),
                show_minimax: Cell::new(visibility.minimax),
                position: Cell::new(settings::load_hud_position()),
                pinned: Cell::new(settings::hud_pinned()),
                layout: Cell::new(PanelLayout {
                    width: BASE_WIDTH_DIP,
                    height: HEADER_HEIGHT_DIP + 6,
                    columns: 1,
                    rows: 0,
                    row_height: DEFAULT_ROW_HEIGHT_DIP,
                }),
                hovered: Cell::new(false),
                dragging: Cell::new(false),
                items: RefCell::new(Vec::new()),
                clock: RefCell::new(String::new()),
                show_context_menu: Box::new(show_context_menu),
                acknowledge_reminder: Box::new(acknowledge_reminder),
            });
            let title = wide("SeeUsage");
            let extended_style = WS_EX_LAYERED
                | WS_EX_NOACTIVATE
                | WS_EX_TOOLWINDOW
                | if state.pinned.get() { WS_EX_TOPMOST } else { 0 };
            let hwnd = unsafe {
                CreateWindowExW(
                    extended_style,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    null_mut(),
                    null_mut(),
                    instance,
                    (&mut *state as *mut HudState).cast(),
                )
            };
            if hwnd.is_null() {
                return Err(last_error("could not create the desktop meter"));
            }

            // Per-pixel alpha owns the silhouette, so DWM must not add a second corner treatment.
            let corner_preference = DWMWCP_DONOTROUND;
            let border_color = DWMWA_COLOR_NONE;
            let dark_mode = 1i32;
            unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                    (&corner_preference as *const i32).cast(),
                    size_of::<i32>() as u32,
                );
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_BORDER_COLOR as u32,
                    (&border_color as *const u32).cast(),
                    size_of::<u32>() as u32,
                );
                DwmSetWindowAttribute(
                    hwnd,
                    20,
                    (&dark_mode as *const i32).cast(),
                    size_of::<i32>() as u32,
                );
            }
            if !position(hwnd, &state) {
                unsafe { DestroyWindow(hwnd) };
                return Err(last_error("could not position the desktop meter"));
            }
            unsafe {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                UpdateWindow(hwnd);
            }
            Ok(Self {
                hwnd,
                _state: state,
            })
        }

        pub fn update(&self, display: &DisplayState) {
            self._state.codex.set(display.codex_remaining);
            self._state.minimax.set(display.minimax_remaining);
            self._state.codex_stale.set(display.codex_stale);
            self._state.minimax_stale.set(display.minimax_stale);
            if !self._state.dragging.get() {
                unsafe {
                    InvalidateRect(self.hwnd, null(), 0);
                }
            }
        }

        pub fn update_personal(&self, items: &[HudItem], clock: &str) {
            let previous_count = self._state.items.borrow().len();
            self._state.items.replace(items.to_vec());
            self._state.clock.replace(clock.to_string());
            let dragging = self._state.dragging.get();
            if should_relayout(previous_count, items.len(), dragging) {
                let _ = position(self.hwnd, &self._state);
            } else if !dragging {
                unsafe {
                    InvalidateRect(self.hwnd, null(), 0);
                }
            }
        }

        pub fn set_provider_visibility(&self, visibility: ProviderVisibility) {
            self._state.show_codex.set(visibility.codex);
            self._state.show_minimax.set(visibility.minimax);
            if !self._state.dragging.get() {
                unsafe { InvalidateRect(self.hwnd, null(), 0) };
            }
        }

        pub fn hwnd(&self) -> isize {
            self.hwnd as isize
        }

        pub fn set_context_menu_active(&self, active: bool) {
            let style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) };
            let no_activate = WS_EX_NOACTIVATE as isize;
            let updated = if active {
                style & !no_activate
            } else {
                style | no_activate
            };
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, updated);
            }
        }

        pub fn set_visible(&self, visible: bool) {
            if visible {
                position(self.hwnd, &self._state);
            }
            unsafe {
                ShowWindow(self.hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }

        pub fn reset_position(&self) {
            self._state.position.set(None);
            let _ = settings::clear_hud_position();
            position(self.hwnd, &self._state);
        }

        pub fn set_pinned(&self, pinned: bool) {
            self._state.pinned.set(pinned);
            let _ = position(self.hwnd, &self._state);
        }
    }

    impl Drop for Hud {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.hwnd);
            }
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
                    let state = unsafe { (*create).lpCreateParams as *mut HudState };
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
                }
            }
            WM_NCDESTROY => {
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            }
            WM_ERASEBKGND => return 1,
            WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
            WM_NCMOUSEMOVE | WM_MOUSEMOVE => {
                if let Some(state) = window_state(hwnd) {
                    set_hovered(hwnd, state, true);
                    track_pointer_leave(hwnd, message == WM_NCMOUSEMOVE);
                }
            }
            WM_NCMOUSELEAVE | WM_MOUSELEAVE => {
                if let Some(state) = window_state(hwnd) {
                    set_hovered(hwnd, state, false);
                }
            }
            WM_LBUTTONUP => {
                if let Some(state) = window_state(hwnd) {
                    let (x, y) = point_from_lparam(lparam);
                    if let Some(hit) = reminder_at_client_point(hwnd, state, x, y)
                        && hit.due
                    {
                        (state.acknowledge_reminder)(hit.id);
                    }
                }
                return 0;
            }
            WM_RBUTTONUP => {
                if let Some(state) = window_state(hwnd) {
                    let (x, y) = point_from_lparam(lparam);
                    let reminder_id = reminder_at_client_point(hwnd, state, x, y).map(|hit| hit.id);
                    (state.show_context_menu)(reminder_id);
                }
                return 0;
            }
            WM_NCRBUTTONUP => {
                if let Some(state) = window_state(hwnd) {
                    (state.show_context_menu)(None);
                }
                return 0;
            }
            WM_NCHITTEST => {
                if let Some(state) = window_state(hwnd)
                    && reminder_at_screen_point(hwnd, state, lparam).is_some()
                {
                    return HTCLIENT as LRESULT;
                }
                return HTCAPTION as LRESULT;
            }
            WM_SETCURSOR => {
                if lparam as u32 & 0xffff == HTCLIENT {
                    unsafe { SetCursor(LoadCursorW(null_mut(), IDC_HAND)) };
                    return 1;
                }
            }
            WM_ENTERSIZEMOVE => {
                if let Some(state) = window_state(hwnd) {
                    state.dragging.set(true);
                }
                return 0;
            }
            WM_EXITSIZEMOVE => {
                if let Some(state) = window_state(hwnd) {
                    state.dragging.set(false);
                    remember_position(hwnd, state);
                    position(hwnd, state);
                    persist_position(hwnd, state);
                }
                return 0;
            }
            WM_PAINT => {
                if let Some(state) = window_state(hwnd) {
                    paint(hwnd, state);
                    return 0;
                }
            }
            WM_PRINTCLIENT => {
                if let Some(state) = window_state(hwnd) {
                    draw(hwnd, state, wparam as HDC);
                    return 0;
                }
            }
            WM_DPICHANGED => {
                if let Some(state) = window_state(hwnd) {
                    let recommended = lparam as *const RECT;
                    if let Some(recommended) = unsafe { recommended.as_ref() } {
                        state.position.set(Some(HudPosition {
                            x: recommended.left,
                            y: recommended.top,
                        }));
                    }
                    position(hwnd, state);
                }
                return 0;
            }
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
                if let Some(state) = window_state(hwnd) {
                    position(hwnd, state);
                }
                return 0;
            }
            _ => {}
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn window_state(hwnd: HWND) -> Option<&'static HudState> {
        let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const HudState;
        unsafe { state.as_ref() }
    }

    #[derive(Clone, Copy)]
    struct ReminderHit {
        id: u64,
        due: bool,
    }

    fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
        let x = (lparam as u16) as i16 as i32;
        let y = ((lparam as u32 >> 16) as u16) as i16 as i32;
        (x, y)
    }

    fn reminder_at_screen_point(
        hwnd: HWND,
        state: &HudState,
        lparam: LPARAM,
    ) -> Option<ReminderHit> {
        let (screen_x, screen_y) = point_from_lparam(lparam);
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
            return None;
        }
        reminder_at_client_point(hwnd, state, screen_x - window.left, screen_y - window.top)
    }

    fn reminder_at_client_point(
        hwnd: HWND,
        state: &HudState,
        x: i32,
        y: i32,
    ) -> Option<ReminderHit> {
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) } == 0 {
            return None;
        }
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let shadow_padding = dip(SHADOW_PADDING_DIP, dpi);
        let surface_x = x - shadow_padding;
        let surface_y = y - shadow_padding;
        let width = client.right - client.left - shadow_padding * 2;
        let height = client.bottom - client.top - shadow_padding * 2;
        if surface_x < 0 || surface_y < 0 || surface_x >= width || surface_y >= height {
            return None;
        }

        let layout = state.layout.get();
        state
            .items
            .borrow()
            .iter()
            .enumerate()
            .find_map(|(index, item)| {
                let id = item.reminder_id?;
                let bounds = item_bounds(layout, index, width, height, dpi)?;
                (surface_x >= bounds.left
                    && surface_x < bounds.right
                    && surface_y >= bounds.top
                    && surface_y < bounds.bottom)
                    .then_some(ReminderHit { id, due: item.due })
            })
    }

    fn set_hovered(hwnd: HWND, state: &HudState, hovered: bool) {
        if state.hovered.replace(hovered) == hovered {
            return;
        }
        if !state.dragging.get() {
            unsafe { InvalidateRect(hwnd, null(), 0) };
        }
    }

    fn track_pointer_leave(hwnd: HWND, non_client: bool) {
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE | if non_client { TME_NONCLIENT } else { 0 },
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            TrackMouseEvent(&mut tracking);
        }
    }

    fn should_relayout(previous_count: usize, next_count: usize, dragging: bool) -> bool {
        !dragging && previous_count != next_count
    }

    fn position(hwnd: HWND, state: &HudState) -> bool {
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let probe_width = dip(BASE_WIDTH_DIP + SHADOW_PADDING_DIP * 2, dpi);
        let probe_height = dip(
            HEADER_HEIGHT_DIP + BOTTOM_PADDING_DIP + SHADOW_PADDING_DIP * 2,
            dpi,
        );
        let margin = dip(MARGIN_DIP, dpi);
        let saved = state.position.get();
        let monitor = match saved {
            Some(position) => {
                let target = RECT {
                    left: position.x,
                    top: position.y,
                    right: position.x + probe_width,
                    bottom: position.y + probe_height,
                };
                unsafe { MonitorFromRect(&target, MONITOR_DEFAULTTONEAREST) }
            }
            None => unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY) },
        };
        let mut monitor_info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut monitor_info) } == 0 {
            return false;
        }

        let work_width = undip(monitor_info.rcWork.right - monitor_info.rcWork.left, dpi);
        let work_height = undip(monitor_info.rcWork.bottom - monitor_info.rcWork.top, dpi);
        let layout = panel_layout(
            state.items.borrow().len(),
            work_width - SHADOW_PADDING_DIP * 2,
            work_height - SHADOW_PADDING_DIP * 2,
        );
        state.layout.set(layout);
        let shadow_padding = dip(SHADOW_PADDING_DIP, dpi);
        let width = dip(layout.width, dpi) + shadow_padding * 2;
        let height = dip(layout.height, dpi) + shadow_padding * 2;

        let default_x = monitor_info.rcWork.right - width - margin;
        let default_y = monitor_info.rcWork.bottom - height - margin;
        let x = saved.map_or(default_x, |position| {
            clamp(
                position.x,
                monitor_info.rcWork.left,
                monitor_info.rcWork.right - width,
            )
        });
        let y = saved.map_or(default_y, |position| {
            clamp(
                position.y,
                monitor_info.rcWork.top,
                monitor_info.rcWork.bottom - height,
            )
        });
        if saved.is_some() {
            state.position.set(Some(HudPosition { x, y }));
        }
        let z_order = if state.pinned.get() {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        if unsafe { SetWindowPos(hwnd, z_order, x, y, width, height, SWP_NOACTIVATE) } == 0 {
            return false;
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };

        true
    }

    fn panel_layout(item_count: usize, work_width: i32, work_height: i32) -> PanelLayout {
        let available_width = (work_width - MARGIN_DIP * 2).max(1);
        let available_height = (work_height - MARGIN_DIP * 2).max(1);
        if item_count == 0 {
            return PanelLayout {
                width: BASE_WIDTH_DIP.min(available_width),
                height: (HEADER_HEIGHT_DIP + 6).min(available_height),
                columns: 1,
                rows: 0,
                row_height: DEFAULT_ROW_HEIGHT_DIP,
            };
        }

        let preferred_columns = match item_count {
            0..=7 => 1,
            8..=16 => 2,
            17..=30 => 3,
            _ => 4,
        };
        let max_columns = ((available_width - CONTENT_PADDING_DIP * 2 + COLUMN_GAP_DIP)
            / (MIN_COLUMN_WIDTH_DIP + COLUMN_GAP_DIP))
            .max(1) as usize;
        let max_rows = ((available_height - HEADER_HEIGHT_DIP - BOTTOM_PADDING_DIP)
            / MIN_ROW_HEIGHT_DIP)
            .max(1) as usize;
        let required_columns = item_count.div_ceil(max_rows);
        let columns = preferred_columns
            .max(required_columns)
            .min(max_columns)
            .min(item_count)
            .max(1);
        let rows = item_count.div_ceil(columns);
        let row_height = ((available_height - HEADER_HEIGHT_DIP - BOTTOM_PADDING_DIP)
            / rows as i32)
            .clamp(MIN_ROW_HEIGHT_DIP, DEFAULT_ROW_HEIGHT_DIP);
        let desired_width = CONTENT_PADDING_DIP * 2
            + columns as i32 * COLUMN_WIDTH_DIP
            + (columns.saturating_sub(1)) as i32 * COLUMN_GAP_DIP;
        let width = desired_width
            .max(BASE_WIDTH_DIP)
            .min(available_width)
            .max(1);
        let height = (HEADER_HEIGHT_DIP + rows as i32 * row_height + BOTTOM_PADDING_DIP)
            .min(available_height)
            .max(1);

        PanelLayout {
            width,
            height,
            columns,
            rows,
            row_height,
        }
    }

    fn item_bounds(
        layout: PanelLayout,
        index: usize,
        width: i32,
        height: i32,
        dpi: u32,
    ) -> Option<RECT> {
        if layout.rows == 0 || index >= layout.rows.saturating_mul(layout.columns) {
            return None;
        }
        let column = index / layout.rows;
        let row = index % layout.rows;
        if column >= layout.columns {
            return None;
        }
        let content_left = dip(CONTENT_PADDING_DIP, dpi);
        let content_right = width - content_left;
        let column_gap = dip(COLUMN_GAP_DIP, dpi);
        let total_gap = layout.columns.saturating_sub(1) as i32 * column_gap;
        let column_width = (content_right - content_left - total_gap) / layout.columns as i32;
        let row_height = dip(layout.row_height, dpi);
        let left = content_left + column as i32 * (column_width + column_gap);
        let top = dip(HEADER_HEIGHT_DIP, dpi).min(height) + row as i32 * row_height;
        Some(RECT {
            left,
            top,
            right: left + column_width,
            bottom: (top + row_height).min(height - dip(BOTTOM_PADDING_DIP, dpi)),
        })
    }

    fn material_alpha(state: &HudState) -> u8 {
        let item_count = state.items.borrow().len();
        let base = match item_count {
            0 => 168u8,
            1..=7 => 184,
            8..=16 => 192,
            _ => 200,
        };
        base.saturating_add(if state.hovered.get() { 8 } else { 0 })
    }

    fn remember_position(hwnd: HWND, state: &HudState) {
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } != 0 {
            state.position.set(Some(HudPosition {
                x: window.left,
                y: window.top,
            }));
        }
    }

    fn persist_position(hwnd: HWND, state: &HudState) {
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } != 0 {
            let position = HudPosition {
                x: window.left,
                y: window.top,
            };
            state.position.set(Some(position));
            let _ = settings::save_hud_position(position);
        }
    }

    fn clamp(value: i32, minimum: i32, maximum: i32) -> i32 {
        if maximum < minimum {
            minimum
        } else {
            value.clamp(minimum, maximum)
        }
    }

    fn paint(hwnd: HWND, state: &HudState) {
        let mut paint = PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
        if hdc.is_null() {
            return;
        }
        if !render_layered(hwnd, state, hdc) {
            draw(hwnd, state, hdc);
        }
        unsafe { EndPaint(hwnd, &paint) };
    }

    fn render_layered(hwnd: HWND, state: &HudState, reference: HDC) -> bool {
        let mut client = RECT::default();
        unsafe { GetClientRect(hwnd, &mut client) };
        let width = client.right - client.left;
        let height = client.bottom - client.top;
        if width <= 0 || height <= 0 {
            return false;
        }

        let source_width = width.saturating_mul(RENDER_SCALE);
        let source_height = height.saturating_mul(RENDER_SCALE);
        let source_dc = unsafe { CreateCompatibleDC(reference) };
        if source_dc.is_null() {
            return false;
        }
        let Some((source_bitmap, source_bits)) =
            create_argb_bitmap(reference, source_width, source_height)
        else {
            unsafe { DeleteDC(source_dc) };
            return false;
        };
        let source_previous = unsafe { SelectObject(source_dc, source_bitmap) };
        unsafe {
            std::ptr::write_bytes(
                source_bits,
                0,
                (source_width as usize).saturating_mul(source_height as usize),
            )
        };
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        draw_scaled(
            state,
            source_dc,
            source_width,
            source_height,
            dpi.saturating_mul(RENDER_SCALE as u32),
        );

        let output_dc = unsafe { CreateCompatibleDC(reference) };
        let output = if output_dc.is_null() {
            None
        } else {
            create_argb_bitmap(reference, width, height)
        };
        let updated = if let Some((output_bitmap, output_bits)) = output {
            let output_previous = unsafe { SelectObject(output_dc, output_bitmap) };
            let source = unsafe {
                std::slice::from_raw_parts(
                    source_bits,
                    (source_width as usize).saturating_mul(source_height as usize),
                )
            };
            let destination = unsafe {
                std::slice::from_raw_parts_mut(
                    output_bits,
                    (width as usize).saturating_mul(height as usize),
                )
            };
            composite_glass(
                source,
                source_width,
                destination,
                width,
                height,
                dpi,
                material_alpha(state),
                state.hovered.get(),
            );

            let mut window = RECT::default();
            unsafe { GetWindowRect(hwnd, &mut window) };
            let destination_point = POINT {
                x: window.left,
                y: window.top,
            };
            let source_point = POINT::default();
            let size = SIZE {
                cx: width,
                cy: height,
            };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let screen = unsafe { GetDC(null_mut()) };
            let result = !screen.is_null()
                && unsafe {
                    UpdateLayeredWindow(
                        hwnd,
                        screen,
                        &destination_point,
                        &size,
                        output_dc,
                        &source_point,
                        0,
                        &blend,
                        ULW_ALPHA,
                    )
                } != 0;
            if !screen.is_null() {
                unsafe { ReleaseDC(null_mut(), screen) };
            }
            unsafe {
                SelectObject(output_dc, output_previous);
                DeleteObject(output_bitmap);
            }
            result
        } else {
            false
        };

        if !output_dc.is_null() {
            unsafe { DeleteDC(output_dc) };
        }
        unsafe {
            SelectObject(source_dc, source_previous);
            DeleteObject(source_bitmap);
            DeleteDC(source_dc);
        }
        updated
    }

    fn create_argb_bitmap(
        reference: HDC,
        width: i32,
        height: i32,
    ) -> Option<(*mut c_void, *mut u32)> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: (width as u32)
                    .saturating_mul(height as u32)
                    .saturating_mul(4),
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bits = null_mut();
        let bitmap =
            unsafe { CreateDIBSection(reference, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
        (!bitmap.is_null() && !bits.is_null()).then_some((bitmap, bits.cast()))
    }

    #[allow(clippy::too_many_arguments)]
    fn composite_glass(
        source: &[u32],
        source_width: i32,
        destination: &mut [u32],
        width: i32,
        height: i32,
        dpi: u32,
        material_alpha: u8,
        hovered: bool,
    ) {
        let padding = dip(SHADOW_PADDING_DIP, dpi) as f32;
        let radius = dip(SURFACE_RADIUS_DIP, dpi) as f32;
        let surface = (
            padding,
            padding,
            width as f32 - padding,
            height as f32 - padding,
        );
        let scale = RENDER_SCALE as usize;
        let samples = (scale * scale) as u32;
        let shadow_blur = padding.max(1.0);
        let shadow_offset = dip(2, dpi) as f32;

        for y in 0..height as usize {
            for x in 0..width as usize {
                let mut blue = 0u32;
                let mut green = 0u32;
                let mut red = 0u32;
                for sample_y in 0..scale {
                    let row = (y * scale + sample_y) * source_width as usize;
                    for sample_x in 0..scale {
                        let pixel = source[row + x * scale + sample_x];
                        blue += pixel & 0xff;
                        green += (pixel >> 8) & 0xff;
                        red += (pixel >> 16) & 0xff;
                    }
                }
                let mut blue = (blue / samples) as f32;
                let mut green = (green / samples) as f32;
                let mut red = (red / samples) as f32;
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let distance = rounded_rect_distance(px, py, surface, radius);
                let coverage = (0.5 - distance).clamp(0.0, 1.0);

                let vertical = ((py - surface.1) / (surface.3 - surface.1)).clamp(0.0, 1.0);
                let base_red = 35.0 + (22.0 - 35.0) * vertical;
                let base_green = 39.0 + (25.0 - 39.0) * vertical;
                let base_blue = 47.0 + (31.0 - 47.0) * vertical;
                let content_delta = (red - base_red)
                    .max(green - base_green)
                    .max(blue - base_blue);
                let content_strength = ((content_delta - 5.0) / 52.0).clamp(0.0, 1.0);

                let shifted_surface = (
                    surface.0,
                    surface.1 + shadow_offset,
                    surface.2,
                    surface.3 + shadow_offset,
                );
                let shadow_distance = rounded_rect_distance(px, py, shifted_surface, radius);
                let shadow_strength =
                    (1.0 - shadow_distance.max(0.0) / shadow_blur).clamp(0.0, 1.0);
                let shadow_alpha = 40.0 * shadow_strength * shadow_strength * (1.0 - coverage);

                if coverage > 0.0 {
                    let edge_depth = (-distance).max(0.0);
                    let outer_rim = (1.0 - edge_depth / 2.4).clamp(0.0, 1.0);
                    let inner_rim = (1.0 - (edge_depth - 3.8).abs() / 2.8).clamp(0.0, 1.0);
                    let horizontal = ((px - surface.0) / (surface.2 - surface.0)).clamp(0.0, 1.0);
                    let light_direction = (1.0 - horizontal) * 0.35 + (1.0 - vertical) * 0.65;
                    let top_sheet = (1.0 - (py - surface.1) / 9.0).clamp(0.0, 1.0)
                        * (0.3 + (1.0 - horizontal) * 0.7);
                    let highlight = outer_rim
                        * (10.0 + light_direction * if hovered { 48.0 } else { 40.0 })
                        + inner_rim * light_direction * if hovered { 15.0 } else { 11.0 }
                        + top_sheet * if hovered { 13.0 } else { 9.0 };
                    let shade = outer_rim * (horizontal * 0.3 + vertical * 0.7) * 12.0;
                    let veil = (1.0 - vertical) * 3.5;
                    blue = (blue + highlight * 1.1 + veil * 1.18 - shade).clamp(0.0, 255.0);
                    green = (green + highlight + veil - shade).clamp(0.0, 255.0);
                    red = (red + highlight * 0.92 + veil * 0.82 - shade).clamp(0.0, 255.0);
                }

                let content_alpha =
                    material_alpha as f32 + (248.0 - material_alpha as f32) * content_strength;
                let edge_alpha = if distance > -2.4 {
                    (1.0 - (-distance).max(0.0) / 2.4).clamp(0.0, 1.0)
                        * if hovered { 28.0 } else { 22.0 }
                } else {
                    0.0
                };
                let surface_alpha = (content_alpha + edge_alpha).min(252.0) * coverage;
                let total_alpha = surface_alpha + shadow_alpha * (1.0 - surface_alpha / 255.0);
                let alpha = total_alpha.round().clamp(0.0, 255.0) as u32;
                let premultiply = |channel: f32| -> u32 {
                    (channel * surface_alpha / 255.0).round().clamp(0.0, 255.0) as u32
                };
                destination[y * width as usize + x] = (alpha << 24)
                    | (premultiply(red) << 16)
                    | (premultiply(green) << 8)
                    | premultiply(blue);
            }
        }
    }

    fn rounded_rect_distance(x: f32, y: f32, bounds: (f32, f32, f32, f32), radius: f32) -> f32 {
        let half_width = (bounds.2 - bounds.0) * 0.5;
        let half_height = (bounds.3 - bounds.1) * 0.5;
        let radius = radius.min(half_width).min(half_height).max(0.0);
        let center_x = (bounds.0 + bounds.2) * 0.5;
        let center_y = (bounds.1 + bounds.3) * 0.5;
        let qx = (x - center_x).abs() - (half_width - radius);
        let qy = (y - center_y).abs() - (half_height - radius);
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
    }

    fn draw(hwnd: HWND, state: &HudState, hdc: HDC) {
        let mut bounds = RECT::default();
        unsafe { GetClientRect(hwnd, &mut bounds) };
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        draw_scaled(
            state,
            hdc,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            dpi,
        );
    }

    fn draw_scaled(state: &HudState, hdc: HDC, total_width: i32, total_height: i32, dpi: u32) {
        let shadow_padding = dip(SHADOW_PADDING_DIP, dpi);
        let width = (total_width - shadow_padding * 2).max(1);
        let height = (total_height - shadow_padding * 2).max(1);
        let bounds = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let saved = unsafe { SaveDC(hdc) };
        unsafe { SetViewportOrgEx(hdc, shadow_padding, shadow_padding, null_mut()) };
        gradient(hdc, &bounds, rgb(35, 39, 47), rgb(22, 25, 31));
        unsafe { SetBkMode(hdc, TRANSPARENT as i32) };

        let header_height = dip(HEADER_HEIGHT_DIP, dpi).min(height);

        let label_font = create_font(9, 600, dpi);
        let value_font = create_font(17, 600, dpi);
        let clock_font = create_font(10, 500, dpi);
        let layout = state.layout.get();
        let item_font = create_font(if layout.row_height <= 19 { 9 } else { 10 }, 500, dpi);

        let padding = dip(16, dpi);
        let clock_width = dip(52, dpi);
        let clock_gap = dip(14, dpi);
        let meter_gap = dip(18, dpi);
        let meter_area_width = (width - padding * 2 - clock_width - clock_gap).max(dip(160, dpi));
        let visible_meters =
            usize::from(state.show_codex.get()) + usize::from(state.show_minimax.get());
        let meter_width = if visible_meters > 1 {
            (meter_area_width - meter_gap) / 2
        } else {
            meter_area_width
        };
        let mut meter_left = padding;
        if state.show_codex.get() {
            draw_meter(
                hdc,
                RECT {
                    left: meter_left,
                    top: 0,
                    right: meter_left + meter_width,
                    bottom: header_height,
                },
                "CODEX",
                state.codex.get(),
                state.codex_stale.get(),
                dpi,
                label_font,
                value_font,
                rgb(105, 190, 255),
            );
            meter_left += meter_width + meter_gap;
        }
        if state.show_minimax.get() {
            draw_meter(
                hdc,
                RECT {
                    left: meter_left,
                    top: 0,
                    right: meter_left + meter_width,
                    bottom: header_height,
                },
                "MINIMAX",
                state.minimax.get(),
                state.minimax_stale.get(),
                dpi,
                label_font,
                value_font,
                rgb(255, 158, 126),
            );
        }

        let clock = state.clock.borrow();
        let clock_left = width - padding - clock_width;
        draw_text(
            hdc,
            &clock,
            RECT {
                left: clock_left,
                top: 0,
                right: width - padding,
                bottom: header_height,
            },
            rgb(220, 226, 235),
            clock_font,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );

        let items = state.items.borrow();
        if !items.is_empty() && layout.rows > 0 {
            for (index, item) in items.iter().enumerate() {
                let Some(row_bounds) = item_bounds(layout, index, width, height, dpi) else {
                    continue;
                };
                if item.due {
                    fill_round_rect(
                        hdc,
                        &RECT {
                            left: row_bounds.left,
                            top: row_bounds.top + dip(2, dpi),
                            right: row_bounds.right,
                            bottom: row_bounds.bottom - dip(2, dpi),
                        },
                        dip(9, dpi),
                        if item.flashing {
                            rgb(103, 53, 42)
                        } else {
                            rgb(59, 39, 36)
                        },
                    );
                } else if item.urgent {
                    fill_round_rect(
                        hdc,
                        &RECT {
                            left: row_bounds.left,
                            top: row_bounds.top + dip(2, dpi),
                            right: row_bounds.right,
                            bottom: row_bounds.bottom - dip(2, dpi),
                        },
                        dip(9, dpi),
                        rgb(62, 50, 31),
                    );
                }
                let indicator_color = if item.due && item.flashing {
                    rgb(255, 133, 103)
                } else if item.due || item.urgent {
                    rgb(255, 198, 92)
                } else if item.kind == HudItemKind::Reminder {
                    rgb(125, 229, 198)
                } else {
                    rgb(201, 211, 224)
                };
                fill_capsule(
                    hdc,
                    &RECT {
                        left: row_bounds.left + dip(4, dpi),
                        top: (row_bounds.top + row_bounds.bottom) / 2 - dip(2, dpi),
                        right: row_bounds.left + dip(8, dpi),
                        bottom: (row_bounds.top + row_bounds.bottom) / 2 + dip(2, dpi),
                    },
                    indicator_color,
                );
                draw_text(
                    hdc,
                    &item.text,
                    RECT {
                        left: row_bounds.left + dip(15, dpi),
                        top: row_bounds.top,
                        right: row_bounds.right - dip(4, dpi),
                        bottom: row_bounds.bottom,
                    },
                    if item.due && item.flashing {
                        rgb(255, 241, 222)
                    } else if item.due || item.urgent {
                        rgb(255, 232, 188)
                    } else if item.kind == HudItemKind::Reminder {
                        rgb(238, 244, 247)
                    } else {
                        rgb(231, 236, 244)
                    },
                    item_font,
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
                );
            }
        }
        for font in [label_font, value_font, clock_font, item_font] {
            if !font.is_null() {
                unsafe { DeleteObject(font) };
            }
        }
        if saved != 0 {
            unsafe { RestoreDC(hdc, saved) };
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_meter(
        hdc: HDC,
        bounds: RECT,
        label: &str,
        remaining: Option<u8>,
        stale: bool,
        dpi: u32,
        label_font: HFONT,
        value_font: HFONT,
        accent: COLORREF,
    ) {
        let value = remaining.map(|value| value.min(100));
        draw_text(
            hdc,
            label,
            RECT {
                left: bounds.left,
                top: dip(7, dpi),
                right: bounds.right - dip(48, dpi),
                bottom: dip(30, dpi),
            },
            if stale {
                rgb(156, 164, 176)
            } else {
                rgb(218, 225, 235)
            },
            label_font,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        draw_text(
            hdc,
            &value.map_or_else(|| "--".into(), |value| format!("{value}%")),
            RECT {
                left: bounds.right - dip(52, dpi),
                top: dip(4, dpi),
                right: bounds.right,
                bottom: dip(32, dpi),
            },
            if stale {
                rgb(179, 186, 196)
            } else {
                rgb(252, 253, 255)
            },
            value_font,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );

        let bar_height = dip(5, dpi).max(3);
        let track = RECT {
            left: bounds.left,
            top: bounds.bottom - dip(13, dpi),
            right: bounds.right,
            bottom: bounds.bottom - dip(13, dpi) + bar_height,
        };
        fill_capsule(hdc, &track, rgb(74, 82, 95));
        if let Some(value) = value {
            let mut progress = track;
            progress.right = progress.left + (track.right - track.left) * value as i32 / 100;
            let color = if stale {
                rgb(139, 147, 158)
            } else {
                quota_color(value, accent)
            };
            fill_capsule(hdc, &progress, color);
        }
    }

    fn fill_capsule(hdc: HDC, bounds: &RECT, color: COLORREF) {
        let diameter = (bounds.right - bounds.left).min(bounds.bottom - bounds.top);
        fill_round_rect(hdc, bounds, (diameter + 1) / 2, color);
    }

    fn fill_round_rect(hdc: HDC, bounds: &RECT, radius: i32, color: COLORREF) {
        if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
            return;
        }
        let diameter = round_rect_diameter(bounds, radius);
        let region = unsafe {
            CreateRoundRectRgn(
                bounds.left,
                bounds.top,
                bounds.right,
                bounds.bottom,
                diameter,
                diameter,
            )
        };
        let brush = unsafe { CreateSolidBrush(color) };
        if !region.is_null() && !brush.is_null() {
            unsafe { FillRgn(hdc, region, brush) };
        }
        if !brush.is_null() {
            unsafe { DeleteObject(brush) };
        }
        if !region.is_null() {
            unsafe { DeleteObject(region) };
        }
    }

    fn round_rect_diameter(bounds: &RECT, radius: i32) -> i32 {
        (radius.saturating_mul(2))
            .min(bounds.right - bounds.left)
            .min(bounds.bottom - bounds.top)
            .max(1)
    }

    fn gradient(hdc: HDC, bounds: &RECT, top: COLORREF, bottom: COLORREF) {
        let vertices = [
            vertex(bounds.left, bounds.top, top),
            vertex(bounds.right, bounds.bottom, bottom),
        ];
        let mesh = GRADIENT_RECT {
            UpperLeft: 0,
            LowerRight: 1,
        };
        unsafe {
            GradientFill(
                hdc,
                vertices.as_ptr(),
                vertices.len() as u32,
                (&mesh as *const GRADIENT_RECT).cast(),
                1,
                GRADIENT_FILL_RECT_V,
            );
        }
    }

    fn vertex(x: i32, y: i32, color: COLORREF) -> TRIVERTEX {
        TRIVERTEX {
            x,
            y,
            Red: ((color & 0xff) as u16) * 257,
            Green: (((color >> 8) & 0xff) as u16) * 257,
            Blue: (((color >> 16) & 0xff) as u16) * 257,
            Alpha: 0,
        }
    }

    fn create_font(height: i32, weight: i32, dpi: u32) -> HFONT {
        let face = wide("Segoe UI Variable Text");
        unsafe {
            CreateFontW(
                -dip(height, dpi),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                ANTIALIASED_QUALITY as u32,
                (DEFAULT_PITCH | FF_DONTCARE) as u32,
                face.as_ptr(),
            )
        }
    }

    fn draw_text(
        hdc: HDC,
        text: &str,
        mut bounds: RECT,
        color: COLORREF,
        font: HFONT,
        format: u32,
    ) {
        if text.is_empty() {
            return;
        }
        let previous = if font.is_null() {
            null_mut()
        } else {
            unsafe { SelectObject(hdc, font) }
        };
        unsafe { SetTextColor(hdc, color) };
        let text: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            DrawTextW(hdc, text.as_ptr(), text.len() as i32, &mut bounds, format);
        }
        if !font.is_null() {
            unsafe { SelectObject(hdc, previous) };
        }
    }

    const fn quota_color(remaining: u8, accent: COLORREF) -> COLORREF {
        if remaining < 20 {
            rgb(255, 104, 104)
        } else if remaining < 50 {
            rgb(255, 194, 92)
        } else {
            accent
        }
    }

    const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
        red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
    }

    fn dip(value: i32, dpi: u32) -> i32 {
        (value * dpi as i32 + 48) / 96
    }

    fn undip(value: i32, dpi: u32) -> i32 {
        (value * 96 + dpi as i32 / 2) / dpi as i32
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain([0]).collect()
    }

    fn last_error(action: &str) -> String {
        format!("{action} (Windows error {})", unsafe { GetLastError() })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn scales_device_independent_pixels() {
            assert_eq!(dip(10, 96), 10);
            assert_eq!(dip(10, 144), 15);
        }

        #[test]
        fn quota_colors_change_at_warning_thresholds() {
            let accent = rgb(105, 190, 255);
            assert_ne!(quota_color(19, accent), quota_color(20, accent));
            assert_ne!(quota_color(49, accent), quota_color(50, accent));
        }

        #[test]
        fn rounded_geometry_never_exceeds_its_bounds() {
            let bar = RECT {
                left: 0,
                top: 0,
                right: 160,
                bottom: 5,
            };
            let short_progress = RECT { right: 3, ..bar };
            let tile = RECT {
                right: 20,
                bottom: 20,
                ..bar
            };

            assert_eq!(round_rect_diameter(&bar, 5), 5);
            assert_eq!(round_rect_diameter(&short_progress, 5), 3);
            assert_eq!(round_rect_diameter(&tile, 4), 8);
        }

        #[test]
        fn glass_compositor_keeps_the_surface_visible_and_corners_clear() {
            let width = 120;
            let height = 72;
            let source_width = width * RENDER_SCALE;
            let source = vec![rgb(30, 34, 41); (source_width * height * RENDER_SCALE) as usize];
            let mut destination = vec![0; (width * height) as usize];

            composite_glass(
                &source,
                source_width,
                &mut destination,
                width,
                height,
                96,
                168,
                false,
            );

            assert!((destination[(height / 2 * width + width / 2) as usize] >> 24) > 150);
            assert_eq!(destination[0], 0);
        }

        #[test]
        fn every_item_receives_a_visible_layout_slot() {
            for count in [1, 3, 12, 24, 56] {
                let layout = panel_layout(count, 800, 480);
                assert!(layout.rows * layout.columns >= count);
                assert!(layout.width <= 780);
                assert!(layout.height <= 460);
            }
        }

        #[test]
        fn personal_ticks_never_reposition_an_active_drag() {
            assert!(!should_relayout(8, 8, false));
            assert!(!should_relayout(8, 9, true));
            assert!(should_relayout(8, 9, false));
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{DisplayState, ProviderVisibility};
    use crate::personal::HudItem;

    pub struct Hud;

    impl Hud {
        pub fn new(
            _: &DisplayState,
            _: ProviderVisibility,
            _: impl Fn(Option<u64>) + 'static,
            _: impl Fn(u64) + 'static,
        ) -> Result<Self, String> {
            Err("the desktop meter is only available on Windows".into())
        }

        pub fn update(&self, _: &DisplayState) {}

        pub fn update_personal(&self, _: &[HudItem], _: &str) {}

        pub fn set_provider_visibility(&self, _: ProviderVisibility) {}
    }
}

pub use platform::Hud;
