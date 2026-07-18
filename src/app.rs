use crate::{
    autostart::AutoStart,
    cache,
    format::{self, ProviderVisibility},
    hud::Hud,
    i18n::{self, Language, key},
    icon,
    model::{self, AppSnapshot},
    personal::{self, PersonalItems, ReminderPreset},
    platform_ui, providers, settings, tool_approval,
};
use single_instance::SingleInstance;
use std::{
    env,
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant},
};
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
};
#[cfg(target_os = "windows")]
use tray_icon::menu::ContextMenu;
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};

enum UserEvent {
    Menu(MenuEvent),
    HudContextMenu(Option<u64>),
    ReminderAcknowledged(u64),
    RefreshStarted,
    SnapshotUpdated(AppSnapshot),
}

enum WorkerCommand {
    Refresh,
    Shutdown,
}

const DELETE_MENU_PAGE_SIZE: usize = 10;

struct MenuState {
    codex: MenuItem,
    minimax: MenuItem,
    updated: MenuItem,
    #[cfg(target_os = "windows")]
    meter: CheckMenuItem,
    #[cfg(target_os = "windows")]
    pin_meter: CheckMenuItem,
    refresh: MenuItem,
    autostart: CheckMenuItem,
    show_distant_reminders: CheckMenuItem,
    show_distant_reminders_saved: bool,
    show_codex: CheckMenuItem,
    show_minimax: CheckMenuItem,
    provider_visibility_saved: ProviderVisibility,
    forget_approvals: MenuItem,
    #[cfg(target_os = "windows")]
    meter_id: MenuId,
    #[cfg(target_os = "windows")]
    reset_meter_id: MenuId,
    #[cfg(target_os = "windows")]
    pin_meter_id: MenuId,
    refresh_id: MenuId,
    autostart_id: MenuId,
    show_distant_reminders_id: MenuId,
    show_codex_id: MenuId,
    show_minimax_id: MenuId,
    forget_approvals_id: MenuId,
    quit_id: MenuId,
}

pub fn run_tray() -> Result<(), String> {
    configure_windows_dpi();
    let instance = SingleInstance::new("com.seeusage.app")
        .map_err(|error| format!("could not create the single-instance guard: {error}"))?;
    if !instance.is_single() {
        return Ok(());
    }

    let cached = cache::load().unwrap_or_default();
    let mut current_snapshot = cached.clone();
    let mut personal = settings::load_personal_items();
    let mut show_distant_reminders = settings::show_distant_reminders();
    let mut provider_visibility = settings::provider_visibility();
    let mut language = settings::language();
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    configure_macos(&mut event_loop);
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(UserEvent::Menu(event));
        }
    }));

    let autostart = AutoStart::new().ok();
    let autostart_enabled = autostart
        .as_ref()
        .and_then(|manager| manager.is_enabled().ok())
        .unwrap_or(false);
    let initial_display = format::display(&cached, language.effective(), provider_visibility);
    let hud = Hud::new(
        &initial_display,
        provider_visibility,
        {
            let proxy = proxy.clone();
            move |reminder_id| {
                let _ = proxy.send_event(UserEvent::HudContextMenu(reminder_id));
            }
        },
        {
            let proxy = proxy.clone();
            move |reminder_id| {
                let _ = proxy.send_event(UserEvent::ReminderAcknowledged(reminder_id));
            }
        },
    )
    .ok();
    update_hud_personal(
        hud.as_ref(),
        &personal,
        model::unix_timestamp(),
        show_distant_reminders,
        language.effective(),
    );
    let (mut menu, mut menu_state) = build_menu(
        &cached,
        &personal,
        autostart_enabled,
        hud.is_some(),
        hud.is_some(),
        show_distant_reminders,
        language,
        provider_visibility,
    )?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_menu_on_left_click(true)
        .with_menu_on_right_click(true)
        .with_tooltip(&initial_display.tooltip)
        .with_title(&initial_display.title)
        .with_icon(icon::make_icon(initial_display.minimum_remaining)?)
        .with_icon_as_template(cfg!(target_os = "macos"))
        .build()
        .map_err(|error| format!("could not create the tray icon: {error}"))?;

    #[cfg(target_os = "windows")]
    let menu_proxy = proxy.clone();
    let worker = spawn_worker(cached, proxy, refresh_interval());
    menu_state.refresh.set_enabled(false);
    menu_state
        .refresh
        .set_text(i18n::text(language.effective(), key::REFRESHING));
    let mut next_personal_tick = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        let _keep_instance_alive = &instance;
        let _keep_tray_alive = &tray;
        let _keep_hud_alive = &hud;
        *control_flow = ControlFlow::WaitUntil(next_personal_tick);

        match event {
            Event::UserEvent(UserEvent::Menu(event)) => {
                if let Some(selected_language) = Language::from_menu_id(event.id.as_ref()) {
                    let previous_language = language;
                    if selected_language != language {
                        match settings::set_language(selected_language) {
                            Ok(()) => {
                                language = selected_language;
                            }
                            Err(error) => platform_ui::show_error(&error),
                        }
                    }
                    let effective = language.effective();
                    rebuild_menu(
                        &tray,
                        &mut menu,
                        &mut menu_state,
                        &current_snapshot,
                        &personal,
                        hud.is_some(),
                        show_distant_reminders,
                        language,
                        provider_visibility,
                    );
                    if language != previous_language {
                        update_hud_personal(
                            hud.as_ref(),
                            &personal,
                            model::unix_timestamp(),
                            show_distant_reminders,
                            effective,
                        );
                        apply_snapshot(
                            &tray,
                            &menu_state,
                            hud.as_ref(),
                            &current_snapshot,
                            effective,
                            provider_visibility,
                        );
                    }
                } else {
                    let content_changed = handle_menu_event(
                        event,
                        &menu_state,
                        &worker,
                        autostart.as_ref(),
                        hud.as_ref(),
                        &mut personal,
                        control_flow,
                        language.effective(),
                    );
                    if content_changed {
                        show_distant_reminders = menu_state.show_distant_reminders.is_checked();
                        provider_visibility = ProviderVisibility {
                            codex: menu_state.show_codex.is_checked(),
                            minimax: menu_state.show_minimax.is_checked(),
                        };
                        if let Some(hud) = hud.as_ref() {
                            hud.set_provider_visibility(provider_visibility);
                        }
                        update_hud_personal(
                            hud.as_ref(),
                            &personal,
                            model::unix_timestamp(),
                            show_distant_reminders,
                            language.effective(),
                        );
                        rebuild_menu(
                            &tray,
                            &mut menu,
                            &mut menu_state,
                            &current_snapshot,
                            &personal,
                            hud.is_some(),
                            show_distant_reminders,
                            language,
                            provider_visibility,
                        );
                        apply_snapshot(
                            &tray,
                            &menu_state,
                            hud.as_ref(),
                            &current_snapshot,
                            language.effective(),
                            provider_visibility,
                        );
                    }
                }
            }
            Event::UserEvent(UserEvent::HudContextMenu(reminder_id)) => {
                #[cfg(target_os = "windows")]
                if let Some(hud) = hud.as_ref() {
                    if let Some(event) =
                        show_hud_menu(&menu, hud, reminder_id, language.effective())
                    {
                        let _ = menu_proxy.send_event(UserEvent::Menu(event));
                    }
                }
                #[cfg(not(target_os = "windows"))]
                let _ = reminder_id;
            }
            Event::UserEvent(UserEvent::ReminderAcknowledged(reminder_id)) => {
                let before = personal.clone();
                let now = model::unix_timestamp();
                if personal.acknowledge_due(reminder_id, now) {
                    if let Err(error) = settings::save_personal_items(&personal) {
                        personal = before;
                        menu_state.updated.set_text(error);
                    } else {
                        update_hud_personal(
                            hud.as_ref(),
                            &personal,
                            now,
                            show_distant_reminders,
                            language.effective(),
                        );
                        rebuild_menu(
                            &tray,
                            &mut menu,
                            &mut menu_state,
                            &current_snapshot,
                            &personal,
                            hud.is_some(),
                            show_distant_reminders,
                            language,
                            provider_visibility,
                        );
                    }
                }
            }
            Event::UserEvent(UserEvent::RefreshStarted) => {
                menu_state.refresh.set_enabled(false);
                menu_state
                    .refresh
                    .set_text(i18n::text(language.effective(), key::REFRESHING));
            }
            Event::UserEvent(UserEvent::SnapshotUpdated(snapshot)) => {
                apply_snapshot(
                    &tray,
                    &menu_state,
                    hud.as_ref(),
                    &snapshot,
                    language.effective(),
                    provider_visibility,
                );
                current_snapshot = snapshot;
                menu_state
                    .refresh
                    .set_text(i18n::text(language.effective(), key::REFRESH_NOW));
                menu_state.refresh.set_enabled(true);
            }
            Event::MainEventsCleared if Instant::now() >= next_personal_tick => {
                let now = model::unix_timestamp();
                update_hud_personal(
                    hud.as_ref(),
                    &personal,
                    now,
                    show_distant_reminders,
                    language.effective(),
                );
                next_personal_tick = Instant::now() + Duration::from_secs(1);
                *control_flow = ControlFlow::WaitUntil(next_personal_tick);
            }
            _ => {}
        }
    });
}

#[cfg(target_os = "windows")]
fn configure_windows_dpi() {
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };

    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_dpi() {}

#[allow(clippy::too_many_arguments)]
fn build_menu(
    snapshot: &AppSnapshot,
    personal: &PersonalItems,
    autostart_enabled: bool,
    hud_available: bool,
    hud_visible: bool,
    show_distant_reminders_enabled: bool,
    configured_language: Language,
    provider_visibility: ProviderVisibility,
) -> Result<(Menu, MenuState), String> {
    let language = configured_language.effective();
    let display = format::display(snapshot, language, provider_visibility);
    let menu = Menu::new();
    let codex = MenuItem::with_id("status.codex", display.codex_line, false, None);
    let minimax = MenuItem::with_id("status.minimax", display.minimax_line, false, None);
    let updated = MenuItem::with_id("status.updated", display.updated_line, false, None);
    #[cfg(target_os = "windows")]
    let meter = CheckMenuItem::with_id(
        "action.meter",
        i18n::text(language, key::SHOW_METER),
        hud_available,
        hud_available && hud_visible,
        None,
    );
    #[cfg(target_os = "windows")]
    let reset_meter = MenuItem::with_id(
        "action.meter.reset",
        i18n::text(language, key::RESET_METER),
        hud_available,
        None,
    );
    #[cfg(target_os = "windows")]
    let pin_meter_saved = settings::hud_pinned();
    #[cfg(target_os = "windows")]
    let pin_meter = CheckMenuItem::with_id(
        "action.meter.pin",
        i18n::text(language, key::PIN_METER),
        hud_available,
        hud_available && pin_meter_saved,
        None,
    );
    let refresh = MenuItem::with_id(
        "action.refresh",
        i18n::text(language, key::REFRESH_NOW),
        true,
        None,
    );
    let autostart = CheckMenuItem::with_id(
        "action.autostart",
        i18n::text(language, key::LAUNCH_LOGIN),
        true,
        autostart_enabled,
        None,
    );
    let forget_approvals = MenuItem::with_id(
        "action.approvals.clear",
        i18n::text(language, key::FORGET_APPROVALS),
        settings::has_unsafe_tool_approvals(),
        None,
    );
    let quit = MenuItem::with_id("action.quit", i18n::text(language, key::QUIT), true, None);
    let language_menu = build_language_menu(configured_language, language)?;
    let display_menu = Submenu::with_id("action.display", i18n::text(language, key::DISPLAY), true);
    let show_codex = CheckMenuItem::with_id(
        "action.display.codex",
        i18n::text(language, key::SHOW_CODEX),
        true,
        provider_visibility.codex,
        None,
    );
    let show_minimax = CheckMenuItem::with_id(
        "action.display.minimax",
        i18n::text(language, key::SHOW_MINIMAX),
        true,
        provider_visibility.minimax,
        None,
    );
    display_menu
        .append_items(&[&show_codex, &show_minimax])
        .map_err(|error| format!("could not build display menu: {error}"))?;
    let separator_one = PredefinedMenuItem::separator();
    let notes = build_notes_menu(personal, language)?;
    let show_distant_reminders = CheckMenuItem::with_id(
        "personal.reminders.show_distant",
        i18n::text(language, key::SHOW_DISTANT),
        true,
        show_distant_reminders_enabled,
        None,
    );
    let reminders = build_reminders_menu(personal, &show_distant_reminders, language)?;
    let personal_separator = PredefinedMenuItem::separator();
    #[cfg(target_os = "windows")]
    let meter_separator = PredefinedMenuItem::separator();
    let separator_two = PredefinedMenuItem::separator();

    if provider_visibility.codex {
        menu.append(&codex)
            .map_err(|error| format!("could not build tray menu: {error}"))?;
    }
    if provider_visibility.minimax {
        menu.append(&minimax)
            .map_err(|error| format!("could not build tray menu: {error}"))?;
    }
    menu.append_items(&[&updated, &separator_one])
        .map_err(|error| format!("could not build tray menu: {error}"))?;
    menu.append_items(&[&notes, &reminders, &personal_separator])
        .map_err(|error| format!("could not build tray menu: {error}"))?;
    #[cfg(target_os = "windows")]
    menu.append_items(&[&meter, &pin_meter, &reset_meter, &meter_separator])
        .map_err(|error| format!("could not build tray menu: {error}"))?;
    #[cfg(not(target_os = "windows"))]
    let _ = (hud_available, hud_visible);
    menu.append_items(&[
        &refresh,
        &autostart,
        &display_menu,
        &language_menu,
        &forget_approvals,
        &separator_two,
        &quit,
    ])
    .map_err(|error| format!("could not build tray menu: {error}"))?;

    let state = MenuState {
        #[cfg(target_os = "windows")]
        meter_id: meter.id().clone(),
        #[cfg(target_os = "windows")]
        reset_meter_id: reset_meter.id().clone(),
        #[cfg(target_os = "windows")]
        pin_meter_id: pin_meter.id().clone(),
        refresh_id: refresh.id().clone(),
        autostart_id: autostart.id().clone(),
        show_distant_reminders_id: show_distant_reminders.id().clone(),
        show_codex_id: show_codex.id().clone(),
        show_minimax_id: show_minimax.id().clone(),
        forget_approvals_id: forget_approvals.id().clone(),
        quit_id: quit.id().clone(),
        codex,
        minimax,
        updated,
        #[cfg(target_os = "windows")]
        meter,
        #[cfg(target_os = "windows")]
        pin_meter,
        refresh,
        autostart,
        show_distant_reminders,
        show_distant_reminders_saved: show_distant_reminders_enabled,
        show_codex,
        show_minimax,
        provider_visibility_saved: provider_visibility,
        forget_approvals,
    };
    Ok((menu, state))
}

fn build_language_menu(
    configured_language: Language,
    interface_language: Language,
) -> Result<Submenu, String> {
    let menu = Submenu::with_id(
        "action.language",
        i18n::text(interface_language, key::LANGUAGE),
        true,
    );
    for language in Language::ALL {
        let item = CheckMenuItem::with_id(
            language.menu_id(),
            language.menu_label(interface_language),
            true,
            language == configured_language,
            None,
        );
        menu.append(&item)
            .map_err(|error| format!("could not build language menu: {error}"))?;
    }
    Ok(menu)
}

fn build_notes_menu(personal: &PersonalItems, language: Language) -> Result<Submenu, String> {
    let notes = Submenu::with_id("personal.notes", i18n::text(language, key::NOTES), true);
    let add = MenuItem::with_id(
        "personal.note.add",
        i18n::text(language, key::ADD_NOTE),
        true,
        None,
    );
    notes
        .append(&add)
        .map_err(|error| format!("could not build notes menu: {error}"))?;

    if personal.notes.is_empty() {
        let empty = MenuItem::new(i18n::text(language, key::NO_NOTES), false, None);
        notes
            .append(&empty)
            .map_err(|error| format!("could not build notes menu: {error}"))?;
    } else {
        let separator = PredefinedMenuItem::separator();
        let delete = Submenu::with_id(
            "personal.notes.delete",
            i18n::text(language, key::DELETE_NOTE),
            true,
        );
        let entries = personal
            .notes
            .iter()
            .map(|note| {
                (
                    format!("personal.note.delete.{}", note.id),
                    menu_text(&note.text),
                )
            })
            .collect::<Vec<_>>();
        append_delete_entries(&delete, &entries, "notes")?;
        notes
            .append_items(&[&separator, &delete])
            .map_err(|error| format!("could not build notes menu: {error}"))?;
    }
    Ok(notes)
}

fn build_reminders_menu(
    personal: &PersonalItems,
    show_distant_reminders: &CheckMenuItem,
    language: Language,
) -> Result<Submenu, String> {
    let reminders = Submenu::with_id(
        "personal.reminders",
        i18n::text(language, key::REMINDERS),
        true,
    );
    let add = Submenu::with_id(
        "personal.reminders.add",
        i18n::text(language, key::ADD_REMINDER),
        true,
    );
    let after = Submenu::with_id(
        "personal.reminders.add.after",
        i18n::text(language, key::AFTER_DELAY),
        true,
    );
    append_reminder_presets(
        &after,
        &[
            (
                "personal.reminder.add.after.minutes",
                i18n::text(language, key::MINUTES),
            ),
            (
                "personal.reminder.add.after.hours",
                i18n::text(language, key::HOURS),
            ),
            (
                "personal.reminder.add.after.days",
                i18n::text(language, key::DAYS),
            ),
            (
                "personal.reminder.add.after.weeks",
                i18n::text(language, key::WEEKS),
            ),
        ],
    )?;
    let every = Submenu::with_id(
        "personal.reminders.add.every",
        i18n::text(language, key::REPEAT_EVERY),
        true,
    );
    append_reminder_presets(
        &every,
        &[
            (
                "personal.reminder.add.every.minutes",
                i18n::text(language, key::MINUTES),
            ),
            (
                "personal.reminder.add.every.hours",
                i18n::text(language, key::HOURS),
            ),
            (
                "personal.reminder.add.every.days",
                i18n::text(language, key::DAYS),
            ),
            (
                "personal.reminder.add.every.weeks",
                i18n::text(language, key::WEEKS),
            ),
        ],
    )?;
    let schedule = Submenu::with_id(
        "personal.reminders.add.schedule",
        i18n::text(language, key::AT_SCHEDULE),
        true,
    );
    append_reminder_presets(
        &schedule,
        &[
            (
                "personal.reminder.add.once",
                i18n::text(language, key::ONE_TIME),
            ),
            (
                "personal.reminder.add.daily",
                i18n::text(language, key::DAILY),
            ),
            (
                "personal.reminder.add.weekdays",
                i18n::text(language, key::WEEKDAYS),
            ),
            (
                "personal.reminder.add.weekends",
                i18n::text(language, key::WEEKENDS),
            ),
            (
                "personal.reminder.add.weekly",
                i18n::text(language, key::WEEKLY),
            ),
            (
                "personal.reminder.add.selected_weekdays",
                i18n::text(language, key::SELECTED_WEEKDAYS),
            ),
            (
                "personal.reminder.add.monthly",
                i18n::text(language, key::MONTHLY),
            ),
            (
                "personal.reminder.add.monthly_last",
                i18n::text(language, key::MONTHLY_LAST),
            ),
            (
                "personal.reminder.add.yearly",
                i18n::text(language, key::SOLAR_DATE),
            ),
            (
                "personal.reminder.add.lunar_yearly",
                i18n::text(language, key::LUNAR_DATE),
            ),
        ],
    )?;
    add.append_items(&[&after, &every, &schedule])
        .map_err(|error| format!("could not build reminders menu: {error}"))?;
    reminders
        .append_items(&[&add, show_distant_reminders])
        .map_err(|error| format!("could not build reminders menu: {error}"))?;

    if personal.reminders.is_empty() {
        let empty = MenuItem::new(i18n::text(language, key::NO_REMINDERS), false, None);
        reminders
            .append(&empty)
            .map_err(|error| format!("could not build reminders menu: {error}"))?;
    } else {
        let separator = PredefinedMenuItem::separator();
        let complete = Submenu::with_id(
            "personal.reminders.complete",
            i18n::text(language, key::MARK_COMPLETE),
            true,
        );
        let delete = Submenu::with_id(
            "personal.reminders.delete",
            i18n::text(language, key::DELETE_REMINDER),
            true,
        );
        let mut ordered = personal.reminders.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|reminder| reminder.next_at);
        let complete_entries = ordered
            .iter()
            .map(|reminder| {
                (
                    format!("personal.reminder.complete.{}", reminder.id),
                    menu_text(&personal.reminder_menu_text(reminder, language)),
                )
            })
            .collect::<Vec<_>>();
        let delete_entries = ordered
            .iter()
            .map(|reminder| {
                (
                    format!("personal.reminder.delete.{}", reminder.id),
                    menu_text(&personal.reminder_menu_text(reminder, language)),
                )
            })
            .collect::<Vec<_>>();
        append_delete_entries(&complete, &complete_entries, "reminders")?;
        append_delete_entries(&delete, &delete_entries, "reminders")?;
        reminders
            .append_items(&[&separator, &complete, &delete])
            .map_err(|error| format!("could not build reminders menu: {error}"))?;
    }
    Ok(reminders)
}

fn append_reminder_presets(menu: &Submenu, entries: &[(&str, &str)]) -> Result<(), String> {
    for (id, label) in entries {
        let item = MenuItem::with_id(*id, format!("{label}..."), true, None);
        menu.append(&item)
            .map_err(|error| format!("could not build reminders menu: {error}"))?;
    }
    Ok(())
}

fn append_delete_entries(
    menu: &Submenu,
    entries: &[(String, String)],
    noun: &str,
) -> Result<(), String> {
    if entries.len() <= DELETE_MENU_PAGE_SIZE {
        for (id, label) in entries {
            let item = MenuItem::with_id(id, label, true, None);
            menu.append(&item)
                .map_err(|error| format!("could not build {noun} menu: {error}"))?;
        }
        return Ok(());
    }

    for (page, chunk) in entries.chunks(DELETE_MENU_PAGE_SIZE).enumerate() {
        let first = page * DELETE_MENU_PAGE_SIZE + 1;
        let last = first + chunk.len() - 1;
        let group = Submenu::new(format!("{first}-{last}"), true);
        for (id, label) in chunk {
            let item = MenuItem::with_id(id, label, true, None);
            group
                .append(&item)
                .map_err(|error| format!("could not build {noun} menu: {error}"))?;
        }
        menu.append(&group)
            .map_err(|error| format!("could not build {noun} menu: {error}"))?;
    }
    Ok(())
}

fn menu_text(value: &str) -> String {
    let mut text = value.chars().take(52).collect::<String>();
    if value.chars().count() > 52 {
        text.push_str("...");
    }
    text.replace('&', "&&")
}

fn apply_snapshot(
    tray: &TrayIcon,
    menu: &MenuState,
    hud: Option<&Hud>,
    snapshot: &AppSnapshot,
    language: Language,
    provider_visibility: ProviderVisibility,
) {
    let display = format::display(snapshot, language, provider_visibility);
    if let Some(hud) = hud {
        hud.update(&display);
    }
    menu.codex.set_text(display.codex_line);
    menu.minimax.set_text(display.minimax_line);
    menu.updated.set_text(display.updated_line);
    menu.forget_approvals
        .set_enabled(settings::has_unsafe_tool_approvals());
    let _ = tray.set_tooltip(Some(display.tooltip));
    tray.set_title(Some(display.title));
    if let Ok(icon) = icon::make_icon(display.minimum_remaining) {
        let _ = tray.set_icon(Some(icon));
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_menu_event(
    event: MenuEvent,
    menu: &MenuState,
    worker: &Sender<WorkerCommand>,
    autostart: Option<&AutoStart>,
    hud: Option<&Hud>,
    personal: &mut PersonalItems,
    control_flow: &mut ControlFlow,
    language: Language,
) -> bool {
    if let Some(changed) = handle_personal_menu_event(event.id.as_ref(), personal, language) {
        return changed;
    }

    if event.id == menu.show_codex_id || event.id == menu.show_minimax_id {
        let mut requested = menu.provider_visibility_saved;
        if event.id == menu.show_codex_id {
            requested.codex = !requested.codex;
        } else {
            requested.minimax = !requested.minimax;
        }
        menu.show_codex.set_checked(requested.codex);
        menu.show_minimax.set_checked(requested.minimax);
        match settings::set_provider_visibility(requested) {
            Ok(()) => return true,
            Err(error) => {
                menu.show_codex
                    .set_checked(menu.provider_visibility_saved.codex);
                menu.show_minimax
                    .set_checked(menu.provider_visibility_saved.minimax);
                menu.updated.set_text(error);
            }
        }
        return false;
    }

    if event.id == menu.show_distant_reminders_id {
        let requested = resolve_check_toggle(
            menu.show_distant_reminders_saved,
            menu.show_distant_reminders.is_checked(),
        );
        menu.show_distant_reminders.set_checked(requested);
        match settings::set_show_distant_reminders(requested) {
            Ok(()) => return true,
            Err(error) => {
                menu.show_distant_reminders
                    .set_checked(menu.show_distant_reminders_saved);
                menu.updated.set_text(error);
            }
        }
        return false;
    }

    #[cfg(target_os = "windows")]
    if event.id == menu.pin_meter_id {
        let requested = !settings::hud_pinned();
        menu.pin_meter.set_checked(requested);
        match settings::set_hud_pinned(requested) {
            Ok(()) => {
                if let Some(hud) = hud {
                    hud.set_pinned(requested);
                }
                return true;
            }
            Err(error) => {
                menu.pin_meter.set_checked(!requested);
                menu.updated.set_text(error);
            }
        }
        return false;
    }
    #[cfg(target_os = "windows")]
    if event.id == menu.meter_id {
        let visible = menu.meter.is_checked();
        if let Some(hud) = hud {
            hud.set_visible(visible);
        } else {
            menu.meter.set_checked(false);
        }
        return false;
    }
    #[cfg(target_os = "windows")]
    if event.id == menu.reset_meter_id {
        if let Some(hud) = hud {
            hud.reset_position();
        }
        return false;
    }
    #[cfg(not(target_os = "windows"))]
    let _ = hud;

    if event.id == menu.refresh_id {
        let _ = worker.send(WorkerCommand::Refresh);
    } else if event.id == menu.autostart_id {
        match autostart {
            Some(manager) => match manager.toggle() {
                Ok(enabled) => menu.autostart.set_checked(enabled),
                Err(error) => {
                    menu.autostart.set_checked(false);
                    menu.updated.set_text(error);
                }
            },
            None => {
                menu.autostart.set_checked(false);
                menu.updated
                    .set_text(i18n::text(language, key::LAUNCH_UNAVAILABLE));
            }
        }
    } else if event.id == menu.forget_approvals_id {
        match settings::clear_unsafe_tool_approvals() {
            Ok(()) => {
                tool_approval::reset_session_decisions();
                menu.forget_approvals.set_enabled(false);
                let _ = worker.send(WorkerCommand::Refresh);
            }
            Err(_) => menu
                .updated
                .set_text(i18n::text(language, key::CLEAR_APPROVALS_FAILED)),
        }
    } else if event.id == menu.quit_id {
        let _ = worker.send(WorkerCommand::Shutdown);
        *control_flow = ControlFlow::Exit;
    }
    false
}

fn resolve_check_toggle(previous_saved: bool, _observed_checked: bool) -> bool {
    !previous_saved
}

fn handle_personal_menu_event(
    id: &str,
    personal: &mut PersonalItems,
    language: Language,
) -> Option<bool> {
    let before = personal.clone();
    let changed = match id {
        "personal.note.add" => add_note(personal, language),
        "personal.reminder.add.after.minutes" => {
            add_reminder(personal, ReminderPreset::AfterMinutes, language)
        }
        "personal.reminder.add.after.hours" => {
            add_reminder(personal, ReminderPreset::AfterHours, language)
        }
        "personal.reminder.add.after.days" => {
            add_reminder(personal, ReminderPreset::AfterDays, language)
        }
        "personal.reminder.add.after.weeks" => {
            add_reminder(personal, ReminderPreset::AfterWeeks, language)
        }
        "personal.reminder.add.every.minutes" => {
            add_reminder(personal, ReminderPreset::EveryMinutes, language)
        }
        "personal.reminder.add.every.hours" => {
            add_reminder(personal, ReminderPreset::EveryHours, language)
        }
        "personal.reminder.add.every.days" => {
            add_reminder(personal, ReminderPreset::EveryDays, language)
        }
        "personal.reminder.add.every.weeks" => {
            add_reminder(personal, ReminderPreset::EveryWeeks, language)
        }
        "personal.reminder.add.once" => add_reminder(personal, ReminderPreset::Once, language),
        "personal.reminder.add.daily" => add_reminder(personal, ReminderPreset::Daily, language),
        "personal.reminder.add.weekdays" => {
            add_reminder(personal, ReminderPreset::Weekdays, language)
        }
        "personal.reminder.add.weekly" => add_reminder(personal, ReminderPreset::Weekly, language),
        "personal.reminder.add.selected_weekdays" => {
            add_reminder(personal, ReminderPreset::SelectedWeekdays, language)
        }
        "personal.reminder.add.weekends" => {
            add_reminder(personal, ReminderPreset::Weekends, language)
        }
        "personal.reminder.add.monthly" => {
            add_reminder(personal, ReminderPreset::Monthly, language)
        }
        "personal.reminder.add.monthly_last" => {
            add_reminder(personal, ReminderPreset::MonthlyLast, language)
        }
        "personal.reminder.add.yearly" => add_reminder(personal, ReminderPreset::Yearly, language),
        "personal.reminder.add.lunar_yearly" => {
            add_reminder(personal, ReminderPreset::LunarYearly, language)
        }
        _ => {
            if let Some(value) = id.strip_prefix("personal.note.delete.") {
                value
                    .parse::<u64>()
                    .ok()
                    .is_some_and(|id| personal.remove_note(id))
            } else if let Some(value) = id.strip_prefix("personal.reminder.complete.") {
                value
                    .parse::<u64>()
                    .ok()
                    .is_some_and(|id| personal.complete_next(id, model::unix_timestamp()))
            } else {
                let value = id.strip_prefix("personal.reminder.delete.")?;
                value
                    .parse::<u64>()
                    .ok()
                    .is_some_and(|id| personal.remove_reminder(id))
            }
        }
    };

    if changed && let Err(error) = settings::save_personal_items(personal) {
        *personal = before;
        platform_ui::show_error(&error);
        return Some(false);
    }
    Some(changed)
}

fn add_note(personal: &mut PersonalItems, language: Language) -> bool {
    let Some(text) = prompt_required(
        i18n::text(language, key::ADD_NOTE_TITLE),
        i18n::text(language, key::SHORT_NOTE_PROMPT),
        "",
        80,
        language,
    ) else {
        return false;
    };
    match personal.add_note(&text, language) {
        Ok(()) => true,
        Err(error) => {
            platform_ui::show_error(&error);
            false
        }
    }
}

fn add_reminder(personal: &mut PersonalItems, preset: ReminderPreset, language: Language) -> bool {
    let mut label_default = if matches!(
        preset,
        ReminderPreset::AfterMinutes
            | ReminderPreset::AfterHours
            | ReminderPreset::AfterDays
            | ReminderPreset::AfterWeeks
            | ReminderPreset::EveryMinutes
            | ReminderPreset::EveryHours
            | ReminderPreset::EveryDays
            | ReminderPreset::EveryWeeks
    ) {
        i18n::text(language, key::FOCUS)
    } else {
        i18n::text(language, key::REMINDER)
    }
    .to_string();
    let (schedule_prompt, mut schedule_default) =
        personal::preset_prompt(preset, model::unix_timestamp(), language);

    loop {
        let Some((label, schedule)) = platform_ui::prompt_reminder(
            &personal::preset_label(preset, language),
            i18n::text(language, key::REMINDER_TEXT_PROMPT),
            &label_default,
            60,
            &schedule_prompt,
            &schedule_default,
            64,
        ) else {
            return false;
        };
        match personal.add_reminder(preset, &label, &schedule, model::unix_timestamp(), language) {
            Ok(()) => return true,
            Err(error) => {
                platform_ui::show_error(&error);
                if personal.reminder_limit_reached() {
                    return false;
                }
                label_default = label;
                schedule_default = schedule;
            }
        }
    }
}

fn prompt_required(
    title: &str,
    prompt: &str,
    default: &str,
    max_chars: usize,
    language: Language,
) -> Option<String> {
    loop {
        let value = platform_ui::prompt_text(title, prompt, default, max_chars)?;
        if !value.split_whitespace().collect::<String>().is_empty() {
            return Some(value);
        }
        platform_ui::show_error(i18n::text(language, key::TEXT_EMPTY));
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_menu(
    tray: &TrayIcon,
    menu: &mut Menu,
    state: &mut MenuState,
    snapshot: &AppSnapshot,
    personal: &PersonalItems,
    hud_available: bool,
    show_distant_reminders: bool,
    language: Language,
    provider_visibility: ProviderVisibility,
) {
    let autostart_enabled = state.autostart.is_checked();
    let refresh_enabled = state.refresh.is_enabled();
    #[cfg(target_os = "windows")]
    let hud_visible = state.meter.is_checked();
    #[cfg(not(target_os = "windows"))]
    let hud_visible = false;

    match build_menu(
        snapshot,
        personal,
        autostart_enabled,
        hud_available,
        hud_visible,
        show_distant_reminders,
        language,
        provider_visibility,
    ) {
        Ok((new_menu, new_state)) => {
            new_state.refresh.set_enabled(refresh_enabled);
            if !refresh_enabled {
                new_state
                    .refresh
                    .set_text(i18n::text(language.effective(), key::REFRESHING));
            }
            tray.set_menu(Some(Box::new(new_menu.clone())));
            *menu = new_menu;
            *state = new_state;
        }
        Err(error) => state.updated.set_text(error),
    }
}

fn update_hud_personal(
    hud: Option<&Hud>,
    personal: &PersonalItems,
    now: i64,
    show_distant_reminders: bool,
    language: Language,
) {
    if let Some(hud) = hud {
        let content = personal.hud_content(now, show_distant_reminders, language);
        hud.update_personal(&content.items, &content.clock);
    }
}

#[cfg(target_os = "windows")]
fn show_hud_menu(
    menu: &Menu,
    hud: &Hud,
    reminder_id: Option<u64>,
    language: Language,
) -> Option<MenuEvent> {
    use std::ptr::null;
    use windows_sys::Win32::{
        Foundation::POINT,
        UI::WindowsAndMessaging::{
            GetCursorPos, InsertMenuW, MF_BYPOSITION, MF_SEPARATOR, MF_STRING, PostMessageW,
            RemoveMenu, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD,
            TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
        },
    };

    const COMPLETE_COMMAND: u32 = 0x7fff_ff01;
    let hwnd = hud.hwnd();
    let mut cursor = POINT::default();
    let handle = menu.hpopupmenu() as _;
    let mut inserted = 0;
    if reminder_id.is_some()
        && unsafe { InsertMenuW(handle, 0, MF_BYPOSITION | MF_SEPARATOR, 0, null()) } != 0
    {
        inserted = 1;
        let label = i18n::text(language, key::MARK_COMPLETE)
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        if unsafe {
            InsertMenuW(
                handle,
                0,
                MF_BYPOSITION | MF_STRING,
                COMPLETE_COMMAND as usize,
                label.as_ptr(),
            )
        } != 0
        {
            inserted = 2;
        }
    }
    hud.set_context_menu_active(true);
    let selected = unsafe {
        GetCursorPos(&mut cursor);
        SetForegroundWindow(hwnd as _);
        let selected = TrackPopupMenu(
            handle,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            cursor.x,
            cursor.y,
            0,
            hwnd as _,
            null(),
        );
        PostMessageW(hwnd as _, WM_NULL, 0, 0);
        selected
    };
    hud.set_context_menu_active(false);
    for _ in 0..inserted {
        unsafe { RemoveMenu(handle, 0, MF_BYPOSITION) };
    }
    if selected as u32 == COMPLETE_COMMAND {
        return reminder_id.map(|id| MenuEvent {
            id: MenuId::new(format!("personal.reminder.complete.{id}")),
        });
    }
    (selected > 0)
        .then(|| find_menu_id(menu.items(), menu.hpopupmenu(), selected as u32))
        .flatten()
        .map(|id| MenuEvent { id })
}

#[cfg(target_os = "windows")]
fn find_menu_id(
    items: Vec<tray_icon::menu::MenuItemKind>,
    handle: isize,
    selected: u32,
) -> Option<MenuId> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetMenuItemID;

    for (index, item) in items.into_iter().enumerate() {
        if let Some(submenu) = item.as_submenu() {
            if let Some(id) = find_menu_id(submenu.items(), submenu.hpopupmenu(), selected) {
                return Some(id);
            }
        } else if unsafe { GetMenuItemID(handle as _, index as i32) } == selected {
            return Some(item.id().clone());
        }
    }
    None
}

fn spawn_worker(
    initial: AppSnapshot,
    proxy: EventLoopProxy<UserEvent>,
    interval: Duration,
) -> Sender<WorkerCommand> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut snapshot = initial;
        loop {
            let _ = proxy.send_event(UserEvent::RefreshStarted);
            snapshot = providers::refresh(snapshot, true);
            let _ = cache::save(&snapshot);
            let _ = proxy.send_event(UserEvent::SnapshotUpdated(snapshot.clone()));

            match receiver.recv_timeout(interval) {
                Ok(WorkerCommand::Shutdown) => break,
                Ok(WorkerCommand::Refresh) => {
                    if receiver
                        .try_iter()
                        .any(|command| matches!(command, WorkerCommand::Shutdown))
                    {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    sender
}

fn refresh_interval() -> Duration {
    let seconds = env::var("SEEUSAGE_REFRESH_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(180)
        .max(30);
    Duration::from_secs(seconds)
}

#[cfg(target_os = "macos")]
fn configure_macos(event_loop: &mut tao::event_loop::EventLoop<UserEvent>) {
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    event_loop.set_dock_visibility(false);
}

#[cfg(not(target_os = "macos"))]
fn configure_macos(_: &mut tao::event_loop::EventLoop<UserEvent>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_interval_has_a_floor() {
        unsafe { env::set_var("SEEUSAGE_REFRESH_SECONDS", "1") };
        assert_eq!(refresh_interval(), Duration::from_secs(30));
        unsafe { env::remove_var("SEEUSAGE_REFRESH_SECONDS") };
    }

    #[test]
    fn synthesized_context_menu_event_toggles_from_the_saved_state() {
        assert!(resolve_check_toggle(false, false));
        assert!(!resolve_check_toggle(true, true));
        assert!(resolve_check_toggle(false, true));
        assert!(!resolve_check_toggle(true, false));
    }
}
