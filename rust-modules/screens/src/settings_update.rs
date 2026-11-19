//! **Settings › Software update** (this fork): the self-updater's page, one level below the root's
//! Software update row (whose value names a newer version when one is known).
//!
//! Two rows: *Check for updates*, whose value says what the last check found, and *Install
//! {version}*, present once a check has found a newer release, whose value follows the install.
//! While either runs the section is disabled, and the page asks for the state every tick.
//!
//! Like every page of the family it states no IO: a press leaves as an [`AppFx::Update`] and the
//! state returns as an addressed `AppMsg::Update` (the application's `app::update` owns the check,
//! the install and the version).

use super::*;
use crate::registry::{UpdateCmd, UpdatePhase, UpdateStatus};

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum UpdateId {
    Check,
    Install,
}

impl FormId for UpdateId {
    fn key(&self) -> RowKey {
        RowKey(match self {
            UpdateId::Check => 1,
            UpdateId::Install => 2,
        })
    }
    fn a11y(&self) -> Option<&'static str> {
        Some(match self {
            UpdateId::Check => "check",
            UpdateId::Install => "install",
        })
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum UpdateAction {
    Check,
    Install,
}

/// The value beside *Check for updates*: what the last check found.
fn check_value(status: &UpdateStatus) -> String {
    use plx_platform::i18n::msg;
    match status.phase {
        UpdatePhase::Unavailable => msg::settings_update_unavailable().to_owned(),
        UpdatePhase::Checking => msg::settings_update_checking().to_owned(),
        UpdatePhase::UpToDate => msg::settings_update_up_to_date().to_owned(),
        UpdatePhase::CheckFailed => msg::settings_update_check_failed().to_owned(),
        UpdatePhase::Idle => String::new(),
        _ => status.latest.as_deref().map(msg::settings_update_available).unwrap_or_default(),
    }
}

/// The value beside *Install {version}*: how the install is going, once it has started.
fn install_value(phase: UpdatePhase) -> String {
    use plx_platform::i18n::msg;
    match phase {
        UpdatePhase::Downloading(None) => msg::settings_update_downloading().to_owned(),
        UpdatePhase::Downloading(Some(p)) => msg::settings_update_downloading_percent(&p.to_string()),
        UpdatePhase::Verifying => msg::settings_update_verifying().to_owned(),
        UpdatePhase::Installing => msg::settings_update_installing().to_owned(),
        UpdatePhase::Installed => msg::settings_update_installed().to_owned(),
        UpdatePhase::NoHomebrewChannel => msg::settings_update_needs_hbc().to_owned(),
        UpdatePhase::InstallFailed => msg::settings_update_install_failed().to_owned(),
        UpdatePhase::Unavailable | UpdatePhase::Idle | UpdatePhase::Checking | UpdatePhase::UpToDate
        | UpdatePhase::CheckFailed | UpdatePhase::Available => String::new(),
    }
}

/// The page's rows from the updater's state alone.
pub(super) fn update_form(status: &UpdateStatus) -> Form<UpdateId, UpdateAction, SettingsPage> {
    use plx_platform::i18n::msg;
    let installing = matches!(status.phase, UpdatePhase::Downloading(_) | UpdatePhase::Verifying
        | UpdatePhase::Installing | UpdatePhase::Installed | UpdatePhase::NoHomebrewChannel | UpdatePhase::InstallFailed);
    let latest = status.latest.as_deref().filter(|_| status.phase != UpdatePhase::Unavailable);
    let section = FormSection::new("")
        .disabled(status.phase.busy() || status.phase == UpdatePhase::Unavailable)
        .item(UpdateId::Check, RowKind::Button, UpdateAction::Check,
            Row::new(msg::settings_update_check()).value(if installing { String::new() } else { check_value(status) }))
        .item_if(latest.is_some(), UpdateId::Install, RowKind::Button, UpdateAction::Install,
            Row::new(latest.map(msg::settings_update_install).unwrap_or_default()).value(install_value(status.phase)));
    Form::new().section(section)
}

/// The root's Software update value: a newer version when one is known, else nothing.
pub(super) fn root_value(status: &UpdateStatus) -> String {
    match (status.phase, status.latest.as_deref()) {
        (UpdatePhase::Unavailable, _) | (_, None) => String::new(),
        (_, Some(version)) => plx_platform::i18n::msg::settings_update_available(version),
    }
}

/// The updater's state before the first answer: nothing known.
pub(super) fn unknown() -> UpdateStatus {
    UpdateStatus { phase: UpdatePhase::Idle, current: String::new(), latest: None }
}

pub(super) struct UpdatePage {
    entry: EntryId,
    /// `pub(super)` for the fit tests, which grade the table [`Self::at`] builds.
    pub(super) form: FormTable<UpdateId, UpdateAction, SettingsPage>,
    state: UpdateState,
    /// The sentence under the title, which names this build's version.
    copy: String,
}

struct UpdateState {
    sel: RowKey,
    status: UpdateStatus,
}

impl LogicalState for UpdateState {
    fn write(&self, w: &mut Canon) {
        w.u32(self.sel.0).u8(self.status.phase.code());
        if let UpdatePhase::Downloading(p) = self.status.phase {
            w.u8(p.map_or(u8::MAX, |p| p));
        }
        w.str(&self.status.current).str(self.status.latest.as_deref().unwrap_or(""));
    }
    fn probe(&self, out: &mut String) {
        out.push_str(&format!("update sel={} phase={:?} latest={:?}", self.sel.0, self.status.phase, self.status.latest));
    }
}

impl UpdatePage {
    pub(super) fn new(entry: EntryId) -> Self {
        Self::at(entry, unknown())
    }

    /// The page as it stands once the runner has answered `status`: the table the fit tests grade
    /// in every state, the one a viewer sees.
    pub(super) fn at(entry: EntryId, status: UpdateStatus) -> Self {
        let mut page = Self {
            entry,
            form: FormTable::page(super::super::registry::BAND),
            state: UpdateState { sel: UpdateId::Check.key(), status },
            copy: String::new(),
        };
        page.rebuild();
        page
    }

    fn rebuild(&mut self) {
        let keep = self.form.selected_id().cloned().unwrap_or(UpdateId::Check);
        self.form.set(update_form(&self.state.status), Some(&keep));
        self.form.table.list_focused = true;
        self.copy = plx_platform::i18n::msg::settings_update_detail(&self.state.status.current);
    }

    fn view(&self) -> TableScreen<'_> {
        use plx_platform::i18n::msg;
        TableScreen::new(
            Header::new(RouteLayout::screen(), Some(msg::settings_title()), msg::settings_update_title(), &self.copy),
            &self.form.table, GroupId(0), self.entry,
        )
        .keyed(&self.form)
    }

    fn activate(&mut self, key: u32, fx: &mut Effects<'_, InnerHost>) {
        let Some(action) = form_activate(&self.form, key, fx) else { return };
        fx.push(Fx::App(AppFx::Update(match action {
            UpdateAction::Check => UpdateCmd::Check,
            UpdateAction::Install => UpdateCmd::Install,
        })));
    }
}

impl Machine<InnerHost> for UpdatePage {
    type Ev = ScreenEvent<InnerHost>;
    fn step(&mut self, ev: &Self::Ev, cx: &Cx<'_, InnerHost>, fx: &mut Effects<'_, InnerHost>) -> Handled {
        match ev {
            ScreenEvent::Mount => {
                fx.push(Fx::Deliver(
                    MachineId::Instance(InstanceId(0)),
                    Delivery::Screen(ScreenEvent::Enter(Enter::Fresh {
                        focus: FocusTarget::Elem(FocusKey { entry: self.entry, elem: self.state.sel.0 }),
                    })),
                ));
                // Read the state through the runner, never through a global in the screen.
                fx.push(Fx::App(AppFx::Update(UpdateCmd::Status)));
                Handled::Yes
            }
            ScreenEvent::App(super::super::registry::AppMsg::Update(status)) => {
                if self.state.status != *status {
                    self.state.status = status.clone();
                    self.rebuild();
                    fx.invalidate(Provenance::Landing(MachineId::Session));
                }
                Handled::Yes
            }
            ScreenEvent::Tick(t) => {
                if self.state.status.phase.busy() {
                    fx.push(Fx::App(AppFx::Update(UpdateCmd::Status)));
                }
                self.form.table.update(t.dt(), RouteLayout::screen().sectioned_table().h);
                Handled::Yes
            }
            ScreenEvent::FocusMoved { to, .. } => {
                form_focus(&mut self.form, to.elem);
                if let Some(key) = self.form.key_at(self.form.table.sel.max(0) as usize) {
                    self.state.sel = key;
                }
                Handled::Yes
            }
            ScreenEvent::Activate(key) => {
                self.activate(*key, fx);
                Handled::Yes
            }
            ScreenEvent::Input(plx_machine::machine::InputEvent {
                kind: plx_machine::machine::InputKind::Key { key: Key::Right, at_edge: true, .. }, ..
            }) => {
                if let Some(key) = cx.focus.current.and_then(|k| form_right_target(&self.form, k.elem)) {
                    self.activate(key, fx);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

plx_ui::focusable_via_view!(UpdatePage, InnerHost, view);

impl Screen<InnerHost> for UpdatePage {
    fn name(&self) -> &'static str { super::super::registry::word::SOFTWARE_UPDATE }
    fn state(&self) -> &dyn LogicalState { &self.state }
    fn crumb(&self, _cx: &Cx<'_, InnerHost>) -> Option<Cow<'_, str>> { Some(Cow::Borrowed(plx_platform::i18n::msg::settings_title())) }
    fn prepare(&mut self, _b: &mut Budget, _cx: &Cx<'_, InnerHost>) {}
    fn draw(&mut self, f: &mut DrawFrame<'_, '_, InnerHost>) {
        plx_ui::screen::Part::<InnerHost>::draw(&mut self.view(), f, Rect::FULL);
    }
    fn render(&self) -> RenderStrategy { RenderStrategy::Page }
    fn focus_source(&self) -> FocusSource { FocusSource::Engine }
    fn hit_source(&self) -> HitSource { HitSource::Engine }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(phase: UpdatePhase, latest: Option<&str>) -> UpdateStatus {
        UpdateStatus { phase, current: "0.10.24".into(), latest: latest.map(str::to_owned) }
    }

    fn rows(s: &UpdateStatus) -> Vec<UpdateId> {
        let mut page = UpdatePage::at(EntryId(1), s.clone());
        page.rebuild();
        (0..4).filter_map(|i| page.form.key_at(i)).map(|k| if k == UpdateId::Check.key() { UpdateId::Check } else { UpdateId::Install }).collect()
    }

    #[test]
    fn install_is_offered_only_once_a_newer_release_is_known() {
        assert_eq!(rows(&status(UpdatePhase::Idle, None)), vec![UpdateId::Check]);
        assert_eq!(rows(&status(UpdatePhase::UpToDate, None)), vec![UpdateId::Check]);
        assert_eq!(rows(&status(UpdatePhase::Available, Some("0.10.25"))), vec![UpdateId::Check, UpdateId::Install]);
        assert_eq!(rows(&status(UpdatePhase::InstallFailed, Some("0.10.25"))), vec![UpdateId::Check, UpdateId::Install],
            "a failed install can be tried again");
        assert_eq!(rows(&status(UpdatePhase::Unavailable, Some("0.10.25"))), vec![UpdateId::Check]);
    }

    #[test]
    fn the_values_say_what_was_found_and_how_the_install_goes() {
        use plx_platform::i18n::msg;
        assert_eq!(check_value(&status(UpdatePhase::UpToDate, None)), msg::settings_update_up_to_date());
        assert_eq!(check_value(&status(UpdatePhase::Available, Some("0.10.25"))), msg::settings_update_available("0.10.25"));
        assert_eq!(check_value(&status(UpdatePhase::Unavailable, None)), msg::settings_update_unavailable());
        assert_eq!(install_value(UpdatePhase::Downloading(Some(41))), msg::settings_update_downloading_percent("41"));
        assert_eq!(install_value(UpdatePhase::NoHomebrewChannel), msg::settings_update_needs_hbc());
        assert_eq!(install_value(UpdatePhase::Available), "");
        assert_eq!(root_value(&status(UpdatePhase::Available, Some("0.10.25"))), msg::settings_update_available("0.10.25"));
        assert_eq!(root_value(&status(UpdatePhase::UpToDate, None)), "");
    }

    #[test]
    fn a_press_leaves_as_an_update_command() {
        let s = status(UpdatePhase::Available, Some("0.10.25"));
        let form = update_form(&s);
        let mut table: FormTable<UpdateId, UpdateAction, SettingsPage> = FormTable::page(super::super::super::registry::BAND);
        table.set(form, None);
        let index = table.index_of_key(UpdateId::Install.key()).expect("the Install row");
        assert!(matches!(table.activate(index), Some(plx_ui::form::Activation::Action(UpdateAction::Install))));
    }
}
