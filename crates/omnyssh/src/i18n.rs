//! Fluent-backed localization for the terminal frontend.

use std::sync::{OnceLock, RwLock};

use fluent_bundle::{concurrent::FluentBundle, FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

const EN_MESSAGES: &str = include_str!("../../../locales/en-US/tui.ftl");
const ZH_MESSAGES: &str = include_str!("../../../locales/zh-CN/tui.ftl");

struct Translator {
    locale: &'static str,
    bundle: FluentBundle<FluentResource>,
}

impl Translator {
    fn new(preference: &str) -> Self {
        let system = sys_locale::get_locale();
        let locale = omnyssh_core::locale::resolve_locale(preference, system.as_deref());
        let langid: LanguageIdentifier = locale.parse().expect("built-in locale is valid");
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle.set_use_isolating(false);
        let source = if locale == omnyssh_core::locale::ZH_CN {
            ZH_MESSAGES
        } else {
            EN_MESSAGES
        };
        let resource = FluentResource::try_new(source.to_string())
            .unwrap_or_else(|(_, errors)| panic!("invalid built-in {locale} catalog: {errors:?}"));
        bundle
            .add_resource(resource)
            .expect("built-in catalog has unique message ids");
        Self { locale, bundle }
    }

    fn format<S: AsRef<str>>(&self, id: &str, values: &[(&str, S)]) -> String {
        let message = self.bundle.get_message(id);
        let Some(pattern) = message.and_then(|message| message.value()) else {
            tracing::warn!(message_id = id, locale = self.locale, "missing translation");
            return id.to_string();
        };
        let mut args = FluentArgs::new();
        for (name, value) in values {
            args.set(*name, FluentValue::from(value.as_ref()));
        }
        let mut errors = Vec::new();
        self.bundle
            .format_pattern(pattern, Some(&args), &mut errors)
            .into_owned()
    }
}

static TRANSLATOR: OnceLock<RwLock<Translator>> = OnceLock::new();

fn state() -> &'static RwLock<Translator> {
    TRANSLATOR.get_or_init(|| RwLock::new(Translator::new(omnyssh_core::locale::SYSTEM)))
}

pub fn init(preference: &str) {
    let mut translator = state()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *translator = Translator::new(preference);
}

pub fn set_preference(preference: &str) {
    init(preference);
}

pub fn current_locale() -> &'static str {
    state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .locale
}

pub fn tr(id: &str) -> String {
    state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .format::<&str>(id, &[])
}

pub fn tr_args<S: AsRef<str>>(id: &str, values: &[(&str, S)]) -> String {
    state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .format(id, values)
}
