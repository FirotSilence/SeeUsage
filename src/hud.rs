use crate::format::{DisplayState, ProviderVisibility};

#[cfg(target_os = "windows")]
mod platform {
    use super::{DisplayState, ProviderVisibility};
    use crate::{
        personal::{HudItem, HudItemKind},
        settings::{self, GlassSettings, HudPosition},
    };
    use std::{
        cell::{Cell, RefCell},
        ffi::c_void,
        mem::size_of,
        ptr::{null, null_mut},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{COLORREF, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
        Graphics::{
            Dwm::{
                DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
                DWMWCP_DONOTROUND, DwmFlush, DwmSetWindowAttribute,
            },
            Gdi::{
                AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO,
                BITMAPINFOHEADER, BLENDFUNCTION, BeginPaint, BitBlt, CLIP_DEFAULT_PRECIS,
                CreateCompatibleDC, CreateDIBSection, CreateFontW, CreatePen, CreateRoundRectRgn,
                CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_END_ELLIPSIS,
                DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
                DrawTextW, EndPaint, FF_DONTCARE, FillRgn, GRADIENT_FILL_RECT_V, GRADIENT_RECT,
                GetDC, GetMonitorInfoW, GradientFill, HALFTONE, HDC, HFONT, InvalidateRect, LineTo,
                MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromRect,
                MonitorFromWindow, MoveToEx, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, ReleaseDC,
                RestoreDC, SRCCOPY, SaveDC, SelectObject, SetBkMode, SetStretchBltMode,
                SetTextColor, SetViewportOrgEx, StretchBlt, TRANSPARENT, TRIVERTEX, UpdateWindow,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::WM_MOUSELEAVE,
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent},
            WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE,
                GWLP_USERDATA, GetClientRect, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
                HTCAPTION, HTCLIENT, HWND_NOTOPMOST, HWND_TOPMOST, IDC_HAND, IDC_SIZEALL,
                IsWindowVisible, KillTimer, LoadCursorW, MA_NOACTIVATE, RegisterClassW,
                SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
                SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetCursor,
                SetTimer, SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                ULW_ALPHA, UpdateLayeredWindow, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WM_DISPLAYCHANGE,
                WM_DPICHANGED, WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_LBUTTONUP,
                WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST,
                WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_NCRBUTTONUP, WM_PAINT, WM_PRINTCLIENT,
                WM_RBUTTONUP, WM_SETCURSOR, WM_SETTINGCHANGE, WM_TIMER, WM_WINDOWPOSCHANGED,
                WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_POPUP,
            },
        },
    };

    const CLASS_NAME: &str = "SeeUsage.DesktopMeter";
    const BASE_WIDTH_DIP: i32 = 332;
    const HEADER_HEIGHT_DIP: i32 = 58;
    const BOTTOM_PADDING_DIP: i32 = 7;
    const DISCLOSURE_HEIGHT_DIP: i32 = 12;
    const DEFAULT_ROW_HEIGHT_DIP: i32 = 26;
    const MIN_ROW_HEIGHT_DIP: i32 = 18;
    const COLUMN_WIDTH_DIP: i32 = 220;
    const MIN_COLUMN_WIDTH_DIP: i32 = 140;
    const COLUMN_GAP_DIP: i32 = 12;
    const CONTENT_PADDING_DIP: i32 = 12;
    const MARGIN_DIP: i32 = 10;
    const SHADOW_PADDING_DIP: i32 = 8;
    const SURFACE_RADIUS_DIP: i32 = 24;
    const BACKDROP_MARGIN_DIP: i32 = 20;
    const BACKDROP_CACHE_TTL: Duration = Duration::from_secs(5);
    const GLASS_TIMER_ID: usize = 1;
    const GLASS_TIMER_PERIOD_MS: u32 = 500;
    const SCENE_SIGNATURE_WIDTH: i32 = 32;
    const SCENE_SIGNATURE_HEIGHT: i32 = 18;
    const RENDER_SCALE: i32 = 2;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct PanelLayout {
        width: i32,
        height: i32,
        columns: usize,
        rows: usize,
        row_height: i32,
        body_items: usize,
        has_disclosure: bool,
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
        glass: Cell<GlassSettings>,
        layout: Cell<PanelLayout>,
        hovered: Cell<bool>,
        pointer: Cell<Option<(i32, i32)>>,
        dragging: Cell<bool>,
        rendering_drag_frame: Cell<bool>,
        capture_excluded: Cell<bool>,
        last_drag_position: Cell<Option<(i32, i32)>>,
        drag_motion: Cell<(i32, i32)>,
        animation_started: Instant,
        content_generation: Cell<u64>,
        content: RefCell<Option<CachedContent>>,
        backdrop: RefCell<Option<CachedBackdrop>>,
        scene_signature: RefCell<Option<SceneSignature>>,
        items: RefCell<Vec<HudItem>>,
        clock: RefCell<String>,
        show_context_menu: Box<dyn Fn(Option<u64>)>,
        acknowledge_reminder: Box<dyn Fn(u64)>,
        toggle_reminders: Box<dyn Fn()>,
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
            toggle_reminders: impl Fn() + 'static,
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
                glass: Cell::new(settings::glass_settings()),
                layout: Cell::new(PanelLayout {
                    width: BASE_WIDTH_DIP,
                    height: HEADER_HEIGHT_DIP + 6,
                    columns: 1,
                    rows: 0,
                    row_height: DEFAULT_ROW_HEIGHT_DIP,
                    body_items: 0,
                    has_disclosure: false,
                }),
                hovered: Cell::new(false),
                pointer: Cell::new(None),
                dragging: Cell::new(false),
                rendering_drag_frame: Cell::new(false),
                capture_excluded: Cell::new(false),
                last_drag_position: Cell::new(None),
                drag_motion: Cell::new((0, 0)),
                animation_started: Instant::now(),
                content_generation: Cell::new(0),
                content: RefCell::new(None),
                backdrop: RefCell::new(None),
                scene_signature: RefCell::new(None),
                items: RefCell::new(Vec::new()),
                clock: RefCell::new(String::new()),
                show_context_menu: Box::new(show_context_menu),
                acknowledge_reminder: Box::new(acknowledge_reminder),
                toggle_reminders: Box::new(toggle_reminders),
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
            if unsafe { SetTimer(hwnd, GLASS_TIMER_ID, GLASS_TIMER_PERIOD_MS, None) } == 0 {
                unsafe { DestroyWindow(hwnd) };
                return Err(last_error("could not start the glass refresh timer"));
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
            let codex_changed =
                self._state.codex.replace(display.codex_remaining) != display.codex_remaining;
            let minimax_changed =
                self._state.minimax.replace(display.minimax_remaining) != display.minimax_remaining;
            let codex_stale_changed =
                self._state.codex_stale.replace(display.codex_stale) != display.codex_stale;
            let minimax_stale_changed =
                self._state.minimax_stale.replace(display.minimax_stale) != display.minimax_stale;
            let changed =
                codex_changed || minimax_changed || codex_stale_changed || minimax_stale_changed;
            if changed {
                bump_content_generation(&self._state);
            }
            if !self._state.dragging.get() {
                unsafe {
                    InvalidateRect(self.hwnd, null(), 0);
                }
            }
        }

        pub fn update_personal(&self, items: &[HudItem], clock: &str) {
            let previous_count = self._state.items.borrow().len();
            let changed = self._state.items.borrow().as_slice() != items
                || self._state.clock.borrow().as_str() != clock;
            if changed {
                self._state.items.replace(items.to_vec());
                self._state.clock.replace(clock.to_string());
                bump_content_generation(&self._state);
            }
            let dragging = self._state.dragging.get();
            if should_relayout(previous_count, items.len(), dragging) {
                let _ = position(self.hwnd, &self._state);
            } else if !dragging {
                maintain_topmost(self.hwnd, &self._state);
                if should_repaint_personal(changed, dragging) {
                    unsafe { InvalidateRect(self.hwnd, null(), 0) };
                }
            }
        }

        pub fn set_provider_visibility(&self, visibility: ProviderVisibility) {
            let codex_changed =
                self._state.show_codex.replace(visibility.codex) != visibility.codex;
            let minimax_changed =
                self._state.show_minimax.replace(visibility.minimax) != visibility.minimax;
            let changed = codex_changed || minimax_changed;
            if changed {
                bump_content_generation(&self._state);
            }
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

        pub fn set_glass_settings(&self, glass: GlassSettings) {
            let glass = glass.sanitized();
            let previous = self._state.glass.replace(glass);
            if previous == glass {
                return;
            }
            if previous.blur != glass.blur {
                self._state.backdrop.borrow_mut().take();
            }
            unsafe { InvalidateRect(self.hwnd, null(), 0) };
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
            WM_NCDESTROY => unsafe {
                KillTimer(hwnd, GLASS_TIMER_ID);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            },
            WM_ERASEBKGND => return 1,
            WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
            WM_NCMOUSEMOVE | WM_MOUSEMOVE => {
                if let Some(state) = window_state(hwnd) {
                    set_hovered(hwnd, state, true);
                    update_pointer(hwnd, state, message, lparam);
                    track_pointer_leave(hwnd, message == WM_NCMOUSEMOVE);
                }
            }
            WM_NCMOUSELEAVE | WM_MOUSELEAVE => {
                if let Some(state) = window_state(hwnd) {
                    state.pointer.set(None);
                    set_hovered(hwnd, state, false);
                }
            }
            WM_LBUTTONUP => {
                if let Some(state) = window_state(hwnd) {
                    let (x, y) = point_from_lparam(lparam);
                    match hud_item_at_client_point(hwnd, state, x, y) {
                        Some(HudHit::Reminder(hit)) if hit.due => {
                            (state.acknowledge_reminder)(hit.id);
                        }
                        Some(HudHit::ToggleReminders) => (state.toggle_reminders)(),
                        _ => {}
                    }
                }
                return 0;
            }
            WM_RBUTTONUP => {
                if let Some(state) = window_state(hwnd) {
                    let (x, y) = point_from_lparam(lparam);
                    let reminder_id = match hud_item_at_client_point(hwnd, state, x, y) {
                        Some(HudHit::Reminder(hit)) => Some(hit.id),
                        _ => None,
                    };
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
                    && hud_item_at_screen_point(hwnd, state, lparam).is_some()
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
                    begin_drag(hwnd, state);
                }
                return 0;
            }
            WM_WINDOWPOSCHANGED => {
                if let Some(state) = window_state(hwnd)
                    && should_repaint_window_change(
                        state.dragging.get(),
                        state.rendering_drag_frame.get(),
                    )
                {
                    update_drag_motion(hwnd, state);
                    repaint_drag_frame(hwnd, state);
                }
            }
            WM_EXITSIZEMOVE => {
                if let Some(state) = window_state(hwnd) {
                    end_drag(hwnd, state);
                    remember_position(hwnd, state);
                    position(hwnd, state);
                    persist_position(hwnd, state);
                    unsafe { UpdateWindow(hwnd) };
                }
                return 0;
            }
            WM_TIMER if wparam == GLASS_TIMER_ID => {
                if let Some(state) = window_state(hwnd) {
                    poll_backdrop(hwnd, state);
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

    fn bump_content_generation(state: &HudState) {
        state
            .content_generation
            .set(state.content_generation.get().wrapping_add(1));
    }

    fn poll_backdrop(hwnd: HWND, state: &HudState) {
        if state.dragging.get() || unsafe { IsWindowVisible(hwnd) } == 0 {
            return;
        }

        let mut client = RECT::default();
        unsafe { GetClientRect(hwnd, &mut client) };
        let width = client.right - client.left;
        let height = client.bottom - client.top;
        if width <= 0 || height <= 0 {
            return;
        }
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
            return;
        }
        let screen = unsafe { GetDC(null_mut()) };
        if screen.is_null() {
            return;
        }
        let already_excluded = state.capture_excluded.get();
        let temporary_exclusion = !already_excluded
            && unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) } != 0;
        if !already_excluded && !temporary_exclusion {
            unsafe { ReleaseDC(null_mut(), screen) };
            return;
        }
        if temporary_exclusion {
            unsafe { DwmFlush() };
        }
        let signature = capture_scene_signature(screen, screen, window, width, height);
        if temporary_exclusion {
            unsafe { SetWindowDisplayAffinity(hwnd, WDA_NONE) };
        }
        unsafe { ReleaseDC(null_mut(), screen) };
        let changed = state
            .scene_signature
            .borrow()
            .as_ref()
            .is_none_or(|previous| !previous.matches(&signature));
        if changed {
            state.scene_signature.replace(Some(signature));
            state.backdrop.borrow_mut().take();
            unsafe { InvalidateRect(hwnd, null(), 0) };
        }
    }

    #[derive(Clone, Copy)]
    struct ReminderHit {
        id: u64,
        due: bool,
    }

    #[derive(Clone, Copy)]
    enum HudHit {
        Reminder(ReminderHit),
        ToggleReminders,
    }

    fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
        let x = (lparam as u16) as i16 as i32;
        let y = ((lparam as u32 >> 16) as u16) as i16 as i32;
        (x, y)
    }

    fn hud_item_at_screen_point(hwnd: HWND, state: &HudState, lparam: LPARAM) -> Option<HudHit> {
        let (screen_x, screen_y) = point_from_lparam(lparam);
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
            return None;
        }
        hud_item_at_client_point(hwnd, state, screen_x - window.left, screen_y - window.top)
    }

    fn hud_item_at_client_point(hwnd: HWND, state: &HudState, x: i32, y: i32) -> Option<HudHit> {
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
                let bounds = item_bounds(layout, index, width, height, dpi)?;
                if !(surface_x >= bounds.left
                    && surface_x < bounds.right
                    && surface_y >= bounds.top
                    && surface_y < bounds.bottom)
                {
                    return None;
                }
                match item.kind {
                    HudItemKind::Reminder => item
                        .reminder_id
                        .map(|id| HudHit::Reminder(ReminderHit { id, due: item.due })),
                    HudItemKind::ReminderExpand | HudItemKind::ReminderCollapse => {
                        Some(HudHit::ToggleReminders)
                    }
                    HudItemKind::Note => None,
                }
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

    fn update_pointer(hwnd: HWND, state: &HudState, message: u32, lparam: LPARAM) {
        if state.dragging.get() {
            return;
        }
        let (mut x, mut y) = point_from_lparam(lparam);
        if message == WM_NCMOUSEMOVE {
            let mut window = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
                return;
            }
            x -= window.left;
            y -= window.top;
        }
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let threshold = dip(10, dpi).max(1);
        if state.pointer.get().is_some_and(|(previous_x, previous_y)| {
            (x - previous_x).abs() < threshold && (y - previous_y).abs() < threshold
        }) {
            return;
        }
        state.pointer.set(Some((x, y)));
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }

    fn begin_drag(hwnd: HWND, state: &HudState) {
        state.dragging.set(true);
        state.drag_motion.set((0, 0));
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } != 0 {
            state
                .last_drag_position
                .set(Some((window.left, window.top)));
        }
        let excluded = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) } != 0;
        state.capture_excluded.set(excluded);
        if excluded {
            unsafe { DwmFlush() };
        }
        repaint_drag_frame(hwnd, state);
    }

    fn end_drag(hwnd: HWND, state: &HudState) {
        state.dragging.set(false);
        state.last_drag_position.set(None);
        state.drag_motion.set((0, 0));
        state.backdrop.borrow_mut().take();
        if state.capture_excluded.replace(false) {
            unsafe { SetWindowDisplayAffinity(hwnd, WDA_NONE) };
        }
    }

    fn update_drag_motion(hwnd: HWND, state: &HudState) {
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
            return;
        }
        let current = (window.left, window.top);
        let motion = state
            .last_drag_position
            .replace(Some(current))
            .map_or((0, 0), |previous| drag_motion_between(previous, current));
        state.drag_motion.set(motion);
    }

    fn drag_motion_between(previous: (i32, i32), current: (i32, i32)) -> (i32, i32) {
        (
            (current.0 - previous.0).clamp(-24, 24),
            (current.1 - previous.1).clamp(-24, 24),
        )
    }

    fn should_repaint_window_change(dragging: bool, rendering_drag_frame: bool) -> bool {
        dragging && !rendering_drag_frame
    }

    fn repaint_drag_frame(hwnd: HWND, state: &HudState) {
        if state.rendering_drag_frame.replace(true) {
            return;
        }
        unsafe {
            InvalidateRect(hwnd, null(), 0);
            UpdateWindow(hwnd);
        }
        state.rendering_drag_frame.set(false);
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

    fn should_repaint_personal(changed: bool, dragging: bool) -> bool {
        changed && !dragging
    }

    fn maintain_topmost(hwnd: HWND, state: &HudState) {
        if should_maintain_topmost(state.pinned.get(), state.dragging.get()) {
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }

    fn should_maintain_topmost(pinned: bool, dragging: bool) -> bool {
        pinned && !dragging
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
        let (body_items, has_disclosure) = {
            let items = state.items.borrow();
            let has_disclosure = items.last().is_some_and(|item| {
                matches!(
                    item.kind,
                    HudItemKind::ReminderExpand | HudItemKind::ReminderCollapse
                )
            });
            (
                items.len().saturating_sub(usize::from(has_disclosure)),
                has_disclosure,
            )
        };
        let layout = panel_layout(
            body_items,
            has_disclosure,
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

    fn panel_layout(
        item_count: usize,
        has_disclosure: bool,
        work_width: i32,
        work_height: i32,
    ) -> PanelLayout {
        let available_width = (work_width - MARGIN_DIP * 2).max(1);
        let available_height = (work_height - MARGIN_DIP * 2).max(1);
        let disclosure_height = if has_disclosure {
            DISCLOSURE_HEIGHT_DIP
        } else {
            0
        };
        if item_count == 0 {
            return PanelLayout {
                width: BASE_WIDTH_DIP.min(available_width),
                height: (HEADER_HEIGHT_DIP + disclosure_height + 6).min(available_height),
                columns: 1,
                rows: 0,
                row_height: DEFAULT_ROW_HEIGHT_DIP,
                body_items: 0,
                has_disclosure,
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
        let max_rows =
            ((available_height - HEADER_HEIGHT_DIP - BOTTOM_PADDING_DIP - disclosure_height)
                / MIN_ROW_HEIGHT_DIP)
                .max(1) as usize;
        let required_columns = item_count.div_ceil(max_rows);
        let columns = preferred_columns
            .max(required_columns)
            .min(max_columns)
            .min(item_count)
            .max(1);
        let rows = item_count.div_ceil(columns);
        let row_height =
            ((available_height - HEADER_HEIGHT_DIP - BOTTOM_PADDING_DIP - disclosure_height)
                / rows as i32)
                .clamp(MIN_ROW_HEIGHT_DIP, DEFAULT_ROW_HEIGHT_DIP);
        let desired_width = CONTENT_PADDING_DIP * 2
            + columns as i32 * COLUMN_WIDTH_DIP
            + (columns.saturating_sub(1)) as i32 * COLUMN_GAP_DIP;
        let width = desired_width
            .max(BASE_WIDTH_DIP)
            .min(available_width)
            .max(1);
        let height =
            (HEADER_HEIGHT_DIP + rows as i32 * row_height + disclosure_height + BOTTOM_PADDING_DIP)
                .min(available_height)
                .max(1);

        PanelLayout {
            width,
            height,
            columns,
            rows,
            row_height,
            body_items: item_count,
            has_disclosure,
        }
    }

    fn item_bounds(
        layout: PanelLayout,
        index: usize,
        width: i32,
        height: i32,
        dpi: u32,
    ) -> Option<RECT> {
        let content_left = dip(CONTENT_PADDING_DIP, dpi);
        let content_right = width - content_left;
        if layout.has_disclosure && index == layout.body_items {
            let top = dip(
                HEADER_HEIGHT_DIP + layout.rows as i32 * layout.row_height,
                dpi,
            )
            .min(height - dip(BOTTOM_PADDING_DIP, dpi));
            return Some(RECT {
                left: content_left,
                top,
                right: content_right,
                bottom: (top + dip(DISCLOSURE_HEIGHT_DIP, dpi))
                    .min(height - dip(BOTTOM_PADDING_DIP, dpi)),
            });
        }
        if layout.rows == 0 || index >= layout.body_items {
            return None;
        }
        let column = index / layout.rows;
        let row = index % layout.rows;
        if column >= layout.columns {
            return None;
        }
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

    #[derive(Clone, Copy, Debug)]
    struct GlassStyle {
        material_alpha: u8,
        blur_radius: i32,
        displacement: f32,
        tint_scale: f32,
        highlight_scale: f32,
        motion_scale: f32,
    }

    fn glass_style(
        settings: GlassSettings,
        dpi: u32,
        hovered: bool,
        dragging: bool,
        capture_excluded: bool,
    ) -> GlassStyle {
        let settings = settings.sanitized();
        let base_alpha = if settings.opacity <= 80 {
            settings.opacity as u16 * 236 / 80
        } else {
            236 + (settings.opacity as u16 - 80) * 14 / 20
        } as u8;
        let material_alpha = if base_alpha == 0 {
            0
        } else if dragging {
            base_alpha.saturating_sub(if capture_excluded { 6 } else { 66 })
        } else if hovered {
            base_alpha.saturating_add(6).min(250)
        } else {
            base_alpha
        };
        GlassStyle {
            material_alpha,
            blur_radius: dipf(settings.blur as f32 * 0.2, dpi).round() as i32,
            displacement: dipf(settings.refraction as f32 * 0.112, dpi),
            tint_scale: settings.tint as f32 * 0.02,
            highlight_scale: settings.highlight as f32 * 0.032,
            motion_scale: settings.motion as f32 * 0.02,
        }
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

    struct DibSurface {
        dc: HDC,
        bitmap: *mut c_void,
        previous: *mut c_void,
        bits: *mut u32,
        len: usize,
    }

    impl DibSurface {
        fn new(reference: HDC, width: i32, height: i32, fill: u32) -> Option<Self> {
            let dc = unsafe { CreateCompatibleDC(reference) };
            if dc.is_null() {
                return None;
            }
            let Some((bitmap, bits)) = create_argb_bitmap(reference, width, height) else {
                unsafe { DeleteDC(dc) };
                return None;
            };
            let previous = unsafe { SelectObject(dc, bitmap) };
            let len = (width as usize).saturating_mul(height as usize);
            unsafe { std::slice::from_raw_parts_mut(bits, len) }.fill(fill);
            Some(Self {
                dc,
                bitmap,
                previous,
                bits,
                len,
            })
        }

        fn pixels(&self) -> &[u32] {
            unsafe { std::slice::from_raw_parts(self.bits, self.len) }
        }

        fn pixels_mut(&mut self) -> &mut [u32] {
            unsafe { std::slice::from_raw_parts_mut(self.bits, self.len) }
        }
    }

    impl Drop for DibSurface {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.previous);
                DeleteObject(self.bitmap);
                DeleteDC(self.dc);
            }
        }
    }

    struct Backdrop {
        pixels: Vec<u32>,
        blurred: Vec<u32>,
        width: i32,
        height: i32,
        margin: i32,
    }

    struct CapturedBackdrop {
        pixels: Vec<u32>,
        width: i32,
        height: i32,
        margin: i32,
    }

    struct CachedContent {
        premultiplied: Vec<u32>,
        alpha: Vec<u8>,
        source_width: i32,
        source_height: i32,
        dpi: u32,
        generation: u64,
    }

    impl CachedContent {
        fn matches(&self, width: i32, height: i32, dpi: u32, generation: u64) -> bool {
            self.source_width == width
                && self.source_height == height
                && self.dpi == dpi
                && self.generation == generation
        }
    }

    struct CachedBackdrop {
        backdrop: Backdrop,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
        dpi: u32,
        blur_radius: i32,
        captured_at: Instant,
    }

    struct SceneSignature {
        left: i32,
        top: i32,
        width: i32,
        height: i32,
        pixels: Vec<u32>,
    }

    impl SceneSignature {
        fn matches(&self, other: &Self) -> bool {
            self.left == other.left
                && self.top == other.top
                && self.width == other.width
                && self.height == other.height
                && self.pixels == other.pixels
        }
    }

    impl CachedBackdrop {
        fn matches(
            &self,
            window: RECT,
            width: i32,
            height: i32,
            dpi: u32,
            blur_radius: i32,
        ) -> bool {
            self.left == window.left
                && self.top == window.top
                && self.width == width
                && self.height == height
                && self.dpi == dpi
                && self.blur_radius == blur_radius
        }
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
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let style = glass_style(
            state.glass.get(),
            dpi,
            state.hovered.get(),
            state.dragging.get(),
            state.capture_excluded.get(),
        );
        let generation = state.content_generation.get();
        let needs_content = state
            .content
            .borrow()
            .as_ref()
            .is_none_or(|cached| !cached.matches(source_width, source_height, dpi, generation));
        if needs_content {
            let Some(content_black) = DibSurface::new(reference, source_width, source_height, 0)
            else {
                return false;
            };
            draw_content_scaled(
                state,
                content_black.dc,
                source_width,
                source_height,
                dpi.saturating_mul(RENDER_SCALE as u32),
            );
            let Some(content_white) = DibSurface::new(
                reference,
                source_width,
                source_height,
                pixel_rgb(255, 255, 255),
            ) else {
                return false;
            };
            draw_content_scaled(
                state,
                content_white.dc,
                source_width,
                source_height,
                dpi.saturating_mul(RENDER_SCALE as u32),
            );
            let (premultiplied, alpha) = extract_content(
                content_black.pixels(),
                content_white.pixels(),
                source_width,
                width,
                height,
            );
            state.content.replace(Some(CachedContent {
                premultiplied,
                alpha,
                source_width,
                source_height,
                dpi,
                generation,
            }));
        }
        let Some(mut output) = DibSurface::new(reference, width, height, 0) else {
            return false;
        };

        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
            return false;
        }
        let screen = unsafe { GetDC(null_mut()) };
        if screen.is_null() {
            return false;
        }
        let now = Instant::now();
        let needs_backdrop = state.backdrop.borrow().as_ref().is_none_or(|cached| {
            !cached.matches(window, width, height, dpi, style.blur_radius)
                || now.duration_since(cached.captured_at) >= BACKDROP_CACHE_TTL
        });
        if needs_backdrop {
            let already_excluded = state.capture_excluded.get();
            let temporary_exclusion = !already_excluded
                && unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) } != 0;
            if temporary_exclusion {
                unsafe { DwmFlush() };
            }
            if already_excluded || temporary_exclusion || state.backdrop.borrow().is_none() {
                let backdrop = capture_backdrop(
                    reference,
                    screen,
                    window,
                    width,
                    height,
                    dpi,
                    style.blur_radius,
                    state.dragging.get(),
                );
                state.backdrop.replace(Some(CachedBackdrop {
                    backdrop,
                    left: window.left,
                    top: window.top,
                    width,
                    height,
                    dpi,
                    blur_radius: style.blur_radius,
                    captured_at: now,
                }));
            }
            if temporary_exclusion {
                unsafe { SetWindowDisplayAffinity(hwnd, WDA_NONE) };
            }
        }
        {
            let content = state.content.borrow();
            let Some(content) = content.as_ref() else {
                unsafe { ReleaseDC(null_mut(), screen) };
                return false;
            };
            let cached = state.backdrop.borrow();
            let Some(cached) = cached.as_ref() else {
                unsafe { ReleaseDC(null_mut(), screen) };
                return false;
            };
            composite_glass(
                &content.premultiplied,
                &content.alpha,
                &cached.backdrop,
                output.pixels_mut(),
                width,
                height,
                dpi,
                style,
                state.hovered.get(),
                state.pointer.get(),
                (window.left, window.top),
                state.drag_motion.get(),
                state.animation_started.elapsed().as_secs_f32() * 0.22 * style.motion_scale,
            );
        }

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
        let result = unsafe {
            UpdateLayeredWindow(
                hwnd,
                screen,
                &destination_point,
                &size,
                output.dc,
                &source_point,
                0,
                &blend,
                ULW_ALPHA,
            )
        } != 0;
        unsafe { ReleaseDC(null_mut(), screen) };
        result
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

    fn capture_scene_signature(
        reference: HDC,
        screen: HDC,
        window: RECT,
        width: i32,
        height: i32,
    ) -> SceneSignature {
        let pixels = DibSurface::new(reference, SCENE_SIGNATURE_WIDTH, SCENE_SIGNATURE_HEIGHT, 0)
            .map(|surface| {
                unsafe {
                    SetStretchBltMode(surface.dc, HALFTONE);
                    StretchBlt(
                        surface.dc,
                        0,
                        0,
                        SCENE_SIGNATURE_WIDTH,
                        SCENE_SIGNATURE_HEIGHT,
                        screen,
                        window.left,
                        window.top,
                        width,
                        height,
                        SRCCOPY,
                    );
                }
                surface.pixels().to_vec()
            })
            .unwrap_or_else(|| vec![0; (SCENE_SIGNATURE_WIDTH * SCENE_SIGNATURE_HEIGHT) as usize]);
        SceneSignature {
            left: window.left,
            top: window.top,
            width,
            height,
            pixels,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_backdrop(
        reference: HDC,
        screen: HDC,
        window: RECT,
        width: i32,
        height: i32,
        dpi: u32,
        blur_radius: i32,
        fast: bool,
    ) -> Backdrop {
        finish_backdrop(
            capture_backdrop_pixels(reference, screen, window, width, height, dpi),
            blur_radius,
            fast,
        )
    }

    fn capture_backdrop_pixels(
        reference: HDC,
        screen: HDC,
        window: RECT,
        width: i32,
        height: i32,
        dpi: u32,
    ) -> CapturedBackdrop {
        let margin = dip(BACKDROP_MARGIN_DIP, dpi).max(1);
        let capture_width = width + margin * 2;
        let capture_height = height + margin * 2;
        let mut fallback = Vec::with_capacity((capture_width * capture_height) as usize);
        for y in 0..capture_height {
            let vertical = y as f32 / capture_height.max(1) as f32;
            let shade = (32.0 - vertical * 10.0).round() as u8;
            fallback.extend(std::iter::repeat_n(
                pixel_rgb(shade, shade.saturating_add(3), shade.saturating_add(7)),
                capture_width as usize,
            ));
        }

        let pixels = if let Some(mut surface) =
            DibSurface::new(reference, capture_width, capture_height, 0)
        {
            surface.pixels_mut().copy_from_slice(&fallback);
            let virtual_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
            let virtual_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
            let virtual_right = virtual_left + unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
            let virtual_bottom = virtual_top + unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
            let source_left = window.left - margin;
            let source_top = window.top - margin;
            let copy_left = source_left.max(virtual_left);
            let copy_top = source_top.max(virtual_top);
            let copy_right = (source_left + capture_width).min(virtual_right);
            let copy_bottom = (source_top + capture_height).min(virtual_bottom);
            if copy_right > copy_left && copy_bottom > copy_top {
                unsafe {
                    BitBlt(
                        surface.dc,
                        copy_left - source_left,
                        copy_top - source_top,
                        copy_right - copy_left,
                        copy_bottom - copy_top,
                        screen,
                        copy_left,
                        copy_top,
                        SRCCOPY,
                    )
                };
            }
            surface.pixels().to_vec()
        } else {
            fallback
        };
        CapturedBackdrop {
            pixels,
            width: capture_width,
            height: capture_height,
            margin,
        }
    }

    fn finish_backdrop(captured: CapturedBackdrop, blur_radius: i32, fast: bool) -> Backdrop {
        let CapturedBackdrop {
            pixels,
            width,
            height,
            margin,
        } = captured;
        let blurred = if fast {
            box_blur_rgb(&pixels, width, height, blur_radius)
        } else {
            blur_rgb(&pixels, width, height, blur_radius)
        };
        Backdrop {
            pixels,
            blurred,
            width,
            height,
            margin,
        }
    }

    fn blur_rgb(pixels: &[u32], width: i32, height: i32, radius: i32) -> Vec<u32> {
        if radius <= 0 || width <= 0 || height <= 0 {
            return pixels.to_vec();
        }
        let pass_radius = ((radius + 1) / 2).max(1);
        let first = box_blur_rgb(pixels, width, height, pass_radius);
        box_blur_rgb(&first, width, height, pass_radius)
    }

    fn box_blur_rgb(pixels: &[u32], width: i32, height: i32, radius: i32) -> Vec<u32> {
        if radius <= 0 || width <= 0 || height <= 0 {
            return pixels.to_vec();
        }
        let width = width as usize;
        let height = height as usize;
        let radius = radius as usize;
        let divisor = (radius * 2 + 1) as u64;
        let mut horizontal = vec![0; pixels.len()];
        for y in 0..height {
            let row = y * width;
            let mut sum = [0u64; 3];
            for offset in 0..=radius * 2 {
                add_pixel(
                    &mut sum,
                    pixels[row + offset.saturating_sub(radius).min(width - 1)],
                );
            }
            for x in 0..width {
                horizontal[row + x] = average_pixel(sum, divisor);
                let leaving = x.saturating_sub(radius);
                let entering = (x + radius + 1).min(width - 1);
                subtract_pixel(&mut sum, pixels[row + leaving]);
                add_pixel(&mut sum, pixels[row + entering]);
            }
        }

        let mut output = vec![0; pixels.len()];
        for x in 0..width {
            let mut sum = [0u64; 3];
            for offset in 0..=radius * 2 {
                let y = offset.saturating_sub(radius).min(height - 1);
                add_pixel(&mut sum, horizontal[y * width + x]);
            }
            for y in 0..height {
                output[y * width + x] = average_pixel(sum, divisor);
                let leaving = y.saturating_sub(radius);
                let entering = (y + radius + 1).min(height - 1);
                subtract_pixel(&mut sum, horizontal[leaving * width + x]);
                add_pixel(&mut sum, horizontal[entering * width + x]);
            }
        }
        output
    }

    fn add_pixel(sum: &mut [u64; 3], pixel: u32) {
        sum[0] += ((pixel >> 16) & 0xff) as u64;
        sum[1] += ((pixel >> 8) & 0xff) as u64;
        sum[2] += (pixel & 0xff) as u64;
    }

    fn subtract_pixel(sum: &mut [u64; 3], pixel: u32) {
        sum[0] -= ((pixel >> 16) & 0xff) as u64;
        sum[1] -= ((pixel >> 8) & 0xff) as u64;
        sum[2] -= (pixel & 0xff) as u64;
    }

    fn average_pixel(sum: [u64; 3], divisor: u64) -> u32 {
        pixel_rgb(
            (sum[0] / divisor) as u8,
            (sum[1] / divisor) as u8,
            (sum[2] / divisor) as u8,
        )
    }

    fn sample_rgb(pixels: &[u32], width: i32, height: i32, x: f32, y: f32) -> [f32; 3] {
        let x = x.clamp(0.0, width.saturating_sub(1) as f32);
        let y = y.clamp(0.0, height.saturating_sub(1) as f32);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(width as usize - 1);
        let y1 = (y0 + 1).min(height as usize - 1);
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;
        let top_left = unpack_rgb(pixels[y0 * width as usize + x0]);
        let top_right = unpack_rgb(pixels[y0 * width as usize + x1]);
        let bottom_left = unpack_rgb(pixels[y1 * width as usize + x0]);
        let bottom_right = unpack_rgb(pixels[y1 * width as usize + x1]);
        let mut result = [0.0; 3];
        for channel in 0..3 {
            let top = top_left[channel] + (top_right[channel] - top_left[channel]) * tx;
            let bottom = bottom_left[channel] + (bottom_right[channel] - bottom_left[channel]) * tx;
            result[channel] = top + (bottom - top) * ty;
        }
        result
    }

    fn unpack_rgb(pixel: u32) -> [f32; 3] {
        [
            ((pixel >> 16) & 0xff) as f32,
            ((pixel >> 8) & 0xff) as f32,
            (pixel & 0xff) as f32,
        ]
    }

    fn extract_content(
        content_black: &[u32],
        content_white: &[u32],
        source_width: i32,
        width: i32,
        height: i32,
    ) -> (Vec<u32>, Vec<u8>) {
        let scale = RENDER_SCALE as usize;
        let samples = (scale * scale) as u32;
        let mut premultiplied = vec![0; (width * height) as usize];
        let mut alpha = vec![0; premultiplied.len()];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let mut black = [0u32; 3];
                let mut white = [0u32; 3];
                for sample_y in 0..scale {
                    let row = (y * scale + sample_y) * source_width as usize;
                    for sample_x in 0..scale {
                        let index = row + x * scale + sample_x;
                        let black_pixel = content_black[index];
                        let white_pixel = content_white[index];
                        black[0] += (black_pixel >> 16) & 0xff;
                        black[1] += (black_pixel >> 8) & 0xff;
                        black[2] += black_pixel & 0xff;
                        white[0] += (white_pixel >> 16) & 0xff;
                        white[1] += (white_pixel >> 8) & 0xff;
                        white[2] += white_pixel & 0xff;
                    }
                }
                let black = black.map(|value| value / samples);
                let white = white.map(|value| value / samples);
                let content_alpha = (1.0
                    - ((white[0] as f32 - black[0] as f32).clamp(0.0, 255.0)
                        + (white[1] as f32 - black[1] as f32).clamp(0.0, 255.0)
                        + (white[2] as f32 - black[2] as f32).clamp(0.0, 255.0))
                        / (255.0 * 3.0))
                    .clamp(0.0, 1.0);
                let index = y * width as usize + x;
                premultiplied[index] = (black[0] << 16) | (black[1] << 8) | black[2];
                alpha[index] = (content_alpha * 255.0).round() as u8;
            }
        }
        (premultiplied, alpha)
    }

    #[allow(clippy::too_many_arguments)]
    fn composite_glass(
        content_premultiplied: &[u32],
        content_alpha: &[u8],
        backdrop: &Backdrop,
        destination: &mut [u32],
        width: i32,
        height: i32,
        dpi: u32,
        style: GlassStyle,
        hovered: bool,
        pointer: Option<(i32, i32)>,
        screen_origin: (i32, i32),
        drag_motion: (i32, i32),
        animation_phase: f32,
    ) {
        let padding = dip(SHADOW_PADDING_DIP, dpi) as f32;
        let radius = dip(SURFACE_RADIUS_DIP, dpi) as f32;
        let surface = (
            padding,
            padding,
            width as f32 - padding,
            height as f32 - padding,
        );
        let shadow_blur = padding.max(1.0);
        let shadow_offset = dip(2, dpi) as f32;
        let outer_width = dipf(4.0, dpi);
        let bevel_center = dipf(4.8, dpi);
        let bevel_width = dipf(2.8, dpi);
        let inner_center = dipf(8.0, dpi);
        let inner_width = dipf(5.2, dpi);
        let lens_width = dipf(18.0, dpi);
        let logical_scale = 96.0 / dpi as f32;
        let light_x = -0.54 + animation_phase.sin() * 0.08;
        let light_y = -0.84 + (animation_phase * 0.73).cos() * 0.05;
        let light_length = light_x.hypot(light_y).max(f32::EPSILON);
        let light_x = light_x / light_length;
        let light_y = light_y / light_length;
        let wave_x = (0..width)
            .map(|x| {
                ((x as f32 + 0.5 + screen_origin.0 as f32) * logical_scale * 0.020
                    + animation_phase)
                    .sin()
            })
            .collect::<Vec<_>>();
        let wave_y = (0..height)
            .map(|y| {
                ((y as f32 + 0.5 + screen_origin.1 as f32) * logical_scale * 0.017
                    - animation_phase * 0.7)
                    .cos()
            })
            .collect::<Vec<_>>();

        for (y, &vertical_wave) in wave_y.iter().enumerate() {
            for (x, &horizontal_wave) in wave_x.iter().enumerate() {
                let index = y * width as usize + x;
                let content_pixel = content_premultiplied[index];
                let black = [
                    ((content_pixel >> 16) & 0xff) as f32,
                    ((content_pixel >> 8) & 0xff) as f32,
                    (content_pixel & 0xff) as f32,
                ];
                let content_alpha = content_alpha[index] as f32 / 255.0;
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let distance = rounded_rect_distance(px, py, surface, radius);
                let coverage = (0.5 - distance).clamp(0.0, 1.0);

                let shifted_surface = (
                    surface.0,
                    surface.1 + shadow_offset,
                    surface.2,
                    surface.3 + shadow_offset,
                );
                let shadow_distance = rounded_rect_distance(px, py, shifted_surface, radius);
                let ambient_shadow = (1.0 - shadow_distance.max(0.0) / shadow_blur).clamp(0.0, 1.0);
                let contact_shadow =
                    (1.0 - shadow_distance.max(0.0) / dipf(3.0, dpi).max(1.0)).clamp(0.0, 1.0);
                let shadow_alpha = (24.0 * ambient_shadow * ambient_shadow
                    + 18.0 * contact_shadow * contact_shadow * contact_shadow)
                    * (1.0 - coverage);

                let mut glass = [0.0; 3];
                let mut glass_alpha = 0.0;
                if coverage > 0.0 {
                    let edge_depth = (-distance).max(0.0);
                    let outer_rim = smoothstep01(1.0 - edge_depth / outer_width);
                    let bevel_trough =
                        smoothstep01(1.0 - (edge_depth - bevel_center).abs() / bevel_width);
                    let inner_rim =
                        smoothstep01(1.0 - (edge_depth - inner_center).abs() / inner_width);
                    let lens = smoothstep01(1.0 - edge_depth / lens_width);
                    let (normal_x, normal_y) = if lens > 0.001 {
                        rounded_rect_normal(px, py, surface, radius)
                    } else {
                        (0.0, 0.0)
                    };
                    let liquid_wave = if lens > 0.001 {
                        ((horizontal_wave * 0.58 + vertical_wave * 0.42) * style.motion_scale)
                            .clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    let bend = style.displacement * lens * lens * (1.0 + liquid_wave * 0.08);
                    let motion_lag_x = drag_motion.0 as f32 * 0.16 * lens * style.motion_scale;
                    let motion_lag_y = drag_motion.1 as f32 * 0.16 * lens * style.motion_scale;
                    let sample_x = px + backdrop.margin as f32 - normal_x * bend - motion_lag_x;
                    let sample_y = py + backdrop.margin as f32 - normal_y * bend - motion_lag_y;
                    let (blurred, raw) = if lens > 0.001 {
                        (
                            sample_rgb(
                                &backdrop.blurred,
                                backdrop.width,
                                backdrop.height,
                                sample_x,
                                sample_y,
                            ),
                            sample_rgb(
                                &backdrop.pixels,
                                backdrop.width,
                                backdrop.height,
                                sample_x,
                                sample_y,
                            ),
                        )
                    } else {
                        let source_x =
                            (x as i32 + backdrop.margin).clamp(0, backdrop.width.saturating_sub(1));
                        let source_y = (y as i32 + backdrop.margin)
                            .clamp(0, backdrop.height.saturating_sub(1));
                        let source_index = (source_y * backdrop.width + source_x).max(0) as usize;
                        (
                            unpack_rgb(backdrop.blurred[source_index]),
                            unpack_rgb(backdrop.pixels[source_index]),
                        )
                    };
                    let refraction_scale =
                        (style.displacement / dipf(11.2, dpi).max(f32::EPSILON)).clamp(0.0, 1.0);
                    let clarity = (0.10
                        + liquid_wave * 0.018
                        + lens * lens * (0.28 + refraction_scale * 0.32))
                        .clamp(0.06, 0.68);
                    for channel in 0..3 {
                        glass[channel] =
                            blurred[channel] * (1.0 - clarity) + raw[channel] * clarity;
                    }

                    let mut optical_gradient = 0.0;
                    if lens > 0.001 {
                        let chroma_shift = dipf(1.15, dpi) * lens * lens * refraction_scale;
                        let red_edge = sample_rgb(
                            &backdrop.pixels,
                            backdrop.width,
                            backdrop.height,
                            sample_x + normal_x * chroma_shift,
                            sample_y + normal_y * chroma_shift,
                        );
                        let blue_edge = sample_rgb(
                            &backdrop.pixels,
                            backdrop.width,
                            backdrop.height,
                            sample_x - normal_x * chroma_shift,
                            sample_y - normal_y * chroma_shift,
                        );
                        optical_gradient =
                            ((luminance(red_edge) - luminance(blue_edge)) / 255.0).clamp(-1.0, 1.0);
                        glass[0] = glass[0] * 0.91 + red_edge[0] * 0.09;
                        glass[2] = glass[2] * 0.91 + blue_edge[2] * 0.09;
                    }

                    let scene_luma = luminance(glass);
                    let neutral = scene_luma;
                    for channel in &mut glass {
                        *channel = *channel * 0.92 + neutral * 0.08;
                    }
                    let tint =
                        ((0.17 + scene_luma / 255.0 * 0.13) * style.tint_scale).clamp(0.0, 0.72);
                    let tint_color = [22.0, 25.0, 30.0];
                    for channel in 0..3 {
                        glass[channel] = glass[channel] * (1.0 - tint) + tint_color[channel] * tint;
                    }

                    let mut edge_alpha = 0.0;
                    if lens > 0.001 {
                        let vertical = ((py - surface.1) / (surface.3 - surface.1)).clamp(0.0, 1.0);
                        let horizontal =
                            ((px - surface.0) / (surface.2 - surface.0)).clamp(0.0, 1.0);
                        let normal_light =
                            (normal_x * light_x + normal_y * light_y).clamp(-1.0, 1.0);
                        let facing_light = normal_light.max(0.0);
                        let facing_away = (-normal_light).max(0.0);
                        let specular = facing_light * facing_light * facing_light;
                        let upper_film =
                            inner_rim * (-normal_y).max(0.0) * (0.45 + (1.0 - horizontal) * 0.55);
                        let raw_detail =
                            ((luminance(raw) - luminance(blurred)).abs() / 255.0).clamp(0.0, 1.0);
                        let pointer_glint = pointer.map_or(0.0, |(pointer_x, pointer_y)| {
                            let distance = (px - pointer_x as f32).hypot(py - pointer_y as f32);
                            outer_rim
                                * smoothstep01(1.0 - distance / dipf(150.0, dpi))
                                * if hovered { 1.0 } else { 0.0 }
                        });
                        let pointer_bloom = pointer.map_or(0.0, |(pointer_x, pointer_y)| {
                            let distance = (px - pointer_x as f32).hypot(py - pointer_y as f32);
                            smoothstep01(1.0 - distance / dipf(115.0, dpi))
                                * if hovered { 1.0 } else { 0.0 }
                        });
                        let corner_inset = radius * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
                        let corner_glint_radius = dipf(64.0, dpi);
                        let top_left_glint = radial_falloff(
                            px,
                            py,
                            surface.0 + corner_inset,
                            surface.1 + corner_inset,
                            corner_glint_radius,
                        );
                        let bottom_right_glint = radial_falloff(
                            px,
                            py,
                            surface.2 - corner_inset,
                            surface.3 - corner_inset,
                            corner_glint_radius,
                        );
                        let top_left_fill = 0.24 + (1.0 - specular) * 0.22;
                        let fixed_corner_glint = outer_rim
                            * (top_left_glint * top_left_fill + bottom_right_glint * 0.96);
                        let fixed_corner_bloom =
                            lens * (top_left_glint * 0.22 + bottom_right_glint * 0.90);
                        let liquid_light = liquid_wave * (inner_rim * 3.5 + outer_rim * 4.5);
                        let motion_strength =
                            (drag_motion.0 as f32).hypot(drag_motion.1 as f32) / 24.0;
                        let motion_glint =
                            outer_rim * motion_strength.clamp(0.0, 1.0) * 5.0 * style.motion_scale;
                        let background_response = 0.65 + (1.0 - scene_luma / 255.0) * 0.45;
                        let highlight = (outer_rim * (6.0 + specular * 68.0)
                            + inner_rim * (2.5 + specular * 23.0)
                            + upper_film * 15.0
                            + pointer_glint * 36.0
                            + pointer_bloom * lens * 5.0
                            + fixed_corner_glint * 40.0
                            + fixed_corner_bloom * 5.0
                            + liquid_light
                            + motion_glint
                            + raw_detail * outer_rim * 16.0
                            + optical_gradient.max(0.0) * lens * 27.0)
                            * background_response
                            * style.highlight_scale;
                        let shade = (bevel_trough * (3.0 + facing_away * 16.0)
                            + optical_gradient.min(0.0).abs() * lens * 18.0
                            + outer_rim * (horizontal * 0.35 + vertical * 0.65) * 3.5)
                            * style.highlight_scale;
                        let raw_luma = luminance(raw);
                        for channel in 0..3 {
                            let environmental_spill = (raw[channel] - raw_luma)
                                * (outer_rim + inner_rim * 0.45)
                                * 0.10
                                * style.highlight_scale;
                            glass[channel] = (glass[channel] + highlight - shade
                                + environmental_spill)
                                .clamp(0.0, 255.0);
                        }
                        edge_alpha = outer_rim
                            * (2.0
                                + specular * 5.0
                                + fixed_corner_glint * 1.6
                                + if hovered { pointer_glint * 2.0 } else { 0.0 });
                    }
                    if style.material_alpha > 0 {
                        glass_alpha = ((style.material_alpha as f32 + edge_alpha).min(250.0)
                            / 255.0)
                            * coverage;
                    }
                }

                let content_alpha = content_alpha * coverage;
                let combined_alpha = content_alpha + glass_alpha * (1.0 - content_alpha);
                let total_alpha = combined_alpha + shadow_alpha / 255.0 * (1.0 - combined_alpha);
                let mut premultiplied = [0u32; 3];
                for channel in 0..3 {
                    let content = black[channel] * coverage;
                    let glass = glass[channel] * glass_alpha * (1.0 - content_alpha);
                    premultiplied[channel] = (content + glass).round().clamp(0.0, 255.0) as u32;
                }
                destination[index] = ((total_alpha * 255.0).round().clamp(0.0, 255.0) as u32) << 24
                    | (premultiplied[0] << 16)
                    | (premultiplied[1] << 8)
                    | premultiplied[2];
            }
        }
    }

    fn rounded_rect_normal(
        x: f32,
        y: f32,
        bounds: (f32, f32, f32, f32),
        radius: f32,
    ) -> (f32, f32) {
        let step = 0.75;
        let dx = rounded_rect_distance(x + step, y, bounds, radius)
            - rounded_rect_distance(x - step, y, bounds, radius);
        let dy = rounded_rect_distance(x, y + step, bounds, radius)
            - rounded_rect_distance(x, y - step, bounds, radius);
        let length = dx.hypot(dy);
        if length > 0.0001 {
            (dx / length, dy / length)
        } else {
            (0.0, -1.0)
        }
    }

    fn luminance(color: [f32; 3]) -> f32 {
        color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
    }

    fn smoothstep01(value: f32) -> f32 {
        let value = value.clamp(0.0, 1.0);
        value * value * (3.0 - 2.0 * value)
    }

    fn radial_falloff(x: f32, y: f32, center_x: f32, center_y: f32, radius: f32) -> f32 {
        let dx = x - center_x;
        let dy = y - center_y;
        let radius_squared = radius * radius;
        smoothstep01(1.0 - (dx * dx + dy * dy) / radius_squared.max(f32::EPSILON))
    }

    fn pixel_rgb(red: u8, green: u8, blue: u8) -> u32 {
        ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
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
        gradient(hdc, &bounds, rgb(42, 47, 56), rgb(20, 24, 30));
        if saved != 0 {
            unsafe { RestoreDC(hdc, saved) };
        }
        draw_content_scaled(state, hdc, total_width, total_height, dpi);
    }

    fn draw_content_scaled(
        state: &HudState,
        hdc: HDC,
        total_width: i32,
        total_height: i32,
        dpi: u32,
    ) {
        let shadow_padding = dip(SHADOW_PADDING_DIP, dpi);
        let width = (total_width - shadow_padding * 2).max(1);
        let height = (total_height - shadow_padding * 2).max(1);
        let saved = unsafe { SaveDC(hdc) };
        unsafe { SetViewportOrgEx(hdc, shadow_padding, shadow_padding, null_mut()) };
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
                if matches!(
                    item.kind,
                    HudItemKind::ReminderExpand | HudItemKind::ReminderCollapse
                ) {
                    draw_chevron(
                        hdc,
                        &row_bounds,
                        item.kind == HudItemKind::ReminderCollapse,
                        rgb(238, 244, 247),
                        dpi,
                    );
                    continue;
                }
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

    fn draw_chevron(hdc: HDC, bounds: &RECT, points_up: bool, color: COLORREF, dpi: u32) {
        let center_x = (bounds.left + bounds.right) / 2;
        let center_y = (bounds.top + bounds.bottom) / 2;
        let half_width = dip(5, dpi);
        let half_height = dip(2, dpi);
        let (edge_y, point_y) = if points_up {
            (center_y + half_height, center_y - half_height)
        } else {
            (center_y - half_height, center_y + half_height)
        };
        let pen = unsafe { CreatePen(PS_SOLID, dip(1, dpi).max(1), color) };
        if pen.is_null() {
            return;
        }
        let previous = unsafe { SelectObject(hdc, pen) };
        unsafe {
            MoveToEx(hdc, center_x - half_width, edge_y, null_mut());
            LineTo(hdc, center_x, point_y);
            LineTo(hdc, center_x + half_width, edge_y);
            SelectObject(hdc, previous);
            DeleteObject(pen);
        }
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

    fn dipf(value: f32, dpi: u32) -> f32 {
        value * dpi as f32 / 96.0
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
            let source_len = (source_width * height * RENDER_SCALE) as usize;
            let content_black = vec![0; source_len];
            let content_white = vec![pixel_rgb(255, 255, 255); source_len];
            let (content_premultiplied, content_alpha) =
                extract_content(&content_black, &content_white, source_width, width, height);
            let margin = 20;
            let backdrop_width = width + margin * 2;
            let backdrop_height = height + margin * 2;
            let backdrop_pixels =
                vec![pixel_rgb(88, 118, 148); (backdrop_width * backdrop_height) as usize];
            let backdrop = Backdrop {
                blurred: backdrop_pixels.clone(),
                pixels: backdrop_pixels,
                width: backdrop_width,
                height: backdrop_height,
                margin,
            };
            let mut destination = vec![0; (width * height) as usize];

            composite_glass(
                &content_premultiplied,
                &content_alpha,
                &backdrop,
                &mut destination,
                width,
                height,
                96,
                glass_style(GlassSettings::default(), 96, false, false, false),
                false,
                None,
                (0, 0),
                (0, 0),
                0.0,
            );

            assert!((destination[(height / 2 * width + width / 2) as usize] >> 24) > 220);
            assert_eq!(destination[0], 0);
            let top_rim = destination[(12 * width + width / 2) as usize];
            let top_left_rim = destination[(12 * width + 24) as usize];
            let bottom_rim = destination[((height - 12) * width + width / 2) as usize];
            let bottom_right_rim = destination[((height - 12) * width + width - 24) as usize];
            let brightness =
                |pixel: u32| (pixel & 0xff) + ((pixel >> 8) & 0xff) + ((pixel >> 16) & 0xff);
            assert!(brightness(top_rim) > brightness(bottom_rim));
            assert!(
                brightness(top_left_rim) > brightness(top_rim) + 20,
                "top-left rim brightness {} should exceed top rim brightness {}",
                brightness(top_left_rim),
                brightness(top_rim)
            );
            assert!(
                brightness(top_left_rim) < brightness(top_rim) + 40,
                "top-left rim brightness {} should not overpower top rim brightness {}",
                brightness(top_left_rim),
                brightness(top_rim)
            );
            assert!(
                brightness(bottom_right_rim) > brightness(bottom_rim) + 20,
                "bottom-right rim brightness {} should exceed bottom rim brightness {}",
                brightness(bottom_right_rim),
                brightness(bottom_rim)
            );
        }

        #[test]
        fn balanced_glass_defaults_preserve_the_tuned_material() {
            let style = glass_style(GlassSettings::default(), 96, false, false, false);
            assert_eq!(style.material_alpha, 236);
            assert_eq!(style.blur_radius, 10);
            assert!((style.displacement - 5.6).abs() < 0.001);
            assert!((style.tint_scale - 1.0).abs() < 0.001);
            assert!((style.highlight_scale - 1.6).abs() < 0.001);
            assert!((style.motion_scale - 1.0).abs() < 0.001);
        }

        #[test]
        fn static_glass_polls_for_changes_at_a_capped_rate() {
            assert_eq!(GLASS_TIMER_PERIOD_MS, 500);
            assert!(BACKDROP_CACHE_TTL > Duration::from_millis(GLASS_TIMER_PERIOD_MS as u64));
        }

        #[test]
        fn scene_signature_detects_pixel_and_position_changes() {
            let baseline = SceneSignature {
                left: 10,
                top: 20,
                width: 300,
                height: 180,
                pixels: vec![1, 2, 3],
            };
            let same = SceneSignature {
                left: 10,
                top: 20,
                width: 300,
                height: 180,
                pixels: vec![1, 2, 3],
            };
            let moved = SceneSignature { left: 11, ..same };
            assert!(baseline.matches(&SceneSignature {
                left: 10,
                top: 20,
                width: 300,
                height: 180,
                pixels: vec![1, 2, 3],
            }));
            assert!(!baseline.matches(&moved));
            assert!(!baseline.matches(&SceneSignature {
                pixels: vec![1, 2, 4],
                ..baseline
            }));
        }

        #[test]
        fn glass_controls_span_weaker_and_stronger_effects() {
            let weak = glass_style(
                GlassSettings {
                    opacity: 0,
                    blur: 0,
                    tint: 0,
                    refraction: 0,
                    highlight: 0,
                    motion: 0,
                },
                96,
                false,
                false,
                false,
            );
            let strong = glass_style(
                GlassSettings {
                    opacity: 100,
                    blur: 100,
                    tint: 100,
                    refraction: 100,
                    highlight: 100,
                    motion: 100,
                },
                96,
                false,
                false,
                false,
            );
            assert!(weak.material_alpha < strong.material_alpha);
            assert!(weak.blur_radius < strong.blur_radius);
            assert!(weak.displacement < strong.displacement);
            assert!(weak.tint_scale < strong.tint_scale);
            assert!(weak.highlight_scale < strong.highlight_scale);
            assert!(weak.motion_scale < strong.motion_scale);
        }

        #[test]
        fn backdrop_blur_spreads_a_bright_pixel_without_changing_dimensions() {
            let width = 15;
            let height = 9;
            let mut pixels = vec![0; width * height];
            pixels[height / 2 * width + width / 2] = pixel_rgb(255, 255, 255);

            let blurred = blur_rgb(&pixels, width as i32, height as i32, 4);

            assert_eq!(blurred.len(), pixels.len());
            assert!(blurred[height / 2 * width + width / 2] < pixel_rgb(255, 255, 255));
            assert_ne!(blurred[height / 2 * width + width / 2 + 2], 0);
        }

        #[test]
        fn zero_backdrop_blur_preserves_the_original_pixels() {
            let pixels = vec![pixel_rgb(10, 20, 30), pixel_rgb(200, 180, 160)];
            assert_eq!(blur_rgb(&pixels, 2, 1, 0), pixels);
        }

        #[test]
        fn every_item_receives_a_visible_layout_slot() {
            for count in [1, 3, 12, 24, 56] {
                let layout = panel_layout(count, false, 800, 480);
                assert!(layout.rows * layout.columns >= count);
                assert!(layout.width <= 780);
                assert!(layout.height <= 460);
            }
        }

        #[test]
        fn reminder_disclosure_uses_a_compact_full_width_footer() {
            let without_disclosure = panel_layout(2, false, 800, 480);
            let with_disclosure = panel_layout(2, true, 800, 480);
            assert_eq!(
                with_disclosure.height - without_disclosure.height,
                DISCLOSURE_HEIGHT_DIP
            );

            let bounds = item_bounds(
                with_disclosure,
                2,
                with_disclosure.width,
                with_disclosure.height,
                96,
            )
            .expect("disclosure footer");
            assert_eq!(bounds.left, CONTENT_PADDING_DIP);
            assert_eq!(bounds.right, with_disclosure.width - CONTENT_PADDING_DIP);
            assert_eq!(bounds.bottom - bounds.top, DISCLOSURE_HEIGHT_DIP);
        }

        #[test]
        fn personal_ticks_never_reposition_an_active_drag() {
            assert!(!should_relayout(8, 8, false));
            assert!(!should_relayout(8, 9, true));
            assert!(should_relayout(8, 9, false));
            assert!(!should_repaint_personal(false, false));
            assert!(!should_repaint_personal(true, true));
            assert!(should_repaint_personal(true, false));
        }

        #[test]
        fn topmost_maintenance_respects_pin_and_drag_state() {
            assert!(should_maintain_topmost(true, false));
            assert!(!should_maintain_topmost(true, true));
            assert!(!should_maintain_topmost(false, false));
        }

        #[test]
        fn active_window_drag_repaints_each_completed_move() {
            assert!(should_repaint_window_change(true, false));
            assert!(!should_repaint_window_change(false, false));
            assert!(!should_repaint_window_change(true, true));
        }

        #[test]
        fn drag_motion_is_bounded_for_stable_refraction() {
            assert_eq!(drag_motion_between((100, 100), (108, 94)), (8, -6));
            assert_eq!(drag_motion_between((100, 100), (180, 20)), (24, -24));
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
            _: impl Fn() + 'static,
        ) -> Result<Self, String> {
            Err("the desktop meter is only available on Windows".into())
        }

        pub fn update(&self, _: &DisplayState) {}

        pub fn update_personal(&self, _: &[HudItem], _: &str) {}

        pub fn set_provider_visibility(&self, _: ProviderVisibility) {}
    }
}

pub use platform::Hud;
